"""Referans altyazıdan aktarma (TR satırlarını stilli EN .ass'ın satırlarına yazma).

Osi'nin koşulları:
  * EN dosyası stil/aktör/konum bilgisinin sahibidir; zamanlar ondan alınır (isteğe bağlı TR'den).
  * Karşılığı olmayan EN satırı ASLA silinmez, olduğu gibi kalır ve raporlanır.
  * Ev sahibi bulunamayan TR satırı yine de yeni satır olarak eklenir ve raporlanır.
  * Belirsiz / çoklu / zayıf eşleşmeler gizlenmez, rapora yazılır.

Eşleşme ölçüsü: çakışma oranı = çakışan süre / iki satırdan KISA olanın süresi.
Her TR satırı en yüksek oranlı EN satırına gider. Oran `min_overlap` altındaysa
ev sahibi yoktur. İkinci en iyi aday da `ambiguous` eşiğini geçiyorsa belirsizdir.

Hata yüzeyi (bilerek söylenir):
  * TR ve EN farklı bölünmüşse (EN iki satır, TR tek satır) TR satırı yalnız birine
    gider; öbürü "karşılıksız EN" olarak raporlanır — elle düzeltme gerekir.
  * Zamanlar ciddi kaymışsa (senkron farkı > satır süresi) eşleşme çöker; önce
    zamanları hizalamak gerekir (bsync / zaman kaydırma).
  * Tabela/şarkı satırları EN'de ayrı stilde ise `host_styles` ile dışarıda bırakılmalı,
    yoksa üst üste binen diyalog TR'si tabelaya yazılabilir.
"""

from __future__ import annotations

import bisect
import re
from dataclasses import dataclass, field

from .assfile import Event
from .textutil import strip_tags

_LEAD_TAGS = re.compile(r"^(?:\{[^}]*\})+")

ISSUE_LABELS = {
    "multi": "Birden çok TR tek EN satırına birleştirildi",
    "ambiguous": "Belirsiz: TR satırı birden çok EN satırıyla çakışıyor",
    "weak": "Zayıf eşleşme (az çakışma)",
    "no_ref": "Karşılıksız EN satırı (olduğu gibi bırakıldı)",
    "orphan": "Ev sahibi yok: TR satırı yeni satır olarak eklendi",
    "orphan_skipped": "Ev sahibi yok: TR satırı eklenmedi",
}


@dataclass
class TransferOptions:
    min_overlap: float = 0.30
    weak_below: float = 0.60
    ambiguous: float = 0.50
    host_styles: set[str] | None = None      # None = tüm yorum dışı satırlar
    keep_leading_tags: bool = True           # EN satırın baştaki {\an8} vb. etiketleri korunur
    keep_original: bool = False              # EN metni sona {EN: …} yorum bloğu olarak eklenir
    use_ref_timing: bool = False
    insert_orphans: bool = True
    orphan_style: str | None = None          # None = zamanca en yakın ev sahibi satırın stili
    join_sep: str = "\\N"


@dataclass
class Issue:
    kind: str
    row: int | None          # sonuç belgesindeki satır (0 tabanlı)
    ref: int | None          # referans satırı (0 tabanlı)
    detail: str = ""

    @property
    def label(self) -> str:
        return ISSUE_LABELS.get(self.kind, self.kind)


@dataclass
class TransferPlan:
    events: list[Event]
    issues: list[Issue] = field(default_factory=list)
    host_of_ref: dict[int, int | None] = field(default_factory=dict)   # ref -> sonuç satırı
    stats: dict = field(default_factory=dict)


def overlap_ratio(a: Event, b: Event) -> tuple[float, int]:
    ov = min(a.end, b.end) - max(a.start, b.start)
    if ov <= 0:
        return 0.0, 0
    short = max(1, min(a.end - a.start, b.end - b.start))
    return ov / short, ov


def best_match(ev: Event, refs: list[Event], starts: list[int], max_dur: int) -> tuple[int | None, float]:
    """Bir satır için en çok çakışan referans (grid'deki Referans sütunu için)."""
    lo = bisect.bisect_left(starts, ev.start - max_dur)
    hi = bisect.bisect_right(starts, ev.end)
    best, br, bo = None, 0.0, 0
    for j in range(lo, hi):
        r, o = overlap_ratio(ev, refs[j])
        if (r, o) > (br, bo):
            best, br, bo = j, r, o
    return best, br


