// Developer mode in the plugins UI (ui/addons.js + addon-sdk.js) against the real index.html, in jsdom.
// Run: npm i --no-save jsdom && node scripts/ui-plugins-dev-test.js
const { JSDOM } = require('jsdom'); const fs = require('fs'); const path = require('path');
const UI = path.join(__dirname, '..', 'ui');
let bad = 0;
const ok = (c, m) => { console.log((c ? 'ok   ' : 'FAIL ') + m); if (!c) bad = 1; };
const tick = (n = 40) => new Promise((r) => setTimeout(r, n));

async function boot(devMode, links) {
  const html = fs.readFileSync(path.join(UI, 'index.html'), 'utf8').replace(/<script[\s\S]*?<\/script>/g, '');
  const dom = new JSDOM(html, { runScripts: 'outside-only', url: 'http://localhost/' });
  const w = dom.window; const log = []; let state = links;
  w.__TAURI__ = { core: { invoke: async (cmd, a) => {
    log.push([cmd, a && a.name ? a.name : '', a && a.args ? a.args : a]);
    if (cmd === 'pick_folder') return '/home/dev/my-plugin';
    if (cmd === 'open_dev_folder') return null;
    if (cmd !== 'action') return null;
    switch (a.name) {
      case 'settings.get': return { developer_mode: devMode };
      case 'addons.list': return [];
      case 'addons.updates': return [];
      case 'addons.catalog': return { plugins: [], offline: false, sources: [] };
      case 'addons.dev_state': return state;
      case 'addons.dev_link': state = [...state, { path: a.args.path, id: 'my-plugin', name: 'My plugin', rev: 1, synced_at: 1700000000, error: null }]; return state;
      case 'addons.dev_unlink': state = state.filter((s) => s.path !== a.args.path); return state;
      case 'addons.dev_new': state = [...state, { path: a.args.parent + '/' + a.args.id, id: a.args.id, name: a.args.name, rev: 1, synced_at: 1700000000, error: null }]; return state;
      default: return null;
    }
  } } };
  w._t = (s, args) => (args ? String(s).replace(/\{(\d+)\}/g, (_, i) => args[i]) : s); w._tr = (s) => s;
  w.FUI = { toast: (t) => log.push(['toast', t]), openDlg() {}, closeDlg() {} };
  w.eval(fs.readFileSync(path.join(UI, 'addons.js'), 'utf8'));
  await w.FAddons.start(); w.FAddons.shown('plugins'); await tick(80);
  const d = w.document;
  return { w, d, log, setState: (s) => { state = s; }, tab: async () => { d.querySelector('#plg-tabs .plg-tab[data-tab="dev"]').click(); await tick(60); } };
}
const calls = (t, name) => t.log.filter((l) => l[1] === name).length;

