'use strict';
// The instance «Контент» tab, ported from Modrinth App's content page: search,
// sort, quick pills, the «Фильтр» popover (author, environment, properties),
// rows with author, version and file name, per-row update / switch version /
// freeze, multi-select with a floating bar and confirmed deletes that warn
// about dependents (Shift deletes at once). Everything goes through mods.*.
(() => {
  const KINDS = {
    mod: { none: _t("Модов нет"), ext: '.jar', words: ['мод', 'мода', 'модов'] },
    resourcepack: { none: _t("Ресурспаков нет"), ext: '.zip', words: ['ресурспак', 'ресурспака', 'ресурспаков'] },
    shader: { none: _t("Шейдеров нет"), ext: '.zip', words: ['шейдер', 'шейдера', 'шейдеров'] },
    datapack: { none: _t("Датапаков нет"), ext: '.zip', words: ['датапак', 'датапака', 'датапаков'] },
  };
  // OSI-approved licenses (Modrinth's «Open source» filter), without -only / -or-later.
  const OSI = new Set('0BSD AFL-3.0 AGPL-3.0 Apache-2.0 Artistic-2.0 BSD-2-Clause BSD-3-Clause BSL-1.0 CDDL-1.0 ECL-2.0 EPL-1.0 EPL-2.0 EUPL-1.1 EUPL-1.2 GPL-2.0 GPL-3.0 ISC LGPL-2.1 LGPL-3.0 MIT MPL-2.0 NCSA OSL-3.0 PostgreSQL Python-2.0 Unlicense UPL-1.0 Zlib'.split(' '));
  const SORTS = { az: _t("А–Я"), za: _t("Я–А"), new: _t("Новые"), old: _t("Старые"), author: _t("Автор"), size: _t("Размер"), ver: _t("Версия"), verd: _t("Версия") };
  const ENV = { client: _t("Только клиент"), server: _t("Только сервер"), both: _t("Клиент и сервер") };
  const PROPS = { upd: _t("Есть обновление"), open: _t("Открытый код"), ext: _t("Не из каталога"), lock: _t("Заморожены") };
  const svg = (d) => `<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">${d}</svg>`;
  const I = {
    upd: svg('<g fill="none"><path fill="currentColor" d="M20 15a1 1 0 0 1 1 1v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4a1 1 0 1 1 2 0v4h14v-4a1 1 0 0 1 1-1M12 2a1 1 0 0 1 1 1v10.243l2.536-2.536a1 1 0 1 1 1.414 1.414l-4.066 4.066a1.25 1.25 0 0 1-1.768 0L7.05 12.121a1 1 0 1 1 1.414-1.414L11 13.243V3a1 1 0 0 1 1-1"/></g>'),
    swap: svg('<g fill="none" fill-rule="evenodd"><path d="M24 0v24H0V0h24ZM12.594 23.258l-.012.002-.071.035-.02.004-.014-.004-.071-.036c-.01-.003-.019 0-.024.006l-.004.01-.017.428.005.02.01.013.104.074.015.004.012-.004.104-.074.012-.016.004-.017-.017-.427c-.002-.01-.009-.017-.016-.018Zm.264-.113-.014.002-.184.093-.01.01-.003.011.018.43.005.012.008.008.201.092c.012.004.023 0 .029-.008l.004-.014-.034-.614c-.003-.012-.01-.02-.02-.022Zm-.715.002a.023.023 0 0 0-.027.006l-.006.014-.034.614c0 .012.007.02.017.024l.015-.002.201-.093.01-.008.003-.011.018-.43-.003-.012-.01-.01-.184-.092Z"/><path fill="currentColor" d="M11.5 9a1 1 0 1 0 0-2H9V5.207C9 3.87 7.384 3.2 6.44 4.147L1.644 8.938a1.5 1.5 0 0 0 0 2.122l4.794 4.793c.945.945 2.56.275 2.56-1.061V13H16a1 1 0 0 0 1-1v-1.587L20.586 14 17 17.586V16a1 1 0 0 0-1-1h-3.5a1 1 0 1 0 0 2H15v1.793c0 1.336 1.615 2.005 2.56 1.06l4.794-4.793a1.5 1.5 0 0 0 0-2.121L17.56 8.146C16.615 7.2 15 7.87 15 9.206V11H8a1 1 0 0 0-1 1v1.586L3.413 10 7 6.414V8a1 1 0 0 0 1 1h3.5Z"/></g>'),
    lock: svg('<g fill="none"><path fill="currentColor" d="M12 2a6 6 0 0 1 5.996 5.775L18 8h1a2 2 0 0 1 1.995 1.85L21 10v10a2 2 0 0 1-1.85 1.995L19 22H5a2 2 0 0 1-1.995-1.85L3 20V10a2 2 0 0 1 1.85-1.995L5 8h1a6 6 0 0 1 6-6m7 8H5v10h14zm-7 2a2 2 0 0 1 1.134 3.647l-.134.085V17a1 1 0 0 1-1.993.117L11 17v-1.268A2 2 0 0 1 12 12m0-8a4 4 0 0 0-4 4h8a4 4 0 0 0-4-4"/></g>'),
    unlock: svg('<g fill="none"><path fill="currentColor" d="M12 2c1.091 0 2.117.292 3 .804a1 1 0 1 1-1 1.73A4 4 0 0 0 8 8l11 .001a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V10a2 2 0 0 1 2-2h1a6 6 0 0 1 6-6m7 8H5v10h14zm-7 2a2 2 0 0 1 1.134 3.647l-.134.085V17a1 1 0 0 1-1.993.117L11 17v-1.268A2 2 0 0 1 12 12m7.918-6.979.966.26a1 1 0 0 1-.518 1.93l-.965-.258a1 1 0 0 1 .517-1.932M18.633 2.09a1 1 0 0 1 .707 1.225l-.129.482a1 1 0 1 1-1.932-.517l.13-.483a1 1 0 0 1 1.224-.707"/></g>'),
    trash: svg('<g fill="none"><path fill="currentColor" d="M14.28 2a2 2 0 0 1 1.897 1.368L16.72 5H20a1 1 0 1 1 0 2l-.003.071-.867 12.143A3 3 0 0 1 16.138 22H7.862a3 3 0 0 1-2.992-2.786L4.003 7.07A1.01 1.01 0 0 1 4 7a1 1 0 0 1 0-2h3.28l.543-1.632A2 2 0 0 1 9.721 2zm3.717 5H6.003l.862 12.071a1 1 0 0 0 .997.929h8.276a1 1 0 0 0 .997-.929zM10 10a1 1 0 0 1 .993.883L11 11v5a1 1 0 0 1-1.993.117L9 16v-5a1 1 0 0 1 1-1m4 0a1 1 0 0 1 1 1v5a1 1 0 1 1-2 0v-5a1 1 0 0 1 1-1m.28-6H9.72l-.333 1h5.226z"/></g>'),
    more: '<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><g fill="none"><path fill="currentColor" d="M6 10.5a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3m6 0a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3m6 0a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3"/></g></svg>',
    open: svg('<g fill="none"><path fill="currentColor" d="M11 6a1 1 0 1 1 0 2H5v11h11v-6a1 1 0 1 1 2 0v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2zm9-3a1 1 0 0 1 1 1v5a1 1 0 1 1-2 0V6.414l-8.293 8.293a1 1 0 0 1-1.414-1.414L17.586 5H15a1 1 0 1 1 0-2Z"/></g>'),
    folder: svg('<g fill="none"><path fill="currentColor" d="M9.52 3a2 2 0 0 1 1.442.614l.12.137L12.48 5.5H19a2 2 0 0 1 1.995 1.85L21 7.5v1.53a2.001 2.001 0 0 1 1.61 2.398l-.038.147-2.4 8a2 2 0 0 1-1.752 1.418l-.164.007H4a2 2 0 0 1-1.995-1.85L2 19V5a2 2 0 0 1 1.85-1.995L4 3zm11.136 8H6.744l-2.4 8h13.912zM9.519 5H4v8.187l.828-2.762A2 2 0 0 1 6.744 9H19V7.5h-6.52a2 2 0 0 1-1.561-.75z"/></g>'),
    link: svg('<g fill="none"><path fill="currentColor" d="M10.232 10.231a5 5 0 0 1 6.89-.172l.181.172 2.828 2.829a5 5 0 0 1-6.89 7.243l-.18-.172-2.122-2.122a1 1 0 0 1 1.32-1.497l.094.083 2.122 2.122a3 3 0 0 0 4.377-4.1l-.135-.143-2.828-2.828a3 3 0 0 0-4.243 0 1 1 0 0 1-1.414-1.415M3.868 3.867a5 5 0 0 1 6.89-.172l.181.172L13.06 5.99a1 1 0 0 1-1.32 1.497l-.094-.083-2.121-2.121A3 3 0 0 0 5.147 9.38l.135.144 2.829 2.829a3 3 0 0 0 4.242 0 1 1 0 1 1 1.415 1.414 5 5 0 0 1-6.89.172l-.182-.172-2.828-2.829a5 5 0 0 1 0-7.07Z"/></g>'),
    caret: svg('<g fill="none" fill-rule="evenodd"><path d="M24 0v24H0V0h24ZM12.593 23.258l-.011.002-.071.035-.02.004-.014-.004-.071-.035c-.01-.004-.019-.001-.024.005l-.004.01-.017.428.005.02.01.013.104.074.015.004.012-.004.104-.074.012-.016.004-.017-.017-.427c-.002-.01-.009-.017-.017-.018Zm.265-.113-.013.002-.185.093-.01.01-.003.011.018.43.005.012.008.007.201.093c.012.004.023 0 .029-.008l.004-.014-.034-.614c-.003-.012-.01-.02-.02-.022Zm-.715.002a.023.023 0 0 0-.027.006l-.006.014-.034.614c0 .012.007.02.017.024l.015-.002.201-.093.01-.008.004-.011.017-.43-.003-.012-.01-.01-.184-.092Z"/><path fill="currentColor" d="M12.707 15.707a1 1 0 0 1-1.414 0L5.636 10.05A1 1 0 1 1 7.05 8.636l4.95 4.95 4.95-4.95a1 1 0 0 1 1.414 1.414l-5.657 5.657Z"/></g>'),
    check: svg('<g fill="none" fill-rule="evenodd"><path fill="currentColor" d="M21.192 5.465a1 1 0 0 1 0 1.414L9.95 18.122a1.1 1.1 0 0 1-1.556 0l-5.586-5.586a1 1 0 1 1 1.415-1.415l4.95 4.95L19.777 5.465a1 1 0 0 1 1.414 0Z"/></g>'),
  };
  const RIMG = navigator.userAgent.includes('Windows') ? 'http://rimg.localhost' : 'rimg://localhost';
  const imgSrc = (u) => (!u || !/^https:\/\/[^\s"'<>\\]+$/.test(u) ? '' : /^https:\/\/cdn\.modrinth\.com\//.test(u) ? u : `${RIMG}/${encodeURIComponent(u)}`);
  const fail = (e) => toast(String(e), { error: true });
  const plural = (n, [a, b, c]) => _tp(n, a, b, c);
  const count = (n) => `${n} ${plural(n, KINDS[kind].words)}`;
  const coll = new Intl.Collator(_LOC, { sensitivity: 'base', numeric: true });
  const Ops = () => window.FContentOps;

  let inst = null, kind = 'mod', world = null, items = [], updates = new Map(), seq = 0, loading = false;
  let sort = 'az', pill = 'all', anchor = null;
  let cols = Math.min(3, Math.max(1, +localStorage.getItem('fl.ct.cols') || 1));
  const sel = new Set();
  const metaDone = new Set();
  const f = { author: new Set(), env: new Set(), props: new Set() };
  const loc = () => ({ instance: inst, kind, ...(kind === 'datapack' ? { world } : {}) });
  const typeOf = () => kind;
  const CF_CLASS = { mod: 'mc-mods', resourcepack: 'texture-packs', shader: 'shaders', datapack: 'data-packs' };
  const link = (m) => (!m.slug ? '' : String(m.project_id || '').startsWith('cf:')
    ? `https://www.curseforge.com/minecraft/${CF_CLASS[typeOf()] || 'mc-mods'}/${m.slug}`
    : `https://modrinth.com/${typeOf()}/${m.slug}`);

  // ---------- Facets ----------
  const envOf = (m) => {
    const c = m.client_side, s = m.server_side;
    if (!c || !s || (c === 'unknown' && s === 'unknown')) return '';
    if (s === 'unsupported' && c !== 'unsupported') return 'client';
    if (c === 'unsupported' && s !== 'unsupported') return 'server';
    return 'both';
  };
  const openSrc = (m) => OSI.has((m.license || '').replace(/-(only|or-later)$/, ''));
  const external = (m) => !m.project_id;
  const has = (m, p) => (p === 'upd' ? updates.has(m.file) && !m.locked : p === 'open' ? openSrc(m) : p === 'ext' ? external(m) : !!m.locked);
  const authorKey = (m) => (m.author ? m.author.id : external(m) ? '~' : '');
  const pillOk = (m) => pill === 'all' || (pill === 'upd' ? has(m, 'upd') : pill === 'on' ? m.enabled : !m.enabled);
  function passes(m, skip) {
    if (skip !== 'author' && f.author.size && !f.author.has(authorKey(m))) return false;
    if (skip !== 'env' && f.env.size && !f.env.has(envOf(m))) return false;
    for (const p of f.props) if (p !== skip && !has(m, p)) return false;
    return true;
  }
  const words = () => $('#mods-filter').value.trim().toLowerCase().split(/\s+/).filter(Boolean);
  const hay = (m) => `${m.title} ${m.file} ${m.author?.name || ''} ${m.slug || ''} ${m.version_number || ''}`.toLowerCase();
  const BY = {
    az: (a, b) => coll.compare(a.title, b.title),
    za: (a, b) => coll.compare(b.title, a.title),
    new: (a, b) => (b.added || 0) - (a.added || 0) || BY.az(a, b),
    old: (a, b) => (a.added || 0) - (b.added || 0) || BY.az(a, b),
    author: (a, b) => coll.compare(a.author?.name || '\uffff', b.author?.name || '\uffff') || BY.az(a, b),
    size: (a, b) => (b.size || 0) - (a.size || 0) || BY.az(a, b),
    ver: (a, b) => coll.compare(a.version_number || a.file, b.version_number || b.file),
    verd: (a, b) => coll.compare(b.version_number || b.file, a.version_number || a.file),
  };
  function visible() {
    const w = words();
    return items.filter((m) => pillOk(m) && passes(m) && (!w.length || w.every((x) => hay(m).includes(x)))).sort(BY[sort] || BY.az);
  }
  const chosen = () => items.filter((m) => sel.has(m.file));
  const dependents = (list) => {
    const gone = new Set(list.map((m) => m.project_id).filter(Boolean));
    const files = new Set(list.map((m) => m.file));
    return items.filter((m) => m.enabled && !files.has(m.file) && (m.deps || []).some((d) => gone.has(d)));
  };

  // ---------- Load ----------
  async function instance(i, { fresh = false } = {}) {
    const id = i?.id || null;
    if (id !== inst) {
      sel.clear(); f.author.clear(); f.env.clear(); f.props.clear();
      $('#mods-filter').value = '';
      sort = localStorage.getItem(`fl.ct.sort.${id}`) || 'az';
    }
    inst = id;
    const my = ++seq;
    if (!FMods.supports(i, kind)) kind = FMods.supports(i, 'mod') ? 'mod' : 'resourcepack';
    $$('#mods-kind button').forEach((b) => { b.hidden = !FMods.supports(i, b.dataset.kind); b.setAttribute('aria-selected', String(b.dataset.kind === kind)); });
    requestAnimationFrame(() => $('#mods-kind')._place?.(false));
    pill = sessionStorage.getItem(`fl.ct.pill.${inst}.${kind}`) || 'all';
    items = []; updates = new Map(); loading = !!i;
    $('#mods-tools').hidden = !i;
    const wsel = $('#mods-world');
    wsel.hidden = true;
    paint();
    if (!i) return;
    if (kind === 'datapack') {
      const ws = await call('instances.worlds', { instance: i.id }).catch(() => []);
      if (my !== seq) return;
      if (!ws.some((w) => w.id === world)) world = ws[0]?.id || null;
      wsel.replaceChildren(...ws.map((w) => el('option', { value: w.id, selected: w.id === world }, w.name)));
      wsel.hidden = !ws.length;
      if (!world) { loading = false; paint(); return; }
    }
    const at = loc();
    try { items = await call('mods.list', at); } catch (e) { fail(e); }
    if (my !== seq) return;
    loading = false;
    for (const s of [...sel]) if (!items.some((m) => m.file === s)) sel.delete(s);
    paint();
    // Metadata and updates come from the network after the list is on screen.
    const key = JSON.stringify(at);
    if (fresh || !metaDone.has(key)) {
      const list = await call('mods.meta', at).catch(() => null);
      if (my !== seq) return;
      if (list) { metaDone.add(key); items = list; paint(); }
    }
    checkUpdates(my);
  }
  async function checkUpdates(my = seq) {
    const u = await call('mods.updates', loc()).catch(() => []);
    if (my !== seq) return;
    updates = new Map(u.map((x) => [x.file, x]));
    paint();
  }
  const reload = (o) => instance(byId(inst), o);

  // ---------- Paint ----------
  function paint() {
    paintPills();
    paintList();
    paintBar();
    const n = items.filter((m) => has(m, 'upd')).length;
    const ua = $('#mods-update-all');
    ua.hidden = !n;
    ua.querySelector('.lbl').textContent = _t("Обновить все · {0}", [n]);
    $('#ct-sort-lbl').textContent = SORTS[sort];
    const fn = f.author.size + f.env.size + f.props.size;
    $('#ct-filter-n').hidden = !fn;
    $('#ct-filter-n').textContent = String(fn);
    $('#ct-filter').classList.toggle('is-on', !!fn);
    $('#ct-filter').hidden = $('#ct-sort').hidden = $('#ct-view').hidden = items.length < 2;
  }

  function paintPills() {
    const box = $('#ct-pills');
    const on = items.filter((m) => m.enabled).length, off = items.length - on;
    const upd = items.filter((m) => has(m, 'upd')).length;
    const ps = [['all', _t("Все"), items.length]];
    if (upd) ps.push(['upd', _t("Обновления"), upd]);
    if (on && off) ps.push(['on', _t("Включённые"), on], ['off', _t("Выключенные"), off]);
    if (!ps.some((p) => p[0] === pill)) pill = 'all';
    box.hidden = !inst || ps.length < 2;
    box.replaceChildren(...ps.map(([id, t, n]) => el('button', { type: 'button', class: 'ct-pill', 'aria-selected': String(id === pill), 'data-pill': id }, t, el('span', {}, String(n)))));
  }

  const empty = (title, ...acts) => { const n = emptyState(title === _t("Ничего не найдено") ? ES_ICO.search : ES_ICO.box, title, null, ...acts); n.classList.replace('tall', 'ct-empty'); return n; };
  const textBtn = (t, fn, cls = '') => { const b = el('button', { type: 'button', class: `btn btn-sm ${cls}` }, el('span', { class: 'lbl' }, t)); b.addEventListener('click', () => fn(b)); return b; };

  function paintList() {
    const box = $('#mods-list');
    $('#tab-content').classList.toggle('has-sel', sel.size > 0);
    if (!inst) { box.replaceChildren(); return; }
    if (loading) { box.replaceChildren(skeleton()); return; }
    if (kind === 'datapack' && !world) { box.replaceChildren(empty(_t("Миров пока нет"))); return; }
    if (!items.length) {
      box.replaceChildren(empty(KINDS[kind].none, textBtn(_t("Открыть каталог"), () => $('#mods-add').click(), 'btn-primary'), textBtn(_t("Загрузить файлы"), upload)));
      return;
    }
    const rows = visible();
    if (!rows.length) { box.replaceChildren(empty(_t("Ничего не найдено"), textBtn(_t("Сбросить фильтры"), reset))); return; }
    box.replaceChildren(...[dupStrip(), el('div', { class: `ct-list${cols > 1 ? ' is-grid' : ''}`, 'data-cols': cols }, head(rows), ...rows.map(row))].filter(Boolean));
  }

  // ---------- Duplicates ----------
  // Two enabled copies of one mod crash the game, so point them out and offer to keep the newest.
  const stem = (f) => f.toLowerCase().replace(/\.(jar|zip)(\.disabled)?$/, '').split(/[-_+ ](?=v?\d)/)[0];
  const dupHidden = new Set();
  function dupes() {
    if (kind !== 'mod') return [];
    const g = new Map();
    for (const m of items) {
      if (!m.enabled) continue;
      const k = m.project_id ? `p:${m.project_id}` : `f:${stem(m.file)}`;
      g.set(k, [...(g.get(k) || []), m]);
    }
    const age = (m) => (m.published ? Date.parse(m.published) / 1000 : 0) || m.added || 0;
    return [...g.values()].filter((l) => l.length > 1).map((l) => l.sort((a, b) => age(b) - age(a)));
  }
  function dupStrip() {
    const ds = dupes();
    const key = `${inst}:${ds.flat().map((m) => m.file).sort().join('|')}`;
    if (!ds.length || dupHidden.has(key)) return null;
    const names = ds.map((l) => l[0].title || stem(l[0].file));
    const old = ds.flatMap((l) => l.slice(1));
    const fix = textBtn(_t("Отключить старые"), async (b) => {
      b.disabled = true;
      await bulk(old, _t("Выключаем"), async (m) => { await call('mods.toggle', { ...loc(), file: m.file, enabled: false }); m.enabled = false; });
      paint();
    }, 'btn-primary');
    const later = textBtn(_t("Не сейчас"), () => { dupHidden.add(key); paint(); }, 'btn-ghost');
    return el('div', { class: 'ct-dupes', id: 'ct-dupes', role: 'status', 'data-id': 'instance.content.duplicates' },
      el('div', { class: 'ct-dupes-b' },
        el('b', {}, names.length === 1 ? _t("«{0}» установлен дважды", [names[0]]) : _t("Установлены дважды: {0}", [names.join(', ')])),
        el('span', {}, _t("Игра не запустится с двумя копиями одного мода. {0}.", [old.length === 1 ? _t("Оставим новую версию, старую выключим (файл останется)") : _t("Оставим новые версии, старые выключим (файлы останутся)")]))),
      el('div', { class: 'ct-dupes-acts' }, fix, later));
  }

  function skeleton() {
    return el('div', { class: 'ct-list ct-skel', 'aria-hidden': 'true' }, ...[62, 44, 70, 52, 38].map((w) =>
      el('div', { class: 'ct-row' }, el('i', { class: 'sk ct-sk-box' }), el('i', { class: 'sk mod-ico' }),
        el('div', { class: 'ct-main' }, el('i', { class: `sk ct-sk-l w${w}` }), el('i', { class: 'sk ct-sk-s' })),
        el('div', { class: 'ct-ver' }, el('i', { class: 'sk ct-sk-s' })))));
  }

  function head(rows) {
    const all = rows.every((m) => sel.has(m.file));
    const some = rows.some((m) => sel.has(m.file));
    const cb = el('input', { type: 'checkbox', 'aria-label': _t("Выбрать все") });
    cb.checked = all; cb.indeterminate = some && !all;
    cb.addEventListener('change', () => { rows.forEach((m) => (cb.checked ? sel.add(m.file) : sel.delete(m.file))); paintList(); paintBar(); });
    const col = (t, a, d) => {
      const on = sort === a || sort === d;
      const b = el('button', { type: 'button', class: `ct-col${on ? ' is-on' : ''}${sort === d ? ' is-desc' : ''}` }, t, el('i', { html: I.caret }));
      b.addEventListener('click', () => setSort(sort === a ? d : a));
      return b;
    };
    const n = rows.length === items.length ? count(items.length) : _t("{0} из {1}", [rows.length, items.length]);
    return el('div', { class: 'ct-head' }, el('label', { class: 'check ct-sel' }, cb), col(_t("Название"), 'az', 'za'), col(_t("Версия"), 'ver', 'verd'), el('span', { class: 'ct-head-n' }, n));
  }

  // Long names keep their end (the version part) visible, like Modrinth.
  function mid(text, tag) {
    const keep = Math.min(14, Math.ceil(text.length / 2));
    const a = el('span', { class: 'mid-a' }, text.slice(0, text.length - keep));
    const n = el(tag, { class: 'mid' }, a, el('span', { class: 'mid-b' }, text.slice(text.length - keep)));
    n.addEventListener('pointerenter', () => { if (a.scrollWidth > a.clientWidth) n.dataset.tip = text; else delete n.dataset.tip; });
    return n;
  }
  function avatar(a) {
    const n = el('i', { class: `ct-ava${a.org ? ' is-org' : ''}`, 'aria-hidden': 'true' });
    const src = imgSrc(a.avatar);
    if (src) n.append(el('img', { src, alt: '', draggable: false, loading: 'lazy' }));
    else n.textContent = initials(a.name);
    return n;
  }
  function iconBtn(cls, html, tip, fn) {
    const b = el('button', { type: 'button', class: `btn btn-icon btn-ghost ${cls}`, 'data-tip': tip, 'aria-label': tip, html });
    b.addEventListener('click', (e) => { e.stopPropagation(); fn(b, e); });
    return b;
  }

  function row(m) {
    const u = updates.get(m.file);
    const r = el('div', { class: `ct-row${m.enabled ? '' : ' is-off'}${sel.has(m.file) ? ' is-sel' : ''}`, 'data-id': `content.${kind}.${m.file}`, 'data-ctx': 'menu-ct-row', 'data-file': m.file });
    const cb = el('input', { type: 'checkbox', 'aria-label': m.title });
    cb.checked = sel.has(m.file);
    cb.addEventListener('click', (e) => pick(m, cb.checked, e.shiftKey));
    const ico = FMods.icon(m.icon_url, m.title);
    const title = m.project_id
      ? el('button', { type: 'button', class: 'ct-title' }, m.title)
      : el('span', { class: 'ct-title' }, m.title);
    if (m.project_id) { title.addEventListener('click', () => openProject(m)); ico.classList.add('is-link'); ico.addEventListener('click', () => openProject(m)); }
    const by = m.author
      ? el('span', { class: 'ct-by' }, avatar(m.author), el('span', {}, m.author.name))
      : el('span', { class: 'ct-by is-muted' }, external(m) ? _t("Загружен вручную") : '');
    const ver = el('div', { class: 'ct-ver' }, m.version_number ? mid(m.version_number, 'b') : null, mid(m.file, 'span'));
    let slot;
    if (m.locked) slot = iconBtn('ct-lock', I.lock, _t("Версия заморожена"), () => lock(m, false));
    else if (u) slot = iconBtn('ct-upd', I.upd, _t("Обновить до {0}", [u.version_number]), (b) => updateOne(b, m));
    else if (m.project_id) slot = iconBtn('ct-swap', I.swap, _t("Заменить версию"), () => switchVer(m));
    else slot = el('i', { class: 'ct-slot' });
    const sw = el('input', { type: 'checkbox', 'aria-label': m.enabled ? _t("Выключить") : _t("Включить") });
    sw.checked = m.enabled;
    sw.addEventListener('change', () => toggle(m, sw, r));
    const del = iconBtn('ct-del', I.trash, document.body.classList.contains('ct-shift') ? _t("Удалить сразу") : _t("Удалить"), (b, e) => remove([m], e.shiftKey));
    const more = iconBtn('ct-more', I.more, _t("Ещё"), (b) => { const rc = b.getBoundingClientRect(); fillMenu(m); FUI.openMenuAt($('#menu-ct-row'), rc.right - 220, rc.bottom + 4); });
    r.append(el('label', { class: 'check ct-sel' }, cb), ico,
      el('div', { class: 'ct-main' }, title, by), ver,
      el('div', { class: 'ct-acts' }, slot, el('label', { class: 'switch', 'data-tip': m.enabled ? _t("Выключить") : _t("Включить") }, sw, el('i')), del, more));
    r.addEventListener('dblclick', (e) => { if (!e.target.closest('button, label, input')) pick(m, !sel.has(m.file), false); });
    return r;
  }

  function paintBar() {
    const c = chosen();
    $('#ct-bar').toggleAttribute('data-open', c.length > 0);
    $('#tab-content').classList.toggle('has-sel', c.length > 0);
    if (!c.length) return;
    $('#ct-bar-n').textContent = _t("Выбрано: {0}", [c.length]);
    $('#ct-bar-update').hidden = !c.some((m) => has(m, 'upd'));
    $('#ct-bar-on').hidden = !c.some((m) => !m.enabled);
    $('#ct-bar-off').hidden = !c.some((m) => m.enabled);
  }

  // ---------- Selection, sort, filters ----------
  function pick(m, on, range) {
    const rows = visible();
    const a = rows.findIndex((x) => x.file === anchor), b = rows.findIndex((x) => x.file === m.file);
    if (range && a >= 0 && b >= 0) rows.slice(Math.min(a, b), Math.max(a, b) + 1).forEach((x) => (on ? sel.add(x.file) : sel.delete(x.file)));
    else if (on) sel.add(m.file); else sel.delete(m.file);
    anchor = m.file;
    paintList();
    paintBar();
  }
  function setSort(s) {
    sort = s;
    localStorage.setItem(`fl.ct.sort.${inst}`, s);
    paint();
  }
  function reset() {
    f.author.clear(); f.env.clear(); f.props.clear();
    pill = 'all';
    $('#mods-filter').value = '';
    paint();
  }

  function paintFilter() {
    const pop = $('#pop-ct-filter');
    const opt = (set, key, label, n, lead) => {
      const on = set.has(key);
      const b = el('button', { type: 'button', class: 'ct-opt', role: 'menuitemcheckbox', 'aria-checked': String(on) },
        el('i', { class: 'ct-box', html: I.check }), lead || null, el('span', { class: 'ct-opt-t' }, label), el('span', { class: 'ct-opt-n' }, String(n)));
      b.addEventListener('click', () => { if (!set.delete(key)) set.add(key); paint(); paintFilter(); });
      return b;
    };
    const base = (skip) => items.filter((m) => pillOk(m) && passes(m, skip));
    // An option that matches everything (or nothing) filters nothing — hidden, as in Modrinth.
    const useful = (set, key, n, total) => set.has(key) || (n > 0 && n < total);
    const secs = [];
    const ab = base('author');
    const authors = new Map();
    for (const m of ab) {
      const k = authorKey(m);
      if (!k) continue;
      const a = authors.get(k) || { n: 0, name: k === '~' ? _t("Загружены вручную") : m.author.name, author: m.author };
      a.n++;
      authors.set(k, a);
    }
    const aopts = [...authors].filter(([k, a]) => useful(f.author, k, a.n, ab.length)).sort((x, y) => y[1].n - x[1].n || coll.compare(x[1].name, y[1].name));
    if (aopts.length) {
      const list = el('div', { class: 'ct-flist' }, ...aopts.map(([k, a]) => opt(f.author, k, a.name, a.n, a.author ? avatar(a.author) : el('i', { class: 'ct-ava is-none' }))));
      const q = aopts.length > 6 ? el('input', { class: 'field ct-fq', placeholder: _t("Автор"), autocomplete: 'off', spellcheck: false }) : null;
      q?.addEventListener('input', () => { const s = q.value.trim().toLowerCase(); [...list.children].forEach((b) => { b.hidden = !!s && !b.textContent.toLowerCase().includes(s); }); });
      secs.push(el('div', { class: 'ct-fsec' }, el('div', { class: 'menu-label' }, _t("Автор")), q, list));
    }
    const eb = base('env');
    const eopts = Object.entries(ENV).map(([k, t]) => [k, t, eb.filter((m) => envOf(m) === k).length]).filter(([k, , n]) => useful(f.env, k, n, eb.length));
    if (eopts.length) secs.push(el('div', { class: 'ct-fsec' }, el('div', { class: 'menu-label' }, _t("Окружение")), ...eopts.map(([k, t, n]) => opt(f.env, k, t, n))));
    const popts = Object.entries(PROPS).map(([k, t]) => { const pb = base(k); return [k, t, pb.filter((m) => has(m, k)).length, pb.length]; }).filter(([k, , n, tot]) => useful(f.props, k, n, tot));
    if (popts.length) secs.push(el('div', { class: 'ct-fsec' }, el('div', { class: 'menu-label' }, _t("Свойства")), ...popts.map(([k, t, n]) => opt(f.props, k, t, n))));
    if (!secs.length) secs.push(el('div', { class: 'ct-fnone' }, _t("Фильтровать нечего")));
    const active = f.author.size + f.env.size + f.props.size;
    if (active) {
      const r = el('button', { type: 'button', class: 'btn btn-sm btn-ghost ct-freset' }, _t("Сбросить"));
      r.addEventListener('click', () => { f.author.clear(); f.env.clear(); f.props.clear(); paint(); paintFilter(); });
      secs.push(r);
    }
    pop.replaceChildren(...secs);
  }

  // ---------- Operations ----------
  async function toggle(m, sw, r) {
    if (!sw.checked) {
      const deps = dependents([m]);
      if (deps.length && !(await Ops().confirm({ title: _t("Выключить «{0}»?", [m.title]), text: _t("Без него не запустятся:"), list: deps.map((d) => d.title), ok: _t("Выключить"), danger: false }))) { sw.checked = true; return; }
    }
    try {
      await call('mods.toggle', { ...loc(), file: m.file, enabled: sw.checked });
      m.enabled = sw.checked;
      r.classList.toggle('is-off', !m.enabled);
      paintPills();
      paintBar();
    } catch (e) { sw.checked = !sw.checked; fail(e); }
  }

  // Runs `fn` over the list one by one; the bar shows how far it got.
  async function bulk(list, verb, fn) {
    const n = $('#ct-bar-n');
    let bad = 0;
    for (let i = 0; i < list.length; i++) {
      if (list.length > 1) n.textContent = _t("{0} {1} из {2}", [verb, i + 1, list.length]);
      try { await fn(list[i]); } catch (e) { bad++; if (bad === 1) fail(e); }
    }
    return list.length - bad;
  }

  async function remove(list, now) {
    const deps = dependents(list);
    if (!now || deps.length) {
      const one = list.length === 1;
      const ok = await Ops().confirm({
        title: one ? _t("Удалить «{0}»?", [list[0].title]) : _t("Удалить {0}?", [count(list.length)]),
        text: deps.length ? _t("Без удаляемого не запустятся:") : '',
        list: deps.length ? deps.map((d) => d.title) : one ? [] : list.map((m) => m.title),
        ok: deps.length ? _t("Удалить всё равно") : _t("Удалить"),
      });
      if (!ok) return;
    }
    const at = loc();
    const done = [];
    const okN = await bulk(list, _t("Удаляем"), async (m) => { await call('mods.remove', { ...at, file: m.file }); done.push(m.file); });
    if (inst !== at.instance || kind !== at.kind) return;
    items = items.filter((m) => !done.includes(m.file));
    done.forEach((x) => sel.delete(x));
    paint();
    if (okN > 1) toast(_t("Удалено: {0}", [okN]));
  }

  async function updateOne(b, m) {
    const r = await run(b, 'mods.update', { ...loc(), file: m.file }).catch(() => null);
    if (!r) return;
    toast(r.updated.length ? _t("{0} обновлён", [r.updated[0]]) : _t("Обновлений нет"));
    reload();
  }
  async function updateMany(b, list) {
    const all = !list;
    list = list || items.filter((m) => has(m, 'upd'));
    if (!list.length) return;
    const ok = await Ops().confirm({
      title: _t("Обновить {0}?", [count(list.length)]),
      text: '',
      list: list.map((m) => `${m.title}  ${m.version_number || ''} → ${updates.get(m.file)?.version_number || ''}`),
      ok: _t("Обновить"),
      danger: false,
    });
    if (!ok) return;
    let n = 0;
    if (all) {
      const r = await run(b, 'mods.update', loc()).catch(() => null);
      n = r ? r.updated.length : 0;
    } else {
      b.dataset.state = 'loading';
      n = await bulk(list, _t("Обновляем"), (m) => call('mods.update', { ...loc(), file: m.file }));
      b.dataset.state = 'idle';
    }
    if (n) toast(n === 1 ? _t("Обновлено") : _t("Обновлено: {0}", [n]));
    reload();
  }

  async function setMany(on) {
    const list = chosen().filter((m) => m.enabled !== on);
    if (!on) {
      const deps = dependents(list);
      if (deps.length && !(await Ops().confirm({ title: _t("Выключить {0}?", [count(list.length)]), text: _t("Без них не запустятся:"), list: deps.map((d) => d.title), ok: _t("Выключить"), danger: false }))) return;
    }
    await bulk(list, on ? _t("Включаем") : _t("Выключаем"), async (m) => { await call('mods.toggle', { ...loc(), file: m.file, enabled: on }); m.enabled = on; });
    paint();
  }

  async function lock(m, locked) {
    try {
      await call('mods.lock', { ...loc(), file: m.file, locked });
      m.locked = locked;
      toast(locked ? _t("{0}: версия заморожена", [m.title]) : _t("{0}: версия разморожена", [m.title]));
      paint();
      if (!locked) checkUpdates();
    } catch (e) { fail(e); }
  }

  const switchVer = (m) => Ops().versions(m, loc(), () => reload());
  const openProject = (m) => window.FProject?.open(m.project_id, kind, { title: m.title, icon_url: m.icon_url });

  async function upload() {
    try {
      const r = await invoke('pick_content', { args: loc(), ext: KINDS[kind].ext });
      if (r.added.length) added(r);
    } catch (e) { fail(e); }
  }
  function added(r) {
    toast(r.added.length === 1 ? _t("Добавлен {0}", [r.added[0]]) : _t("Добавлено файлов: {0}", [r.added.length]));
    metaDone.delete(JSON.stringify(loc()));
    reload();
  }
  /** Files dropped onto the content tab; the core skips anything that does not fit the section. */
  async function drop(paths) {
    try {
      const r = await invoke('drop_content', { args: loc(), paths });
      if (r.added.length) added(r); else toast(_t("Сюда подходят только файлы {0}", [KINDS[kind].ext]), { error: true });
    } catch (e) { fail(e); }
  }

  function copy(text, msg) {
    if (!navigator.clipboard) { fail(_t("Буфер обмена недоступен")); return; }
    navigator.clipboard.writeText(text).then(() => toast(msg), fail);
  }
  function share(mode) {
    const list = chosen().sort(BY.az);
    const line = {
      names: (m) => m.title,
      files: (m) => m.file,
      links: (m) => link(m) || m.title,
      md: (m) => (link(m) ? `- [${m.title}](${link(m)})` : `- ${m.title}`),
    }[mode];
    copy(list.map(line).join('\n'), mode === 'md' ? _t("Список в Markdown скопирован") : _t("Скопировано"));
  }

  function fillMenu(m) {
    const item = (icon, text, fn, cls = '') => { const b = el('button', { type: 'button', class: `menu-item ${cls}`, html: icon }); b.append(text); b.addEventListener('click', fn); return b; };
    const kids = [];
    if (m.project_id) kids.push(item(I.open, _t("Открыть страницу"), () => openProject(m)), item(I.swap, _t("Заменить версию"), () => switchVer(m)));
    kids.push(item(I.folder, _t("Показать файл"), () => invoke('reveal_content', { args: { ...loc(), file: m.file } }).catch(fail)));
    if (m.slug) kids.push(item(I.link, _t("Скопировать ссылку"), () => copy(link(m), _t("Ссылка скопирована"))));
    if (m.project_id) kids.push(el('div', { class: 'menu-sep' }), item(m.locked ? I.unlock : I.lock, m.locked ? _t("Разморозить версию") : _t("Заморозить версию"), () => lock(m, !m.locked)));
    kids.push(el('div', { class: 'menu-sep' }), item(I.trash, _t("Удалить"), () => remove([m], false), 'is-danger'));
    $('#menu-ct-row').replaceChildren(...kids);
  }

  // ---------- Wiring ----------
  $('#menu-ct-row').addEventListener('ctx-open', (e) => { const m = items.find((x) => x.file === e.detail.dataset.file); if (m) fillMenu(m); });
  $('#ct-pills').addEventListener('click', (e) => {
    const b = e.target.closest('[data-pill]');
    if (!b) return;
    pill = b.dataset.pill;
    sessionStorage.setItem(`fl.ct.pill.${inst}.${kind}`, pill);
    paint();
  });
  function setCols(n) { cols = n; localStorage.setItem('fl.ct.cols', String(n)); paintList(); }
  $('#ct-view').addEventListener('click', () => $$('#menu-ct-view [data-cols]').forEach((b) => b.classList.toggle('is-on', +b.dataset.cols === cols)));
  $('#menu-ct-view').addEventListener('click', (e) => { const b = e.target.closest('[data-cols]'); if (b) { FUI.closeMenu(); setCols(+b.dataset.cols); } });
  $('#ct-sort').addEventListener('click', () => $$('#menu-ct-sort [data-sort]').forEach((b) => b.classList.toggle('is-on', b.dataset.sort === sort)));
  $('#menu-ct-sort').addEventListener('click', (e) => { const b = e.target.closest('[data-sort]'); if (b) setSort(b.dataset.sort); });
  $('#ct-filter').addEventListener('click', paintFilter);
  // Options re-render in place; a detached target must not read as an outside click.
  $('#pop-ct-filter').addEventListener('click', (e) => e.stopPropagation());
  $('#menu-ct-share').addEventListener('click', (e) => { const b = e.target.closest('[data-share]'); if (b) share(b.dataset.share); });
  $('#ct-upload').addEventListener('click', upload);
  $('#ct-refresh').addEventListener('click', () => reload({ fresh: true }));
  $('#mods-update-all').addEventListener('click', (e) => updateMany(e.currentTarget));
  $('#ct-bar-update').addEventListener('click', (e) => updateMany(e.currentTarget, chosen().filter((m) => has(m, 'upd'))));
  $('#ct-bar-on').addEventListener('click', () => setMany(true));
  $('#ct-bar-off').addEventListener('click', () => setMany(false));
  $('#ct-bar-del').addEventListener('click', (e) => remove(chosen(), e.shiftKey));
  $('#ct-bar-clear').addEventListener('click', () => { sel.clear(); paint(); });
  $('#mods-filter').addEventListener('input', () => { paintList(); });
  $('#mods-kind').addEventListener('seg:change', (e) => { kind = e.detail.dataset.kind; sel.clear(); reload(); });
  $('#mods-world').addEventListener('change', (e) => { world = e.target.value; reload(); });
  // Shift turns the trash buttons into «Удалить сразу».
  const shift = (on) => {
    if (document.body.classList.contains('ct-shift') === on) return;
    document.body.classList.toggle('ct-shift', on);
    $$('.ct-del').forEach((b) => { b.dataset.tip = on ? _t("Удалить сразу") : _t("Удалить"); });
  };
  addEventListener('keydown', (e) => {
    if (e.key === 'Shift') shift(true);
    if (e.key === 'Escape' && sel.size && S.view === 'instance' && !document.querySelector('.dialog[data-open], .pop[data-open]')) { sel.clear(); paint(); }
  });
  addEventListener('keyup', (e) => { if (e.key === 'Shift') shift(false); });
  addEventListener('blur', () => shift(false));

  window.FContent = {
    instance,
    reload,
    state: () => ({ inst, kind, world }),
    setKind: (k) => { if (KINDS[k]) kind = k; },
    drop,
    kind: () => kind,
  };
})();
// The sticky column header sits right under the sticky toolbar.
(() => {
  const t = document.getElementById('ct-top'), b = document.getElementById('tab-content');
  if (t && b && window.ResizeObserver) new ResizeObserver(() => b.style.setProperty('--ct-top', `${t.offsetHeight}px`)).observe(t);
  const v = t?.closest('.view');
  if (!t || !v) return;
  let raf = 0;
  const mark = () => {
    raf = 0;
    const top = v.getBoundingClientRect().top, tr = t.getBoundingClientRect();
    t.classList.toggle('is-stuck', v.scrollTop > 0 && tr.top <= top + 1);
    const h = b.querySelector('.ct-head');
    if (h) h.classList.toggle('is-stuck', h.parentElement.getBoundingClientRect().top < h.getBoundingClientRect().top - 1);
  };
  const soon = () => { if (!raf) raf = requestAnimationFrame(mark); };
  v.addEventListener('scroll', soon, { passive: true });
  new MutationObserver(soon).observe(b, { childList: true, subtree: true });
})();