def plan_transfer(host_events: list[Event], refs: list[Event], opt: TransferOptions) -> TransferPlan:
    def eligible(ev: Event) -> bool:
        if ev.comment:
            return False
        return opt.host_styles is None or ev.style in opt.host_styles

    hosts = [i for i, ev in enumerate(host_events) if eligible(ev)]
    hosts_by_start = sorted(hosts, key=lambda i: host_events[i].start)
    h_starts = [host_events[i].start for i in hosts_by_start]
    max_h = max((host_events[i].end - host_events[i].start for i in hosts), default=0)

    assign: dict[int, list[int]] = {}
    orphans: list[int] = []
    pre_issues: list[tuple] = []          # (kind, host_idx | None, ref_idx, detail)
    for j, rf in enumerate(refs):
        if rf.comment or not strip_tags(rf.text).strip():
            continue
        lo = bisect.bisect_left(h_starts, rf.start - max_h)
        hi = bisect.bisect_right(h_starts, rf.end)
        scored = []
        for k in range(lo, hi):
            i = hosts_by_start[k]
            r, o = overlap_ratio(host_events[i], rf)
            if r > 0:
                scored.append((r, o, i))
        scored.sort(reverse=True)
        if not scored or scored[0][0] < opt.min_overlap:
            orphans.append(j)
            continue
        r0, _, i0 = scored[0]
        assign.setdefault(i0, []).append(j)
        if len(scored) > 1 and scored[1][0] >= opt.ambiguous:
            pre_issues.append(("ambiguous", i0, j,
                               f"{len(scored)} EN adayı; en iyi oran %{int(r0 * 100)}, ikinci %{int(scored[1][0] * 100)}"))
        elif r0 < opt.weak_below:
            pre_issues.append(("weak", i0, j, f"çakışma oranı %{int(r0 * 100)}"))

    # --- sonuç satırları
    out: list[Event] = []
    origin: list[tuple[str, int]] = []       # ("host", i) | ("orphan", j)
    for i, ev in enumerate(host_events):
        js = sorted(assign.get(i, []), key=lambda j: refs[j].start)
        if js:
            body = opt.join_sep.join(refs[j].text for j in js)
            lead = ""
            if opt.keep_leading_tags:
                m = _LEAD_TAGS.match(ev.text)
                lead = m.group(0) if m else ""
            text = lead + body
            if opt.keep_original:
                plain = strip_tags(ev.text).replace("{", "(").replace("}", ")")
                text += "{EN: " + plain + "}"
            kw = {"text": text}
            if opt.use_ref_timing:
                kw["start"] = min(refs[j].start for j in js)
                kw["end"] = max(refs[j].end for j in js)
            ev = ev.replace(**kw)
        out.append(ev)
        origin.append(("host", i))

    # --- ev sahipsizler: zamana göre araya yerleştir
    if opt.insert_orphans and orphans:
        merged: list[tuple[Event, tuple]] = list(zip(out, origin))
        for j in sorted(orphans, key=lambda j: refs[j].start):
            rf = refs[j]
            style = opt.orphan_style
            if style is None:
                near = None
                if hosts_by_start:
                    k = bisect.bisect_left(h_starts, rf.start)
                    cands = [hosts_by_start[x] for x in (k - 1, k) if 0 <= x < len(hosts_by_start)]
                    near = min(cands, key=lambda i: abs(host_events[i].start - rf.start)) if cands else None
                style = host_events[near].style if near is not None else "Default"
            new = Event(start=rf.start, end=rf.end, style=style, text=rf.text)
            # ilk "başlangıcı daha büyük" yorum dışı satırın önüne
            pos = len(merged)
            for p, (e, _o) in enumerate(merged):
                if not e.comment and e.start > rf.start:
                    pos = p
                    break
            merged.insert(pos, (new, ("orphan", j)))
        out = [e for e, _ in merged]
        origin = [o for _, o in merged]

    row_of_host = {i: r for r, (kind, i) in enumerate(origin) if kind == "host"}
    row_of_orphan = {j: r for r, (kind, j) in enumerate(origin) if kind == "orphan"}
    plan = TransferPlan(events=out)
    for i, js in assign.items():
        for j in js:
            plan.host_of_ref[j] = row_of_host[i]
        if len(js) > 1:
            plan.issues.append(Issue("multi", row_of_host[i], js[0], f"{len(js)} TR satırı \\N ile birleştirildi"))
    for kind, i, j, detail in pre_issues:
        plan.issues.append(Issue(kind, row_of_host.get(i), j, detail))
    for i in hosts:
        if i not in assign:
            plan.issues.append(Issue("no_ref", row_of_host[i], None, strip_tags(host_events[i].text)[:60]))
    for j in orphans:
        if opt.insert_orphans:
            plan.host_of_ref[j] = row_of_orphan[j]
            plan.issues.append(Issue("orphan", row_of_orphan[j], j, strip_tags(refs[j].text)[:60]))
        else:
            plan.host_of_ref[j] = None
            plan.issues.append(Issue("orphan_skipped", None, j, strip_tags(refs[j].text)[:60]))
    plan.issues.sort(key=lambda x: (x.row if x.row is not None else 10 ** 9, x.kind))
    plan.stats = {
        "ref_lines": sum(1 for r in refs if not r.comment and strip_tags(r.text).strip()),
        "hosts": len(hosts), "filled": len(assign), "orphans": len(orphans),
        "no_ref": sum(1 for i in hosts if i not in assign),
        "multi": sum(1 for js in assign.values() if len(js) > 1),
        "ambiguous": sum(1 for x in pre_issues if x[0] == "ambiguous"),
        "weak": sum(1 for x in pre_issues if x[0] == "weak"),
    }
    return plan
