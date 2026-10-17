'use strict';
// Instance settings (Э5), Modrinth-style: sections, Java picker, memory slider, window, launch hooks.
// The same dialog edits launcher-wide defaults (Settings → Игра); an instance overrides block by block.
(() => {
  const f = $('#dlg-inst');
  const KEYS = ['java', 'mem', 'jvm', 'win', 'env', 'hooks'];
  const TITLE = { general: 'Общее', java: 'Java и память', window: 'Окно', launch: 'Запуск' };
  const VARS = ['INST_ID', 'INST_NAME', 'INST_MC_VER', 'INST_DIR', 'INST_JAVA'];
  const WIN = /Windows/.test(navigator.userAgent);
  const st = { mode: 'inst', inst: null, own: {}, java: null, javas: null, req: null, base: {}, mem: null, snap: '', lastHook: null };
  const gb = (mb) => `${(mb / 1024).toFixed(1).replace('.0', '')} ГБ`;
  const memInfo = async () => (st.mem ||= await call('system.memory').catch(() => ({ total_mb: 8192, recommended_mb: 4096 })));
  const short = (p) => p.split(/[\\/]/).slice(-3).join('/');

  // Collapsible bodies: wrap content so grid rows can animate 0fr → 1fr.
  $$('.isx-body', f).forEach((fs) => { const inn = el('div', { class: 'isx-in' }); inn.append(...fs.childNodes); fs.append(inn); });

  // ---------- Sections ----------
  const nav = $('#isx-nav'), ink = $('.isx-ink', nav);
  const placeInk = () => {
    const b = $('button[aria-selected="true"]', nav);
    if (!b) return;
    ink.style.setProperty('--y', `${b.offsetTop}px`);
    ink.style.setProperty('--h', `${b.offsetHeight}px`);
  };
  function section(sec) {
    $$('button', nav).forEach((b) => b.setAttribute('aria-selected', String(b.dataset.sec === sec)));
    $$('.isx-sec', f).forEach((s) => {
      const on = s.dataset.sec === sec;
      if (on && s.hidden) { s.classList.remove('is-enter'); void s.offsetWidth; s.classList.add('is-enter'); }
      s.hidden = !on;
    });
    $('#isx-head').textContent = TITLE[sec];
    $('#isx-scroll').scrollTop = 0;
    placeInk();
    if (sec === 'java' && !st.javas) scan();
  }
  nav.addEventListener('click', (e) => { const b = e.target.closest('button[data-sec]'); if (b) section(b.dataset.sec); });
  nav.addEventListener('keydown', (e) => {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    const bs = $$('button:not([hidden])', nav), i = bs.findIndex((b) => b.getAttribute('aria-selected') === 'true');
    const n = bs[(i + (e.key === 'ArrowDown' ? 1 : bs.length - 1)) % bs.length];
    e.preventDefault(); section(n.dataset.sec); n.focus();
  });

  // ---------- Own / inherited blocks ----------
  function setOwn(k, on) {
    st.own[k] = on;
    const b = $(`.isx-block[data-own="${k}"]`, f);
    b.classList.toggle('is-on', on);
    $('[data-own-toggle]', b).checked = on;
    $('.isx-body', b).disabled = !on;
  }
  f.addEventListener('change', (e) => { const k = e.target.dataset?.ownToggle; if (k) { setOwn(k, e.target.checked); dirty(); } });
  function inherited() {
    const g = st.base, inst = st.mode === 'inst';
    const pre = inst ? 'Как в лаунчере: ' : '';
    const auto = st.mem ? `Авто · ${gb(st.mem.recommended_mb)}` : 'Авто';
    const hooks = g.hooks || {};
    const txt = {
      java: inst && g.java_path ? pre + short(g.java_path) : `${pre}Авто${st.req ? ` · Java ${st.req}` : ''}`,
      mem: inst && g.memory_mb ? pre + gb(g.memory_mb) : pre + auto,
      jvm: inst && g.jvm_args ? pre + g.jvm_args : `${pre}Нет`,
      win: inst && (g.width || g.fullscreen) ? pre + [g.width ? `${g.width} × ${g.height}` : '', g.fullscreen ? 'полный экран' : ''].filter(Boolean).join(' · ') : `${pre}Как в игре`,
      env: inst && g.env && Object.keys(g.env).length ? `${pre}${Object.keys(g.env).length} шт.` : `${pre}Нет`,
      hooks: inst && (hooks.pre_launch || hooks.wrapper || hooks.post_exit) ? `${pre}заданы` : `${pre}Нет`,
    };
    for (const k of KEYS) $(`[data-inh="${k}"]`, f).textContent = txt[k];
  }

  // ---------- Java ----------
  function javaCard(j, i) {
    const r = st.req;
    const fit = r == null ? null : j.major === r ? ['is-accent', 'Подходит'] : ['is-warn', `Нужна ${r}`];
    const inp = el('input', { type: 'radio', name: 'isx-java', value: j.path, checked: j.path === st.java });
    const card = el('label', { class: 'isx-jv', title: j.path },
      inp, el('span', { class: 'isx-jv-n' }, String(j.major)),
      el('span', { class: 'isx-jv-t' }, el('b', {}, [j.vendor || 'Java', j.version].join(' · ')), el('span', { class: 'mono' }, j.path)),
      j.managed ? el('span', { class: 'badge' }, 'Лаунчер') : null,
      fit ? el('span', { class: `badge ${fit[0]}` }, fit[1]) : null);
    card.style.setProperty('--i', i);
    inp.addEventListener('change', () => { st.java = j.path; $('#isx-java-path').value = j.path; dirty(); });
    return card;
  }
  function paintJavas() {
    const box = $('#isx-javas');
    const list = st.javas || [];
    box.replaceChildren(...list.map(javaCard));
    const have = new Set(list.map((j) => j.major));
    const want = (st.req ? [st.req] : [8, 17, 21]).filter((m) => !have.has(m));
    if (want.length) {
      box.append(el('div', { class: 'isx-get' }, ...want.map((m) => {
        const b = el('button', { type: 'button', class: 'btn btn-sm' }, el('span', { class: 'lbl' }, `Скачать Java ${m}`));
        b.addEventListener('click', () => install(b, m));
        return b;
      })));
    }
    if (!list.length && !want.length) box.append(el('p', { class: 'hint' }, 'Java не найдена'));
  }
  async function scan() {
    const b = $('#isx-java-scan');
    b.dataset.state = 'loading';
    if (!st.javas) $('#isx-javas').replaceChildren(...[0, 1, 2].map(() => el('div', { class: 'isx-skel' })));
    try {
      const r = await call('java.list', { version: st.mode === 'inst' ? st.inst.version : null });
      st.javas = r.installs;
      if (st.java && !st.javas.some((j) => j.path === st.java)) {
        const extra = await call('java.test', { path: st.java }).catch(() => null);
        if (extra) st.javas.unshift(extra);
      }
      b.dataset.state = 'idle';
    } catch (e) { st.javas = []; b.dataset.state = 'error'; toast(String(e), { error: true }); }
    paintJavas();
  }
  $('#isx-java-scan').addEventListener('click', () => { st.javas = null; scan(); });
  async function testPath() {
    const p = $('#isx-java-path').value.trim();
    if (!p) return;
    try {
      const j = await run($('#isx-java-test'), 'java.test', { path: p });
      st.javas = [j, ...(st.javas || []).filter((x) => x.path !== j.path)];
      st.java = j.path;
      $('#isx-java-path').value = j.path;
      paintJavas(); dirty();
    } catch { /* toast shown */ }
  }
  $('#isx-java-test').addEventListener('click', testPath);
  $('#isx-java-path').addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); testPath(); } });
  async function install(btn, major) {
    const lbl = $('.lbl', btn);
    const t = setInterval(async () => {
      const p = await call('game.progress', { instance: `java:${major}` }).catch(() => null);
      if (p?.total_bytes) lbl.textContent = `${Math.floor((p.done_bytes / p.total_bytes) * 100)}%`;
    }, 400);
    try {
      const j = await run(btn, 'java.install', { major });
      st.java = j.path;
      st.javas = [j, ...(st.javas || []).filter((x) => x.path !== j.path)];
      $('#isx-java-path').value = j.path;
      paintJavas(); dirty();
    } catch { lbl.textContent = `Скачать Java ${major}`; } finally { clearInterval(t); }
  }

  // ---------- Memory ----------
  const range = $('#isx-mem'), num = $('#isx-mem-num');
  function paintMem(v) {
    const max = +range.max, min = +range.min;
    const pct = (x) => `${((Math.min(max, Math.max(min, x)) - min) / (max - min)) * 100}%`;
    $('#isx-slider').style.setProperty('--fill', pct(v));
    $('#isx-slider').style.setProperty('--rec', pct(st.mem.recommended_mb));
    $('#isx-mem-warn').hidden = v <= st.mem.total_mb * 0.8;
  }
  range.addEventListener('input', () => { num.value = range.value; paintMem(+range.value); });
  num.addEventListener('input', () => { range.value = num.value; paintMem(+num.value || 0); });
  $('#isx-rec').addEventListener('click', () => { range.value = num.value = st.mem.recommended_mb; paintMem(st.mem.recommended_mb); dirty(); });

  // ---------- Window ----------
  $('#isx-presets').addEventListener('click', (e) => {
    const b = e.target.closest('[data-res]');
    if (!b) return;
    [$('#isx-w').value, $('#isx-h').value] = b.dataset.res.split('x');
    paintPresets(); dirty();
  });
  const paintPresets = () => $$('[data-res]', f).forEach((b) => b.classList.toggle('is-on', b.dataset.res === `${$('#isx-w').value}x${$('#isx-h').value}`));
  ['#isx-w', '#isx-h'].forEach((s) => $(s).addEventListener('input', paintPresets));

  // ---------- Env + hooks ----------
  function envRow(k = '', v = '') {
    const row = el('div', { class: 'isx-env-r' },
      el('input', { class: 'field mono', value: k, placeholder: 'ИМЯ', spellcheck: false, 'aria-label': 'Имя' }),
      el('input', { class: 'field mono', value: v, placeholder: 'значение', spellcheck: false, 'aria-label': 'Значение' }));
    const rm = el('button', { type: 'button', class: 'btn btn-ghost btn-icon btn-sm', 'aria-label': 'Убрать', html: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="m4.5 4.5 7 7m0-7-7 7"/></svg>' });
    rm.addEventListener('click', () => { row.classList.add('is-out'); setTimeout(() => { row.remove(); dirty(); }, 160); });
    row.append(rm);
    return row;
  }
  $('#isx-env-add').addEventListener('click', () => { const r = envRow(); $('#isx-env').append(r); $('input', r).focus(); dirty(); });
  $('#isx-vars').replaceChildren(...VARS.map((v) => el('button', { type: 'button', class: 'isx-var mono', 'data-v': v }, WIN ? `%${v}%` : `$${v}`)));
  ['#isx-pre', '#isx-wrap', '#isx-post'].forEach((s) => $(s).addEventListener('focus', (e) => { st.lastHook = e.target; }));
  $('#isx-vars').addEventListener('pointerdown', (e) => e.preventDefault());
  $('#isx-vars').addEventListener('click', (e) => {
    const b = e.target.closest('[data-v]');
    const t = st.lastHook || $('#isx-pre');
    if (!b) return;
    const s = t.selectionStart ?? t.value.length;
    t.setRangeText(b.textContent, s, t.selectionEnd ?? s, 'end');
    t.focus(); dirty();
  });

  // ---------- Fill / collect ----------
  function fill(s) {
    const g = st.base, inst = st.mode === 'inst';
    const hooks = { ...(g.hooks || {}), ...(s.hooks || {}) };
    st.own = {};
    setOwn('java', !!s.java_path);
    setOwn('mem', s.memory_mb != null);
    setOwn('jvm', s.jvm_args != null);
    setOwn('win', s.width != null || s.fullscreen != null);
    setOwn('env', s.env != null);
    setOwn('hooks', !!(s.hooks && (s.hooks.pre_launch || s.hooks.wrapper || s.hooks.post_exit)));
    st.java = s.java_path || g.java_path || null;
    $('#isx-java-path').value = st.java || '';
    const max = Math.max(1024, Math.floor(st.mem.total_mb / 256) * 256);
    range.max = num.max = max;
    $('#isx-mem-max').textContent = gb(max);
    range.value = num.value = s.memory_mb || g.memory_mb || st.mem.recommended_mb;
    paintMem(+range.value);
    $('#isx-jvm').value = s.jvm_args ?? g.jvm_args ?? '';
    $('#isx-w').value = s.width ?? g.width ?? '';
    $('#isx-h').value = s.height ?? g.height ?? '';
    $('#isx-fs').checked = !!(s.fullscreen ?? g.fullscreen);
    paintPresets();
    $('#isx-env').replaceChildren(...Object.entries(s.env ?? g.env ?? {}).map(([k, v]) => envRow(k, v)));
    $('#isx-pre').value = hooks.pre_launch || '';
    $('#isx-wrap').value = hooks.wrapper || '';
    $('#isx-post').value = hooks.post_exit || '';
    const after = $('#isx-after');
    const opts = [['keep', 'Оставить лаунчер'], ['minimize', 'Свернуть лаунчер'], ['close', 'Закрыть лаунчер']];
    const label = Object.fromEntries(opts)[g.after_launch || 'keep'];
    after.replaceChildren(...(inst ? [['', `Как в лаунчере · ${label}`]] : []).concat(opts).map(([v, t]) => el('option', { value: v }, t)));
    after.value = s.after_launch || (inst ? '' : 'keep');
    inherited();
  }
  const str = (v) => v.trim() || undefined;
  function collect() {
    const o = {}, on = st.own;
    if (on.java && st.java) o.java_path = st.java;
    if (on.mem) o.memory_mb = Math.round(+num.value) || undefined;
    if (on.jvm) o.jvm_args = str($('#isx-jvm').value);
    if (on.win) {
      const w = parseInt($('#isx-w').value, 10), h = parseInt($('#isx-h').value, 10);
      if (w && h) { o.width = w; o.height = h; }
      o.fullscreen = $('#isx-fs').checked;
    }
    if (on.env) o.env = Object.fromEntries($$('.isx-env-r', f).map((r) => $$('input', r).map((i) => i.value.trim())).filter(([k]) => k));
    if (on.hooks) {
      const h = { pre_launch: str($('#isx-pre').value), wrapper: str($('#isx-wrap').value), post_exit: str($('#isx-post').value) };
      if (h.pre_launch || h.wrapper || h.post_exit) o.hooks = h;
    }
    const a = $('#isx-after').value;
    if (a && !(st.mode === 'global' && a === 'keep')) o.after_launch = a;
    return JSON.parse(JSON.stringify(o));
  }
  const key = () => JSON.stringify([collect(), $('#isx-name').value]);
  function dirty() { $('#isx-save').disabled = key() === st.snap; }
  f.addEventListener('input', dirty);
  f.addEventListener('change', dirty);

  function bad(sel, sec) {
    section(sec);
    const n = $(sel);
    n.setAttribute('aria-invalid', 'true');
    n.focus();
    setTimeout(() => n.removeAttribute('aria-invalid'), 1600);
  }

  // ---------- Open / save ----------
  async function open(id, sec) {
    const inst = id ? byId(id) : null;
    if (id && !inst) return;
    const [g] = await Promise.all([call('settings.get'), memInfo()]);
    Object.assign(st, { mode: inst ? 'inst' : 'global', inst, javas: null, lastHook: null });
    st.base = inst ? g.game || {} : {};
    st.req = inst ? null : null;
    f.classList.toggle('is-global', !inst);
    $('[data-sec="general"]', nav).hidden = !inst;
    $('#isx-ava').textContent = inst ? initials(inst.name) : '';
    $('#isx-ava').classList.toggle('is-global', !inst);
    $('#isx-title').textContent = inst ? inst.name : 'Игра';
    $('#isx-sub').textContent = inst ? [inst.version, inst.loader === 'vanilla' ? '' : inst.loader].filter(Boolean).join(' · ') : 'Для всех сборок';
    $('#isx-name').value = inst ? inst.name : '';
    if (inst) st.req = (await call('java.list', { version: inst.version }).then((r) => { st.javas = r.installs; return r.required; }).catch(() => null));
    fill(inst ? inst.settings || {} : g.game || {});
    if (st.javas) paintJavas();
    st.snap = key();
    $('#isx-save').disabled = true;
    openDlg('dlg-inst');
    requestAnimationFrame(() => section(sec || (inst ? 'general' : 'java')));
  }
  f.addEventListener('submit', async (ev) => {
    ev.preventDefault();
    const s = collect();
    const w = $('#isx-w').value, h = $('#isx-h').value;
    if (st.own.win && !!w !== !!h) return bad(w ? '#isx-h' : '#isx-w', 'window');
    if (st.mode === 'inst' && !$('#isx-name').value.trim()) return bad('#isx-name', 'general');
    try {
      if (st.mode === 'inst') {
        await run($('#isx-save'), 'instances.configure', { instance: st.inst.id, name: $('#isx-name').value.trim(), settings: s });
        await reloadInstances();
      } else {
        await run($('#isx-save'), 'settings.set', { game: s });
        window.FGameSettings?.reload();
      }
      setTimeout(closeDlg, 260);
    } catch { /* toast shown */ }
  });
  f.addEventListener('click', (e) => {
    const a = e.target.closest('[data-isx-act]');
    if (!a) return;
    closeDlg();
    setTimeout(() => $(`#${a.dataset.isxAct}`)?.click(), 60);
  });

  // ---------- Entry points ----------
  $('#mi-settings').addEventListener('click', () => { closeMenu(); open(S.current); });
  addEventListener('resize', placeInk);
  window.FInstSettings = { open, section };
})();