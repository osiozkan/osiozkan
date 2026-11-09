import { parseAss, serializeAss, newEvent, Style, DEFAULT_STYLE, getInfo, setInfo } from "../core/ass";
import { detectAndParse, toSrt, toVtt, toPlainText, toLrc, toSbv, toTtml } from "../core/formats";
import { formatAssTime, parseAssTime } from "../core/time";
import { cps, runQc, autoFix, DEFAULT_QC } from "../core/qc";
import { findAll, replaceAll, mergeEvents, splitEvent, shiftTimes, sortEvents, insertEventAfter, duplicateEvent, fixOverlaps, chainGaps, snapToFrames } from "../core/ops";
import { stripTags } from "../core/tags";
import { LuaRuntime, MacroInfo } from "../lua/runtime";
import { Store } from "./store";
import { renderPreview } from "./preview";
import { Waveform } from "./wave";
import { modal, field, inp, luaDialog } from "./dialogs";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const store = new Store();
(window as any).yoru = { store };

/* ---------- Durum çubuğu ---------- */
const status = (m: string) => { $("status").textContent = m; };

/* ---------- Izgara (sanal kaydırma) ---------- */
const ROW = 26;
const COLS = ["#", "", "Kat", "Başlangıç", "Bitiş", "Süre", "CPS", "Stil", "Oyuncu", "Metin"];
$("grid-head").innerHTML = COLS.map((c) => `<div>${c}</div>`).join("");
const grid = $("grid"), inner = $("grid-inner");
let playingIdx = -1;
let issues = new Map<number, string>();

function renderGrid() {
  const evs = store.file.events;
  inner.style.height = evs.length * ROW + "px";
  const top = Math.max(0, Math.floor(grid.scrollTop / ROW) - 5);
  const bottom = Math.min(evs.length, Math.ceil((grid.scrollTop + grid.clientHeight) / ROW) + 5);
  let html = "";
  for (let i = top; i < bottom; i++) {
    const e = evs[i]; const dur = e.end - e.start; const c = cps(e.text, dur);
    const cls = ["grow-row", store.active === i ? "act" : store.sel.has(i) ? "sel" : "", e.comment ? "cmt" : "", playingIdx === i ? "playing" : ""].join(" ");
    const iss = issues.get(i);
    html += `<div class="${cls}" data-i="${i}" style="top:${i * ROW}px" ${iss ? `title="${esc(iss)}"` : ""}>
      <div>${i + 1}</div><div>${e.comment ? "#" : ""}</div><div>${e.layer}</div><div>${formatAssTime(e.start)}</div><div>${formatAssTime(e.end)}</div>
      <div>${(dur / 1000).toFixed(2)}</div><div class="${c > DEFAULT_QC.maxCps ? "bad" : ""}">${Number.isFinite(c) ? c.toFixed(0) : "∞"}</div>
      <div>${esc(e.style)}</div><div>${esc(e.actor)}</div><div class="${iss ? "warn" : ""}">${esc(e.text)}</div></div>`;
  }
  inner.innerHTML = html;
}
const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" } as any)[c]);
grid.addEventListener("scroll", renderGrid);
grid.addEventListener("mousedown", (ev) => {
  const row = (ev.target as HTMLElement).closest(".grow-row") as HTMLElement | null; if (!row) return;
  const i = +row.dataset.i!;
  store.select(i, ev.shiftKey ? "range" : ev.ctrlKey || ev.metaKey ? "toggle" : "one");
  if (!ev.shiftKey) seekTo(store.file.events[i].start);
});
function ensureVisible(i: number) {
  const y = i * ROW;
  if (y < grid.scrollTop) grid.scrollTop = y; else if (y + ROW > grid.scrollTop + grid.clientHeight) grid.scrollTop = y - grid.clientHeight + ROW * 2;
}

