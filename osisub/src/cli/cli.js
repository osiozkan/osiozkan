#!/usr/bin/env node
import { readFileSync, writeFileSync } from "node:fs";
import { extname } from "node:path";
import { load, save, check, runAllFixes, fixes, shift, stretch, changeFps, DEFAULT_RULES, formatAssTime } from "../core/index.js";
import { AutomationHost } from "../lua/host.js";

const HELP = `osisub - anime subtitle toolkit

  osisub convert <in> <out>                 convert between .ass .srt .vtt .txt
  osisub check <file> [--cps N --cpl N --lines N --min MS --max MS --json]
  osisub fix <in> [out] [--min-gap MS --min-duration MS]
  osisub shift <in> <out> <ms>              shift all times (negative allowed)
  osisub sync <in> <out> a0 a1 b0 b1        linear retime (ms): a0->b0, a1->b1
  osisub fps <in> <out> <from> <to>         convert frame rate, e.g. 23.976 25
  osisub macros <script.lua>                list macros/filters in a Lua script
  osisub run <in> <script.lua> [out] [--macro NAME] [--select 1-5,9] [--set key=value ...] [--button NAME] [--fps N]
  osisub serve [--port N]                   start the web editor
`;

function parseArgs(argv) {
  const pos = [], flags = {}, sets = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--set") { const [k, ...v] = argv[++i].split("="); sets[k] = v.join("="); }
    else if (a.startsWith("--")) {
      const k = a.slice(2);
      if (i + 1 < argv.length && !argv[i + 1].startsWith("--") && !["json"].includes(k)) flags[k] = argv[++i]; else flags[k] = true;
    } else pos.push(a);
  }
  return { pos, flags, sets };
}

const fmtOf = (p) => ({ ".ass": "ass", ".ssa": "ass", ".srt": "srt", ".vtt": "vtt", ".txt": "txt" })[extname(p).toLowerCase()] ?? "ass";
const readDoc = (p) => load(readFileSync(p, "utf8"), p);
const writeDoc = (doc, p) => writeFileSync(p, save(doc, fmtOf(p)));

function parseSelect(s, n) {
  if (!s) return null;
  const out = new Set();
  for (const part of String(s).split(",")) {
    const m = /^(\d+)(?:-(\d+))?$/.exec(part.trim());
    if (!m) continue;
    for (let i = +m[1]; i <= +(m[2] ?? m[1]); i++) if (i >= 1 && i <= n) out.add(i - 1);
  }
  return [...out].sort((a, b) => a - b);
}

async function main() {
  const [cmd, ...rest] = process.argv.slice(2);
  const { pos, flags, sets } = parseArgs(rest);
  switch (cmd) {
    case "convert": { const { doc } = readDoc(pos[0]); writeDoc(doc, pos[1]); break; }
    case "check": {
      const { doc } = readDoc(pos[0]);
      const rules = {};
      if (flags.cps) rules.maxCps = +flags.cps;
      if (flags.cpl) rules.maxCpl = +flags.cpl;
      if (flags.lines) rules.maxLines = +flags.lines;
      if (flags.min) rules.minDuration = +flags.min;
      if (flags.max) rules.maxDuration = +flags.max;
      const issues = check(doc, rules);
      if (flags.json) console.log(JSON.stringify(issues, null, 2));
      else for (const i of issues) console.log(`#${i.index + 1} [${formatAssTime(doc.events[i.index].start)}] ${i.severity.padEnd(5)} ${i.code}: ${i.message}`);
      console.error(`${issues.length} issue(s) in ${doc.events.length} events`);
      process.exitCode = issues.some((i) => i.severity === "error") ? 2 : 0;
      break;
    }
    case "fix": {
      const { doc } = readDoc(pos[0]);
      const rules = {};
      if (flags["min-gap"]) rules.minGap = +flags["min-gap"];
      if (flags["min-duration"]) rules.minDuration = +flags["min-duration"];
      const r = runAllFixes(doc, rules);
      writeDoc(doc, pos[1] ?? pos[0]);
      console.error(JSON.stringify(r));
      break;
    }
    case "shift": { const { doc } = readDoc(pos[0]); shift(doc, +pos[2]); writeDoc(doc, pos[1]); break; }
    case "sync": { const { doc } = readDoc(pos[0]); stretch(doc, ...pos.slice(2, 6).map(Number)); writeDoc(doc, pos[1]); break; }
    case "fps": { const { doc } = readDoc(pos[0]); changeFps(doc, +pos[2], +pos[3]); writeDoc(doc, pos[1]); break; }
    case "macros": {
      const h = new AutomationHost({ info: [], styles: [], events: [] });
      const s = h.loadScriptFile(pos[0]);
      console.log(JSON.stringify(s, null, 2));
      break;
    }
    case "run": {
      const { doc } = readDoc(pos[0]);
      const h = new AutomationHost(doc, { fps: flags.fps ? +flags.fps : 23.976, dialog: { values: sets, button: flags.button }, filename: pos[0] });
      const info = h.loadScriptFile(pos[1]);
      const name = flags.macro ?? info.macros[0] ?? info.filters[0];
      if (!name) throw new Error("script registered no macros");
      const res = info.macros.includes(name) ? h.runMacro(name, { selected: parseSelect(flags.select, doc.events.length) }) : h.runFilter(name);
      for (const l of h.logs) process.stderr.write(l.message);
      if (res.cancelled) { console.error("macro cancelled"); process.exitCode = 1; break; }
      writeDoc(doc, pos[2] ?? pos[0]);
      break;
    }
    case "serve": { const { serve } = await import("./server.js"); await serve({ port: flags.port ? +flags.port : 8787 }); break; }
    default: console.log(HELP); process.exitCode = cmd ? 1 : 0;
  }
}
main().catch((e) => { console.error("error:", e.message); process.exit(1); });
