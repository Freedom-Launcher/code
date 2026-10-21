'use strict';
// «Управление серверами»: a server is like an instance, but a server. A list of
// the user's servers, an add wizard (where it is hosted → a few steps), and per
// server: overview, players, files and settings.
(() => {
  const app = document.getElementById('app');
  const modal = document.getElementById('modal');
  let servers = [];
  let cur = null; // server id
  let tab = 'overview';
  let info = null;
  let filePath = '';
  let playerList = 'whitelist';
  let busy = false; // an install runs: the sheet stays open

  // ---------- helpers ----------
  function h(tag, cls, ...kids) {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    for (const k of kids.flat()) if (k != null && k !== false) n.append(k);
    return n;
  }
  const btn = (label, cls, onclick, title) => {
    const b = h('button', `btn ${cls || ''}`.trim(), label);
    b.type = 'button';
    if (title) b.title = title;
    b.onclick = onclick;
    return b;
  };
  const errText = (e) => String(e?.message || e).replace(/^(invalid arguments|permission denied|file error|network error|data error): /, '');
  const fail = (e) => fl.toast(errText(e));
  const call = (name, args) => fl.call(name, args);
  const plural = (n, one, few, many) => {
    const a = Math.abs(n) % 100, b = a % 10;
    return a > 10 && a < 20 ? many : b === 1 ? one : b > 1 && b < 5 ? few : many;
  };
  const size = (b) => (b < 1024 ? `${b} Б` : b < 1048576 ? `${(b / 1024).toFixed(1)} КБ` : b < 1073741824 ? `${(b / 1048576).toFixed(1)} МБ` : `${(b / 1073741824).toFixed(2)} ГБ`);
  const date = (s) => (s ? new Date(s * 1000).toLocaleString('ru-RU', { day: 'numeric', month: 'short', year: 'numeric', hour: '2-digit', minute: '2-digit' }) : '');
  const hue = (s) => [...String(s)].reduce((a, c) => (a * 31 + c.charCodeAt(0)) % 360, 7);

  function ICO(name) {
    const P = {
      plus: 'M12 5v14M5 12h14',
      back: 'M15 6l-6 6 6 6',
      folder: 'M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z',
      file: 'M7 3h7l5 5v13H7zM14 3v5h5',
      trash: 'M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13',
      pc: 'M3 5h18v11H3zM8 20h8M12 16v4',
      cloud: 'M7 18h10a4 4 0 0 0 .5-8A6 6 0 0 0 6 9.5 4.5 4.5 0 0 0 7 18z',
      remote: 'M4 4h16v6H4zM4 14h16v6H4zM8 7h.01M8 17h.01',
      user: 'M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM4 21a8 8 0 0 1 16 0',
      edit: 'M4 20h4L19 9l-4-4L4 16zM14 6l4 4',
      img: 'M4 5h16v14H4zM4 15l4-4 4 4 3-3 5 5M15 9h.01',
      check: 'M5 12l5 5 9-10',
      warn: 'M12 3l10 18H2zM12 10v4M12 17h.01',
      download: 'M12 4v11M7 10l5 5 5-5M5 20h14',
      swap: 'M7 7h11l-3-3M17 17H6l3 3',
      play: 'M7 5l12 7-12 7z',
      stop: 'M6 6h12v12H6z',
      restart: 'M4 12a8 8 0 1 0 2.4-5.7M4 4v4h4',
      term: 'M4 5h16v14H4zM7 9l3 3-3 3M12 15h5',
      archive: 'M3 5h18v4H3zM5 9v10h14V9M10 13h4',
      send: 'M4 12l16-8-6 16-3-7z',
    };
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('viewBox', '0 0 24 24');
    svg.setAttribute('class', 'ico');
    const p = document.createElementNS('http://www.w3.org/2000/svg', 'path');
    p.setAttribute('d', P[name] || '');
    svg.append(p);
    return svg;
  }

  function serverIcon(s, big) {
    const box = h('div', `sicon${big ? ' big' : ''}`);
    if (s.icon) {
      const img = h('img');
      img.src = s.icon;
      img.alt = '';
      box.append(img);
    } else {
      box.classList.add('empty');
      box.style.setProperty('--h', hue(s.id || s.name));
      box.append(([...String(s.name || '?').trim()][0] || '?').toUpperCase());
    }
    return box;
  }

  const softLabel = (sw) => (sw ? [sw.kind, sw.version].filter(Boolean).join(' ') : 'Ядро не найдено');

  // ---------- modal ----------
  function openModal(...kids) {
    modal.replaceChildren(h('div', 'sheet', ...kids));
    modal.hidden = false;
    modal.onclick = (e) => { if (e.target === modal && !busy) closeModal(); };
  }
  function closeModal() { modal.hidden = true; modal.replaceChildren(); }
  document.addEventListener('keydown', (e) => { if (e.key === 'Escape' && !modal.hidden && !busy) closeModal(); });

  function confirm(title, text, okLabel, danger) {
    return new Promise((resolve) => {
      const done = (v) => { closeModal(); resolve(v); };
      openModal(
        h('h2', null, title),
        h('p', 'muted', text),
        h('div', 'row end', btn('Отмена', 'ghost', () => done(false)), btn(okLabel, danger ? 'danger' : 'primary', () => done(true))),
      );
    });
  }

  // ---------- running ----------
  const RUN = { starting: 'Запускается', running: 'Работает', stopping: 'Останавливается', stopped: 'Остановлен', crashed: 'Остановился с ошибкой' };
  const alive = (r) => ['starting', 'running', 'stopping'].includes(r?.state);
  let timer = null;
  let onStatus = null; // the open tab redraws itself on a status change
  let conPoll = null;
  const stopTimer = () => { if (timer) clearInterval(timer); timer = null; };

  function runPill(r) {
    const st = r?.state || 'stopped';
    const n = r?.online || 0;
    return h('span', `run-pill ${st}`, h('i'), st === 'running' && n ? `${RUN[st]} · ${n} ${plural(n, 'игрок', 'игрока', 'игроков')}` : RUN[st]);
  }

  // ---------- list ----------
  async function loadList() {
    try { servers = await call('servers.list'); } catch (e) { servers = []; fail(e); }
  }

  async function showList() {
    stopTimer();
    cur = null; info = null; onStatus = null; conPoll = null;
    await loadList();
    const head = h('header', 'top',
      h('div', null, h('h1', null, 'Серверы'), h('p', 'muted', servers.length ? `${servers.length} ${plural(servers.length, 'сервер', 'сервера', 'серверов')}` : 'Здесь будут ваши серверы Minecraft')),
      btn([ICO('plus'), 'Добавить сервер'], 'primary', addWizard));
    const grid = h('section', 'grid');
    for (const s of servers) grid.append(card(s));
    if (!servers.length) {
      grid.append(h('div', 'empty-state',
        h('div', 'empty-art', ICO('pc')),
        h('h2', null, 'Пока ни одного сервера'),
        h('p', 'muted', 'Создайте новый сервер с нужным ядром и версией или добавьте готовую папку. Файлы, игроки и настройки — отсюда.'),
        btn([ICO('plus'), 'Добавить сервер'], 'primary', addWizard)));
    }
    app.replaceChildren(head, grid);
    // Running servers update their chips by themselves.
    if (servers.some((s) => alive(s.run))) {
      timer = setInterval(async () => {
        if (cur) return stopTimer();
        await loadList();
        if (cur || !grid.isConnected) return;
        grid.replaceChildren(...servers.map(card));
        if (!servers.some((s) => alive(s.run))) stopTimer();
      }, 3000);
    }
  }

  function card(s) {
    const c = h('button', `scard${s.exists ? '' : ' missing'}`);
    c.type = 'button';
    c.onclick = () => openServer(s.id);
    c.append(
      h('div', 'scard-mc', serverIcon(s), h('div', 'scard-text',
        h('div', 'scard-name', s.name),
        MOTD.render(s.motd))),
      h('div', 'scard-meta',
        s.run && s.run.state !== 'stopped' ? h('span', `chip run ${s.run.state}`, h('i'), s.run.state === 'running' ? `Работает · ${s.run.online}/${s.max_players}` : RUN[s.run.state]) : null,
        h('span', 'chip', s.exists ? softLabel(s.software) : 'Папка не найдена'),
        h('span', 'chip', ICO('pc'), 'Этот компьютер'),
        h('span', 'chip', `:${s.port}`),
        h('span', 'chip', ICO('user'), `до ${s.max_players}`)));
    return c;
  }

  // ---------- add wizard ----------
  function addWizard() {
    const HOSTS = [
      { id: 'local', icon: 'pc', title: 'Этот компьютер', text: 'Сервер в папке на этом ПК: готовый или из сборки.' },
      { id: 'panel', icon: 'cloud', title: 'Хостинг', text: 'Панель хостинга: Aternos, Pterodactyl и другие.', soon: true },
      { id: 'remote', icon: 'remote', title: 'Свой VPS или другой ПК', text: 'По SFTP или SSH.', soon: true },
    ];
    const list = h('div', 'hosts');
    for (const o of HOSTS) {
      const b = h('button', `host${o.soon ? ' soon' : ''}`, h('div', 'host-ico', ICO(o.icon)),
        h('div', null, h('b', null, o.title, o.soon ? h('span', 'badge', 'скоро') : null), h('p', 'muted', o.text)));
      b.type = 'button';
      b.disabled = !!o.soon;
      b.onclick = () => localChoice();
      list.append(b);
    }
    openModal(h('div', 'steps', h('i', 'on'), h('i'), h('i')), h('h2', null, 'Где работает сервер?'), list,
      h('div', 'row end', btn('Отмена', 'ghost', closeModal)));
  }

  function localChoice() {
    const OPTS = [
      { icon: 'download', title: 'Новый сервер', text: 'Выберите ядро и версию Minecraft: лаунчер скачает всё сам и подготовит запуск.', go: newServer },
      { icon: 'folder', title: 'Готовая папка', text: 'Сервер уже есть на этом ПК: добавить его папку как есть.', go: localStep },
    ];
    const list = h('div', 'hosts');
    for (const o of OPTS) {
      const b = h('button', 'host', h('div', 'host-ico', ICO(o.icon)), h('div', null, h('b', null, o.title), h('p', 'muted', o.text)));
      b.type = 'button';
      b.onclick = o.go;
      list.append(b);
    }
    openModal(h('div', 'steps', h('i', 'on'), h('i', 'on'), h('i')), h('h2', null, 'Сервер на этом компьютере'), list,
      h('div', 'row end', btn('Назад', 'ghost', addWizard)));
  }

  async function newServer() {
    const name = h('input', 'input');
    name.placeholder = 'Например, «Ванилла с друзьями»';
    name.maxLength = 64;
    let form;
    try { form = await coreForm(); } catch (e) { return fail(e); }
    const ram = ramSelect(2048);
    let where = 'launcher';
    const seg = h('nav', 'seg');
    for (const [id, label] of [['launcher', 'В папке лаунчера'], ['pick', 'Выбрать папку…']]) {
      const b = h('button', null, label);
      b.type = 'button';
      b.setAttribute('aria-selected', String(id === where));
      b.onclick = () => { where = id; for (const x of seg.children) x.setAttribute('aria-selected', String(x === b)); };
      seg.append(b);
    }
    const eula = h('label', 'switch', h('input'), h('i'), h('span', null, 'Принимаю EULA Minecraft (aka.ms/MinecraftEULA): без этого сервер не запустится'));
    const eulaBox = eula.querySelector('input');
    eulaBox.type = 'checkbox';
    const go = btn([ICO('download'), 'Создать сервер'], 'primary', async () => {
      if (!form.ready()) return fl.toast('Выберите версию Minecraft');
      const job = newJob();
      const a = { name: name.value.trim(), ...form.value(), memory_mb: Number(ram.value), eula: eulaBox.checked, job };
      if (where === 'pick') a.choose_dir = true;
      const r = await runJob(job, `Создаём сервер ${form.label()}`, () => call('servers.create', a), where === 'pick');
      if (!r.ok) return fail(r.error);
      if (!r.value) return fl.toast('Папку не выбрали');
      closeModal();
      fl.toast('Сервер готов');
      await openServer(r.value.id);
    });
    openModal(h('div', 'steps', h('i', 'on'), h('i', 'on'), h('i', 'on')),
      h('h2', null, 'Новый сервер'),
      h('label', 'field', h('span', null, 'Название в лаунчере'), name),
      form.node,
      h('div', 'frow2', h('label', 'field', h('span', null, 'Память для сервера'), ram),
        h('div', 'field', h('span', null, 'Где создать'), seg)),
      eula,
      h('div', 'row end', btn('Назад', 'ghost', localChoice), go));
    modal.firstChild.classList.add('wide');
    name.focus();
  }

  function localStep() {
    const name = h('input', 'input');
    name.placeholder = 'Например, «Ванилла с друзьями»';
    name.maxLength = 64;
    const pick = btn([ICO('folder'), 'Выбрать папку сервера'], 'primary', async () => {
      pick.disabled = true;
      try {
        const r = await call('servers.pick', { name: name.value.trim() });
        if (!r) return;
        closeModal();
        if (r.existing) fl.toast('Этот сервер уже в списке');
        await openServer(r.id);
      } catch (e) { fail(e); } finally { pick.disabled = false; }
    });
    openModal(h('div', 'steps', h('i', 'on'), h('i', 'on'), h('i', 'on')),
      h('h2', null, 'Готовая папка сервера'),
      h('label', 'field', h('span', null, 'Название в лаунчере'), name),
      h('p', 'muted small', 'Выберите папку, где лежит сервер: там обычно есть server.properties и jar-файл ядра. Если поле названия пустое, возьмём имя папки.'),
      h('div', 'row end', btn('Назад', 'ghost', localChoice), pick));
    name.focus();
  }

  // ---------- cores ----------
  let coreList = null;
  const verCache = new Map();
  const RAM = [1024, 1536, 2048, 3072, 4096, 6144, 8192, 12288, 16384];
  const gb = (mb) => `${mb % 1024 ? (mb / 1024).toFixed(1) : mb / 1024} ГБ`;
  const ADDS = { plugins: 'плагины', mods: 'моды', '': 'без модов' };
  const STAGE = {
    meta: 'Ищем ядро…', java: 'Готовим Java…', download: 'Скачиваем файлы…', install: 'Устанавливаем ядро: это может занять минуту…',
    finish: 'Пишем скрипт запуска…', done: 'Готово', failed: 'Не получилось',
  };
  const newJob = () => `j${Date.now().toString(36)}${Math.random().toString(36).slice(2, 7)}`;
  const opt = (v, t) => { const o = h('option', null, t); o.value = v; return o; };
  const loading = (sel) => { sel.replaceChildren(opt('', 'Загрузка…')); sel.disabled = true; };

  async function cores() {
    if (!coreList) coreList = await call('servers.cores');
    return coreList;
  }
  async function mcVersions(core, all) {
    const k = `${core}|${all}`;
    if (!verCache.has(k)) verCache.set(k, await call('servers.core.versions', { core, all }));
    return verCache.get(k);
  }

  function ramSelect(mb) {
    const sel = h('select', 'input');
    const list = RAM.includes(mb) ? RAM : [...RAM, mb].sort((a, b) => a - b);
    for (const m of list) sel.append(opt(String(m), gb(m)));
    sel.value = String(mb);
    return sel;
  }

  /** Core cards + Minecraft version + build. */
  async function coreForm(init = {}) {
    const list = await cores();
    let core = list.some((c) => c.id === init.core) ? init.core : 'paper';
    let firstBuild = init.build || null;
    let vTok = 0, bTok = 0;
    const grid = h('div', 'cores');
    const ver = h('select', 'input');
    const build = h('select', 'input');
    const buildLabel = h('span');
    const snaps = h('label', 'switch', h('input'), h('i'), h('span', 'small', 'Показать снапшоты и пре-релизы'));
    const snapBox = snaps.querySelector('input');
    snapBox.type = 'checkbox';
    snapBox.checked = !!init.mc && /[a-z]/i.test(init.mc);

    function drawCores() {
      grid.replaceChildren(...list.map((c) => {
        const b = h('button', 'core', h('b', null, c.name), h('span', 'core-adds', ADDS[c.adds] ?? ''), h('p', 'muted', c.text));
        b.type = 'button';
        b.setAttribute('aria-pressed', String(c.id === core));
        b.onclick = () => { if (core === c.id) return; core = c.id; drawCores(); loadVersions(ver.value); };
        return b;
      }));
      buildLabel.textContent = core === 'fabric' ? 'Версия загрузчика' : 'Сборка ядра';
    }
    async function loadVersions(want) {
      const my = ++vTok;
      loading(ver); loading(build);
      try {
        const vs = await mcVersions(core, snapBox.checked);
        if (my !== vTok) return;
        ver.replaceChildren(...vs.map((v) => opt(v.id, v.stable ? v.id : `${v.id} · тестовая`)));
        const pick = want && vs.some((v) => v.id === want) ? want : (vs.find((v) => v.stable) || vs[0])?.id;
        if (!pick) { ver.replaceChildren(opt('', 'Нет версий')); build.replaceChildren(); return; }
        ver.value = pick;
        ver.disabled = false;
        const b = firstBuild && pick === init.mc && core === init.core ? firstBuild : null;
        firstBuild = null;
        await loadBuilds(b);
      } catch (e) {
        if (my !== vTok) return;
        ver.replaceChildren(opt('', 'Не удалось загрузить'));
        build.replaceChildren();
        fail(e);
      }
    }
    async function loadBuilds(want) {
      const my = ++bTok;
      if (core === 'vanilla') { build.replaceChildren(opt('', 'Не нужна')); build.disabled = true; return; }
      loading(build);
      try {
        const r = await call('servers.core.builds', { core, mc: ver.value });
        if (my !== bTok) return;
        build.replaceChildren(opt('', `Рекомендуемая${r.recommended ? ` (${r.recommended})` : ''}`),
          ...r.builds.map((b) => opt(b.id, b.stable ? b.id : `${b.id} · тестовая`)));
        build.value = want && r.builds.some((b) => b.id === want) ? want : '';
        build.disabled = false;
      } catch (e) {
        if (my !== bTok) return;
        build.replaceChildren(opt('', 'Для этой версии нет сборок'));
        build.disabled = true;
      }
    }
    ver.onchange = () => loadBuilds(null);
    snapBox.onchange = () => loadVersions(ver.value);
    drawCores();
    loadVersions(init.mc);
    return {
      node: h('div', 'core-form',
        h('div', 'field', h('span', null, 'Ядро'), grid),
        h('div', 'frow2', h('label', 'field', h('span', null, 'Версия Minecraft'), ver), h('label', 'field', buildLabel, build)),
        snaps),
      ready: () => !!ver.value && !ver.disabled && (core === 'vanilla' || !build.disabled),
      value: () => ({ core, mc: ver.value, build: build.value || null }),
      label: () => `${list.find((c) => c.id === core)?.name || core} ${ver.value}`,
    };
  }

  /** Runs an install, showing its progress in the sheet; the sheet comes back after. */
  async function runJob(job, title, fn, picking) {
    const sheet = modal.firstChild;
    const stage = h('p', 'muted', picking ? 'Выберите папку в открывшемся окне…' : 'Начинаем…');
    const bar = h('div', 'bar busy', h('i'));
    const count = h('small', 'muted', '');
    busy = true;
    modal.replaceChildren(h('div', 'sheet', h('h2', null, title), stage, bar, count,
      h('p', 'muted small', 'Ядро и Java скачиваются с официальных сайтов и проверяются по контрольным суммам.')));
    const t = setInterval(async () => {
      const p = await call('servers.progress', { job }).catch(() => null);
      if (!p) return;
      stage.textContent = STAGE[p.stage] || 'Готовим…';
      const known = p.total_bytes > 0 && (p.stage === 'download' || p.stage === 'java');
      bar.classList.toggle('busy', !known);
      bar.firstChild.style.width = known ? `${Math.min(100, (p.done_bytes / p.total_bytes) * 100)}%` : '';
      count.textContent = known ? `${size(p.done_bytes)} из ${size(p.total_bytes)}` : '';
    }, 300);
    try { return { ok: true, value: await fn() }; } catch (e) { return { ok: false, error: e }; } finally {
      clearInterval(t);
      busy = false;
      modal.replaceChildren(sheet);
    }
  }

  async function changeCore() {
    const s = info;
    const guess = s.software?.kind?.toLowerCase();
    let form;
    try {
      form = await coreForm(s.core ? { core: s.core.core, mc: s.core.mc, build: s.core.build } : { core: guess, mc: s.software?.version });
    } catch (e) { return fail(e); }
    const go = btn([ICO('download'), 'Установить'], 'primary', async () => {
      if (!form.ready()) return fl.toast('Выберите версию Minecraft');
      const job = newJob();
      const r = await runJob(job, `Ставим ${form.label()}`, () => call('servers.core.install', { id: cur, ...form.value(), job }));
      if (!r.ok) return fail(r.error);
      closeModal();
      fl.toast(r.value.removed ? `Ядро установлено, старое (${r.value.removed}) удалено` : 'Ядро установлено');
      await loadInfo(); render();
    });
    openModal(h('h2', null, s.core ? 'Сменить или обновить ядро' : 'Поставить ядро'), form.node,
      h('div', 'notice', ICO('warn'), h('p', null, 'Мир, плагины, моды и настройки останутся. Старый jar ядра удалится, когда новое встанет. Плагины и моды другого ядра или другой версии могут не подойти; перед переходом на версию старее сделайте копию мира.')),
      h('div', 'row end', btn('Отмена', 'ghost', closeModal), go));
    modal.firstChild.classList.add('wide');
  }

  function corePanel(s) {
    const c = s.core;
    const panel = h('div', 'panel', h('h2', null, 'Ядро и запуск'));
    const running = alive(s.run);
    const mem = s.memory_mb || c?.memory_mb || 2048;
    const ram = ramSelect(mem);
    ram.onchange = async () => {
      ram.disabled = true;
      try {
        await call('servers.memory.set', { id: cur, memory_mb: Number(ram.value) });
        fl.toast(running ? `Память: ${gb(Number(ram.value))}. Сработает после перезапуска сервера` : `Память: ${gb(Number(ram.value))}`);
        await loadInfo();
      } catch (e) { ram.value = String(mem); fail(e); } finally { ram.disabled = false; }
    };
    const change = (label, cls) => {
      const b = btn(label, cls, changeCore, running ? 'Сначала остановите сервер' : undefined);
      b.disabled = running;
      return b;
    };
    if (!c) {
      if (!s.software) {
        panel.append(h('div', 'core-line', h('p', 'muted grow', 'В папке нет ядра сервера.'), change([ICO('download'), 'Поставить ядро'], 'primary')));
        return panel;
      }
      panel.append(h('div', 'core-line',
        h('div', 'grow', h('b', null, softLabel(s.software)), h('p', 'muted small', 'Ядро найдено по файлам. Лаунчер запускает его сам, с этой памятью; ваши скрипты в папке не меняются.')),
        h('label', 'field inline', h('span', null, 'Память'), ram),
        change([ICO('download'), 'Поставить ядро'], null)), autoSwitch(s));
      return panel;
    }
    const sub = [c.build && (c.core === 'fabric' ? `загрузчик ${c.build}` : `сборка ${c.build}`), c.java_major && `Java ${c.java_major}`, c.installed_at && `поставлено ${date(c.installed_at)}`].filter(Boolean).join(' · ');
    panel.append(
      h('div', 'core-line',
        h('div', 'grow', h('b', null, `${s.software?.kind || c.core} ${c.mc}`), h('p', 'muted small', sub)),
        h('label', 'field inline', h('span', null, 'Память'), ram),
        change([ICO('swap'), 'Сменить или обновить'], null)),
      autoSwitch(s),
      s.script ? h('p', 'muted small', `Запускается кнопкой «Запустить» вверху, а без лаунчера — скриптом ${s.script} в папке сервера (Java и память уже прописаны).`) : null);
    return panel;
  }

  function autoSwitch(s) {
    const l = h('label', 'switch', h('input'), h('i'), h('span', null, 'Перезапускать после падения'));
    const box = l.querySelector('input');
    box.type = 'checkbox';
    box.checked = !!s.auto_restart;
    box.onchange = async () => {
      try {
        await call('servers.autorestart.set', { id: cur, on: box.checked });
        info.auto_restart = box.checked;
        fl.toast(box.checked ? 'Автоперезапуск включён' : 'Автоперезапуск выключен');
      } catch (e) { box.checked = !box.checked; fail(e); }
    };
    return h('div', 'col auto-line', l, h('p', 'muted small', 'Если сервер упадёт, лаунчер запустит его снова, но не больше 3 раз за 10 минут.'));
  }

  // ---------- start, stop, status ----------
  let ctl = null;
  function drawCtl() {
    if (!ctl || !info) return;
    const r = info.run || {};
    const st = r.state || 'stopped';
    const kids = [runPill(r)];
    if (!info.exists) {
      // nothing to start
    } else if (st === 'stopped' || st === 'crashed') {
      kids.push(btn([ICO('play'), 'Запустить'], 'primary', startServer));
    } else if (st === 'stopping') {
      kids.push(btn([ICO('stop'), 'Завершить сразу'], 'ghost danger-text', forceStop, 'Закрыть без сохранения мира'));
    } else {
      kids.push(btn([ICO('restart'), 'Перезапустить'], null, restartServer), btn([ICO('stop'), 'Остановить'], 'danger', stopServer));
    }
    ctl.replaceChildren(...kids);
  }

  function applyStatus(r) {
    if (!info || !r) return;
    const old = info.run || {};
    const key = (x) => JSON.stringify([x.state, x.players, x.note]);
    const changed = key(old) !== key(r);
    info.run = r;
    if (changed) {
      drawCtl();
      onStatus?.(r, old);
    }
  }

  function watch() {
    stopTimer();
    let busyTick = false;
    timer = setInterval(async () => {
      if (!cur) return stopTimer();
      if (busyTick) return;
      busyTick = true;
      try {
        if (tab === 'console' && conPoll) await conPoll();
        else applyStatus(await call('servers.status', { id: cur }));
      } catch { /* the next tick tries again */ } finally { busyTick = false; }
    }, tab === 'console' ? 700 : 1500);
  }

  async function startServer() {
    let eula = false;
    if (!info.eula) {
      const ok = await confirm('Принять EULA Minecraft?', 'Без принятого лицензионного соглашения Mojang для серверов (aka.ms/MinecraftEULA) сервер не запустится. Лаунчер запишет eula=true в eula.txt и запустит сервер.', 'Принимаю, запустить');
      if (!ok) return;
      eula = true;
    }
    try {
      info.run = await call('servers.start', { id: cur, eula });
      if (eula) info.eula = true;
      tab = 'console';
      render();
    } catch (e) { fail(e); }
  }
  async function stopServer() {
    try { applyStatus(await call('servers.stop', { id: cur })); } catch (e) { fail(e); }
  }
  async function forceStop() {
    const ok = await confirm('Завершить сервер сразу?', 'Сервер закроется без сохранения: изменения мира с последнего автосохранения пропадут.', 'Завершить', true);
    if (!ok) return;
    try { applyStatus(await call('servers.stop', { id: cur, force: true })); } catch (e) { fail(e); }
  }
  async function restartServer() {
    try { applyStatus(await call('servers.restart', { id: cur })); } catch (e) { fail(e); }
  }

  // ---------- one server ----------
  async function loadInfo() {
    info = await call('servers.info', { id: cur });
  }

  async function openServer(id, t = 'overview') {
    cur = id; tab = t; filePath = '';
    try { await loadInfo(); } catch (e) { fail(e); return showList(); }
    render();
  }

  const TABS = [['overview', 'Обзор'], ['console', 'Консоль'], ['players', 'Игроки'], ['backups', 'Резервные копии'], ['files', 'Файлы'], ['settings', 'Настройки']];

  function render() {
    const s = info;
    const head = h('header', 'shead',
      btn(ICO('back'), 'icon ghost', showList, 'Все серверы'),
      serverIcon(s, true),
      h('div', 'shead-text', h('h1', null, s.name), h('p', 'muted', s.exists ? `${softLabel(s.software)} · порт ${s.port}` : 'Папка сервера не найдена')),
      ctl = h('div', 'run-ctl'));
    drawCtl();
    const tabs = h('nav', 'tabs');
    for (const [id, label] of TABS) {
      const b = h('button', null, label);
      b.type = 'button';
      b.setAttribute('aria-selected', String(id === tab));
      b.disabled = !s.exists && id !== 'overview';
      b.onclick = () => { if (tab !== id) { tab = id; render(); } };
      tabs.append(b);
    }
    const body = h('section', 'body');
    app.replaceChildren(head, tabs, body);
    onStatus = null;
    conPoll = null;
    ({ overview, console: consoleTab, players, backups: backupsTab, files, settings })[tab](body);
    watch();
  }

  // ----- overview -----
  function overview(body) {
    const s = info;
    if (!s.exists) {
      body.append(h('div', 'notice warn', ICO('warn'), h('div', null, h('b', null, 'Папка не найдена'), h('p', null, s.dir))),
        h('div', 'row', btn([ICO('trash'), 'Убрать из списка'], 'danger', () => removeServer())));
      return;
    }
    const preview = h('div', 'mc-entry', serverIcon(s), h('div', 'mc-text',
      h('div', 'mc-top', h('span', 'mc-name', s.name), h('span', 'mc-count', `${s.run?.online || 0}/${s.max_players}`)),
      MOTD.render(s.motd)));
    const stats = h('div', 'stats',
      stat('Ядро', s.software?.kind || '—', s.software?.version ? `Minecraft ${s.software.version}` : s.software?.jar || 'jar не найден'),
      stat('Игроки', `до ${s.max_players}`, s.properties?.find((p) => p.key === 'white-list')?.value === 'true' ? 'вайтлист включён' : 'вход для всех'),
      stat('Мир', s.world?.name || 'world', s.world?.exists ? 'создан' : 'появится при первом запуске'),
      s.plugins ? stat('Плагины', String(s.plugins), 'в папке plugins') : stat('Моды', String(s.mods || 0), 'в папке mods'));
    const notes = [];
    const r = s.run || {};
    if (r.state === 'crashed' && r.note) notes.push(h('div', 'notice warn', ICO('warn'), h('p', null, r.note), btn([ICO('term'), 'Консоль'], 'small', () => { tab = 'console'; render(); })));
    else if (alive(r) && r.note) notes.push(h('div', 'notice', ICO('warn'), h('p', null, r.note)));
    if (r.state === 'running') {
      notes.push(h('div', 'panel', h('h2', null, r.players?.length ? `Сейчас на сервере: ${r.players.length}` : 'Сейчас на сервере никого'),
        r.players?.length ? h('div', 'online', ...r.players.map((n) => h('span', 'chip', avatar(n), n))) : h('p', 'muted small', `Подключиться: адрес localhost:${s.port} на этом компьютере или IP этого компьютера в вашей сети.`)));
    }
    onStatus = () => { if (tab === 'overview' && body.isConnected) { body.replaceChildren(); overview(body); } };
    if (!s.has_properties) notes.push(h('div', 'notice', ICO('warn'), h('p', null, 'server.properties ещё нет: сервер создаст его при первом запуске. Настройки, которые вы сохраните здесь, сервер возьмёт.')));
    if (!s.eula) notes.push(h('div', 'notice', ICO('warn'), h('p', null, 'EULA не принята (eula.txt). Без этого сервер не запустится.'),
      btn('Принять EULA', 'small', acceptEula)));
    body.append(
      h('div', 'panel', h('h2', null, 'Так сервер выглядит в списке серверов'), preview),
      stats, ...notes, corePanel(s),
      h('div', 'panel', h('h2', null, 'Где лежит'), h('div', 'row',
        h('div', 'host-line', ICO('pc'), h('div', null, h('b', null, s.managed ? 'Этот компьютер · папка лаунчера' : 'Этот компьютер'), h('p', 'muted mono', s.dir))))),
      h('div', 'row', btn([ICO('edit'), 'Название, иконка и MOTD'], null, () => { tab = 'settings'; render(); }),
        h('span', 'grow'), btn([ICO('trash'), 'Убрать из списка'], 'ghost danger-text', removeServer)));
  }

  function stat(label, value, sub) {
    return h('div', 'stat', h('span', null, label), h('b', null, value), h('small', null, sub));
  }

  async function acceptEula() {
    const ok = await confirm('Принять EULA Minecraft?', 'Это лицензионное соглашение Mojang для серверов: aka.ms/MinecraftEULA. Лаунчер запишет eula=true в eula.txt.', 'Принимаю');
    if (!ok) return;
    try {
      await call('servers.files.write', { id: cur, path: 'eula.txt', text: '#By changing the setting below to TRUE you are indicating your agreement to our EULA (https://aka.ms/MinecraftEULA).\neula=true\n' });
      await loadInfo(); render();
    } catch (e) { fail(e); }
  }

  async function removeServer() {
    const del = h('label', 'switch', h('input'), h('i'), h('span', null, 'Удалить и файлы: мир, настройки и ядро'));
    const box = del.querySelector('input');
    box.type = 'checkbox';
    const go = btn('Убрать', 'danger', () => done(true));
    box.onchange = () => { go.textContent = box.checked ? 'Удалить сервер' : 'Убрать'; };
    let done;
    const ok = await new Promise((resolve) => {
      done = (v) => { closeModal(); resolve(v); };
      openModal(h('h2', null, `Убрать «${info.name}» из списка?`),
        h('p', 'muted', info.managed ? 'Этот сервер создал лаунчер, его папка лежит в данных лаунчера. Без галочки файлы останутся там.' : 'Файлы сервера останутся в папке, удаляется только запись в лаунчере.'),
        info.managed ? del : null,
        h('div', 'row end', btn('Отмена', 'ghost', () => done(false)), go));
    });
    if (!ok) return;
    try { await call('servers.remove', { id: cur, delete_files: info.managed && box.checked }); showList(); } catch (e) { fail(e); }
  }

  // ----- console -----
  const QUICK = [['list', 'Кто онлайн'], ['save-all', 'Сохранить мир'], ['weather clear', 'Ясная погода'], ['time set day', 'День']];
  const history = [];

  function consoleTab(body) {
    const out = h('pre', 'console');
    const empty = h('p', 'muted console-empty', 'Здесь появится вывод сервера. Нажмите «Запустить» вверху.');
    const input = h('input', 'input mono grow');
    input.placeholder = 'Команда сервера, например: op Steve';
    input.spellcheck = false;
    input.maxLength = 1000;
    const send = btn([ICO('send'), 'Отправить'], 'primary', () => run(input.value));
    const quick = h('div', 'row quick', ...QUICK.map(([c, l]) => btn(l, 'small ghost', () => run(c), `/${c}`)));
    const bar = h('div', 'row console-bar', h('span', 'prompt mono', '>'), input, send);
    let cursor = 0, stick = true, hi = -1, first = true;
    out.onscroll = () => { stick = out.scrollTop + out.clientHeight >= out.scrollHeight - 30; };

    const cls = (t) => (t.startsWith('[Лаунчер]') ? 'l-fl' : t.startsWith('> ') ? 'l-cmd' : /\b(ERROR|FATAL|SEVERE)\b|Exception|^\s+at /.test(t) ? 'l-err' : /\bWARN(ING)?\b/.test(t) ? 'l-warn' : null);
    function add(lines) {
      if (!lines.length) return;
      const frag = document.createDocumentFragment();
      for (const [, t] of lines) {
        const d = h('div', cls(t), t || '\u00a0');
        frag.append(d);
      }
      out.append(frag);
      while (out.childElementCount > 3000) out.firstElementChild.remove();
      empty.hidden = true;
      if (stick) out.scrollTop = out.scrollHeight;
    }
    function enable() {
      const r = info.run || {};
      const ok = r.state === 'starting' || r.state === 'running';
      input.disabled = !ok; send.disabled = !ok;
      for (const b of quick.children) b.disabled = r.state !== 'running';
      input.placeholder = ok ? 'Команда сервера, например: op Steve' : 'Команды работают, когда сервер запущен';
    }
    async function poll() {
      const v = await call('servers.console', { id: cur, from: cursor });
      if (!body.isConnected) return;
      if (v.reset) { out.replaceChildren(); }
      if (v.dropped && !first) out.append(h('div', 'l-fl', '[Лаунчер] … часть вывода пропущена'));
      add(v.lines);
      cursor = v.cursor;
      first = false;
      applyStatus(v.status);
      enable();
    }
    async function run(cmd) {
      cmd = cmd.trim().replace(/^\//, '');
      if (!cmd) return;
      if (cmd === 'stop') return stopServer();
      try {
        await call('servers.command', { id: cur, command: cmd });
        if (history[history.length - 1] !== cmd) history.push(cmd);
        if (history.length > 50) history.shift();
        hi = -1; input.value = ''; stick = true;
        await poll();
      } catch (e) { fail(e); }
    }
    input.onkeydown = (e) => {
      if (e.key === 'Enter') { e.preventDefault(); run(input.value); }
      else if (e.key === 'ArrowUp' && history.length) { e.preventDefault(); hi = hi < 0 ? history.length - 1 : Math.max(0, hi - 1); input.value = history[hi]; }
      else if (e.key === 'ArrowDown' && hi >= 0) { e.preventDefault(); hi = hi + 1 >= history.length ? -1 : hi + 1; input.value = hi < 0 ? '' : history[hi]; }
    };
    const copy = btn('Копировать всё', 'small ghost', async () => {
      try { await navigator.clipboard.writeText([...out.children].map((d) => d.textContent).join('\n')); fl.toast('Скопировано'); } catch { fl.toast('Не получилось скопировать'); }
    });
    body.append(h('div', 'panel console-panel',
      h('div', 'row', h('h2', 'grow', 'Консоль сервера'), copy),
      h('div', 'console-wrap', out, empty), bar, quick,
      h('p', 'muted small', 'Команды пишутся без «/». Стрелки ↑ и ↓ листают прошлые команды. Команда stop останавливает сервер с сохранением мира.')));
    enable();
    conPoll = poll;
    onStatus = () => enable();
    poll().catch(fail);
    setTimeout(() => input.focus(), 50);
  }

  // ----- backups -----
  async function backupsTab(body) {
    let v;
    try { v = await call('servers.backups', { id: cur }); } catch (e) { return fail(e); }
    if (!body.isConnected) return;
    const running = alive(info.run);
    const make = btn([ICO('archive'), 'Сделать копию сейчас'], 'primary', async () => {
      make.disabled = true;
      make.textContent = 'Копируем мир…';
      try {
        const r = await call('servers.backup.create', { id: cur });
        fl.toast(`Копия готова: ${size(r.size)}`);
      } catch (e) { fail(e); }
      if (body.isConnected) { body.replaceChildren(); backupsTab(body); }
    });
    make.disabled = !v.worlds.length || v.busy;
    const head = h('div', 'panel',
      h('div', 'row', h('div', 'grow', h('h2', null, 'Резервные копии мира'),
        h('p', 'muted small', v.worlds.length
          ? `Копируются папки ${v.worlds.join(', ')}. ${running ? 'Сервер работает: лаунчер сам сохранит мир перед копией, игроков не выкинет.' : ''}`
          : 'Мира ещё нет: он появится после первого запуска сервера.')), make),
      h('p', 'muted small mono', v.dir));
    const list = h('div', 'blist');
    if (!v.backups.length) list.append(h('p', 'empty', 'Копий пока нет.'));
    for (const b of v.backups) {
      const restore = btn('Восстановить', 'small', async () => {
        const ok = await confirm('Восстановить мир из копии?', `Текущий мир заменится копией от ${date(b.created)}. Перед этим лаунчер сохранит его отдельной копией, так что это можно отменить.`, 'Восстановить', true);
        if (!ok) return;
        try {
          const r = await call('servers.backup.restore', { id: cur, name: b.name });
          fl.toast(r.safety ? 'Мир восстановлен, прежний сохранён копией' : 'Мир восстановлен');
          await loadInfo();
        } catch (e) { fail(e); }
        if (body.isConnected) { body.replaceChildren(); backupsTab(body); }
      }, running ? 'Сначала остановите сервер' : undefined);
      restore.disabled = running || v.busy;
      const del = btn(ICO('trash'), 'icon ghost danger-text', async () => {
        const ok = await confirm('Удалить копию?', `${b.name} (${size(b.size)}) удалится насовсем.`, 'Удалить', true);
        if (!ok) return;
        try { await call('servers.backup.delete', { id: cur, name: b.name }); } catch (e) { fail(e); }
        if (body.isConnected) { body.replaceChildren(); backupsTab(body); }
      }, 'Удалить копию');
      list.append(h('div', 'brow',
        h('div', 'bico', ICO('archive')),
        h('div', 'grow', h('b', null, date(b.created)), h('p', 'muted small mono', b.name)),
        b.auto ? h('span', 'chip', 'перед восстановлением') : null,
        h('span', 'muted small', size(b.size)), restore, del));
    }
    body.append(head, h('div', 'panel', h('div', 'row', h('h2', 'grow', 'Копии'), v.backups.length ? h('span', 'muted small', `всего ${size(v.total)}`) : null), list));
    onStatus = (r, old) => { if (alive(r) !== alive(old) && body.isConnected) { body.replaceChildren(); backupsTab(body); } };
  }

  // ----- players -----
  const LISTS = [
    ['whitelist', 'Вайтлист'], ['ops', 'Операторы'], ['banned', 'Баны'], ['banned_ips', 'Баны по IP'], ['known', 'Заходили'],
  ];

  async function players(body) {
    body.append(h('p', 'muted', 'Загрузка…'));
    let p;
    try { p = await call('servers.players', { id: cur }); } catch (e) { body.replaceChildren(); return fail(e); }
    body.replaceChildren();

    const wl = h('label', 'switch', h('input'), h('i'), h('span', null, 'Вайтлист включён: зайти могут только игроки из списка'));
    const box = wl.querySelector('input');
    box.type = 'checkbox';
    box.checked = p.whitelist_enabled;
    box.onchange = async () => {
      try { await call('servers.properties.set', { id: cur, values: { 'white-list': box.checked, 'enforce-whitelist': box.checked } }); fl.toast(box.checked ? 'Вайтлист включён' : 'Вайтлист выключен'); }
      catch (e) { box.checked = !box.checked; fail(e); }
    };
    const top = h('div', 'panel row', wl);
    if (!p.online_mode) top.append(h('span', 'chip', 'online-mode=false'));

    const seg = h('nav', 'seg');
    for (const [id, label] of LISTS) {
      const b = h('button', null, label, h('span', 'count', String(p[id].length)));
      b.type = 'button';
      b.setAttribute('aria-selected', String(id === playerList));
      b.onclick = () => { playerList = id; body.replaceChildren(); players(body); };
      seg.append(b);
    }

    const editable = ['whitelist', 'ops', 'banned'].includes(playerList);
    const items = p[playerList] || [];
    const panel = h('div', 'panel');
    if (editable) panel.append(addForm(body));
    const rows = h('div', 'plist');
    for (const e of items) rows.append(playerRow(e, editable, body));
    if (!items.length) rows.append(h('p', 'empty', {
      whitelist: 'Список пуст. Добавьте ник, чтобы пустить игрока.',
      ops: 'Операторов нет.', banned: 'Банов нет.', banned_ips: 'Банов по IP нет.',
      known: 'На сервер ещё никто не заходил.',
    }[playerList]));
    panel.append(rows);
    body.append(top, seg, panel);
    if (playerList === 'banned_ips') body.append(h('p', 'muted small', 'Баны по IP ставятся командой /ban-ip на запущенном сервере.'));
  }

  function addForm(body) {
    const name = h('input', 'input');
    name.placeholder = 'Ник игрока';
    name.maxLength = 16;
    name.spellcheck = false;
    const reason = h('input', 'input');
    reason.placeholder = 'Причина (необязательно)';
    reason.maxLength = 120;
    const label = { whitelist: 'Добавить', ops: 'Сделать оператором', banned: 'Забанить' }[playerList];
    const go = btn([ICO('plus'), label], playerList === 'banned' ? 'danger' : 'primary', async () => {
      const n = name.value.trim();
      if (!n) return name.focus();
      go.disabled = true;
      try {
        const r = await call('servers.players.edit', { id: cur, list: playerList, op: 'add', name: n, reason: reason.value });
        fl.toast(r.existing ? `${n} уже в списке` : `${r.name || n}: готово`);
        body.replaceChildren(); players(body);
      } catch (e) { fail(e); go.disabled = false; }
    });
    name.onkeydown = (e) => { if (e.key === 'Enter') go.click(); };
    const f = h('div', 'addrow', name, playerList === 'banned' ? reason : null, go);
    return f;
  }

  function avatar(name, uuid) {
    const a = h('div', 'avatar', ([...String(name || '?')][0] || '?').toUpperCase());
    a.style.setProperty('--h', hue(uuid || name));
    return a;
  }

  function playerRow(e, editable, body) {
    const name = e.name || e.ip || '?';
    const sub = [];
    if (e.uuid) sub.push(e.uuid);
    if (playerList === 'ops' && e.level) sub.push(`уровень ${e.level}`);
    if ((playerList === 'banned' || playerList === 'banned_ips') && e.reason) sub.push(e.reason);
    if (playerList === 'known' && e.expiresOn) sub.push(`в кэше до ${e.expiresOn.slice(0, 10)}`);
    const row = h('div', 'prow', avatar(name, e.uuid), h('div', 'prow-text', h('b', null, name), h('small', 'mono', sub.join(' · '))));
    if (editable) {
      row.append(btn(ICO('trash'), 'icon ghost', async () => {
        const verb = { whitelist: 'Убрать из вайтлиста', ops: 'Снять оператора', banned: 'Разбанить' }[playerList];
        try { await call('servers.players.edit', { id: cur, list: playerList, op: 'remove', name: e.uuid || name }); fl.toast(`${name}: ${verb.toLowerCase()} — готово`); body.replaceChildren(); players(body); }
        catch (err) { fail(err); }
      }, { whitelist: 'Убрать', ops: 'Снять оператора', banned: 'Разбанить' }[playerList]));
    }
    return row;
  }

  // ----- files -----
  const TEXT = /\.(txt|properties|json|json5|yml|yaml|toml|cfg|conf|ini|log|md|mcmeta|snbt|csv|xml|lang|secret)$/i;
  const fileKind = (n) => (/\.(jar|zip)$/i.test(n) ? 'Архив' : /\.(png|jpe?g|gif|webp)$/i.test(n) ? 'Картинка' : /\.(dat|dat_old|mca|nbt)$/i.test(n) ? 'Данные' : TEXT.test(n) ? 'Текст' : '');

  async function files(body) {
    const crumbs = h('div', 'crumbs');
    const parts = filePath ? filePath.split('/') : [];
    const go = (p) => { filePath = p; render(); };
    const root = h('button', null, ICO('folder'), info.name);
    root.type = 'button';
    root.onclick = () => go('');
    crumbs.append(root);
    parts.forEach((part, i) => {
      const b = h('button', null, part);
      b.type = 'button';
      b.onclick = () => go(parts.slice(0, i + 1).join('/'));
      crumbs.append(h('span', 'sep', '/'), b);
    });
    const list = h('div', 'flist', h('p', 'muted', 'Загрузка…'));
    body.append(h('div', 'panel', crumbs, list));
    let items;
    try { items = await call('servers.files.list', { id: cur, path: filePath }); } catch (e) { list.replaceChildren(); return fail(e); }
    list.replaceChildren();
    if (filePath) {
      const up = h('button', 'frow', h('span', 'fico', ICO('back')), h('span', 'fname', '..'), h('span'), h('span'));
      up.type = 'button';
      up.onclick = () => go(parts.slice(0, -1).join('/'));
      list.append(up);
    }
    for (const f of items) {
      const path = filePath ? `${filePath}/${f.name}` : f.name;
      const r = h('button', `frow${f.dir ? ' dir' : ''}`,
        h('span', 'fico', ICO(f.dir ? 'folder' : 'file')),
        h('span', 'fname', f.name, !f.dir && fileKind(f.name) ? h('em', null, fileKind(f.name)) : null),
        h('span', 'fsize', f.dir ? '' : size(f.size)),
        h('span', 'fdate', date(f.modified)));
      r.type = 'button';
      r.onclick = () => (f.dir ? go(path) : openFile(path, f));
      list.append(r);
    }
    if (!items.length) list.append(h('p', 'empty', 'Папка пуста'));
  }

  async function openFile(path, f) {
    if (/\.(jar|zip|png|jpe?g|gif|webp|dat|dat_old|mca|nbt|exe|dll|so)$/i.test(f.name) || f.size > 2 * 1024 * 1024) {
      return fl.toast(f.size > 2 * 1024 * 1024 ? 'Файл больше 2 МБ, здесь его не открыть' : 'Это не текстовый файл');
    }
    let r;
    try { r = await call('servers.files.read', { id: cur, path }); } catch (e) { return fail(e); }
    const ta = h('textarea', 'editor');
    ta.value = r.text;
    ta.spellcheck = false;
    ta.readOnly = !r.writable;
    const save = btn('Сохранить', 'primary', async () => {
      save.disabled = true;
      try {
        await call('servers.files.write', { id: cur, path, text: ta.value });
        fl.toast('Сохранено');
        closeModal();
        if (path === 'server.properties' || path === 'eula.txt') await loadInfo();
      } catch (e) { fail(e); save.disabled = false; }
    });
    const del = btn([ICO('trash'), 'Удалить'], 'ghost danger-text', async () => {
      closeModal();
      if (!(await confirm(`Удалить ${f.name}?`, 'Файл удалится с диска насовсем.', 'Удалить', true))) return;
      try { await call('servers.files.delete', { id: cur, path }); render(); } catch (e) { fail(e); }
    });
    openModal(
      h('div', 'sheet-head', h('h2', 'mono', path), h('span', 'muted small', `${size(f.size)} · ${date(f.modified)}`)),
      r.writable ? null : h('div', 'notice', ICO('warn'), h('p', null, 'Этот файл только для просмотра: программы и моды плагины не меняют.')),
      ta,
      h('div', 'row', r.writable ? del : null, h('span', 'grow'), btn('Закрыть', 'ghost', closeModal), r.writable ? save : null));
    modal.firstChild.classList.add('wide');
  }

  // ----- settings -----
  const FIELDS = [
    ['Игра', [
      ['gamemode', 'Режим игры', 'select', [['survival', 'Выживание'], ['creative', 'Творческий'], ['adventure', 'Приключение'], ['spectator', 'Наблюдатель']], 'survival'],
      ['difficulty', 'Сложность', 'select', [['peaceful', 'Мирная'], ['easy', 'Лёгкая'], ['normal', 'Нормальная'], ['hard', 'Сложная']], 'easy'],
      ['hardcore', 'Хардкор', 'bool', null, 'false'],
      ['pvp', 'PvP между игроками', 'bool', null, 'true'],
      ['allow-flight', 'Разрешить полёт (нужно некоторым модам)', 'bool', null, 'false'],
      ['spawn-protection', 'Защита спавна, блоков', 'number', [0, 64], '16'],
    ]],
    ['Подключение', [
      ['max-players', 'Максимум игроков', 'number', [1, 1000], '20'],
      ['server-port', 'Порт', 'number', [1, 65535], '25565'],
      ['online-mode', 'Только лицензионные аккаунты', 'bool', null, 'true'],
      ['white-list', 'Вайтлист', 'bool', null, 'false'],
    ]],
    ['Мир', [
      ['level-name', 'Папка мира', 'text', null, 'world'],
      ['level-seed', 'Сид', 'text', null, ''],
      ['view-distance', 'Дальность прорисовки, чанков', 'number', [2, 32], '10'],
      ['simulation-distance', 'Дальность симуляции, чанков', 'number', [2, 32], '10'],
    ]],
  ];

  function settings(body) {
    const props = Object.fromEntries((info.properties || []).map((p) => [p.key, p.value]));
    const changed = {};
    const saveBar = h('div', 'savebar', h('span', null, 'Есть несохранённые изменения'),
      btn('Отменить', 'ghost', () => render()),
      btn('Сохранить', 'primary', async () => {
        try {
          if ('__name' in changed) {
            await call('servers.update', { id: cur, name: changed.__name });
            delete changed.__name;
          }
          if (Object.keys(changed).length) await call('servers.properties.set', { id: cur, values: changed });
          fl.toast('Сохранено');
          await loadInfo(); render();
        } catch (e) { fail(e); }
      }));
    saveBar.hidden = true;
    const mark = (k, v) => { changed[k] = v; saveBar.hidden = false; };

    // basics: name, icon, MOTD
    const nameIn = h('input', 'input');
    nameIn.value = info.name;
    nameIn.maxLength = 64;
    nameIn.oninput = () => mark('__name', nameIn.value);

    const iconBox = h('div', 'icon-edit', serverIcon(info, true));
    const fileIn = h('input');
    fileIn.type = 'file';
    fileIn.accept = 'image/png,image/jpeg,image/webp';
    fileIn.hidden = true;
    fileIn.onchange = () => fileIn.files[0] && setIcon(fileIn.files[0]);
    const iconBtns = h('div', 'col',
      btn([ICO('img'), info.icon ? 'Сменить иконку' : 'Выбрать иконку'], null, () => fileIn.click()),
      info.icon ? btn('Убрать', 'ghost', async () => { try { await call('servers.icon.delete', { id: cur }); await loadInfo(); render(); } catch (e) { fail(e); } }) : null,
      h('small', 'muted', 'Любая картинка, лаунчер сам сделает 64×64.'));

    const motdIn = h('textarea', 'input motd-in');
    motdIn.rows = 2;
    motdIn.value = String(props.motd ?? 'A Minecraft Server').replace(/\\n/g, '\n');
    const prev = h('div', 'mc-entry');
    const count = h('small', 'muted');
    const upd = () => {
      const lines = motdIn.value.split('\n');
      if (lines.length > 2) { motdIn.value = lines.slice(0, 2).join('\n'); }
      prev.replaceChildren(serverIcon(info), h('div', 'mc-text', h('div', 'mc-top', h('span', 'mc-name', nameIn.value || info.name), h('span', 'mc-count', `0/${props['max-players'] || 20}`)), MOTD.render(motdIn.value)));
      count.textContent = motdIn.value.split('\n').map((l) => `${MOTD.plain(l).length}/59`).join(' · ');
    };
    motdIn.oninput = () => { upd(); mark('motd', motdIn.value.replace(/\n/g, '\\n')); };
    nameIn.addEventListener('input', upd);
    const insert = (code) => {
      const a = motdIn.selectionStart, b = motdIn.selectionEnd;
      motdIn.setRangeText(`§${code}`, a, b, 'end');
      motdIn.focus();
      motdIn.oninput();
    };
    const palette = h('div', 'palette');
    for (const [c, label] of MOTD.NAMES) {
      const b = h('button', 'sw');
      b.type = 'button';
      b.title = label;
      b.style.background = MOTD.COLORS[c];
      b.onclick = () => insert(c);
      palette.append(b);
    }
    for (const [c, label, cls] of [['l', 'Жирный', 'B'], ['o', 'Курсив', 'I'], ['n', 'Подчёркнутый', 'U'], ['m', 'Зачёркнутый', 'S'], ['r', 'Сбросить', '↺']]) {
      const b = h('button', `fx fx-${c}`, cls);
      b.type = 'button';
      b.title = label;
      b.onclick = () => insert(c);
      palette.append(b);
    }
    upd();

    body.append(
      h('div', 'panel', h('h2', null, 'Основное'),
        h('div', 'basics', iconBox, iconBtns, fileIn,
          h('label', 'field grow', h('span', null, 'Название в лаунчере'), nameIn))),
      h('div', 'panel', h('h2', null, 'MOTD — описание в списке серверов'),
        prev, palette, motdIn, count));

    for (const [title, fields] of FIELDS) {
      const grid = h('div', 'fields');
      for (const [key, label, type, opt, def] of fields) grid.append(field(key, label, type, opt, props[key] ?? def, mark));
      body.append(h('div', 'panel', h('h2', null, title), grid));
    }

    // every property, for what the form above does not cover
    const known = new Set(['motd', ...FIELDS.flatMap(([, f]) => f.map((x) => x[0]))]);
    const rest = (info.properties || []).filter((p) => !known.has(p.key));
    if (rest.length) {
      const all = h('div', 'kv');
      for (const p of rest) {
        const inp = h('input', 'input mono');
        inp.value = p.value;
        inp.oninput = () => mark(p.key, inp.value);
        all.append(h('label', 'kv-row', h('span', 'mono', p.key), inp));
      }
      const det = h('details', 'panel', h('summary', null, `Все параметры server.properties (${rest.length})`), all);
      body.append(det);
    }
    body.append(saveBar);
  }

  function field(key, label, type, opt, value, mark) {
    if (type === 'bool') {
      const l = h('label', 'switch', h('input'), h('i'), h('span', null, label));
      const c = l.querySelector('input');
      c.type = 'checkbox';
      c.checked = String(value) === 'true';
      c.onchange = () => mark(key, c.checked);
      return l;
    }
    let inp;
    if (type === 'select') {
      inp = h('select', 'input');
      for (const [v, t] of opt) { const o = h('option', null, t); o.value = v; inp.append(o); }
      if (![...inp.options].some((o) => o.value === value)) { const o = h('option', null, value); o.value = value; inp.prepend(o); }
      inp.value = value;
      inp.onchange = () => mark(key, inp.value);
    } else {
      inp = h('input', 'input');
      inp.type = type === 'number' ? 'number' : 'text';
      if (opt) { inp.min = opt[0]; inp.max = opt[1]; }
      inp.value = value;
      inp.oninput = () => mark(key, inp.value);
    }
    return h('label', 'field', h('span', null, label), inp);
  }

  /** Any picture → a 64×64 PNG, centre-cropped. */
  function setIcon(file) {
    const fr = new FileReader();
    fr.onload = () => {
      const img = new Image();
      img.onload = async () => {
        const cv = document.createElement('canvas');
        cv.width = 64; cv.height = 64;
        const g = cv.getContext('2d');
        const s = Math.min(img.width, img.height);
        g.imageSmoothingEnabled = s > 64;
        g.imageSmoothingQuality = 'high';
        g.drawImage(img, (img.width - s) / 2, (img.height - s) / 2, s, s, 0, 0, 64, 64);
        try { await call('servers.icon.set', { id: cur, png: cv.toDataURL('image/png') }); fl.toast('Иконка обновлена'); await loadInfo(); render(); }
        catch (e) { fail(e); }
      };
      img.onerror = () => fl.toast('Не получилось открыть картинку');
      img.src = fr.result;
    };
    fr.readAsDataURL(file);
  }

  showList();
})();