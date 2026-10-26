import {
  load, save, newDocument, defaultEvent, parseAssTime, formatAssTime, check, runAllFixes, shift, stretch,
  visibleText, serialize, stripTags,
} from "../src/core/index.js";

const $ = (s) => document.querySelector(s);
const ROW = 26;

const st = {
  doc: newDocument(), format: "ass", name: "untitled.ass",
  sel: new Set(), active: -1, undo: [], redo: [], issues: new Map(), luaScript: null, luaInfo: null, peaks: null, dur: 0,
};

// ---- undo ---------------------------------------------------------------
const snap = () => JSON.stringify({ e: st.doc.events, s: st.doc.styles });
function commit(label) {
  st.undo.push(st.last ?? snap()); if (st.undo.length > 200) st.undo.shift();
  st.redo.length = 0; st.last = snap(); st.dirty = true;
  refresh();
}
function restore(json) { const o = JSON.parse(json); st.doc.events = o.e; st.doc.styles = o.s; st.last = json; refresh(); }
function undo() { if (!st.undo.length) return; st.redo.push(snap()); restore(st.undo.pop()); }
function redo() { if (!st.redo.length) return; st.undo.push(snap()); restore(st.redo.pop()); }

// ---- grid ---------------------------------------------------------------
const grid = $("#grid"), fill = $("#grid-fill");
let rows = [];

function renderGrid() {
  const n = st.doc.events.length;
  fill.style.height = n * ROW + "px";
  const top = grid.scrollTop, h = grid.clientHeight;
  const a = Math.max(0, Math.floor(top / ROW) - 5), b = Math.min(n, Math.ceil((top + h) / ROW) + 5);
  const frag = document.createDocumentFragment();
  for (let i = a; i < b; i++) {
    const e = st.doc.events[i];
    const d = document.createElement("div");
    const iss = st.issues.get(i);
    d.className = "grow" + (st.sel.has(i) ? " sel" : "") + (i === st.active ? " act" : "") + (e.comment ? " comment" : "") +
      (iss ? (iss.some((x) => x.severity === "error") ? " err" : " warn") : "");
    d.style.top = i * ROW + "px";
    d.dataset.i = i;
    const dur = e.end - e.start, cps = dur > 0 ? visibleText(e.text).replace(/\s/g, "").length / (dur / 1000) : 0;
    d.innerHTML = `<span class="n">${i + 1}</span><span>${formatAssTime(e.start)}</span><span>${formatAssTime(e.end)}</span>` +
      `<span>${(dur / 1000).toFixed(2)}</span><span class="cps ${cps > 20 ? "hi" : ""}">${cps.toFixed(0)}</span><span>${esc(e.style)}</span><span>${esc(e.text)}</span>`;
    if (iss) d.title = iss.map((x) => x.message).join("\n");
    frag.append(d);
  }
  fill.replaceChildren(frag);
}
const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
grid.addEventListener("scroll", renderGrid);
new ResizeObserver(renderGrid).observe(grid);

grid.addEventListener("mousedown", (ev) => {
  const r = ev.target.closest(".grow"); if (!r) return;
  const i = +r.dataset.i;
  if (ev.shiftKey && st.active >= 0) { st.sel = new Set(); for (let k = Math.min(i, st.active); k <= Math.max(i, st.active); k++) st.sel.add(k); }
  else if (ev.ctrlKey || ev.metaKey) { st.sel.has(i) ? st.sel.delete(i) : st.sel.add(i); st.active = i; }
  else { st.sel = new Set([i]); st.active = i; }
  const e = st.doc.events[i]; if (e && !ev.ctrlKey && !ev.shiftKey && video.src) video.currentTime = e.start / 1000;
  fillEditor(); renderGrid(); drawWave();
});

function scrollTo(i) {
  const y = i * ROW;
  if (y < grid.scrollTop) grid.scrollTop = y; else if (y + ROW > grid.scrollTop + grid.clientHeight) grid.scrollTop = y - grid.clientHeight + ROW * 2;
}

