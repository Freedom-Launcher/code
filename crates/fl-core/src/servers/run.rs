//! Running servers from the launcher: start, stop, restart, the live console
//! with commands, who is online, and restarting by itself after a crash.
//!
//! The launcher starts Java itself (the same Java and arguments as the start
//! script), with the console on pipes: output goes into a ring buffer the UI
//! polls with `servers.console`, commands go to the server's stdin. A stop is
//! the server's own `stop` command, so the world is saved; a server that does
//! not exit in time is killed. Quitting the launcher stops its servers.

use std::{
    collections::{BTreeSet, HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
    time::Duration,
};

use serde::Deserialize;
use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin},
};

use super::{args, bad, cores, find, prop, read_props, software, valid_player, IdArg, Server};
use crate::{actions::ActionRegistry, game::download::Progress, game::javas, unix_now, Core, CoreError, Permission as P};

const LOG_MAX: usize = 5000;
const LINE_MAX: usize = 4000;
const PAGE_MAX: usize = 2000;
const COMMAND_MAX: usize = 1000;
/// How long a server gets to save and exit after `stop`.
const STOP_WAIT: Duration = Duration::from_secs(90);
/// Crash restarts: at most `CRASH_MAX` within `CRASH_WINDOW` seconds.
const CRASH_WINDOW: u64 = 600;
const CRASH_MAX: usize = 3;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Server processes by server id.
#[derive(Default)]
pub struct Procs {
    map: Mutex<HashMap<String, Arc<Proc>>>,
    /// Backups and restores in progress.
    busy: Mutex<HashSet<String>>,
    /// The launcher is quitting: no restarts.
    closing: AtomicBool,
}

impl Procs {
    pub fn get(&self, id: &str) -> Option<Arc<Proc>> {
        lock(&self.map).get(id).cloned()
    }

    fn entry(&self, id: &str) -> Arc<Proc> {
        Arc::clone(lock(&self.map).entry(id.to_owned()).or_default())
    }

    pub fn alive(&self, id: &str) -> bool {
        self.get(id).is_some_and(|p| p.alive())
    }

    pub fn status(&self, id: &str) -> Value {
        match self.get(id) {
            Some(p) => p.status(),
            None => json!({ "state": "stopped", "players": [], "online": 0 }),
        }
    }

    /// Drops what is left of a stopped server (after it is removed).
    pub fn forget(&self, id: &str) {
        let mut m = lock(&self.map);
        if m.get(id).is_some_and(|p| !p.alive()) {
            m.remove(id);
        }
    }

    pub fn running(&self) -> Vec<String> {
        let mut v: Vec<String> = lock(&self.map).iter().filter(|(_, p)| p.alive()).map(|(k, _)| k.clone()).collect();
        v.sort();
        v
    }

    /// Reserves the server for a backup or restore; `false` if one runs.
    pub fn begin_busy(&self, id: &str) -> bool {
        lock(&self.busy).insert(id.to_owned())
    }

    pub fn end_busy(&self, id: &str) {
        lock(&self.busy).remove(id);
    }

    pub fn is_busy(&self, id: &str) -> bool {
        lock(&self.busy).contains(id)
    }
}

#[derive(Default)]
pub struct Proc {
    st: Mutex<St>,
    stdin: tokio::sync::Mutex<Option<ChildStdin>>,
    child: Mutex<Option<Child>>,
}

#[derive(Default)]
struct St {
    /// `starting`, `running`, `stopping`, `stopped`, `crashed` (empty = never started).
    state: &'static str,
    log: VecDeque<(u64, String)>,
    next: u64,
    pid: Option<u32>,
    started_at: u64,
    ready_at: Option<u64>,
    players: BTreeSet<String>,
    saves: u64,
    note: Option<String>,
    exit_code: Option<i32>,
    stop_requested: bool,
    restart_after: bool,
    crashes: Vec<u64>,
    /// Bumped on every start, so a late stop timer does not hit the next run.
    gen: u64,
}

impl St {
    fn push(&mut self, line: String) {
        let n = self.next;
        self.next += 1;
        self.log.push_back((n, line));
        while self.log.len() > LOG_MAX {
            self.log.pop_front();
        }
    }
}

fn alive_state(s: &str) -> bool {
    matches!(s, "starting" | "running" | "stopping")
}

