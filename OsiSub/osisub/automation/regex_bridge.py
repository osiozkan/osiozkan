"""aegisub.re için Python tarafı: boost::regex (Perl) davranışını taklit eder.

Lua'dan gelen tüm konumlar UTF-8 BAYT konumudur; Python düzenli ifadeleri
karakter üzerinde çalıştığı için dönüşüm burada yapılır.
"""

from __future__ import annotations

try:  # `regex` modülü Perl'e daha yakın (\p{L}, iyelikli niceleyiciler ...)
    import regex as _re
    _HAVE_REGEX = True
except ImportError:  # pragma: no cover
    import re as _re
    _HAVE_REGEX = False

ICASE, NOSUB, COLLATE, NEWLINE_ALT = 1, 2, 4, 8
NO_MOD_M, NO_MOD_S, MOD_X, NO_EMPTY_SUBEXPRESSIONS = 16, 32, 64, 128


def _dec(b) -> str:
    if isinstance(b, bytes):
        return b.decode("utf-8", errors="surrogateescape")
    return str(b)


def _enc(s: str) -> bytes:
    return s.encode("utf-8", errors="surrogateescape")


class _Text:
    """Bayt <-> karakter konum dönüşümü (ASCII ise birebir)."""

    __slots__ = ("raw", "text", "ascii", "_cum")

    def __init__(self, raw: bytes):
        self.raw = raw
        self.text = _dec(raw)
        self.ascii = len(self.text) == len(raw)
        self._cum = None

    def _build(self):
        cum = [0]
        total = 0
        for ch in self.text:
            total += len(_enc(ch))
            cum.append(total)
        self._cum = cum

    def b2c(self, bpos: int) -> int:
        if self.ascii:
            return bpos
        if self._cum is None:
            self._build()
        import bisect
        return bisect.bisect_left(self._cum, bpos)

    def c2b(self, cpos: int) -> int:
        if self.ascii:
            return cpos
        if self._cum is None:
            self._build()
        return self._cum[cpos]


class RegexBridge:
    def __init__(self):
        self._compiled: list = []
        self._last: _Text | None = None

    def _text(self, raw: bytes) -> _Text:
        if self._last is None or self._last.raw is not raw and self._last.raw != raw:
            self._last = _Text(raw)
        return self._last

    def compile(self, pattern, flags=0):
        pat = _dec(pattern)
        f = _re.UNICODE
        if flags & ICASE:
            f |= _re.IGNORECASE
        if not flags & NO_MOD_M:
            f |= _re.MULTILINE
        if not flags & NO_MOD_S:
            f |= _re.DOTALL
        if flags & MOD_X:
            f |= _re.VERBOSE
        try:
            rx = _re.compile(pat, f)
        except Exception as e:  # noqa: BLE001 — hata metni Lua'ya döner
            return None, _enc(f"Düzenli ifade hatası: {e}")
        self._compiled.append(rx)
        return len(self._compiled) - 1, None

    # Arama alt metin üzerinde yapılır: boost, str+start işaretçisinden
    # başlar ve '^' o noktada eşleşir.
    def search(self, h, raw, start):
        t = self._text(raw)
        start = int(start)
        cstart = t.b2c(start)
        sub = t.text[cstart:]
        m = self._compiled[int(h)].search(sub)
        if not m:
            return None
        bfirst = t.c2b(cstart + m.start())
        blast = t.c2b(cstart + m.end())
        return bfirst + 1, blast

    def match(self, h, raw, start):
        t = self._text(raw)
        start = int(start)
        cstart = t.b2c(start)
        sub = t.text[cstart:]
        m = self._compiled[int(h)].search(sub)
        if not m:
            return None
        out = []
        for gi in range(0, (m.re.groups if hasattr(m, "re") else 0) + 1):
            if m.start(gi) < 0:
                break           # Aegisub: ilk eşleşmeyen grupta durur
            bf = t.c2b(cstart + m.start(gi))
            bl = t.c2b(cstart + m.end(gi))
            out.append((bf + 1, bl))
        return out

    def replace(self, h, repl, raw, max_count):
        t = self._text(raw)
        rx = self._compiled[int(h)]
        fn = _perl_format(_dec(repl))
        count = int(max_count)
        if count < 0:
            count = 0
        return _enc(rx.sub(fn, t.text, count=count))


def _perl_format(fmt: str):
    """boost format_perl dizgisini m -> str işlevine çevirir."""
    parts: list = []          # str ya da ("g", n) / ("pre",) / ("post",)
    i, n = 0, len(fmt)
    buf = []

    def flush():
        if buf:
            parts.append("".join(buf))
            buf.clear()

    escapes = {"n": "\n", "t": "\t", "r": "\r", "a": "\a", "f": "\f", "v": "\v", "e": "\x1b"}
    while i < n:
        c = fmt[i]
        if c == "$" and i + 1 < n:
            d = fmt[i + 1]
            if d == "$":
                buf.append("$"); i += 2; continue
            if d == "&":
                flush(); parts.append(("g", 0)); i += 2; continue
            if d == "`":
                flush(); parts.append(("pre",)); i += 2; continue
            if d == "'":
                flush(); parts.append(("post",)); i += 2; continue
            if d == "{":
                j = fmt.find("}", i + 2)
                if j > 0 and fmt[i + 2:j].isdigit():
                    flush(); parts.append(("g", int(fmt[i + 2:j]))); i = j + 1; continue
            if d.isdigit():
                j = i + 1
                while j < n and fmt[j].isdigit():
                    j += 1
                flush(); parts.append(("g", int(fmt[i + 1:j]))); i = j; continue
        if c == "\\" and i + 1 < n:
            d = fmt[i + 1]
            if d.isdigit():
                flush(); parts.append(("g", int(d))); i += 2; continue
            buf.append(escapes.get(d, d)); i += 2; continue
        buf.append(c)
        i += 1
    flush()

    def fn(m):
        out = []
        for p in parts:
            if isinstance(p, str):
                out.append(p)
            elif p[0] == "g":
                try:
                    out.append(m.group(p[1]) or "")
                except (IndexError, _re.error):
                    pass
            elif p[0] == "pre":
                out.append(m.string[:m.start()])
            else:
                out.append(m.string[m.end():])
        return "".join(out)

    return fn
