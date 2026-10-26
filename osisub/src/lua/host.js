// Aegisub Automation 4 host: runs .lua macros/filters against an osisub document.
process.env.FENGARICONF ??= '{"LUA_COMPAT_FLOATSTRING":true}'; // print 480 not 480.0, like LuaJIT
const { lua, lauxlib, lualib, to_luastring, to_jsstring } = (await import("fengari")).default;
import { readFileSync, existsSync, statSync, writeFileSync, appendFileSync, unlinkSync, renameSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve, basename } from "node:path";
import { fileURLToPath } from "node:url";
import { randomUUID } from "node:crypto";
import { STYLE_FIELDS, getInfo } from "../core/ass.js";
import { frameFromMs, msFromFrame } from "../core/time.js";
import { measureText } from "./metrics.js";

const here = dirname(fileURLToPath(import.meta.url));
const RUNTIME = readFileSync(join(here, "runtime.lua"), "utf8");

const S = (s) => to_luastring(String(s));
const STYLE_KEYS = STYLE_FIELDS; // name .. encoding (margin_b mirrors margin_t)

export class AutomationHost {
  /**
   * @param {object} doc osisub document (mutated in place by run())
   * @param {object} [opt] {fps, keyframes, videoSize, includePaths, dialog:{values,button,open,save}, scriptDir, filename, textMetrics}
   */
  constructor(doc, opt = {}) {
    this.doc = doc;
    this.opt = { fps: 23.976, keyframes: [], includePaths: [], dialog: {}, ...opt };
    this.logs = [];
    this.progress = 0;
    this.task = ""; this.title = "";
    this.dialogs = [];
    this.stdout = [];
    this.clipboard = "";
    this.scripts = [];
    this.macros = [];
    this.filters = [];
    this.L = lauxlib.luaL_newstate();
    lualib.luaL_openlibs(this.L);
    this.#installHost();
    this.#exec(RUNTIME, "=osisub-runtime");
  }

  #exec(src, chunkname) {
    const L = this.L;
    const st = lauxlib.luaL_loadbuffer(L, to_luastring(src), null, to_luastring(chunkname));
    if (st !== 0) throw new Error(this.#err());
    if (lua.lua_pcall(L, 0, 0, 0) !== 0) throw new Error(this.#err());
  }
  #err() {
    const L = this.L;
    const m = lua.lua_isstring(L, -1) ? to_jsstring(lua.lua_tostring(L, -1)) : "(lua error)";
    lua.lua_pop(L, 1);
    return m;
  }

