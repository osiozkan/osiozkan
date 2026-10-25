// Real font metrics for aegisub.text_extents (server side), via opentype.js.
// Font lookup: <project>/fonts, extra dirs from OSISUB_FONTS (path-delimited), then system font dirs.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const opentype = require('opentype.js');

const SYSTEM_DIRS = ['/usr/share/fonts', '/usr/local/share/fonts', path.join(os.homedir(), '.fonts'), path.join(os.homedir(), '.local/share/fonts'),
  '/System/Library/Fonts', '/Library/Fonts', path.join(os.homedir(), 'Library/Fonts'), 'C:\\Windows\\Fonts'];

export class FontManager {
  constructor(extraDirs = []) {
    this.dirs = [...extraDirs, ...(process.env.OSISUB_FONTS ? process.env.OSISUB_FONTS.split(path.delimiter) : []), ...SYSTEM_DIRS].filter(d => fs.existsSync(d));
    this.index = null; // lowercase family(+style) -> [{file, bold, italic}]
    this.cache = new Map();
  }
  _scan(dir, out, depth = 0) {
    if (depth > 6) return;
    let ents; try { ents = fs.readdirSync(dir, { withFileTypes: true }); } catch { return; }
    for (const e of ents) {
      const p = path.join(dir, e.name);
      if (e.isDirectory()) this._scan(p, out, depth + 1);
      else if (/\.(ttf|otf|ttc)$/i.test(e.name)) out.push(p);
    }
  }
  _build() {
    this.index = new Map(); const files = [];
    for (const d of this.dirs) this._scan(d, files);
    const cacheFile = path.join(os.homedir(), '.cache', 'osisub', 'fontindex.json');
    let known = {}; try { known = JSON.parse(fs.readFileSync(cacheFile, 'utf8')); } catch { /* first run */ }
    const next = {};
    for (const file of files) {
      let st; try { st = fs.statSync(file); } catch { continue; }
      let rec = known[file];
      if (!rec || rec.mtime !== st.mtimeMs || rec.size !== st.size) {
        rec = { mtime: st.mtimeMs, size: st.size, keys: [] };
        try {
          const buf = fs.readFileSync(file); const f = opentype.parse(buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength)); const n = f.names;
          const fam = (n.fontFamily?.en || Object.values(n.fontFamily || {})[0] || '').toLowerCase(), sub = (n.fontSubfamily?.en || '').toLowerCase(), full = (n.fullName?.en || '').toLowerCase();
          rec.keys = [...new Set([fam, full].filter(Boolean))]; rec.bold = /bold|black|heavy/.test(sub); rec.italic = /italic|oblique/.test(sub);
        } catch { /* unsupported (ttc, broken) */ }
      }
      next[file] = rec;
      for (const k of rec.keys) { if (!this.index.has(k)) this.index.set(k, []); this.index.get(k).push({ file, bold: !!rec.bold, italic: !!rec.italic }); }
    }
    try { fs.mkdirSync(path.dirname(cacheFile), { recursive: true }); fs.writeFileSync(cacheFile, JSON.stringify(next)); } catch { /* read-only home */ }
  }
  /** Resolve a style's font; falls back to a generic sans so metrics are still sensible. */
  resolve(name, bold, italic) {
    if (!this.index) this._build();
    const key = String(name || '').replace(/^@/, '').toLowerCase();
    let cands = this.index.get(key);
    if (!cands) for (const fb of ['arial', 'liberation sans', 'dejavu sans', 'inter']) if ((cands = this.index.get(fb))) break;
    if (!cands) return null;
    const score = r => Math.abs(+r.bold - +!!bold) + Math.abs(+r.italic - +!!italic);
    const pick = cands.reduce((m, r) => (score(r) < score(m) ? r : m), cands[0]);
    if (!this.cache.has(pick.file)) { const buf = fs.readFileSync(pick.file); this.cache.set(pick.file, opentype.parse(buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength))); }
    return this.cache.get(pick.file);
  }
  /** [width, height, descent, external leading] like Aegisub (GDI-style metrics, scaled by font size). */
  extents(style, text) {
    const font = this.resolve(style.fontname, style.bold, style.italic);
    const size = style.fontsize || 48; const sx = (style.scale_x ?? 100) / 100, sy = (style.scale_y ?? 100) / 100;
    if (!font) { let w = 0; for (const ch of text) w += (ch.codePointAt(0) > 0x2E80 ? 1 : 0.5) * size; return [w * sx, size * sy, size * 0.2 * sy, 0]; }
    const upm = font.unitsPerEm, os2 = font.tables.os2 || {}, hhea = font.tables.hhea || {};
    const asc = os2.usWinAscent ?? hhea.ascender ?? font.ascender, desc = os2.usWinDescent ?? -(hhea.descender ?? font.descender);
    // GDI "font height" is the cell height scaled so that cell height == fontsize
    const scale = size / (asc + desc);
    let w = 0; const glyphs = font.stringToGlyphs(text);
    glyphs.forEach((g, i) => { w += (g.advanceWidth ?? upm / 2) * scale; if (i) w += font.getKerningValue(glyphs[i - 1], g) * scale; w += style.spacing || 0; });
    const lineGap = hhea.lineGap || 0;
    return [w * sx, (asc + desc) * scale * sy, desc * scale * sy, Math.max(0, lineGap * scale * sy)];
  }
}
let shared; export const fontManager = () => (shared ??= new FontManager([path.join(path.dirname(new URL(import.meta.url).pathname), '..', '..', 'fonts')]));