/* ---------- Düzenleme paneli ---------- */
const E = { start: $<HTMLInputElement>("e-start"), end: $<HTMLInputElement>("e-end"), dur: $<HTMLInputElement>("e-dur"), layer: $<HTMLInputElement>("e-layer"),
  style: $<HTMLSelectElement>("e-style"), actor: $<HTMLInputElement>("e-actor"), effect: $<HTMLInputElement>("e-effect"), comment: $<HTMLInputElement>("e-comment"),
  text: $<HTMLTextAreaElement>("e-text"), cps: $("e-cps") };
let syncing = false;
function syncEdit() {
  syncing = true;
  E.style.innerHTML = store.file.styles.map((s) => `<option>${esc(s.name)}</option>`).join("");
  const e = store.activeEvent;
  for (const k of Object.values(E)) (k as any).disabled = !e;
  if (e) {
    E.start.value = formatAssTime(e.start); E.end.value = formatAssTime(e.end); E.dur.value = ((e.end - e.start) / 1000).toFixed(2);
    E.layer.value = String(e.layer); E.style.value = e.style; E.actor.value = e.actor; E.effect.value = e.effect; E.comment.checked = e.comment;
    if (document.activeElement !== E.text || E.text.value !== e.text) E.text.value = e.text;
    const c = cps(e.text, e.end - e.start); E.cps.textContent = `${Number.isFinite(c) ? c.toFixed(1) : "∞"} cps · ${stripTags(e.text).length} krk`;
  } else { E.text.value = ""; }
  E.dur.disabled = true;
  syncing = false;
}
function bindEdit() {
  const upd = (fn: (e: ReturnType<typeof newEvent>) => void) => () => {
    if (syncing) return; const e = store.activeEvent; if (!e) return;
    store.checkpoint(true);
    const targets = store.sel.size > 1 && fn.length ? [...store.sel].map((i) => store.file.events[i]) : [e];
    targets.forEach((t) => fn(t)); store.emit();
  };
  E.text.addEventListener("input", () => { if (syncing) return; const e = store.activeEvent; if (!e) return; store.checkpoint(true); e.text = E.text.value; store.emit(); });
  E.start.addEventListener("change", upd((e) => { e.start = parseAssTime(E.start.value); if (e.end < e.start) e.end = e.start; }));
  E.end.addEventListener("change", upd((e) => { e.end = Math.max(parseAssTime(E.end.value), e.start); }));
  E.layer.addEventListener("change", upd((e) => { e.layer = +E.layer.value || 0; }));
  E.style.addEventListener("change", upd((e) => { e.style = E.style.value; }));
  E.actor.addEventListener("change", upd((e) => { e.actor = E.actor.value; }));
  E.effect.addEventListener("change", upd((e) => { e.effect = E.effect.value; }));
  E.comment.addEventListener("change", upd((e) => { e.comment = E.comment.checked; }));
}
bindEdit();
const wrapSel = (open: string, close: string) => {
  const t = E.text, a = t.selectionStart, b = t.selectionEnd; if (!store.activeEvent) return;
  store.checkpoint(); const v = t.value; t.value = v.slice(0, a) + open + v.slice(a, b) + close + v.slice(b); store.activeEvent.text = t.value;
  t.focus(); t.selectionStart = a + open.length; t.selectionEnd = b + open.length; store.emit();
};
$("e-i").onclick = () => wrapSel("{\\i1}", "{\\i0}"); $("e-b").onclick = () => wrapSel("{\\b1}", "{\\b0}"); $("e-u").onclick = () => wrapSel("{\\u1}", "{\\u0}");
$("e-an8").onclick = () => { const e = store.activeEvent; if (e) { store.checkpoint(); e.text = "{\\an8}" + e.text.replace(/^\{\\an\d\}/, ""); store.emit(); } };
$("e-nl").onclick = () => wrapSel("\\N", "");
$("e-new").onclick = () => { store.checkpoint(); const e = insertEventAfter(store.file, store.active < 0 ? store.file.events.length - 1 : store.active); store.select(store.file.events.indexOf(e)); E.text.focus(); };
$("e-dup").onclick = () => { if (store.active < 0) return; store.checkpoint(); duplicateEvent(store.file, store.active); store.select(store.active + 1); };
$("e-del").onclick = () => {
  const s = store.selected; if (!s.length) return; store.checkpoint();
  store.file.events = store.file.events.filter((_, i) => !store.sel.has(i)); store.sel = new Set(); store.active = Math.min(s[0], store.file.events.length - 1);
  if (store.active >= 0) store.sel.add(store.active); store.emit();
};
$("e-merge").onclick = () => { const s = store.selected; if (s.length < 2) return status("Birleştirmek için birden çok satır seçin"); store.checkpoint(); mergeEvents(store.file, s); store.select(s[0]); };
$("e-split").onclick = () => {
  const e = store.activeEvent; if (!e) return; store.checkpoint();
  const pos = E.text.selectionStart; const at = Math.round(e.start + ((e.end - e.start) * pos) / Math.max(1, E.text.value.length));
  splitEvent(store.file, store.active, video.src && !video.paused ? Math.round(video.currentTime * 1000) : at, pos); store.emit();
};