// ---- editor -------------------------------------------------------------
const E = { start: $("#e-start"), end: $("#e-end"), dur: $("#e-dur"), style: $("#e-style"), actor: $("#e-actor"), layer: $("#e-layer"), comment: $("#e-comment"), text: $("#e-text"), info: $("#e-info") };
const cur = () => st.doc.events[st.active];

function fillEditor() {
  E.style.replaceChildren(...st.doc.styles.map((s) => new Option(s.name, s.name)));
  const e = cur();
  for (const k of Object.values(E)) k.disabled = !e;
  if (!e) { E.text.value = ""; E.info.textContent = ""; return; }
  E.start.value = formatAssTime(e.start); E.end.value = formatAssTime(e.end);
  E.dur.value = ((e.end - e.start) / 1000).toFixed(2);
  E.style.value = e.style; E.actor.value = e.actor; E.layer.value = e.layer; E.comment.checked = e.comment;
  if (document.activeElement !== E.text) E.text.value = e.text;
  updateInfo();
}
function updateInfo() {
  const e = cur(); if (!e) return;
  const v = visibleText(E.text.value), lines = E.text.value.split("\\N").map((l) => stripTags(l, { keepBreaks: true }).length);
  const dur = (e.end - e.start) / 1000, cps = dur > 0 ? v.replace(/\s/g, "").length / dur : 0;
  E.info.innerHTML = `<span class="${cps > 20 ? "bad" : ""}">${cps.toFixed(1)} cps</span> · ${lines.map((l) => `<span class="${l > 42 ? "bad" : ""}">${l}</span>`).join("+")} ch`;
}
let typing = null;
E.text.addEventListener("input", () => {
  const e = cur(); if (!e) return;
  e.text = E.text.value; updateInfo(); renderGrid(); updateOverlay();
  clearTimeout(typing); typing = setTimeout(() => commit("text"), 600);
});
const bindField = (el, apply) => el.addEventListener("change", () => { const e = cur(); if (!e) return; apply(e, el); commit(); fillEditor(); });
bindField(E.start, (e, el) => (e.start = parseAssTime(el.value)));
bindField(E.end, (e, el) => (e.end = parseAssTime(el.value)));
bindField(E.style, (e, el) => (e.style = el.value));
bindField(E.actor, (e, el) => (e.actor = el.value));
bindField(E.layer, (e, el) => (e.layer = +el.value || 0));
bindField(E.comment, (e, el) => (e.comment = el.checked));

// ---- actions ------------------------------------------------------------
const video = $("#video");
const selIdx = () => [...st.sel].sort((a, b) => a - b);

function addLine() {
  const at = st.active >= 0 ? st.active + 1 : st.doc.events.length;
  const t = video.src ? Math.round(video.currentTime * 1000) : (st.doc.events[at - 1]?.end ?? 0);
  st.doc.events.splice(at, 0, defaultEvent({ start: t, end: t + 2000, style: st.doc.styles[0]?.name ?? "Default" }));
  st.sel = new Set([at]); st.active = at; commit(); scrollTo(at); fillEditor(); E.text.focus();
}
function delLines() {
  const idx = selIdx(); if (!idx.length) return;
  st.doc.events = st.doc.events.filter((_, i) => !st.sel.has(i));
  st.active = Math.min(idx[0], st.doc.events.length - 1); st.sel = new Set(st.active >= 0 ? [st.active] : []);
  commit(); fillEditor();
}
function splitLine() {
  const e = cur(); if (!e) return;
  const p = E.text.selectionStart, a = e.text.slice(0, p).trim(), b = e.text.slice(p).trim();
  if (!a || !b) return;
  const mid = e.start + Math.round(((e.end - e.start) * a.length) / (a.length + b.length));
  const n = { ...e, start: mid, text: b }; e.end = mid; e.text = a;
  st.doc.events.splice(st.active + 1, 0, n); commit(); fillEditor();
}
function mergeLines() {
  const idx = selIdx(); if (idx.length < 2) return;
  const first = st.doc.events[idx[0]];
  first.text = idx.map((i) => st.doc.events[i].text).join("\\N");
  first.end = Math.max(...idx.map((i) => st.doc.events[i].end));
  st.doc.events = st.doc.events.filter((_, i) => !idx.slice(1).includes(i));
  st.sel = new Set([idx[0]]); st.active = idx[0]; commit(); fillEditor();
}
function duplicate() {
  const idx = selIdx(); if (!idx.length) return;
  const last = idx[idx.length - 1];
  st.doc.events.splice(last + 1, 0, ...idx.map((i) => ({ ...st.doc.events[i] })));
  commit();
}
function setFromVideo(which) {
  const e = cur(); if (!e || !video.src) return;
  e[which] = Math.round(video.currentTime * 1000); commit(); fillEditor();
}

