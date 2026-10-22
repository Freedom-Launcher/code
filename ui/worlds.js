// Worlds tab of an instance: the saves/ folder, last played first, like Modrinth App.
(() => {
  const { $, openDlg, closeDlg, toast } = FUI;
  const box = $('#tab-worlds');
  const FOLDER = '<svg class="pi" viewBox="0 0 256 256" fill="currentColor" aria-hidden="true"><path d="M208,88v24H69.77a8,8,0,0,0-7.59,5.47L32,208V64a8,8,0,0,1,8-8H93.33a8,8,0,0,1,4.8,1.6L128,80h72A8,8,0,0,1,208,88Z" class="duo"/><path d="M248.23,112.31A20,20,0,0,0,232,104H220V88a20,20,0,0,0-20-20H132L105.34,48a20.12,20.12,0,0,0-12-4H40A20,20,0,0,0,20,64V208a12,12,0,0,0,12,12H211.1a12,12,0,0,0,11.33-8l28.49-81.47.06-.17A20,20,0,0,0,248.23,112.31ZM92,68l28.8,21.6A12,12,0,0,0,128,92h68v12H69.77a20,20,0,0,0-18.94,13.58L44,137.15V68ZM202.59,196H48.89l23.72-68H226.37Z"/></svg>';
  const TRASH = '<svg class="pi" viewBox="0 0 256 256" fill="currentColor" aria-hidden="true"><path d="M200,56V208a8,8,0,0,1-8,8H64a8,8,0,0,1-8-8V56Z" class="duo"/><path d="M216,48H180V36A28,28,0,0,0,152,8H104A28,28,0,0,0,76,36V48H40a12,12,0,0,0,0,24h4V208a20,20,0,0,0,20,20H192a20,20,0,0,0,20-20V72h4a12,12,0,0,0,0-24ZM100,36a4,4,0,0,1,4-4h48a4,4,0,0,1,4,4V48H100Zm88,168H68V72H188ZM116,104v64a12,12,0,0,1-24,0V104a12,12,0,0,1,24,0Zm48,0v64a12,12,0,0,1-24,0V104a12,12,0,0,1,24,0Z"/></svg>';
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