/* ---------- Video / önizleme ---------- */
const video = $<HTMLVideoElement>("video");
const wave = new Waveform($<HTMLCanvasElement>("wave"), store, (ms) => seekTo(ms));
function seekTo(ms: number) { if (video.src) video.currentTime = ms / 1000; wave.setTime(ms); updatePreview(ms); }
function resolution(): [number, number] { return [+(getInfo(store.file, "PlayResX") ?? 1920) || 1920, +(getInfo(store.file, "PlayResY") ?? 1080) || 1080]; }
function updatePreview(ms: number) {
  let evs = store.file.events.filter((e) => !e.comment && e.start <= ms && ms < e.end);
  if (!video.src) evs = store.activeEvent ? [store.activeEvent] : []; // video yokken seçili satırı göster
  const [rx, ry] = resolution();
  renderPreview($("overlay"), store.file, evs, rx, ry);
  if (!video.src) return;
  const idx = store.file.events.findIndex((e) => !e.comment && e.start <= ms && ms < e.end);
  if (idx !== playingIdx) { playingIdx = idx; renderGrid(); }
}
function tick() { const ms = video.currentTime * 1000; wave.setTime(ms); updatePreview(ms); requestAnimationFrame(tick); }
requestAnimationFrame(tick);
$("b-video").onclick = () => $<HTMLInputElement>("f-video").click();
$<HTMLInputElement>("f-video").onchange = (ev) => {
  const f = (ev.target as HTMLInputElement).files?.[0]; if (!f) return;
  const url = URL.createObjectURL(f); video.src = url; $("vwrap").classList.add("has");
  video.onloadedmetadata = () => { status(`Video: ${f.name} (${video.videoWidth}×${video.videoHeight})`); wave.load(url); };
};

/* ---------- Dosya işlemleri ---------- */
function download(name: string, data: string, mime = "text/plain") {
  const a = document.createElement("a"); a.href = URL.createObjectURL(new Blob([data], { type: mime + ";charset=utf-8" })); a.download = name; a.click();
}
$("b-open").onclick = () => $<HTMLInputElement>("f-open").click();
$<HTMLInputElement>("f-open").onchange = async (ev) => {
  const f = (ev.target as HTMLInputElement).files?.[0]; if (!f) return;
  const buf = new Uint8Array(await f.arrayBuffer());
  let text = new TextDecoder("utf-8").decode(buf);
  if (text.includes("�")) text = new TextDecoder("windows-1254").decode(buf); // Türkçe eski kodlama
  store.load(detectAndParse(f.name, text), f.name.replace(/\.[^.]+$/, "") + ".ass");
  status(`${f.name} açıldı: ${store.file.events.length} satır`);
};
const save = () => { download(store.name, serializeAss(store.file)); store.dirty = false; try { localStorage.removeItem(AUTOSAVE); } catch { /* */ } status("Kaydedildi"); };
$("b-save").onclick = save;
$<HTMLSelectElement>("b-export").onchange = (ev) => {
  const s = ev.target as HTMLSelectElement; const base = store.name.replace(/\.[^.]+$/, "");
  if (s.value === "srt") download(base + ".srt", toSrt(store.file)); if (s.value === "vtt") download(base + ".vtt", toVtt(store.file), "text/vtt"); if (s.value === "lrc") download(base + ".lrc", toLrc(store.file)); if (s.value === "sbv") download(base + ".sbv", toSbv(store.file)); if (s.value === "ttml") download(base + ".ttml", toTtml(store.file), "application/ttml+xml"); if (s.value === "txt") download(base + ".txt", toPlainText(store.file));
  s.value = "";
};
$("b-undo").onclick = () => store.undo(); $("b-redo").onclick = () => store.redo();

