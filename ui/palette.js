'use strict';
// Command palette (Ctrl K): one bar to reach every instance, screen and setting.
// Runs on the same globals as the built-in screens (show, play, openInstance…), never on private state.
(() => {
  const { $, $$, toast } = window.FUI;
  const root = $('#palette'), input = $('#pal-q'), body = $('#pal-body'), list = $('#pal-list'), hlEl = $('#pal-hl');
  if (!root) return;
  const MAC = /Mac|iPhone|iPad/.test(navigator.platform);
  const MOD = MAC ? '⌘' : 'Ctrl';
  const P = {
    home: '<path d="M216,120v96H152V152H104v64H40V120a8,8,0,0,1,2.34-5.66l80-80a8,8,0,0,1,11.32,0l80,80A8,8,0,0,1,216,120Z" class="duo"/><path d="M222.14,105.85l-80-80a20,20,0,0,0-28.28,0l-80,80A19.86,19.86,0,0,0,28,120v96a12,12,0,0,0,12,12h64a12,12,0,0,0,12-12V164h24v52a12,12,0,0,0,12,12h64a12,12,0,0,0,12-12V120A19.86,19.86,0,0,0,222.14,105.85ZM204,204H164V152a12,12,0,0,0-12-12H104a12,12,0,0,0-12,12v52H52V121.65l76-76,76,76Z"/>', plus: '<path d="M228,128a12,12,0,0,1-12,12H140v76a12,12,0,0,1-24,0V140H40a12,12,0,0,1,0-24h76V40a12,12,0,0,1,24,0v76h76A12,12,0,0,1,228,128Z"/>',
    compass: '<path d="M128,32a96,96,0,1,0,96,96A96,96,0,0,0,128,32Zm16,112L80,176l32-64,64-32Z" class="duo"/><path d="M128,20A108,108,0,1,0,236,128,108.12,108.12,0,0,0,128,20Zm0,192a84,84,0,1,1,84-84A84.09,84.09,0,0,1,128,212ZM163.27,77l-56,24a12,12,0,0,0-6.3,6.3l-24,56A12,12,0,0,0,92.73,179l56-24a12,12,0,0,0,6.3-6.3l24-56A12,12,0,0,0,163.27,77Zm-28.41,57.89-24,10.29,10.29-24,24-10.29Z"/>',
    gear: '<path d="M230.1,108.76,198.25,90.62c-.64-1.16-1.31-2.29-2-3.41l-.12-36A104.61,104.61,0,0,0,162,32L130,49.89c-1.34,0-2.69,0-4,0L94,32A104.58,104.58,0,0,0,59.89,51.25l-.16,36c-.7,1.12-1.37,2.26-2,3.41l-31.84,18.1a99.15,99.15,0,0,0,0,38.46l31.85,18.14c.64,1.16,1.31,2.29,2,3.41l.12,36A104.61,104.61,0,0,0,94,224l32-17.87c1.34,0,2.69,0,4,0L162,224a104.58,104.58,0,0,0,34.08-19.25l.16-36c.7-1.12,1.37-2.26,2-3.41l31.84-18.1A99.15,99.15,0,0,0,230.1,108.76ZM128,168a40,40,0,1,1,40-40A40,40,0,0,1,128,168Z" class="duo"/><path d="M128,76a52,52,0,1,0,52,52A52.06,52.06,0,0,0,128,76Zm0,80a28,28,0,1,1,28-28A28,28,0,0,1,128,156Zm113.86-49.57A12,12,0,0,0,236,98.34L208.21,82.49l-.11-31.31a12,12,0,0,0-4.25-9.12,116,116,0,0,0-38-21.41,12,12,0,0,0-9.68.89L128,37.27,99.83,21.53a12,12,0,0,0-9.7-.9,116.06,116.06,0,0,0-38,21.47,12,12,0,0,0-4.24,9.1l-.14,31.34L20,98.35a12,12,0,0,0-5.85,8.11,110.7,110.7,0,0,0,0,43.11A12,12,0,0,0,20,157.66l27.82,15.85.11,31.31a12,12,0,0,0,4.25,9.12,116,116,0,0,0,38,21.41,12,12,0,0,0,9.68-.89L128,218.73l28.14,15.74a12,12,0,0,0,9.7.9,116.06,116.06,0,0,0,38-21.47,12,12,0,0,0,4.24-9.1l.14-31.34,27.81-15.81a12,12,0,0,0,5.85-8.11A110.7,110.7,0,0,0,241.86,106.43Zm-22.63,33.18-26.88,15.28a11.94,11.94,0,0,0-4.55,4.59c-.54,1-1.11,1.93-1.7,2.88a12,12,0,0,0-1.83,6.31L184.13,199a91.83,91.83,0,0,1-21.07,11.87l-27.15-15.19a12,12,0,0,0-5.86-1.53h-.29c-1.14,0-2.3,0-3.44,0a12.08,12.08,0,0,0-6.14,1.51L93,210.82A92.27,92.27,0,0,1,71.88,199l-.11-30.24a12,12,0,0,0-1.83-6.32c-.58-.94-1.16-1.91-1.7-2.88A11.92,11.92,0,0,0,63.7,155L36.8,139.63a86.53,86.53,0,0,1,0-23.24l26.88-15.28a12,12,0,0,0,4.55-4.58c.54-1,1.11-1.94,1.7-2.89a12,12,0,0,0,1.83-6.31L71.87,57A91.83,91.83,0,0,1,92.94,45.17l27.15,15.19a11.92,11.92,0,0,0,6.15,1.52c1.14,0,2.3,0,3.44,0a12.08,12.08,0,0,0,6.14-1.51L163,45.18A92.27,92.27,0,0,1,184.12,57l.11,30.24a12,12,0,0,0,1.83,6.32c.58.94,1.16,1.91,1.7,2.88A11.92,11.92,0,0,0,192.3,101l26.9,15.33A86.53,86.53,0,0,1,219.23,139.61Z"/>',
    user: '<path d="M192,96a64,64,0,1,1-64-64A64,64,0,0,1,192,96Z" class="duo"/><path d="M234.38,210a123.36,123.36,0,0,0-60.78-53.23,76,76,0,1,0-91.2,0A123.36,123.36,0,0,0,21.62,210a12,12,0,1,0,20.77,12c18.12-31.32,50.12-50,85.61-50s67.49,18.69,85.61,50a12,12,0,0,0,20.77-12ZM76,96a52,52,0,1,1,52,52A52.06,52.06,0,0,1,76,96Z"/>',
    import: '<path d="M208,28H48A20,20,0,0,0,28,48V208a20,20,0,0,0,20,20H208a20,20,0,0,0,20-20V48A20,20,0,0,0,208,28Zm-4,24v92H179.31a19.86,19.86,0,0,0-14.14,5.86L147,168H109L90.83,149.86A19.86,19.86,0,0,0,76.69,144H52V52ZM52,204V168H75l18.14,18.14A19.86,19.86,0,0,0,107.31,192h41.38a19.86,19.86,0,0,0,14.14-5.86L181,168h23v36Zm35.51-87.51a12,12,0,0,1,17-17L116,111V76a12,12,0,0,1,24,0v35l11.51-11.52a12,12,0,0,1,17,17l-32,32a12,12,0,0,1-17,0Z"/>', folder: '<path d="M208,88v24H69.77a8,8,0,0,0-7.59,5.47L32,208V64a8,8,0,0,1,8-8H93.33a8,8,0,0,1,4.8,1.6L128,80h72A8,8,0,0,1,208,88Z" class="duo"/><path d="M248.23,112.31A20,20,0,0,0,232,104H220V88a20,20,0,0,0-20-20H132L105.34,48a20.12,20.12,0,0,0-12-4H40A20,20,0,0,0,20,64V208a12,12,0,0,0,12,12H211.1a12,12,0,0,0,11.33-8l28.49-81.47.06-.17A20,20,0,0,0,248.23,112.31ZM92,68l28.8,21.6A12,12,0,0,0,128,92h68v12H69.77a20,20,0,0,0-18.94,13.58L44,137.15V68ZM202.59,196H48.89l23.72-68H226.37Z"/>',
    moon: '<path d="M227.89,147.89A96,96,0,1,1,108.11,28.11,96.09,96.09,0,0,0,227.89,147.89Z" class="duo"/><path d="M236.37,139.4a12,12,0,0,0-12-3A84.07,84.07,0,0,1,119.6,31.59a12,12,0,0,0-15-15A108.86,108.86,0,0,0,49.69,55.07,108,108,0,0,0,136,228a107.09,107.09,0,0,0,64.93-21.69,108.86,108.86,0,0,0,38.44-54.94A12,12,0,0,0,236.37,139.4Zm-49.88,47.74A84,84,0,0,1,68.86,69.51,84.93,84.93,0,0,1,92.27,48.29Q92,52.13,92,56A108.12,108.12,0,0,0,200,164q3.87,0,7.71-.27A84.79,84.79,0,0,1,186.49,187.14Z"/>',
    sun: '<path d="M184,128a56,56,0,1,1-56-56A56,56,0,0,1,184,128Z" class="duo"/><path d="M116,36V20a12,12,0,0,1,24,0V36a12,12,0,0,1-24,0Zm80,92a68,68,0,1,1-68-68A68.07,68.07,0,0,1,196,128Zm-24,0a44,44,0,1,0-44,44A44.05,44.05,0,0,0,172,128ZM51.51,68.49a12,12,0,1,0,17-17l-12-12a12,12,0,0,0-17,17Zm0,119-12,12a12,12,0,0,0,17,17l12-12a12,12,0,1,0-17-17ZM196,72a12,12,0,0,0,8.49-3.51l12-12a12,12,0,0,0-17-17l-12,12A12,12,0,0,0,196,72Zm8.49,115.51a12,12,0,0,0-17,17l12,12a12,12,0,0,0,17-17ZM48,128a12,12,0,0,0-12-12H20a12,12,0,0,0,0,24H36A12,12,0,0,0,48,128Zm80,80a12,12,0,0,0-12,12v16a12,12,0,0,0,24,0V220A12,12,0,0,0,128,208Zm108-92H220a12,12,0,0,0,0,24h16a12,12,0,0,0,0-24Z"/>',
    image: '<path d="M224,56V178.06l-39.72-39.72a8,8,0,0,0-11.31,0L147.31,164,97.66,114.34a8,8,0,0,0-11.32,0L32,168.69V56a8,8,0,0,1,8-8H216A8,8,0,0,1,224,56Z" class="duo"/><path d="M144,96a16,16,0,1,1,16,16A16,16,0,0,1,144,96Zm92-40V200a20,20,0,0,1-20,20H40a20,20,0,0,1-20-20V56A20,20,0,0,1,40,36H216A20,20,0,0,1,236,56ZM44,60v79.72l33.86-33.86a20,20,0,0,1,28.28,0L147.31,147l17.18-17.17a20,20,0,0,1,28.28,0L212,149.09V60Zm0,136H162.34L92,125.66l-48,48Zm168,0V183l-33.37-33.37L164.28,164l32,32Z"/>', motion: '<path d="M244,128a12,12,0,0,1-12,12H207.42l-36.69,73.37A12,12,0,0,1,160,220h-.6a12,12,0,0,1-10.61-7.72L95,71.15,66.92,133A12,12,0,0,1,56,140H24a12,12,0,0,1,0-24H48.27L85.08,35a12,12,0,0,1,22.13.7l54.28,142.46,27.78-55.56A12,12,0,0,1,200,116h32A12,12,0,0,1,244,128Z"/>',
    stop: '<path d="M208,56V200a8,8,0,0,1-8,8H56a8,8,0,0,1-8-8V56a8,8,0,0,1,8-8H200A8,8,0,0,1,208,56Z" class="duo"/><path d="M200,36H56A20,20,0,0,0,36,56V200a20,20,0,0,0,20,20H200a20,20,0,0,0,20-20V56A20,20,0,0,0,200,36Zm-4,160H60V60H196Z"/>', search: '<path d="M192,112a80,80,0,1,1-80-80A80,80,0,0,1,192,112Z" class="duo"/><path d="M232.49,215.51,185,168a92.12,92.12,0,1,0-17,17l47.53,47.54a12,12,0,0,0,17-17ZM44,112a68,68,0,1,1,68,68A68.07,68.07,0,0,1,44,112Z"/>', bar: '<path d="M28,64A12,12,0,0,1,40,52H216a12,12,0,0,1,0,24H40A12,12,0,0,1,28,64ZM64,92a12,12,0,0,0,0,24H192a12,12,0,0,0,0-24Zm152,40H40a12,12,0,0,0,0,24H216a12,12,0,0,0,0-24Zm-24,40H64a12,12,0,0,0,0,24H192a12,12,0,0,0,0-24Z"/>',
  };
  const svg = (k) => `<svg class="pi" viewBox="0 0 256 256" fill="currentColor" aria-hidden="true">${P[k]}</svg>`;
  const GROUP = { recent: 'Недавние', inst: 'Сборки', go: 'Перейти', act: 'Действия', mods: 'Modrinth' };
  const HINT = { inst: 'Играть', go: 'Перейти', act: 'Выполнить', mods: 'Искать' };

  // ---------- Settings: bar position + backdrop blur ----------
  const pos = () => S.look.palette_pos;
  const blurOn = () => S.look.palette_blur;
  function applyPal() {
    const r = document.documentElement;
    r.dataset.palettePos = pos();
    r.dataset.paletteBlur = blurOn() ? 'on' : 'off';
    $$('#pal-pos-seg button').forEach((b) => b.setAttribute('aria-selected', String(b.dataset.palPos === pos())));
    $('#pal-pos-seg')?._place?.(false);
    if ($('#pal-blur')) $('#pal-blur').checked = blurOn();
  }
  $('#pal-pos-seg')?.addEventListener('seg:change', (e) => { setLook({ palette_pos: e.detail.dataset.palPos }); applyPal(); });
  $('#pal-blur')?.addEventListener('change', (e) => { setLook({ palette_blur: e.target.checked }); applyPal(); });
  document.addEventListener('look:change', applyPal);
  // The settings pane is hidden until opened; place the pill once it gets a size.
  if (window.ResizeObserver && $('#pal-pos-seg')) new ResizeObserver(() => $('#pal-pos-seg')._place?.(true)).observe($('#pal-pos-seg'));
  $$('[data-pal-kbd]').forEach((k) => { k.textContent = `${MOD} K`; });
  applyPal();

  // ---------- Commands ----------
  const openSettings = (p) => window.FSettings?.open(p);
  const folder = (cmd, args) => invoke(cmd, args).catch((e) => toast(String(e), { error: true }));
  const label = (b) => b.textContent.trim();
  function commands() {
    const out = [];
    const add = (g, id, t, ico, run, extra = {}) => out.push({ g, id, t, ico, run, ...extra });
    recent().forEach((i) => {
      const on = isRunning(i.id);
      add('inst', `inst.${i.id}`, i.name, null, (o) => (o.alt || on ? openInstance(i.id) : play(i.id, true)),
        { s: instMeta(i), k: `${i.loader || 'vanilla'} ${i.version} сборка`, cover: iconName(i), live: on, hint: on ? 'Открыть' : 'Играть', alt: !on && 'Открыть' });
    });
    add('go', 'go.home', 'Библиотека', 'home', () => show('home'), { k: 'библиотека главная домой сборки library home' });
    add('go', 'go.create', 'Новая сборка', 'plus', () => show('create'), { k: 'создать new instance', keys: [MOD, 'N'] });
    add('go', 'go.skins', 'Скины', 'user', () => show('skins'), { k: 'скины скин плащ модель рук skin skins cape' });
    add('go', 'go.discover', 'Каталог', 'compass', () => show('discover'), { k: 'каталог обзор моды сборки модпаки ресурспаки шейдеры датапаки modrinth browse mods modpacks' });
    add('go', 'go.accounts', 'Аккаунты', 'user', () => show('accounts'), { k: 'профиль вход microsoft account login' });
    [['look', 'Внешний вид', 'тема оформление раскладка анимации theme'], ['wall', 'Обои и стекло', 'фон картинка прозрачность wallpaper glass'],
      ['ui', 'Интерфейс', 'редактор командная строка каталог'], ['game', 'Параметры сборок', 'java память ram окно хуки папка'],
      ['stats', 'Статистика', 'время в игре часы playtime stats'], ['addons', 'Плагины', 'плагины аддоны дополнения addons plugins'],
      ['keys', 'Клавиши', 'горячие клавиши сочетания shortcuts hotkeys'], ['about', 'О лаунчере', 'версия обновления update about']]
      .forEach(([p, t, k]) => add('go', `set.${p}`, `Настройки · ${t}`, 'gear', () => openSettings(p), { k: `settings настройки ${k}` }));
    (window.FSettings?.entries() || []).forEach((x) => add('go', x.id, x.t, 'gear', x.go, { s: `Настройки · ${x.s}`, k: `settings настройки ${x.k}`, deep: true }));
    add('act', 'act.ui-edit', 'Настроить интерфейс', 'gear', () => window.FEditor?.open(), { k: 'редактор интерфейса вид углы скругление размер кнопок расположение панели customize editor layout', keys: [MOD, 'Shift', 'E'] });
    add('act', 'act.foreign', 'Импорт из других лаунчеров', 'import', () => window.FForeign?.open(), { k: 'prism multimc curseforge atlauncher gdlauncher modrinth app перенос импорт import launcher' });
    add('act', 'act.import', 'Импорт .mrpack', 'import', () => { show('create'); $('#c-import')?.click(); }, { k: 'modpack модпак import файл' });
    S.running.forEach((id) => { const i = byId(id); if (i) add('act', `act.stop.${id}`, `Остановить «${i.name}»`, 'stop', () => stopGame(id), { k: 'закрыть игру stop kill' }); });
    $$('#theme-seg button:not([aria-selected="true"]):not(:disabled)').forEach((b) =>
      add('act', `theme.${b.dataset.themeSet}`, `Тема · ${label(b)}`, b.dataset.themeSet === 'light' ? 'sun' : 'moon', () => b.click(), { k: 'оформление theme dark light' }));
    $$('#wall-seg button:not([aria-selected="true"])').forEach((b) =>
      add('act', `wall.${b.dataset.wallSet}`, `Обои · ${label(b)}`, 'image', () => b.click(), { k: 'фон wallpaper стекло' }));
    const motion = $('#fx-motion');
    add('act', 'act.motion', motion.checked ? 'Выключить анимации' : 'Включить анимации', 'motion', () => { motion.checked = !motion.checked; motion.dispatchEvent(new Event('change')); }, { k: 'motion эффекты' });
    const other = pos() === 'top' ? 'bottom' : 'top';
    add('act', 'act.palpos', `Командная строка ${other === 'top' ? 'сверху' : 'снизу'}`, 'bar', () => { setLook({ palette_pos: other }); applyPal(); }, { k: 'палитра palette положение' });
    add('act', 'act.palblur', blurOn() ? 'Не размывать фон под строкой' : 'Размывать фон под строкой', 'bar', () => { setLook({ palette_blur: !blurOn() }); applyPal(); }, { k: 'blur размытие палитра' });
    if (S.current) add('act', 'act.folder.inst', 'Открыть папку сборки', 'folder', () => folder('open_instance_folder', { instance: S.current }), { k: 'folder проводник' });
    add('act', 'act.folder.themes', 'Открыть папку тем', 'folder', () => folder('open_themes_folder'), { k: 'folder themes' });
    add('act', 'act.folder.addons', 'Открыть папку плагинов', 'folder', () => folder('open_addons_folder'), { k: 'folder addons plugins' });
    return out;
  }

  // ---------- Matching: substring first, then fuzzy; a query typed in the wrong layout still matches ----------
  const EN = "qwertyuiop[]asdfghjkl;'zxcvbnm,.`", RU = 'йцукенгшщзхъфывапролджэячсмитьбюё';
  const swap = (s) => [...s].map((c) => { const i = EN.indexOf(c); if (i >= 0) return RU[i]; const j = RU.indexOf(c); return j >= 0 ? EN[j] : c; }).join('');
  const norm = (s) => s.toLowerCase().replace(/ё/g, 'е').replace(/\s+/g, ' ').trim();
  function match(q, s) {
    const t = norm(s);
    const at = t.indexOf(q);
    if (at >= 0) {
      const word = at === 0 || /[\s·«(.\-_]/.test(t[at - 1]);
      return { score: 1000 - at + (word ? 300 : 0) + (at === 0 ? 200 : 0), hits: [...Array(q.length).keys()].map((k) => at + k) };
    }
    const hits = []; let from = 0, gaps = 0;
    for (const c of q) {
      if (c === ' ') continue;
      const k = t.indexOf(c, from);
      if (k < 0) return null;
      if (hits.length) gaps += k - from;
      hits.push(k); from = k + 1;
    }
    return gaps > 6 + q.length * 2 ? null : { score: 400 - gaps * 10 - hits[0] * 2, hits };
  }
  function rate(c, qs) {
    let best = null;
    for (const q of qs) {
      const m = match(q, c.t);
      if (m && (!best || m.score > best.score)) best = m;
      const k = match(q, `${c.s || ''} ${c.k || ''}`);
      if (k && (!best || k.score * .55 > best.score)) best = { score: k.score * .55, hits: [] };
    }
    return best;
  }
  const used = () => { try { return JSON.parse(localStorage.getItem('fl.palette-recent') || '[]'); } catch { return []; } };
  function remember(id) { localStorage.setItem('fl.palette-recent', JSON.stringify([id, ...used().filter((x) => x !== id)].slice(0, 8))); }

  function results(raw) {
    const all = commands();
    const u = used();
    if (!raw) {
      const rec = u.map((id) => all.find((c) => c.id === id)).filter(Boolean).slice(0, 4);
      const seen = new Set(rec.map((c) => c.id));
      const rest = (g, n = 99) => all.filter((c) => c.g === g && !seen.has(c.id) && !c.deep).slice(0, n);
      return [['recent', rec], ['inst', rest('inst', 5)], ['go', rest('go')], ['act', rest('act')]].filter(([, l]) => l.length);
    }
    const qs = [...new Set([norm(raw), norm(swap(raw.toLowerCase()))])];
    const hit = all.map((c) => { const m = rate(c, qs); return m && { ...c, m, score: m.score + (u.includes(c.id) ? 80 - u.indexOf(c.id) * 10 : 0) }; }).filter(Boolean);
    const groups = ['inst', 'go', 'act'].map((g) => [g, hit.filter((c) => c.g === g).sort((a, b) => b.score - a.score).slice(0, g === 'inst' ? 6 : 8)]).filter(([, l]) => l.length);
    groups.sort((a, b) => b[1][0].score - a[1][0].score);
    if (S.instances.some((i) => i.loader && i.loader !== 'vanilla')) {
      groups.push(['mods', [{ g: 'mods', id: 'mods.search', t: `Найти «${raw.trim()}»`, s: 'моды на Modrinth', ico: 'search',
        run: () => { show('discover'); const f = $('#disc-q'); if (f) { f.value = raw.trim(); f.dispatchEvent(new Event('input')); } } }]]);
    }
    return groups;
  }

  // ---------- Rendering: rows are keyed, so a row that survives a keystroke stays put and only new rows animate in ----------
  let nodes = new Map(), flat = [], sel = 0, open = false, prevFocus = null;
  function marked(text, hits = []) {
    const set = new Set(hits), out = [];
    let buf = '', on = false;
    [...text].forEach((ch, k) => {
      const h = set.has(k);
      if (h !== on && buf) { out.push(on ? el('mark', {}, buf) : buf); buf = ''; }
      on = h; buf += ch;
    });
    if (buf) out.push(on ? el('mark', {}, buf) : buf);
    return out;
  }
  function row(c, pool, rank) {
    let n = nodes.get(c.id);
    const fresh = !n;
    if (fresh) n = el('div', { class: 'pal-item is-new', role: 'option', 'data-id': `palette.${c.id}` });
    const ico = el('span', { class: `pal-ico${c.cover ? ' is-cover' : ''}`, html: c.ico ? svg(c.ico) : '' });
    if (c.cover) bg(ico, c.cover);
    if (c.live) ico.append(el('i', { class: 'live' }));
    const hint = el('span', { class: 'pal-hint' });
    if (c.alt) hint.append(el('em', {}, c.alt), el('kbd', {}, '⇧ ↵'));
    hint.append(el('em', {}, c.hint || HINT[c.g]), el('kbd', {}, '↵'));
    n.replaceChildren(ico, el('span', { class: 'pal-t' }, el('b', {}, ...marked(c.t, c.m?.hits)), c.s ? el('span', {}, c.s) : ''),
      c.keys ? el('span', { class: 'pal-key' }, ...c.keys.map((k) => el('kbd', {}, k))) : '', hint);
    n.style.setProperty('--i', rank);
    n._c = c;
    pool.set(c.id, n);
    return n;
  }
  function render(first = false) {
    const groups = results(input.value.trim());
    const bottom = pos() === 'bottom';
    // Bottom bar: the best match sits right above the input, so the list is mirrored.
    const ranked = groups.flatMap(([, l]) => l);
    const order = bottom ? [...groups].reverse().map(([g, l]) => [g, [...l].reverse()]) : groups;
    const pool = new Map(), kids = [];
    flat = [];
    for (const [g, l] of order) {
      const key = `group.${g}`;
      const h = nodes.get(key) || el('div', { class: 'pal-group is-new' }, GROUP[g]);
      h.style.setProperty('--i', ranked.indexOf(l[bottom ? l.length - 1 : 0]));
      pool.set(key, h); kids.push(h);
      for (const c of l) { const n = row(c, pool, ranked.indexOf(c)); flat.push(n); kids.push(n); }
    }
    nodes = pool;
    list.replaceChildren(hlEl, ...(kids.length ? kids : [el('div', { class: 'pal-empty is-new' }, 'Ничего не найдено')]));
    body.classList.toggle('is-instant', first);
    body.style.height = `${list.offsetHeight + 13}px`;
    select(bottom ? flat.length - 1 : 0, false, first);
    body.scrollTop = bottom ? body.scrollHeight : 0;
    if (first) requestAnimationFrame(() => body.classList.remove('is-instant'));
  }
  list.addEventListener('animationend', (e) => e.target.classList.remove('is-new'));
  function select(i, scroll = true, instant = false) {
    hlEl.hidden = !flat.length;
    if (!flat.length) return;
    sel = (i + flat.length) % flat.length;
    flat.forEach((n, k) => n.setAttribute('aria-selected', String(k === sel)));
    const n = flat[sel];
    if (instant) { hlEl.style.transition = 'none'; requestAnimationFrame(() => { hlEl.style.transition = ''; }); }
    hlEl.style.transform = `translateY(${n.offsetTop}px)`;
    hlEl.style.height = `${n.offsetHeight}px`;
    if (scroll) n.scrollIntoView({ block: 'nearest' });
  }
  function exec(i, ev) {
    const c = flat[i]?._c;
    if (!c) return;
    if (c.g !== 'mods') remember(c.id);
    close(false);
    try { Promise.resolve(c.run({ alt: !!ev?.shiftKey })).catch((e) => toast(String(e), { error: true })); } catch (e) { toast(String(e), { error: true }); }
  }

  // ---------- Open / close ----------
  function show_(q = '') {
    if (open) { input.select(); return; }
    if ($('#app').hidden || document.querySelector('.dialog[data-open]')) return;
    window.FUI.closeMenu();
    open = true; prevFocus = document.activeElement;
    input.value = q;
    root.toggleAttribute('data-query', !!q.trim());
    nodes = new Map();
    root.removeAttribute('aria-hidden');
    root.setAttribute('data-open', '');
    render(true);
    input.focus({ preventScroll: true });
  }
  function close(restore = true) {
    if (!open) return;
    open = false;
    root.removeAttribute('data-open');
    root.setAttribute('aria-hidden', 'true');
    input.blur();
    if (restore && prevFocus && document.contains(prevFocus)) prevFocus.focus?.({ preventScroll: true });
  }
  input.addEventListener('input', () => { root.toggleAttribute('data-query', !!input.value.trim()); render(); });
  input.addEventListener('keydown', (e) => {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); select(sel + (e.key === 'ArrowDown' ? 1 : -1)); }
    else if (e.key === 'Enter') { e.preventDefault(); exec(sel, e); }
    else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); close(); }
    else if (e.key === 'Tab') e.preventDefault();
  });
  list.addEventListener('mousemove', (e) => { const i = flat.indexOf(e.target.closest('.pal-item')); if (i >= 0 && i !== sel) select(i, false); });
  list.addEventListener('click', (e) => { const i = flat.indexOf(e.target.closest('.pal-item')); if (i >= 0) exec(i, e); });
  $('#pal-scrim').addEventListener('click', () => close());
  $('#pal-open')?.addEventListener('click', () => show_());
  // KeyK by physical key, so Ctrl K also works on the Russian layout (Ctrl Л).
  document.addEventListener('keydown', (e) => {
    if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.code === 'KeyK') { e.preventDefault(); open ? close() : show_(); }
  }, true);
  window.FPalette = { open: show_, close };

  // Preview hook: ?pal=<query> opens the palette once the app is shown.
  const want = new URLSearchParams(location.search).get('pal');
  if (want !== null) { const t = setInterval(() => { if (!$('#app').hidden) { clearInterval(t); setTimeout(() => show_(want), 50); } }, 30); }
})();