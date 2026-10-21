'use strict';
// Pack files whose authors allow downloads only from curseforge.com
// (`modpacks.blocked`). The user opens each file's page in the browser; the core
// picks the downloads up from the Downloads folder by SHA-1 (`modpacks.resolve_blocked`).
(() => {
  const { $, toast, openDlg, closeDlg } = window.FUI;
  const plural = (n, a, b, c) => { const m = n % 10, h = n % 100; return m === 1 && h !== 11 ? a : m >= 2 && m <= 4 && (h < 12 || h > 14) ? b : c; };
  const size = (x) => (!x ? '' : x < 1 << 20 ? `${Math.max(1, Math.round(x / 1024))} КБ` : `${(x / 1048576).toFixed(1)} МБ`);
  const dlg = $('#dlg-blocked'), list = $('#bk-list'), note = $('#bk-note');
  let inst = null, items = [], timer = 0, checking = false, folder = null;
  const counts = new Map();

  function row(b) {
    const open = el('button', { type: 'button', class: 'btn btn-ghost bk-open', 'data-id': `blocked.open.${b.file_name}` }, el('span', { class: 'lbl' }, 'Скачать'));
    open.addEventListener('click', () => invoke('open_link', { url: b.url }).catch((e) => toast(String(e), { error: true })));
    return el('div', { class: 'fg-row bk-row' },
      el('span', { class: 'fg-ico', 'aria-hidden': 'true' }, (b.title.trim()[0] || '?').toUpperCase()),
      el('span', { class: 'fg-t' }, el('b', {}, b.title), el('span', {}, [b.file_name, size(b.size)].filter(Boolean).join(' · '))),
      open);
  }
  function paint() {
    list.replaceChildren(...items.map(row));
    const n = items.length;
    note.textContent = n
      ? `Авторы ${n === 1 ? 'этого файла' : 'этих файлов'} разрешают скачивать только с сайта CurseForge. Нажмите «Скачать» у каждого — лаунчер сам заберёт ${n === 1 ? 'файл' : 'файлы'} из папки «Загрузки» и положит в сборку.`
      : 'Все файлы на месте — сборка готова.';
    $('#bk-left').textContent = n ? `Осталось ${n} ${plural(n, 'файл', 'файла', 'файлов')}` : '';
  }
  function setCount(id, n) {
    counts.set(id, n);
    window.FBlocked.paintButton();
  }

  async function check(quiet) {
    if (!inst || checking) return;
    checking = true;
    try {
      const r = await call('modpacks.resolve_blocked', { instance: inst, ...(folder ? { folder } : {}) });
      items = r.left || [];
      setCount(inst, items.length);
      if (r.added?.length) toast(`Добавлено в сборку: ${r.added.join(', ')}`);
      else if (!quiet && items.length) toast('В «Загрузках» пока нет нужных файлов');
      paint();
      if (!items.length) { stop(); window.FMods?.instance(byId(inst)); }
    } catch (e) { if (!quiet) toast(String(e), { error: true }); }
    finally { checking = false; }
  }
  function stop() { clearInterval(timer); timer = 0; }

  function open(id, files) {
    inst = id; items = files || []; folder = null;
    setCount(id, items.length);
    if (!items.length) return;
    paint();
    openDlg('dlg-blocked');
    stop();
    timer = setInterval(() => { if (dlg.hasAttribute('data-open')) check(true); else stop(); }, 3000);
  }
  async function reopen(id) {
    const files = await call('modpacks.blocked', { instance: id }).catch(() => []);
    if (files.length) open(id, files); else setCount(id, 0);
  }
  // After an install or update: show the dialog when something is missing.
  function after(created) {
    if (created?.id && created.blocked_files?.length) open(created.id, created.blocked_files);
  }

  $('#bk-check').addEventListener('click', () => check(false));
  $('#bk-pick').addEventListener('click', async () => {
    const p = await invoke('pick_folder', { title: 'Папка со скачанными файлами' }).catch(() => null);
    if (p) { folder = p; check(false); }
  });
  dlg.addEventListener('submit', (e) => { e.preventDefault(); stop(); closeDlg(); });
  $('#ip-blocked')?.addEventListener('click', () => S.current && reopen(S.current));

  window.FBlocked = {
    open, reopen, after,
    // Instance page: a button while files are missing.
    async instance(i) {
      if (!i?.pack) { window.FBlocked.paintButton(); return; }
      const files = await call('modpacks.blocked', { instance: i.id }).catch(() => []);
      setCount(i.id, files.length);
    },
    paintButton() {
      const b = $('#ip-blocked');
      if (!b) return;
      const n = (S.current && counts.get(S.current)) || 0;
      b.hidden = !n;
      if (n) b.querySelector('.t').textContent = `Докачать ${n}`;
    },
  };
})();