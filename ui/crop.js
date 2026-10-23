'use strict';
// Crop dialog for own instance pictures (icon 1:1, cover 16:9). The frame moves by
// dragging, grows or shrinks with the wheel, corners resize it; the crop is drawn to a
// canvas here and only the finished image goes to the core.
(() => {
  const root = document.documentElement;
  const SPEC = {
    icon: { aspect: 1, out: [256, 256], type: 'image/png', pv: ['pv-ico-l', 'pv-ico-m', 'pv-ico-s'] },
    cover: { aspect: 16 / 9, out: [1600, 900], type: 'image/webp', pv: ['pv-cov-l', 'pv-cov-s'] },
  };
  const motion = () => root.dataset.motion !== 'off' && !matchMedia('(prefers-reduced-motion: reduce)').matches;
  const clamp = (x, a, b) => Math.min(b, Math.max(a, x));
  let ov = null, cv, ctx, stage, pvBox, meta, sl, okB, title;
  let st = null; // { kind, spec, bmp, iw, ih, t: target frame, d: drawn frame, fade, grid, save, prev }
  let W = 0, H = 0, dpr = 1, fit = { s: 1, ox: 0, oy: 0 }, raf = 0, dirty = true, act = null, gridT = 0;

  function build() {
    ov = document.createElement('div');
    ov.className = 'crop-ov';
    ov.innerHTML = '<div class="crop" role="dialog" aria-modal="true" data-id="dialog.crop"><h2></h2>'
      + _t("<div class=\"crop-body\"><div class=\"crop-stage\"><canvas class=\"crop-cv\" tabindex=\"0\" aria-label=\"Область кадра: перетаскивайте, колесо меняет размер\"></canvas></div>")
      + '<div class="crop-side"><div class="crop-pv"></div><span class="crop-meta mono"></span></div></div>'
      + '<div class="crop-zoom"><svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><g fill="none"><path fill="currentColor" d="M10.5 2a8.5 8.5 0 0 1 6.676 13.762l3.652 3.652a1 1 0 0 1-1.414 1.414l-3.652-3.652A8.5 8.5 0 1 1 10.5 2m0 2a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13M14 9.5a1 1 0 1 1 0 2H7a1 1 0 1 1 0-2z"/></g></svg>'
      + _t("<input type=\"range\" class=\"ue-sl crop-sl\" min=\"0\" max=\"100\" step=\"0.1\" aria-label=\"Масштаб\">")
      + '<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><g fill="none"><path fill="currentColor" d="M10.5 2a8.5 8.5 0 0 1 6.873 13.502l-.197.26 3.652 3.652a1 1 0 0 1-1.32 1.498l-.094-.084-3.652-3.652A8.5 8.5 0 1 1 10.5 2m0 2a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13m0 2a1 1 0 0 1 .993.883L11.5 7v2.5H14a1 1 0 0 1 .117 1.993L14 11.5h-2.5V14a1 1 0 0 1-1.993.117L9.5 14v-2.5H7a1 1 0 0 1-.117-1.993L7 9.5h2.5V7a1 1 0 0 1 1-1"/></g></svg></div>'
      + _t("<div class=\"actions\"><button type=\"button\" class=\"btn btn-ghost crop-other\">Другое фото</button><span class=\"spacer\"></span>")
      + _t("<button type=\"button\" class=\"btn btn-ghost crop-x\">Отмена</button><button type=\"button\" class=\"btn btn-primary crop-ok\"><span class=\"lbl\">Сохранить</span></button></div></div>");
    document.body.append(ov);
    const q = (s) => ov.querySelector(s);
    cv = q('.crop-cv'); ctx = cv.getContext('2d'); stage = q('.crop-stage'); pvBox = q('.crop-pv'); meta = q('.crop-meta');
    sl = q('.crop-sl'); okB = q('.crop-ok'); title = q('h2');
    ov.addEventListener('pointerdown', (e) => { if (e.target === ov) close(); });
    q('.crop-x').addEventListener('click', close);
    q('.crop-other').addEventListener('click', () => choose((f) => load(f)));
    okB.addEventListener('click', finish);
    sl.addEventListener('input', () => { if (!st) return; zoomTo(maxW() * (1 - 0.9 * (+sl.value / 100))); showGrid(); });
    cv.addEventListener('pointerdown', down);
    cv.addEventListener('pointermove', (e) => { if (!act) cv.style.cursor = cursorAt(pt(e)); });
    cv.addEventListener('wheel', wheel, { passive: false });
    cv.addEventListener('keydown', keys);
    stage.addEventListener('dragover', (e) => e.preventDefault());
    stage.addEventListener('drop', (e) => { e.preventDefault(); const f = e.dataTransfer?.files?.[0]; if (f) load(f); });
    new ResizeObserver(() => { if (st) { size(); dirty = true; } }).observe(stage);
    document.addEventListener('keydown', (e) => {
      if (!st) return;
      if (e.key === 'Escape') { e.preventDefault(); e.stopImmediatePropagation(); close(); }
      else if (e.key === 'Enter' && !e.target.closest?.('.actions')) { e.preventDefault(); e.stopImmediatePropagation(); finish(); }
    }, true);
  }

  // ---------- Geometry (frame in image pixels) ----------
  const A = () => st.spec.aspect;
  const maxW = () => Math.min(st.iw, st.ih * A());
  function clampF(f) {
    const w = clamp(f.w, maxW() * 0.1, maxW()), h = w / A();
    return { w, x: clamp(f.x, 0, st.iw - w), y: clamp(f.y, 0, st.ih - h) };
  }
  function size() {
    const r = stage.getBoundingClientRect();
    dpr = devicePixelRatio || 1; W = r.width; H = r.height;
    cv.width = Math.round(W * dpr); cv.height = Math.round(H * dpr);
    const pad = 18, s = Math.min((W - 2 * pad) / st.iw, (H - 2 * pad) / st.ih);
    fit = { s, ox: (W - st.iw * s) / 2, oy: (H - st.ih * s) / 2 };
  }
  const pt = (e) => { const r = cv.getBoundingClientRect(); return { x: (e.clientX - r.left - fit.ox) / fit.s, y: (e.clientY - r.top - fit.oy) / fit.s }; };
  function corner(p) {
    const f = st.t, h = f.w / A(), tol = 12 / fit.s;
    const cs = [[f.x, f.y, 'nwse'], [f.x + f.w, f.y, 'nesw'], [f.x, f.y + h, 'nesw'], [f.x + f.w, f.y + h, 'nwse']];
    const i = cs.findIndex(([x, y]) => Math.abs(p.x - x) < tol && Math.abs(p.y - y) < tol);
    return i < 0 ? null : { i, cur: `${cs[i][2]}-resize`, ax: cs[3 - i][0], ay: cs[3 - i][1] };
  }
  const inside = (p) => p.x >= st.t.x && p.x <= st.t.x + st.t.w && p.y >= st.t.y && p.y <= st.t.y + st.t.w / A();
  const cursorAt = (p) => corner(p)?.cur || (inside(p) ? 'grab' : 'crosshair');
  function zoomTo(w, ax, ay) {
    const f = st.t, h = f.w / A();
    ax ??= f.x + f.w / 2; ay ??= f.y + h / 2;
    const rx = (ax - f.x) / f.w, ry = (ay - f.y) / h, nw = clamp(w, maxW() * 0.1, maxW());
    st.t = clampF({ w: nw, x: ax - rx * nw, y: ay - ry * (nw / A()) });
    syncUi();
  }
  function showGrid() { gridT = performance.now() + 650; dirty = true; }
  function syncUi() {
    const v = clamp((1 - st.t.w / maxW()) / 0.9, 0, 1) * 100;
    sl.value = v; sl.style.setProperty('--p', `${v}%`);
    meta.textContent = `${Math.round(st.t.w)} × ${Math.round(st.t.w / A())}`;
    dirty = true;
  }

  // ---------- Input ----------
  function down(e) {
    if (e.button !== 0 || !st) return;
    e.preventDefault(); cv.focus({ preventScroll: true }); cv.setPointerCapture(e.pointerId);
    const p = pt(e), c = corner(p);
    if (c) act = { mode: 'size', c };
    else {
      if (!inside(p)) { st.t = clampF({ ...st.t, x: p.x - st.t.w / 2, y: p.y - st.t.w / A() / 2 }); syncUi(); }
      act = { mode: 'move', p0: p, f0: { ...st.t } };
      cv.style.cursor = 'grabbing';
    }
    const mv = (ev) => {
      const q = pt(ev);
      if (act.mode === 'move') st.t = clampF({ ...act.f0, x: act.f0.x + q.x - act.p0.x, y: act.f0.y + q.y - act.p0.y });
      else {
        const { ax, ay, i } = act.c, left = i === 0 || i === 2, top = i < 2;
        const room = Math.min(left ? ax : st.iw - ax, (top ? ay : st.ih - ay) * A());
        const w = clamp(Math.max(Math.abs(q.x - ax), Math.abs(q.y - ay) * A()), maxW() * 0.1, room);
        st.t = { w, x: left ? ax - w : ax, y: top ? ay - w / A() : ay };
      }
      st.d = { ...st.t }; syncUi(); showGrid();
    };
    const up = () => {
      act = null; cv.style.cursor = cursorAt(pt(e));
      cv.removeEventListener('pointermove', mv); cv.removeEventListener('pointerup', up); cv.removeEventListener('pointercancel', up);
    };
    cv.addEventListener('pointermove', mv); cv.addEventListener('pointerup', up); cv.addEventListener('pointercancel', up);
    showGrid();
  }
  function wheel(e) {
    if (!st) return;
    e.preventDefault();
    const p = pt(e), k = Math.exp(clamp(e.deltaY, -120, 120) * 0.0016);
    if (inside(p)) zoomTo(st.t.w * k, p.x, p.y); else zoomTo(st.t.w * k);
    showGrid();
  }
  function keys(e) {
    if (!st) return;
    const step = maxW() * (e.shiftKey ? 0.1 : 0.01);
    const d = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] }[e.key];
    if (d) { e.preventDefault(); st.t = clampF({ ...st.t, x: st.t.x + d[0], y: st.t.y + d[1] }); syncUi(); showGrid(); }
    else if (e.key === '+' || e.key === '=') { e.preventDefault(); zoomTo(st.t.w / 1.1); showGrid(); }
    else if (e.key === '-') { e.preventDefault(); zoomTo(st.t.w * 1.1); showGrid(); }
  }

  // ---------- Drawing ----------
  function rr(c, x, y, w, h, r) { c.beginPath(); c.roundRect(x, y, w, h, r); }
  function draw(now) {
    raf = requestAnimationFrame(draw);
    if (!st || !W) return;
    const m = motion(), k = m ? 0.28 : 1, d = st.d, t = st.t;
    const g = now < gridT || act ? 1 : 0;
    const moving = Math.abs(d.x - t.x) + Math.abs(d.y - t.y) + Math.abs(d.w - t.w) > 0.05 || Math.abs(st.fade - 1) > 0.01 || Math.abs(st.grid - g) > 0.01;
    if (!moving && !dirty) return;
    dirty = false;
    for (const key of ['x', 'y', 'w']) d[key] += (t[key] - d[key]) * k;
    st.fade += (1 - st.fade) * (m ? 0.18 : 1); st.grid += (g - st.grid) * (m ? 0.2 : 1);
    const { s, ox, oy } = fit;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0); ctx.clearRect(0, 0, W, H);
    ctx.imageSmoothingQuality = 'high';
    ctx.drawImage(st.bmp, ox, oy, st.iw * s, st.ih * s);
    const fx = ox + d.x * s, fy = oy + d.y * s, fw = d.w * s, fh = (d.w / A()) * s;
    const r = st.kind === 'icon' ? fw * 0.22 : 8;
    ctx.beginPath(); ctx.rect(0, 0, W, H); ctx.roundRect(fx, fy, fw, fh, r);
    ctx.fillStyle = `rgba(6, 7, 10, ${0.64 * st.fade})`; ctx.fill('evenodd');
    if (st.grid > 0.01) {
      ctx.save(); rr(ctx, fx, fy, fw, fh, r); ctx.clip();
      ctx.strokeStyle = `rgba(255, 255, 255, ${0.32 * st.grid})`; ctx.lineWidth = 1; ctx.beginPath();
      for (const f of [1 / 3, 2 / 3]) { ctx.moveTo(fx + fw * f, fy); ctx.lineTo(fx + fw * f, fy + fh); ctx.moveTo(fx, fy + fh * f); ctx.lineTo(fx + fw, fy + fh * f); }
      ctx.stroke(); ctx.restore();
    }
    ctx.strokeStyle = 'rgba(255, 255, 255, .92)'; ctx.lineWidth = 1.5; rr(ctx, fx, fy, fw, fh, r); ctx.stroke();
    const L = Math.min(16, fw / 4), o = 3;
    ctx.lineWidth = 3; ctx.lineCap = 'round'; ctx.strokeStyle = '#fff'; ctx.beginPath();
    for (const [x, y, sx, sy] of [[fx - o, fy - o, 1, 1], [fx + fw + o, fy - o, -1, 1], [fx - o, fy + fh + o, 1, -1], [fx + fw + o, fy + fh + o, -1, -1]]) {
      ctx.moveTo(x + sx * L, y); ctx.lineTo(x, y); ctx.lineTo(x, y + sy * L);
    }
    ctx.stroke();
    for (const p of pvBox.children) {
      const c = p.getContext('2d'), w = p.clientWidth, h = p.clientHeight;
      if (p.width !== Math.round(w * dpr)) { p.width = Math.round(w * dpr); p.height = Math.round(h * dpr); }
      c.setTransform(dpr, 0, 0, dpr, 0, 0); c.imageSmoothingQuality = 'high';
      c.clearRect(0, 0, w, h); c.drawImage(st.bmp, d.x, d.y, d.w, d.w / A(), 0, 0, w, h);
    }
  }

  // ---------- Open / load / save ----------
  function choose(then) {
    const i = document.createElement('input');
    i.type = 'file'; i.accept = 'image/png,image/jpeg,image/webp,image/gif';
    i.addEventListener('change', () => { const f = i.files?.[0]; if (f) then(f); });
    i.click();
  }
  async function load(src) {
    let bmp;
    try { bmp = await createImageBitmap(src); } catch { window.FUI?.toast(_t("Не удалось открыть картинку"), { error: true }); return false; }
    st.bmp?.close?.();
    Object.assign(st, { bmp, iw: bmp.width, ih: bmp.height });
    size();
    const w = maxW(), t = clampF({ w: w * 0.9, x: (st.iw - w * 0.9) / 2, y: (st.ih - (w * 0.9) / A()) / 2 });
    const full = clampF({ w, x: (st.iw - w) / 2, y: (st.ih - w / A()) / 2 });
    Object.assign(st, { t, d: full, fade: 0, grid: 0 });
    syncUi();
    return true;
  }
  async function open({ kind = 'cover', title: tt, save, source }) {
    if (!ov) build();
    const go = async (src) => {
      st = { kind, spec: SPEC[kind], save, prev: document.activeElement, fade: 0, grid: 0 };
      title.textContent = tt || '';
      pvBox.replaceChildren(...st.spec.pv.map((c) => { const n = document.createElement('canvas'); n.className = c; return n; }));
      okB.disabled = false; delete okB.dataset.state;
      ov.setAttribute('data-open', '');
      if (!(await load(src))) { close(); return; }
      cancelAnimationFrame(raf); raf = requestAnimationFrame(draw);
      setTimeout(() => cv.focus({ preventScroll: true }), 40);
    };
    if (source) go(source); else choose(go);
  }
  function close() {
    if (!st) return;
    ov.removeAttribute('data-open');
    const s = st; st = null; act = null;
    s.prev?.focus?.({ preventScroll: true });
    setTimeout(() => { if (!st) { cancelAnimationFrame(raf); s.bmp?.close?.(); } }, 320);
  }
  async function finish() {
    if (!st || okB.disabled) return;
    const s = st, f = s.t, [mw, mh] = s.spec.out;
    const ow = Math.round(Math.min(mw, Math.max(s.kind === 'icon' ? mw : 320, f.w))), oh = Math.round(ow / A());
    const c = document.createElement('canvas'); c.width = ow; c.height = oh;
    const x = c.getContext('2d'); x.imageSmoothingQuality = 'high';
    x.drawImage(s.bmp, f.x, f.y, f.w, f.w / A(), 0, 0, ow, oh);
    okB.disabled = true; okB.dataset.state = 'loading';
    try {
      const blob = await new Promise((r) => c.toBlob(r, s.spec.type, 0.9)) || await new Promise((r) => c.toBlob(r, 'image/png'));
      await s.save(new Uint8Array(await blob.arrayBuffer()));
      okB.dataset.state = 'success';
      close();
    } catch (e) {
      okB.disabled = false; delete okB.dataset.state;
      window.FUI?.toast(String(e?.message || e), { error: true });
    }
  }
  window.FCrop = { open, close };
})();