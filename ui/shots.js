// Screenshots tab: screenshots/ newest first, grouped by day, with a viewer (←/→, Esc), like Modrinth App.
(() => {
  const { $, toast } = FUI;
  const box = $('#tab-shots'), view = $('#shot-view'), img = $('#sv-img');
  const isWin = navigator.userAgent.includes('Windows');
  const src = (inst, f) => `${isWin ? 'http://shot.localhost' : 'shot://localhost'}/${encodeURIComponent(inst)}/${encodeURIComponent(f)}`;
  const still = () => document.documentElement.dataset.motion === 'off' || matchMedia('(prefers-reduced-motion: reduce)').matches;
  const EASE = 'cubic-bezier(.2,.9,.25,1)';
  let list = [], shown = null, seq = 0, at = -1, armed = 0;

  const visible = () => !box.hidden && S.view === 'instance';
  const plural = (n, a, b, c) => { const m = n % 10, h = n % 100; return m === 1 && h !== 11 ? a : m >= 2 && m <= 4 && (h < 12 || h > 14) ? b : c; };
  const midnight = (d) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  function day(t) {
    const d = new Date(t * 1000), n = new Date();
    const k = Math.round((midnight(n) - midnight(d)) / 864e5);
    if (k === 0) return 'Сегодня';
    if (k === 1) return 'Вчера';
    return d.toLocaleDateString('ru-RU', { day: 'numeric', month: 'long', ...(d.getFullYear() === n.getFullYear() ? {} : { year: 'numeric' }) });
  }
  const when = (t) => new Date(t * 1000).toLocaleString('ru-RU', { day: 'numeric', month: 'long', hour: '2-digit', minute: '2-digit' });
  const folder = () => invoke('open_shots_folder', { instance: S.current }).catch((e) => toast(String(e), { error: true }));

  function tile(s, n) {
    const b = el('button', { class: 'shot', type: 'button', 'data-id': `shot.${s.file}`, 'aria-label': when(s.taken) });
    b.style.setProperty('--i', String(Math.min(n, 16)));
    const im = el('img', { alt: '', loading: n < 12 ? 'eager' : 'lazy', decoding: 'async', draggable: false, src: src(S.current, s.file) });
    im.addEventListener('load', () => b.classList.add('is-ready'));
    im.addEventListener('error', () => b.classList.add('is-ready', 'is-broken'));
    b.append(im);
    b.addEventListener('click', () => open(list.indexOf(s), b));
    return b;
  }
  function grid() {
    const groups = new Map();
    list.forEach((s) => { const d = day(s.taken); if (!groups.has(d)) groups.set(d, []); groups.get(d).push(s); });
    let n = 0;
    const open = el('button', { class: 'btn btn-sm btn-ghost', type: 'button', 'data-id': 'instance.shots.folder' }, 'Открыть папку');
    open.addEventListener('click', folder);
    const head = el('div', { class: 'shots-head' }, el('span', { class: 'n' }, `${list.length} ${plural(list.length, 'скриншот', 'скриншота', 'скриншотов')}`), open);
    return [head, ...[...groups].map(([d, ss]) => el('section', { class: 'shot-day' },
      el('h3', {}, d, el('span', { class: 'n' }, String(ss.length))),
      el('div', { class: 'shot-grid' }, ...ss.map((s) => tile(s, n++)))))];
  }
  function empty() {
    const b = el('button', { class: 'btn btn-sm', type: 'button' }, 'Открыть папку');
    b.addEventListener('click', folder);
    return el('div', { class: 'table-empty tall' }, el('b', {}, 'Скриншотов пока нет'), el('span', {}, 'F2 в игре'), b);
  }
  async function paint(force = false) {
    if (!visible() || !S.current) return;
    if (!force && shown === S.current) return;
    const id = S.current, my = ++seq;
    if (shown !== id) box.replaceChildren(el('div', { class: 'shot-grid shot-skel' }, ...[0, 1, 2, 3, 4, 5].map(() => el('i', { class: 'shot' }))));
    list = await call('instances.screenshots', { instance: id }).catch(() => []);
    if (my !== seq) return;
    shown = id;
    box.replaceChildren(...(list.length ? grid() : [empty()]));
  }

  // ---------- Viewer ----------
  function show(i, dir = 0) {
    at = (i + list.length) % list.length;
    const s = list[at];
    disarm();
    img.src = src(S.current, s.file);
    $('#sv-when').textContent = when(s.taken);
    $('#sv-meta').textContent = `${at + 1} из ${list.length} · ${fmtBytes(s.size)}`;
    view.querySelectorAll('.sv-nav').forEach((b) => { b.hidden = list.length < 2; });
    if (dir && !still()) img.animate([{ opacity: 0, transform: `translateX(${dir * 28}px) scale(.985)` }, { opacity: 1, transform: 'none' }], { duration: 240, easing: EASE });
  }
  function open(i, from) {
    if (i < 0) return;
    show(i);
    view.hidden = false;
    requestAnimationFrame(() => view.setAttribute('data-open', ''));
    $('#sv-close').focus({ preventScroll: true });
    if (!from || still()) return;
    const zoom = () => {
      const a = from.getBoundingClientRect(), b = img.getBoundingClientRect();
      if (!b.width) return;
      const dx = a.left + a.width / 2 - (b.left + b.width / 2), dy = a.top + a.height / 2 - (b.top + b.height / 2);
      img.animate([{ transform: `translate(${dx}px, ${dy}px) scale(${a.width / b.width})`, opacity: 0.4 }, { transform: 'none', opacity: 1 }], { duration: 340, easing: EASE });
    };
    if (img.complete && img.naturalWidth) zoom(); else img.addEventListener('load', zoom, { once: true });
  }
  function close() {
    if (view.hidden) return;
    view.removeAttribute('data-open');
    disarm();
    setTimeout(() => { if (!view.hasAttribute('data-open')) view.hidden = true; }, still() ? 0 : 200);
  }
  function disarm() { clearTimeout(armed); armed = 0; $('#sv-del').classList.remove('is-armed'); $('#sv-del .t').textContent = 'Удалить'; }
  async function del() {
    const b = $('#sv-del');
    if (!b.classList.contains('is-armed')) {
      b.classList.add('is-armed'); $('#sv-del .t').textContent = 'Точно удалить?';
      armed = setTimeout(disarm, 3000);
      return;
    }
    const s = list[at];
    try {
      await run(b, 'screenshots.delete', { instance: S.current, file: s.file });
      list.splice(at, 1);
      toast('Скриншот удалён');
      if (!list.length) close(); else show(Math.min(at, list.length - 1));
      box.replaceChildren(...(list.length ? grid() : [empty()]));
    } catch { /* run() shows it */ } finally { b.dataset.state = 'idle'; }
  }
  $('#sv-prev').addEventListener('click', () => show(at - 1, -1));
  $('#sv-next').addEventListener('click', () => show(at + 1, 1));
  $('#sv-close').addEventListener('click', close);
  $('#sv-folder').addEventListener('click', folder);
  $('#sv-del').addEventListener('click', del);
  $('#sv-stage').addEventListener('click', (e) => { if (e.target === e.currentTarget) close(); });
  document.addEventListener('keydown', (e) => {
    if (view.hidden) return;
    const k = { Escape: close, ArrowLeft: () => show(at - 1, -1), ArrowRight: () => show(at + 1, 1), Delete: del }[e.key];
    if (!k || (e.key.startsWith('Arrow') && list.length < 2)) return;
    e.preventDefault(); e.stopPropagation(); k();
  }, true);
  $('#ip-tabs').addEventListener('click', (e) => { if (e.target.closest('[data-tab="shots"]')) paint(true); });
  window.FShots = { paint, reset() { if (shown !== S.current) { shown = null; close(); paint(); } } };
})();