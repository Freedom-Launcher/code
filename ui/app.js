'use strict';
// Built-in UI. It talks to the core only through named actions, exactly like addons.
const { invoke } = window.__TAURI__.core;
const { $, $$, toast, progress, openDlg, closeDlg, closeMenu, swapText } = window.FUI;
const call = (name, args = null) => invoke('action', { name, args });

const S = { accounts: { accounts: [], selected: null, offline_available: false }, instances: [], current: null, playing: null, running: [], view: 'home', history: [], sort: 'recent' };
const KIND = { microsoft: 'Microsoft', offline: 'Офлайн', provider: 'Сервис' };
const STAGE = { idle: 'Подготовка', java: 'Java', version: 'Версия', libraries: 'Библиотеки', assets: 'Ресурсы', natives: 'Нативные файлы', launch: 'Запуск', done: 'Готово' };
const TITLES = { home: 'Главная', library: 'Библиотека', discover: 'Обзор', settings: 'Настройки' };
const PLAY_SVG = '<svg viewBox="0 0 16 16" fill="currentColor"><path d="M5 3.5v9l7-4.5z"/></svg>';

function el(tag, attrs = {}, ...kids) {
  const n = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === 'class') n.className = v;
    else if (k === 'html') n.innerHTML = v;
    else if (k.startsWith('data-') || k.startsWith('aria-')) n.setAttribute(k, v);
    else n[k] = v;
  }
  n.append(...kids);
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
async function run(target, name, args = null) {
  target.dataset.state = 'loading';
  try {
    const r = await call(name, args);
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
  if (v) { show(v.dataset.view); closeMenu(); return; }
  const a = ev.target.closest('[data-action]');
  if (!a) return;
  ev.preventDefault();
  const args = a.dataset.args ? JSON.parse(a.dataset.args) : null;
  const result = await run(a, a.dataset.action, args).catch(() => null);
  if (result === null) return;
  const name = a.dataset.action;
  if (name.startsWith('accounts.')) renderAccounts(result);
  if (name.startsWith('safe.')) $('#safe-msg').textContent = 'Готово. Перезапустите лаунчер.';
});

// ---------- Navigation: views, back history, breadcrumbs ----------
function show(view, { push = true } = {}) {
  if (push && view !== S.view) S.history.push({ view: S.view, current: S.current });
  S.view = view;
  $$('.view').forEach((x) => x.classList.toggle('is-active', x.id === `view-${view}`));
  $$('.nav-btn[data-view]').forEach((b) => b.toggleAttribute('aria-current', b.dataset.view === view));
  $$('.nav-btn[aria-current]').forEach((b) => b.setAttribute('aria-current', 'page'));
  $$('.nav-pin').forEach((p) => p.toggleAttribute('aria-current', view === 'instance' && p.dataset.inst === S.current));
  $$('.nav-pin[aria-current]').forEach((p) => p.setAttribute('aria-current', 'page'));
  $('#back').disabled = !S.history.length;
  renderCrumbs();
  if (view === 'settings') requestAnimationFrame(() => $('#set-seg')._place?.(false));
  if (view === 'library') requestAnimationFrame(() => $('#lib-sort')._place?.(false));
}
function renderCrumbs() {
  const c = $('#crumbs');
  if (S.view === 'instance') {
    const i = byId(S.current);
    c.replaceChildren(el('button', { 'data-view': 'library' }, 'Библиотека'), el('span', { class: 'sep' }, '/'), el('b', {}, i ? i.name : ''));
  } else c.replaceChildren(el('b', {}, TITLES[S.view] || ''));
}
$('#back').addEventListener('click', () => {
  const prev = S.history.pop();
  if (!prev) return;
  S.current = prev.current;
  if (prev.view === 'instance') renderInstancePage();
  show(prev.view, { push: false });
});
function openInstance(id) {
  S.current = id;
  localStorage.setItem('fl.current', id);
  renderInstancePage();
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
  }, headFor(a, 'head head-lg'), el('span', { class: 'acc-txt' }, el('b', {}, a.name), el('span', { class: 'sub' }, KIND[a.kind.type] || '')), a.id === state.selected ? el('span', { class: 'acc-check' }, '✓') : '')));
  if (!state.accounts.length) $('#acc-list').append(el('div', { class: 'menu-item faint' }, 'Нет аккаунтов'));
  $('#offline-open').hidden = !state.offline_available;
  const sel = state.accounts.find((a) => a.id === state.selected);
  swapText($('#account-name'), sel ? sel.name : 'Войти');
  $('#account-head').replaceWith(Object.assign(headFor(sel), { id: 'account-head' }));
  $('#account-kind').textContent = sel ? (KIND[sel.kind.type] || '') : 'Нет аккаунта';
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

