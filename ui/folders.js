'use strict';
// Settings → Игра → «Папка сборок»: pick a folder (another drive is fine) and move every instance there.
(() => {
  const row = $('#gs-root-row');
  if (!row) return;
  const label = $('#gs-root'), bar = $('#gs-root-prog'), pick = $('#gs-root-pick'), reset = $('#gs-root-reset');
  const KEY = 'move:instances';
  const size = (b) => (b >= 1 << 30 ? `${(b / (1 << 30)).toFixed(1)} ГБ` : `${Math.max(1, Math.round(b / (1 << 20)))} МБ`);

  async function refresh() {
    const r = await call('instances.root').catch(() => null);
    if (!r) return;
    label.textContent = r.path;
    label.title = r.path;
    reset.hidden = !r.custom;
  }

  async function move(path) {
    const btn = path ? pick : reset;
    pick.disabled = reset.disabled = true;
    bar.hidden = false;
    bar.setAttribute('data-indeterminate', '');
    const tick = setInterval(async () => {
      const p = await call('game.progress', { instance: KEY }).catch(() => null);
      if (!p || !p.total_bytes) return;
      bar.removeAttribute('data-indeterminate');
      bar.style.setProperty('--p', String(Math.min(1, p.done_bytes / p.total_bytes)));
      label.textContent = `Перенос… ${size(p.done_bytes)} из ${size(p.total_bytes)}`;
    }, 250);
    try {
      const r = await run(btn, 'instances.move_root', { path });
      toast(r.moved ? `Сборки перенесены: ${r.moved}` : 'Папка сборок изменена');
      await window.reloadInstances?.();
    } catch { /* toast shown by run() */ }
    clearInterval(tick);
    bar.style.setProperty('--p', '1');
    setTimeout(() => { bar.hidden = true; bar.style.removeProperty('--p'); }, 320);
    pick.disabled = reset.disabled = false;
    refresh();
  }

  pick.addEventListener('click', async () => {
    const path = await invoke('pick_folder', { title: 'Папка для сборок' }).catch((e) => { toast(String(e), { error: true }); return null; });
    if (path) move(path);
  });
  reset.addEventListener('click', () => move(null));
  refresh();
})();