/* ---------- Diyaloglar ---------- */
$("b-find").onclick = async () => {
  const find = inp(), rep = inp(), rx = inp("", "checkbox"), mc = inp("", "checkbox"), tags = inp("", "checkbox"); tags.checked = true;
  const body = document.createElement("div"); body.className = "list";
  body.append(field("Bul", find), field("Değiştir", rep), field("Düzenli ifade", rx), field("Büyük/küçük harf", mc), field("Yalnızca görünen metin (etiketleri atla)", tags));
  const r = await modal("Bul / Değiştir", body, [{ label: "Tümünü değiştir", value: "rep", primary: true }, { label: "Say", value: "cnt" }, { label: "Kapat", value: "x" }]);
  const o = { find: find.value, replace: rep.value, regex: rx.checked, matchCase: mc.checked, skipTags: tags.checked };
  if (!o.find) return;
  try {
    if (r === "cnt") status(`${findAll(store.file, o).length} satırda bulundu`);
    if (r === "rep") { store.checkpoint(); const n = replaceAll(store.file, o); store.emit(); status(`${n} satır değiştirildi`); }
  } catch (e) { status("Hata: " + (e as Error).message); }
};
$("b-shift").onclick = async () => {
  const ms = inp("0", "number"), which = document.createElement("select"), scope = document.createElement("select");
  which.innerHTML = "<option value='both'>Başlangıç+Bitiş</option><option value='start'>Yalnızca başlangıç</option><option value='end'>Yalnızca bitiş</option>";
  scope.innerHTML = "<option value='all'>Tüm satırlar</option><option value='sel'>Seçili satırlar</option><option value='from'>Seçiliden sona</option>";
  const body = document.createElement("div"); body.className = "list"; body.append(field("Kaydırma (ms, − geri)", ms), field("Hedef", which), field("Kapsam", scope));
  if (await modal("Zaman kaydır", body, [{ label: "Uygula", value: "ok", primary: true }, { label: "İptal", value: "x" }]) !== "ok") return;
  store.checkpoint();
  const sel = scope.value === "sel" ? store.selected : scope.value === "from" ? store.file.events.map((_, i) => i).filter((i) => i >= (store.selected[0] ?? 0)) : undefined;
  shiftTimes(store.file, { ms: +ms.value, start: which.value !== "end", end: which.value !== "start", selection: sel }); store.emit();
};
$("b-props").onclick = async () => {
  const f = store.file; const rx = inp(getInfo(f, "PlayResX") ?? "1920"), ry = inp(getInfo(f, "PlayResY") ?? "1080"), title = inp(getInfo(f, "Title") ?? ""), fps = inp(store.fps, "number");
  const body = document.createElement("div"); body.className = "list"; body.append(field("Başlık", title), field("PlayResX", rx), field("PlayResY", ry), field("FPS", fps));
  if (await modal("Betik özellikleri", body, [{ label: "Tamam", value: "ok", primary: true }, { label: "İptal", value: "x" }]) !== "ok") return;
  store.checkpoint(); setInfo(f, "Title", title.value); setInfo(f, "PlayResX", rx.value); setInfo(f, "PlayResY", ry.value); store.fps = +fps.value || 23.976; store.emit();
};
$("b-styles").onclick = () => styleManager();
async function styleManager() {
  let cur = 0;
  const body = document.createElement("div"); body.style.display = "grid"; body.style.gap = "6px";
  const draw = () => {
    body.innerHTML = ""; const st = store.file.styles; const s = st[cur];
    const sel = document.createElement("select"); sel.innerHTML = st.map((x, i) => `<option value="${i}">${esc(x.name)}</option>`).join(""); sel.value = String(cur); sel.onchange = () => { cur = +sel.value; draw(); };
    body.append(field("Stil", sel));
    if (!s) return;
    const num = (k: keyof Style) => { const i = inp(s[k] as number, "number"); i.step = "any"; i.onchange = () => { (s as any)[k] = +i.value; store.emit(); }; return i; };
    const txt = (k: keyof Style) => { const i = inp(s[k] as string); i.onchange = () => { (s as any)[k] = i.value; store.emit(); }; return i; };
    const chk = (k: keyof Style) => { const i = inp("", "checkbox"); i.checked = s[k] as boolean; i.onchange = () => { (s as any)[k] = i.checked; store.emit(); }; return i; };
    const col = (k: keyof Style) => {
      const i = document.createElement("input"); i.type = "color"; const h = String(s[k]).replace("&H", "").padStart(8, "0");
      i.value = "#" + h.slice(6, 8) + h.slice(4, 6) + h.slice(2, 4);
      i.onchange = () => { const v = i.value.slice(1); (s as any)[k] = "&H" + String(s[k]).replace("&H", "").padStart(8, "0").slice(0, 2) + v.slice(4, 6) + v.slice(2, 4) + v.slice(0, 2); store.emit(); }; return i;
    };
    body.append(field("Ad", txt("name")), field("Yazı tipi", txt("fontname")), field("Boyut", num("fontsize")), field("Ana renk", col("primary")), field("İkincil", col("secondary")),
      field("Kontur rengi", col("outline")), field("Gölge rengi", col("back")), field("Kalın", chk("bold")), field("İtalik", chk("italic")), field("Altı çizili", chk("underline")),
      field("Üstü çizili", chk("strikeout")), field("Kontur", num("outlineW")), field("Gölge", num("shadow")), field("Hizalama (numpad)", num("alignment")),
      field("Kenar boşluğu S", num("marginL")), field("Kenar boşluğu R", num("marginR")), field("Kenar boşluğu V", num("marginV")), field("Ölçek X", num("scalex")), field("Ölçek Y", num("scaley")), field("Aralık", num("spacing")));
    const row = document.createElement("div"); row.className = "row";
    const add = document.createElement("button"); add.textContent = "Yeni"; add.onclick = () => { store.checkpoint(); st.push({ ...DEFAULT_STYLE, name: "Yeni" + st.length }); cur = st.length - 1; store.emit(); draw(); };
    const dup = document.createElement("button"); dup.textContent = "Çoğalt"; dup.onclick = () => { store.checkpoint(); st.push({ ...s, name: s.name + " kopya" }); cur = st.length - 1; store.emit(); draw(); };
    const del = document.createElement("button"); del.textContent = "Sil"; del.onclick = () => { if (st.length < 2) return; store.checkpoint(); st.splice(cur, 1); cur = 0; store.emit(); draw(); };
    row.append(add, dup, del); body.append(row);
  };
  store.checkpoint(); draw();
  await modal("Stil yöneticisi", body, [{ label: "Kapat", value: "x", primary: true }]); store.emit();
}
$("b-qc").onclick = async () => {
  const iss = runQc(store.file, DEFAULT_QC); issues = new Map(); for (const i of iss) issues.set(i.index, (issues.get(i.index) ? issues.get(i.index) + "; " : "") + i.message);
  renderGrid();
  const body = document.createElement("div"); body.className = "list";
  if (!iss.length) body.textContent = "Sorun bulunamadı 🎉";
  for (const i of iss.slice(0, 500)) {
    const d = document.createElement("div"); d.className = "it"; d.innerHTML = `<span class="tag ${i.severity}">${i.code}</span><b>#${i.index + 1}</b><span>${esc(i.message)}</span>`;
    d.onclick = () => { store.select(i.index); ensureVisible(i.index); seekTo(store.file.events[i.index].start); }; body.appendChild(d);
  }
  const tools = document.createElement("div"); tools.className = "row";
  const mk = (t: string, fn: () => string) => { const b = document.createElement("button"); b.textContent = t; b.onclick = () => { store.checkpoint(); status(fn()); store.emit(); }; tools.appendChild(b); };
  mk("Boşlukları düzelt", () => `${autoFix(store.file)} düzeltme`); mk("Çakışmaları gider", () => `${fixOverlaps(store.file)} çakışma`);
  mk("Boşlukları kapat (≤250ms)", () => `${chainGaps(store.file, 250)} boşluk`); mk("Karelere oturt", () => (snapToFrames(store.file, store.fps), "Kareler hizalandı")); mk("Sırala", () => (sortEvents(store.file), "Sıralandı"));
  body.prepend(tools);
  await modal(`Denetim — ${iss.length} bulgu`, body, [{ label: "Kapat", value: "x", primary: true }]);
};

