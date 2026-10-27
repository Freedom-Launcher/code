'use strict';
// Home hover card (0.3.42): a Steam-style card next to an instance — playtime, last launch and what is inside.
// Solid surface, no blur (Правила стекла). Counts come from instances.stats and are cached for 30 s.
(() => {
  const grid = $('#home-all');
  if (!grid) return;
  const box = el('div', { class: 'peek', 'aria-hidden': 'true' });
  document.body.append(box);
  const cache = new Map();
  let timer = 0, cur = null, open = false, seq = 0, warm = 0;
  const enabled = () => document.documentElement.dataset.peek !== 'off';
  const fresh = (id) => { const c = cache.get(id); return c && Date.now() - c.t < 30000 ? c.v : null; };
  async function stats(id) {
    const f = fresh(id);
    if (f) return f;
    const v = await call('instances.stats', { instance: id }).catch(() => null);
    if (v) cache.set(id, { t: Date.now(), v });
    return v;
  }
  const row = (k, v) => el('div', { class: 'pk-r' }, el('span', {}, k), el('b', {}, v));
  const tile = (n, label) => el('div', { class: `pk-t${n == null ? ' sk' : ''}` }, el('b', {}, n == null ? '00' : String(n)), el('span', {}, label));
  function cover(hc) {
    const img = hc.querySelector('img');
    if (img?.currentSrc) return `url("${img.currentSrc}")`;
    for (const n of hc.querySelectorAll('i, span, div')) {
      const b = getComputedStyle(n).backgroundImage;
      if (b && b.includes('url(')) return b;
    }
    return '';
  }
  function tiles(i, s) {
    const v = (k) => (s ? s[k] ?? 0 : null);
    const first = !i.loader || i.loader === 'vanilla' ? [v('resourcepacks'), _t("Ресурспаки")] : [v('mods'), _t("Моды")];
    return el('div', { class: 'pk-tiles' }, tile(...first), tile(v('worlds'), _t("Миры")), tile(v('screenshots'), _t("Скриншоты")));
  }
  function paint(i, hc, s) {
    const c = el('div', { class: 'pk-cov' }, el('b', {}, i.name));
    const bgi = cover(hc);
    if (bgi) c.style.backgroundImage = bgi;
    const created = i.created_at
      ? new Date(i.created_at * 1000).toLocaleDateString(document.documentElement.lang || undefined, { day: 'numeric', month: 'short', year: 'numeric' })
      : '—';
    box.replaceChildren(c, el('div', { class: 'pk-body' },
      el('div', { class: 'pk-meta' }, i.group ? `${instMeta(i)} · ${i.group}` : instMeta(i)),
      el('div', { class: 'pk-rows' },
        row(_t("Время в игре"), i.playtime_secs > 0 ? fmtPlay(i.playtime_secs) : '—'),
        row(_t("Последний запуск"), i.last_played ? ago(i.last_played) : _t("ещё не запускали")),
        row(_t("Создана"), created)),
      tiles(i, s)));
  }
  function place(hc) {
    const r = hc.getBoundingClientRect(), w = box.offsetWidth, h = box.offsetHeight, gap = 12;
    let x = r.right + gap;
    if (x + w > innerWidth - 8) x = r.left - w - gap;
    if (x < 8) x = Math.max(8, Math.min(innerWidth - w - 8, r.left));
    const y = Math.max(8, Math.min(innerHeight - h - 8, r.top));
    box.style.left = `${Math.round(x)}px`;
    box.style.top = `${Math.round(y)}px`;
  }
  async function show(hc) {
    timer = 0;
    const i = byId(hc.dataset.inst);
    if (!i || !hc.isConnected) return;
    cur = hc;
    const my = ++seq;
    paint(i, hc, fresh(i.id));
    place(hc);
    box.setAttribute('data-open', '');
    open = true;
    const s = await stats(i.id);
    if (my !== seq || cur !== hc) return;
    paint(i, hc, s);
    place(hc);
  }
  function hide() {
    clearTimeout(timer); timer = 0; seq++;
    if (open) warm = Date.now() + 400;
    cur = null; open = false;
    box.removeAttribute('data-open');
  }
  document.addEventListener('pointerover', (e) => {
    if (e.pointerType === 'touch') return;
    const hc = e.target.closest?.('#home-all .hcard');
    if (!hc || !enabled() || hc.classList.contains('is-dragging')) { if (cur || timer) hide(); return; }
    if (hc === cur) return;
    clearTimeout(timer);
    timer = setTimeout(() => show(hc), open || Date.now() < warm ? 60 : 450);
  });
  for (const ev of ['pointerdown', 'keydown', 'wheel']) document.addEventListener(ev, hide, { passive: true, capture: true });
  document.addEventListener('scroll', hide, { passive: true, capture: true });
  addEventListener('blur', hide);
  window.FPeek = { hide };
})();
