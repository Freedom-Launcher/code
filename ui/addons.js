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
  function shown(v) {
    const was = view;
    view = v;
    lit();
    if (v !== 'plugins' || was === 'plugins') return;
    const boxes = ['addon-list', 'plg-catalog'].map((id) => document.getElementById(id)).filter(Boolean);
    boxes.forEach((b) => b.classList.add('is-enter'));
    render();
    if (tab === 'catalog') loadCat(false);
    setTimeout(() => boxes.forEach((b) => b.classList.remove('is-enter')), 900);
  }

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
    window.FUI.closeDlg();
    window.FUI.openDlg('dlg-addon-perms');
  }

  // ---------- «Плагины» page (launcher extensions; code name "addons") ----------
  let filter = 'all', query = '', detail = null, armed = 0;
  let tab = (() => { try { return localStorage.getItem('fl.plg.tab') === 'installed' ? 'installed' : 'catalog'; } catch { return 'catalog'; } })();
  let cat = [], catSt = 'idle', catErr = '';
  const busy = new Set();
  const attn = (a) => !!(a.error || a.pending || a.signature_error);
  const FILTERS = [['all', _t("Все")], ['on', _t("Включены")], ['off', _t("Выключены")], ['attn', _t("Требуют внимания")]];
  const pass = (k, a) => k === 'all' || (k === 'on' ? active(a) : k === 'off' ? !a.error && !a.pending && !a.enabled : attn(a));
  const match = (a) => !query || [a.name, a.id, a.author, a.description].filter(Boolean).join(' ').toLowerCase().includes(query);
  const hue = (s) => { let x = 7; for (const ch of String(s)) x = (x * 31 + ch.codePointAt(0)) % 360; return x; };
  const hueOf = (a) => a.hue ?? cat.find((e) => e.id === a.id)?.hue ?? hue(a.id);
  const stop = (n) => { n.addEventListener('click', (e) => e.stopPropagation()); return n; };

  function icon(a, cls = '') {
    const n = h('span', `plg-ico ${cls}`.trim(), letter(a));
    n.style.setProperty('--h', String(hueOf(a)));
    n.setAttribute('aria-hidden', 'true');
    return n;
  }
  function trustBadge(a) {
    if (a.error) return h('span', 'badge is-warn', _t("Ошибка"));
    const b = a.signature_error
      ? h('span', 'badge is-warn', _t("Подпись нарушена"))
      : h('span', `badge${a.trust === 'community' ? '' : ' is-accent'}`, TRUST[a.trust] || a.trust || '');
    if (a.signature_error) b.title = _t("{0}. Файлы пакета изменены после подписи — он работает как «Сообщество».", [a.signature_error]);
    return b;
  }
  function chips(a, max) {
    const box = h('div', 'addon-perms plg-perms');
    const ps = a.permissions || [];
    for (const p of ps.slice(0, max)) {
      const label = p.id === 'network' && a.hosts?.length ? _t("Сеть: {0}", [a.hosts.join(', ')]) : PERM[p.id]?.[0] || p.id;
      const off = !p.granted && p.declined;
      const c = h('span', `chip tier-${p.tier}${off ? ' is-off' : p.granted ? '' : ' is-new'}`, label);
      c.title = off ? _t("Не разрешено") : _t("Уровень «{0}»", [TIER[p.tier] || p.tier]);
      box.append(c);
    }
    if (ps.length > max) box.append(h('span', 'chip is-more', `+${ps.length - max}`));
    if (!ps.length) box.append(h('span', 'chip is-safe', _t("Без особых прав")));
    return box;
  }
  function state(a) {
    if (a.error) return ['err', _t("Не загрузился")];
    if (a.pending) return ['attn', a.enabled ? _t("Просит новые права") : _t("Ждёт разрешения")];
    return a.enabled ? ['on', _t("Включён")] : ['off', _t("Выключен")];
  }
  function toggle(a) {
    const sw = h('label', 'switch');
    const inp = h('input');
    inp.type = 'checkbox';
    inp.checked = !!a.enabled;
    inp.setAttribute('aria-label', a.name || a.id);
    inp.onchange = () => act('addons.set_enabled', { id: a.id, enabled: inp.checked });
    sw.append(inp, h('i'));
    return stop(sw);
  }
  function allowBtn(a) {
    const b = h('button', 'btn btn-primary', h('span', 'lbl', a.enabled ? _t("Новые права…") : _t("Разрешить…")));
    b.type = 'button';
    b.onclick = () => askPerms(a);
    return stop(b);
  }
  const SVG_SHIELD = '<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M12 2.2 4.5 5v6.1c0 4.6 3.1 8.9 7.5 10.7 4.4-1.8 7.5-6.1 7.5-10.7V5L12 2.2Z"/></svg>';
  const letter = (a) => ([...String(a.name ? _t(a.name) : a.id).trim()][0] || '?').toUpperCase();
  function banner(a) {
    const b = h('div', 'plg-banner', icon(a));
    b.style.setProperty('--h', String(hueOf(a)));
    b.dataset.l = letter(a);
    b.setAttribute('aria-hidden', 'true');
    return b;
  }
  function flags(a, st, stText) {
    const [cls, txt] = a.error ? ['is-warn', _t("Ошибка")]
      : a.signature_error ? ['is-warn', _t("Подпись нарушена")]
      : [a.trust === 'community' ? '' : 'is-accent', TRUST[a.trust] || a.trust || ''];
    const t = h('span', `plg-flag ${cls}`.trim(), h('i'), txt);
    if (a.signature_error) t.title = _t("{0}. Файлы пакета изменены после подписи — он работает как «Сообщество».", [a.signature_error]);
    return h('div', 'plg-flags', t, h('span', `plg-flag is-st-${st}`, h('i'), stText));
  }
  function permStat(a) {
    const ps = a.permissions || [];
    const s = h('span', `plg-stat${ps.length ? '' : ' is-safe'}${a.pending ? ' is-attn' : ''}`);
    s.innerHTML = SVG_SHIELD;
    s.append(ps.length ? _t("Права: {0}", [String(ps.length)]) : _t("Без особых прав"));
    if (ps.length) s.title = ps.map((p) => PERM[p.id]?.[0] || p.id).join(', ');
    return s;
  }
  function card(a, i) {
    const [st, stText] = state(a);
    const c = h('article', `plg-card is-${st}`);
    c.dataset.id = `addon.${a.id}`;
    c.style.setProperty('--i', String(i));
    const meta = a.error ? _t("Ошибка в пакете") : [a.author, a.version && `v${a.version}`].filter(Boolean).join(' · ');
    c.append(banner(a), flags(a, st, stText),
      h('div', 'plg-body', h('div', 'plg-name', a.name ? _t(a.name) : a.id), h('div', 'plg-meta', meta),
        h('p', `plg-desc${a.error ? ' is-error' : ''}`, a.error || (a.description && _t(a.description)) || _t("Автор не добавил описание."))));
    const foot = h('div', 'plg-foot');
    const ce = cat.find((e) => e.id === a.id);
    if (!a.error) foot.append(ce ? nums(ce) : permStat(a));
    foot.append(h('span', 'grow'));
    if (a.error) {
      const b = h('button', 'btn btn-ghost', h('span', 'lbl', _t("Открыть папку")));
      b.type = 'button';
      b.onclick = () => invoke('open_addons_folder').catch((e) => toast(String(e), { error: true }));
      foot.append(b);
    } else foot.append(a.pending ? allowBtn(a) : toggle(a));
    c.append(foot);
    if (!a.error) {
      c.tabIndex = 0;
      c.onclick = () => openDetail(a.id);
      c.onkeydown = (e) => { if ((e.key === 'Enter' || e.key === ' ') && e.target === c) { e.preventDefault(); openDetail(a.id); } };
    }
    return c;
  }
  function emptyState() {
    const b = h('button', 'btn btn-primary', h('span', 'lbl', _t("Открыть каталог")));
    b.type = 'button';
    b.onclick = () => setTab('catalog');
    return h('div', 'plg-empty', h('h3', null, _t("Плагинов пока нет")), b);
  }
  function noneState() {
    const b = h('button', 'btn btn-ghost', h('span', 'lbl', _t("Сбросить фильтр")));
    b.type = 'button';
    b.onclick = () => {
      filter = 'all'; query = '';
      const q = document.getElementById('plg-q');
      if (q) q.value = '';
      render();
    };
    return h('div', 'plg-none', h('b', null, _t("Ничего не нашлось")), h('p', null, _t("Попробуйте другое слово или сбросьте фильтр.")), b);
  }

  // ---------- Catalog: plugins/catalog.json of Freedom-Launcher/releases, live numbers from GitHub ----------
  const SVG_DL = '<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M12 3a1 1 0 0 1 1 1v9.59l3.3-3.3a1 1 0 1 1 1.4 1.42l-5 5a1 1 0 0 1-1.4 0l-5-5a1 1 0 1 1 1.4-1.42l3.3 3.3V4a1 1 0 0 1 1-1ZM5 19a1 1 0 1 0 0 2h14a1 1 0 1 0 0-2H5Z"/></svg>';
  const SVG_HEART = '<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M12 20.7 10.6 19.4C5.4 14.7 2 11.6 2 7.8 2 4.7 4.4 2.3 7.5 2.3c1.7 0 3.4.8 4.5 2.1 1.1-1.3 2.8-2.1 4.5-2.1 3.1 0 5.5 2.4 5.5 5.5 0 3.8-3.4 6.9-8.6 11.6L12 20.7Z"/></svg>';
  const fmtN = (n) => (n >= 1e6 ? _t("{0} млн", [(n / 1e6).toFixed(n >= 1e7 ? 0 : 1)]) : n >= 1e3 ? _t("{0} тыс", [Math.round(n / 1e3)]) : String(n));
  const newer = (a, b) => {
    const x = String(a).split('.').map(Number), y = String(b || '').split('.').map(Number);
    for (let i = 0; i < Math.max(x.length, y.length); i++) { const d = (x[i] || 0) - (y[i] || 0); if (d) return d > 0; }
    return false;
  };
  function nums(e) {
    const dl = h('span', 'plg-num');
    dl.innerHTML = SVG_DL;
    dl.append(fmtN(e.downloads || 0));
    dl.title = _t("Скачиваний: {0}", [String(e.downloads || 0)]);
    const like = h(e.like_url ? 'button' : 'span', 'plg-num is-like');
    like.innerHTML = SVG_HEART;
    like.append(fmtN(e.likes || 0));
    if (e.like_url) {
      like.type = 'button';
      like.dataset.tip = _t("Поставить лайк на GitHub");
      like.setAttribute('aria-label', _t("Поставить лайк на GitHub"));
      like.onclick = () => invoke('open_link', { url: e.like_url }).catch((x) => toast(String(x), { error: true }));
    }
    return stop(h('div', 'plg-nums', dl, like));
  }
  function catCard(e, i) {
    const have = list.find((a) => a.id === e.id);
    const upd = !!have && !have.error && newer(e.version, have.version);
    const c = h('article', `plg-card is-cat${have ? ' is-have' : ''}`);
    c.style.setProperty('--i', String(i));
    const fl = [h('span', 'plg-flag is-accent', h('i'), TRUST.official)];
    if (have) fl.push(upd ? h('span', 'plg-flag is-st-attn', h('i'), _t("Есть обновление")) : h('span', 'plg-flag is-st-on', h('i'), _t("Установлен")));
    const meta = [e.author, e.version && `v${e.version}`].filter(Boolean).join(' · ');
    c.append(banner(e), h('div', 'plg-flags', ...fl),
      h('div', 'plg-body', h('div', 'plg-name', _t(e.name)), h('div', 'plg-meta', meta), h('p', 'plg-desc', _t(e.description || ''))));
    const wait = busy.has(e.id);
    const b = h('button', `btn ${have && !upd ? 'btn-ghost' : 'btn-primary'}`, h('span', 'lbl',
      wait ? _t("Установка…") : upd ? _t("Обновить") : have ? _t("Открыть") : _t("Установить")));
    b.type = 'button';
    b.disabled = wait;
    b.onclick = () => (have && !upd ? (have.error ? setTab('installed') : openDetail(e.id)) : installFrom(e.id));
    c.append(h('div', 'plg-foot', nums(e), h('span', 'grow'), stop(b)));
    return c;
  }
  function renderCat() {
    const box = document.getElementById('plg-catalog');
    if (!box) return;
    box.hidden = tab !== 'catalog';
    if (tab !== 'catalog') return;
    if (!cat.length && (catSt === 'idle' || catSt === 'loading')) {
      box.replaceChildren(h('div', 'plg-none', h('b', null, _t("Загрузка…"))));
      return;
    }
    if (!cat.length && catSt === 'err') {
      const b = h('button', 'btn btn-ghost', h('span', 'lbl', _t("Повторить")));
      b.type = 'button';
      b.onclick = () => loadCat(true);
      box.replaceChildren(h('div', 'plg-none', h('b', null, _t("Каталог недоступен")), h('p', null, _tr(catErr)), b));
      return;
    }
    const ok = (e) => !query || [e.name, _t(e.name), e.id, e.description, _t(e.description || '')].filter(Boolean).join(' ').toLowerCase().includes(query);
    const shown = cat.filter(ok);
    box.replaceChildren(...(shown.length ? shown.map(catCard) : [noneState()]));
  }
  async function loadCat(refresh) {
    if (catSt === 'loading') return;
    catSt = 'loading';
    renderCat();
    try {
      const r = await call('addons.catalog', { refresh: !!refresh });
      cat = Array.isArray(r?.plugins) ? r.plugins : [];
      catSt = 'ok';
    } catch (e) {
      catErr = String(e);
      catSt = 'err';
      if (cat.length) toast(_tr(catErr), { error: true });
    }
    render();
  }
  async function installFrom(id) {
    if (busy.has(id)) return;
    busy.add(id);
    renderCat();
    let ok = false;
    try { list = (await call('addons.install_from', { id })) || list; ok = true; } catch (e) { toast(_tr(String(e)), { error: true }); }
    busy.delete(id);
    render();
    if (!ok) return;
    mount();
    const a = list.find((x) => x.id === id);
    if (a?.pending) askPerms(a); else toast(_t("Плагин установлен"));
  }
  function setTab(t) {
    tab = t === 'installed' ? 'installed' : 'catalog';
    try { localStorage.setItem('fl.plg.tab', tab); } catch { /* no storage */ }
    if (tab === 'catalog' && catSt !== 'ok') loadCat(false);
    render();
  }
  function tabs(need) {
    document.querySelectorAll('#plg-tabs .plg-tab').forEach((b) => {
      const on = b.dataset.tab === tab;
      b.classList.toggle('is-on', on);
      b.setAttribute('aria-selected', String(on));
    });
    const n = document.getElementById('plg-count');
    if (n) { n.textContent = list.length ? String(list.length) : ''; n.classList.toggle('is-attn', !!need); }
  }

  function render() {
    const box = document.getElementById('addon-list');
    if (!box) return;
    const on = list.filter(active).length, need = list.filter(attn).length;
    if (filter === 'attn' && !need) filter = 'all';
    document.getElementById('plg-filter')?.replaceChildren(...FILTERS.filter(([k]) => k !== 'attn' || need).map(([k, t]) => {
      const b = h('button', `chip${filter === k ? ' is-on' : ''}${k === 'attn' ? ' is-attn' : ''}`, t, h('b', null, String(list.filter((a) => pass(k, a)).length)));
      b.type = 'button';
      b.setAttribute('aria-pressed', String(filter === k));
      b.onclick = () => { filter = k; render(); };
      return b;
    }));
    const bar = document.getElementById('plg-bar');
    if (bar) bar.hidden = tab !== 'installed' || !list.length;
    const shown = list.filter((a) => pass(filter, a) && match(a));
    box.replaceChildren(...(!list.length ? [emptyState()] : shown.length ? shown.map(card) : [noneState()]));
    box.hidden = tab !== 'installed';
    tabs(need);
    renderCat();
    if (detail && document.getElementById('dlg-plugin')?.hasAttribute('data-open')) fill();
  }

  // ---------- Plugin details ----------
  function openDetail(id) {
    detail = id;
    disarm();
    fill();
    window.FUI.closeDlg();
    window.FUI.openDlg('dlg-plugin');
  }
  function disarm() {
    clearTimeout(armed);
    armed = 0;
    const b = document.getElementById('plg-revoke');
    if (!b) return;
    b.classList.remove('is-danger');
    b.querySelector('.lbl').textContent = _t("Отозвать права");
  }
  function fill() {
    const a = list.find((x) => x.id === detail);
    const body = document.getElementById('plg-dlg-body');
    if (!body) return;
    if (!a || a.error) { window.FUI.closeDlg(); return; }
    const meta = [a.version && `v${a.version}`, a.author].filter(Boolean).join(' · ');
    const [st, stText] = state(a);
    const head = h('div', 'plg-dh', icon(a, 'is-big'),
      h('div', 'plg-who', h('h2', null, a.name ? _t(a.name) : a.id),
        h('div', 'plg-meta', trustBadge(a), meta ? h('span', null, meta) : null, h('span', `plg-state is-${st}`, h('i'), stText))),
      a.pending ? null : toggle(a));
    const parts = [head, h('p', 'plg-ddesc', (a.description && _t(a.description)) || _t("Автор не добавил описание."))];
    if (a.signature_error) parts.push(h('p', 'ap-warn', _t("Подпись нарушена: файлы плагина изменены после проверки.")));
    const rows = (a.permissions || []).map((p) => {
      const [label, desc, risky] = PERM[p.id] || [p.id, ''];
      const [k, t] = p.granted ? ['ok', _t("Разрешено")] : p.declined ? ['no', _t("Не разрешено")] : ['new', _t("Ждёт решения")];
      const extra = p.id === 'network' && a.hosts?.length ? ` ${a.hosts.join(', ')}` : '';
      return h('div', `plg-prow${risky ? ' is-risky' : ''}`, h('div', 'grow', h('div', 't', label), h('div', 's', desc + extra)), h('span', `plg-pst is-${k}`, t));
    });
    parts.push(h('div', 'plg-sec', h('h3', null, _t("Права")),
      rows.length ? h('div', 'plg-plist', ...rows) : h('div', 'hint', _t("Плагину не нужны особые права."))));
    if (active(a) && a.pages?.length) {
      parts.push(h('div', 'plg-sec', h('h3', null, _t("Вкладки")), h('div', 'plg-pages', ...a.pages.map((p) => {
        const b = h('button', 'btn', h('span', 'lbl', p.title));
        b.type = 'button';
        b.onclick = () => { window.FUI.closeDlg(); openPage(a.id, p.id); };
        return b;
      }))));
    }
    parts.push(h('div', 'plg-id', a.id));
    body.replaceChildren(...parts);
    const pb = document.getElementById('plg-perms');
    if (pb) {
      pb.hidden = !a.permissions?.length;
      pb.querySelector('.lbl').textContent = a.pending ? (a.enabled ? _t("Новые права…") : _t("Разрешить…")) : _t("Права…");
    }
  }
  async function install() {
    try {
      const r = await invoke('pick_addon');
      if (!r) return;
      list = r;
      setTab('installed');
      mount();
      toast(_t("Плагин установлен. Разрешите его права, чтобы включить."));
    } catch (e) { toast(String(e), { error: true }); }
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
    document.querySelector('#addon-install')?.addEventListener('click', install);
    document.getElementById('plg-q')?.addEventListener('input', (e) => { query = e.target.value.trim().toLowerCase(); render(); });
    document.getElementById('plg-perms')?.addEventListener('click', () => {
      const a = list.find((x) => x.id === detail);
      if (a) askPerms(a);
    });
    document.getElementById('plg-revoke')?.addEventListener('click', (e) => {
      const b = e.currentTarget;
      if (!armed) {
        b.classList.add('is-danger');
        b.querySelector('.lbl').textContent = _t("Точно отозвать?");
        armed = setTimeout(disarm, 2600);
        return;
      }
      const id = detail;
      disarm();
      window.FUI.closeDlg();
      act('addons.revoke', { id });
    });
    document.querySelector('#addon-reload')?.addEventListener('click', () => { act('addons.list'); loadCat(true); });
    document.querySelectorAll('#plg-tabs .plg-tab').forEach((b) => b.addEventListener('click', () => setTab(b.dataset.tab)));
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