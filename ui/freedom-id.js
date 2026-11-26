'use strict';
// Freedom ID — launcher account (separate from Minecraft accounts).
// Local-first placeholder: stores account on this device until backend auth is wired.
(() => {
  const KEY = 'fl.freedom_id';
  const $ = (s, r = document) => r.querySelector(s);
  const view = $('#view-freedom-id');
  if (!view) return;

  const modeSeg = $('#fid-mode');
  const form = $('#fid-form');
  const nickRow = $('#fid-nick-row');
  const nick = $('#fid-nick');
  const email = $('#fid-email');
  const pass = $('#fid-pass');
  const submit = $('#fid-submit');
  const msg = $('#fid-msg');
  const profile = $('#fid-profile');
  const card = $('#fid-card');
  const logout = $('#fid-logout');

  let mode = 'login';

  const read = () => {
    try { return JSON.parse(localStorage.getItem(KEY) || 'null'); } catch { return null; }
  };
  const write = (v) => {
    try { if (v) localStorage.setItem(KEY, JSON.stringify(v)); else localStorage.removeItem(KEY); } catch {}
  };
  const setMsg = (text = '', err = false) => {
    msg.textContent = text;
    msg.className = `fid-msg${err ? ' is-error' : text ? ' is-ok' : ''}`;
  };

  function applyI18n() {
    const login = mode === 'login';
    $('#fid-sub').textContent = _t('Единый аккаунт лаунчера: синхронизация и сервисы Freedom Launcher.');
    modeSeg.querySelector('[data-mode="login"]').textContent = _t('Вход');
    modeSeg.querySelector('[data-mode="signup"]').textContent = _t('Регистрация');
    $('label[for="fid-nick"]').textContent = _t('Ник');
    $('label[for="fid-email"]').textContent = _t('Почта');
    $('label[for="fid-pass"]').textContent = _t('Пароль');
    nick.placeholder = _t('Например, PlayerOne');
    email.placeholder = 'name@example.com';
    pass.placeholder = login ? _t('Ваш пароль') : _t('Минимум 8 символов');
    submit.querySelector('.lbl').textContent = login ? _t('Войти') : _t('Создать аккаунт');
    logout.querySelector('.lbl').textContent = _t('Выйти');
    $('#fid-profile .badge').textContent = 'Freedom ID';
    const alt = view.querySelector('.fid-alt .hint');
    if (alt) alt.textContent = _t('Скоро: вход через Google и Discord.');
  }

  function setMode(m) {
    mode = m === 'signup' ? 'signup' : 'login';
    $$('button', modeSeg).forEach((b) => b.setAttribute('aria-selected', String(b.dataset.mode === mode)));
    modeSeg._place?.(false);
    nickRow.hidden = mode !== 'signup';
    pass.autocomplete = mode === 'login' ? 'current-password' : 'new-password';
    setMsg('');
    applyI18n();
  }

  function render() {
    const u = read();
    const on = !!u;
    card.hidden = on;
    profile.hidden = !on;
    if (on) {
      $('#fid-user-name').textContent = u.nick || u.email || 'Freedom ID';
      $('#fid-user-email').textContent = u.email || '';
    }
    applyI18n();
  }

  modeSeg.addEventListener('click', (e) => {
    const b = e.target.closest('button[data-mode]');
    if (b) setMode(b.dataset.mode);
  });
  modeSeg.addEventListener('seg:change', (e) => setMode(e.detail.dataset.mode));

  form.addEventListener('submit', (e) => {
    e.preventDefault();
    setMsg('');
    const u = read();
    if (u) return render();
    const em = email.value.trim();
    const pw = pass.value;
    const nk = nick.value.trim();

    if (!em || !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(em)) return setMsg(_t('Введите корректную почту.'), true);
    if (pw.length < 8) return setMsg(_t('Пароль должен быть не короче 8 символов.'), true);
    if (mode === 'signup' && nk.length < 2) return setMsg(_t('Введите ник (минимум 2 символа).'), true);

    const cur = read();
    if (mode === 'login') {
      if (!cur) return setMsg(_t('Аккаунт не найден на этом устройстве. Нажмите «Регистрация».'), true);
      if (cur.email.toLowerCase() !== em.toLowerCase() || cur.pass !== pw) return setMsg(_t('Неверная почта или пароль.'), true);
      setMsg(_t('Вход выполнен.'));
    } else {
      write({ nick: nk, email: em, pass: pw, created_at: Date.now() });
      setMsg(_t('Аккаунт Freedom ID создан на этом устройстве.'));
    }
    pass.value = '';
    render();
  });

  logout.addEventListener('click', () => {
    write(null);
    setMode('login');
    email.value = '';
    nick.value = '';
    pass.value = '';
    setMsg(_t('Вы вышли из Freedom ID.'));
    render();
  });

  addEventListener('i18n:applied', () => { applyI18n(); render(); });
  render();
  window.FFreedomId = { render, open: () => { window.show?.('freedom-id'); render(); } };
})();
