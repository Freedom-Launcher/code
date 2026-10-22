// Worlds tab of an instance: the saves/ folder, last played first, like Modrinth App.
(() => {
  const { $, openDlg, closeDlg, toast } = FUI;
  const box = $('#tab-worlds');
  const FOLDER = '<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><g fill="none"><path fill="currentColor" d="M9.52 3a2 2 0 0 1 1.442.614l.12.137L12.48 5.5H19a2 2 0 0 1 1.995 1.85L21 7.5v1.53a2.001 2.001 0 0 1 1.61 2.398l-.038.147-2.4 8a2 2 0 0 1-1.752 1.418l-.164.007H4a2 2 0 0 1-1.995-1.85L2 19V5a2 2 0 0 1 1.85-1.995L4 3zm11.136 8H6.744l-2.4 8h13.912zM9.519 5H4v8.187l.828-2.762A2 2 0 0 1 6.744 9H19V7.5h-6.52a2 2 0 0 1-1.561-.75z"/></g></svg>';
  const TRASH = '<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><g fill="none"><path fill="currentColor" d="M14.28 2a2 2 0 0 1 1.897 1.368L16.72 5H20a1 1 0 1 1 0 2l-.003.071-.867 12.143A3 3 0 0 1 16.138 22H7.862a3 3 0 0 1-2.992-2.786L4.003 7.07A1.01 1.01 0 0 1 4 7a1 1 0 0 1 0-2h3.28l.543-1.632A2 2 0 0 1 9.721 2zm3.717 5H6.003l.862 12.071a1 1 0 0 0 .997.929h8.276a1 1 0 0 0 .997-.929zM10 10a1 1 0 0 1 .993.883L11 11v5a1 1 0 0 1-1.993.117L9 16v-5a1 1 0 0 1 1-1m4 0a1 1 0 0 1 1 1v5a1 1 0 1 1-2 0v-5a1 1 0 0 1 1-1m.28-6H9.72l-.333 1h5.226z"/></g></svg>';
  let shown = null, pending = null, seq = 0;

  const visible = () => !box.hidden && S.view === 'instance';
  function ico(w) {
    const i = el('i', { class: 'mod-ico w-ico' });
    if (w.icon) { const img = el('img', { alt: '', src: w.icon, draggable: false }); img.addEventListener('load', () => i.classList.add('is-img')); i.append(img); }
    else i.textContent = initials(w.name);
    return i;
  }
  function row(w, n) {
    const r = el('div', { class: 'mod-row w-row', 'data-id': `world.${w.id}` });
    r.style.setProperty('--i', String(Math.min(n, 12)));
    const meta = [w.last_played ? ago(w.last_played) : null, w.size ? fmtBytes(w.size) : null].filter(Boolean).join(' · ');
    const open = el('button', { class: 'btn btn-icon btn-ghost', 'data-tip': 'Открыть папку', 'aria-label': 'Открыть папку', html: FOLDER });
    open.addEventListener('click', () => invoke('open_world_folder', { instance: S.current, world: w.id }).catch((e) => toast(String(e), { error: true })));
    const del = el('button', { class: 'btn btn-icon btn-ghost w-del', 'data-tip': 'Удалить', 'aria-label': 'Удалить', html: TRASH });
    del.addEventListener('click', () => { pending = w; $('#wdel-name').textContent = w.name; openDlg('dlg-world-del'); });
    r.append(ico(w), el('div', { class: 't' }, el('b', {}, w.name), el('span', {}, meta)), open, del);
    return r;
  }
  async function paint(force = false) {
    if (!visible() || !S.current) return;
    if (!force && shown === S.current) return;
    const id = S.current, my = ++seq;
    if (shown !== id) box.replaceChildren(el('div', { class: 'mod-list w-skel' }, ...[0, 1, 2].map(() => el('div', { class: 'mod-row' }, el('i', { class: 'mod-ico sk' }), el('div', { class: 't' }, el('b', { class: 'sk' }), el('span', { class: 'sk' }))))));
    const ws = await call('instances.worlds', { instance: id }).catch(() => []);
    if (my !== seq) return;
    shown = id;
    box.replaceChildren(ws.length
      ? el('div', { class: 'mod-list w-list' }, ...ws.map(row))
      : el('div', { class: 'table-empty tall' }, el('b', {}, 'Миров пока нет')));
  }
  $('#ip-tabs').addEventListener('click', (e) => { if (e.target.closest('[data-tab="worlds"]')) paint(true); });
  $('#dlg-world-del').addEventListener('submit', async (ev) => {
    ev.preventDefault();
    const w = pending; if (!w) return;
    try {
      await run($('#wdel-submit'), 'worlds.delete', { instance: S.current, world: w.id });
      closeDlg(); toast(`Мир «${w.name}» удалён`);
      paint(true);
    } catch { /* run() shows it */ } finally { $('#wdel-submit').dataset.state = 'idle'; }
  });
  window.FWorlds = { paint, reset() { shown = null; paint(); } };
})();
