// LuaJIT (Aegisub) uyumu: 5.3 "5.0" yerine "5" yazdırmalı → LUA_COMPAT_FLOATSTRING=true.
import { readFileSync, writeFileSync, existsSync } from "node:fs";
const p = new URL("../node_modules/fengari/src/luaconf.js", import.meta.url);
if (existsSync(p)) {
  const s = readFileSync(p, "utf8");
  const t = s.replace("conf.LUA_COMPAT_FLOATSTRING || false;", "true;");
  if (s !== t) { writeFileSync(p, t); console.log("fengari patched (FLOATSTRING)"); }
}
