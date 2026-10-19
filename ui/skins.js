// Skins page: the account's skin, arm model, saved skins and capes (Modrinth App style).
// Everything goes through the official Minecraft services API in the core (skins.*).
(() => {
  const q = (s) => document.querySelector(s);
  const page = () => q('#view-skins .skins');
  let acc = null, busy = false, has = false;

  const load = (src) => new Promise((ok, no) => { const i = new Image(); i.onload = () => ok(i); i.onerror = no; i.src = src; });

  // Flat front view of a skin: 16x32 px, shown pixelated.
  async function front(src, slim) {
    const img = await load(src), tall = img.naturalHeight === 64;
    const c = document.createElement('canvas'); c.width = 16; c.height = 32;
    const x = c.getContext('2d'); x.imageSmoothingEnabled = false;
    const d = (sx, sy, w, h, dx, dy, flip) => {
      if (!flip) return x.drawImage(img, sx, sy, w, h, dx, dy, w, h);
      x.save(); x.translate(dx + w, dy); x.scale(-1, 1); x.drawImage(img, sx, sy, w, h, 0, 0, w, h); x.restore();
    };
    const aw = slim ? 3 : 4;
    d(8, 8, 8, 8, 4, 0); d(20, 20, 8, 12, 4, 8);
    d(44, 20, aw, 12, 4 - aw, 8);
    tall ? d(36, 52, aw, 12, 12, 8) : d(44, 20, aw, 12, 12, 8, true);
    d(4, 20, 4, 12, 4, 20);
    tall ? d(20, 52, 4, 12, 8, 20) : d(4, 20, 4, 12, 8, 20, true);
    // Like the game: a hat layer with no transparent pixel at all is ignored (old 64x32 skins).
    const hat = (() => {
      const t = document.createElement('canvas'); t.width = 32; t.height = 16;
      const g = t.getContext('2d'); g.drawImage(img, 32, 0, 32, 16, 0, 0, 32, 16);
      const a = g.getImageData(0, 0, 32, 16).data;
      for (let i = 3; i < a.length; i += 4) if (a[i] < 255) return true;
      return false;
    })();
    if (hat) d(40, 8, 8, 8, 4, 0);
    if (tall) { d(20, 36, 8, 12, 4, 8); d(44, 36, aw, 12, 4 - aw, 8); d(52, 52, aw, 12, 12, 8); d(4, 36, 4, 12, 4, 20); d(4, 52, 4, 12, 8, 20); }
    return c.toDataURL();
  }
  async function capeFront(src) {
    const img = await load(src);
    const k = img.naturalWidth / 64, c = document.createElement('canvas'); c.width = 10; c.height = 16;
    const x = c.getContext('2d'); x.imageSmoothingEnabled = false;
    x.drawImage(img, k, k, 10 * k, 16 * k, 0, 0, 10, 16);
    return c.toDataURL();
  }
  const pic = (src, slim, fn = front) => {
    const im = el('img', { alt: '', draggable: 'false' });
    if (src) fn(src, slim).then((u) => { im.src = u; }).catch(() => {});
    return im;
  };

  const seg = () => q('#sk-variant [aria-selected="true"]')?.dataset.v || 'classic';
  function setSeg(v) {
    document.querySelectorAll('#sk-variant > button').forEach((b) => b.setAttribute('aria-selected', String(b.dataset.v === v)));
    requestAnimationFrame(() => q('#sk-variant')._place?.(false));
  }

  function paint(p) {
    q('#sk-name').textContent = p.name || acc.name;
    const v = p.skin?.variant === 'slim' ? 'slim' : 'classic';
    has = !!p.skin?.img;
    setSeg(v);
    const f = q('#sk-front');
    f.removeAttribute('src');
    if (p.skin?.img) front(p.skin.img, v === 'slim').then((u) => { f.src = u; }).catch(() => {});
    const saved = p.saved || [];
    q('#sk-saved-h').hidden = q('#sk-saved').hidden = !saved.length;
    q('#sk-saved').replaceChildren(...saved.map((sk) => {
      const x = el('span', { class: 'x', role: 'button', 'aria-label': 'Убрать', html: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="m4.5 4.5 7 7m0-7-7 7"/></svg>' });
      const t = el('button', { class: 'sk-tile', type: 'button', title: sk.variant === 'slim' ? 'Тонкая' : 'Классическая' }, pic(sk.img, sk.variant === 'slim'), x);
      t.addEventListener('click', (e) => {
        if (e.target.closest('.x')) {
          e.stopPropagation();
          call('skins.forget', { name: sk.name }).then(() => { t.remove(); if (!q('#sk-saved').children.length) q('#sk-saved-h').hidden = q('#sk-saved').hidden = true; }).catch((er) => toast(String(er?.message || er), { error: true }));
          return;
        }
        act('skins.use', { name: sk.name });
      });
      return t;
    }));
    const capes = p.capes || [];
    q('#sk-capes-h').hidden = q('#sk-capes').hidden = !capes.length;
    const none = el('button', { class: `sk-tile cape${capes.some((c) => c.active) ? '' : ' is-on'}`, type: 'button' }, el('b', {}, 'Без плаща'));
    none.addEventListener('click', () => act('skins.cape', { cape: null }));
    q('#sk-capes').replaceChildren(none, ...capes.map((c) => {
      const t = el('button', { class: `sk-tile cape${c.active ? ' is-on' : ''}`, type: 'button' }, pic(c.img, false, capeFront), el('span', {}, c.alias || ''));
      t.addEventListener('click', () => { if (!c.active) act('skins.cape', { cape: c.id }); });
      return t;
    }));
  }

  function headChanged() {
    if (typeof HEADS !== 'undefined') HEADS.delete(acc.id);
    try { renderAccounts(S.accounts); } catch {}
  }

  async function act(name, args = {}, quiet) {
    if (busy || !acc) return;
    busy = true; page().classList.add('is-busy');
    try {
      const p = await call(name, { id: acc.id, ...args });
      paint(p);
      if (name !== 'skins.profile') headChanged();
      return true;
    } catch (e) {
      if (quiet) throw e;
      toast(String(e?.message || e), { error: true });
    } finally { busy = false; page().classList.remove('is-busy'); }
  }

  function empty(text) {
    q('#sk-body').hidden = true;
    q('#sk-empty').hidden = false;
    q('#sk-empty-t').textContent = text;
  }

  async function shown() {
    const st = S.accounts;
    acc = st?.accounts?.find((a) => a.id === st.selected) || null;
    if (!acc || acc.kind?.type !== 'microsoft') return empty('Скин меняется у аккаунта Microsoft');
    q('#sk-empty').hidden = true; q('#sk-body').hidden = false;
    try { await act('skins.profile', {}, true); } catch (e) { empty(String(e?.message || e)); }
  }

  q('#sk-upload').addEventListener('click', () => q('#sk-file').click());
  q('#sk-file').addEventListener('change', async (e) => {
    const f = e.target.files?.[0]; e.target.value = '';
    if (!f) return;
    const data = await new Promise((ok) => { const r = new FileReader(); r.onload = () => ok(r.result); r.readAsDataURL(f); });
    const im = await load(data).catch(() => null);
    if (!im || im.naturalWidth !== 64 || (im.naturalHeight !== 64 && im.naturalHeight !== 32)) return toast('Скин должен быть PNG 64×64 или 64×32', { error: true });
    if (await act('skins.upload', { variant: seg(), data })) toast('Скин обновлён');
  });
  q('#sk-reset').addEventListener('click', () => act('skins.reset'));
  q('#sk-variant').addEventListener('seg:change', (e) => { if (has) act('skins.variant', { variant: e.detail.dataset.v }); });

  window.FSkins = { shown };
})();
