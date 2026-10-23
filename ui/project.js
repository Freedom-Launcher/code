// The project page, laid out like Modrinth's: a header with the install button,
// tabs Описание / Версии / Галерея, and a sidebar with compatibility, links and
// details. Installing goes through the catalog's flow (FMods.install), so a
// chosen instance, the «Установить в…» menu and modpacks behave the same.
(() => {
  const { $, $$, toast, openMenuAt } = FUI;
  const { invoke } = window.__TAURI__.core;
  const KIND_OF = { mod: 'mod', modpack: 'modpack', resourcepack: 'resourcepack', shader: 'shader', datapack: 'datapack', plugin: 'mod' };
  const PLATFORM = { iris: 'Iris', optifine: 'OptiFine', minecraft: 'Minecraft', datapack: _t("Датапак"), canvas: 'Canvas' };
  const CHANNEL = { release: ['R', _t("Релиз")], beta: ['B', _t("Бета")], alpha: ['A', _t("Альфа")] };
  const ENV = { required: 2, optional: 1, unsupported: 0 };
  const day = new Intl.DateTimeFormat(_LOC, { day: 'numeric', month: 'long', year: 'numeric' });
  const rel = new Intl.RelativeTimeFormat(_LOC, { numeric: 'auto' });
  const fail = (e) => toast(String(e), { error: true });
  const platform = (l) => LOADER_NAMES[l] || PLATFORM[l] || l;
  const isRelease = (g) => /^\d+\.\d+(\.\d+)?$/.test(g);
  // Version families: 1.21.x by its minor, and the yearly 26.x scheme after it.
  const family = (g) => { const [a, b] = g.split('.').map(Number); return a === 1 ? b : a * 100 + b; };

  let p = null;
  let kind = 'mod';
  let versions = [];
  let seq = 0;
  let back = 'discover';
  let tab = 'desc';
  let shown = 30;
  const f = { loader: '', game: '', channel: '' };

  function ago(iso) {
    const s = (Date.parse(iso) - Date.now()) / 1000;
    const steps = [[60, 'second', 1], [3600, 'minute', 60], [86400, 'hour', 3600], [2592000, 'day', 86400], [31536000, 'month', 2592000], [Infinity, 'year', 31536000]];
    const [, unit, div] = steps.find(([lim]) => Math.abs(s) < lim);
    return rel.format(Math.round(s / div), unit);
  }
  const date = (iso) => (iso ? day.format(new Date(iso)) : '—');

  // Runs of three or more families collapse to «1.16.5–1.21.4»; shorter ones stay per family.
  function gameRanges(list) {
    const rels = list.filter(isRelease).sort((a, b) => a.localeCompare(b, undefined, { numeric: true }));
    if (!rels.length) return list.slice(-1);
    const fam = new Map();
    for (const g of rels) { const k = family(g); if (!fam.has(k)) fam.set(k, []); fam.get(k).push(g); }
    const keys = [...fam.keys()].sort((a, b) => a - b);
    const first = (k) => fam.get(k)[0];
    const last = (k) => fam.get(k).at(-1);
    const span = (a, b) => (first(a) === last(b) ? first(a) : `${first(a)}–${last(b)}`);
    const out = [];
    for (let i = 0; i < keys.length;) {
      let j = i;
      while (j + 1 < keys.length && keys[j + 1] === keys[j] + 1) j++;
      if (j - i >= 2) out.push(span(keys[i], keys[j]));
      else for (let k = i; k <= j; k++) out.push(span(keys[k], keys[k]));
      i = j + 1;
    }
    return out.reverse();
  }

  function env() {
    const c = ENV[p.client_side] ?? 1;
    const s = ENV[p.server_side] ?? 1;
    if (c === 2 && s === 2) return _t("Клиент и сервер");
    if (c >= 1 && s === 0) return _t("Только клиент");
    if (c === 0 && s >= 1) return _t("Только сервер");
    if (c === 2) return _t("Клиент, на сервере по желанию");
    if (s === 2) return _t("Сервер, на клиенте по желанию");
    return _t("Клиент или сервер");
  }

  // Links to other Modrinth projects open here; everything else in the browser.
  function follow(url) {
    const m = /^https:\/\/modrinth\.com\/(mod|modpack|resourcepack|shader|datapack|plugin)\/([A-Za-z0-9_.-]{1,64})\/?$/.exec(url);
    if (m) { open(m[2], KIND_OF[m[1]]); return; }
    invoke('open_link', { url }).catch(fail);
  }
  const isCf = () => String(p?.id || '').startsWith('cf:');
  const page = () => p.web_url || `https://modrinth.com/${p.project_type || 'mod'}/${p.slug}`;
  const hit = () => ({ project_id: p.id, slug: p.slug, title: p.title });

  async function open(id, k, h) {
    const my = ++seq;
    if (S.view !== 'project') back = S.view || 'discover';
    kind = k || 'mod';
    tab = 'desc';
    shown = 30;
    p = null;
    versions = [];
    Object.assign(f, { loader: '', game: '', channel: '' });
    paintSkeleton(h);
    window.FNav?.project({ id, kind, title: h?.title || '', icon: h?.icon_url || '' });
    show('project');
    $('#view-project').closest('.main')?.scrollTo({ top: 0 });
    try {
      const [proj, vs] = await Promise.all([call('modrinth.project', { id }), call('modrinth.versions', { id })]);
      if (my !== seq) return;
      p = proj;
      $('#menu-proj [data-do="open"]').textContent = isCf() ? _t("Открыть на CurseForge") : _t("Открыть на Modrinth");
      versions = vs || [];
      if (!k) kind = p.loaders.length && p.loaders.every((l) => l === 'datapack') ? 'datapack' : KIND_OF[p.project_type] || 'mod';
      presetFilters();
      paint();
      window.FNav?.project({ id, kind, title: p.title, icon: p.icon_url || '' });
      if (S.view === 'project') window.FNav?.note();
    } catch (e) {
      if (my !== seq) return;
      $('#proj-side').replaceChildren();
      const retry = el('button', { class: 'btn', onclick: () => open(id, k, h) }, el('span', { class: 'lbl' }, _t("Повторить")));
      $('#proj-pane').replaceChildren(el('div', { class: 'table-empty' }, el('b', {}, _t("Не удалось открыть проект")), el('span', {}, String(e)), retry));
    }
  }

  function paintSkeleton(h) {
    $('#proj-tabs').hidden = true;
    $('#proj-for').hidden = true;
    $('#proj-head').replaceChildren(
      h ? FMods.icon(h.icon_url, h.title) : el('i', { class: 'mod-ico skel' }),
      el('div', { class: 'proj-title' }, h ? el('h1', {}, h.title) : el('div', { class: 'skel sk-t' }), h ? el('p', {}, h.description) : el('div', { class: 'skel sk-l' })));
    $('#proj-side').replaceChildren(...[0, 1, 2].map(() => el('section', { class: 'proj-card' }, el('div', { class: 'skel' }), el('div', { class: 'skel short' }))));
    $('#proj-pane').replaceChildren(el('div', { class: 'proj-card' }, ...[0, 1, 2, 3, 4].map(() => el('div', { class: 'skel' }))));
  }

  // With an instance chosen in «Каталог», versions start filtered to it, as in Modrinth.
  function presetFilters() {
    const t = FMods.context().target;
    if (!t || kind === 'modpack') return;
    if (kind === 'mod' && versions.some((v) => v.loaders.includes(t.loader))) f.loader = t.loader;
    if (versions.some((v) => v.game_versions.includes(t.version))) f.game = t.version;
  }

  function paint() {
    const ctx = FMods.context();
    const t = ctx.target;
    const forEl = $('#proj-for');
    forEl.hidden = !t || kind === 'modpack';
    if (t) forEl.replaceChildren(el('span', {}, _t("Установка в")), el('b', {}, t.name), el('span', {}, `${LOADER_NAMES[t.loader] || LOADER_NAMES.vanilla} ${t.version}`));
    paintHead(ctx);
    paintSide();
    const tabs = $('#proj-tabs');
    tabs.hidden = false;
    $('[data-tab="gallery"]', tabs).hidden = !p.gallery.length;
    $('[data-tab="versions"] .n', tabs).textContent = versions.length || '';
    $('[data-tab="gallery"] .n', tabs).textContent = p.gallery.length || '';
    $$('button', tabs).forEach((b) => b.setAttribute('aria-selected', String(b.dataset.tab === tab)));
    requestAnimationFrame(() => tabs._place?.(false));
    paintPane();
  }

  function paintHead(ctx) {
    const on = kind !== 'modpack' && ctx.have.has(p.id);
    const b = el('button', { class: `btn btn-lg${on ? '' : ' btn-primary'}`, disabled: on, 'data-id': 'project.install' }, el('span', { class: 'lbl' }, on ? _t("Установлен") : _t("Установить")));
    b.addEventListener('click', (e) => { e.stopPropagation(); FMods.install(b, hit(), kind); });
    const more = el('button', { class: 'btn btn-icon btn-lg', 'aria-label': _t("Ещё"), 'data-id': 'project.more' }, el('span', { class: 'dots' }, '⋯'));
    more.addEventListener('click', (e) => { e.stopPropagation(); const r = more.getBoundingClientRect(); openMenuAt($('#menu-proj'), r.right - 220, r.bottom + 6); });
    const stats = el('div', { class: 'proj-stats' },
      el('span', {}, el('b', {}, FMods.fmtN(p.downloads)), _t(" загрузок")),
      el('span', {}, el('b', {}, FMods.fmtN(p.followers)), isCf() ? _t(" лайков") : _t(" подписчиков")),
      p.updated ? el('span', {}, _t("обновлён {0}", [ago(p.updated)])) : null);
    const tags = el('div', { class: 'proj-tags' }, ...p.categories.filter((c) => !LOADER_NAMES[c] && !PLATFORM[c]).map((c) => el('span', { class: 'badge' }, FMods.catName(c))));
    $('#proj-head').replaceChildren(
      FMods.icon(p.icon_url, p.title),
      el('div', { class: 'proj-title' }, el('h1', {}, p.title), el('p', {}, p.description), stats, tags),
      el('div', { class: 'proj-actions' }, b, more));
  }

  const row = (label, value) => el('div', { class: 'proj-row' }, el('span', {}, label), el('b', {}, value));
  function linkBtn(label, url) {
    const a = el('a', { class: 'proj-link', href: url, title: url }, el('span', {}, label), el('i', { 'aria-hidden': 'true' }, '↗'));
    a.addEventListener('click', (e) => { e.preventDefault(); follow(url); });
    return a;
  }

  function paintSide() {
    const plats = p.loaders.filter((l) => !(kind === 'datapack' && l === 'datapack'));
    const sec = (title, ...kids) => el('section', { class: 'proj-card' }, el('h3', {}, title), ...kids);
    const envOn = !isCf() && (kind === 'mod' || kind === 'modpack');
    const compat = sec(_t("Совместимость"),
      el('div', { class: 'proj-sub' }, 'Minecraft: Java Edition'),
      el('div', { class: 'proj-chips' }, ...gameRanges(p.game_versions).map((g) => el('span', { class: 'badge' }, g))),
      plats.length ? el('div', { class: 'proj-sub' }, _t("Платформы")) : null,
      plats.length ? el('div', { class: 'proj-chips' }, ...plats.map((l) => el('span', { class: 'badge is-accent' }, platform(l)))) : null,
      envOn ? el('div', { class: 'proj-sub' }, _t("Окружение")) : null,
      envOn ? el('div', { class: 'proj-env' }, env()) : null);
    const links = [[_t("Исходный код"), p.source_url], [_t("Сообщить о проблеме"), p.issues_url], [_t("Вики"), p.wiki_url], ['Discord', p.discord_url],
      ...p.donation_urls.map((d) => [_t("Поддержать{0}", [d.platform ? ` · ${d.platform}` : '']), d.url])].filter(([, u]) => u);
    const lic = p.license && (p.license.name || p.license.id.replace(/^LicenseRef-/, '').replace(/-/g, ' '));
    $('#proj-side').replaceChildren(compat,
      ...(links.length ? [sec(_t("Ссылки"), el('div', { class: 'proj-links' }, ...links.map(([l, u]) => linkBtn(l, u))))] : []),
      sec(_t("Подробности"), lic ? row(_t("Лицензия"), lic) : null, row(_t("Опубликован"), date(p.published)), row(_t("Обновлён"), date(p.updated))));
  }

  function paintPane() {
    const pane = $('#proj-pane');
    if (tab === 'desc') pane.replaceChildren(el('article', { class: 'proj-card proj-desc' }, FMarkdown.render(p.body || p.description, follow)));
    else if (tab === 'gallery') pane.replaceChildren(gallery());
    else pane.replaceChildren(versionsPane());
  }

  // ---------- Versions ----------
  const fits = (v) => (!f.loader || v.loaders.includes(f.loader)) && (!f.game || v.game_versions.includes(f.game)) && (!f.channel || v.version_type === f.channel);
  // Only versions the chosen instance can take get an active button; the core re-checks.
  function fitsTarget(v) {
    const t = FMods.context().target;
    if (!t || kind === 'modpack' || kind === 'resourcepack') return true;
    const ok = t.loader === 'quilt' ? ['quilt', 'fabric'] : [t.loader];
    return v.game_versions.includes(t.version) && (kind !== 'mod' || v.loaders.some((l) => ok.includes(l)));
  }

  function select(label, key, opts) {
    const s = el('select', { class: 'field', 'aria-label': label, 'data-id': `project.filter.${key}` },
      el('option', { value: '' }, label), ...opts.map(([v, t]) => el('option', { value: v, selected: f[key] === v }, t)));
    s.addEventListener('change', () => { f[key] = s.value; shown = 30; paintPane(); });
    return s;
  }

  function versionsPane() {
    const loaders = [...new Set(versions.flatMap((v) => v.loaders))];
    const games = [...new Set(versions.flatMap((v) => v.game_versions))].filter(isRelease)
      .sort((a, b) => b.localeCompare(a, undefined, { numeric: true }));
    const chans = [...new Set(versions.map((v) => v.version_type))].filter((c) => CHANNEL[c]);
    const list = versions.filter(fits);
    const bar = el('div', { class: 'proj-vbar' },
      loaders.length > 1 ? select(_t("Все платформы"), 'loader', loaders.map((l) => [l, platform(l)])) : null,
      games.length > 1 ? select(_t("Все версии игры"), 'game', games.map((g) => [g, g])) : null,
      chans.length > 1 ? select(_t("Все каналы"), 'channel', chans.map((c) => [c, CHANNEL[c][1]])) : null,
      el('span', { class: 'proj-count' }, _t("{0} из {1}", [list.length, versions.length])));
    const rows = list.slice(0, shown).map(versionRow);
    const box = el('div', { class: 'proj-versions' }, ...(rows.length ? rows : [el('div', { class: 'table-empty' }, el('b', {}, _t("Нет версий под эти фильтры")))]));
    const more = list.length > shown ? el('button', { class: 'btn disc-more', onclick: () => { shown += 30; paintPane(); } }, el('span', { class: 'lbl' }, _t("Ещё"))) : null;
    return el('div', {}, bar, box, more);
  }

  function versionRow(v) {
    const [ab, name] = CHANNEL[v.version_type] || ['?', v.version_type];
    const ok = fitsTarget(v);
    const b = el('button', { class: 'btn btn-sm', disabled: !ok, title: ok ? null : _t("Не подходит к выбранной сборке"), 'data-id': `project.version.install.${v.id}` }, el('span', { class: 'lbl' }, ok ? _t("Установить") : _t("Не подходит")));
    b.addEventListener('click', (e) => { e.stopPropagation(); FMods.install(b, hit(), kind, v.id); });
    const r = el('div', { class: 'proj-vrow', tabindex: '0', 'data-version': v.id },
      el('i', { class: `proj-ch ch-${v.version_type}`, title: name }, ab),
      el('div', { class: 'proj-vname' }, el('b', {}, v.name || v.version_number), el('span', {}, v.version_number)),
      el('div', { class: 'proj-vmeta' }, el('span', {}, gameRanges(v.game_versions).slice(0, 3).join(', ')), el('span', {}, v.loaders.map(platform).join(', '))),
      el('div', { class: 'proj-vmeta' }, el('span', {}, date(v.date_published)), el('span', {}, _t("{0} загрузок", [FMods.fmtN(v.downloads)]))),
      b);
    const toggle = () => {
      const next = r.nextElementSibling;
      if (next?.classList.contains('proj-log')) { next.remove(); r.removeAttribute('aria-expanded'); return; }
      r.setAttribute('aria-expanded', 'true');
      r.after(el('div', { class: 'proj-log' }, v.changelog?.trim() ? FMarkdown.render(v.changelog, follow) : el('p', { class: 'muted' }, _t("Список изменений не указан"))));
    };
    r.addEventListener('click', toggle);
    r.addEventListener('keydown', (e) => { if (e.key === 'Enter' && e.target === r) toggle(); });
    return r;
  }

  // ---------- Gallery ----------
  function gallery() {
    return el('div', { class: 'proj-gallery' }, ...p.gallery.map((g, i) =>
      el('button', { class: 'proj-shot', onclick: () => lightbox(i), 'data-id': `project.gallery.${i}` },
        el('img', { src: g.url, alt: g.title || '', loading: 'lazy' }),
        g.title || g.description ? el('span', { class: 'cap' }, el('b', {}, g.title || ''), g.description ? el('small', {}, g.description) : null) : null)));
  }

  const lb = el('div', { class: 'proj-lightbox', hidden: true, 'data-id': 'project.lightbox' });
  document.body.append(lb);
  let li = 0;
  function lightbox(i) {
    li = (i + p.gallery.length) % p.gallery.length;
    const g = p.gallery[li];
    const nav = (d, s) => el('button', { class: `btn btn-icon lb-${s}`, 'aria-label': s === 'prev' ? _t("Назад") : _t("Дальше"), onclick: (e) => { e.stopPropagation(); lightbox(li + d); } }, s === 'prev' ? '‹' : '›');
    lb.replaceChildren(
      el('figure', { onclick: (e) => e.stopPropagation() }, el('img', { src: g.url, alt: g.title || '' }),
        g.title || g.description ? el('figcaption', {}, el('b', {}, g.title || ''), g.description ? el('span', {}, g.description) : null) : null),
      p.gallery.length > 1 ? nav(-1, 'prev') : null, p.gallery.length > 1 ? nav(1, 'next') : null,
      el('button', { class: 'btn btn-icon lb-close', 'aria-label': _t("Закрыть"), onclick: closeLb }, '✕'));
    lb.hidden = false;
  }
  function closeLb() { lb.hidden = true; }
  lb.addEventListener('click', closeLb);
  addEventListener('keydown', (e) => {
    if (lb.hidden) return;
    if (e.key === 'Escape') closeLb();
    if (e.key === 'ArrowLeft') lightbox(li - 1);
    if (e.key === 'ArrowRight') lightbox(li + 1);
  });

  // ---------- Wiring ----------
  document.body.append($('#menu-proj'));
  $('#proj-tabs').addEventListener('seg:change', (e) => { tab = e.detail.dataset.tab; if (p) paintPane(); });
  $('#proj-back').addEventListener('click', () => window.FNav?.back() || show(back === 'project' ? 'discover' : back));
  $('#menu-proj [data-do="open"]').addEventListener('click', () => p && invoke('open_link', { url: page() }).catch(fail));
  $('#menu-proj [data-do="copy"]').addEventListener('click', () => {
    if (!p) return;
    if (!navigator.clipboard) { fail(_t("Буфер обмена недоступен")); return; }
    navigator.clipboard.writeText(page()).then(() => toast(_t("Ссылка скопирована")), fail);
  });

  window.FProject = { open };
})();