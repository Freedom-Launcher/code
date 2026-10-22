'use strict';
// Freedom UI kit: custom dropdowns. Every native <select> stays in the DOM (value, change events and
// forms keep working) but is drawn as a launcher control with an animated listbox.
(() => {
  const NS = 'http://www.w3.org/2000/svg';
  const VALUE = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value');
  const INDEX = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'selectedIndex');
  let cur = null; // { sel, btn, list, hl, items, active }
  let uid = 0, typed = '', typedAt = 0;

  function icon(d) {
    const s = document.createElementNS(NS, 'svg'); s.setAttribute('viewBox', '0 0 24 24'); s.setAttribute('aria-hidden', 'true');
    s.setAttribute('class', 'mi'); s.setAttribute('fill', 'currentColor'); s.innerHTML = d;
    return s;
  }

  function enhance(sel) {
    if (sel._dd || sel.multiple) return;
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = `${sel.className} dd-btn`;
    btn.setAttribute('aria-haspopup', 'listbox');
    btn.setAttribute('aria-expanded', 'false');
    if (sel.getAttribute('aria-label')) btn.setAttribute('aria-label', sel.getAttribute('aria-label'));
    if (sel.dataset.id) btn.dataset.id = `${sel.dataset.id}.button`;
    const txt = document.createElement('span'); txt.className = 'dd-value';
    btn.append(txt, icon('<g fill="none" fill-rule="evenodd"><path d="M24 0v24H0V0h24ZM12.593 23.258l-.011.002-.071.035-.02.004-.014-.004-.071-.035c-.01-.004-.019-.001-.024.005l-.004.01-.017.428.005.02.01.013.104.074.015.004.012-.004.104-.074.012-.016.004-.017-.017-.427c-.002-.01-.009-.017-.017-.018Zm.265-.113-.013.002-.185.093-.01.01-.003.011.018.43.005.012.008.007.201.093c.012.004.023 0 .029-.008l.004-.014-.034-.614c-.003-.012-.01-.02-.02-.022Zm-.715.002a.023.023 0 0 0-.027.006l-.006.014-.034.614c0 .012.007.02.017.024l.015-.002.201-.093.01-.008.004-.011.017-.43-.003-.012-.01-.01-.184-.092Z"/><path fill="currentColor" d="M12.707 15.707a1 1 0 0 1-1.414 0L5.636 10.05A1 1 0 1 1 7.05 8.636l4.95 4.95 4.95-4.95a1 1 0 0 1 1.414 1.414l-5.657 5.657Z"/></g>'));
    sel.classList.add('dd-native'); sel.tabIndex = -1; sel.setAttribute('aria-hidden', 'true');
    sel.after(btn);
    const sync = () => {
      const o = sel.options[INDEX.get.call(sel)];
      txt.textContent = o ? o.textContent : '—';
      txt.classList.toggle('is-empty', !o);
      btn.hidden = sel.hidden;
      btn.disabled = sel.disabled;
      if (cur && cur.sel === sel) close(false);
    };
    Object.defineProperty(sel, 'value', { configurable: true, get() { return VALUE.get.call(this); }, set(v) { VALUE.set.call(this, v); sync(); } });
    Object.defineProperty(sel, 'selectedIndex', { configurable: true, get() { return INDEX.get.call(this); }, set(v) { INDEX.set.call(this, v); sync(); } });
    new MutationObserver(sync).observe(sel, { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: ['hidden', 'disabled', 'selected'] });
    sel.addEventListener('change', sync);
    sel.addEventListener('focus', () => btn.focus()); // <label for> and dialogs focus the select
    btn.addEventListener('click', () => {
      const kbd = !!btn._kbd; btn._kbd = false;
      if (cur && cur.sel === sel) close(true); else open(sel, btn, kbd);
    });
    btn.addEventListener('keydown', (e) => {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); open(sel, btn, true); }
    });
    sel._dd = { btn, sync };
    sync();
  }

  function open(sel, btn, kbd) {
    close(false);
    window.FUI?.closeMenu?.();
    const list = document.createElement('div');
    list.className = 'pop dd-pop'; list.id = `dd-list-${++uid}`;
    list.setAttribute('role', 'listbox'); list.tabIndex = -1;
    list.dataset.id = `${sel.dataset.id || 'select'}.list`;
    const hl = document.createElement('i'); hl.className = 'dd-hl is-instant'; hl.hidden = true;
    list.append(hl);
    const items = [];
    const add = (o) => {
      const it = document.createElement('div');
      it.className = 'dd-opt'; it.id = `dd-o-${++uid}`; it.setAttribute('role', 'option');
      it.setAttribute('aria-selected', String(o.selected));
      if (o.disabled) it.setAttribute('aria-disabled', 'true');
      const t = document.createElement('span'); t.textContent = o.textContent;
      it.append(t, icon('<g fill="none" fill-rule="evenodd"><path fill="currentColor" d="M21.192 5.465a1 1 0 0 1 0 1.414L9.95 18.122a1.1 1.1 0 0 1-1.556 0l-5.586-5.586a1 1 0 1 1 1.415-1.415l4.95 4.95L19.777 5.465a1 1 0 0 1 1.414 0Z"/></g>'));
      it._o = o; list.append(it); items.push(it);
    };
    for (const n of sel.children) {
      if (n.tagName === 'OPTGROUP') {
        const g = document.createElement('div'); g.className = 'dd-group'; g.textContent = n.label; list.append(g);
        [...n.children].forEach(add);
      } else if (n.tagName === 'OPTION') add(n);
    }
    if (kbd) list.setAttribute('data-instant', '');
    document.body.append(list);
    const r = btn.getBoundingClientRect();
    list.style.minWidth = `${r.width}px`;
    const below = innerHeight - r.bottom - 14, above = r.top - 14;
    const up = list.offsetHeight > below && above > below;
    list.style.maxHeight = `${Math.max(120, Math.min(340, up ? above : below))}px`;
    list.classList.toggle('is-up', up);
    list.style.left = `${Math.max(8, Math.min(r.left, innerWidth - list.offsetWidth - 8))}px`;
    list.style.top = `${up ? r.top - 6 - list.offsetHeight : r.bottom + 6}px`;
    list.style.setProperty('--origin', up ? 'bottom left' : 'top left');
    cur = { sel, btn, list, hl, items, active: -1 };
    btn.setAttribute('aria-expanded', 'true');
    btn.setAttribute('aria-controls', list.id);
    const i = items.findIndex((x) => x._o.selected);
    setActive(i < 0 ? step(-1, 1) : i, true);
    list.addEventListener('pointermove', (e) => {
      const it = e.target.closest('.dd-opt');
      if (it && cur && it._o && !it._o.disabled) setActive(cur.items.indexOf(it), false, true);
    });
    list.addEventListener('click', (e) => { const it = e.target.closest('.dd-opt'); if (it && cur) choose(cur.items.indexOf(it)); });
    list.addEventListener('keydown', onKey);
    list.focus({ preventScroll: true });
    requestAnimationFrame(() => { list.setAttribute('data-open', ''); requestAnimationFrame(() => hl.classList.remove('is-instant')); });
  }

  function step(from, d) {
    const c = cur; if (!c) return from;
    for (let i = from + d; i >= 0 && i < c.items.length; i += d) if (!c.items[i]._o.disabled) return i;
    return from;
  }

  function setActive(i, center, noScroll) {
    const c = cur; const it = c && c.items[i];
    if (!it) return;
    c.active = i;
    c.items.forEach((x) => x.classList.toggle('is-active', x === it));
    c.list.setAttribute('aria-activedescendant', it.id);
    c.hl.hidden = false;
    c.hl.style.height = `${it.offsetHeight}px`;
    c.hl.style.transform = `translateY(${it.offsetTop}px)`;
    if (noScroll) return;
    const L = c.list, pad = 4;
    if (center) L.scrollTop = it.offsetTop - L.clientHeight / 2 + it.offsetHeight / 2;
    else if (it.offsetTop < L.scrollTop + pad) L.scrollTop = it.offsetTop - pad;
    else if (it.offsetTop + it.offsetHeight > L.scrollTop + L.clientHeight - pad) L.scrollTop = it.offsetTop + it.offsetHeight - L.clientHeight + pad;
  }

  function choose(i) {
    const c = cur; const it = c && c.items[i];
    if (!it || it._o.disabled) return;
    const changed = !it._o.selected;
    close(true);
    if (!changed) return;
    it._o.selected = true;
    c.sel._dd.sync();
    c.sel.dispatchEvent(new Event('input', { bubbles: true }));
    c.sel.dispatchEvent(new Event('change', { bubbles: true }));
  }

  function onKey(e) {
    const c = cur; if (!c) return;
    const n = c.items.length;
    const k = e.key;
    if (k === 'ArrowDown' || k === 'ArrowUp') { e.preventDefault(); setActive(step(c.active, k === 'ArrowDown' ? 1 : -1)); }
    else if (k === 'Home' || k === 'End') { e.preventDefault(); setActive(k === 'Home' ? step(-1, 1) : step(n, -1)); }
    else if (k === 'PageDown' || k === 'PageUp') { e.preventDefault(); setActive(Math.max(0, Math.min(n - 1, c.active + (k === 'PageDown' ? 8 : -8)))); }
    else if (k === 'Enter' || (k === ' ' && !typed)) { e.preventDefault(); choose(c.active); }
    else if (k === 'Escape') { e.preventDefault(); e.stopPropagation(); close(true); }
    else if (k === 'Tab') close(true);
    else if (k.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
      const now = Date.now();
      typed = (now - typedAt > 700 ? '' : typed) + k.toLowerCase(); typedAt = now;
      const same = typed.length > 1 && [...typed].every((ch) => ch === typed[0]);
      const q = same ? typed[0] : typed;
      const start = same || typed.length === 1 ? c.active + 1 : c.active;
      for (let j = 0; j < n; j++) {
        const it = c.items[(start + j) % n];
        if (!it._o.disabled && it.textContent.trim().toLowerCase().startsWith(q)) { setActive(c.items.indexOf(it)); break; }
      }
      setTimeout(() => { if (Date.now() - typedAt >= 700) typed = ''; }, 720);
    }
  }

  function close(focus) {
    const c = cur; if (!c) return;
    cur = null;
    c.btn.setAttribute('aria-expanded', 'false');
    c.btn.removeAttribute('aria-controls');
    c.list.removeAttribute('data-open');
    setTimeout(() => c.list.remove(), c.list.hasAttribute('data-instant') ? 0 : 240);
    if (focus) c.btn.focus({ preventScroll: true });
  }

  document.addEventListener('pointerdown', (e) => { if (cur && !cur.list.contains(e.target) && !cur.btn.contains(e.target)) close(false); }, true);
  document.addEventListener('scroll', (e) => { if (cur && !cur.list.contains(e.target)) close(false); }, true);
  document.addEventListener('focusin', (e) => { if (cur && !cur.list.contains(e.target) && e.target !== cur.btn) close(false); });
  addEventListener('resize', () => close(false));

  document.querySelectorAll('select').forEach(enhance);
  new MutationObserver((ms) => {
    for (const m of ms) for (const n of m.addedNodes) {
      if (n.nodeType !== 1) continue;
      if (n.tagName === 'SELECT') enhance(n); else n.querySelectorAll?.('select').forEach(enhance);
    }
  }).observe(document.body, { childList: true, subtree: true });
  if (window.FUI) Object.assign(window.FUI, { select: enhance, closeSelect: () => close(false) });
})();