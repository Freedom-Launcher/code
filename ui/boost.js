// «Ускорение сборки»: memory, Java flags and the well-known performance mods for one instance,
// checked and fixed from a single dialog (instance menu → «Ускорение сборки»).
(() => {
  const { $, openDlg, toast } = FUI;
  const dlg = $('#dlg-perf'), body = $('#pf-body'), submit = $('#pf-submit'), sub = $('#pf-inst');
  const IC = { memory: '<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><g fill="none" fill-rule="evenodd"><path fill="currentColor" d="M12 2a9.96 9.96 0 0 1 6.33 2.258l-5.037 5.034a3 3 0 1 0 1.5 1.614l-.085-.2 5.034-5.036A9.959 9.959 0 0 1 22 12c0 5.523-4.477 10-10 10S2 17.523 2 12 6.477 2 12 2m-7 9h-.938a8.079 8.079 0 0 0-.034 1.67l.034.33H5a1 1 0 0 0 .117-1.993zm14.938 0H19a1 1 0 0 0-.117 1.993L19 13h.938a8.069 8.069 0 0 0 .034-1.67zM12 11a1 1 0 1 1 0 2 1 1 0 0 1 0-2M7.094 5.68a8.048 8.048 0 0 0-1.16 1.104l-.254.31.663.663a1 1 0 0 0 1.498-1.32l-.084-.094zM12 4a8.05 8.05 0 0 0-1 .062V5a1 1 0 1 0 2 0v-.938A8.079 8.079 0 0 0 12 4"/></g></svg>', flags: '<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><g fill="none" fill-rule="evenodd"><path fill="currentColor" d="M13.232 1.36c.632-.758 1.863-.24 1.763.742L14.289 9H20a1 1 0 0 1 .768 1.64l-10 12c-.632.758-1.863.24-1.763-.742L9.711 15H4a1 1 0 0 1-.768-1.64z"/></g></svg>' };
  const DESC = {
    sodium: "Новый движок отрисовки — в разы больше FPS",
    embeddium: "Sodium для Forge — в разы больше FPS",
    lithium: "Ускоряет физику, мобов и тики мира",
    'ferrite-core': "Заметно меньше расход памяти",
    entityculling: "Не рисует то, чего не видно за стенами",
    immediatelyfast: "Быстрее рисует интерфейс, текст и предметы",
    modernfix: "Быстрее запуск и меньше памяти",
  };
  let cur = null, data = null, seq = 0;

  const gb = (mb) => {
    let n = (mb / 1024).toFixed(mb % 1024 ? 1 : 0);
    try { n = Number(n).toLocaleString(_LOC, { maximumFractionDigits: 1 }); } catch { /* keep */ }
    return _t("{0} ГБ", [n]);
  };
  const kids = (...a) => a.filter((x) => x != null && x !== false);

  function ring(score) {
    const C = 2 * Math.PI * 26;
    const w = el('div', { class: 'pf-ring', 'data-tone': score >= 80 ? 'good' : score >= 50 ? 'mid' : 'low',
      html: `<svg viewBox="0 0 64 64" aria-hidden="true"><circle class="bg" cx="32" cy="32" r="26"/><circle class="fg" cx="32" cy="32" r="26" stroke-dasharray="${C.toFixed(1)}" stroke-dashoffset="${C.toFixed(1)}"/></svg>` });
    w.append(el('b', { class: 'num' }, String(score)));
    requestAnimationFrame(() => requestAnimationFrame(() => w.querySelector('.fg')?.setAttribute('stroke-dashoffset', (C * (1 - score / 100)).toFixed(1))));
    return w;
  }
  function row({ key, mod, ico, img, title, desc, note, state, on }) {
    const r = el('label', { class: `pf-row${state ? ' is-done' : ''}`, 'data-key': key });
    if (mod) r.dataset.mod = mod;
    const i = el('i', { class: 'pf-ico' });
    if (img) { const im = el('img', { alt: '', src: img, draggable: false }); im.addEventListener('error', () => { im.remove(); i.textContent = initials(title); }); i.append(im); }
    else if (ico) i.innerHTML = ico;
    else i.textContent = initials(title);
    let tail;
    if (state) tail = el('span', { class: `badge-sm pf-state${on === 'warn' ? ' is-warn' : ''}` }, state);
    else {
      const cb = el('input', { type: 'checkbox' }); cb.checked = !!on;
      cb.addEventListener('change', sync);
      tail = el('span', { class: 'switch' }, cb, el('i'));
    }
    r.append(i, el('div', { class: 't' }, ...kids(el('b', {}, title), el('span', {}, desc), note && el('span', { class: 'pf-note' }, note))), tail);
    return r;
  }
  function sync() {
    const n = body.querySelectorAll('.pf-row input:checked').length;
    submit.disabled = !n;
  }
  function headline(d) {
    const s = d.score;
    const title = s >= 90 ? _t("Сборка уже работает быстро") : s >= 50 ? _t("Можно сделать быстрее") : _t("Сборка работает медленнее, чем могла бы");
    const sub = [d.loader === 'vanilla' ? 'Vanilla' : d.loader[0].toUpperCase() + d.loader.slice(1), d.mc,
      d.loader !== 'vanilla' ? `${d.mods} ${_tp(d.mods, 'мод', 'мода', 'модов')}` : null,
      d.total_mb ? _t("ОЗУ {0}", [gb(d.total_mb)]) : null].filter(Boolean).join(' · ');
    return el('div', { class: 'pf-head' }, ring(s), el('div', { class: 't' }, el('b', {}, title), el('span', {}, sub)));
  }
  function paint() {
    const d = data;
    const memNote = d.memory_src === 'auto' ? _t("Сейчас выбирается автоматически") : d.memory_src === 'global' ? _t("Сейчас из общих настроек") : null;
    const rows = [
      row({ key: 'memory', ico: IC.memory, title: _t("Память"),
        desc: d.memory_ok ? _t("Выделено {0}", [gb(d.memory_mb)]) : _t("Сейчас {0} → рекомендуем {1}", [gb(d.memory_mb), gb(d.recommended_mb)]),
        note: d.memory_ok ? null : memNote, state: d.memory_ok ? _t("В порядке") : null, on: true }),
      row({ key: 'flags', ico: IC.flags, title: _t("Флаги Java"), desc: _t("Сборщик мусора G1 с настройками Aikar — меньше фризов"),
        note: d.heap_conflict ? _t("Уберём -Xmx и -Xms из аргументов: память задаётся отдельно") : null,
        state: d.flags_ok ? _t("Включены") : null, on: true }),
    ];
    const mods = d.candidates.map((c) => row({ key: c.slug, mod: c.id, img: c.icon_url, title: c.title, desc: _t(DESC[c.slug] || ''),
      state: c.present ? _t("Уже стоит") : c.blocked ? _t("Мешает OptiFine") : !c.available ? _t("Нет для {0}", [d.mc]) : null,
      on: c.present ? false : (c.blocked || !c.available) ? 'warn' : true }));
    body.replaceChildren(...kids(headline(d),
      el('div', { class: 'pf-sec' }, _t("Запуск")), el('div', { class: 'pf-list' }, ...rows),
      mods.length ? el('div', { class: 'pf-sec' }, _t("Моды для скорости")) : null,
      mods.length ? el('div', { class: 'pf-list' }, ...mods) : null,
      !mods.length ? el('p', { class: 'hint pf-van' }, _t("В Vanilla моды не ставятся — ускоряем память и Java.")) : null,
      d.optifine ? el('p', { class: 'hint pf-van' }, _t("В сборке есть OptiFine — Sodium и его аналоги с ним не работают.")) : null));
    sync();
  }
  function skel() {
    return el('div', { class: 'pf-skel' }, el('div', { class: 'pf-head' }, el('div', { class: 'pf-ring sk' }),
      el('div', { class: 't' }, el('b', { class: 'sk' }), el('span', { class: 'sk' }))),
      el('div', { class: 'pf-list' }, ...[0, 1, 2, 3].map(() => el('div', { class: 'pf-row' }, el('i', { class: 'pf-ico sk' }),
        el('div', { class: 't' }, el('b', { class: 'sk' }), el('span', { class: 'sk' }))))));
  }
  async function analyze(id, keep = false) {
    const my = ++seq;
    if (!keep) { body.replaceChildren(skel()); submit.disabled = true; }
    try { data = await call('perf.analyze', { instance: id }); }
    catch (e) { if (my === seq) body.replaceChildren(el('div', { class: 'table-empty' }, el('b', {}, String(e)))); return; }
    if (my === seq && cur === id) paint();
  }
  function open(id) {
    if (!id) return;
    cur = id; data = null;
    sub.textContent = byId(id)?.name || '';
    openDlg('dlg-perf');
    analyze(id);
  }
  dlg.addEventListener('submit', async (ev) => {
    ev.preventDefault();
    if (submit.dataset.state === 'loading' || !data) return;
    const on = (k) => !!body.querySelector(`.pf-row[data-key="${k}"] input:checked`);
    const mods = [...body.querySelectorAll('.pf-row[data-mod] input:checked')].map((i) => i.closest('.pf-row').dataset.mod);
    const id = cur;
    try {
      const r = await run(submit, 'perf.apply', { instance: id, memory: on('memory'), flags: on('flags'), mods });
      const n = r?.installed?.length || 0;
      toast(n ? _t("Сборка ускорена, добавлено модов: {0}", [n]) : _t("Сборка ускорена"));
      if (r?.failed?.length) toast(_t("Не удалось поставить: {0}", [r.failed.map((f) => f.title).join(', ')]), { error: true, ms: 6000 });
      const it = byId(id); if (it && r?.memory_mb) { it.settings = { ...(it.settings || {}), memory_mb: r.memory_mb, jvm_args: r.jvm_args }; }
      if (n && S.view === 'instance' && S.current === id) window.FMods?.instance(byId(id));
      analyze(id, true);
    } catch { /* run() shows it */ } finally { setTimeout(() => { submit.dataset.state = 'idle'; }, 900); }
  });
  $('#mi-perf').addEventListener('click', () => { FUI.closeMenu(); open(actInst()); });
  window.FPerf = { open };
})();