impl Proc {
    pub fn state(&self) -> &'static str {
        match lock(&self.st).state {
            "" => "stopped",
            s => s,
        }
    }

    pub fn alive(&self) -> bool {
        alive_state(lock(&self.st).state)
    }

    /// How many times the server said it saved the world.
    pub fn saves(&self) -> u64 {
        lock(&self.st).saves
    }

    /// A launcher line in the console.
    pub fn note(&self, m: &str) {
        let mut g = lock(&self.st);
        g.note = Some(m.to_owned());
        g.push(format!("[Лаунчер] {m}"));
    }

    fn say(&self, m: &str) {
        lock(&self.st).push(format!("[Лаунчер] {m}"));
    }

    /// Sends a console command to the server.
    pub async fn send(&self, cmd: &str) -> Result<(), CoreError> {
        let mut g = self.stdin.lock().await;
        let w = g.as_mut().ok_or_else(|| bad("сервер ещё не запущен"))?;
        let line = format!("{cmd}\n");
        let res = async {
            w.write_all(line.as_bytes()).await?;
            w.flush().await
        }
        .await;
        res.map_err(|e| CoreError::msg(format!("сервер не принял команду: {e}")))
    }

    fn gen(&self) -> u64 {
        lock(&self.st).gen
    }

    fn has_child(&self) -> bool {
        lock(&self.child).is_some()
    }

    fn kill(&self) {
        if let Some(c) = lock(&self.child).as_mut() {
            let _ = c.start_kill();
        }
    }

    /// Marks a new run; `false` if the server is already running.
    fn begin(&self, manual: bool) -> bool {
        let mut g = lock(&self.st);
        if alive_state(g.state) {
            return false;
        }
        g.state = "starting";
        g.stop_requested = false;
        g.restart_after = false;
        g.players.clear();
        g.exit_code = None;
        g.note = None;
        g.pid = None;
        g.ready_at = None;
        g.started_at = unix_now();
        g.gen += 1;
        if manual {
            g.crashes.clear();
        }
        if g.next > 0 {
            g.push(String::new());
        }
        g.push("[Лаунчер] Запускаем сервер…".into());
        true
    }

    fn stop_requested(&self) -> bool {
        lock(&self.st).stop_requested
    }

    /// The launch failed before or while starting Java.
    fn fail(&self, m: &str) {
        let mut g = lock(&self.st);
        if g.stop_requested {
            g.state = "stopped";
            g.push("[Лаунчер] Запуск отменён".into());
        } else {
            g.state = "crashed";
            g.note = Some(m.to_owned());
            g.push(format!("[Лаунчер] {m}"));
        }
    }

    async fn started(&self, child: Child, stdin: Option<ChildStdin>) {
        let pid = child.id();
        *self.stdin.lock().await = stdin;
        *lock(&self.child) = Some(child);
        let mut g = lock(&self.st);
        g.pid = pid;
        g.note = None;
    }

    /// Waits for the process to exit; its exit code.
    async fn wait(&self) -> Option<i32> {
        loop {
            {
                let mut g = lock(&self.child);
                match g.as_mut().map(|c| c.try_wait()) {
                    None => return None,
                    Some(Ok(None)) => {}
                    Some(Ok(Some(st))) => {
                        *g = None;
                        return Some(st.code().unwrap_or(-1));
                    }
                    Some(Err(_)) => {
                        *g = None;
                        return None;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    /// Bookkeeping after an exit; the delay before starting again, if it should.
    fn exited(&self, code: Option<i32>, auto: bool, closing: bool) -> Option<Duration> {
        let mut g = lock(&self.st);
        g.pid = None;
        g.players.clear();
        g.exit_code = code;
        let clean = g.stop_requested || code == Some(0);
        if clean {
            g.state = "stopped";
            g.note = None;
            g.push("[Лаунчер] Сервер остановлен".into());
        } else {
            g.state = "crashed";
            let m = match code {
                Some(c) => format!("Сервер завершился с ошибкой (код {c}). Причина обычно в последних строках консоли."),
                None => "Сервер завершился с ошибкой.".to_owned(),
            };
            g.note = Some(m.clone());
            g.push(format!("[Лаунчер] {m}"));
        }
        if closing {
            return None;
        }
        if g.restart_after {
            g.restart_after = false;
            return Some(Duration::from_secs(2));
        }
        if !clean && auto {
            let now = unix_now();
            g.crashes.retain(|t| now.saturating_sub(*t) < CRASH_WINDOW);
            g.crashes.push(now);
            if g.crashes.len() <= CRASH_MAX {
                g.push("[Лаунчер] Автоперезапуск через 5 секунд…".into());
                return Some(Duration::from_secs(5));
            }
            g.push("[Лаунчер] Сервер падает слишком часто: автоперезапуск ждёт ручного запуска".into());
        }
        None
    }

    fn on_line(&self, raw: &str) {
        let mut line = strip_ansi(raw);
        if line.len() > LINE_MAX {
            let mut cut = LINE_MAX;
            while !line.is_char_boundary(cut) {
                cut -= 1;
            }
            line.truncate(cut);
            line.push('…');
        }
        let mut g = lock(&self.st);
        if let Some(ev) = event(&line) {
            match ev {
                Event::Ready => {
                    if g.state == "starting" {
                        g.state = "running";
                    }
                    g.ready_at.get_or_insert_with(unix_now);
                }
                Event::Join(n) => {
                    g.players.insert(n);
                }
                Event::Leave(n) => {
                    g.players.remove(&n);
                }
                Event::Saved => g.saves += 1,
                Event::PortBusy => {
                    g.note = Some("Порт сервера занят другой программой (или вторым сервером)".into());
                }
                Event::Online(list) => g.players = list.into_iter().collect(),
            }
        }
        g.push(line);
    }

    pub fn status(&self) -> Value {
        let g = lock(&self.st);
        let state = if g.state.is_empty() { "stopped" } else { g.state };
        let alive = alive_state(state);
        json!({
            "state": state,
            "pid": g.pid,
            "started_at": alive.then_some(g.started_at),
            "ready_at": if alive { g.ready_at } else { None },
            "uptime": if alive { g.ready_at.map(|t| unix_now().saturating_sub(t)) } else { None },
            "players": g.players.iter().collect::<Vec<_>>(),
            "online": g.players.len(),
            "note": g.note,
            "exit_code": g.exit_code,
        })
    }

    /// Console lines numbered `from` and later (at most a page, the newest).
    pub fn lines(&self, from: u64) -> Value {
        let g = lock(&self.st);
        let reset = from > g.next;
        let from = if reset { 0 } else { from };
        let first = g.log.front().map_or(g.next, |l| l.0);
        let mut v: Vec<Value> = g.log.iter().filter(|l| l.0 >= from).map(|(n, t)| json!([n, t])).collect();
        let dropped = from < first && from > 0 || v.len() > PAGE_MAX;
        if v.len() > PAGE_MAX {
            v.drain(..v.len() - PAGE_MAX);
        }
        json!({ "lines": v, "cursor": g.next, "reset": reset, "dropped": dropped })
    }

    /// Asks the server to save and exit; kills it if it does not in time.
    async fn request_stop(self: &Arc<Self>) {
        let (gen, had) = {
            let mut g = lock(&self.st);
            if !alive_state(g.state) {
                return;
            }
            let had = g.stop_requested;
            g.stop_requested = true;
            g.state = "stopping";
            (g.gen, had)
        };
        if had {
            return;
        }
        if self.has_child() {
            self.say("Останавливаем сервер: сохраняем мир…");
            if self.send("stop").await.is_err() {
                self.kill();
            }
        }
        let p = Arc::clone(self);
        tokio::spawn(async move {
            tokio::time::sleep(STOP_WAIT).await;
            if p.gen() == gen && p.has_child() && p.stop_requested() {
                p.say("Сервер не остановился за полторы минуты: завершаем принудительно");
                p.kill();
            }
        });
    }
}

enum Event {
    Ready,
    Join(String),
    Leave(String),
    Saved,
    PortBusy,
    Online(Vec<String>),
}

/// What a console line means for the launcher.
fn event(line: &str) -> Option<Event> {
    if line.contains("FAILED TO BIND TO PORT") {
        return Some(Event::PortBusy);
    }
    let msg = line.split_once("]: ").map(|(_, m)| m)?.trim();
    if msg.starts_with("Done (") && msg.contains("For help") {
        return Some(Event::Ready);
    }
    if msg == "Saved the game" || msg == "Saved the world" {
        return Some(Event::Saved);
    }
    if let Some(n) = msg.strip_suffix(" joined the game") {
        return valid_player(n).then(|| Event::Join(n.to_owned()));
    }
    if let Some(n) = msg.strip_suffix(" left the game") {
        return valid_player(n).then(|| Event::Leave(n.to_owned()));
    }
    // `There are 2 of a max of 20 players online: Steve, Alex`
    if msg.starts_with("There are ") && msg.contains(" players online:") {
        let names = msg.split_once("online:").map_or("", |(_, r)| r);
        let list: Vec<String> = names.split(',').map(str::trim).filter(|n| valid_player(n)).map(str::to_owned).collect();
        return Some(Event::Online(list));
    }
    None
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\u{1b}' {
            if it.peek() == Some(&'[') {
                it.next();
                for d in it.by_ref() {
                    if d.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        if c == '\r' || (c.is_control() && c != '\t') {
            continue;
        }
        out.push(c);
    }
    out
}

/// UTF-8, else Windows-1251 (old Java on a Russian Windows).
fn decode(b: &[u8]) -> String {
    match std::str::from_utf8(b) {
        Ok(s) => s.to_owned(),
        Err(_) => b
            .iter()
            .map(|&x| match x {
                0..=0x7f => x as char,
                0xc0..=0xff => char::from_u32(0x410 + u32::from(x - 0xc0)).unwrap_or('?'),
                0xa8 => 'Ё',
                0xb8 => 'ё',
                _ => '?',
            })
            .collect(),
    }
}

async fn read_lines<R: AsyncRead + Unpin>(p: Arc<Proc>, r: R) {
    let mut r = BufReader::new(r);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match r.read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                while matches!(buf.last(), Some(b'\n' | b'\r')) {
                    buf.pop();
                }
                p.on_line(&decode(&buf));
            }
        }
    }
}

/// What to run: Minecraft version (for Java), arguments after memory, memory.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub mc: String,
    pub launch: Vec<String>,
    pub memory_mb: u32,
}

fn plain(n: &str) -> bool {
    !n.is_empty() && n.len() <= 128 && !n.starts_with('.') && n.chars().all(|c| c.is_ascii_alphanumeric() || "._+-".contains(c))
}

/// The args file of a Forge / NeoForge server installed by its own installer.
pub fn forge_args(root: &Path) -> Option<String> {
    let os = if cfg!(windows) { "win" } else { "unix" };
    for base in ["libraries/net/neoforged/neoforge", "libraries/net/minecraftforge/forge"] {
        let Ok(rd) = std::fs::read_dir(root.join(base)) else { continue };
        let mut vs: Vec<String> = rd
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| plain(n) && root.join(base).join(n).join(format!("{os}_args.txt")).is_file())
            .collect();
        vs.sort_by_key(|v| cores::ver_key(v));
        if let Some(v) = vs.pop() {
            return Some(format!("@{base}/{v}/{os}_args.txt"));
        }
    }
    None
}

