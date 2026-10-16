// Freedom Launcher — first-run guide: sign in → look → first instance. Shown once (settings.onboarded).
(() => {
  const { $, $$ } = window.FUI;
  const W = () => $('#welcome');
  const N = 3;
  let step = 0;

  const signedIn = () => S.accounts.accounts.length > 0;
  function paintAccount() {
    const sel = S.accounts.accounts.find((a) => a.id === S.accounts.selected) || S.accounts.accounts[0];
    $('#wc-login').hidden = !!sel;
    $('#wc-signed').hidden = !sel;
    if (sel) $('#wc-signed').replaceChildren(headFor(sel, 'head head-lg'), el('b', {}, sel.name), el('i', { class: 'wc-ok', 'aria-hidden': 'true' }));
  }
  function paintLook() {
    $$('[data-wc-theme]').forEach((b) => b.setAttribute('aria-pressed', String(b.dataset.wcTheme === (S.theme?.id || 'default'))));
    $$('[data-wc-wall]').forEach((b) => b.setAttribute('aria-pressed', String(b.dataset.wcWall === S.look.wall)));
    W().dataset.wall = S.look.wall;
  }
  function go(n) {
    step = Math.max(0, Math.min(N - 1, n));
    $$('#welcome .wc-step').forEach((s) => { s.hidden = +s.dataset.step !== step; });
    $$('#welcome .wc-dots i').forEach((d, i) => d.toggleAttribute('data-on', i === step));
    $('#wc-back').hidden = step === 0;
    $('#wc-next .lbl').textContent = step === N - 1 ? 'Готово' : step === 0 && !signedIn() ? 'Позже' : 'Далее';
    paintAccount(); paintLook();
  }
  function finish(then) {
    S.onboarded = true;
    call('settings.set', { onboarded: true }).catch(() => {});
    const w = W();
    w.dataset.leaving = '';
    setTimeout(() => { w.hidden = true; delete w.dataset.leaving; }, 240);
    then?.();
  }
  function open(n = 0) {
    W().hidden = false;
    go(n);
    setTimeout(() => $('#wc-next').focus(), 60);
  }
  function start() {
    if (S.onboarded) return;
    // People who used the launcher before this guide existed skip it.
    if (S.instances.length || signedIn()) { S.onboarded = true; call('settings.set', { onboarded: true }).catch(() => {}); return; }
    open();
  }

  $('#wc-next').addEventListener('click', () => (step === N - 1 ? finish() : go(step + 1)));
  $('#wc-back').addEventListener('click', () => go(step - 1));
  $('#wc-skip').addEventListener('click', () => finish());
  $$('[data-wc-theme]').forEach((b) => b.addEventListener('click', () => {
    $(`#theme-seg [data-theme-set="${b.dataset.wcTheme}"]`)?.click();
    S.theme = { ...(S.theme || {}), id: b.dataset.wcTheme };
    paintLook();
  }));
  $$('[data-wc-wall]').forEach((b) => b.addEventListener('click', () => {
    $(`#wall-seg [data-wall-set="${b.dataset.wcWall}"]`)?.click();
    paintLook();
  }));
  $('#wc-create').addEventListener('click', () => finish(() => show('create')));
  $('#wc-import').addEventListener('click', () => finish(() => { show('create'); $('#c-import').click(); }));
  document.addEventListener('accounts:change', () => {
    if (W().hidden || step !== 0) return;
    go(0);
    if (signedIn()) setTimeout(() => step === 0 && go(1), 700);
  });
  document.addEventListener('look:change', () => { if (!W().hidden) paintLook(); });

  window.FWelcome = { start, open };
})();