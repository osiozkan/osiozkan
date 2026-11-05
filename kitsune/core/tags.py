"""ASS override-tag helpers."""
import re

_BLOCK = re.compile(r"\{[^}]*\}")

def strip_tags(text: str) -> str:
    return _BLOCK.sub("", text).replace("\\N", "\n").replace("\\n", " ").replace("\\h", "\u00a0")

def split_blocks(text: str):
    """Yield (is_tag_block, content) pairs."""
    pos = 0
    for m in _BLOCK.finditer(text):
        if m.start() > pos:
            yield False, text[pos:m.start()]
        yield True, m.group(0)
        pos = m.end()
    if pos < len(text):
        yield False, text[pos:]

def visible_length(text: str) -> int:
    return len(_BLOCK.sub("", text).replace("\\N", "").replace("\\n", "").replace("\\h", " "))

def cps(text: str, duration_ms: int) -> float:
    return 0.0 if duration_ms <= 0 else visible_length(text) / (duration_ms / 1000.0)

def parse_color(s: str):
    """&HBBGGRR& / &HAABBGGRR -> (r,g,b,a)"""
    h = re.sub(r"[^0-9A-Fa-f]", "", s.replace("&H", "").replace("&h", ""))
    h = h.rjust(8, "0")[-8:]
    a, b, g, r = (int(h[i:i+2], 16) for i in (0, 2, 4, 6))
    return r, g, b, a

def fmt_color(r, g, b, a=0, alpha=True) -> str:
    return f"&H{a:02X}{b:02X}{g:02X}{r:02X}" if alpha else f"&H{b:02X}{g:02X}{r:02X}&"

_KARA = re.compile(r"\\(k|K|kf|ko)(\d+(?:\.\d+)?)")

def karaoke_syllables(text: str):
    """Return list of (duration_cs, tag, syllable_text) for a karaoke line."""
    out, cur = [], None
    for is_tag, c in split_blocks(text):
        if is_tag:
            m = _KARA.search(c)
            if m:
                if cur: out.append(cur)
                cur = [int(float(m.group(2))), m.group(1), ""]
        elif cur is not None:
            cur[2] += c
        else:
            cur = [0, "k", c]
    if cur: out.append(cur)
    return [tuple(x) for x in out]
