"use strict";
// «Поделиться логом» is a launcher action: no window of its own. On the crash card it
// uploads the newest crash report, elsewhere logs/latest.log; the launcher copies the link.
const MAX = 900000;
const T = {
  nolog: {
    ru: "В этой сборке пока нет логов. Запустите игру хотя бы раз.", en: "This instance has no logs yet. Launch the game once.",
    uk: "У цій збірці ще немає логів. Запустіть гру хоча б раз.", es: "Esta instancia aún no tiene logs. Inicia el juego una vez.",
    de: "Dieses Modpack hat noch keine Logs. Starte das Spiel einmal.", it: "Questo modpack non ha ancora log. Avvia il gioco una volta.",
    fr: "Ce modpack n'a pas encore de logs. Lancez le jeu une fois.", pl: "Ta paczka nie ma jeszcze logów. Uruchom grę choć raz.",
    tr: "Bu pakette henüz günlük yok. Oyunu bir kez başlatın.", pt: "Esta instância ainda não tem logs. Inicie o jogo uma vez.",
  },
  empty: {
    ru: "Лог пуст", en: "The log is empty", uk: "Лог порожній", es: "El log está vacío", de: "Das Log ist leer",
    it: "Il log è vuoto", fr: "Le log est vide", pl: "Log jest pusty", tr: "Günlük boş", pt: "O log está vazio",
  },
};
const tr = (k) => T[k][fl.lang] || T[k].en;

async function newestCrash(inst) {
  const all = await fl.files.list(inst, "crash-reports").catch(() => []);
  const c = all.filter((f) => !f.dir && f.name.endsWith(".txt")).sort((a, b) => b.name.localeCompare(a.name))[0];
  return c ? `crash-reports/${c.name}` : null;
}

async function pick(inst, place) {
  if (place === "crash") {
    const c = await newestCrash(inst);
    if (c) return c;
  }
  const logs = await fl.files.list(inst, "logs").catch(() => []);
  if (logs.some((f) => f.name === "latest.log")) return "logs/latest.log";
  return newestCrash(inst);
}

fl.action("share", async ({ instance, place }) => {
  const path = await pick(instance, place);
  if (!path) throw new Error(tr("nolog"));
  let text = await fl.files.read(instance, path);
  if (!text.trim()) throw new Error(tr("empty"));
  if (text.length > MAX) text = text.slice(-MAX);
  const r = await fl.fetch("https://api.mclo.gs/1/log", {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body: "content=" + encodeURIComponent(text),
  });
  const j = r.json();
  if (!j.success) throw new Error(j.error || `mclo.gs: ${r.status}`);
  return { copy: j.url, open: j.url };
});
