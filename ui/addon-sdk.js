// Freedom Launcher addon SDK. Served to every addon as `fl-sdk.js`.
// An addon runs in a sandboxed iframe with no Tauri access; this is its only
// way to the launcher. Every call is checked by the core against the
// permissions the user approved for this addon.
(() => {
  'use strict';
  let seq = 0;
  const waiting = new Map();
  const post = (m) => window.parent.postMessage(Object.assign({ fl: 1 }, m), '*');
  window.addEventListener('message', (e) => {
    if (e.source !== window.parent || !e.data || e.data.fl !== 1) return;
    const m = e.data;
    if (m.type === 'result' && waiting.has(m.seq)) {
      const w = waiting.get(m.seq);
      waiting.delete(m.seq);
      m.ok ? w.resolve(m.value) : w.reject(new Error(m.error || 'error'));
    } else if (m.type === 'theme') {
      for (const [k, v] of Object.entries(m.tokens || {})) document.documentElement.style.setProperty(k, v);
      document.documentElement.dataset.theme = m.base || 'dark';
    }
  });
  const resize = () => post({ type: 'resize', height: Math.ceil(document.documentElement.scrollHeight) });
  window.addEventListener('load', () => {
    resize();
    if (window.ResizeObserver) new ResizeObserver(resize).observe(document.documentElement);
  });
  window.fl = Object.freeze({
    /** Calls a launcher action, e.g. `fl.call('instances.list')`. */
    call(name, args) {
      return new Promise((resolve, reject) => {
        const s = ++seq;
        waiting.set(s, { resolve, reject });
        post({ type: 'call', seq: s, name: String(name), args: args === undefined ? null : args });
      });
    },
    /** Shows a short notification in the launcher. */
    toast(text) { post({ type: 'toast', text: String(text).slice(0, 200) }); },
    resize,
  });
})();