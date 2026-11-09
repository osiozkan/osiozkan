import * as fengari from "fengari";
import { AssFile, Style } from "../core/ass";
import preludeSrc from "./prelude.lua?raw";
import utilSrc from "./util.lua?raw";
import karaskelSrc from "./karaskel.lua?raw";

const { lua, lauxlib, lualib, to_luastring, to_jsstring } = fengari as any;

export interface MacroInfo { id: number; name: string; desc: string; kind: "macro" | "filter"; script: string }
export interface ScriptInfo { name: string; script_name: string; script_description: string; script_author: string; script_version: string }
export interface DialogRequest { config: any[]; buttons: string[]; ids?: any }
export interface HostHooks {
  log?: (level: number, msg: string) => void;
  progress?: (kind: string, v: string) => void;
  /** Diyalog: kullanıcı yanıtı {button, values}. Verilmezse varsayılan değerlerle ilk düğme. */
  dialog?: (req: DialogRequest) => Promise<{ button: string | false; values: Record<string, any> }>;
  fileDialog?: (req: { kind: "open" | "save"; title?: string; wildcards?: string }) => Promise<string | null>;
  textExtents?: (style: any, text: string) => { w: number; h: number; d: number; el: number };
  readScriptFile?: (name: string) => string | null;
  fps?: number;
  videoSize?: { w: number; h: number };
  keyframes?: number[];
  project?: Record<string, any>;
  clipboard?: { get(): string; set(s: string): void };
}

export interface RunResult {
  status: "done" | "cancelled" | "error";
  message?: string;
  file?: AssFile;
  sel?: number[];
  active?: number;
}

/** Varsayılan metin ölçümü (tarayıcıda canvas ile değiştirilir): ortalama genişlik ≈ 0.52em. */
export function estimateExtents(style: any, text: string) {
  const fs = style.fontsize ?? 40;
  const sx = (style.scale_x ?? 100) / 100;
  let w = 0;
  for (const ch of text) w += (ch.charCodeAt(0) > 0x2e80 ? 1.0 : ch === " " ? 0.3 : 0.55) * fs * sx + (style.spacing ?? 0);
  return { w, h: fs * 1.2, d: fs * 0.2, el: 0 };
}

const EMBEDDED: Record<string, string> = {
  karaskel: karaskelSrc, utils: utilSrc, util: utilSrc, "aegisub.util": utilSrc,
};

export class LuaRuntime {
  private L: any;
  private hooks: HostHooks;
  logs: { level: number; msg: string }[] = [];
  macros: MacroInfo[] = [];
  scripts: ScriptInfo[] = [];

  constructor(hooks: HostHooks = {}) {
    this.hooks = hooks;
    const L = (this.L = lauxlib.luaL_newstate());
    lualib.luaL_openlibs(L);
    this.installHost();
    this.exec(preludeSrc, "prelude");
  }

