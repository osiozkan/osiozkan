import { parseAss, writeAss, newScript, defaultDialogue, defaultStyle, getInfo, setInfo, STYLE_FIELDS } from '/src/core/ass.js';
import { parseSrt, writeSrt, writeVtt, writePlain } from '/src/core/srt.js';
import { parseAssTime, formatAssTime, msToFrame, frameToMs } from '/src/core/time.js';
import { stripTags, cps as calcCps } from '/src/core/tags.js';
import { runQC } from '/src/core/qc.js';
import { balanceText, splitLongLines, mergeShortLines, removeHI, fixPunctuation, removeDuplicates, enforceGap } from '/src/core/fixes.js';
import JASSUB from '/web/vendor/jassub.js';

const $ = s => document.querySelector(s);
const S = { lines: newScript(), sel: new Set(), active: -1, fileName: 'untitled.ass', dirty: false, undo: [], redo: [], fps: 23.976, scripts: [], issues: new Map(), videoName: '' };
const ROW = 24;
const clone = o => JSON.parse(JSON.stringify(o));
const status = m => { $('#status').textContent = m; };
const esc = s => String(s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

/* ------------------------------------------------------------ model helpers */
const isEv = l => l.class === 'dialogue';
// "view" = indices into S.lines of dialogue lines, in file order
const view = () => { const v = []; S.lines.forEach((l, i) => isEv(l) && v.push(i)); return v; };
const styleNames = () => S.lines.filter(l => l.class === 'style').map(l => l.name);
function commit(label) { // push undo snapshot BEFORE mutation: call snapshot() first
  S.dirty = true; S.redo = []; refresh();
}
function snapshot() { S.undo.push(JSON.stringify({ l: S.lines, s: [...S.sel], a: S.active })); if (S.undo.length > 300) S.undo.shift(); }
function restore(js) { const o = JSON.parse(js); S.lines = o.l; S.sel = new Set(o.s); S.active = o.a; refresh(); }
function undo() { if (!S.undo.length) return; S.redo.push(JSON.stringify({ l: S.lines, s: [...S.sel], a: S.active })); restore(S.undo.pop()); }
function redo() { if (!S.redo.length) return; S.undo.push(JSON.stringify({ l: S.lines, s: [...S.sel], a: S.active })); restore(S.redo.pop()); }
function mutate(fn) { snapshot(); fn(); S.dirty = true; S.redo = []; refresh(); }
const selected = () => view().filter(i => S.sel.has(i));
const selOrAll = () => (S.sel.size ? selected() : view());

/* ------------------------------------------------------------ grid */
const gh = $('#gridHead'), gs = $('#gridScroll'), gr = $('#gridRows'), gsp = $('#gridSpacer');
gh.innerHTML = ['#', 'Start', 'End', 'Dur', 'Style', 'Actor', 'CPS', 'Text'].map(h => `<div>${h}</div>`).join('');
function renderGrid() {
  const v = view(); gsp.style.height = v.length * ROW + 'px';
  const top = Math.max(0, Math.floor(gs.scrollTop / ROW) - 5), cnt = Math.ceil(gs.clientHeight / ROW) + 10;
  gr.style.top = top * ROW + 'px'; let h = '';
  for (let k = top; k < Math.min(v.length, top + cnt); k++) {
    const i = v[k], l = S.lines[i], dur = l.end_time - l.start_time, c = calcCps(l.text, dur), iss = S.issues.get(i);
    const sev = c > 30 ? 'bad' : c > 20 ? 'warn' : '';
    h += `<div class="grow ${S.sel.has(i) ? 'sel' : ''} ${S.active === i ? 'active' : ''} ${l.comment ? 'comment' : ''} ${sev}" data-i="${i}" title="${iss ? esc(iss.map(x => x.message).join('; ')) : ''}">` +
      `<div class="num">${k + 1}</div><div>${formatAssTime(l.start_time)}</div><div>${formatAssTime(l.end_time)}</div><div>${(dur / 1000).toFixed(2)}</div>` +
      `<div>${esc(l.style)}</div><div>${esc(l.actor)}</div><div class="cps">${isFinite(c) ? c.toFixed(0) : '∞'}</div><div class="txt">${esc(l.text)}</div></div>`;
  }
  gr.innerHTML = h;
}
gs.addEventListener('scroll', renderGrid);
gr.addEventListener('mousedown', e => {
  const r = e.target.closest('.grow'); if (!r) return; const i = +r.dataset.i, v = view();
  if (e.shiftKey && S.active >= 0) { const a = v.indexOf(S.active), b = v.indexOf(i); S.sel = new Set(v.slice(Math.min(a, b), Math.max(a, b) + 1)); }
  else if (e.ctrlKey || e.metaKey) { S.sel.has(i) ? S.sel.delete(i) : S.sel.add(i); S.active = i; }
  else { S.sel = new Set([i]); S.active = i; seekToLine(i); }
  refresh(false);
});
function seekToLine(i) { const vd = $('#video'); if (vd.src && !vd.dataset.playing) vd.currentTime = S.lines[i].start_time / 1000; }
gr.addEventListener('dblclick', e => { const r = e.target.closest('.grow'); if (r) { playRange(S.lines[+r.dataset.i]); } });
function scrollToActive() { const k = view().indexOf(S.active); if (k < 0) return; const y = k * ROW; if (y < gs.scrollTop) gs.scrollTop = y; else if (y + ROW > gs.scrollTop + gs.clientHeight) gs.scrollTop = y + ROW - gs.clientHeight; }
function moveActive(d, extend) {
  const v = view(); if (!v.length) return; let k = v.indexOf(S.active); k = Math.max(0, Math.min(v.length - 1, (k < 0 ? 0 : k) + d));
  const i = v[k]; if (extend) S.sel.add(i); else S.sel = new Set([i]); S.active = i; refresh(false); scrollToActive(); renderGrid(); seekToLine(i);
}

/* ------------------------------------------------------------ edit pane */
const E = { style: $('#eStyle'), actor: $('#eActor'), layer: $('#eLayer'), start: $('#eStart'), end: $('#eEnd'), dur: $('#eDur'), ml: $('#eML'), mr: $('#eMR'), mv: $('#eMV'), effect: $('#eEffect'), comment: $('#eComment'), text: $('#eText') };
function fillEdit() {
  const l = S.lines[S.active]; const ok = l && isEv(l);
  for (const el of Object.values(E)) el.disabled = !ok;
  E.style.innerHTML = styleNames().map(n => `<option>${esc(n)}</option>`).join('');
  if (!ok) { E.text.value = ''; $('#eInfo').textContent = ''; return; }
  if (document.activeElement !== E.text) E.text.value = l.text;
  E.style.value = l.style; E.actor.value = l.actor; E.layer.value = l.layer; E.start.value = formatAssTime(l.start_time); E.end.value = formatAssTime(l.end_time);
  E.dur.value = ((l.end_time - l.start_time) / 1000).toFixed(2); E.ml.value = l.margin_l; E.mr.value = l.margin_r; E.mv.value = l.margin_t; E.effect.value = l.effect; E.comment.checked = l.comment;
  const c = calcCps(l.text, l.end_time - l.start_time);
  $('#eInfo').textContent = `${stripTags(l.text).length} chars · ${isFinite(c) ? c.toFixed(1) : '∞'} cps`;
}
let editSnap = false;
function bindEdit(el, apply, ev = 'input') {
  el.addEventListener(ev, () => {
    const l = S.lines[S.active]; if (!l) return;
    if (!editSnap) { snapshot(); editSnap = true; } apply(l, el); S.dirty = true; S.redo = []; refresh(false, el);
  });
  el.addEventListener('blur', () => { editSnap = false; });
}
const parseT = s => (/^\d+:\d+:\d+[.]\d+$/.test(s) ? parseAssTime(s) : null);
bindEdit(E.text, (l, e) => { l.text = e.value; });
bindEdit(E.style, (l, e) => { l.style = e.value; }, 'change');
bindEdit(E.actor, (l, e) => { l.actor = e.value; }); bindEdit(E.effect, (l, e) => { l.effect = e.value; });
bindEdit(E.layer, (l, e) => { l.layer = +e.value || 0; }); bindEdit(E.ml, (l, e) => { l.margin_l = +e.value || 0; });
bindEdit(E.mr, (l, e) => { l.margin_r = +e.value || 0; }); bindEdit(E.mv, (l, e) => { l.margin_t = +e.value || 0; });
bindEdit(E.comment, (l, e) => { l.comment = e.checked; }, 'change');
bindEdit(E.start, (l, e) => { const t = parseT(e.value); if (t != null) l.start_time = t; }, 'change');
bindEdit(E.end, (l, e) => { const t = parseT(e.value); if (t != null) l.end_time = t; }, 'change');
bindEdit(E.dur, (l, e) => { const d = parseFloat(e.value); if (d >= 0) l.end_time = l.start_time + Math.round(d * 1000); }, 'change');
E.text.addEventListener('keydown', e => {
  if (e.key === 'Enter' && e.ctrlKey) { e.preventDefault(); act('insert-after'); }
  else if (e.key === 'Enter' && e.shiftKey) { e.preventDefault(); const p = E.text.selectionStart; E.text.setRangeText('\\N', p, E.text.selectionEnd, 'end'); E.text.dispatchEvent(new Event('input')); }
  else if (e.key === 'Enter') { e.preventDefault(); moveActive(1); E.text.focus(); }
  else if (e.altKey && e.key === 'ArrowDown') { e.preventDefault(); moveActive(1); }
  else if (e.altKey && e.key === 'ArrowUp') { e.preventDefault(); moveActive(-1); }
});

/* ------------------------------------------------------------ refresh */
function refresh(full = true, from) {
  if (full || S.issueDirty) S.issues = groupIssues(runQC(S.lines));
  renderGrid(); if (from !== E.text) fillEdit(); else { const l = S.lines[S.active]; if (l) $('#eInfo').textContent = `${stripTags(l.text).length} chars`; }
  const n = view().length; document.title = `${S.dirty ? '• ' : ''}${S.fileName} – OsiSub`;
  status(`${n} lines · ${S.sel.size} selected · ${S.fps} fps${S.videoName ? ' · ' + S.videoName : ''}`);
  updateOverlay(); pushToRenderer();
}
function groupIssues(list) { const m = new Map(); for (const i of list) (m.get(i.index) ?? m.set(i.index, []).get(i.index)).push(i); return m; }

/* ------------------------------------------------------------ modal */
function modal(title, bodyHtml, buttons = ['OK', 'Cancel']) {
  return new Promise(resolve => {
    const box = $('#modalBox'); box.innerHTML = `<h3>${esc(title)}</h3><div class="body">${bodyHtml}</div><div class="btns">${buttons.map(b => `<button data-b="${esc(b)}">${esc(b)}</button>`).join('')}</div>`;
    $('#modal').hidden = false;
    const done = v => { $('#modal').hidden = true; box.onclick = null; box.onkeydown = null; resolve(v); };
    box.onclick = e => { const b = e.target.closest('button[data-b]'); if (b) done({ button: b.dataset.b, box }); };
    box.onkeydown = e => { if (e.key === 'Escape') done({ button: buttons[buttons.length - 1], box, cancelled: true }); else if (e.key === 'Enter' && e.target.tagName !== 'TEXTAREA' && e.target.tagName !== 'BUTTON') done({ button: buttons[0], box }); };
    (box.querySelector('input,select,textarea') || box.querySelector('button')).focus();
  });
}
const val = (box, n) => box.querySelector(`[name="${n}"]`);

/* ------------------------------------------------------------ file io */
function pickFile(input) { return new Promise(res => { input.value = ''; input.onchange = () => res([...input.files]); input.click(); }); }
function download(name, text, type = 'text/plain') { const a = document.createElement('a'); a.href = URL.createObjectURL(new Blob([text], { type: type + ';charset=utf-8' })); a.download = name; a.click(); setTimeout(() => URL.revokeObjectURL(a.href), 2000); }
async function decodeText(file) { // detect BOM / utf-16 / fall back to utf-8
  const buf = new Uint8Array(await file.arrayBuffer());
  if (buf[0] === 0xFF && buf[1] === 0xFE) return new TextDecoder('utf-16le').decode(buf);
  if (buf[0] === 0xFE && buf[1] === 0xFF) return new TextDecoder('utf-16be').decode(buf);
  try { return new TextDecoder('utf-8', { fatal: true }).decode(buf); } catch { return new TextDecoder('windows-1254').decode(buf); }
}
function loadSubtitleText(text, name) {
  snapshot();
  if (/\.ass$|\.ssa$/i.test(name) || /^\s*\[Script Info\]/i.test(text)) S.lines = parseAss(text);
  else { const ev = parseSrt(text); S.lines = newScript(); S.lines.push(...ev); }
  S.fileName = name.replace(/\.(srt|vtt|ssa)$/i, '.ass'); S.sel = new Set(); S.active = view()[0] ?? -1; S.dirty = false;
  const fps = getInfo(S.lines, 'Video FPS'); refresh(); status(`Loaded ${name}`);
}

/* ------------------------------------------------------------ video / waveform / overlay */
const vd = $('#video'), wave = $('#wave'); let peaks = null, audioDur = 0, zoomStart = 0, zoomLen = 30;
function updateOverlay() {
  const t = vd.currentTime * 1000; const act = S.lines.filter(l => isEv(l) && !l.comment && l.start_time <= t && l.end_time > t);
  $('#overlay').textContent = (act.length ? act : (vd.src ? [] : [S.lines[S.active]].filter(l => l && isEv(l)))).map(l => stripTags(l.text).replace(/\\N/g, '\n').replace(/\\n/g, ' ')).join('\n');
  $('#clock').textContent = formatAssTime(t).replace(/(\.\d\d)$/, '$1') + ` · f${msToFrame(t, S.fps)}`;
  drawWave();
}
vd.addEventListener('timeupdate', updateOverlay);
let renderBusy = false;
async function renderNow() { // explicit repaint: paused video / seeks don't always trigger requestVideoFrameCallback
  if (!jassub || renderBusy || !vd.videoWidth) return; renderBusy = true;
  try { await jassub.ready; await jassub.resize(true); await jassub.manualRender({ mediaTime: vd.currentTime, width: vd.videoWidth, height: vd.videoHeight, expectedDisplayTime: performance.now() }, true); }
  catch (e) { console.warn(e); } finally { renderBusy = false; }
}
vd.addEventListener('seeked', () => { updateOverlay(); renderNow(); }); vd.addEventListener('loadeddata', renderNow);
function playRange(l) { vd.currentTime = l.start_time / 1000; vd.play(); const stop = () => { if (vd.currentTime * 1000 >= l.end_time) { vd.pause(); vd.removeEventListener('timeupdate', stop); } }; vd.addEventListener('timeupdate', stop); }
let jassub = null, jassubTimer = null, fontMap = null;
async function startRenderer() {
  try {
    fontMap ??= await (await fetch('/api/fonts')).json();
    if (jassub) await jassub.destroy();
    $('#overlay').hidden = true;
    jassub = new JASSUB({ video: vd, subContent: writeAss(S.lines), availableFonts: fontMap, workerUrl: '/web/vendor/jassub-worker.js',
      wasmUrl: '/web/vendor/jassub-worker.wasm', modernWasmUrl: '/web/vendor/jassub-worker-modern.wasm', queryFonts: false });
  } catch (e) { console.warn('ASS renderer unavailable, using text overlay', e); jassub = null; $('#overlay').hidden = false; }
}
function pushToRenderer() {
  if (!jassub) return; clearTimeout(jassubTimer);
  jassubTimer = setTimeout(async () => { try { await jassub.ready; await jassub.renderer.setTrack(writeAss(S.lines)); await renderNow(); } catch (e) { console.warn(e); } }, 120);
}
async function openMedia(file) {
  vd.src = URL.createObjectURL(file); S.videoName = file.name; setTimeout(refresh, 300); startRenderer();
  try { const ac = new (window.OfflineAudioContext || window.AudioContext)(1, 1, 44100); const ab = await ac.decodeAudioData(await file.arrayBuffer());
    const data = ab.getChannelData(0), N = 4000, step = Math.max(1, Math.floor(data.length / N)); peaks = new Float32Array(N);
    for (let i = 0; i < N; i++) { let m = 0; for (let j = 0; j < step; j++) m = Math.max(m, Math.abs(data[i * step + j] || 0)); peaks[i] = m; }
    audioDur = ab.duration; zoomLen = 30; drawWave(); } catch (e) { peaks = null; }
}
function drawWave() {
  const c = wave, w = c.width = c.clientWidth, h = c.height = 90, g = c.getContext('2d'); g.fillStyle = '#101219'; g.fillRect(0, 0, w, h);
  const t0 = zoomStart, len = zoomLen; const cur = vd.currentTime || 0;
  if (cur < zoomStart || cur > zoomStart + zoomLen) zoomStart = Math.max(0, cur - 2);
  if (peaks && audioDur) { g.fillStyle = '#3a6ea5'; for (let x = 0; x < w; x++) { const p = peaks[Math.min(peaks.length - 1, Math.floor(((t0 + (x / w) * len) / audioDur) * peaks.length))] || 0; const a = p * h * 0.9; g.fillRect(x, (h - a) / 2, 1, a); } }
  const X = t => ((t / 1000 - t0) / len) * w;
  for (const l of S.lines) if (isEv(l) && l.end_time / 1000 > t0 && l.start_time / 1000 < t0 + len) { g.fillStyle = S.lines[S.active] === l ? '#ff6b9d44' : '#6bc5ff22'; g.fillRect(X(l.start_time), 0, X(l.end_time) - X(l.start_time), h); }
  g.fillStyle = '#ff6b9d'; g.fillRect(X(cur * 1000), 0, 1, h);
}
const waveTime = e => { const r = wave.getBoundingClientRect(); return zoomStart + ((e.clientX - r.left) / r.width) * zoomLen; };
let drag = null;
wave.addEventListener('mousedown', e => {
  if (e.button !== 0) return; const t = waveTime(e) * 1000, px = zoomLen * 1000 / wave.clientWidth, l = S.lines[S.active];
  if (e.shiftKey && l && isEv(l)) { snapshot(); l.start_time = Math.round(t); S.dirty = true; refresh(); return; }
  if (l && isEv(l)) { const edge = [['start_time', Math.abs(t - l.start_time)], ['end_time', Math.abs(t - l.end_time)]].sort((a, b) => a[1] - b[1])[0];
    if (edge[1] <= 5 * px) { snapshot(); drag = edge[0]; return; } }
  vd.currentTime = t / 1000;
});
window.addEventListener('mousemove', e => { if (!drag) return; const l = S.lines[S.active]; if (!l) return; const t = Math.max(0, Math.round(waveTime(e) * 1000)); l[drag] = t; S.dirty = true; renderGrid(); drawWave(); fillEdit(); });
window.addEventListener('mouseup', () => { if (drag) { drag = null; S.redo = []; refresh(); } });
wave.addEventListener('contextmenu', e => { e.preventDefault(); const l = S.lines[S.active]; if (l && isEv(l)) { snapshot(); l.end_time = Math.round(waveTime(e) * 1000); S.dirty = true; refresh(); } });
wave.addEventListener('mousemove', e => { const l = S.lines[S.active]; if (!l || !isEv(l) || drag) return; const t = waveTime(e) * 1000, px = zoomLen * 1000 / wave.clientWidth; wave.style.cursor = Math.min(Math.abs(t - l.start_time), Math.abs(t - l.end_time)) <= 5 * px ? 'ew-resize' : 'crosshair'; });
wave.addEventListener('wheel', e => { e.preventDefault(); if (e.ctrlKey) zoomLen = Math.max(2, Math.min(300, zoomLen * (e.deltaY > 0 ? 1.2 : 0.83))); else zoomStart = Math.max(0, zoomStart + (e.deltaY / 100) * zoomLen * 0.1); drawWave(); }, { passive: false });
window.addEventListener('resize', () => { renderGrid(); drawWave(); });

/* ------------------------------------------------------------ actions */
const blankLine = (after) => defaultDialogue({ start_time: after ? after.end_time : 0, end_time: after ? after.end_time + 2000 : 2000, style: after?.style ?? 'Default' });
const actions = {
  new() { snapshot(); S.lines = newScript(); S.fileName = 'untitled.ass'; S.sel = new Set(); S.active = -1; S.dirty = false; refresh(); },
  async open() { const [f] = await pickFile($('#fileOpen')); if (f) loadSubtitleText(await decodeText(f), f.name); },
  save() { download(S.fileName, writeAss(S.lines)); S.dirty = false; try { localStorage.removeItem('osisub.autosave'); } catch { /* ignore */ } refresh(); },
  'export-srt'() { download(S.fileName.replace(/\.\w+$/, '.srt'), writeSrt(S.lines)); },
  'export-vtt'() { download(S.fileName.replace(/\.\w+$/, '.vtt'), writeVtt(S.lines), 'text/vtt'); },
  'export-txt'() { download(S.fileName.replace(/\.\w+$/, '.txt'), writePlain(S.lines)); },
  async 'open-video'() { const [f] = await pickFile($('#fileVideo')); if (f) openMedia(f); },
  undo, redo,
  'insert-before'() { mutate(() => { const i = S.active >= 0 ? S.active : S.lines.length; const ref = S.lines[i]; const n = defaultDialogue({ start_time: Math.max(0, (ref?.start_time ?? 2000) - 2000), end_time: ref?.start_time ?? 2000, style: ref?.style ?? 'Default' }); S.lines.splice(i, 0, n); S.active = i; S.sel = new Set([i]); }); E.text.focus(); },
  'insert-after'() { mutate(() => { const ref = S.lines[S.active]; const i = S.active >= 0 ? S.active + 1 : S.lines.length; S.lines.splice(i, 0, blankLine(ref)); S.active = i; S.sel = new Set([i]); }); E.text.focus(); },
  delete() { const s = selected(); if (!s.length) return; mutate(() => { const first = view().indexOf(s[0]); S.lines = S.lines.filter((_, i) => !S.sel.has(i)); S.sel = new Set(); const v = view(); S.active = v[Math.min(first, v.length - 1)] ?? -1; if (S.active >= 0) S.sel.add(S.active); }); },
  duplicate() { const s = selected(); if (!s.length) return; mutate(() => { const last = s[s.length - 1]; S.lines.splice(last + 1, 0, ...s.map(i => clone(S.lines[i]))); }); },
  comment() { mutate(() => { const s = selected(); const to = !s.every(i => S.lines[i].comment); s.forEach(i => (S.lines[i].comment = to)); }); },
  join() { const s = selected(); if (s.length < 2) return; mutate(() => { const a = S.lines[s[0]]; a.end_time = Math.max(...s.map(i => S.lines[i].end_time)); a.text = s.map(i => S.lines[i].text).join('\\N'); for (let k = s.length - 1; k > 0; k--) S.lines.splice(s[k], 1); S.sel = new Set([s[0]]); S.active = s[0]; }); },
  split() { const l = S.lines[S.active]; if (!l || !isEv(l)) return; const p = E.text.selectionStart; mutate(() => { const a = l.text.slice(0, p).trim(), b = l.text.slice(p).trim(); const ratio = Math.max(0.1, Math.min(0.9, stripTags(a).length / Math.max(1, stripTags(l.text).length))); const mid = Math.round(l.start_time + (l.end_time - l.start_time) * ratio); const n = clone(l); n.text = b; n.start_time = mid; l.text = a; l.end_time = mid; S.lines.splice(S.active + 1, 0, n); }); },
  'select-all'() { S.sel = new Set(view()); refresh(false); },
  async shift() {
    const r = await modal('Shift times', `<div class="f"><label>Amount</label><input name="amt" value="0.000" autofocus><label>Unit</label><select name="unit"><option value="s">seconds</option><option value="ms">milliseconds</option><option value="f">frames</option></select>
      <label>Apply to</label><select name="what"><option value="both">Start &amp; end</option><option value="start">Start only</option><option value="end">End only</option></select>
      <label>Lines</label><select name="rng"><option value="sel">Selected</option><option value="all">All</option><option value="after">Selected onward</option></select></div>`);
    if (r.button !== 'OK') return; const b = r.box; let amt = parseFloat(val(b, 'amt').value) || 0; const u = val(b, 'unit').value;
    amt = u === 's' ? amt * 1000 : u === 'f' ? frameToMs(amt, S.fps) : amt; const what = val(b, 'what').value, rng = val(b, 'rng').value;
    let t = rng === 'all' ? view() : rng === 'after' ? view().filter(i => i >= (selected()[0] ?? 0)) : selected();
    mutate(() => t.forEach(i => { const l = S.lines[i]; if (what !== 'end') l.start_time = Math.max(0, l.start_time + amt); if (what !== 'start') l.end_time = Math.max(0, l.end_time + amt); }));
  },
  async 'fps-convert'() {
    const r = await modal('Change frame rate', `<div class="f"><label>From fps</label><input name="a" value="23.976"><label>To fps</label><input name="b" value="25"></div>`);
    if (r.button !== 'OK') return; const a = parseFloat(val(r.box, 'a').value), b = parseFloat(val(r.box, 'b').value); if (!(a > 0 && b > 0)) return;
    mutate(() => selOrAll().forEach(i => { const l = S.lines[i]; l.start_time = Math.round(l.start_time * a / b); l.end_time = Math.round(l.end_time * a / b); }));
  },
  snap() { mutate(() => selOrAll().forEach(i => { const l = S.lines[i]; l.start_time = frameToMs(msToFrame(l.start_time, S.fps), S.fps); l.end_time = frameToMs(msToFrame(l.end_time, S.fps), S.fps); })); },
  'fix-overlaps'() {
    mutate(() => { const v = view().sort((a, b) => S.lines[a].start_time - S.lines[b].start_time); for (let k = 1; k < v.length; k++) { const p = S.lines[v[k - 1]], c = S.lines[v[k]]; if (p.style === c.style && p.end_time > c.start_time) p.end_time = Math.max(p.start_time + 1, c.start_time); } });
  },
  async 'min-dur'() {
    const r = await modal('Enforce duration', `<div class="f"><label>Min (ms)</label><input name="mn" value="500"><label>Max (ms)</label><input name="mx" value="7000"><label>Extend to chars/sec</label><input name="cps" value="17"></div>`);
    if (r.button !== 'OK') return; const mn = +val(r.box, 'mn').value, mx = +val(r.box, 'mx').value, cp = +val(r.box, 'cps').value;
    mutate(() => { const v = view(); v.forEach((i, k) => { const l = S.lines[i]; const nxt = S.lines.slice(i + 1).find(isEv); const lim = nxt ? nxt.start_time : Infinity; const need = Math.max(mn, cp ? Math.ceil((stripTags(l.text).replace(/\s/g, '').length / cp) * 1000) : 0);
      if (l.end_time - l.start_time < need) l.end_time = Math.min(lim, l.start_time + need); if (l.end_time - l.start_time > mx) l.end_time = l.start_time + mx; }); });
  },
  sort() { mutate(() => { const head = S.lines.filter(l => !isEv(l)); const ev = S.lines.filter(isEv).sort((a, b) => a.start_time - b.start_time || a.layer - b.layer); S.lines = [...head, ...ev]; }); },
  async find() {
    const r = await modal('Find & Replace', `<div class="f"><label>Find</label><input name="f"><label>Replace</label><input name="r"><label>Field</label><select name="fld"><option value="text">Text</option><option value="style">Style</option><option value="actor">Actor</option><option value="effect">Effect</option></select>
      <label>Options</label><span><label><input type="checkbox" name="rx"> Regex</label> <label><input type="checkbox" name="cs"> Case</label> <label><input type="checkbox" name="sel"> Selected only</label></span></div>`, ['Replace all', 'Find next', 'Close']);
    if (r.button === 'Close') return; const b = r.box; const f = val(b, 'f').value; if (!f) return;
    let re; try { re = new RegExp(val(b, 'rx').checked ? f : f.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'g' + (val(b, 'cs').checked ? '' : 'i')); } catch (e) { return status('Bad regex: ' + e.message); }
    const fld = val(b, 'fld').value, pool = val(b, 'sel').checked ? selected() : view();
    if (r.button === 'Find next') { const v = view(); const st = Math.max(0, v.indexOf(S.active) + 1); const hit = [...v.slice(st), ...v.slice(0, st)].find(i => { re.lastIndex = 0; return re.test(S.lines[i][fld]); }); if (hit != null) { S.sel = new Set([hit]); S.active = hit; refresh(false); scrollToActive(); renderGrid(); } else status('Not found'); return; }
    let n = 0; mutate(() => pool.forEach(i => { const l = S.lines[i]; const nv = l[fld].replace(re, (...m) => { n++; return val(b, 'r').value.replace(/\$(\d)/g, (_, d) => m[+d] ?? ''); }); l[fld] = nv; })); status(`${n} replacement(s)`);
  },
  async qc() {
    const issues = runQC(S.lines); const v = view();
    const html = issues.length ? issues.map(i => `<div class="issue sev-${i.severity}" data-i="${i.index}">#${v.indexOf(i.index) + 1} [${i.code}] ${esc(i.message)} — <span style="color:var(--dim)">${esc(stripTags(S.lines[i.index].text).slice(0, 50))}</span></div>`).join('') : '<p>No issues found 🎉</p>';
    const box = $('#modalBox'); const p = modal(`Quality check — ${issues.length} issue(s)`, `<div style="max-height:55vh;overflow:auto;min-width:520px">${html}</div>`, ['Close']);
    box.querySelectorAll('.issue').forEach(el => el.onclick = () => { const i = +el.dataset.i; S.sel = new Set([i]); S.active = i; refresh(false); scrollToActive(); renderGrid(); seekToLine(i); });
    await p;
  },
  async styles() {
    const st = S.lines.filter(l => l.class === 'style'); const rows = st.map((s, k) => `<tr><td><input name="name${k}" value="${esc(s.name)}"></td><td><input name="font${k}" value="${esc(s.fontname)}"></td><td><input type="number" name="size${k}" value="${s.fontsize}"></td><td><input type="number" name="out${k}" value="${s.outline}"></td><td><input type="number" name="sh${k}" value="${s.shadow}"></td><td><input type="number" name="al${k}" value="${s.align}"></td><td><input type="number" name="mv${k}" value="${s.margin_t}"></td><td><input name="c1${k}" value="${s.color1}" size="10"></td><td><label><input type="checkbox" name="b${k}" ${s.bold ? 'checked' : ''}>B</label> <label><input type="checkbox" name="i${k}" ${s.italic ? 'checked' : ''}>I</label></td></tr>`).join('');
    const r = await modal('Styles', `<table><tr><th>Name</th><th>Font</th><th>Size</th><th>Outl</th><th>Shad</th><th>Align</th><th>MarginV</th><th>Primary</th><th></th></tr>${rows}</table>`, ['Add style', 'Apply', 'Cancel']);
    if (r.button === 'Cancel') return;
    mutate(() => { st.forEach((s, k) => { const b = r.box; const old = s.name; s.name = val(b, 'name' + k).value || old; s.fontname = val(b, 'font' + k).value; s.fontsize = +val(b, 'size' + k).value; s.outline = +val(b, 'out' + k).value; s.shadow = +val(b, 'sh' + k).value; s.align = +val(b, 'al' + k).value; s.margin_t = +val(b, 'mv' + k).value; s.color1 = val(b, 'c1' + k).value; s.bold = val(b, 'b' + k).checked; s.italic = val(b, 'i' + k).checked; if (old !== s.name) S.lines.forEach(l => isEv(l) && l.style === old && (l.style = s.name)); });
      if (r.button === 'Add style') { const last = S.lines.map(l => l.class).lastIndexOf('style'); const ns = { ...defaultStyle('Style' + (st.length + 1)), section: 'V4+ Styles' }; S.lines.splice(last + 1, 0, ns); } });
    if (r.button === 'Add style') actions.styles();
  },
  async props() {
    const keys = ['Title', 'PlayResX', 'PlayResY', 'WrapStyle', 'ScaledBorderAndShadow', 'YCbCr Matrix']; 
    const r = await modal('Script properties', `<div class="f">${keys.map((k, n) => `<label>${k}</label><input name="k${n}" value="${esc(getInfo(S.lines, k) ?? '')}">`).join('')}<label>Frame rate</label><input name="fps" value="${S.fps}"></div>`);
    if (r.button !== 'OK') return; mutate(() => { keys.forEach((k, n) => { const v = val(r.box, 'k' + n).value; if (v !== '') setInfo(S.lines, k, v); }); S.fps = parseFloat(val(r.box, 'fps').value) || S.fps; });
  },
  cleanup() { mutate(() => selOrAll().forEach(i => { const l = S.lines[i]; l.text = l.text.replace(/\{\}/g, '').replace(/ {2,}/g, ' ').replace(/^\s+|\s+$/g, '').replace(/\s+(\\N)/g, '$1').replace(/(\\N)\s+/g, '$1'); })); },
  async case() {
    const r = await modal('Change case', `<div class="f"><label>Mode</label><select name="m"><option value="up">UPPERCASE</option><option value="low">lowercase</option><option value="sent">Sentence case</option><option value="title">Title Case</option></select></div>`); if (r.button !== 'OK') return;
    const m = val(r.box, 'm').value; const f = { up: s => s.toUpperCase(), low: s => s.toLowerCase(), sent: s => s.toLowerCase().replace(/(^\s*|[.!?]\s+)(\p{L})/gu, (_, a, b) => a + b.toUpperCase()), title: s => s.toLowerCase().replace(/(^|\s)(\p{L})/gu, (_, a, b) => a + b.toUpperCase()) }[m];
    // only touch text outside {tags} and outside \N escapes
    mutate(() => selOrAll().forEach(i => { const l = S.lines[i]; l.text = l.text.split(/(\{[^}]*\}|\\[Nnh])/).map((p, k) => (k % 2 ? p : f(p))).join(''); }));
  },
  async balance() {
    const r = await modal('Balance line breaks', `<div class="f"><label>Max chars/line</label><input name="n" value="42"><label>Max lines</label><input name="m" value="2"></div>`); if (r.button !== 'OK') return;
    const n = +val(r.box, 'n').value || 42, m = +val(r.box, 'm').value || 2; mutate(() => selOrAll().forEach(i => { const l = S.lines[i]; if (!/\\p\d|\\k/.test(l.text)) l.text = balanceText(l.text, n, m); }));
  },
  async 'split-long'() {
    const r = await modal('Split long lines', `<div class="f"><label>Max chars per line</label><input name="n" value="84"></div>`); if (r.button !== 'OK') return;
    mutate(() => { S.lines = splitLongLines(S.lines, { maxChars: +val(r.box, 'n').value || 84 }); S.sel = new Set(); });
  },
  async 'merge-short'() {
    const r = await modal('Merge short lines', `<div class="f"><label>Max gap (ms)</label><input name="g" value="250"><label>Max total chars</label><input name="c" value="60"></div>`); if (r.button !== 'OK') return;
    mutate(() => { S.lines = mergeShortLines(S.lines, { maxGap: +val(r.box, 'g').value, maxChars: +val(r.box, 'c').value }); S.sel = new Set(); S.active = -1; });
  },
  'remove-hi'() { mutate(() => { selOrAll().forEach(i => { S.lines[i].text = removeHI(S.lines[i].text); }); S.lines = S.lines.filter(l => !(isEv(l) && !l.comment && l.text === '')); S.sel = new Set(); S.active = -1; }); },
  punct() { mutate(() => selOrAll().forEach(i => { S.lines[i].text = fixPunctuation(S.lines[i].text); })); },
  dedupe() { mutate(() => { const n = S.lines.length; S.lines = removeDuplicates(S.lines); S.sel = new Set(); S.active = -1; status(`${n - S.lines.length} duplicate(s) removed`); }); },
  async 'min-gap'() {
    const r = await modal('Enforce minimum gap', `<div class="f"><label>Min gap (ms)</label><input name="g" value="${Math.round(1000 / S.fps)}"></div>`); if (r.button !== 'OK') return;
    mutate(() => enforceGap(S.lines, +val(r.box, 'g').value || 0));
  },
  play() { vd.paused ? vd.play() : vd.pause(); },
  'play-line'() { const l = S.lines[S.active]; if (l && isEv(l)) playRange(l); },
  'set-start'() { const l = S.lines[S.active]; if (l && isEv(l)) mutate(() => { l.start_time = Math.round(vd.currentTime * 1000); }); },
  'set-end'() { const l = S.lines[S.active]; if (l && isEv(l)) mutate(() => { l.end_time = Math.round(vd.currentTime * 1000); }); },
  async 'load-lua'() { for (const f of await pickFile($('#fileLua'))) await loadLua(await f.text(), f.name); },
  async 'reload-lua'() { const old = S.scripts; S.scripts = []; for (const s of old) await loadLua(s.source, s.filename); },
};
const act = n => { const a = actions[n]; if (a) return a(); };

