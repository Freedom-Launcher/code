'use strict';
// Navigation history and the path in the title bar, as in Modrinth App:
// ← → step through views and tabs (also Alt+←/→ and the mouse side buttons),
// the crumbs show where you are and jump to any parent.
(() => {
  const past = [], next = [];
  let cur = null, replaying = false, shown = [];
  const sel = (root) => $(`${root} [aria-selected="true"]`);
  const label = (root, attr, v) => $(`${root} [data-${attr}="${v}"]`)?.textContent.trim() || v;
  let proj = null; // { id, kind, title, icon } of the open project page

  function snap() {
    const v = S.view || 'home';
    const e = { view: v };
    if (v === 'instance') { e.inst = S.current; e.sub = sel('#ip-tabs')?.dataset.tab || 'content'; }
    if (v === 'settings') e.sub = sel('#set-seg')?.dataset.pane || 'look';
    if (v === 'discover') { e.sub = sel('#disc-kind')?.dataset.kind || 'modpack'; const sc = window.FMods?.scopeOf?.(); if (sc) e.inst = sc; }
    if (v === 'project' && proj) { e.proj = { ...proj }; e.sub = proj.id; }
    return e;
  }
  const key = (e) => e && [e.view, e.inst || '', e.sub || ''].join('|');

  /** Records the current place; called after every view or tab change. */
  function note() {
    const e = snap();
    if (!replaying && cur && key(cur) !== key(e)) {
      past.push(cur);
      if (past.length > 60) past.shift();
      next.length = 0;
    }
    cur = e;
    paint();
  }

  async function go(e) {
    replaying = true;
    try {
      if (e.view === 'instance' && byId(e.inst)) {
        if (S.view !== 'instance' || S.current !== e.inst) openInstance(e.inst);
        const t = $(`#ip-tabs [data-tab="${e.sub}"]`);
        if (t && t.getAttribute('aria-selected') !== 'true') t.click();
      } else if (e.view === 'discover' && e.inst && byId(e.inst)) {
        window.FMods?.scoped(e.inst, e.sub);
      } else if (e.view === 'project' && e.proj) {
        await window.FProject?.open(e.proj.id, e.proj.kind, { project_id: e.proj.id, title: e.proj.title, icon_url: e.proj.icon });
      } else {
        show(byId(e.inst) || e.view !== 'instance' ? e.view : 'home');
        if (e.view === 'settings') window.FSettings?.open(e.sub);
        const b = e.view === 'discover' ? $(`#disc-kind [data-kind="${e.sub}"]`) : null;
        if (b && b.getAttribute('aria-selected') !== 'true') b.click();
      }
    } finally { replaying = false; }
    cur = snap();
    paint();
  }
  function back() { if (!past.length) return false; next.push(cur); go(past.pop()); return true; }
  function fwd() { if (!next.length) return false; past.push(cur); go(next.pop()); return true; }

  // ---------- Crumbs ----------
  const home = () => (S.instances.length ? show('home') : show('empty'));
  const toKind = (k) => () => { show('discover'); const b = $(`#disc-kind [data-kind="${k}"]`); if (b?.getAttribute('aria-selected') !== 'true') b?.click(); };
  function crumbs(e) {
    const lib = { t: 'Библиотека', go: home };
    switch (e.view) {
      case 'create': return [lib, { t: 'Новая сборка' }];
      case 'instance': {
        const i = byId(e.inst);
        if (!i) return [lib];
        const ico = bg(el('i', { class: 'cr-ico', 'aria-hidden': 'true' }), iconName(i));
        const out = [lib, { t: i.name, ico, go: () => go({ view: 'instance', inst: i.id, sub: 'content' }) }];
        if (e.sub && e.sub !== 'content') out.push({ t: label('#ip-tabs', 'tab', e.sub) });
        return out;
      }
      case 'discover': {
        const i = e.inst && byId(e.inst);
        if (!i) return [{ t: 'Каталог', go: toKind('modpack') }, { t: label('#disc-kind', 'kind', e.sub) }];
        const ico = bg(el('i', { class: 'cr-ico', 'aria-hidden': 'true' }), iconName(i));
        return [lib, { t: i.name, ico, go: () => openInstance(i.id) }, { t: `Добавить · ${label('#disc-kind', 'kind', e.sub)}` }];
      }
      case 'project': {
        const p = e.proj || {};
        const ico = window.FMods?.icon(p.icon, p.title || '');
        ico?.classList.add('cr-ico');
        return [{ t: 'Каталог', go: toKind('modpack') }, { t: label('#disc-kind', 'kind', p.kind || 'mod'), go: toKind(p.kind || 'mod') }, { t: p.title || '…', ico }];
      }
      case 'settings': return [{ t: 'Настройки', go: () => go({ view: 'settings', sub: 'look' }) }, { t: label('#set-seg', 'pane', e.sub) }];
      case 'accounts': return [{ t: 'Аккаунты' }];
      case 'skins': return [{ t: 'Скины' }];
      default: return [lib];
    }
  }
  const CHEV = '<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><g fill="none" fill-rule="evenodd"><path d="M24 0v24H0V0h24ZM12.593 23.258l-.011.002-.071.035-.02.004-.014-.004-.071-.035c-.01-.004-.019-.001-.024.005l-.004.01-.017.428.005.02.01.013.104.074.015.004.012-.004.104-.074.012-.016.004-.017-.017-.427c-.002-.01-.009-.017-.017-.018Zm.265-.113-.013.002-.185.093-.01.01-.003.011.018.43.005.012.008.007.201.093c.012.004.023 0 .029-.008l.004-.014-.034-.614c-.003-.012-.01-.02-.02-.022Zm-.715.002a.023.023 0 0 0-.027.006l-.006.014-.034.614c0 .012.007.02.017.024l.015-.002.201-.093.01-.008.004-.011.017-.43-.003-.012-.01-.01-.184-.092Z"/><path fill="currentColor" d="M15.707 11.293a1 1 0 0 1 0 1.414l-5.657 5.657a1 1 0 1 1-1.414-1.414l4.95-4.95-4.95-4.95a1 1 0 0 1 1.414-1.414l5.657 5.657Z"/></g></svg>';
  function paint() {
    $('#nav-back').disabled = !past.length;
    $('#nav-fwd').disabled = !next.length;
    if (!cur) return;
    const list = crumbs(cur), box = $('#crumbs'), nodes = [];
    list.forEach((c, i) => {
      const last = i === list.length - 1;
      const n = last || !c.go
        ? el('span', { class: 'cr', 'data-tauri-drag-region': '', ...(last ? { 'aria-current': 'page' } : {}) })
        : el('button', { type: 'button', class: 'cr', onclick: c.go });
      if (c.ico) n.append(c.ico);
      n.append(el('span', { class: 'cr-t' }, c.t));
      if (shown[i] !== c.t) n.classList.add('is-new');
      nodes.push(n);
      if (!last) nodes.push(el('i', { class: 'cr-sep', 'aria-hidden': 'true', 'data-tauri-drag-region': '', html: CHEV }));
    });
    shown = list.map((c) => c.t);
    box.replaceChildren(...nodes);
  }

  $('#nav-back').addEventListener('click', back);
  $('#nav-fwd').addEventListener('click', fwd);
  document.addEventListener('keydown', (e) => {
    if (!e.altKey || e.ctrlKey || e.shiftKey || e.metaKey) return;
    if (e.key === 'ArrowLeft' && back()) e.preventDefault();
    if (e.key === 'ArrowRight' && fwd()) e.preventDefault();
  });
  // Mouse side buttons (3 = back, 4 = forward); suppress the webview's own handling.
  document.addEventListener('mouseup', (e) => {
    if (e.button === 3) { e.preventDefault(); back(); }
    if (e.button === 4) { e.preventDefault(); fwd(); }
  });
  document.addEventListener('mousedown', (e) => { if (e.button === 3 || e.button === 4) e.preventDefault(); });

  window.FNav = { note, back, fwd, canBack: () => past.length > 0, project: (p) => { proj = p; } };
  note();
})();