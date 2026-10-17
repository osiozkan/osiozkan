"""Çoklu değiştir (Subtitle Edit "Multiple replace" karşılığı).

Kural grupları sırayla, her grubun kuralları da sırayla uygulanır. Kural türleri:
  plain  düz metin
  word   bütün kelime (Türkçe harfler kelime sayılır; "Naruto" → "Naruto'yu" içinde de eşleşir)
  regex  düzenli ifade (değiştirmede \\1, \\g<ad> kullanılabilir)

Büyük/küçük harfe duyarsız düz/kelime kuralları Türkçe doğru eşleşir: i↔İ ve ı↔I (Python'un
IGNORECASE'i i'yi ı ile de eşleştirir — "kır"/"kir" karışmasın diye kullanılmaz). Bu kurallarda
akıllı harf: eşleşme büyük harfle başlıyorsa yerine geçen de büyük harfle başlar ("Herşey" →
"Her şey"), tamamı büyükse tamamı büyük olur.

Değiştirme YALNIZ görünen metne uygulanır; {…} etiketleri ve \\p çizimleri korunur.
"""

from __future__ import annotations

import json
import re
import xml.etree.ElementTree as ET
from dataclasses import asdict, dataclass, field

from .qc import map_plain, tr_lower, tr_upper

MODES = {"plain": "Düz metin", "word": "Bütün kelime", "regex": "Düzenli ifade"}


@dataclass
class Rule:
    find: str
    replace: str = ""
    mode: str = "word"
    case: bool = False               # büyük/küçük harf duyarlı
    enabled: bool = True
    note: str = ""


@dataclass
class Group:
    name: str
    rules: list = field(default_factory=list)
    enabled: bool = True


def _ci_class(ch: str) -> str:
    """Türkçe doğru büyük/küçük harf duyarsız karakter sınıfı."""
    low, up = tr_lower(ch), tr_upper(ch)
    vs = sorted({ch, low, up} - {""}, key=len)
    vs = [v for v in vs if len(v) == 1]
    if len(vs) <= 1:
        return re.escape(ch)
    return "[" + "".join(re.escape(v) for v in vs) + "]"


def compile_rule(r: Rule):
    if r.mode == "regex":
        return re.compile(r.find, 0 if r.case else re.IGNORECASE)
    body = re.escape(r.find) if r.case else "".join(_ci_class(c) for c in r.find)
    if r.mode == "word":
        body = rf"(?<![^\W_]){body}(?![^\W_])"
    return re.compile(body)


def _smart(match_text: str, repl: str) -> str:
    letters = [c for c in match_text if c.isalpha()]
    if not letters or not repl:
        return repl
    if len(letters) > 1 and all(c == tr_upper(c) for c in letters):
        return tr_upper(repl)
    if letters[0] == tr_upper(letters[0]) and letters[0] != tr_lower(letters[0]):
        return tr_upper(repl[0]) + repl[1:]
    return repl


@dataclass
class Compiled:
    rule: Rule
    rx: re.Pattern
    group: str


def compile_groups(groups: list[Group]) -> tuple[list[Compiled], list[str]]:
    out, errors = [], []
    for g in groups:
        if not g.enabled:
            continue
        for r in g.rules:
            if not r.enabled or not r.find:
                continue
            try:
                out.append(Compiled(r, compile_rule(r), g.name))
            except re.error as e:
                errors.append(f"{g.name}: '{r.find}' — {e}")
    return out, errors


def _ass_template(repl: str) -> str:
    """Regex değiştirmesinde ASS kaçışları (\\N, \\n, \\h) olduğu gibi yazılsın (re 'bad escape' vermesin)."""
    return re.sub(r"(?<!\\)\\([Nnh])", r"\\\\\1", repl)


def apply_text(text: str, rules: list[Compiled]) -> tuple[str, list[str]]:
    """Kuralları görünen metne sırayla uygular. Döndürür: (yeni metin, isabet eden kural notları)."""
    if re.search(r"\\p[1-9]", text):
        return text, []
    hits: list[str] = []
    cur = text
    for c in rules:
        r = c.rule

        def sub(p, _f, _l, c=c, r=r):
            if r.mode == "regex":
                new, n = c.rx.subn(_ass_template(r.replace), p)
            elif r.case:
                new, n = c.rx.subn(lambda m: r.replace, p)
            else:
                new, n = c.rx.subn(lambda m: _smart(m.group(0), r.replace), p)
            if n:
                hits.append(r.note or f"{r.find} → {r.replace}")
            return new
        try:
            cur = map_plain(cur, sub)
        except (re.error, IndexError) as e:          # regex değiştirmesinde geçersiz grup başvurusu
            hits.append(f"HATA {r.find}: {e}")
    return cur, hits