pub fn plan(s: &Server, root: &Path) -> Result<Plan, CoreError> {
    if let Some(i) = &s.core {
        return Ok(Plan { mc: i.mc.clone(), launch: i.launch.clone(), memory_mb: i.memory_mb });
    }
    let memory_mb = s.memory_mb.unwrap_or(cores::MEMORY_DEFAULT);
    let sw = software(root);
    let mc = sw["version"].as_str().unwrap_or("").to_owned();
    if let Some(a) = forge_args(root) {
        return Ok(Plan { mc, launch: vec![a], memory_mb });
    }
    let jar = sw["jar"]
        .as_str()
        .filter(|j| plain(j) && j.to_ascii_lowercase().ends_with(".jar") && root.join(j).is_file())
        .ok_or_else(|| bad("в папке нет jar-файла сервера: поставьте ядро на вкладке «Обзор»"))?;
    Ok(Plan { mc, launch: vec!["-jar".into(), jar.to_owned()], memory_mb })
}

fn eula_ok(root: &Path) -> bool {
    std::fs::read_to_string(root.join("eula.txt"))
        .unwrap_or_default()
        .lines()
        .any(|l| l.trim().eq_ignore_ascii_case("eula=true"))
}

pub const EULA_TEXT: &str = "#By changing the setting below to TRUE you are indicating your agreement to our EULA (https://aka.ms/MinecraftEULA).\neula=true\n";

