'use strict';
// Interface editor: corners, sizes, placement and per-element tweaks, live, on every screen.
// Everything is data (settings.ui, checked by the core) turned into CSS variables and
// [data-id] rules through CSSOM, so the CSP stays strict and nothing a user saves can run.
(() => {
  const { $, toast } = window.FUI;
  const root = document.documentElement;
  const RADIUS = [['btn', 'Кнопки'], ['field', 'Поля'], ['card', 'Карточки'], ['icon', 'Иконки'], ['menu', 'Меню'], ['chip', 'Метки'], ['nav', 'Боковая панель'], ['dialog', 'Окна']];
  const SIZE = [['btn', 'Кнопки', 70, 150], ['field', 'Поля', 70, 150], ['text', 'Текст', 85, 130], ['gap', 'Отступы', 60, 160], ['rail', 'Боковая панель', 80, 150], ['top', 'Верхняя панель', 80, 180]];
  const LAYOUT = [['rail', 'Боковая панель', 'left'], ['filters', 'Фильтры каталога', 'left'], ['project', 'Панель проекта', 'right']];
  const NAMES = {
    rail: 'Боковая панель', 'rail.logo': 'Логотип', 'rail.instances': 'Сборки в панели', topbar: 'Верхняя панель', main: 'Основная область',
    'instance.hero': 'Обложка сборки', 'instance.tabs': 'Вкладки сборки', 'instance.content.kind': 'Типы контента', 'discover.filters': 'Фильтры',
    'discover.kind': 'Типы контента', 'project.header': 'Шапка проекта', 'project.sidebar': 'Панель проекта', 'project.tabs': 'Вкладки проекта',
    'view.home': 'Библиотека', 'view.instance': 'Сборка', 'view.discover': 'Каталог', 'view.create': 'Новая сборка', 'view.accounts': 'Аккаунты', 'view.project': 'Проект',
  };
  const keepVisible = (id) => ['app', 'main', 'rail', 'rail.settings', 'safe'].includes(id) || /^(settings|view\.settings|palette)/.test(id);
  const blank = () => ({ radius: {}, size: {}, layout: {}, els: {}, order: {} });
  const cap = (k) => `l${k[0].toUpperCase()}${k.slice(1)}`;
  const q = (id) => `[data-id="${id}"]`;
  let ui = blank(), past = [], future = [], sel = null, hov = null, isOpen = false, saveT = 0, drag = false, safe = false;

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
  function apply() {
    for (const [g] of RADIUS) setVar(`--rk-${g}`, ui.radius[g]);
    for (const [g] of SIZE) setVar(`--sz-${g}`, ui.size[g]);
    for (const [k, , def] of LAYOUT) { const v = ui.layout[k]; if (v && v !== def) root.dataset[cap(k)] = v; else delete root.dataset[cap(k)]; }
    const out = [];
    for (const [id, e] of Object.entries(ui.els)) {
      const d = [];
      if (e.radius != null) d.push(`border-radius: ${e.radius}px !important`);
      if (e.scale != null && e.scale !== 100) d.push(`zoom: ${e.scale / 100}`);
      if (e.hide && !keepVisible(id)) d.push('display: none !important');
      if (d.length) out.push(`${q(id)} { ${d.join('; ')} }`);
    }
    for (const [id, list] of Object.entries(ui.order)) {
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
  function tidy(u, id) { const e = u.els[id]; if (e && e.radius == null && (e.scale == null || e.scale === 100) && !e.hide) delete u.els[id]; }

  // ---------- Building blocks ----------
  function h(tag, cls, html) { const n = document.createElement(tag); if (cls) n.className = cls; if (html != null) n.innerHTML = html; return n; }
  const ICO = {
    undo: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M5.5 4 2.5 7l3 3"/><path d="M3 7h6.5a3.5 3.5 0 0 1 0 7H8"/></svg>',
    redo: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M10.5 4l3 3-3 3"/><path d="M13 7H6.5a3.5 3.5 0 0 0 0 7H8"/></svg>',
    x: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="m4.5 4.5 7 7m0-7-7 7"/></svg>',
    pick: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"><path d="M3 2.5 13 7l-4.3 1.2L6.8 12.6z"/></svg>',
    back: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M10 3.5 5.5 8l4.5 4.5"/></svg>',
    fwd: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M6 3.5 10.5 8 6 12.5"/></svg>',
  };
  const ib = (ico, tip) => { const b = h('button', 'btn btn-ghost btn-icon ue-ib', ICO[ico]); b.type = 'button'; b.dataset.tip = tip; b.setAttribute('aria-label', tip); return b; };
  let rows = [];
  // A slider row. Double-click on the label resets it.
  function slider({ label, min, max, get, set, changed, fmt = (x) => `${x}%`, reset }) {
    const r = h('div', 'ue-row'), l = h('span', 'l'), v = h('span', 'v num'), i = h('input', 'ue-sl');
    l.textContent = label; i.type = 'range'; i.min = min; i.max = max; i.step = 1; i.setAttribute('aria-label', label);
    r.append(l, v, i);
    r._sync = () => {
      const x = get(); if (!drag || document.activeElement !== i) i.value = x;
      v.textContent = fmt(x); i.style.setProperty('--p', `${((x - min) / (max - min)) * 100}%`);
      r.toggleAttribute('data-changed', changed());
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
  const sec = (title, ...kids) => { const s = h('div', 'ue-sec'); s.append(h('h4', '', title), ...kids); return s; };

  // ---------- Panel ----------
  const panel = h('div', 'ue'); panel.dataset.id = 'editor'; panel.dataset.side = 'right'; panel.setAttribute('role', 'dialog'); panel.setAttribute('aria-label', 'Интерфейс');
  const head = h('div', 'ue-head'); const title = h('b', '', 'Интерфейс');
  const bUndo = ib('undo', 'Отменить · Ctrl Z'), bRedo = ib('redo', 'Повторить · Ctrl Shift Z');
  const bDone = h('button', 'btn btn-primary', '<span class="lbl">Готово</span>'); bDone.type = 'button';
  head.append(title, bUndo, bRedo, bDone);
  const tabs = h('div', 'seg ue-tabs', '<button type="button" data-tab="gen" aria-selected="true">Общее</button><button type="button" data-tab="el">Элемент</button>');
  const body = h('div', 'ue-body'), pGen = h('div', 'ue-pane'), pEl = h('div', 'ue-pane'); pEl.hidden = true;
  body.append(pGen, pEl);
  const foot = h('div', 'ue-foot'); const bReset = h('button', 'btn btn-ghost', '<span class="lbl">Сбросить всё</span>'); bReset.type = 'button'; foot.append(bReset);
  panel.append(head, tabs, body, foot);
  const hidSec = h('div', 'ue-sec'); const hidList = h('div', 'ue-picks');
  hidSec.append(h('h4', '', 'Скрытые'), hidList);
  pGen.append(
    sec('Углы', ...RADIUS.map(([g, label]) => slider({
      label, min: 0, max: 300, get: () => ui.radius[g] ?? 100, changed: () => (ui.radius[g] ?? 100) !== 100,
      set: (u, x) => { if (x === 100) delete u.radius[g]; else u.radius[g] = x; }, reset: (u) => delete u.radius[g],
    }))),
    sec('Размеры', ...SIZE.map(([g, label, lo, hi]) => slider({
      label, min: lo, max: hi, get: () => ui.size[g] ?? 100, changed: () => (ui.size[g] ?? 100) !== 100,
      set: (u, x) => { if (x === 100) delete u.size[g]; else u.size[g] = x; }, reset: (u) => delete u.size[g],
    }))),
    sec('Расположение', ...LAYOUT.map(([k, label, def]) => sides(label, k, def))),
    hidSec,
  );
  document.body.append(panel);
  window.FUI.initSegs(panel);
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
    const hidden = Object.entries(ui.els).filter(([, e]) => e.hide).map(([id]) => id);
    hidSec.hidden = !hidden.length;
    hidList.replaceChildren(...hidden.map((id) => {
      const c = h('button', 'ue-chip'); c.type = 'button'; c.textContent = name(id); c.dataset.tip = 'Показать снова';
      c.addEventListener('click', () => commit((u) => { u.els[id].hide = false; tidy(u, id); }));
      return c;
    }));
  }
  bUndo.addEventListener('click', undo); bRedo.addEventListener('click', redo);
  bDone.addEventListener('click', () => close());
  bReset.addEventListener('click', () => commit((u) => Object.assign(u, blank())));

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
      e.append(h('span', 'ue-empty-ico', ICO.pick), h('span', 't', 'Кликните по элементу'), h('span', 'muted', '<kbd>Alt</kbd> + клик — обычное нажатие'));
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
    }), slider({
      label: 'Размер', min: 50, max: 200, get: () => ui.els[id]?.scale ?? 100, changed: () => (ui.els[id]?.scale ?? 100) !== 100,
      set: (u, x) => { elOf(u, id).scale = x === 100 ? null : x; tidy(u, id); }, reset: (u) => { if (u.els[id]) { u.els[id].scale = null; tidy(u, id); } },
    }));
    if (!keepVisible(id)) {
      const r = h('label', 'ue-row ue-row-seg'), l = h('span', 'l', 'Скрыть'), sw = h('span', 'switch', '<input type="checkbox"><i></i>');
      const inp = sw.querySelector('input'); r.append(l, sw);
      inp.addEventListener('change', () => commit((u) => { elOf(u, id).hide = inp.checked; tidy(u, id); if (inp.checked) select(null); }));
      r._sync = () => { inp.checked = !!ui.els[id]?.hide; r.toggleAttribute('data-changed', inp.checked); };
      rows.push(r); s.append(r);
    }
    const p = node.parentElement;
    if (p?.dataset.id && p.children.length <= 64 && /flex|grid/.test(getComputedStyle(p).display)) {
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
    const f = h('div', 'ue-elf'), br = h('button', 'btn btn-ghost', '<span class="lbl">Сбросить элемент</span>'); br.type = 'button';
    br.addEventListener('click', () => commit((u) => delete u.els[id]));
    f.append(br);
    pEl.append(hd, s, f);
    sync();
  }
  function select(n) {
    if (!n) { sel = null; renderEl(); return; }
    const id = n.dataset.id;
    const had = ui.els[id]?.radius != null;
    sel = { id, node: n, base: had ? ui.els[id].radius : Math.round(parseFloat(getComputedStyle(n).borderTopLeftRadius) || 0) };
    const r = n.getBoundingClientRect();
    panel.dataset.side = r.right > innerWidth - 330 && r.left > 330 ? 'left' : 'right';
    renderEl(); tab('el');
  }

  // ---------- Picking ----------
  function pickable(t) {
    const n = t?.closest?.('[data-id]');
    if (!n || n.id === 'app' || n.closest('.ue') || !n.closest('#app')) return null;
    return n;
  }
  document.addEventListener('pointermove', (e) => { if (isOpen) hov = e.altKey ? null : pickable(e.target); }, true);
  for (const ev of ['pointerdown', 'mousedown', 'click', 'dblclick', 'contextmenu']) {
    document.addEventListener(ev, (e) => {
      if (!isOpen || e.altKey || e.target.closest?.('.ue, .palette')) return;
      const n = pickable(e.target); if (!n) return;
      e.preventDefault(); e.stopPropagation();
      if (ev === 'click') select(n);
    }, true);
  }
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
    place(hl, hov && hov !== sel?.node ? hov : null);
    requestAnimationFrame(loop);
  }

  // ---------- Open / close ----------
  function open() {
    if (safe) return;
    if (typeof S !== 'undefined' && S.view === 'settings') show(S.instances.length ? 'home' : 'empty');
    window.FUI.closeMenu?.();
    isOpen = true; root.dataset.uiEdit = '';
    panel.setAttribute('data-open', '');
    renderEl(); sync(); tab(sel ? 'el' : 'gen', false);
    requestAnimationFrame(loop);
  }
  function close() {
    isOpen = false; hov = null; delete root.dataset.uiEdit;
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
    apply(); sync();
  })();

  window.FEditor = { open, close, apply, get state() { return ui; } };
})();