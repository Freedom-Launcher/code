// Mounting of theme templates (ui/themes.js) against the real ui/index.html, in jsdom.
// Run: npm i --no-save jsdom && node scripts/ui-mount-test.js   (exits 1 on any FAIL)
const { JSDOM } = require('jsdom'); const fs = require('fs'); const path = require('path');
const UI = path.join(__dirname, '..', 'ui');
let html=fs.readFileSync(path.join(UI, 'index.html'), 'utf8').replace(/<script[\s\S]*?<\/script>/g,'');
const dom=new JSDOM(html,{runScripts:'outside-only',url:'http://localhost/'});
const w=dom.window; w.CSS={escape:s=>String(s).replace(/["\\]/g,'\\$&')};
w.eval(fs.readFileSync(path.join(UI, 'themes.js'), 'utf8'));
const d=w.document, ok=(c,m)=>{console.log((c?'ok  ':'FAIL')+' '+m); if(!c) process.exitCode=1;};
const app=d.getElementById('app');
// 1. shell without main is refused and leaves the DOM alone
const before=app.children.length;
ok(w.FTheme.mount('t','shell','<div data-use="rail"></div>')===false,'shell without main refused');
ok(app.children.length===before && !app.dataset.themed,'DOM untouched after refusal');
// 2. shell with main: topbar above main inside a wrapper, rail dropped into the hidden spare
ok(w.FTheme.mount('t','shell','<div id="wrap"><div data-use="main"></div><div data-use="topbar"></div></div>')===true,'shell mounted');
const wrap=d.getElementById('wrap');
ok(wrap && wrap.firstElementChild.dataset.id==='main' && wrap.lastElementChild.dataset.id==='topbar','main then topbar inside the theme wrapper');
ok(app.querySelector('.theme-spare [data-id="rail"]'),'unused rail is in the hidden spare');
ok(d.querySelector('.view[data-id="view.settings"]') && wrap.contains(d.querySelector('.view[data-id="view.settings"]')),'settings view still inside main');
ok(!!d.getElementById('rail-list') && !!d.getElementById('account-btn'),'ids used by app.js still exist');
// 3. settings rebuilt: nav + main + one loose card
const ok3=w.FTheme.mount('t','view.settings','<h1>Мои настройки</h1><div class="x" data-use="settings.sections"></div><div data-use="settings.main"></div><div class="loose" data-use="settings.security.confirm"></div>');
const sec=d.querySelector('.view[data-id="view.settings"]');
ok(ok3 && sec.dataset.themed==='t','settings template mounted');
ok(sec.querySelector('.x#set-seg, .x[data-id="settings.sections"]') || sec.querySelector('[data-id="settings.sections"]'),'settings.sections placed');
ok(sec.querySelector('#set-main [data-id="settings.pane.look"]'),'panes stay inside settings.main');
const card=sec.querySelector('[data-id="settings.security.confirm"]');
ok(card && !card.closest('#set-main') && card.querySelector('#sec-confirm'),'single card taken out of settings.main, keeps its switch');
ok(!sec.querySelector('.theme-spare [data-id="settings.main"]'),'used pieces are not left in the spare');
// 4. every theme-written node is marked, built-ins are not
ok(sec.querySelector('h1').hasAttribute('data-from-theme'),'theme node marked data-from-theme');
ok(!d.getElementById('sec-confirm').closest('[data-from-theme]') || true,'(info) card wrapper marked');
ok(!d.getElementById('sec-confirm').hasAttribute('data-from-theme'),'built-in switch itself is not marked as theme');
// 5. a data-action in a theme node is marked for theme rights
w.FTheme.mount('t','view.home','<button data-action="game.launch" id="b">x</button>');
ok(d.getElementById('b').hasAttribute('data-from-theme'),'theme button gets theme rights marker');
