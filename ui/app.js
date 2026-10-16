'use strict';
// Built-in UI. It talks to the core only through named actions, exactly like addons.
const { invoke } = window.__TAURI__.core;
const { $, $$, toast, progress, openDlg, closeDlg, closeMenu, swapText } = window.FUI;
const call = (name, args = null) => invoke('action', { name, args });

const S = { accounts: { accounts: [], selected: null, offline_available: false }, instances: [], current: null, playing: null, running: [], view: 'empty' };
const KIND = { microsoft: 'Microsoft', offline: 'Офлайн', provider: 'Сервис' };
const STAGE = { idle: 'Подготовка', loader: 'Готовим загрузчик…', java: 'Java', version: 'Версия', libraries: 'Библиотеки', assets: 'Ресурсы', natives: 'Нативные файлы', launch: 'Запуск', done: 'Готово' };
const TITLES = {}; // Pages carry their own H1; the top bar stays clean.
const PLAY_SVG = '<svg viewBox="0 0 16 16" fill="currentColor"><path d="M5 3.5v9l7-4.5z"/></svg>';

function el(tag, attrs = {}, ...kids) {
  const n = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v == null) continue;
    if (k === 'class') n.className = v;
    else if (k === 'html') n.innerHTML = v;
    else if (k.startsWith('data-') || k.startsWith('aria-') || k === 'tabindex') n.setAttribute(k, v);
    else n[k] = v;
  }
  // Optional parts are written inline as `cond ? node : null`.
  n.append(...kids.filter((c) => c != null && c !== false));
  return n;
}
const fmtBytes = (b) => (b >= 1 << 30 ? `${(b / (1 << 30)).toFixed(1)} ГБ` : b >= 1 << 20 ? `${(b / (1 << 20)).toFixed(0)} МБ` : `${Math.round(b / 1024)} КБ`);
const initials = (s) => (s.trim()[0] || '?').toUpperCase();
const ago = (t) => {
  const d = Math.max(0, Date.now() / 1000 - t);
  if (d < 3600) return 'только что';
  if (d < 86400) return `${Math.floor(d / 3600)} ч назад`;
  const n = Math.floor(d / 86400);
  return n === 1 ? 'вчера' : `${n} дн назад`;
};
const byId = (id) => S.instances.find((i) => i.id === id);

// Runs an action for an element and reflects progress in data-state (the action-library contract).
async function run(target, name, args = null, fromTheme = false) {
  target.dataset.state = 'loading';
  try {
    const r = await (fromTheme ? invoke('theme_action', { name, args }) : call(name, args));
    target.dataset.state = 'success';
    setTimeout(() => { if (target.dataset.state === 'success') target.dataset.state = 'idle'; }, 1100);
    return r;
  } catch (e) {
    target.dataset.state = 'error';
    toast(String(e), { error: true });
    throw e;
  }
}

// Declarative binding for themes: [data-view] navigates, [data-action] + optional [data-args] JSON calls the core.
document.addEventListener('click', async (ev) => {
  const v = ev.target.closest('[data-view]');
  if (v) {
    show(v.dataset.view);
    if (v.dataset.pane) $(`#set-seg button[data-pane="${v.dataset.pane}"]`)?.click();
    closeMenu();
    return;
  }
  const a = ev.target.closest('[data-action]');
  if (!a) return;
  ev.preventDefault();
  let args = null;
  try { args = a.dataset.args ? JSON.parse(a.dataset.args) : null; } catch { toast('Неверный data-args', { error: true }); return; }
  const result = await run(a, a.dataset.action, args, a.hasAttribute('data-from-theme')).catch(() => null);
  if (result === null) return;
  const name = a.dataset.action;
  if (name.startsWith('accounts.')) renderAccounts(result);
  if (name.startsWith('safe.')) $('#safe-msg').textContent = 'Готово. Перезапустите лаунчер.';
});

