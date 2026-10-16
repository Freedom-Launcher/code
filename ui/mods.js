// Mods: the instance «Моды» tab and the Modrinth browser in «Моды».
// Uses the same actions an addon would: mods.list / search / install / toggle / remove.
(() => {
  const modded = (i) => !!i && !!i.loader && i.loader !== 'vanilla';
  const fmtN = (n) => (n >= 1e6 ? `${(n / 1e6).toFixed(n >= 1e7 ? 0 : 1)} млн` : n >= 1e3 ? `${Math.round(n / 1e3)} тыс` : String(n));
  const CDN = /^https:\/\/cdn\.modrinth\.com\/[^"'\\()\s]+$/;
  const TRASH = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.6 8.5h5.8l.6-8.5"/></svg>';
  const fail = (e) => toast(String(e), { error: true });
  const empty = (title, ...rest) => el('div', { class: 'table-empty' }, el('b', {}, title), ...rest);

  function icon(url, title) {
    const n = el('i', { class: 'mod-ico', 'aria-hidden': 'true' });
    if (url && CDN.test(url)) n.style.backgroundImage = `url("${url}")`;
    else n.textContent = initials(title);
    return n;
  }

  // ---------- Instance tab ----------
  let inst = null;
  let installed = [];
  let updates = new Map();

  async function instance(i) {
    inst = i?.id || null;
    installed = [];
    updates = new Map();
    paintAll();
    $('#mods-filter').value = '';
    $('#mods-tools').hidden = !modded(i);
    paintList();
    if (!modded(i)) return;
    try { installed = await call('mods.list', { instance: i.id }); } catch (e) { fail(e); }
    if (inst !== i.id) return;
    paintList();
    // Network check after the list is on screen; silent when offline.
    const u = await call('mods.updates', { instance: i.id }).catch(() => []);
    if (inst !== i.id) return;
    updates = new Map(u.map((x) => [x.file, x]));
    paintList();
    paintAll();
  }

  function paintAll() {
    const b = $('#mods-update-all');
    b.hidden = !updates.size;
    b.querySelector('.lbl').textContent = `Обновить все · ${updates.size}`;
  }

  async function update(btn, file) {
    const r = await run(btn, 'mods.update', file ? { instance: inst, file } : { instance: inst }).catch(() => null);
    if (!r) return;
    const n = r.updated.length;
    toast(n === 0 ? 'Обновлений нет' : n === 1 ? `${r.updated[0]} обновлён` : `Обновлено модов: ${n}`);
    instance(byId(inst));
  }
  $('#mods-update-all').addEventListener('click', (e) => update(e.currentTarget));

  function paintList() {
    const box = $('#mods-list');
    if (!modded(byId(inst))) {
      box.replaceChildren(empty('Моды недоступны в Vanilla', 'Нужна сборка с Fabric, Quilt, Forge или NeoForge'));
      return;
    }
    const q = $('#mods-filter').value.trim().toLowerCase();
    const rows = installed.filter((m) => !q || m.title.toLowerCase().includes(q));
    if (!installed.length) box.replaceChildren(empty('Модов нет'));
    else if (!rows.length) box.replaceChildren(empty('Ничего не найдено'));
    else box.replaceChildren(el('div', { class: 'mod-list' }, ...rows.map(row)));
  }

  function row(m) {
    const r = el('div', { class: `mod-row${m.enabled ? '' : ' is-off'}`, 'data-id': `mod.${m.file}` });
    const sw = el('input', { type: 'checkbox', checked: m.enabled, 'aria-label': m.title });
    sw.addEventListener('change', async () => {
      try {
        await call('mods.toggle', { instance: inst, file: m.file, enabled: sw.checked });
        m.enabled = sw.checked;
        r.classList.toggle('is-off', !m.enabled);
      } catch (e) { sw.checked = !sw.checked; fail(e); }
    });
    const del = el('button', { class: 'btn btn-icon btn-ghost', 'data-tip': 'Удалить', 'aria-label': 'Удалить', html: TRASH });
    del.addEventListener('click', async () => {
      try {
        await call('mods.remove', { instance: inst, file: m.file });
        installed = installed.filter((x) => x !== m);
        paintList();
      } catch (e) { fail(e); }
    });
    const u = updates.get(m.file);
    const ver = el('span', { class: 'mono' }, m.version_number || m.file);
    if (u) ver.append(el('em', { class: 'upd' }, ` → ${u.version_number}`));
    r.append(icon(m.icon_url, m.title), el('div', { class: 't' }, el('b', {}, m.title), ver));
    if (u) {
      const ub = el('button', { class: 'btn btn-sm', 'data-id': `mod.${m.file}.update` }, el('span', { class: 'lbl' }, 'Обновить'));
      ub.addEventListener('click', () => update(ub, m.file));
      r.append(ub);
    }
    r.append(el('label', { class: 'switch' }, sw, el('i')), del);
    return r;
  }
  $('#mods-filter').addEventListener('input', paintList);

  // ---------- Modrinth browser ----------
  let target = null;
  let query = '';
  let offset = 0;
  let seq = 0;
  let have = new Set();
  let timer = 0;

  function discover() {
    const list = S.instances.filter(modded);
    if (!list.some((i) => i.id === target)) target = modded(byId(S.current)) ? S.current : list[0]?.id || null;
    const sel = $('#disc-target');
    sel.replaceChildren(...list.map((i) => el('option', { value: i.id, selected: i.id === target }, `${i.name} · ${LOADER_NAMES[i.loader] || i.loader} ${i.version}`)));
    sel.hidden = !list.length;
    $('#disc-q').disabled = !list.length;
    search(true);
  }

  async function refreshHave() {
    const l = await call('mods.list', { instance: target });
    have = new Set(l.map((m) => m.project_id).filter(Boolean));
  }

  async function search(reset) {
    const box = $('#disc-results');
    const more = $('#disc-more');
    if (!target) { box.replaceChildren(empty('Нет сборок для модов', 'Создайте сборку с Fabric, Quilt, Forge или NeoForge')); more.hidden = true; return; }
    if (reset) { offset = 0; box.dataset.state = 'loading'; }
    const my = ++seq;
    let res = null;
    try {
      if (reset) await refreshHave();
      res = await call('mods.search', { instance: target, query, offset });
    } catch (e) { fail(e); }
    if (my !== seq) return;
    delete box.dataset.state;
    if (!res) return;
    const cards = res.hits.map(card);
    if (reset) box.replaceChildren(...(cards.length ? cards : [empty('Ничего не найдено')]));
    else box.append(...cards);
    offset += res.hits.length;
    more.hidden = offset >= res.total_hits;
  }

  function card(h) {
    const on = have.has(h.project_id);
    const b = el('button', { class: `btn${on ? '' : ' btn-primary'}`, disabled: on, 'data-id': `discover.install.${h.slug}` }, el('span', { class: 'lbl' }, on ? 'Установлен' : 'Установить'));
    b.addEventListener('click', async () => {
      const where = target;
      const r = await run(b, 'mods.install', { instance: where, project: h.project_id }).catch(() => null);
      if (!r) return;
      b.disabled = true;
      b.classList.remove('btn-primary');
      b.querySelector('.lbl').textContent = 'Установлен';
      toast(r.added.length > 1 ? `Установлено: ${r.added.join(', ')}` : `${h.title} установлен`);
      if (where === inst) instance(byId(inst));
      if (where === target) refreshHave().then(markHave).catch(() => {});
    });
    return el('article', { class: 'mod-card', 'data-project': h.project_id },
      icon(h.icon_url, h.title),
      el('div', { class: 't' }, el('b', {}, h.title), el('span', {}, h.author)),
      el('p', {}, h.description),
      el('div', { class: 'foot' }, el('span', { class: 'dl' }, `${fmtN(h.downloads)} загрузок`), b));
  }

  // Dependencies installed along the way also flip to «Установлен».
  function markHave() {
    $$('#disc-results .mod-card').forEach((c) => {
      const b = c.querySelector('.btn');
      if (!have.has(c.dataset.project) || b.disabled) return;
      b.disabled = true;
      b.classList.remove('btn-primary');
      b.querySelector('.lbl').textContent = 'Установлен';
    });
  }

  $('#disc-q').addEventListener('input', (e) => {
    clearTimeout(timer);
    timer = setTimeout(() => { query = e.target.value; search(true); }, 250);
  });
  $('#disc-target').addEventListener('change', (e) => { target = e.target.value; search(true); });
  $('#disc-more').addEventListener('click', () => search(false));

  window.FMods = { instance, discover };
})();// app.js may have opened a view before this script ran.
if (S.view === 'instance' && S.current) window.FMods.instance(byId(S.current));
if (S.view === 'discover') window.FMods.discover();
