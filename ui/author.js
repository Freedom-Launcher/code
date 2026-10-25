'use strict';
// The author or organization page: who they are, who they work with, and every
// project they publish — browsable and installable here instead of in a browser.
// Data comes from catalog.person (Modrinth users and organizations, CurseForge authors).
(() => {
  const { $, $$, toast } = FUI;
  const { invoke } = window.__TAURI__.core;
  const RIMG = navigator.userAgent.includes('Windows') ? 'http://rimg.localhost' : 'rimg://localhost';
  const imgSrc = (u) => (!u || !/^https:\/\/[^\s"'<>\\]+$/.test(u) ? '' : /^https:\/\/cdn\.modrinth\.com\//.test(u) ? u : `${RIMG}/${encodeURIComponent(u)}`);
  const ONE = { mod: _t("Мод"), modpack: _t("Сборка"), resourcepack: _t("Ресурспак"), shader: _t("Шейдер"), datapack: _t("Датапак") };
  const MANY = { modpack: _t("Сборки"), mod: _t("Моды"), resourcepack: _t("Ресурспаки"), shader: _t("Шейдеры"), datapack: _t("Датапаки") };
  const ORDER = ['modpack', 'mod', 'resourcepack', 'shader', 'datapack'];
  const SORTS = [['downloads', _t("По загрузкам")], ['follows', _t("По подписчикам")], ['updated', _t("Обновлённые")], ['newest', _t("Новые")], ['name', _t("По имени")]];
  const STEP = 24;
  const day = new Intl.DateTimeFormat(_LOC, { day: 'numeric', month: 'long', year: 'numeric' });
  const svg = (d) => `<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path fill-rule="evenodd" d="${d}"/></svg>`;
  const EXT = svg('M21 4a1 1 0 0 0-1-1h-5a1 1 0 1 0 0 2h2.586l-2.11 2.11a6.007 6.007 0 0 1 1.415 1.413L19 6.414V9a1 1 0 1 0 2 0zm-5.523 3.11A5.973 5.973 0 0 0 12 6H5a2 2 0 0 0-2 2v11a2 2 0 0 0 2 2h11a2 2 0 0 0 2-2v-7c0-1.296-.41-2.496-1.11-3.477l-6.183 6.184a1 1 0 0 1-1.414-1.414z');
  const COPY = svg('M9 2a2 2 0 0 0-2 2v2h2V4h11v11h-2v2h2a2 2 0 0 0 2-2V4a2 2 0 0 0-2-2zM4 7a2 2 0 0 0-2 2v11a2 2 0 0 0 2 2h11a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2z');
  const FIND = svg('M10.5 2a8.5 8.5 0 1 0 5.262 15.176l3.652 3.652a1 1 0 0 0 1.414-1.414l-3.652-3.652A8.5 8.5 0 0 0 10.5 2M4 10.5a6.5 6.5 0 1 1 13 0 6.5 6.5 0 0 1-13 0');
  const NEXT = svg('M15.707 11.293a1 1 0 0 1 0 1.414l-5.657 5.657a1 1 0 1 1-1.414-1.414l4.95-4.95-4.95-4.95a1 1 0 0 1 1.414-1.414l5.657 5.657Z');
  const fail = (e) => toast(String(e), { error: true });
  const ts = (s) => Date.parse(s) || 0;
  const CMP = {
    downloads: (a, b) => b.downloads - a.downloads,
    follows: (a, b) => b.follows - a.follows,
    updated: (a, b) => ts(b.updated) - ts(a.updated),
    newest: (a, b) => ts(b.published) - ts(a.published),
    name: (a, b) => a.title.localeCompare(b.title, _LOC),
  };

  let who = null; // { kind, id, name, avatar } of the open page
  let prof = null;
  let seq = 0;
  let shown = STEP;
  const f = { kind: '', q: '', sort: 'downloads' };

  /** An avatar: the picture when there is one, else the initial. Organizations are square. */
  function ava(c, cls = 'ct-ava') {
    const n = el('i', { class: `${cls}${c.kind === 'org' ? ' is-org' : ''}`, 'aria-hidden': 'true' });
    const src = imgSrc(c.avatar);
    if (src) n.append(el('img', { src, alt: '', draggable: false, loading: 'lazy', onerror: () => n.replaceChildren(initials(c.name)) }));
    else n.textContent = initials(c.name);
    return n;
  }
  const role = (r) => (r === 'Owner' ? _t("Владелец") : r);

  /** A pill with the avatar and name that opens that person's or organization's page. */
  function person(c, sub) {
    const b = el('button', { type: 'button', class: 'auth-person', title: c.name, 'data-id': `author.person.${c.kind}.${c.id}` },
      ava(c), el('span', {}, c.name), sub ? el('small', {}, sub) : null);
    b.addEventListener('click', (e) => { e.stopPropagation(); open(c.kind, c.id, c); });
    return b;
  }

  async function open(kind, id, hint) {
    const my = ++seq;
    who = { kind, id: String(id), name: hint?.name || '', avatar: hint?.avatar || '' };
    prof = null;
    shown = STEP;
    Object.assign(f, { kind: '', q: '', sort: 'downloads' });
    paintSkeleton();
    window.FNav?.author({ ...who });
    show('author');
    $('#view-author').closest('.main')?.scrollTo({ top: 0 });
    try {
      const r = await call('catalog.person', { kind, id: who.id });
      if (my !== seq) return;
      prof = r;
      Object.assign(who, { name: r.creator.name, avatar: r.creator.avatar || '' });
      paint();
      window.FNav?.author({ ...who });
      if (S.view === 'author') window.FNav?.note();
    } catch (e) {
      if (my !== seq) return;
      const retry = el('button', { class: 'btn', onclick: () => open(kind, id, hint) }, el('span', { class: 'lbl' }, _t("Повторить")));
      $('#auth-body').replaceChildren(el('div', { class: 'table-empty auth-fail' }, el('b', {}, _t("Не удалось открыть профиль")), el('span', {}, String(e)), retry));
    }
  }

  function paintSkeleton() {
    $('#auth-head').replaceChildren(
      who.name ? ava(who, 'auth-ava') : el('i', { class: 'auth-ava skel' }),
      el('div', { class: 'auth-title' }, who.name ? el('h1', {}, who.name) : el('div', { class: 'skel sk-t' }), el('div', { class: 'skel sk-l' })));
    $('#auth-body').replaceChildren(
      el('div', { class: 'auth-stats' }, ...[0, 1, 2].map(() => el('div', { class: 'auth-stat' }, el('div', { class: 'skel sk-s' }), el('div', { class: 'skel sk-n' })))),
      el('div', { class: 'mod-grid auth-grid' }, ...[0, 1, 2, 3, 4, 5].map(() => el('div', { class: 'mod-card is-skel' }, el('i', { class: 'mod-ico skel' }), el('div', { class: 'skel sk-l' })))));
  }

  function paint() {
    const c = prof.creator;
    const kicker = el('div', { class: 'auth-kicker' },
      el('span', { class: 'badge is-accent' }, c.kind === 'org' ? _t("Организация") : _t("Автор")),
      el('span', { class: 'badge' }, c.kind === 'cf' ? 'CurseForge' : 'Modrinth'));
    const web = el('button', { class: 'btn', 'data-id': 'author.open', html: EXT }, el('span', { class: 'lbl' }, _t("Открыть в браузере")));
    web.addEventListener('click', () => invoke('open_link', { url: c.url }).catch(fail));
    const copy = el('button', { class: 'btn btn-icon', 'aria-label': _t("Скопировать ссылку"), 'data-tip': _t("Скопировать ссылку"), 'data-id': 'author.copy', html: COPY });
    copy.addEventListener('click', () => {
      if (!navigator.clipboard) { fail(_t("Буфер обмена недоступен")); return; }
      navigator.clipboard.writeText(c.url).then(() => toast(_t("Ссылка скопирована")), fail);
    });
    $('#auth-head').replaceChildren(ava(c, 'auth-ava'),
      el('div', { class: 'auth-title' }, kicker, el('h1', {}, c.name), prof.bio ? el('p', { class: 'auth-bio' }, prof.bio) : null),
      el('div', { class: 'auth-actions' }, web, copy));
    paintBody();
  }

  function paintBody() {
    const c = prof.creator;
    const list = prof.projects;
    const top = list.length > 1 ? [...list].sort(CMP.downloads)[0] : null;
    const stat = (label, value, cls = '') => el('div', { class: `auth-stat${cls}` }, el('span', {}, label), el('b', {}, value));
    let hit = null;
    if (top) {
      hit = el('button', { type: 'button', class: 'auth-stat is-text is-link', title: top.title, 'data-id': 'author.top' },
        el('span', {}, _t("Самый популярный")), el('b', {}, top.title), el('i', { class: 'go', 'aria-hidden': 'true', html: NEXT }));
      hit.addEventListener('click', () => openWork(top));
    }
    const stats = el('div', { class: 'auth-stats' },
      stat(_t("Проекты"), String(list.length)),
      stat(_t("Загрузки"), FMods.fmtN(prof.downloads)),
      prof.follows ? stat(c.kind === 'cf' ? _t("Лайки") : _t("Подписчики"), FMods.fmtN(prof.follows)) : null,
      prof.created && ts(prof.created) ? stat(_t("На Modrinth с"), day.format(new Date(prof.created)), ' is-text') : null,
      hit);
    const ppl = c.kind === 'org' ? prof.members : prof.orgs;
    const people = ppl.length ? el('div', { class: 'auth-people', 'data-id': 'author.people' },
      el('span', { class: 'auth-lbl' }, c.kind === 'org' ? _t("Участники") : _t("Организации")),
      ...ppl.slice(0, 24).map((m) => person(m, m.role ? role(m.role) : ''))) : null;

    const counts = new Map();
    for (const w of list) counts.set(w.kind, (counts.get(w.kind) || 0) + 1);
    const kinds = ORDER.filter((k) => counts.has(k));
    if (f.kind && !counts.has(f.kind)) f.kind = '';
    const pill = (k, label, n) => {
      const b = el('button', { type: 'button', class: 'ct-pill', 'data-k': k, 'aria-selected': String(f.kind === k), 'data-id': `author.kind.${k || 'all'}` }, label, el('span', {}, String(n)));
      b.addEventListener('click', () => { f.kind = k; shown = STEP; syncPills(); paintGrid(); });
      return b;
    };
    const pills = kinds.length > 1 ? el('div', { class: 'ct-pills auth-pills', id: 'auth-pills', 'data-id': 'author.kinds' },
      pill('', _t("Все"), list.length), ...kinds.map((k) => pill(k, MANY[k], counts.get(k)))) : null;
    const q = el('input', { class: 'field', id: 'auth-q', value: f.q, placeholder: _t("Поиск по проектам"), 'data-id': 'author.search' });
    q.addEventListener('input', () => { f.q = q.value; shown = STEP; paintGrid(); });
    const sort = el('select', { class: 'field auth-sort', 'aria-label': _t("Сортировка"), 'data-id': 'author.sort' },
      ...SORTS.map(([v, t]) => el('option', { value: v, selected: f.sort === v }, t)));
    sort.addEventListener('change', () => { f.sort = sort.value; shown = STEP; paintGrid(); });
    const bar = list.length > 1 ? el('div', { class: 'auth-bar' }, el('label', { class: 'field-wrap grow', html: FIND }, q), sort) : null;
    const more = el('button', { class: 'btn disc-more', id: 'auth-more', hidden: true, 'data-id': 'author.more', onclick: () => { shown += STEP; paintGrid(); } },
      el('span', { class: 'lbl' }, _t("Показать ещё")));
    $('#auth-body').replaceChildren(stats, people, pills, bar, el('div', { class: 'mod-grid auth-grid', id: 'auth-grid', 'data-id': 'author.projects' }), more);
    paintGrid();
  }

  function syncPills() {
    $$('#auth-pills .ct-pill').forEach((b) => b.setAttribute('aria-selected', String(b.dataset.k === f.kind)));
  }

  function paintGrid() {
    const grid = $('#auth-grid');
    const more = $('#auth-more');
    if (!grid || !prof) return;
    if (!prof.projects.length) {
      grid.replaceChildren(el('div', { class: 'table-empty' }, el('b', {}, _t("Проектов нет"))));
      more.hidden = true;
      return;
    }
    const q = f.q.trim().toLowerCase();
    const list = prof.projects
      .filter((w) => (!f.kind || w.kind === f.kind) && (!q || `${w.title}\n${w.description}`.toLowerCase().includes(q)))
      .sort((a, b) => (a.archived - b.archived) || CMP[f.sort](a, b));
    if (!list.length) {
      const reset = el('button', { class: 'btn', 'data-id': 'author.reset', onclick: () => { f.kind = ''; f.q = ''; const i = $('#auth-q'); if (i) i.value = ''; syncPills(); paintGrid(); } },
        el('span', { class: 'lbl' }, _t("Сбросить")));
      grid.replaceChildren(el('div', { class: 'table-empty' }, el('b', {}, _t("Ничего не найдено")), reset));
      more.hidden = true;
      return;
    }
    const have = FMods.context().have;
    grid.replaceChildren(...list.slice(0, shown).map((w) => card(w, have)));
    more.hidden = list.length <= shown;
  }

  const openWork = (w) => window.FProject?.open(w.project_id, w.kind, { project_id: w.project_id, slug: w.slug, title: w.title, description: w.description, icon_url: w.icon_url });

  function card(w, have) {
    const on = w.kind !== 'modpack' && !!have?.has(w.project_id);
    const b = el('button', { class: `btn${on ? '' : ' btn-primary'}`, disabled: on, 'data-id': `author.install.${w.slug || w.project_id}` },
      el('span', { class: 'lbl' }, on ? _t("Установлен") : _t("Установить")));
    b.addEventListener('click', (e) => { e.stopPropagation(); FMods.install(b, { project_id: w.project_id, slug: w.slug, title: w.title }, w.kind); });
    const go = () => openWork(w);
    const meta = [ONE[w.kind] || w.kind, w.mc].filter(Boolean).join(' · ');
    return el('article', { class: `mod-card${w.archived ? ' is-archived' : ''}`, 'data-project': w.project_id, tabindex: '0', onclick: go, onkeydown: (e) => { if (e.key === 'Enter' && e.target === e.currentTarget) go(); } },
      FMods.icon(w.icon_url, w.title),
      el('div', { class: 't' }, el('b', {}, w.title), el('span', {}, meta, w.archived ? el('i', { class: 'auth-arch' }, _t("Архив")) : null)),
      el('p', {}, w.description),
      el('div', { class: 'foot' }, el('span', { class: 'dl' }, _t("{0} загрузок", [FMods.fmtN(w.downloads)])), b));
  }

  $('#auth-back').addEventListener('click', () => window.FNav?.back() || show('discover'));

  window.FAuthor = { open, person, ava, role };
})();