// ---------- Instances: home rows, library grid, pinned icons, instance page ----------
const lastTs = (i) => i.last_played || i.created_at;
const recent = () => [...S.instances].sort((a, b) => lastTs(b) - lastTs(a));
const playedText = (i) => (i.last_played ? `играли ${ago(i.last_played)}` : 'ещё не запускали');
const isRunning = (id) => S.running.includes(id);
function quickPlay(i, cls) {
  const on = isRunning(i.id);
  return el('button', { class: cls + (on ? ' is-running' : ''), 'data-id': `instance.${i.id}.play`, 'aria-label': on ? `Играет: ${i.name}` : `Играть: ${i.name}`, html: on ? '<i class="live"></i><span>Играет</span>' : PLAY_SVG, onclick: (e) => { e.stopPropagation(); if (on) openInstance(i.id); else play(i.id); } });
}
function jumpRow(i) {
  const busy = S.playing === i.id;
  const on = isRunning(i.id);
  const b = el('button', { class: on ? 'btn btn-sm btn-live' : 'btn btn-primary btn-sm', 'data-id': `instance.${i.id}.play`, onclick: (e) => { e.stopPropagation(); if (on) openInstance(i.id); else play(i.id); } },
    el('span', { class: 'lbl', html: on ? '<i class="live"></i><span>Играет</span>' : `${PLAY_SVG}<span>${busy ? 'Запуск…' : 'Играть'}</span>` }));
  if (busy) b.dataset.state = 'loading';
  return el('div', { class: 'jump-row', 'data-id': `home.jump.${i.id}`, 'data-ctx': 'menu-ctx-instance', 'data-inst': i.id, tabIndex: 0, onclick: () => openInstance(i.id) },
    el('span', { class: 'inst-ico' }, initials(i.name)),
    el('div', { class: 'grow' }, el('div', { class: 'title' }, i.name),
      el('div', { class: 'sub' }, el('span', {}, 'Vanilla'), el('span', { class: 'mono' }, i.version), el('span', { class: 'sep' }, '·'), el('span', {}, playedText(i)))),
    b);
}
function card(i) {
  return el('div', { class: 'card-inst', 'data-id': `instance.${i.id}`, 'data-ctx': 'menu-ctx-instance', 'data-inst': i.id, tabIndex: 0, onclick: () => openInstance(i.id) },
    el('span', { class: 'inst-ico art' }, initials(i.name)),
    el('div', {}, el('div', { class: 'title' }, i.name),
      el('div', { class: 'badges' }, el('span', { class: 'badge-sm' }, 'Vanilla'), el('span', { class: 'badge-sm mono' }, i.version)),
      el('div', { class: 'sub' }, playedText(i))),
    quickPlay(i, 'quick'));
}
function renderInstances() {
  const has = S.instances.length > 0;
  $('#home-empty').hidden = has;
  $('#view-home .page-inner').hidden = !has;
  $('#jump').replaceChildren(...recent().slice(0, 3).map(jumpRow));
  $('#home-grid').replaceChildren(...recent().slice(0, 6).map(card));
  const q = $('#lib-search').value.trim().toLowerCase();
  let list = S.instances.filter((i) => !q || i.name.toLowerCase().includes(q));
  list = S.sort === 'name' ? list.sort((a, b) => a.name.localeCompare(b.name, 'ru')) : list.sort((a, b) => b.created_at - a.created_at);
  $('#lib-grid').replaceChildren(...list.map(card));
  if (!list.length) $('#lib-grid').append(el('div', { class: 'grid-empty' }, has ? 'Ничего не найдено' : 'Сборок пока нет'));
  $('#nav-pins').replaceChildren(...recent().slice(0, 5).map((i) => el('button', {
    class: 'nav-pin', 'data-inst': i.id, 'data-ctx': 'menu-ctx-instance', 'data-tip': i.name, 'data-id': `nav.pin.${i.id}`, 'aria-label': i.name, onclick: () => openInstance(i.id),
  }, el('span', { class: 'inst-ico' }, initials(i.name)))));
  if (S.view === 'instance') renderInstancePage();
}
function renderInstancePage() {
  const i = byId(S.current);
  if (!i) return;
  $('#ip-ico').textContent = initials(i.name);
  $('#ip-name').textContent = i.name;
  $('#ip-version').textContent = i.version;
  $('#ip-created').textContent = `создана ${new Date(i.created_at * 1000).toLocaleDateString('ru-RU', { day: 'numeric', month: 'long' })}`;
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
}
$('#lib-search').addEventListener('input', renderInstances);
$('#lib-sort').addEventListener('seg:change', (e) => { S.sort = e.detail.dataset.sort; renderInstances(); });
$('#ip-tabs').addEventListener('click', (e) => {
  const t = e.target.closest('[data-tab]');
  if (!t) return;
  $$('#ip-tabs [data-tab]').forEach((b) => b.setAttribute('aria-selected', String(b === t)));
  $$('#view-instance .tab-body').forEach((b) => { b.hidden = b.id !== `tab-${t.dataset.tab}`; });
});

