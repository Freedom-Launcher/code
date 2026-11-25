// The build doctor (ui/doctor.js): a small button instead of a banner, and a switch that turns it all off.
// Run: npm i --no-save jsdom && node scripts/ui-doctor-test.js
const { JSDOM } = require('jsdom'); const fs = require('fs'); const path = require('path');
const UI = path.join(__dirname, '..', 'ui');
let bad = 0;
const ok = (c, m) => { console.log((c ? 'ok   ' : 'FAIL ') + m); if (!c) bad = 1; };
const tick = (n = 20) => new Promise((r) => setTimeout(r, n));

async function boot(issues) {
  const html = fs.readFileSync(path.join(UI, 'index.html'), 'utf8').replace(/<script[\s\S]*?<\/script>/g, '');
  const dom = new JSDOM(html, { runScripts: 'outside-only', url: 'http://localhost/' });
  const w = dom.window; const d = w.document; const log = []; const calls = [];
  // `el` is the app's own helper: take it from app.js instead of guessing its behaviour.
  const app = fs.readFileSync(path.join(UI, 'app.js'), 'utf8');
  const start = app.indexOf('function el(tag, attrs = {}, ...kids) {');
  let depth = 0, end = start;
  for (let i = app.indexOf(') {', start) + 2; i < app.length; i++) { if (app[i] === '{') depth++; if (app[i] === '}' && --depth === 0) { end = i + 1; break; } }
  w.eval(app.slice(start, end));
  w.S = { view: 'instance', current: 'i1', instances: [{ id: 'i1', loader: 'fabric', version: '1.21', name: 'Test' }] };
  w.byId = (id) => w.S.instances.find((i) => i.id === id);
  w.call = async (name, args) => { calls.push(name); if (name === 'doctor.check') return { issues, mods: 5 }; return {}; };
  w._t = (s, a) => (a ? String(s).replace(/\{(\d+)\}/g, (_, i) => a[i]) : s); w._tp = (n, a) => a;
  w.toast = (t) => log.push(['toast', t]);
  w.actInst = () => 'i1';
  w.FContent = { reloads: 0, reload() { this.reloads++; } };
  w.FUI = { $: (s, r = d) => r.querySelector(s), openDlg: (id) => d.getElementById(id).setAttribute('data-open', ''), closeDlg: () => d.querySelector('[data-open]')?.removeAttribute('data-open'), closeMenu() {}, toast: w.toast };
  w.eval(fs.readFileSync(path.join(UI, 'doctor.js'), 'utf8'));
  return { w, d, calls, log, count: (n) => calls.filter((c) => c === n).length };
}
const two = [{ level: 'error', kind: 'duplicate', names: ['Sodium'], fix: { op: 'disable' } }, { level: 'error', kind: 'broken', names: ['x.jar'], fix: { op: 'disable' } }, { level: 'warn', kind: 'stray_pack', names: ['p.zip'], detail: 'shaderpacks', fix: { op: 'move' } }];

(async () => {
  // 1. on: a small button with the number of problems, no banner
  let t = await boot(two);
  await t.w.FDoctor.refresh('i1');
  const row = t.w.FDoctor.strip('i1');
  ok(row && row.className === 'ct-doc-row', 'with problems the doctor gives one small row');
  const btn = row.querySelector('button');
  ok(btn && btn.textContent.trim() === 'Доктор · 2', 'the button says how many errors (warnings are not counted)');
  ok(!/Исправить всё|Подробнее/.test(row.textContent) && row.querySelectorAll('button').length === 1, 'no banner text and no second button next to it');
  btn.click(); await tick();
  ok(t.d.getElementById('dlg-doctor').hasAttribute('data-open'), 'a click opens the details dialog');
  t.w.FUI.closeDlg();

  // 2. nothing to say: nothing is drawn
  t = await boot([{ level: 'warn', kind: 'stray_pack', names: ['p.zip'], detail: 'shaderpacks' }]);
  await t.w.FDoctor.refresh('i1');
  ok(t.w.FDoctor.strip('i1') === null, 'only warnings: no button at all');

  // 3. switched off: no requests, no button, no check before launch, menu entries gone
  t = await boot(two);
  ok(t.w.FDoctor.isOn() && !t.d.getElementById('mi-doctor').hidden, 'on by default, the menu entry is there');
  await t.w.FDoctor.refresh('i1');
  t.w.FDoctor.setEnabled(false); await tick();
  ok(!t.w.FDoctor.isOn() && t.w.localStorage.getItem('fl.doctor') === 'off', 'the choice is remembered');
  ok(t.w.FDoctor.strip('i1') === null, 'off: no button even if a report was there before');
  const before = t.count('doctor.check');
  await t.w.FDoctor.refresh('i1');
  ok(t.count('doctor.check') === before, 'off: opening a mod list does not run a check');
  ok(await t.w.FDoctor.beforeLaunch('i1') === true && t.count('doctor.check') === before, 'off: nothing is checked before a launch and the launch goes on');
  ok(t.d.getElementById('mi-doctor').hidden && t.d.getElementById('cx-doctor').hidden, 'off: both menu entries are hidden');
  ok(t.w.FContent.reloads >= 1, 'off: the open list is redrawn without the button');

  // 4. on again
  t.w.FDoctor.setEnabled(true); await tick(60);
  ok(t.w.FDoctor.isOn() && !t.d.getElementById('mi-doctor').hidden, 'on again: the menu entries are back');
  await t.w.FDoctor.refresh('i1');
  ok(t.w.FDoctor.strip('i1') !== null, 'on again: the button comes back after the next check');

  // 5. the narrower choice: no check before launch, but the button stays
  t = await boot(two); t.w.localStorage.setItem('fl.doctor.launch', 'off');
  await t.w.FDoctor.refresh('i1'); const c0 = t.count('doctor.check');
  ok(await t.w.FDoctor.beforeLaunch('i1') === true && t.count('doctor.check') === c0 && t.w.FDoctor.strip('i1') !== null, 'launch check off alone: no check before launch, the button still works');

  // 6. the setting is on the page, and remembered across pages
  ok(!!t.d.getElementById('doctor-on'), 'there is a switch in Settings');
  console.log(bad ? '\nFAILED' : '\nall ok'); process.exit(bad);
})();
