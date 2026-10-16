// Content: the instance «Контент» tab and «Каталог», the Modrinth browser for
// every kind (modpacks, mods, resource packs, data packs, shaders). Content in
// an instance goes through mods.* with `kind`, the same actions an addon uses.
(() => {
  const KINDS = {
    mod: { one: 'мод', many: 'Моды', none: 'Модов нет', search: 'Моды на Modrinth' },
    resourcepack: { one: 'ресурспак', many: 'Ресурспаки', none: 'Ресурспаков нет', search: 'Ресурспаки на Modrinth' },
    shader: { one: 'шейдер', many: 'Шейдеры', none: 'Шейдеров нет', search: 'Шейдеры на Modrinth' },
    datapack: { one: 'датапак', many: 'Датапаки', none: 'Датапаков нет', search: 'Датапаки на Modrinth' },
    modpack: { one: 'сборка', many: 'Сборки', none: 'Сборок нет', search: 'Сборки на Modrinth' },
  };
  // Which instances can hold a kind: mods and shaders need a loader, data packs
  // Minecraft 1.13+; a modpack is never installed into an instance.
  const mcOk = (v) => { const m = /^1\.(\d+)/.exec(v || ''); return !m || +m[1] >= 13; };
  const supports = (i, kind) => {
    if (!i || kind === 'modpack') return false;
    if (kind === 'resourcepack') return true;
    if (kind === 'datapack') return mcOk(i.version);
    return !!i.loader && i.loader !== 'vanilla';
  };
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
  let iworld = null;
  // Where the tab's content lives: data packs belong to one world.
  const loc = () => ({ instance: inst, kind: ikind, ...(ikind === 'datapack' ? { world: iworld } : {}) });

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
    const wsel = $('#mods-world');
    wsel.hidden = true;
    paintList();
    if (!i) return;
    if (ikind === 'datapack') {
      const ws = await call('instances.worlds', { instance: i.id }).catch(() => []);
      if (my !== iseq) return;
      if (!ws.some((w) => w.id === iworld)) iworld = ws[0]?.id || null;
      wsel.replaceChildren(...ws.map((w) => el('option', { value: w.id, selected: w.id === iworld }, w.name)));
      wsel.hidden = !ws.length;
      paintList();
      if (!iworld) return;
    }
    const at = loc();
    try { installed = await call('mods.list', at); } catch (e) { fail(e); }
    if (my !== iseq) return;
    paintList();
    // Network check after the list is on screen; silent when offline.
    const u = await call('mods.updates', at).catch(() => []);
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
    const args = loc();
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
    if (ikind === 'datapack' && !iworld) box.replaceChildren(empty('Миров пока нет'));
    else if (!installed.length) box.replaceChildren(empty(KINDS[ikind].none));
    else if (!rows.length) box.replaceChildren(empty('Ничего не найдено'));
    else box.replaceChildren(el('div', { class: 'mod-list' }, ...rows.map(row)));
  }

  function row(m) {
    const kind = ikind;
    const at = loc();
    const r = el('div', { class: `mod-row${m.enabled ? '' : ' is-off'}`, 'data-id': `${kind}.${m.file}` });
    const sw = el('input', { type: 'checkbox', checked: m.enabled, 'aria-label': m.title });
    sw.addEventListener('change', async () => {
      try {
        await call('mods.toggle', { ...at, file: m.file, enabled: sw.checked });
        m.enabled = sw.checked;
        r.classList.toggle('is-off', !m.enabled);
      } catch (e) { sw.checked = !sw.checked; fail(e); }
    });
    const del = el('button', { class: 'btn btn-icon btn-ghost', 'data-tip': 'Удалить', 'aria-label': 'Удалить', html: TRASH });
    del.addEventListener('click', async () => {
      try {
        await call('mods.remove', { ...at, file: m.file });
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
  $('#mods-add').addEventListener('click', () => { kind = ikind; target = inst; dworld = iworld; });
  $('#mods-world').addEventListener('change', (e) => { iworld = e.target.value; instance(byId(inst)); });

  // ---------- Modrinth browser ----------
  let kind = 'modpack';
  let target = null;
  let dworld = null;
  let query = '';
  let sort = 'relevance';
  let cats = new Set();
  let hide = false;
  // Filters, as in Modrinth Browse. `order` is the loader priority (first wins when several fit).
  let loaders = new Set(), order = ['neoforge', 'fabric', 'quilt', 'forge'];
  let vers = new Set(), snaps = false, vq = '';
  let env = new Set(), oss = false, excl = new Set();
  const verCache = new Map();
  const toggle = (set, v) => (set.has(v) ? set.delete(v) : set.add(v));
  let offset = 0;
  let seq = 0;
  let have = new Set();
  let timer = 0;
  const catCache = new Map();

  const instLabel = (i) => `${i.name} · ${LOADER_NAMES[i.loader] || LOADER_NAMES.vanilla} ${i.version}`;
  // The query/install context: an instance (and world) narrows the results,
  // like an instance picked in Modrinth's Browse; without one it is all of Modrinth.
  const where = () => ({ instance: target, kind, ...(kind === 'datapack' ? { world: dworld } : {}) });

  function discover() {
    selectSeg($('#disc-kind'), kind);
    $('#disc-q').placeholder = KINDS[kind].search;
    const list = S.instances.filter((i) => supports(i, kind));
    if (!list.some((i) => i.id === target)) target = null;
    const sel = $('#disc-target');
    sel.replaceChildren(el('option', { value: '', selected: !target }, 'Любая сборка'),
      ...list.map((i) => el('option', { value: i.id, selected: i.id === target }, instLabel(i))));
    sel.hidden = kind === 'modpack' || !list.length;
    paintLoaders();
    paintCats();
    paintVersions(); paintEnv(); paintChips();
    paintWorlds().then(() => search(true));
  }

  async function paintWorlds() {
    const w = $('#disc-world');
    if (kind !== 'datapack' || !target) { w.hidden = true; return; }
    const t = target;
    const ws = await call('instances.worlds', { instance: t }).catch(() => []);
    if (t !== target) return;
    if (!ws.some((x) => x.id === dworld)) dworld = ws[0]?.id || null;
    w.replaceChildren(...ws.map((x) => el('option', { value: x.id, selected: x.id === dworld }, x.name)));
    w.hidden = !ws.length;
  }

  async function paintCats() {
    const k = kind;
    if (!catCache.has(k)) catCache.set(k, call('mods.categories', { kind: k }).catch(() => { catCache.delete(k); return []; }));
    const list = await catCache.get(k);
    if (k !== kind) return;
    // Modrinth's sidebar: one group per tag header.
    const groups = new Map();
    for (const c of list) {
      const h = c.header || 'categories';
      if (h === 'loaders') continue;
      if (!groups.has(h)) groups.set(h, []);
      groups.get(h).push(c);
    }
    const order = ['categories', 'features', 'resolutions', 'performance impact'];
    const heads = [...groups.keys()].sort((a, b) => (order.indexOf(a) + 1 || 99) - (order.indexOf(b) + 1 || 99));
    $('#disc-cats').replaceChildren(...heads.map((h) => el('section', { class: 'disc-group' },
      el('h3', {}, gh(HEAD_RU[h] || catName(h))),
      el('div', { class: 'disc-gb' }, el('div', { class: 'disc-opts' }, ...groups.get(h).map((c) => catOpt(c.name)))))));
    $$('#disc-cats .disc-group').forEach((g) => { g.dataset.g = `cat:${g.querySelector('.disc-gh span').textContent}`; });
    applyClosed();
  }

  // A filter row: a check mark and a label, toggled in place.
  function opt(label, on, id, onClick) {
    const b = el('button', { class: 'disc-opt', 'aria-pressed': String(on), 'data-id': id }, el('i'), el('span', {}, label));
    b.addEventListener('click', () => onClick(b));
    return b;
  }

const CHEV = '<svg viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="m2.5 3.75 2.5 2.5 2.5-2.5"/></svg>';
  const GRIP = '<svg viewBox="0 0 10 14" fill="currentColor"><circle cx="3" cy="3" r="1.1"/><circle cx="7" cy="3" r="1.1"/><circle cx="3" cy="7" r="1.1"/><circle cx="7" cy="7" r="1.1"/><circle cx="3" cy="11" r="1.1"/><circle cx="7" cy="11" r="1.1"/></svg>';
  const BAN = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="8" r="5"/><path d="m4.6 11.4 6.8-6.8"/></svg>';
  const XS = '<svg viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="m3 3 4 4M7 3 3 7"/></svg>';
  const ENV_RU = { client: 'Клиент', server: 'Сервер' };
  const lk = () => kind === 'mod' || kind === 'modpack';
  const chev = () => el('i', { class: 'disc-chev', html: CHEV });
  const gh = (label) => el('button', { class: 'disc-gh', type: 'button' }, el('span', {}, label), chev());

  // Loaders: a check filters; the order is the priority, dragged by the grip (or Alt+↑/↓).
  function paintLoaders() {
    const free = !target && lk();
    $('#disc-loaders-g').hidden = !free;
    if (!free) return;
    const box = $('#disc-loaders');
    const before = new Map([...box.children].map((r) => [r.dataset.loader, r.getBoundingClientRect().top]));
    box.replaceChildren(...order.map((l, i) => {
      const b = opt(LOADER_NAMES[l] || l, loaders.has(l), `discover.loader.${l}`, () => {
        toggle(loaders, l); b.setAttribute('aria-pressed', String(loaders.has(l))); changed();
      });
      b.dataset.loader = l;
      b.append(el('span', { class: 'disc-rank' }, String(i + 1)), el('span', { class: 'disc-grip', title: 'Приоритет', html: GRIP }));
      return b;
    }));
    for (const r of box.children) {
      const d = (before.get(r.dataset.loader) ?? 0) - r.getBoundingClientRect().top;
      if (!before.size || !d) continue;
      r.style.transition = 'none'; r.style.transform = `translateY(${d}px)`;
      r.getBoundingClientRect();
      r.style.transition = ''; r.style.transform = '';
    }
  }
  function setOrder(o) {
    order = o; paintLoaders();
    call('settings.set', { loader_priority: o }).catch(fail);
  }
  function dragLoaders(box) {
    box.addEventListener('pointerdown', (e) => {
      const row = e.target.closest('.disc-grip')?.closest('.disc-opt');
      if (!row || e.button !== 0) return;
      e.preventDefault();
      const rows = [...box.children], from = rows.indexOf(row), h = row.offsetHeight + 1, y0 = e.clientY;
      let to = from;
      row.setPointerCapture(e.pointerId); box.dataset.drag = ''; row.dataset.dragging = '';
      const move = (ev) => {
        const dy = Math.max(-from * h, Math.min((rows.length - 1 - from) * h, ev.clientY - y0));
        row.style.transform = `translateY(${dy}px)`;
        to = Math.round(from + dy / h);
        rows.forEach((r, i) => {
          if (r === row) return;
          const sh = to > from ? (i > from && i <= to ? -h : 0) : (i < from && i >= to ? h : 0);
          r.style.transform = sh ? `translateY(${sh}px)` : '';
        });
      };
      const up = () => {
        row.removeEventListener('pointermove', move); row.removeEventListener('pointerup', up); row.removeEventListener('pointercancel', up);
        delete box.dataset.drag; delete row.dataset.dragging;
        if (to === from) { rows.forEach((r) => { r.style.transform = ''; }); return; }
        const o = [...order]; o.splice(to, 0, o.splice(from, 1)[0]); setOrder(o);
      };
      row.addEventListener('pointermove', move); row.addEventListener('pointerup', up); row.addEventListener('pointercancel', up);
    });
    box.addEventListener('click', (e) => { if (e.target.closest('.disc-grip')) { e.preventDefault(); e.stopPropagation(); } }, true);
    box.addEventListener('keydown', (e) => {
      const row = e.target.closest('.disc-opt');
      if (!row || !e.altKey || (e.key !== 'ArrowUp' && e.key !== 'ArrowDown')) return;
      e.preventDefault();
      const i = order.indexOf(row.dataset.loader), j = i + (e.key === 'ArrowUp' ? -1 : 1);
      if (j < 0 || j >= order.length) return;
      const o = [...order]; [o[i], o[j]] = [o[j], o[i]]; setOrder(o);
      box.querySelector(`[data-loader="${o[j]}"]`)?.focus();
    });
  }

  // Game versions: only without an instance (an instance locks its own version).
  async function paintVersions() {
    $('#disc-ver-g').hidden = !!target;
    if (target) return;
    const k = snaps ? 's' : 'r';
    if (!verCache.has(k)) verCache.set(k, call('versions.list', { snapshots: snaps }).then((r) => (r?.versions || []).map((v) => v.id)).catch(() => { verCache.delete(k); return []; }));
    const all = await verCache.get(k);
    const q = vq.trim().toLowerCase();
    const list = all.filter((v) => !q || v.toLowerCase().includes(q));
    const shown = [...list.filter((v) => vers.has(v)), ...list.filter((v) => !vers.has(v))].slice(0, 300);
    $('#disc-vers').replaceChildren(...(shown.length ? shown.map((v) => opt(v, vers.has(v), `discover.version.${v}`, (b) => {
      toggle(vers, v); b.setAttribute('aria-pressed', String(vers.has(v))); changed();
    })) : [el('div', { class: 'disc-none' }, 'Ничего')]));
  }
  function paintEnv() {
    $('#disc-env-g').hidden = !lk();
    $('#disc-env').replaceChildren(...Object.keys(ENV_RU).map((x) => opt(ENV_RU[x], env.has(x), `discover.env.${x}`, (b) => {
      toggle(env, x); b.setAttribute('aria-pressed', String(env.has(x))); changed();
    })));
    $('#disc-lic').replaceChildren(opt('Открытый код', oss, 'discover.open-source', (b) => { oss = !oss; b.setAttribute('aria-pressed', String(oss)); changed(); }));
  }
  // A category: click includes it, the ⊘ beside it excludes it.
  function catOpt(n) {
    const b = opt(catName(n), cats.has(n), `discover.cat.${n}`, () => {
      if (!excl.delete(n)) toggle(cats, n);
      mark(); changed();
    });
    const x = el('button', { class: 'disc-x', type: 'button', title: 'Исключить', 'aria-label': 'Исключить', 'data-id': `discover.exclude.${n}`, html: BAN });
    const w = el('div', { class: 'disc-optw' }, b, x);
    const mark = () => { b.setAttribute('aria-pressed', String(cats.has(n))); w.dataset.mode = excl.has(n) ? 'exclude' : ''; };
    x.addEventListener('click', () => { if (!excl.delete(n)) { excl.add(n); cats.delete(n); } mark(); changed(); });
    mark();
    return w;
  }

  // Active filters as chips above the results; only the ones that apply here.
  function items() {
    const out = [], free = !target;
    if (free && lk()) order.filter((l) => loaders.has(l)).forEach((l) => out.push([`l:${l}`, LOADER_NAMES[l] || l, () => loaders.delete(l)]));
    if (free) [...vers].forEach((v) => out.push([`v:${v}`, v, () => vers.delete(v)]));
    if (lk()) [...env].forEach((x) => out.push([`e:${x}`, ENV_RU[x], () => env.delete(x)]));
    if (oss) out.push(['o', 'Открытый код', () => { oss = false; }]);
    [...cats].forEach((c) => out.push([`c:${c}`, catName(c), () => cats.delete(c)]));
    [...excl].forEach((c) => out.push([`x:${c}`, catName(c), () => excl.delete(c), true]));
    return out;
  }
  function searchArgs() {
    const free = !target;
    const e = lk() && env.size ? (env.size === 2 ? 'both' : [...env][0]) : null;
    return { instance: kind === 'modpack' ? null : target, kind, query, offset, sort, categories: [...cats], exclude: [...excl], open_source: oss,
      ...(free && lk() && loaders.size ? { loaders: order.filter((l) => loaders.has(l)) } : {}),
      ...(free && vers.size ? { versions: [...vers] } : {}),
      ...(e ? { environment: e } : {}) };
  }
  function paintChips() {
    const box = $('#disc-chips'), list = items();
    const old = new Map([...box.children].map((n) => [n.dataset.key, n]));
    const nodes = list.map(([key, label, off, x]) => old.get(key) || el('button', { class: `disc-chip${x ? ' is-x' : ''}`, type: 'button', 'data-key': key, onclick: () => { off(); refresh(); } },
      el('span', {}, label), el('i', { html: XS })));
    if (nodes.length) nodes.push(old.get('*') || el('button', { class: 'disc-chip-clear', type: 'button', 'data-key': '*', onclick: resetAll }, 'Сбросить всё'));
    box.replaceChildren(...nodes);
    box.hidden = !list.length;
    $('#disc-filters-toggle').dataset.count = list.length ? String(list.length) : '';
  }
  function changed() { paintChips(); search(true); }
  function refresh() { paintLoaders(); paintVersions(); paintEnv(); paintCats(); changed(); }
  function resetAll() { [loaders, vers, env, cats, excl].forEach((x) => x.clear()); oss = false; refresh(); }

  // Collapsible sections and the panel itself, both remembered.
  let closed;
  try { closed = new Set(JSON.parse(localStorage.getItem('fl.disc-closed') || '[]')); } catch { closed = new Set(); }
  function applyClosed() {
    $$('.disc-side .disc-group').forEach((g) => {
      const c = closed.has(g.dataset.g);
      g.toggleAttribute('data-collapsed', c);
      g.querySelector('.disc-gh')?.setAttribute('aria-expanded', String(!c));
    });
  }
  function setPanel(on) {
    $('.disc-body').dataset.filters = on ? 'on' : 'off';
    $('#disc-filters-toggle').setAttribute('aria-pressed', String(on));
    localStorage.setItem('fl.disc-filters', on ? 'on' : 'off');
  }
  function initFilters() {
    $$('.disc-side .disc-gh').forEach((b) => { if (!b.querySelector('.disc-chev')) b.append(chev()); });
    $('.disc-side').addEventListener('click', (e) => {
      const h = e.target.closest('.disc-gh');
      if (!h) return;
      toggle(closed, h.closest('.disc-group').dataset.g);
      localStorage.setItem('fl.disc-closed', JSON.stringify([...closed]));
      applyClosed();
    });
    applyClosed();
    setPanel(localStorage.getItem('fl.disc-filters') !== 'off');
    $('#disc-filters-toggle').addEventListener('click', () => setPanel($('.disc-body').dataset.filters === 'off'));
    $('#disc-vq').addEventListener('input', (e) => { vq = e.target.value; paintVersions(); });
    $('#disc-snaps').addEventListener('change', (e) => { snaps = e.target.checked; paintVersions(); });
    dragLoaders($('#disc-loaders'));
    call('settings.get').then((st) => { if (Array.isArray(st?.loader_priority) && st.loader_priority.length) { order = st.loader_priority; paintLoaders(); } }).catch(() => {});
  }
  const HEAD_RU = { categories: 'Категории', features: 'Особенности', resolutions: 'Разрешение', 'performance impact': 'Нагрузка' };

  const CAT_RU = {
    adventure: 'Приключения', cursed: 'Странное', decoration: 'Декор', economy: 'Экономика', equipment: 'Снаряжение',
    food: 'Еда', 'game-mechanics': 'Механики', library: 'Библиотеки', magic: 'Магия', management: 'Управление',
    minigame: 'Мини-игры', mobs: 'Мобы', optimization: 'Оптимизация', social: 'Общение', storage: 'Хранение',
    technology: 'Технологии', transportation: 'Транспорт', utility: 'Утилиты', worldgen: 'Генерация мира',
    blocks: 'Блоки', combat: 'Бой', 'core-shaders': 'Core-шейдеры', entities: 'Сущности', environment: 'Окружение',
    fonts: 'Шрифты', gui: 'Интерфейс', items: 'Предметы', locale: 'Язык', models: 'Модели', audio: 'Звук',
    simplistic: 'Минимализм', themed: 'Тематические', tweaks: 'Твики', realistic: 'Реализм', modded: 'Для модов',
    'vanilla-like': 'Как в ванилле', challenging: 'Испытания', 'kitchen-sink': 'Всё и сразу',
    potato: 'Очень слабые ПК', low: 'Низкая', medium: 'Средняя', high: 'Высокая', screenshot: 'Для скриншотов',
    lightweight: 'Лёгкие', multiplayer: 'Мультиплеер', quests: 'Квесты', atmosphere: 'Атмосфера', bloom: 'Свечение', 'colored-lighting': 'Цветной свет',
    foliage: 'Листва', 'path-tracing': 'Трассировка пути', pbr: 'PBR', reflections: 'Отражения', shadows: 'Тени',
    cartoon: 'Мультяшные', fantasy: 'Фэнтези', 'semi-realistic': 'Полуреализм',
  };
  const catName = (n) => CAT_RU[n] || n.replace(/-/g, ' ').replace(/^./, (c) => c.toUpperCase());

  async function refreshHave() {
    if (!target || kind === 'modpack' || (kind === 'datapack' && !dworld)) { have = new Set(); return; }
    const l = await call('mods.list', where());
    have = new Set(l.map((m) => m.project_id).filter(Boolean));
  }

  async function search(reset) {
    const box = $('#disc-results');
    const more = $('#disc-more');
    $('.disc-side > .disc-hide').hidden = kind === 'modpack' || !target;
    $('#disc-reset').hidden = !items().length;
    if (reset) { offset = 0; box.dataset.state = 'loading'; }
    const my = ++seq;
    let res = null;
    try {
      if (reset) await refreshHave();
      res = await call('mods.search', searchArgs());
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
    const k = kind;
    const on = k !== 'modpack' && have.has(h.project_id);
    const b = el('button', { class: `btn${on ? '' : ' btn-primary'}`, disabled: on, 'data-id': `discover.install.${h.slug}` }, el('span', { class: 'lbl' }, on ? 'Установлен' : 'Установить'));
    b.addEventListener('click', (e) => { e.stopPropagation(); install(b, h, k); });
    const open = () => window.FProject?.open(h.project_id, k, h);
    return el('article', { class: 'mod-card', 'data-project': h.project_id, tabindex: '0', onclick: open, onkeydown: (e) => { if (e.key === 'Enter' && e.target === e.currentTarget) open(); } },
      icon(h.icon_url, h.title),
      el('div', { class: 't' }, el('b', {}, h.title), el('span', {}, h.author)),
      el('p', {}, h.description),
      el('div', { class: 'foot' }, el('span', { class: 'dl' }, `${fmtN(h.downloads)} загрузок`), b));
  }

  function install(b, h, k, version) {
    const v = version ? { version } : {};
    if (k === 'modpack') installPack(b, h, version);
    else if (target && k === kind && (k !== 'datapack' || dworld)) installInto(b, h, { ...where(), ...v });
    else pickTarget(b, h, k, version);
  }

  async function installInto(b, h, at) {
    const r = await run(b, 'mods.install', { ...at, project: h.project_id }).catch(() => null);
    if (!r) return;
    const name = byId(at.instance)?.name || '';
    toast(r.added.length > 1 ? `Установлено: ${r.added.join(', ')}` : `${h.title} → ${name}`);
    if (at.instance === target && at.kind === kind) { setInstalled(b); refreshHave().then(markHave).catch(() => {}); }
    if (at.instance === inst && at.kind === ikind) instance(byId(inst));
  }

  // «Установить» without a chosen instance asks where, like Modrinth's install modal.
  const pick = el('div', { class: 'pop', id: 'menu-disc-pick', role: 'menu', 'data-id': 'discover.pick' });
  document.body.append(pick);
  async function pickTarget(b, h, k, version) {
    const list = S.instances.filter((i) => supports(i, k));
    let items = list.map((i) => ({ at: { instance: i.id, kind: k }, label: i.name, sub: `${LOADER_NAMES[i.loader] || LOADER_NAMES.vanilla} ${i.version}` }));
    if (k === 'datapack') {
      const ws = await Promise.all(list.map((i) => call('instances.worlds', { instance: i.id }).catch(() => [])));
      items = list.flatMap((i, n) => ws[n].map((w) => ({ at: { instance: i.id, kind: k, world: w.id }, label: w.name, sub: i.name })));
    }
    if (!items.length) { toast(k === 'datapack' ? 'Миров пока нет' : 'Нет подходящих сборок', { error: true }); return; }
    pick.replaceChildren(el('div', { class: 'menu-label' }, 'Установить в'), ...items.map((it) => {
      const m = el('button', { class: 'menu-item', 'data-id': `discover.pick.${it.at.instance}` }, el('span', { class: 'mi-t' }, it.label), el('span', { class: 'mi-sub' }, it.sub));
      m.addEventListener('click', () => installInto(b, h, version ? { ...it.at, version } : it.at));
      return m;
    }));
    const r = b.getBoundingClientRect();
    FUI.openMenuAt(pick, r.left, r.bottom + 6);
  }

  const uuid = () => crypto.randomUUID?.() || '10000000-1000-4000-8000-100000000000'.replace(/[018]/g, (c) => (c ^ (crypto.getRandomValues(new Uint8Array(1))[0] & (15 >> (c / 4)))).toString(16));

  // A modpack becomes a new instance; the button shows the download progress.
  async function installPack(b, h, version) {
    const lbl = b.querySelector('.lbl');
    const token = uuid();
    b.disabled = true;
    b.dataset.state = 'loading';
    lbl.textContent = 'Готовим…';
    const poll = setInterval(async () => {
      const p = await call('game.progress', { instance: token }).catch(() => null);
      if (!p) return;
      const f = p.total_bytes > 0 ? p.done_bytes / p.total_bytes : p.total_files > 0 ? p.done_files / p.total_files : null;
      if (f != null) lbl.textContent = `${Math.floor(f * 100)}%`;
    }, 250);
    try {
      const created = await call('modpacks.install', { project: h.project_id, token, ...(version ? { version } : {}), ...(loaders.size ? { loaders: [...loaders] } : {}) });
      clearInterval(poll);
      b.dataset.state = 'success';
      lbl.textContent = 'Установлена';
      S.instances = await call('instances.list');
      renderInstances();
      toast(`${created.name} установлена`);
      openInstance(created.id);
    } catch (e) {
      clearInterval(poll);
      b.disabled = false;
      b.dataset.state = 'error';
      lbl.textContent = 'Установить';
      fail(e);
    }
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
    cats = new Set(); excl = new Set();
    discover();
  });
  $('#disc-target').addEventListener('change', (e) => { target = e.target.value || null; paintLoaders(); paintVersions(); paintChips(); paintWorlds().then(() => search(true)); });
  $('#disc-world').addEventListener('change', (e) => { dworld = e.target.value; search(true); });
  $('#disc-sort').addEventListener('change', (e) => { sort = e.target.value; search(true); });
  $('#disc-hide').addEventListener('change', (e) => { hide = e.target.checked; search(true); });
  $('#disc-reset').addEventListener('click', resetAll);
  initFilters();
  $('#disc-more').addEventListener('click', () => search(false));

  // The project page reuses the catalog's install flow and context.
  const context = () => ({ target: target && byId(target), kind, have });
  window.FMods = { instance, discover, install, context, icon, fmtN, supports, catName };
})();// app.js may have opened a view before this script ran.
if (S.view === 'instance' && S.current) window.FMods.instance(byId(S.current));
if (S.view === 'discover') window.FMods.discover();