'use strict';
// One note per instance in the plugin's own storage (key `n:<instance id>`), saved as you type.
const $ = (id) => document.getElementById(id);
let inst = '', timer = 0;
const key = (id) => `n:${id}`;

async function save() {
  clearTimeout(timer);
  timer = 0;
  if (!inst) return;
  try {
    const v = $('txt').value;
    if (v) await fl.storage.set(key(inst), v); else await fl.storage.delete(key(inst));
    $('st').textContent = 'Сохранено';
  } catch (e) { $('st').textContent = String(e.message || e); }
}

async function open(id) {
  if (timer) await save();
  inst = id;
  $('st').textContent = '';
  const v = await fl.storage.get(key(id)).catch(() => null);
  $('txt').value = typeof v === 'string' ? v : '';
}

async function start() {
  const t = $('txt');
  t.addEventListener('input', () => { $('st').textContent = '…'; clearTimeout(timer); timer = setTimeout(save, 600); });
  t.addEventListener('blur', () => { if (timer) save(); });
  try {
    const all = await fl.call('instances.list');
    const sel = $('inst');
    sel.replaceChildren(...all.map((i) => new Option(i.name, i.id)));
    t.disabled = !all.length;
    if (!all.length) { $('st').textContent = 'Сборок пока нет'; return; }
    sel.onchange = () => open(sel.value);
    open(sel.value);
  } catch (e) { $('st').textContent = String(e.message || e); }
}
start();