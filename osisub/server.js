// OsiSub local server: static UI + Lua automation sessions (Lua runs server-side via fengari).
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomUUID } from 'node:crypto';
import { LuaScript } from './src/core/lua.js';

const root = path.dirname(fileURLToPath(import.meta.url));
const MIME = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.css': 'text/css', '.json': 'application/json', '.svg': 'image/svg+xml' };
const scripts = new Map(); // id -> { script, pending }

// one outstanding wait per script: resolved by the next dialog request or the final result
const pump = s => new Promise(r => { s.signal = r; });

const readBody = req => new Promise((res, rej) => { let b = ''; req.on('data', c => (b += c)); req.on('end', () => res(b ? JSON.parse(b) : {})); req.on('error', rej); });
const send = (res, code, obj) => { res.writeHead(code, { 'Content-Type': 'application/json' }); res.end(JSON.stringify(obj)); };

async function api(req, res, url) {
  const body = await readBody(req);
  try {
    if (url === '/api/lua/load') {
      const host = { fps: body.fps || 23.976, videoSize: body.videoSize, fileName: body.fileName || '', properties: body.properties || {},
        readFile: name => body.files?.[name] ?? '' };
      const script = new LuaScript(body.source, body.filename || 'script.lua', host);
      const id = randomUUID(); scripts.set(id, { script });
      return send(res, 200, { id, info: script.info });
    }
    const s = scripts.get(body.id);
    if (!s) return send(res, 404, { error: 'unknown script' });
    if (url === '/api/lua/validate') return send(res, 200, await s.script.validate(body.index, body.lines, body.selection, body.active));
    if (url === '/api/lua/run') {
      s.script.logs = []; if (body.fps) s.script.host.fps = body.fps;
      const next = pump(s);
      const onDialog = request => new Promise(resume => { s.resume = resume; s.signal({ status: 'dialog', request }); });
      s.script.run(body.kind || 'macro', body.index, body.lines, body.selection || [], body.active || 0, onDialog)
        .then(r => s.signal({ status: 'done', ...r }), e => s.signal({ status: 'error', error: e.message, logs: s.script.logs }));
      return send(res, 200, await next);
    }
    if (url === '/api/lua/resume') {
      const next = pump(s); s.resume(body.response);
      return send(res, 200, await next);
    }
    if (url === '/api/lua/unload') { scripts.delete(body.id); return send(res, 200, {}); }
  } catch (e) { return send(res, 200, { status: 'error', error: e.message, logs: scripts.get(body.id)?.script.logs }); }
  send(res, 404, { error: 'not found' });
}

http.createServer(async (req, res) => {
  const url = decodeURIComponent(req.url.split('?')[0]);
  if (req.method === 'POST' && url.startsWith('/api/')) return api(req, res, url);
  let file = url === '/' ? '/web/index.html' : url;
  if (!file.startsWith('/web/') && !file.startsWith('/src/core/')) return send(res, 404, { error: 'not found' });
  const p = path.join(root, file);
  if (!p.startsWith(root) || !fs.existsSync(p) || fs.statSync(p).isDirectory()) return send(res, 404, { error: 'not found' });
  res.writeHead(200, { 'Content-Type': MIME[path.extname(p)] || 'application/octet-stream', 'Cache-Control': 'no-cache' });
  fs.createReadStream(p).pipe(res);
}).listen(process.env.PORT || 8765, () => console.log(`OsiSub running at http://localhost:${process.env.PORT || 8765}`));
