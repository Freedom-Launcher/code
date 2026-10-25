// Servers tab of an instance: the game's own multiplayer list (servers.dat) with a live ping,
// so a server can be added, checked and joined without opening the game first.
(() => {
  const { $, openDlg, closeDlg, toast } = FUI;
  const box = $('#tab-servers');
  const IC = { refresh: '<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><g fill="none"><path fill="currentColor" d="M1.498 12.082c-.01-1.267 1.347-1.987 2.379-1.406l.113.07 2.678 1.804c1.424.96.538 3.146-1.1 2.915l-.137-.025-.109-.024a7.504 7.504 0 0 0 13.175.335 1.5 1.5 0 1 1 2.6 1.498c-2.317 4.02-7.119 6.152-11.815 4.893a10.503 10.503 0 0 1-7.784-10.06m1.406-5.33C5.22 2.731 10.022.6 14.718 1.857a10.503 10.503 0 0 1 7.784 10.06c.01 1.267-1.347 1.987-2.379 1.407l-.113-.07-2.678-1.805c-1.424-.959-.538-3.145 1.099-2.914l.138.025.108.023A7.504 7.504 0 0 0 5.502 8.25a1.5 1.5 0 1 1-2.598-1.498"/></g></svg>', add: '<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><g fill="none"><path fill="currentColor" d="M10.5 20a1.5 1.5 0 0 0 3 0v-6.5H20a1.5 1.5 0 0 0 0-3h-6.5V4a1.5 1.5 0 0 0-3 0v6.5H4a1.5 1.5 0 0 0 0 3h6.5z"/></g></svg>', edit: '<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><g fill="none"><path fill="currentColor" d="m10.756 6.17 7.07 7.071-7.173 7.174a2 2 0 0 1-1.238.578L9.239 21H4.006a1.01 1.01 0 0 1-1.004-.9l-.006-.11v-5.233a2 2 0 0 1 .467-1.284l.12-.13 7.173-7.174Zm3.14-3.14a2 2 0 0 1 2.701-.117l.127.117 4.243 4.243a2 2 0 0 1 .117 2.7l-.117.128-1.726 1.726-7.07-7.071z"/></g></svg>', del: '<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><g fill="none" fill-rule="evenodd"><path fill="currentColor" d="M14.28 2a2 2 0 0 1 1.897 1.368L16.72 5H20a1 1 0 1 1 0 2l-.003.071-.867 12.143A3 3 0 0 1 16.138 22H7.862a3 3 0 0 1-2.992-2.786L4.003 7.07A1.01 1.01 0 0 1 4 7a1 1 0 0 1 0-2h3.28l.543-1.632A2 2 0 0 1 9.721 2zM9 10a1 1 0 0 0-.993.883L8 11v6a1 1 0 0 0 1.993.117L10 17v-6a1 1 0 0 0-1-1m6 0a1 1 0 0 0-1 1v6a1 1 0 1 0 2 0v-6a1 1 0 0 0-1-1m-.72-6H9.72l-.333 1h5.226z"/></g></svg>' };
  let shown = null, seq = 0, editing = null;
  const cache = new Map();
  const visible = () => !box.hidden && S.view === 'instance';

  // One ping per address per 30 s; the list and the dialog share it.
  function pingOf(addr, force = false) {
    const c = cache.get(addr);
    if (c && !force && Date.now() - c.at < 30000) return c.p;
    const p = call('servers.ping', { address: addr }).catch((e) => ({ online: false, error: String(e) }));
    cache.set(addr, { at: Date.now(), p });
    return p;
  }

  // ---------- MOTD: legacy § codes and JSON chat components ----------
  const COL = { black: '#000000', dark_blue: '#0000AA', dark_green: '#00AA00', dark_aqua: '#00AAAA', dark_red: '#AA0000',
    dark_purple: '#AA00AA', gold: '#FFAA00', gray: '#AAAAAA', dark_gray: '#555555', blue: '#5555FF', green: '#55FF55',
    aqua: '#55FFFF', red: '#FF5555', light_purple: '#FF55FF', yellow: '#FFFF55', white: '#FFFFFF' };
  const NAMES = Object.keys(COL), CODES = '0123456789abcdef';
  function motd(m) {
    const out = el('span', { class: 'sv-motd' });
    const push = (text, st) => {
      if (!text) return;
      const s = el('span', {}, text.replace(/\s*\n\s*/g, ' '));
      if (st.c) s.style.setProperty('--c', st.c);
      if (st.b) s.style.fontWeight = '600';
      if (st.i) s.style.fontStyle = 'italic';
      const deco = [st.u && 'underline', st.s && 'line-through'].filter(Boolean).join(' ');
      if (deco) s.style.textDecoration = deco;
      out.append(s);
    };
    const legacy = (str, base) => {
      const parts = String(str).split('§');
      let st = { ...base };
      push(parts[0], st);
      for (const p of parts.slice(1)) {
        const k = (p[0] || '').toLowerCase(), i = CODES.indexOf(k);
        if (i >= 0) st = { c: COL[NAMES[i]] };
        else if (k === 'l') st.b = true;
        else if (k === 'o') st.i = true;
        else if (k === 'n') st.u = true;
        else if (k === 'm') st.s = true;
        else if (k === 'r') st = { ...base };
        push(p.slice(1), st);
      }
    };
    const comp = (c, base, d = 0) => {
      if (c == null || d > 24) return;
      if (typeof c === 'string' || typeof c === 'number') return legacy(String(c), base);
      if (Array.isArray(c)) return c.forEach((x) => comp(x, base, d + 1));
      if (typeof c !== 'object') return;
      const st = { ...base };
      if (c.color) st.c = COL[c.color] || (/^#[0-9a-f]{6}$/i.test(c.color) ? c.color : st.c);
      if (c.bold != null) st.b = !!c.bold;
      if (c.italic != null) st.i = !!c.italic;
      if (c.underlined != null) st.u = !!c.underlined;
      if (c.strikethrough != null) st.s = !!c.strikethrough;
      legacy(c.text ?? c.translate ?? '', st);
      (Array.isArray(c.extra) ? c.extra : []).forEach((x) => comp(x, st, d + 1));
    };
    comp(m, {});
    if (!out.textContent.trim()) out.append(el('span', { class: 'sv-dim' }, _t("Без описания")));
    return out;
  }

  // ---------- Pieces ----------
  function setImg(i, src) {
    i.querySelector('img')?.remove();
    const img = el('img', { alt: '', src, draggable: false });
    img.addEventListener('load', () => i.classList.add('is-img'));
    img.addEventListener('error', () => { img.remove(); i.classList.remove('is-img'); });
    i.append(img);
  }
  function bars(n) {
    const b = el('span', { class: 'sv-sig', 'data-n': String(n) }, el('i'), el('i'), el('i'), el('i'));
    return b;
  }
  const level = (ms) => (ms < 80 ? 4 : ms < 150 ? 3 : ms < 300 ? 2 : 1);
  function card(s, actions) {
    const r = el('div', { class: 'mod-row sv-row', 'data-id': `server.${s.index ?? 'new'}` });
    const ico = el('i', { class: 'mod-ico sv-ico' }, initials(s.name || s.ip));
    if (s.icon) setImg(ico, `data:image/png;base64,${s.icon}`);
    const line = el('div', { class: 'sv-line' }, el('span', { class: 'sv-addr mono' }, s.ip),
      el('span', { class: 'sv-motd sv-dim' }, _t("Опрашиваем…")));
    const players = el('span', { class: 'sv-players mono num' });
    const ver = el('span', { class: 'badge-sm sv-ver', hidden: true });
    const sig = bars(0);
    r.append(ico, el('div', { class: 't' }, el('b', {}, s.name || _t("Сервер Minecraft")), line), el('div', { class: 'sv-stat' }, ver, players, sig));
    if (actions) {
      const go = el('button', { class: 'btn btn-sm btn-ghost sv-play', 'data-tip': _t("Зайти на сервер"), html: PLAY_SVG }, el('span', { class: 'lbl' }, _t("Играть")));
      go.addEventListener('click', () => play(S.current, true, false, { server: s.ip }));
      const ed = el('button', { class: 'btn btn-icon btn-ghost', 'data-tip': _t("Изменить"), 'aria-label': _t("Изменить"), html: IC.edit });
      ed.addEventListener('click', () => openEdit(s));
      const del = el('button', { class: 'btn btn-icon btn-ghost sv-del', html: IC.del,
        'data-tip': _t("Удалить"), 'aria-label': _t("Удалить"), 'data-tip-off': _t("Удалить"), 'data-tip-on': _t("Точно удалить?") });
      del.addEventListener('click', () => remove(s, del));
      r.append(go, ed, del);
    }
    const fill = (p) => {
      line.lastChild.replaceWith(p.online ? motd(p.motd) : el('span', { class: 'sv-motd sv-err' }, _t(p.error || "Сервер не отвечает")));
      if (p.favicon) setImg(ico, p.favicon);
      r.classList.toggle('is-off', !p.online);
      if (!p.online) { players.textContent = ''; ver.hidden = true; sig.dataset.n = '0'; sig.dataset.tip = _t("Не в сети"); return; }
      const pl = p.players || {};
      players.textContent = pl.online != null ? `${pl.online} / ${pl.max ?? '?'}` : '';
      const names = (pl.sample || []).map((n) => String(n).replace(/§./g, '')).filter((n) => n.trim());
      if (names.length) players.dataset.tip = names.join(', '); else delete players.dataset.tip;
      const v = String(p.version || '').replace(/§./g, '').trim();
      ver.hidden = !v; ver.textContent = v.length > 18 ? `${v.slice(0, 17)}…` : v; if (v) ver.dataset.tip = v;
      sig.dataset.n = String(level(p.latency_ms));
      sig.dataset.tip = _t("{0} мс", [p.latency_ms]);
    };
    return { r, fill };
  }

  // ---------- List ----------
  function row(s, n) {
    const { r, fill } = card(s, true);
    r.style.setProperty('--i', String(Math.min(n, 12)));
    pingOf(s.ip).then(fill);
    return r;
  }
  function toolbar(n) {
    const re = el('button', { class: 'btn btn-icon btn-ghost', 'data-tip': _t("Обновить"), 'aria-label': _t("Обновить"), html: IC.refresh });
    re.addEventListener('click', () => { cache.clear(); paint(true); });
    const add = el('button', { class: 'btn btn-sm sv-add', html: IC.add }, el('span', { class: 'lbl' }, _t("Добавить сервер")));
    add.addEventListener('click', () => openEdit(null));
    return el('div', { class: 'srv-bar' },
      el('span', { class: 'sv-count' }, n ? `${n} ${_tp(n, 'сервер', 'сервера', 'серверов')}` : ''),
      el('span', { class: 'spacer' }), re, add);
  }
  function empty() {
    return el('div', { class: 'table-empty tall' }, el('b', {}, _t("Серверов пока нет")),
      el('span', { class: 'hint' }, _t("Добавленный сервер появится и в игре, в списке «Сетевая игра».")));
  }
  function skel() {
    return el('div', { class: 'mod-list w-skel' }, ...[0, 1, 2].map(() => el('div', { class: 'mod-row' },
      el('i', { class: 'mod-ico sk' }), el('div', { class: 't' }, el('b', { class: 'sk' }), el('span', { class: 'sk' })))));
  }
  async function paint(force = false) {
    if (!visible() || !S.current) return;
    if (!force && shown === S.current) return;
    const id = S.current, my = ++seq;
    if (shown !== id) box.replaceChildren(skel());
    let res;
    try { res = await call('servers.list', { instance: id }); }
    catch (e) {
      if (my !== seq) return;
      shown = null;
      box.replaceChildren(el('div', { class: 'table-empty tall' }, el('b', {}, String(e))));
      return;
    }
    if (my !== seq) return;
    shown = id;
    const list = res?.servers || [];
    box.replaceChildren(toolbar(list.length), list.length ? el('div', { class: 'mod-list sv-list' }, ...list.map(row)) : empty());
  }
  $('#ip-tabs').addEventListener('click', (e) => { if (e.target.closest('[data-tab="servers"]')) paint(true); });

  // Destructive buttons ask twice: the first press arms them for a moment.
  function armed(b, on) {
    if (on) { b.dataset.armed = ''; clearTimeout(b._arm); b._arm = setTimeout(() => armed(b, false), 2600); }
    else { delete b.dataset.armed; clearTimeout(b._arm); }
    const t = on ? b.dataset.tipOn : b.dataset.tipOff;
    b.dataset.tip = t; b.setAttribute('aria-label', t);
  }
  async function remove(s, b) {
    if (b.dataset.state === 'loading') return;
    if (!('armed' in b.dataset)) { box.querySelectorAll('[data-armed]').forEach((x) => armed(x, false)); armed(b, true); return; }
    armed(b, false);
    try {
      await run(b, 'servers.delete', { instance: S.current, index: s.index, ip: s.ip });
      toast(_t("Сервер «{0}» удалён", [s.name]));
      paint(true);
    } catch { paint(true); } finally { b.dataset.state = 'idle'; }
  }

  // ---------- Add / edit dialog with a live preview ----------
  const dlg = $('#dlg-server'), name = $('#sv-name'), ip = $('#sv-ip'), pre = $('#sv-preview'), submit = $('#sv-submit');
  let preT = 0, preSeq = 0;
  function preview() {
    clearTimeout(preT);
    const a = ip.value.trim(), my = ++preSeq;
    if (!a) { pre.hidden = true; pre.replaceChildren(); return; }
    preT = setTimeout(() => {
      const { r, fill } = card({ name: name.value.trim(), ip: a }, false);
      pre.replaceChildren(r); pre.hidden = false;
      pingOf(a).then((p) => { if (my === preSeq) fill(p); });
    }, 550);
  }
  function openEdit(s) {
    editing = s;
    $('#sv-title').textContent = s ? _t("Изменить сервер") : _t("Добавить сервер");
    submit.querySelector('.lbl').textContent = s ? _t("Сохранить") : _t("Добавить");
    name.value = s?.name || ''; ip.value = s?.ip || '';
    pre.hidden = true; pre.replaceChildren();
    openDlg('dlg-server');
    if (s) preview();
  }
  ip.addEventListener('input', preview);
  name.addEventListener('input', () => { const b = pre.querySelector('.t > b'); if (b) b.textContent = name.value.trim() || _t("Сервер Minecraft"); });
  dlg.addEventListener('submit', async (ev) => {
    ev.preventDefault();
    if (submit.dataset.state === 'loading') return;
    const s = editing;
    try {
      const r = await run(submit, 'servers.save', { instance: S.current, index: s?.index, ip_was: s?.ip, name: name.value, ip: ip.value });
      closeDlg();
      toast(s ? _t("Сервер сохранён") : _t("Сервер «{0}» добавлен", [r?.name || ip.value.trim()]));
      paint(true);
    } catch { /* run() shows it */ } finally { submit.dataset.state = 'idle'; }
  });

  window.FServers = { paint };
})();