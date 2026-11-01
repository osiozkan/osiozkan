"""ASS override-tag helpers (tokenising, stripping, karaoke parsing)."""
from __future__ import annotations

import re
from dataclasses import dataclass

_BLOCK = re.compile(r"(\{[^}]*\})")
_TAG = re.compile(r"\\(?:"
                  r"(?P<fn>[1-4]?[a-zA-Z]+?)(?P<fargs>\([^)]*\))"          # \t(...) \pos(...) \move(...)
                  r"|(?P<name>fscx|fscy|fsp|fs|fn|frx|fry|frz|fr|fax|fay|fad|fade|"
                  r"bord|xbord|ybord|shad|xshad|yshad|be|blur|"
                  r"alpha|[1-4]a|[1-4]c|c|an|a|q|r|org|pos|move|clip|iclip|"
                  r"kf|ko|k|K|kt|"
                  r"b|i|u|s|p|pbo)(?P<arg>[^\\]*)"
                  r")")


def split_blocks(text: str) -> list[tuple[bool, str]]:
    """[(is_override_block, content)]; override blocks keep their braces."""
    out = []
    for part in _BLOCK.split(text):
        if part == "":
            continue
        out.append((part.startswith("{") and part.endswith("}"), part))
    return out


def strip_tags(text: str) -> str:
    return _BLOCK.sub("", text)


def plain_text(text: str) -> str:
    """Visible text with \\N / \\n -> newline, \\h -> space; drawings removed."""
    out = []
    drawing = False
    for is_tag, part in split_blocks(text):
        if is_tag:
            for m in re.finditer(r"\\p(\d+)", part):
                drawing = int(m.group(1)) > 0
            continue
        if drawing:
            continue
        out.append(part)
    s = "".join(out)
    return s.replace("\\N", "\n").replace("\\n", " ").replace("\\h", " ")


def char_count(text: str) -> int:
    return len(plain_text(text).replace("\n", ""))


def parse_block_tags(block: str) -> list[tuple[str, str]]:
    inner = block[1:-1] if block.startswith("{") else block
    res = []
    for m in _TAG.finditer(inner):
        name = m.group("name") or m.group("fn")
        arg = m.group("arg") if m.group("name") else m.group("fargs")
        res.append((name, arg or ""))
    return res


@dataclass
class Syllable:
    duration_cs: int
    text: str
    tag: str = "k"
    start_cs: int = 0
    prefix_tags: str = ""


def parse_karaoke(text: str) -> list[Syllable]:
    """Split text into karaoke syllables (durations in centiseconds)."""
    syls: list[Syllable] = []
    cur: Syllable | None = None
    pos = 0
    pending = ""
    for is_tag, part in split_blocks(text):
        if is_tag:
            ks = [(n, a) for n, a in parse_block_tags(part) if n in ("k", "K", "kf", "ko")]
            if ks:
                n, a = ks[0]
                cur = Syllable(int(float(a or 0)), "", "kf" if n == "K" else n, pos, pending)
                pending = ""
                pos += cur.duration_cs
                syls.append(cur)
                rest = re.sub(r"\\(?:kf|ko|k|K)\d+(\.\d+)?", "", part)
                if rest != "{}":
                    cur.prefix_tags = rest
            elif cur is not None:
                cur.text += part
            else:
                pending += part
        else:
            if cur is None:
                cur = Syllable(0, "", "k", 0, pending)
                pending = ""
                syls.append(cur)
            cur.text += part
    return syls


def has_override(text: str, tag: str) -> bool:
    return bool(re.search(r"\{[^}]*\\" + re.escape(tag), text))


def set_override(text: str, tag: str, value: str) -> str:
    """Set/replace an override tag (e.g. 'pos', '(100,200)') in the first {} block of `text`.

    The tag is removed from every leading block position it already exists in (outside \\t(...)),
    then inserted at the start of the first block (creating one if the text has none at offset 0)."""
    m = re.match(r"\{([^}]*)\}", text)
    new = "\\" + tag + value
    if not m:
        return "{" + new + "}" + text
    inner = m.group(1)
    # strip existing occurrence at top level (not inside parentheses)
    out, depth, i = [], 0, 0
    parts = re.split(r"(?=\\)", inner)
    keep = []
    depth = 0
    for p in parts:
        if depth == 0 and re.match(r"\\" + re.escape(tag) + r"(?![a-zA-Z])", p):
            # drop it unless it opened a paren group we must balance
            depth += p.count("(") - p.count(")")
            continue
        depth += p.count("(") - p.count(")")
        keep.append(p)
    inner = "".join(keep)
    return "{" + new + inner + "}" + text[m.end():]


def get_override(text: str, tag: str):
    m = re.match(r"\{([^}]*)\}", text)
    if not m:
        return None
    mm = re.search(r"\\" + re.escape(tag) + r"(?![a-zA-Z])(\([^)]*\)|[^\\}]*)", m.group(1))
    return mm.group(1) if mm else None
