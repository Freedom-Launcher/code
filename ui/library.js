'use strict';
// Library (Э6): search, sort and grouping for «Все сборки», plus «Группа…» in the instance context menu.
// View choices are local to this computer; the group itself is stored on the instance (instances.update {group}).
(() => {
  const grid = $('#home-all');
  if (!grid) return;
  const q = $('#lib-q'), sortSel = $('#lib-sort'), bySel = $('#lib-by');
  const LS = { sort: 'fl.lib.sort', by: 'fl.lib.by', shut: 'fl.lib.shut' };
  const shut = new Set(JSON.parse(localStorage.getItem(LS.shut) || '[]'));
  // Pinned and hidden instances and the manual order are local to this computer, like the view choices.
  const rd = (k) => JSON.parse(localStorage.getItem(k) || '[]');
  const keep = (k, v) => localStorage.setItem(k, JSON.stringify([...v]));
  const pin = new Set(rd('fl.lib.pin')), hid = new Set(rd('fl.lib.hidden'));
  let order = rd('fl.lib.order'), showHid = false;
  const rank = (i) => { const k = order.indexOf(i.id); return k < 0 ? 1e9 : k; };
  const PIN_SVG = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M5.5 2.5h5M6.5 2.5v4L4 9.5h8L9.5 6.5v-4M8 9.5v4"/></svg>';
  sortSel.value = localStorage.getItem(LS.sort) || 'played';
  bySel.value = localStorage.getItem(LS.by) || 'group';
  const coll = new Intl.Collator('ru', { numeric: true, sensitivity: 'base' });
  const verKey = (v) => v.split(/[.-]/).map((x) => String(+x || 0).padStart(4, '0')).join('.');
  const SORT = {
    played: (a, b) => (b.last_played || 0) - (a.last_played || 0) || b.created_at - a.created_at,
    name: (a, b) => coll.compare(a.name, b.name),
    created: (a, b) => b.created_at - a.created_at,
    time: (a, b) => (b.playtime_secs || 0) - (a.playtime_secs || 0),
    manual: (a, b) => rank(a) - rank(b) || b.created_at - a.created_at,
  };
  const KEY = {
    group: (i) => i.group || '',
    loader: (i) => LOADER_NAMES[i.loader] || 'Vanilla',
    version: (i) => i.version,
  };

  function playBtn(i) {
    const on = isRunning(i.id), busy = S.playing === i.id;
    return el('span', {
      class: `hp${on ? ' on' : ''}${busy ? ' busy' : ''}`, role: 'button', title: on ? 'Остановить' : 'Играть',
      'aria-label': `${on ? 'Остановить' : 'Играть'}: ${i.name}`, 'data-id': 'home.card.play',
      onpointerdown: (e) => e.stopPropagation(),
      onclick: (e) => { e.stopPropagation(); e.preventDefault(); if (!busy) togglePlay(i.id, true); },
    }, svgIco(on ? STOP_D : PLAY_D));
  }

  function card(i) {
    return el('button', { class: `hcard${hid.has(i.id) ? ' is-hid' : ''}`, 'data-ctx': 'menu-ctx-instance', 'data-inst': i.id, 'data-id': `home.card.${i.id}`, onclick: () => openInstance(i.id) },
      bg(el('i', {}, playBtn(i)), coverName(i)), el('b', {}, i.name), el('span', {}, instMeta(i)),
      pin.has(i.id) ? el('em', { class: 'pin', 'aria-label': 'Закреплена', html: PIN_SVG }) : '');
  }

  function head(name, n) {
    const id = `${bySel.value}:${name}`;
    const h = el('button', { class: 'lib-gh', type: 'button', 'aria-expanded': String(!shut.has(id)), 'data-id': 'home.group' },
      svgIco('<path d="m6 4 4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>'),
      el('b', {}, name || 'Без группы'), el('span', {}, String(n)));
    h.addEventListener('click', () => {
      shut.has(id) ? shut.delete(id) : shut.add(id);
      localStorage.setItem(LS.shut, JSON.stringify([...shut]));
      render();
    });
    return [h, shut.has(id)];
  }

  function render() {
    const term = q.value.trim().toLocaleLowerCase('ru');
    const list = S.instances
      .filter((i) => !term || [i.name, i.version, LOADER_NAMES[i.loader] || 'vanilla', i.group || ''].some((s) => s.toLocaleLowerCase('ru').includes(term)))
      .filter((i) => term || showHid || !hid.has(i.id))
      .sort(SORT[sortSel.value] || SORT.played)
      .sort((a, b) => pin.has(b.id) - pin.has(a.id));
    const nh = S.instances.filter((i) => hid.has(i.id)).length, hb = $('#lib-hid');
    hb.hidden = !nh; hb.textContent = `Скрытые · ${nh}`; hb.setAttribute('aria-pressed', String(showHid));
    // Only real group sections switch the row to block layout; a flat list must stay a grid.
    grid.classList.remove('is-grouped');
    if (!list.length) return grid.replaceChildren(el('p', { class: 'lib-none' }, term ? 'Ничего не найдено' : ''));
    const key = KEY[bySel.value];
    if (!key) return grid.replaceChildren(...list.map(card));
    const groups = new Map();
    for (const i of list) { const k = key(i); groups.has(k) || groups.set(k, []); groups.get(k).push(i); }
    const names = [...groups.keys()].sort((a, b) => (!a) - (!b) || (bySel.value === 'version' ? coll.compare(verKey(b), verKey(a)) : coll.compare(a, b)));
    if (bySel.value === 'group' && names.length === 1 && !names[0]) return grid.replaceChildren(...list.map(card));
    grid.classList.add('is-grouped');
    grid.replaceChildren(...names.map((n) => {
      const [h, closed] = head(n, groups.get(n).length);
      return el('section', { class: `lib-g${closed ? ' is-shut' : ''}`, 'data-group': n }, h, el('div', { class: 'home-row' }, ...(closed ? [] : groups.get(n).map(card))));
    }));
  }

  q.addEventListener('input', render);
  q.addEventListener('keydown', (e) => { if (e.key === 'Escape' && q.value) { q.value = ''; render(); e.stopPropagation(); } });
  sortSel.addEventListener('change', () => { localStorage.setItem(LS.sort, sortSel.value); render(); });
  bySel.addEventListener('change', () => { localStorage.setItem(LS.by, bySel.value); render(); });

  // ---------- «Группа…» ----------
  const dlg = $('#dlg-group'), name = $('#grp-name'), chips = $('#grp-chips');
  let target = null;
  $('#cx-group').addEventListener('click', () => {
    FUI.closeMenu();
    target = byId(ctxInst());
    if (!target) return;
    name.value = target.group || '';
    const all = [...new Set(S.instances.map((i) => i.group).filter(Boolean))].sort(coll.compare);
    chips.replaceChildren(...all.map((g) => el('button', { type: 'button', class: `chip${g === target.group ? ' is-on' : ''}`, onclick: () => { name.value = g; name.focus(); } }, g)));
    chips.hidden = !all.length;
    $('#grp-clear').hidden = !target.group;
    FUI.openDlg('dlg-group');
  });
  async function save(value) {
    try {
      await run($('#grp-save'), 'instances.update', { instance: target.id, group: value });
      await reloadInstances();
      setTimeout(FUI.closeDlg, 200);
    } catch { /* toast shown */ }
  }
  dlg.addEventListener('submit', (ev) => { ev.preventDefault(); save(name.value.trim()); });
  $('#grp-clear').addEventListener('click', () => save(''));

  // ---------- Pin / hide ----------
  $('#menu-ctx-instance').addEventListener('ctx-open', (e) => {
    const id = e.detail.dataset.inst;
    $('#cx-pin').lastChild.textContent = pin.has(id) ? 'Открепить' : 'Закрепить';
    $('#cx-hide').lastChild.textContent = hid.has(id) ? 'Показать' : 'Скрыть';
  });
  const flip = (set, key, id) => { set.has(id) ? set.delete(id) : set.add(id); keep(key, set); render(); renderRail(); };
  $('#cx-pin').addEventListener('click', () => { FUI.closeMenu(); flip(pin, 'fl.lib.pin', ctxInst()); });
  $('#cx-hide').addEventListener('click', () => {
    FUI.closeMenu();
    const id = ctxInst(), was = hid.has(id);
    flip(hid, 'fl.lib.hidden', id);
    if (!was) toast('Сборка скрыта', { action: { label: 'Вернуть', run: () => flip(hid, 'fl.lib.hidden', id) } });
  });
  $('#lib-hid').addEventListener('click', () => { showHid = !showHid; render(); });

  // ---------- Drag: reorder (switches to «Вручную») and move between groups ----------
  // Pointer-driven, not HTML5 drag: the window's own file drop handling swallows HTML5 drags on Windows.
  let drag = null;
  const mark = (c, g, after) => {
    $$('#home-all .is-over').forEach((x) => { if (x !== c && x !== g) x.classList.remove('is-over', 'is-after'); });
    c?.classList.add('is-over'); c?.classList.toggle('is-after', !!after); g?.classList.add('is-over');
  };
  const stop = () => {
    const d = drag; drag = null;
    if (!d?.on) return null;
    d.g.remove(); d.c.classList.remove('is-dragging'); mark(null, null);
    return d;
  };
  grid.addEventListener('pointerdown', (e) => {
    const c = e.target.closest('.hcard');
    if (c && e.button === 0) drag = { c, x: e.clientX, y: e.clientY, on: false };
  });
  addEventListener('pointermove', (e) => {
    const d = drag;
    if (!d) return;
    if (!d.on) {
      if (Math.hypot(e.clientX - d.x, e.clientY - d.y) < 6) return;
      const r = d.c.getBoundingClientRect();
      Object.assign(d, { on: true, dx: d.x - r.left, dy: d.y - r.top, g: d.c.cloneNode(true) });
      d.g.classList.add('lib-ghost'); d.g.style.width = `${r.width}px`;
      document.body.append(d.g); d.c.classList.add('is-dragging');
    }
    d.g.style.transform = `translate(${e.clientX - d.dx}px, ${e.clientY - d.dy}px) rotate(-1.5deg)`;
    const at = document.elementFromPoint(e.clientX, e.clientY);
    d.over = at?.closest('#home-all .hcard:not(.is-dragging)') || null;
    d.sec = bySel.value === 'group' ? at?.closest('#home-all .lib-g') || null : null;
    d.after = d.over && e.clientX > d.over.getBoundingClientRect().left + d.over.offsetWidth / 2;
    mark(d.over, d.sec, d.after);
  });
  addEventListener('pointercancel', stop);
  addEventListener('keydown', (e) => { if (e.key === 'Escape' && drag?.on) stop(); });
  addEventListener('pointerup', async () => {
    const d = stop();
    if (!d) { drag = null; return; }
    // The click that ends a drag must not open the instance.
    const eat = (e) => { e.stopPropagation(); e.preventDefault(); };
    addEventListener('click', eat, true); setTimeout(() => removeEventListener('click', eat, true), 0);
    const id = d.c.dataset.inst, inst = byId(id);
    if (!inst) return;
    if (d.over) {
      const base = [...S.instances].sort(SORT[sortSel.value] || SORT.played).map((i) => i.id).filter((x) => x !== id);
      const k = base.indexOf(d.over.dataset.inst);
      base.splice(k + (d.after ? 1 : 0), 0, id);
      order = base; keep('fl.lib.order', order);
      if (sortSel.value !== 'manual') { sortSel.value = 'manual'; localStorage.setItem(LS.sort, 'manual'); }
    }
    const tg = (d.over?.closest('.lib-g') || d.sec)?.dataset.group;
    if (bySel.value === 'group' && tg != null && tg !== (inst.group || '')) {
      try { await call('instances.update', { instance: id, group: tg }); await reloadInstances(); return; } catch (e) { toast(String(e), { error: true }); }
    }
    render(); renderRail();
  });

  window.FLib = { render };
  if (S.instances.length && S.view === 'home') render();
})();