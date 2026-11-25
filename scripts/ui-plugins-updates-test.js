// Plugin updates in the UI (ui/addons.js): banner, per-plugin button, update all, catalog tile.
// Run: npm i --no-save jsdom && node scripts/ui-plugins-updates-test.js
const { JSDOM } = require('jsdom'); const fs = require('fs'); const path = require('path');
const UI = path.join(__dirname, '..', 'ui');
let bad = 0;
const ok = (c, m) => { console.log((c ? 'ok   ' : 'FAIL ') + m); if (!c) bad = 1; };
const tick = (n = 30) => new Promise((r) => setTimeout(r, n));

async function boot({ installed, updates, catalog }) {
  const html = fs.readFileSync(path.join(UI, 'index.html'), 'utf8').replace(/<script[\s\S]*?<\/script>/g, '');
  const dom = new JSDOM(html, { runScripts: 'outside-only', url: 'http://localhost/' });
  const w = dom.window; const log = []; let ups = updates; let inst = installed;
  w.__TAURI__ = { core: { invoke: async (cmd, a) => {
    if (cmd !== 'action') return null;
    log.push([a.name, a.args]);
    switch (a.name) {
      case 'addons.list': return inst;
      case 'addons.updates': return ups;
      case 'addons.catalog': return { plugins: catalog, offline: false, sources: [{ url: '', name: 'Freedom Launcher', official: true, count: catalog.length, error: null }] };
      case 'addons.install_from': {
        const u = ups.find((x) => x.id === a.args.id);
        inst = inst.map((p) => (p.id === a.args.id && u ? { ...p, version: u.version } : p));
        ups = ups.filter((x) => x.id !== a.args.id);
        return inst;
      }
      default: return null;
    }
  } } };
  w._t = (s, args) => (args ? String(s).replace(/\{(\d+)\}/g, (_, i) => args[i]) : s); w._tr = (s) => s;
  w.FUI = { toast: (t) => log.push(['toast', t]), openDlg: (id) => log.push(['dlg', id]), closeDlg() {} };
  w.eval(fs.readFileSync(path.join(UI, 'addons.js'), 'utf8'));
  await w.FAddons.start(); w.FAddons.shown('plugins'); await tick(80);
  const tab = async (name) => { w.document.querySelector(`#plg-tabs .plg-tab[data-tab="${name}"]`).click(); await tick(); };
  return { w, d: w.document, log, tab };
}
const plugin = (o) => ({ id: 'a', name: 'A', version: '1.0.0', author: 'x', description: 'd', enabled: true, pending: false, permissions: [], trust: 'community', source: '', ...o });
const cat = (o) => ({ id: 'a', name: 'A', version: '1.0.0', author: 'x', description: 'd', permissions: [], official: true, source: 'Freedom Launcher', source_url: '', downloads: 0, likes: 0, url: 'https://x.org/a.zip', sha256: 'a'.repeat(64), ...o });

(async () => {
  // 1. nothing to update: no banner, no button
  let t = await boot({ installed: [plugin({})], updates: [], catalog: [cat({})] });
  await t.tab('installed');
  ok(!t.d.querySelector('.plg-updates'), 'no banner when everything is current');

  // 2. one update: banner and a button that names the new version
  const u1 = { id: 'a', name: 'A', have: '1.0.0', version: '1.1.0', source: 'official', source_url: '', official: true };
  t = await boot({ installed: [plugin({})], updates: [u1], catalog: [cat({ version: '1.1.0' })] });
  await t.tab('installed');
  const banner = t.d.querySelector('#addon-list .plg-updates');
  ok(banner && /1/.test(banner.textContent) && /1\.0\.0 → 1\.1\.0/.test(banner.textContent), 'the banner counts updates and shows old → new');
  const btn = [...t.d.querySelectorAll('#addon-list .plg-card .btn-primary')].find((b) => /Обновить до v1\.1\.0/.test(b.textContent));
  ok(!!btn, 'the plugin card offers «Обновить до v1.1.0»');
  btn.click(); await tick(100);
  const inv = t.log.find((l) => l[0] === 'addons.install_from');
  ok(inv && inv[1].id === 'a' && inv[1].source === '', 'the update is installed from the same source (official = empty source)');
  ok(!t.d.querySelector('#addon-list .plg-updates'), 'the banner goes away once it is current');

  // 3. update all: several plugins, each from its own source
  const src = 'https://example.org/list.json';
  const ps = [plugin({}), plugin({ id: 'b', name: 'B', source: src })];
  const us = [u1, { id: 'b', name: 'B', have: '1.0.0', version: '2.0.0', source: src, source_url: src, official: false }];
  t = await boot({ installed: ps, updates: us, catalog: [cat({ version: '1.1.0' }), cat({ id: 'b', name: 'B', version: '2.0.0', official: false, source: 'S', source_url: src })] });
  await t.tab('installed');
  const all = [...t.d.querySelectorAll('#addon-list .plg-updates button')].find((b) => /Обновить всё/.test(b.textContent));
  ok(!!all, 'with several updates the banner offers «Обновить всё»');
  all.click(); await tick(200);
  const calls = t.log.filter((l) => l[0] === 'addons.install_from').map((l) => `${l[1].id}@${l[1].source}`);
  ok(calls.includes('a@') && calls.includes(`b@${src}`), 'every plugin is updated from its own source');

  // 4. catalog tile: only the source the plugin came from counts
  t = await boot({ installed: [plugin({})], updates: [u1], catalog: [cat({ version: '1.1.0' }), cat({ id: 'a', version: '9.9.9', official: false, source: 'Other', source_url: src })] });
  await t.tab('catalog');
  const tiles = [...t.d.querySelectorAll('#plg-catalog .plg-card')];
  const official = tiles.find((c) => !/Other/.test(c.textContent));
  const other = tiles.find((c) => /Other/.test(c.textContent));
  ok(official && /Есть обновление/.test(official.textContent), 'the tile of the source it came from says «Есть обновление»');
  ok(other && !/Есть обновление/.test(other.textContent), 'a higher version in another source is not offered as an update');
  // 5. sign-in plugins that would show nothing yet are pointed out on the accounts page
  const prov = (o) => plugin({ id: 'ely', name: 'Ely.by', provider: 'Ely.by', enabled: false, ...o });
  t = await boot({ installed: [prov({}), prov({ id: 'p2', provider: 'Two', enabled: true, pending: true }), prov({ id: 'p3', enabled: true, pending: false }), prov({ id: 'p4', error: 'broken' }), plugin({ id: 'plain', enabled: false })], updates: [], catalog: [] });
  const hints = t.w.FAddons.providerHints();
  ok(hints.map((h) => h.id).join() === 'ely,p2', 'only sign-in plugins that are off or waiting for approval are pointed out');
  ok(hints[0].pending === false && hints[1].pending === true && hints[1].label === 'Two', 'off and «waiting for rights» are told apart');
  t.w.FAddons.reveal('ely'); await tick();
  ok(t.log.some((l) => l[0] === 'dlg' && l[1] === 'dlg-plugin') || t.d.querySelector('#plg-tabs .plg-tab[aria-selected="true"]')?.dataset.tab === 'installed', 'reveal opens the installed tab and the plugin\'s dialog');
  console.log(bad ? '\nFAILED' : '\nall ok'); process.exit(bad);
})();
