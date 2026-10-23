'use strict';
// Addon host (M3). Each addon is a sandboxed iframe (scripts only, null origin, no Tauri):
// it can only postMessage to us, and we relay its calls to the core as `addon_action`,
// where they are checked against the permissions the user approved for that addon.
// Frames live in three places: `data-slot` spots, own pages (rail tabs, `ui_pages`)
// and the click-through launch overlay (`ui_overlay`).
(() => {
  const { invoke } = window.__TAURI__.core;
  const call = (name, args = null) => invoke('action', { name, args });
  const isWin = navigator.userAgent.includes('Windows');
  const base = (id) => (isWin ? `http://addon.localhost/${id}/` : `addon://localhost/${id}/`);
  // [label, what it means in plain words, risky = highlighted in the consent dialog]
  const PERM = {
    instances_read: [_t("Видеть сборки"), _t("Список сборок, их версии и моды. Ничего не меняет.")],
    accounts_read: [_t("Видеть аккаунты"), _t("Имена аккаунтов. Токены входа плагинам недоступны.")],
    ui_pages: [_t("Свои вкладки"), _t("Добавляет свои страницы в боковую панель.")],
    ui_overlay: [_t("Анимация запуска"), _t("Показывает анимацию поверх окна, пока запускается игра. Клики проходят сквозь неё, Esc убирает.")],
    storage: [_t("Своё хранилище"), _t("Хранит свои настройки внутри лаунчера (до 1 МБ). Другие плагины их не видят.")],
    instances_manage: [_t("Изменять сборки"), _t("Менять настройки сборок, включать, выключать и удалять моды."), true],
    instance_files: [_t("Изменять файлы сборок"), _t("Читать и менять конфиги, настройки и данные миров внутри папок сборок. Моды, программы и файлы вне сборок ему недоступны."), true],
    accounts_manage: [_t("Управлять аккаунтами"), _t("Выбирать и удалять аккаунты."), true],
    network: [_t("Сеть"), _t("Обращаться к сайтам из списка плагина:"), true],
    game_launch: [_t("Запускать игру"), _t("Устанавливать и запускать сборки, останавливать игру."), true],
    accounts_provide: [_t("Вход через сторонний сервис"), _t("Сохранять аккаунты своего сервиса входа."), true],
    process: [_t("Запускать программы"), _t("Запускать программы на компьютере."), true],
  };
  const TIER = { interface: _t("Интерфейс"), function: _t("Функция"), extended: _t("Расширенный") };
  const TRUST = { official: _t("Официальный"), verified: _t("Проверено"), community: _t("Сообщество") };
  const TOKENS = ['--bg', '--surface-1', '--surface-2', '--text', '--text-2', '--text-3', '--line', '--accent', '--on-accent', '--font-ui', '--r-control', '--r-card'];
  let list = [];
  let view = '';

  function h(tag, cls, ...kids) {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    n.append(...kids.filter((k) => k != null));
    return n;
  }
  const toast = (t, o) => window.FUI.toast(t, o);
  const active = (a) => !a.error && a.enabled && !a.pending;
  const has = (a, p) => (a.permissions || []).some((x) => x.id === p && x.granted);

  function themeMsg() {
    const cs = getComputedStyle(document.documentElement);
    const tokens = {};
    for (const k of TOKENS) tokens[k] = cs.getPropertyValue(k).trim();
    return { fl: 1, type: 'theme', base: document.documentElement.dataset.theme || 'dark', tokens };
  }

  function frame(a, file, ctx, cls = '') {
    const f = document.createElement('iframe');
    f.className = `addon-frame ${cls}`.trim();
    f.dataset.addon = a.id;
    f.dataset.kind = ctx.kind;
    f.title = a.name;
    f.setAttribute('sandbox', 'allow-scripts');
    f.setAttribute('referrerpolicy', 'no-referrer');
    f.src = base(a.id) + file;
    f.addEventListener('load', () => {
      f.contentWindow?.postMessage(themeMsg(), '*');
      f.contentWindow?.postMessage({ fl: 1, type: 'context', ...ctx }, '*');
    });
    return f;
  }

  // ---------- Frames in data-slot places ----------
  function mount() {
    document.querySelectorAll('iframe.addon-frame:not(.addon-overlay)').forEach((f) => f.remove());
    pages.clear();
    for (const a of list) {
      if (!active(a)) continue;
      for (const slot of a.slots || []) {
        document.querySelectorAll(`[data-slot="${slot}"]`).forEach((place) => place.append(frame(a, a.entry, { kind: 'slot', slot })));
      }
    }
    rail();
    if (cur && view === 'addon') {
      const [id, pid] = cur.split('/');
      if (!openPage(id, pid)) { cur = null; window.show?.('home'); }
    }
  }

  // ---------- Own pages: a rail tab each (`ui_pages`) ----------
  const pages = new Map(); // "addon/page" -> iframe, kept while the launcher runs
  let cur = null;
  function rail() {
    const box = document.getElementById('rail-addons');
    if (!box) return;
    const btns = [];
    for (const a of list) {
      if (!active(a)) continue;
      for (const p of a.pages || []) {
        const b = h('button', 'rail-btn rail-addon', h('span', 'mono', ([...p.title.trim()][0] || '?').toUpperCase()));
        b.type = 'button';
        b.dataset.page = `${a.id}/${p.id}`;
        b.dataset.tip = `${p.title} · ${a.name}`;
        b.dataset.id = `rail.addon.${a.id}.${p.id}`;
        b.setAttribute('aria-label', p.title);
        b.onclick = () => openPage(a.id, p.id);
        btns.push(b);
      }
    }
    box.replaceChildren(...btns);
    lit();
  }
  function openPage(id, pid) {
    const a = list.find((x) => x.id === id && active(x));
    const p = a?.pages?.find((x) => x.id === pid);
    const host = document.getElementById('addon-page');
    if (!p || !host) return false;
    const key = `${id}/${pid}`;
    let f = pages.get(key);
    if (!f) {
      f = frame(a, p.entry, { kind: 'page', page: pid }, 'addon-page-frame');
      pages.set(key, f);
      host.append(f);
    }
    for (const [k, x] of pages) x.hidden = k !== key;
    cur = key;
    if (view !== 'addon') window.show?.('addon');
    lit();
    return true;
  }
  function lit() {
    document.querySelectorAll('#rail-addons .rail-btn').forEach((b) => (view === 'addon' && b.dataset.page === cur
      ? b.setAttribute('aria-current', 'page') : b.removeAttribute('aria-current')));
  }
  function shown(v) { view = v; lit(); }

  // ---------- Launch overlay (`ui_overlay`): full window, click-through ----------
  let overlay = null;
  let overlayTimer = 0;
  function overlayOff() {
    clearTimeout(overlayTimer);
    overlay?.remove();
    overlay = null;
  }
  function overlayOn(instance) {
    overlayOff();
    const a = list.find((x) => active(x) && x.launch_overlay);
    if (!a) return;
    overlay = frame(a, a.launch_overlay, { kind: 'overlay', instance }, 'addon-overlay');
    overlay.setAttribute('aria-hidden', 'true');
    overlay.tabIndex = -1;
    document.body.append(overlay);
    overlayTimer = setTimeout(overlayOff, 20000); // never stuck on screen
  }
  document.addEventListener('keydown', (e) => { if (e.key === 'Escape' && overlay) overlayOff(); });

  // ---------- Events to addons that may see instances ----------
  function emit(name, data = {}) {
    if (name === 'game.launching') overlayOn(data.instance);
    if (name === 'game.failed') overlayOff();
    if (name === 'game.launched' && overlay) { clearTimeout(overlayTimer); overlayTimer = setTimeout(overlayOff, 6000); }
    const ok = new Set(list.filter((a) => active(a) && (has(a, 'instances_read') || has(a, 'ui_overlay'))).map((a) => a.id));
    document.querySelectorAll('iframe.addon-frame').forEach((f) => {
      if (ok.has(f.dataset.addon)) f.contentWindow?.postMessage({ fl: 1, type: 'event', name, data }, '*');
    });
  }

  window.addEventListener('message', async (e) => {
    const m = e.data;
    if (!m || m.fl !== 1) return;
    const f = [...document.querySelectorAll('iframe.addon-frame')].find((x) => x.contentWindow === e.source);
    if (!f) return; // only our own addon frames may talk to us
    const id = f.dataset.addon;
    if (m.type === 'resize') {
      if (f.dataset.kind === 'slot') f.style.height = `${Math.max(0, Math.min(2000, Number(m.height) || 0))}px`;
    } else if (m.type === 'toast') {
      toast(`${f.title}: ${String(m.text).slice(0, 200)}`);
    } else if (m.type === 'overlay-done') {
      if (f === overlay) overlayOff();
    } else if (m.type === 'call' && typeof m.name === 'string') {
      let reply;
      try { reply = { ok: true, value: await invoke('addon_action', { id, name: m.name, args: m.args ?? null }) }; }
      catch (err) { reply = { ok: false, error: String(err) }; }
      e.source.postMessage({ fl: 1, type: 'result', seq: m.seq, ...reply }, '*');
    }
  });

  // ---------- Consent: each permission separately ----------
  let asking = null;
  function askPerms(a) {
    asking = a.id;
    document.getElementById('ap-title').textContent = _t("Права плагина «{0}»", [a.name]);
    const warn = document.getElementById('ap-warn');
    warn.hidden = a.trust !== 'community' && !a.signature_error;
    warn.textContent = a.signature_error
      ? _t("Подпись нарушена: файлы плагина изменены после проверки.")
      : _t("Плагин не проверен командой Freedom Launcher. Рискованные права давайте только тем, кому доверяете.");
    const box = document.getElementById('ap-list');
    const rows = (a.permissions || []).map((p) => {
      const [label, desc, risky] = PERM[p.id] || [p.id, ''];
      const cb = h('input');
      cb.type = 'checkbox';
      cb.value = p.id;
      cb.checked = p.granted || !p.declined;
      const extra = p.id === 'network' && a.hosts?.length ? ` ${a.hosts.join(', ')}` : '';
      return h('label', `ap-row${risky ? ' is-risky' : ''}`, cb,
        h('span', 'grow', h('span', 't', label, risky ? h('span', 'badge is-warn', _t("Внимание")) : null),
          h('span', 's', desc + extra)));
    });
    box.replaceChildren(...(rows.length ? rows : [h('div', 'hint', _t("Плагину не нужны особые права."))]));
    window.FUI.openDlg('dlg-addon-perms');
  }

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
      ? h('span', 'badge is-warn', _t("Подпись нарушена"))
      : h('span', `badge${a.trust === 'community' ? '' : ' is-accent'}`, TRUST[a.trust] || a.trust);
    if (a.signature_error) badge.title = _t("{0}. Файлы пакета изменены после подписи — он работает как «Сообщество».", [a.signature_error]);
    const perms = h('div', 'addon-perms');
    for (const p of a.permissions || []) {
      const label = p.id === 'network' && a.hosts?.length ? _t("Сеть: {0}", [a.hosts.join(', ')]) : PERM[p.id]?.[0] || p.id;
      const off = !p.granted && p.declined;
      const c = h('span', `chip tier-${p.tier}${off ? ' is-off' : p.granted ? '' : ' is-new'}`, label);
      c.title = off ? _t("Не разрешено") : _t("Уровень «{0}»", [TIER[p.tier] || p.tier]);
      perms.append(c);
    }
    if (!a.permissions?.length) perms.append(h('span', 'chip', _t("Без особых прав")));
    const info = h('div', 'grow', h('div', 't', a.name, ' ', badge), h('div', 's', a.description || meta), perms);
    r.append(info);
    if (a.pending) {
      const b = h('button', 'btn btn-primary', h('span', 'lbl', a.enabled ? _t("Новые права…") : _t("Разрешить…")));
      b.onclick = () => askPerms(a);
      r.append(b);
    } else {
      if (a.permissions?.length) {
        const b = h('button', 'btn btn-ghost', h('span', 'lbl', _t("Права…")));
        b.onclick = () => askPerms(a);
        r.append(b);
      }
      const sw = h('label', 'switch');
      const inp = h('input');
      inp.type = 'checkbox';
      inp.checked = a.enabled;
      inp.onchange = () => act('addons.set_enabled', { id: a.id, enabled: inp.checked });
      sw.append(inp, h('i'));
      r.append(sw);
    }
    if (a.enabled || !a.pending) {
      const x = h('button', 'btn btn-ghost', h('span', 'lbl', _t("Отозвать")));
      x.onclick = () => act('addons.revoke', { id: a.id });
      r.append(x);
    }
    return r;
  }

  function render() {
    const box = document.querySelector('#addon-list');
    if (!box) return;
    box.replaceChildren(...(list.length ? list.map(row)
      : [h('div', 'r', h('div', 'grow', h('div', 't', _t("Плагинов пока нет"))))]));
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
    document.getElementById('dlg-addon-perms')?.addEventListener('submit', (e) => {
      e.preventDefault();
      const permissions = [...document.querySelectorAll('#ap-list input:checked')].map((x) => x.value);
      window.FUI.closeDlg();
      if (asking) act('addons.approve', { id: asking, permissions });
    });
  }

  // Theme switched live: tell running addons.
  const retheme = () => document.querySelectorAll('iframe.addon-frame').forEach((f) => f.contentWindow?.postMessage(themeMsg(), '*'));
  window.FAddons = Object.freeze({ start, mount, retheme, emit, shown, openPage });
})();