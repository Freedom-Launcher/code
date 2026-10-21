'use strict';
// «Время в игре» launch screen, shown by the launcher while the game starts.
// It also records every launch — time, instance and the instance's total time
// before it — so the statistics page can show sessions and the last 14 days.
(() => {
  const MAX = 1000; // ~80 KB of the 1 MB storage
  let started = false;
  let mine = null; // the record this launch added

  function dur(s) {
    const hh = Math.floor(s / 3600);
    const mm = Math.floor((s % 3600) / 60);
    return hh ? `${hh} ч ${mm} мин` : `${Math.max(1, mm)} мин`;
  }

  async function record(id, inst) {
    try {
      const prev = await fl.storage.get('launches');
      const list = Array.isArray(prev) ? prev : [];
      mine = { t: Math.floor(Date.now() / 1000), id, before: inst ? inst.playtime_secs || 0 : null };
      list.push(mine);
      await fl.storage.set('launches', list.slice(-MAX));
    } catch { mine = null; /* no «Своё хранилище»: the screen still works */ }
  }

  async function start(ctx) {
    if (started || !ctx || ctx.kind !== 'overlay' || !ctx.instance) return;
    started = true;
    let inst = null;
    try { inst = (await fl.call('instances.list')).find((i) => i.id === ctx.instance) || null; } catch { /* no «Видеть сборки» */ }
    if (inst) {
      document.getElementById('name').textContent = inst.name;
      const loader = inst.loader && inst.loader !== 'vanilla' ? ` · ${inst.loader[0].toUpperCase()}${inst.loader.slice(1)}` : '';
      const time = inst.playtime_secs > 0 ? `наиграно ${dur(inst.playtime_secs)}` : 'первый запуск';
      document.getElementById('meta').textContent = `${inst.version}${loader} · ${time}`;
    }
    record(ctx.instance, inst);
  }

  function finish() {
    document.getElementById('kicker').textContent = 'Игра запущена';
    document.body.classList.add('ok');
    setTimeout(() => {
      document.body.classList.add('out');
      setTimeout(() => fl.overlay.done(), 450);
    }, 1600);
  }

  // A failed launch is not a session: take its record back.
  async function failed() {
    if (!mine) return;
    try {
      const list = await fl.storage.get('launches');
      if (Array.isArray(list)) await fl.storage.set('launches', list.filter((l) => !(l.t === mine.t && l.id === mine.id)));
    } catch { /* nothing to undo */ }
  }

  fl.on('context', start);
  start(fl.context);
  fl.on('game.launched', finish);
  fl.on('game.failed', failed);
})();