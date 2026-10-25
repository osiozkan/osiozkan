// Aegisub-compatible Lua automation runtime on fengari (Lua 5.3, pure JS).
import fengari from 'fengari';
import { fileURLToPath } from 'node:url';
import fs from 'node:fs';
import path from 'node:path';
import { fontManager } from './fonts.js';

const { lua, lauxlib, lualib, to_luastring, to_jsstring } = fengari;
const dir = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', 'lua');
export const LIB_SOURCES = {};
for (const f of fs.readdirSync(dir)) if (f.endsWith('.lua')) LIB_SOURCES[f.slice(0, -4)] = fs.readFileSync(path.join(dir, f), 'utf8');
export const LIB_ALIASES = {};
const stripBom = s => s.replace(/^\uFEFF/, '');

/** Default host: approximate text metrics, no video. Override fields for real hosts. */
export function defaultHost(extra = {}) {
  return {
    log: () => {}, progress: () => {}, fps: 23.976, videoSize: null, fileName: '', readFile: () => '', keyframes: [],
    textExtents(style, text) { return fontManager().extents(style, text); },
    ...extra,
  };
}

function toRegExp(pat, flags) {
  let f = 'u' + 'g'; if (flags & 1) f += 'i';
  let p = pat; if (flags & 4) p = p.replace(/\s+|#.*$/gm, '');
  p = p.replace(/\[\[:alpha:\]\]/g, '\\p{L}').replace(/\[:alpha:\]/g, '\\p{L}').replace(/\[:digit:\]/g, '\\d').replace(/\[:space:\]/g, '\\s').replace(/\(\?<([A-Za-z])/g, '(?<$1');
  try { return new RegExp(p, f); } catch { return new RegExp(p.replace(/\\/g, '\\\\'), f); }
}
const byteLen = s => Buffer.byteLength(s, 'utf8');
// 1-based byte offsets (as Aegisub returns for UTF-8 strings)
function groupsOf(m, str) {
  const out = [];
  const idx = m.indices;
  for (let i = 0; i < m.length; i++) {
    const g = m[i]; if (g === undefined) { out.push(['', 0, -1]); continue; }
    const st = idx[i][0]; const first = byteLen(str.slice(0, st)) + 1;
    out.push([g, first, first + byteLen(g) - 1]);
  }
  return out;
}
const withD = r => new RegExp(r.source, r.flags.includes('d') ? r.flags : r.flags + 'd');

export class LuaScript {
  constructor(source, filename = 'script.lua', host = {}) {
    this.host = defaultHost(host); this.filename = filename; this.logs = [];
    const L = this.L = lauxlib.luaL_newstate();
    lualib.luaL_openlibs(L);
    lua.lua_pushjsfunction(L, st => this._hostCall(st));
    lua.lua_setglobal(L, to_luastring('__host'));
    this._exec(LIB_SOURCES.prelude, 'prelude');
    this._exec(source, filename);
    this.info = JSON.parse(this._call('__info'));
    this.macros = this.info.macros; this.filters = this.info.filters;
  }
  _exec(src, name) {
    const L = this.L;
    const buf = to_luastring(stripBom(src));
    if (lauxlib.luaL_loadbuffer(L, buf, buf.length, to_luastring('=' + name)) !== lua.LUA_OK ||
        lua.lua_pcall(L, 0, 0, 0) !== lua.LUA_OK) {
      const msg = to_jsstring(lua.lua_tostring(L, -1)); lua.lua_pop(L, 1); throw new Error(msg);
    }
  }
  _call(fn, ...args) {
    const L = this.L;
    lua.lua_getglobal(L, to_luastring(fn));
    for (const a of args) {
      if (typeof a === 'number') lua.lua_pushinteger(L, a); else if (typeof a === 'boolean') lua.lua_pushboolean(L, a);
      else if (a == null) lua.lua_pushnil(L); else lua.lua_pushstring(L, to_luastring(String(a)));
    }
    if (lua.lua_pcall(L, args.length, 1, 0) !== lua.LUA_OK) {
      const msg = to_jsstring(lua.lua_tostring(L, -1)); lua.lua_pop(L, 1); throw new Error(msg);
    }
    const r = lua.lua_isnil(L, -1) ? '' : to_jsstring(lua.lua_tostring(L, -1)); lua.lua_pop(L, 1); return r;
  }
  _hostCall(L) {
    const name = to_jsstring(lua.lua_tostring(L, 1)); const argStr = to_jsstring(lua.lua_tostring(L, 2));
    let res = '';
    try { res = this._dispatch(name, argStr) ?? ''; } catch (e) { res = ''; this.logs.push('[host error] ' + e.message); }
    lua.lua_pushstring(L, to_luastring(typeof res === 'string' ? res : JSON.stringify(res)));
    return 1;
  }
  _dispatch(name, raw) {
    const h = this.host;
    if (name === 'lib') { const k = LIB_ALIASES[raw] ?? raw; return LIB_SOURCES[k] ?? ''; }
    if (name === 'readfile') return stripBom(h.readFile(raw) ?? '');
    const a = raw ? JSON.parse(raw) : [];
    switch (name) {
      case 'log': this.logs.push(a[0]); h.log(a[0]); return '';
      case 'progress': h.progress(a[0]); return '';
      case 'task': case 'title': case 'undo_point': return '';
      case 'case': return JSON.stringify(a[0] === 'upper' ? a[1].toUpperCase() : a[1].toLowerCase());
      case 'text_extents': return JSON.stringify(h.textExtents(a[0], a[1]));
      case 'frame_from_ms': return h.fps ? JSON.stringify(Math.floor((a[0] * h.fps) / 1000 + 1e-6)) : '';
      case 'ms_from_frame': return h.fps ? JSON.stringify(Math.round((a[0] * 1000) / h.fps)) : '';
      case 'video_size': return h.videoSize ? JSON.stringify(h.videoSize) : '';
      case 'project_properties': return JSON.stringify(h.properties ?? {});
      case 'file_name': return JSON.stringify(h.fileName);
      case 'keyframes': return JSON.stringify(h.keyframes);
      case 'clipboard_get': return JSON.stringify(h.clipboard ?? '');
      case 'clipboard_set': h.clipboard = a[0]; return '';
      case 're_match': { const r = withD(toRegExp(a[0], a[2])); r.lastIndex = 0; const m = r.exec(a[1]); return m ? JSON.stringify(groupsOf(m, a[1])) : ''; }
      case 're_find': {
        const r = withD(toRegExp(a[0], a[2])); const out = []; let m;
        while ((m = r.exec(a[1]))) { out.push(...groupsOf(m, a[1]).slice(0, 1)); if (m[0] === '') r.lastIndex++; }
        return JSON.stringify(out);
      }
      case 're_sub': {
        const r = toRegExp(a[0], a[4]); let n = 0, lim = a[3] || Infinity;
        const repl = a[1].replace(/\\(\d)/g, '$$$1').replace(/\$(?![\d&$`'<])/g, '$$$$');
        return JSON.stringify(a[2].replace(r, (...m) => (++n > lim ? m[0] : m[0].replace(new RegExp(r.source, r.flags.replace('g', '')), repl))));
      }
      case 're_split': return JSON.stringify(a[1].split(toRegExp(a[0], a[2])));
      default: return '';
    }
  }
  /** Run macro/filter. onDialog(request) -> Promise<{button, values}|{cancelled:true}>. */
  async run(kind, index, lines, selection = [], active = 0, onDialog = async () => ({ cancelled: true })) {
    // raw (unparsed/other-section) lines are invisible to scripts, as in Aegisub; they are re-appended afterwards
    const toCore = new Map(); const core = []; const raws = [];
    lines.forEach((l, i) => (l.class === 'raw' ? raws.push(l) : (toCore.set(i, core.length), core.push(l))));
    selection = selection.filter(i => toCore.has(i)).map(i => toCore.get(i)); active = toCore.get(active) ?? 0;
    let out = JSON.parse(this._call('__start', kind, index + 1, JSON.stringify(core), JSON.stringify(selection.map(i => i + 1)), active + 1));
    while (out.status === 'dialog') {
      const resp = await onDialog(out.request);
      out = JSON.parse(this._call('__resume', JSON.stringify(resp ?? { cancelled: true })));
    }
    if (out.status === 'error') { const e = new Error(out.message); e.logs = this.logs; throw e; }
    if (out.status === 'cancelled') return { cancelled: true, lines, selection };
    legacyNumbers(out.lines, core);
    return { lines: [...out.lines, ...raws], selection: out.selection?.map(i => i - 1) ?? selection, active: out.active != null ? out.active - 1 : active, logs: this.logs };
  }
  async validate(index, lines, selection = [], active = 0) {
    return JSON.parse(this._call('__validate', index + 1, JSON.stringify(lines), JSON.stringify(selection.map(i => i + 1)), active + 1));
  }
}

// Aegisub runs LuaJIT where 100.0 prints as "100"; Lua 5.3 prints "100.0". Normalise inside override blocks of lines the script changed.
function legacyNumbers(outLines, inLines) {
  const before = new Set(inLines.filter(l => l.class === 'dialogue').map(l => l.text));
  for (const l of outLines) {
    if (l.class !== 'dialogue' || before.has(l.text) || typeof l.text !== 'string') continue;
    l.text = l.text.replace(/\{[^}]*\}/g, b => b.replace(/(\d)\.0(?=[,)\\}&\s]|$)/g, '$1'));
  }
}
