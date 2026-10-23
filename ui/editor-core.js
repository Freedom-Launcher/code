'use strict';
// Interface editor, startup half: the saved settings.ui (with the theme's own edits under it)
// becomes CSS variables and [data-id] rules from the first frame. The editor panel itself
// (editor.js, ~45 KB) loads the first time it is opened — or as soon as the pointer is over
// its button, so the first open is still instant.
(() => {
  const { $ } = window.FUI;
  const root = document.documentElement;
  const RADIUS = [['btn', _t("Кнопки")], ['field', _t("Поля")], ['card', _t("Карточки")], ['icon', _t("Иконки")], ['menu', _t("Меню")], ['chip', _t("Метки")], ['nav', _t("Боковая панель")], ['dialog', _t("Окна")]];
  const SIZE = [['btn', _t("Кнопки"), 70, 150], ['field', _t("Поля"), 70, 150], ['text', _t("Текст"), 85, 130], ['gap', _t("Отступы"), 60, 160], ['rail', _t("Боковая панель"), 80, 150], ['top', _t("Верхняя панель"), 80, 180], ['card', _t("Карточки сборок"), 60, 200]];
  const LAYOUT = [['rail', _t("Боковая панель"), 'left'], ['filters', _t("Фильтры каталога"), 'left'], ['project', _t("Панель проекта"), 'right']];
  const NAMES = {
    rail: _t("Боковая панель"), 'rail.logo': _t("Логотип"), 'rail.instances': _t("Сборки в панели"), topbar: _t("Верхняя панель"), main: _t("Основная область"),
    'instance.hero': _t("Обложка сборки"), 'instance.tabs': _t("Вкладки сборки"), 'instance.content.kind': _t("Типы контента"), 'discover.filters': _t("Фильтры"),
    'discover.kind': _t("Типы контента"), 'project.header': _t("Шапка проекта"), 'project.sidebar': _t("Панель проекта"), 'project.tabs': _t("Вкладки проекта"),
    'view.home': _t("Библиотека"), 'view.instance': _t("Сборка"), 'view.discover': _t("Каталог"), 'view.create': _t("Новая сборка"), 'view.accounts': _t("Аккаунты"), 'view.project': _t("Проект"), 'view.skins': _t("Скины"),
  };
  const FIXED_IDS = new Set(['home.all', 'home.recent', 'rail.instances']);
  const FIXED = (p) => FIXED_IDS.has(p.dataset.id) || p.hasAttribute('data-ue-fixed');
  const isCard = (id) => /^home\.card\./.test(id);
  const ghostOn = () => isOpen && localStorage.getItem('fl.ue.ghost') !== 'off';
  const keepVisible = (id) => ['app', 'main', 'rail', 'rail.settings', 'safe'].includes(id) || /^(settings|view\.settings|palette)/.test(id);
  const blank = () => ({ radius: {}, size: {}, layout: {}, els: {}, order: {} });
  const EL_KEYS = ['radius', 'scale', 'bg', 'opacity', 'bright', 'contrast', 'saturate', 'blur', 'border', 'border_color', 'shadow', 'w', 'h', 'x', 'y'];
  const HEX = /^#([0-9a-f]{6}|[0-9a-f]{8})$/i;
  const SHADOW = [null, '0 1px 2px rgba(0, 0, 0, .18)', '0 2px 6px -1px rgba(0, 0, 0, .24), 0 1px 2px rgba(0, 0, 0, .12)',
    '0 10px 24px -8px rgba(0, 0, 0, .34), 0 2px 4px rgba(0, 0, 0, .14)', '0 22px 48px -12px rgba(0, 0, 0, .48), 0 4px 10px rgba(0, 0, 0, .18)'];
  const num = (x, lo, hi) => (x == null || !Number.isFinite(+x) ? null : Math.min(hi, Math.max(lo, Math.round(+x))));
  // Per-element look. Only checked numbers and #hex reach the stylesheet.
  // Size and place set by dragging the selection's edges / grip. Real width/height, never
  // zoom: zoom on a glass panel re-rasterises its backdrop, which is what broke the background.
  const clampN = (v, lo, hi) => (Number.isFinite(v) ? Math.min(hi, Math.max(lo, Math.round(v))) : null);
  function geo(e) {
    const d = [], w = clampN(e.w, 24, 4000), hh = clampN(e.h, 24, 4000), x = clampN(e.x, -4000, 4000) || 0, y = clampN(e.y, -4000, 4000) || 0;
    if (w) d.push(`width: ${w}px !important`, 'max-width: 100% !important', 'min-width: 0 !important', 'flex: none !important', 'box-sizing: border-box !important');
    if (hh) d.push(`height: ${hh}px !important`, 'min-height: 0 !important', 'max-height: none !important', 'box-sizing: border-box !important');
    if (x || y) d.push(`translate: ${x}px ${y}px !important`, 'z-index: 3');
    return d;
  }
  function elDecl(e) {
    const d = [], f = [];
    if (HEX.test(e.bg || '')) d.push(`background: ${e.bg} !important`);
    const op = num(e.opacity, 10, 100); if (op != null && op !== 100) d.push(`opacity: ${op / 100} !important`);
    for (const [k, fn, lo, hi] of [['bright', 'brightness', 30, 200], ['contrast', 'contrast', 30, 200], ['saturate', 'saturate', 0, 300]]) {
      const x = num(e[k], lo, hi); if (x != null && x !== 100) f.push(`${fn}(${x / 100})`);
    }
    if (f.length) d.push(`filter: ${f.join(' ')} !important`);
    const bl = num(e.blur, 0, 40); if (bl) d.push(`backdrop-filter: blur(${bl}px) saturate(1.4) !important`, `-webkit-backdrop-filter: blur(${bl}px) saturate(1.4) !important`);
    const bw = num(e.border, 0, 6), bc = HEX.test(e.border_color || '') ? e.border_color : null;
    if (bw) d.push(`outline: ${bw}px solid ${bc || 'var(--line-strong)'} !important`, `outline-offset: -${bw}px !important`); else if (bc) d.push(`border-color: ${bc} !important`);
    const sh = num(e.shadow, 0, 4); if (sh) d.push(`box-shadow: ${SHADOW[sh]} !important`);
    return d;
  }
  const cap = (k) => `l${k[0].toUpperCase()}${k.slice(1)}`;
  const q = (id) => `[data-id="${id}"]`;
  let base = null, ui = blank(), isOpen = false, safe = false;
  const OK_ID = /^[a-z0-9._-]{1,64}$/;

  // ---------- Apply ----------
  let live = null;
  try { live = new CSSStyleSheet(); document.adoptedStyleSheets = [...document.adoptedStyleSheets, live]; } catch { live = null; }
  function writeRules(text) {
    if (live) return live.replaceSync(text);
    const sh = $('#ui-live')?.sheet; if (!sh) return;
    while (sh.cssRules.length) sh.deleteRule(0);
    text.split('\n').filter(Boolean).forEach((r) => sh.insertRule(r, sh.cssRules.length));
  }
  function setVar(name, v) { if (v == null || v === 100) root.style.removeProperty(name); else root.style.setProperty(name, String(v / 100)); }
  // A theme's own edits sit under the user's: percents multiply, the rest is replaced (as in the core).
  function layered(b, o) {
    const u = JSON.parse(JSON.stringify({ ...blank(), ...b }));
    for (const g of ['radius', 'size']) for (const [k, v] of Object.entries(o[g])) u[g][k] = Math.round(((u[g][k] ?? 100) * v) / 100);
    Object.assign(u.layout, o.layout); Object.assign(u.order, o.order);
    for (const [id, e] of Object.entries(o.els)) { const x = (u.els[id] ||= {}); for (const k of EL_KEYS) if (e[k] != null) x[k] = e[k]; if (e.hide != null) x.hide = !!e.hide; }
    return u;
  }
  function apply() {
    const U = base ? layered(base, ui) : ui;
    for (const [g] of RADIUS) setVar(`--rk-${g}`, U.radius[g]);
    for (const [g] of SIZE) setVar(`--sz-${g}`, U.size[g]);
    for (const [k, , def] of LAYOUT) { const v = U.layout[k]; if (v && v !== def) root.dataset[cap(k)] = v; else delete root.dataset[cap(k)]; }
    const out = [];
    for (const [id, e] of Object.entries(U.els)) {
      const d = [];
      if (e.radius != null) d.push(`border-radius: ${e.radius}px !important`);
      if (e.scale != null && e.scale !== 100 && !isCard(id)) d.push(`zoom: ${e.scale / 100}`);
      d.push(...geo(e), ...elDecl(e));
      if (e.hide && !keepVisible(id)) d.push(...(ghostOn() ? ['opacity: .36 !important', 'outline: 1.5px dashed var(--accent) !important', 'outline-offset: -1.5px !important'] : ['display: none !important']));
      if (d.length) out.push(`${q(id)} { ${d.join('; ')} }`);
      // Opacity or a filter on an element turns frosted children black in WebView2: they go plain inside it.
      if (d.some((x) => /^(opacity|filter):/.test(x))) out.push(`${q(id)} * { backdrop-filter: none !important; -webkit-backdrop-filter: none !important }`);
    }
    for (const [id, list] of Object.entries(U.order)) {
      if (FIXED_IDS.has(id)) continue;
      const p = document.querySelector(q(id));
      if (!p) continue;
      const n = p.children.length, seen = new Set(list.filter((ci) => ci < n));
      [...seen, ...[...Array(n).keys()].filter((ci) => !seen.has(ci))].forEach((ci, pos) => out.push(`${q(id)} > :nth-child(${ci + 1}) { order: ${pos} }`));
    }
    writeRules(out.join('\n'));
  }

  // ---------- Startup ----------
  const ready = (async () => {
    try { safe = !!(await call('app.info'))?.safe_mode; } catch { /* not in the app */ }
    if (safe) return;
    try { const st = await call('settings.get'); if (st?.ui) ui = { ...blank(), ...st.ui }; } catch { /* defaults */ }
    for (const id of Object.keys(ui.els)) if (!OK_ID.test(id)) delete ui.els[id];
    try { const cur = await call('themes.current'); if (cur?.ui) base = cur.ui; } catch { /* none */ }
    apply();
  })();

  function setTheme(u) { base = u || null; apply(); }

  // ---------- The panel: loaded on first use ----------
  let loadP = null, loaded = false;
  const load = () => (loadP ||= ready.then(() => window.FUI.lazy('editor.js')).then(() => { loaded = true; }, (e) => { loadP = null; throw e; }));
  async function open() {
    await ready;
    if (safe) return;
    try { await load(); } catch (e) { window.FUI.toast(e.message || String(e), { error: true }); return; }
    if (window.FEditor !== stub) window.FEditor.open();
  }
  document.addEventListener('keydown', (e) => {
    if (loaded || !(e.ctrlKey || e.metaKey) || !e.shiftKey || e.code !== 'KeyE') return;
    e.preventDefault(); open();
  }, true);
  document.addEventListener('pointerover', (e) => { if (!loadP && !safe && e.target.closest?.('#ui-edit-open')) load().catch(() => {}); }, { passive: true });

  const state = {
    get ui() { return ui; }, set ui(v) { ui = v; },
    get base() { return base; }, set base(v) { base = v; },
    get open() { return isOpen; }, set open(v) { isOpen = v; },
    get safe() { return safe; },
  };
  window.FEditorCore = { root, RADIUS, SIZE, LAYOUT, NAMES, FIXED_IDS, FIXED, isCard, keepVisible, blank, EL_KEYS, HEX, SHADOW, num, clampN, geo, elDecl, cap, q, layered, apply, state, ready };
  const stub = { open, close() {}, apply, setTheme, get state() { return ui; } };
  window.FEditor = stub;
})();
