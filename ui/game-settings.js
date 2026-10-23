'use strict';
// Settings → Игра, inline like Modrinth's "Instance defaults" + "Resources": Java per major version,
// memory, launch, window, hooks and the instances folder. Every change saves itself;
// each instance can override any block in its own settings.
(() => {
  const pane = $('#set-game');
  if (!pane) return;
  const MAJORS = [8, 17, 21, 25];
  const VARS = ['INST_ID', 'INST_NAME', 'INST_MC_VER', 'INST_DIR', 'INST_JAVA'];
  const WIN = /Windows/.test(navigator.userAgent);
  const st = { game: {}, javas: {}, found: null, mem: { total_mb: 8192, recommended_mb: 4096 } };
  const gb = (mb) => _t("{0} ГБ", [(mb / 1024).toFixed(1).replace('.0', '')]);
  const svg = (d) => `<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">${d}</svg>`;
  const I = {
    find: '<g fill="none" fill-rule="evenodd"><path fill="currentColor" d="M10.5 2a8.5 8.5 0 1 0 5.262 15.176l3.652 3.652a1 1 0 0 0 1.414-1.414l-3.652-3.652A8.5 8.5 0 0 0 10.5 2M4 10.5a6.5 6.5 0 1 1 13 0 6.5 6.5 0 0 1-13 0"/></g>',
    folder: '<g fill="none"><path fill="currentColor" d="M9.52 3a2 2 0 0 1 1.442.614l.12.137L12.48 5.5H19a2 2 0 0 1 1.995 1.85L21 7.5v1.53a2.001 2.001 0 0 1 1.61 2.398l-.038.147-2.4 8a2 2 0 0 1-1.752 1.418l-.164.007H4a2 2 0 0 1-1.995-1.85L2 19V5a2 2 0 0 1 1.85-1.995L4 3zm11.136 8H6.744l-2.4 8h13.912zM9.519 5H4v8.187l.828-2.762A2 2 0 0 1 6.744 9H19V7.5h-6.52a2 2 0 0 1-1.561-.75z"/></g>',
    down: '<g fill="none"><path fill="currentColor" d="M20 15a1 1 0 0 1 1 1v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4a1 1 0 1 1 2 0v4h14v-4a1 1 0 0 1 1-1M12 2a1 1 0 0 1 1 1v10.243l2.536-2.536a1 1 0 1 1 1.414 1.414l-4.066 4.066a1.25 1.25 0 0 1-1.768 0L7.05 12.121a1 1 0 1 1 1.414-1.414L11 13.243V3a1 1 0 0 1 1-1"/></g>',
    x: '<g fill="none" fill-rule="evenodd"><path fill="currentColor" d="m12 13.414 5.657 5.657a1 1 0 0 0 1.414-1.414L13.414 12l5.657-5.657a1 1 0 0 0-1.414-1.414L12 10.586 6.343 4.929A1 1 0 0 0 4.93 6.343L10.586 12l-5.657 5.657a1 1 0 1 0 1.414 1.414z"/></g>',
    plus: '<g fill="none"><path fill="currentColor" d="M11 20a1 1 0 1 0 2 0v-7h7a1 1 0 1 0 0-2h-7V4a1 1 0 1 0-2 0v7H4a1 1 0 1 0 0 2h7z"/></g>',
  };
  const btn = (label, icon, cls = '') => el('button', { type: 'button', class: `btn btn-sm ${cls}`, html: `${svg(icon)}<span class="lbl">${label}</span>` });
  const iconBtn = (icon, label) => el('button', { type: 'button', class: 'btn btn-ghost btn-icon btn-sm', 'aria-label': label, 'data-tip': label, html: svg(icon) });
  const h = (t) => el('h2', { class: 'gs-h' }, t);

  // ---------- Saving ----------
  let timer = 0;
  const clean = (o) => JSON.parse(JSON.stringify(o, (_k, v) => (v === '' || v === null ? undefined : v)));
  function saveGame(now = false) {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      const g = clean(st.game);
      if (g.env && !Object.keys(g.env).length) delete g.env;
      if (g.hooks && !Object.keys(g.hooks).length) delete g.hooks;
      if (!!g.width !== !!g.height) return;
      try { await call('settings.set', { game: g }); } catch (e) { toast(String(e), { error: true }); }
    }, now ? 0 : 450);
  }
  async function saveJavas() {
    try { await call('settings.set', { javas: st.javas }); } catch (e) { toast(String(e), { error: true }); }
  }

  // ---------- Java per major ----------
  async function found() {
    if (!st.found) st.found = (await call('java.list', { version: null }).catch(() => ({ installs: [] }))).installs || [];
    return st.found;
  }
  function setJava(m, j) {
    if (j) st.javas[m] = j.path; else delete st.javas[m];
    if (j && st.found && !st.found.some((x) => x.path === j.path)) st.found.unshift(j);
    saveJavas();
    paintJava(m);
  }
  function jvRow(m) {
    const row = el('div', { class: 'r gs-jv', 'data-major': m });
    const find = btn(_t("Найти"), I.find, 'btn-ghost'), browse = btn(_t("Обзор"), I.folder, 'btn-ghost'), get = btn(_t("Скачать"), I.down);
    const reset = iconBtn(I.x, _t("Авто"));
    find.addEventListener('click', async () => {
      const list = (await found()).filter((j) => j.major === m);
      const box = row.nextElementSibling?.classList.contains('gs-found') ? row.nextElementSibling : null;
      if (box) { box.remove(); return; }
      if (!list.length) { toast(_t("Java {0} на компьютере не найдена", [m])); get.classList.remove('gs-pulse'); void get.offsetWidth; get.classList.add('gs-pulse'); return; }
      if (list.length === 1) return setJava(m, list[0]);
      const pick = el('div', { class: 'gs-found' }, ...list.map((j) => {
        const b = el('button', { type: 'button', 'aria-pressed': String(st.javas[m] === j.path) },
          el('b', {}, [j.vendor || 'Java', j.version].join(' · ')), el('span', { class: 'mono' }, j.path));
        b.addEventListener('click', () => { pick.remove(); setJava(m, j); });
        return b;
      }));
      row.after(pick);
    });
    browse.addEventListener('click', async () => {
      const path = await invoke('pick_folder', { title: _t("Папка Java {0}", [m]) }).catch(() => null);
      if (!path) return;
      try {
        const j = await run(browse, 'java.test', { path });
        if (j.major !== m) toast(_t("Это Java {0}, а не {1}", [j.major, m]), { error: true }); else setJava(m, j);
      } catch { /* toast shown */ }
    });
    get.addEventListener('click', async () => {
      const lbl = $('.lbl', get);
      const t = setInterval(async () => {
        const p = await call('game.progress', { instance: `java:${m}` }).catch(() => null);
        if (p?.total_bytes) lbl.textContent = `${Math.floor((p.done_bytes / p.total_bytes) * 100)}%`;
      }, 400);
      try { setJava(m, await run(get, 'java.install', { major: m })); } catch { /* toast shown */ } finally { clearInterval(t); lbl.textContent = _t("Скачать"); }
    });
    reset.addEventListener('click', () => setJava(m, null));
    row.append(el('span', { class: 'gs-jn' }, String(m)),
      el('div', { class: 'grow' }, el('div', { class: 't' }, `Java ${m}`), el('div', { class: 's mono gs-jp' })),
      el('div', { class: 'gs-acts' }, find, browse, get, reset));
    return row;
  }
  function paintJava(m) {
    const row = $(`.gs-jv[data-major="${m}"]`, pane);
    const p = st.javas[m] || '';
    row.classList.toggle('is-set', !!p);
    const s = $('.gs-jp', row);
    s.textContent = p || _t("Авто");
    s.title = p;
    $('.gs-acts > .btn-icon', row).hidden = !p;
  }

  // ---------- Memory ----------
  function memBlock() {
    const max = Math.max(1024, Math.floor(st.mem.total_mb / 256) * 256);
    const range = el('input', { type: 'range', min: 512, max, step: 256, 'aria-label': _t("Память") });
    const rec = el('span', { class: 'isx-rec', 'aria-hidden': 'true' });
    const slider = el('div', { class: 'isx-slider' }, range, rec);
    const num = el('input', { class: 'field mono', type: 'number', min: 512, max, step: 256, 'aria-label': _t("Память, МБ") });
    const warn = el('span', { class: 'badge is-warn', hidden: true }, _t("Почти вся память компьютера"));
    const pct = (x) => `${((Math.min(max, Math.max(512, x)) - 512) / (max - 512)) * 100}%`;
    const paint = (v) => { slider.style.setProperty('--fill', pct(v)); slider.style.setProperty('--rec', pct(st.mem.recommended_mb)); warn.hidden = v <= st.mem.total_mb * 0.8; };
    const set = (v, save = true) => { range.value = num.value = v; paint(+v); if (save) { st.game.memory_mb = Math.round(+v); saveGame(); } };
    range.addEventListener('input', () => set(range.value));
    num.addEventListener('change', () => set(Math.min(max, Math.max(512, +num.value || 512))));
    rec.addEventListener('click', () => set(st.mem.recommended_mb));
    set(st.game.memory_mb || st.mem.recommended_mb, false);
    return el('div', { class: 'gs-block' },
      el('div', { class: 'gs-line' }, el('div', { class: 't' }, _t("Память")), el('label', { class: 'isx-num' }, num, el('span', {}, _t("МБ")))),
      slider, el('div', { class: 'isx-scale' }, el('span', {}, _t("512 МБ")), warn, el('span', {}, gb(max))));
  }

  // ---------- Launch: JVM, env, after launch ----------
  function field(label, key, ph) {
    const i = el('input', { class: 'field mono', value: st.game[key] || '', placeholder: ph || '', spellcheck: false, autocomplete: 'off' });
    i.addEventListener('input', () => { st.game[key] = i.value.trim(); saveGame(); });
    return el('label', { class: 'gs-block' }, el('span', { class: 't' }, label), i);
  }
  function envBlock() {
    const box = el('div', { class: 'isx-env' });
    const sync = () => { st.game.env = Object.fromEntries($$('.isx-env-r', box).map((r) => $$('input', r).map((i) => i.value.trim())).filter(([k]) => k)); saveGame(); };
    const row = (k = '', v = '') => {
      const r = el('div', { class: 'isx-env-r' },
        el('input', { class: 'field mono', value: k, placeholder: _t("ИМЯ"), spellcheck: false, 'aria-label': _t("Имя") }),
        el('input', { class: 'field mono', value: v, placeholder: _t("значение"), spellcheck: false, 'aria-label': _t("Значение") }));
      const rm = iconBtn(I.x, _t("Убрать"));
      rm.addEventListener('click', () => { r.classList.add('is-out'); setTimeout(() => { r.remove(); sync(); }, 160); });
      r.append(rm);
      r.addEventListener('input', sync);
      return r;
    };
    box.append(...Object.entries(st.game.env || {}).map(([k, v]) => row(k, v)));
    const add = btn(_t("Добавить"), I.plus, 'btn-ghost isx-add');
    add.addEventListener('click', () => { const r = row(); box.append(r); $('input', r).focus(); });
    return el('div', { class: 'gs-block' }, el('span', { class: 't' }, _t("Переменные среды")), box, el('div', {}, add));
  }
  function afterRow() {
    const s = el('select', { class: 'field gs-sel', 'aria-label': _t("Когда игра запустилась") },
      ...[['keep', _t("Оставить лаунчер")], ['minimize', _t("Свернуть лаунчер")], ['close', _t("Закрыть лаунчер")]].map(([v, t]) => el('option', { value: v }, t)));
    s.value = st.game.after_launch || 'keep';
    s.addEventListener('change', () => { st.game.after_launch = s.value === 'keep' ? undefined : s.value; saveGame(true); });
    return el('div', { class: 'r' }, el('div', { class: 'grow' }, el('div', { class: 't' }, _t("Когда игра запустилась"))), s);
  }

  // ---------- Window ----------
  function winRows() {
    const w = el('input', { class: 'field', type: 'number', min: 320, max: 7680, placeholder: _t("Ширина"), 'aria-label': _t("Ширина"), value: st.game.width ?? '' });
    const hh = el('input', { class: 'field', type: 'number', min: 320, max: 7680, placeholder: _t("Высота"), 'aria-label': _t("Высота"), value: st.game.height ?? '' });
    const sync = () => {
      const a = parseInt(w.value, 10), b = parseInt(hh.value, 10);
      const ok = (x) => x >= 320 && x <= 7680;
      if (!w.value && !hh.value) { st.game.width = st.game.height = undefined; saveGame(); return; }
      if (ok(a) && ok(b)) { st.game.width = a; st.game.height = b; saveGame(); }
    };
    [w, hh].forEach((i) => i.addEventListener('input', sync));
    const fs = el('input', { type: 'checkbox', checked: !!st.game.fullscreen, 'aria-label': _t("Полный экран") });
    fs.addEventListener('change', () => { st.game.fullscreen = fs.checked || undefined; saveGame(true); });
    return [
      el('div', { class: 'r' }, el('div', { class: 'grow' }, el('div', { class: 't' }, _t("Разрешение"))), el('div', { class: 'gs-res' }, w, el('span', {}, '×'), hh)),
      el('label', { class: 'r' }, el('div', { class: 'grow' }, el('div', { class: 't' }, _t("Полный экран"))), el('span', { class: 'switch' }, fs, el('i'))),
    ];
  }

  // ---------- Hooks ----------
  function hooksBlock() {
    st.game.hooks ||= {};
    let last = null;
    const one = (label, key, ph) => {
      const i = el('input', { class: 'field mono', value: st.game.hooks[key] || '', placeholder: ph || '', spellcheck: false, autocomplete: 'off' });
      i.addEventListener('focus', () => { last = i; });
      i.addEventListener('input', () => { st.game.hooks[key] = i.value.trim(); saveGame(); });
      return el('label', { class: 'gs-hook' }, el('span', { class: 'label' }, label), i);
    };
    const fields = [one(_t("Перед запуском"), 'pre_launch'), one(_t("Обёртка"), 'wrapper', 'prime-run'), one(_t("После выхода"), 'post_exit')];
    const vars = el('div', { class: 'isx-vars' }, ...VARS.map((v) => el('button', { type: 'button', class: 'isx-var mono' }, WIN ? `%${v}%` : `$${v}`)));
    vars.addEventListener('pointerdown', (e) => e.preventDefault());
    vars.addEventListener('click', (e) => {
      const b = e.target.closest('.isx-var');
      const t = last || $('input', fields[0]);
      if (!b) return;
      const s = t.selectionStart ?? t.value.length;
      t.setRangeText(b.textContent, s, t.selectionEnd ?? s, 'end');
      t.focus(); t.dispatchEvent(new Event('input'));
    });
    return el('div', { class: 'gs-block gs-hooks' }, ...fields, vars);
  }

  // ---------- Build ----------
  async function build() {
    const [s0, mem] = await Promise.all([call('settings.get').catch(() => null), call('system.memory').catch(() => null)]);
    const s = s0 || {};
    if (mem) st.mem = mem;
    st.game = s.game || {};
    st.javas = Object.fromEntries(Object.entries(s.javas || {}).map(([k, v]) => [+k, v]));
    const root = $('#gs-root-row');
    const sec = (title, ...rows) => [h(title), el('div', { class: 'rows' }, ...rows)];
    pane.replaceChildren(
      ...sec('Java', ...MAJORS.map(jvRow)),
      ...sec(_t("Память и аргументы"), memBlock(), field(_t("Аргументы JVM"), 'jvm_args', '-XX:+UseG1GC'), envBlock()),
      ...sec(_t("Окно"), ...winRows(), afterRow()),
      ...sec(_t("Хуки запуска"), hooksBlock()),
      ...sec(_t("Ресурсы"), ...(root ? [root] : [])),
    );
    MAJORS.forEach(paintJava);
  }
  build();
  window.FGameSettings = { reload: build };
})();