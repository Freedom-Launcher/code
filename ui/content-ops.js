'use strict';
// Dialogs of the «Контент» tab: a shared confirm (delete / disable / update,
// with the list of affected content) and «Заменить версию», Modrinth's
// version switcher — searchable list with channels, «Текущая» and
// incompatible versions, the changelog beside it.
(() => {
  const CDN = /^https:\/\/(?:cdn\.modrinth\.com|media\.forgecdn\.net)\/[^"'\\()\s]+$/;
  const CH = { release: ['R', 'Релиз'], beta: ['B', 'Бета'], alpha: ['A', 'Альфа'] };
  const fmtDate = (s) => { const d = new Date(s); return isNaN(d) ? '' : d.toLocaleDateString('ru', { day: 'numeric', month: 'short', year: 'numeric' }); };
  const fail = (e) => toast(String(e), { error: true });

  // Resolves true on submit, false when the dialog is closed any other way.
  let pending = null;
  const cdlg = $('#dlg-ct-confirm');
  function confirm({ title, text = '', list = [], ok = 'Удалить', danger = true }) {
    pending?.(false);
    $('#ctc-title').textContent = title;
    $('#ctc-text').textContent = text;
    $('#ctc-text').hidden = !text;
    const ul = $('#ctc-list');
    const shown = list.slice(0, 8);
    ul.replaceChildren(...shown.map((t) => el('li', {}, t)), list.length > shown.length ? el('li', { class: 'is-more' }, `и ещё ${list.length - shown.length}`) : null);
    ul.hidden = !list.length;
    const b = $('#ctc-submit');
    b.className = `btn ${danger ? 'btn-danger' : 'btn-primary'}`;
    b.querySelector('.lbl').textContent = ok;
    FUI.openDlg('dlg-ct-confirm');
    setTimeout(() => b.focus(), 40);
    return new Promise((res) => { pending = (v) => { pending = null; res(v); }; });
  }
  cdlg.addEventListener('submit', (e) => { e.preventDefault(); const p = pending; FUI.closeDlg(); p?.(true); });
  new MutationObserver(() => { if (!cdlg.hasAttribute('data-open')) setTimeout(() => pending?.(false), 0); }).observe(cdlg, { attributes: true, attributeFilter: ['data-open'] });

  // ---------- «Заменить версию» ----------
  const vdlg = $('#dlg-ct-ver');
  let cur = null, at = null, after = null, all = [], pickId = null, vseq = 0;
  const curV = () => all.find((v) => v.id === cur?.version_id);

  async function versions(m, loc, done) {
    cur = m; at = loc; after = done; all = []; pickId = null;
    const my = ++vseq;
    const ico = $('#ctv-ico');
    ico.style.backgroundImage = m.icon_url && CDN.test(m.icon_url) ? `url("${m.icon_url}")` : '';
    ico.textContent = m.icon_url && CDN.test(m.icon_url) ? '' : initials(m.title);
    $('#ctv-title').textContent = m.title;
    $('#ctv-cur').textContent = m.version_number ? `Сейчас ${m.version_number}` : m.file;
    $('#ctv-q').value = '';
    $('#ctv-all').checked = false;
    $('#ctv-list').replaceChildren(...[0, 1, 2, 3, 4, 5].map(() => el('div', { class: 'ctv-item ctv-sk' }, el('i', { class: 'sk' }), el('i', { class: 'sk' }))));
    $('#ctv-log').replaceChildren(el('div', { class: 'ctv-hint' }, 'Выберите версию, чтобы увидеть список изменений'));
    paintSubmit();
    FUI.openDlg('dlg-ct-ver');
    try {
      const vs = await call('mods.versions', { ...loc, file: m.file });
      if (my !== vseq) return;
      all = vs || [];
      const c = curV();
      // Opens on the newest fitting version above the current one, if any.
      const up = all.find((v) => v.compatible && v.id !== m.version_id && (!c || v.date_published > c.date_published));
      pickId = (up || c)?.id || null;
      paintList();
      paintLog();
    } catch (e) {
      if (my !== vseq) return;
      $('#ctv-list').replaceChildren(el('div', { class: 'ctv-hint' }, 'Не удалось загрузить версии'));
      fail(e);
    }
  }

  function paintList() {
    const q = $('#ctv-q').value.trim().toLowerCase();
    const showAll = $('#ctv-all').checked;
    const bad = all.filter((v) => !v.compatible).length;
    $('#ctv-all').closest('label').hidden = !bad;
    const list = all.filter((v) => (showAll || v.compatible || v.id === cur.version_id) && (!q || `${v.version_number} ${v.name}`.toLowerCase().includes(q)));
    const box = $('#ctv-list');
    if (!list.length) { box.replaceChildren(el('div', { class: 'ctv-hint' }, all.length ? 'Ничего не найдено' : 'Нет версий')); return; }
    box.replaceChildren(...list.map((v) => {
      const ch = CH[v.version_type] || CH.release;
      const b = el('button', { type: 'button', role: 'option', class: `ctv-item${v.compatible ? '' : ' is-bad'}`, 'aria-selected': String(v.id === pickId), 'data-v': v.id },
        el('i', { class: `ctv-ch is-${v.version_type || 'release'}`, 'data-tip': ch[1] }, ch[0]),
        el('span', { class: 'ctv-t' }, el('b', {}, v.version_number), el('span', {}, [fmtDate(v.date_published), (v.loaders || []).map((l) => LOADER_NAMES[l] || l).join(', ')].filter(Boolean).join(' · '))),
        v.id === cur.version_id ? el('span', { class: 'badge is-accent' }, 'Текущая') : !v.compatible ? el('span', { class: 'badge is-warn' }, 'Не подходит') : null);
      return b;
    }));
  }

  function paintLog() {
    const v = all.find((x) => x.id === pickId);
    const box = $('#ctv-log');
    paintSubmit();
    if (!v) { box.replaceChildren(el('div', { class: 'ctv-hint' }, 'Выберите версию, чтобы увидеть список изменений')); return; }
    const gv = v.game_versions || [];
    const meta = el('div', { class: 'ctv-meta' },
      el('b', {}, v.name && v.name !== v.version_number ? v.name : v.version_number),
      el('span', {}, [fmtDate(v.date_published), gv.length ? (gv.length > 3 ? `${gv[0]} – ${gv[gv.length - 1]}` : gv.join(', ')) : '', v.downloads ? `${FMods.fmtN(v.downloads)} загрузок` : ''].filter(Boolean).join(' · ')));
    const log = v.changelog?.trim() ? FMarkdown.render(v.changelog, (url) => invoke('open_link', { url }).catch(fail)) : el('p', { class: 'ctv-hint' }, 'Список изменений не указан');
    box.replaceChildren(meta, log);
    box.scrollTop = 0;
  }

  function paintSubmit() {
    const b = $('#ctv-submit');
    const v = all.find((x) => x.id === pickId);
    const c = curV();
    let t = 'Выберите версию';
    if (v && v.id === cur?.version_id) t = 'Уже установлена';
    else if (v && !v.compatible) t = `Установить ${v.version_number} всё равно`;
    else if (v && c && v.date_published < c.date_published) t = `Откатить до ${v.version_number}`;
    else if (v && c) t = `Обновить до ${v.version_number}`;
    else if (v) t = `Установить ${v.version_number}`;
    b.disabled = !v || v.id === cur?.version_id;
    b.className = `btn ${v && !v.compatible ? 'btn-danger' : 'btn-primary'}`;
    b.querySelector('.lbl').textContent = t;
    $('#ctv-warn').hidden = !v || v.id === cur?.version_id;
  }

  $('#ctv-list').addEventListener('click', (e) => {
    const b = e.target.closest('[data-v]');
    if (!b) return;
    pickId = b.dataset.v;
    $$('#ctv-list [data-v]').forEach((x) => x.setAttribute('aria-selected', String(x === b)));
    paintLog();
  });
  $('#ctv-list').addEventListener('keydown', (e) => {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    e.preventDefault();
    const bs = $$('#ctv-list [data-v]');
    const i = bs.findIndex((x) => x.dataset.v === pickId);
    const n = bs[Math.max(0, Math.min(bs.length - 1, i + (e.key === 'ArrowDown' ? 1 : -1)))];
    if (n) { n.click(); n.focus(); n.scrollIntoView({ block: 'nearest' }); }
  });
  $('#ctv-q').addEventListener('input', paintList);
  $('#ctv-all').addEventListener('change', paintList);
  vdlg.addEventListener('submit', async (e) => {
    e.preventDefault();
    const v = all.find((x) => x.id === pickId);
    if (!v || !cur) return;
    const r = await run($('#ctv-submit'), 'mods.switch', { ...at, file: cur.file, version: v.id }).catch(() => null);
    if (!r) return;
    FUI.closeDlg();
    toast(`${r.title} → ${v.version_number}`);
    after?.();
  });

  window.FContentOps = { confirm, versions };
})();