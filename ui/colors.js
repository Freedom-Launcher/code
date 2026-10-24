'use strict';
// Base colors — Settings → Лаунчер → «Цвета». The user's accent, backgrounds, text, lines and
// status colors are laid over the active theme, separately for its dark and light base.
// Shades (hover, press, soft, on-accent, focus ring, glow) are derived here. Everything goes
// through CSSOM, so the CSP stays strict. Stored in settings.colors; localStorage mirrors it
// so the first paint is right before the core answers.
(() => {
  const root = document.documentElement;
  const call = (name, args = null) => window.__TAURI__.core.invoke('action', { name, args });
  const toast = (t, o) => window.FUI?.toast(t, o);
  const LS = 'fl.colors';
  const GROUPS = [
    [_t("Акцент"), [['accent', _t("Акцент")]]],
    [_t("Фон"), [['bg', _t("Фон окна")], ['surface-1', _t("Панели")], ['surface-2', _t("Карточки")], ['surface-3', _t("Поля и кнопки")], ['glass', _t("Тон стекла")]]],
    [_t("Текст"), [['text', _t("Основной")], ['text-2', _t("Второстепенный")], ['text-3', _t("Подсказки")]]],
    [_t("Линии"), [['line', _t("Разделители")], ['line-strong', _t("Рамки")]]],
    [_t("Статусы"), [['danger', _t("Ошибка")], ['warn', _t("Предупреждение")]]],
  ];
  const NAMES = Object.fromEntries(GROUPS.flatMap(([, l]) => l));
  const TOKENS = Object.keys(NAMES);
  const ALPHA = new Set(['line', 'line-strong']);
  const SOLID = new Set(['bg', 'surface-1', 'surface-2', 'surface-3']); // glass replaces them over a wallpaper
  const FAMILY = { accent: 'accent', bg: 'bg', 'surface-1': 'bg', 'surface-2': 'bg', 'surface-3': 'bg', text: 'text', danger: 'danger', warn: 'warn', glass: 'glass' };
  const PRESETS = {
    dark: {
      accent: ['#f4f4f5', '#8ab4ff', '#5b8cff', '#4cc2ff', '#22d3ee', '#f472b6', '#fb7185', '#ff8a4c', '#fbbf24'],
      bg: ['#08090a', '#0b0d12', '#0a0f1c', '#110e0c', '#000000'],
      text: ['#f7f8f8', '#ffffff', '#ece8e1', '#e3ebf7'],
      danger: ['#ff6b63', '#f43f5e', '#ef4444'],
      warn: ['#f2b64b', '#f59e0b', '#fb923c'],
      glass: ['#0e1016', '#0a0f1c', '#140f0c', '#000000'],
    },
    light: {
      accent: ['#18181b', '#2f6fde', '#1d4ed8', '#0284c7', '#0891b2', '#db2777', '#e11d48', '#ea580c', '#d97706'],
      bg: ['#f5f5f6', '#f1f5f9', '#f6f3ee', '#ffffff'],
      text: ['#111113', '#000000', '#1c1917', '#0f172a'],
      danger: ['#e5484d', '#dc2626', '#be123c'],
      warn: ['#c98a12', '#b45309', '#ea580c'],
      glass: ['#ffffff', '#f1f5f9', '#f6f3ee'],
    },
  };
  const SETS = {
    dark: [
      [_t("Полночь"), { accent: '#6ea8ff', bg: '#070a12', 'surface-1': '#0c111c', 'surface-2': '#111827', 'surface-3': '#172033', glass: '#0a0f1c' }],
      [_t("Графит"), { accent: '#e4e4e7', bg: '#0c0c0d', 'surface-1': '#141415', 'surface-2': '#1b1b1d', 'surface-3': '#232326' }],
      [_t("Уголь"), { bg: '#000000', 'surface-1': '#0a0a0a', 'surface-2': '#111111', 'surface-3': '#181818', glass: '#000000' }],
      [_t("Закат"), { accent: '#ff8a4c', bg: '#0f0c0a', 'surface-1': '#171310', 'surface-2': '#1d1814', 'surface-3': '#251f1a', glass: '#140f0c' }],
    ],
    light: [
      [_t("Бумага"), { accent: '#c2410c', bg: '#f6f3ee', 'surface-1': '#fffdf9', 'surface-2': '#f1ede6', 'surface-3': '#e8e3da', glass: '#f6f3ee' }],
      [_t("Лёд"), { accent: '#0284c7', bg: '#f1f5f9', 'surface-1': '#ffffff', 'surface-2': '#eef2f7', 'surface-3': '#e2e8f0', glass: '#f1f5f9' }],
    ],
  };

  // ---------- Color math ----------
  const clamp = (x, a, b) => Math.min(b, Math.max(a, x));
  const h2 = (n) => Math.round(clamp(n, 0, 255)).toString(16).padStart(2, '0');
  function parse(v) {
    v = String(v || '').trim();
    let m = /^#([0-9a-f]{3,8})$/i.exec(v);
    if (m) {
      let h = m[1];
      if (h.length === 3 || h.length === 4) h = [...h].map((c) => c + c).join('');
      if (h.length !== 6 && h.length !== 8) return null;
      const n = (i) => parseInt(h.slice(i, i + 2), 16);
      return { r: n(0), g: n(2), b: n(4), a: h.length === 8 ? n(6) / 255 : 1 };
    }
    m = /^rgba?\(([^)]+)\)$/i.exec(v);
    if (!m) return null;
    const p = m[1].split(/[\s,/]+/).filter(Boolean).map(parseFloat);
    return p.length < 3 || p.some(Number.isNaN) ? null : { r: p[0], g: p[1], b: p[2], a: p.length > 3 ? clamp(p[3], 0, 1) : 1 };
  }
  const toHex = ({ r, g, b, a = 1 }) => `#${h2(r)}${h2(g)}${h2(b)}${a < 0.998 ? h2(a * 255) : ''}`;
  const rgba = (c, a) => `rgba(${c.r}, ${c.g}, ${c.b}, ${a})`;
  const mix = (c, d, t) => ({ r: Math.round(c.r + (d.r - c.r) * t), g: Math.round(c.g + (d.g - c.g) * t), b: Math.round(c.b + (d.b - c.b) * t), a: 1 });
  const lum = (c) => { const f = (x) => { x /= 255; return x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4; }; return 0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b); };
  const contrast = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };
  function toHsv({ r, g, b }) {
    r /= 255; g /= 255; b /= 255;
    const mx = Math.max(r, g, b), d = mx - Math.min(r, g, b);
    let h = 0;
    if (d) { h = mx === r ? ((g - b) / d) % 6 : mx === g ? (b - r) / d + 2 : (r - g) / d + 4; h = (h * 60 + 360) % 360; }
    return { h, s: mx ? d / mx : 0, v: mx };
  }
  function fromHsv({ h, s, v }) {
    const f = (n) => { const k = (n + h / 60) % 6; return Math.round((v - v * s * Math.max(0, Math.min(k, 4 - k, 1))) * 255); };
    return { r: f(5), g: f(3), b: f(1) };
  }
  const W = { r: 255, g: 255, b: 255 }, K = { r: 0, g: 0, b: 0 };

  function derived(base, t, c) {
    const L = base === 'light';
    if (t === 'accent') {
      const dark = mix(c, K, 0.82);
      const d = [
        ['--accent-hover', toHex(mix(c, W, 0.14))], ['--accent-press', toHex(mix(c, K, 0.12))],
        ['--on-accent', contrast(c, W) >= contrast(c, dark) ? '#ffffff' : toHex(dark)],
        ['--accent-soft', rgba(c, L ? 0.11 : 0.12)], ['--accent-glow', rgba(c, 0.3)],
        ['--focus-ring', `0 0 0 2px var(--bg), 0 0 0 4px ${rgba(c, L ? 0.45 : 0.5)}`],
      ];
      if (L) d.push(['--shadow-accent', `inset 0 .5px rgba(255, 255, 255, .4), 0 1px 1px rgba(0, 0, 0, .12), 0 4px 10px -4px ${rgba(c, 0.5)}`]);
      return d;
    }
    if (t === 'danger') return [['--danger-soft', rgba(c, L ? 0.1 : 0.12)]];
    if (t === 'glass') {
      const v = `${c.r}, ${c.g}, ${c.b}`;
      return L ? [['--glass', `rgba(${v}, min(1, calc(var(--glass-a) * 1.27)))`], ['--glass-strong', `rgba(${v}, .86)`]]
        : [['--glass', `rgba(${v}, var(--glass-a))`], ['--glass-strong', `rgba(${v}, .78)`]];
    }
    return [];
  }

  // ---------- State ----------
  let colors = {};
  try { colors = JSON.parse(localStorage.getItem(LS) || '{}') || {}; } catch { colors = {}; }
  let ready = false, off = false, mine = new Set(), tv = null, saveT = 0, lsT = 0;
  const base = () => (root.dataset.theme === 'light' ? 'light' : 'dark');
  const wall = () => root.dataset.wall === 'on';
  const own = () => colors[base()] || {};

  function apply() {
    const b = base(), w = wall(), next = new Map();
    if (!off) {
      for (const [t, v] of Object.entries(colors[b] || {})) {
        const c = parse(v);
        if (!c || !NAMES[t] || (w && SOLID.has(t))) continue;
        if (t !== 'glass') next.set(`--${t}`, v);
        for (const [k, x] of derived(b, t, c)) next.set(k, x);
      }
    }
    const theme = window.FTheme?.tokens?.() || {};
    for (const k of mine) {
      if (next.has(k)) continue;
      if (theme[k] != null) root.style.setProperty(k, theme[k]); else root.style.removeProperty(k);
    }
    for (const [k, v] of next) root.style.setProperty(k, v);
    mine = new Set(next.keys());
  }

  // What the theme itself says, read with our overrides lifted for a moment (no paint in between).
  const cv = document.createElement('canvas').getContext('2d');
  function norm(v, t) {
    let c = parse(v);
    if (!c && v && cv) { cv.fillStyle = '#000'; cv.fillStyle = v.trim(); c = parse(cv.fillStyle); }
    if (!c) return null;
    if (!ALPHA.has(t)) c.a = 1;
    return toHex(c);
  }
  function themeVals() {
    if (tv) return tv;
    const keep = [...mine].map((k) => [k, root.style.getPropertyValue(k)]);
    const theme = window.FTheme?.tokens?.() || {};
    for (const [k] of keep) { if (theme[k] != null) root.style.setProperty(k, theme[k]); else root.style.removeProperty(k); }
    const cs = getComputedStyle(root);
    tv = {};
    for (const t of TOKENS) tv[t] = norm(cs.getPropertyValue(`--${t}`), t) || (t === 'glass' ? (base() === 'light' ? '#ffffff' : '#0e1016') : '#808080');
    for (const [k, v] of keep) root.style.setProperty(k, v);
    return tv;
  }
  const value = (t) => own()[t] || themeVals()[t];

  function persist() {
    const clean = {};
    for (const [b, m] of Object.entries(colors)) if (m && Object.keys(m).length) clean[b] = m;
    clearTimeout(lsT); lsT = setTimeout(() => localStorage.setItem(LS, JSON.stringify(clean)), 120);
    clearTimeout(saveT);
    saveT = setTimeout(() => { if (ready) call('settings.set', { colors: clean }).catch((e) => toast(String(e), { error: true })); }, 350);
  }
  function set(t, hex) {
    const b = base(), m = (colors[b] ||= {});
    if (hex == null || hex.toLowerCase() === themeVals()[t]) delete m[t]; else m[t] = hex.toLowerCase();
    apply(); paint(); persist();
  }
  function setAll(m) { colors[base()] = { ...m }; apply(); paint(); persist(); }

  // ---------- Settings row + inline panel ----------
  const ic = (d) => `<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">${d}</svg>`;
  const I = {
    chev: '<g fill="none" fill-rule="evenodd"><path d="M24 0v24H0V0h24ZM12.593 23.258l-.011.002-.071.035-.02.004-.014-.004-.071-.035c-.01-.004-.019-.001-.024.005l-.004.01-.017.428.005.02.01.013.104.074.015.004.012-.004.104-.074.012-.016.004-.017-.017-.427c-.002-.01-.009-.017-.017-.018Zm.265-.113-.013.002-.185.093-.01.01-.003.011.018.43.005.012.008.007.201.093c.012.004.023 0 .029-.008l.004-.014-.034-.614c-.003-.012-.01-.02-.02-.022Zm-.715.002a.023.023 0 0 0-.027.006l-.006.014-.034.614c0 .012.007.02.017.024l.015-.002.201-.093.01-.008.004-.011.017-.43-.003-.012-.01-.01-.184-.092Z"/><path fill="currentColor" d="M12.707 15.707a1 1 0 0 1-1.414 0L5.636 10.05A1 1 0 1 1 7.05 8.636l4.95 4.95 4.95-4.95a1 1 0 0 1 1.414 1.414l-5.657 5.657Z"/></g>',
    undo: '<g fill="none" fill-rule="evenodd"><path fill="currentColor" d="M14.07 19.727a8.003 8.003 0 0 1-9.146-3.99 1 1 0 0 0-1.77.933c2.13 4.04 6.836 6.221 11.434 4.99 5.335-1.43 8.5-6.914 7.071-12.248-1.43-5.335-6.913-8.5-12.247-7.071a10.003 10.003 0 0 0-7.414 9.58c-.007.903.995 1.402 1.713.919l2.673-1.801c1.008-.68.332-2.251-.854-1.986l-1.058.236a8 8 0 1 1 9.598 10.439Z"/></g>',
    drop: '<g fill="none" fill-rule="evenodd"><path fill="currentColor" d="M20.477 3.511a3 3 0 0 1 0 4.243l-1.533 1.533a2.991 2.991 0 0 1-.581 3.41l-.715.714a2 2 0 0 1-2.828 0l-6.485 6.485a3 3 0 0 1-2.122.879h-1.8a1.2 1.2 0 0 1-1.2-1.2v-1.8a3 3 0 0 1 .88-2.122l6.484-6.485a2 2 0 0 1 0-2.828l.714-.714a2.991 2.991 0 0 1 3.41-.582l1.533-1.533a3 3 0 0 1 4.243 0m-8.485 7.071-6.486 6.486-.087.099a1 1 0 0 0-.206.608v1h1l.132-.009a1 1 0 0 0 .575-.284l6.486-6.485zm7.07-5.657a1 1 0 0 0-1.414 0L15.534 7.04a1.01 1.01 0 0 1-1.428 0 .99.99 0 0 0-1.4 0l-.714.714 4.242 4.243.714-.715a.99.99 0 0 0 0-1.4 1.01 1.01 0 0 1 0-1.428l2.115-2.114a1 1 0 0 0 0-1.415Z"/></g>',
    warn: '<g fill="none"><path fill="currentColor" d="M12 2c5.523 0 10 4.477 10 10s-4.477 10-10 10S2 17.523 2 12 6.477 2 12 2m0 2a8 8 0 1 0 0 16 8 8 0 0 0 0-16m0 11a1 1 0 1 1 0 2 1 1 0 0 1 0-2m0-9a1 1 0 0 1 1 1v6a1 1 0 1 1-2 0V7a1 1 0 0 1 1-1"/></g>',
  };
  const STRIP = ['accent', 'bg', 'surface-2', 'text', 'danger'];
  const themeRow = document.getElementById('theme-seg')?.closest('.r');
  let row = null, panel = null, setsBase = '';
  if (themeRow) {
    row = document.createElement('div');
    row.className = 'r col-row';
    row.dataset.id = 'settings.colors';
    row.innerHTML = _t("<div class=\"grow\"><div class=\"t\">Цвета</div><div class=\"s\" id=\"col-sub\"></div></div>")
      + `<div class="col-strip" aria-hidden="true">${'<i></i>'.repeat(STRIP.length)}</div>`
      + _t("<button class=\"btn\" id=\"col-open\" type=\"button\" aria-expanded=\"false\" aria-controls=\"col-panel\"><span class=\"lbl\">Настроить</span>{0}</button>", [ic(I.chev)]);
    panel = document.createElement('div');
    panel.className = 'col-panel';
    panel.id = 'col-panel';
    panel.dataset.id = 'settings.colors.panel';
    panel.inert = true;
    panel.innerHTML = _t("<div class=\"col-in\"><div class=\"col-body\"><div class=\"col-sets\" id=\"col-sets\" role=\"group\" aria-label=\"Наборы\"></div>{0}</div></div>", [GROUPS.map(([g, l]) => `<section class="col-g"><h3 class="col-gh">${g}</h3><div class="col-list">${l.map(([t, n]) => _t("<div class=\"col-tok\" data-tok=\"{0}\"><button type=\"button\" class=\"col-pick\" aria-haspopup=\"dialog\" aria-expanded=\"false\"><i class=\"col-sw\"><b></b></i><span class=\"col-name\">{1}</span><span class=\"col-warn\" hidden>{2}</span><span class=\"col-hex mono\"></span></button><button type=\"button\" class=\"btn btn-ghost btn-icon btn-sm col-reset\" aria-label=\"Как в теме\" data-tip=\"Как в теме\">{3}</button></div>", [t, n, ic(I.warn), ic(I.undo)])).join('')}</div></section>`).join('')]);
    themeRow.after(row, panel);
    const openBtn = row.querySelector('#col-open');
    openBtn.addEventListener('click', () => {
      const o = !panel.hasAttribute('data-open');
      panel.toggleAttribute('data-open', o);
      panel.inert = !o;
      openBtn.setAttribute('aria-expanded', String(o));
      if (!o) closePicker(false);
    });
    panel.addEventListener('click', (e) => {
      const tok = e.target.closest('.col-tok')?.dataset.tok;
      if (e.target.closest('.col-reset')) { set(tok, null); if (pk?.t === tok) syncPicker(); return; }
      const pick = e.target.closest('.col-pick');
      if (pick) { if (pk?.t === tok) closePicker(true); else openPicker(tok, pick); return; }
      const chip = e.target.closest('.col-set');
      if (chip) { closePicker(false); setAll(chip.dataset.set === '' ? {} : SETS[base()][+chip.dataset.set][1]); }
    });
  }

  function renderSets() {
    const b = base(), tv0 = themeVals();
    const sw = (m) => `<span class="col-set-sw">${['accent', 'surface-2', 'bg'].map(() => '<i></i>').join('')}</span>`;
    const host = panel.querySelector('#col-sets');
    host.innerHTML = _t("<button type=\"button\" class=\"col-set\" data-set=\"\">{0}Как в теме</button>", [sw()])
      + SETS[b].map(([n], i) => `<button type="button" class="col-set" data-set="${i}">${sw()}${n}</button>`).join('');
    host.querySelectorAll('.col-set').forEach((chip) => {
      const m = chip.dataset.set === '' ? {} : SETS[b][+chip.dataset.set][1];
      chip.querySelectorAll('i').forEach((i, k) => { const t = ['accent', 'surface-2', 'bg'][k]; i.style.setProperty('--c', m[t] || tv0[t]); });
    });
    setsBase = b;
  }

  function paint() {
    if (!row) return;
    const m = own(), w = wall(), b = base();
    if (setsBase !== b || !tv) { tv = null; renderSets(); }
    const n = Object.keys(m).length;
    row.querySelector('#col-sub').textContent = n ? _t("Изменено: {0}", [n]) : _t("Как в теме");
    const strip = w ? ['accent', 'glass', 'text', 'danger', 'warn'] : STRIP;
    row.querySelectorAll('.col-strip > i').forEach((i, k) => i.style.setProperty('--c', value(strip[k])));
    const bg = parse(value('bg'));
    const LIM = { text: 4.5, 'text-2': 3, accent: 3 };
    for (const tok of panel.querySelectorAll('.col-tok')) {
      const t = tok.dataset.tok, v = value(t);
      tok.hidden = w ? SOLID.has(t) : t === 'glass';
      tok.toggleAttribute('data-own', !!m[t]);
      tok.querySelector('b').style.setProperty('--c', v);
      tok.querySelector('.col-hex').textContent = v.toUpperCase();
      tok.querySelector('.col-pick').setAttribute('aria-label', `${NAMES[t]}: ${v.toUpperCase()}`);
      const warn = tok.querySelector('.col-warn');
      const r = LIM[t] && !w ? contrast(parse(v), bg) : 99;
      warn.hidden = r >= LIM[t] || !LIM[t];
      if (!warn.hidden) warn.dataset.tip = _t("Контраст {0}:1", [r.toFixed(1)]);
    }
    const key = JSON.stringify(Object.entries(m).sort());
    panel.querySelectorAll('.col-set').forEach((chip) => {
      const s = chip.dataset.set === '' ? {} : SETS[b][+chip.dataset.set][1];
      chip.setAttribute('aria-pressed', String(JSON.stringify(Object.entries(s).sort()) === key));
    });
  }

  // ---------- Picker popover ----------
  const pop = document.createElement('div');
  pop.className = 'pop cp';
  pop.setAttribute('role', 'dialog');
  pop.inert = true;
  pop.innerHTML = _t("<div class=\"cp-head\"><span class=\"cp-title\"></span><span class=\"cp-cmp\"><button type=\"button\" class=\"cp-was\" aria-label=\"Вернуть как было\" data-tip=\"Как было\"></button><i class=\"cp-now\"></i></span></div>")
    + _t("<div class=\"cp-sv\" tabindex=\"0\" role=\"slider\" aria-label=\"Насыщенность и яркость\"><i class=\"cp-th\"></i></div>")
    + _t("<div class=\"cp-tr cp-hue\" tabindex=\"0\" role=\"slider\" aria-label=\"Оттенок\" aria-valuemin=\"0\" aria-valuemax=\"360\"><i class=\"cp-th\"></i></div>")
    + _t("<div class=\"cp-tr cp-alpha\" tabindex=\"0\" role=\"slider\" aria-label=\"Непрозрачность\" aria-valuemin=\"0\" aria-valuemax=\"100\"><i class=\"cp-th\"></i></div>")
    + _t("<div class=\"cp-io\"><input class=\"field mono cp-hex\" spellcheck=\"false\" autocomplete=\"off\" maxlength=\"9\" aria-label=\"HEX\"><button type=\"button\" class=\"btn btn-ghost btn-icon btn-sm cp-drop\" aria-label=\"Пипетка\" data-tip=\"Пипетка\">{0}</button></div>", [ic(I.drop)])
    + '<div class="cp-pre"></div>';
  document.body.append(pop);
  const q = (s) => pop.querySelector(s);
  const sv = q('.cp-sv'), hue = q('.cp-hue'), alpha = q('.cp-alpha'), hexIn = q('.cp-hex'), pre = q('.cp-pre');
  let pk = null;

  function draw(fromInput) {
    const { hsv, a, t } = pk, rgb = fromHsv(hsv), hex = toHex({ ...rgb, a: hasA() ? a : 1 });
    pop.style.setProperty('--c', toHex(rgb));
    sv.style.setProperty('--h', hsv.h.toFixed(1));
    sv.style.setProperty('--x', hsv.s.toFixed(4)); sv.style.setProperty('--y', (1 - hsv.v).toFixed(4));
    hue.style.setProperty('--x', (hsv.h / 360).toFixed(4)); hue.setAttribute('aria-valuenow', String(Math.round(hsv.h)));
    alpha.style.setProperty('--x', a.toFixed(3)); alpha.setAttribute('aria-valuenow', String(Math.round(a * 100)));
    q('.cp-now').style.setProperty('--c', hex);
    if (!fromInput) hexIn.value = hex.toUpperCase();
    pre.querySelectorAll('.cp-p').forEach((p) => p.setAttribute('aria-pressed', String(p.dataset.c === hex)));
    return hex;
  }
  const hasA = () => (pk?.custom ? !!pk.custom.alpha : ALPHA.has(pk?.t));
  const change = (fromInput) => { const x = draw(fromInput); if (pk.custom) { pk.custom.cur = x; pk.custom.onChange?.(x); } else set(pk.t, x); };
  function take(c) { // keeps the hue of greys, so the thumb does not jump to red
    const h = toHsv(c);
    if (!h.s && pk) h.h = pk.hsv.h;
    pk.hsv = h;
    if (hasA()) pk.a = c.a;
  }
  function syncPicker() { take(parse(pk.custom ? pk.custom.cur : value(pk.t))); draw(); }

  function place() {
    const r = pk.anchor.getBoundingClientRect();
    const hgt = pop.offsetHeight, below = innerHeight - r.bottom - 12, up = hgt > below && r.top - 12 > below;
    pop.style.left = `${clamp(r.left, 8, innerWidth - pop.offsetWidth - 8)}px`;
    pop.style.top = `${up ? r.top - 6 - hgt : r.bottom + 6}px`;
    pop.style.setProperty('--origin', up ? 'bottom left' : 'top left');
  }
  function openPicker(t, anchor) {
    if (pk) closePicker(false, true);
    pop.classList.remove('cp-top');
    const v = value(t);
    pk = { t, a: 1, hsv: { h: 0, s: 0, v: 0 }, anchor, was: own()[t] ?? null, wasV: v };
    take(parse(v));
    q('.cp-title').textContent = NAMES[t];
    q('.cp-was').style.setProperty('--c', v);
    alpha.hidden = !ALPHA.has(t);
    const fam = PRESETS[base()][FAMILY[t]] || [];
    const tvv = themeVals()[t];
    const list = [tvv, ...fam.filter((c) => c !== tvv)];
    pre.innerHTML = list.map((c, i) => `<button type="button" class="cp-p${i ? '' : ' is-theme'}" data-c="${c}" aria-label="${i ? c.toUpperCase() : _t("Как в теме")}" data-tip="${i ? c.toUpperCase() : _t("Как в теме")}"></button>`).join('');
    pre.querySelectorAll('.cp-p').forEach((p) => p.style.setProperty('--c', p.dataset.c));
    draw();
    pop.inert = false;
    pop.setAttribute('aria-label', NAMES[t]);
    pop.setAttribute('data-instant', '');
    place();
    pop.removeAttribute('data-instant');
    anchor.setAttribute('aria-expanded', 'true');
    requestAnimationFrame(() => { pop.setAttribute('data-open', ''); sv.focus({ preventScroll: true }); });
  }
  function closePicker(focus, instant) {
    if (!pk) return;
    pk.anchor.setAttribute('aria-expanded', 'false');
    if (focus) pk.anchor.focus({ preventScroll: true });
    if (instant) pop.setAttribute('data-instant', '');
    pop.removeAttribute('data-open');
    pop.inert = true;
    if (instant) requestAnimationFrame(() => pop.removeAttribute('data-instant'));
    const cu = pk.custom; pk = null; cu?.onClose?.();
  }

  function drag(node, fn) {
    node.addEventListener('pointerdown', (e) => {
      if (e.button !== 0 || !pk) return;
      e.preventDefault();
      node.focus({ preventScroll: true });
      node.setPointerCapture(e.pointerId);
      pop.classList.add('is-drag');
      const mv = (ev) => { const r = node.getBoundingClientRect(); fn(clamp((ev.clientX - r.left) / r.width, 0, 1), clamp((ev.clientY - r.top) / r.height, 0, 1)); change(); };
      const up = () => { pop.classList.remove('is-drag'); node.removeEventListener('pointermove', mv); node.removeEventListener('pointerup', up); node.removeEventListener('pointercancel', up); };
      node.addEventListener('pointermove', mv); node.addEventListener('pointerup', up); node.addEventListener('pointercancel', up);
      mv(e);
    });
  }
  function keys(node, fn) {
    node.addEventListener('keydown', (e) => {
      const s = e.shiftKey ? 10 : 1;
      const d = { ArrowLeft: [-s, 0], ArrowRight: [s, 0], ArrowUp: [0, -s], ArrowDown: [0, s] }[e.key];
      if (!d || !pk) return;
      e.preventDefault(); fn(...d); change();
    });
  }
  drag(sv, (x, y) => { pk.hsv.s = x; pk.hsv.v = 1 - y; });
  drag(hue, (x) => { pk.hsv.h = x * 360; });
  drag(alpha, (x) => { pk.a = Math.round(x * 100) / 100; });
  keys(sv, (dx, dy) => { pk.hsv.s = clamp(pk.hsv.s + dx / 100, 0, 1); pk.hsv.v = clamp(pk.hsv.v - dy / 100, 0, 1); });
  keys(hue, (dx, dy) => { pk.hsv.h = (pk.hsv.h + (dx || -dy) + 360) % 360; });
  keys(alpha, (dx, dy) => { pk.a = clamp(pk.a + (dx || -dy) / 100, 0, 1); });

  hexIn.addEventListener('input', () => {
    if (!pk) return;
    let v = hexIn.value.trim();
    if (!v.startsWith('#')) v = `#${v}`;
    if (!/^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i.test(v)) return;
    take(parse(v)); change(true);
  });
  hexIn.addEventListener('blur', () => pk && draw());
  hexIn.addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); closePicker(true); } });
  pre.addEventListener('click', (e) => {
    const p = e.target.closest('.cp-p');
    if (!p || !pk) return;
    if (p.classList.contains('is-theme')) { if (pk.custom) { pk.custom.cur = pk.custom.def; pk.custom.onChange?.(null); } else set(pk.t, null); syncPicker(); } else { take(parse(p.dataset.c)); change(); }
  });
  q('.cp-was').addEventListener('click', () => { if (!pk) return; if (pk.custom) { pk.custom.cur = pk.custom.v0; pk.custom.onChange?.(pk.custom.was); } else set(pk.t, pk.was); syncPicker(); });
  const drop = q('.cp-drop');
  if ('EyeDropper' in window) {
    drop.addEventListener('click', async () => {
      try { const r = await new window.EyeDropper().open(); const c = parse(r.sRGBHex); if (c && pk) { take({ ...c, a: pk.a }); change(); } } catch { /* cancelled */ }
    });
  } else drop.hidden = true;

  pop.addEventListener('keydown', (e) => { if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); closePicker(true); } });
  document.addEventListener('pointerdown', (e) => { if (pk && !pop.contains(e.target) && !e.target.closest('.col-pick, .ue-sw')) closePicker(false); }, true);
  addEventListener('resize', () => pk && place());
  document.addEventListener('scroll', () => pk && place(), true);

  // ---------- Keep in step with theme / wallpaper ----------
  const refresh = () => { tv = null; apply(); paint(); if (pk) syncPicker(); };
  new MutationObserver(() => { if (pk) closePicker(false, true); refresh(); }).observe(root, { attributes: true, attributeFilter: ['data-theme', 'data-wall'] });
  document.addEventListener('theme:applied', refresh);

  apply();
  paint();
  (async () => {
    try {
      const info = await call('app.info');
      if (info?.safe_mode) { off = true; apply(); return; }
      const st = await call('settings.get');
      colors = st?.colors && typeof st.colors === 'object' ? st.colors : {};
      localStorage.setItem(LS, JSON.stringify(colors));
      ready = true;
      refresh();
    } catch (e) { console.warn('colors', e); }
  })();
  // Theme save: the current base's colours as CSS custom properties, derived shades included.
  function bake() {
    const b = base(), out = {};
    for (const [t, v] of Object.entries(colors[b] || {})) {
      const c = parse(v);
      if (!c || !NAMES[t]) continue;
      if (t !== 'glass') out[`--${t}`] = v;
      for (const [k, x] of derived(b, t, c)) out[k] = x;
    }
    return out;
  }
  // The colours now live in the theme: drop our copy without a repaint flash.
  function baked() { delete colors[base()]; tv = null; persist(); refresh(); }
  function cssHex(v) {
    if (!v) return null;
    let c = parse(v);
    if (!c && cv) { cv.fillStyle = '#000'; cv.fillStyle = String(v).trim(); c = parse(cv.fillStyle); }
    return c ? toHex(c) : null;
  }
  // The same popover for any colour (interface editor): own value, default and callbacks.
  function pick(anchor, o) {
    if (pk) closePicker(false, true);
    const d = cssHex(o.def) || '#808080', v = cssHex(o.own) || cssHex(o.value) || d;
    pk = { t: null, custom: { ...o, cur: v, v0: v, def: d, was: o.own ?? null }, a: 1, hsv: { h: 0, s: 0, v: 0 }, anchor };
    take(parse(v));
    q('.cp-title').textContent = o.title;
    q('.cp-was').style.setProperty('--c', v);
    alpha.hidden = !o.alpha;
    const fam = [...new Set(Object.values(PRESETS[base()] || {}).flat())].filter((c) => c !== d).slice(0, 11);
    pre.innerHTML = [d, ...fam].map((c, i) => `<button type="button" class="cp-p${i ? '' : ' is-theme'}" data-c="${c}" aria-label="${i ? c.toUpperCase() : _t("Как в теме")}" data-tip="${i ? c.toUpperCase() : _t("Как в теме")}"></button>`).join('');
    pre.querySelectorAll('.cp-p').forEach((p) => p.style.setProperty('--c', p.dataset.c));
    draw();
    pop.classList.add('cp-top');
    pop.inert = false;
    pop.setAttribute('aria-label', o.title);
    pop.setAttribute('data-instant', '');
    place();
    pop.removeAttribute('data-instant');
    anchor.setAttribute('aria-expanded', 'true');
    requestAnimationFrame(() => { pop.setAttribute('data-open', ''); sv.focus({ preventScroll: true }); });
  }
  window.FColors = { bake, baked, pick, close: () => closePicker(false, true), open: (t) => { const b = panel?.querySelector(`.col-tok[data-tok="${t}"] .col-pick`); if (b) openPicker(t, b); }, refresh };
})();