  private installHost() {
    const L = this.L, h = this.hooks;
    lua.lua_newtable(L);
    // Not: ana thread yerine çağıran (coroutine) thread'inin yığını kullanılmalı.
    let T: any = L;
    const reg = (name: string, fn: () => number) => {
      lua.lua_pushjsfunction(L, (Lc: any) => { T = Lc; return fn(); });
      lua.lua_setfield(L, -2, to_luastring(name));
    };
    const str = (i: number): string => to_jsstring(lua.lua_tostring(T, i)) ?? "";
    const num = (i: number): number => lua.lua_tonumber(T, i);
    const pushS = (s: string) => lua.lua_pushstring(T, to_luastring(s));
    reg("log", () => { const lv = num(1), m = str(2); this.logs.push({ level: lv, msg: m }); h.log?.(lv, m); return 0; });
    reg("progress", () => { h.progress?.(str(1), str(2)); return 0; });
    reg("extents", () => {
      const st = JSON.parse(str(1)); const t = str(2);
      const r = (h.textExtents ?? estimateExtents)(st, t) as any;
      pushS(`${r.w}|${r.h}|${r.d}|${r.el}`); return 1;
    });
    reg("frame_from_ms", () => { const fps = h.fps; lua.lua_pushnumber(T, fps ? Math.floor((num(1) * fps) / 1000 + 1e-6) : -1); return 1; });
    reg("ms_from_frame", () => { const fps = h.fps; lua.lua_pushnumber(T, fps ? Math.round((num(1) * 1000) / fps) : -1); return 1; });
    reg("video_size", () => { pushS(h.videoSize ? `${h.videoSize.w}|${h.videoSize.h}` : "0|0"); return 1; });
    reg("keyframes", () => { pushS(JSON.stringify(h.keyframes ?? [])); return 1; });
    reg("project", () => { pushS(JSON.stringify(h.project ?? {})); return 1; });
    reg("project_name", () => { pushS(String(h.project?.script_file ?? "")); return 1; });
    reg("clipboard_get", () => { pushS(h.clipboard?.get() ?? ""); return 1; });
    reg("clipboard_set", () => { h.clipboard?.set(str(1)); return 0; });
    reg("upper", () => { pushS(str(1).toUpperCase()); return 1; });
    reg("lower", () => { pushS(str(1).toLowerCase()); return 1; });
    reg("embedded", () => {
      const name = str(1);
      const src = EMBEDDED[name];
      if (src === undefined) lua.lua_pushnil(T); else pushS(src);
      return 1;
    });
    reg("read_script_file", () => {
      const r = h.readScriptFile?.(str(1)) ?? null;
      if (r === null) lua.lua_pushnil(T); else pushS(r);
      return 1;
    });
    reg("re", () => { pushS(JSON.stringify(reOp(JSON.parse(str(1))))); return 1; });
    lua.lua_setglobal(L, to_luastring("__host"));
  }

  private exec(src: string, name: string) {
    const L = this.L;
    const st = lauxlib.luaL_loadbuffer(L, to_luastring(src), null, to_luastring("=" + name));
    if (st !== 0 || lua.lua_pcall(L, 0, 0, 0) !== 0) {
      const err = to_jsstring(lua.lua_tostring(L, -1));
      lua.lua_pop(L, 1);
      throw new Error(`Lua init (${name}): ${err}`);
    }
  }

  /** JS değerini yerel olarak Lua yığınına it (büyük veri için JSON'dan çok daha hızlı). */
  private pushValue(v: any) {
    const L = this.L;
    if (v === null || v === undefined) lua.lua_pushnil(L);
    else if (typeof v === "boolean") lua.lua_pushboolean(L, v);
    else if (typeof v === "number") Number.isInteger(v) ? lua.lua_pushinteger(L, v) : lua.lua_pushnumber(L, v);
    else if (typeof v === "string") lua.lua_pushstring(L, to_luastring(v));
    else if (Array.isArray(v)) {
      lua.lua_createtable(L, v.length, 0);
      for (let i = 0; i < v.length; i++) { this.pushValue(v[i]); lua.lua_rawseti(L, -2, i + 1); }
    } else {
      lua.lua_createtable(L, 0, 8);
      for (const k in v) { if (v[k] === undefined) continue; this.pushValue(v[k]); lua.lua_setfield(L, -2, to_luastring(k)); }
    }
  }
  private toValue(idx: number): any {
    const L = this.L;
    switch (lua.lua_type(L, idx)) {
      case lua.LUA_TBOOLEAN: return !!lua.lua_toboolean(L, idx);
      case lua.LUA_TNUMBER: return lua.lua_tonumber(L, idx);
      case lua.LUA_TSTRING: return to_jsstring(lua.lua_tostring(L, idx));
      case lua.LUA_TTABLE: {
        idx = lua.lua_absindex(L, idx);
        const arr: any[] = []; const obj: Record<string, any> = {}; let isArr = true, n = 0;
        lua.lua_pushnil(L);
        while (lua.lua_next(L, idx) !== 0) {
          const kt = lua.lua_type(L, -2);
          const vt = lua.lua_type(L, -1);
          if (vt !== lua.LUA_TFUNCTION && vt !== lua.LUA_TUSERDATA && vt !== lua.LUA_TTHREAD) {
            const val = this.toValue(-1);
            if (kt === lua.LUA_TNUMBER && Number.isInteger(lua.lua_tonumber(L, -2)) && lua.lua_tonumber(L, -2) >= 1) { arr[lua.lua_tonumber(L, -2) - 1] = val; n++; obj[String(lua.lua_tonumber(L, -2))] = val; }
            else { isArr = false; obj[to_jsstring(lua.lua_tostring(L, -2))] = val; }
          }
          lua.lua_pop(L, 1);
        }
        return isArr && (n === 0 || arr.length === n) ? arr : obj;
      }
      default: return undefined;
    }
  }
  /** Lua işlevini JS değerleriyle çağırır, tek dönüş değerini JS'e çevirir. */
  private callV(fn: string, ...args: any[]): any {
    const L = this.L;
    lua.lua_getglobal(L, to_luastring(fn));
    for (const a of args) this.pushValue(a);
    if (lua.lua_pcall(L, args.length, 1, 0) !== 0) {
      const err = to_jsstring(lua.lua_tostring(L, -1)); lua.lua_pop(L, 1); throw new Error(err);
    }
    const out = this.toValue(-1); lua.lua_pop(L, 1); return out;
  }

