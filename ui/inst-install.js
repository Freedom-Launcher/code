'use strict';
// Instance settings → «Установка» (Modrinth's Installation tab): platform, game version and
// loader version, «Проверить и восстановить», unlink from a Modrinth modpack.
(() => {
  const f = $('#dlg-inst');
  if (!f) return;
  const pick = $('#isx-loader'), ver = $('#isx-ver'), lver = $('#isx-lver'), apply = $('#isx-apply'), chg = $('#isx-chg');
  const NAMES = { vanilla: 'Ванилла', fabric: 'Fabric', forge: 'Forge', neoforge: 'NeoForge', quilt: 'Quilt' };
  const st = { inst: null, loader: 'vanilla', versions: null, seq: 0 };
  const opt = (v, t = v) => el('option', { value: v }, t);
  const label = (l, v, lv) => [NAMES[l] || l, v, l === 'vanilla' ? '' : lv].filter(Boolean).join(' ');

  async function versions() {
    if (!st.versions) st.versions = ((await call('versions.list', { snapshots: false }).catch(() => null))?.versions || []).map((v) => v.id);
    return st.versions;
  }
  function changed() {
    const i = st.inst;
    if (!i) return;
    const vanilla = st.loader === 'vanilla';
    const same = i.version === ver.value && (i.loader || 'vanilla') === st.loader && (vanilla || lver.value === (i.loader_version || ''));
    apply.disabled = same || !!i.pack || (!vanilla && !lver.value);
    chg.classList.toggle('is-on', !same);
    chg.replaceChildren(...(same ? [] : [label(i.loader || 'vanilla', i.version, i.loader_version), ' → ', el('b', {}, label(st.loader, ver.value, lver.value))]));
  }
  async function loaderVersions() {
    const seq = ++st.seq, i = st.inst;
    const vanilla = st.loader === 'vanilla';
    $('#isx-lver-row').hidden = vanilla;
    if (vanilla) { lver.replaceChildren(); changed(); return; }
    $('#isx-lver-l').textContent = `Версия ${NAMES[st.loader]}`;
    lver.disabled = true;
    lver.replaceChildren(opt('', 'Загрузка…'));
    const list = await call('loaders.versions', { loader: st.loader, game: ver.value }).catch(() => []);
    if (seq !== st.seq) return;
    lver.replaceChildren(...(list.length ? list.map((x) => opt(x.version, x.stable ? x.version : `${x.version} · beta`)) : [opt('', 'Нет для этой версии')]));
    const keep = i.loader === st.loader && i.version === ver.value && list.some((x) => x.version === i.loader_version);
    lver.value = keep ? i.loader_version : (list.find((x) => x.stable) || list[0])?.version || '';
    lver.disabled = !list.length || !!i.pack;
    changed();
  }
  function setLoader(l) {
    st.loader = l;
    $$('button', pick).forEach((b) => b.setAttribute('aria-pressed', String(b.dataset.loader === l)));
    loaderVersions();
  }
  pick.addEventListener('click', (e) => { const b = e.target.closest('button[data-loader]'); if (b && !b.disabled) setLoader(b.dataset.loader); });
  ver.addEventListener('change', loaderVersions);
  lver.addEventListener('change', changed);

  apply.addEventListener('click', async () => {
    const i = st.inst;
    if (isRunning(i.id)) { toast('Сначала закройте игру'); return; }
    try {
      await run(apply, 'instances.set_game', { instance: i.id, version: ver.value, loader: st.loader, loader_version: st.loader === 'vanilla' ? null : lver.value || null });
      await reloadInstances();
      setTimeout(() => { closeDlg(); setTimeout(() => play(i.id, false), 120); }, 260);
    } catch { /* toast shown */ }
  });

  const unlink = $('#isx-unlink');
  let armed = 0;
  unlink.addEventListener('click', async () => {
    const lbl = $('.lbl', unlink);
    if (!armed) {
      lbl.textContent = 'Точно отвязать?';
      unlink.classList.add('is-danger');
      armed = setTimeout(() => { armed = 0; lbl.textContent = 'Отвязать'; unlink.classList.remove('is-danger'); }, 2600);
      return;
    }
    clearTimeout(armed); armed = 0;
    try {
      await run(unlink, 'instances.unlink', { instance: st.inst.id });
      await reloadInstances();
      lbl.textContent = 'Отвязать';
      unlink.classList.remove('is-danger');
      open(byId(st.inst.id));
    } catch { /* toast shown */ }
  });

  async function open(i) {
    if (!i) return;
    st.inst = i;
    const linked = !!i.pack;
    $('#isx-pack-block').hidden = !linked;
    if (linked) $('#isx-pack').textContent = `Версия ${i.pack.version_number} · версия игры и загрузчик меняются вместе со сборкой`;
    $('#isx-game').classList.toggle('is-locked', linked);
    $$('button', pick).forEach((b) => { b.disabled = linked; });
    ver.disabled = linked;
    const vs = await versions();
    ver.replaceChildren(...(vs.includes(i.version) ? vs : [i.version, ...vs]).map((v) => opt(v)));
    ver.value = i.version;
    setLoader(i.loader || 'vanilla');
  }
  f.addEventListener('isx:open', (e) => open(e.detail));
})();