/* ------------------------------------------------------------ Lua automation */
async function post(url, body) { const r = await fetch(url, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }); return r.json(); }
async function loadLua(source, filename) {
  const r = await post('/api/lua/load', { source, filename, fps: S.fps, fileName: S.fileName, files: {} });
  if (r.status === 'error' || r.error) { await modal('Lua error: ' + filename, `<pre class="log">${esc(r.error)}</pre>`, ['Close']); return; }
  S.scripts.push({ id: r.id, info: r.info, source, filename }); renderMacros();
}
function renderMacros() {
  const el = $('#macroList'); const items = [];
  S.scripts.forEach((s, si) => { s.info.macros.forEach((m, mi) => items.push(`<a data-s="${si}" data-m="${mi}" data-k="macro" title="${esc(m.description)}">${esc(m.name)}</a>`)); s.info.filters.forEach((m, mi) => items.push(`<a data-s="${si}" data-m="${mi}" data-k="filter" title="${esc(m.description)}">[Filter] ${esc(m.name)}</a>`)); });
  el.innerHTML = items.length ? items.join('') : '<em>No macros loaded</em>';
}
$('#macroList').addEventListener('click', e => { const a = e.target.closest('a[data-m]'); if (a) runMacro(+a.dataset.s, +a.dataset.m, a.dataset.k); });
function dialogHtml(fields) {
  const cells = fields.map((f, n) => {
    const nm = esc(f.name ?? 'f' + n), st = `grid-column:${(f.x ?? 0) + 1} / span ${f.width ?? 1};grid-row:${(f.y ?? n) + 1} / span ${f.height ?? 1}`;
    let inner;
    switch (f.class) {
      case 'label': inner = `<span>${esc(f.label ?? '')}</span>`; break;
      case 'checkbox': inner = `<label><input type="checkbox" name="${nm}" ${f.value ? 'checked' : ''}> ${esc(f.label ?? '')}</label>`; break;
      case 'dropdown': inner = `<select name="${nm}">${(f.items ?? []).map(it => `<option ${it === f.value ? 'selected' : ''}>${esc(it)}</option>`).join('')}</select>`; break;
      case 'textbox': inner = `<textarea name="${nm}" rows="${Math.max(2, f.height ?? 3)}" style="width:100%">${esc(f.text ?? f.value ?? '')}</textarea>`; break;
      case 'intedit': case 'floatedit': inner = `<input type="number" step="${f.class === 'intedit' ? 1 : 'any'}" ${f.min != null ? `min="${f.min}"` : ''} ${f.max != null ? `max="${f.max}"` : ''} name="${nm}" value="${esc(f.value ?? 0)}">`; break;
      case 'color': case 'coloralpha': case 'alpha': inner = `<input name="${nm}" value="${esc(f.value ?? '')}">`; break;
      default: inner = `<input name="${nm}" value="${esc(f.text ?? f.value ?? '')}" style="width:100%">`;
    }
    return `<div style="${st}">${inner}</div>`;
  });
  return `<div style="display:grid;gap:6px 8px;grid-auto-columns:minmax(80px,auto)">${cells.join('')}</div>`;
}
function collectDialog(box, fields) {
  const v = {};
  fields.forEach((f, n) => { if (!f.name || f.class === 'label') return; const el = val(box, f.name); if (!el) return;
    v[f.name] = f.class === 'checkbox' ? el.checked : f.class === 'intedit' ? Math.round(+el.value) : f.class === 'floatedit' ? +el.value : el.value; });
  return v;
}
async function handleDialog(req) {
  const fields = Array.isArray(req.dialog) ? req.dialog : Object.values(req.dialog ?? {});
  const buttons = req.buttons ?? ['OK', 'Cancel'];
  const r = await modal('Automation', dialogHtml(fields), buttons);
  if (r.cancelled) return { cancelled: true };
  return { button: r.button, values: collectDialog(r.box, fields) };
}
async function runMacro(si, mi, kind) {
  const s = S.scripts[si]; status('Running ' + (kind === 'filter' ? s.info.filters : s.info.macros)[mi].name + '…');
  const selIdx = selected().map(i => i);
  const body = { id: s.id, kind, index: mi, lines: S.lines, selection: S.lines.map((l, i) => i).filter(i => S.sel.has(i)), active: Math.max(0, S.active), fps: S.fps };
  let r = await post('/api/lua/run', body);
  while (r.status === 'dialog') r = await post('/api/lua/resume', { id: s.id, response: await handleDialog(r.request) });
  if (r.status === 'error') { await modal('Script error', `<pre class="log">${esc(r.error)}${r.logs?.length ? '\n\n' + esc(r.logs.join('')) : ''}</pre>`, ['Close']); return status('Script failed'); }
  if (r.cancelled) return status('Cancelled');
  mutate(() => { S.lines = r.lines; S.sel = new Set((r.selection ?? []).filter(i => i >= 0 && i < r.lines.length)); S.active = r.active != null && r.active < r.lines.length ? r.active : (view()[0] ?? -1); });
  if (r.logs?.length) await modal('Script output', `<pre class="log">${esc(r.logs.join(''))}</pre>`, ['Close']);
  status('Done');
}

