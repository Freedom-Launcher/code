'use strict';
// Minecraft text with § codes (MOTD) as DOM, the way the multiplayer list shows it.
window.MOTD = (() => {
  const COLORS = {
    0: '#000000', 1: '#0000AA', 2: '#00AA00', 3: '#00AAAA', 4: '#AA0000', 5: '#AA00AA', 6: '#FFAA00', 7: '#AAAAAA',
    8: '#555555', 9: '#5555FF', a: '#55FF55', b: '#55FFFF', c: '#FF5555', d: '#FF55FF', e: '#FFFF55', f: '#FFFFFF',
  };
  const STYLE = { l: 'b', m: 's', n: 'u', o: 'i', k: 'k' };
  const NAMES = [
    ['0', 'Чёрный'], ['1', 'Тёмно-синий'], ['2', 'Тёмно-зелёный'], ['3', 'Бирюзовый'], ['4', 'Тёмно-красный'],
    ['5', 'Фиолетовый'], ['6', 'Золотой'], ['7', 'Серый'], ['8', 'Тёмно-серый'], ['9', 'Синий'], ['a', 'Зелёный'],
    ['b', 'Голубой'], ['c', 'Красный'], ['d', 'Розовый'], ['e', 'Жёлтый'], ['f', 'Белый'],
  ];

  /** Lines of spans for `text`; `\n` (real or escaped) starts the second line. */
  function render(text, base = '#AAAAAA') {
    const box = document.createElement('div');
    box.className = 'motd';
    const lines = String(text ?? '').replace(/\\n/g, '\n').split('\n').slice(0, 2);
    for (const line of lines) {
      const row = document.createElement('div');
      row.className = 'motd-line';
      let color = base;
      let fx = new Set();
      let buf = '';
      const flush = () => {
        if (!buf) return;
        const s = document.createElement('span');
        s.textContent = buf;
        s.style.color = color;
        for (const f of fx) s.classList.add(`mf-${f}`);
        row.append(s);
        buf = '';
      };
      for (let i = 0; i < line.length; i++) {
        const ch = line[i];
        if ((ch === '§' || ch === '&') && i + 1 < line.length) {
          const code = line[i + 1].toLowerCase();
          if (code in COLORS) { flush(); color = COLORS[code]; fx = new Set(); i++; continue; }
          if (code in STYLE) { flush(); fx.add(STYLE[code]); i++; continue; }
          if (code === 'r') { flush(); color = base; fx = new Set(); i++; continue; }
        }
        buf += ch;
      }
      flush();
      if (!row.childNodes.length) row.append('\u00a0');
      box.append(row);
    }
    return box;
  }

  /** Visible length without codes, for the 59-characters-per-line hint. */
  const plain = (t) => String(t ?? '').replace(/[§&][0-9a-fk-or]/gi, '');

  return { render, plain, COLORS, NAMES };
})();