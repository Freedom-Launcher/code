'use strict';
// Settings → Лаунчер → «Обновления»: check the release feed, download, verify, install over itself, restart.
// A quiet check runs a few seconds after start; a found update lights the dot and shows one toast.
(() => {
  const row = $('#upd-row');
  if (!row) return;
  const sub = $('#upd-sub'), bar = $('#upd-prog'), btn = $('#upd-btn'), lbl = btn.querySelector('.lbl .t');
  const st = { found: null, current: '' };
  const mb = (b) => `${(b / (1 << 20)).toFixed(1)} МБ`;

  function show() {
    row.dataset.state = st.found ? 'available' : 'idle';
    sub.textContent = st.found ? `Доступна ${st.found.version} · у вас ${st.current}` : (st.current ? `Версия ${st.current}` : `Freedom Launcher`);
    FUI.swapText ? FUI.swapText(lbl, st.found ? 'Обновить' : 'Проверить') : (lbl.textContent = st.found ? 'Обновить' : 'Проверить');
  }

  async function check(quiet) {
    try {
      const r = await invoke('update_check');
      st.current = r.current || st.current;
      st.found = r.available ? r : null;
      show();
      if (st.found && quiet) toast(`Доступна версия ${st.found.version} — Настройки → О лаунчере`);
      if (!st.found && !quiet) toast('У вас последняя версия');
    } catch (e) {
      if (!quiet) throw e;
    }
  }

  async function install() {
    btn.disabled = true;
    bar.hidden = false;
    bar.setAttribute('data-indeterminate', '');
    const tick = setInterval(async () => {
      const p = await invoke('update_progress').catch(() => null);
      if (!p || !p.total) return;
      bar.removeAttribute('data-indeterminate');
      bar.style.setProperty('--p', String(Math.min(1, p.done / p.total)));
      sub.textContent = `Загрузка ${st.found.version}… ${mb(p.done)} из ${mb(p.total)}`;
    }, 200);
    try {
      await invoke('update_install'); // restarts the app on success
    } catch (e) {
      toast(String(e), { error: true });
      st.found = null;
      show();
    }
    clearInterval(tick);
    bar.hidden = true;
    btn.disabled = false;
  }

  btn.addEventListener('click', async () => {
    if (st.found) return install();
    btn.dataset.state = 'loading';
    try {
      await check(false);
      btn.dataset.state = 'success';
    } catch (e) {
      btn.dataset.state = 'error';
      toast(String(e), { error: true });
    }
    setTimeout(() => { btn.dataset.state = 'idle'; }, 1100);
  });

  call('app.info').then((i) => { st.current = i.version; show(); }).catch(() => {});
  setTimeout(() => check(true), 4000);
})();