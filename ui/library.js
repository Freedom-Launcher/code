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
  // 0.3.9: the panel and the library hide separately; the old shared list now hides from the panel only.
  (() => { if (localStorage.getItem('fl.hid.v2')) return; const o = localStorage.getItem('fl.lib.hidden') || '[]'; localStorage.setItem('fl.rail.hidden', o); localStorage.setItem('fl.lib.hidden', '[]'); localStorage.setItem('fl.hid.v2', '1'); })();
  const pin = new Set(rd('fl.lib.pin')), hid = new Set(rd('fl.lib.hidden')), railHid = new Set(rd('fl.rail.hidden'));
  let order = rd('fl.lib.order'), showHid = false;
  const sel = new Set();
  // Pictures under groups, local to this computer like the view choices: {group: {id, v, mode, h, zoom, x, y}}.
  // The file itself is art kind `deco` under id `g-<hash of the group name>`.
  let deco = (() => { try { return JSON.parse(localStorage.getItem('fl.lib.deco') || '{}') || {}; } catch { return {}; } })();
  const keepDeco = () => localStorage.setItem('fl.lib.deco', JSON.stringify(deco));
  const DECO_DEF = { mode: 'banner', h: 160, zoom: 100, x: 50, y: 50 };
  const decoId = (g) => { let h = 0x811c9dc5; for (const b of new TextEncoder().encode(g)) { h ^= b; h = Math.imul(h, 0x01000193) >>> 0; } return `g-${h.toString(16).padStart(8, '0')}`; };
  const IMG_SVG = '<svg class="pi" viewBox="0 0 256 256" fill="currentColor" aria-hidden="true"><path d="M224,56V178.06l-39.72-39.72a8,8,0,0,0-11.31,0L147.31,164,97.66,114.34a8,8,0,0,0-11.32,0L32,168.69V56a8,8,0,0,1,8-8H216A8,8,0,0,1,224,56Z" class="duo"/><path d="M144,96a16,16,0,1,1,16,16A16,16,0,0,1,144,96Zm92-40V200a20,20,0,0,1-20,20H40a20,20,0,0,1-20-20V56A20,20,0,0,1,40,36H216A20,20,0,0,1,236,56ZM44,60v79.72l33.86-33.86a20,20,0,0,1,28.28,0L147.31,147l17.18-17.17a20,20,0,0,1,28.28,0L212,149.09V60Zm0,136H162.34L92,125.66l-48,48Zm168,0V183l-33.37-33.37L164.28,164l32,32Z"/></svg>';
  function decoEl(d) {
    const i = el('i');
    i.style.backgroundImage = `url("${ART}/${d.id}/deco?v=${d.v}")`;
    i.style.backgroundPosition = i.style.transformOrigin = `${d.x ?? 50}% ${d.y ?? 50}%`;
    i.style.transform = `scale(${(d.zoom ?? 100) / 100})`;
    const w = el('div', { class: 'lib-deco', 'aria-hidden': 'true' }, i);
    if (d.mode !== 'back') w.style.height = `${d.h ?? 160}px`;
    return w;
  }
  const rank = (i) => { const k = order.indexOf(i.id); return k < 0 ? 1e9 : k; };
  const PIN_SVG = '<svg class="pi" viewBox="0 0 256 256" fill="currentColor" aria-hidden="true"><path d="M229.66,98.34,172.39,155.8c11.46,22.93-1.72,45.86-10.11,57a8,8,0,0,1-12,.83L42.34,105.76A8,8,0,0,1,43,93.85c29.65-23.92,57.4-10,57.4-10l57.27-57.46a8,8,0,0,1,11.31,0L229.66,87A8,8,0,0,1,229.66,98.34Z" class="duo"/><path d="M238.15,78.54,177.46,17.86a20,20,0,0,0-28.3,0L97.2,70c-12.43-3.33-36.68-5.72-61.74,14.5a20,20,0,0,0-1.6,29.73l45.46,45.47-39.8,39.8a12,12,0,0,0,17,17l39.8-39.81,45.47,45.46A20,20,0,0,0,155.91,228c.46,0,.93,0,1.4-.05A20,20,0,0,0,171.87,220c4.69-6.23,11-16.13,14.44-28s3.45-22.88.16-33.4l51.7-51.87A20,20,0,0,0,238.15,78.54Zm-74.26,68.79a12,12,0,0,0-2.23,13.84c3.43,6.86,6.9,21-6.28,40.65L54.08,100.53c21.09-14.59,39.53-6.64,41-6a11.67,11.67,0,0,0,13.81-2.29l54.43-54.61,55,55Z"/></svg>';
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
    return el('button', { class: `hcard${hid.has(i.id) ? ' is-hid' : ''}${sel.has(i.id) ? ' is-sel' : ''}`, 'data-ctx': 'menu-ctx-instance', 'data-inst': i.id, 'data-id': `home.card.${i.id}`, onclick: () => openInstance(i.id) },
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
      // «Вручную» is exactly where the user put each card; other orders keep pinned ones first.
      .sort((a, b) => (sortSel.value === 'manual' ? 0 : pin.has(b.id) - pin.has(a.id)));
    for (const id of sel) if (!byId(id)) sel.delete(id);
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
      const own = bySel.value === 'group' && n, d = own && deco[n]?.v ? { ...DECO_DEF, ...deco[n] } : null;
      return el('section', { class: `lib-g${closed ? ' is-shut' : ''}${d ? ` has-deco is-${d.mode === 'back' ? 'back' : 'banner'}` : ''}`, 'data-group': n }, h,
        own ? el('button', { type: 'button', class: 'lib-gpic', 'data-tip': 'Картинка группы', 'aria-label': `Картинка группы «${n}»`, 'data-id': 'home.group.pic', html: IMG_SVG, onclick: () => openDeco(n) }) : null,
        d ? decoEl(d) : null,
        el('div', { class: 'home-row' }, ...(closed ? [] : groups.get(n).map(card))));
    }));
  }

  // ---------- Group picture: a band above the group or its background, with zoom and focus point ----------
  const DD = {};
  let dg = '';
  function decoDlg() {
    if (DD.f) return;
    const file = el('input', { type: 'file', accept: 'image/png,image/jpeg,image/webp,image/gif', hidden: true });
    const mk = (key, t, min, max, step) => {
      const r = el('input', { class: 'range', type: 'range', min, max, step, 'data-id': `dialog.library.deco.${key}` });
      const v = el('div', { class: 's' });
      r.addEventListener('input', () => setD({ [key]: +r.value }));
      r.addEventListener('dblclick', () => setD({ [key]: DECO_DEF[key] }));
      DD[key] = { r, v };
      return el('div', { class: 'r', 'data-k': key }, el('div', { class: 'grow' }, el('div', { class: 't' }, t), v), r);
    };
    DD.title = el('h2');
    DD.pv = el('div', { class: 'deco-pv' });
    DD.pick = el('button', { type: 'button', class: 'btn', 'data-id': 'dialog.library.deco.pick', onclick: () => file.click() }, el('span', { class: 'lbl' }, 'Выбрать картинку…'));
    DD.del = el('button', { type: 'button', class: 'btn btn-ghost', 'data-id': 'dialog.library.deco.remove', onclick: dropD }, 'Убрать картинку');
    DD.modes = el('div', { class: 'grp-chips deco-modes' }, ...[['banner', 'Полосой над группой'], ['back', 'Фоном группы']]
      .map(([m, t]) => el('button', { type: 'button', class: 'chip', 'data-m': m, onclick: () => setD({ mode: m }) }, t)));
    DD.rows = el('div', { class: 'rows deco-rows' }, mk('h', 'Высота полосы', 60, 420, 10), mk('zoom', 'Масштаб', 100, 300, 5), mk('x', 'Сдвиг по горизонтали', 0, 100, 1), mk('y', 'Сдвиг по вертикали', 0, 100, 1));
    file.addEventListener('change', async () => {
      const f = file.files?.[0]; file.value = '';
      if (!f) return;
      try {
        const id = decoId(dg);
        await invoke('set_art', new Uint8Array(await f.arrayBuffer()), { headers: { 'x-inst': id, 'x-kind': 'deco' } });
        setD({ id, v: Date.now() });
      } catch (err) { toast(String(err), { error: true }); }
    });
    DD.f = el('form', { class: 'dialog dlg-deco', id: 'dlg-deco', 'data-id': 'dialog.library.deco', role: 'dialog', 'aria-label': 'Картинка группы' },
      DD.title, DD.pv, el('div', { class: 'deco-pick' }, DD.pick, file), DD.modes, DD.rows,
      el('div', { class: 'actions' }, DD.del, el('span', { class: 'spacer' }), el('button', { type: 'button', class: 'btn btn-primary', 'data-close': '' }, el('span', { class: 'lbl' }, 'Готово'))));
    DD.f.addEventListener('submit', (e) => e.preventDefault());
    ($('#scrim') || document.body.lastChild).after(DD.f);
  }
  function paintD() {
    const d = { ...DECO_DEF, ...deco[dg] }, has = !!d.v;
    DD.title.textContent = `Картинка группы · ${dg}`;
    DD.pv.replaceChildren(has ? decoEl(d) : el('span', {}, 'Картинка не выбрана'));
    DD.pick.lastChild.textContent = has ? 'Заменить…' : 'Выбрать картинку…';
    DD.del.hidden = !has;
    for (const b of DD.modes.children) b.classList.toggle('is-on', b.dataset.m === d.mode);
    for (const k of ['h', 'zoom', 'x', 'y']) {
      DD[k].r.value = d[k];
      DD[k].v.textContent = k === 'h' ? `${d.h} px` : k === 'zoom' ? (d.zoom === 100 ? 'По размеру' : `${d.zoom}%`) : d[k] === 50 ? 'По центру' : `${d[k]}%`;
    }
    DD.rows.querySelector('[data-k="h"]').hidden = d.mode === 'back';
  }
  function setD(p) { deco[dg] = { ...DECO_DEF, ...deco[dg], ...p }; keepDeco(); paintD(); render(); }
  async function dropD() {
    const d = deco[dg];
    if (!d) return;
    if (d.id) await invoke('clear_art', { instance: d.id, kind: 'deco' }).catch(() => {});
    delete deco[dg]; keepDeco(); paintD(); render();
  }
  function openDeco(n) { dg = n; decoDlg(); paintD(); FUI.openDlg('dlg-deco'); }

  q.addEventListener('input', render);
  q.addEventListener('keydown', (e) => { if (e.key === 'Escape' && q.value) { q.value = ''; render(); e.stopPropagation(); } });
  sortSel.addEventListener('change', () => { localStorage.setItem(LS.sort, sortSel.value); render(); });
  bySel.addEventListener('change', () => { localStorage.setItem(LS.by, bySel.value); render(); });

  // ---------- «Группа…» ----------
  const dlg = $('#dlg-group'), name = $('#grp-name'), chips = $('#grp-chips');
  let target = null, targets = [];
  $('#cx-group').addEventListener('click', () => {
    FUI.closeMenu();
    target = byId(ctxInst());
    if (!target) return;
    targets = sel.has(target.id) && sel.size > 1 ? [...sel].filter((x) => byId(x)) : [target.id];
    $('#dlg-group h2').textContent = targets.length > 1 ? `Группа · ${targets.length} сборки` : 'Группа';
    name.value = target.group || '';
    const all = [...new Set(S.instances.map((i) => i.group).filter(Boolean))].sort(coll.compare);
    chips.replaceChildren(...all.map((g) => el('button', { type: 'button', class: `chip${g === target.group ? ' is-on' : ''}`, onclick: () => { name.value = g; name.focus(); } }, g)));
    chips.hidden = !all.length;
    $('#grp-clear').hidden = !targets.some((x) => byId(x)?.group);
    FUI.openDlg('dlg-group');
  });
  async function save(value) {
    try {
      const [first, ...rest] = targets.length ? targets : [target.id];
      await run($('#grp-save'), 'instances.update', { instance: first, group: value });
      for (const x of rest) await call('instances.update', { instance: x, group: value });
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
    $('#cx-hide').lastChild.textContent = hid.has(id) ? 'Показать в библиотеке' : 'Скрыть в библиотеке';
    $('#cx-rail').lastChild.textContent = railHid.has(id) ? 'Вернуть на панель' : 'Скрыть с панели';
  });
  const flip = (set, key, id) => { set.has(id) ? set.delete(id) : set.add(id); keep(key, set); render(); renderRail(); };
  $('#cx-pin').addEventListener('click', () => { FUI.closeMenu(); flip(pin, 'fl.lib.pin', ctxInst()); });
  $('#cx-hide').addEventListener('click', () => {
    FUI.closeMenu();
    const id = ctxInst(), was = hid.has(id);
    flip(hid, 'fl.lib.hidden', id);
    if (!was) toast('Скрыта в библиотеке', { action: { label: 'Вернуть', run: () => flip(hid, 'fl.lib.hidden', id) } });
  });
  $('#lib-hid').addEventListener('click', () => { showHid = !showHid; render(); });
  $('#cx-rail').addEventListener('click', () => {
    FUI.closeMenu();
    const id = ctxInst(), was = railHid.has(id);
    const tog = () => { railHid.has(id) ? railHid.delete(id) : railHid.add(id); keep('fl.rail.hidden', railHid); renderRail(); };
    tog();
    if (!was) toast('Скрыта с панели', { action: { label: 'Вернуть', run: tog } });
  });

  // ---------- Selection: Ctrl/Shift-click, box select with the left button (like Explorer / Blender) ----------
  const cards = () => $$('#home-all .hcard');
  const paintSel = () => { for (const c of cards()) c.classList.toggle('is-sel', sel.has(c.dataset.inst)); };
  const clearSel = () => { if (sel.size) { sel.clear(); paintSel(); } };
  const editing = () => document.documentElement.hasAttribute('data-ui-edit');
  let anchor = null;
  // Modifier clicks select instead of opening the instance (capture runs before the card's own onclick).
  grid.addEventListener('click', (e) => {
    const c = e.target.closest('.hcard');
    if (!c || e.target.closest('.hp')) return;
    const id = c.dataset.inst;
    if (e.shiftKey && anchor) {
      e.preventDefault(); e.stopPropagation();
      const ids = cards().map((x) => x.dataset.inst), i = ids.indexOf(anchor), j = ids.indexOf(id);
      if (!(e.ctrlKey || e.metaKey)) sel.clear();
      if (i < 0) sel.add(id); else ids.slice(Math.min(i, j), Math.max(i, j) + 1).forEach((x) => sel.add(x));
      paintSel();
    } else if (e.ctrlKey || e.metaKey || e.shiftKey) {
      e.preventDefault(); e.stopPropagation();
      sel.has(id) ? sel.delete(id) : sel.add(id); anchor = id; paintSel();
    } else clearSel();
  }, true);
  $('#menu-ctx-instance').addEventListener('ctx-open', (e) => { if (!sel.has(e.detail.dataset.inst)) clearSel(); });
  addEventListener('keydown', (e) => {
    if (S.view !== 'home' || e.target.matches?.('input, textarea, select, [contenteditable]') || document.querySelector('.dialog[data-open], .pop[data-open], .palette[data-open]')) return;
    if ((e.ctrlKey || e.metaKey) && e.code === 'KeyA') { e.preventDefault(); cards().forEach((c) => sel.add(c.dataset.inst)); paintSel(); }
    else if (e.key === 'Escape' && sel.size && !drag?.on) clearSel();
  });
  // The box starts on empty space anywhere on the library screen, not on cards or controls.
  const home = grid.closest('.view') || grid.parentElement;
  const NOT_BOX = '.hcard, .rc, button, input, select, textarea, a, label, [role="button"], .dd-btn, .lib-gh, [data-slot] *';
  let box = null;
  home.addEventListener('pointerdown', (e) => {
    if (e.button !== 0 || editing() || e.target.closest(NOT_BOX)) return;
    const add = e.ctrlKey || e.metaKey || e.shiftKey;
    box = { x: e.clientX, y: e.clientY + home.scrollTop, cx: e.clientX, cy: e.clientY, add, base: add ? new Set(sel) : new Set(), on: false, n: null };
  });
  function boxPaint() {
    const b = box, top = home.scrollTop;
    const L = Math.min(b.x, b.cx), R = Math.max(b.x, b.cx), T = Math.min(b.y - top, b.cy), B = Math.max(b.y - top, b.cy);
    const vr = home.getBoundingClientRect(), t2 = Math.max(T, vr.top), b2 = Math.min(B, vr.bottom);
    Object.assign(b.n.style, { transform: `translate(${L}px, ${t2}px)`, width: `${R - L}px`, height: `${Math.max(0, b2 - t2)}px` });
    sel.clear(); b.base.forEach((x) => sel.add(x));
    for (const c of cards()) {
      const r = c.getBoundingClientRect();
      if (r.right > L && r.left < R && r.bottom > T && r.top < B) sel.add(c.dataset.inst);
    }
    paintSel();
  }
  function boxTick() {
    const b = box; if (!b?.on) return;
    const vr = home.getBoundingClientRect(), edge = 48;
    const v = b.cy > vr.bottom - edge ? (b.cy - vr.bottom + edge) / 3 : b.cy < vr.top + edge ? -(vr.top + edge - b.cy) / 3 : 0;
    if (v) { home.scrollTop += Math.max(-24, Math.min(24, v)); boxPaint(); }
    requestAnimationFrame(boxTick);
  }
  addEventListener('pointermove', (e) => {
    const b = box; if (!b) return;
    b.cx = e.clientX; b.cy = e.clientY;
    if (!b.on) {
      if (Math.hypot(e.clientX - b.x, e.clientY + home.scrollTop - b.y) < 5) return;
      b.on = true; b.n = el('div', { class: 'lib-marq' }); document.body.append(b.n);
      document.documentElement.classList.add('is-marq'); getSelection()?.removeAllRanges();
      requestAnimationFrame(boxTick);
    }
    boxPaint();
  });
  const boxEnd = () => {
    const b = box; box = null; if (!b) return;
    if (!b.on) { if (!b.add) clearSel(); return; }
    b.n.remove(); document.documentElement.classList.remove('is-marq');
    const eat = (e) => { e.stopPropagation(); e.preventDefault(); };
    addEventListener('click', eat, true); setTimeout(() => removeEventListener('click', eat, true), 0);
  };
  addEventListener('pointerup', boxEnd); addEventListener('pointercancel', boxEnd); addEventListener('blur', boxEnd);

  // ---------- Drag: reorder (switches to «Вручную») and move between groups; a selection moves together ----------
  // Pointer-driven, not HTML5 drag: the window's own file drop handling swallows HTML5 drags on Windows.
  let drag = null;
  const mark = (c, g, after) => {
    $$('#home-all .is-over').forEach((x) => { if (x !== c && x !== g) x.classList.remove('is-over', 'is-after'); });
    c?.classList.add('is-over'); c?.classList.toggle('is-after', !!after); g?.classList.add('is-over');
  };
  const stop = () => {
    const d = drag; drag = null;
    if (!d?.on) return null;
    d.g.remove(); d.moving.forEach((c) => c.classList.remove('is-dragging')); mark(null, null);
    return d;
  };
  // Dropping between cards, past the end of a row or in an empty part of a group used to do nothing,
  // so the card jumped back. Now the nearest card (same row first) decides the place.
  function nearest(scope, x, y) {
    let best = null, bd = Infinity;
    for (const c of scope.querySelectorAll('.hcard:not(.is-dragging)')) {
      const r = c.getBoundingClientRect();
      const dx = x < r.left ? r.left - x : x > r.right ? x - r.right : 0, dy = y < r.top ? r.top - y : y > r.bottom ? y - r.bottom : 0;
      const dist = dy * 4 + dx;
      if (dist < bd) { bd = dist; best = c; }
    }
    return best;
  }
  const afterOf = (c, x, y) => { const r = c.getBoundingClientRect(); return y > r.bottom ? true : y < r.top ? false : x > r.left + r.width / 2; };
  function dropAt(x, y) {
    const at = document.elementFromPoint(x, y);
    const sec = bySel.value === 'group' ? at?.closest('#home-all .lib-g') || null : null;
    let over = at?.closest('#home-all .hcard:not(.is-dragging)') || null;
    if (!over) {
      const r = grid.getBoundingClientRect();
      const near = x > r.left - 48 && x < r.right + 48 && y > r.top - 48 && y < r.bottom + 72;
      const scope = sec || (near ? grid : null);
      if (scope) over = nearest(scope, x, y);
      if (over && sec && !sec.contains(over)) over = null;
    }
    return { over, sec, after: over ? afterOf(over, x, y) : false };
  }
  // The order the user sees (pinned first unless the order is manual), over every instance.
  const shown = () => {
    const l = [...S.instances].sort(SORT[sortSel.value] || SORT.played);
    if (sortSel.value !== 'manual') l.sort((a, b) => pin.has(b.id) - pin.has(a.id));
    return l.map((i) => i.id);
  };
  grid.addEventListener('pointerdown', (e) => {
    const c = e.target.closest('.hcard');
    if (c && e.button === 0 && !e.ctrlKey && !e.metaKey && !e.shiftKey && !editing()) drag = { c, x: e.clientX, y: e.clientY, on: false };
  });
  addEventListener('pointermove', (e) => {
    const d = drag;
    if (!d) return;
    if (!d.on) {
      if (Math.hypot(e.clientX - d.x, e.clientY - d.y) < 6) return;
      const id = d.c.dataset.inst;
      if (!sel.has(id)) clearSel();
      d.ids = sel.size > 1 ? cards().map((x) => x.dataset.inst).filter((x) => sel.has(x)) : [id];
      d.moving = cards().filter((x) => d.ids.includes(x.dataset.inst));
      const r = d.c.getBoundingClientRect();
      Object.assign(d, { on: true, dx: d.x - r.left, dy: d.y - r.top, g: d.c.cloneNode(true) });
      d.g.classList.add('lib-ghost'); d.g.classList.remove('is-sel'); d.g.style.width = `${r.width}px`;
      if (d.ids.length > 1) d.g.dataset.n = String(d.ids.length);
      document.body.append(d.g); d.moving.forEach((c) => c.classList.add('is-dragging'));
    }
    d.g.style.transform = `translate(${e.clientX - d.dx}px, ${e.clientY - d.dy}px) rotate(-1.5deg)`;
    Object.assign(d, dropAt(e.clientX, e.clientY));
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
    const ids = d.ids.filter((x) => byId(x));
    if (!ids.length) return;
    if (d.over) {
      const base = shown().filter((x) => !ids.includes(x)), k = base.indexOf(d.over.dataset.inst);
      base.splice(k < 0 ? base.length : k + (d.after ? 1 : 0), 0, ...ids);
      order = base; keep('fl.lib.order', order);
      if (sortSel.value !== 'manual') { sortSel.value = 'manual'; localStorage.setItem(LS.sort, 'manual'); }
    }
    const tg = (d.over?.closest('.lib-g') || d.sec)?.dataset.group;
    const moved = bySel.value === 'group' && tg != null ? ids.filter((x) => (byId(x).group || '') !== tg) : [];
    if (moved.length) {
      try {
        for (const x of moved) await call('instances.update', { instance: x, group: tg });
        await reloadInstances();
        if (moved.length > 1) toast(`${moved.length} сборки перенесены в «${tg || 'Без группы'}»`);
        return;
      } catch (e) { toast(String(e), { error: true }); }
    }
    render(); renderRail();
  });

  window.FLib = { render };
  if (S.instances.length && S.view === 'home') render();
})();