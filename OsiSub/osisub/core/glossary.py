"""Terim sözlüğü: seriye özel terimlerin tutarlı çevrilmesi.

Dosya biçimi (UTF-8 TSV, ilk satır başlık, # ile başlayan satırlar yorum):
    kaynak<TAB>türkçe<TAB>yasaklar<TAB>not
    Nen<TAB>Nen<TAB>Nen gücü|nen<TAB>büyük harfle
    Stand<TAB>Stand<TAB>Stant|Ruh<TAB>
    Hokage<TAB>Hokage<TAB>Hokage-sama|Ateş Gölgesi<TAB>

Denetim (QC "glossary" kuralı):
  * Türkçe satırda YASAK bir varyant geçiyorsa uyarı; düzeltme önerisi tercih edilen
    terimle değiştirmektir (ek korunur: "Stantlar" -> "Standlar" DEĞİL, kök değişir ve ek
    aynen kalır: "Stant|lar" -> "Stand|lar"). Türkçe ses uyumu her zaman tutmayabileceği
    için bu düzeltme varsayılan olarak işaretsiz gelir.
  * Referans satırında (EN/JA) kaynak terim geçtiği halde Türkçe satırda tercih edilen
    terim (ekli biçimleri dahil) yoksa uyarı — düzeltme yok, bakılması gerekir.
Eşleşmeler büyük/küçük harfe duyarsızdır (Türkçe i/İ, ı/I doğru) ve kelime başından aranır,
böylece Türkçe ekli biçimler ("Hokage'nin", "Nen'i") yakalanır.
"""

from __future__ import annotations

import os
import re
from dataclasses import dataclass, field

HEADER = ["kaynak", "türkçe", "yasaklar", "not"]


def tr_fold(s: str) -> str:
    return s.replace("I", "ı").replace("İ", "i").lower()


@dataclass
class Term:
    source: str
    target: str
    forbidden: list[str] = field(default_factory=list)
    note: str = ""


class Glossary:
    def __init__(self, terms: list[Term] | None = None, path: str | None = None):
        self.terms = terms or []
        self.path = path

    # ------------------------------------------------------------ dosya
    @classmethod
    def load(cls, path: str) -> "Glossary":
        terms = []
        with open(path, encoding="utf-8-sig") as f:
            for n, line in enumerate(f):
                line = line.rstrip("\r\n")
                if not line.strip() or line.lstrip().startswith("#"):
                    continue
                cols = line.split("\t")
                if n == 0 and tr_fold(cols[0].strip()) in ("kaynak", "source"):
                    continue
                cols += [""] * (4 - len(cols))
                src, tgt, forb, note = (c.strip() for c in cols[:4])
                if not src and not tgt:
                    continue
                terms.append(Term(src, tgt, [x.strip() for x in forb.split("|") if x.strip()], note))
        return cls(terms, path)

    def save(self, path: str | None = None):
        path = path or self.path
        if not path:
            raise ValueError("Kayıt yolu yok")
        lines = ["\t".join(HEADER)]
        for t in self.terms:
            lines.append("\t".join([t.source, t.target, "|".join(t.forbidden), t.note]))
        tmp = path + ".tmp"
        with open(tmp, "w", encoding="utf-8", newline="\n") as f:
            f.write("\n".join(lines) + "\n")
        os.replace(tmp, path)
        self.path = path

    # ------------------------------------------------------------ eşleşme
    @staticmethod
    def _pattern(term: str) -> re.Pattern:
        # kelime başında başlayan, devamında Türkçe ek olabilen eşleşme
        return re.compile(r"(?<![\w])" + re.escape(tr_fold(term)) + r"(?P<suffix>['’]?\w*)")

    def find_forbidden(self, text: str):
        """[(term, yasak, başlangıç, bitiş, ek)] — text içinde (katlanmış harflerle aranır)."""
        folded = tr_fold(text)
        out = []
        for t in self.terms:
            for bad in t.forbidden:
                for m in self._pattern(bad).finditer(folded):
                    out.append((t, bad, m.start(), m.end(), text[m.start() + len(bad):m.end()]))
        return out

    def missing_terms(self, ref_text: str, tr_text: str) -> list[Term]:
        rf, tf = tr_fold(ref_text), tr_fold(tr_text)
        out = []
        for t in self.terms:
            if not t.source or not t.target:
                continue
            if self._pattern(t.source).search(rf) and not self._pattern(t.target).search(tf):
                out.append(t)
        return out

    def fix_forbidden(self, text: str) -> str:
        """Yasak varyantları tercih edilen terimle değiştirir (ek korunur, baş harf büyüklüğü uyarlanır)."""
        hits = sorted(self.find_forbidden(text), key=lambda h: h[2], reverse=True)
        used = []
        for t, bad, a, b, suffix in hits:
            if any(not (b <= ua or a >= ub) for ua, ub in used):
                continue                       # çakışan eşleşme (uzun olan önce işlendi)
            orig = text[a:a + len(bad)]
            rep = t.target
            if orig[:1].isupper() and rep[:1].islower():
                rep = rep[:1].replace("i", "İ").replace("ı", "I").upper() + rep[1:]
            text = text[:a] + rep + text[a + len(bad):]
            used.append((a, a + len(rep)))
        return text


def default_paths(ass_path: str | None) -> list[str]:
    from .. import config
    out = []
    if ass_path:
        d = os.path.dirname(ass_path)
        out += [os.path.join(d, "terimler.tsv"), os.path.join(d, "glossary.tsv")]
    out.append(os.path.join(config.user_dir(), "terimler.tsv"))
    return out
