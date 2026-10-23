// Modpacks from Modrinth on the instance page: update, change version, export to .mrpack.
(() => {
  const { $, toast, openDlg, closeDlg, closeMenu } = window.FUI;
  let cur = null, info = null, busy = false, picked = new Set();
  const plural = (n, a, b, c) => _tp(n, a, b, c);
  const size = (x) => (x < 1024 ? _t("{0} Б", [x]) : x < 1 << 20 ? _t("{0} КБ", [Math.round(x / 1024)]) : x < 1 << 30 ? _t("{0} МБ", [(x / 1048576).toFixed(1)]) : _t("{0} ГБ", [(x / 1073741824).toFixed(2)]));
  const TYPE_RU = { beta: _t("бета"), alpha: _t("альфа") };
  const label = (b, t) => { b.querySelector('.t').textContent = t; };

  async function instance(i) {
    cur = i?.id || null; info = null; paint();
    if (!i?.pack) return;
    const r = await call('modpacks.check', { instance: i.id }).catch(() => null);
    if (cur === i.id) { info = r; paint(); }
  }
  function paint() {
    const i = cur && byId(cur), p = i?.pack;
    $('#ip-pack').hidden = $('#ip-pack-sep').hidden = !p;
    if (p) $('#ip-pack').textContent = `${String(p.project_id || '').startsWith('cf:') ? 'CurseForge' : 'Modrinth'} ${p.version_number}`.trim();
    $('#mi-pack').hidden = !p;
    if (busy) return;
    const u = $('#ip-update'), up = !!(info?.update && info.latest);
    u.hidden = !up;
    if (up) u.dataset.tip = _t("Новая версия {0}", [info.latest.version_number]);
  }

  async function update(version) {
    const i = byId(cur);
    if (!i || busy) return;
    busy = true;
    const b = $('#ip-update'), token = crypto.randomUUID();
    b.hidden = false; b.disabled = true; b.dataset.state = 'loading';
    label(b, _t("Готовим…"));
    const poll = setInterval(async () => {
      const p = await call('game.progress', { instance: token }).catch(() => null);
      if (!p) return;
      const f = p.total_bytes > 0 ? p.done_bytes / p.total_bytes : p.total_files > 0 ? p.done_files / p.total_files : null;
      if (f != null) label(b, `${Math.floor(f * 100)}%`);
    }, 250);
    try {
      const r = await call('modpacks.update', { instance: i.id, token, ...(version ? { version } : {}) });
      await reloadInstances();
      renderInstancePage();
      window.FMods?.instance(byId(i.id));
      toast(_t("Сборка обновлена до {0}", [r?.pack?.version_number || _t("новой версии")]));
      window.FBlocked?.after(r);
    } catch (e) { toast(String(e), { error: true }); }
    finally {
      clearInterval(poll);
      busy = false; b.disabled = false; delete b.dataset.state;
      label(b, _t("Обновить"));
      instance(byId(i.id));
    }
  }
  $('#ip-update').addEventListener('click', () => update(null));

  // Change version: every build for this loader, newest first.
  $('#mi-pack').addEventListener('click', async () => {
    closeMenu();
    if (!info) info = await call('modpacks.check', { instance: cur }).catch((e) => { toast(String(e), { error: true }); return null; });
    if (!info?.pack) return;
    const sel = $('#pv-ver');
    sel.replaceChildren(...info.versions.map((v) => el('option', { value: v.id },
      [v.version_number, v.game_versions?.[0], TYPE_RU[v.version_type], v.id === info.pack.version_id ? _t("текущая") : null].filter(Boolean).join(' · '))));
    sel.value = info.pack.version_id;
    openDlg('dlg-packver');
  });
  $('#dlg-packver').addEventListener('submit', (ev) => {
    ev.preventDefault();
    const v = $('#pv-ver').value;
    closeDlg();
    if (v && v !== info?.pack?.version_id) update(v);
  });

  // Export: Modrinth's defaults pre-checked; hosted files are linked, the rest bundled.
  function row(c) {
    const b = el('button', { type: 'button', class: 'disc-opt ex-row', 'aria-pressed': String(picked.has(c.path)), 'data-id': `export.path.${c.path}` },
      el('i'), el('span', {}, c.path), el('span', { class: 'ex-meta' }, c.dir ? `${c.files} ${plural(c.files, 'файл', 'файла', 'файлов')} · ${size(c.size)}` : size(c.size)));
    b.addEventListener('click', () => {
      if (!picked.delete(c.path)) picked.add(c.path);
      b.setAttribute('aria-pressed', String(picked.has(c.path)));
      $('#ex-submit').disabled = !picked.size;
    });
    return b;
  }
  let exInst = null;
  $('#mi-export').addEventListener('click', async () => {
    closeMenu();
    exInst = actInst() || cur;
    const f = $('#dlg-export'), b = $('#ex-submit');
    delete f.dataset.done; $('#ex-done').hidden = true; label(b, _t("Экспортировать")); b.disabled = true;
    $('#ex-list').replaceChildren(el('div', { class: 'disc-none' }, _t("Загрузка…")));
    openDlg('dlg-export');
    const r = await call('instances.export_candidates', { instance: exInst }).catch((e) => { toast(String(e), { error: true }); return null; });
    if (!r) return;
    $('#ex-name').value = r.name; $('#ex-ver').value = r.version;
    picked = new Set(r.candidates.filter((c) => c.selected).map((c) => c.path));
    $('#ex-list').replaceChildren(...(r.candidates.length ? r.candidates.map(row) : [el('div', { class: 'disc-none' }, _t("Папка сборки пуста"))]));
    b.disabled = !picked.size;
  });
  $('#dlg-export').addEventListener('submit', async (ev) => {
    ev.preventDefault();
    const f = ev.currentTarget, b = $('#ex-submit');
    if (f.dataset.done) {
      invoke('open_exports_folder').catch((e) => toast(String(e), { error: true }));
      closeDlg();
      return;
    }
    b.disabled = true; b.dataset.state = 'loading';
    try {
      const r = await call('instances.export', { instance: exInst, paths: [...picked], name: $('#ex-name').value, version: $('#ex-ver').value });
      f.dataset.done = '1';
      $('#ex-done').replaceChildren(el('b', {}, r.file), el('span', {}, _t("{0} с Modrinth · {1} в архиве", [r.linked, r.bundled])));
      $('#ex-done').hidden = false;
      label(b, _t("Открыть папку"));
    } catch (e) { toast(String(e), { error: true }); }
    finally { b.disabled = false; delete b.dataset.state; }
  });

  window.FPacks = { instance, paint };
})();