  private call(fn: string, ...args: (string | number)[]): any {
    const L = this.L;
    lua.lua_getglobal(L, to_luastring(fn));
    for (const a of args) typeof a === "number" ? lua.lua_pushnumber(L, a) : lua.lua_pushstring(L, to_luastring(a));
    if (lua.lua_pcall(L, args.length, 1, 0) !== 0) {
      const err = to_jsstring(lua.lua_tostring(L, -1));
      lua.lua_pop(L, 1);
      throw new Error(err);
    }
    const out = to_jsstring(lua.lua_tostring(L, -1));
    lua.lua_pop(L, 1);
    return JSON.parse(out);
  }

  /** Bir Lua betiğini yükler; kayıtlı makroları döndürür. */
  loadScript(name: string, source: string): { ok: boolean; message?: string; macros: MacroInfo[] } {
    const r = this.call("__load_script", name, source);
    if (!r.ok) return { ok: false, message: r.message, macros: [] };
    this.scripts = this.scripts.filter((s) => s.name !== name);
    this.macros = this.macros.filter((m) => m.script !== name);
    this.scripts.push({ name, ...r.meta });
    const ms: MacroInfo[] = (r.macros ?? []).map((m: any) => ({ ...m, script: name }));
    this.macros.push(...ms);
    return { ok: true, macros: ms };
  }

  private input(file: AssFile, sel: number[], active?: number) {
    return { info: file.info, styles: file.styles, events: file.events, sel, active };
  }

  validate(id: number, file: AssFile, sel: number[], active?: number): boolean {
    try { return !!this.callV("__validate", id, this.input(file, sel, active)).ok; } catch { return false; }
  }

  async run(id: number, file: AssFile, sel: number[] = [], active?: number): Promise<RunResult> {
    const nHead = file.info.length + file.styles.length;
    this.logs = [];
    let r = this.callV("__start", id, this.input(file, sel.map((i) => i + 1 + nHead), active !== undefined ? active + 1 + nHead : undefined));
    for (let guard = 0; guard < 10000; guard++) {
      if (r.status === "request") {
        const req = r.req;
        let resp: any;
        if (req.__req === "dialog") {
          resp = this.hooks.dialog ? await this.hooks.dialog({ config: req.config ?? [], buttons: req.buttons ?? ["OK", "Cancel"], ids: req.ids })
            : defaultDialog(req.config ?? [], req.buttons ?? ["OK", "Cancel"]);
        } else resp = { path: (await this.hooks.fileDialog?.({ kind: req.__req, title: req.title, wildcards: req.wildcards })) ?? null };
        r = this.callV("__resume", resp ?? {});
        continue;
      }
      break;
    }
    if (r.status === "cancelled") return { status: "cancelled" };
    if (r.status === "error") return { status: "error", message: r.message };
    // sonuç: Lua tarafı dump_lines → AssFile
    const out: AssFile = {
      info: r.info, extra: file.extra, styleFormat: file.styleFormat, eventFormat: file.eventFormat,
      styles: (r.styles as any[]).map(fillStyle),
      events: (r.events as any[]).map((e) => ({ ...e, end: e["end"] })),
    };
    const sel2 = r.sel ? (r.sel as number[]).map((i) => i - 1 - (out.info.length + out.styles.length)).filter((i) => i >= 0) : undefined;
    return { status: "done", file: out, sel: sel2, active: r.active !== undefined ? r.active - 1 - (out.info.length + out.styles.length) : undefined };
  }
}

