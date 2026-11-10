// Sign-in through a Yggdrasil service (Ely.by, LittleSkin, a self-hosted authlib-injector server...).
// The password goes from this page to the service through fl.fetch and nowhere else. The token goes
// to the core (accounts.provider.save) and never comes back: the core validates and refreshes it
// before each launch, so the player signs in here only when the service refuses.
(function () {
  const P = window.PROVIDER;
  const L = {
    ru: { title: (s) => `Аккаунты ${s}`, pass: 'Пароль', code: 'Код из приложения (2FA)', go: 'Войти', busy: 'Входим…',
      none: 'Пока нет аккаунтов', remove: 'Убрать', ok: (n) => `Готово: ${n}`, bad: 'Неверный логин или пароль',
      need2fa: 'Аккаунт защищён двухфакторной защитой: введите код.', bad2fa: 'Неверный пароль или код',
      noprofile: 'У аккаунта нет игрового профиля', net: 'Не удалось связаться с сервисом', fail: 'Сервис ответил ошибкой' },
    en: { title: (s) => `${s} accounts`, pass: 'Password', code: 'Code from your app (2FA)', go: 'Sign in', busy: 'Signing in…',
      none: 'No accounts yet', remove: 'Remove', ok: (n) => `Done: ${n}`, bad: 'Wrong login or password',
      need2fa: 'This account uses two-factor authentication: enter the code.', bad2fa: 'Wrong password or code',
      noprofile: 'This account has no game profile', net: 'Could not reach the service', fail: 'The service returned an error' },
  };
  const T = L[fl.lang] || L.ru;
  const lang = L[fl.lang] ? fl.lang : 'ru';
  const $ = (id) => document.getElementById(id);
  const msg = (text, err) => { $('msg').textContent = text || ''; $('msg').className = err ? 'err' : ''; fl.resize && fl.resize(); };

  $('title').textContent = T.title(P.label);
  $('l-login').textContent = P.loginLabel[lang] || P.loginLabel.ru;
  $('l-pass').textContent = T.pass;
  $('l-code').textContent = T.code;
  $('go').textContent = T.go;

  const hex = (n) => Array.from(crypto.getRandomValues(new Uint8Array(n)), (b) => b.toString(16).padStart(2, '0')).join('');
  const dashed = (id) => id.length === 32 ? `${id.slice(0, 8)}-${id.slice(8, 12)}-${id.slice(12, 16)}-${id.slice(16, 20)}-${id.slice(20)}` : id;

  async function render() {
    const r = await fl.call('accounts.provider.list').catch(() => ({ accounts: [] }));
    const ul = $('accounts');
    ul.textContent = '';
    if (!r.accounts.length) { const li = document.createElement('li'); li.textContent = T.none; ul.append(li); }
    for (const a of r.accounts) {
      const li = document.createElement('li');
      const name = document.createElement('span'); name.textContent = a.name;
      const rm = document.createElement('button'); rm.className = 'ghost'; rm.textContent = T.remove;
      rm.onclick = async () => { await fl.call('accounts.provider.remove', { id: a.id }); render(); };
      li.append(name, rm); ul.append(li);
    }
    fl.resize && fl.resize();
  }

  // Which sign-in step failed, in words the player understands.
  async function authenticate(login, password) {
    let res;
    try {
      res = await fl.fetch(`${P.authApi}/authenticate`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ username: login, password, clientToken: hex(16), requestUser: false,
          agent: { name: 'Minecraft', version: 1 } }),
      });
    } catch (e) { return { error: 'net' }; }
    let data = null;
    try { data = await res.json(); } catch (e) { /* not JSON */ }
    if (res.status === 200 && data && data.accessToken) return { data };
    const text = String((data && data.errorMessage) || '');
    if (res.status === 401 && /two.?factor/i.test(text)) return { error: '2fa' };
    if (res.status === 401 || res.status === 403) return { error: 'bad' };
    return { error: 'fail' };
  }

  let need2fa = false;
  $('form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const login = $('login').value.trim();
    let password = $('pass').value;
    const code = $('code').value.trim();
    if (need2fa && P.twoFactor === 'colon' && code) password = `${password}:${code}`;
    $('go').disabled = true; msg(T.busy);
    const r = await authenticate(login, password);
    password = ''; // not kept
    $('go').disabled = false;
    if (r.error === '2fa' && P.twoFactor === 'colon') {
      need2fa = true; $('code-row').hidden = false; $('code').focus(); return msg(T.need2fa);
    }
    if (r.error) return msg(r.error === 'bad' && need2fa ? T.bad2fa : T[r.error] || T.fail, true);
    const d = r.data, p = d.selectedProfile;
    if (!p || !p.id || !p.name) return msg(T.noprofile, true);
    try {
      await fl.call('accounts.provider.save', {
        name: p.name, uuid: dashed(p.id), token: d.accessToken,
        expires_at: Math.floor(Date.now() / 1000) + P.sessionHours * 3600,
        client_token: d.clientToken,
      });
    } catch (err) { return msg(String(err && err.message || err), true); }
    $('pass').value = ''; $('code').value = ''; $('code-row').hidden = true; need2fa = false;
    msg(T.ok(p.name)); render();
  });

  render();
})();