/* ---------- Otomasyon (Lua) ---------- */
let rt: LuaRuntime | null = null;
let logBuf: string[] = [];
function measure(style: any, text: string) {
  const c = (measure as any).c ?? ((measure as any).c = document.createElement("canvas").getContext("2d")!);
  c.font = `${style.italic ? "italic " : ""}${style.bold ? "bold " : ""}${style.fontsize}px "${style.fontname}", sans-serif`;
  const m = c.measureText(text); const sx = (style.scale_x ?? 100) / 100;
  return { w: m.width * sx + (style.spacing ?? 0) * [...text].length, h: style.fontsize * 1.15, d: style.fontsize * 0.2, el: 0 };
}
function runtime() {
  if (!rt) rt = new LuaRuntime({
    log: (_l: number, m: string) => { logBuf.push(m); }, dialog: luaDialog, textExtents: measure, get fps() { return store.fps; },
    get videoSize() { return video.videoWidth ? { w: video.videoWidth, h: video.videoHeight } : undefined; },
    project: {}, clipboard: { get: () => "", set: (s: string) => navigator.clipboard?.writeText(s) },
  } as any);
  return rt;
}
async function runMacro(m: MacroInfo) {
  logBuf = []; const r = runtime();
  const res = await r.run(m.id, store.file, store.selected, store.active);
  if (res.status === "done" && res.file) { store.replaceFile(res.file, res.sel); status(`${m.name}: tamamlandı`); }
  else if (res.status === "error") { logBuf.push("HATA: " + res.message); }
  else status(`${m.name}: iptal edildi`);
  if (logBuf.length) { const pre = document.createElement("pre"); pre.className = "log"; pre.textContent = logBuf.join("\n"); await modal(m.name + " — çıktı", pre, [{ label: "Tamam", value: "x", primary: true }]); }
}
$("b-auto").onclick = async () => {
  const r = runtime();
  const body = document.createElement("div"); body.className = "list";
  const draw = () => {
    body.innerHTML = "";
    if (!r.macros.length) body.textContent = "Yüklü makro yok. Aegisub Lua betiğinizi (.lua) yükleyin.";
    for (const m of r.macros) {
      const d = document.createElement("div"); d.className = "it"; d.innerHTML = `<b>${esc(m.name)}</b><span class="tag">${esc(m.script)}</span><span style="color:var(--dim)">${esc(m.desc)}</span>`;
      d.onclick = () => { document.querySelector(".modal-ov")?.remove(); runMacro(m); }; body.appendChild(d);
    }
    const add = document.createElement("button"); add.textContent = "＋ Lua betiği yükle…"; add.onclick = () => $<HTMLInputElement>("f-lua").click(); body.appendChild(add);
  };
  draw();
  $<HTMLInputElement>("f-lua").onchange = async (ev) => {
    for (const f of Array.from((ev.target as HTMLInputElement).files ?? [])) {
      const res = r.loadScript(f.name, await f.text());
      status(res.ok ? `${f.name}: ${res.macros.length} makro` : `${f.name}: ${res.message}`);
      if (!res.ok) { const pre = document.createElement("pre"); pre.className = "log"; pre.textContent = res.message ?? ""; modal("Betik hatası", pre, [{ label: "Tamam", value: "x" }]); }
    }
    (ev.target as HTMLInputElement).value = ""; draw();
  };
  await modal("Otomasyon", body, [{ label: "Kapat", value: "x", primary: true }]);
};

