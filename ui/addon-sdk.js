// Freedom Launcher addon SDK. Served to every addon as `fl-sdk.js`.
// An addon runs in a sandboxed iframe with no Tauri access; this is its only
// way to the launcher. Every call is checked by the core against the
// permissions the user approved for this addon.
(() => {
  'use strict';
  let seq = 0;
  let context = {};
  const waiting = new Map();
  const handlers = new Map();
  const post = (m) => window.parent.postMessage(Object.assign({ fl: 1 }, m), '*');
  const fire = (name, data) => {
    for (const cb of [...(handlers.get(name) || []), ...(handlers.get('*') || [])]) {
      try { cb(data, name); } catch (err) { console.error(err); }
    }
  };
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
    } else if (m.type === 'context') {
      const { fl: _f, type: _t, ...rest } = m;
      context = Object.freeze(rest);
      fire('context', context);
    } else if (m.type === 'event' && typeof m.name === 'string') {
      fire(m.name, m.data);
    }
  });
  const resize = () => post({ type: 'resize', height: Math.ceil(document.documentElement.scrollHeight) });
  window.addEventListener('load', () => {
    resize();
    if (window.ResizeObserver) new ResizeObserver(resize).observe(document.documentElement);
  });
  const call = (name, args) => new Promise((resolve, reject) => {
    const s = ++seq;
    waiting.set(s, { resolve, reject });
    post({ type: 'call', seq: s, name: String(name), args: args === undefined ? null : args });
  });
  window.fl = Object.freeze({
    /** Calls a launcher action, e.g. `fl.call('instances.list')`. */
    call,
    /**
     * HTTPS request through the launcher, only to hosts from `hosts` in
     * addon.json; needs the `network` permission. Resolves to
     * `{status, ok, headers, body}`; `json()` parses the body.
     */
    async fetch(url, opts = {}) {
      const body = opts.body === undefined || typeof opts.body === 'string' ? opts.body : JSON.stringify(opts.body);
      const r = await call('net.fetch', {
        url: String(url), method: opts.method || 'GET', headers: opts.headers || {}, body,
      });
      return Object.assign(r, { json: () => JSON.parse(r.body) });
    },
    /**
     * Launcher events: `game.launching`, `game.launched`, `game.failed`,
     * `game.exited` (data `{instance}`), `context`, or `*` for all.
     * Needs `instances_read` or `ui_overlay`. Returns an unsubscribe function.
     */
    on(name, cb) {
      const set = handlers.get(name) || new Set();
      set.add(cb);
      handlers.set(name, set);
      return () => set.delete(cb);
    },
    /** Where this frame is: `{kind: 'slot'|'page'|'overlay', slot?, page?, instance?}`. */
    get context() { return context; },
    /** Own key/value storage, needs `storage`. Values are any JSON. */
    storage: Object.freeze({
      get: (key) => call('storage.get', { key: String(key) }).then((r) => r.value),
      set: (key, value) => call('storage.set', { key: String(key), value }),
      delete: (key) => call('storage.delete', { key: String(key) }),
      keys: () => call('storage.list').then((r) => r.keys),
    }),
    /** Text files inside an instance folder, needs `instance_files`. */
    files: Object.freeze({
      list: (instance, path = '') => call('files.list', { instance, path }),
      read: (instance, path) => call('files.read', { instance, path }).then((r) => r.text),
      write: (instance, path, text) => call('files.write', { instance, path, text: String(text) }),
      delete: (instance, path) => call('files.delete', { instance, path }),
    }),
    /** In a launch overlay: the animation is over, remove it now. */
    overlay: Object.freeze({ done: () => post({ type: 'overlay-done' }) }),
    /** Shows a short notification in the launcher. */
    toast(text) { post({ type: 'toast', text: String(text).slice(0, 200) }); },
    resize,
  });
})();