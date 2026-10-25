// Skins page: live 3D stage, your library (order, cape per skin), vanilla skins, edit dialog.
// Everything goes through the official Minecraft services API in the core (skins.*).
(() => {
  const q = (s) => document.querySelector(s);
  const page = () => q('#view-skins .skins');
  const SVG = (d) => `<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">${d}</svg>`;
  const I = {
    check: SVG('<g fill="none" fill-rule="evenodd"><path fill="currentColor" d="M21.192 5.465a1 1 0 0 1 0 1.414L9.95 18.122a1.1 1.1 0 0 1-1.556 0l-5.586-5.586a1 1 0 1 1 1.415-1.415l4.95 4.95L19.777 5.465a1 1 0 0 1 1.414 0Z"/></g>'),
    edit: SVG('<g fill="none" fill-rule="evenodd"><path fill="currentColor" d="M13.896 3.03a2 2 0 0 1 2.701-.117l.127.117 4.243 4.243a2 2 0 0 1 .117 2.7l-.117.128-10.314 10.314a2 2 0 0 1-1.238.578L9.239 21H4.006a1.01 1.01 0 0 1-1.004-.9l-.006-.11v-5.233a2 2 0 0 1 .467-1.284l.12-.13L13.895 3.03ZM12.17 7.584l-7.174 7.174V19H9.24l7.174-7.174-4.243-4.243Zm3.14-3.14L13.584 6.17l4.243 4.243 1.726-1.726z"/></g>'),
    del: SVG('<g fill="none"><path fill="currentColor" d="M14.28 2a2 2 0 0 1 1.897 1.368L16.72 5H20a1 1 0 1 1 0 2l-.003.071-.867 12.143A3 3 0 0 1 16.138 22H7.862a3 3 0 0 1-2.992-2.786L4.003 7.07A1.01 1.01 0 0 1 4 7a1 1 0 0 1 0-2h3.28l.543-1.632A2 2 0 0 1 9.721 2zm3.717 5H6.003l.862 12.071a1 1 0 0 0 .997.929h8.276a1 1 0 0 0 .997-.929zM10 10a1 1 0 0 1 .993.883L11 11v5a1 1 0 0 1-1.993.117L9 16v-5a1 1 0 0 1 1-1m4 0a1 1 0 0 1 1 1v5a1 1 0 1 1-2 0v-5a1 1 0 0 1 1-1m.28-6H9.72l-.333 1h5.226z"/></g>'),
    plus: SVG('<g fill="none"><path fill="currentColor" d="M11 20a1 1 0 1 0 2 0v-7h7a1 1 0 1 0 0-2h-7V4a1 1 0 1 0-2 0v7H4a1 1 0 1 0 0 2h7z"/></g>'),
    none: SVG('<g fill="none"><path fill="currentColor" d="M12 2c5.523 0 10 4.477 10 10s-4.477 10-10 10S2 17.523 2 12 6.477 2 12 2m0 2a8 8 0 1 0 0 16 8 8 0 0 0 0-16m2.829 3.757a1 1 0 0 1 1.497 1.32l-.083.095-7.071 7.07a1 1 0 0 1-1.498-1.32l.083-.094 7.072-7.07Z"/></g>'),
  };
  const VAR = { classic: _t("Классическая"), slim: _t("Тонкая") };
  const LS_ANIM = 'fl.sk.anim', LS_ELY = 'fl.sk.ely';
  let acc = null, prof = null, defs = null, busy = false, sel = null, suppress = false;
  let ely = localStorage.getItem(LS_ELY) === '1';

  // ---------- skinview3d (MIT), loaded the first time the page opens ----------
  let libP = null;
  const lib = () => libP ||= new Promise((ok, no) => {
    if (window.skinview3d) return ok(window.skinview3d);
    const s = document.createElement('script');
    s.src = 'vendor/skinview3d.min.js';
    s.onload = () => ok(window.skinview3d);
    s.onerror = () => { libP = null; no(new Error(_t("Не удалось загрузить 3D"))); };
    document.head.append(s);
  });
  const ANIMS = { idle: 'IdleAnimation', walk: 'WalkingAnimation', run: 'RunningAnimation', fly: 'FlyingAnimation' };
  const anim = (L, k) => new L[ANIMS[k] || ANIMS.idle]();

  const load = (src) => new Promise((ok, no) => { const i = new Image(); i.onload = () => ok(i); i.onerror = no; i.src = src; });
  // Arm model from the texture: any opaque pixel where the 4th arm column would be means classic.
  async function detect(src) {
    const img = await load(src);
    if (img.naturalHeight !== 64) return 'classic';
    const c = document.createElement('canvas'); c.width = 64; c.height = 64;
    const x = c.getContext('2d'); x.drawImage(img, 0, 0);
    const a = x.getImageData(54, 20, 2, 12).data;
    for (let i = 3; i < a.length; i += 4) if (a[i] > 0) return 'classic';
    return 'slim';
  }
  async function capeFront(src) {
    const img = await load(src);
    const k = img.naturalWidth / 64, c = document.createElement('canvas'); c.width = 10; c.height = 16;
    const x = c.getContext('2d'); x.imageSmoothingEnabled = false;
    x.drawImage(img, k, k, 10 * k, 16 * k, 0, 0, 10, 16);
    return c.toDataURL();
  }

  // ---------- Main stage ----------
  let view = null, viewP = null, mode = localStorage.getItem(LS_ANIM) || 'idle', nameShown = null, drawN = 0;
  function stage() {
    return viewP ||= lib().then((L) => {
      const box = q('#sk-stage'), c = q('#sk-canvas');
      view = new L.SkinViewer({ canvas: c, width: Math.max(1, box.clientWidth), height: Math.max(1, box.clientHeight), zoom: 0.68, fov: 38 });
      view.controls.enableZoom = false; view.controls.enablePan = false;
      view.playerWrapper.rotation.y = -0.42;
      view.animation = anim(L, mode);
      new ResizeObserver(() => { view.setSize(Math.max(1, box.clientWidth), Math.max(1, box.clientHeight)); q('#sk-anim')._place?.(false); }).observe(box);
      new IntersectionObserver((es) => { view.renderPaused = !es[0].isIntersecting; }).observe(box);
      // A click without a drag waves back, like poking the player in the game.
      let down = null, waveT = 0;
      c.addEventListener('pointerdown', (e) => { down = [e.clientX, e.clientY]; });
      c.addEventListener('pointerup', (e) => {
        if (!down || Math.hypot(e.clientX - down[0], e.clientY - down[1]) > 4) return;
        down = null; clearTimeout(waveT);
        view.animation = new L.WaveAnimation();
        waveT = setTimeout(() => { view.animation = anim(L, mode); }, 1800);
      });
      return view;
    }).catch((e) => { viewP = null; throw e; });
  }

  const activeCape = () => prof?.capes?.find((c) => c.active) || null;
  const shownCapeId = () => (sel && sel.cape !== undefined ? sel.cape : activeCape()?.id ?? null);
  function shown3d() {
    const capeId = shownCapeId(), cape = prof?.capes?.find((c) => c.id === capeId);
    const src = sel || prof?.skin || {};
    return { img: src.img, slim: src.variant === 'slim', cape: cape?.img || null };
  }
  async function draw() {
    const n = ++drawN, st = q('#sk-stage');
    const v = await stage().catch((e) => { toast(String(e.message || e), { error: true }); return null; });
    if (!v || n !== drawN) return;
    const s = shown3d();
    st.classList.add('is-loading');
    try {
      if (s.img) await v.loadSkin(s.img, { model: s.slim ? 'slim' : 'default' }); else v.loadSkin(null);
      if (n !== drawN) return;
      if (s.cape) await v.loadCape(s.cape, { backEquipment: ely ? 'elytra' : 'cape' }); else v.loadCape(null);
    } catch { /* keep the previous picture */ } finally { if (n === drawN) st.classList.remove('is-loading'); }
    const name = prof?.name || acc?.name || null;
    if (name !== nameShown) {
      const L = window.skinview3d, f = getComputedStyle(document.documentElement).getPropertyValue('--font-display').trim() || 'system-ui, sans-serif';
      v.nameTag = name ? new L.NameTagObject(name, { font: `600 44px ${f}`, margin: [8, 14, 8, 14], textStyle: '#fff', backgroundStyle: 'rgba(0,0,0,.42)' }) : null;
      nameShown = name;
    }
    const e = q('#sk-ely');
    e.disabled = !s.cape; e.setAttribute('aria-pressed', String(ely && !!s.cape));
  }

  // ---------- Baked thumbnails: one offscreen viewer renders front and back ----------
  let baker = null, chain = Promise.resolve();
  const baked = new Map();
  function bake(img, slim) {
    const k = (slim ? 's' : 'c') + img;
    if (!baked.has(k)) {
      const p = chain.then(async () => {
        const L = await lib();
        if (!baker) {
          baker = new L.SkinViewer({ canvas: document.createElement('canvas'), width: 240, height: 360, pixelRatio: 1, preserveDrawingBuffer: true, renderPaused: true, zoom: 0.9, fov: 34 });
          baker.controls.enabled = false;
        }
        await baker.loadSkin(img, { model: slim ? 'slim' : 'default' });
        baker.playerObject.rotation.y = 0.5; baker.render(); const f = baker.canvas.toDataURL();
        baker.playerObject.rotation.y = Math.PI + 0.5; baker.render(); const b = baker.canvas.toDataURL();
        return { f, b };
      }).catch(() => null);
      chain = p; baked.set(k, p);
    }
    return baked.get(k);
  }
  const io = new IntersectionObserver((es) => es.forEach((e) => { if (e.isIntersecting) { io.unobserve(e.target); e.target._bake?.(); } }), { rootMargin: '400px' });

  function card({ k, key, img, variant, name, meta, tools, onclick, ondbl }) {
    const f = el('img', { class: 'f', alt: '', draggable: false }), b = el('img', { class: 'b', alt: '', draggable: false });
    const c = el('button', { type: 'button', class: 'sk-card', title: name, 'data-id': 'skins.card', 'data-k': k, 'data-key': key || '' },
      el('span', { class: 'sk-fig' }, f, b, tools || null),
      el('span', { class: 'sk-cap' }, el('b', {}, name), el('span', {}, meta || '')));
    c.addEventListener('click', (e) => { if (suppress || e.target.closest('.sk-t')) return; onclick(); });
    if (ondbl) c.addEventListener('dblclick', (e) => { if (!e.target.closest('.sk-t')) ondbl(); });
    if (img) { c._bake = () => bake(img, variant === 'slim').then((r) => { if (r) { f.src = r.f; b.src = r.b; c.classList.add('is-ready'); } }); io.observe(c); }
    return c;
  }
  const skel = (n) => Array.from({ length: n }, () => el('span', { class: 'sk-card is-skel', 'aria-hidden': 'true' }));

  // Worn / selected marks without rebuilding the grids.
  function marks() {
    const cur = prof?.skin?.key;
    document.querySelectorAll('#view-skins .sk-card[data-k]').forEach((c) => {
      const on = !!cur && c.dataset.key === cur;
      c.classList.toggle('is-on', on);
      c.classList.toggle('is-sel', !!sel && sel.k === c.dataset.k);
      const em = c.querySelector('.sk-on');
      if (on && !em) c.querySelector('.sk-fig').append(el('em', { class: 'sk-on', html: _t("{0}Надет", [I.check]) }));
      else if (!on && em) em.remove();
    });
  }

  // ---------- Pending preview ----------
  function pending() {
    const p = !!sel;
    q('#sk-badge').hidden = !p;
    q('#sk-acts-pend').hidden = !p;
    q('#sk-acts-cur').hidden = p;
    q('#sk-keep').hidden = !p || sel.kind !== 'default' || !sel.img;
    const cape = activeCape();
    q('#sk-meta').textContent = q('#sk-meta').title = p ? _t("Предпросмотр: {0}", [sel.label])
      : _t("{0} модель{1}", [VAR[prof?.skin?.variant] || VAR.classic, cape ? _t(" · плащ {0}", [cape.alias]) : '']);
    requestAnimationFrame(() => q('#sk-anim')._place?.(false));
    marks(); capes(); draw();
  }
  function pick(s) {
    const worn = s.key && s.key === prof?.skin?.key;
    sel = sel?.k === s.k || worn ? null : s;
    pending();
  }
  async function apply() {
    const s = sel;
    if (!s) return;
    await run(async (c) => {
      let p = null;
      if (s.kind === 'saved') p = await c('skins.use', { name: s.name });
      else if (s.kind === 'default') p = await c('skins.default', { key: s.key });
      if (s.touched) p = await c('skins.cape', { cape: s.cape ?? null });
      return p;
    }, s.kind === 'cur' ? _t("Плащ обновлён") : _t("Скин надет"));
  }

  // ---------- Requests ----------
  function headChanged() {
    if (typeof HEADS !== 'undefined') HEADS.delete(acc.id);
    try { renderAccounts(S.accounts); } catch {}
  }
  async function run(fn, okMsg) {
    if (busy || !acc) return false;
    busy = true; page().classList.add('is-busy');
    try {
      const p = await fn((name, args = {}) => call(name, { id: acc.id, ...args }));
      if (p) { prof = p; sel = null; paint(); headChanged(); }
      if (okMsg) toast(okMsg);
      return true;
    } catch (e) {
      toast(String(e?.message || e), { error: true });
      return false;
    } finally { busy = false; page().classList.remove('is-busy'); }
  }

  // ---------- Library ----------
  function paint() {
    q('#sk-name').textContent = q('#sk-name').title = prof.name || acc.name;
    const saved = prof.saved || [];
    q('#sk-saved-n').textContent = saved.length ? String(saved.length) : '';
    const add = el('button', { type: 'button', class: 'sk-card sk-add', 'data-id': 'skins.add', onclick: () => choose('new') },
      el('span', { class: 'sk-add-i', html: I.plus }), el('b', {}, _t("Добавить скин")), el('span', {}, _t("PNG 64×64 или перетащите файл сюда")));
    q('#sk-saved').replaceChildren(add, ...saved.map((sk) => {
      const capeOf = sk.has_cape ? prof.capes.find((c) => c.id === sk.cape) : null;
      const tools = el('span', { class: 'sk-tools' },
        el('span', { class: 'sk-t', role: 'button', title: _t("Изменить"), 'aria-label': _t("Изменить"), html: I.edit, onclick: () => edit(sk) }),
        el('span', { class: 'sk-t is-del', role: 'button', title: _t("Удалить"), 'aria-label': _t("Удалить"), html: I.del, onclick: () => ask(sk) }));
      const c = card({
        k: `s:${sk.name}`, key: sk.key, img: sk.img, variant: sk.variant, tools,
        name: VAR[sk.variant] || VAR.classic,
        meta: sk.has_cape ? (capeOf ? _t("Плащ: {0}", [capeOf.alias]) : _t("Без плаща")) : _t("Плащ не меняется"),
        onclick: () => pick({ kind: 'saved', k: `s:${sk.name}`, name: sk.name, key: sk.key, img: sk.img, variant: sk.variant, cape: sk.has_cape ? sk.cape ?? null : undefined, label: _t("скин из библиотеки") }),
        ondbl: () => { sel = { kind: 'saved', name: sk.name }; apply(); },
      });
      c.dataset.name = sk.name;
      return c;
    }));
    pending();
  }

  function capeTile(c, on, choose) {
    const im = el('img', { alt: '', draggable: false });
    if (c?.img) capeFront(c.img).then((u) => { im.src = u; }).catch(() => {});
    return el('button', { type: 'button', class: `sk-cape${on ? ' is-on' : ''}`, title: c ? c.alias : _t("Без плаща"), onclick: () => choose(c ? c.id : null) },
      c ? im : el('span', { class: 'sk-cape-none', html: I.none }), el('span', {}, c ? c.alias || _t("Плащ") : _t("Без плаща")));
  }
  function capeRow(box, cur, choose) {
    box.replaceChildren(capeTile(null, cur == null, choose), ...(prof?.capes || []).map((c) => capeTile(c, c.id === cur, choose)));
  }
  function capes() {
    const list = prof?.capes || [];
    q('#sk-capes-sec').hidden = !list.length;
    if (!list.length) return;
    capeRow(q('#sk-capes'), shownCapeId(), (id) => {
      if (id === shownCapeId()) return;
      if (!sel) sel = { kind: 'cur', k: null, img: prof.skin?.img, variant: prof.skin?.variant, label: _t("другой плащ") };
      sel.cape = id; sel.touched = true;
      if (sel.kind === 'cur' && id === (activeCape()?.id ?? null)) sel = null;
      pending();
    });
  }

  // ---------- Vanilla skins ----------
  function renderDefaults() {
    const box = q('#sk-defaults');
    if (!defs) {
      box.replaceChildren(el('section', { class: 'sk-sec', 'data-id': 'skins.pack.loading' }, el('h2', { class: 'sk-h' }, el('span', {}, _t("Стандартные"))), el('div', { class: 'sk-grid' }, ...skel(6))));
      return;
    }
    const term = q('#sk-q').value.trim().toLocaleLowerCase(_LOC);
    const groups = new Map();
    defs.filter((d) => d.img && (!term || d.name.toLocaleLowerCase(_LOC).includes(term) || d.section.toLocaleLowerCase(_LOC).includes(term)))
      .forEach((d) => { if (!groups.has(d.section)) groups.set(d.section, []); groups.get(d.section).push(d); });
    const secs = [...groups].map(([name, list]) => el('section', { class: 'sk-sec', 'data-id': `skins.pack.${name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'std'}` },
      el('h2', { class: 'sk-h' }, el('span', {}, name), el('i', {}, String(list.length))),
      el('div', { class: 'sk-grid' }, ...list.map((d) => card({
        k: `d:${d.key}`, key: d.key, img: d.img, variant: d.variant, name: d.name, meta: _t("{0} модель", [VAR[d.variant]]),
        onclick: () => pick({ kind: 'default', k: `d:${d.key}`, key: d.key, img: d.img, variant: d.variant, label: d.name }),
        ondbl: () => { sel = { kind: 'default', key: d.key }; apply(); },
      })))));
    box.replaceChildren(...(secs.length ? secs : [el('p', { class: 'sk-nothing' }, _t("Ничего не нашлось"))]));
    marks();
  }
  async function loadDefaults() {
    // Coming back to the page keeps the grid that is already there.
    if (defs) return q('#sk-defaults .sk-card:not(.is-skel), #sk-defaults .sk-nothing') ? marks() : renderDefaults();
    renderDefaults();
    const r = await call('skins.defaults').catch(() => null);
    if (r?.skins) { defs = r.skins; renderDefaults(); }
    else q('#sk-defaults').replaceChildren();
  }

  // ---------- Reorder by dragging ----------
  (() => {
    const grid = q('#sk-saved');
    let d = null;
    grid.addEventListener('pointerdown', (e) => {
      const c = e.target.closest('.sk-card[data-name]');
      if (c && e.button === 0 && !e.target.closest('.sk-t')) d = { c, x: e.clientX, y: e.clientY, on: false };
    });
    addEventListener('pointermove', (e) => {
      if (!d) return;
      if (!d.on) {
        if (Math.hypot(e.clientX - d.x, e.clientY - d.y) < 6) return;
        d.on = true; d.c.classList.add('is-dragging'); page().classList.add('is-sorting');
      }
      const o = document.elementFromPoint(e.clientX, e.clientY)?.closest('#sk-saved .sk-card[data-name]');
      if (!o || o === d.c) return;
      const r = o.getBoundingClientRect();
      o[e.clientX > r.left + r.width / 2 ? 'after' : 'before'](d.c);
    });
    addEventListener('pointerup', () => {
      const x = d; d = null;
      if (!x?.on) return;
      x.c.classList.remove('is-dragging'); page().classList.remove('is-sorting');
      suppress = true; setTimeout(() => { suppress = false; }, 0);
      const names = [...grid.querySelectorAll('.sk-card[data-name]')].map((n) => n.dataset.name);
      prof.saved.sort((a, b) => names.indexOf(a.name) - names.indexOf(b.name));
      call('skins.order', { names }).catch((e) => toast(String(e?.message || e), { error: true }));
    });
  })();

  // ---------- Edit / new skin dialog ----------
  let ed = null, edView = null;
  function setSeg(id, v) {
    document.querySelectorAll(`${id} > button`).forEach((b) => b.setAttribute('aria-selected', String(b.dataset.v === v)));
    requestAnimationFrame(() => q(id)._place?.(false));
  }
  async function edDraw() {
    const L = await lib().catch(() => null);
    if (!L || !ed) return;
    if (!edView) {
      const box = q('.ske-stage');
      edView = new L.SkinViewer({ canvas: q('#ske-canvas'), width: box.clientWidth || 240, height: box.clientHeight || 340, zoom: 0.86, fov: 40 });
      edView.controls.enableZoom = false; edView.controls.enablePan = false;
      edView.playerWrapper.rotation.y = -0.42;
      edView.animation = new L.IdleAnimation();
      new ResizeObserver(() => { if (box.clientWidth) edView.setSize(box.clientWidth, box.clientHeight); }).observe(box);
    }
    await edView.loadSkin(ed.data, { model: ed.variant === 'slim' ? 'slim' : 'default' }).catch(() => {});
    const cape = prof?.capes?.find((c) => c.id === ed.cape);
    if (cape?.img) await edView.loadCape(cape.img).catch(() => {}); else edView.loadCape(null);
  }
  function editor(o) {
    ed = o;
    q('#ske-h').textContent = o.name ? _t("Изменить скин") : _t("Новый скин");
    setSeg('#ske-var', o.variant);
    q('#ske-cape-f').hidden = !prof?.capes?.length;
    const row = () => capeRow(q('#ske-capes'), ed.cape, (id) => { ed.cape = id; row(); edDraw(); });
    row();
    FUI.openDlg('dlg-skin');
    requestAnimationFrame(edDraw);
  }
  async function fresh(data) {
    const im = await load(data).catch(() => null);
    if (!im || im.naturalWidth !== 64 || (im.naturalHeight !== 64 && im.naturalHeight !== 32)) { toast(_t("Скин должен быть PNG 64×64 или 64×32"), { error: true }); return null; }
    return detect(data);
  }
  async function create(data) {
    const v = await fresh(data);
    if (v) editor({ data, variant: v, cape: activeCape()?.id ?? null });
  }
  function edit(sk) {
    editor({ name: sk.name, data: sk.img, variant: sk.variant, cape: sk.has_cape ? sk.cape ?? null : activeCape()?.id ?? null });
  }
  let target = 'new';
  function choose(t) { target = t; q('#sk-file').click(); }
  q('#sk-file').addEventListener('change', async (e) => {
    const f = e.target.files?.[0]; e.target.value = '';
    if (!f) return;
    const data = await new Promise((ok) => { const r = new FileReader(); r.onload = () => ok(r.result); r.readAsDataURL(f); });
    if (target === 'new') return create(data);
    const v = await fresh(data);
    if (!v || !ed) return;
    Object.assign(ed, { data, variant: v, replaced: true });
    setSeg('#ske-var', v); edDraw();
  });
  q('#ske-tex').addEventListener('click', () => choose('replace'));
  q('#ske-var').addEventListener('seg:change', (e) => { if (ed) { ed.variant = e.detail.dataset.v; edDraw(); } });
  // «Сохранить» keeps the skin in the library only; «Сохранить и надеть» also puts it on.
  async function commitEd(wearIt) {
    const o = ed;
    if (!o) return;
    const cape = o.cape == null ? '' : o.cape;
    const done = await run((c) => (o.name
      ? c('skins.edit', { name: o.name, variant: o.variant, data: o.replaced ? o.data : undefined, cape, wear: wearIt })
      : c(wearIt ? 'skins.upload' : 'skins.save', { variant: o.variant, data: o.data, cape })), wearIt ? _t("Скин надет") : _t("Скин сохранён"));
    if (done) FUI.closeDlg();
  }
  q('#dlg-skin').addEventListener('submit', (e) => { e.preventDefault(); commitEd(true); });
  q('#ske-keep').addEventListener('click', () => commitEd(false));

  // ---------- Delete ----------
  let doomed = null;
  function ask(sk) { doomed = sk; FUI.openDlg('dlg-sk-del'); }
  q('#dlg-sk-del').addEventListener('submit', async (e) => {
    e.preventDefault();
    const sk = doomed; doomed = null;
    FUI.closeDlg();
    if (!sk) return;
    try {
      await call('skins.forget', { name: sk.name });
      prof.saved = prof.saved.filter((x) => x.name !== sk.name);
      if (sel?.name === sk.name) sel = null;
      paint();
    } catch (er) { toast(String(er?.message || er), { error: true }); }
  });

  // ---------- Page ----------
  function empty(text) {
    q('#sk-body').hidden = true;
    q('#sk-empty').hidden = false;
    q('#sk-empty-t').textContent = text;
  }
  async function shown() {
    const st = S.accounts;
    acc = st?.accounts?.find((a) => a.id === st.selected) || null;
    if (!acc || acc.kind?.type !== 'microsoft') return empty(_t("Скины меняются у аккаунта Microsoft"));
    q('#sk-empty').hidden = true; q('#sk-body').hidden = false;
    if (!prof) q('#sk-name').textContent = acc.name;
    loadDefaults();
    try {
      const p = await call('skins.profile', { id: acc.id });
      // Same profile as on screen: only the marks change, the cards stay (no re-bake, no fade).
      if (prof && p && JSON.stringify(p) === JSON.stringify(prof) && q('#sk-saved').childElementCount) { prof = p; sel = null; marks(); return; }
      prof = p; sel = null; paint(); if (defs) marks(); else renderDefaults();
    }
    catch (e) { empty(String(e?.message || e)); }
  }
  async function drop(path) {
    if (!acc || !prof) return;
    try { const r = await call('skins.read', { path }); await create(r.data); }
    catch (e) { toast(String(e?.message || e), { error: true }); }
  }

  q('#sk-apply').addEventListener('click', apply);
  q('#sk-keep').addEventListener('click', () => {
    const s = sel;
    if (s?.kind === 'default' && s.img) run((c) => c('skins.save', { variant: s.variant || 'classic', data: s.img }), _t("Скин добавлен в библиотеку"));
  });
  q('#sk-cancel').addEventListener('click', () => { sel = null; pending(); });
  q('#sk-edit').addEventListener('click', () => {
    if (!prof?.skin?.img) return choose('new');
    const sk = (prof.saved || []).find((x) => x.key && x.key === prof.skin.key);
    if (sk) edit(sk); else editor({ data: prof.skin.img, variant: prof.skin.variant || 'classic', cape: activeCape()?.id ?? null });
  });
  q('#sk-q').addEventListener('input', renderDefaults);
  setSeg('#sk-anim', mode);
  q('#sk-anim').addEventListener('seg:change', (e) => {
    mode = e.detail.dataset.v; localStorage.setItem(LS_ANIM, mode);
    if (view) lib().then((L) => { view.animation = anim(L, mode); });
  });
  q('#sk-ely').addEventListener('click', () => { ely = !ely; localStorage.setItem(LS_ELY, ely ? '1' : '0'); draw(); });
  addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && sel && S.view === 'skins' && !document.querySelector('.dialog[data-open], .dialog[open]')) { sel = null; pending(); }
    if (e.key === 'Enter' && sel && S.view === 'skins' && (e.ctrlKey || e.metaKey)) apply();
  });

  window.FSkins = { shown, drop };
})();