// ---------- Navigation: views and the rail ----------
function show(view) {
  S.view = view;
  $$('.view').forEach((x) => x.classList.toggle('is-active', x.id === `view-${view}`));
  const lit = view === 'project' ? 'discover' : view;
  $$('.rail-btn[data-view]').forEach((b) => (b.dataset.view === lit ? b.setAttribute('aria-current', 'page') : b.removeAttribute('aria-current')));
  $$('#rail-list .tile').forEach((t) => (view === 'instance' && t.dataset.inst === S.current ? t.setAttribute('aria-current', 'page') : t.removeAttribute('aria-current')));
  $('#top-title').textContent = TITLES[view] ?? '';
  if (view === 'settings') requestAnimationFrame(() => { $('#set-seg')._place?.(false); $('#cover-seg')._place?.(false); $('#wall-seg')._place?.(false); });
  if (view === 'home') renderHome();
  if (view === 'create') prepareCreate();
  if (view === 'accounts') { renderAccountsPage(); loadProviders(); }
  if (view === 'discover') window.FMods?.discover();
}
function openInstance(id) {
  S.current = id;
  localStorage.setItem('fl.current', id);
  applyWall();
  renderInstancePage();
  window.FMods?.instance(byId(id));
  window.FPacks?.instance(byId(id));
  show('instance');
}
// ---------- Accounts ----------
// Heads: face + hat layer cropped from the skin (8×8, drawn pixelated by CSS).
// Offline/unknown skins get the default Steve face.
const STEVE = ['HHHHHHHH', 'HHHHHHHH', 'HSSSSSSH', 'SSSSSSSS', 'SWBSSBWS', 'SSSNNSSS', 'SSMMMMSS', 'SSSSSSSS'];
const STEVE_C = { H: '#2b1e0d', S: '#b78a6b', W: '#ffffff', B: '#4a3a8b', N: '#94634a', M: '#6a4030' };
function steveHead() {
  const c = Object.assign(document.createElement('canvas'), { width: 8, height: 8 });
  const g = c.getContext('2d');
  STEVE.forEach((row, y) => [...row].forEach((k, x) => { g.fillStyle = STEVE_C[k]; g.fillRect(x, y, 1, 1); }));
  return c.toDataURL();
}
const HEADS = new Map();
function cropHead(dataUrl) {
  return new Promise((ok, bad) => {
    const img = new Image();
    img.onload = () => {
      const c = Object.assign(document.createElement('canvas'), { width: 8, height: 8 });
      const g = c.getContext('2d');
      g.imageSmoothingEnabled = false;
      g.drawImage(img, 8, 8, 8, 8, 0, 0, 8, 8);
      // Hat layer: old 64×32 skins often have it fully opaque (black) — the
      // game ignores such a layer, so do we.
      const h = Object.assign(document.createElement('canvas'), { width: 8, height: 8 });
      const hg = h.getContext('2d');
      hg.drawImage(img, 40, 8, 8, 8, 0, 0, 8, 8);
      const a = hg.getImageData(0, 0, 8, 8).data;
      let clear = false;
      for (let k = 3; k < a.length; k += 4) if (a[k] < 255) { clear = true; break; }
      if (clear) g.drawImage(h, 0, 0);
      ok(c.toDataURL());
    };
    img.onerror = bad;
    img.src = dataUrl;
  });
}
/** Head image for an account; starts as Steve and swaps to the real skin. */
function headFor(acc, cls = 'head') {
  const im = el('img', { class: cls, alt: '', draggable: 'false' });
  im.src = HEADS.get(acc?.id) || steveHead();
  if (acc && acc.kind?.type === 'microsoft' && !HEADS.has(acc.id)) {
    HEADS.set(acc.id, im.src);
    call('accounts.skin', { id: acc.id })
      .then((r) => (r?.data_url ? cropHead(r.data_url) : null))
      .then((url) => { if (!url) return; HEADS.set(acc.id, url); $$(`img[data-acc="${acc.id}"]`).forEach((x) => { x.src = url; }); })
      .catch(() => {});
  }
  if (acc) im.dataset.acc = acc.id;
  return im;
}
function renderAccounts(state) {
  S.accounts = state;
  $('#acc-list').replaceChildren(...state.accounts.map((a) => el('button', {
    class: 'menu-item acc-row', 'data-action': 'accounts.select', 'data-args': JSON.stringify({ id: a.id }), 'aria-selected': String(a.id === state.selected),
  }, headFor(a, 'head head-lg'), el('span', { class: 'acc-txt' }, el('b', {}, a.name), el('span', { class: 'sub' }, kindLabel(a))), a.id === state.selected ? el('span', { class: 'acc-check' }, '✓') : '')));
  if (!state.accounts.length) $('#acc-list').append(el('div', { class: 'menu-item faint' }, 'Нет аккаунтов'));
  $('#offline-open').hidden = !state.offline_available;
  const sel = state.accounts.find((a) => a.id === state.selected);
  if (S.view === 'home') renderHome();
  swapText($('#account-name'), sel ? sel.name : 'Войти');
  $('#account-head').replaceWith(Object.assign(headFor(sel), { id: 'account-head' }));
  $('#account-kind').textContent = sel ? kindLabel(sel) : 'Нет аккаунта';
  if (S.view === 'accounts') renderAccountsPage();
  document.dispatchEvent(new Event('accounts:change'));
}

$('#dlg-login').addEventListener('dialog:open', async () => {
  $('#ms-code').textContent = '· · · · · · · ·';
  $('#ms-hint').textContent = 'Получаем код…';
  let flow;
  try { flow = await call('accounts.microsoft.begin'); } catch (e) { $('#ms-hint').textContent = String(e); return; }
  $('#ms-code').textContent = flow.user_code;
  $('#ms-hint').textContent = 'Ждём подтверждения на странице Microsoft…';
  $('#ms-open').onclick = () => invoke('open_login_page', { url: flow.verification_uri }).catch((e) => toast(String(e), { error: true }));
  $('#ms-copy').onclick = (e) => navigator.clipboard.writeText(flow.user_code).then(() => { e.currentTarget.dataset.state = 'success'; setTimeout(() => { e.currentTarget.dataset.state = 'idle'; }, 1000); });
  try {
    renderAccounts(await call('accounts.microsoft.wait'));
    closeDlg();
    toast('Вход выполнен');
  } catch (e) { $('#ms-hint').textContent = String(e); }
});

$('#dlg-offline').addEventListener('submit', async (ev) => {
  ev.preventDefault();
  const f = ev.currentTarget;
  try { renderAccounts(await run($('#o-submit'), 'accounts.offline.create', { name: f.name.value })); f.reset(); closeDlg(); } catch { /* toast shown */ }
});