$("#b-add").onclick = addLine; $("#b-del").onclick = delLines; $("#b-split").onclick = splitLine; $("#b-merge").onclick = mergeLines;
$("#b-undo").onclick = undo; $("#b-redo").onclick = redo;
$("#b-new").onclick = () => { if (st.dirty && !confirm("Kaydedilmemiş değişiklikler silinsin mi?")) return; setDoc(newDocument(), "ass", "untitled.ass"); };
$("#b-open").onclick = () => $("#f-open").click();
$("#f-open").onchange = async (ev) => {
  const f = ev.target.files[0]; if (!f) return;
  const buf = new Uint8Array(await f.arrayBuffer());
  let text = new TextDecoder("utf-8", { fatal: false }).decode(buf);
  if (text.includes("�")) { const enc = prompt("Dosya UTF-8 değil. Kodlama (windows-1254, iso-8859-9, shift_jis…):", "windows-1254"); if (enc) text = new TextDecoder(enc).decode(buf); }
  const { doc, format } = load(text, f.name);
  setDoc(doc, format, f.name); ev.target.value = "";
};
$("#b-save").onchange = (ev) => {
  const fmt = ev.target.value; if (!fmt) return;
  const out = save(st.doc, fmt);
  const a = document.createElement("a");
  a.href = URL.createObjectURL(new Blob([out], { type: "text/plain;charset=utf-8" }));
  a.download = st.name.replace(/\.[^.]+$/, "") + "." + fmt; a.click(); URL.revokeObjectURL(a.href);
  st.dirty = false; ev.target.value = "";
};
$("#b-video").onclick = () => $("#f-video").click();
$("#f-video").onchange = async (ev) => {
  const f = ev.target.files[0]; if (!f) return;
  video.src = URL.createObjectURL(f);
  status("Ses dalga biçimi çözümleniyor…");
  try { await decodePeaks(f); status(""); } catch (e) { status("Dalga biçimi çıkarılamadı: " + e.message); }
};

function setDoc(doc, format, name) {
  st.doc = doc; st.format = format; st.name = name; st.sel = new Set(); st.active = doc.events.length ? 0 : -1;
  st.undo = []; st.redo = []; st.last = snap(); st.dirty = false; st.issues = new Map();
  if (st.active >= 0) st.sel.add(0);
  refresh(); document.title = "osisub — " + name;
}
function refresh() { fillEditor(); renderGrid(); drawWave(); updateOverlay(); if (panelName === "qc") renderPanel(); }
function status(m) { $("#status").textContent = m; }

// ---- video overlay ------------------------------------------------------
function updateOverlay() {
  const t = video.currentTime * 1000;
  const lines = st.doc.events.filter((e) => !e.comment && e.start <= t && t < e.end && !/\\p[1-9]/.test(e.text));
  $("#overlay").innerHTML = lines.map((e) => {
    let h = esc(e.text.replace(/\{[^}]*\}/g, (b) => `\u0001${b}\u0002`));
    h = h.replace(/\u0001[^\u0002]*\u0002/g, (b) => (/\\i1/.test(b) ? "<i>" : /\\i0/.test(b) ? "</i>" : /\\b1/.test(b) ? "<b>" : /\\b0/.test(b) ? "</b>" : ""));
    return h.replace(/\\N/g, "<br>").replace(/\\h/g, "&nbsp;");
  }).join("<br>");
}
video.addEventListener("timeupdate", () => { updateOverlay(); drawWave(); });
(function loop() { if (!video.paused) { updateOverlay(); drawWave(); } requestAnimationFrame(loop); })();

