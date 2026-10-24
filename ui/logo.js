'use strict';
// Logo look — Settings → Внешний вид → «Логотип»: glyph colour, a plate behind it, the plate's
// shape and the feather fade. Stored in localStorage `fl.logo` (only what differs from the default)
// and applied as <html> attributes + CSS variables through CSSOM, so the CSP stays strict.
// Loaded in <head> without defer: the attributes are set before the first paint.
(() => {
  const root = document.documentElement;
  const LS = 'fl.logo';
  const DEF = { fg: 'text', fgc: '#8ab4ff', bg: 'none', bgc: '#1a1a1a', shape: 'square', fade: true };
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
  }
  function picker(btn, key, title) {
    const P = window.FColors;
    if (!P?.pick) return;
    if (btn.getAttribute('aria-expanded') === 'true') { P.close(); return; }
    P.pick(btn, { title, own: o[key], value: o[key], def: DEF[key], alpha: false, onChange: (hex) => { if (hex) { o[key] = hex.toLowerCase(); save(); } } });
  }

  document.addEventListener('DOMContentLoaded', () => {
    const prev = $('logo-prev'), mark = document.querySelector('.rail-logo .brand-svg');
    if (prev && mark) prev.append(mark.cloneNode(true));
    $('logo-fg')?.addEventListener('seg:change', (e) => { o.fg = e.detail.dataset.v; save(); });
    $('logo-bg')?.addEventListener('seg:change', (e) => {
      const was = o.bg;
      o.bg = e.detail.dataset.v;
      // A filled plate under a dark-text or same-accent glyph would swallow it: switch the glyph to white once.
      if (was === 'none' && o.bg !== 'none' && (o.fg === 'text' || (o.fg === 'accent' && (o.bg === 'accent' || o.bg === 'grad')))) o.fg = 'white';
      save();
    });
    $('logo-shape')?.addEventListener('seg:change', (e) => { o.shape = e.detail.dataset.v; save(); });
    $('logo-fade')?.addEventListener('change', (e) => { o.fade = e.target.checked; save(); });
    $('logo-fg-c')?.addEventListener('click', (e) => picker(e.currentTarget, 'fgc', _t("Цвет знака")));
    $('logo-bg-c')?.addEventListener('click', (e) => picker(e.currentTarget, 'bgc', _t("Цвет подложки")));
    $('logo-reset')?.addEventListener('click', () => { window.FColors?.close?.(); o = { ...DEF }; save(); });
    sync();
    requestAnimationFrame(sync);
  });
})();