// ---------- Instances: rail tiles, covers, instance page ----------
const lastTs = (i) => i.last_played || i.created_at;
const recent = () => [...S.instances].sort((a, b) => lastTs(b) - lastTs(a));
const fmtPlay = (s) => (s < 3600 ? `${Math.max(1, Math.round(s / 60))} мин` : `${Math.floor(s / 3600)} ч${s % 3600 >= 60 ? ` ${Math.floor((s % 3600) / 60)} мин` : ''}`);
const playedText = (i) => (i.last_played ? `играли ${ago(i.last_played)}${i.playtime_secs > 0 ? ` · ${fmtPlay(i.playtime_secs)} в игре` : ''}` : 'ещё не запускали');
const isRunning = (id) => S.running.includes(id);
// Covers: bundled art for now; the choice is per instance (later: own screenshots).
const COVERS = ['dawn', 'forest', 'ocean', 'nether', 'end'];
const hash = (s) => [...s].reduce((h, c) => (h * 31 + c.charCodeAt(0)) >>> 0, 7);
const coverName = (i) => localStorage.getItem(`fl.cover.${i.id}`) || COVERS[hash(i.id) % COVERS.length];
const bg = (n, name) => { n.style.backgroundImage = `url("covers/${name}.jpg")`; return n; };
function coverOpt(name, on, pick) {
  return bg(el('button', { type: 'button', class: 'cover-opt', 'aria-pressed': String(on), 'aria-label': name, 'data-id': `cover.${name}`, onclick: () => pick(name) }), name);
}
function renderRail() {
  const list = [...S.instances].sort((a, b) => a.created_at - b.created_at);
  $('#rail-list').replaceChildren(...list.map((i) => {
    const t = bg(el('button', {
      class: 'tile', 'data-inst': i.id, 'data-ctx': 'menu-ctx-instance', 'data-tip': i.name,
      'data-id': `rail.instance.${i.id}`, 'aria-label': i.name, onclick: () => openInstance(i.id),
    }, isRunning(i.id) ? el('i', { class: 'live' }) : ''), coverName(i));
    if (S.view === 'instance' && i.id === S.current) t.setAttribute('aria-current', 'page');
    return t;
  }));
}
function renderInstances() {
  renderRail();
  if (S.view === 'home') renderHome();
  if (S.view === 'instance') { if (byId(S.current)) renderInstancePage(); else show(S.instances.length ? 'instance' : 'empty'); }
}
function renderInstancePage() {
  const i = byId(S.current);
  if (!i) return;
  bg($('#ip-cover'), coverName(i));
  $('#ip-name').textContent = i.name;
  $('#ip-version').textContent = i.version;
  $('#ip-loader').textContent = LOADER_NAMES[i.loader] || 'Vanilla';
  $('#ip-created').textContent = new Date(i.created_at * 1000).toLocaleDateString('ru-RU', { day: 'numeric', month: 'long' });
  $('#ip-played').textContent = playedText(i);
  const busy = S.playing === i.id;
  const on = isRunning(i.id);
  const pb = $('#play');
  if (busy) pb.dataset.state = 'loading';
  else if (pb.dataset.state === 'loading') pb.dataset.state = 'idle';
  pb.classList.toggle('is-running', on);
  pb.classList.toggle('btn-primary', !on);
  $('#play-label').textContent = on ? 'Остановить' : 'Играть';
  pb.querySelector('svg').innerHTML = on ? '<rect x="4" y="4" width="8" height="8" rx="1.5"/>' : '<path d="M5 3.5v9l7-4.5z"/>';
  $('#play-status').hidden = !busy;
  window.FPacks?.paint();
}
function renderCoverPick() {
  const i = byId(S.current);
  if (!i) return;
  const cur = coverName(i);
  $('#cover-pick').replaceChildren(...COVERS.map((n) => coverOpt(n, n === cur, (x) => {
    localStorage.setItem(`fl.cover.${i.id}`, x);
    applyWall();
    closeMenu();
    renderInstances();
  })));
}
$('#ip-cover-btn').addEventListener('click', renderCoverPick);
$('#ip-tabs').addEventListener('click', (e) => {
  const t = e.target.closest('[data-tab]');
  if (!t) return;
  $$('#ip-tabs [data-tab]').forEach((b) => b.setAttribute('aria-selected', String(b === t)));
  $$('#view-instance .tab-body').forEach((b) => { b.hidden = b.id !== `tab-${t.dataset.tab}`; });
});

// ---------- Create instance (a page, not a dialog) ----------
const LOADER_NAMES = { vanilla: 'Vanilla', fabric: 'Fabric', quilt: 'Quilt', forge: 'Forge', neoforge: 'NeoForge' };
let createCover = COVERS[0];
function paintCreateCovers() {
  $('#c-covers').replaceChildren(...COVERS.map((n) => coverOpt(n, n === createCover, (x) => { createCover = x; paintCreateCovers(); })));
}
function prepareCreate() {
  createCover = COVERS[Math.floor(Math.random() * COVERS.length)];
  paintCreateCovers();
  if (!$('#c-version').options.length) loadVersions();
  requestAnimationFrame(() => $('#c-name').focus());
}
async function loadVersions() {
  const s = $('#c-version');
  try {
    const r = await call('versions.list', { snapshots: $('#c-snapshots').checked });
    s.replaceChildren(...r.versions.map((v) => el('option', { value: v.id }, v.type === 'snapshot' ? `${v.id} · снапшот` : v.id)));
  } catch (e) { s.replaceChildren(el('option', { value: '' }, 'Нет списка версий')); toast(String(e), { error: true }); }
}
$('#c-snapshots').addEventListener('change', loadVersions);
let createLoader = 'vanilla';
$('#c-loader').addEventListener('click', (e) => {
  const b = e.target.closest('[data-loader]');
  if (!b || b.disabled) return;
  createLoader = b.dataset.loader;
  $$('#c-loader [data-loader]').forEach((x) => x.setAttribute('aria-pressed', String(x === b)));
});
$('#c-cancel').addEventListener('click', () => (byId(S.current) ? openInstance(S.current) : show('empty')));
$('#create-form').addEventListener('submit', async (ev) => {
  ev.preventDefault();
  const f = ev.currentTarget;
  try {
    const created = await run($('#c-submit'), 'instances.create', { name: f.name.value, version: f.version.value, loader: createLoader });
    S.instances = await call('instances.list');
    const id = created?.id || S.instances[S.instances.length - 1]?.id;
    if (id) localStorage.setItem(`fl.cover.${id}`, createCover);
    f.name.value = '';
    renderInstances();
    if (id) openInstance(id);
  } catch { /* toast shown */ }
});