fn port_of(root: &Path) -> u16 {
    prop(&read_props(root), "server-port").and_then(|p| p.trim().parse().ok()).unwrap_or(25565)
}

fn port_free(port: u16) -> bool {
    std::net::TcpListener::bind(("0.0.0.0", port)).is_ok()
}

/// Starts the server; returns at once, the console shows the progress.
pub fn start(c: &Arc<Core>, id: &str, manual: bool) -> Result<Value, CoreError> {
    if c.server_procs.closing.load(Ordering::SeqCst) {
        return Err(bad("лаунчер закрывается"));
    }
    if c.server_procs.alive(id) {
        return Err(bad("сервер уже запущен"));
    }
    if c.server_procs.is_busy(id) {
        return Err(bad("идёт резервное копирование или восстановление мира, подождите"));
    }
    let s = find(c, id)?;
    let root = s.root()?;
    if !eula_ok(&root) {
        return Err(bad("EULA не принята: без неё сервер не запустится"));
    }
    let plan = plan(&s, &root)?;
    let port = port_of(&root);
    if !port_free(port) {
        return Err(bad(format!(
            "порт {port} уже занят: этот сервер уже запущен не из лаунчера, или порт занят другой программой. Порт меняется в «Настройках»"
        )));
    }
    let p = c.server_procs.entry(id);
    if !p.begin(manual) {
        return Err(bad("сервер уже запущен"));
    }
    let (c2, p2, id2) = (Arc::clone(c), Arc::clone(&p), id.to_owned());
    tokio::spawn(async move { run(c2, id2, p2, root, plan).await });
    Ok(p.status())
}

