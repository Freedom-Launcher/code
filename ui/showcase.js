'use strict';
// Component showcase: interactive demos only, no core calls.
const { $, $$, toast, progress, initSegs } = window.FUI;

$('#sc-theme').addEventListener('seg:change', (e) => { document.documentElement.dataset.theme = e.detail.dataset.t; });
$('#sc-motion').addEventListener('change', (e) => { document.documentElement.dataset.motion = e.target.checked ? 'on' : 'off'; });

$$('[data-demo]').forEach((b) => b.addEventListener('click', () => {
  b.dataset.state = 'loading';
  setTimeout(() => { b.dataset.state = b.dataset.demo; setTimeout(() => { b.dataset.state = 'idle'; }, 1200); }, 900);
}));

$('#sc-toast').addEventListener('click', () => toast('Сборка создана'));
$('#sc-toast-err').addEventListener('click', () => toast('Не удалось скачать файлы: нет сети', { error: true }));

let mb = 0;
setInterval(() => {
  mb = mb >= 612 ? 0 : Math.min(612, mb + 37);
  progress($('#sc-bar'), mb / 612);
  $('#sc-pct').textContent = `${mb} / 612 МБ`;
}, 400);

$('#sc-load').addEventListener('click', () => {
  const box = $('#sc-skel');
  const rows = [['Fishing Horizons', '1.21.1'], ['Мини-игры', '1.20.4']];
  setTimeout(() => {
    box.replaceChildren(...rows.map(([n, v]) => {
      const r = document.createElement('div');
      r.className = 'row fade-in';
      r.innerHTML = '<span class="inst-ico"></span><div class="grow"><div class="title"></div><div class="sub mono"></div></div>';
      r.querySelector('.inst-ico').textContent = n[0];
      r.querySelector('.title').textContent = n;
      r.querySelector('.sub').textContent = v;
      return r;
    }));
  }, 700);
});

document.addEventListener('DOMContentLoaded', () => initSegs());