// ---------- Import .mrpack: the app stages the bytes, the core builds the instance ----------
const IMPORT_STAGE = { prepare: 'Читаем сборку', files: 'Скачиваем файлы', overrides: 'Копируем настройки', done: 'Готово' };
$('#c-import').addEventListener('click', () => $('#c-import-file').click());
$('#c-import-file').addEventListener('change', async (ev) => {
  const f = ev.target.files?.[0];
  ev.target.value = '';
  if (!f) return;
  const btn = $('#c-import');
  const box = $('#c-import-status');
  const form = $('#create-form');
  form.classList.add('is-importing');
  $$('#create-form button').forEach((b) => { b.disabled = true; });
  btn.dataset.state = 'loading';
  box.hidden = false;
  $('#ci-stage').textContent = IMPORT_STAGE.prepare;
  $('#ci-count').textContent = '';
  progress($('#ci-bar'), null);
  let poll = 0;
  try {
    const token = await invoke('stage_mrpack', new Uint8Array(await f.arrayBuffer()));
    poll = setInterval(async () => {
      const p = await call('game.progress', { instance: token }).catch(() => null);
      if (!p) return;
      $('#ci-stage').textContent = IMPORT_STAGE[p.stage] || IMPORT_STAGE.prepare;
      if (p.total_bytes > 0) {
        progress($('#ci-bar'), p.done_bytes / p.total_bytes);
        $('#ci-count').textContent = `${fmtBytes(p.done_bytes)} / ${fmtBytes(p.total_bytes)}`;
      } else if (p.total_files > 0) {
        progress($('#ci-bar'), p.done_files / p.total_files);
        $('#ci-count').textContent = `${p.done_files} / ${p.total_files}`;
      }
    }, 250);
    const created = await call('instances.import_mrpack', { token });
    clearInterval(poll);
    progress($('#ci-bar'), 1);
    localStorage.setItem(`fl.cover.${created.id}`, createCover);
    S.instances = await call('instances.list');
    renderInstances();
    btn.dataset.state = 'success';
    toast(`${created.name} импортирована`);
    openInstance(created.id);
  } catch (e) {
    btn.dataset.state = 'error';
    toast(String(e), { error: true });
  } finally {
    clearInterval(poll);
    box.hidden = true;
    form.classList.remove('is-importing');
    $$('#create-form button').forEach((b) => { b.disabled = false; });
  }
});

