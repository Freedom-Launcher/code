'use strict';
// Built-in UI. It talks to the core only through named actions, exactly like addons.
const { invoke } = window.__TAURI__.core;
const { $, $$, toast, progress, openDlg, closeDlg, closeMenu, swapText } = window.FUI;
const call = (name, args = null) => invoke('action', { name, args });

const S = { accounts: { accounts: [], selected: null, offline_available: false }, instances: [], current: null, playing: false };
const KIND = { microsoft: 'Microsoft', offline: 'Офлайн', provider: 'Сервис' };
const STAGE = { idle: 'Подготовка', java: 'Java', version: 'Версия', libraries: 'Библиотеки', assets: 'Ресурсы', natives: 'Нативные файлы', launch: 'Запуск', done: 'Готово' };

function el(tag, attrs = {}, ...kids) {
  const n = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === 'class') n.className = v;
    else if (k.startsWith('data-') || k.startsWith('aria-')) n.setAttribute(k, v);
    else n[k] = v;
  }
  n.append(...kids);
  return n;
}
const fmtBytes = (b) => (b >= 1 << 30 ? `${(b / (1 << 30)).toFixed(1)} ГБ` : b >= 1 << 20 ? `${(b / (1 << 20)).toFixed(0)} МБ` : `${Math.round(b / 1024)} КБ`);
const initials = (s) => (s.trim()[0] || '?').toUpperCase();

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

// Declarative binding for themes: [data-action] + optional [data-args] JSON.
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

function show(view) {
  $$('.view').forEach((x) => x.classList.toggle('is-active', x.id === `view-${view}`));
  $$('#nav button').forEach((b) => b.setAttribute('aria-selected', String(b.dataset.view === view)));
  $('#nav')._place?.(false);
}

// ---------- Accounts ----------
function renderAccounts(state) {
  S.accounts = state;
  $('#acc-list').replaceChildren(...state.accounts.map((a) => el('button', {
    class: 'menu-item acc-row', 'data-action': 'accounts.select', 'data-args': JSON.stringify({ id: a.id }), 'aria-selected': String(a.id === state.selected),
  }, el('span', { class: 'inst-ico head' }, initials(a.name)), a.name, el('span', { class: 'sub' }, a.id === state.selected ? '✓ ' + (KIND[a.kind.type] || '') : KIND[a.kind.type] || ''))));
  if (!state.accounts.length) $('#acc-list').append(el('div', { class: 'menu-item faint' }, 'Нет аккаунтов'));
  $('#offline-open').hidden = !state.offline_available;
  const sel = state.accounts.find((a) => a.id === state.selected);
  swapText($('#account-name'), sel ? sel.name : 'Войти');
  $('#account-head').textContent = sel ? initials(sel.name) : '?';
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

// ---------- Instances ----------
function renderInstances() {
  const q = $('#inst-search').value.trim().toLowerCase();
  const list = S.instances.filter((i) => !q || i.name.toLowerCase().includes(q));
  $('#inst-list').replaceChildren(...list.map((i) => el('div', {
    class: 'row', role: 'option', 'aria-selected': String(i.id === S.current), 'data-id': `instance.${i.id}`, tabIndex: 0, onclick: () => select(i.id),
  }, el('span', { class: 'inst-ico' }, initials(i.name)), el('div', { class: 'grow' }, el('div', { class: 'title' }, i.name), el('div', { class: 'sub mono' }, i.version)))));
  if (S.instances.length && !list.length) $('#inst-list').append(el('div', { class: 'rail-group' }, 'Ничего не найдено'));
  const cur = S.instances.find((i) => i.id === S.current);
  $('#stage').hidden = !cur;
  $('#stage-empty').hidden = !!cur;
  if (!cur) return;
  $('#st-name').textContent = cur.name;
  $('#st-version').textContent = cur.version;
  $('#f-version').textContent = cur.version;
  $('#st-meta').textContent = `Создана ${new Date(cur.created_at * 1000).toLocaleDateString('ru-RU', { day: 'numeric', month: 'long' })}`;
}
function select(id) { if (S.playing) return; S.current = id; localStorage.setItem('fl.current', id); renderInstances(); }
$('#inst-search').addEventListener('input', renderInstances);

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
    S.current = created?.id || S.instances[S.instances.length - 1]?.id;
    f.name.value = '';
    closeDlg();
    renderInstances();
  } catch { /* toast shown */ }
});

// ---------- Play: install (with progress polling) → launch ----------
async function play(launch = true) {
  if (S.playing || !S.current) return;
  if (launch && !S.accounts.selected) { toast('Сначала войдите в аккаунт', { error: true }); $('#account-btn').click(); return; }
  const btn = $('#play');
  const instance = S.current;
  S.playing = true;
  btn.dataset.state = 'loading';
  $('#play-status').hidden = false;
  progress($('#ps-bar'), null);
  const poll = setInterval(async () => {
    const p = await call('game.progress', { instance }).catch(() => null);
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
    await call('game.install', { instance });
    progress($('#ps-bar'), 1);
    if (launch) { $('#ps-stage').textContent = STAGE.launch; await call('game.launch', { instance }); }
    btn.dataset.state = 'success';
    toast(launch ? 'Игра запущена' : 'Файлы в порядке');
  } catch (e) {
    btn.dataset.state = 'error';
    toast(String(e), { error: true });
  } finally {
    clearInterval(poll);
    S.playing = false;
    setTimeout(() => { $('#play-status').hidden = true; $('#ps-count').textContent = ''; if (btn.dataset.state !== 'loading') btn.dataset.state = 'idle'; }, 1400);
  }
}
$('#play').addEventListener('click', () => play(true));
$('#mi-install').addEventListener('click', () => play(false));

document.addEventListener('keydown', (e) => {
  if (e.target.matches('input, select, textarea')) return;
  if (e.ctrlKey && e.key.toLowerCase() === 'n') { e.preventDefault(); openDlg('dlg-create'); }
  if (e.key === 'Enter' && $('#view-instances').classList.contains('is-active') && !document.querySelector('.dialog[data-open]')) play(true);
});

// ---------- Settings: theme + effects (saved locally until settings.set exists) ----------
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
    $('#app').hidden = false;
    window.FUI.initSegs();
  }
  requestAnimationFrame(() => requestAnimationFrame(async () => {
    const r = await call('app.ready');
    $('#startup').textContent = `запуск ${r.startup_ms} мс`;
    if (!info.safe_mode) { loadVersions(); if (location.search.includes('dev')) window.FUI.devCheck(); }
  }));
}
start().catch((e) => toast(String(e), { error: true }));