"""Override-tag helpers: stripping, visible text, character counting, splitting."""
from __future__ import annotations
import re

BLOCK = re.compile(r"\{[^}]*\}")
BREAKS = re.compile(r"\\[Nnh]")


def strip_tags(text: str) -> str:
    return BLOCK.sub("", text)


def visible_text(text: str, keep_breaks: bool = False) -> str:
    t = strip_tags(text)
    if not keep_breaks:
        t = t.replace("\\N", " ").replace("\\n", " ")
    t = t.replace("\\h", " ")
    return t


def split_blocks(text: str) -> list[tuple[bool, str]]:
    """[(is_tag_block, content)] sequence."""
    out, pos = [], 0
    for m in BLOCK.finditer(text):
        if m.start() > pos:
            out.append((False, text[pos:m.start()]))
        out.append((True, m.group(0)))
        pos = m.end()
    if pos < len(text):
        out.append((False, text[pos:]))
    return out


def char_count(text: str, ignore_spaces: bool = True) -> int:
    t = strip_tags(text)
    t = BREAKS.sub("" if ignore_spaces else " ", t)
    return len(t.replace(" ", "")) if ignore_spaces else len(t)


def has_drawing(text: str) -> bool:
    return bool(re.search(r"\\p[1-9]", text))


def lines(text: str) -> list[str]:
    return [p for p in re.split(r"\\N|\\n", strip_tags(text))]


_TAG_RE = {}


def get_tag(text: str, tag: str):
    """First value of simple override tag (e.g. 'fs', 'pos') in text, or None."""
    m = re.search(r"\\" + re.escape(tag) + r"(?=[\d\-&(])([^\\}]*)", text)
    return m.group(1) if m else None


def starts_with_italics(text: str) -> bool:
    return bool(re.match(r"^\{[^}]*\\i1", text))


# ------------------------------------------------------------------ tag editing
_FIRST_BLOCK = re.compile(r"^\{[^}]*\}")


def _tag_pattern(name: str) -> "re.Pattern":
    # \name followed by simple value or (args); name must not be a prefix of a longer tag (e.g. fs vs fscx)
    if name in ("pos", "org", "move", "clip", "iclip", "fad", "fade"):
        return re.compile(r"\\" + name + r"\([^)]*\)")
    return re.compile(r"\\" + name + r"(?![a-zA-Z])[^\\}]*")


def set_tag(text: str, name: str, value: str) -> str:
    """Set override tag in the first block (create the block if needed). value is the full text after \\name."""
    new = "\\" + name + value
    m = _FIRST_BLOCK.match(text)
    if m:
        blk = m.group(0)
        pat = _tag_pattern(name)
        if pat.search(blk):
            blk2 = pat.sub(lambda _: new, blk, count=1)
        else:
            blk2 = blk[:-1] + new + "}"
        return blk2 + text[m.end():]
    return "{" + new + "}" + text


def remove_tag(text: str, name: str) -> str:
    m = _FIRST_BLOCK.match(text)
    if not m:
        return text
    blk = _tag_pattern(name).sub("", m.group(0))
    return ("" if blk == "{}" else blk) + text[m.end():]


def get_first_tag(text: str, name: str):
    """Value (string after \\name) of the first occurrence in the first block, else None."""
    m = _FIRST_BLOCK.match(text)
    if not m:
        return None
    mm = _tag_pattern(name).search(m.group(0))
    return mm.group(0)[len(name) + 1:] if mm else None


def get_pos(text: str):
    v = get_first_tag(text, "pos")
    if v:
        try:
            a, b = v.strip("()").split(",")[:2]
            return float(a), float(b)
        except ValueError:
            return None
    return None


def default_position(style, res: tuple[int, int], ev_margins=(0, 0, 0), an: int | None = None) -> tuple[float, float]:
    """Where libass places the anchor for this style/alignment (used as drag start)."""
    w, h = res
    ml = ev_margins[0] or style.margin_l
    mr = ev_margins[1] or style.margin_r
    mv = ev_margins[2] or style.margin_t
    a = an or style.align
    col = (a - 1) % 3
    x = ml if col == 0 else (w - mr if col == 2 else (w + ml - mr) / 2)
    row = (a - 1) // 3  # 0 bottom, 1 middle, 2 top
    y = h - mv if row == 0 else (mv if row == 2 else h / 2)
    return x, y


def fmt_num(v: float) -> str:
    return ("%.2f" % v).rstrip("0").rstrip(".")
