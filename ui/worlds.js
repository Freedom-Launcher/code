// Worlds tab of an instance: the saves/ folder, last played first, like Modrinth App.
(() => {
  const { $, openDlg, closeDlg, toast } = FUI;
  const box = $('#tab-worlds');
  const FOLDER = '<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><g fill="none"><path fill="currentColor" d="M9.52 3a2 2 0 0 1 1.442.614l.12.137L12.48 5.5H19a2 2 0 0 1 1.995 1.85L21 7.5v1.53a2.001 2.001 0 0 1 1.61 2.398l-.038.147-2.4 8a2 2 0 0 1-1.752 1.418l-.164.007H4a2 2 0 0 1-1.995-1.85L2 19V5a2 2 0 0 1 1.85-1.995L4 3zm11.136 8H6.744l-2.4 8h13.912zM9.519 5H4v8.187l.828-2.762A2 2 0 0 1 6.744 9H19V7.5h-6.52a2 2 0 0 1-1.561-.75z"/></g></svg>';
  const TRASH = '<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><g fill="none"><path fill="currentColor" d="M14.28 2a2 2 0 0 1 1.897 1.368L16.72 5H20a1 1 0 1 1 0 2l-.003.071-.867 12.143A3 3 0 0 1 16.138 22H7.862a3 3 0 0 1-2.992-2.786L4.003 7.07A1.01 1.01 0 0 1 4 7a1 1 0 0 1 0-2h3.28l.543-1.632A2 2 0 0 1 9.721 2zm3.717 5H6.003l.862 12.071a1 1 0 0 0 .997.929h8.276a1 1 0 0 0 .997-.929zM10 10a1 1 0 0 1 .993.883L11 11v5a1 1 0 0 1-1.993.117L9 16v-5a1 1 0 0 1 1-1m4 0a1 1 0 0 1 1 1v5a1 1 0 1 1-2 0v-5a1 1 0 0 1 1-1m.28-6H9.72l-.333 1h5.226z"/></g></svg>';
  const HISTORY = '<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><g fill="none"><path fill="currentColor" d="M2.907 6.75c2.9-5.022 9.321-6.743 14.343-3.843a10.502 10.502 0 0 1 4.915 11.732c-.318 1.225-1.81 1.57-2.658.746l-.095-.1-2.251-2.587c-1.003-1.152.024-2.89 1.478-2.628l.133.03.518.134a7.5 7.5 0 1 0-2.031 7.117 1.5 1.5 0 1 1 2.104 2.139c-3.281 3.228-8.43 4.018-12.613 1.603-5.022-2.9-6.743-9.32-3.843-14.343M12 5.5a1.5 1.5 0 0 1 1.493 1.355L13.5 7v4.379l2.06 2.06a1.5 1.5 0 0 1-2.007 2.225l-.114-.103-2.5-2.5a1.5 1.5 0 0 1-.432-.913L10.5 12V7A1.5 1.5 0 0 1 12 5.5"/></g></svg>';
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
    const meta = [w.last_played ? ago(w.last_played) : null, w.size ? fmtBytes(w.size) : null,
      w.backups ? `${w.backups} ${_tp(w.backups, 'копия', 'копии', 'копий')}` : null].filter(Boolean).join(' · ');
    const bk = el('button', { class: 'btn btn-icon btn-ghost', 'data-tip': _t("Резервные копии"), 'aria-label': _t("Резервные копии"), html: HISTORY });
    bk.addEventListener('click', () => openBk(w));
    const open = el('button', { class: 'btn btn-icon btn-ghost', 'data-tip': _t("Открыть папку"), 'aria-label': _t("Открыть папку"), html: FOLDER });
    open.addEventListener('click', () => invoke('open_world_folder', { instance: S.current, world: w.id }).catch((e) => toast(String(e), { error: true })));
    const del = el('button', { class: 'btn btn-icon btn-ghost w-del', 'data-tip': _t("Удалить"), 'aria-label': _t("Удалить"), html: TRASH });
    del.addEventListener('click', () => { pending = w; $('#wdel-name').textContent = w.name; openDlg('dlg-world-del'); });
    const go = qpOk(byId(S.current)?.version)
      ? el('button', { class: 'btn btn-sm btn-ghost w-play', 'data-tip': _t("Играть в этом мире"), html: PLAY_SVG }, el('span', { class: 'lbl' }, _t("Играть")))
      : null;
    go?.addEventListener('click', () => play(S.current, true, false, { world: w.id }));
    r.append(ico(w), el('div', { class: 't' }, el('b', {}, w.name), el('span', {}, meta)), go, bk, open, del);
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
      : el('div', { class: 'table-empty tall' }, el('b', {}, _t("Миров пока нет"))));
  }
  $('#ip-tabs').addEventListener('click', (e) => { if (e.target.closest('[data-tab="worlds"]')) paint(true); });
  $('#dlg-world-del').addEventListener('submit', async (ev) => {
    ev.preventDefault();
    const w = pending; if (!w) return;
    try {
      await run($('#wdel-submit'), 'worlds.delete', { instance: S.current, world: w.id });
      closeDlg(); toast(_t("Мир «{0}» удалён", [w.name]));
      paint(true);
    } catch { /* run() shows it */ } finally { $('#wdel-submit').dataset.state = 'idle'; }
  });
  // ---------- Backups dialog ----------
  const list = $('#wbk-list'), make = $('#wbk-make');
  let bk = null, bkSeq = 0;
  const when = (t) => {
    const d = new Date(t * 1000), n = new Date();
    const o = { day: 'numeric', month: 'long', hour: '2-digit', minute: '2-digit', ...(d.getFullYear() === n.getFullYear() ? {} : { year: 'numeric' }) };
    try { return d.toLocaleString(_LOC, o); } catch { return d.toLocaleString(); }
  };
  // Destructive buttons ask twice: the first press arms them for a moment.
  function armed(b, on) {
    if (on) { b.dataset.armed = ''; clearTimeout(b._arm); b._arm = setTimeout(() => armed(b, false), 2600); }
    else { delete b.dataset.armed; clearTimeout(b._arm); }
    if (b.dataset.tipOff) { const t = on ? b.dataset.tipOn : b.dataset.tipOff; b.dataset.tip = t; b.setAttribute('aria-label', t); }
  }
  const disarmAll = (except) => list.querySelectorAll('[data-armed]').forEach((b) => b !== except && armed(b, false));
  const refresh = () => { paintBk(); paint(true); };
  function bkRow(b) {
    const r = el('div', { class: 'mod-row wbk-row' });
    const rest = el('button', { type: 'button', class: 'btn btn-sm btn-ghost wbk-restore',
      html: `<span class="lbl"><span class="a"></span><span class="b"></span></span>` });
    rest.querySelector('.a').textContent = _t("Восстановить");
    rest.querySelector('.b').textContent = _t("Точно?");
    rest.addEventListener('click', async () => {
      if (rest.dataset.state === 'loading') return;
      if (!('armed' in rest.dataset)) { disarmAll(rest); armed(rest, true); return; }
      armed(rest, false);
      const w = bk;
      try {
        await run(rest, 'worlds.restore', { instance: w.inst, world: w.id, backup: b.id });
        toast(_t("Мир «{0}» восстановлен", [w.name]));
        refresh();
      } catch { rest.dataset.state = 'idle'; }
    });
    const tOff = _t("Удалить копию"), tOn = _t("Нажмите ещё раз, чтобы удалить");
    const del = el('button', { type: 'button', class: 'btn btn-icon btn-ghost wbk-del', 'data-tip': tOff, 'aria-label': tOff, 'data-tip-off': tOff, 'data-tip-on': tOn, html: TRASH });
    del.addEventListener('click', async () => {
      if (!('armed' in del.dataset)) { disarmAll(del); armed(del, true); return; }
      armed(del, false);
      try {
        await call('worlds.backup_delete', { instance: bk.inst, world: bk.id, backup: b.id });
        toast(_t("Копия удалена"));
        refresh();
      } catch (e) { toast(String(e), { error: true }); }
    });
    const meta = [ago(b.at), b.size ? fmtBytes(b.size) : null].filter(Boolean).join(' · ');
    r.append(el('i', { class: 'mod-ico wbk-ico', html: HISTORY }), el('div', { class: 't' }, el('b', {}, when(b.at), b.auto ? el('em', { class: 'badge-sm wbk-auto' }, _t("авто")) : null), el('span', {}, meta)), rest, del);
    return r;
  }
  async function paintBk() {
    const w = bk; if (!w) return;
    const my = ++bkSeq;
    const v = await call('worlds.backups', { instance: w.inst, world: w.id }).catch((e) => { toast(String(e), { error: true }); return []; });
    if (my !== bkSeq || bk !== w) return;
    list.replaceChildren(v.length
      ? el('div', { class: 'mod-list wbk-rows' }, ...v.map(bkRow))
      : el('div', { class: 'wbk-empty' }, el('i', { class: 'wbk-big', html: HISTORY }), el('b', {}, _t("Копий пока нет")),
        el('span', {}, _t("Создайте первую, чтобы можно было вернуть мир к этому моменту."))));
  }
  function openBk(w) {
    bk = { id: w.id, name: w.name, inst: S.current };
    $('#wbk-world').textContent = w.name;
    make.dataset.state = 'idle';
    list.replaceChildren(el('div', { class: 'mod-list wbk-rows w-skel' }, ...[0, 1].map(() => el('div', { class: 'mod-row wbk-row' },
      el('i', { class: 'mod-ico sk' }), el('div', { class: 't' }, el('b', { class: 'sk' }), el('span', { class: 'sk' }))))));
    openDlg('dlg-world-bk');
    paintBk();
  }
  $('#dlg-world-bk').addEventListener('submit', async (ev) => {
    ev.preventDefault();
    if (!bk || make.dataset.state === 'loading') return;
    try {
      await run(make, 'worlds.backup', { instance: bk.inst, world: bk.id });
      toast(_t("Копия создана"));
      refresh();
    } catch { make.dataset.state = 'idle'; }
  });
  $('#wbk-folder').addEventListener('click', () => bk && invoke('open_backups_folder', { instance: bk.inst }).catch((e) => toast(String(e), { error: true })));

  window.FWorlds = { paint, reset() { shown = null; paint(); } };
})();
