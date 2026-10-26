// Crash doctor: when a game exits with an error, read the log and say in plain words what went wrong.
(() => {
  const { $, toast } = FUI;
  const seen = new Map();
  const dismissed = new Map();
  const tab = $('#tab-logs');
  const card = el('div', { class: 'log-doc', id: 'log-doc', 'data-id': 'instance.logs.crash', role: 'status' });
  card.hidden = true;
  tab.insertBefore(card, tab.querySelector('.log-wrap'));

  const settings = (id) => ({ label: _t("Настройки Java и памяти"), run: () => window.FInstSettings?.open(id, 'java') });
  const content = () => ({ label: _t("Открыть «Контент»"), run: () => $('#ip-tabs [data-tab="content"]')?.click() });
  const find = (id, q) => ({
    label: _t("Найти «{0}» в каталоге", [q]),
    run: () => {
      window.FMods?.scoped(id);
      requestAnimationFrame(() => { const f = $('#disc-q'); if (f) { f.value = q; f.dispatchEvent(new Event('input', { bubbles: true })); } });
    },
  });
  const pick = (text, res) => { for (const r of res) { const m = r.exec(text); if (m) return m; } return null; };

  // First matching rule wins; order goes from the most specific cause to the most generic.
  const RULES = [
    (t, id) => /java\.lang\.OutOfMemoryError|Could not reserve enough space|insufficient memory for the Java Runtime/.test(t) && {
      title: _t("Игре не хватило памяти"),
      text: _t("Java упёрлась в лимит памяти. Выделите сборке больше (обычно 4–6 ГБ для модов хватает) или уберите тяжёлые моды и шейдеры."),
      acts: [settings(id)],
    },
    (t, id) => /UnsupportedClassVersionError|has been compiled by a more recent version of the Java Runtime|requires Java \d+|Incompatible Java/i.test(t) && {
      title: _t("Не та версия Java"),
      text: _t("Игра или мод собраны под более новую Java. Выберите в настройках сборки Java той версии, которую требует Minecraft (для 1.20.5+ — Java 21)."),
      acts: [settings(id)],
    },
    (t, id) => {
      const m = pick(t, [/requires (?:any version|version [^\s,]+|[^,\n]*?) of (?:mod )?'?([\w.\- ]+?)'?,? which is missing/i, /- Install ([\w.\-]+), (?:any version|version)/, /Mod ID: '([\w.\-]+)'.*?Requested by/s, /Missing (?:mandatory )?dependenc(?:y|ies)[^\n]*?'([\w.\-]+)'/i]);
      return m && { title: _t("Не хватает мода «{0}»", [m[1].trim()]), text: _t("Один из модов не работает без него. Установите его в эту сборку — версия подберётся под игру и загрузчик."), acts: [find(id, m[1].trim())] };
    },
    (t, id) => /Found duplicate mods?|Duplicate mods? found|duplicate mod ids?/i.test(t) && {
      title: _t("Один мод установлен дважды"),
      text: _t("В папке модов лежат две версии одного мода. Оставьте одну — более новую."),
      acts: [content(id)],
    },
    (t, id) => {
      const m = pick(t, [/Mixin apply for mod ([\w.\-]+) failed/i, /mixins?\.json:[^\n]*from mod ([\w.\-]+)/i]);
      return (m || /MixinApplyError|InvalidInjectionException|Mixin transformation of [^\n]+ failed/.test(t)) && {
        title: m ? _t("Конфликт мода «{0}» с другими", [m[1]]) : _t("Моды конфликтуют между собой"),
        text: _t("Мод не смог встроиться в игру — обычно из-за несовместимости с другим модом или версией. Попробуйте обновить его или временно отключить."),
        acts: [content(id)],
      };
    },
    (t, id) => {
      const m = pick(t, [/Incompatible mods? found!?[^\n]*\n[^\n]*?([^\n]+)/i, /(Mod [^\n]+? is incompatible with [^\n]+)/i]);
      return m && { title: _t("Моды несовместимы"), text: m[1].trim().replace(/^[-\s]+/, ''), acts: [content(id)] };
    },
    (t) => /Pixel format not accelerated|GLFW error 65542|driver does not appear to support OpenGL|Couldn't set pixel format|No OpenGL context/i.test(t) && {
      title: _t("Проблема с видеодрайвером"),
      text: _t("Игра не смогла создать окно OpenGL. Обновите драйвер видеокарты; на ноутбуке запускайте Java на дискретной видеокарте."),
      acts: [],
    },
  ];

  function diagnose(text, id) {
    for (const r of RULES) { const d = r(text, id); if (d) return d; }
    const desc = /^Description: (.+)$/m.exec(text);
    const err = text.split('\n').find((l) => /\b(ERROR|FATAL)\b|Exception:/.test(l));
    return {
      title: _t("Игра вылетела"),
      text: desc ? _t("Причина из отчёта: {0}", [desc[1].trim()]) : err ? err.replace(/^\[[^\]]*\]\s*(\[[^\]]*\]:?\s*)?/, '').trim().slice(0, 240) : _t("Точную причину в логе найти не удалось — посмотрите строки с ошибками."),
      acts: [],
    };
  }

  function paint(id, ex, d, report) {
    const btn = (a, cls = '') => { const b = el('button', { class: `btn btn-sm ${cls}`, type: 'button' }, a.label); b.addEventListener('click', a.run); return b; };
    const close = el('button', { class: 'btn btn-icon btn-ghost log-doc-x', type: 'button', 'aria-label': _t("Скрыть") }, '×');
    close.addEventListener('click', () => { dismissed.set(id, ex.at); card.hidden = true; });
    const errs = { label: _t("Только ошибки"), run: () => $('#log-level [data-lv="error"]')?.click() };
    const code = ex.code == null ? _t("процесс завершён системой") : _t("код выхода {0}", [ex.code]);
    card.replaceChildren(
      el('div', { class: 'log-doc-ic', 'aria-hidden': 'true' }, '!'),
      el('div', { class: 'log-doc-b' },
        el('b', {}, d.title),
        el('p', {}, d.text),
        el('div', { class: 'log-doc-acts' }, ...d.acts.map((a, i) => btn(a, i ? '' : 'btn-primary')), btn(errs, 'btn-ghost'), ...(window.FAddons?.buttons('crash', id) || [])),
        el('span', { class: 'log-doc-m' }, report ? _t("{0} · отчёт: {1}", [code, report]) : code)),
      close);
    card.hidden = false;
  }

  async function check(id = S.current) {
    const ex = S.exits?.[id];
    if (!ex?.crashed || dismissed.get(id) === ex.at || S.running.includes(id)) { card.hidden = true; return; }
    const r = await call('instances.log', { instance: id }).catch(() => null);
    if (id !== S.current) return;
    const text = r?.text || '';
    const rep = /Crash report saved to:?\s*(?:#@!@#\s*)?(\S+)/.exec(text);
    paint(id, ex, diagnose(text, id), rep ? rep[1].split(/[\\/]/).pop() : '');
  }

  function exited(exits) {
    for (const [id, ex] of Object.entries(exits || {})) {
      if (seen.get(id) === ex.at) continue;
      seen.set(id, ex.at);
      if (!ex.crashed) continue;
      const name = byId(id)?.name || _t("Сборка");
      toast(_t("{0}: игра вылетела", [name]), {
        error: true, ms: 9000,
        action: { label: _t("Разобрать"), run: () => { openInstance(id); $('#ip-tabs [data-tab="logs"]')?.click(); } },
      });
      if (S.view === 'instance' && S.current === id && !tab.hidden) check(id);
    }
  }

  $('#ip-tabs').addEventListener('click', (e) => { if (e.target.closest('[data-tab="logs"]')) check(); });
  window.FCrash = { exited, check, diagnose };
})();