async fn launch(c: &Core, p: &Arc<Proc>, root: &Path, plan: &Plan) -> Result<(), CoreError> {
    p.say(&format!("Готовим Java {}…", javas::required_major(&plan.mc)));
    let (_, java) = cores::java_for(c, &plan.mc, Arc::new(Progress::default())).await?;
    if p.stop_requested() {
        return Err(CoreError::msg("запуск отменён"));
    }
    let mut cmd = tokio::process::Command::new(&java);
    cmd.arg(format!("-Xms{}M", plan.memory_mb.min(1024)))
        .arg(format!("-Xmx{}M", plan.memory_mb))
        .arg("-Dstdout.encoding=UTF-8")
        .arg("-Dstderr.encoding=UTF-8")
        .args(&plan.launch)
        .arg("nogui")
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(false);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    p.say(&format!("java -Xmx{}M {} nogui", plan.memory_mb, plan.launch.join(" ")));
    let mut child = cmd.spawn().map_err(|e| CoreError::msg(format!("Java не запустилась: {e}")))?;
    if let Some(o) = child.stdout.take() {
        tokio::spawn(read_lines(Arc::clone(p), o));
    }
    if let Some(e) = child.stderr.take() {
        tokio::spawn(read_lines(Arc::clone(p), e));
    }
    let stdin = child.stdin.take();
    p.started(child, stdin).await;
    if p.stop_requested() {
        // Stop pressed while Java was being prepared.
        let _ = p.send("stop").await;
    }
    Ok(())
}

async fn run(c: Arc<Core>, id: String, p: Arc<Proc>, root: PathBuf, plan: Plan) {
    if let Err(e) = launch(&c, &p, &root, &plan).await {
        p.fail(&format!("Не удалось запустить: {e}"));
        return;
    }
    let code = p.wait().await;
    *p.stdin.lock().await = None;
    let auto = find(&c, &id).map(|s| s.auto_restart).unwrap_or(false);
    let closing = c.server_procs.closing.load(Ordering::SeqCst);
    if let Some(d) = p.exited(code, auto, closing) {
        tokio::time::sleep(d).await;
        if c.server_procs.closing.load(Ordering::SeqCst) || p.alive() {
            return;
        }
        if let Err(e) = start(&c, &id, false) {
            p.note(&format!("Не удалось запустить снова: {e}"));
        }
    }
}

/// Whether the launcher runs any server now (quitting has to stop them).
pub fn any_running(c: &Core) -> bool {
    !c.server_procs.running().is_empty()
}

