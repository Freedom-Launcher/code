// Worlds tab of an instance: the saves/ folder, last played first, like Modrinth App.
(() => {
  const { $, openDlg, closeDlg, toast } = FUI;
  const box = $('#tab-worlds');
  const FOLDER = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"><path d="M2 4.5c0-.8.7-1.5 1.5-1.5h2.8l1.4 1.5h4.8c.8 0 1.5.7 1.5 1.5v5.5c0 .8-.7 1.5-1.5 1.5h-9c-.8 0-1.5-.7-1.5-1.5z"/></svg>';
  const TRASH = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.6 8.1c.1.8.7 1.4 1.5 1.4h2.8c.8 0 1.4-.6 1.5-1.4l.6-8.1"/></svg>';
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