// ---------- Create instance ----------
async function loadVersions() {
  const s = $('#c-version');
  try {
    const r = await call('versions.list', { snapshots: $('#c-snapshots').checked });
    s.replaceChildren(...r.versions.map((v) => el('option', { value: v.id }, v.type === 'snapshot' ? `${v.id} · снапшот` : v.id)));
  } catch (e) { s.replaceChildren(el('option', { value: '' }, 'Нет списка версий')); toast(String(e), { error: true }); }
}
$('#c-snapshots').addEventListener('change', loadVersions);
$('#dlg-create').addEventListener('submit', async (ev) => {
  ev.preventDefault();
  const f = ev.currentTarget;
  try {
    const created = await run($('#c-submit'), 'instances.create', { name: f.name.value, version: f.version.value });
    S.instances = await call('instances.list');
    const id = created?.id || S.instances[S.instances.length - 1]?.id;
    f.name.value = '';
    closeDlg();
    renderInstances();
    if (id) openInstance(id);
  } catch { /* toast shown */ }
});

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
// Right-click menu on instance cards/rows.
const ctxInst = () => $('#menu-ctx-instance')._host?.dataset.inst;
$('#cx-play').addEventListener('click', () => { FUI.closeMenu(); play(ctxInst(), true); });
$('#cx-open').addEventListener('click', () => { FUI.closeMenu(); openInstance(ctxInst()); });
$('#cx-verify').addEventListener('click', () => { FUI.closeMenu(); play(ctxInst(), false); });

document.addEventListener('keydown', (e) => {
  if (e.target.matches('input, select, textarea')) return;
  if (e.ctrlKey && e.key.toLowerCase() === 'n') { e.preventDefault(); openDlg('dlg-create'); }
  if (e.key === 'Enter' && S.view === 'instance' && !document.querySelector('.dialog[data-open]')) play(S.current, true);
  if (e.altKey && e.key === 'ArrowLeft') $('#back').click();
});

// ---------- Settings: sections, theme + effects (saved locally until settings.set exists) ----------
$('#set-seg').addEventListener('seg:change', (e) => {
  $$('#view-settings .set-pane').forEach((p) => { p.hidden = p.id !== `set-${e.detail.dataset.pane}`; });
  requestAnimationFrame(() => $('#theme-seg')._place?.(false));
});
function applyLook() {
  document.documentElement.dataset.theme = localStorage.getItem('fl.theme') || document.documentElement.dataset.theme || 'dark';
  document.documentElement.dataset.motion = localStorage.getItem('fl.motion') || 'on';
  $$('#theme-seg button').forEach((b) => b.setAttribute('aria-selected', String(b.dataset.themeSet === document.documentElement.dataset.theme)));
  $('#fx-motion').checked = document.documentElement.dataset.motion !== 'off';
}
$('#theme-seg').addEventListener('seg:change', (e) => { localStorage.setItem('fl.theme', e.detail.dataset.themeSet); applyLook(); });
$('#fx-motion').addEventListener('change', (e) => { localStorage.setItem('fl.motion', e.target.checked ? 'on' : 'off'); applyLook(); });

// ---------- Startup ----------
async function start() {
  applyLook();
  const info = await call('app.info');
  if (info.safe_mode) { $('#safe').hidden = false; }
  else {
    const [acc, inst] = await Promise.all([call('accounts.list'), call('instances.list')]);
    renderAccounts(acc);
    S.instances = inst;
    const saved = localStorage.getItem('fl.current');
    S.current = inst.some((i) => i.id === saved) ? saved : inst[0]?.id || null;
    renderInstances();
    refreshRunning();
    $('#app').hidden = false;
    window.FUI.initSegs();
    show(location.hash === '#instance' && S.current ? 'instance' : 'home', { push: false });
    if (location.hash === '#instance') renderInstancePage();
  }
  requestAnimationFrame(() => requestAnimationFrame(async () => {
    const r = await call('app.ready');
    $('#startup').textContent = `запуск ${r.startup_ms} мс`;
    if (!info.safe_mode) { loadVersions(); if (location.search.includes('dev')) window.FUI.devCheck(); }
  }));
}
start().catch((e) => toast(String(e), { error: true }));
