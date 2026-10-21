'use strict';
// «Время в игре»: own page with play statistics. Total time and the last
// launch per instance come from `instances.list` (the launcher keeps them);
// sessions and the 14-day chart come from the launch history that the launch
// screen (launch.js) writes into this plugin's storage.
(() => {
  const $ = (id) => document.getElementById(id);
  const h = (tag, cls, text) => {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text != null) n.textContent = text;
    return n;
  };
  const DAYS = 14;
  const running = new Set();

  function dur(s) {
    s = Math.max(0, Math.round(s || 0));
    const hh = Math.floor(s / 3600);
    const mm = Math.floor((s % 3600) / 60);
    if (hh) return mm ? `${hh} ч ${mm} мин` : `${hh} ч`;
    if (mm) return `${mm} мин`;
    return s ? 'меньше минуты' : '0 мин';
  }
  const midnight = (ms) => { const d = new Date(ms); d.setHours(0, 0, 0, 0); return d.getTime(); };
  function when(t) {
    if (!t) return 'ещё не запускалась';
    const d = new Date(t * 1000);
    const days = Math.round((midnight(Date.now()) - midnight(d.getTime())) / 864e5);
    const hm = d.toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' });
    if (days <= 0) return `сегодня в ${hm}`;
    if (days === 1) return `вчера в ${hm}`;
    return d.toLocaleDateString('ru-RU', { day: 'numeric', month: 'long' }) + ` в ${hm}`;
  }

  // A session lasts as long as the instance's total time grew until its next
  // launch (or until now, for the last one).
  function sessions(launches, byId) {
    const out = launches.map((l) => ({ ...l, secs: null }));
    const next = new Map();
    for (let i = out.length - 1; i >= 0; i--) {
      const l = out[i];
      const n = next.get(l.id);
      const after = n ? n.before : byId.get(l.id)?.playtime_secs;
      if (!n && running.has(l.id)) l.live = true;
      else if (typeof after === 'number' && typeof l.before === 'number') l.secs = Math.max(0, after - l.before);
      next.set(l.id, l);
    }
    return out;
  }

  function card(label, value, note) {
    const c = h('div', 'card');
    c.append(h('span', null, label), h('b', null, value));
    if (note) c.append(h('small', null, note));
    return c;
  }

  function chart(ses) {
    const today = midnight(Date.now());
    const days = [];
    for (let i = DAYS - 1; i >= 0; i--) days.push({ t: today - i * 864e5, secs: 0 });
    for (const s of ses) {
      const d = days.find((x) => x.t === midnight(s.t * 1000));
      if (d && s.secs) d.secs += s.secs;
    }
    const max = Math.max(...days.map((d) => d.secs), 1);
    $('chart').replaceChildren(...days.map((d, i) => {
      const col = h('div', d.t === today ? 'col today' : 'col');
      const bar = h('i', d.secs ? null : 'zero');
      bar.style.height = `${Math.max(2, (d.secs / max) * 100)}%`;
      bar.style.animationDelay = `${i * 25}ms`;
      const date = new Date(d.t);
      col.title = `${date.toLocaleDateString('ru-RU', { weekday: 'long', day: 'numeric', month: 'long' })}: ${dur(d.secs)}`;
      col.append(bar, h('small', null, String(date.getDate())));
      return col;
    }));
    return days.reduce((a, d) => a + d.secs, 0);
  }

  function bars(played) {
    if (!played.length) { $('bars').replaceChildren(h('p', 'empty', 'Пока ни одна сборка не запускалась.')); return; }
    const max = played[0].playtime_secs;
    $('bars').replaceChildren(...played.slice(0, 12).map((i) => {
      const row = h('div', 'bar');
      const name = h('div', 'name', i.name);
      name.append(h('small', null, running.has(i.id) ? 'играет сейчас' : when(i.last_played)));
      if (running.has(i.id)) name.lastChild.className = 'live';
      const track = h('div', 'track');
      const fill = h('div', 'fill');
      fill.style.width = `${Math.max(2, (i.playtime_secs / max) * 100)}%`;
      track.append(fill);
      row.append(name, track, h('span', 'val', dur(i.playtime_secs)));
      return row;
    }));
  }

  function recent(ses, byId, storageOk) {
    const last = ses.slice(-10).reverse();
    if (!last.length) {
      $('recent').replaceChildren(h('li', 'empty', storageOk
        ? 'История появится после следующего запуска игры.'
        : 'Истории нет: плагину не разрешено «Своё хранилище».'));
      return;
    }
    $('recent').replaceChildren(...last.map((s) => {
      const li = h('li');
      li.append(h('span', null, `${byId.get(s.id)?.name || 'Удалённая сборка'} · ${when(s.t)}`));
      const v = h('span', s.live ? 'live' : null, s.live ? 'играет сейчас' : s.secs == null ? '—' : dur(s.secs));
      li.append(v);
      return li;
    }));
  }

  async function render() {
    let list;
    try {
      list = await fl.call('instances.list');
    } catch (e) {
      $('sub').textContent = `Нет доступа к сборкам: ${e.message}. Разрешите «Видеть сборки» в Настройки → Плагины → «Права…».`;
      return;
    }
    let storageOk = true;
    const stored = await fl.storage.get('launches').catch(() => { storageOk = false; return null; });
    const r = await fl.call('game.running').catch(() => null);
    if (Array.isArray(r?.running)) { running.clear(); r.running.forEach((id) => running.add(id)); }

    const byId = new Map(list.map((i) => [i.id, i]));
    const ses = sessions(Array.isArray(stored) ? stored : [], byId);
    const played = list.filter((i) => i.playtime_secs > 0).sort((a, b) => b.playtime_secs - a.playtime_secs);
    const total = played.reduce((a, i) => a + i.playtime_secs, 0);
    const lastOne = [...list].sort((a, b) => (b.last_played || 0) - (a.last_played || 0))[0];

    const week = chart(ses);
    const hint = $('hint');
    hint.hidden = ses.length > 0;
    hint.textContent = storageOk
      ? 'График копится с первого запуска после установки плагина: время каждой игры записывает анимация запуска.'
      : 'Для графика нужно право «Своё хранилище» и «Анимация запуска».';

    $('sub').textContent = total
      ? `${played.length} из ${list.length} сборок уже запускались`
      : 'Запустите любую сборку — время появится здесь.';
    $('cards').replaceChildren(
      card('Всего наиграно', dur(total)),
      card('За 14 дней', ses.length ? dur(week) : '—', ses.length ? `${ses.filter((s) => s.t * 1000 >= midnight(Date.now()) - (DAYS - 1) * 864e5).length} запусков` : null),
      card('Любимая сборка', played[0]?.name || '—', played[0] ? dur(played[0].playtime_secs) : null),
      card('Последняя игра', lastOne?.last_played ? lastOne.name : '—', lastOne?.last_played ? (running.has(lastOne.id) ? 'играет сейчас' : when(lastOne.last_played)) : null),
    );
    bars(played);
    recent(ses, byId, storageOk);
    requestAnimationFrame(() => setTimeout(() => document.body.classList.add('calm'), 900));
  }

  let soon = 0;
  const later = (ms) => { clearTimeout(soon); soon = setTimeout(render, ms); };
  fl.on('game.launching', () => later(1500));
  fl.on('game.launched', (d) => { if (d?.instance) running.add(d.instance); later(300); });
  fl.on('game.exited', (d) => { running.delete(d?.instance); later(1500); }); // let the core save the new total
  setInterval(() => { if (running.size) render(); }, 60000);
  render();
})();