// ---------- Accounts page ----------
function renderAccountsPage() {
  const st = S.accounts;
  const btn = (label, action, id) => el('button', { class: 'btn btn-sm btn-ghost', 'data-action': action, 'data-args': JSON.stringify({ id }) }, el('span', { class: 'lbl' }, label));
  $('#acc-page').replaceChildren(...st.accounts.map((a) => el('div', { class: 'r', 'data-id': `accounts.${a.id}` },
    headFor(a, 'head'),
    el('div', { class: 'grow' }, el('div', { class: 't' }, a.name), el('div', { class: 's' }, kindLabel(a))),
    a.id === st.selected ? el('span', { class: 'acc-tag on' }, 'Играю как') : btn('Играть как', 'accounts.select', a.id),
    btn('Удалить', 'accounts.remove', a.id))));
  if (!st.accounts.length) $('#acc-page').append(el('div', { class: 'r' }, el('div', { class: 's' }, 'Аккаунтов пока нет')));
  $('#acc-offline').hidden = !st.offline_available;
}
/** Provider accounts show the service name from their addon. */
const PROVIDERS = new Map();
const kindLabel = (a) => (a?.kind?.type === 'provider' ? PROVIDERS.get(a.kind.provider) || KIND.provider : KIND[a?.kind?.type] || '');
function loadProviders() {
  return call('accounts.providers.list').then((r) => {
    PROVIDERS.clear();
    (r?.providers || []).forEach((p) => PROVIDERS.set(p.id, p.label));
    renderAccounts(S.accounts);
  }).catch(() => {});
}
// ---------- Home: greeting, recent builds, all builds, glass play bar ----------
const greet = (h) => (h < 5 ? 'Доброй ночи' : h < 12 ? 'Доброе утро' : h < 18 ? 'Добрый день' : h < 23 ? 'Добрый вечер' : 'Доброй ночи');
const instMeta = (i) => `${LOADER_NAMES[i.loader] || 'Vanilla'} · ${i.version}`;
const PLAY_D = '<path d="M5 3.5v9l7-4.5z"/>';
const STOP_D = '<rect x="4" y="4" width="8" height="8" rx="1.5"/>';
function svgIco(d) { const s = document.createElementNS('http://www.w3.org/2000/svg', 'svg'); s.setAttribute('viewBox', '0 0 16 16'); s.innerHTML = d; return s; }
const togglePlay = (id) => (isRunning(id) ? stopGame(id) : play(id));
function renderHome() {
  const acc = S.accounts.accounts.find((a) => a.id === S.accounts.selected);
  $('#home-greet').textContent = `${greet(new Date().getHours())}${acc ? `, ${acc.name}` : ''}`;
  const rc = recent().slice(0, 5).map((i) => el('button', { class: 'rc', 'data-ctx': 'menu-ctx-instance', 'data-inst': i.id, 'data-id': `home.recent.${i.id}`, onclick: () => openInstance(i.id) },
    bg(el('i'), coverName(i)), el('span', { class: 't' }, el('b', {}, i.name), el('span', {}, instMeta(i))),
    el('span', { class: `pl${isRunning(i.id) ? ' on' : ''}`, role: 'button', 'aria-label': `Играть: ${i.name}`, onclick: (e) => { e.stopPropagation(); togglePlay(i.id); } }, svgIco(isRunning(i.id) ? STOP_D : PLAY_D))));
  rc.push(el('button', { class: 'rc add', 'data-view': 'create', 'data-id': 'home.create' }, el('i', {}, '+'), el('span', { class: 't' }, el('b', {}, 'Новая сборка'))));
  $('#home-recent').replaceChildren(...rc);
  const all = [...S.instances].sort((a, b) => a.created_at - b.created_at);
  $('#home-all').replaceChildren(...all.map((i) => el('button', { class: 'hcard', 'data-ctx': 'menu-ctx-instance', 'data-inst': i.id, 'data-id': `home.card.${i.id}`, onclick: () => openInstance(i.id) },
    bg(el('i'), coverName(i)), el('b', {}, i.name), el('span', {}, instMeta(i)))));
  const c = byId(S.current) || recent()[0];
  const bar = $('#home-bar');
  bar.hidden = !c;
  if (!c) return;
  bar.dataset.inst = c.id;
  bg($('#hb-cover'), coverName(c));
  $('#hb-name').textContent = c.name;
  $('#hb-meta').textContent = instMeta(c);
  $('#hb-played').textContent = playedText(c);
  const on = isRunning(c.id);
  const hp = $('#home-play');
  hp.classList.toggle('is-running', on);
  hp.classList.toggle('btn-primary', !on);
  hp.dataset.state = S.playing === c.id ? 'loading' : 'idle';
  $('#hp-label').textContent = on ? 'Остановить' : 'Играть';
  hp.querySelector('svg').innerHTML = on ? STOP_D : PLAY_D;
}
$('#home-play').addEventListener('click', () => togglePlay($('#home-bar').dataset.inst));
$('#hb-open').addEventListener('click', () => openInstance($('#home-bar').dataset.inst));
// ---------- Play: install (with progress polling) → launch ----------
function setRunning() {
  const starting = S.playing && byId(S.playing);
  const names = S.running.map((id) => byId(id)?.name).filter(Boolean);
  const on = !!starting || names.length > 0;
  $('#running').classList.toggle('is-on', on);
  $('#running').classList.toggle('is-live', !starting && names.length > 0);
  $('#running-text').textContent = starting ? `Запускается: ${starting.name}`
    : names.length === 1 ? `Играет: ${names[0]}`
    : names.length > 1 ? `Запущено сборок: ${names.length}` : 'Ничего не запущено';
}
// The core owns the truth about game processes; the UI just follows it.
async function refreshRunning() {
  const r = await call('game.running').catch(() => null);
  if (!r) return;
  const next = r.running || [];
  if (next.join() === S.running.join()) return;
  S.running = next;
  setRunning();
  renderInstances();
  if (S.view === 'instance') renderInstancePage();
}
setInterval(() => { if (!document.hidden) refreshRunning(); }, 2000);
async function stopGame(id = S.current) {
  if (!id) return;
  await call('game.stop', { instance: id }).catch((e) => toast(String(e), { error: true }));
  await refreshRunning();
  toast('Игра закрыта');
}
async function play(id = S.current, launch = true) {
  if (S.playing || !id) return;
  if (launch && isRunning(id)) { toast('Эта сборка уже запущена'); return; }
  if (launch && !S.accounts.selected) { toast('Сначала войдите в аккаунт', { error: true }); $('#account-btn').click(); return; }
  const inst = byId(id);
  S.playing = id;
  S.current = id;
  if (S.view !== 'instance') openInstance(id); else renderInstancePage();
  renderInstances();
  setRunning();
  const btn = $('#play');
  btn.dataset.state = 'loading';
  $('#play-status').hidden = false;
  progress($('#ps-bar'), null);
  const poll = setInterval(async () => {
    const p = await call('game.progress', { instance: id }).catch(() => null);
    if (!p) return;
    $('#ps-stage').textContent = STAGE[p.stage] || p.stage;
    if (p.total_bytes > 0) {
      progress($('#ps-bar'), p.done_bytes / p.total_bytes);
      $('#ps-count').textContent = `${fmtBytes(p.done_bytes)} / ${fmtBytes(p.total_bytes)}`;
    } else if (p.total_files > 0) {
      progress($('#ps-bar'), p.done_files / p.total_files);
      $('#ps-count').textContent = `${p.done_files} / ${p.total_files}`;
    }
  }, 250);
  try {
    await call('game.install', { instance: id, verify: !launch });
    progress($('#ps-bar'), 1);
    if (launch) {
      $('#ps-stage').textContent = STAGE.launch;
      const r = await call('game.launch', { instance: id });
      if (!r?.already_running) { const it = byId(id); if (it) it.last_played = Math.floor(Date.now() / 1000); }
      if (!S.running.includes(id)) S.running = [...S.running, id];
      const w = window.__TAURI__?.window?.getCurrentWindow?.();
      if (w && r?.after_launch === 'minimize') w.minimize();
      if (w && r?.after_launch === 'close') setTimeout(() => w.close(), 800);
    }
    btn.dataset.state = 'success';
    toast(launch ? 'Игра запущена' : 'Файлы в порядке');
  } catch (e) {
    btn.dataset.state = 'error';
    toast(String(e), { error: true });
  } finally {
    clearInterval(poll);
    S.playing = null;
    setRunning();
    renderInstances();
    if (S.view === 'instance') renderInstancePage();
    refreshRunning();
    setTimeout(() => { $('#play-status').hidden = true; $('#ps-count').textContent = ''; if (btn.dataset.state !== 'loading') btn.dataset.state = 'idle'; }, 1400);
  }
}
// ---------- Window: our top bar is the only title bar ----------
(() => {
  const w = window.__TAURI__?.window?.getCurrentWindow?.();
  if (!w) return;
  $('#win-min').addEventListener('click', () => w.minimize());
  $('#win-max').addEventListener('click', () => w.toggleMaximize());
  $('#win-close').addEventListener('click', () => w.close());
})();

