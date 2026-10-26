'use strict';
// Lists logs/latest.log and crash reports of a chosen instance and uploads one to mclo.gs.
const $ = (id) => document.getElementById(id);
const MAX = 900000;

function say(text, err) {
  const m = $('msg');
  m.hidden = !text;
  m.textContent = text || '';
  m.classList.toggle('is-err', !!err);
}

function row(inst, path, label, crash) {
  const li = document.createElement('li');
  li.className = `row${crash ? ' is-crash' : ''}`;
  const g = document.createElement('div');
  g.className = 'grow';
  const t = document.createElement('div');
  t.className = 't';
  t.textContent = label;
  const s = document.createElement('div');
  s.className = 's';
  s.textContent = path;
  g.append(t, s);
  const b = document.createElement('button');
  b.type = 'button';
  b.textContent = 'Загрузить';
  b.onclick = () => upload(inst, path, li, b);
  li.append(g, b);
  return li;
}

async function upload(inst, path, li, b) {
  b.disabled = true;
  b.textContent = 'Загрузка…';
  try {
    let text = await fl.files.read(inst, path);
    if (text.length > MAX) text = text.slice(-MAX);
    const r = await fl.fetch('https://api.mclo.gs/1/log', {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: 'content=' + encodeURIComponent(text),
    });
    const j = r.json();
    if (!j.success) throw new Error(j.error || `mclo.gs: ${r.status}`);
    const link = document.createElement('input');
    link.className = 'link';
    link.readOnly = true;
    link.value = j.url;
    link.onfocus = () => link.select();
    b.replaceWith(link);
    link.focus();
    fl.toast('Ссылка готова — скопируйте её');
  } catch (e) {
    b.disabled = false;
    b.textContent = 'Загрузить';
    say(String(e.message || e), true);
  }
}

async function load(inst) {
  say('');
  const list = $('list');
  list.replaceChildren();
  const rows = [];
  try {
    const logs = await fl.files.list(inst, 'logs').catch(() => []);
    if (logs.some((f) => f.name === 'latest.log')) rows.push(row(inst, 'logs/latest.log', 'Последний запуск', false));
    const crashes = await fl.files.list(inst, 'crash-reports').catch(() => []);
    crashes.filter((f) => !f.dir && f.name.endsWith('.txt')).sort((a, b) => b.name.localeCompare(a.name)).slice(0, 8)
      .forEach((f) => rows.push(row(inst, `crash-reports/${f.name}`, `Вылет ${f.name.replace(/^crash-|-(client|server)\.txt$/g, '')}`, true)));
  } catch (e) { say(String(e.message || e), true); return; }
  list.append(...rows);
  if (!rows.length) say('В этой сборке пока нет логов. Запустите игру хотя бы раз.');
}

async function start() {
  try {
    const all = await fl.call('instances.list');
    const sel = $('inst');
    sel.replaceChildren(...all.map((i) => new Option(i.name, i.id)));
    if (!all.length) { say('Сборок пока нет.'); return; }
    sel.onchange = () => load(sel.value);
    load(sel.value);
  } catch (e) { say(String(e.message || e), true); }
}
start();