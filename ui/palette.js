'use strict';
// Command palette (Ctrl K): one bar to reach every instance, screen and setting.
// Runs on the same globals as the built-in screens (show, play, openInstance…), never on private state.
(() => {
  const { $, $$, toast } = window.FUI;
  const root = $('#palette'), input = $('#pal-q'), body = $('#pal-body'), list = $('#pal-list'), hlEl = $('#pal-hl');
  if (!root) return;
  const MAC = /Mac|iPhone|iPad/.test(navigator.platform);
  const MOD = MAC ? '⌘' : 'Ctrl';
  const P = {
    home: 'M2.5 7 8 2.5 13.5 7v6.5h-11z', plus: 'M8 3.5v9M3.5 8h9',
    compass: 'M8 13.5a5.5 5.5 0 1 0 0-11 5.5 5.5 0 0 0 0 11ZM10.2 5.8l-1.4 3-3 1.4 1.4-3z',
    gear: 'M8 10.2a2.2 2.2 0 1 0 0-4.4 2.2 2.2 0 0 0 0 4.4ZM8 1.8v1.7M8 12.5v1.7M1.8 8h1.7M12.5 8h1.7M3.6 3.6l1.2 1.2M11.2 11.2l1.2 1.2M3.6 12.4l1.2-1.2M11.2 4.8l1.2-1.2',
    user: 'M8 8a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5ZM3 13.5c.6-2.4 2.6-3.5 5-3.5s4.4 1.1 5 3.5',
    import: 'M8 2.5v7m-3-3 3 3 3-3M3 10.5v3h10v-3', folder: 'M2.5 4.5h4l1.5 1.5h5.5v6.5h-11z',
    moon: 'M12.5 9.5A5 5 0 0 1 6.5 3.5a5 5 0 1 0 6 6Z',
    sun: 'M8 10.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5ZM8 1.5V3M8 13v1.5M1.5 8H3M13 8h1.5M3.4 3.4l1 1M11.6 11.6l1 1M3.4 12.6l1-1M11.6 4.4l1-1',
    image: 'M2.5 3.5h11v9h-11zM2.5 10.5l3-3 3 3 2-2 3 3', motion: 'M2.5 8h3l1.5-3 2 6 1.5-3h3',
    stop: 'M4.5 4.5h7v7h-7z', search: 'M7 11.5a4.5 4.5 0 1 0 0-9 4.5 4.5 0 0 0 0 9ZM10.5 10.5l3 3', bar: 'M2.5 4h11M2.5 12h11M5 8h6',
  };
  const svg = (k) => `<svg viewBox="0 0 16 16"><path d="${P[k]}"/></svg>`;
  const GROUP = { recent: 'Недавние', inst: 'Сборки', go: 'Перейти', act: 'Действия', mods: 'Modrinth' };
  const HINT = { inst: 'Играть', go: 'Перейти', act: 'Выполнить', mods: 'Искать' };

  // ---------- Settings: bar position + backdrop blur ----------
  const pos = () => S.look.palette_pos;
  const blurOn = () => S.look.palette_blur;
  function applyPal() {
    const r = document.documentElement;
    r.dataset.palettePos = pos();
    r.dataset.paletteBlur = blurOn() ? 'on' : 'off';
    $$('#pal-pos-seg button').forEach((b) => b.setAttribute('aria-selected', String(b.dataset.palPos === pos())));
    $('#pal-pos-seg')?._place?.(false);
    if ($('#pal-blur')) $('#pal-blur').checked = blurOn();
  }
  $('#pal-pos-seg')?.addEventListener('seg:change', (e) => { setLook({ palette_pos: e.detail.dataset.palPos }); applyPal(); });
  $('#pal-blur')?.addEventListener('change', (e) => { setLook({ palette_blur: e.target.checked }); applyPal(); });
  document.addEventListener('look:change', applyPal);
  // The settings pane is hidden until opened; place the pill once it gets a size.
  if (window.ResizeObserver && $('#pal-pos-seg')) new ResizeObserver(() => $('#pal-pos-seg')._place?.(true)).observe($('#pal-pos-seg'));
  $$('[data-pal-kbd]').forEach((k) => { k.textContent = `${MOD} K`; });
  applyPal();

  // ---------- Commands ----------
  const openSettings = (p) => { show('settings'); $(`#set-seg button[data-pane="${p}"]`)?.click(); };
  const folder = (cmd, args) => invoke(cmd, args).catch((e) => toast(String(e), { error: true }));
  const label = (b) => b.textContent.trim();
  function commands() {
    const out = [];
    const add = (g, id, t, ico, run, extra = {}) => out.push({ g, id, t, ico, run, ...extra });
    recent().forEach((i) => {
      const on = isRunning(i.id);
      add('inst', `inst.${i.id}`, i.name, null, (o) => (o.alt || on ? openInstance(i.id) : play(i.id, true)),
        { s: instMeta(i), k: `${i.loader || 'vanilla'} ${i.version} сборка`, cover: coverName(i), live: on, hint: on ? 'Открыть' : 'Играть', alt: !on && 'Открыть' });
    });
    add('go', 'go.home', 'Главная', 'home', () => show('home'), { k: 'домой home' });
    add('go', 'go.create', 'Новая сборка', 'plus', () => show('create'), { k: 'создать new instance', keys: [MOD, 'N'] });
    add('go', 'go.discover', 'Дополнения', 'compass', () => show('discover'), { k: 'моды каталог modrinth addons mods' });
    add('go', 'go.accounts', 'Аккаунты', 'user', () => show('accounts'), { k: 'профиль вход microsoft account login' });
    [['game', 'Игра', 'память java ram'], ['launcher', 'Лаунчер', 'тема обои стекло анимации theme'], ['addons', 'Дополнения', 'аддоны addons']]
      .forEach(([p, t, k]) => add('go', `set.${p}`, `Настройки · ${t}`, 'gear', () => openSettings(p), { k: `settings ${k}` }));
    add('act', 'act.import', 'Импорт .mrpack', 'import', () => { show('create'); $('#c-import')?.click(); }, { k: 'modpack модпак import файл' });
    S.running.forEach((id) => { const i = byId(id); if (i) add('act', `act.stop.${id}`, `Остановить «${i.name}»`, 'stop', () => stopGame(id), { k: 'закрыть игру stop kill' }); });
    $$('#theme-seg button:not([aria-selected="true"]):not(:disabled)').forEach((b) =>
      add('act', `theme.${b.dataset.themeSet}`, `Тема · ${label(b)}`, b.dataset.themeSet === 'light' ? 'sun' : 'moon', () => b.click(), { k: 'оформление theme dark light' }));
    $$('#wall-seg button:not([aria-selected="true"])').forEach((b) =>
      add('act', `wall.${b.dataset.wallSet}`, `Обои · ${label(b)}`, 'image', () => b.click(), { k: 'фон wallpaper стекло' }));
    const motion = $('#fx-motion');
    add('act', 'act.motion', motion.checked ? 'Выключить анимации' : 'Включить анимации', 'motion', () => { motion.checked = !motion.checked; motion.dispatchEvent(new Event('change')); }, { k: 'motion эффекты' });
    const other = pos() === 'top' ? 'bottom' : 'top';
    add('act', 'act.palpos', `Командная строка ${other === 'top' ? 'сверху' : 'снизу'}`, 'bar', () => { setLook({ palette_pos: other }); applyPal(); }, { k: 'палитра palette положение' });
    add('act', 'act.palblur', blurOn() ? 'Не размывать фон под строкой' : 'Размывать фон под строкой', 'bar', () => { setLook({ palette_blur: !blurOn() }); applyPal(); }, { k: 'blur размытие палитра' });
    if (S.current) add('act', 'act.folder.inst', 'Открыть папку сборки', 'folder', () => folder('open_instance_folder', { instance: S.current }), { k: 'folder проводник' });
    add('act', 'act.folder.themes', 'Открыть папку тем', 'folder', () => folder('open_themes_folder'), { k: 'folder themes' });
    add('act', 'act.folder.addons', 'Открыть папку дополнений', 'folder', () => folder('open_addons_folder'), { k: 'folder addons' });
    return out;
  }

  // ---------- Matching: substring first, then fuzzy; a query typed in the wrong layout still matches ----------
  const EN = "qwertyuiop[]asdfghjkl;'zxcvbnm,.`", RU = 'йцукенгшщзхъфывапролджэячсмитьбюё';
  const swap = (s) => [...s].map((c) => { const i = EN.indexOf(c); if (i >= 0) return RU[i]; const j = RU.indexOf(c); return j >= 0 ? EN[j] : c; }).join('');
  const norm = (s) => s.toLowerCase().replace(/ё/g, 'е').replace(/\s+/g, ' ').trim();
  function match(q, s) {
    const t = norm(s);
    const at = t.indexOf(q);
    if (at >= 0) {
      const word = at === 0 || /[\s·«(.\-_]/.test(t[at - 1]);
      return { score: 1000 - at + (word ? 300 : 0) + (at === 0 ? 200 : 0), hits: [...Array(q.length).keys()].map((k) => at + k) };
    }
    const hits = []; let from = 0, gaps = 0;
    for (const c of q) {
      if (c === ' ') continue;
      const k = t.indexOf(c, from);
      if (k < 0) return null;
      if (hits.length) gaps += k - from;
      hits.push(k); from = k + 1;
    }
    return gaps > 6 + q.length * 2 ? null : { score: 400 - gaps * 10 - hits[0] * 2, hits };
  }
  function rate(c, qs) {
    let best = null;
    for (const q of qs) {
      const m = match(q, c.t);
      if (m && (!best || m.score > best.score)) best = m;
      const k = match(q, `${c.s || ''} ${c.k || ''}`);
      if (k && (!best || k.score * .55 > best.score)) best = { score: k.score * .55, hits: [] };
    }
    return best;
  }
  const used = () => { try { return JSON.parse(localStorage.getItem('fl.palette-recent') || '[]'); } catch { return []; } };
  function remember(id) { localStorage.setItem('fl.palette-recent', JSON.stringify([id, ...used().filter((x) => x !== id)].slice(0, 8))); }

  function results(raw) {
    const all = commands();
    const u = used();
    if (!raw) {
      const rec = u.map((id) => all.find((c) => c.id === id)).filter(Boolean).slice(0, 4);
      const seen = new Set(rec.map((c) => c.id));
      const rest = (g, n = 99) => all.filter((c) => c.g === g && !seen.has(c.id)).slice(0, n);
      return [['recent', rec], ['inst', rest('inst', 5)], ['go', rest('go')], ['act', rest('act')]].filter(([, l]) => l.length);
    }
    const qs = [...new Set([norm(raw), norm(swap(raw.toLowerCase()))])];
    const hit = all.map((c) => { const m = rate(c, qs); return m && { ...c, m, score: m.score + (u.includes(c.id) ? 80 - u.indexOf(c.id) * 10 : 0) }; }).filter(Boolean);
    const groups = ['inst', 'go', 'act'].map((g) => [g, hit.filter((c) => c.g === g).sort((a, b) => b.score - a.score).slice(0, g === 'inst' ? 6 : 8)]).filter(([, l]) => l.length);
    groups.sort((a, b) => b[1][0].score - a[1][0].score);
    if (S.instances.some((i) => i.loader && i.loader !== 'vanilla')) {
      groups.push(['mods', [{ g: 'mods', id: 'mods.search', t: `Найти «${raw.trim()}»`, s: 'моды на Modrinth', ico: 'search',
        run: () => { show('discover'); const f = $('#disc-q'); if (f) { f.value = raw.trim(); f.dispatchEvent(new Event('input')); } } }]]);
    }
    return groups;
  }

  // ---------- Rendering: rows are keyed, so a row that survives a keystroke stays put and only new rows animate in ----------
  let nodes = new Map(), flat = [], sel = 0, open = false, prevFocus = null;
  function marked(text, hits = []) {
    const set = new Set(hits), out = [];
    let buf = '', on = false;
    [...text].forEach((ch, k) => {
      const h = set.has(k);
      if (h !== on && buf) { out.push(on ? el('mark', {}, buf) : buf); buf = ''; }
      on = h; buf += ch;
    });
    if (buf) out.push(on ? el('mark', {}, buf) : buf);
    return out;
  }
  function row(c, pool, rank) {
    let n = nodes.get(c.id);
    const fresh = !n;
    if (fresh) n = el('div', { class: 'pal-item is-new', role: 'option', 'data-id': `palette.${c.id}` });
    const ico = el('span', { class: `pal-ico${c.cover ? ' is-cover' : ''}`, html: c.ico ? svg(c.ico) : '' });
    if (c.cover) bg(ico, c.cover);
    if (c.live) ico.append(el('i', { class: 'live' }));
    const hint = el('span', { class: 'pal-hint' });
    if (c.alt) hint.append(el('em', {}, c.alt), el('kbd', {}, '⇧ ↵'));
    hint.append(el('em', {}, c.hint || HINT[c.g]), el('kbd', {}, '↵'));
    n.replaceChildren(ico, el('span', { class: 'pal-t' }, el('b', {}, ...marked(c.t, c.m?.hits)), c.s ? el('span', {}, c.s) : ''),
      c.keys ? el('span', { class: 'pal-key' }, ...c.keys.map((k) => el('kbd', {}, k))) : '', hint);
    n.style.setProperty('--i', rank);
    n._c = c;
    pool.set(c.id, n);
    return n;
  }
  function render(first = false) {
    const groups = results(input.value.trim());
    const bottom = pos() === 'bottom';
    // Bottom bar: the best match sits right above the input, so the list is mirrored.
    const ranked = groups.flatMap(([, l]) => l);
    const order = bottom ? [...groups].reverse().map(([g, l]) => [g, [...l].reverse()]) : groups;
    const pool = new Map(), kids = [];
    flat = [];
    for (const [g, l] of order) {
      const key = `group.${g}`;
      const h = nodes.get(key) || el('div', { class: 'pal-group is-new' }, GROUP[g]);
      h.style.setProperty('--i', ranked.indexOf(l[bottom ? l.length - 1 : 0]));
      pool.set(key, h); kids.push(h);
      for (const c of l) { const n = row(c, pool, ranked.indexOf(c)); flat.push(n); kids.push(n); }
    }
    nodes = pool;
    list.replaceChildren(hlEl, ...(kids.length ? kids : [el('div', { class: 'pal-empty is-new' }, 'Ничего не найдено')]));
    body.classList.toggle('is-instant', first);
    body.style.height = `${list.offsetHeight + 13}px`;
    select(bottom ? flat.length - 1 : 0, false, first);
    body.scrollTop = bottom ? body.scrollHeight : 0;
    if (first) requestAnimationFrame(() => body.classList.remove('is-instant'));
  }
  list.addEventListener('animationend', (e) => e.target.classList.remove('is-new'));
  function select(i, scroll = true, instant = false) {
    hlEl.hidden = !flat.length;
    if (!flat.length) return;
    sel = (i + flat.length) % flat.length;
    flat.forEach((n, k) => n.setAttribute('aria-selected', String(k === sel)));
    const n = flat[sel];
    if (instant) { hlEl.style.transition = 'none'; requestAnimationFrame(() => { hlEl.style.transition = ''; }); }
    hlEl.style.transform = `translateY(${n.offsetTop}px)`;
    hlEl.style.height = `${n.offsetHeight}px`;
    if (scroll) n.scrollIntoView({ block: 'nearest' });
  }
  function exec(i, ev) {
    const c = flat[i]?._c;
    if (!c) return;
    if (c.g !== 'mods') remember(c.id);
    close(false);
    try { Promise.resolve(c.run({ alt: !!ev?.shiftKey })).catch((e) => toast(String(e), { error: true })); } catch (e) { toast(String(e), { error: true }); }
  }

  // ---------- Open / close ----------
  function show_(q = '') {
    if (open) { input.select(); return; }
    if ($('#app').hidden || document.querySelector('.dialog[data-open]')) return;
    window.FUI.closeMenu();
    open = true; prevFocus = document.activeElement;
    input.value = q;
    root.toggleAttribute('data-query', !!q.trim());
    nodes = new Map();
    root.removeAttribute('aria-hidden');
    root.setAttribute('data-open', '');
    render(true);
    input.focus({ preventScroll: true });
  }
  function close(restore = true) {
    if (!open) return;
    open = false;
    root.removeAttribute('data-open');
    root.setAttribute('aria-hidden', 'true');
    input.blur();
    if (restore && prevFocus && document.contains(prevFocus)) prevFocus.focus?.({ preventScroll: true });
  }
  input.addEventListener('input', () => { root.toggleAttribute('data-query', !!input.value.trim()); render(); });
  input.addEventListener('keydown', (e) => {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); select(sel + (e.key === 'ArrowDown' ? 1 : -1)); }
    else if (e.key === 'Enter') { e.preventDefault(); exec(sel, e); }
    else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); close(); }
    else if (e.key === 'Tab') e.preventDefault();
  });
  list.addEventListener('mousemove', (e) => { const i = flat.indexOf(e.target.closest('.pal-item')); if (i >= 0 && i !== sel) select(i, false); });
  list.addEventListener('click', (e) => { const i = flat.indexOf(e.target.closest('.pal-item')); if (i >= 0) exec(i, e); });
  $('#pal-scrim').addEventListener('click', () => close());
  $('#pal-open')?.addEventListener('click', () => show_());
  // KeyK by physical key, so Ctrl K also works on the Russian layout (Ctrl Л).
  document.addEventListener('keydown', (e) => {
    if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.code === 'KeyK') { e.preventDefault(); open ? close() : show_(); }
  }, true);
  window.FPalette = { open: show_, close };

  // Preview hook: ?pal=<query> opens the palette once the app is shown.
  const want = new URLSearchParams(location.search).get('pal');
  if (want !== null) { const t = setInterval(() => { if (!$('#app').hidden) { clearInterval(t); setTimeout(() => show_(want), 50); } }, 30); }
})();