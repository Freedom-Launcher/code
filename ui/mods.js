// Content: the instance «Контент» tab and the Modrinth browser in «Моды».
// Mods, resource packs and shaders share the same actions (mods.* with `kind`),
// the same ones an addon would use.
(() => {
  const KINDS = {
    mod: { one: 'мод', many: 'Моды', none: 'Модов нет', search: 'Моды на Modrinth' },
    resourcepack: { one: 'ресурспак', many: 'Ресурспаки', none: 'Ресурспаков нет', search: 'Ресурспаки на Modrinth' },
    shader: { one: 'шейдер', many: 'Шейдеры', none: 'Шейдеров нет', search: 'Шейдеры на Modrinth' },
  };
  // Which instances can hold a kind: mods need a loader, shaders a loader with Iris or OptiFine.
  const supports = (i, kind) => !!i && (kind === 'resourcepack' || (!!i.loader && i.loader !== 'vanilla'));
  const modded = (i) => supports(i, 'mod');
  const fmtN = (n) => (n >= 1e6 ? `${(n / 1e6).toFixed(n >= 1e7 ? 0 : 1)} млн` : n >= 1e3 ? `${Math.round(n / 1e3)} тыс` : String(n));
  const CDN = /^https:\/\/cdn\.modrinth\.com\/[^"'\\()\s]+$/;
  const TRASH = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.6 8.5h5.8l.6-8.5"/></svg>';
  const fail = (e) => toast(String(e), { error: true });
  const empty = (title, ...rest) => el('div', { class: 'table-empty' }, el('b', {}, title), ...rest);
  const selectSeg = (root, kind) => {
    $$('button', root).forEach((b) => b.setAttribute('aria-selected', String(b.dataset.kind === kind)));
    requestAnimationFrame(() => root._place?.(false));
  };

  function icon(url, title) {
    const n = el('i', { class: 'mod-ico', 'aria-hidden': 'true' });
    if (url && CDN.test(url)) n.style.backgroundImage = `url("${url}")`;
    else n.textContent = initials(title);
    return n;
  }

  // ---------- Instance tab ----------
  let inst = null;
  let ikind = 'mod';
  let installed = [];
  let updates = new Map();
  let iseq = 0;

  async function instance(i) {
    inst = i?.id || null;
    const my = ++iseq;
    if (!supports(i, ikind)) ikind = modded(i) ? 'mod' : 'resourcepack';
    $$('#mods-kind button').forEach((b) => { b.hidden = !supports(i, b.dataset.kind); });
    selectSeg($('#mods-kind'), ikind);
    installed = [];
    updates = new Map();
    paintAll();
    $('#mods-filter').value = '';
    $('#mods-tools').hidden = !i;
    paintList();
    if (!i) return;
    const kind = ikind;
    try { installed = await call('mods.list', { instance: i.id, kind }); } catch (e) { fail(e); }
    if (my !== iseq) return;
    paintList();
    // Network check after the list is on screen; silent when offline.
    const u = await call('mods.updates', { instance: i.id, kind }).catch(() => []);
    if (my !== iseq) return;
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
    const args = { instance: inst, kind: ikind };
    if (file) args.file = file;
    const r = await run(btn, 'mods.update', args).catch(() => null);
    if (!r) return;
    const n = r.updated.length;
    toast(n === 0 ? 'Обновлений нет' : n === 1 ? `${r.updated[0]} обновлён` : `Обновлено: ${n}`);
    instance(byId(inst));
  }
  $('#mods-update-all').addEventListener('click', (e) => update(e.currentTarget));

  function paintList() {
    const box = $('#mods-list');
    if (!inst) { box.replaceChildren(); return; }
    const q = $('#mods-filter').value.trim().toLowerCase();
    const rows = installed.filter((m) => !q || m.title.toLowerCase().includes(q));
    if (!installed.length) box.replaceChildren(empty(KINDS[ikind].none));
    else if (!rows.length) box.replaceChildren(empty('Ничего не найдено'));
    else box.replaceChildren(el('div', { class: 'mod-list' }, ...rows.map(row)));
  }

  function row(m) {
    const kind = ikind;
    const r = el('div', { class: `mod-row${m.enabled ? '' : ' is-off'}`, 'data-id': `${kind}.${m.file}` });
    const sw = el('input', { type: 'checkbox', checked: m.enabled, 'aria-label': m.title });
    sw.addEventListener('change', async () => {
      try {
        await call('mods.toggle', { instance: inst, kind, file: m.file, enabled: sw.checked });
        m.enabled = sw.checked;
        r.classList.toggle('is-off', !m.enabled);
      } catch (e) { sw.checked = !sw.checked; fail(e); }
    });
    const del = el('button', { class: 'btn btn-icon btn-ghost', 'data-tip': 'Удалить', 'aria-label': 'Удалить', html: TRASH });
    del.addEventListener('click', async () => {
      try {
        await call('mods.remove', { instance: inst, kind, file: m.file });
        installed = installed.filter((x) => x !== m);
        paintList();
      } catch (e) { fail(e); }
    });
    const u = updates.get(m.file);
    const ver = el('span', { class: 'mono' }, m.version_number || m.file);
    if (u) ver.append(el('em', { class: 'upd' }, ` → ${u.version_number}`));
    r.append(icon(m.icon_url, m.title), el('div', { class: 't' }, el('b', {}, m.title), ver));
    if (u) {
      const ub = el('button', { class: 'btn btn-sm', 'data-id': `${kind}.${m.file}.update` }, el('span', { class: 'lbl' }, 'Обновить'));
      ub.addEventListener('click', () => update(ub, m.file));
      r.append(ub);
    }
    r.append(el('label', { class: 'switch' }, sw, el('i')), del);
    return r;
  }
  $('#mods-filter').addEventListener('input', paintList);
  $('#mods-kind').addEventListener('seg:change', (e) => { ikind = e.detail.dataset.kind; instance(byId(inst)); });
  // «Добавить» opens the browser on the same kind and instance.
  $('#mods-add').addEventListener('click', () => { kind = ikind; target = inst; });

  // ---------- Modrinth browser ----------
  let kind = 'mod';
  let target = null;
  let query = '';
  let sort = 'relevance';
  let cats = new Set();
  let hide = false;
  let offset = 0;
  let seq = 0;
  let have = new Set();
  let timer = 0;
  const catCache = new Map();

  function discover() {
    selectSeg($('#disc-kind'), kind);
    $('#disc-q').placeholder = KINDS[kind].search;
    const list = S.instances.filter((i) => supports(i, kind));
    if (!list.some((i) => i.id === target)) target = supports(byId(S.current), kind) ? S.current : list[0]?.id || null;
    const sel = $('#disc-target');
    sel.replaceChildren(...list.map((i) => el('option', { value: i.id, selected: i.id === target }, `${i.name} · ${LOADER_NAMES[i.loader] || LOADER_NAMES.vanilla} ${i.version}`)));
    sel.hidden = !list.length;
    $('#disc-q').disabled = !list.length;
    paintCats();
    search(true);
  }

  async function paintCats() {
    const k = kind;
    if (!catCache.has(k)) catCache.set(k, call('mods.categories', { kind: k }).catch(() => { catCache.delete(k); return []; }));
    const list = await catCache.get(k);
    if (k !== kind) return;
    // Modrinth groups tags by header; the browser shows the content categories only.
    const shown = list.filter((c) => c.header === 'categories' || !c.header);
    $('#disc-cats').replaceChildren(...shown.map((c) => {
      const b = el('button', { 'aria-pressed': String(cats.has(c.name)), 'data-id': `discover.cat.${c.name}` }, catName(c.name));
      b.addEventListener('click', () => {
        if (cats.has(c.name)) cats.delete(c.name); else cats.add(c.name);
        b.setAttribute('aria-pressed', String(cats.has(c.name)));
        search(true);
      });
      return b;
    }));
  }

  const CAT_RU = {
    adventure: 'Приключения', cursed: 'Странное', decoration: 'Декор', economy: 'Экономика', equipment: 'Снаряжение',
    food: 'Еда', 'game-mechanics': 'Механики', library: 'Библиотеки', magic: 'Магия', management: 'Управление',
    minigame: 'Мини-игры', mobs: 'Мобы', optimization: 'Оптимизация', social: 'Общение', storage: 'Хранение',
    technology: 'Технологии', transportation: 'Транспорт', utility: 'Утилиты', worldgen: 'Генерация мира',
    blocks: 'Блоки', combat: 'Бой', 'core-shaders': 'Core-шейдеры', entities: 'Сущности', environment: 'Окружение',
    fonts: 'Шрифты', gui: 'Интерфейс', items: 'Предметы', locale: 'Язык', models: 'Модели', audio: 'Звук',
    simplistic: 'Минимализм', themed: 'Тематические', tweaks: 'Твики', realistic: 'Реализм', modded: 'Для модов',
    'vanilla-like': 'Как в ванилле', atmosphere: 'Атмосфера', bloom: 'Свечение', 'colored-lighting': 'Цветной свет',
    foliage: 'Листва', 'path-tracing': 'Трассировка пути', pbr: 'PBR', reflections: 'Отражения', shadows: 'Тени',
    cartoon: 'Мультяшные', fantasy: 'Фэнтези', 'semi-realistic': 'Полуреализм',
  };
  const catName = (n) => CAT_RU[n] || n.replace(/-/g, ' ').replace(/^./, (c) => c.toUpperCase());

  async function refreshHave() {
    const l = await call('mods.list', { instance: target, kind });
    have = new Set(l.map((m) => m.project_id).filter(Boolean));
  }

  async function search(reset) {
    const box = $('#disc-results');
    const more = $('#disc-more');
    if (!target) {
      box.replaceChildren(kind === 'resourcepack'
        ? empty('Нет сборок', 'Создайте сборку')
        : empty('Нет подходящих сборок', 'Создайте сборку с Fabric, Quilt, Forge или NeoForge'));
      more.hidden = true;
      return;
    }
    if (reset) { offset = 0; box.dataset.state = 'loading'; }
    const my = ++seq;
    let res = null;
    try {
      if (reset) await refreshHave();
      res = await call('mods.search', { instance: target, kind, query, offset, sort, categories: [...cats] });
    } catch (e) { fail(e); }
    if (my !== seq) return;
    delete box.dataset.state;
    if (!res) return;
    offset += res.hits.length;
    const cards = res.hits.filter((h) => !hide || !have.has(h.project_id)).map(card);
    if (reset) box.replaceChildren(...(cards.length ? cards : [empty('Ничего не найдено')]));
    else box.append(...cards);
    more.hidden = offset >= res.total_hits;
  }

  function card(h) {
    const on = have.has(h.project_id);
    const k = kind;
    const b = el('button', { class: `btn${on ? '' : ' btn-primary'}`, disabled: on, 'data-id': `discover.install.${h.slug}` }, el('span', { class: 'lbl' }, on ? 'Установлен' : 'Установить'));
    b.addEventListener('click', async () => {
      const where = target;
      const r = await run(b, 'mods.install', { instance: where, kind: k, project: h.project_id }).catch(() => null);
      if (!r) return;
      setInstalled(b);
      toast(r.added.length > 1 ? `Установлено: ${r.added.join(', ')}` : `${h.title} установлен`);
      if (where === inst && k === ikind) instance(byId(inst));
      if (where === target && k === kind) refreshHave().then(markHave).catch(() => {});
    });
    return el('article', { class: 'mod-card', 'data-project': h.project_id },
      icon(h.icon_url, h.title),
      el('div', { class: 't' }, el('b', {}, h.title), el('span', {}, h.author)),
      el('p', {}, h.description),
      el('div', { class: 'foot' }, el('span', { class: 'dl' }, `${fmtN(h.downloads)} загрузок`), b));
  }

  function setInstalled(b) {
    b.disabled = true;
    b.classList.remove('btn-primary');
    b.querySelector('.lbl').textContent = 'Установлен';
  }

  // Dependencies installed along the way also flip to «Установлен».
  function markHave() {
    $$('#disc-results .mod-card').forEach((c) => {
      const b = c.querySelector('.btn');
      if (have.has(c.dataset.project) && !b.disabled) setInstalled(b);
    });
  }

  $('#disc-q').addEventListener('input', (e) => {
    clearTimeout(timer);
    timer = setTimeout(() => { query = e.target.value; search(true); }, 250);
  });
  $('#disc-kind').addEventListener('seg:change', (e) => {
    if (e.detail.dataset.kind === kind) return;
    kind = e.detail.dataset.kind;
    cats = new Set();
    discover();
  });
  $('#disc-target').addEventListener('change', (e) => { target = e.target.value; search(true); });
  $('#disc-sort').addEventListener('change', (e) => { sort = e.target.value; search(true); });
  $('#disc-hide').addEventListener('change', (e) => { hide = e.target.checked; search(true); });
  $('#disc-more').addEventListener('click', () => search(false));

  window.FMods = { instance, discover };
})();// app.js may have opened a view before this script ran.
if (S.view === 'instance' && S.current) window.FMods.instance(byId(S.current));
if (S.view === 'discover') window.FMods.discover();