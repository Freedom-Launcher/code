'use strict';
// Freedom UI kit: behaviour for components (segmented pill, menus, dialogs, tooltip, toast, progress).
// No framework. Everything is driven by markup attributes, so themes can restructure screens freely.
(() => {
  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => [...r.querySelectorAll(s)];

  // ---------- Segmented control: sliding pill ----------
  function seg(root) {
    let pill = $('.seg-pill', root);
    if (!pill) { pill = document.createElement('i'); pill.className = 'seg-pill'; root.prepend(pill); }
    const place = (instant) => {
      const b = $('button[aria-selected="true"]', root) || $('button', root);
      if (!b) return;
      if (instant) pill.style.transition = 'none';
      pill.style.width = `${b.offsetWidth}px`;
      pill.style.transform = `translateX(${b.offsetLeft}px)`;
      if (instant) requestAnimationFrame(() => { pill.style.transition = ''; });
    };
    root.addEventListener('click', (e) => {
      const b = e.target.closest('button');
      if (!b || !root.contains(b)) return;
      $$('button', root).forEach((x) => x.setAttribute('aria-selected', String(x === b)));
      place(false);
      root.dispatchEvent(new CustomEvent('seg:change', { bubbles: true, detail: b }));
    });
    root._place = place;
    place(true);
  }
  const initSegs = (r = document) => $$('.seg', r).forEach(seg);

  // ---------- Popover menus: open from their origin ----------
  let openMenu = null;
  function closeMenu() { if (openMenu) { openMenu.removeAttribute('data-open'); openMenu = null; } }
  function toggleMenu(btn) {
    const m = document.getElementById(btn.dataset.menu);
    if (!m) return;
    if (openMenu === m) { closeMenu(); return; }
    closeMenu();
    const r = btn.getBoundingClientRect();
    const right = r.left > innerWidth / 2;
    m.style.top = `${r.bottom + 6}px`;
    m.style.left = right ? '' : `${r.left}px`;
    m.style.right = right ? `${innerWidth - r.right}px` : '';
    m.style.setProperty('--origin', right ? 'top right' : 'top left');
    // Keyboard-opened menus appear instantly (frequent action, no motion needed).
    m.toggleAttribute('data-instant', !!btn._kbd);
    m.setAttribute('data-open', '');
    openMenu = m;
  }

  // ---------- Dialogs ----------
  let openDialog = null;
  function openDlg(id) {
    const d = document.getElementById(id);
    if (!d) return;
    closeMenu();
    openDialog = d;
    $('#scrim')?.setAttribute('data-open', '');
    d.setAttribute('data-open', '');
    setTimeout(() => $('input:not([type=checkbox]), select', d)?.focus(), 30);
    d.dispatchEvent(new CustomEvent('dialog:open', { bubbles: true }));
  }
  function closeDlg() {
    if (!openDialog) return;
    openDialog.removeAttribute('data-open');
    $('#scrim')?.removeAttribute('data-open');
    openDialog = null;
  }

  // ---------- Tooltip: delayed in, travels between targets, instant out ----------
  let tipTimer = 0, tipShown = false;
  function showTip(t) {
    const tip = $('#tip');
    if (!tip) return;
    tip.textContent = t.dataset.tip;
    if (t.dataset.kbd) { const k = document.createElement('kbd'); k.textContent = t.dataset.kbd; tip.append(k); }
    const r = t.getBoundingClientRect();
    const w = tip.offsetWidth;
    tip.style.left = `${Math.max(8, Math.min(innerWidth - w - 8, r.left + r.width / 2 - w / 2))}px`;
    tip.style.top = `${r.bottom + 8}px`;
    tip.setAttribute('data-open', '');
    tipShown = true;
  }
  function hideTip() { clearTimeout(tipTimer); $('#tip')?.removeAttribute('data-open'); }
  document.addEventListener('pointerover', (e) => {
    const t = e.target.closest('[data-tip]');
    clearTimeout(tipTimer);
    if (!t) { hideTip(); setTimeout(() => { if (!$('#tip[data-open]')) tipShown = false; }, 300); return; }
    tipTimer = setTimeout(() => showTip(t), tipShown ? 0 : 500);
  });
  document.addEventListener('pointerdown', hideTip);

  // ---------- Toasts ----------
  function toast(text, { error = false, ms = 3200, action = null } = {}) {
    const box = $('#toasts');
    if (!box) return;
    const t = document.createElement('div');
    t.className = `toast${error ? ' is-error' : ''}`;
    t.setAttribute('role', error ? 'alert' : 'status');
    const dot = document.createElement('span'); dot.className = 'dot';
    const msg = document.createElement('span'); msg.textContent = text;
    t.append(dot, msg);
    const leave = () => { if (t.classList.contains('is-leaving')) return; t.classList.add('is-leaving'); t.addEventListener('animationend', () => t.remove(), { once: true }); };
    if (action) {
      const b = document.createElement('button');
      b.type = 'button'; b.className = 'toast-act'; b.textContent = action.label;
      b.addEventListener('click', () => { leave(); action.run(); });
      t.append(b); t.classList.add('has-act');
    }
    box.append(t);
    setTimeout(leave, action ? Math.max(ms, 6000) : ms);
  }

  // ---------- Progress ----------
  function progress(el, value) {
    if (value == null) { el.setAttribute('data-indeterminate', ''); return; }
    el.removeAttribute('data-indeterminate');
    el.style.setProperty('--p', Math.max(0, Math.min(1, value)));
  }

  // ---------- Text swap: blur + scale ----------
  function swapText(el, text) {
    if (el.textContent === text) return;
    el.classList.add('swap', 'is-out');
    setTimeout(() => { el.textContent = text; el.classList.remove('is-out'); }, 120);
  }

  // ---------- Global wiring ----------
  document.addEventListener('keydown', (e) => { if (e.key === 'Enter' || e.key === ' ') e.target._kbd = true; });
  document.addEventListener('click', (e) => {
    const m = e.target.closest('[data-menu]');
    if (m) { toggleMenu(m); m._kbd = false; return; }
    const o = e.target.closest('[data-open]:not(.pop):not(.dialog):not(.scrim)');
    if (o && o.dataset.open) { openDlg(o.dataset.open); return; }
    if (e.target.closest('[data-close]') || e.target.id === 'scrim') { closeDlg(); return; }
    if (openMenu && !openMenu.contains(e.target)) closeMenu();
    else if (openMenu && e.target.closest('.menu-item')) closeMenu();
  });
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') { closeMenu(); closeDlg(); hideTip(); }
  });
  addEventListener('resize', () => { closeMenu(); $$('.seg').forEach((s) => s._place && s._place(true)); });

  // Dev check: every visible region needs a data-id, every interactive element a data-action or handler id.
  function devCheck() {
    const missing = $$('section, aside, main, header, nav, .card, .dialog, .pop').filter((n) => !n.closest('[data-id]'));
    if (missing.length) console.warn('[freedom] regions without data-id:', missing);
    return missing.length;
  }

  /** Opens a menu at the pointer (custom context menu), kept inside the window. */
  function openMenuAt(m, x, y) {
    closeMenu();
    m.style.right = '';
    m.style.left = `${x}px`; m.style.top = `${y}px`;
    m.style.setProperty('--origin', 'top left');
    m.setAttribute('data-instant', '');
    m.setAttribute('data-open', '');
    openMenu = m;
    const r = m.getBoundingClientRect();
    if (r.right > innerWidth - 8) m.style.left = `${Math.max(8, x - r.width)}px`;
    if (r.bottom > innerHeight - 8) m.style.top = `${Math.max(8, y - r.height)}px`;
  }
  // No browser context menu (Печать, Проверить…): this is an app, not a page.
  // Text fields keep the native one for copy/paste; elements with
  // `data-ctx="<menu id>"` get our own menu.
  document.addEventListener('contextmenu', (e) => {
    if (e.target.closest('input, textarea, [contenteditable]')) return;
    e.preventDefault();
    const host = e.target.closest('[data-ctx]');
    const m = host && document.getElementById(host.dataset.ctx);
    if (!m) { closeMenu(); return; }
    m._host = host;
    m.dispatchEvent(new CustomEvent('ctx-open', { detail: host }));
    openMenuAt(m, e.clientX, e.clientY);
  });
  // Release builds: no reload / print / devtools shortcuts.
  document.addEventListener('keydown', (e) => {
    if (location.search.includes('dev')) return;
    const k = e.key.toLowerCase();
    if (k === 'f5' || k === 'f12' || ((e.ctrlKey || e.metaKey) && (k === 'r' || k === 'p' || (e.shiftKey && (k === 'i' || k === 'j'))))) e.preventDefault();
  });
  window.FUI = { $, $$, initSegs, toggleMenu, closeMenu, openMenuAt, openDlg, closeDlg, toast, progress, swapText, devCheck };
  document.addEventListener('DOMContentLoaded', () => initSegs());
})();