/* ------------------------------------------------------------ wiring */
document.querySelectorAll('[data-act]').forEach(a => a.addEventListener('click', () => { act(a.dataset.act); document.activeElement?.blur(); }));
document.addEventListener('keydown', e => {
  if (!$('#modal').hidden) return;
  const mod = e.ctrlKey || e.metaKey, inText = ['INPUT', 'TEXTAREA', 'SELECT'].includes(document.activeElement?.tagName);
  if (mod && e.key === 's') { e.preventDefault(); act('save'); } else if (mod && e.key === 'o') { e.preventDefault(); act('open'); }
  else if (mod && e.key === 'h') { e.preventDefault(); act('find'); } else if (mod && e.key === 'z' && !inText) { e.preventDefault(); undo(); }
  else if (mod && e.key === 'y' && !inText) { e.preventDefault(); redo(); } else if (mod && e.key === 'a' && !inText) { e.preventDefault(); act('select-all'); }
  else if (!inText && e.key === 'ArrowDown') { e.preventDefault(); moveActive(1, e.shiftKey); } else if (!inText && e.key === 'ArrowUp') { e.preventDefault(); moveActive(-1, e.shiftKey); }
  else if (!inText && e.key === 'Delete') act('delete'); else if (!inText && e.key === 'Enter') { E.text.focus(); }
  else if (e.key === ' ' && e.ctrlKey) { e.preventDefault(); act('play'); }
});
// autosave / crash recovery
setInterval(() => { if (!S.dirty) return; try { localStorage.setItem('osisub.autosave', JSON.stringify({ at: Date.now(), name: S.fileName, ass: writeAss(S.lines) })); } catch { /* storage full/blocked */ } }, 15000);
(async () => { try { const a = JSON.parse(localStorage.getItem('osisub.autosave') || 'null'); if (!a) return;
  const r = await modal('Recover session?', `<p>Unsaved work from ${new Date(a.at).toLocaleString()} (<b>${esc(a.name)}</b>) was found.</p>`, ['Restore', 'Discard']);
  if (r.button === 'Restore') { loadSubtitleText(a.ass, a.name); S.dirty = true; refresh(); } localStorage.removeItem('osisub.autosave'); } catch { /* ignore */ } })();
window.addEventListener('beforeunload', e => { if (S.dirty) e.preventDefault(); });
// drag & drop files
document.addEventListener('dragover', e => e.preventDefault());
document.addEventListener('drop', async e => { e.preventDefault(); for (const f of e.dataTransfer.files) { if (/\.(ass|ssa|srt|vtt)$/i.test(f.name)) loadSubtitleText(await decodeText(f), f.name); else if (/\.lua$/i.test(f.name)) await loadLua(await f.text(), f.name); else openMedia(f); } });
window.OsiSub = { jassub: () => jassub, S, actions, loadSubtitleText, view, loadLua, runMacro }; // for automation/tests
refresh();