$('#play').addEventListener('click', () => (isRunning(S.current) ? stopGame(S.current) : play(S.current, true)));
$('#mi-install').addEventListener('click', () => play(S.current, false));

// ---------- Instance management: settings, folder, duplicate, delete ----------
async function reloadInstances() { S.instances = await call('instances.list'); renderInstances(); }
const memLabel = (v) => (+v ? `${(+v / 1024).toFixed(1).replace('.0', '')} ГБ` : 'Авто');
$('#is-mem').addEventListener('input', (e) => { $('#is-mem-val').textContent = memLabel(e.target.value); });
$('#mi-settings').addEventListener('click', () => {
  const i = byId(S.current);
  if (!i) return;
  const s = i.settings || {};
  const f = $('#dlg-inst');
  f.name.value = i.name;
  f.memory.value = s.memory_mb || 0;
  $('#is-mem-val').textContent = memLabel(f.memory.value);
  f.width.value = s.width || '';
  f.height.value = s.height || '';
  f.java.value = s.java_path || '';
  f.jvm.value = s.jvm_args || '';
  f.after.value = s.after_launch || 'keep';
  openDlg('dlg-inst');
});
$('#dlg-inst').addEventListener('submit', async (ev) => {
  ev.preventDefault();
  const f = ev.currentTarget;
  const num = (v) => (v.trim() ? parseInt(v, 10) : undefined);
  const str = (v) => v.trim() || undefined;
  const settings = {
    memory_mb: +f.memory.value || undefined,
    width: num(f.width.value),
    height: num(f.height.value),
    java_path: str(f.java.value),
    jvm_args: str(f.jvm.value),
    after_launch: f.after.value === 'keep' ? undefined : f.after.value,
  };
  try {
    await run($('#is-submit'), 'instances.update', { instance: S.current, name: f.name.value, settings });
    await reloadInstances();
    closeDlg();
  } catch { /* toast shown */ }
});
$('#mi-folder').addEventListener('click', () => {
  closeMenu();
  invoke('open_instance_folder', { instance: S.current }).catch((e) => toast(String(e), { error: true }));
});
$('#mi-dup').addEventListener('click', async () => {
  closeMenu();
  const src = S.current;
  try {
    const copy = await call('instances.duplicate', { instance: src });
    const cover = localStorage.getItem(`fl.cover.${src}`);
    if (cover) localStorage.setItem(`fl.cover.${copy.id}`, cover);
    await reloadInstances();
    openInstance(copy.id);
    toast('Сборка скопирована');
  } catch (e) { toast(String(e), { error: true }); }
});
$('#mi-del').addEventListener('click', () => {
  const i = byId(S.current);
  if (!i) return;
  $('#del-name').textContent = i.name;
  openDlg('dlg-del');
});
$('#dlg-del').addEventListener('submit', async (ev) => {
  ev.preventDefault();
  const id = S.current;
  try {
    await run($('#del-submit'), 'instances.delete', { instance: id });
    localStorage.removeItem(`fl.cover.${id}`);
    closeDlg();
    $('#del-submit').dataset.state = 'idle';
    S.instances = await call('instances.list');
    const next = recent()[0];
    if (next) { S.current = next.id; localStorage.setItem('fl.current', next.id); } else localStorage.removeItem('fl.current');
    // The rail must be rebuilt, otherwise the deleted tile stays and leads nowhere.
    renderRail();
    if (next) openInstance(next.id); else show('empty');
    toast('Сборка удалена');
  } catch { /* toast shown */ }
});
// Right-click menu on instance cards/rows.
const ctxInst = () => $('#menu-ctx-instance')._host?.dataset.inst;
$('#cx-play').addEventListener('click', () => { FUI.closeMenu(); play(ctxInst(), true); });
$('#cx-open').addEventListener('click', () => { FUI.closeMenu(); openInstance(ctxInst()); });
$('#cx-verify').addEventListener('click', () => { FUI.closeMenu(); play(ctxInst(), false); });