// ---- waveform -----------------------------------------------------------
const wave = $("#wave"), wctx = wave.getContext("2d");
const view = { start: 0, span: 12 }; // seconds
async function decodePeaks(file) {
  const ac = new (window.AudioContext || window.webkitAudioContext)();
  const ab = await ac.decodeAudioData(await file.arrayBuffer());
  const ch = ab.getChannelData(0), per = Math.max(1, Math.floor(ab.sampleRate / 200)); // 200 peaks/s
  const n = Math.floor(ch.length / per), peaks = new Float32Array(n * 2);
  for (let i = 0; i < n; i++) { let mn = 1, mx = -1; for (let k = 0; k < per; k++) { const v = ch[i * per + k]; if (v < mn) mn = v; if (v > mx) mx = v; } peaks[i * 2] = mn; peaks[i * 2 + 1] = mx; }
  st.peaks = peaks; st.pps = 200; st.dur = ab.duration; ac.close(); drawWave();
}
function drawWave() {
  const w = wave.clientWidth, h = wave.clientHeight;
  if (wave.width !== w) wave.width = w;
  wctx.clearRect(0, 0, w, h);
  const t = video.currentTime || 0;
  if (t < view.start || t > view.start + view.span) view.start = Math.max(0, t - view.span * 0.25);
  const x = (s) => ((s - view.start) / view.span) * w;
  const cs = getComputedStyle(document.body);
  for (const [i, e] of st.doc.events.entries()) {
    const a = e.start / 1000, b = e.end / 1000;
    if (b < view.start || a > view.start + view.span) continue;
    wctx.fillStyle = st.sel.has(i) ? "rgba(90,169,255,.35)" : "rgba(160,160,160,.15)";
    wctx.fillRect(x(a), 0, x(b) - x(a), h);
  }
  if (st.peaks) {
    wctx.fillStyle = cs.getPropertyValue("--dim"); const mid = h / 2;
    for (let px = 0; px < w; px++) {
      const s0 = view.start + (px / w) * view.span, i = Math.floor(s0 * st.pps);
      if (i < 0 || i * 2 >= st.peaks.length) continue;
      const span = Math.max(1, Math.floor((view.span / w) * st.pps));
      let mn = 1, mx = -1;
      for (let k = 0; k < span && (i + k) * 2 < st.peaks.length; k++) { mn = Math.min(mn, st.peaks[(i + k) * 2]); mx = Math.max(mx, st.peaks[(i + k) * 2 + 1]); }
      wctx.fillRect(px, mid - mx * mid, 1, Math.max(1, (mx - mn) * mid));
    }
  }
  wctx.fillStyle = "#ef6b6b"; wctx.fillRect(x(t), 0, 1.5, h);
  $("#wave-info").textContent = `${t.toFixed(2)}s  ·  yakınlaştır: tekerlek  ·  kenarları sürükle`;
}
wave.addEventListener("wheel", (ev) => {
  ev.preventDefault();
  const w = wave.clientWidth, anchor = view.start + (ev.offsetX / w) * view.span;
  view.span = Math.min(120, Math.max(1, view.span * (ev.deltaY > 0 ? 1.2 : 0.83)));
  view.start = Math.max(0, anchor - (ev.offsetX / w) * view.span); drawWave();
}, { passive: false });
let drag = null;
wave.addEventListener("mousedown", (ev) => {
  const w = wave.clientWidth, t = view.start + (ev.offsetX / w) * view.span, tol = (view.span / w) * 6;
  for (const i of [st.active, ...st.doc.events.keys()]) {
    const e = st.doc.events[i]; if (!e) continue;
    if (Math.abs(e.start / 1000 - t) < tol) { drag = { i, f: "start" }; break; }
    if (Math.abs(e.end / 1000 - t) < tol) { drag = { i, f: "end" }; break; }
  }
  if (!drag && video.src) { video.currentTime = t; drawWave(); }
});
window.addEventListener("mousemove", (ev) => {
  if (!drag) return;
  const r = wave.getBoundingClientRect(), t = view.start + ((ev.clientX - r.left) / r.width) * view.span;
  st.doc.events[drag.i][drag.f] = Math.max(0, Math.round(t * 100) * 10); drawWave(); if (drag.i === st.active) fillEditor();
});
window.addEventListener("mouseup", () => { if (drag) { drag = null; commit(); } });

