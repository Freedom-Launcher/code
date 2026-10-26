"use strict";
// A note right on the instance page: one per instance (key `n:<instance id>`), saved as you type.
const $ = (id) => document.getElementById(id);
const T = {
  ph: {
    ru: "Заметка к сборке: координаты, IP сервера, что сделать в следующий раз…",
    en: "Instance note: coordinates, server IP, what to do next time…",
    uk: "Нотатка до збірки: координати, IP сервера, що зробити наступного разу…",
    es: "Nota de la instancia: coordenadas, IP del servidor, qué hacer la próxima vez…",
    de: "Notiz zum Modpack: Koordinaten, Server-IP, was als Nächstes zu tun ist…",
    it: "Nota del modpack: coordinate, IP del server, cosa fare la prossima volta…",
    fr: "Note du modpack : coordonnées, IP du serveur, quoi faire la prochaine fois…",
    pl: "Notatka do paczki: koordynaty, IP serwera, co zrobić następnym razem…",
    tr: "Paket notu: koordinatlar, sunucu IP'si, bir dahaki sefere yapılacaklar…",
    pt: "Nota da instância: coordenadas, IP do servidor, o que fazer da próxima vez…",
  },
  saved: { ru: "Сохранено", en: "Saved", uk: "Збережено", es: "Guardado", de: "Gespeichert", it: "Salvato", fr: "Enregistré", pl: "Zapisano", tr: "Kaydedildi", pt: "Salvo" },
};
const tr = (k) => T[k][fl.lang] || T[k].en;
const key = (id) => `n:${id}`;
let inst = "", timer = 0, fade = 0;

function grow() {
  const t = $("txt");
  t.style.height = "0";
  t.style.height = `${Math.min(260, Math.max(21, t.scrollHeight))}px`;
}

function status(text) {
  clearTimeout(fade);
  $("st").textContent = text;
  if (text === tr("saved")) fade = setTimeout(() => { $("st").textContent = ""; }, 1500);
}

async function save() {
  clearTimeout(timer);
  timer = 0;
  if (!inst) return;
  try {
    const v = $("txt").value;
    if (v.trim()) await fl.storage.set(key(inst), v); else await fl.storage.delete(key(inst));
    status(tr("saved"));
  } catch (e) { status(String(e.message || e)); }
}

async function open(id) {
  if (!id || id === inst) return;
  if (timer) await save();
  inst = id;
  status("");
  const v = await fl.storage.get(key(id)).catch(() => null);
  if (id !== inst) return;
  $("txt").value = typeof v === "string" ? v : "";
  grow();
}

const t = $("txt");
t.addEventListener("input", () => { grow(); status("…"); clearTimeout(timer); timer = setTimeout(save, 600); });
t.addEventListener("blur", () => { if (timer) save(); });
fl.on("context", (c) => { t.placeholder = tr("ph"); open(c.instance); });
