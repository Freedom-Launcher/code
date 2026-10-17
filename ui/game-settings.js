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
  const gb = (mb) => `${(mb / 1024).toFixed(1).replace('.0', '')} ГБ`;
  const svg = (d) => `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true">${d}</svg>`;
  const I = {
    find: '<circle cx="7" cy="7" r="4.5"/><path d="m10.5 10.5 3 3"/>',
    folder: '<path d="M2.5 4.5h4l1.5 1.5h5.5v6.5h-11z"/>',
    down: '<path d="M8 3v7m-3-3 3 3 3-3M3.5 13h9"/>',
    x: '<path d="m4.5 4.5 7 7m0-7-7 7"/>',
    plus: '<path d="M8 3.5v9M3.5 8h9"/>',
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
    const find = btn('Найти', I.find, 'btn-ghost'), browse = btn('Обзор', I.folder, 'btn-ghost'), get = btn('Скачать', I.down);
    const reset = iconBtn(I.x, 'Авто');
    find.addEventListener('click', async () => {
      const list = (await found()).filter((j) => j.major === m);
      const box = row.nextElementSibling?.classList.contains('gs-found') ? row.nextElementSibling : null;
      if (box) { box.remove(); return; }
      if (!list.length) { toast(`Java ${m} на компьютере не найдена`); get.classList.remove('gs-pulse'); void get.offsetWidth; get.classList.add('gs-pulse'); return; }
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
      const path = await invoke('pick_folder', { title: `Папка Java ${m}` }).catch(() => null);
      if (!path) return;
      try {
        const j = await run(browse, 'java.test', { path });
        if (j.major !== m) toast(`Это Java ${j.major}, а не ${m}`, { error: true }); else setJava(m, j);
      } catch { /* toast shown */ }
    });
    get.addEventListener('click', async () => {
      const lbl = $('.lbl', get);
      const t = setInterval(async () => {
        const p = await call('game.progress', { instance: `java:${m}` }).catch(() => null);
        if (p?.total_bytes) lbl.textContent = `${Math.floor((p.done_bytes / p.total_bytes) * 100)}%`;
      }, 400);
      try { setJava(m, await run(get, 'java.install', { major: m })); } catch { /* toast shown */ } finally { clearInterval(t); lbl.textContent = 'Скачать'; }
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
    s.textContent = p || 'Авто';
    s.title = p;
    $('.gs-acts > .btn-icon', row).hidden = !p;
  }

  // ---------- Memory ----------
  function memBlock() {
    const max = Math.max(1024, Math.floor(st.mem.total_mb / 256) * 256);
    const range = el('input', { type: 'range', min: 512, max, step: 256, 'aria-label': 'Память' });
    const rec = el('span', { class: 'isx-rec', 'aria-hidden': 'true' });
    const slider = el('div', { class: 'isx-slider' }, range, rec);
    const num = el('input', { class: 'field mono', type: 'number', min: 512, max, step: 256, 'aria-label': 'Память, МБ' });
    const warn = el('span', { class: 'badge is-warn', hidden: true }, 'Почти вся память компьютера');
    const pct = (x) => `${((Math.min(max, Math.max(512, x)) - 512) / (max - 512)) * 100}%`;
    const paint = (v) => { slider.style.setProperty('--fill', pct(v)); slider.style.setProperty('--rec', pct(st.mem.recommended_mb)); warn.hidden = v <= st.mem.total_mb * 0.8; };
    const set = (v, save = true) => { range.value = num.value = v; paint(+v); if (save) { st.game.memory_mb = Math.round(+v); saveGame(); } };
    range.addEventListener('input', () => set(range.value));
    num.addEventListener('change', () => set(Math.min(max, Math.max(512, +num.value || 512))));
    rec.addEventListener('click', () => set(st.mem.recommended_mb));
    set(st.game.memory_mb || st.mem.recommended_mb, false);
    return el('div', { class: 'gs-block' },
      el('div', { class: 'gs-line' }, el('div', { class: 't' }, 'Память'), el('label', { class: 'isx-num' }, num, el('span', {}, 'МБ'))),
      slider, el('div', { class: 'isx-scale' }, el('span', {}, '512 МБ'), warn, el('span', {}, gb(max))));
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
        el('input', { class: 'field mono', value: k, placeholder: 'ИМЯ', spellcheck: false, 'aria-label': 'Имя' }),
        el('input', { class: 'field mono', value: v, placeholder: 'значение', spellcheck: false, 'aria-label': 'Значение' }));
      const rm = iconBtn(I.x, 'Убрать');
      rm.addEventListener('click', () => { r.classList.add('is-out'); setTimeout(() => { r.remove(); sync(); }, 160); });
      r.append(rm);
      r.addEventListener('input', sync);
      return r;
    };
    box.append(...Object.entries(st.game.env || {}).map(([k, v]) => row(k, v)));
    const add = btn('Добавить', I.plus, 'btn-ghost isx-add');
    add.addEventListener('click', () => { const r = row(); box.append(r); $('input', r).focus(); });
    return el('div', { class: 'gs-block' }, el('span', { class: 't' }, 'Переменные среды'), box, el('div', {}, add));
  }
  function afterRow() {
    const s = el('select', { class: 'field gs-sel', 'aria-label': 'Когда игра запустилась' },
      ...[['keep', 'Оставить лаунчер'], ['minimize', 'Свернуть лаунчер'], ['close', 'Закрыть лаунчер']].map(([v, t]) => el('option', { value: v }, t)));
    s.value = st.game.after_launch || 'keep';
    s.addEventListener('change', () => { st.game.after_launch = s.value === 'keep' ? undefined : s.value; saveGame(true); });
    return el('div', { class: 'r' }, el('div', { class: 'grow' }, el('div', { class: 't' }, 'Когда игра запустилась')), s);
  }

  // ---------- Window ----------
  function winRows() {
    const w = el('input', { class: 'field', type: 'number', min: 320, max: 7680, placeholder: 'Ширина', 'aria-label': 'Ширина', value: st.game.width ?? '' });
    const hh = el('input', { class: 'field', type: 'number', min: 320, max: 7680, placeholder: 'Высота', 'aria-label': 'Высота', value: st.game.height ?? '' });
    const sync = () => {
      const a = parseInt(w.value, 10), b = parseInt(hh.value, 10);
      const ok = (x) => x >= 320 && x <= 7680;
      if (!w.value && !hh.value) { st.game.width = st.game.height = undefined; saveGame(); return; }
      if (ok(a) && ok(b)) { st.game.width = a; st.game.height = b; saveGame(); }
    };
    [w, hh].forEach((i) => i.addEventListener('input', sync));
    const fs = el('input', { type: 'checkbox', checked: !!st.game.fullscreen, 'aria-label': 'Полный экран' });
    fs.addEventListener('change', () => { st.game.fullscreen = fs.checked || undefined; saveGame(true); });
    return [
      el('div', { class: 'r' }, el('div', { class: 'grow' }, el('div', { class: 't' }, 'Разрешение')), el('div', { class: 'gs-res' }, w, el('span', {}, '×'), hh)),
      el('label', { class: 'r' }, el('div', { class: 'grow' }, el('div', { class: 't' }, 'Полный экран')), el('span', { class: 'switch' }, fs, el('i'))),
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
    const fields = [one('Перед запуском', 'pre_launch'), one('Обёртка', 'wrapper', 'prime-run'), one('После выхода', 'post_exit')];
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
      ...sec('Память и аргументы', memBlock(), field('Аргументы JVM', 'jvm_args', '-XX:+UseG1GC'), envBlock()),
      ...sec('Окно', ...winRows(), afterRow()),
      ...sec('Хуки запуска', hooksBlock()),
      ...sec('Ресурсы', ...(root ? [root] : [])),
    );
    MAJORS.forEach(paintJava);
  }
  build();
  window.FGameSettings = { reload: build };
})();