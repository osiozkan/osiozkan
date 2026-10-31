"""Override etiket ayrıştırıcı ve karaoke yardımcıları."""
import re

BLOCK = re.compile(r"\{([^}]*)\}")


def split_blocks(text: str):
    """[(is_tag_block, content)] listesi."""
    out, pos = [], 0
    for m in BLOCK.finditer(text):
        if m.start() > pos:
            out.append((False, text[pos:m.start()]))
        out.append((True, m.group(1)))
        pos = m.end()
    if pos < len(text):
        out.append((False, text[pos:]))
    return out


def strip_tags(text: str) -> str:
    return BLOCK.sub("", text)


def unbalanced_braces(text: str) -> bool:
    depth = 0
    for ch in text:
        if ch == "{":
            depth += 1
            if depth > 1:
                return True
        elif ch == "}":
            depth -= 1
            if depth < 0:
                return True
    return depth != 0


KARA = re.compile(r"\\(k[fo]?|K)(\d+)")


def parse_karaoke(text: str):
    """[(tag, süre_cs, hece)] – {\\k..}hece biçimini çözer."""
    res = []
    for m in re.finditer(r"\{([^}]*)\}([^{]*)", text):
        k = KARA.search(m.group(1))
        if k:
            res.append((k.group(1), int(k.group(2)), m.group(2)))
    return res
