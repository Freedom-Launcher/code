/* Freedom Launcher — icons. Monochrome 16px line icons, plus a glyph and tint for every instance.
   Markup can ask for an icon without JS: <button data-icon="plus">…</button>. Themes may restyle .ico. */
(() => {
  const P = {
    plus: 'M8 3.5v9M3.5 8h9',
    arrow: 'M6 3.5 10.5 8 6 12.5',
    cube: 'M8 1.8 13.5 5v6L8 14.2 2.5 11V5zM2.5 5 8 8.2 13.5 5M8 8.2v6',
    clock: 'M8 2.5a5.5 5.5 0 1 0 0 11 5.5 5.5 0 0 0 0-11zM8 5v3.2l2 1.3',
    tag: 'M2.5 3.5v4l6 6 5-5-6-6h-4a1 1 0 0 0-1 1zM5.3 5.3h.01',
    layers: 'M8 2.5 13.5 5.5 8 8.5 2.5 5.5zM2.5 8 8 11l5.5-3M2.5 10.5 8 13.5l5.5-3',
    globe: 'M8 2.5a5.5 5.5 0 1 0 0 11 5.5 5.5 0 0 0 0-11zM2.5 8h11M8 2.5c-1.6 1.6-2.3 3.4-2.3 5.5s.7 3.9 2.3 5.5c1.6-1.6 2.3-3.4 2.3-5.5S9.6 4.1 8 2.5z',
    terminal: 'M3 3.5h10a.5.5 0 0 1 .5.5v8a.5.5 0 0 1-.5.5H3a.5.5 0 0 1-.5-.5V4a.5.5 0 0 1 .5-.5zM5 6.5 7 8l-2 1.5M8.5 10h2.5',
    calendar: 'M3.5 3.5h9a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1v-8a1 1 0 0 1 1-1zM2.5 6.5h11M5.5 2v3M10.5 2v3',
    folder: 'M2.5 4.5a1 1 0 0 1 1-1h3l1.5 1.5h4.5a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1z',
    // instance glyphs
    flame: 'M8 13.8c-2.6 0-4.3-1.8-4.3-4.1 0-2.8 2.7-4.4 3.2-7.3 1.9 1.4 2.4 3.3 2.1 4.8.8-.4 1.3-1.1 1.5-1.9 1.2 1.1 1.8 2.8 1.8 4.4 0 2.3-1.7 4.1-4.3 4.1z',
    tree: 'M8 1.8 4.2 6.8h2.3l-2.8 3.7h8.6L9.5 6.8h2.3zM8 10.5v3.7',
    house: 'M2.5 7.5 8 3l5.5 4.5M4 6.5V13h8V6.5M6.6 13V9.6h2.8V13',
    island: 'M2.5 8.5h11l-2.2 3.2H4.7zM8 8.5V3M8 3c1.8 0 3.2.9 3.8 2.3H8',
    brush: 'M13.3 2.7 7.4 8.6M7.4 8.6c-1.4-.5-3 .4-3 2.1 0 1.2-.6 1.9-1.6 2.3 2.6.9 5.5.1 5.6-2.6',
    sword: 'M13.5 2.5h-3L4.5 8.5l3 3 6-6zM3 13l2.5-2.5M3.8 9.8l2.4 2.4',
    hook: 'M10 2.5v6.2a3 3 0 0 1-6 0V7.5M4 7.5l1.6 1.4M10 2.5h1.5',
    gem: 'M4.5 3h7l2.5 3.2L8 13.5 2 6.2zM2 6.2h12M6.5 3 5.5 6.2 8 13.5l2.5-7.3-1-3.2',
    bolt: 'M9 1.8 3.8 9h3.6L6.8 14.2 12.2 7H8.6z',
    star: 'M8 2.2l1.8 3.7 4 .6-2.9 2.8.7 4-3.6-1.9-3.6 1.9.7-4-2.9-2.8 4-.6z',
  };
  const svg = (n) => `<svg class="ico" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="${P[n] || P.cube}"/></svg>`;

  // A name-derived glyph keeps instances recognisable at a glance; the tint is muted, never the accent.
  const KEYS = [[/хард|hard/i, 'flame'], [/выжив|surviv/i, 'tree'], [/скай|sky/i, 'island'], [/творч|creat|build/i, 'brush'],
    [/pvp|пвп|бой/i, 'sword'], [/fish|рыб/i, 'hook'], [/дом|town|город/i, 'house'], [/тех|tech|мод/i, 'bolt']];
  const POOL = ['gem', 'star', 'bolt', 'tree', 'house', 'island'];
  const HUES = ['#8ab4ff', '#f0a35e', '#e8c45a', '#ef7f7f', '#6fcfc4', '#e98fb9', '#9fb8d6'];
  const hash = (s) => [...s].reduce((h, c) => (h * 31 + c.charCodeAt(0)) >>> 0, 7);
  function look(i) {
    const h = hash(i.id || i.name);
    const k = KEYS.find(([re]) => re.test(i.name));
    return { glyph: i.icon || (k ? k[1] : POOL[h % POOL.length]), hue: HUES[h % HUES.length] };
  }
  function paint(node, i) {
    if (i.icon_url) { node.innerHTML = ''; const im = new Image(); im.src = i.icon_url; im.alt = ''; node.append(im); return node; }
    const { glyph, hue } = look(i);
    node.classList.add('glyph');
    node.style.setProperty('--ic', hue);
    node.innerHTML = svg(glyph);
    return node;
  }
  function decorate(root = document) {
    root.querySelectorAll('[data-icon]:not([data-iconed])').forEach((n) => {
      n.dataset.iconed = '';
      n.insertAdjacentHTML(n.dataset.iconAt === 'end' ? 'beforeend' : 'afterbegin', svg(n.dataset.icon));
    });
  }
  window.FI = { svg, paint, decorate, look };
  document.addEventListener('DOMContentLoaded', () => decorate());
})();