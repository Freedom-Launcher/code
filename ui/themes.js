'use strict';
// Theme engine (M2). Themes are data: tokens, CSS and screen templates. No theme code ever runs:
// the core checks every file, the CSP blocks script, and templates are cleaned again here.
(() => {
  const root = document.documentElement;
  let applied = [];
  const isWin = navigator.userAgent.includes('Windows');
  const base = (id) => (isWin ? `http://theme.localhost/${id}/` : `theme://localhost/${id}/`);
  const BAD_TAGS = 'script,iframe,frame,object,embed,link,meta,base,style,form,portal,foreignObject,animate,set';

  // Tokens and CSS: safe to switch live.
  function apply(cur) {
    root.dataset.theme = cur.base === 'light' ? 'light' : 'dark';
    root.dataset.themeId = cur.id;
    for (const k of applied) root.style.removeProperty(k);
    applied = Object.keys(cur.tokens || {});
    for (const [k, v] of Object.entries(cur.tokens || {})) root.style.setProperty(k, v);
    document.querySelectorAll('link[data-theme-css]').forEach((n) => n.remove());
    for (const c of cur.css || []) {
      const l = document.createElement('link');
      l.rel = 'stylesheet';
      l.href = base(cur.id) + c;
      l.dataset.themeCss = '';
      document.head.append(l);
    }
  }

  // A template replaces a screen's markup. Built-in pieces are pulled in with
  // <div data-use="<data-id>"></div>, so they keep their behaviour; pieces the template
  // does not use stay in a hidden spare box. Everything the theme itself wrote is marked
  // data-from-theme, and its data-action clicks run with theme rights (bindable actions only).
  function mount(id, screen, html) {
    const sec = document.querySelector(`.view[data-id="${CSS.escape(screen)}"]`);
    if (!sec || html == null) return false;
    const doc = new DOMParser().parseFromString(`<!doctype html><body>${html}`, 'text/html');
    doc.body.querySelectorAll(BAD_TAGS).forEach((n) => n.remove());
    for (const n of doc.body.querySelectorAll('*')) {
      for (const a of [...n.attributes]) {
        if (/^on/i.test(a.name) || a.name === 'style' || a.name === 'data-from-theme') n.removeAttribute(a.name);
      }
      for (const k of ['src', 'href']) {
        const v = n.getAttribute(k);
        if (v && !v.startsWith('#')) {
          if (/^[a-z][a-z0-9+.-]*:/i.test(v) || v.startsWith('/') || v.includes('..')) n.removeAttribute(k);
          else n.setAttribute(k, base(id) + v);
        }
      }
      n.setAttribute('data-from-theme', '');
    }
    const spare = document.createElement('div');
    spare.className = 'theme-spare';
    spare.hidden = true;
    spare.append(...sec.childNodes);
    for (const ph of doc.body.querySelectorAll('[data-use]')) {
      const orig = spare.querySelector(`[data-id="${CSS.escape(ph.getAttribute('data-use'))}"]`);
      if (orig) ph.replaceWith(orig);
      else ph.remove();
    }
    sec.append(...doc.body.childNodes, spare);
    sec.dataset.themed = id;
    return true;
  }

  window.FTheme = { apply, mount, base };
})();