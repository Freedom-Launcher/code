// Behaviour of the Yggdrasil sign-in plugin template against a fake `fl` (jsdom).
// Run: npm i --no-save jsdom && node scripts/yggdrasil-plugin-test.js [plugin-folder]
const { JSDOM } = require('jsdom'); const fs = require('fs'); const path = require('path');
const dir = path.resolve(process.argv[2] || path.join(__dirname, '..', 'docs', 'addons', 'elyby'));
let bad = 0;
const ok = (c, m) => { console.log((c ? 'ok   ' : 'FAIL ') + m); if (!c) bad = 1; };
const tick = () => new Promise((r) => setTimeout(r, 10));

async function run(script, accounts = []) {
  const css = fs.readFileSync(path.join(dir, 'style.css'), 'utf8');
  const html = fs.readFileSync(path.join(dir, 'index.html'), 'utf8').replace(/<script[\s\S]*?<\/script>/g, '').replace(/<link[^>]*stylesheet[^>]*>/, `<style>${css}</style>`);
  const dom = new JSDOM(html, { runScripts: 'outside-only' });
  const w = dom.window; const calls = []; const fetches = []; w.__accounts = accounts;
  w.crypto = require('crypto').webcrypto;
  w.fl = {
    lang: 'ru', resize() {},
    async call(a, args) { calls.push([a, args]); if (a === 'accounts.provider.list') return { accounts: w.__accounts || [] }; if (w.__saveFails && a === 'accounts.provider.save') throw new Error('core says no'); return { accounts: [] }; },
    async fetch(url, o) { fetches.push([url, o]); const r = script.shift(); if (r instanceof Error) throw r; return { status: r.status, ok: r.status === 200, async json() { if (r.body === undefined) throw new Error('no json'); return r.body; } }; },
  };
  w.eval(fs.readFileSync(path.join(dir, 'config.js'), 'utf8'));
  w.eval(fs.readFileSync(path.join(dir, 'main.js'), 'utf8'));
  await tick();
  const d = w.document;
  // What a person does: types, then clicks «Войти» (or presses Enter). The frame is sandboxed without
  // allow-forms in the launcher, so a real <form> submit would never happen; the page must not rely on one.
  const submit = async (login, pass, code, how = 'click') => {
    d.getElementById('login').value = login; d.getElementById('pass').value = pass;
    if (code !== undefined) d.getElementById('code').value = code;
    if (how === 'enter') d.getElementById('pass').dispatchEvent(new w.KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
    else d.getElementById('go').click();
    await tick(); await tick();
  };
  return { w, d, calls, fetches, submit, visible: (id) => w.getComputedStyle(d.getElementById(id)).display !== 'none', msg: () => d.getElementById('msg').textContent, saves: () => calls.filter((c) => c[0] === 'accounts.provider.save') };
}
const good = { status: 200, body: { accessToken: 'TOK', clientToken: 'CLI', selectedProfile: { id: '0f1e2d3c4b5a49788a695a4b3c2d1e0f', name: 'Steve' } } };

// The page may only use theme variables the launcher really passes to plugin frames (ui/addons.js TOKENS);
// anything else silently falls back to a fixed colour, which is what made the first version look wrong.
const passed = (fs.readFileSync(path.join(__dirname, '..', 'ui', 'addons.js'), 'utf8').match(/const TOKENS = \[([^\]]*)\]/) || [, ''])[1].match(/--[a-z0-9-]+/g) || [];
const used = [...new Set((fs.readFileSync(path.join(dir, 'style.css'), 'utf8').match(/var\((--[a-z0-9-]+)/g) || []).map((v) => v.slice(4)))];
ok(passed.length > 5 && used.length > 3, 'the launcher\'s token list and the page\'s variables were both found');
ok(used.every((v) => passed.includes(v)), `style.css uses only variables the launcher passes (unknown: ${used.filter((v) => !passed.includes(v)).join(', ') || 'none'})`);

ok(!/<form[\s>]/i.test(fs.readFileSync(path.join(dir, 'index.html'), 'utf8')), 'the page has no <form> (a sandboxed plugin frame cannot submit one)');

(async () => {
  // 0. first look: the 2FA row must really be hidden until the service asks for a code
  let t0 = await run([]);
  ok(!t0.visible('code-row'), 'the 2FA row is not shown at first (computed style, not only the hidden attribute)');
  ok(t0.visible('login') && t0.visible('pass') && t0.visible('go'), 'login, password and the button are shown');
  // Enter in the password field signs in too
  let te = await run([good]); await te.submit('steve', 'pw', undefined, 'enter');
  ok(te.saves().length === 1, 'Enter in the password field signs in');
  // empty fields: a hint, no request
  let tf = await run([good]); await tf.submit('', '');
  ok(tf.fetches.length === 0 && /Введите/.test(tf.msg()), 'empty fields: a hint and no request to the service');
  // a double click does not send two requests
  let td = await run([good, good]); td.d.getElementById('login').value = 'a'; td.d.getElementById('pass').value = 'b';
  td.d.getElementById('go').click(); td.d.getElementById('go').click(); await tick(); await tick();
  ok(td.fetches.length === 1, 'clicking twice while signing in sends one request');

  // 1. plain success
  let t = await run([good]); const P = t.w.PROVIDER;
  await t.submit('steve@example.org', 'hunter2');
  const [url, opt] = t.fetches[0]; const sent = JSON.parse(opt.body);
  ok(url === `${P.authApi}/authenticate` && opt.method === 'POST', 'posts to <authApi>/authenticate');
  ok(/^https:\/\/[^/]+\/.+/.test(P.authApi) && new URL(P.authApi).host === new URL(P.apiRoot).host, 'the sign-in API is on the service\'s own host');
  ok(sent.username === 'steve@example.org' && sent.password === 'hunter2' && sent.requestUser === false, 'sends login and password, no user object');
  const save = t.saves()[0][1];
  ok(save.name === 'Steve' && save.uuid === '0f1e2d3c-4b5a-4978-8a69-5a4b3c2d1e0f', 'saves the name and a dashed uuid');
  ok(save.token === 'TOK' && save.client_token === 'CLI', 'saves token and client_token for the core to refresh');
  const dt = save.expires_at - Math.floor(Date.now() / 1000);
  ok(dt > 23 * 3600 && dt <= 24 * 3600, 'expires_at is about sessionHours ahead');
  ok(!JSON.stringify(t.calls).includes('hunter2') && !t.d.body.innerHTML.includes('hunter2'), 'the password reaches only the service, not the core or the page');
  ok(t.d.getElementById('pass').value === '', 'password field is cleared after success');

  // 2. wrong password
  t = await run([{ status: 401, body: { error: 'ForbiddenOperationException', errorMessage: 'Invalid credentials. Invalid email or password.' } }]);
  await t.submit('steve', 'nope');
  ok(t.saves().length === 0 && /Неверный/.test(t.msg()), 'wrong password: nothing saved, clear message');

  if (P.twoFactor === 'colon') {
    // 3. two-factor: ask for the code only when the service asks, then send password:code
    t = await run([{ status: 401, body: { errorMessage: 'Account protected with two factor auth.' } }, good]);
    await t.submit('steve', 'pw');
    ok(t.saves().length === 0 && t.visible('code-row'), '2FA required: the code field appears, nothing saved yet');
    await t.submit('steve', 'pw', '123456');
    ok(JSON.parse(t.fetches[1][1].body).password === 'pw:123456', 'the code is appended as password:code');
    ok(t.saves().length === 1 && !t.visible('code-row'), '2FA success: saved, code field hidden again');

    // 4. wrong code after 2FA
    t = await run([{ status: 401, body: { errorMessage: 'Account protected with two factor auth.' } }, { status: 401, body: { errorMessage: 'Invalid credentials. Invalid email or password.' } }]);
    await t.submit('steve', 'pw'); await t.submit('steve', 'pw', '000000');
    ok(t.saves().length === 0 && /код/.test(t.msg()), 'wrong code: message mentions the code');

  } else {
    // A service without 2FA support: a "two factor" answer is just a failed sign-in, no code field.
    t = await run([{ status: 401, body: { errorMessage: 'Account protected with two factor auth.' } }]);
    await t.submit('steve', 'pw');
    ok(t.saves().length === 0 && t.d.getElementById('code-row').hidden, 'twoFactor off: no code field, nothing saved');
  }

  // 5. account without a profile, offline service, odd answer
  t = await run([{ status: 200, body: { accessToken: 'T', clientToken: 'C' } }]); await t.submit('a', 'b');
  ok(t.saves().length === 0 && /профиля/.test(t.msg()), 'no game profile: refused');
  t = await run([new Error('offline')]); await t.submit('a', 'b');
  ok(t.saves().length === 0 && /связаться/.test(t.msg()), 'service unreachable: clear message');
  t = await run([{ status: 500 }]); await t.submit('a', 'b');
  ok(t.saves().length === 0 && /ошибкой/.test(t.msg()), 'server error without JSON: no crash, nothing saved');

  // 6. the core refuses the account (e.g. a name it does not allow): shown, password kept for a retry
  t = await run([good]); t.w.__saveFails = true; await t.submit('a', 'b');
  ok(/core says no/.test(t.msg()), 'a refusal from the core is shown to the player');

  // 7. list and remove
  t = await run([]);
  ok(/Пока нет/.test(t.d.getElementById('accounts').textContent), 'empty list is explained');
  t = await run([], [{ id: 'x1', name: 'Alex' }, { id: 'x2', name: 'Bob' }]);
  const rows = [...t.d.querySelectorAll('#accounts li')];
  ok(rows.length === 2 && rows[0].textContent.includes('Alex'), 'saved accounts are listed by name');
  rows[0].querySelector('button').click(); await tick();
  const rm = t.calls.find((c) => c[0] === 'accounts.provider.remove');
  ok(rm && rm[1].id === 'x1', 'the remove button removes exactly that account');
  // names from the service are shown as text, never as markup
  t = await run([], [{ id: 'x3', name: '<img src=x onerror=alert(1)>' }]);
  ok(!t.d.querySelector('#accounts img') && t.d.getElementById('accounts').textContent.includes('<img'), 'an account name is text, not HTML');

  console.log(bad ? '\nFAILED' : '\nall ok'); process.exit(bad);
})();