function fillStyle(s: any): Style {
  return { ...s };
}

/** Diyalog varsayılanlarıyla ilk düğmeyi "basar" (başsız/test modu). */
export function defaultDialog(config: any[], buttons: string[]) {
  const values: Record<string, any> = {};
  for (const c of config) {
    if (!c.name) continue;
    if (c.class === "checkbox") values[c.name] = !!c.value;
    else if (c.class === "dropdown") values[c.name] = c.value ?? (c.items?.[0] ?? "");
    else if (["intedit", "floatedit", "scrollbar", "slider"].includes(c.class)) values[c.name] = c.value ?? c.min ?? 0;
    else if (["color", "coloralpha", "alpha"].includes(c.class)) values[c.name] = c.value ?? "&H00FFFFFF&";
    else values[c.name] = c.text ?? c.value ?? "";
  }
  return { button: buttons[0] ?? "OK", values };
}

/* ---- Boost.Regex benzeri işlemler JS RegExp ile ---- */
const enc = new TextEncoder();
const byteOff = (s: string, i: number) => enc.encode(s.slice(0, i)).length;
function mkRe(pat: string, flags: number, extra = "") {
  return new RegExp(pat, (flags & 1 ? "i" : "") + "u" + extra);
}
function groups(s: string, m: RegExpExecArray) {
  const out: any[] = [];
  const idx = (m as any).indices as [number, number][] | undefined;
  for (let k = 0; k < m.length; k++) {
    const g = m[k];
    if (g === undefined) { out.push({ str: "", first: 0, last: -1 }); continue; }
    const st = idx?.[k]?.[0] ?? (k === 0 ? m.index : s.indexOf(g, m.index));
    out.push({ str: g, first: byteOff(s, st) + 1, last: byteOff(s, st) + enc.encode(g).length });
  }
  return out;
}
function reOp(q: { op: string; args: any[] }): any {
  const [s, pat, a3, a4, a5] = q.args as [string, string, any, any, any];
  try {
    if (q.op === "find") {
      const re = mkRe(pat, a3 ?? 0, "gd"); const res: any[] = [];
      for (const m of s.matchAll(re)) res.push(groups(s, m as any));
      return res;
    }
    if (q.op === "match") {
      const re = mkRe(pat, a3 ?? 0, "d"); const m = re.exec(s);
      return m ? groups(s, m) : null;
    }
    if (q.op === "sub") {
      const repl = String(a3).replace(/\$/g, "$$$$").replace(/\\(\d)/g, "$$$1").replace(/\\g<(\d+)>/g, "$$$1");
      const count = a4 ?? 0; const re = mkRe(pat, a5 ?? 0, "g"); let n = 0;
      return s.replace(re, (...m) => {
        n++;
        if (count > 0 && n > count) return m[0];
        const groupsArr = m.slice(0, -2).filter((x) => typeof x === "string" || x === undefined);
        return String(a3).replace(/\\(\d)/g, (_x, d) => groupsArr[+d] ?? "").replace(/\\\\/g, "\\");
      });
      void repl;
    }
    if (q.op === "split") return s.split(mkRe(pat, a3 ?? 0, "g"));
  } catch { return null; }
  return null;
}
