'use strict';
// Import from other launchers (Э6): the core finds the instances of Prism / MultiMC,
// CurseForge, ATLauncher, GDLauncher, Modrinth App and the official launcher
// (`foreign.scan`), and copies their game content into new instances (`foreign.import`).
(() => {
  const SRC = { prism: 'Prism Launcher', multimc: 'MultiMC', curseforge: 'CurseForge', atlauncher: 'ATLauncher', gdlauncher: 'GDLauncher', modrinth: 'Modrinth App', minecraft: 'Minecraft Launcher', folder: _t("Папка") };
  const ORDER = Object.keys(SRC);
  const STAGE = { prepare: _t("Читаем сборку"), copy: _t("Копируем файлы"), done: _t("Готово") };
  const CHECK = '<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><g fill="none" fill-rule="evenodd"><path fill="currentColor" d="M21.192 5.465a1 1 0 0 1 0 1.414L9.95 18.122a1.1 1.1 0 0 1-1.556 0l-5.586-5.586a1 1 0 1 1 1.415-1.415l4.95 4.95L19.777 5.465a1 1 0 0 1 1.414 0Z"/></g></svg>';
  const plural = (n, a, b, c) => _tp(n, a, b, c);
  const doneKey = 'fl.foreign.done';
  const done = () => { try { return new Set(JSON.parse(localStorage.getItem(doneKey) || '[]')); } catch { return new Set(); } };
  const markDone = (p) => { const s = done(); s.add(p); localStorage.setItem(doneKey, JSON.stringify([...s].slice(-300))); };

  let found = [], picked = new Map(), busy = false, releases = null;
  const dlg = $('#dlg-foreign'), list = $('#fg-list'), submit = $('#fg-submit');

  async function versions() {
    if (releases) return releases;
    const r = await call('versions.list', { snapshots: false }).catch(() => ({ versions: [] }));
    releases = r.versions.filter((v) => v.type !== 'snapshot').map((v) => v.id);
    return releases;
  }
  const meta = (f) => [LOADER_NAMES[f.loader] || LOADER_NAMES.vanilla, f.loader_version, f.mc].filter(Boolean).join(' · ');
  const counts = (f) => [f.mods && `${f.mods} ${plural(f.mods, 'мод', 'мода', 'модов')}`, f.worlds && `${f.worlds} ${plural(f.worlds, 'мир', 'мира', 'миров')}`].filter(Boolean).join(' · ');

  function row(f, had) {
    const r = el('div', { class: 'fg-row', role: 'checkbox', tabindex: '0', 'aria-checked': String(picked.has(f.path)), 'data-id': `foreign.item.${f.source}` },
      el('i', { class: 'fg-check', 'aria-hidden': 'true', html: CHECK }),
      el('span', { class: 'fg-ico', 'aria-hidden': 'true' }, (f.name.trim()[0] || '?').toUpperCase()),
      el('span', { class: 'fg-t' }, el('b', {}, f.name), el('span', {}, f.mc ? meta(f) : [meta(f), counts(f)].filter(Boolean).join(' · '))));
    if (!f.mc) {
      const s = el('select', { class: 'field fg-ver', 'aria-label': _t("Версия игры") }, el('option', { value: '' }, _t("Версия…")));
      versions().then((vs) => s.append(...vs.map((v) => el('option', { value: v }, v))));
      s.addEventListener('click', (e) => e.stopPropagation());
      s.addEventListener('change', () => { f.pick = s.value; if (s.value && !picked.has(f.path)) toggle(r, f); else paintFoot(); });
      r.append(s);
    }
    if (had || f.mc) r.append(el('span', { class: 'fg-meta' }, had ? el('span', { class: 'badge' }, _t("Уже есть")) : counts(f)));
    r.addEventListener('click', () => toggle(r, f));
    r.addEventListener('keydown', (e) => { if (e.key === ' ' || e.key === 'Enter') { e.preventDefault(); toggle(r, f); } });
    return r;
  }
  function toggle(r, f) {
    if (busy) return;
    if (!picked.delete(f.path)) picked.set(f.path, f);
    r.setAttribute('aria-checked', String(picked.has(f.path)));
    paintFoot();
  }
  function paintFoot() {
    const ready = [...picked.values()].filter((f) => f.mc || f.pick);
    submit.disabled = busy || !ready.length;
    submit.querySelector('.lbl').textContent = ready.length > 1 ? _t("Импортировать · {0}", [ready.length]) : _t("Импортировать");
  }
  function paint() {
    const had = done();
    if (!found.length) {
      list.replaceChildren(el('div', { class: 'fg-empty' }, el('b', {}, _t("Сборок не найдено")), el('span', {}, _t("Укажите папку лаунчера или сборки"))));
      paintFoot();
      return;
    }
    const groups = ORDER.map((s) => [s, found.filter((f) => f.source === s)]).filter(([, l]) => l.length);
    list.replaceChildren(...groups.map(([s, l]) => el('section', { class: 'fg-group' },
      el('h3', { class: 'fg-gh' }, el('span', {}, SRC[s]), el('span', { class: 'num' }, String(l.length))),
      ...l.map((f) => row(f, had.has(f.path))))));
    paintFoot();
  }

  async function open() {
    if (busy) { openDlg('dlg-foreign'); return; }
    picked = new Map();
    $('#fg-status').hidden = true;
    list.dataset.state = 'loading';
    list.replaceChildren(...[0, 1, 2].map(() => el('div', { class: 'fg-row skel' })));
    paintFoot();
    openDlg('dlg-foreign');
    found = await call('foreign.scan').catch((e) => { toast(String(e), { error: true }); return []; });
    const had = done();
    found.forEach((f) => { if (f.mc && !had.has(f.path) && f.source !== 'minecraft') picked.set(f.path, f); });
    delete list.dataset.state;
    paint();
  }

  $('#fg-pick').addEventListener('click', async () => {
    const path = await invoke('pick_folder', { title: _t("Папка лаунчера или сборки") }).catch(() => null);
    if (!path) return;
    const more = await call('foreign.scan', { path }).catch((e) => { toast(String(e), { error: true }); return []; });
    if (!more.length) { toast(_t("В этой папке нет сборок"), { error: true }); return; }
    const known = new Set(found.map((f) => f.path));
    more.forEach((f) => { if (!known.has(f.path)) { found.push(f); if (f.mc) picked.set(f.path, f); } });
    paint();
  });

  dlg.addEventListener('submit', async (ev) => {
    ev.preventDefault();
    const todo = [...picked.values()].filter((f) => f.mc || f.pick);
    if (!todo.length || busy) return;
    busy = true;
    dlg.classList.add('is-importing');
    $('#fg-pick').disabled = true;
    paintFoot();
    const st = $('#fg-status');
    st.hidden = false;
    const made = [], failed = [];
    for (const [n, f] of todo.entries()) {
      const token = crypto.randomUUID();
      $('#fg-stage').textContent = `${todo.length > 1 ? _t("{0} из {1} · ", [n + 1, todo.length]) : ''}${f.name}`;
      $('#fg-count').textContent = '';
      progress($('#fg-bar'), null);
      const poll = setInterval(async () => {
        const p = await call('game.progress', { instance: token }).catch(() => null);
        if (!p || !p.total_bytes) return;
        progress($('#fg-bar'), p.done_bytes / p.total_bytes);
        $('#fg-count').textContent = `${STAGE[p.stage] || ''} · ${fmtBytes(p.done_bytes)} / ${fmtBytes(p.total_bytes)}`;
      }, 250);
      try {
        made.push(await call('foreign.import', { path: f.path, token, version: f.pick || null }));
        markDone(f.path);
      } catch (e) { failed.push(`${f.name}: ${e}`); } finally { clearInterval(poll); }
    }
    busy = false;
    dlg.classList.remove('is-importing');
    $('#fg-pick').disabled = false;
    st.hidden = true;
    S.instances = await call('instances.list');
    renderInstances();
    if (failed.length) toast(failed.join('\n'), { error: true });
    if (!made.length) { paint(); return; }
    closeDlg();
    if (made.length === 1) landed(made[0].id, _t("{0} импортирована", [made[0].name]));
    else toast(_t("Импортировано сборок: {0}", [made.length]), { action: { label: _t("Открыть"), run: () => show('home') } });
  });

  $('#c-foreign')?.addEventListener('click', open);
  window.FForeign = { open };
})();