/// Stops every server the launcher runs (on quit): `stop`, then kill.
pub async fn stop_all(c: &Core) {
    let procs = &c.server_procs;
    procs.closing.store(true, Ordering::SeqCst);
    let all: Vec<Arc<Proc>> = lock(&procs.map).values().cloned().collect();
    let alive: Vec<Arc<Proc>> = all.into_iter().filter(|p| p.alive()).collect();
    for p in &alive {
        p.request_stop().await;
    }
    let end = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let left = alive.iter().filter(|p| {
            let mut g = lock(&p.child);
            matches!(g.as_mut().map(|c| c.try_wait()), Some(Ok(None)))
        });
        if left.count() == 0 || tokio::time::Instant::now() >= end {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    for p in &alive {
        p.kill();
    }
}

pub(super) fn register(r: &mut ActionRegistry) {
    r.register("servers.start", P::Servers, false, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            id: String,
            /// The user accepted the Minecraft EULA in the start dialog.
            #[serde(default)]
            eula: bool,
        }
        let A { id, eula } = args(a)?;
        if eula {
            let root = find(&c, &id)?.root()?;
            if !eula_ok(&root) {
                std::fs::write(root.join("eula.txt"), EULA_TEXT)?;
            }
        }
        start(&c, &id, true)
    });

    r.register("servers.stop", P::Servers, false, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            id: String,
            /// Kill at once, without saving.
            #[serde(default)]
            force: bool,
        }
        let A { id, force } = args(a)?;
        let p = c.server_procs.get(&id).filter(|p| p.alive()).ok_or_else(|| bad("сервер не запущен"))?;
        p.request_stop().await;
        if force {
            p.say("Завершаем сервер принудительно, без сохранения");
            p.kill();
        }
        Ok(p.status())
    });

    r.register("servers.restart", P::Servers, false, |c, a| async move {
        let IdArg { id } = args(a)?;
        match c.server_procs.get(&id).filter(|p| p.alive()) {
            Some(p) => {
                lock(&p.st).restart_after = true;
                p.request_stop().await;
                Ok(p.status())
            }
            None => start(&c, &id, true),
        }
    });

    r.register("servers.status", P::Servers, false, |c, a| async move {
        let IdArg { id } = args(a)?;
        Ok(c.server_procs.status(&id))
    });

    r.register("servers.running", P::Servers, false, |c, _| async move {
        Ok(json!(c.server_procs.running()))
    });

    r.register("servers.console", P::Servers, false, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            id: String,
            #[serde(default)]
            from: u64,
        }
        let A { id, from } = args(a)?;
        let status = c.server_procs.status(&id);
        let mut v = match c.server_procs.get(&id) {
            Some(p) => p.lines(from),
            None => json!({ "lines": [], "cursor": 0, "reset": from > 0, "dropped": false }),
        };
        v["status"] = status;
        Ok(v)
    });

    r.register("servers.command", P::Servers, false, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            id: String,
            command: String,
        }
        let A { id, command } = args(a)?;
        let cmd = command.trim().trim_start_matches('/').trim();
        if cmd.is_empty() || cmd.len() > COMMAND_MAX || cmd.chars().any(char::is_control) {
            return Err(bad(format!("команда: одна строка до {COMMAND_MAX} символов")));
        }
        let p = c.server_procs.get(&id).filter(|p| p.alive() && p.has_child()).ok_or_else(|| bad("сервер не запущен"))?;
        lock(&p.st).push(format!("> {cmd}"));
        if cmd == "stop" || cmd == "end" {
            lock(&p.st).stop_requested = true;
        }
        p.send(cmd).await?;
        Ok(json!({ "ok": true }))
    });

    r.register("servers.autorestart.set", P::Servers, false, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            id: String,
            on: bool,
        }
        let A { id, on } = args(a)?;
        let _g = c.state_lock.lock().await;
        let mut st = super::load(&c)?;
        let s = st.servers.iter_mut().find(|s| s.id == id).ok_or_else(|| bad("нет такого сервера"))?;
        s.auto_restart = on;
        crate::store::save(&c.path(super::FILE), &st, false)?;
        Ok(json!({ "ok": true }))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_events() {
        assert!(matches!(event("[12:00:01] [Server thread/INFO]: Done (3.512s)! For help, type \"help\""), Some(Event::Ready)));
        assert!(matches!(event("[12:00:01 INFO]: Done (9.1s)! For help, type \"help\""), Some(Event::Ready)));
        assert!(matches!(event("[12:00:02] [Server thread/INFO]: Steve joined the game"), Some(Event::Join(n)) if n == "Steve"));
        assert!(matches!(event("[12:00:02 INFO]: Alex_2 left the game"), Some(Event::Leave(n)) if n == "Alex_2"));
        assert!(event("[12:00:02] [Server thread/INFO]: <Steve> Bob joined the game").is_none());
        assert!(matches!(event("[12:00:03] [Server thread/INFO]: Saved the game"), Some(Event::Saved)));
        assert!(matches!(event("[12:00:04] [Server thread/INFO]: There are 2 of a max of 20 players online: Steve, Alex"), Some(Event::Online(v)) if v == ["Steve", "Alex"]));
        assert!(matches!(event("[12:00:05] [Server thread/WARN]: **** FAILED TO BIND TO PORT!"), Some(Event::PortBusy)));
        assert_eq!(strip_ansi("\u{1b}[33;1m[INFO]\u{1b}[m hi\r"), "[INFO] hi");
        assert_eq!(decode(&[0xcf, 0xf0, 0xe8, 0xe2, 0xe5, 0xf2]), "Привет");
        assert_eq!(decode("Привет".as_bytes()), "Привет");
    }

    #[test]
    fn log_ring_and_pages() {
        let p = Proc::default();
        assert!(p.begin(true));
        assert!(!p.begin(true));
        for i in 0..(LOG_MAX + 10) {
            p.on_line(&format!("line {i}"));
        }
        let v = p.lines(0);
        assert_eq!(v["lines"].as_array().unwrap().len(), PAGE_MAX);
        let cur = v["cursor"].as_u64().unwrap();
        p.on_line("[x] [Server thread/INFO]: Steve joined the game");
        let v = p.lines(cur);
        assert_eq!(v["lines"].as_array().unwrap().len(), 1);
        assert_eq!(p.status()["online"], 1);
        assert_eq!(p.lines(cur + 100)["reset"], true);
        // A crash restarts by itself, but not forever.
        assert!(p.exited(Some(1), true, false).is_some());
        assert_eq!(p.state(), "crashed");
        for _ in 0..CRASH_MAX {
            assert!(p.begin(false));
            let _ = p.exited(Some(1), true, false);
        }
        assert!(p.begin(false));
        assert!(p.exited(Some(1), true, false).is_none(), "too many crashes");
        assert!(p.begin(true));
        assert!(p.exited(Some(1), true, true).is_none(), "no restarts while quitting");
        assert!(p.begin(true));
        lock(&p.st).stop_requested = true;
        assert!(p.exited(Some(1), true, false).is_none());
        assert_eq!(p.state(), "stopped");
    }

    #[test]
    fn plans() {
        let d = std::env::temp_dir().join(format!("fl-plan-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        let s = Server {
            id: "a".into(),
            name: "A".into(),
            host: super::super::Host::Local { dir: d.to_string_lossy().into_owned() },
            created_at: 0,
            core: None,
            memory_mb: Some(3072),
            auto_restart: false,
        };
        assert!(plan(&s, &d).is_err());
        std::fs::write(d.join("paper-1.21.4-232.jar"), "x").unwrap();
        assert_eq!(plan(&s, &d).unwrap(), Plan { mc: "1.21.4".into(), launch: vec!["-jar".into(), "paper-1.21.4-232.jar".into()], memory_mb: 3072 });
        let os = if cfg!(windows) { "win" } else { "unix" };
        std::fs::create_dir_all(d.join("libraries/net/neoforged/neoforge/21.1.77")).unwrap();
        std::fs::write(d.join(format!("libraries/net/neoforged/neoforge/21.1.77/{os}_args.txt")), "x").unwrap();
        assert_eq!(plan(&s, &d).unwrap().launch, vec![format!("@libraries/net/neoforged/neoforge/21.1.77/{os}_args.txt")]);
        assert!(!eula_ok(&d));
        std::fs::write(d.join("eula.txt"), EULA_TEXT).unwrap();
        assert!(eula_ok(&d));
        std::fs::remove_dir_all(d).unwrap();
    }
}