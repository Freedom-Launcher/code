'use strict';
// Settings: grouped sections in a side nav (as in Modrinth App), search across every row,
// and an index of single settings for Ctrl K. Panes render lazily (stats, keys, about).
(() => {
  const nav = $('#set-seg');
  if (!nav) return;
  const main = $('#set-main'), q = $('#set-q'), ink = $('.set-ink', nav), none = $('#set-none'), wrap = nav.closest('.set-wrap');
  const ALIAS = { launcher: 'look' };
  const MAC = /Mac|iPhone|iPad/.test(navigator.platform);
  const MOD = MAC ? '⌘' : 'Ctrl';
  const btns = () => $$('button[data-pane]', nav);
  const panes = () => $$('.set-pane', main);
  const paneOf = (p) => $(`.set-pane[data-pane="${p}"]`, main);
  const cur = () => $('button[aria-selected="true"]', nav)?.dataset.pane || 'look';
  const norm = (s) => s.toLowerCase().replace(/ё/g, 'е');
  const plural = (n, a, b, c) => _tp(n, a, b, c);

  // Words people search for that are not on the row itself.
  const KW = {
    'logo-fg': 'логотип лого знак цвет logo mark color', 'logo-bg': 'логотип подложка фон плитка logo background', 'logo-shape': 'логотип форма круг квадрат logo shape', 'logo-fade': 'логотип перья затухание logo feathers',
    'theme-seg': 'цвет темная светлая theme dark light', 'theme-folder': 'темы папка', 'panels-seg': 'отступы карточки раскладка',
    'fx-lite': 'облегчённые эффекты производительность лаги тормоза плавность размытие fps performance lite', 'diag-btn': 'диагностика логи отчёт ошибка вылет краш crash logs',
    'fx-wide': 'ширина широкий', 'cover-seg': 'шапка баннер hero', 'fx-lang': 'язык language lang перевод translation мова sprache idioma langue język dil', 'fx-tray': 'трей tray закрытие крестик свернуть окно', 'fx-motion': 'анимация движение motion плавность',
    'pop-alpha': 'меню списки выпадающие прозрачность непрозрачность читаемость', 'pop-blur': 'меню списки выпадающие размытие blur', 'plate-size': 'название плашка карточки размер шрифта', 'plate-weight': 'название плашка жирный насыщенность', 'plate-meta': 'подпись версия загрузчик карточки', 'wall-seg': 'фон картинка wallpaper размытие blur стекло прозрачность', 'wall-blur': 'размытие blur', 'wall-zoom': 'обои масштаб зум увеличение картинка фон', 'wall-x': 'обои сдвиг положение горизонталь фон', 'wall-y': 'обои сдвиг положение вертикаль фон', 'glass-blur': 'размытие blur', 'wall-glass': 'прозрачность непрозрачность',
    'ui-edit-open': 'редактор скругление размер кнопок расположение', 'pal-pos-seg': 'ctrl k палитра поиск команды', 'pal-blur': 'палитра',
    'browse-seg': 'страницы лента прокрутка каталог', 'gs-root-pick': 'путь папка диск сборки', 'upd-btn': 'обновить версия update', 'addon-folder': 'аддоны',
  };
  function hay(r) {
    if (r._hay) return r._hay;
    const p = r.closest('.set-pane'), rows = r.closest('.rows'), lab = rows?.previousElementSibling;
    const ids = $$('[id]', r).map((n) => KW[n.id] || '').join(' ');
    return (r._hay = norm([$('.set-h', p)?.textContent, lab?.matches('.set-label, .gs-h') ? lab.textContent : '', r.textContent, ids].join(' ')));
  }

  // ---------- Navigation ----------
  function placeInk(instant) {
    const b = $('button[aria-selected="true"]', nav);
    if (!b || !b.offsetHeight) return;
    if (instant) ink.style.transition = 'none';
    ink.style.setProperty('--y', `${b.offsetTop}px`);
    ink.style.setProperty('--h', `${b.offsetHeight}px`);
    if (instant) requestAnimationFrame(() => { ink.style.transition = ''; });
  }
  const placeSegs = (root) => requestAnimationFrame(() => $$('.seg', root).forEach((s) => s._place?.(true)));
  function reveal(p) {
    panes().forEach((x) => { x.hidden = x.dataset.pane !== p; });
    const pane = paneOf(p);
    if (!pane) return;
    RENDER[p]?.();
    placeSegs(pane);
    pane.classList.remove('is-enter'); void pane.offsetWidth; pane.classList.add('is-enter');
  }
  function select(p, quiet) {
    p = ALIAS[p] || p;
    const b = $(`button[data-pane="${p}"]`, nav);
    if (!b) return;
    if (q.value) { q.value = ''; clearSearch(); }
    btns().forEach((x) => x.setAttribute('aria-selected', String(x === b)));
    placeInk(false);
    reveal(p);
    const v = $('#view-settings'); if (v) v.scrollTop = 0;
    if (!quiet) nav.dispatchEvent(new CustomEvent('seg:change', { bubbles: true, detail: b }));
  }
  nav.addEventListener('click', (e) => { const b = e.target.closest('button[data-pane]'); if (b) select(b.dataset.pane); });
  nav.addEventListener('keydown', (e) => {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    const bs = btns(), i = bs.indexOf(document.activeElement);
    if (i < 0) return;
    e.preventDefault();
    const n = bs[(i + (e.key === 'ArrowDown' ? 1 : bs.length - 1)) % bs.length];
    n.focus(); select(n.dataset.pane);
  });
  if (window.ResizeObserver) new ResizeObserver(() => placeInk(true)).observe(nav);

  /** Opens a pane and, optionally, points at one row. */
  function open(p, row) {
    if (S.view !== 'settings') show('settings');
    select(p);
    const r = typeof row === 'string' ? $(row, main) : row;
    if (!r) return;
    requestAnimationFrame(() => {
      r.scrollIntoView({ block: 'center', behavior: 'smooth' });
      r.classList.remove('is-flash'); void r.offsetWidth; r.classList.add('is-flash');
      $('input, button, select', r)?.focus({ preventScroll: true });
    });
  }

  // ---------- Search ----------
  function clearSearch() {
    $$('[data-miss]', main).forEach((n) => n.removeAttribute('data-miss'));
    wrap.removeAttribute('data-search');
    none.hidden = true;
    reveal(cur());
    placeInk(true);
  }
  function filter() {
    const s = norm(q.value.trim());
    if (!s) { clearSearch(); return; }
    wrap.setAttribute('data-search', '');
    renderKeys(); renderAbout();
    const words = s.split(/\s+/);
    let any = false;
    panes().forEach((p) => {
      let hit = 0;
      if (p.dataset.pane !== 'stats') $$('.rows', p).forEach((rows) => {
        let h = 0;
        $$(':scope > *', rows).forEach((r) => {
          const ok = words.every((w) => hay(r).includes(w));
          r.toggleAttribute('data-miss', !ok);
          if (ok && !r.hidden) h++;
        });
        rows.toggleAttribute('data-miss', !h);
        const lab = rows.previousElementSibling;
        if (lab?.matches('.set-label, .gs-h')) lab.toggleAttribute('data-miss', !h);
        hit += h;
      });
      p.hidden = !hit;
      if (hit) { any = true; placeSegs(p); }
    });
    none.hidden = any;
  }
  q.addEventListener('input', filter);
  q.addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && q.value) { e.stopPropagation(); q.value = ''; clearSearch(); }
    if (e.key === 'Enter') { const r = $$('.set-pane:not([hidden]) .rows > :not([data-miss]):not([hidden])', main)[0]; if (r) { const p = r.closest('.set-pane').dataset.pane; q.value = ''; open(p, r); } }
  });
  document.addEventListener('keydown', (e) => {
    if (e.key !== '/' || S.view !== 'settings' || e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.target.matches('input, select, textarea, [contenteditable]') || document.querySelector('.dialog[data-open]')) return;
    e.preventDefault(); q.focus(); q.select();
  });

  /** Every visible setting row, for the Ctrl K palette. */
  function entries() {
    const out = [];
    panes().forEach((p) => {
      const pn = p.dataset.pane;
      if (pn === 'stats' || pn === 'keys') return;
      const title = $('.set-h', p)?.textContent.trim() || '';
      $$('.rows > .r', p).forEach((r) => {
        if (r.hidden) return;
        const t = $('.t', r)?.textContent.trim();
        if (!t) return;
        out.push({ id: `set.${pn}.${t}`, t, s: title, k: hay(r), go: () => open(pn, r) });
      });
    });
    return out;
  }

  // ---------- Statistics ----------
  function renderStats() {
    const box = $('#set-stats');
    const list = [...(S.instances || [])];
    const now = Date.now() / 1000;
    const total = list.reduce((a, i) => a + (i.playtime_secs || 0), 0);
    const played = list.filter((i) => i.playtime_secs > 0).sort((a, b) => b.playtime_secs - a.playtime_secs);
    const week = list.filter((i) => i.last_played && now - i.last_played < 7 * 86400).length;
    const tile = (v, t) => el('div', { class: 'set-tile' }, el('b', {}, v), el('span', {}, t));
    const tiles = el('div', { class: 'set-tiles' },
      tile(total ? fmtPlay(total) : _t("0 мин"), _t("в игре всего")),
      tile(String(list.length), plural(list.length, 'сборка', 'сборки', 'сборок')),
      tile(String(week), _t("запускались за неделю")));
    const max = played[0]?.playtime_secs || 1;
    const row = (i) => el('button', { type: 'button', class: 'r set-top', onclick: () => openInstance(i.id) },
      el('span', { class: 'set-top-ic' }, initials(i.name), el('img', { src: coverSrc(iconName(i)), alt: '', loading: 'lazy', onerror() { this.remove(); } })),
      el('div', { class: 'grow' }, el('div', { class: 't' }, i.name), el('div', { class: 'set-bar' }, el('i', { style: `width:${Math.max(3, (i.playtime_secs / max) * 100).toFixed(1)}%` }))),
      el('span', { class: 'set-top-v' }, fmtPlay(i.playtime_secs)));
    box.replaceChildren(tiles, ...(played.length
      ? [el('div', { class: 'set-label' }, _t("Больше всего")), el('div', { class: 'rows set-card' }, ...played.slice(0, 8).map(row))]
      : [el('div', { class: 'set-empty' }, _t("Ещё не играли"))]));
  }

  // ---------- Shortcuts ----------
  const KEYS = [
    [_t("Везде"), [[[MOD, 'K'], _t("Командная строка")], [[MOD, 'N'], _t("Новая сборка")], [[MOD, 'Shift', 'E'], _t("Редактор интерфейса")], [['Alt', '←'], _t("Назад")], [['Alt', '→'], _t("Вперёд")], [['Esc'], _t("Закрыть окно или меню")]]],
    [_t("Сборка"), [[['Enter'], _t("Играть")], [['Shift', _t("Удалить")], _t("Удалить без подтверждения")], [['Esc'], _t("Снять выделение")]]],
    [_t("Каталог"), [[['PageUp'], _t("Предыдущая страница")], [['PageDown'], _t("Следующая страница")]]],
    [_t("Настройки"), [[['/'], _t("Поиск")]]],
    [_t("Редактор интерфейса"), [[[MOD, 'Z'], _t("Отменить")], [[MOD, 'Shift', 'Z'], _t("Повторить")]]],
  ];
  let keysDone = false;
  function renderKeys() {
    if (keysDone) return;
    keysDone = true;
    $('#set-keys').replaceChildren(...KEYS.flatMap(([g, list]) => [
      el('div', { class: 'set-label' }, g),
      el('div', { class: 'rows set-card' }, ...list.map(([ks, t]) => el('div', { class: 'r' },
        el('div', { class: 'grow' }, el('div', { class: 't' }, t)),
        el('span', { class: 'set-kbd' }, ...ks.map((k) => el('kbd', {}, k)))))),
    ]));
  }

  // ---------- About ----------
  let aboutDone = false;
  function renderAbout() {
    if (aboutDone) return;
    aboutDone = true;
    const v = $('#set-ver');
    const put = (x) => { if (x) v.textContent = _t("Версия {0}", [x]); };
    window.__TAURI__?.app?.getVersion?.().then(put, () => {});
  }
  $('#set-releases')?.addEventListener('click', () => invoke('open_link', { url: 'https://github.com/Freedom-Launcher/releases/releases' }).catch((e) => toast(String(e), { error: true })));

  const RENDER = { stats: renderStats, keys: renderKeys, about: renderAbout };

  /** Called when the settings view becomes visible. */
  function shown() { if (!wrap.hasAttribute('data-search')) reveal(cur()); placeInk(true); }

  reveal(cur());
  window.FSettings = { open, entries, shown, current: cur };
})();