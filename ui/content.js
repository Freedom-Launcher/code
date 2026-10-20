'use strict';
// The instance «Контент» tab, ported from Modrinth App's content page: search,
// sort, quick pills, the «Фильтр» popover (author, environment, properties),
// rows with author, version and file name, per-row update / switch version /
// freeze, multi-select with a floating bar and confirmed deletes that warn
// about dependents (Shift deletes at once). Everything goes through mods.*.
(() => {
  const KINDS = {
    mod: { none: 'Модов нет', ext: '.jar', words: ['мод', 'мода', 'модов'] },
    resourcepack: { none: 'Ресурспаков нет', ext: '.zip', words: ['ресурспак', 'ресурспака', 'ресурспаков'] },
    shader: { none: 'Шейдеров нет', ext: '.zip', words: ['шейдер', 'шейдера', 'шейдеров'] },
    datapack: { none: 'Датапаков нет', ext: '.zip', words: ['датапак', 'датапака', 'датапаков'] },
  };
  // OSI-approved licenses (Modrinth's «Open source» filter), without -only / -or-later.
  const OSI = new Set('0BSD AFL-3.0 AGPL-3.0 Apache-2.0 Artistic-2.0 BSD-2-Clause BSD-3-Clause BSL-1.0 CDDL-1.0 ECL-2.0 EPL-1.0 EPL-2.0 EUPL-1.1 EUPL-1.2 GPL-2.0 GPL-3.0 ISC LGPL-2.1 LGPL-3.0 MIT MPL-2.0 NCSA OSL-3.0 PostgreSQL Python-2.0 Unlicense UPL-1.0 Zlib'.split(' '));
  const SORTS = { az: 'А–Я', za: 'Я–А', new: 'Новые', old: 'Старые', author: 'Автор', size: 'Размер', ver: 'Версия', verd: 'Версия' };
  const ENV = { client: 'Только клиент', server: 'Только сервер', both: 'Клиент и сервер' };
  const PROPS = { upd: 'Есть обновление', open: 'Открытый код', ext: 'Не с Modrinth', lock: 'Заморожены' };
  const svg = (d, w = 1.5) => `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="${w}" stroke-linecap="round" stroke-linejoin="round">${d}</svg>`;
  const I = {
    upd: svg('<path d="M8 3v7m-3-3 3 3 3-3M3.5 13h9"/>', 1.7),
    swap: svg('<path d="M3 5.5h9.5L10 3M13 10.5H3.5L6 13"/>'),
    lock: svg('<rect x="3.5" y="7" width="9" height="6" rx="1.5"/><path d="M5.5 7V5.5a2.5 2.5 0 0 1 5 0V7"/>'),
    unlock: svg('<rect x="3.5" y="7" width="9" height="6" rx="1.5"/><path d="M5.5 7V5.5a2.5 2.5 0 0 1 4.8-1"/>'),
    trash: svg('<path d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.6 8.5h5.8l.6-8.5"/>'),
    more: '<svg viewBox="0 0 16 16" fill="currentColor"><circle cx="3.5" cy="8" r="1.25"/><circle cx="8" cy="8" r="1.25"/><circle cx="12.5" cy="8" r="1.25"/></svg>',
    open: svg('<path d="M6 3.5h-2.5v9h9V10M9 3.5h3.5V7M12.5 3.5 7.5 8.5"/>'),
    folder: svg('<path d="M2.5 4.5h4l1.5 1.5h5.5v6.5h-11z"/>'),
    link: svg('<path d="M7 9a2.5 2.5 0 0 0 3.5 0l2-2A2.5 2.5 0 0 0 9 3.5l-.7.7M9 7a2.5 2.5 0 0 0-3.5 0l-2 2A2.5 2.5 0 0 0 7 12.5l.7-.7"/>'),
    caret: svg('<path d="m5 6.5 3 3 3-3"/>', 1.7),
    check: svg('<path d="m4 8.2 2.6 2.6L12 5.4"/>', 2),
  };
  const RIMG = navigator.userAgent.includes('Windows') ? 'http://rimg.localhost' : 'rimg://localhost';
  const imgSrc = (u) => (!u || !/^https:\/\/[^\s"'<>\\]+$/.test(u) ? '' : /^https:\/\/cdn\.modrinth\.com\//.test(u) ? u : `${RIMG}/${encodeURIComponent(u)}`);
  const fail = (e) => toast(String(e), { error: true });
  const plural = (n, [a, b, c]) => { const m = n % 10, h = n % 100; return m === 1 && h !== 11 ? a : m >= 2 && m <= 4 && (h < 12 || h > 14) ? b : c; };
  const count = (n) => `${n} ${plural(n, KINDS[kind].words)}`;
  const coll = new Intl.Collator('ru', { sensitivity: 'base', numeric: true });
  const Ops = () => window.FContentOps;

  let inst = null, kind = 'mod', world = null, items = [], updates = new Map(), seq = 0, loading = false;
  let sort = 'az', pill = 'all', anchor = null;
  let cols = Math.min(3, Math.max(1, +localStorage.getItem('fl.ct.cols') || 1));
  const sel = new Set();
  const metaDone = new Set();
  const f = { author: new Set(), env: new Set(), props: new Set() };
  const loc = () => ({ instance: inst, kind, ...(kind === 'datapack' ? { world } : {}) });
  const typeOf = () => kind;
  const link = (m) => (m.slug ? `https://modrinth.com/${typeOf()}/${m.slug}` : '');

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
    ua.querySelector('.lbl').textContent = `Обновить все · ${n}`;
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
    const ps = [['all', 'Все', items.length]];
    if (upd) ps.push(['upd', 'Обновления', upd]);
    if (on && off) ps.push(['on', 'Включённые', on], ['off', 'Выключенные', off]);
    if (!ps.some((p) => p[0] === pill)) pill = 'all';
    box.hidden = !inst || ps.length < 2;
    box.replaceChildren(...ps.map(([id, t, n]) => el('button', { type: 'button', class: 'ct-pill', 'aria-selected': String(id === pill), 'data-pill': id }, t, el('span', {}, String(n)))));
  }

  const empty = (title, ...acts) => el('div', { class: 'table-empty ct-empty' }, el('b', {}, title), acts.length ? el('div', { class: 'ct-empty-acts' }, ...acts) : null);
  const textBtn = (t, fn, cls = '') => { const b = el('button', { type: 'button', class: `btn btn-sm ${cls}` }, el('span', { class: 'lbl' }, t)); b.addEventListener('click', () => fn(b)); return b; };

  function paintList() {
    const box = $('#mods-list');
    $('#tab-content').classList.toggle('has-sel', sel.size > 0);
    if (!inst) { box.replaceChildren(); return; }
    if (loading) { box.replaceChildren(skeleton()); return; }
    if (kind === 'datapack' && !world) { box.replaceChildren(empty('Миров пока нет')); return; }
    if (!items.length) {
      box.replaceChildren(empty(KINDS[kind].none, textBtn('Открыть каталог', () => $('#mods-add').click(), 'btn-primary'), textBtn('Загрузить файлы', upload)));
      return;
    }
    const rows = visible();
    if (!rows.length) { box.replaceChildren(empty('Ничего не найдено', textBtn('Сбросить фильтры', reset))); return; }
    box.replaceChildren(el('div', { class: `ct-list${cols > 1 ? ' is-grid' : ''}`, 'data-cols': cols }, head(rows), ...rows.map(row)));
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
    const cb = el('input', { type: 'checkbox', 'aria-label': 'Выбрать все' });
    cb.checked = all; cb.indeterminate = some && !all;
    cb.addEventListener('change', () => { rows.forEach((m) => (cb.checked ? sel.add(m.file) : sel.delete(m.file))); paintList(); paintBar(); });
    const col = (t, a, d) => {
      const on = sort === a || sort === d;
      const b = el('button', { type: 'button', class: `ct-col${on ? ' is-on' : ''}${sort === d ? ' is-desc' : ''}` }, t, el('i', { html: I.caret }));
      b.addEventListener('click', () => setSort(sort === a ? d : a));
      return b;
    };
    const n = rows.length === items.length ? count(items.length) : `${rows.length} из ${items.length}`;
    return el('div', { class: 'ct-head' }, el('label', { class: 'check ct-sel' }, cb), col('Название', 'az', 'za'), col('Версия', 'ver', 'verd'), el('span', { class: 'ct-head-n' }, n));
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
      : el('span', { class: 'ct-by is-muted' }, external(m) ? 'Загружен вручную' : '');
    const ver = el('div', { class: 'ct-ver' }, m.version_number ? mid(m.version_number, 'b') : null, mid(m.file, 'span'));
    let slot;
    if (m.locked) slot = iconBtn('ct-lock', I.lock, 'Версия заморожена', () => lock(m, false));
    else if (u) slot = iconBtn('ct-upd', I.upd, `Обновить до ${u.version_number}`, (b) => updateOne(b, m));
    else if (m.project_id) slot = iconBtn('ct-swap', I.swap, 'Заменить версию', () => switchVer(m));
    else slot = el('i', { class: 'ct-slot' });
    const sw = el('input', { type: 'checkbox', 'aria-label': m.enabled ? 'Выключить' : 'Включить' });
    sw.checked = m.enabled;
    sw.addEventListener('change', () => toggle(m, sw, r));
    const del = iconBtn('ct-del', I.trash, document.body.classList.contains('ct-shift') ? 'Удалить сразу' : 'Удалить', (b, e) => remove([m], e.shiftKey));
    const more = iconBtn('ct-more', I.more, 'Ещё', (b) => { const rc = b.getBoundingClientRect(); fillMenu(m); FUI.openMenuAt($('#menu-ct-row'), rc.right - 220, rc.bottom + 4); });
    r.append(el('label', { class: 'check ct-sel' }, cb), ico,
      el('div', { class: 'ct-main' }, title, by), ver,
      el('div', { class: 'ct-acts' }, slot, el('label', { class: 'switch', 'data-tip': m.enabled ? 'Выключить' : 'Включить' }, sw, el('i')), del, more));
    r.addEventListener('dblclick', (e) => { if (!e.target.closest('button, label, input')) pick(m, !sel.has(m.file), false); });
    return r;
  }

  function paintBar() {
    const c = chosen();
    $('#ct-bar').toggleAttribute('data-open', c.length > 0);
    $('#tab-content').classList.toggle('has-sel', c.length > 0);
    if (!c.length) return;
    $('#ct-bar-n').textContent = `Выбрано: ${c.length}`;
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
      const a = authors.get(k) || { n: 0, name: k === '~' ? 'Загружены вручную' : m.author.name, author: m.author };
      a.n++;
      authors.set(k, a);
    }
    const aopts = [...authors].filter(([k, a]) => useful(f.author, k, a.n, ab.length)).sort((x, y) => y[1].n - x[1].n || coll.compare(x[1].name, y[1].name));
    if (aopts.length) {
      const list = el('div', { class: 'ct-flist' }, ...aopts.map(([k, a]) => opt(f.author, k, a.name, a.n, a.author ? avatar(a.author) : el('i', { class: 'ct-ava is-none' }))));
      const q = aopts.length > 6 ? el('input', { class: 'field ct-fq', placeholder: 'Автор', autocomplete: 'off', spellcheck: false }) : null;
      q?.addEventListener('input', () => { const s = q.value.trim().toLowerCase(); [...list.children].forEach((b) => { b.hidden = !!s && !b.textContent.toLowerCase().includes(s); }); });
      secs.push(el('div', { class: 'ct-fsec' }, el('div', { class: 'menu-label' }, 'Автор'), q, list));
    }
    const eb = base('env');
    const eopts = Object.entries(ENV).map(([k, t]) => [k, t, eb.filter((m) => envOf(m) === k).length]).filter(([k, , n]) => useful(f.env, k, n, eb.length));
    if (eopts.length) secs.push(el('div', { class: 'ct-fsec' }, el('div', { class: 'menu-label' }, 'Окружение'), ...eopts.map(([k, t, n]) => opt(f.env, k, t, n))));
    const popts = Object.entries(PROPS).map(([k, t]) => { const pb = base(k); return [k, t, pb.filter((m) => has(m, k)).length, pb.length]; }).filter(([k, , n, tot]) => useful(f.props, k, n, tot));
    if (popts.length) secs.push(el('div', { class: 'ct-fsec' }, el('div', { class: 'menu-label' }, 'Свойства'), ...popts.map(([k, t, n]) => opt(f.props, k, t, n))));
    if (!secs.length) secs.push(el('div', { class: 'ct-fnone' }, 'Фильтровать нечего'));
    const active = f.author.size + f.env.size + f.props.size;
    if (active) {
      const r = el('button', { type: 'button', class: 'btn btn-sm btn-ghost ct-freset' }, 'Сбросить');
      r.addEventListener('click', () => { f.author.clear(); f.env.clear(); f.props.clear(); paint(); paintFilter(); });
      secs.push(r);
    }
    pop.replaceChildren(...secs);
  }

  // ---------- Operations ----------
  async function toggle(m, sw, r) {
    if (!sw.checked) {
      const deps = dependents([m]);
      if (deps.length && !(await Ops().confirm({ title: `Выключить «${m.title}»?`, text: 'Без него не запустятся:', list: deps.map((d) => d.title), ok: 'Выключить', danger: false }))) { sw.checked = true; return; }
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
      if (list.length > 1) n.textContent = `${verb} ${i + 1} из ${list.length}`;
      try { await fn(list[i]); } catch (e) { bad++; if (bad === 1) fail(e); }
    }
    return list.length - bad;
  }

  async function remove(list, now) {
    const deps = dependents(list);
    if (!now || deps.length) {
      const one = list.length === 1;
      const ok = await Ops().confirm({
        title: one ? `Удалить «${list[0].title}»?` : `Удалить ${count(list.length)}?`,
        text: deps.length ? 'Без удаляемого не запустятся:' : '',
        list: deps.length ? deps.map((d) => d.title) : one ? [] : list.map((m) => m.title),
        ok: deps.length ? 'Удалить всё равно' : 'Удалить',
      });
      if (!ok) return;
    }
    const at = loc();
    const done = [];
    const okN = await bulk(list, 'Удаляем', async (m) => { await call('mods.remove', { ...at, file: m.file }); done.push(m.file); });
    if (inst !== at.instance || kind !== at.kind) return;
    items = items.filter((m) => !done.includes(m.file));
    done.forEach((x) => sel.delete(x));
    paint();
    if (okN > 1) toast(`Удалено: ${okN}`);
  }

  async function updateOne(b, m) {
    const r = await run(b, 'mods.update', { ...loc(), file: m.file }).catch(() => null);
    if (!r) return;
    toast(r.updated.length ? `${r.updated[0]} обновлён` : 'Обновлений нет');
    reload();
  }
  async function updateMany(b, list) {
    const all = !list;
    list = list || items.filter((m) => has(m, 'upd'));
    if (!list.length) return;
    const ok = await Ops().confirm({
      title: `Обновить ${count(list.length)}?`,
      text: '',
      list: list.map((m) => `${m.title}  ${m.version_number || ''} → ${updates.get(m.file)?.version_number || ''}`),
      ok: 'Обновить',
      danger: false,
    });
    if (!ok) return;
    let n = 0;
    if (all) {
      const r = await run(b, 'mods.update', loc()).catch(() => null);
      n = r ? r.updated.length : 0;
    } else {
      b.dataset.state = 'loading';
      n = await bulk(list, 'Обновляем', (m) => call('mods.update', { ...loc(), file: m.file }));
      b.dataset.state = 'idle';
    }
    if (n) toast(n === 1 ? 'Обновлено' : `Обновлено: ${n}`);
    reload();
  }

  async function setMany(on) {
    const list = chosen().filter((m) => m.enabled !== on);
    if (!on) {
      const deps = dependents(list);
      if (deps.length && !(await Ops().confirm({ title: `Выключить ${count(list.length)}?`, text: 'Без них не запустятся:', list: deps.map((d) => d.title), ok: 'Выключить', danger: false }))) return;
    }
    await bulk(list, on ? 'Включаем' : 'Выключаем', async (m) => { await call('mods.toggle', { ...loc(), file: m.file, enabled: on }); m.enabled = on; });
    paint();
  }

  async function lock(m, locked) {
    try {
      await call('mods.lock', { ...loc(), file: m.file, locked });
      m.locked = locked;
      toast(locked ? `${m.title}: версия заморожена` : `${m.title}: версия разморожена`);
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
    toast(r.added.length === 1 ? `Добавлен ${r.added[0]}` : `Добавлено файлов: ${r.added.length}`);
    metaDone.delete(JSON.stringify(loc()));
    reload();
  }
  /** Files dropped onto the content tab; the core skips anything that does not fit the section. */
  async function drop(paths) {
    try {
      const r = await invoke('drop_content', { args: loc(), paths });
      if (r.added.length) added(r); else toast(`Сюда подходят только файлы ${KINDS[kind].ext}`, { error: true });
    } catch (e) { fail(e); }
  }

  function copy(text, msg) {
    if (!navigator.clipboard) { fail('Буфер обмена недоступен'); return; }
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
    copy(list.map(line).join('\n'), mode === 'md' ? 'Список в Markdown скопирован' : 'Скопировано');
  }

  function fillMenu(m) {
    const item = (icon, text, fn, cls = '') => { const b = el('button', { type: 'button', class: `menu-item ${cls}`, html: icon }); b.append(text); b.addEventListener('click', fn); return b; };
    const kids = [];
    if (m.project_id) kids.push(item(I.open, 'Открыть страницу', () => openProject(m)), item(I.swap, 'Заменить версию', () => switchVer(m)));
    kids.push(item(I.folder, 'Показать файл', () => invoke('reveal_content', { args: { ...loc(), file: m.file } }).catch(fail)));
    if (m.slug) kids.push(item(I.link, 'Скопировать ссылку', () => copy(link(m), 'Ссылка скопирована')));
    if (m.project_id) kids.push(el('div', { class: 'menu-sep' }), item(m.locked ? I.unlock : I.lock, m.locked ? 'Разморозить версию' : 'Заморозить версию', () => lock(m, !m.locked)));
    kids.push(el('div', { class: 'menu-sep' }), item(I.trash, 'Удалить', () => remove([m], false), 'is-danger'));
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
    $$('.ct-del').forEach((b) => { b.dataset.tip = on ? 'Удалить сразу' : 'Удалить'; });
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
