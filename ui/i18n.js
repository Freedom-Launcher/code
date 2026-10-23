// Interface language. Loaded before every other script (not deferred): picks the language,
// pulls in its dictionary and exposes _t() / _tp() / _tr() / _LOC to the rest of the UI.
// Russian is the source language: dictionaries map the Russian text to the translation,
// and anything missing falls back to Russian.
(() => {
  const LANGS = [
    ['ru', 'Русский'], ['en', 'English'], ['uk', 'Українська'], ['es', 'Español'], ['de', 'Deutsch'],
    ['it', 'Italiano'], ['fr', 'Français'], ['pl', 'Polski'], ['tr', 'Türkçe'], ['pt', 'Português (Brasil)'],
  ];
  const CODES = LANGS.map((l) => l[0]);
  const ALIAS = { ua: 'uk', be: 'ru', kk: 'ru', ky: 'ru' };
  const KEY = 'fl.lang';

  function pick() {
    let saved = null;
    try { saved = localStorage.getItem(KEY); } catch { /* no storage */ }
    if (saved && CODES.includes(saved)) return saved;
    let lang = 'en', used = false;
    // People who used the launcher before it had translations keep Russian.
    try { for (let i = 0; i < localStorage.length; i++) if (String(localStorage.key(i)).startsWith('fl.')) { used = true; break; } } catch { /* no storage */ }
    if (used) lang = 'ru';
    else {
      for (const l of navigator.languages || [navigator.language || '']) {
        const c = String(l).slice(0, 2).toLowerCase(), m = ALIAS[c] || c;
        if (CODES.includes(m)) { lang = m; break; }
      }
    }
    try { localStorage.setItem(KEY, lang); } catch { /* no storage */ }
    return lang;
  }

  const lang = pick();
  const loc = lang === 'pt' ? 'pt-BR' : lang;
  document.documentElement.lang = loc;
  if (lang !== 'ru') document.write(`<script src="i18n/${lang}.js"><\/script>`);

  const fill = (s, a) => (a && a.length ? s.replace(/\{(\d+)\}/g, (m, i) => (i < a.length ? String(a[i]) : m)) : s);
  const dict = () => window.FL_DICT || null;

  // Static text: _t('Русский текст') or, for text with values in it, _t('Осталось {0}', [n]).
  window._t = (k, a) => { const d = dict(), v = d && d[k]; return fill(typeof v === 'string' && v ? v : k, a); };

  // Plurals keep the Russian three forms as the key; other languages pick by CLDR category.
  const ru = (n, a, b, c) => { const m = n % 10, h = n % 100; return m === 1 && h !== 11 ? a : m >= 2 && m <= 4 && (h < 12 || h > 14) ? b : c; };
  let rules = null;
  window._tp = (n, a, b, c) => {
    const f = lang !== 'ru' && window.FL_PLURALS && window.FL_PLURALS[`${a}|${b}|${c}`];
    if (!f) return ru(Math.abs(Math.trunc(Number(n) || 0)), a, b, c);
    rules ||= new Intl.PluralRules(loc);
    return f[rules.select(Number(n) || 0)] || f.other || f.one || a;
  };

  // Text that arrives at run time (errors from the core): exact match, then the patterned keys.
  const CYR = /[\u0400-\u04FF]/;
  let pats = null;
  const esc = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  window._tr = (s) => {
    const d = dict();
    if (!d || typeof s !== 'string' || !CYR.test(s)) return s;
    if (d[s]) return d[s];
    pats ||= Object.keys(d).filter((k) => /\{\d+\}/.test(k)).sort((x, y) => y.length - x.length).map((k) => {
      const order = [];
      const re = new RegExp('^' + k.split(/(\{\d+\})/).map((p) => { const m = /^\{(\d+)\}$/.exec(p); if (m) { order.push(+m[1]); return '([\\s\\S]+?)'; } return esc(p); }).join('') + '$');
      return [re, order, k];
    });
    for (const [re, order, k] of pats) {
      const m = re.exec(s);
      if (!m) continue;
      const a = []; order.forEach((i, j) => { a[i] = m[j + 1]; });
      return fill(d[k], a);
    }
    return s;
  };

  window._LOC = loc;
  window.FI18N = {
    lang, loc, langs: LANGS,
    set(l) { if (!CODES.includes(l) || l === lang) return; try { localStorage.setItem(KEY, l); } catch { /* no storage */ } location.reload(); },
  };
})();