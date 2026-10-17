'use strict';
// Library (Э6): search, sort and grouping for «Все сборки», plus «Группа…» in the instance context menu.
// View choices are local to this computer; the group itself is stored on the instance (instances.update {group}).
(() => {
  const grid = $('#home-all');
  if (!grid) return;
  const q = $('#lib-q'), sortSel = $('#lib-sort'), bySel = $('#lib-by');
  const LS = { sort: 'fl.lib.sort', by: 'fl.lib.by', shut: 'fl.lib.shut' };
  const shut = new Set(JSON.parse(localStorage.getItem(LS.shut) || '[]'));
  sortSel.value = localStorage.getItem(LS.sort) || 'played';
  bySel.value = localStorage.getItem(LS.by) || 'group';
  const coll = new Intl.Collator('ru', { numeric: true, sensitivity: 'base' });
  const verKey = (v) => v.split(/[.-]/).map((x) => String(+x || 0).padStart(4, '0')).join('.');
  const SORT = {
    played: (a, b) => (b.last_played || 0) - (a.last_played || 0) || b.created_at - a.created_at,
    name: (a, b) => coll.compare(a.name, b.name),
    created: (a, b) => b.created_at - a.created_at,
    time: (a, b) => (b.playtime_secs || 0) - (a.playtime_secs || 0),
  };
  const KEY = {
    group: (i) => i.group || '',
    loader: (i) => LOADER_NAMES[i.loader] || 'Vanilla',
    version: (i) => i.version,
  };

  function card(i) {
    return el('button', { class: 'hcard', 'data-ctx': 'menu-ctx-instance', 'data-inst': i.id, 'data-id': `home.card.${i.id}`, onclick: () => openInstance(i.id) },
      bg(el('i'), coverName(i)), el('b', {}, i.name), el('span', {}, instMeta(i)));
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
      .sort(SORT[sortSel.value] || SORT.played);
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
      return el('section', { class: `lib-g${closed ? ' is-shut' : ''}` }, h, el('div', { class: 'home-row' }, ...(closed ? [] : groups.get(n).map(card))));
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

  window.FLib = { render };
  if (S.instances.length && S.view === 'home') render();
})();