  #installHost() {
    const L = this.L;
    const self = this;
    const fn = (name, f) => { lua.lua_pushjsfunction(L, f); lua.lua_setfield(L, -2, S(name)); };
    const str = (i) => to_jsstring(lauxlib.luaL_checklstring(L, i));
    const optstr = (i, d = "") => (lua.lua_isnoneornil(L, i) ? d : str(i));
    const num = (i, d) => (lua.lua_isnoneornil(L, i) ? d : lauxlib.luaL_checknumber(L, i));
    lua.lua_newtable(L);
    fn("log", () => { self.logs.push({ level: num(1, 2), message: str(2) }); return 0; });
    fn("progress", () => { self.progress = num(1, 0); return 0; });
    fn("task", () => { self.task = str(1); return 0; });
    fn("title", () => { self.title = str(1); return 0; });
    fn("uuid", () => { lua.lua_pushstring(L, S(randomUUID())); return 1; });
    fn("frame_from_ms", () => { lua.lua_pushinteger(L, frameFromMs(num(1, 0), self.opt.fps)); return 1; });
    fn("ms_from_frame", () => { lua.lua_pushinteger(L, msFromFrame(num(1, 0), self.opt.fps)); return 1; });
    fn("video_size", () => {
      const v = self.opt.videoSize;
      if (!v) { lua.lua_pushnil(L); return 1; }
      lua.lua_pushinteger(L, v[0]); lua.lua_pushinteger(L, v[1]); lua.lua_pushnumber(L, v[0] / v[1]); lua.lua_pushinteger(L, 0);
      return 4;
    });
    fn("keyframes", () => {
      lua.lua_newtable(L);
      self.opt.keyframes.forEach((k, i) => { lua.lua_pushinteger(L, frameFromMs(k, self.opt.fps)); lua.lua_rawseti(L, -2, i + 1); });
      return 1;
    });
    fn("decode_path", () => { lua.lua_pushstring(L, S(self.#decodePath(str(1)))); return 1; });
    fn("project_properties", () => {
      lua.lua_newtable(L);
      const put = (k, v) => { lua.lua_pushstring(L, S(v)); lua.lua_setfield(L, -2, S(k)); };
      put("video_file", self.opt.video ?? ""); put("audio_file", ""); put("keyframes_file", "");
      put("timecodes_file", ""); put("automation_scripts", ""); put("export_filters", ""); put("export_encoding", "UTF-8");
      return 1;
    });
    fn("file_name", () => { lua.lua_pushstring(L, S(self.opt.filename ?? "")); return 1; });
    fn("cwd", () => { lua.lua_pushstring(L, S(process.cwd())); return 1; });
    fn("file_attr", () => {
      try { const st = statSync(str(1)); const k = str(2); if (k === "mode") lua.lua_pushstring(L, S(st.isDirectory() ? "directory" : "file")); else lua.lua_pushinteger(L, st.size); }
      catch { lua.lua_pushnil(L); }
      return 1;
    });
    fn("fs_read", () => {
      try { lua.lua_pushstring(L, new Uint8Array(readFileSync(self.#fsPath(str(1))))); } catch { lua.lua_pushnil(L); }
      return 1;
    });
    fn("fs_write", () => {
      try {
        const p = self.#fsPath(str(1)), data = Buffer.from(lauxlib.luaL_checklstring(L, 2));
        lua.lua_toboolean(L, 3) ? appendFileSync(p, data) : writeFileSync(p, data);
        lua.lua_pushboolean(L, true);
      } catch { lua.lua_pushboolean(L, false); }
      return 1;
    });
    fn("fs_remove", () => { try { unlinkSync(self.#fsPath(str(1))); lua.lua_pushboolean(L, true); } catch { lua.lua_pushboolean(L, false); } return 1; });
    fn("fs_rename", () => { try { renameSync(self.#fsPath(str(1)), self.#fsPath(str(2))); lua.lua_pushboolean(L, true); } catch { lua.lua_pushboolean(L, false); } return 1; });
    fn("fs_tmpname", () => { lua.lua_pushstring(L, S(join(tmpdir(), "osisub-" + randomUUID().slice(0, 8)))); return 1; });
    fn("getenv", () => { const v = process.env[str(1)]; v === undefined ? lua.lua_pushnil(L) : lua.lua_pushstring(L, S(v)); return 1; });
    fn("stdout", () => { self.stdout.push(str(1)); return 0; });
    fn("stderr", () => { self.stdout.push(str(1)); return 0; });
    fn("clipboard_get", () => { lua.lua_pushstring(L, S(self.clipboard)); return 1; });
    fn("clipboard_set", () => { self.clipboard = str(1); return 0; });
    fn("text_extents", () => {
      const m = (self.opt.textMetrics ?? measureText)({
        font: str(1), size: num(2, 48), bold: lua.lua_toboolean(L, 3), italic: lua.lua_toboolean(L, 4),
        scaleX: num(5, 100), scaleY: num(6, 100), spacing: num(7, 0), text: str(8),
      });
      lua.lua_pushnumber(L, m.width); lua.lua_pushnumber(L, m.height); lua.lua_pushnumber(L, m.descent); lua.lua_pushnumber(L, m.extlead);
      return 4;
    });
    fn("read_include", () => {
      const name = str(1);
      const dirs = [...(self.scriptDirs ?? []), ...self.opt.includePaths, process.cwd()];
      for (const d of dirs) {
        const p = resolve(d, name);
        if (existsSync(p) && statSync(p).isFile()) { lua.lua_pushstring(L, S(readFileSync(p, "utf8").replace(/^﻿/, ""))); lua.lua_pushstring(L, S(p)); return 2; }
      }
      lua.lua_pushnil(L);
      return 1;
    });
    fn("dialog_overrides", () => {
      const d = self.opt.dialog ?? {};
      lua.lua_newtable(L);
      lua.lua_newtable(L);
      for (const [k, v] of Object.entries(d.values ?? {})) { self.#push(v); lua.lua_setfield(L, -2, S(k)); }
      lua.lua_setfield(L, -2, S("values"));
      if (d.button) { lua.lua_pushstring(L, S(d.button)); lua.lua_setfield(L, -2, S("button")); }
      if (d.open) { lua.lua_pushstring(L, S(d.open)); lua.lua_setfield(L, -2, S("open")); }
      if (d.save) { lua.lua_pushstring(L, S(d.save)); lua.lua_setfield(L, -2, S("save")); }
      return 1;
    });
    fn("dialog_log", () => {
      // record the control definitions that the macro requested
      const controls = [];
      const n = lua.lua_rawlen(L, 1);
      for (let i = 1; i <= n; i++) {
        lua.lua_rawgeti(L, 1, i);
        const c = {};
        for (const key of ["class", "name", "label", "hint"]) {
          lua.lua_getfield(L, -1, S(key));
          if (lua.lua_isstring(L, -1)) c[key] = to_jsstring(lua.lua_tostring(L, -1));
          lua.lua_pop(L, 1);
        }
        controls.push(c);
        lua.lua_pop(L, 1);
      }
      self.dialogs.push(controls);
      return 0;
    });
    fn("re_upper", () => { lua.lua_pushstring(L, S(str(1).toUpperCase())); return 1; });
    fn("re_lower", () => { lua.lua_pushstring(L, S(str(1).toLowerCase())); return 1; });
    fn("re_find", () => self.#reMatch(str(1), str(2), optstr(3), false));
    fn("re_match", () => self.#reMatch(str(1), str(2), optstr(3), true));
    fn("re_sub", () => {
      const [s, pat, rep] = [str(1), str(2), str(3)];
      const count = num(4, 0); const flags = optstr(5);
      let re;
      try { re = makeRegex(pat, flags, true); } catch (e) { return lauxlib.luaL_error(L, S("regex error: " + e.message)); }
      let n = 0;
      const out = s.replace(re, (...m) => {
        if (count && n >= count) return m[0];
        n++;
        return rep.replace(/\\(\d)|\$(\d)/g, (_, a, b) => m[Number(a ?? b)] ?? "");
      });
      lua.lua_pushstring(L, S(out)); lua.lua_pushinteger(L, n);
      return 2;
    });
    lua.lua_setglobal(L, S("__host"));
  }

  #reMatch(s, pat, flags, whole) {
    const L = this.L;
    let re;
    try { re = makeRegex(pat, flags, !whole); } catch (e) { return lauxlib.luaL_error(L, S("regex error: " + e.message)); }
    const matches = [];
    if (re.global) { for (const m of s.matchAll(re)) matches.push(m); } else { const m = re.exec(s); if (m) matches.push(m); }
    if (!matches.length) { lua.lua_pushnil(L); return 1; }
    lua.lua_newtable(L);
    let k = 0;
    for (const m of matches) for (let g = 0; g < m.length; g++) {
      if (m[g] === undefined) continue;
      lua.lua_newtable(L);
      lua.lua_pushstring(L, S(m[g])); lua.lua_setfield(L, -2, S("str"));
      const first = g === 0 ? m.index : s.indexOf(m[g], m.index);
      lua.lua_pushinteger(L, first + 1); lua.lua_setfield(L, -2, S("first"));
      lua.lua_pushinteger(L, first + m[g].length); lua.lua_setfield(L, -2, S("last"));
      lua.lua_rawseti(L, -2, ++k);
    }
    return 1;
  }

  #fsPath(p) { return resolve(this.opt.scriptDir ?? process.cwd(), this.#decodePath(p)); }

  #decodePath(p) {
    const base = this.opt.scriptDir ?? process.cwd();
    if (p.startsWith("?script")) return join(base, p.slice(7));
    if (p.startsWith("?data")) return join(base, p.slice(5));
    if (p.startsWith("?temp")) return join(process.env.TMPDIR ?? "/tmp", p.slice(5));
    if (p.startsWith("?user")) return join(process.env.HOME ?? ".", ".osisub", p.slice(5));
    if (p.startsWith("?video")) return join(dirname(this.opt.video ?? base + "/x"), p.slice(6));
    return p;
  }

  // -- value marshalling ----------------------------------------------------
  #push(v) {
    const L = this.L;
    if (v === null || v === undefined) lua.lua_pushnil(L);
    else if (typeof v === "boolean") lua.lua_pushboolean(L, v);
    else if (typeof v === "number") Number.isInteger(v) ? lua.lua_pushinteger(L, v) : lua.lua_pushnumber(L, v);
    else if (typeof v === "string") lua.lua_pushstring(L, S(v));
    else if (Array.isArray(v)) { lua.lua_newtable(L); v.forEach((x, i) => { this.#push(x); lua.lua_rawseti(L, -2, i + 1); }); }
    else { lua.lua_newtable(L); for (const [k, x] of Object.entries(v)) { this.#push(x); lua.lua_setfield(L, -2, S(k)); } }
  }
  #set(key, v) { this.#push(v); lua.lua_setfield(this.L, -2, S(key)); }

  /** Push the subtitles table for the current doc. */
  #pushSubs() {
    const L = this.L;
    const doc = this.doc;
    lua.lua_newtable(L);
    let n = 0;
    const add = (fill) => {
      lua.lua_newtable(L);
      fill();
      lua.lua_rawseti(L, -2, ++n);
    };
    for (const i of doc.info) {
      if (i.comment !== undefined) { add(() => { this.#set("class", "info"); this.#set("key", ""); this.#set("value", i.comment); this.#set("section", "[Script Info]"); this.#set("raw", i.comment); }); continue; }
      add(() => { this.#set("class", "info"); this.#set("key", i.key); this.#set("value", i.value); this.#set("section", "[Script Info]"); this.#set("raw", `${i.key}: ${i.value}`); });
    }
    for (const s of doc.styles) {
      add(() => {
        this.#set("class", "style"); this.#set("section", "[V4+ Styles]");
        for (const k of STYLE_KEYS) this.#set(k, s[k]);
        this.#set("margin_b", s.margin_t); this.#set("margin_v", s.margin_t);
      });
    }
    for (const e of doc.events) {
      add(() => {
        this.#set("class", "dialogue"); this.#set("section", "[Events]");
        this.#set("comment", !!e.comment); this.#set("layer", e.layer);
        this.#set("start_time", e.start); this.#set("end_time", e.end);
        this.#set("style", e.style); this.#set("actor", e.actor); this.#set("effect", e.effect);
        this.#set("margin_l", e.margin_l); this.#set("margin_r", e.margin_r); this.#set("margin_t", e.margin_t);
        this.#set("margin_b", e.margin_t); this.#set("margin_v", e.margin_t);
        this.#set("text", e.text); this.#set("extradata", {});
      });
    }
    lua.lua_getglobal(L, S("__subs_mt"));
    lua.lua_setmetatable(L, -2);
  }

  /** Read the Lua subtitles table at stack index idx back into the doc. */
  #pullSubs(idx) {
    const L = this.L;
    idx = lua.lua_absindex(L, idx);
    const doc = this.doc;
    const info = [], styles = [], events = [];
    const n = lua.lua_rawlen(L, idx);
    const getS = (k, d = "") => { lua.lua_getfield(L, -1, S(k)); const v = lua.lua_isstring(L, -1) ? to_jsstring(lua.lua_tostring(L, -1)) : d; lua.lua_pop(L, 1); return v; };
    const getN = (k, d = 0) => { lua.lua_getfield(L, -1, S(k)); const v = lua.lua_isnumber(L, -1) ? lua.lua_tonumber(L, -1) : d; lua.lua_pop(L, 1); return v; };
    const getB = (k) => { lua.lua_getfield(L, -1, S(k)); const v = lua.lua_toboolean(L, -1); lua.lua_pop(L, 1); return v; };
    const rawField = (k) => { lua.lua_getfield(L, -1, S(k)); const t = lua.lua_type(L, -1); lua.lua_pop(L, 1); return t; };
    for (let i = 1; i <= n; i++) {
      lua.lua_rawgeti(L, idx, i);
      const cls = getS("class");
      if (cls === "info") {
        const key = getS("key");
        if (key === "" && rawField("value") === lua.LUA_TSTRING) info.push({ comment: getS("value") });
        else info.push({ key, value: getS("value") });
      } else if (cls === "style") {
        const s = {};
        for (const k of STYLE_KEYS) {
          if (["bold", "italic", "underline", "strikeout"].includes(k)) s[k] = getB(k);
          else if (["name", "fontname", "color1", "color2", "color3", "color4"].includes(k)) s[k] = getS(k);
          else s[k] = getN(k);
        }
        s.margin_b = getN("margin_b", s.margin_t);
        styles.push(s);
      } else if (cls === "dialogue") {
        events.push({
          comment: getB("comment"), layer: getN("layer"), start: Math.round(getN("start_time")), end: Math.round(getN("end_time")),
          style: getS("style", "Default"), actor: getS("actor"), margin_l: getN("margin_l"), margin_r: getN("margin_r"),
          margin_t: getN("margin_t", getN("margin_v")), effect: getS("effect"), text: getS("text"),
        });
      }
      lua.lua_pop(L, 1);
    }
    doc.info = info; doc.styles = styles; doc.events = events;
  }

  /** Load a macro script from source text. Returns the list of macro names it registered. */
  loadScript(source, path = "script.lua") {
    const L = this.L;
    this.scriptDirs = [...(this.scriptDirs ?? []), dirname(resolve(path))];
    this.opt.scriptDir ??= dirname(resolve(path));
    const before = this.#count("__macros"), beforeF = this.#count("__filters");
    const st = lauxlib.luaL_loadbuffer(L, to_luastring(source.replace(/^﻿/, "").replace(/^#![^\n]*/, "")), null, to_luastring("@" + path));
    if (st !== 0) throw new Error(this.#err());
    if (lua.lua_pcall(L, 0, 0, 0) !== 0) throw new Error(this.#err());
    const meta = {};
    for (const k of ["script_name", "script_description", "script_author", "script_version"]) {
      lua.lua_getglobal(L, S(k));
      if (lua.lua_isstring(L, -1)) meta[k] = to_jsstring(lua.lua_tostring(L, -1));
      lua.lua_pop(L, 1);
    }
    const entries = this.#registrations();
    const macros = entries.macros.slice(before);
    const filters = entries.filters.slice(beforeF);
    const rec = { path, ...meta, macros: macros.map((m) => m.name), filters: filters.map((m) => m.name) };
    this.scripts.push(rec);
    this.macros.push(...macros.map((m) => ({ ...m, script: path })));
    this.filters.push(...filters.map((m) => ({ ...m, script: path })));
    return rec;
  }
  loadScriptFile(path) { return this.loadScript(readFileSync(path, "utf8"), path); }

  #count(which) {
    const L = this.L;
    lua.lua_getglobal(L, S("aegisub"));
    lua.lua_getfield(L, -1, S(which));
    const n = lua.lua_rawlen(L, -1);
    lua.lua_pop(L, 2);
    return n;
  }
  #registrations() {
    const L = this.L;
    const read = (which) => {
      const out = [];
      lua.lua_getglobal(L, S("aegisub"));
      lua.lua_getfield(L, -1, S(which));
      const n = lua.lua_rawlen(L, -1);
      for (let i = 1; i <= n; i++) {
        lua.lua_rawgeti(L, -1, i);
        lua.lua_getfield(L, -1, S("name"));
        const name = to_jsstring(lua.lua_tostring(L, -1));
        lua.lua_pop(L, 1);
        lua.lua_getfield(L, -1, S("description"));
        const description = lua.lua_isstring(L, -1) ? to_jsstring(lua.lua_tostring(L, -1)) : "";
        lua.lua_pop(L, 2);
        out.push({ name, description, index: i });
      }
      lua.lua_pop(L, 2);
      return out;
    };
    return { macros: read("__macros"), filters: read("__filters") };
  }

  /** Run a macro by (exact or case-insensitive) name. selected is 0-based indices into doc.events. */
  runMacro(name, { selected = null, active = null } = {}) {
    const m = this.macros.find((x) => x.name === name) ?? this.macros.find((x) => x.name.toLowerCase() === String(name).toLowerCase());
    if (!m) throw new Error(`Macro not found: ${name}`);
    return this.#call("__macros", m.index ?? this.macros.indexOf(m) + 1, m.name, selected, active, false);
  }
  runFilter(name, opts = {}) {
    const m = this.filters.find((x) => x.name === name);
    if (!m) throw new Error(`Filter not found: ${name}`);
    return this.#call("__filters", m.index, m.name, null, null, true);
  }

  #call(which, index, name, selected, active, isFilter) {
    const L = this.L;
    const top = lua.lua_gettop(L);
    const headerCount = this.doc.info.length + this.doc.styles.length;
    this.#pushSubs();
    const subsIdx = lua.lua_gettop(L);
    lua.lua_getglobal(L, S("aegisub"));
    lua.lua_getfield(L, -1, S(which));
    lua.lua_rawgeti(L, -1, index);
    lua.lua_getfield(L, -1, S("run"));
    lua.lua_pushvalue(L, subsIdx);
    let nargs;
    if (isFilter) { this.#push({}); nargs = 2; }
    else {
      const sel = (selected ?? this.doc.events.map((_, i) => i)).map((i) => i + headerCount + 1);
      this.#push(sel);
      this.#push(active === null ? (sel[0] ?? 1) : active + headerCount + 1);
      nargs = 3;
    }
    const st = lua.lua_pcall(L, nargs, lua.LUA_MULTRET, 0);
    if (st !== 0) {
      const msg = this.#err();
      lua.lua_settop(L, top);
      if (msg === "__aegisub_cancel__") return { cancelled: true, logs: this.logs };
      throw new Error(`Macro "${name}" failed: ${msg}`);
    }
    this.#pullSubs(subsIdx);
    const base = subsIdx + 3; // aegisub, list, entry remain below the results
    const ret = lua.lua_gettop(L) - base;
    let newSel = null, newActive = null;
    const hc = this.doc.info.length + this.doc.styles.length;
    if (ret >= 1 && lua.lua_istable(L, base + 1)) {
      newSel = [];
      const n = lua.lua_rawlen(L, base + 1);
      for (let i = 1; i <= n; i++) { lua.lua_rawgeti(L, base + 1, i); newSel.push(lua.lua_tointeger(L, -1) - hc - 1); lua.lua_pop(L, 1); }
    }
    if (ret >= 2 && lua.lua_isnumber(L, base + 2)) newActive = lua.lua_tointeger(L, base + 2) - hc - 1;
    lua.lua_settop(L, top);
    return { cancelled: false, selected: newSel, active: newActive, logs: this.logs };
  }

  close() { lua.lua_close(this.L); }
}

function makeRegex(pat, flags, global) {
  let f = "u";
  if (global || /g/.test(flags)) f += "g";
  if (/i/.test(flags)) f += "i";
  // Boost/Perl-ish: (?i) prefix
  if (pat.startsWith("(?i)")) { pat = pat.slice(4); if (!f.includes("i")) f += "i"; }
  return new RegExp(pat, f);
}

export async function runLuaFile({ doc, script, macro, selected, active, opt }) {
  const h = new AutomationHost(doc, opt);
  const info = h.loadScriptFile(script);
  const name = macro ?? info.macros[0];
  const res = h.runMacro(name, { selected, active });
  return { host: h, info, result: res };
}
