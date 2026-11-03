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
