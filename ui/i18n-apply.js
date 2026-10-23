// First deferred script: translates the static markup, fills the language picker and hands
// the strings the native side draws itself (tray menu, window titles, file dialogs) to Rust.
(() => {
  const d = window.FL_DICT;
  const root = document.documentElement;
  root.style.setProperty('--t-rec', JSON.stringify(_t('Рекомендуется')));

  if (d) {
    const ATTRS = ['title', 'placeholder', 'aria-label', 'data-tip', 'alt'];
    const walk = (top) => {
      const w = document.createTreeWalker(top, NodeFilter.SHOW_ELEMENT | NodeFilter.SHOW_TEXT);
      for (let n = w.currentNode; n; n = w.nextNode()) {
        if (n.nodeType === 3) {
          const v = n.nodeValue, k = v.trim().replace(/\s+/g, ' ');
          if (k && d[k]) n.nodeValue = v.replace(/^(\s*)[\s\S]*?(\s*)$/, (m, a, b) => a + d[k] + b);
        } else if (n.nodeType === 1) {
          for (const a of ATTRS) { const v = n.getAttribute(a); if (v && d[v]) n.setAttribute(a, d[v]); }
          if (n.tagName === 'TEMPLATE') walk(n.content);
        }
      }
    };
    walk(document.body || root);
  }

  const sel = document.getElementById('fx-lang');
  if (sel) {
    for (const [code, label] of FI18N.langs) {
      const o = document.createElement('option');
      o.value = code; o.textContent = label; o.selected = code === FI18N.lang;
      sel.append(o);
    }
    sel.addEventListener('change', () => FI18N.set(sel.value));
  }

  if (FI18N.lang !== 'ru') {
    const NATIVE = ['Открыть', 'Безопасные настройки', 'Выход', 'Вход через Microsoft', 'Файлы', 'Импорт сборки', 'Сборка Modrinth', 'Freedom Launcher — безопасные настройки'];
    const strings = Object.fromEntries(NATIVE.map((k) => [k, _t(k)]));
    window.__TAURI__?.core?.invoke?.('set_ui_strings', { strings })?.catch?.(() => {});
  }
})();