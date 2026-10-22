// Skins page: live 3D stage, your library (order, cape per skin), vanilla skins, edit dialog.
// Everything goes through the official Minecraft services API in the core (skins.*).
(() => {
  const q = (s) => document.querySelector(s);
  const page = () => q('#view-skins .skins');
  const SVG = (d) => `<svg class="pi" viewBox="0 0 256 256" fill="currentColor" aria-hidden="true">${d}</svg>`;
  const I = {
    check: SVG('<path d="M232.49,80.49l-128,128a12,12,0,0,1-17,0l-56-56a12,12,0,1,1,17-17L96,183,215.51,63.51a12,12,0,0,1,17,17Z"/>'),
    edit: SVG('<path d="M221.66,90.34,192,120,136,64l29.66-29.66a8,8,0,0,1,11.31,0L221.66,79A8,8,0,0,1,221.66,90.34Z" class="duo"/><path d="M230.14,70.54,185.46,25.85a20,20,0,0,0-28.29,0L33.86,149.17A19.85,19.85,0,0,0,28,163.31V208a20,20,0,0,0,20,20H92.69a19.86,19.86,0,0,0,14.14-5.86L230.14,98.82a20,20,0,0,0,0-28.28ZM91,204H52V165l84-84,39,39ZM192,103,153,64l18.34-18.34,39,39Z"/>'),
    del: SVG('<path d="M200,56V208a8,8,0,0,1-8,8H64a8,8,0,0,1-8-8V56Z" class="duo"/><path d="M216,48H180V36A28,28,0,0,0,152,8H104A28,28,0,0,0,76,36V48H40a12,12,0,0,0,0,24h4V208a20,20,0,0,0,20,20H192a20,20,0,0,0,20-20V72h4a12,12,0,0,0,0-24ZM100,36a4,4,0,0,1,4-4h48a4,4,0,0,1,4,4V48H100Zm88,168H68V72H188ZM116,104v64a12,12,0,0,1-24,0V104a12,12,0,0,1,24,0Zm48,0v64a12,12,0,0,1-24,0V104a12,12,0,0,1,24,0Z"/>'),
    plus: SVG('<path d="M228,128a12,12,0,0,1-12,12H140v76a12,12,0,0,1-24,0V140H40a12,12,0,0,1,0-24h76V40a12,12,0,0,1,24,0v76h76A12,12,0,0,1,228,128Z"/>'),
    none: SVG('<path d="M128,20A108,108,0,1,0,236,128,108.12,108.12,0,0,0,128,20Zm84,108a83.6,83.6,0,0,1-16.75,50.28L77.72,60.75A84,84,0,0,1,212,128ZM44,128A83.6,83.6,0,0,1,60.75,77.72L178.28,195.25A84,84,0,0,1,44,128Z"/>'),
  };
  const VAR = { classic: 'Классическая', slim: 'Тонкая' };
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
    s.onerror = () => { libP = null; no(new Error('Не удалось загрузить 3D')); };
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
      if (on && !em) c.querySelector('.sk-fig').append(el('em', { class: 'sk-on', html: `${I.check}Надет` }));
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
    q('#sk-meta').textContent = q('#sk-meta').title = p ? `Предпросмотр: ${sel.label}`
      : `${VAR[prof?.skin?.variant] || VAR.classic} модель${cape ? ` · плащ ${cape.alias}` : ''}`;
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
    }, s.kind === 'cur' ? 'Плащ обновлён' : 'Скин надет');
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
      el('span', { class: 'sk-add-i', html: I.plus }), el('b', {}, 'Добавить скин'), el('span', {}, 'PNG 64×64 или перетащите файл сюда'));
    q('#sk-saved').replaceChildren(add, ...saved.map((sk) => {
      const capeOf = sk.has_cape ? prof.capes.find((c) => c.id === sk.cape) : null;
      const tools = el('span', { class: 'sk-tools' },
        el('span', { class: 'sk-t', role: 'button', title: 'Изменить', 'aria-label': 'Изменить', html: I.edit, onclick: () => edit(sk) }),
        el('span', { class: 'sk-t is-del', role: 'button', title: 'Удалить', 'aria-label': 'Удалить', html: I.del, onclick: () => ask(sk) }));
      const c = card({
        k: `s:${sk.name}`, key: sk.key, img: sk.img, variant: sk.variant, tools,
        name: VAR[sk.variant] || VAR.classic,
        meta: sk.has_cape ? (capeOf ? `Плащ: ${capeOf.alias}` : 'Без плаща') : 'Плащ не меняется',
        onclick: () => pick({ kind: 'saved', k: `s:${sk.name}`, name: sk.name, key: sk.key, img: sk.img, variant: sk.variant, cape: sk.has_cape ? sk.cape ?? null : undefined, label: 'скин из библиотеки' }),
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
    return el('button', { type: 'button', class: `sk-cape${on ? ' is-on' : ''}`, title: c ? c.alias : 'Без плаща', onclick: () => choose(c ? c.id : null) },
      c ? im : el('span', { class: 'sk-cape-none', html: I.none }), el('span', {}, c ? c.alias || 'Плащ' : 'Без плаща'));
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
      if (!sel) sel = { kind: 'cur', k: null, img: prof.skin?.img, variant: prof.skin?.variant, label: 'другой плащ' };
      sel.cape = id; sel.touched = true;
      if (sel.kind === 'cur' && id === (activeCape()?.id ?? null)) sel = null;
      pending();
    });
  }

  // ---------- Vanilla skins ----------
  function renderDefaults() {
    const box = q('#sk-defaults');
    if (!defs) {
      box.replaceChildren(el('section', { class: 'sk-sec', 'data-id': 'skins.pack.loading' }, el('h2', { class: 'sk-h' }, el('span', {}, 'Стандартные')), el('div', { class: 'sk-grid' }, ...skel(6))));
      return;
    }
    const term = q('#sk-q').value.trim().toLocaleLowerCase('ru');
    const groups = new Map();
    defs.filter((d) => d.img && (!term || d.name.toLocaleLowerCase('ru').includes(term) || d.section.toLocaleLowerCase('ru').includes(term)))
      .forEach((d) => { if (!groups.has(d.section)) groups.set(d.section, []); groups.get(d.section).push(d); });
    const secs = [...groups].map(([name, list]) => el('section', { class: 'sk-sec', 'data-id': `skins.pack.${name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'std'}` },
      el('h2', { class: 'sk-h' }, el('span', {}, name), el('i', {}, String(list.length))),
      el('div', { class: 'sk-grid' }, ...list.map((d) => card({
        k: `d:${d.key}`, key: d.key, img: d.img, variant: d.variant, name: d.name, meta: `${VAR[d.variant]} модель`,
        onclick: () => pick({ kind: 'default', k: `d:${d.key}`, key: d.key, img: d.img, variant: d.variant, label: d.name }),
        ondbl: () => { sel = { kind: 'default', key: d.key }; apply(); },
      })))));
    box.replaceChildren(...(secs.length ? secs : [el('p', { class: 'sk-nothing' }, 'Ничего не нашлось')]));
    marks();
  }
  async function loadDefaults() {
    if (defs) return renderDefaults();
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
    q('#ske-h').textContent = o.name ? 'Изменить скин' : 'Новый скин';
    setSeg('#ske-var', o.variant);
    q('#ske-cape-f').hidden = !prof?.capes?.length;
    const row = () => capeRow(q('#ske-capes'), ed.cape, (id) => { ed.cape = id; row(); edDraw(); });
    row();
    FUI.openDlg('dlg-skin');
    requestAnimationFrame(edDraw);
  }
  async function fresh(data) {
    const im = await load(data).catch(() => null);
    if (!im || im.naturalWidth !== 64 || (im.naturalHeight !== 64 && im.naturalHeight !== 32)) { toast('Скин должен быть PNG 64×64 или 64×32', { error: true }); return null; }
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
      : c(wearIt ? 'skins.upload' : 'skins.save', { variant: o.variant, data: o.data, cape })), wearIt ? 'Скин надет' : 'Скин сохранён');
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
    if (!acc || acc.kind?.type !== 'microsoft') return empty('Скины меняются у аккаунта Microsoft');
    q('#sk-empty').hidden = true; q('#sk-body').hidden = false;
    if (!prof) q('#sk-name').textContent = acc.name;
    loadDefaults();
    try { prof = await call('skins.profile', { id: acc.id }); sel = null; paint(); renderDefaults(); }
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
    if (s?.kind === 'default' && s.img) run((c) => c('skins.save', { variant: s.variant || 'classic', data: s.img }), 'Скин добавлен в библиотеку');
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