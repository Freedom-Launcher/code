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
    home: '<g fill="none" fill-rule="evenodd"><path fill="currentColor" d="M10.8 2.65a2 2 0 0 1 2.4 0l7 5.25a2 2 0 0 1 .8 1.6V19a2 2 0 0 1-2 2h-4.9a1.1 1.1 0 0 1-1.1-1.1V14a1 1 0 1 0-2 0v5.9A1.1 1.1 0 0 1 9.9 21H5a2 2 0 0 1-2-2V9.5a2 2 0 0 1 .8-1.6zm1.2 1.6L5 9.5V19h4v-5a3 3 0 1 1 6 0v5h4V9.5z"/></g>', plus: '<g fill="none"><path fill="currentColor" d="M11 20a1 1 0 1 0 2 0v-7h7a1 1 0 1 0 0-2h-7V4a1 1 0 1 0-2 0v7H4a1 1 0 1 0 0 2h7z"/></g>',
    compass: '<g fill="none"><path fill="currentColor" d="M12 2c5.523 0 10 4.477 10 10s-4.477 10-10 10S2 17.523 2 12 6.477 2 12 2m0 2a8 8 0 1 0 0 16 8 8 0 0 0 0-16m4.95 3.05c.395.396.397.98.336 1.51l-.037.283c-.16 1.12-.605 2.775-1.242 4.156-.31.672-.7 1.351-1.179 1.83-.478.477-1.156.868-1.83 1.178-1.38.637-3.035 1.082-4.155 1.242l-.282.037c-.53.06-1.118.056-1.51-.336-.357-.357-.393-.875-.351-1.365l.032-.29.019-.138c.16-1.12.605-2.775 1.242-4.156.31-.672.7-1.351 1.179-1.83.478-.478 1.157-.868 1.83-1.178 1.38-.637 3.035-1.082 4.155-1.242l.282-.037c.53-.06 1.115-.06 1.51.336Zm-1.712 1.712c-.908.155-2.323.55-3.4 1.047-.597.276-1.023.548-1.252.777-.23.229-.501.655-.777 1.252-.496 1.077-.892 2.492-1.047 3.4.908-.155 2.323-.55 3.4-1.047.597-.276 1.023-.548 1.252-.777.23-.229.502-.655.777-1.252.496-1.077.893-2.492 1.047-3.4M12 11a1 1 0 1 1 0 2 1 1 0 0 1 0-2"/></g>',
    gear: '<g fill="none" fill-rule="evenodd"><path fill="currentColor" d="M14.035 2.809c.37-.266.89-.39 1.401-.203a9.99 9.99 0 0 1 2.982 1.725c.417.35.57.861.524 1.313-.075.753.057 1.48.42 2.106.32.557.802.997 1.39 1.307l.225.11c.414.187.782.576.875 1.113a10.06 10.06 0 0 1 0 3.44c-.083.484-.39.847-.753 1.051l-.122.063c-.69.31-1.254.79-1.616 1.416-.362.627-.494 1.353-.419 2.106.045.452-.107.964-.524 1.313a9.99 9.99 0 0 1-2.982 1.725 1.51 1.51 0 0 1-1.4-.203C13.42 20.75 12.723 20.5 12 20.5s-1.42.249-2.035.691a1.51 1.51 0 0 1-1.401.203 9.989 9.989 0 0 1-2.982-1.725 1.511 1.511 0 0 1-.524-1.313c.075-.753-.058-1.48-.42-2.106a3.414 3.414 0 0 0-1.39-1.307l-.225-.11a1.511 1.511 0 0 1-.875-1.113 10.057 10.057 0 0 1 0-3.44c.083-.484.39-.847.753-1.051l.122-.062c.69-.311 1.254-.79 1.616-1.417.361-.626.494-1.353.419-2.106a1.511 1.511 0 0 1 .524-1.313 9.99 9.99 0 0 1 2.982-1.725 1.511 1.511 0 0 1 1.4.203c.615.442 1.312.691 2.036.691s1.42-.249 2.035-.691m.957 1.769c-.866.57-1.887.922-2.992.922s-2.126-.353-2.992-.922A7.99 7.99 0 0 0 7.068 5.7c.06 1.033-.145 2.093-.697 3.05-.553.956-1.368 1.663-2.293 2.128a8.078 8.078 0 0 0 0 2.242c.925.465 1.74 1.172 2.293 2.13.552.955.757 2.015.697 3.048a7.99 7.99 0 0 0 1.94 1.123c.866-.57 1.887-.922 2.992-.922s2.126.353 2.992.922a7.993 7.993 0 0 0 1.94-1.122c-.06-1.034.145-2.094.697-3.05.552-.957 1.368-1.664 2.293-2.13a8.066 8.066 0 0 0 0-2.24c-.925-.466-1.74-1.173-2.293-2.13-.552-.956-.757-2.016-.697-3.05a7.99 7.99 0 0 0-1.94-1.122ZM12 8a4 4 0 1 1 0 8 4 4 0 0 1 0-8m0 2a2 2 0 1 0 0 4 2 2 0 0 0 0-4"/></g>',
    user: '<g fill="none" fill-rule="evenodd"><path fill="currentColor" d="M12 13c2.396 0 4.575.694 6.178 1.672.8.488 1.484 1.064 1.978 1.69.486.615.844 1.351.844 2.138 0 .845-.411 1.511-1.003 1.986-.56.45-1.299.748-2.084.956-1.578.417-3.684.558-5.913.558s-4.335-.14-5.913-.558c-.785-.208-1.524-.506-2.084-.956C3.41 20.01 3 19.345 3 18.5c0-.787.358-1.523.844-2.139.494-.625 1.177-1.2 1.978-1.69C7.425 13.695 9.605 13 12 13m0 2c-2.023 0-3.843.59-5.136 1.379-.647.394-1.135.822-1.45 1.222-.324.41-.414.72-.414.899 0 .122.037.251.255.426.249.2.682.407 1.344.582C7.917 19.858 9.811 20 12 20c2.19 0 4.083-.143 5.4-.492.663-.175 1.096-.382 1.345-.582.218-.175.255-.304.255-.426 0-.18-.09-.489-.413-.899-.316-.4-.804-.828-1.451-1.222C15.843 15.589 14.023 15 12 15m0-13a5 5 0 1 1 0 10 5 5 0 0 1 0-10m0 2a3 3 0 1 0 0 6 3 3 0 0 0 0-6"/></g>',
    import: '<g fill="none"><path fill="currentColor" d="M16.382 4a2 2 0 0 1 1.71.964l.079.142 3.512 7.025a3 3 0 0 1 .308 1.109l.009.232V19a2 2 0 0 1-1.85 1.995L20 21H4a2 2 0 0 1-1.995-1.85L2 19v-5.528a3 3 0 0 1 .22-1.13l.097-.212 3.512-7.024a2 2 0 0 1 1.628-1.1L7.618 4zM8 14H4v5h16v-5h-4v.5a1.5 1.5 0 0 1-1.5 1.5h-5A1.5 1.5 0 0 1 8 14.5zm8.382-8H7.618l-3 6H8.5a1.5 1.5 0 0 1 1.493 1.356L10 13.5v.5h4v-.5a1.5 1.5 0 0 1 1.356-1.493L15.5 12h3.882z"/></g>', folder: '<g fill="none"><path fill="currentColor" d="M9.52 3a2 2 0 0 1 1.442.614l.12.137L12.48 5.5H19a2 2 0 0 1 1.995 1.85L21 7.5v1.53a2.001 2.001 0 0 1 1.61 2.398l-.038.147-2.4 8a2 2 0 0 1-1.752 1.418l-.164.007H4a2 2 0 0 1-1.995-1.85L2 19V5a2 2 0 0 1 1.85-1.995L4 3zm11.136 8H6.744l-2.4 8h13.912zM9.519 5H4v8.187l.828-2.762A2 2 0 0 1 6.744 9H19V7.5h-6.52a2 2 0 0 1-1.561-.75z"/></g>',
    moon: '<g fill="none" fill-rule="evenodd"><path fill="currentColor" d="M13.574 3.137a1.01 1.01 0 0 0-1.097 1.409 6 6 0 0 1-7.931 7.931 1.01 1.01 0 0 0-1.409 1.097A9 9 0 0 0 21 12c0-4.434-3.206-8.118-7.426-8.863m1.307 2.481A7.002 7.002 0 0 1 12 19a7.002 7.002 0 0 1-6.382-4.12 8 8 0 0 0 9.263-9.263Z"/></g>',
    sun: '<g fill="none"><path fill="currentColor" d="M12 19a1 1 0 0 1 1 1v1a1 1 0 1 1-2 0v-1a1 1 0 0 1 1-1m6.364-2.05.707.707a1 1 0 0 1-1.414 1.414l-.707-.707a1 1 0 0 1 1.414-1.414m-12.728 0a1 1 0 0 1 1.497 1.32l-.083.094-.707.707a1 1 0 0 1-1.497-1.32l.083-.094zM12 6a6 6 0 1 1 0 12 6 6 0 0 1 0-12m0 2a4 4 0 1 0 0 8 4 4 0 0 0 0-8m-8 3a1 1 0 0 1 .117 1.993L4 13H3a1 1 0 0 1-.117-1.993L3 11zm17 0a1 1 0 1 1 0 2h-1a1 1 0 1 1 0-2zM4.929 4.929a1 1 0 0 1 1.32-.083l.094.083.707.707a1 1 0 0 1-1.32 1.497l-.094-.083-.707-.707a1 1 0 0 1 0-1.414m14.142 0a1 1 0 0 1 0 1.414l-.707.707a1 1 0 1 1-1.414-1.414l.707-.707a1 1 0 0 1 1.414 0M12 2a1 1 0 0 1 1 1v1a1 1 0 1 1-2 0V3a1 1 0 0 1 1-1"/></g>',
    image: '<g fill="none"><path fill="currentColor" d="M20 3a2 2 0 0 1 1.995 1.85L22 5v14a2 2 0 0 1-1.85 1.995L20 21H4a2 2 0 0 1-1.995-1.85L2 19V5a2 2 0 0 1 1.85-1.995L4 3zM9.879 12.05l-5.657 5.657a1.002 1.002 0 0 1-.222.168V19h16v-1.125a1 1 0 0 1-.222-.168L16.95 14.88l-.707.707.207.207a1 1 0 0 1-1.415 1.414L9.88 12.05ZM20 5H4v10.1l4.995-4.994a1.25 1.25 0 0 1 1.666-.091l.101.09 4.066 4.067 1.238-1.238a1.25 1.25 0 0 1 1.666-.091l.102.091L20 15.101zm-4.5 2a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3"/></g>', motion: '<g fill="none" fill-rule="evenodd"><path fill="currentColor" d="M9.514 3a1 1 0 0 1 .949.73l3.673 13.06 1.928-5.141A1 1 0 0 1 17 11h3a1 1 0 1 1 0 2h-2.307l-2.757 7.351a1 1 0 0 1-1.899-.08L9.45 7.514l-1.496 4.784A1 1 0 0 1 7 13H4a1 1 0 1 1 0-2h2.265l2.28-7.298A1 1 0 0 1 9.516 3Z"/></g>',
    stop: '<g fill="none" fill-rule="evenodd"><path fill="currentColor" d="M4 6a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2zm14 0H6v12h12z"/></g>', search: '<g fill="none" fill-rule="evenodd"><path fill="currentColor" d="M10.5 2a8.5 8.5 0 1 0 5.262 15.176l3.652 3.652a1 1 0 0 0 1.414-1.414l-3.652-3.652A8.5 8.5 0 0 0 10.5 2M4 10.5a6.5 6.5 0 1 1 13 0 6.5 6.5 0 0 1-13 0"/></g>', bar: '<g fill="none"><path fill="currentColor" d="M17 18a1 1 0 0 1 .117 1.993L17 20H7a1 1 0 0 1-.117-1.993L7 18zm3-5a1 1 0 1 1 0 2H4a1 1 0 1 1 0-2zm-3-5a1 1 0 0 1 .117 1.993L17 10H7a1 1 0 0 1-.117-1.993L7 8zm3-5a1 1 0 0 1 .117 1.993L20 5H4a1 1 0 0 1-.117-1.993L4 3z"/></g>',
  };
  const svg = (k) => `<svg class="mi" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">${P[k]}</svg>`;
  const GROUP = { recent: _t("Недавние"), inst: _t("Сборки"), go: _t("Перейти"), act: _t("Действия"), mods: 'Modrinth' };
  const HINT = { inst: _t("Играть"), go: _t("Перейти"), act: _t("Выполнить"), mods: _t("Искать") };

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
        { s: instMeta(i), k: _t("{0} {1} сборка", [i.loader || 'vanilla', i.version]), cover: iconName(i), live: on, hint: on ? _t("Открыть") : _t("Играть"), alt: !on && _t("Открыть") });
    });
    add('go', 'go.home', _t("Библиотека"), 'home', () => show('home'), { k: _t("библиотека главная домой сборки library home") });
    add('go', 'go.create', _t("Новая сборка"), 'plus', () => show('create'), { k: _t("создать new instance"), keys: [MOD, 'N'] });
    add('go', 'go.skins', _t("Скины"), 'user', () => show('skins'), { k: _t("скины скин плащ модель рук skin skins cape") });
    add('go', 'go.discover', _t("Каталог"), 'compass', () => show('discover'), { k: _t("каталог обзор моды сборки модпаки ресурспаки шейдеры датапаки modrinth browse mods modpacks") });
    add('go', 'go.accounts', _t("Аккаунты"), 'user', () => show('accounts'), { k: _t("профиль вход microsoft account login") });
    [['look', _t("Внешний вид"), _t("тема оформление раскладка анимации theme")], ['wall', _t("Обои и стекло"), _t("фон картинка прозрачность wallpaper glass")],
      ['ui', _t("Интерфейс"), _t("редактор командная строка каталог")], ['game', _t("Параметры сборок"), _t("java память ram окно хуки папка")],
      ['stats', _t("Статистика"), _t("время в игре часы playtime stats")], ['addons', _t("Плагины"), _t("плагины аддоны дополнения addons plugins")],
      ['keys', _t("Клавиши"), _t("горячие клавиши сочетания shortcuts hotkeys")], ['about', _t("О лаунчере"), _t("версия обновления update about")]]
      .forEach(([p, t, k]) => add('go', `set.${p}`, _t("Настройки · {0}", [t]), 'gear', () => openSettings(p), { k: _t("settings настройки {0}", [k]) }));
    (window.FSettings?.entries() || []).forEach((x) => add('go', x.id, x.t, 'gear', x.go, { s: _t("Настройки · {0}", [x.s]), k: _t("settings настройки {0}", [x.k]), deep: true }));
    add('act', 'act.ui-edit', _t("Настроить интерфейс"), 'gear', () => window.FEditor?.open(), { k: _t("редактор интерфейса вид углы скругление размер кнопок расположение панели customize editor layout"), keys: [MOD, 'Shift', 'E'] });
    add('act', 'act.foreign', _t("Импорт из других лаунчеров"), 'import', () => window.FForeign?.open(), { k: _t("prism multimc curseforge atlauncher gdlauncher modrinth app перенос импорт import launcher") });
    add('act', 'act.import', _t("Импорт .mrpack"), 'import', () => { show('create'); $('#c-import')?.click(); }, { k: _t("modpack модпак import файл") });
    S.running.forEach((id) => { const i = byId(id); if (i) add('act', `act.stop.${id}`, _t("Остановить «{0}»", [i.name]), 'stop', () => stopGame(id), { k: _t("закрыть игру stop kill") }); });
    $$('#theme-seg button:not([aria-selected="true"]):not(:disabled)').forEach((b) =>
      add('act', `theme.${b.dataset.themeSet}`, _t("Тема · {0}", [label(b)]), b.dataset.themeSet === 'light' ? 'sun' : 'moon', () => b.click(), { k: _t("оформление theme dark light") }));
    $$('#wall-seg button:not([aria-selected="true"])').forEach((b) =>
      add('act', `wall.${b.dataset.wallSet}`, _t("Обои · {0}", [label(b)]), 'image', () => b.click(), { k: _t("фон wallpaper стекло") }));
    const motion = $('#fx-motion');
    add('act', 'act.motion', motion.checked ? _t("Выключить анимации") : _t("Включить анимации"), 'motion', () => { motion.checked = !motion.checked; motion.dispatchEvent(new Event('change')); }, { k: _t("motion эффекты") });
    const other = pos() === 'top' ? 'bottom' : 'top';
    add('act', 'act.palpos', _t("Командная строка {0}", [other === 'top' ? _t("сверху") : _t("снизу")]), 'bar', () => { setLook({ palette_pos: other }); applyPal(); }, { k: _t("палитра palette положение") });
    add('act', 'act.palblur', blurOn() ? _t("Не размывать фон под строкой") : _t("Размывать фон под строкой"), 'bar', () => { setLook({ palette_blur: !blurOn() }); applyPal(); }, { k: _t("blur размытие палитра") });
    if (S.current) add('act', 'act.folder.inst', _t("Открыть папку сборки"), 'folder', () => folder('open_instance_folder', { instance: S.current }), { k: _t("folder проводник") });
    add('act', 'act.folder.themes', _t("Открыть папку тем"), 'folder', () => folder('open_themes_folder'), { k: 'folder themes' });
    add('act', 'act.folder.addons', _t("Открыть папку плагинов"), 'folder', () => folder('open_addons_folder'), { k: 'folder addons plugins' });
    return out;
  }

  // ---------- Matching: substring first, then fuzzy; a query typed in the wrong layout still matches ----------
  const EN = "qwertyuiop[]asdfghjkl;'zxcvbnm,.`", RU = _t("йцукенгшщзхъфывапролджэячсмитьбюё");
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
      groups.push(['mods', [{ g: 'mods', id: 'mods.search', t: _t("Найти «{0}»", [raw.trim()]), s: _t("моды на Modrinth"), ico: 'search',
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
    list.replaceChildren(hlEl, ...(kids.length ? kids : [el('div', { class: 'pal-empty is-new' }, _t("Ничего не найдено"))]));
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