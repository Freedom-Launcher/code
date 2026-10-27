// Share-by-code: an instance's recipe as a link or one string («Поделиться сборкой»), and a new
// instance built from such a string («Новая сборка → По коду»). See core `share`.
(() => {
  const { $, toast, openDlg, closeDlg, closeMenu, progress } = window.FUI;
  const clipFail = () => toast(_t("Буфер обмена недоступен"), { error: true });

  // Share (0.3.44): pick what goes in, like the .mrpack export, then a short link or an offline code.
  const KIND = [['m', _t("Моды")], ['r', _t("Ресурспаки")], ['s', _t("Шейдеры")]];
  const list = $('#sh-list'), out = $('#sh-out'), box = $('#sh-code'), hint = $('#sh-hint'), q = $('#sh-q');
  const linkBtn = $('#sh-link'), codeBtn = $('#sh-codebtn');
  let inst = null, items = [], local = [], localCount = 0, pick = new Set(), busy = false;
  const rows = new Map(), heads = new Map();
  const bare = (e) => _t(String(e).replace(/^Error:\s*/, ''));

  function stale() { out.hidden = true; box.value = ''; }
  function sync() {
    for (const [k, b] of rows) b.setAttribute('aria-pressed', String(pick.has(k)));
    for (const [tag, hd] of heads) {
      const g = items.filter((x) => x.kind === tag), n = g.filter((x) => pick.has(x.key)).length;
      hd.setAttribute('aria-pressed', n === 0 ? 'false' : n === g.length ? 'true' : 'mixed');
      hd.querySelector('.ex-meta').textContent = `${n} / ${g.length}`;
    }
    $('#sh-count').textContent = items.length ? _t("Выбрано {0} из {1}", [pick.size, items.length]) : '';
    linkBtn.disabled = codeBtn.disabled = busy || !pick.size;
    $('#sh-all').disabled = $('#sh-none').disabled = busy || !items.length;
  }
  const toggle = (keys, on) => { for (const k of keys) on ? pick.add(k) : pick.delete(k); stale(); sync(); };
  function build() {
    rows.clear(); heads.clear();
    const kids = [];
    for (const [tag, name] of KIND) {
      const g = items.filter((x) => x.kind === tag);
      if (!g.length) continue;
      const hd = el('button', { type: 'button', class: 'disc-opt ex-row sh-group', 'data-id': `share.group.${tag}` },
        el('i'), el('span', {}, name), el('span', { class: 'ex-meta' }));
      hd.addEventListener('click', () => toggle(g.map((x) => x.key), hd.getAttribute('aria-pressed') !== 'true'));
      heads.set(tag, hd); kids.push(hd);
      for (const x of g) {
        const meta = [x.version, x.enabled ? '' : _t("выкл.")].filter(Boolean).join(' · ');
        const b = el('button', { type: 'button', class: 'disc-opt ex-row sh-item', 'data-id': `share.item.${x.key}`, 'data-q': x.title.toLowerCase() },
          el('i'), el('span', {}, x.title), el('span', { class: 'ex-meta' }, meta));
        b.addEventListener('click', () => toggle([x.key], !pick.has(x.key)));
        rows.set(x.key, b); kids.push(b);
      }
    }
    if (localCount) {
      kids.push(el('div', { class: 'sh-local-h' }, _t("Добавлены вручную — не передаются")));
      for (const f of local) kids.push(el('div', { class: 'sh-local' }, f.file));
      if (localCount > local.length) kids.push(el('div', { class: 'sh-local' }, `+${localCount - local.length}`));
    }
    if (!items.length) kids.unshift(el('div', { class: 'disc-none' }, _t("В сборке нет модов из каталога")));
    list.replaceChildren(...kids);
  }

  $('#mi-share').addEventListener('click', async () => {
    closeMenu();
    inst = actInst(); items = []; local = []; localCount = 0; pick = new Set(); busy = false;
    rows.clear(); heads.clear(); stale(); q.value = ''; q.hidden = true;
    list.replaceChildren(el('div', { class: 'disc-none' }, _t("Загрузка…")));
    sync();
    openDlg('dlg-share');
    try {
      const r = await call('instances.share_items', { instance: inst });
      items = r.items || []; local = r.local || []; localCount = r.local_count || 0;
      pick = new Set(items.filter((x) => x.enabled).map((x) => x.key));
      q.hidden = items.length < 12;
      build(); sync();
    } catch (e) { closeDlg(); toast(bare(e), { error: true }); }
  });
  q.addEventListener('input', () => {
    const t = q.value.trim().toLowerCase();
    for (const b of rows.values()) b.hidden = !!t && !b.dataset.q.includes(t);
  });
  q.addEventListener('keydown', (ev) => { if (ev.key === 'Enter') ev.preventDefault(); });
  $('#sh-all').addEventListener('click', () => toggle(items.map((x) => x.key), true));
  $('#sh-none').addEventListener('click', () => toggle(items.map((x) => x.key), false));

  function copy(text, kind, loud) {
    const ok = () => toast(kind === 'link' ? _t("Ссылка скопирована") : _t("Код скопирован"));
    if (!navigator.clipboard) { if (loud) clipFail(); return; }
    navigator.clipboard.writeText(text).then(ok, () => { if (loud) clipFail(); });
  }
  async function make(kind) {
    if (busy || !pick.size) return;
    const b = kind === 'link' ? linkBtn : codeBtn;
    busy = true; sync(); b.dataset.state = 'loading';
    try {
      const r = await call(kind === 'link' ? 'instances.share_link' : 'instances.share_code', { instance: inst, items: [...pick] });
      const text = kind === 'link' ? r.url : r.code;
      box.value = text; box.classList.toggle('is-link', kind === 'link');
      out.dataset.kind = kind; out.hidden = false;
      const parts = [kind === 'link'
        ? _t("Друг вставит её при создании новой сборки. Ссылка хранится на pastes.dev.")
        : _t("Длина кода: {0}. Работает без интернета.", [text.length])];
      if (r.local) parts.push(_t("Добавлены вручную и не попадут в код: {0}", [r.local]));
      hint.textContent = parts.join(' · ');
      b.dataset.state = 'success';
      setTimeout(() => { b.dataset.state = 'idle'; }, 1100);
      copy(text, kind, false);
      box.focus(); box.select();
    } catch (e) {
      b.dataset.state = 'idle';
      toast(bare(e), { error: true });
    } finally { busy = false; sync(); }
  }
  $('#dlg-share').addEventListener('submit', (ev) => { ev.preventDefault(); make('link'); });
  codeBtn.addEventListener('click', () => make('code'));
  $('#sh-copy').addEventListener('click', () => { if (box.value) copy(box.value, out.dataset.kind, true); });

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