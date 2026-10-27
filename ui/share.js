// Share-by-code: an instance's recipe as one string («Поделиться кодом»), and a new
// instance built from such a string («Новая сборка → По коду»). See core `share`.
(() => {
  const { $, toast, openDlg, closeDlg, closeMenu, progress } = window.FUI;
  const clipFail = () => toast(_t("Буфер обмена недоступен"), { error: true });

  $('#mi-share').addEventListener('click', async () => {
    closeMenu();
    const id = actInst();
    const box = $('#sh-code'), hint = $('#sh-hint'), copy = $('#sh-copy');
    box.value = ''; hint.textContent = _t("Загрузка…"); copy.disabled = true;
    openDlg('dlg-share');
    try {
      const r = await call('instances.share_code', { instance: id });
      box.value = r.code;
      const parts = [_t("Из каталога: {0}", [r.items])];
      if (r.local) parts.push(_t("Добавлены вручную и не попадут в код: {0}", [r.local]));
      hint.textContent = parts.join(' · ');
      copy.disabled = false;
      box.focus(); box.select();
    } catch (e) { closeDlg(); toast(String(e), { error: true }); }
  });
  $('#dlg-share').addEventListener('submit', (ev) => {
    ev.preventDefault();
    const v = $('#sh-code').value;
    if (!v) return;
    if (!navigator.clipboard) { clipFail(); return; }
    navigator.clipboard.writeText(v).then(() => { toast(_t("Код скопирован")); closeDlg(); }, clipFail);
  });

  // Quick icon change from the menus; the cropper is the same as in the instance settings.
  $('#mi-icon').addEventListener('click', () => {
    closeMenu();
    const i = byId(actInst());
    if (i) pickArt(i, 'icon');
  });

  const fc = $('#dlg-code');
  $('#c-code').addEventListener('click', () => {
    if (fc.dataset.busy) { openDlg('dlg-code'); return; }
    $('#fc-code').value = ''; $('#fc-name').value = '';
    $('#fc-status').hidden = true;
    const b = $('#fc-submit'); b.disabled = false; b.dataset.state = 'idle';
    openDlg('dlg-code');
    setTimeout(() => $('#fc-code').focus(), 40);
  });
  fc.addEventListener('submit', async (ev) => {
    ev.preventDefault();
    if (fc.dataset.busy) return;
    const code = $('#fc-code').value.trim();
    if (!code) { $('#fc-code').focus(); return; }
    const b = $('#fc-submit');
    const token = crypto.randomUUID?.() || `code-${Date.now()}`;
    fc.dataset.busy = '1'; b.disabled = true; b.dataset.state = 'loading';
    $('#fc-status').hidden = false;
    $('#fc-stage').textContent = _t("Читаем сборку"); $('#fc-count').textContent = '';
    progress($('#fc-bar'), null);
    const poll = setInterval(async () => {
      const p = await call('game.progress', { instance: token }).catch(() => null);
      if (!p || !(p.total_files > 0)) return;
      $('#fc-stage').textContent = _t("Скачиваем файлы");
      progress($('#fc-bar'), p.done_files / p.total_files);
      $('#fc-count').textContent = `${p.done_files} / ${p.total_files}`;
    }, 250);
    try {
      const r = await call('instances.from_code', { code, name: $('#fc-name').value.trim() || null, token });
      progress($('#fc-bar'), 1);
      b.dataset.state = 'success';
      if (fc.hasAttribute('data-open')) closeDlg();
      await reloadInstances();
      openInstance(r.instance.id);
      const bad = r.failed?.length || 0;
      toast(bad ? `${_t("Сборка создана")}. ${_t("Не установилось: {0}", [bad])} — ${r.failed[0]}` : _t("Сборка создана"), bad ? { error: true } : undefined);
    } catch (e) {
      b.dataset.state = 'error';
      toast(_t(String(e).replace(/^Error:\s*/, '')), { error: true });
    } finally {
      clearInterval(poll);
      delete fc.dataset.busy;
      b.disabled = false;
    }
  });
})();