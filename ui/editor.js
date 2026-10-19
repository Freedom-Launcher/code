'use strict';
// Interface editor: corners, sizes, placement and per-element tweaks, live, on every screen.
// Everything is data (settings.ui, checked by the core) turned into CSS variables and
// [data-id] rules through CSSOM, so the CSP stays strict and nothing a user saves can run.
(() => {
  const { $, toast } = window.FUI;
  const root = document.documentElement;
  const RADIUS = [['btn', 'Кнопки'], ['field', 'Поля'], ['card', 'Карточки'], ['icon', 'Иконки'], ['menu', 'Меню'], ['chip', 'Метки'], ['nav', 'Боковая панель'], ['dialog', 'Окна']];
  const SIZE = [['btn', 'Кнопки', 70, 150], ['field', 'Поля', 70, 150], ['text', 'Текст', 85, 130], ['gap', 'Отступы', 60, 160], ['rail', 'Боковая панель', 80, 150], ['top', 'Верхняя панель', 80, 180], ['card', 'Карточки сборок', 60, 200]];
  const LAYOUT = [['rail', 'Боковая панель', 'left'], ['filters', 'Фильтры каталога', 'left'], ['project', 'Панель проекта', 'right']];
  const NAMES = {
    rail: 'Боковая панель', 'rail.logo': 'Логотип', 'rail.instances': 'Сборки в панели', topbar: 'Верхняя панель', main: 'Основная область',
    'instance.hero': 'Обложка сборки', 'instance.tabs': 'Вкладки сборки', 'instance.content.kind': 'Типы контента', 'discover.filters': 'Фильтры',
    'discover.kind': 'Типы контента', 'project.header': 'Шапка проекта', 'project.sidebar': 'Панель проекта', 'project.tabs': 'Вкладки проекта',
    'view.home': 'Библиотека', 'view.instance': 'Сборка', 'view.discover': 'Каталог', 'view.create': 'Новая сборка', 'view.accounts': 'Аккаунты', 'view.project': 'Проект',
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
    if (w) d.push(`width: ${w}px !important`, 'max-width: none !important', 'min-width: 0 !important', 'flex: none !important', 'box-sizing: border-box !important');
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
  let base = null, ui = blank(), past = [], future = [], sel = null, hov = null, isOpen = false, saveT = 0, drag = false, safe = false;

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
    }
    for (const [id, list] of Object.entries(U.order)) {
      if (FIXED_IDS.has(id)) continue;
      const p = document.querySelector(q(id));
      if (!p || p.children.length !== list.length) continue;
      list.forEach((ci, pos) => out.push(`${q(id)} > :nth-child(${ci + 1}) { order: ${pos} }`));
    }
    writeRules(out.join('\n'));
  }

  // ---------- Changes, undo, save ----------
  const snap = () => JSON.stringify(ui);
  function push() { past.push(snap()); if (past.length > 100) past.shift(); future = []; }
  function save() {
    clearTimeout(saveT);
    saveT = setTimeout(() => call('settings.set', { ui }).catch((e) => toast(String(e), { error: true })), 300);
  }
  function commit(mut) { push(); mut(ui); apply(); sync(); save(); }
  function liveSet(mut) { if (!drag) { push(); drag = true; } mut(ui); apply(); save(); }
  function undo() { if (!past.length) return; future.push(snap()); ui = JSON.parse(past.pop()); apply(); sync(); save(); }
  function redo() { if (!future.length) return; past.push(snap()); ui = JSON.parse(future.pop()); apply(); sync(); save(); }
  const elOf = (u, id) => (u.els[id] ||= {});
  function tidy(u, id) { const e = u.els[id]; if (e && !e.hide && !(e.hide === false && base?.els?.[id]?.hide) && EL_KEYS.every((k) => e[k] == null || (k === 'scale' && e[k] === 100))) delete u.els[id]; }

  // ---------- Building blocks ----------
  function h(tag, cls, html) { const n = document.createElement(tag); if (cls) n.className = cls; if (html != null) n.innerHTML = html; return n; }
  const ICO = {
    undo: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M5.5 4 2.5 7l3 3"/><path d="M3 7h6.5a3.5 3.5 0 0 1 0 7H8"/></svg>',
    redo: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M10.5 4l3 3-3 3"/><path d="M13 7H6.5a3.5 3.5 0 0 0 0 7H8"/></svg>',
    x: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="m4.5 4.5 7 7m0-7-7 7"/></svg>',
    pick: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"><path d="M3 2.5 13 7l-4.3 1.2L6.8 12.6z"/></svg>',
    back: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M10 3.5 5.5 8l4.5 4.5"/></svg>',
    save: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"><path d="M8 1.8 9.6 6l4.4.3-3.4 2.8 1.1 4.3L8 11l-3.7 2.4 1.1-4.3L2 6.3 6.4 6z"/></svg>',
    reset: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M2.9 8.6A5.2 5.2 0 1 0 4.4 4.3L2.8 5.9"/><path d="M2.8 2.9v3h3"/></svg>',
    eye: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M1.6 8S4 3.5 8 3.5 14.4 8 14.4 8 12 12.5 8 12.5 1.6 8 1.6 8z"/><circle cx="8" cy="8" r="2"/></svg>',
    move: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M8 2v12M2 8h12M6 4l2-2 2 2M6 12l2 2 2-2M4 6 2 8l2 2M12 6l2 2-2 2"/></svg>',
    min: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M4 9.5 8 5.5l4 4"/></svg>',
    fwd: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M6 3.5 10.5 8 6 12.5"/></svg>',
  };
  const ib = (ico, tip) => { const b = h('button', 'btn btn-ghost btn-icon ue-ib', ICO[ico]); b.type = 'button'; b.dataset.tip = tip; b.setAttribute('aria-label', tip); return b; };
  let rows = [];
  const rstBtn = (fn) => { const b = ib('reset', 'Сбросить'); b.classList.add('ue-rst'); b.addEventListener('click', fn); return b; };
  // A slider row. Double-click on the label resets it.
  function slider({ label, min, max, get, set, changed, fmt = (x) => `${x}%`, reset }) {
    const r = h('div', 'ue-row'), l = h('span', 'l'), v = h('span', 'v num'), i = h('input', 'ue-sl');
    l.textContent = label; i.type = 'range'; i.min = min; i.max = max; i.step = 1; i.setAttribute('aria-label', label);
    const w = h('span', 'ue-vw'); if (reset) w.append(rstBtn(() => commit(reset))); w.append(v);
    r.append(l, w, i);
    r._sync = () => {
      const x = get(); if (!drag || document.activeElement !== i) i.value = x;
      v.textContent = fmt(x); i.style.setProperty('--p', `${((x - min) / (max - min)) * 100}%`);
      r.toggleAttribute('data-changed', changed());
      const rb = w.firstElementChild; if (rb !== v) rb.disabled = !r.hasAttribute('data-changed');
    };
    i.addEventListener('input', () => { liveSet((u) => set(u, +i.value)); r._sync(); });
    i.addEventListener('change', () => { drag = false; sync(); });
    l.addEventListener('dblclick', () => commit(reset));
    rows.push(r); return r;
  }
  function sides(label, key, def) {
    const r = h('div', 'ue-row ue-row-seg'), l = h('span', 'l'), s = h('div', 'seg');
    l.textContent = label;
    s.innerHTML = '<button type="button" data-v="left">Слева</button><button type="button" data-v="right">Справа</button>';
    r.append(l, s);
    s.addEventListener('seg:change', (e) => commit((u) => { const v = e.detail.dataset.v; if (v === def) delete u.layout[key]; else u.layout[key] = v; }));
    r._sync = () => {
      const v = ui.layout[key] || def;
      s.querySelectorAll('button').forEach((b) => b.setAttribute('aria-selected', String(b.dataset.v === v)));
      s._place?.(false); r.toggleAttribute('data-changed', v !== def);
    };
    rows.push(r); return r;
  }
  // A colour row: swatch opens the shared colour picker (colors.js).
  function colorRow(id, node, label, key, defVar, prop) {
    const r = h('div', 'ue-row ue-row-seg'), l = h('span', 'l'), w = h('span', 'ue-vw'), sw = h('button', 'ue-sw');
    l.textContent = label; sw.type = 'button'; sw.setAttribute('aria-label', label); sw.setAttribute('aria-haspopup', 'dialog'); sw.setAttribute('aria-expanded', 'false');
    const own = () => ui.els[id]?.[key] ?? null;
    const def = () => getComputedStyle(root).getPropertyValue(defVar).trim();
    const now = () => { const v = getComputedStyle(node)[prop]; return v && !/^(transparent|rgba\(\s*0,\s*0,\s*0,\s*0\s*\))$/.test(v) ? v : def(); };
    const rst = rstBtn(() => { window.FColors?.close?.(); commit((u) => { if (u.els[id]) { u.els[id][key] = null; tidy(u, id); } }); });
    w.append(rst, sw); r.append(l, w);
    const put = (hex) => { liveSet((u) => { elOf(u, id)[key] = hex ? hex.toLowerCase() : null; tidy(u, id); }); r._sync(); };
    sw.addEventListener('click', () => {
      const P = window.FColors; if (!P?.pick) return;
      if (sw.getAttribute('aria-expanded') === 'true') { P.close(); return; }
      P.pick(sw, { title: label, own: own(), value: now(), def: def(), alpha: true, onChange: put, onClose: () => { drag = false; sync(); } });
    });
    r._sync = () => { const o = own(); sw.style.setProperty('--c', o || now()); r.toggleAttribute('data-changed', o != null); rst.disabled = o == null; };
    rows.push(r); return r;
  }
  const sec = (title, ...kids) => { const s = h('div', 'ue-sec'); s.append(h('h4', '', title), ...kids); return s; };

  // ---------- Panel ----------
  const panel = h('div', 'ue'); panel.dataset.id = 'editor'; panel.dataset.side = 'right'; panel.setAttribute('role', 'dialog'); panel.setAttribute('aria-label', 'Интерфейс');
  const head = h('div', 'ue-head'); const title = h('b', '', 'Интерфейс');
  const bUndo = ib('undo', 'Отменить · Ctrl Z'), bRedo = ib('redo', 'Повторить · Ctrl Shift Z');
  const bDone = h('button', 'btn btn-primary', '<span class="lbl">Готово</span>'); bDone.type = 'button';
  const bMin = ib('min', 'Свернуть'); bMin.classList.add('ue-min');
  head.append(title, bUndo, bRedo, bMin, bDone);
  const tabs = h('div', 'seg ue-tabs', '<button type="button" data-tab="gen" aria-selected="true">Общее</button><button type="button" data-tab="el">Элемент</button>');
  const body = h('div', 'ue-body'), pGen = h('div', 'ue-pane'), pEl = h('div', 'ue-pane'); pEl.hidden = true;
  body.append(pGen, pEl);
  const foot = h('div', 'ue-foot'); const bReset = h('button', 'btn btn-ghost', '<span class="lbl">Сбросить всё</span>'); bReset.type = 'button';
  const bSave = h('button', 'btn btn-ghost ue-save', `${ICO.save}<span class="lbl">Сохранить как тему</span>`); bSave.type = 'button';
  const fa = h('div', 'ue-foot-a'), fb = h('div', 'ue-foot-b'); fb.hidden = true;
  const nameIn = h('input', 'field ue-name'); nameIn.maxLength = 60; nameIn.placeholder = 'Название темы'; nameIn.setAttribute('aria-label', 'Название темы'); nameIn.spellcheck = false;
  const bOk = h('button', 'btn btn-primary', '<span class="lbl">Сохранить</span>'); bOk.type = 'button';
  const bCol = h('button', 'btn btn-ghost ue-col', '<span class="ue-col-sw"></span><span class="lbl">Цвета</span>'); bCol.type = 'button';
  bCol.setAttribute('aria-pressed', 'true'); bCol.dataset.tip = 'Сохранить свои цвета в теме';
  bCol.addEventListener('click', () => bCol.setAttribute('aria-pressed', String(bCol.getAttribute('aria-pressed') !== 'true')));
  fa.append(bSave, bReset); fb.append(nameIn, bCol, bOk); foot.append(fa, fb);
  panel.append(head, tabs, body, foot);
  const hidSec = h('div', 'ue-sec ue-hid'); const hidList = h('div', 'ue-picks');
  const hidHead = h('div', 'ue-hid-h'), hidTitle = h('h4'), hidSw = h('label', 'ue-hid-sw', '<span>На месте</span><span class="switch"><input type="checkbox"><i></i></span>');
  const hidGhost = hidSw.querySelector('input'); hidSw.dataset.tip = 'Показывать скрытые полупрозрачными, пока открыт редактор';
  hidGhost.addEventListener('change', () => { localStorage.setItem('fl.ue.ghost', hidGhost.checked ? 'on' : 'off'); apply(); });
  hidHead.append(hidTitle, hidSw); hidSec.append(hidHead, hidList);
  pGen.append(
    hidSec,
    sec('Углы', ...RADIUS.map(([g, label]) => slider({
      label, min: 0, max: 300, get: () => ui.radius[g] ?? 100, changed: () => (ui.radius[g] ?? 100) !== 100,
      set: (u, x) => { if (x === 100) delete u.radius[g]; else u.radius[g] = x; }, reset: (u) => delete u.radius[g],
    }))),
    sec('Размеры', ...SIZE.map(([g, label, lo, hi]) => slider({
      label, min: lo, max: hi, get: () => ui.size[g] ?? 100, changed: () => (ui.size[g] ?? 100) !== 100,
      set: (u, x) => { if (x === 100) delete u.size[g]; else u.size[g] = x; }, reset: (u) => delete u.size[g],
    }))),
    sec('Расположение', ...LAYOUT.map(([k, label, def]) => sides(label, k, def))),
  );
  document.body.append(panel);
  window.FUI.initSegs(panel);
  const fit = (x, y) => [Math.min(Math.max(8, x), innerWidth - panel.offsetWidth - 8), Math.min(Math.max(8, y), innerHeight - 44)];
  function float(x, y) {
    [x, y] = fit(x, y);
    panel.dataset.free = ''; Object.assign(panel.style, { left: `${x}px`, top: `${y}px`, right: 'auto', maxHeight: `${innerHeight - y - 12}px`, transformOrigin: 'top left' });
    return [x, y];
  }
  function dock() { delete panel.dataset.free; panel.style.cssText = ''; localStorage.removeItem('fl.ue.pos'); }
  function setMin(on) { panel.toggleAttribute('data-min', on); bMin.dataset.tip = on ? 'Развернуть' : 'Свернуть'; localStorage.setItem('fl.ue.min', on ? '1' : ''); }
  bMin.addEventListener('click', () => setMin(!panel.hasAttribute('data-min')));
  let mv = null;
  head.addEventListener('pointerdown', (e) => {
    if (e.button !== 0 || e.target.closest('button, input')) return;
    const r = panel.getBoundingClientRect(); mv = { dx: e.clientX - r.left, dy: e.clientY - r.top };
    head.setPointerCapture(e.pointerId); panel.dataset.moving = '';
  });
  head.addEventListener('pointermove', (e) => { if (mv) mv.at = float(e.clientX - mv.dx, e.clientY - mv.dy); });
  const endMv = () => { if (!mv) return; if (mv.at) localStorage.setItem('fl.ue.pos', JSON.stringify(mv.at)); mv = null; delete panel.dataset.moving; };
  head.addEventListener('pointerup', endMv); head.addEventListener('lostpointercapture', endMv);
  head.addEventListener('dblclick', (e) => { if (!e.target.closest('button')) dock(); });
  try { const at = JSON.parse(localStorage.getItem('fl.ue.pos') || 'null'); if (Array.isArray(at)) requestAnimationFrame(() => float(...at)); } catch { /* none */ }
  if (localStorage.getItem('fl.ue.min')) setMin(true);
  addEventListener('resize', () => { if (panel.dataset.free != null) float(parseFloat(panel.style.left), parseFloat(panel.style.top)); });
  const hl = h('div', 'ue-hl', '<span class="ue-tag"></span>'), sb = h('div', 'ue-sel', '<span class="ue-tag"></span>');
  document.body.append(hl, sb);

  function tab(t, anim = true) {
    tabs.querySelectorAll('button').forEach((b) => b.setAttribute('aria-selected', String(b.dataset.tab === t)));
    tabs._place?.(false);
    const show = t === 'el' ? pEl : pGen; const was = show.hidden;
    pGen.hidden = t !== 'gen'; pEl.hidden = t !== 'el';
    if (anim && was) { show.classList.remove('ue-in'); void show.offsetWidth; show.classList.add('ue-in'); }
    if (t === 'gen') requestAnimationFrame(() => pGen.querySelectorAll('.seg').forEach((s) => s._place?.(true)));
  }
  tabs.addEventListener('seg:change', (e) => tab(e.detail.dataset.tab));

  function sync() {
    rows = rows.filter((r) => r.isConnected);
    rows.forEach((r) => r._sync());
    bUndo.disabled = !past.length; bRedo.disabled = !future.length;
    bReset.disabled = snap() === JSON.stringify(blank());
    const U = base ? layered(base, ui) : ui;
    const hidden = Object.entries(U.els).filter(([id, e]) => e.hide && !keepVisible(id)).map(([id]) => id);
    hidSec.hidden = !hidden.length;
    hidTitle.textContent = `Скрытые · ${hidden.length}`;
    hidGhost.checked = localStorage.getItem('fl.ue.ghost') !== 'off';
    hidList.replaceChildren(...hidden.map((id) => {
      const c = h('button', 'ue-chip ue-chip-hid', ICO.eye); c.type = 'button'; c.append(name(id)); c.dataset.tip = 'Показать снова';
      c.addEventListener('click', () => commit((u) => { elOf(u, id).hide = false; tidy(u, id); }));
      c.addEventListener('pointerenter', () => { const n = document.querySelector(q(id)); if (n && n.getClientRects().length) hov = n; });
      c.addEventListener('pointerleave', () => { hov = null; });
      return c;
    }));
  }
  bUndo.addEventListener('click', undo); bRedo.addEventListener('click', redo);
  bDone.addEventListener('click', () => close());
  bReset.addEventListener('click', () => commit((u) => Object.assign(u, blank())));
  function saveMode(on) {
    const show = on ? fb : fa; fa.hidden = on; fb.hidden = !on;
    show.classList.remove('ue-in'); void show.offsetWidth; show.classList.add('ue-in');
    if (!on) return;
    const own = window.FColors?.bake?.() || {};
    bCol.hidden = !Object.keys(own).length; bCol.setAttribute('aria-pressed', 'true');
    const sw = bCol.querySelector('.ue-col-sw'), cs = getComputedStyle(document.documentElement);
    for (const [i, k] of ['--accent', '--surface-2', '--text'].entries()) sw.style.setProperty(`--c${i}`, own[k] || cs.getPropertyValue(k).trim());
    const t = typeof S !== 'undefined' && S.theme && !S.theme.builtin ? S.theme.name : '';
    nameIn.value = t ? `${t} 2` : 'Моя тема'; nameIn.focus(); nameIn.select();
  }
  async function saveTheme() {
    const name = nameIn.value.trim(); if (!name) { nameIn.focus(); return; }
    bOk.disabled = true;
    try {
      clearTimeout(saveT); await call('settings.set', { ui });
      const tokens = !bCol.hidden && bCol.getAttribute('aria-pressed') === 'true' ? (window.FColors?.bake?.() || {}) : {};
      const cur = await call('themes.save', { name, tokens });
      if (Object.keys(tokens).length) document.addEventListener('theme:applied', () => window.FColors?.baked?.(), { once: true });
      ui = blank(); past = []; future = []; base = cur.ui || null;
      apply(); sync(); saveMode(false);
      document.dispatchEvent(new CustomEvent('fl:theme', { detail: cur }));
      toast(`Тема «${cur.name}» сохранена`);
    } catch (e) { toast(String(e), { error: true }); } finally { bOk.disabled = false; }
  }
  bSave.addEventListener('click', () => saveMode(true));
  bOk.addEventListener('click', saveTheme);
  nameIn.addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); saveTheme(); } else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); saveMode(false); } });

  // ---------- Element pane ----------
  function name(id, n = document.querySelector(q(id))) {
    if (NAMES[id]) return NAMES[id];
    const t = n && (n.dataset.tip || n.getAttribute('aria-label') || n.querySelector?.('h1, h2, h3, .t, .lbl')?.textContent || n.textContent);
    const s = (t || '').replace(/\s+/g, ' ').trim();
    return s ? (s.length > 28 ? `${s.slice(0, 27)}…` : s) : id;
  }
  function renderEl() {
    pEl.replaceChildren();
    if (!sel) {
      const e = h('div', 'ue-empty');
      e.append(h('span', 'ue-empty-ico', ICO.pick), h('span', 't', 'Кликните по элементу'), h('span', 'muted', 'Перетащите, чтобы переставить'), h('span', 'muted', '<kbd>Alt</kbd> + клик — обычное нажатие'));
      const picks = h('div', 'ue-picks');
      for (const id of ['rail', 'topbar', 'main', 'instance.hero', 'discover.filters']) {
        const n = document.querySelector(q(id)); if (!n || !n.getClientRects().length) continue;
        const c = h('button', 'ue-chip'); c.type = 'button'; c.textContent = name(id, n); c.addEventListener('click', () => select(n));
        picks.append(c);
      }
      if (picks.children.length) e.append(picks);
      pEl.append(e); return;
    }
    const { id, node } = sel;
    const hd = h('div', 'ue-elh'), g = h('div', 'grow'), bx = ib('x', 'Снять выбор · Esc');
    const b = h('b'); b.textContent = name(id, node); const m = h('span', 'mono faint'); m.textContent = id;
    g.append(b, m); hd.append(g, bx); bx.addEventListener('click', () => select(null));
    const s = h('div', 'ue-sec');
    s.append(slider({
      label: 'Скругление', min: 0, max: 64, fmt: (x) => `${x} px`,
      get: () => ui.els[id]?.radius ?? Math.min(64, sel.base), changed: () => ui.els[id]?.radius != null,
      set: (u, x) => { elOf(u, id).radius = x; }, reset: (u) => { if (u.els[id]) { u.els[id].radius = null; tidy(u, id); } },
    }), isCard(id) ? slider({
      label: 'Размер карточек', min: 60, max: 200, get: () => ui.size.card ?? 100, changed: () => (ui.size.card ?? 100) !== 100,
      set: (u, x) => { if (x === 100) delete u.size.card; else u.size.card = x; }, reset: (u) => delete u.size.card,
    }) : slider({
      label: 'Размер', min: 50, max: 200, get: () => ui.els[id]?.scale ?? 100, changed: () => (ui.els[id]?.scale ?? 100) !== 100,
      set: (u, x) => { elOf(u, id).scale = x === 100 ? null : x; tidy(u, id); }, reset: (u) => { if (u.els[id]) { u.els[id].scale = null; tidy(u, id); } },
    }));
    if (!keepVisible(id)) {
      const r = h('label', 'ue-row ue-row-seg'), l = h('span', 'l', 'Скрыть'), sw = h('span', 'switch', '<input type="checkbox"><i></i>');
      const inp = sw.querySelector('input'); r.append(l, sw);
      inp.addEventListener('change', () => commit((u) => { elOf(u, id).hide = inp.checked; tidy(u, id); if (inp.checked && !ghostOn()) select(null); }));
      r._sync = () => { inp.checked = !!ui.els[id]?.hide; r.toggleAttribute('data-changed', inp.checked); };
      rows.push(r); s.append(r);
    }
    const p = node.parentElement;
    if (p?.dataset.id && !FIXED(p) && p.children.length <= 64 && /flex|grid/.test(getComputedStyle(p).display)) {
      const col = /column/.test(getComputedStyle(p).flexDirection);
      const r = h('div', 'ue-row ue-row-seg'), l = h('span', 'l', 'Порядок'), pair = h('div', 'ue-pair');
      const bA = ib('back', col ? 'Выше' : 'Левее'), bB = ib('fwd', col ? 'Ниже' : 'Правее');
      if (col) { bA.style.transform = 'rotate(90deg)'; bB.style.transform = 'rotate(90deg)'; }
      pair.append(bA, bB); r.append(l, pair);
      const ci = [...p.children].indexOf(node), pid = p.dataset.id, n = p.children.length;
      const list = () => (ui.order[pid]?.length === n ? [...ui.order[pid]] : [...Array(n).keys()]);
      const move = (d) => commit((u) => {
        const L = list(), i = L.indexOf(ci), j = i + d; if (j < 0 || j >= n) return;
        [L[i], L[j]] = [L[j], L[i]];
        if (L.every((x, k) => x === k)) delete u.order[pid]; else u.order[pid] = L;
      });
      bA.addEventListener('click', () => move(-1)); bB.addEventListener('click', () => move(1));
      r._sync = () => { const i = list().indexOf(ci); bA.disabled = i <= 0; bB.disabled = i >= n - 1; r.toggleAttribute('data-changed', !!ui.order[pid]); };
      rows.push(r); s.append(r);
    }
    const nm = (label, key, min, max, neutral, fmt) => slider({
      label, min, max, fmt, get: () => ui.els[id]?.[key] ?? neutral, changed: () => ui.els[id]?.[key] != null,
      set: (u, x) => { elOf(u, id)[key] = x === neutral ? null : x; tidy(u, id); }, reset: (u) => { if (u.els[id]) { u.els[id][key] = null; tidy(u, id); } },
    });
    const px = (x) => `${x} px`;
    const gr = h('div', 'ue-row ue-row-seg ue-geo'), gv = h('span', 'v'), gp = h('div', 'ue-pair');
    const gRst = rstBtn(() => commit((u) => { const e = u.els[id]; if (e) { e.w = e.h = e.x = e.y = null; tidy(u, id); } }));
    gp.append(gv, gRst); gr.append(h('span', 'l', 'Размер и место'), gp);
    gr._sync = () => {
      const e = ui.els[id] || {}, on = [e.w, e.h, e.x, e.y].some((v) => v != null);
      gv.textContent = on ? `${e.w ?? 'авто'} × ${e.h ?? 'авто'}${e.x || e.y ? ` · ${e.x || 0}, ${e.y || 0}` : ''}` : 'авто';
      gRst.hidden = !on; gr.toggleAttribute('data-changed', on);
    };
    rows.push(gr); s.append(gr);
    const extra = [
      sec('Фон', colorRow(id, node, 'Цвет', 'bg', '--surface-2', 'backgroundColor'), nm('Непрозрачность', 'opacity', 10, 100, 100), nm('Размытие', 'blur', 0, 40, 0, px)),
      sec('Эффекты', nm('Яркость', 'bright', 30, 200, 100), nm('Контраст', 'contrast', 30, 200, 100), nm('Насыщенность', 'saturate', 0, 300, 100)),
      sec('Рамка', nm('Толщина', 'border', 0, 6, 0, px), colorRow(id, node, 'Цвет', 'border_color', '--line-strong', 'borderTopColor')),
      sec('Тень', nm('Глубина', 'shadow', 0, 4, 0, (x) => ['Нет', 'Слабая', 'Средняя', 'Сильная', 'Глубокая'][x] ?? x)),
    ];
    const f = h('div', 'ue-elf'), br = h('button', 'btn btn-ghost', '<span class="lbl">Сбросить элемент</span>'); br.type = 'button';
    br.addEventListener('click', () => commit((u) => delete u.els[id]));
    f.append(br);
    pEl.append(hd, s, ...extra, f);
    sync();
  }
  function select(n) {
    window.FColors?.close?.();
    if (!n) { sel = null; renderEl(); return; }
    const id = n.dataset.id;
    const had = ui.els[id]?.radius != null;
    sel = { id, node: n, base: had ? ui.els[id].radius : Math.round(parseFloat(getComputedStyle(n).borderTopLeftRadius) || 0) };
    const r = n.getBoundingClientRect();
    if (panel.dataset.free != null) { renderEl(); tab('el'); return; }
    const side = panel.dataset.side, covered = side === 'right' ? r.right > innerWidth - 330 : r.left < 330;
    const fits = side === 'right' ? r.left > 330 : r.right < innerWidth - 330;
    if (covered && fits) panel.dataset.side = side === 'right' ? 'left' : 'right';
    renderEl(); tab('el');
  }

  // ---------- Picking ----------
  // Only ids the core keeps: a file name or world name with spaces picks its row's container.
  const OK_ID = /^[a-z0-9._-]{1,64}$/;
  function pickable(t) {
    let n = t?.closest?.('[data-id]');
    while (n && !OK_ID.test(n.dataset.id)) n = n.parentElement?.closest('[data-id]');
    if (!n || n.id === 'app' || n.closest('.ue') || !n.closest('#app')) return null;
    return n;
  }
  document.addEventListener('pointermove', (e) => { if (isOpen) hov = e.altKey ? null : pickable(e.target); }, true);
  for (const ev of ['pointerdown', 'mousedown', 'click', 'dblclick', 'contextmenu']) {
    document.addEventListener(ev, (e) => {
      if (!isOpen || e.altKey || e.target.closest?.('.ue, .palette')) return;
      const n = pickable(e.target); if (!n) return;
      e.preventDefault(); e.stopPropagation();
      if (ev === 'pointerdown' && ctx.hasAttribute('data-open')) window.FUI.closeMenu?.();
      if (ev === 'click' && !noClick) select(n);
      if (ev === 'contextmenu') { select(n); openCtx(n, e.clientX, e.clientY); }
    }, true);
  }
  const ctx = h('div', 'pop ue-ctx'); ctx.dataset.id = 'editor.ctx'; ctx.setAttribute('role', 'menu');
  document.body.append(ctx);
  function openCtx(n, x, y) {
    const id = n.dataset.id, e = ui.els[id] || {};
    const item = (label, fn, cls = '') => { const b = h('button', `menu-item ${cls}`, label); b.type = 'button'; b.addEventListener('click', () => { window.FUI.closeMenu?.(); fn(); }); return b; };
    const kids = [h('div', 'menu-label', name(id, n)),
      item('Настроить…', () => { setMin(false); if (!panel.hasAttribute('data-open')) open(); tab('el'); })];
    if ([e.w, e.h, e.x, e.y].some((v) => v != null)) kids.push(item('Вернуть размер и место', () => commit((u) => { const el = u.els[id]; if (el) { el.w = el.h = el.x = el.y = null; tidy(u, id); } })));
    if (!keepVisible(id)) kids.push(item('Скрыть', () => commit((u) => { elOf(u, id).hide = true; })));
    if (ui.els[id]) kids.push(h('div', 'menu-sep'), item('Сбросить элемент', () => commit((u) => delete u.els[id]), 'is-danger'));
    ctx.replaceChildren(...kids);
    window.FUI.openMenuAt(ctx, x, y);
  }
  let dg = null, noClick = false;
  const motion = () => root.dataset.motion !== 'off' && !matchMedia('(prefers-reduced-motion: reduce)').matches;
  const EASE = 'cubic-bezier(.22, 1, .36, 1)';
  function orderable(n) {
    const p = n?.parentElement;
    if (!p?.dataset.id || FIXED(p) || p.children.length < 2 || p.children.length > 64 || !p.closest('#app')) return null;
    return /flex|grid/.test(getComputedStyle(p).display) ? p : null;
  }
  const orderOf = (pid, n) => (ui.order[pid]?.length === n ? [...ui.order[pid]] : [...Array(n).keys()]);
  function layoutRect(n) { const t = n.style.transform; n.style.transform = ''; const r = n.getBoundingClientRect(); n.style.transform = t; return r; }
  function follow() { dg.n.style.transform = `translate(${dg.x - dg.gx - dg.lay.left}px, ${dg.y - dg.gy - dg.lay.top}px) scale(1.02)`; }
  function startDrag() {
    dg.on = true; dg.snap = snap(); drag = true; push();
    if (sel?.node !== dg.n) select(dg.n);
    const r = dg.n.getBoundingClientRect(); dg.gx = dg.x0 - r.left; dg.gy = dg.y0 - r.top;
    const cs = getComputedStyle(dg.p); dg.col = cs.display.includes('flex') && /column/.test(cs.flexDirection);
    dg.n.setAttribute('data-ue-drag', ''); root.dataset.ueDragging = '';
    dg.lay = layoutRect(dg.n);
    dg.rects = new Map([...dg.p.children].filter((k) => k !== dg.n).map((k) => [k, k.getBoundingClientRect()]));
  }
  function reorder() {
    const kids = [...dg.p.children], L = orderOf(dg.pid, kids.length);
    let best = null, bd = Infinity;
    for (const [k, r] of dg.rects) {
      if (!r.width || !r.height) continue;
      const cx = r.left + r.width / 2, cy = r.top + r.height / 2, d = Math.hypot(dg.x - cx, dg.y - cy);
      if (d < bd) { bd = d; best = { k, r, cx, cy }; }
    }
    if (!best) return;
    const across = !dg.col && Math.abs(dg.y - best.cy) < best.r.height / 2;
    const after = across ? dg.x > best.cx : dg.y > best.cy;
    const M = L.filter((x) => x !== dg.ci);
    M.splice(M.indexOf(kids.indexOf(best.k)) + (after ? 1 : 0), 0, dg.ci);
    if (M.every((x, i) => x === L[i])) return;
    const others = kids.filter((k) => k !== dg.n), before = new Map(others.map((k) => [k, k.getBoundingClientRect()]));
    others.forEach((k) => k._ueA?.cancel());
    if (M.every((x, i) => x === i)) delete ui.order[dg.pid]; else ui.order[dg.pid] = M;
    apply();
    dg.rects = new Map(others.map((k) => [k, k.getBoundingClientRect()]));
    dg.lay = layoutRect(dg.n); follow();
    if (!motion()) return;
    for (const k of others) {
      const a = before.get(k), b = dg.rects.get(k), dx = a.left - b.left, dy = a.top - b.top;
      if (Math.abs(dx) + Math.abs(dy) > 0.5) k._ueA = k.animate([{ transform: `translate(${dx}px, ${dy}px)` }, { transform: 'none' }], { duration: 280, easing: EASE });
    }
  }
  function endDrag() {
    const d = dg; dg = null; if (!d?.on) return;
    noClick = true; setTimeout(() => { noClick = false; }, 0);
    const r = d.n.getBoundingClientRect(); d.n.style.transform = ''; const l = d.n.getBoundingClientRect();
    d.n.removeAttribute('data-ue-drag'); delete root.dataset.ueDragging; drag = false;
    if (motion()) d.n.animate([{ transform: `translate(${r.left + r.width / 2 - l.left - l.width / 2}px, ${r.top + r.height / 2 - l.top - l.height / 2}px) scale(1.02)` }, { transform: 'none' }], { duration: 260, easing: EASE });
    if (snap() === d.snap) past.pop(); else save();
    sync();
  }
  document.addEventListener('pointerdown', (e) => {
    if (!isOpen || e.altKey || e.button !== 0 || e.target.closest?.('.ue, .palette')) return;
    const n = sel?.node?.contains(e.target) && orderable(sel.node) ? sel.node : pickable(e.target);
    const p = n && orderable(n); if (!p) return;
    dg = { n, p, pid: p.dataset.id, ci: [...p.children].indexOf(n), x0: e.clientX, y0: e.clientY, on: false };
  }, true);
  document.addEventListener('pointermove', (e) => {
    if (!dg) return;
    if (!dg.on) { if (Math.hypot(e.clientX - dg.x0, e.clientY - dg.y0) < 5) return; startDrag(); }
    dg.x = e.clientX; dg.y = e.clientY; follow(); reorder();
  }, true);
  addEventListener('pointerup', endDrag, true); addEventListener('pointercancel', endDrag, true);
  addEventListener('blur', endDrag);

  // ---------- Resize by edges, move by the grip ----------
  for (const d of ['n', 's', 'e', 'w', 'ne', 'nw', 'se', 'sw']) { const k = h('i', `ue-rz ue-rz-${d}`); k.dataset.rz = d; sb.append(k); }
  const grip = h('button', 'ue-grip', ICO.move); grip.type = 'button'; grip.dataset.rz = 'move'; grip.dataset.tip = 'Перетащить · двойной щелчок вернёт на место'; grip.setAttribute('aria-label', 'Переместить');
  sb.append(grip);
  let rz = null;
  sb.addEventListener('pointerdown', (e) => {
    const t = e.target.closest('[data-rz]'); if (!t || !sel || e.button !== 0) return;
    e.preventDefault(); e.stopPropagation();
    const cur = ui.els[sel.id] || {}, r = sel.node.getBoundingClientRect();
    const z = cur.scale && cur.scale !== 100 && !isCard(sel.id) ? cur.scale / 100 : 1;
    rz = { d: t.dataset.rz, id: sel.id, x0: e.clientX, y0: e.clientY, z, w: r.width / z, h: r.height / z, x: cur.x || 0, y: cur.y || 0, snap: snap() };
    push(); t.setPointerCapture(e.pointerId); root.dataset.ueResizing = rz.d;
  });
  sb.addEventListener('pointermove', (e) => {
    if (!rz) return;
    const dx = e.clientX - rz.x0, dy = e.clientY - rz.y0, el = elOf(ui, rz.id), d = rz.d, R = Math.round;
    if (d === 'move') { el.x = R(rz.x + dx) || null; el.y = R(rz.y + dy) || null; }
    else {
      // Dragging the left / top edge keeps the opposite edge where it is.
      if (d.includes('e')) el.w = Math.max(24, R(rz.w + dx / rz.z));
      if (d.includes('w')) { el.w = Math.max(24, R(rz.w - dx / rz.z)); el.x = R(rz.x + (rz.w - el.w) * rz.z) || null; }
      if (d.includes('s')) el.h = Math.max(24, R(rz.h + dy / rz.z));
      if (d.includes('n')) { el.h = Math.max(24, R(rz.h - dy / rz.z)); el.y = R(rz.y + (rz.h - el.h) * rz.z) || null; }
    }
    apply();
  });
  function endRz() {
    if (!rz) return;
    const r = rz; rz = null; delete root.dataset.ueResizing;
    tidy(ui, r.id);
    if (snap() === r.snap) past.pop(); else save();
    sync();
  }
  sb.addEventListener('pointerup', endRz); sb.addEventListener('pointercancel', endRz); sb.addEventListener('lostpointercapture', endRz);
  sb.addEventListener('dblclick', (e) => {
    const t = e.target.closest('[data-rz]'); if (!t || !sel) return;
    e.preventDefault(); e.stopPropagation();
    const id = sel.id, mv = t.dataset.rz === 'move';
    commit((u) => { const el = u.els[id]; if (!el) return; if (mv) el.x = el.y = null; else { if (/[ew]/.test(t.dataset.rz)) el.w = el.x = null; if (/[ns]/.test(t.dataset.rz)) el.h = el.y = null; } tidy(u, id); });
  });

  function place(box, n) {
    const r = n?.isConnected ? n.getBoundingClientRect() : null;
    if (!r || !r.width || !r.height) { box.removeAttribute('data-on'); return; }
    box.style.transform = `translate(${r.left}px, ${r.top}px)`;
    box.style.width = `${r.width}px`; box.style.height = `${r.height}px`;
    box.style.borderRadius = getComputedStyle(n).borderTopLeftRadius;
    box.toggleAttribute('data-below', r.top < 70);
    const tag = box.firstChild, t = name(n.dataset.id, n); if (tag.textContent !== t) tag.textContent = t;
    box.setAttribute('data-on', '');
  }
  function loop() {
    if (!isOpen) return;
    if (sel && !sel.node.isConnected) { const n = document.querySelector(q(sel.id)); if (n) sel.node = n; }
    place(sb, sel?.node);
    place(hl, hov && !dg?.on && hov !== sel?.node ? hov : null);
    requestAnimationFrame(loop);
  }

  // ---------- Open / close ----------
  function open() {
    if (safe) return;
    if (typeof S !== 'undefined' && S.view === 'settings') show(S.instances.length ? 'home' : 'empty');
    window.FUI.closeMenu?.();
    isOpen = true; root.dataset.uiEdit = ''; apply();
    panel.setAttribute('data-open', '');
    renderEl(); sync(); tab(sel ? 'el' : 'gen', false);
    requestAnimationFrame(loop);
  }
  function close() {
    endDrag(); if (!fb.hidden) saveMode(false);
    isOpen = false; hov = null; delete root.dataset.uiEdit; apply();
    panel.removeAttribute('data-open'); hl.removeAttribute('data-on'); sb.removeAttribute('data-on');
  }
  document.addEventListener('keydown', (e) => {
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.shiftKey && e.code === 'KeyE') { e.preventDefault(); if (isOpen) close(); else open(); return; }
    if (!isOpen || e.target.matches?.('input:not([type=range]):not([type=checkbox]), textarea')) return;
    if (mod && e.code === 'KeyZ') { e.preventDefault(); if (e.shiftKey) redo(); else undo(); }
    else if (mod && e.code === 'KeyY') { e.preventDefault(); redo(); }
    else if (e.key === 'Escape' && $('#palette')?.getAttribute('aria-hidden') !== 'false') { e.preventDefault(); if (sel) select(null); else close(); }
  }, true);
  addEventListener('resize', () => { if (isOpen) tabs._place?.(true); });

  (async () => {
    try { safe = !!(await call('app.info'))?.safe_mode; } catch { /* not in the app */ }
    if (safe) return;
    try { const st = await call('settings.get'); if (st?.ui) ui = { ...blank(), ...st.ui }; } catch { /* defaults */ }
    for (const id of Object.keys(ui.els)) if (!OK_ID.test(id)) delete ui.els[id];
    try { const cur = await call('themes.current'); if (cur?.ui) base = cur.ui; } catch { /* none */ }
    apply(); sync();
  })();

  function setTheme(u) { base = u || null; apply(); }
  window.FEditor = { open, close, apply, setTheme, get state() { return ui; } };
})();