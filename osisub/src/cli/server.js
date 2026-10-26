import http from "node:http";
import { readFile } from "node:fs/promises";
import { join, extname, normalize, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { load, save, serialize } from "../core/index.js";
import { AutomationHost } from "../lua/host.js";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const MIME = { ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".css": "text/css", ".json": "application/json", ".svg": "image/svg+xml" };

async function body(req) {
  const chunks = [];
  let n = 0;
  for await (const c of req) { n += c.length; if (n > 64 * 1024 * 1024) throw new Error("body too large"); chunks.push(c); }
  return JSON.parse(Buffer.concat(chunks).toString("utf8") || "{}");
}

/** POST /api/macros {script} -> script info. POST /api/run {text,name,script,macro,selected,active,dialog,fps} -> {text,logs,selected,active} */
export async function api(path, data) {
  if (path === "/api/macros") {
    const h = new AutomationHost({ info: [], styles: [], events: [] });
    try { return h.loadScript(data.script, data.scriptPath ?? "script.lua"); } finally { h.close(); }
  }
  if (path === "/api/run") {
    const { doc } = load(data.text, data.name ?? "x.ass");
    const h = new AutomationHost(doc, { fps: data.fps ?? 23.976, dialog: { collect: true, ...(data.dialog ?? {}) }, keyframes: data.keyframes ?? [], videoSize: data.videoSize });
    try {
      const info = h.loadScript(data.script, data.scriptPath ?? "script.lua");
      const name = data.macro ?? info.macros[0];
      const res = info.filters.includes(name) ? h.runFilter(name) : h.runMacro(name, { selected: data.selected ?? null, active: data.active ?? null });
      if (res.needsDialog) return { needsDialog: res.needsDialog, logs: h.logs };
      return { text: serialize(doc), logs: h.logs, cancelled: res.cancelled, selected: res.selected, active: res.active, dialogs: h.dialogs };
    } finally { h.close(); }
  }
  throw Object.assign(new Error("not found"), { status: 404 });
}

export function serve({ port = 8787, host = "127.0.0.1" } = {}) {
  const server = http.createServer(async (req, res) => {
    try {
      const url = new URL(req.url, "http://x");
      if (req.method === "POST" && url.pathname.startsWith("/api/")) {
        const out = await api(url.pathname, await body(req));
        res.writeHead(200, { "content-type": "application/json" }).end(JSON.stringify(out));
        return;
      }
      if (url.pathname === "/") { res.writeHead(302, { location: "/web/index.html" }).end(); return; }
      let p = url.pathname;
      p = normalize(p).replace(/^(\.\.[/\\])+/, "");
      if (!(p.startsWith("/web/") || p.startsWith("/src/core/"))) { res.writeHead(404).end("not found"); return; }
      const buf = await readFile(join(root, p));
      res.writeHead(200, { "content-type": MIME[extname(p)] ?? "application/octet-stream" }).end(buf);
    } catch (e) {
      const status = e.code === "ENOENT" ? 404 : e.status ?? 500;
      res.writeHead(status, { "content-type": "application/json" }).end(JSON.stringify({ error: e.message }));
    }
  });
  return new Promise((resolve) => server.listen(port, host, () => { console.error(`osisub editor: http://${host}:${port}/`); resolve(server); }));
}
