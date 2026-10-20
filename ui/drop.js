'use strict';
// Drag and drop from the file manager: a .mrpack anywhere imports a modpack,
// other files on an instance page go into its open content section.
(() => {
  const wv = window.__TAURI__?.webview?.getCurrentWebview?.();
  if (!wv?.onDragDropEvent) return;
  const SV = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">';
  const ICO = {
    pack: `${SV}<path d="M12 3 20 7.5v9L12 21l-8-4.5v-9z"/><path d="M4 7.5 12 12l8-4.5M12 12v9"/></svg>`,
    content: `${SV}<path d="M12 15V4M7.5 8.5 12 4l4.5 4.5"/><path d="M4 14v4.5A1.5 1.5 0 0 0 5.5 20h13a1.5 1.5 0 0 0 1.5-1.5V14"/></svg>`,
  };
  const WHAT = { mod: 'моды', resourcepack: 'ресурспаки', shader: 'шейдеры', datapack: 'датапаки' };
  const ov = el('div', { class: 'drop-ov', 'aria-hidden': 'true' });
  const ic = el('span', { class: 'drop-ic' }), t = el('b'), s = el('span', { class: 'drop-s' });
  ov.append(el('div', { class: 'drop-card' }, ic, t, s));
  document.body.append(ov);
  let mode = null, paths = [];

  const isPack = (p) => /\.mrpack$/i.test(p);
  function target(list) {
    if (list.some(isPack)) return 'pack';
    if (S.view === 'skins' && window.FSkins && list.some((p) => /\.png$/i.test(p)) && !document.querySelector('.dialog[data-open]')) return 'skin';
    if (S.view === 'instance' && S.current && window.FContent && !document.querySelector('.dialog[data-open]')) return 'content';
    return null;
  }
  function paint() {
    if (!mode) { ov.removeAttribute('data-on'); return; }
    ic.innerHTML = ICO[mode];
    if (mode === 'skin') { ic.innerHTML = ICO.content; t.textContent = 'Добавить скин'; s.textContent = paths.find((p) => /\.png$/i.test(p))?.split(/[\\/]/).pop() || ''; }
    else if (mode === 'pack') { t.textContent = 'Импортировать сборку'; s.textContent = paths.find(isPack)?.split(/[\\/]/).pop() || ''; }
    else {
      const k = window.FContent.kind?.() || 'mod';
      t.textContent = `Добавить ${WHAT[k] || 'файлы'}`;
      s.textContent = `в «${byId(S.current)?.name || 'сборку'}»`;
    }
    ov.setAttribute('data-on', '');
  }
  async function drop(list) {
    if (mode === 'skin') { await window.FSkins.drop(list.find((p) => /\.png$/i.test(p))); return; }
    if (mode === 'pack') { show('create'); window.flImportMrpackPath?.(list.find(isPack)); return; }
    if (mode !== 'content') return;
    const tab = $('#ip-tabs [data-tab="content"]');
    if (tab && tab.getAttribute('aria-selected') !== 'true') tab.click();
    await window.FContent.drop(list);
  }
  wv.onDragDropEvent((ev) => {
    const p = ev.payload || {};
    if (p.type === 'enter') { paths = p.paths || []; mode = target(paths); paint(); }
    else if (p.type === 'drop') { const list = p.paths || paths, m = mode; mode = null; paint(); if (m && list.length) { mode = m; drop(list).finally(() => { mode = null; }); } }
    else if (p.type === 'leave') { mode = null; paint(); }
  }).catch(() => {});
})();