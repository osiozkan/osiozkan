"""Override tag helpers."""
from __future__ import annotations
import re

BLOCK = re.compile(r"\{[^}]*\}")


def strip_tags(text: str) -> str:
    t = BLOCK.sub("", text)
    return t.replace("\\N", "\n").replace("\\n", " ").replace("\\h", "\u00a0")


def visible_text(text: str) -> str:
    return strip_tags(text)


def split_blocks(text: str) -> list[tuple[bool, str]]:
    """[(is_tag_block, content)] preserving order."""
    out, pos = [], 0
    for m in BLOCK.finditer(text):
        if m.start() > pos:
            out.append((False, text[pos:m.start()]))
        out.append((True, m.group(0)))
        pos = m.end()
    if pos < len(text):
        out.append((False, text[pos:]))
    return out


def srt_to_ass_text(t: str) -> str:
    t = t.replace("\r", "")
    t = re.sub(r"<\s*(/?)\s*([biu])\s*>", lambda m: "{\\%s%d}" % (m.group(2), 0 if m.group(1) else 1), t, flags=re.I)
    t = re.sub(r"<\s*font\s+color\s*=\s*[\"']?#?([0-9a-fA-F]{6})[\"']?\s*>",
               lambda m: "{\\c&H%s%s%s&}" % (m.group(1)[4:6], m.group(1)[2:4], m.group(1)[0:2]), t, flags=re.I)
    t = re.sub(r"<\s*/\s*font\s*>", "{\\c}", t, flags=re.I)
    t = re.sub(r"</?[a-zA-Z][^>]*>", "", t)
    t = re.sub(r"\{\\an(\d)\}", r"{\\an\1}", t)
    return t.replace("\n", "\\N")


def ass_to_srt_text(t: str) -> str:
    out, state = [], {"i": False, "b": False, "u": False}
    for is_tag, c in split_blocks(t):
        if not is_tag:
            out.append(c.replace("\\N", "\n").replace("\\n", " ").replace("\\h", " "))
            continue
        for m in re.finditer(r"\\([biu])(\d)", c):
            k, v = m.group(1), m.group(2) == "1"
            if v != state[k]:
                out.append(f"<{'' if v else '/'}{k}>")
                state[k] = v
    for k, v in state.items():
        if v:
            out.append(f"</{k}>")
    return "".join(out)
