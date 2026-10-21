'use strict';
// «Управление серверами»: a server is like an instance, but a server. A list of
// the user's servers, an add wizard (where it is hosted → a few steps), and per
// server: overview, players, files and settings.
(() => {
  const app = document.getElementById('app');
  const modal = document.getElementById('modal');
  let servers = [];
  let cur = null; // server id
  let tab = 'overview';
  let info = null;
  let filePath = '';
  let playerList = 'whitelist';

  // ---------- helpers ----------
  function h(tag, cls, ...kids) {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    for (const k of kids.flat()) if (k != null && k !== false) n.append(k);
    return n;
  }
  const btn = (label, cls, onclick, title) => {
    const b = h('button', `btn ${cls || ''}`.trim(), label);
    b.type = 'button';
    if (title) b.title = title;
    b.onclick = onclick;
    return b;
  };
  const errText = (e) => String(e?.message || e).replace(/^(invalid arguments|permission denied|file error|network error|data error): /, '');
  const fail = (e) => fl.toast(errText(e));
  const call = (name, args) => fl.call(name, args);
  const plural = (n, one, few, many) => {
    const a = Math.abs(n) % 100, b = a % 10;
    return a > 10 && a < 20 ? many : b === 1 ? one : b > 1 && b < 5 ? few : many;
  };
  const size = (b) => (b < 1024 ? `${b} Б` : b < 1048576 ? `${(b / 1024).toFixed(1)} КБ` : b < 1073741824 ? `${(b / 1048576).toFixed(1)} МБ` : `${(b / 1073741824).toFixed(2)} ГБ`);
  const date = (s) => (s ? new Date(s * 1000).toLocaleString('ru-RU', { day: 'numeric', month: 'short', year: 'numeric', hour: '2-digit', minute: '2-digit' }) : '');
  const hue = (s) => [...String(s)].reduce((a, c) => (a * 31 + c.charCodeAt(0)) % 360, 7);

  function ICO(name) {
    const P = {
      plus: 'M12 5v14M5 12h14',
      back: 'M15 6l-6 6 6 6',
      folder: 'M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z',
      file: 'M7 3h7l5 5v13H7zM14 3v5h5',
      trash: 'M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13',
      pc: 'M3 5h18v11H3zM8 20h8M12 16v4',
      cloud: 'M7 18h10a4 4 0 0 0 .5-8A6 6 0 0 0 6 9.5 4.5 4.5 0 0 0 7 18z',
      remote: 'M4 4h16v6H4zM4 14h16v6H4zM8 7h.01M8 17h.01',
      user: 'M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM4 21a8 8 0 0 1 16 0',
      edit: 'M4 20h4L19 9l-4-4L4 16zM14 6l4 4',
      img: 'M4 5h16v14H4zM4 15l4-4 4 4 3-3 5 5M15 9h.01',
      check: 'M5 12l5 5 9-10',
      warn: 'M12 3l10 18H2zM12 10v4M12 17h.01',
    };
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('viewBox', '0 0 24 24');
    svg.setAttribute('class', 'ico');
    const p = document.createElementNS('http://www.w3.org/2000/svg', 'path');
    p.setAttribute('d', P[name] || '');
    svg.append(p);
    return svg;
  }

  function serverIcon(s, big) {
    const box = h('div', `sicon${big ? ' big' : ''}`);
    if (s.icon) {
      const img = h('img');
      img.src = s.icon;
      img.alt = '';
      box.append(img);
    } else {
      box.classList.add('empty');
      box.style.setProperty('--h', hue(s.id || s.name));
      box.append(([...String(s.name || '?').trim()][0] || '?').toUpperCase());
    }
    return box;
  }

  const softLabel = (sw) => (sw ? [sw.kind, sw.version].filter(Boolean).join(' ') : 'Ядро не найдено');

  // ---------- modal ----------
  function openModal(...kids) {
    modal.replaceChildren(h('div', 'sheet', ...kids));
    modal.hidden = false;
    modal.onclick = (e) => { if (e.target === modal) closeModal(); };
  }
  function closeModal() { modal.hidden = true; modal.replaceChildren(); }
  document.addEventListener('keydown', (e) => { if (e.key === 'Escape' && !modal.hidden) closeModal(); });

  function confirm(title, text, okLabel, danger) {
    return new Promise((resolve) => {
      const done = (v) => { closeModal(); resolve(v); };
      openModal(
        h('h2', null, title),
        h('p', 'muted', text),
        h('div', 'row end', btn('Отмена', 'ghost', () => done(false)), btn(okLabel, danger ? 'danger' : 'primary', () => done(true))),
      );
    });
  }

  // ---------- list ----------
  async function loadList() {
    try { servers = await call('servers.list'); } catch (e) { servers = []; fail(e); }
  }

  async function showList() {
    cur = null; info = null;
    await loadList();
    const head = h('header', 'top',
      h('div', null, h('h1', null, 'Серверы'), h('p', 'muted', servers.length ? `${servers.length} ${plural(servers.length, 'сервер', 'сервера', 'серверов')}` : 'Здесь будут ваши серверы Minecraft')),
      btn([ICO('plus'), 'Добавить сервер'], 'primary', addWizard));
    const grid = h('section', 'grid');
    for (const s of servers) grid.append(card(s));
    if (!servers.length) {
      grid.append(h('div', 'empty-state',
        h('div', 'empty-art', ICO('pc')),
        h('h2', null, 'Пока ни одного сервера'),
        h('p', 'muted', 'Добавьте сервер, как сборку: выберите, где он работает, и управляйте файлами, игроками и настройками отсюда.'),
        btn([ICO('plus'), 'Добавить сервер'], 'primary', addWizard)));
    }
    app.replaceChildren(head, grid);
  }

  function card(s) {
    const c = h('button', `scard${s.exists ? '' : ' missing'}`);
    c.type = 'button';
    c.onclick = () => openServer(s.id);
    c.append(
      h('div', 'scard-mc', serverIcon(s), h('div', 'scard-text',
        h('div', 'scard-name', s.name),
        MOTD.render(s.motd))),
      h('div', 'scard-meta',
        h('span', 'chip', s.exists ? softLabel(s.software) : 'Папка не найдена'),
        h('span', 'chip', ICO('pc'), 'Этот компьютер'),
        h('span', 'chip', `:${s.port}`),
        h('span', 'chip', ICO('user'), `до ${s.max_players}`)));
    return c;
  }

  // ---------- add wizard ----------
  function addWizard() {
    const HOSTS = [
      { id: 'local', icon: 'pc', title: 'Этот компьютер', text: 'Сервер в папке на этом ПК: готовый или из сборки.' },
      { id: 'panel', icon: 'cloud', title: 'Хостинг', text: 'Панель хостинга: Aternos, Pterodactyl и другие.', soon: true },
      { id: 'remote', icon: 'remote', title: 'Свой VPS или другой ПК', text: 'По SFTP или SSH.', soon: true },
    ];
    const list = h('div', 'hosts');
    for (const o of HOSTS) {
      const b = h('button', `host${o.soon ? ' soon' : ''}`, h('div', 'host-ico', ICO(o.icon)),
        h('div', null, h('b', null, o.title, o.soon ? h('span', 'badge', 'скоро') : null), h('p', 'muted', o.text)));
      b.type = 'button';
      b.disabled = !!o.soon;
      b.onclick = () => localStep();
      list.append(b);
    }
    openModal(h('div', 'steps', h('i', 'on'), h('i')), h('h2', null, 'Где работает сервер?'), list,
      h('div', 'row end', btn('Отмена', 'ghost', closeModal)));
  }

  function localStep() {
    const name = h('input', 'input');
    name.placeholder = 'Например, «Ванилла с друзьями»';
    name.maxLength = 64;
    const pick = btn([ICO('folder'), 'Выбрать папку сервера'], 'primary', async () => {
      pick.disabled = true;
      try {
        const r = await call('servers.pick', { name: name.value.trim() });
        if (!r) return;
        closeModal();
        if (r.existing) fl.toast('Этот сервер уже в списке');
        await openServer(r.id);
      } catch (e) { fail(e); } finally { pick.disabled = false; }
    });
    openModal(h('div', 'steps', h('i', 'on'), h('i', 'on')),
      h('h2', null, 'Сервер на этом компьютере'),
      h('label', 'field', h('span', null, 'Название в лаунчере'), name),
      h('p', 'muted small', 'Выберите папку, где лежит сервер: там обычно есть server.properties и jar-файл ядра. Если поле названия пустое, возьмём имя папки.'),
      h('div', 'row end', btn('Назад', 'ghost', addWizard), pick));
    name.focus();
  }

  // ---------- one server ----------
  async function loadInfo() {
    info = await call('servers.info', { id: cur });
  }

  async function openServer(id, t = 'overview') {
    cur = id; tab = t; filePath = '';
    try { await loadInfo(); } catch (e) { fail(e); return showList(); }
    render();
  }

  const TABS = [['overview', 'Обзор'], ['players', 'Игроки'], ['files', 'Файлы'], ['settings', 'Настройки']];

  function render() {
    const s = info;
    const head = h('header', 'shead',
      btn(ICO('back'), 'icon ghost', showList, 'Все серверы'),
      serverIcon(s, true),
      h('div', 'shead-text', h('h1', null, s.name), h('p', 'muted', s.exists ? `${softLabel(s.software)} · порт ${s.port}` : 'Папка сервера не найдена')));
    const tabs = h('nav', 'tabs');
    for (const [id, label] of TABS) {
      const b = h('button', null, label);
      b.type = 'button';
      b.setAttribute('aria-selected', String(id === tab));
      b.disabled = !s.exists && id !== 'overview';
      b.onclick = () => { tab = id; render(); };
      tabs.append(b);
    }
    const body = h('section', 'body');
    app.replaceChildren(head, tabs, body);
    ({ overview, players, files, settings })[tab](body);
  }

  // ----- overview -----
  function overview(body) {
    const s = info;
    if (!s.exists) {
      body.append(h('div', 'notice warn', ICO('warn'), h('div', null, h('b', null, 'Папка не найдена'), h('p', null, s.dir))),
        h('div', 'row', btn([ICO('trash'), 'Убрать из списка'], 'danger', () => removeServer())));
      return;
    }
    const preview = h('div', 'mc-entry', serverIcon(s), h('div', 'mc-text',
      h('div', 'mc-top', h('span', 'mc-name', s.name), h('span', 'mc-count', `0/${s.max_players}`)),
      MOTD.render(s.motd)));
    const stats = h('div', 'stats',
      stat('Ядро', s.software?.kind || '—', s.software?.version ? `Minecraft ${s.software.version}` : s.software?.jar || 'jar не найден'),
      stat('Игроки', `до ${s.max_players}`, s.properties?.find((p) => p.key === 'white-list')?.value === 'true' ? 'вайтлист включён' : 'вход для всех'),
      stat('Мир', s.world?.name || 'world', s.world?.exists ? 'создан' : 'появится при первом запуске'),
      s.plugins ? stat('Плагины', String(s.plugins), 'в папке plugins') : stat('Моды', String(s.mods || 0), 'в папке mods'));
    const notes = [];
    if (!s.has_properties) notes.push(h('div', 'notice', ICO('warn'), h('p', null, 'server.properties ещё нет: сервер создаст его при первом запуске. Настройки, которые вы сохраните здесь, сервер возьмёт.')));
    if (!s.eula) notes.push(h('div', 'notice', ICO('warn'), h('p', null, 'EULA не принята (eula.txt). Без этого сервер не запустится.'),
      btn('Принять EULA', 'small', acceptEula)));
    body.append(
      h('div', 'panel', h('h2', null, 'Так сервер выглядит в списке серверов'), preview),
      stats, ...notes,
      h('div', 'panel', h('h2', null, 'Где лежит'), h('div', 'row',
        h('div', 'host-line', ICO('pc'), h('div', null, h('b', null, 'Этот компьютер'), h('p', 'muted mono', s.dir))))),
      h('div', 'row', btn([ICO('edit'), 'Название, иконка и MOTD'], null, () => { tab = 'settings'; render(); }),
        h('span', 'grow'), btn([ICO('trash'), 'Убрать из списка'], 'ghost danger-text', removeServer)));
  }

  function stat(label, value, sub) {
    return h('div', 'stat', h('span', null, label), h('b', null, value), h('small', null, sub));
  }

  async function acceptEula() {
    const ok = await confirm('Принять EULA Minecraft?', 'Это лицензионное соглашение Mojang для серверов: aka.ms/MinecraftEULA. Лаунчер запишет eula=true в eula.txt.', 'Принимаю');
    if (!ok) return;
    try {
      await call('servers.files.write', { id: cur, path: 'eula.txt', text: '#By changing the setting below to TRUE you are indicating your agreement to our EULA (https://aka.ms/MinecraftEULA).\neula=true\n' });
      await loadInfo(); render();
    } catch (e) { fail(e); }
  }

  async function removeServer() {
    const ok = await confirm(`Убрать «${info.name}» из списка?`, 'Файлы сервера останутся в папке, удаляется только запись в лаунчере.', 'Убрать', true);
    if (!ok) return;
    try { await call('servers.remove', { id: cur }); showList(); } catch (e) { fail(e); }
  }

  // ----- players -----
  const LISTS = [
    ['whitelist', 'Вайтлист'], ['ops', 'Операторы'], ['banned', 'Баны'], ['banned_ips', 'Баны по IP'], ['known', 'Заходили'],
  ];

  async function players(body) {
    body.append(h('p', 'muted', 'Загрузка…'));
    let p;
    try { p = await call('servers.players', { id: cur }); } catch (e) { body.replaceChildren(); return fail(e); }
    body.replaceChildren();

    const wl = h('label', 'switch', h('input'), h('i'), h('span', null, 'Вайтлист включён: зайти могут только игроки из списка'));
    const box = wl.querySelector('input');
    box.type = 'checkbox';
    box.checked = p.whitelist_enabled;
    box.onchange = async () => {
      try { await call('servers.properties.set', { id: cur, values: { 'white-list': box.checked, 'enforce-whitelist': box.checked } }); fl.toast(box.checked ? 'Вайтлист включён' : 'Вайтлист выключен'); }
      catch (e) { box.checked = !box.checked; fail(e); }
    };
    const top = h('div', 'panel row', wl);
    if (!p.online_mode) top.append(h('span', 'chip', 'online-mode=false'));

    const seg = h('nav', 'seg');
    for (const [id, label] of LISTS) {
      const b = h('button', null, label, h('span', 'count', String(p[id].length)));
      b.type = 'button';
      b.setAttribute('aria-selected', String(id === playerList));
      b.onclick = () => { playerList = id; body.replaceChildren(); players(body); };
      seg.append(b);
    }

    const editable = ['whitelist', 'ops', 'banned'].includes(playerList);
    const items = p[playerList] || [];
    const panel = h('div', 'panel');
    if (editable) panel.append(addForm(body));
    const rows = h('div', 'plist');
    for (const e of items) rows.append(playerRow(e, editable, body));
    if (!items.length) rows.append(h('p', 'empty', {
      whitelist: 'Список пуст. Добавьте ник, чтобы пустить игрока.',
      ops: 'Операторов нет.', banned: 'Банов нет.', banned_ips: 'Банов по IP нет.',
      known: 'На сервер ещё никто не заходил.',
    }[playerList]));
    panel.append(rows);
    body.append(top, seg, panel);
    if (playerList === 'banned_ips') body.append(h('p', 'muted small', 'Баны по IP ставятся командой /ban-ip на запущенном сервере.'));
  }

  function addForm(body) {
    const name = h('input', 'input');
    name.placeholder = 'Ник игрока';
    name.maxLength = 16;
    name.spellcheck = false;
    const reason = h('input', 'input');
    reason.placeholder = 'Причина (необязательно)';
    reason.maxLength = 120;
    const label = { whitelist: 'Добавить', ops: 'Сделать оператором', banned: 'Забанить' }[playerList];
    const go = btn([ICO('plus'), label], playerList === 'banned' ? 'danger' : 'primary', async () => {
      const n = name.value.trim();
      if (!n) return name.focus();
      go.disabled = true;
      try {
        const r = await call('servers.players.edit', { id: cur, list: playerList, op: 'add', name: n, reason: reason.value });
        fl.toast(r.existing ? `${n} уже в списке` : `${r.name || n}: готово`);
        body.replaceChildren(); players(body);
      } catch (e) { fail(e); go.disabled = false; }
    });
    name.onkeydown = (e) => { if (e.key === 'Enter') go.click(); };
    const f = h('div', 'addrow', name, playerList === 'banned' ? reason : null, go);
    return f;
  }

  function avatar(name, uuid) {
    const a = h('div', 'avatar', ([...String(name || '?')][0] || '?').toUpperCase());
    a.style.setProperty('--h', hue(uuid || name));
    return a;
  }

  function playerRow(e, editable, body) {
    const name = e.name || e.ip || '?';
    const sub = [];
    if (e.uuid) sub.push(e.uuid);
    if (playerList === 'ops' && e.level) sub.push(`уровень ${e.level}`);
    if ((playerList === 'banned' || playerList === 'banned_ips') && e.reason) sub.push(e.reason);
    if (playerList === 'known' && e.expiresOn) sub.push(`в кэше до ${e.expiresOn.slice(0, 10)}`);
    const row = h('div', 'prow', avatar(name, e.uuid), h('div', 'prow-text', h('b', null, name), h('small', 'mono', sub.join(' · '))));
    if (editable) {
      row.append(btn(ICO('trash'), 'icon ghost', async () => {
        const verb = { whitelist: 'Убрать из вайтлиста', ops: 'Снять оператора', banned: 'Разбанить' }[playerList];
        try { await call('servers.players.edit', { id: cur, list: playerList, op: 'remove', name: e.uuid || name }); fl.toast(`${name}: ${verb.toLowerCase()} — готово`); body.replaceChildren(); players(body); }
        catch (err) { fail(err); }
      }, { whitelist: 'Убрать', ops: 'Снять оператора', banned: 'Разбанить' }[playerList]));
    }
    return row;
  }

  // ----- files -----
  const TEXT = /\.(txt|properties|json|json5|yml|yaml|toml|cfg|conf|ini|log|md|mcmeta|snbt|csv|xml|lang|secret)$/i;
  const fileKind = (n) => (/\.(jar|zip)$/i.test(n) ? 'Архив' : /\.(png|jpe?g|gif|webp)$/i.test(n) ? 'Картинка' : /\.(dat|dat_old|mca|nbt)$/i.test(n) ? 'Данные' : TEXT.test(n) ? 'Текст' : '');

  async function files(body) {
    const crumbs = h('div', 'crumbs');
    const parts = filePath ? filePath.split('/') : [];
    const go = (p) => { filePath = p; render(); };
    const root = h('button', null, ICO('folder'), info.name);
    root.type = 'button';
    root.onclick = () => go('');
    crumbs.append(root);
    parts.forEach((part, i) => {
      const b = h('button', null, part);
      b.type = 'button';
      b.onclick = () => go(parts.slice(0, i + 1).join('/'));
      crumbs.append(h('span', 'sep', '/'), b);
    });
    const list = h('div', 'flist', h('p', 'muted', 'Загрузка…'));
    body.append(h('div', 'panel', crumbs, list));
    let items;
    try { items = await call('servers.files.list', { id: cur, path: filePath }); } catch (e) { list.replaceChildren(); return fail(e); }
    list.replaceChildren();
    if (filePath) {
      const up = h('button', 'frow', h('span', 'fico', ICO('back')), h('span', 'fname', '..'), h('span'), h('span'));
      up.type = 'button';
      up.onclick = () => go(parts.slice(0, -1).join('/'));
      list.append(up);
    }
    for (const f of items) {
      const path = filePath ? `${filePath}/${f.name}` : f.name;
      const r = h('button', `frow${f.dir ? ' dir' : ''}`,
        h('span', 'fico', ICO(f.dir ? 'folder' : 'file')),
        h('span', 'fname', f.name, !f.dir && fileKind(f.name) ? h('em', null, fileKind(f.name)) : null),
        h('span', 'fsize', f.dir ? '' : size(f.size)),
        h('span', 'fdate', date(f.modified)));
      r.type = 'button';
      r.onclick = () => (f.dir ? go(path) : openFile(path, f));
      list.append(r);
    }
    if (!items.length) list.append(h('p', 'empty', 'Папка пуста'));
  }

  async function openFile(path, f) {
    if (/\.(jar|zip|png|jpe?g|gif|webp|dat|dat_old|mca|nbt|exe|dll|so)$/i.test(f.name) || f.size > 2 * 1024 * 1024) {
      return fl.toast(f.size > 2 * 1024 * 1024 ? 'Файл больше 2 МБ, здесь его не открыть' : 'Это не текстовый файл');
    }
    let r;
    try { r = await call('servers.files.read', { id: cur, path }); } catch (e) { return fail(e); }
    const ta = h('textarea', 'editor');
    ta.value = r.text;
    ta.spellcheck = false;
    ta.readOnly = !r.writable;
    const save = btn('Сохранить', 'primary', async () => {
      save.disabled = true;
      try {
        await call('servers.files.write', { id: cur, path, text: ta.value });
        fl.toast('Сохранено');
        closeModal();
        if (path === 'server.properties' || path === 'eula.txt') await loadInfo();
      } catch (e) { fail(e); save.disabled = false; }
    });
    const del = btn([ICO('trash'), 'Удалить'], 'ghost danger-text', async () => {
      closeModal();
      if (!(await confirm(`Удалить ${f.name}?`, 'Файл удалится с диска насовсем.', 'Удалить', true))) return;
      try { await call('servers.files.delete', { id: cur, path }); render(); } catch (e) { fail(e); }
    });
    openModal(
      h('div', 'sheet-head', h('h2', 'mono', path), h('span', 'muted small', `${size(f.size)} · ${date(f.modified)}`)),
      r.writable ? null : h('div', 'notice', ICO('warn'), h('p', null, 'Этот файл только для просмотра: программы и моды плагины не меняют.')),
      ta,
      h('div', 'row', r.writable ? del : null, h('span', 'grow'), btn('Закрыть', 'ghost', closeModal), r.writable ? save : null));
    modal.firstChild.classList.add('wide');
  }

  // ----- settings -----
  const FIELDS = [
    ['Игра', [
      ['gamemode', 'Режим игры', 'select', [['survival', 'Выживание'], ['creative', 'Творческий'], ['adventure', 'Приключение'], ['spectator', 'Наблюдатель']], 'survival'],
      ['difficulty', 'Сложность', 'select', [['peaceful', 'Мирная'], ['easy', 'Лёгкая'], ['normal', 'Нормальная'], ['hard', 'Сложная']], 'easy'],
      ['hardcore', 'Хардкор', 'bool', null, 'false'],
      ['pvp', 'PvP между игроками', 'bool', null, 'true'],
      ['allow-flight', 'Разрешить полёт (нужно некоторым модам)', 'bool', null, 'false'],
      ['spawn-protection', 'Защита спавна, блоков', 'number', [0, 64], '16'],
    ]],
    ['Подключение', [
      ['max-players', 'Максимум игроков', 'number', [1, 1000], '20'],
      ['server-port', 'Порт', 'number', [1, 65535], '25565'],
      ['online-mode', 'Только лицензионные аккаунты', 'bool', null, 'true'],
      ['white-list', 'Вайтлист', 'bool', null, 'false'],
    ]],
    ['Мир', [
      ['level-name', 'Папка мира', 'text', null, 'world'],
      ['level-seed', 'Сид', 'text', null, ''],
      ['view-distance', 'Дальность прорисовки, чанков', 'number', [2, 32], '10'],
      ['simulation-distance', 'Дальность симуляции, чанков', 'number', [2, 32], '10'],
    ]],
  ];

  function settings(body) {
    const props = Object.fromEntries((info.properties || []).map((p) => [p.key, p.value]));
    const changed = {};
    const saveBar = h('div', 'savebar', h('span', null, 'Есть несохранённые изменения'),
      btn('Отменить', 'ghost', () => render()),
      btn('Сохранить', 'primary', async () => {
        try {
          if ('__name' in changed) {
            await call('servers.update', { id: cur, name: changed.__name });
            delete changed.__name;
          }
          if (Object.keys(changed).length) await call('servers.properties.set', { id: cur, values: changed });
          fl.toast('Сохранено');
          await loadInfo(); render();
        } catch (e) { fail(e); }
      }));
    saveBar.hidden = true;
    const mark = (k, v) => { changed[k] = v; saveBar.hidden = false; };

    // basics: name, icon, MOTD
    const nameIn = h('input', 'input');
    nameIn.value = info.name;
    nameIn.maxLength = 64;
    nameIn.oninput = () => mark('__name', nameIn.value);

    const iconBox = h('div', 'icon-edit', serverIcon(info, true));
    const fileIn = h('input');
    fileIn.type = 'file';
    fileIn.accept = 'image/png,image/jpeg,image/webp';
    fileIn.hidden = true;
    fileIn.onchange = () => fileIn.files[0] && setIcon(fileIn.files[0]);
    const iconBtns = h('div', 'col',
      btn([ICO('img'), info.icon ? 'Сменить иконку' : 'Выбрать иконку'], null, () => fileIn.click()),
      info.icon ? btn('Убрать', 'ghost', async () => { try { await call('servers.icon.delete', { id: cur }); await loadInfo(); render(); } catch (e) { fail(e); } }) : null,
      h('small', 'muted', 'Любая картинка, лаунчер сам сделает 64×64.'));

    const motdIn = h('textarea', 'input motd-in');
    motdIn.rows = 2;
    motdIn.value = String(props.motd ?? 'A Minecraft Server').replace(/\\n/g, '\n');
    const prev = h('div', 'mc-entry');
    const count = h('small', 'muted');
    const upd = () => {
      const lines = motdIn.value.split('\n');
      if (lines.length > 2) { motdIn.value = lines.slice(0, 2).join('\n'); }
      prev.replaceChildren(serverIcon(info), h('div', 'mc-text', h('div', 'mc-top', h('span', 'mc-name', nameIn.value || info.name), h('span', 'mc-count', `0/${props['max-players'] || 20}`)), MOTD.render(motdIn.value)));
      count.textContent = motdIn.value.split('\n').map((l) => `${MOTD.plain(l).length}/59`).join(' · ');
    };
    motdIn.oninput = () => { upd(); mark('motd', motdIn.value.replace(/\n/g, '\\n')); };
    nameIn.addEventListener('input', upd);
    const insert = (code) => {
      const a = motdIn.selectionStart, b = motdIn.selectionEnd;
      motdIn.setRangeText(`§${code}`, a, b, 'end');
      motdIn.focus();
      motdIn.oninput();
    };
    const palette = h('div', 'palette');
    for (const [c, label] of MOTD.NAMES) {
      const b = h('button', 'sw');
      b.type = 'button';
      b.title = label;
      b.style.background = MOTD.COLORS[c];
      b.onclick = () => insert(c);
      palette.append(b);
    }
    for (const [c, label, cls] of [['l', 'Жирный', 'B'], ['o', 'Курсив', 'I'], ['n', 'Подчёркнутый', 'U'], ['m', 'Зачёркнутый', 'S'], ['r', 'Сбросить', '↺']]) {
      const b = h('button', `fx fx-${c}`, cls);
      b.type = 'button';
      b.title = label;
      b.onclick = () => insert(c);
      palette.append(b);
    }
    upd();

    body.append(
      h('div', 'panel', h('h2', null, 'Основное'),
        h('div', 'basics', iconBox, iconBtns, fileIn,
          h('label', 'field grow', h('span', null, 'Название в лаунчере'), nameIn))),
      h('div', 'panel', h('h2', null, 'MOTD — описание в списке серверов'),
        prev, palette, motdIn, count));

    for (const [title, fields] of FIELDS) {
      const grid = h('div', 'fields');
      for (const [key, label, type, opt, def] of fields) grid.append(field(key, label, type, opt, props[key] ?? def, mark));
      body.append(h('div', 'panel', h('h2', null, title), grid));
    }

    // every property, for what the form above does not cover
    const known = new Set(['motd', ...FIELDS.flatMap(([, f]) => f.map((x) => x[0]))]);
    const rest = (info.properties || []).filter((p) => !known.has(p.key));
    if (rest.length) {
      const all = h('div', 'kv');
      for (const p of rest) {
        const inp = h('input', 'input mono');
        inp.value = p.value;
        inp.oninput = () => mark(p.key, inp.value);
        all.append(h('label', 'kv-row', h('span', 'mono', p.key), inp));
      }
      const det = h('details', 'panel', h('summary', null, `Все параметры server.properties (${rest.length})`), all);
      body.append(det);
    }
    body.append(saveBar);
  }

  function field(key, label, type, opt, value, mark) {
    if (type === 'bool') {
      const l = h('label', 'switch', h('input'), h('i'), h('span', null, label));
      const c = l.querySelector('input');
      c.type = 'checkbox';
      c.checked = String(value) === 'true';
      c.onchange = () => mark(key, c.checked);
      return l;
    }
    let inp;
    if (type === 'select') {
      inp = h('select', 'input');
      for (const [v, t] of opt) { const o = h('option', null, t); o.value = v; inp.append(o); }
      if (![...inp.options].some((o) => o.value === value)) { const o = h('option', null, value); o.value = value; inp.prepend(o); }
      inp.value = value;
      inp.onchange = () => mark(key, inp.value);
    } else {
      inp = h('input', 'input');
      inp.type = type === 'number' ? 'number' : 'text';
      if (opt) { inp.min = opt[0]; inp.max = opt[1]; }
      inp.value = value;
      inp.oninput = () => mark(key, inp.value);
    }
    return h('label', 'field', h('span', null, label), inp);
  }

  /** Any picture → a 64×64 PNG, centre-cropped. */
  function setIcon(file) {
    const fr = new FileReader();
    fr.onload = () => {
      const img = new Image();
      img.onload = async () => {
        const cv = document.createElement('canvas');
        cv.width = 64; cv.height = 64;
        const g = cv.getContext('2d');
        const s = Math.min(img.width, img.height);
        g.imageSmoothingEnabled = s > 64;
        g.imageSmoothingQuality = 'high';
        g.drawImage(img, (img.width - s) / 2, (img.height - s) / 2, s, s, 0, 0, 64, 64);
        try { await call('servers.icon.set', { id: cur, png: cv.toDataURL('image/png') }); fl.toast('Иконка обновлена'); await loadInfo(); render(); }
        catch (e) { fail(e); }
      };
      img.onerror = () => fl.toast('Не получилось открыть картинку');
      img.src = fr.result;
    };
    fr.readAsDataURL(file);
  }

  showList();
})();