/* ---------- Klavye ---------- */
document.addEventListener("keydown", (e) => {
  const mod = e.ctrlKey || e.metaKey;
  if (mod && e.key.toLowerCase() === "z") { e.preventDefault(); e.shiftKey ? store.redo() : store.undo(); }
  else if (mod && e.key.toLowerCase() === "y") { e.preventDefault(); store.redo(); }
  else if (mod && e.key.toLowerCase() === "s") { e.preventDefault(); save(); }
  else if (mod && e.key.toLowerCase() === "o") { e.preventDefault(); $("b-open").click(); }
  else if (mod && e.key.toLowerCase() === "f") { e.preventDefault(); $("b-find").click(); }
  else if (mod && e.key === "Enter") { e.preventDefault(); const n = Math.min(store.active + 1, store.file.events.length - 1); store.select(n); ensureVisible(n); E.text.focus(); }
  else if ((e.target as HTMLElement).tagName !== "TEXTAREA" && (e.target as HTMLElement).tagName !== "INPUT") {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") { e.preventDefault(); const n = Math.max(0, Math.min(store.file.events.length - 1, store.active + (e.key === "ArrowDown" ? 1 : -1))); store.select(n, e.shiftKey ? "range" : "one"); ensureVisible(n); }
    else if (e.key === "Delete") $("e-del").click();
    else if (e.key === " " && video.src) { e.preventDefault(); video.paused ? video.play() : video.pause(); }
  }
});
window.addEventListener("beforeunload", (e) => { if (store.dirty) e.preventDefault(); });

