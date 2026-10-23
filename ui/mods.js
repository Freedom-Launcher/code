// Content: the instance «Контент» tab and «Каталог», the Modrinth browser for
// every kind (modpacks, mods, resource packs, data packs, shaders). Content in
// an instance goes through mods.* with `kind`, the same actions an addon uses.
(() => {
  const KINDS = {
    mod: { one: _t("мод"), many: _t("Моды"), none: _t("Модов нет"), search: _t("Моды на Modrinth") },
    resourcepack: { one: _t("ресурспак"), many: _t("Ресурспаки"), none: _t("Ресурспаков нет"), search: _t("Ресурспаки на Modrinth") },
    shader: { one: _t("шейдер"), many: _t("Шейдеры"), none: _t("Шейдеров нет"), search: _t("Шейдеры на Modrinth") },
    datapack: { one: _t("датапак"), many: _t("Датапаки"), none: _t("Датапаков нет"), search: _t("Датапаки на Modrinth") },
    modpack: { one: _t("сборка"), many: _t("Сборки"), none: _t("Сборок нет"), search: _t("Сборки на Modrinth") },
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
  const fmtN = (n) => (n >= 1e6 ? _t("{0} млн", [(n / 1e6).toFixed(n >= 1e7 ? 0 : 1)]) : n >= 1e3 ? _t("{0} тыс", [Math.round(n / 1e3)]) : String(n));
  const CDN = /^https:\/\/(?:cdn\.modrinth\.com|media\.forgecdn\.net)\/[^"'\\()\s]+$/;
  const TRASH = '<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><g fill="none"><path fill="currentColor" d="M14.28 2a2 2 0 0 1 1.897 1.368L16.72 5H20a1 1 0 1 1 0 2l-.003.071-.867 12.143A3 3 0 0 1 16.138 22H7.862a3 3 0 0 1-2.992-2.786L4.003 7.07A1.01 1.01 0 0 1 4 7a1 1 0 0 1 0-2h3.28l.543-1.632A2 2 0 0 1 9.721 2zm3.717 5H6.003l.862 12.071a1 1 0 0 0 .997.929h8.276a1 1 0 0 0 .997-.929zM10 10a1 1 0 0 1 .993.883L11 11v5a1 1 0 0 1-1.993.117L9 16v-5a1 1 0 0 1 1-1m4 0a1 1 0 0 1 1 1v5a1 1 0 1 1-2 0v-5a1 1 0 0 1 1-1m.28-6H9.72l-.333 1h5.226z"/></g></svg>';
  const fail = (e) => toast(String(e), { error: true });
  const empty = (title, ...rest) => el('div', { class: 'table-empty' }, el('b', {}, title), ...rest);
  const selectSeg = (root, kind) => {
    $$('button', root).forEach((b) => b.setAttribute('aria-selected', String(b.dataset.kind === kind)));
    requestAnimationFrame(() => root._place?.(false));
  };

  function icon(url, title) {
    const n = el('i', { class: 'mod-ico', 'aria-hidden': 'true' });
    if (url && CDN.test(url)) n.style.backgroundImage = `url("${url}")`;
    else {
      // Letter tile tinted by name, so cards without an icon are still easy to tell apart.
      n.textContent = initials(title);
      let h = 0; for (const ch of String(title ?? '')) h = (h * 31 + ch.codePointAt(0)) % 360;
      n.classList.add('is-tile'); n.style.setProperty('--tile', String(h));
    }
    return n;
  }

  // ---------- Instance tab ----------
  // The list itself lives in content.js (FContent); the catalog keeps its scope.
  const instance = (i) => window.FContent.instance(i);
  // «Добавить» opens the catalog scoped to this instance, as in Modrinth App.
  $('#mods-add').addEventListener('click', () => {
    const st = window.FContent.state();
    kind = st.kind; target = st.inst; dworld = st.world; scope = st.inst;
  });

  // ---------- Modrinth browser ----------
  let kind = 'modpack';
  // The catalog searched: Modrinth or CurseForge, remembered between sessions.
  let src = localStorage.getItem('fl.disc.src') === 'curseforge' ? 'curseforge' : 'modrinth';
  const SRC_NAME = { modrinth: 'Modrinth', curseforge: 'CurseForge' };
  const cf = () => src === 'curseforge';
  let target = null;
  let scope = null; // instance the catalog is scoped to («Установка в …»)
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
  let offset = 0, page = 1, pages = 1;
  const per = () => S.look.per_page || 20;
  const paged = () => (S.look.browse || 'pages') === 'pages';
  let seq = 0;
  let have = new Set();
  let timer = 0;
  const catCache = new Map();
  const catLabels = new Map(), headLabels = new Map();

  const instLabel = (i) => `${i.name} · ${LOADER_NAMES[i.loader] || LOADER_NAMES.vanilla} ${i.version}`;
  // The query/install context: an instance (and world) narrows the results,
  // like an instance picked in Modrinth's Browse; without one it is all of Modrinth.
  const where = () => ({ instance: target, kind, ...(kind === 'datapack' ? { world: dworld } : {}) });

  const LOCK = '<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><g fill="none"><path fill="currentColor" d="M12 2a6 6 0 0 1 5.996 5.775L18 8h1a2 2 0 0 1 1.995 1.85L21 10v10a2 2 0 0 1-1.85 1.995L19 22H5a2 2 0 0 1-1.995-1.85L3 20V10a2 2 0 0 1 1.85-1.995L5 8h1a6 6 0 0 1 6-6m7 8H5v10h14zm-7 2a2 2 0 0 1 1.134 3.647l-.134.085V17a1 1 0 0 1-1.993.117L11 17v-1.268A2 2 0 0 1 12 12m0-8a4 4 0 0 0-4 4h8a4 4 0 0 0-4-4"/></g></svg>';
  function paintScope() {
    const i = scope && byId(scope);
    if (scope && !i) scope = null;
    $('#view-discover').toggleAttribute('data-scoped', !!i);
    $('#disc-scope').hidden = !i;
    const ok = (k) => !i || (k !== 'modpack' && supports(i, k));
    $$('#disc-kind button').forEach((b) => { b.hidden = !ok(b.dataset.kind); });
    requestAnimationFrame(() => $('#disc-kind')._place?.(false));
    // Scoped: the rail lights the instance, not «Каталог» (as in Modrinth App).
    $$('.rail-btn[data-view="discover"]').forEach((b) => (i ? b.removeAttribute('aria-current') : S.view === 'discover' && b.setAttribute('aria-current', 'page')));
    $$('#rail-list .tile').forEach((t) => (i && t.dataset.inst === i.id ? t.setAttribute('aria-current', 'page') : t.removeAttribute('aria-current')));
    if (!i) return;
    if (!ok(kind)) kind = ['mod', 'resourcepack', 'shader', 'datapack'].find(ok) || 'resourcepack';
    target = i.id;
    bg($('#disc-scope-ico'), iconName(i));
    $('#disc-scope-name').textContent = i.name;
    $('#disc-scope-tags').replaceChildren(...[LOADER_NAMES[i.loader] || LOADER_NAMES.vanilla, i.version]
      .map((t) => el('span', { class: 'disc-scope-tag' }, el('i', { 'aria-hidden': 'true', html: LOCK }), t)));
  }
  function scoped(id, k) { scope = id; target = id; if (k) kind = k; show('discover'); }
  function unscope() { if (!scope) return; scope = null; target = null; }
  document.addEventListener('click', (e) => { const v = e.target.closest('[data-view="discover"]'); if (v && v.id !== 'mods-add') unscope(); }, true);
  $('#disc-scope-back').addEventListener('click', () => {
    const id = scope;
    if (!byId(id)) return show('home');
    window.FContent.setKind(kind);
    openInstance(id);
  });

  // Coming back to the catalog keeps the page and the scroll position as long as
  // the query is the same (Modrinth App behaviour); a changed query starts over.
  let lastSig = '', lastTop = 0;
  const mainEl = $('#view-discover').closest('.main');
  mainEl?.addEventListener('scroll', () => { if (S.view === 'discover') lastTop = mainEl.scrollTop; }, { passive: true });
  const sig = () => { const a = searchArgs(); delete a.offset; return JSON.stringify([a, hide, dworld, scope, paged() ? page : 0]); };

  function discover() {
    const top = lastTop;
    paintScope();
    selectSeg($('#disc-kind'), kind);
    $$('#disc-src button').forEach((b) => b.setAttribute('aria-selected', String(b.dataset.src === src)));
    requestAnimationFrame(() => $('#disc-src')._place?.(false));
    $('#disc-q').placeholder = _t("{0} на {1}", [KINDS[kind].many, SRC_NAME[src]]);
    const list = S.instances.filter((i) => supports(i, kind));
    if (!list.some((i) => i.id === target)) target = null;
    const sel = $('#disc-target');
    sel.replaceChildren(el('option', { value: '', selected: !target }, _t("Любая сборка")),
      ...list.map((i) => el('option', { value: i.id, selected: i.id === target }, instLabel(i))));
    sel.hidden = !!scope || kind === 'modpack' || !list.length;
    paintLoaders();
    paintCats();
    paintVersions(); paintEnv(); paintChips();
    paintWorlds().then(async () => {
      const box = $('#disc-results');
      if (lastSig && sig() === lastSig && box.children.length && !box.dataset.state) {
        await refreshHave().catch(() => {});
        markHave();
        requestAnimationFrame(() => mainEl?.scrollTo({ top }));
        return;
      }
      search(true);
    });
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
    const k = kind, s = src, key = `${s}:${k}`;
    if (!catCache.has(key)) catCache.set(key, call('mods.categories', { kind: k, source: s }).catch(() => { catCache.delete(key); return []; }));
    const list = await catCache.get(key);
    if (k !== kind || s !== src) return;
    for (const c of list) { if (c.label) catLabels.set(c.name, c.label); if (c.header_label) headLabels.set(c.header, c.header_label); }
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
      el('h3', {}, gh(HEAD_RU[h] || headLabels.get(h) || catName(h))),
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

const CHEV = '<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><g fill="none" fill-rule="evenodd"><path d="M24 0v24H0V0h24ZM12.593 23.258l-.011.002-.071.035-.02.004-.014-.004-.071-.035c-.01-.004-.019-.001-.024.005l-.004.01-.017.428.005.02.01.013.104.074.015.004.012-.004.104-.074.012-.016.004-.017-.017-.427c-.002-.01-.009-.017-.017-.018Zm.265-.113-.013.002-.185.093-.01.01-.003.011.018.43.005.012.008.007.201.093c.012.004.023 0 .029-.008l.004-.014-.034-.614c-.003-.012-.01-.02-.02-.022Zm-.715.002a.023.023 0 0 0-.027.006l-.006.014-.034.614c0 .012.007.02.017.024l.015-.002.201-.093.01-.008.004-.011.017-.43-.003-.012-.01-.01-.184-.092Z"/><path fill="currentColor" d="M12.707 15.707a1 1 0 0 1-1.414 0L5.636 10.05A1 1 0 1 1 7.05 8.636l4.95 4.95 4.95-4.95a1 1 0 0 1 1.414 1.414l-5.657 5.657Z"/></g></svg>';
  const GRIP = '<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><g fill="none"><path fill="currentColor" d="M12 7.5a1.5 1.5 0 1 0 0 3 1.5 1.5 0 0 0 0-3M4.5 15a1.5 1.5 0 1 1 3 0 1.5 1.5 0 0 1-3 0m7.5-1.5a1.5 1.5 0 1 0 0 3 1.5 1.5 0 0 0 0-3m6 0a1.5 1.5 0 1 0 0 3 1.5 1.5 0 0 0 0-3M16.5 9a1.5 1.5 0 1 1 3 0 1.5 1.5 0 0 1-3 0M6 7.5a1.5 1.5 0 1 0 0 3 1.5 1.5 0 0 0 0-3"/></g></svg>';
  const BAN = '<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><g fill="none"><path fill="currentColor" d="M12 2c5.523 0 10 4.477 10 10s-4.477 10-10 10S2 17.523 2 12 6.477 2 12 2m0 2a8 8 0 1 0 0 16 8 8 0 0 0 0-16m2.829 3.757a1 1 0 0 1 1.497 1.32l-.083.095-7.071 7.07a1 1 0 0 1-1.498-1.32l.083-.094 7.072-7.07Z"/></g></svg>';
  const XS = '<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><g fill="none" fill-rule="evenodd"><path fill="currentColor" d="m12 13.414 5.657 5.657a1 1 0 0 0 1.414-1.414L13.414 12l5.657-5.657a1 1 0 0 0-1.414-1.414L12 10.586 6.343 4.929A1 1 0 0 0 4.93 6.343L10.586 12l-5.657 5.657a1 1 0 1 0 1.414 1.414z"/></g></svg>';
  const ENV_RU = { client: _t("Клиент"), server: _t("Сервер") };
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
      b.append(el('span', { class: 'disc-rank' }, String(i + 1)), el('span', { class: 'disc-grip', title: _t("Приоритет"), html: GRIP }));
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
    })) : [el('div', { class: 'disc-none' }, _t("Ничего"))]));
  }
  function paintEnv() {
    $('#disc-env-g').hidden = !lk() || cf();
    $('#disc-lic-g').hidden = cf();
    $('#disc-env').replaceChildren(...Object.keys(ENV_RU).map((x) => opt(ENV_RU[x], env.has(x), `discover.env.${x}`, (b) => {
      toggle(env, x); b.setAttribute('aria-pressed', String(env.has(x))); changed();
    })));
    $('#disc-lic').replaceChildren(opt(_t("Открытый код"), oss, 'discover.open-source', (b) => { oss = !oss; b.setAttribute('aria-pressed', String(oss)); changed(); }));
  }
  // A category: click includes it, the ⊘ beside it excludes it.
  function catOpt(n) {
    const b = opt(catName(n), cats.has(n), `discover.cat.${n}`, () => {
      if (!excl.delete(n)) toggle(cats, n);
      mark(); changed();
    });
    const x = el('button', { class: 'disc-x', type: 'button', title: _t("Исключить"), 'aria-label': _t("Исключить"), 'data-id': `discover.exclude.${n}`, html: BAN });
    // CurseForge cannot exclude a category.
    const w = el('div', { class: 'disc-optw' }, b, cf() ? null : x);
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
    if (oss) out.push(['o', _t("Открытый код"), () => { oss = false; }]);
    [...cats].forEach((c) => out.push([`c:${c}`, catName(c), () => cats.delete(c)]));
    [...excl].forEach((c) => out.push([`x:${c}`, catName(c), () => excl.delete(c), true]));
    return out;
  }
  function searchArgs() {
    const free = !target;
    const e = !cf() && lk() && env.size ? (env.size === 2 ? 'both' : [...env][0]) : null;
    return { instance: kind === 'modpack' ? null : target, kind, query, offset, limit: per(), sort, categories: [...cats], exclude: cf() ? [] : [...excl], open_source: !cf() && oss, source: src,
      ...(free && lk() && loaders.size ? { loaders: order.filter((l) => loaders.has(l)) } : {}),
      ...(free && vers.size ? { versions: [...vers] } : {}),
      ...(e ? { environment: e } : {}) };
  }
  function paintChips() {
    const box = $('#disc-chips'), list = items();
    const old = new Map([...box.children].map((n) => [n.dataset.key, n]));
    const nodes = list.map(([key, label, off, x]) => old.get(key) || el('button', { class: `disc-chip${x ? ' is-x' : ''}`, type: 'button', 'data-key': key, onclick: () => { off(); refresh(); } },
      el('span', {}, label), el('i', { html: XS })));
    if (nodes.length) nodes.push(old.get('*') || el('button', { class: 'disc-chip-clear', type: 'button', 'data-key': '*', onclick: resetAll }, _t("Сбросить всё")));
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
  const HEAD_RU = { categories: _t("Категории"), features: _t("Особенности"), resolutions: _t("Разрешение"), 'performance impact': _t("Нагрузка") };

  const CAT_RU = {
    adventure: _t("Приключения"), cursed: _t("Странное"), decoration: _t("Декор"), economy: _t("Экономика"), equipment: _t("Снаряжение"),
    food: _t("Еда"), 'game-mechanics': _t("Механики"), library: _t("Библиотеки"), magic: _t("Магия"), management: _t("Управление"),
    minigame: _t("Мини-игры"), mobs: _t("Мобы"), optimization: _t("Оптимизация"), social: _t("Общение"), storage: _t("Хранение"),
    technology: _t("Технологии"), transportation: _t("Транспорт"), utility: _t("Утилиты"), worldgen: _t("Генерация мира"),
    blocks: _t("Блоки"), combat: _t("Бой"), 'core-shaders': _t("Core-шейдеры"), entities: _t("Сущности"), environment: _t("Окружение"),
    fonts: _t("Шрифты"), gui: _t("Интерфейс"), items: _t("Предметы"), locale: _t("Язык"), models: _t("Модели"), audio: _t("Звук"),
    simplistic: _t("Минимализм"), themed: _t("Тематические"), tweaks: _t("Твики"), realistic: _t("Реализм"), modded: _t("Для модов"),
    'vanilla-like': _t("Как в ванилле"), challenging: _t("Испытания"), 'kitchen-sink': _t("Всё и сразу"),
    potato: _t("Очень слабые ПК"), low: _t("Низкая"), medium: _t("Средняя"), high: _t("Высокая"), screenshot: _t("Для скриншотов"),
    lightweight: _t("Лёгкие"), multiplayer: _t("Мультиплеер"), quests: _t("Квесты"), atmosphere: _t("Атмосфера"), bloom: _t("Свечение"), 'colored-lighting': _t("Цветной свет"),
    foliage: _t("Листва"), 'path-tracing': _t("Трассировка пути"), pbr: 'PBR', reflections: _t("Отражения"), shadows: _t("Тени"),
    cartoon: _t("Мультяшные"), fantasy: _t("Фэнтези"), 'semi-realistic': _t("Полуреализм"),
  };
  const catName = (n) => CAT_RU[n] || catLabels.get(n) || n.replace(/-/g, ' ').replace(/^./, (c) => c.toUpperCase());

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
    if (reset) { page = typeof reset === 'number' ? reset : 1; offset = paged() ? (page - 1) * per() : 0; box.dataset.state = 'loading'; }
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
    cards.forEach((c, i) => c.style.setProperty('--i', Math.min(i, 12)));
    if (reset) box.replaceChildren(...(cards.length ? cards : [empty(_t("Ничего не найдено"))]));
    else box.append(...cards);
    // Modrinth serves at most 10 000 results per query.
    const total = Math.min(res.total_hits, 10000);
    pages = Math.max(1, Math.ceil(total / per()));
    more.hidden = paged() || offset >= total;
    pager(total);
    lastSig = sig();
    if (reset && typeof reset === 'number') $('#view-discover').closest('.main')?.scrollTo({ top: 0, behavior: 'smooth' });
  }

  // ---------- Pages (Modrinth style) or endless scroll ----------
  const ARR = (d) => `<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">${d}</svg>`;
  function nums() {
    const out = [], add = (n) => out.push(n);
    const lo = Math.max(2, page - 1), hi = Math.min(pages - 1, page + 1);
    add(1);
    if (lo > 2) add(lo === 3 ? 2 : '…');
    for (let n = lo; n <= hi; n++) add(n);
    if (hi < pages - 1) add(hi === pages - 2 ? pages - 1 : '…');
    if (pages > 1) add(pages);
    return out;
  }
  function pager(total) {
    const box = $('#disc-pager');
    box.hidden = !paged() || !total;
    if (box.hidden) return;
    const go = (n) => () => { if (n !== page && n >= 1 && n <= pages) search(n); };
    const btn = (cls, html, n, lbl) => el('button', { type: 'button', class: `pg-b ${cls}`, 'aria-label': lbl, disabled: n < 1 || n > pages || n === page, html, onclick: go(n) });
    const list = nums().map((n) => (n === '…' ? el('span', { class: 'pg-gap' }, '…')
      : el('button', { type: 'button', class: 'pg-b pg-n', 'aria-current': n === page ? 'page' : null, onclick: go(n) }, String(n))));
    $('#disc-pages').replaceChildren(btn('pg-arrow', ARR('<g fill="none" fill-rule="evenodd"><path d="M24 0v24H0V0h24ZM12.593 23.258l-.011.002-.071.035-.02.004-.014-.004-.071-.035c-.01-.004-.019-.001-.024.005l-.004.01-.017.428.005.02.01.013.104.074.015.004.012-.004.104-.074.012-.016.004-.017-.017-.427c-.002-.01-.009-.017-.017-.018Zm.265-.113-.013.002-.185.093-.01.01-.003.011.018.43.005.012.008.007.201.093c.012.004.023 0 .029-.008l.004-.014-.034-.614c-.003-.012-.01-.02-.02-.022Zm-.715.002a.023.023 0 0 0-.027.006l-.006.014-.034.614c0 .012.007.02.017.024l.015-.002.201-.093.01-.008.004-.011.017-.43-.003-.012-.01-.01-.184-.092Z"/><path fill="currentColor" d="M8.293 12.707a1 1 0 0 1 0-1.414l5.657-5.657a1 1 0 1 1 1.414 1.414L10.414 12l4.95 4.95a1 1 0 0 1-1.414 1.414l-5.657-5.657Z"/></g>'), page - 1, _t("Предыдущая")), ...list, btn('pg-arrow', ARR('<g fill="none" fill-rule="evenodd"><path d="M24 0v24H0V0h24ZM12.593 23.258l-.011.002-.071.035-.02.004-.014-.004-.071-.035c-.01-.004-.019-.001-.024.005l-.004.01-.017.428.005.02.01.013.104.074.015.004.012-.004.104-.074.012-.016.004-.017-.017-.427c-.002-.01-.009-.017-.017-.018Zm.265-.113-.013.002-.185.093-.01.01-.003.011.018.43.005.012.008.007.201.093c.012.004.023 0 .029-.008l.004-.014-.034-.614c-.003-.012-.01-.02-.02-.022Zm-.715.002a.023.023 0 0 0-.027.006l-.006.014-.034.614c0 .012.007.02.017.024l.015-.002.201-.093.01-.008.004-.011.017-.43-.003-.012-.01-.01-.184-.092Z"/><path fill="currentColor" d="M15.707 11.293a1 1 0 0 1 0 1.414l-5.657 5.657a1 1 0 1 1-1.414-1.414l4.95-4.95-4.95-4.95a1 1 0 0 1 1.414-1.414l5.657 5.657Z"/></g>'), page + 1, _t("Следующая")));
    $('#disc-per').value = String(per());
  }
  $('#disc-per').addEventListener('change', (e) => { setLook({ per_page: +e.target.value }); search(true); });
  // Endless mode: load the next page when the end comes into view.
  if (window.IntersectionObserver) new IntersectionObserver((es) => {
    const m = $('#disc-more');
    if (es[0].isIntersecting && !paged() && !m.hidden && S.view === 'discover' && !$('#disc-results').dataset.state) search(false);
  }, { rootMargin: '600px' }).observe($('#disc-more'));
  document.addEventListener('look:browse', () => { if (S.view === 'discover') search(true); });

  function card(h) {
    const k = kind;
    const on = k !== 'modpack' && have.has(h.project_id);
    const b = el('button', { class: `btn${on ? '' : ' btn-primary'}`, disabled: on, 'data-id': `discover.install.${h.slug}` }, el('span', { class: 'lbl' }, on ? _t("Установлен") : _t("Установить")));
    b.addEventListener('click', (e) => { e.stopPropagation(); install(b, h, k); });
    const open = () => window.FProject?.open(h.project_id, k, h);
    return el('article', { class: 'mod-card', 'data-project': h.project_id, tabindex: '0', onclick: open, onkeydown: (e) => { if (e.key === 'Enter' && e.target === e.currentTarget) open(); } },
      icon(h.icon_url, h.title),
      el('div', { class: 't' }, el('b', {}, h.title), el('span', {}, h.author)),
      el('p', {}, h.description),
      el('div', { class: 'foot' }, el('span', { class: 'dl' }, _t("{0} загрузок", [fmtN(h.downloads)])), b));
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
    toast(r.added.length > 1 ? _t("Установлено: {0}", [r.added.join(', ')]) : `${h.title} → ${name}`);
    if (at.instance === target && at.kind === kind) { setInstalled(b); refreshHave().then(markHave).catch(() => {}); }
    const st = window.FContent.state();
    if (at.instance === st.inst && at.kind === st.kind) window.FContent.reload();
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
    if (!items.length) { toast(k === 'datapack' ? _t("Миров пока нет") : _t("Нет подходящих сборок"), { error: true }); return; }
    pick.replaceChildren(el('div', { class: 'menu-label' }, _t("Установить в")), ...items.map((it) => {
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
    lbl.textContent = _t("Готовим…");
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
      lbl.textContent = _t("Установлена");
      S.instances = await call('instances.list');
      renderInstances();
      landed(created.id, _t("{0} установлена", [created.name]));
      if (created.blocked_files?.length) window.FBlocked?.after(created);
      else if (created.blocked?.length) toast(_t("Не скачано (автор разрешает только сайт CurseForge): {0}", [created.blocked.join(', ')]), { error: true });
    } catch (e) {
      clearInterval(poll);
      b.disabled = false;
      b.dataset.state = 'error';
      lbl.textContent = _t("Установить");
      fail(e);
    }
  }

  function setInstalled(b) {
    b.disabled = true;
    b.classList.remove('btn-primary');
    b.querySelector('.lbl').textContent = _t("Установлен");
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
    window.FNav?.note();
  });
  $('#disc-src').addEventListener('seg:change', (e) => {
    const s = e.detail.dataset.src;
    if (s === src) return;
    src = s;
    localStorage.setItem('fl.disc.src', s);
    cats = new Set(); excl = new Set(); env.clear(); oss = false;
    discover();
    window.FNav?.note();
  });
  $('#disc-target').addEventListener('change', (e) => { target = e.target.value || null; paintLoaders(); paintVersions(); paintChips(); paintWorlds().then(() => search(true)); });
  $('#disc-world').addEventListener('change', (e) => { dworld = e.target.value; search(true); });
  $('#disc-sort').addEventListener('change', (e) => { sort = e.target.value; search(true); });
  // Grid or list, remembered between sessions.
  const paintView = (v) => {
    $('#disc-results').dataset.layout = v;
    $$('#disc-view > button').forEach((b) => b.setAttribute('aria-selected', String(b.dataset.v === v)));
    requestAnimationFrame(() => $('#disc-view')._place?.(false));
  };
  paintView(localStorage.getItem('fl.disc.view') === 'list' ? 'list' : 'grid');
  $('#disc-view').addEventListener('seg:change', (e) => { const v = e.detail.dataset.v; localStorage.setItem('fl.disc.view', v); paintView(v); });
  $('#disc-hide').addEventListener('change', (e) => { hide = e.target.checked; search(true); });
  $('#disc-reset').addEventListener('click', resetAll);
  initFilters();
  $('#disc-more').addEventListener('click', () => search(false));
  document.addEventListener('keydown', (e) => {
    if (S.view !== 'discover' || !paged() || e.target.matches?.('input, textarea, select') || e.altKey || e.ctrlKey) return;
    if (e.key === 'PageDown' && page < pages) { e.preventDefault(); search(page + 1); }
    if (e.key === 'PageUp' && page > 1) { e.preventDefault(); search(page - 1); }
  });

  // The project page reuses the catalog's install flow and context.
  const context = () => ({ target: target && byId(target), kind, have });
  window.FMods = { instance, discover, install, context, icon, fmtN, supports, catName, scoped, unscope, scopeOf: () => scope };
})();// app.js may have opened a view before this script ran.
if (S.view === 'instance' && S.current) window.FMods.instance(byId(S.current));
if (S.view === 'discover') window.FMods.discover();