'use strict';
// «Доктор сборки» (0.3.48): what would stop the game — mods for another loader or game
// version, missing or switched-off dependencies, two copies of one mod, declared conflicts,
// OptiFine next to Sodium, broken jars, packs dropped into mods/ — with one-click fixes,
// a check before every launch and a malware scan. Everything goes through doctor.*.
(() => {
  const { $, openDlg, closeDlg, toast } = FUI;
  const dlg = $('#dlg-doctor'), body = $('#dc-body'), sub = $('#dc-inst'), submit = $('#dc-submit'), skip = $('#dc-skip');
  const NAME = { vanilla: 'Vanilla', fabric: 'Fabric', quilt: 'Quilt', forge: 'Forge', neoforge: 'NeoForge' };
  const sv = (d) => `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${d}</svg>`;
  const IC = {
    error: sv('<circle cx="12" cy="12" r="9"/><path d="M12 7.5v5.5M12 16.5v.01"/>'),
    warn: sv('<path d="M10.3 4.2 2.9 17a2 2 0 0 0 1.7 3h14.8a2 2 0 0 0 1.7-3L13.7 4.2a2 2 0 0 0-3.4 0z"/><path d="M12 9.5v4M12 17v.01"/>'),
    ok: sv('<circle cx="12" cy="12" r="9"/><path d="m8 12.5 2.7 2.7L16 9.8"/>'),
    shield: sv('<path d="M12 3 5 6v5.5c0 4.3 3 8.1 7 9.5 4-1.4 7-5.2 7-9.5V6l-7-3z"/><path d="m9 12 2.2 2.2L15.5 10"/>'),
  };
  const LOCAL = new Set(['disable', 'enable', 'move']);
  const enabled = () => localStorage.getItem('fl.doctor.launch') !== 'off';
  const reports = new Map();
  let cur = null, data = null, seq = 0, mode = 'view', pending = null, busy = false, scan = null;

  const kids = (...a) => a.filter((x) => x != null && x !== false);
  const list = (a, n = 3) => (a.length <= n ? a.join(', ') : _t("{0} и ещё {1}", [a.slice(0, n).join(', '), a.length - n]));
  const loaderOf = (id) => byId(id)?.loader || 'vanilla';
  const errors = (r) => (r?.issues || []).filter((i) => i.level === 'error');
  const autoFix = (i) => !!i.fix && i.fix.op !== 'search';
  const btn = (t, fn, cls = '') => {
    const b = el('button', { type: 'button', class: `btn btn-sm ${cls}` }, el('span', { class: 'lbl' }, t));
    b.addEventListener('click', (e) => { e.preventDefault(); e.stopPropagation(); fn(b); });
    return b;
  };

  // [title, text, fix button label]
  function describe(i, id) {
    const n = i.names || [];
    switch (i.kind) {
      case 'broken': return [_t("Повреждённые файлы"), _t("Не открываются как мод: {0}", [list(n)]), _t("Отключить")];
      case 'wrong_loader': return [_t("Моды для {0}", [NAME[i.detail] || i.detail]), _t("Сборка на {0}, они не запустятся: {1}", [NAME[loaderOf(id)] || '', list(n)]), _t("Отключить")];
      case 'wrong_game': return [_t("Моды для другой версии игры"), _t("Не подходят к Minecraft {0}: {1}", [i.detail, list(n)]), _t("Отключить")];
      case 'duplicate': return [_t("Две копии «{0}»", [n[0]]), _t("С двумя копиями игра не запустится. Оставим новую."), _t("Отключить старые")];
      case 'conflict': return [_t("«{0}» и «{1}» несовместимы", [n[0], n[1]]), _t("Так указал автор мода. Оставьте один из них."), null];
      case 'optifine': return [_t("OptiFine мешает {0}", [n[n.length - 1]]), _t("С Sodium и его аналогами OptiFine не работает. Для шейдеров есть Iris."), _t("Отключить")];
      case 'missing_dep': return [_t("Не хватает «{0}»", [i.detail]), _t("Нужен для: {0}", [list(n)]), i.fix?.op === 'install' ? _t("Установить") : _t("Найти")];
      case 'dep_off': return [_t("«{0}» выключен", [i.detail]), _t("Нужен для: {0}", [list(n)]), _t("Включить")];
      case 'stray_pack': return [i.detail === 'shaderpacks' ? _t("Шейдер в папке модов") : _t("Ресурспак в папке модов"), _t("«{0}» лежит не там — перенесём в нужную папку.", [n[0]]), _t("Перенести")];
      case 'no_loader': return [_t("Моды в Vanilla"), _t("Без загрузчика моды не работают. Создайте сборку на Fabric, Quilt, Forge или NeoForge."), null];
      default: return [i.kind, list(n), null];
    }
  }

  async function check(id) {
    const r = await call('doctor.check', { instance: id });
    reports.set(id, r);
    return r;
  }
  async function refresh(id) {
    if (!id || loaderOf(id) === 'vanilla') { reports.delete(id); return; }
    try { await check(id); } catch { reports.delete(id); }
  }

  // Applies fixes; returns how many things changed, or -1 on failure.
  async function fixAll(id, issues, b) {
    if (b?.dataset.state === 'loading') return 0;
    if (b) b.dataset.state = 'loading';
    let n = 0;
    const failed = [];
    const local = issues.filter((i) => LOCAL.has(i.fix?.op)).map((i) => i.fix);
    try {
      if (local.length) n += (await call('doctor.fix', { instance: id, fixes: local }))?.changed || 0;
      for (const i of issues.filter((x) => x.fix?.op === 'install')) {
        try { await call('mods.install', { instance: id, project: i.fix.project, kind: 'mod' }); n++; } catch { failed.push(i.detail); }
      }
    } catch (e) {
      toast(String(e), { error: true });
      if (b) b.dataset.state = 'idle';
      return -1;
    }
    if (b) b.dataset.state = 'idle';
    if (n) toast(_t("Исправлено: {0}", [n]));
    if (failed.length) toast(_t("Не удалось поставить: {0}", [failed.join(', ')]), { error: true, ms: 6000 });
    await check(id).catch(() => {});
    if (S.view === 'instance' && S.current === id) window.FContent?.reload();
    if (cur === id && dlg.hasAttribute('data-open')) { data = reports.get(id) || data; paint(); }
    return n;
  }

  function search(id, q) {
    settle(false);
    closeDlg();
    if (S.view !== 'instance' || S.current !== id) openInstance(id);
    window.FContent?.setKind('mod');
    requestAnimationFrame(() => {
      $('#mods-add')?.click();
      requestAnimationFrame(() => { const f = $('#disc-q'); if (f) { f.value = q; f.dispatchEvent(new Event('input')); } });
    });
  }

  // ---------- Strip in the «Контент» tab ----------
  function strip(id, { skipDup = false } = {}) {
    const errs = errors(reports.get(id)).filter((i) => !(skipDup && i.kind === 'duplicate'));
    if (!errs.length) return null;
    const [title] = describe(errs[0], id);
    const fixable = errs.filter(autoFix);
    return el('div', { class: 'ct-dupes ct-doc' },
      el('div', { class: 'ct-dupes-b' }, el('b', {}, _t("Доктор нашёл проблемы в модах")),
        el('span', {}, errs.length > 1 ? _t("{0} и ещё {1}", [title, errs.length - 1]) : title)),
      el('div', { class: 'ct-dupes-acts' }, btn(_t("Подробнее"), () => open(id)),
        fixable.length ? btn(_t("Исправить всё"), (b) => fixAll(id, fixable, b), 'btn-primary') : null));
  }

  // ---------- Dialog ----------
  function row(i, id) {
    const [title, text, act] = describe(i, id);
    const tail = act && i.fix ? btn(act, (b) => (i.fix.op === 'search' ? search(id, i.fix.query) : fixAll(id, [i], b))) : null;
    return el('div', { class: 'pf-row dc-row', 'data-l': i.level },
      el('i', { class: 'pf-ico dc-ico', html: IC[i.level] || IC.warn }),
      el('div', { class: 't' }, el('b', {}, title), el('span', {}, text)), tail);
  }
  function head(r, id) {
    const errs = errors(r), n = r.issues.length;
    const tone = errs.length ? 'error' : n ? 'warn' : 'ok';
    const i = byId(id);
    const title = !n ? _t("Всё в порядке") : mode === 'launch' && errs.length ? _t("Есть проблемы, которые помешают запуску") : _t("Найдено проблем: {0}", [n]);
    const line = [NAME[i?.loader] || '', i?.version, `${r.mods} ${_tp(r.mods, 'мод', 'мода', 'модов')}`].filter(Boolean).join(' · ');
    return el('div', { class: 'pf-head dc-head' }, el('div', { class: 'dc-badge', 'data-l': tone, html: IC[tone] }),
      el('div', { class: 't' }, el('b', {}, title), el('span', {}, n ? line : `${line} · ${_t("Ничего не мешает запуску")}`)));
  }
  function safety(id) {
    const s = scan && scan.id === id ? scan : null, v = s?.r;
    let title = _t("Проверка на вирусы"), text = _t("Сверим файлы с Modrinth и поищем следы известных вирусов"), tone = 'shield', tail, notes = [];
    if (s?.busy) tail = el('span', { class: 'badge-sm pf-state is-warn' }, _t("Проверяем…"));
    else if (v?.suspects.length) {
      tone = 'error'; title = _t("Найден вредоносный код"); text = list(v.suspects.map((x) => x.name));
      tail = btn(_t("Отключить"), (b) => fixAll(id, [{ fix: { op: 'disable', files: v.suspects.map((x) => x.file) } }], b).then(() => { scan = null; paint(); }), 'btn-primary');
    } else if (v) {
      tone = 'ok'; title = _t("Угроз не найдено"); text = _t("Проверено модов: {0}", [v.total]);
      if (v.unknown.length) notes.push(_t("Не из каталога: {0}. Скачивайте моды только из надёжных мест.", [list(v.unknown.map((x) => x.name))]));
      if (!v.online) notes.push(_t("Нет связи с Modrinth — проверили только по сигнатурам"));
      tail = btn(_t("Проверить"), () => verify(id));
    } else tail = btn(_t("Проверить"), () => verify(id));
    return el('div', { class: 'pf-row dc-row', 'data-l': tone === 'shield' ? '' : tone },
      el('i', { class: 'pf-ico dc-ico', html: IC[tone] }),
      el('div', { class: 't' }, el('b', {}, title), el('span', {}, text), ...notes.map((t) => el('span', { class: 'pf-note' }, t))), tail);
  }
  async function verify(id) {
    scan = { id, busy: true };
    paint();
    try { const r = await call('doctor.verify', { instance: id }); if (scan?.id === id) scan = { id, r }; }
    catch (e) { scan = null; toast(String(e), { error: true }); }
    if (cur === id) paint();
  }
  function paint() {
    const r = data, id = cur;
    if (!r) return;
    const cb = el('input', { type: 'checkbox' });
    cb.checked = enabled();
    cb.addEventListener('change', () => localStorage.setItem('fl.doctor.launch', cb.checked ? 'on' : 'off'));
    body.replaceChildren(...kids(head(r, id),
      r.issues.length ? el('div', { class: 'pf-list' }, ...r.issues.map((i) => row(i, id))) : null,
      el('div', { class: 'pf-sec' }, _t("Безопасность")), el('div', { class: 'pf-list' }, safety(id)),
      el('label', { class: 'dc-auto' }, el('span', { class: 'switch' }, cb, el('i')), el('span', {}, _t("Проверять перед запуском")))));
    const fixable = r.issues.filter(autoFix);
    submit.hidden = !fixable.length;
    submit.querySelector('.lbl').textContent = mode === 'launch' ? _t("Исправить и играть") : _t("Исправить всё");
    skip.hidden = mode !== 'launch';
    skip.querySelector('.lbl').textContent = errors(r).length ? _t("Играть так") : _t("Играть");
  }
  function skel() {
    return el('div', { class: 'pf-skel' }, el('div', { class: 'pf-head' }, el('div', { class: 'dc-badge sk' }),
      el('div', { class: 't' }, el('b', { class: 'sk' }), el('span', { class: 'sk' }))),
      el('div', { class: 'pf-list' }, ...[0, 1, 2].map(() => el('div', { class: 'pf-row' }, el('i', { class: 'pf-ico sk' }),
        el('div', { class: 't' }, el('b', { class: 'sk' }), el('span', { class: 'sk' }))))));
  }
  async function open(id, m = 'view', r = null) {
    if (!id) return;
    closeDlg();
    cur = id; mode = m; data = r;
    if (scan?.id !== id) scan = null;
    sub.textContent = byId(id)?.name || '';
    dlg.dataset.mode = m;
    submit.hidden = true;
    skip.hidden = m !== 'launch';
    if (r) paint(); else body.replaceChildren(skel());
    openDlg('dlg-doctor');
    if (r) return;
    const my = ++seq;
    try { const x = await check(id); if (my === seq && cur === id) { data = x; paint(); } }
    catch (e) { if (my === seq) body.replaceChildren(el('div', { class: 'table-empty' }, el('b', {}, String(e)))); }
  }

  // ---------- Before launch ----------
  function settle(v) { const p = pending; pending = null; p?.(v); }
  // Resolves to false when the player chose not to start the game.
  async function beforeLaunch(id) {
    if (!enabled()) return true;
    if (busy) return false;
    if (loaderOf(id) === 'vanilla') return true;
    busy = true;
    try {
      let r = null;
      try { r = await Promise.race([check(id), new Promise((res) => setTimeout(() => res(null), 5000))]); } catch { return true; }
      if (!r || !errors(r).length) return true;
      return await new Promise((resolve) => { pending = resolve; open(id, 'launch', r); });
    } finally { busy = false; }
  }

  dlg.addEventListener('submit', async (ev) => {
    ev.preventDefault();
    if (!data || submit.dataset.state === 'loading') return;
    const n = await fixAll(cur, data.issues.filter(autoFix), submit);
    if (mode === 'launch' && n >= 0) { settle(true); closeDlg(); }
  });
  skip.addEventListener('click', () => { settle(true); closeDlg(); });
  new MutationObserver(() => { if (!dlg.hasAttribute('data-open')) settle(false); }).observe(dlg, { attributes: true, attributeFilter: ['data-open'] });
  $('#mi-doctor').addEventListener('click', () => { FUI.closeMenu(); open(actInst()); });
  window.FDoctor = { open, strip, refresh, beforeLaunch, check };
})();