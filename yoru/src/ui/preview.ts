import { AssFile, Event, Style } from "../core/ass";
import { parseTags, splitBlocks } from "../core/tags";

/** &HAABBGGRR -> css rgba */
export function assColorToCss(c: string, alphaOverride?: number): string {
  const m = /^&?H?([0-9a-f]{2})?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})&?$/i.exec(c.replace(/^&H/i, "&H"));
  const h = c.replace(/[&H]/gi, "").padStart(8, "0");
  void m;
  const a = alphaOverride ?? parseInt(h.slice(0, 2), 16);
  const b = parseInt(h.slice(2, 4), 16), g = parseInt(h.slice(4, 6), 16), r = parseInt(h.slice(6, 8), 16);
  return `rgba(${r},${g},${b},${(1 - a / 255).toFixed(3)})`;
}

interface Run { text: string; italic: boolean; bold: boolean; underline: boolean; strike: boolean; color?: string; size?: number; font?: string; }

/** Hafif DOM önizleme: stil, \an, \pos, \i \b \u \s \c \fs \fn, \N. (Gerçek libass işleme sonraki sürümde.) */
export function renderPreview(host: HTMLElement, f: AssFile, evs: Event[], resX: number, resY: number) {
  host.innerHTML = "";
  const scale = host.clientHeight / resY;
  for (const e of evs) {
    const style: Style = f.styles.find((s) => s.name === e.style) ?? f.styles[0];
    if (!style) continue;
    let an = style.alignment; let pos: [number, number] | null = null;
    const runs: Run[] = [];
    let cur: Run = { text: "", italic: style.italic, bold: style.bold, underline: style.underline, strike: style.strikeout };
    for (const b of splitBlocks(e.text)) {
      if (b.kind === "tags") {
        const next = { ...cur, text: "" };
        for (const t of parseTags(b.raw)) {
          if (t.name === "an") an = +t.arg || an;
          else if (t.name === "a") { const v = +t.arg; an = ({ 1: 1, 2: 2, 3: 3, 5: 7, 6: 8, 7: 9, 9: 4, 10: 5, 11: 6 } as any)[v] ?? an; }
          else if (t.name === "pos") { const m = /\(([-\d.]+),([-\d.]+)\)/.exec(t.arg); if (m) pos = [+m[1], +m[2]]; }
          else if (t.name === "i") next.italic = t.arg === "1";
          else if (t.name === "b") next.bold = t.arg !== "0";
          else if (t.name === "u") next.underline = t.arg === "1";
          else if (t.name === "s") next.strike = t.arg === "1";
          else if (t.name === "c" || t.name === "1c") next.color = t.arg ? assColorToCss("&H00" + t.arg.replace(/[&H]/gi, "").padStart(6, "0")) : undefined;
          else if (t.name === "fs") next.size = parseFloat(t.arg) || undefined;
          else if (t.name === "fn") next.font = t.arg;
          else if (t.name === "r") { next.italic = style.italic; next.bold = style.bold; next.color = undefined; next.size = undefined; next.font = undefined; }
        }
        if (cur.text) runs.push(cur);
        cur = next;
      } else {
        const parts = b.raw.split(/\\N|\\n/);
        parts.forEach((p, k) => {
          if (k > 0) { if (cur.text) runs.push(cur); runs.push({ ...cur, text: "\n" }); cur = { ...cur, text: "" }; }
          cur.text += p.replace(/\\h/g, " ");
        });
      }
    }
    if (cur.text) runs.push(cur);
    const box = document.createElement("div");
    box.className = "pv-line";
    const ml = e.marginL || style.marginL, mr = e.marginR || style.marginR, mv = e.marginV || style.marginV;
    const hAl = (an - 1) % 3, vAl = Math.floor((an - 1) / 3);
    box.style.position = "absolute";
    box.style.textAlign = ["left", "center", "right"][hAl];
    if (pos) {
      box.style.left = pos[0] * scale + "px"; box.style.top = pos[1] * scale + "px";
      box.style.transform = `translate(${["0", "-50%", "-100%"][hAl]}, ${["-100%", "-50%", "0"][vAl]})`;
    } else {
      box.style.left = ml * scale + "px"; box.style.right = mr * scale + "px";
      if (vAl === 0) box.style.bottom = mv * scale + "px";
      else if (vAl === 2) box.style.top = mv * scale + "px";
      else { box.style.top = "50%"; box.style.transform = "translateY(-50%)"; }
    }
    const outline = assColorToCss(style.outline); const ow = style.outlineW * scale;
    const back = assColorToCss(style.back);
    for (const r of runs) {
      if (r.text === "\n") { box.appendChild(document.createElement("br")); continue; }
      const sp = document.createElement("span");
      sp.textContent = r.text;
      sp.style.fontFamily = `"${r.font ?? style.fontname}", sans-serif`;
      sp.style.fontSize = (r.size ?? style.fontsize) * scale + "px";
      sp.style.fontWeight = r.bold ? "700" : "400"; sp.style.fontStyle = r.italic ? "italic" : "normal";
      sp.style.textDecoration = [r.underline ? "underline" : "", r.strike ? "line-through" : ""].join(" ").trim() || "none";
      sp.style.color = r.color ?? assColorToCss(style.primary);
      sp.style.letterSpacing = style.spacing * scale + "px";
      const sh = style.shadow * scale;
      const strokes = ow > 0 ? [[-ow, -ow], [ow, -ow], [-ow, ow], [ow, ow], [0, -ow], [0, ow], [-ow, 0], [ow, 0]].map(([x, y]) => `${x}px ${y}px 0 ${outline}`) : [];
      if (sh > 0) strokes.push(`${sh}px ${sh}px 0 ${back}`);
      sp.style.textShadow = strokes.join(",");
      box.appendChild(sp);
    }
    host.appendChild(box);
  }
}