// ---- panels -------------------------------------------------------------
let panelName = null;
const panel = $("#panel");
document.querySelectorAll("[data-panel]").forEach((b) => (b.onclick = () => { panelName = panelName === b.dataset.panel ? null : b.dataset.panel; renderPanel(); }));

function renderPanel() {
  panel.hidden = !panelName;
  if (!panelName) return;
  const P = panelFns[panelName]; panel.replaceChildren(); P(panel);
}
const h = (html) => { const d = document.createElement("div"); d.innerHTML = html; return d; };

const panelFns = {
  qc(p) {
    const issues = check(st.doc);
    st.issues = new Map();
    for (const i of issues) (st.issues.get(i.index) ?? st.issues.set(i.index, []).get(i.index)).push(i);
    p.append(h(`<h3>Kontrol — ${issues.length} sorun <button id="qc-fix">Yaygın hataları düzelt</button></h3>`));
    const list = h(""); list.style.maxHeight = "220px"; list.style.overflow = "auto";
    for (const i of issues.slice(0, 500)) {
      const d = h(`#${i.index + 1} ${i.code}: ${esc(i.message)}`); d.className = "issue " + i.severity;
      d.onclick = () => { st.sel = new Set([i.index]); st.active = i.index; scrollTo(i.index); fillEditor(); renderGrid(); };
      list.append(d);
    }
    p.append(list);
    p.querySelector("#qc-fix").onclick = () => { const r = runAllFixes(st.doc); commit(); status("Düzeltildi: " + JSON.stringify(r)); };
    renderGrid();
  },
  find(p) {
    p.append(h(`<h3>Bul / Değiştir</h3><div class="row"><input id="f-q" placeholder="Ara" size="18"><input id="f-r" placeholder="Değiştir" size="18"><label><input type="checkbox" id="f-re"> regex</label><label><input type="checkbox" id="f-i" checked> büyük/küçük harf yok</label><label><input type="checkbox" id="f-sel"> yalnız seçili</label><button id="f-go">Hepsini değiştir</button><button id="f-next">Sonrakini bul</button></div><div id="f-out"></div>`));
    const mk = () => { const q = p.querySelector("#f-q").value; if (!q) return null; return new RegExp(p.querySelector("#f-re").checked ? q : q.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "g" + (p.querySelector("#f-i").checked ? "i" : "")); };
    p.querySelector("#f-go").onclick = () => {
      let re; try { re = mk(); } catch (e) { return status(e.message); } if (!re) return;
      const only = p.querySelector("#f-sel").checked; let n = 0; const rep = p.querySelector("#f-r").value;
      st.doc.events.forEach((e, i) => { if (only && !st.sel.has(i)) return; const t = e.text.replace(re, () => (n++, rep)); if (t !== e.text) e.text = t; });
      if (n) commit(); p.querySelector("#f-out").textContent = n + " değişiklik";
    };
    p.querySelector("#f-next").onclick = () => {
      let re; try { re = mk(); } catch (e) { return status(e.message); } if (!re) return;
      const n = st.doc.events.length;
      for (let k = 1; k <= n; k++) { const i = (st.active + k) % n; re.lastIndex = 0; if (re.test(st.doc.events[i].text)) { st.sel = new Set([i]); st.active = i; scrollTo(i); fillEditor(); renderGrid(); return; } }
      p.querySelector("#f-out").textContent = "Bulunamadı";
    };
  },
  time(p) {
    p.append(h(`<h3>Zamanlama</h3>
      <div class="row"><label>Kaydır (ms) <input id="t-ms" type="number" value="0" style="width:7em"></label>
      <label><select id="t-w"><option value="sel">seçili satırlar</option><option value="all">tümü</option></select></label><label><select id="t-f"><option value="both">başl.+bitiş</option><option value="start">başlangıç</option><option value="end">bitiş</option></select></label><button id="t-go">Uygula</button></div>
      <div class="row"><label>Senkron: <input id="s-a0" size="11" placeholder="ilk (eski)"> → <input id="s-b0" size="11" placeholder="ilk (yeni)"></label>
      <label><input id="s-a1" size="11" placeholder="son (eski)"> → <input id="s-b1" size="11" placeholder="son (yeni)"></label><button id="s-go">Doğrusal ayarla</button></div>
      <div class="row"><button id="t-start">Başlangıcı videodan al (Alt+S)</button><button id="t-end">Bitişi videodan al (Alt+E)</button></div>`));
    p.querySelector("#t-go").onclick = () => {
      const all = p.querySelector("#t-w").value === "all";
      shift(st.doc, +p.querySelector("#t-ms").value || 0, { indices: all ? null : selIdx(), field: p.querySelector("#t-f").value }); commit();
    };
    p.querySelector("#s-go").onclick = () => {
      const v = (id) => parseAssTime(p.querySelector(id).value);
      const [a0, a1, b0, b1] = [v("#s-a0"), v("#s-a1"), v("#s-b0"), v("#s-b1")];
      if (a1 === a0) return status("Geçersiz aralık"); stretch(st.doc, a0, a1, b0, b1); commit();
    };
    p.querySelector("#t-start").onclick = () => setFromVideo("start"); p.querySelector("#t-end").onclick = () => setFromVideo("end");
  },
  styles(p) {
    p.append(h(`<h3>Stiller</h3>`));
    for (const s of st.doc.styles) {
      const d = h(`<div class="row"><b style="min-width:7em">${esc(s.name)}</b>
        <label>Font <input data-k="fontname" value="${esc(s.fontname)}" size="12"></label>
        <label>Boyut <input data-k="fontsize" type="number" value="${s.fontsize}" style="width:4.5em"></label>
        <label>Hiza <input data-k="align" type="number" value="${s.align}" style="width:3.5em"></label>
        <label>Kenar <input data-k="outline" type="number" step="0.5" value="${s.outline}" style="width:4em"></label>
        <label>Gölge <input data-k="shadow" type="number" step="0.5" value="${s.shadow}" style="width:4em"></label>
        <label>MarjV <input data-k="margin_t" type="number" value="${s.margin_t}" style="width:4em"></label></div>`);
      d.querySelectorAll("input").forEach((inp) => (inp.onchange = () => { const k = inp.dataset.k; s[k] = inp.type === "number" ? +inp.value : inp.value; if (k === "margin_t") s.margin_b = s.margin_t; commit(); }));
      p.append(d);
    }
    const add = h(`<div class="row"><input id="ns" placeholder="Yeni stil adı"><button>Ekle</button></div>`);
    add.querySelector("button").onclick = () => { const n = add.querySelector("input").value.trim(); if (!n || st.doc.styles.some((s) => s.name === n)) return; st.doc.styles.push({ ...st.doc.styles[0], name: n }); commit(); renderPanel(); };
    p.append(add);
  },
  lua(p) {
    p.append(h(`<h3>Lua Automation (Aegisub uyumlu)</h3>
      <div class="row"><button id="l-load">Lua dosyası yükle…</button><span id="l-name" style="color:var(--dim)">${st.luaInfo ? esc(st.luaInfo.script_name ?? "script") : "—"}</span></div>
      <div class="row"><select id="l-macro"></select><button id="l-run">Çalıştır</button><label><input type="checkbox" id="l-sel" checked> yalnız seçili satırlar</label></div>
      <div class="row"><label>Diyalog değerleri (JSON, ör. {"n":5}) <input id="l-vals" size="30" value="{}"></label></div>
      <pre class="log" id="l-log"></pre>`));
    const sel = p.querySelector("#l-macro");
    if (st.luaInfo) for (const m of [...st.luaInfo.macros, ...st.luaInfo.filters]) sel.append(new Option(m, m));
    p.querySelector("#l-load").onclick = () => $("#f-lua").click();
    p.querySelector("#l-run").onclick = async () => {
      if (!st.luaScript) return;
      const only = p.querySelector("#l-sel").checked;
      const log = p.querySelector("#l-log"); log.textContent = "çalışıyor…";
      const responses = [];
      try { const extra = JSON.parse(p.querySelector("#l-vals").value || "{}"); if (Object.keys(extra).length) responses.push({ values: extra }); } catch { return status("Diyalog JSON geçersiz"); }
      const preset = responses.length;
      try {
        for (let guard = 0; guard < 20; guard++) {
          const body = { text: serialize(st.doc), script: st.luaScript, scriptPath: st.luaPath, macro: sel.value, selected: only && st.sel.size ? selIdx() : null, active: st.active >= 0 ? st.active : null, dialog: { responses }, fps: 23.976 };
          const r = await post("/api/run", body);
          if (r.needsDialog) {
            if (r.needsDialog.index < preset) { responses.push({}); continue; }
            const ans = await showDialog(r.needsDialog);
            responses.push(ans);
            if (ans.button === "cancel") { log.textContent = "iptal edildi"; return; }
            continue;
          }
          log.textContent = r.logs.map((l) => l.message).join("") || (r.cancelled ? "iptal edildi" : "tamam");
          if (!r.cancelled) { const { doc } = load(r.text, "x.ass"); st.doc.events = doc.events; st.doc.styles = doc.styles; st.doc.info = doc.info; commit(); }
          return;
        }
      } catch (e) { log.textContent = "Hata: " + e.message; }
    };
  },
};
async function post(url, body) {
  const x = await fetch(url, { method: "POST", body: JSON.stringify(body) });
  const j = await x.json(); if (!x.ok) throw new Error(j.error); return j;
}

