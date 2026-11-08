// The plugin catalog with sources (ui/addons.js) against the real ui/index.html, in jsdom.
// Run: npm i --no-save jsdom && node scripts/ui-plugins-catalog-test.js
const { JSDOM } = require('jsdom'); const fs = require('fs'); const path = require('path');
const UI = path.join(__dirname, '..', 'ui');
let bad = 0;
const ok = (c, m) => { console.log((c ? 'ok   ' : 'FAIL ') + m); if (!c) bad = 1; };
const tick = (n = 20) => new Promise((r) => setTimeout(r, n));

const entry = (o) => ({ id: 'p', name: 'P', author: 'a', description: 'd', version: '1.0.0', permissions: [], url: 'https://x.org/p.zip', sha256: 'a'.repeat(64), official: true, source: 'Freedom Launcher', source_url: '', downloads: 5, likes: 2, ...o });

(async () => {
  const html = fs.readFileSync(path.join(UI, 'index.html'), 'utf8').replace(/<script[\s\S]*?<\/script>/g, '');
  const dom = new JSDOM(html, { runScripts: 'outside-only', url: 'http://localhost/' });
  const w = dom.window; const d = w.document;
  const log = []; let sources = [{ url: '', name: 'Freedom Launcher', official: true, count: 1, error: null }];
  let plugins = [entry({ id: 'official-one', name: 'Official One' })];
  const inst = [];
  w.__TAURI__ = { core: { invoke: async (cmd, a) => {
    if (cmd !== 'action') return null;
    log.push([a.name, a.args]);
    switch (a.name) {
      case 'addons.list': return inst;
      case 'addons.catalog': return { plugins, offline: false, sources };
      case 'addons.source_add':
        if (a.args.input === 'bad') throw new Error('Не удалось прочитать список');
        sources = [...sources, { url: 'https://raw.githubusercontent.com/someone/plugins/HEAD/freedom-plugins.json', name: 'Someone', official: false, count: 1, error: null }];
        plugins = [...plugins, entry({ id: 'third', name: 'Third', official: false, source: 'Someone', source_url: sources[1].url, downloads: 0, likes: 0 })];
        return [];
      case 'addons.source_remove': sources = sources.filter((s) => s.url !== a.args.url); plugins = plugins.filter((p) => p.source_url !== a.args.url); return [];
      case 'addons.install_from': inst.push({ id: a.args.id, name: a.args.id, enabled: false, pending: false, permissions: [], version: '1.0.0' }); return inst;
      default: return null;
    }
  } } };
  w._t = (s, args) => (args ? String(s).replace(/\{(\d+)\}/g, (_, i) => args[i]) : s); w._tr = (s) => s;
  w.FUI = { toast: (t) => log.push(['toast', t]) };
  w.eval(fs.readFileSync(path.join(UI, 'addons.js'), 'utf8'));
  try { await w.FAddons.start(); } catch (e) { console.log('start threw:', e.message); }
  w.FAddons.shown('plugins'); // the player opens the «Плагины» screen
  await tick(80);

  const box = d.getElementById('plg-catalog');
  ok(box && !box.hidden, 'catalog tab is shown');
  ok(box.querySelector('.plg-sources'), 'the sources bar is there');
  ok([...box.querySelectorAll('.plg-src b')].map((b) => b.textContent).join() === 'Freedom Launcher', 'only the official source at first');
  ok(!box.querySelector('.plg-src-x'), 'the official source cannot be removed');
  ok(box.querySelectorAll('.plg-card').length === 1 && /Официальный/.test(box.textContent), 'official plugin carries the official badge');

  // add a source (Enter in the field)
  const input = box.querySelector('.plg-src-input');
  input.value = 'someone/plugins'; input.dispatchEvent(new w.Event('input'));
  input.dispatchEvent(new w.KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); await tick(80);
  ok(log.some((l) => l[0] === 'addons.source_add' && l[1].input === 'someone/plugins'), 'the typed repository goes to the core');
  const box2 = d.getElementById('plg-catalog');
  ok([...box2.querySelectorAll('.plg-src b')].map((b) => b.textContent).join() === 'Freedom Launcher,Someone', 'the new source appears as a chip');
  const cards = [...box2.querySelectorAll('.plg-card')];
  const third = cards.find((c) => /Third/.test(c.textContent));
  ok(third && /Сообщество/.test(third.textContent) && !/Официальный/.test(third.textContent), 'a plugin from another source is labelled «Сообщество», not official');
  ok(third && /Someone/.test(third.querySelector('.plg-meta').textContent), 'its source is named on the card');
  ok(third && !third.querySelector('.plg-nums'), 'no download/like numbers for foreign plugins (they could be invented)');
  ok(cards.find((c) => /Official One/.test(c.textContent)).querySelector('.plg-nums'), 'official plugin keeps its numbers');

  // install the foreign one: the source goes along
  third.querySelector('.plg-foot button.btn').click(); await tick(80);
  const inv = log.find((l) => l[0] === 'addons.install_from');
  ok(inv && inv[1].id === 'third' && inv[1].source === sources[1].url, 'install names the source, so a same-id plugin elsewhere is not meant');

  // bad address: a message, nothing added
  const before = sources.length;
  const i3 = d.getElementById('plg-catalog').querySelector('.plg-src-input'); i3.value = 'bad'; i3.dispatchEvent(new w.Event('input'));
  d.getElementById('plg-catalog').querySelector('.plg-src-add button').click(); await tick(60);
  ok(sources.length === before && log.some((l) => l[0] === 'toast' && /прочитать/.test(l[1])), 'a bad address shows the error and adds nothing');

  // remove it
  d.getElementById('plg-catalog').querySelector('.plg-src-x').click(); await tick(80);
  ok(log.some((l) => l[0] === 'addons.source_remove') && ![...d.querySelectorAll('#plg-catalog .plg-src b')].some((b) => b.textContent === 'Someone'), 'removing a source takes its chip and plugins away');

  console.log(bad ? '\nFAILED' : '\nall ok'); process.exit(bad);
})();
