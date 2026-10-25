// Bundles the ASS renderer (JASSUB / libass-wasm) into web/vendor so the browser needs no import map or CDN.
import { build } from 'esbuild';
import fs from 'node:fs';
const out = new URL('../web/vendor/', import.meta.url).pathname; fs.mkdirSync(out, { recursive: true });
const jd = new URL('../node_modules/jassub/dist/', import.meta.url).pathname;
await build({ entryPoints: [jd + 'jassub.js'], bundle: true, format: 'esm', outfile: out + 'jassub.js', logLevel: 'error' });
await build({ entryPoints: [jd + 'worker/worker.js'], bundle: true, format: 'esm', outfile: out + 'jassub-worker.js', logLevel: 'error', loader: { '.wasm': 'file' } });
for (const f of ['jassub-worker.wasm', 'jassub-worker-modern.wasm']) fs.copyFileSync(jd + 'wasm/' + f, out + f);
fs.copyFileSync(jd + 'default.woff2', out + 'default.woff2');
console.log('vendor built');
