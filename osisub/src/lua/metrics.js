// Approximate text measurement for aegisub.text_extents / karaskel when no real font engine is available.
// Widths are em fractions for a typical sans face; UIs may inject real canvas/FreeType metrics instead.

const NARROW = new Set("iljI.,:;'|!`".split(""));
const WIDE = new Set("mwMW@%".split(""));

function isWide(cp) {
  return (cp >= 0x1100 && cp <= 0x11ff) || (cp >= 0x2e80 && cp <= 0xa4cf) || (cp >= 0xac00 && cp <= 0xd7a3) ||
    (cp >= 0xf900 && cp <= 0xfaff) || (cp >= 0xfe30 && cp <= 0xfe6f) || (cp >= 0xff00 && cp <= 0xff60) || (cp >= 0x20000 && cp <= 0x3ffff);
}

export function measureText({ size, bold, scaleX = 100, scaleY = 100, spacing = 0, text }) {
  let w = 0;
  for (const ch of text) {
    const cp = ch.codePointAt(0);
    let em;
    if (isWide(cp)) em = 1;
    else if (ch === " ") em = 0.28;
    else if (NARROW.has(ch)) em = 0.26;
    else if (WIDE.has(ch)) em = 0.84;
    else if (ch >= "A" && ch <= "Z") em = 0.67;
    else if (ch >= "0" && ch <= "9") em = 0.556;
    else em = 0.5;
    if (bold) em *= 1.05;
    w += em * size + spacing;
  }
  return {
    width: (w * scaleX) / 100,
    height: (size * 1.15 * scaleY) / 100,
    descent: (size * 0.21 * scaleY) / 100,
    extlead: 0,
  };
}