def apply_events(events, rows, groups: list[Group]) -> tuple[dict, dict, list[str]]:
    """Döndürür: ({satır: {"text": yeni}}, {satır: [kurallar]}, derleme hataları)."""
    rules, errors = compile_groups(groups)
    changes, why = {}, {}
    if not rules:
        return changes, why, errors
    for r in rows:
        ev = events[r]
        if ev.comment:
            continue
        new, hits = apply_text(ev.text, rules)
        if new != ev.text:
            changes[r] = {"text": new}
            why[r] = hits
    return changes, why, errors


# ---------------------------------------------------------------------------
# Saklama: config.json içinde sözlük, dışa/içe aktarma JSON; Subtitle Edit XML'i içe aktarma
# ---------------------------------------------------------------------------

def to_data(groups: list[Group]) -> list[dict]:
    return [{"name": g.name, "enabled": g.enabled, "rules": [asdict(r) for r in g.rules]} for g in groups]


def from_data(data) -> list[Group]:
    out = []
    for g in data or []:
        rules = []
        for r in g.get("rules", []):
            rules.append(Rule(find=str(r.get("find", "")), replace=str(r.get("replace", "")),
                              mode=r.get("mode", "word") if r.get("mode") in MODES else "word",
                              case=bool(r.get("case", False)), enabled=bool(r.get("enabled", True)),
                              note=str(r.get("note", ""))))
        out.append(Group(name=str(g.get("name", "Grup")), rules=rules, enabled=bool(g.get("enabled", True))))
    return out


def save_json(path: str, groups: list[Group]):
    with open(path, "w", encoding="utf-8") as f:
        json.dump({"osisub_multi_replace": 1, "groups": to_data(groups)}, f, ensure_ascii=False, indent=2)


def load_file(path: str) -> list[Group]:
    """OsiSub JSON'u ya da Subtitle Edit'in çoklu değiştir XML dışa aktarımı."""
    with open(path, "rb") as f:
        raw = f.read()
    head = raw.lstrip(b"\xef\xbb\xbf \r\n\t")[:1]
    if head == b"<":
        return _load_se_xml(raw)
    data = json.loads(raw.decode("utf-8-sig"))
    return from_data(data.get("groups", data) if isinstance(data, dict) else data)


def _load_se_xml(raw: bytes) -> list[Group]:
    """SE biçimine hoşgörülü okuma: FindWhat/ReplaceWith/SearchType/Enabled/Description öğeleri
    (gruplar varsa Group/Name altında)."""
    root = ET.fromstring(raw)

    def txt(el, name, default=""):
        x = el.find(name)
        return x.text if x is not None and x.text is not None else default

    def rule_of(el):
        st = txt(el, "SearchType", "Normal").lower()
        mode = "regex" if "regular" in st else "plain"
        return Rule(find=txt(el, "FindWhat"), replace=txt(el, "ReplaceWith"), mode=mode,
                    case="casesensitive" in st, enabled=txt(el, "Enabled", "True").lower() != "false",
                    note=txt(el, "Description"))
    groups = []
    for gel in root.iter("Group"):
        items = [rule_of(it) for it in gel.iter() if it.find("FindWhat") is not None]
        groups.append(Group(name=txt(gel, "Name", "Subtitle Edit"), rules=items,
                            enabled=txt(gel, "Enabled", "True").lower() != "false"))
    if not groups:
        items = [rule_of(it) for it in root.iter() if it.find("FindWhat") is not None]
        if items:
            groups.append(Group(name="Subtitle Edit", rules=items))
    return groups


def default_groups() -> list[Group]:
    """Başlangıç kuralları: fansub çevirilerinde sık görülen Türkçe yazım hataları."""
    pairs = [("birşey", "bir şey"), ("herşey", "her şey"), ("hiçbirşey", "hiçbir şey"), ("yada", "ya da"),
             ("şuan", "şu an"), ("malesef", "maalesef"), ("herkez", "herkes"), ("yanlız", "yalnız"),
             ("yalnış", "yanlış"), ("yalnışlık", "yanlışlık"), ("orjinal", "orijinal"), ("tabiki", "tabii ki"),
             ("tabi ki", "tabii ki"), ("pekçok", "pek çok"), ("hergün", "her gün"), ("herzaman", "her zaman"),
             ("klavuz", "kılavuz"), ("süpriz", "sürpriz"), ("eşortman", "eşofman"), ("birsürü", "bir sürü")]
    rules = [Rule(a, b, "word") for a, b in pairs]
    return [Group("Türkçe sık yazım hataları", rules, True)]


__all__ = ["Rule", "Group", "MODES", "compile_rule", "compile_groups", "apply_text", "apply_events",
           "to_data", "from_data", "save_json", "load_file", "default_groups"]
