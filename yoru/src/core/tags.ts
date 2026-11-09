/** ASS override tag ayrıştırma ve temizleme. */

export interface Block { kind: "tags" | "text" | "drawing"; raw: string }

/** Metni {…} blokları ve düz metin parçaları olarak böler. */
export function splitBlocks(text: string): Block[] {
  const out: Block[] = [];
  let i = 0;
  while (i < text.length) {
    if (text[i] === "{") {
      const j = text.indexOf("}", i);
      if (j < 0) { out.push({ kind: "text", raw: text.slice(i) }); break; }
      out.push({ kind: "tags", raw: text.slice(i, j + 1) });
      i = j + 1;
    } else {
      let j = text.indexOf("{", i);
      if (j < 0) j = text.length;
      out.push({ kind: "text", raw: text.slice(i, j) });
      i = j;
    }
  }
  return out;
}

export interface Tag { name: string; arg: string }

const TAG_NAMES = [
  "fscx","fscy","fsp","fnx","fad","fade","frx","fry","frz","fax","fay","fn","fs","fr","clip","iclip","move","pos","org","bord","xbord","ybord","shad","xshad","yshad","blur","be","1c","2c","3c","4c","1a","2a","3a","4a","alpha","an","a","b","i","u","s","c","kf","ko","k","K","q","r","t","p","fe",
];

/** "{\an8\pos(1,2)}" içeriğini etiket listesine çevirir; ayrıştırılamayan kısım yorum sayılır ("" adlı etiket). */
export function parseTags(block: string): Tag[] {
  const inner = block.replace(/^\{/, "").replace(/\}$/, "");
  const tags: Tag[] = [];
  let i = 0;
  let comment = "";
  const flush = () => { if (comment) { tags.push({ name: "", arg: comment }); comment = ""; } };
  while (i < inner.length) {
    if (inner[i] !== "\\") { comment += inner[i++]; continue; }
    const rest = inner.slice(i + 1);
    const name = TAG_NAMES.find((t) => rest.startsWith(t));
    if (!name) { comment += inner[i++]; continue; }
    flush();
    let j = i + 1 + name.length;
    let arg = "";
    if (name === "t" && inner[j] === "(") {
      // iç içe parantezler
      let depth = 0; let k = j;
      for (; k < inner.length; k++) { if (inner[k] === "(") depth++; else if (inner[k] === ")") { depth--; if (depth === 0) { k++; break; } } }
      arg = inner.slice(j, k); j = k;
    } else if (inner[j] === "(") {
      const k = inner.indexOf(")", j);
      const e = k < 0 ? inner.length : k + 1;
      arg = inner.slice(j, e); j = e;
    } else {
      let k = j;
      while (k < inner.length && inner[k] !== "\\") k++;
      arg = inner.slice(j, k); j = k;
    }
    tags.push({ name, arg });
    i = j;
  }
  flush();
  return tags;
}

export function buildTags(tags: Tag[]): string {
  return "{" + tags.map((t) => (t.name ? "\\" + t.name + t.arg : t.arg)).join("") + "}";
}

/** Etiketleri/yorumları ve \N,\n,\h kaçışlarını temizler → düz metin. */
export function stripTags(text: string, keepBreaks = false): string {
  let s = text.replace(/\{[^}]*\}/g, "");
  s = keepBreaks ? s.replace(/\\n/g, "\\N") : s.replace(/\\[Nn]/g, " ");
  return s.replace(/\\h/g, " ");
}

/** Görünür karakter sayısı (etiketsiz, \N hariç). */
export function visibleLength(text: string): number {
  return stripTags(text, true).replace(/\\N/g, "").length;
}

export interface KaraSyl { duration: number; kind: string; text: string }

/** Karaoke hecelerine ayırır (duration ms). */
export function parseKaraoke(text: string): KaraSyl[] {
  const out: KaraSyl[] = [];
  let cur: KaraSyl | null = null;
  for (const b of splitBlocks(text)) {
    if (b.kind === "tags") {
      let isK = false;
      for (const t of parseTags(b.raw)) {
        if (["k", "K", "kf", "ko"].includes(t.name)) {
          if (cur) out.push(cur);
          cur = { duration: Math.round(parseFloat(t.arg) * 10) || 0, kind: t.name === "K" ? "kf" : t.name, text: "" };
          isK = true;
        }
      }
      if (!isK && !cur) cur = { duration: 0, kind: "k", text: "" };
    } else {
      if (!cur) cur = { duration: 0, kind: "k", text: "" };
      cur.text += b.raw;
    }
  }
  if (cur) out.push(cur);
  return out;
}

export function buildKaraoke(syls: KaraSyl[]): string {
  return syls.map((s) => `{\\${s.kind}${Math.round(s.duration / 10)}}${s.text}`).join("");
}

/** Karaoke sürelerini verilen toplam süreye ölçekler. */
export function fitKaraoke(text: string, totalMs: number): string {
  const syls = parseKaraoke(text);
  const sum = syls.reduce((a, s) => a + s.duration, 0) || 1;
  let acc = 0;
  syls.forEach((s, i) => {
    if (i === syls.length - 1) s.duration = Math.max(0, totalMs - acc);
    else { s.duration = Math.round((s.duration * totalMs) / sum / 10) * 10; acc += s.duration; }
  });
  return buildKaraoke(syls);
}

/** Etiketleri koruyarak yalnızca görünür metinde değiştir (düz metin üzerinde regex). */
export function replaceInVisibleText(text: string, re: RegExp, repl: string | ((m: string, ...a: any[]) => string)): string {
  return splitBlocks(text).map((b) => (b.kind === "text" ? b.raw.replace(re, repl as any) : b.raw)).join("");
}
