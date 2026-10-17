// Project descriptions from Modrinth: Markdown mixed with HTML, as Modrinth
// renders it. The text is turned into HTML, parsed inert, and rebuilt from an
// allowlist into fresh nodes: no scripts, no styles, no event handlers, images
// from cdn.modrinth.com directly, others through the core (`rimg:`). Links never navigate
// the window; the caller decides what a click does.
(() => {
  const esc = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  const attr = (s) => esc(s).replace(/"/g, '&quot;');

  function inline(s) {
    const codes = [];
    s = s.replace(/`([^`]+)`/g, (_, c) => `\u0000${codes.push(esc(c)) - 1}\u0000`);
    s = s
      .replace(/!\[([^\]]*)\]\(\s*<?([^)\s>]+)>?(?:\s+"[^"]*")?\s*\)/g, (_, a, u) => `<img alt="${attr(a)}" src="${attr(u)}">`)
      .replace(/\[([^\]]+)\]\(\s*<?([^)\s>]+)>?(?:\s+"[^"]*")?\s*\)/g, (_, t, u) => `<a href="${attr(u)}">${t}</a>`)
      .replace(/(^|[\s(])(https?:\/\/[^\s<)]+)/g, (_, p, u) => `${p}<a href="${attr(u)}">${u}</a>`)
      .replace(/\*\*([^*]+)\*\*|__([^_]+)__/g, (_, a, b) => `<strong>${a ?? b}</strong>`)
      .replace(/(^|[^*\w])\*([^*\s][^*]*)\*/g, '$1<em>$2</em>')
      .replace(/(^|[^_\w/])_([^_\s][^_]*)_(?![\w/])/g, '$1<em>$2</em>')
      .replace(/~~([^~]+)~~/g, '<del>$1</del>');
    return s.replace(/\u0000(\d+)\u0000/g, (_, i) => `<code>${codes[i]}</code>`);
  }

  const cells = (row) => row.trim().replace(/^\||\|$/g, '').split('|').map((c) => inline(c.trim()));

  function toHtml(src) {
    const lines = String(src || '').replace(/\r\n?/g, '\n').split('\n');
    const out = [];
    let i = 0;
    while (i < lines.length) {
      const l = lines[i];
      let m;
      if (!l.trim()) { i++; continue; }
      if ((m = /^\s*(```|~~~)/.exec(l))) {
        const body = [];
        for (i++; i < lines.length && !lines[i].trim().startsWith(m[1]); i++) body.push(lines[i]);
        i++;
        out.push(`<pre><code>${esc(body.join('\n'))}</code></pre>`);
      } else if ((m = /^(#{1,6})\s+(.*?)\s*#*$/.exec(l))) {
        out.push(`<h${m[1].length}>${inline(m[2])}</h${m[1].length}>`); i++;
      } else if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(l)) {
        out.push('<hr>'); i++;
      } else if (/^\s*>/.test(l)) {
        const body = [];
        for (; i < lines.length && /^\s*>/.test(lines[i]); i++) body.push(lines[i].replace(/^\s*>\s?/, ''));
        out.push(`<blockquote>${toHtml(body.join('\n'))}</blockquote>`);
      } else if (/^\s*([-*+]|\d+[.)])\s+/.test(l)) {
        const ordered = /^\s*\d/.test(l);
        const items = [];
        for (; i < lines.length && lines[i].trim(); i++) {
          const it = /^\s*(?:[-*+]|\d+[.)])\s+(.*)$/.exec(lines[i]);
          if (it) items.push(it[1]); else if (items.length) items[items.length - 1] += ' ' + lines[i].trim();
        }
        const tag = ordered ? 'ol' : 'ul';
        out.push(`<${tag}>${items.map((t) => `<li>${inline(t)}</li>`).join('')}</${tag}>`);
      } else if (/\|/.test(l) && /^\s*\|?\s*:?-{2,}/.test(lines[i + 1] || '')) {
        const head = cells(l);
        const rows = [];
        for (i += 2; i < lines.length && /\|/.test(lines[i]) && lines[i].trim(); i++) rows.push(cells(lines[i]));
        out.push(`<table><thead><tr>${head.map((c) => `<th>${c}</th>`).join('')}</tr></thead><tbody>${rows.map((r) => `<tr>${r.map((c) => `<td>${c}</td>`).join('')}</tr>`).join('')}</tbody></table>`);
      } else if (/^\s*</.test(l)) {
        const body = [];
        for (; i < lines.length && lines[i].trim(); i++) body.push(lines[i]);
        out.push(inline(body.join('\n')));
      } else {
        const body = [];
        for (; i < lines.length && lines[i].trim() && !/^(#{1,6}\s|\s*>|\s*([-*+]|\d+[.)])\s|\s*```|\s*<(p|div|img|h\d|center|details|table)\b)/i.test(lines[i]); i++) body.push(lines[i].trim());
        if (!body.length) { body.push(lines[i].trim()); i++; }
        out.push(`<p>${inline(body.join('\n')).replace(/\n/g, '<br>')}</p>`);
      }
    }
    return out.join('\n');
  }

  const KEEP = new Set('p br hr h1 h2 h3 h4 h5 h6 strong b em i u s del ins mark code pre blockquote ul ol li a img table thead tbody tr th td details summary sup sub kbd'.split(' '));
  const DROP = new Set('script style iframe object embed form input button textarea select svg math template noscript link meta video audio source canvas'.split(' '));
  const CDN = /^https:\/\/cdn\.modrinth\.com\/[^"'\\()\s]+$/;
  const SAFE_URL = /^https?:\/\/[^\s"'<>\\]+$/i;
  const RIMG = navigator.userAgent.includes('Windows') ? 'http://rimg.localhost' : 'rimg://localhost';

  function rebuild(node, onLink) {
    if (node.nodeType === Node.TEXT_NODE) return document.createTextNode(node.data);
    if (node.nodeType !== Node.ELEMENT_NODE) return null;
    const tag = node.localName;
    if (DROP.has(tag)) {
      const src = node.getAttribute('src') || '';
      if (tag !== 'iframe' || !SAFE_URL.test(src)) return null;
      return link(/youtube|youtu\.be/.test(src) ? 'Видео' : 'Встроенное содержимое', src.replace('/embed/', '/watch?v='), onLink, 'md-embed');
    }
    const kids = () => [...node.childNodes].map((c) => rebuild(c, onLink)).filter(Boolean);
    const center = tag === 'center' || (node.getAttribute('align') || '').toLowerCase() === 'center';
    if (!KEEP.has(tag)) {
      if (tag === 'div' || center) {
        const d = document.createElement('div');
        if (center) d.className = 'md-center';
        d.append(...kids());
        return d;
      }
      const f = document.createDocumentFragment();
      f.append(...kids());
      return f;
    }
    if (tag === 'img') {
      const src = node.getAttribute('src') || '';
      if (!CDN.test(src) && !SAFE_URL.test(src)) return null;
      const img = document.createElement('img');
      img.src = CDN.test(src) ? src : `${RIMG}/${encodeURIComponent(src)}`;
      img.alt = node.getAttribute('alt') || '';
      img.loading = 'lazy';
      img.decoding = 'async';
      for (const k of ['width', 'height']) {
        const v = (node.getAttribute(k) || '').trim();
        if (/^\d{1,4}(%|px)?$/.test(v)) img.setAttribute(k, v.replace('px', ''));
      }
      img.addEventListener('error', () => {
        if (img.alt && !img.closest('a')) img.replaceWith(link(img.alt, src, onLink, 'md-embed'));
        else img.remove();
      }, { once: true });
      return img;
    }
    const n = document.createElement(tag);
    if (center) n.className = 'md-center';
    if (tag === 'a') {
      const href = (node.getAttribute('href') || '').trim();
      n.append(...kids());
      if (SAFE_URL.test(href)) wire(n, href, onLink);
      return n;
    }
    if (tag === 'details' && node.hasAttribute('open')) n.open = true;
    if ((tag === 'td' || tag === 'th') && node.getAttribute('colspan')) n.colSpan = Math.min(20, +node.getAttribute('colspan') || 1);
    n.append(...kids());
    return n;
  }

  function wire(a, href, onLink) {
    a.href = href;
    a.title = href;
    a.addEventListener('click', (e) => { e.preventDefault(); onLink?.(href); });
  }
  function link(text, href, onLink, cls) {
    const a = document.createElement('a');
    a.className = cls;
    a.textContent = text;
    wire(a, href, onLink);
    return a;
  }

  // A detached element with the rendered text; `onLink(url)` handles clicks.
  function render(src, onLink) {
    const doc = new DOMParser().parseFromString(`<body>${toHtml(src)}</body>`, 'text/html');
    const root = document.createElement('div');
    root.className = 'md';
    root.append(...[...doc.body.childNodes].map((c) => rebuild(c, onLink)).filter(Boolean));
    return root;
  }

  window.FMarkdown = { render, toHtml };
})();