// Logs tab: logs/launcher-output.log, followed live while the game runs. Search, levels, copy.
(() => {
  const { $, toast } = FUI;
  const box = $('#tab-logs'), view = $('#log-view'), find = $('#log-find');
  const MAX = 5000;
  const enc = new TextEncoder();
  const bytes = (s) => enc.encode(s).length;
  let lines = [], head = 0, tail = 0, inst = null, timer = 0, seq = 0, level = 'all', q = '', stick = true, fresh = 0, running = false, exists = false;

  const visible = () => !box.hidden && S.view === 'instance';
  const kind = (t) => (/\b(ERROR|FATAL|SEVERE)\b|^\s+at |^Caused by:|Exception(:|$)/.test(t) ? 'error' : /\bWARN(ING)?\b/.test(t) ? 'warn' : '');
  // `atEnd`: the text may stop mid-line; that piece is left for the next read.
  function parse(text, atEnd) {
    const parts = text.split('\n');
    const last = parts.pop();
    return { add: parts.map((t) => ({ t: t.replace(/\r$/, ''), k: kind(t) })), cut: atEnd ? bytes(last) : 0 };
  }
  const match = (l) => (level === 'all' || (level === 'warn' ? !!l.k : l.k === 'error')) && (!q || l.t.toLowerCase().includes(q));
  function hl(d, s) {
    if (!q) { d.append(s); return; }
    const low = s.toLowerCase();
    let i = 0, j;
    while ((j = low.indexOf(q, i)) !== -1) { d.append(s.slice(i, j), el('mark', {}, s.slice(j, j + q.length))); i = j + q.length; }
    d.append(s.slice(i));
  }
  function line(l, isNew) {
    const d = el('div', { class: `ll${l.k ? ` is-${l.k}` : ''}${isNew ? ' is-new' : ''}` });
    const m = /^(\[[0-9:.]+\])( ?\[[^\]]*\]:?)?/.exec(l.t);
    if (m) { d.append(el('span', { class: 'ts' }, m[1])); if (m[2]) d.append(el('span', { class: 'src' }, m[2])); }
    hl(d, m ? l.t.slice(m[0].length) : l.t);
    return d;
  }
  function render() {
    const shown = lines.filter(match);
    const kids = [];
    if (head > 0) {
      const more = el('button', { class: 'btn btn-sm btn-ghost log-more', type: 'button' }, el('span', { class: 'lbl' }, _t("Показать раньше")));
      more.addEventListener('click', () => earlier(more));
      kids.push(more);
    }
    kids.push(...shown.map((l) => line(l)));
    if (!shown.length) kids.push(emptyState(ES_ICO.log, (!exists ? _t("Лог появится после первого запуска") : lines.length ? _t("Ничего не найдено") : _t("Лог пуст"))));
    view.replaceChildren(...kids);
    $('#log-hits').textContent = q && lines.length ? String(shown.length) : '';
    if (stick) view.scrollTop = view.scrollHeight;
  }
  function append(add) {
    if (!add.length) return;
    lines.push(...add);
    const drop = lines.length - MAX;
    if (drop > 0) { head += lines.slice(0, drop).reduce((n, l) => n + bytes(l.t) + 1, 0); lines.splice(0, drop); }
    const vis = add.filter(match);
    if (drop > 0 || view.querySelector('.table-empty')) render();
    else if (vis.length) view.append(...vis.map((l) => line(l, true)));
    if (stick) view.scrollTop = view.scrollHeight;
    else if (vis.length) { fresh += vis.length; $('#log-jump .t').textContent = `${fresh} ${fresh === 1 ? _t("новая строка") : _t("новых строк")}`; $('#log-jump').hidden = false; }
  }
  function setRunning(on) { running = !!on; $('#log-live').hidden = !running; }
  const schedule = () => { clearTimeout(timer); timer = visible() ? setTimeout(poll, running ? 700 : 3000) : 0; };

  async function load() {
    const id = S.current, my = ++seq;
    if (inst !== id) view.replaceChildren(...[0, 1, 2, 3, 4, 5].map((n) => el('div', { class: `ll sk w${n % 3}` })));
    const r = await call('instances.log', { instance: id }).catch(() => null);
    if (my !== seq) return;
    inst = id; stick = true; fresh = 0; $('#log-jump').hidden = true;
    exists = !!r?.exists;
    const { add, cut } = r ? parse(r.text, true) : { add: [], cut: 0 };
    lines = add.slice(-MAX);
    head = r ? r.start + add.slice(0, add.length - lines.length).reduce((n, l) => n + bytes(l.t) + 1, 0) : 0;
    tail = r ? r.end - cut : 0;
    setRunning(r?.running);
    render();
    schedule();
  }
  async function poll() {
    timer = 0;
    if (!visible() || inst !== S.current) return;
    const my = seq;
    const r = await call('instances.log', { instance: inst, from: tail }).catch(() => null);
    if (my !== seq) return;
    if (r) {
      setRunning(r.running);
      if (r.reset || (!exists && r.exists)) { inst = null; load(); return; }
      tail = r.end;
      append(parse(r.text, false).add);
    }
    schedule();
  }
  async function earlier(btn) {
    btn.dataset.state = 'loading';
    const r = await call('instances.log', { instance: inst, before: head }).catch(() => null);
    if (!r) { btn.dataset.state = 'error'; return; }
    const h = view.scrollHeight, top = view.scrollTop;
    lines = parse(r.text, false).add.concat(lines);
    head = r.start; stick = false;
    render();
    view.scrollTop = view.scrollHeight - h + top;
  }

  view.addEventListener('scroll', () => {
    stick = view.scrollHeight - view.scrollTop - view.clientHeight < 24;
    if (stick && fresh) { fresh = 0; $('#log-jump').hidden = true; }
  });
  $('#log-jump').addEventListener('click', () => { view.scrollTo({ top: view.scrollHeight, behavior: document.documentElement.dataset.motion === 'off' ? 'auto' : 'smooth' }); });
  let qt = 0;
  find.addEventListener('input', () => { clearTimeout(qt); qt = setTimeout(() => { q = find.value.trim().toLowerCase(); stick = true; render(); }, 120); });
  find.addEventListener('keydown', (e) => { if (e.key === 'Escape' && find.value) { e.stopPropagation(); find.value = ''; q = ''; render(); } });
  $('#log-level').addEventListener('click', (e) => {
    const b = e.target.closest('[data-lv]');
    if (!b) return;
    $('#log-level').querySelectorAll('[data-lv]').forEach((x) => x.setAttribute('aria-selected', String(x === b)));
    level = b.dataset.lv; stick = true; render();
  });
  $('#log-copy').addEventListener('click', async () => {
    const text = lines.filter(match).map((l) => l.t).join('\n');
    if (!text) return;
    try { await navigator.clipboard.writeText(text); toast(q || level !== 'all' ? _t("Найденные строки скопированы") : _t("Лог скопирован")); } catch (e) { toast(String(e), { error: true }); }
  });
  $('#log-folder').addEventListener('click', () => invoke('open_logs_folder', { instance: S.current }).catch((e) => toast(String(e), { error: true })));
  $('#ip-tabs').addEventListener('click', (e) => { if (e.target.closest('[data-tab="logs"]')) { if (inst !== S.current) load(); else { render(); schedule(); } } });
  window.FLogs = { reset() { if (inst !== S.current) { inst = null; if (visible()) load(); } else if (!timer) schedule(); }, restart() { inst = null; if (visible()) load(); } };
})();