store.on(() => { syncEdit(); renderGrid(); wave.draw(); updatePreview(video.currentTime * 1000); $("b-save").textContent = store.dirty ? "Kaydet (.ass) •" : "Kaydet (.ass)"; });
window.addEventListener("resize", () => { renderGrid(); wave.draw(); });
/* ---------- Otomatik kayıt / kurtarma (localStorage) ---------- */
const AUTOSAVE = "yoru.autosave";
setInterval(() => { try { if (store.dirty) localStorage.setItem(AUTOSAVE, JSON.stringify({ name: store.name, ass: serializeAss(store.file), t: Date.now() })); } catch { /* kota */ } }, 15000);
let recovered = false;
try {
  const raw = localStorage.getItem(AUTOSAVE);
  if (raw) { const o = JSON.parse(raw); store.load(parseAss(o.ass), o.name); store.dirty = true; recovered = true; setTimeout(() => status("Önceki oturum kurtarıldı — kaydetmeyi unutmayın"), 0); }
} catch { /* yok say */ }
if (!recovered) store.load(parseAss(SAMPLE()), "ornek.ass");
function SAMPLE() {
  return `[Script Info]\nScriptType: v4.00+\nPlayResX: 1920\nPlayResY: 1080\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,64,&H00FFFFFF,&H000000FF,&H00101010,&H80000000,0,0,0,0,100,100,0,0,1,3,1,2,40,40,50,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:03.50,Default,,0,0,0,,Yoru'ya hoş geldiniz.\nDialogue: 0,0:00:04.00,0:00:06.00,Default,,0,0,0,,{\\i1}Aegisub{\\i0} Lua betiklerinizi yükleyin,\\Nve gece boyu gelişen sürümleri deneyin.\n`;
}
