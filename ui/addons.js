'use strict';
// Addon host (M3). Each addon is a sandboxed iframe (scripts only, null origin, no Tauri):
// it can only postMessage to us, and we relay its calls to the core as `addon_action`,
// where they are checked against the permissions the user approved for that addon.
(() => {
  const { invoke } = window.__TAURI__.core;
  const call = (name, args = null) => invoke('action', { name, args });
  const isWin = navigator.userAgent.includes('Windows');
  const base = (id) => (isWin ? `http://addon.localhost/${id}/` : `addon://localhost/${id}/`);
  const PERM = {
    instances_read: 'Видеть сборки', accounts_read: 'Видеть аккаунты',
    instances_manage: 'Изменять сборки', accounts_manage: 'Управлять аккаунтами',
    network: 'Сеть', game_launch: 'Запускать игру',
    accounts_provide: 'Вход через сторонний сервис', process: 'Запускать программы',
  };
  const TIER = { interface: 'Интерфейс', function: 'Функция', extended: 'Расширенный' };
  const TRUST = { official: 'Официальный', verified: 'Проверено', community: 'Сообщество' };
  const TOKENS = ['--bg', '--surface-1', '--surface-2', '--text', '--text-2', '--text-3', '--line', '--accent', '--on-accent', '--font-ui', '--r-control', '--r-card'];
  let list = [];

  function h(tag, cls, ...kids) {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    n.append(...kids.filter((k) => k != null));
    return n;
  }
  const toast = (t, o) => window.FUI.toast(t, o);

  function themeMsg() {
    const cs = getComputedStyle(document.documentElement);
    const tokens = {};
    for (const k of TOKENS) tokens[k] = cs.getPropertyValue(k).trim();
    return { fl: 1, type: 'theme', base: document.documentElement.dataset.theme || 'dark', tokens };
  }

  // ---------- Frames in data-slot places ----------
  function mount() {
    document.querySelectorAll('iframe.addon-frame').forEach((f) => f.remove());
    for (const a of list) {
      if (a.error || !a.enabled || a.pending) continue;
      for (const slot of a.slots || []) {
        document.querySelectorAll(`[data-slot="${slot}"]`).forEach((place) => {
          const f = document.createElement('iframe');
          f.className = 'addon-frame';
          f.dataset.addon = a.id;
          f.title = a.name;
          f.setAttribute('sandbox', 'allow-scripts');
          f.setAttribute('referrerpolicy', 'no-referrer');
          f.src = base(a.id) + a.entry;
          f.addEventListener('load', () => f.contentWindow?.postMessage(themeMsg(), '*'));
          place.append(f);
        });
      }
    }
  }

  window.addEventListener('message', async (e) => {
    const m = e.data;
    if (!m || m.fl !== 1) return;
    const f = [...document.querySelectorAll('iframe.addon-frame')].find((x) => x.contentWindow === e.source);
    if (!f) return; // only our own addon frames may talk to us
    const id = f.dataset.addon;
    if (m.type === 'resize') {
      f.style.height = `${Math.max(0, Math.min(800, Number(m.height) || 0))}px`;
    } else if (m.type === 'toast') {
      toast(`${f.title}: ${String(m.text).slice(0, 200)}`);
    } else if (m.type === 'call' && typeof m.name === 'string') {
      let reply;
      try { reply = { ok: true, value: await invoke('addon_action', { id, name: m.name, args: m.args ?? null }) }; }
      catch (err) { reply = { ok: false, error: String(err) }; }
      e.source.postMessage({ fl: 1, type: 'result', seq: m.seq, ...reply }, '*');
    }
  });

  // ---------- Settings → Плагины (launcher extensions; code name "addons") ----------
  function row(a) {
    const r = h('div', 'r addon-row');
    r.dataset.id = `addon.${a.id}`;
    if (a.error) {
      r.append(h('div', 'grow', h('div', 't', a.id), h('div', 's is-error', a.error)));
      return r;
    }
    const meta = [a.version && `v${a.version}`, a.author].filter(Boolean).join(' · ');
    const badge = a.signature_error
      ? h('span', 'badge is-warn', 'Подпись нарушена')
      : h('span', `badge${a.trust === 'community' ? '' : ' is-accent'}`, TRUST[a.trust] || a.trust);
    if (a.signature_error) badge.title = `${a.signature_error}. Файлы пакета изменены после подписи — он работает как «Сообщество».`;
    const perms = h('div', 'addon-perms');
    for (const p of a.permissions || []) {
      const label = p.id === 'network' && a.hosts?.length ? `${PERM.network}: ${a.hosts.join(', ')}` : PERM[p.id] || p.id;
      const c = h('span', `chip tier-${p.tier}${p.granted ? '' : ' is-new'}`, label);
      c.title = `Уровень «${TIER[p.tier] || p.tier}»`;
      perms.append(c);
    }
    if (!a.permissions?.length) perms.append(h('span', 'chip', 'Без особых прав'));
    const info = h('div', 'grow', h('div', 't', a.name, ' ', badge), h('div', 's', a.description || meta), perms);
    r.append(info);
    if (a.pending) {
      const b = h('button', 'btn btn-primary', h('span', 'lbl', a.enabled ? 'Разрешить новые права' : 'Разрешить и включить'));
      b.onclick = () => act('addons.approve', { id: a.id });
      r.append(b);
    } else {
      const sw = h('label', 'switch');
      const inp = h('input');
      inp.type = 'checkbox';
      inp.checked = a.enabled;
      inp.onchange = () => act('addons.set_enabled', { id: a.id, enabled: inp.checked });
      sw.append(inp, h('i'));
      r.append(sw);
    }
    if (a.enabled || !a.pending) {
      const x = h('button', 'btn btn-ghost', h('span', 'lbl', 'Отозвать'));
      x.onclick = () => act('addons.revoke', { id: a.id });
      r.append(x);
    }
    return r;
  }

  function render() {
    const box = document.querySelector('#addon-list');
    if (!box) return;
    box.replaceChildren(...(list.length ? list.map(row)
      : [h('div', 'r', h('div', 'grow', h('div', 't', 'Плагинов пока нет')))]));
  }

  async function act(name, args) {
    try { list = (await call(name, args)) || []; } catch (e) { toast(String(e), { error: true }); }
    render();
    mount();
  }

  async function start() {
    try { list = (await call('addons.list')) || []; } catch { list = []; }
    render();
    mount();
    document.querySelector('#addon-folder')?.addEventListener('click', () => invoke('open_addons_folder').catch((e) => toast(String(e), { error: true })));
    document.querySelector('#addon-reload')?.addEventListener('click', () => act('addons.list'));
  }

  // Theme switched live: tell running addons.
  const retheme = () => document.querySelectorAll('iframe.addon-frame').forEach((f) => f.contentWindow?.postMessage(themeMsg(), '*'));
  window.FAddons = Object.freeze({ start, mount, retheme });
})();