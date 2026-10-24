'use strict';
// Logo look — Settings → Внешний вид → «Логотип»: glyph colour, a plate behind it, the plate's
// shape and the feather fade. Stored in localStorage `fl.logo` (only what differs from the default)
// and applied as <html> attributes + CSS variables through CSSOM, so the CSP stays strict.
// Inside the app the mark only changes colour. The plate and shape go into the system icon
// (window, taskbar, tray): it is drawn here on a canvas and handed to the core (`set_app_icon`).
// Loaded in <head> without defer: the attributes are set before the first paint.
(() => {
  const root = document.documentElement;
  const LS = 'fl.logo';
  const DEF = { fg: 'text', fgc: '#f4f4f5', bg: 'none', bgc: '#1a1a1a', shape: 'square', fade: true };
  const load = () => { try { return { ...DEF, ...JSON.parse(localStorage.getItem(LS) || '{}') }; } catch { return { ...DEF }; } };
  let o = load();
  const apply = () => {
    root.dataset.logoFg = o.fg;
    root.dataset.logoBg = o.bg;
    root.dataset.logoShape = o.shape;
    root.dataset.logoFade = o.fade ? 'on' : 'off';
    root.style.setProperty('--logo-fg-c', o.fgc);
    root.style.setProperty('--logo-bg-c', o.bgc);
  };
  apply();

  // ---------- System icon ----------
  const SIZE = 128;
  let glyph = '';          // inner markup of the brand mark (paths), taken from the rail
  let sentCustom = false;  // a custom icon is on the window now
  const probe = document.createElement('canvas').getContext('2d');
  const rgbOf = (c) => {
    probe.fillStyle = '#000'; probe.fillStyle = c;
    const v = probe.fillStyle;
    if (v[0] === '#') return [1, 3, 5].map((i) => parseInt(v.slice(i, i + 2), 16));
    const m = v.match(/[\d.]+/g) || [0, 0, 0];
    return m.slice(0, 3).map(Number);
  };
  const lum = ([r, g, b]) => { const f = (x) => { x /= 255; return x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4; }; return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b); };
  const contrast = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };
  const hex = (rgb) => '#' + rgb.map((n) => Math.round(n).toString(16).padStart(2, '0')).join('');
  const css = (name) => getComputedStyle(root).getPropertyValue(name).trim();

  function iconSvg() {
    const accent = css('--accent') || '#f4f4f5';
    const plate = o.bg === 'none' ? null
      : o.bg === 'accent' ? { fill: hex(rgbOf(accent)) }
      : o.bg === 'dark' ? { fill: '#111113' }
      : o.bg === 'grad' ? { grad: true, fill: hex(rgbOf(accent)) }
      : { fill: hex(rgbOf(o.bgc)) };
    let fg = hex(rgbOf(css('--logo-fg') || '#ffffff'));
    if (plate) {
      // The glyph must stay readable on its plate whatever the in-app colour is.
      const p = rgbOf(plate.fill);
      if (contrast(rgbOf(fg), p) < 2.2) fg = contrast([255, 255, 255], p) >= contrast([9, 9, 11], p) ? '#ffffff' : '#09090b';
    } else if (o.fg === 'text') {
      fg = '#ffffff'; // the taskbar is not the app: "as text" means the plain white mark
    }
    const body = o.fade ? glyph : glyph.replace(/\sopacity="[^"]*"/g, '');
    const S = SIZE;
    let back = '', defs = '';
    if (plate) {
      const r = o.shape === 'round' ? S / 2 : S * 0.225;
      if (plate.grad) {
        defs = `<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${hex(rgbOf(`color-mix(in srgb, ${accent} 55%, #fff)`))}"/><stop offset=".5" stop-color="${plate.fill}"/><stop offset="1" stop-color="${hex(rgbOf(`color-mix(in srgb, ${accent} 62%, #000)`))}"/></linearGradient></defs>`;
      }
      back = `<rect width="${S}" height="${S}" rx="${r}" fill="${plate.grad ? 'url(#g)' : plate.fill}"/>`;
    }
    // Mark viewBox is 60×40 (from 2,12). With a plate it takes 72% of the width, bare 96%.
    const k = (plate ? 0.72 : 0.96) * S / 60;
    const tx = (S - 60 * k) / 2 - 2 * k, ty = (S - 40 * k) / 2 - 12 * k;
    return `<svg xmlns="http://www.w3.org/2000/svg" width="${S}" height="${S}" viewBox="0 0 ${S} ${S}">${defs}${back}<g fill="${fg}" transform="translate(${tx} ${ty}) scale(${k})">${body}</g></svg>`;
  }

  function render() {
    return new Promise((res) => {
      const img = new Image();
      img.onload = () => {
        const c = document.createElement('canvas'); c.width = c.height = SIZE;
        const x = c.getContext('2d'); x.drawImage(img, 0, 0, SIZE, SIZE);
        res(c);
      };
      img.onerror = () => res(null);
      img.src = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(iconSvg());
    });
  }

  let pend = 0;
  async function refreshIcon() {
    if (!glyph) return;
    const my = ++pend;
    const custom = !!localStorage.getItem(LS);
    const c = await render();
    if (my !== pend || !c) return;
    const prev = document.getElementById('logo-prev');
    if (prev) {
      prev.replaceChildren(c);
    }
    const inv = window.__TAURI__?.core?.invoke;
    if (!inv) return;
    if (custom) {
      const d = c.getContext('2d').getImageData(0, 0, SIZE, SIZE).data;
      inv('set_app_icon', { rgba: Array.from(d), w: SIZE, h: SIZE }).catch(() => {});
      sentCustom = true;
    } else if (sentCustom) {
      inv('set_app_icon', { rgba: [], w: 0, h: 0 }).catch(() => {});
      sentCustom = false;
    }
  }
  let rT = 0;
  const refreshSoon = () => { clearTimeout(rT); rT = setTimeout(refreshIcon, 60); };

  const $ = (id) => document.getElementById(id);
  const segSet = (id, v) => {
    const s = $(id);
    if (!s) return;
    s.querySelectorAll('button').forEach((b) => b.setAttribute('aria-selected', String(b.dataset.v === v)));
    s._place?.(false);
  };
  function sync() {
    const sh = $('logo-shape-row');
    if (sh) sh.hidden = o.bg === 'none';
    const a = $('logo-fg-c'), b = $('logo-bg-c');
    if (a) { a.hidden = o.fg !== 'custom'; a.style.setProperty('--c', o.fgc); }
    if (b) { b.hidden = o.bg !== 'custom'; b.style.setProperty('--c', o.bgc); }
    segSet('logo-fg', o.fg); segSet('logo-bg', o.bg); segSet('logo-shape', o.shape);
    const f = $('logo-fade');
    if (f) f.checked = o.fade;
    const r = $('logo-reset');
    if (r) r.disabled = !localStorage.getItem(LS);
  }
  function save() {
    const d = {};
    for (const k of Object.keys(DEF)) if (o[k] !== DEF[k]) d[k] = o[k];
    if (Object.keys(d).length) localStorage.setItem(LS, JSON.stringify(d)); else localStorage.removeItem(LS);
    apply();
    sync();
    refreshSoon();
  }
  function picker(btn, key, title) {
    const P = window.FColors;
    if (!P?.pick) return;
    if (btn.getAttribute('aria-expanded') === 'true') { P.close(); return; }
    P.pick(btn, { title, own: o[key], value: o[key], def: DEF[key], alpha: false, onChange: (hex) => { if (hex) { o[key] = hex.toLowerCase(); save(); } } });
  }

  document.addEventListener('DOMContentLoaded', () => {
    const mark = document.querySelector('.rail-logo .brand-svg');
    if (mark) glyph = mark.innerHTML;
    $('logo-fg')?.addEventListener('seg:change', (e) => { o.fg = e.detail.dataset.v; save(); });
    $('logo-bg')?.addEventListener('seg:change', (e) => { o.bg = e.detail.dataset.v; save(); });
    $('logo-shape')?.addEventListener('seg:change', (e) => { o.shape = e.detail.dataset.v; save(); });
    $('logo-fade')?.addEventListener('change', (e) => { o.fade = e.target.checked; save(); });
    $('logo-fg-c')?.addEventListener('click', (e) => picker(e.currentTarget, 'fgc', _t("Цвет знака")));
    $('logo-bg-c')?.addEventListener('click', (e) => picker(e.currentTarget, 'bgc', _t("Цвет подложки")));
    $('logo-reset')?.addEventListener('click', () => { window.FColors?.close?.(); o = { ...DEF }; save(); });
    sync();
    requestAnimationFrame(sync);
    // Theme or accent changes recolour "accent" / "as text" glyphs and accent plates.
    new MutationObserver(refreshSoon).observe(root, { attributes: true, attributeFilter: ['data-theme', 'style'] });
    setTimeout(refreshIcon, 300);
  });
})();