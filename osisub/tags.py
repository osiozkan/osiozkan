"""Override-tag helpers."""
import re

_BLOCK = re.compile(r"\{[^}]*\}")


def strip_tags(text: str) -> str:
    return _BLOCK.sub("", text)


def plain_text(text: str) -> str:
    """Visible text: tags removed, \\N/\\n -> newline, \\h -> space."""
    t = strip_tags(text)
    return t.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")


def split_blocks(text: str):
    """Yield (is_tag_block, content) pieces."""
    pos = 0
    for m in _BLOCK.finditer(text):
        if m.start() > pos:
            yield False, text[pos:m.start()]
        yield True, m.group(0)
        pos = m.end()
    if pos < len(text):
        yield False, text[pos:]


def has_karaoke(text: str) -> bool:
    return bool(re.search(r"\\(?:k|K|kf|ko)\d", text))


def leading_tag_value(text: str, tag: str):
    m = re.match(r"\{[^}]*\\" + re.escape(tag) + r"\(?([^\\})]*)", text)
    return m.group(1) if m else None