(async () => {
  // 1. off by default: no tab, no polling
  let t = await boot(false, []);
  ok(t.d.getElementById('plg-tab-dev').hidden, 'developer mode off: the «Разработка» tab is hidden');
  ok(calls(t, 'addons.dev_state') === 0, 'developer mode off: nothing is polled');

  // 2. on: tab, empty panel, link a folder
  t = await boot(true, []);
  ok(!t.d.getElementById('plg-tab-dev').hidden, 'developer mode on: the tab is shown');
  await t.tab();
  const panel = t.d.getElementById('plg-dev');
  ok(!panel.hidden && /Подключить папку/.test(panel.textContent) && /Папок пока нет/.test(panel.textContent), 'the panel offers to connect a folder');
  [...panel.querySelectorAll('button')].find((b) => /Подключить папку/.test(b.textContent)).click(); await tick(80);
  ok(t.log.some((l) => l[1] === 'addons.dev_link' && l[2].path === '/home/dev/my-plugin'), 'the picked folder is linked');
  ok(/My plugin/.test(t.d.getElementById('plg-dev').textContent) && /\/home\/dev\/my-plugin/.test(t.d.getElementById('plg-dev').textContent) && /Синхронизировано/.test(t.d.getElementById('plg-dev').textContent), 'the linked folder is listed with its status');

  // 3. a refreshed copy rebuilds the plugin list (live reload), and is logged
  const lists = calls(t, 'addons.list');
  t.setState([{ path: '/home/dev/my-plugin', id: 'my-plugin', name: 'My plugin', rev: 2, synced_at: 1700000100, error: null }]);
  await tick(1200);
  ok(calls(t, 'addons.list') > lists, 'a new revision reloads the plugin list (and re-mounts the frames)');
  ok(/Обновлено #2/.test(t.d.getElementById('plg-dev').textContent), 'the refresh is written to the log');

  // 4. an error in the folder is shown on the row and in the log
  t.setState([{ path: '/home/dev/my-plugin', id: 'my-plugin', name: 'My plugin', rev: 2, synced_at: 1700000200, error: 'addon.json: неверный JSON' }]);
  await tick(1200);
  const txt = t.d.getElementById('plg-dev').textContent;
  ok((txt.match(/addon\.json: неверный JSON/g) || []).length >= 2, 'the error appears on the row and in the log');

  // 5. what a plugin prints reaches the log, as text only
  const f = t.d.createElement('iframe'); f.className = 'addon-frame'; f.dataset.addon = 'my-plugin'; f.title = 'My plugin'; t.d.body.append(f);
  const post = (data) => t.w.dispatchEvent(new t.w.MessageEvent('message', { data: { fl: 1, ...data }, source: f.contentWindow }));
  post({ type: 'log', level: 'error', text: '<img src=x onerror=alert(1)> boom' }); await tick(40);
  const line = t.d.querySelector('#plg-dev .plg-dev-line.is-error:last-child');
  ok(line && line.textContent.includes('boom') && !t.d.querySelector('#plg-dev img'), 'a plugin error is in the log and is text, not markup');
  post({ type: 'log', level: 'bogus', text: 'x' }); await tick(20);
  ok(!t.d.querySelector('#plg-dev .plg-dev-line.is-bogus'), 'an unknown log level is not trusted');
  // a window that is not one of our plugin frames cannot write to the log
  const before = t.d.querySelectorAll('#plg-dev .plg-dev-line').length;
  t.w.dispatchEvent(new t.w.MessageEvent('message', { data: { fl: 1, type: 'log', level: 'error', text: 'forged' }, source: t.w }));
  await tick(20);
  ok(t.d.querySelectorAll('#plg-dev .plg-dev-line').length === before, 'messages from anything but a plugin frame are ignored');

  // 6. unlink, and switching developer mode off hides everything and stops polling
  [...t.d.querySelectorAll('#plg-dev button')].find((b) => /Отключить/.test(b.textContent)).click(); await tick(80);
  ok(t.log.some((l) => l[1] === 'addons.dev_unlink'), 'unlink goes to the core');
  t.w.FAddons.setDev(false); await tick(50);
  const polled = calls(t, 'addons.dev_state'); await tick(1200);
  ok(t.d.getElementById('plg-tab-dev').hidden && t.d.getElementById('plg-dev').hidden && calls(t, 'addons.dev_state') === polled, 'switched off: the tab and panel go away and polling stops');
  // 7. the SDK inside a plugin: errors and console output reach the launcher, rate-limited
  {
    const dom = new JSDOM('<body></body>', { runScripts: 'outside-only', url: 'http://localhost/' });
    const w = dom.window; const got = [];
    w.addEventListener('message', (e) => { if (e.data && e.data.type === 'log') got.push(e.data); });
    for (const l of ['log', 'info', 'warn', 'error']) w.console[l] = () => {}; // the SDK still wraps and forwards these
    w.eval(fs.readFileSync(path.join(UI, 'addon-sdk.js'), 'utf8'));
    w.console.error('first', { a: 1 }); await tick(10);
    ok(got.length === 1 && got[0].level === 'error' && got[0].text === 'first {"a":1}', 'console.error from a plugin is forwarded with its arguments');
    w.dispatchEvent(new w.ErrorEvent('error', { message: 'bad thing', filename: 'http://x/a/main.js', lineno: 3 })); await tick(10);
    ok(got.some((g) => g.text === 'bad thing (main.js:3)'), 'an uncaught error is forwarded with file and line');
    got.length = 0;
    for (let i = 0; i < 100; i++) w.console.log('spam', i);
    await tick(20);
    ok(got.length <= 20, 'a noisy plugin sends at most 20 log lines a second');
    w.console.log('x'.repeat(5000)); await tick(10);
    ok(got.every((g) => g.text.length <= 600), 'a long line is cut');
  }
  console.log(bad ? '\nFAILED' : '\nall ok'); process.exit(bad);
})();