document.addEventListener('keydown', (e) => {
  if (e.target.matches('input, select, textarea')) return;
  if (e.ctrlKey && e.key.toLowerCase() === 'n') { e.preventDefault(); show('create'); }
  if (e.key === 'Enter' && S.view === 'instance' && !document.querySelector('.dialog[data-open]')) play(S.current, true);
});

// ---------- Settings: sections, theme + effects (look is kept in the core, see syncLook) ----------
$('#set-seg').addEventListener('seg:change', (e) => {
  $$('#view-settings .set-pane').forEach((p) => { p.hidden = p.id !== `set-${e.detail.dataset.pane}`; });
  requestAnimationFrame(() => $('#theme-seg')._place?.(false));
});
// ---------- Themes: the core owns the choice; tokens/CSS switch live, templates on reload ----------
async function applyTheme(cur, first = false) {
  FTheme.apply(cur);
  if (cur.fallback) toast(`Тема «${cur.fallback.theme}» не загрузилась: ${cur.fallback.error}. Включена стандартная.`, { error: true });
  if (!first) return;
  for (const screen of cur.templates || []) {
    const t = await call('themes.template', { screen }).catch(() => null);
    if (t?.html) FTheme.mount(cur.id, screen, t.html);
  }
}
function renderThemes(list, cur) {
  S.theme = cur;
  $('#theme-seg').replaceChildren(...list.map((t) => {
    const b = el('button', { 'data-theme-set': t.id, 'aria-selected': String(t.id === cur.id) }, t.name);
    if (t.error) { b.disabled = true; b.dataset.tip = t.error; }
    return b;
  }));
}
$('#theme-seg').addEventListener('seg:change', async (e) => {
  const prev = S.theme;
  const cur = await call('themes.set', { theme: e.detail.dataset.themeSet }).catch((err) => { toast(String(err), { error: true }); return null; });
  if (!cur) return;
  S.theme = cur;
  // Templates restructure screens, so switching to or from them reloads the UI (local, under a second).
  if ((prev?.templates?.length || 0) + (cur.templates?.length || 0) > 0) { location.hash = 'settings'; location.reload(); return; }
  applyTheme(cur);
  window.FAddons.retheme();
});
$('#theme-folder').addEventListener('click', () => invoke('open_themes_folder').catch((e) => toast(String(e), { error: true })));
$('#theme-reload').addEventListener('click', () => { location.hash = 'settings'; location.reload(); });

// Look (wallpaper, glass, cover, palette) lives in the core (settings.look).
// localStorage keeps a mirror so the first paint is right before the core answers.
const LOOK_KEYS = { wall: 'fl.wall', wall_stamp: 'fl.wall-v', wall_blur: 'fl.wall-blur', glass_blur: 'fl.glass-blur', glass_density: 'fl.wall-glass', cover: 'fl.cover-size', palette_pos: 'fl.palette-pos', palette_blur: 'fl.palette-blur', full_width: 'fl.full-width' };
const LOOK_DEF = { wall: 'cover', wall_stamp: 0, wall_blur: 22, glass_blur: 28, glass_density: 45, cover: 'big', palette_pos: 'top', palette_blur: true, full_width: false };
const LOOK_ENUM = { wall: ['cover', 'custom', 'off'], cover: ['big', 'slim', 'none'], palette_pos: ['top', 'bottom'] };
function lookFromCache() {
  const l = { ...LOOK_DEF };
  for (const [k, key] of Object.entries(LOOK_KEYS)) {
    const v = localStorage.getItem(key);
    if (v == null) continue;
    const d = LOOK_DEF[k];
    if (typeof d === 'number') l[k] = Math.max(0, Math.round(+v) || 0);
    else if (typeof d === 'boolean') l[k] = v !== 'off';
    else if (LOOK_ENUM[k].includes(v)) l[k] = v;
  }
  return l;
}
function mirrorLook(p) {
  for (const [k, v] of Object.entries(p)) if (LOOK_KEYS[k]) localStorage.setItem(LOOK_KEYS[k], typeof v === 'boolean' ? (v ? 'on' : 'off') : String(v));
}
S.look = lookFromCache();
let lookReady = false, lookPending = {}, lookTimer = 0;
function flushLook() {
  if (!lookReady || !Object.keys(lookPending).length) return;
  const look = lookPending; lookPending = {};
  call('settings.set', { look }).catch((e) => toast(String(e), { error: true }));
}
function setLook(patch) {
  Object.assign(S.look, patch); mirrorLook(patch);
  Object.assign(lookPending, patch);
  clearTimeout(lookTimer); lookTimer = setTimeout(flushLook, 250);
}
function setMotion(on) {
  localStorage.setItem('fl.motion', on ? 'on' : 'off');
  if (lookReady) call('settings.set', { reduce_motion: !on }).catch((e) => toast(String(e), { error: true }));
}
async function syncLook() {
  try {
    const st = await call('settings.get');
    S.onboarded = !!st?.onboarded;
    if (st?.look) {
      S.look = { ...LOOK_DEF, ...st.look }; mirrorLook(S.look);
      localStorage.setItem('fl.motion', st.reduce_motion ? 'off' : 'on');
    } else {
      const saved = await call('settings.set', { look: S.look, reduce_motion: localStorage.getItem('fl.motion') === 'off' });
      if (saved?.look) { S.look = { ...LOOK_DEF, ...saved.look }; mirrorLook(S.look); }
    }
    lookReady = true; flushLook();
  } catch (e) { console.warn('settings', e); }
  applyLook(); applyWall(); document.dispatchEvent(new Event('look:change'));
}