/** Render an Aegisub dialog definition as a form; resolves {values, button}. */
function showDialog({ controls, buttons, buttonIds }) {
  return new Promise((resolve) => {
    const dlg = document.createElement("dialog");
    const form = document.createElement("form"); form.method = "dialog";
    const g = document.createElement("div");
    g.style.cssText = "display:grid;grid-template-columns:repeat(auto-fill,minmax(70px,1fr));gap:6px;min-width:420px;max-width:80vw";
    const readers = {};
    for (const c of controls) {
      const wrap = document.createElement("div");
      wrap.style.gridColumn = `${(c.x ?? 0) + 1} / span ${c.width ?? 1}`; wrap.style.gridRow = `${(c.y ?? 0) + 1} / span ${c.height ?? 1}`;
      let el;
      switch (c.class) {
        case "label": el = document.createElement("span"); el.textContent = c.label ?? ""; break;
        case "checkbox": el = document.createElement("label"); const cb = document.createElement("input"); cb.type = "checkbox"; cb.checked = !!c.value; el.append(cb, " " + (c.label ?? "")); readers[c.name] = () => cb.checked; break;
        case "dropdown": { el = document.createElement("select"); for (const it of c.items ?? []) el.append(new Option(it, it)); el.value = c.value ?? (c.items ?? [])[0]; readers[c.name] = () => el.value; break; }
        case "textbox": el = document.createElement("textarea"); el.rows = 4; el.value = c.value ?? c.text ?? ""; readers[c.name] = () => el.value; break;
        case "intedit": case "floatedit": case "slider": el = document.createElement("input"); el.type = c.class === "slider" ? "range" : "number"; if (c.class === "floatedit") el.step = c.step ?? "any"; if (c.min !== undefined) el.min = c.min; if (c.max !== undefined) el.max = c.max; el.value = c.value ?? c.min ?? 0; readers[c.name] = () => Number(el.value); break;
        case "color": case "coloralpha": case "alpha": default: el = document.createElement("input"); el.value = c.value ?? c.text ?? ""; readers[c.name] = () => el.value;
      }
      if (c.hint) el.title = c.hint;
      if (el.tagName === "INPUT" || el.tagName === "SELECT" || el.tagName === "TEXTAREA") el.style.width = "100%";
      wrap.append(el); g.append(wrap);
    }
    const bar = document.createElement("div"); bar.style.cssText = "display:flex;gap:8px;justify-content:flex-end;margin-top:10px";
    const done = (button) => { const values = {}; for (const [k, f] of Object.entries(readers)) if (k !== "undefined") values[k] = f(); dlg.close(); dlg.remove(); resolve({ values, button }); };
    const list = buttons?.length ? buttons : ["OK", "Cancel"];
    const okName = buttonIds?.ok ?? list[0], cancelName = buttonIds?.cancel ?? list[list.length - 1];
    for (const b of list) { const btn = document.createElement("button"); btn.type = "button"; btn.textContent = b; btn.onclick = () => done(b === cancelName && list.length > 1 && b !== okName ? "cancel" : b); bar.append(btn); }
    dlg.addEventListener("cancel", (e) => { e.preventDefault(); done("cancel"); });
    form.append(g, bar); dlg.append(form); document.body.append(dlg); dlg.showModal();
  });
}

