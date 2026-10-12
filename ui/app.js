'use strict';
// Built-in UI. It talks to the core only through named actions, exactly like addons.

const { invoke } = window.__TAURI__.core;
const $ = (s) => document.querySelector(s);
const call = (name, args = null) => invoke('action', { name, args });

let accounts = { accounts: [], selected: null, offline_available: false };

function status(msg, isError = false) {
  const el = $('#status-msg');
  el.textContent = msg;
  el.classList.toggle('is-error', isError);
}

function el(tag, attrs = {}, ...children) {
  const n = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === 'class') n.className = v;
    else if (k.startsWith('data-')) n.setAttribute(k, v);
    else n[k] = v;
  }
  for (const c of children) n.append(c);
  return n;
}

// Runs an action for an element and reflects progress in data-state.
async function run(target, name, args = null) {
  target.dataset.state = 'loading';
  try {
    const r = await call(name, args);
    target.dataset.state = 'success';
    setTimeout(() => { if (target.dataset.state === 'success') target.dataset.state = 'idle'; }, 1200);
    return r;
  } catch (e) {
    target.dataset.state = 'error';
    status(String(e), true);
    throw e;
  }
}

// Declarative binding: any element with data-action (and optional data-args JSON).
document.addEventListener('click', async (ev) => {
  const view = ev.target.closest('[data-view]');
  if (view) { show(view.dataset.view); return; }
  const a = ev.target.closest('[data-action]');
  if (!a) return;
  ev.preventDefault();
  const args = a.dataset.args ? JSON.parse(a.dataset.args) : null;
  const result = await run(a, a.dataset.action, args).catch(() => null);
  if (result !== null) {
    a.dispatchEvent(new CustomEvent('action:done', { bubbles: true, detail: { name: a.dataset.action, result } }));
  }
});

document.addEventListener('action:done', (ev) => {
  const { name, result } = ev.detail;
  if (name.startsWith('accounts.')) renderAccounts(result);
  if (name.startsWith('safe.')) $('#safe-msg').textContent = 'Готово. Перезапустите лаунчер.';
});

function show(view) {
  document.querySelectorAll('.view').forEach((v) => v.classList.toggle('is-active', v.id === `view-${view}`));
  document.querySelectorAll('.tab').forEach((t) => t.classList.toggle('is-active', t.dataset.view === view));
}

// ---------- Accounts ----------
const KIND = { microsoft: 'Microsoft', offline: 'Офлайн-профиль', provider: 'Сторонний сервис' };

function renderAccounts(state) {
  accounts = state;
  const list = $('#account-list');
  list.replaceChildren(...state.accounts.map((a) => el('li', { class: `item${a.id === state.selected ? ' is-selected' : ''}` },
    el('div', { class: 'grow' }, el('div', {}, a.name), el('div', { class: 'sub' }, KIND[a.kind.type] || a.kind.type)),
    a.id === state.selected
      ? el('span', { class: 'sub' }, 'выбран')
      : el('button', { class: 'btn', 'data-action': 'accounts.select', 'data-args': JSON.stringify({ id: a.id }) }, 'Выбрать'),
    el('button', { class: 'btn ghost', 'data-action': 'accounts.remove', 'data-args': JSON.stringify({ id: a.id }) }, 'Удалить'),
  )));
  $('#offline-open').hidden = !state.offline_available;
  $('#offline-hint').hidden = state.offline_available;
  const sel = state.accounts.find((a) => a.id === state.selected);
  $('#account-chip').textContent = sel ? sel.name : 'Войти';
}

$('#ms-login').addEventListener('click', async (ev) => {
  const btn = ev.currentTarget;
  let flow;
  try { flow = await run(btn, 'accounts.microsoft.begin'); } catch { return; }
  $('#ms-code').textContent = flow.user_code;
  $('#ms-flow').hidden = false;
  $('#ms-open').onclick = () => invoke('open_login_page', { url: flow.verification_uri }).catch((e) => status(String(e), true));
  $('#ms-copy').onclick = () => navigator.clipboard.writeText(flow.user_code).then(() => status('Код скопирован'));
  try {
    renderAccounts(await call('accounts.microsoft.wait'));
    status('Вход выполнен');
  } catch (e) {
    status(String(e), true);
  } finally {
    $('#ms-flow').hidden = true;
  }
});

$('#offline-open').addEventListener('click', () => { $('#offline-form').hidden = !$('#offline-form').hidden; });
$('#offline-form').addEventListener('submit', async (ev) => {
  ev.preventDefault();
  const form = ev.currentTarget;
  try {
    renderAccounts(await run(form, 'accounts.offline.create', { name: form.name.value }));
    form.reset();
    form.hidden = true;
  } catch { /* shown in status */ }
});

// ---------- Instances ----------
function renderInstances(list) {
  $('#instances-empty').hidden = list.length > 0;
  $('#instance-list').replaceChildren(...list.map((i) => el('li', { class: 'item' },
    el('div', { class: 'grow' }, el('div', {}, i.name), el('div', { class: 'sub' }, `Minecraft ${i.version}`)),
    el('button', { class: 'btn primary', disabled: true, title: 'Запуск игры появится на следующем этапе' }, 'Играть'),
  )));
}

async function loadVersions() {
  const select = $('#version-select');
  try {
    const r = await call('versions.list', { snapshots: $('#snapshots').checked });
    select.replaceChildren(...r.versions.map((v) => el('option', { value: v.id }, v.type === 'snapshot' ? `${v.id} (снапшот)` : v.id)));
  } catch (e) {
    select.replaceChildren(el('option', { value: '' }, 'Нет списка версий'));
    status(String(e), true);
  }
}
$('#snapshots').addEventListener('change', loadVersions);

$('#create-form').addEventListener('submit', async (ev) => {
  ev.preventDefault();
  const form = ev.currentTarget;
  try {
    await run($('#create-btn'), 'instances.create', { name: form.name.value, version: form.version.value });
    form.name.value = '';
    renderInstances(await call('instances.list'));
  } catch { /* shown in status */ }
});

// ---------- Startup ----------
async function start() {
  const info = await call('app.info');
  if (info.safe_mode) {
    $('#safe').hidden = false;
  } else {
    const [acc, inst] = await Promise.all([call('accounts.list'), call('instances.list')]);
    renderAccounts(acc);
    renderInstances(inst);
    $('#app').hidden = false;
  }
  // Two frames: the first paint has actually happened.
  requestAnimationFrame(() => requestAnimationFrame(async () => {
    const r = await call('app.ready');
    const s = $('#startup');
    if (s) s.textContent = `запуск ${r.startup_ms} мс`;
    if (!info.safe_mode) loadVersions(); // network work only after first paint
  }));
}

start().catch((e) => status(String(e), true));