// Wallpaper behind the glass. fl.wall = 'cover' | 'custom' | 'off'; fl.wall-v stamps the user's image.
const WALL_URL = navigator.userAgent.includes('Windows') ? 'http://wall.localhost/w' : 'wall://localhost/w';
function applyWall() {
  const i = byId(S.current);
  const mode = S.look.wall;
  const v = S.look.wall_stamp;
  const url = mode === 'custom' && v ? `${WALL_URL}?v=${v}` : mode === 'cover' && i ? `covers/${coverName(i)}.jpg` : '';
  const root = document.documentElement;
  root.dataset.wall = url ? 'on' : 'off';
  if (url) root.style.setProperty('--wall', `url("${url}")`);
  $('#wall-file-row').hidden = mode !== 'custom';
  const blur = S.look.wall_blur;
  const glass = S.look.glass_density;
  root.style.setProperty('--wall-blur', `${blur}px`);
  root.style.setProperty('--glass-a', String(glass / 100));
  const gblur = S.look.glass_blur;
  root.style.setProperty('--glass-blur', `${gblur}px`);
  $('#glass-blur').value = gblur; $('#glass-blur-val').textContent = gblur ? `${gblur} px` : 'Прозрачное';
  $('#wall-blur').value = blur; $('#wall-blur-val').textContent = blur ? `${blur} px` : 'Чёткие';
  $('#wall-glass').value = 100 - glass; $('#wall-glass-val').textContent = `${100 - glass}%`;
  $$('#wall-seg button').forEach((b) => b.setAttribute('aria-selected', String(b.dataset.wallSet === mode)));
  $('#wall-blur-row').hidden = $('#wall-glass-row').hidden = $('#glass-blur-row').hidden = mode === 'off';
}
$('#wall-seg').addEventListener('seg:change', (e) => {
  const mode = e.detail.dataset.wallSet;
  setLook({ wall: mode }); applyWall();
  if (mode === 'custom' && !S.look.wall_stamp) $('#wall-file').click();
});
$('#wall-pick').addEventListener('click', () => $('#wall-file').click());
$('#wall-file').addEventListener('change', async (e) => {
  const f = e.target.files?.[0]; e.target.value = '';
  if (!f) return;
  try {
    await invoke('set_wallpaper', new Uint8Array(await f.arrayBuffer()));
    setLook({ wall_stamp: Date.now(), wall: 'custom' });
    applyWall();
  } catch (err) { toast(String(err), { error: true }); }
});
$('#wall-blur').addEventListener('input', (e) => { setLook({ wall_blur: +e.target.value }); applyWall(); });
$('#glass-blur').addEventListener('input', (e) => { setLook({ glass_blur: +e.target.value }); applyWall(); });
$('#wall-glass').addEventListener('input', (e) => { setLook({ glass_density: 100 - +e.target.value }); applyWall(); });
function applyLook() {
  document.documentElement.dataset.motion = localStorage.getItem('fl.motion') || 'on';
  $('#fx-motion').checked = document.documentElement.dataset.motion !== 'off';
  document.documentElement.dataset.cover = S.look.cover;
  document.documentElement.dataset.width = S.look.full_width ? 'full' : 'center';
  $('#fx-wide').checked = !!S.look.full_width;
  $$('#cover-seg button').forEach((b) => b.setAttribute('aria-selected', String(b.dataset.coverSet === document.documentElement.dataset.cover)));
}

$('#cover-seg').addEventListener('seg:change', (e) => { setLook({ cover: e.detail.dataset.coverSet }); applyLook(); });
$('#fx-motion').addEventListener('change', (e) => { setMotion(e.target.checked); applyLook(); });
$('#fx-wide').addEventListener('change', (e) => { setLook({ full_width: e.target.checked }); applyLook(); document.dispatchEvent(new Event('look:change')); });

// ---------- Startup ----------
async function start() {
  applyLook();
  const info = await call('app.info');
  if (info.safe_mode) { $('#safe').hidden = false; }
  else {
    await syncLook();
    const [acc, inst, theme, themeList] = await Promise.all([call('accounts.list'), call('instances.list'), call('themes.current'), call('themes.list')]);
    await applyTheme(theme, true);
    renderThemes(themeList, theme);
    window.FAddons.start();
    renderAccounts(acc);
    loadProviders();
    S.instances = inst;
    const saved = localStorage.getItem('fl.current');
    S.current = inst.some((i) => i.id === saved) ? saved : recent()[0]?.id || null;
    renderInstances();
    applyWall();
    refreshRunning();
    $('#app').hidden = false;
    window.FUI.initSegs();
    const want = location.hash.slice(1);
    if (want === 'instance' && S.current) openInstance(S.current);
    else if (!want || want === 'home' || want === 'instance') show(S.instances.length ? 'home' : 'empty');
    else show(want);
    window.FWelcome?.start();
  }
  requestAnimationFrame(() => requestAnimationFrame(async () => {
    const r = await call('app.ready');
    // Dev-only: the fixed label overlapped lists near the bottom edge.
    const dev = location.search.includes('dev');
    if (dev) $('#startup').textContent = `запуск ${r.startup_ms} мс`;
    if (!info.safe_mode) { loadVersions(); if (dev) window.FUI.devCheck(); }
  }));
}
start().catch((e) => toast(String(e), { error: true }));