$("#f-lua").onchange = async (ev) => {
  const f = ev.target.files[0]; if (!f) return;
  st.luaScript = await f.text(); st.luaPath = f.name;
  try { st.luaInfo = await fetch("/api/macros", { method: "POST", body: JSON.stringify({ script: st.luaScript, scriptPath: f.name }) }).then(async (x) => { const j = await x.json(); if (!x.ok) throw new Error(j.error); return j; }); }
  catch (e) { st.luaInfo = null; status("Lua hatası: " + e.message); }
  panelName = "lua"; renderPanel(); ev.target.value = "";
};

// ---- keyboard -----------------------------------------------------------
window.addEventListener("keydown", (ev) => {
  const mod = ev.ctrlKey || ev.metaKey, inText = ["TEXTAREA", "INPUT", "SELECT"].includes(document.activeElement.tagName);
  if (mod && ev.key === "z") { ev.preventDefault(); undo(); }
  else if (mod && (ev.key === "y" || (ev.shiftKey && ev.key.toLowerCase() === "z"))) { ev.preventDefault(); redo(); }
  else if (mod && ev.key === "s") { ev.preventDefault(); $("#b-save").value = st.format === "txt" ? "ass" : st.format; $("#b-save").onchange({ target: $("#b-save") }); }
  else if (mod && ev.key === "o") { ev.preventDefault(); $("#f-open").click(); }
  else if (mod && ev.key === "Enter") { ev.preventDefault(); if (st.active < st.doc.events.length - 1) { st.active++; st.sel = new Set([st.active]); scrollTo(st.active); fillEditor(); renderGrid(); E.text.focus(); if (video.src) video.currentTime = cur().start / 1000; } else addLine(); }
  else if (ev.altKey && ev.key.toLowerCase() === "s") { ev.preventDefault(); setFromVideo("start"); }
  else if (ev.altKey && ev.key.toLowerCase() === "e") { ev.preventDefault(); setFromVideo("end"); }
  else if (mod && ev.key.toLowerCase() === "d") { ev.preventDefault(); duplicate(); }
  else if (!inText && ev.key === "Delete") delLines();
  else if (!inText && ev.key === " " && video.src) { ev.preventDefault(); video.paused ? video.play() : video.pause(); }
  else if (!inText && (ev.key === "ArrowDown" || ev.key === "ArrowUp")) {
    ev.preventDefault();
    const i = Math.max(0, Math.min(st.doc.events.length - 1, st.active + (ev.key === "ArrowDown" ? 1 : -1)));
    st.active = i; st.sel = new Set([i]); scrollTo(i); fillEditor(); renderGrid(); if (video.src) video.currentTime = cur().start / 1000;
  }
});
window.addEventListener("beforeunload", (ev) => { if (st.dirty) { ev.preventDefault(); ev.returnValue = ""; } });

window.osisub = { st, setDoc, commit }; // handy for scripting / tests
setDoc(newDocument(), "ass", "untitled.ass");
addLine();
st.dirty = false;
