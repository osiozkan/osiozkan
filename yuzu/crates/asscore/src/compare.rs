//! Source (EN) ↔ candidate (TR) alignment check — Osi's aligncompare, in Yuzu.
//!
//! * Lines of the two files are grouped by TIME OVERLAP; 1:1, 1:2, 2:1 (and larger) merges and
//!   splits become one group. Overlap = shared time / the shorter line's duration.
//! * The global shift between the files is found automatically (±15 s) and matching uses it;
//!   it can also be given.
//! * Typesetting (`\pos \move \clip \p`) and screen text ending in `\N \N \N` stay out unless
//!   `include_typeset`.
//! * The candidate is never modified; the marked copy is a separate document.
//!
//! Flags (Turkish codes, as in aligncompare):
//! EKSİK (in the source, missing in the candidate), FAZLA (only in the candidate), İNGİLİZCE?
//! (copy of the source or left in English), UZUNLUK (length ratio out of range), BOŞ (no
//! visible text), ZAMAN (start still off after the shift), SAYI (a number of the source is
//! missing; soft).
//!
//! Known limits: a shift above the search range or a wrong pair of files makes everything
//! EKSİK/FAZLA (reported as "weak correlation"); a shift that changes inside the episode
//! clusters ZAMAN flags — use piecewise sync first. Overlapping speakers fall into one group.

use crate::dsp;
use crate::model::{Event, Script};
use crate::tags::{self, Part};
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::OnceLock;

/// ms per sample of the shift search.
pub const RES: i64 = 10;

pub const FLAG_ORDER: [&str; 7] = ["EKSİK", "FAZLA", "İNGİLİZCE?", "UZUNLUK", "BOŞ", "ZAMAN", "SAYI"];

pub fn flag_help(code: &str) -> &'static str {
    match code {
        "EKSİK" => "Kaynakta var, adayda karşılığı yok (çevrilmemiş içerik)",
        "FAZLA" => "Adayda var, kaynakta yok (halüsinasyon şüphesi; kredi/nota bakar)",
        "İNGİLİZCE?" => "Aday satır kaynağın kopyası gibi ya da İngilizce kalmış",
        "UZUNLUK" => "Aday/kaynak karakter oranı makul aralığın dışında",
        "BOŞ" => "Aday satırın görünür metni yok",
        "ZAMAN" => "Eşleşmiş grupta zamanlama kaynaktan belirgin sapıyor",
        "SAYI" => "Kaynaktaki rakam(lar) adayda yok (yazıyla çeviri olabilir; yumuşak)",
        _ => "",
    }
}

// Words that also occur in Turkish (hey, oh, of, ok, on, at, her, it, can, ben, he…) are left out on purpose.
const EN_WORDS: &str = "the and you your you're i'm im i've i'll i'd me my mine we we're our us they they're them their \
she his him is are was were been being to with what what's that that's this these those there \
there's here where when why who how which don't dont doesn't didn't can't cannot won't wouldn't \
isn't aren't wasn't weren't haven't hasn't would could should will shall have has had just \
about from into because really something nothing anything everything someone anyone everyone \
know think want need going gonna wanna gotta got get let let's please thank thanks sorry yes \
yeah right well now then than very much more some any all only also too back again never \
always maybe still even though thing things way time people come came go goes went see saw look \
like said say says tell told make made take took give gave okay alright but or if so do does did";

fn en_words() -> &'static BTreeSet<&'static str> {
    static S: OnceLock<BTreeSet<&'static str>> = OnceLock::new();
    S.get_or_init(|| EN_WORDS.split_whitespace().collect())
}

// ---------------------------------------------------------------- options and results

#[derive(Clone, Debug)]
pub struct CompareOptions {
    /// None = find automatically; Some(ms) = candidate is this much later than the source.
    pub shift_ms: Option<i64>,
    pub shift_range_ms: i64,
    pub ratio_min: f64,
    pub ratio_max: f64,
    /// İNGİLİZCE? similarity threshold.
    pub sim: f64,
    pub max_start_diff_ms: i64,
    pub min_overlap: f64,
    pub skip_styles_src: String,
    pub skip_styles_cand: String,
    pub include_typeset: bool,
    /// UZUNLUK only when the source has at least this many characters.
    pub min_len: usize,
}

impl Default for CompareOptions {
    fn default() -> Self {
        CompareOptions { shift_ms: None, shift_range_ms: 15_000, ratio_min: 0.45, ratio_max: 2.2, sim: 0.85, max_start_diff_ms: 500,
            min_overlap: 0.25, skip_styles_src: String::new(), skip_styles_cand: String::new(), include_typeset: false, min_len: 10 }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// Event index in its own file.
    pub row: usize,
    pub start: i64,
    pub end: i64,
    pub style: String,
    pub text: String,
    pub plain: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Flag {
    pub code: &'static str,
    pub detail: String,
    /// Candidate rows the flag points at (empty = the whole group).
    pub rows: Vec<usize>,
}

impl Flag {
    pub fn label(&self) -> String { format!("{} {}", self.code, self.detail).trim().to_string() }
}

#[derive(Clone, Debug, Default)]
pub struct Group {
    pub src: Vec<Line>,
    pub cand: Vec<Line>,
    pub flags: Vec<Flag>,
}

impl Group {
    pub fn kind(&self) -> String { format!("{}:{}", self.src.len(), self.cand.len()) }
    pub fn codes(&self) -> Vec<&'static str> { self.flags.iter().map(|f| f.code).collect() }
    pub fn src_text(&self, sep: &str) -> String { self.src.iter().map(|x| x.plain.as_str()).collect::<Vec<_>>().join(sep) }
    pub fn cand_text(&self, sep: &str) -> String { self.cand.iter().map(|x| x.plain.as_str()).collect::<Vec<_>>().join(sep) }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Skipped { pub comment: usize, pub typeset: usize, pub style: usize, pub empty: usize }

impl Skipped {
    fn describe(&self) -> String {
        let parts: Vec<String> = [(self.comment, "yorum"), (self.typeset, "typeset"), (self.style, "stil"), (self.empty, "boş")]
            .iter().filter(|(n, _)| *n > 0).map(|(n, k)| format!("{} {}", n, k)).collect();
        if parts.is_empty() { String::new() } else { format!(" ({} atlandı)", parts.join(", ")) }
    }
}

#[derive(Clone, Debug, Default)]
pub struct CompareResult {
    pub groups: Vec<Group>,
    pub shift_ms: i64,
    pub shift_auto: bool,
    /// How clearly the shift stands out (0..1); for a manual shift the share of matched source lines.
    pub shift_score: f64,
    pub src_count: usize,
    pub cand_count: usize,
    pub src_skipped: Skipped,
    pub cand_skipped: Skipped,
    pub warnings: Vec<String>,
}

impl CompareResult {
    pub fn counts(&self) -> BTreeMap<&'static str, usize> {
        let mut out: BTreeMap<&'static str, usize> = FLAG_ORDER.iter().map(|k| (*k, 0)).collect();
        for g in &self.groups { for f in &g.flags { *out.entry(f.code).or_insert(0) += 1; } }
        out
    }
    pub fn kinds(&self) -> BTreeMap<String, usize> {
        let mut out = BTreeMap::new();
        for g in self.groups.iter().filter(|g| !g.src.is_empty() && !g.cand.is_empty()) { *out.entry(g.kind()).or_insert(0) += 1; }
        out
    }
    pub fn flagged(&self) -> Vec<&Group> { self.groups.iter().filter(|g| !g.flags.is_empty()).collect() }
    /// Flags per candidate row.
    pub fn flags_by_cand_row(&self) -> BTreeMap<usize, Vec<Flag>> {
        let mut out: BTreeMap<usize, Vec<Flag>> = BTreeMap::new();
        for g in &self.groups {
            for f in &g.flags {
                let rows: Vec<usize> = if f.rows.is_empty() { g.cand.iter().map(|c| c.row).collect() } else { f.rows.clone() };
                for r in rows { out.entry(r).or_default().push(f.clone()); }
            }
        }
        out
    }
}

// ---------------------------------------------------------------- text helpers

/// Visible text: tags and drawings removed, `\N \n \h` become spaces, whitespace collapsed.
pub fn plain_of(text: &str) -> String {
    let mut out = String::new();
    let mut drawing = false;
    for p in tags::split_parts(text) {
        match p {
            Part::Block(b) => {
                for t in tags::split_block_tags(&b) {
                    let (n, a) = tags::tag_name_arg(&t);
                    if n == "p" { drawing = a.trim().parse::<f64>().map(|v| v > 0.0).unwrap_or(false); }
                }
            }
            Part::Text(t) => if !drawing { out.push_str(&t) },
        }
    }
    let t = out.replace("\\N", " ").replace("\\n", " ").replace("\\h", " ");
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn ts_tags() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\\(?:pos|move|i?clip)\s*\(|\\p[1-9]").unwrap())
}

fn screen_text() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(\\N\s*){3}$").unwrap())
}

/// Typesetting or screen text (ends in `\N \N \N`).
pub fn is_typeset(e: &Event) -> bool {
    ts_tags().is_match(&e.text) || screen_text().is_match(e.text.trim_end())
}

fn norm(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        for l in c.to_lowercase() {
            if is_mark(l) { continue; }
            o.push(if l.is_alphanumeric() || l.is_whitespace() { l } else { ' ' });
        }
    }
    o.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_mark(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F)
}

fn is_format(c: char) -> bool {
    matches!(c as u32, 0x00AD | 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x2064 | 0xFEFF)
}

/// Longest common block of a[alo..ahi], b[blo..bhi]: earliest in `a`, then earliest in `b`
/// (difflib's tie-breaking).
fn longest_match(a: &[char], b: &[char], alo: usize, ahi: usize, blo: usize, bhi: usize) -> (usize, usize, usize) {
    let w = bhi - blo;
    // prev[x]: length of the common run ending at a[i-1] and b[blo + x - 1]
    let mut prev = vec![0usize; w + 1];
    let (mut bi, mut bj, mut bk) = (alo, blo, 0usize);
    for i in alo..ahi {
        let mut cur = vec![0usize; w + 1];
        for j in blo..bhi {
            if a[i] == b[j] {
                let k = prev[j - blo] + 1;
                cur[j - blo + 1] = k;
                if k > bk { bk = k; bi = i + 1 - k; bj = j + 1 - k; }
            }
        }
        prev = cur;
    }
    (bi, bj, bk)
}

/// difflib.SequenceMatcher(None, a, b, autojunk=False).ratio() on normalised text.
pub fn similarity(a: &str, b: &str) -> f64 {
    let (na, nb) = (norm(a), norm(b));
    if na.is_empty() || nb.is_empty() { return 0.0; }
    let (a, b): (Vec<char>, Vec<char>) = (na.chars().collect(), nb.chars().collect());
    let mut matched = 0usize;
    let mut stack = vec![(0, a.len(), 0, b.len())];
    while let Some((alo, ahi, blo, bhi)) = stack.pop() {
        if alo >= ahi || blo >= bhi { continue; }
        let (i, j, k) = longest_match(&a, &b, alo, ahi, blo, bhi);
        if k == 0 { continue; }
        matched += k;
        stack.push((alo, i, blo, j));
        stack.push((i + k, ahi, j + k, bhi));
    }
    2.0 * matched as f64 / (a.len() + b.len()) as f64
}

/// English function words when the text looks left in English; empty otherwise.
pub fn english_words(text: &str) -> Vec<String> {
    if text.chars().any(|c| "çğıöşüÇĞİÖŞÜ".contains(c)) { return vec![]; }
    static R: OnceLock<Regex> = OnceLock::new();
    let re = R.get_or_init(|| Regex::new(r"[a-z]+(?:'[a-z]+)?").unwrap());
    let low = text.to_lowercase().replace('’', "'");
    let words: Vec<&str> = re.find_iter(&low).map(|m| m.as_str()).collect();
    if words.len() < 3 { return vec![]; }
    let hits: Vec<String> = words.iter().filter(|w| en_words().contains(*w)).map(|w| w.to_string()).collect();
    if hits.len() >= 2 && hits.len() as f64 / words.len() as f64 >= 0.3 { hits } else { vec![] }
}

/// Digit strings in the text ("1.000" and "1000" both give "1000").
pub fn numbers_in(text: &str) -> BTreeSet<String> {
    static R: OnceLock<Regex> = OnceLock::new();
    let re = R.get_or_init(|| Regex::new(r"\d+(?:[.,:]\d+)*").unwrap());
    re.find_iter(text).map(|m| m.as_str().chars().filter(|c| c.is_ascii_digit() || c.is_numeric()).collect()).collect()
}

/// Visible characters: whitespace, combining marks and format characters do not count.
pub fn visible_len(s: &str) -> usize {
    s.chars().filter(|c| !c.is_whitespace() && !is_mark(*c) && !is_format(*c)).count()
}

// ---------------------------------------------------------------- line selection

fn collect(events: &[Event], src_side: bool, o: &CompareOptions) -> (Vec<Line>, Skipped) {
    let pat = if src_side { &o.skip_styles_src } else { &o.skip_styles_cand };
    let rx = if pat.is_empty() { None } else { Regex::new(pat).ok() };
    let mut out = Vec::new();
    let mut sk = Skipped::default();
    for (i, e) in events.iter().enumerate() {
        if e.comment { sk.comment += 1; continue; }
        if rx.as_ref().map(|r| r.is_match(&e.style)).unwrap_or(false) { sk.style += 1; continue; }
        if !o.include_typeset && is_typeset(e) { sk.typeset += 1; continue; }
        let plain = plain_of(&e.text);
        if src_side && plain.is_empty() { sk.empty += 1; continue; }
        out.push(Line { row: i, start: e.start, end: e.end, style: e.style.clone(), text: e.text.clone(), plain });
    }
    (out, sk)
}

// ---------------------------------------------------------------- global shift

fn coverage(lines: &[Line], n: usize) -> Vec<f64> {
    let mut a = vec![0.0; n];
    for x in lines {
        let (s, e) = (x.start.div_euclid(RES).max(0) as usize, x.end.div_euclid(RES).clamp(0, n as i64) as usize);
        for v in a.iter_mut().take(e).skip(s) { *v = 1.0; }
    }
    a
}

fn onsets(lines: &[Line], n: usize) -> Vec<f64> {
    let mut a = vec![0.0; n];
    for x in lines {
        for t in [x.start, x.end] {
            let k = t.div_euclid(RES);
            if k >= 0 && (k as usize) < n { a[k as usize] += 1.0; }
        }
    }
    dsp::convolve_same(&a, &dsp::gaussian(120.0 / RES as f64, false))
}

/// Shift of the candidate relative to the source (ms; positive = candidate LATER) and how
/// clearly it stands out (0..1). Coarse: coverage correlation (robust to different splits),
/// middle of the plateau. Fine: line start/end pulses within ±300 ms.
pub fn find_shift(src: &[Line], cand: &[Line], max_shift_ms: i64) -> (i64, f64) {
    if src.is_empty() || cand.is_empty() { return (0, 0.0); }
    let total = src.iter().chain(cand).map(|x| x.end).max().unwrap_or(0) + max_shift_ms + 1000;
    let n = (total / RES + 1) as usize;
    let r = (max_shift_ms / RES) as usize;
    let (a, b) = (coverage(src, n), coverage(cand, n));
    let vals = dsp::xcorr(&a, &b, r);
    let vmax = vals.iter().cloned().fold(f64::MIN, f64::max);
    if vmax <= 0.0 { return (0, 0.0); }
    let k = vals.iter().position(|&v| v == vmax).unwrap_or(r);
    let (mut lo, mut hi) = (k, k);
    let tol = vmax * 0.995;
    while lo > 0 && vals[lo - 1] >= tol { lo -= 1; }
    while hi + 1 < vals.len() && vals[hi + 1] >= tol { hi += 1; }
    let coarse = ((lo + hi) / 2) as i64 - r as i64;
    let (oa, ob) = (onsets(src, n), onsets(cand, n));
    let fv = dsp::xcorr(&oa, &ob, r);
    let win = 300 / RES;
    let mut fine = coarse;
    let mut best = 0.0;
    for (i, &v) in fv.iter().enumerate() {
        let lag = i as i64 - r as i64;
        if (lag - coarse).abs() <= win && v > best { best = v; fine = lag; }
    }
    let base = dsp::median(&vals);
    let (sa, sb): (f64, f64) = (a.iter().sum(), b.iter().sum());
    let score = (vmax - base) / (sa.min(sb) - base).max(1.0);
    (fine * RES, score.clamp(0.0, 1.0))
}

// ---------------------------------------------------------------- grouping and flags

fn ratio(a0: i64, a1: i64, b0: i64, b1: i64) -> f64 {
    let ov = a1.min(b1) - a0.max(b0);
    if ov <= 0 { return 0.0; }
    ov as f64 / (a1 - a0).min(b1 - b0).max(1) as f64
}

/// Connected components of the overlap graph (candidate times corrected by the shift).
pub fn build_groups(src: &[Line], cand: &[Line], shift_ms: i64, min_overlap: f64) -> Vec<Group> {
    let ns = src.len();
    let mut parent: Vec<usize> = (0..ns + cand.len()).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x { p[x] = p[p[x]]; x = p[x]; }
        x
    }
    let mut order: Vec<usize> = (0..cand.len()).collect();
    order.sort_by_key(|&j| cand[j].start);
    let c_starts: Vec<i64> = order.iter().map(|&j| cand[j].start - shift_ms).collect();
    let max_c = cand.iter().map(|c| c.end - c.start).max().unwrap_or(0);
    for (i, s) in src.iter().enumerate() {
        let lo = c_starts.partition_point(|&v| v < s.start - max_c);
        let hi = c_starts.partition_point(|&v| v <= s.end);
        for &j in &order[lo..hi] {
            let c = &cand[j];
            if ratio(s.start, s.end, c.start - shift_ms, c.end - shift_ms) >= min_overlap {
                let (ra, rb) = (find(&mut parent, i), find(&mut parent, ns + j));
                if ra != rb { parent[rb] = ra; }
            }
        }
    }
    let mut comps: HashMap<usize, Group> = HashMap::new();
    let mut first_seen: Vec<usize> = Vec::new();
    for (i, s) in src.iter().enumerate() {
        let r = find(&mut parent, i);
        comps.entry(r).or_insert_with(|| { first_seen.push(r); Group::default() }).src.push(s.clone());
    }
    for (j, c) in cand.iter().enumerate() {
        let r = find(&mut parent, ns + j);
        comps.entry(r).or_insert_with(|| { first_seen.push(r); Group::default() }).cand.push(c.clone());
    }
    let mut groups: Vec<Group> = first_seen.into_iter().filter_map(|r| comps.remove(&r)).collect();
    for g in groups.iter_mut() {
        g.src.sort_by_key(|x| (x.start, x.row));
        g.cand.sort_by_key(|x| (x.start, x.row));
    }
    groups.sort_by_key(|g| {
        let t = g.src.iter().map(|x| x.start).chain(g.cand.iter().map(|x| x.start - shift_ms)).min().unwrap_or(0);
        (t, g.cand.first().map(|c| c.row as i64).unwrap_or(-1))
    });
    groups
}

fn flag_group(g: &mut Group, shift_ms: i64, o: &CompareOptions) {
    let mut flags = Vec::new();
    if !g.src.is_empty() && g.cand.is_empty() { flags.push(Flag { code: "EKSİK", detail: String::new(), rows: vec![] }); }
    if !g.cand.is_empty() && g.src.is_empty() { flags.push(Flag { code: "FAZLA", detail: String::new(), rows: g.cand.iter().map(|c| c.row).collect() }); }
    let empty: Vec<usize> = g.cand.iter().filter(|c| c.plain.is_empty()).map(|c| c.row).collect();
    if !empty.is_empty() { flags.push(Flag { code: "BOŞ", detail: String::new(), rows: empty }); }
    if !g.src.is_empty() && !g.cand.is_empty() {
        let (s_txt, c_txt) = (g.src_text(" "), g.cand_text(" "));
        if !c_txt.is_empty() {
            let sim = similarity(&s_txt, &c_txt);
            if sim >= o.sim && norm(&c_txt).chars().count() >= 4 {
                flags.push(Flag { code: "İNGİLİZCE?", detail: format!("kaynağa %{} benzer", (sim * 100.0) as i64), rows: vec![] });
            } else {
                let hits = english_words(&c_txt);
                if !hits.is_empty() {
                    let mut seen = Vec::new();
                    for h in hits.iter().take(5) { if !seen.contains(h) { seen.push(h.clone()); } }
                    flags.push(Flag { code: "İNGİLİZCE?", detail: format!("İngilizce sözcükler: {}", seen.join(", ")), rows: vec![] });
                }
            }
            let (ls, lc) = (visible_len(&s_txt), visible_len(&c_txt));
            if ls >= o.min_len && ls > 0 {
                let r = lc as f64 / ls as f64;
                if r < o.ratio_min || r > o.ratio_max { flags.push(Flag { code: "UZUNLUK", detail: format!("oran {:.2} ({}/{})", r, lc, ls), rows: vec![] }); }
            }
            let cn = numbers_in(&c_txt);
            let mut miss: Vec<String> = numbers_in(&s_txt).into_iter().filter(|x| !cn.contains(x)).collect();
            miss.sort_by(|a, b| (a.len(), a).cmp(&(b.len(), b)));
            if !miss.is_empty() { flags.push(Flag { code: "SAYI", detail: format!("eksik: {}", miss.into_iter().take(6).collect::<Vec<_>>().join(", ")), rows: vec![] }); }
        }
        let d = (g.cand.iter().map(|c| c.start).min().unwrap_or(0) - shift_ms) - g.src.iter().map(|s| s.start).min().unwrap_or(0);
        if d.abs() > o.max_start_diff_ms { flags.push(Flag { code: "ZAMAN", detail: format!("{:+.2} sn", d as f64 / 1000.0), rows: vec![] }); }
    }
    flags.sort_by_key(|f| FLAG_ORDER.iter().position(|k| *k == f.code).unwrap_or(99));
    g.flags = flags;
}

/// Compare a source (reference, usually English) with a candidate (translation).
pub fn compare(src_events: &[Event], cand_events: &[Event], o: &CompareOptions) -> CompareResult {
    let (src, s_skip) = collect(src_events, true, o);
    let (cand, c_skip) = collect(cand_events, false, o);
    let mut warnings = Vec::new();
    let (shift, mut score, auto) = match o.shift_ms {
        None => {
            let (sh, sc) = find_shift(&src, &cand, o.shift_range_ms);
            if !src.is_empty() && !cand.is_empty() && sc < 0.35 {
                warnings.push(format!("Zayıf ilinti (%{}): kayma {} sn'den büyük ya da dosyalar aynı bölüme ait değil olabilir.", (sc * 100.0) as i64, o.shift_range_ms as f64 / 1000.0));
            }
            if sh.abs() >= o.shift_range_ms - RES {
                warnings.push("Bulunan kayma arama aralığının sınırında — önce senkronla ya da kaymayı elle ver.".into());
            }
            (sh, sc, true)
        }
        Some(ms) => (ms, 0.0, false),
    };
    let mut groups = build_groups(&src, &cand, shift, o.min_overlap);
    for g in groups.iter_mut() { flag_group(g, shift, o); }
    if !auto {
        score = groups.iter().filter(|g| !g.src.is_empty() && !g.cand.is_empty()).count() as f64 / src.len().max(1) as f64;
    }
    CompareResult { groups, shift_ms: shift, shift_auto: auto, shift_score: score, src_count: src.len(), cand_count: cand.len(), src_skipped: s_skip, cand_skipped: c_skip, warnings }
}

/// Shift of `cand` against `src` with the default rules (for the reference panel).
pub fn shift_between(src_events: &[Event], cand_events: &[Event]) -> (i64, f64) {
    let o = CompareOptions::default();
    let (src, _) = collect(src_events, true, &o);
    let (cand, _) = collect(cand_events, false, &o);
    find_shift(&src, &cand, o.shift_range_ms)
}

// ---------------------------------------------------------------- reports

pub fn shift_text(r: &CompareResult) -> String {
    let s = r.shift_ms as f64 / 1000.0;
    let how = if r.shift_ms == 0 { "kayma yok".to_string() } else { format!("{:+.2} sn (aday kaynaktan {})", s, if s > 0.0 { "GEÇ" } else { "ERKEN" }) };
    format!("{}{}", how, if r.shift_auto { " — otomatik" } else { " — elle verildi" })
}

pub fn summary_lines(r: &CompareResult, src_name: &str, cand_name: &str) -> Vec<String> {
    let c = r.counts();
    let kinds = r.kinds();
    let matched: usize = kinds.values().sum();
    let k = |s: &str| kinds.get(s).copied().unwrap_or(0);
    let (k11, k12, k21) = (k("1:1"), k("1:2"), k("2:1"));
    let mut out = vec![
        format!("Kaynak: {} — {} satır{}", src_name, r.src_count, r.src_skipped.describe()),
        format!("Aday:   {} — {} satır{}", cand_name, r.cand_count, r.cand_skipped.describe()),
        format!("Bulunan kayma: {}", shift_text(r)),
        format!("Eşleşen grup: {} (1:1 {} · 1:2 {} · 2:1 {} · diğer {})", matched, k11, k12, k21, matched - k11 - k12 - k21),
        format!("Bayraklar: {}", FLAG_ORDER.iter().map(|f| format!("{} {}", f, c.get(f).copied().unwrap_or(0))).collect::<Vec<_>>().join(" · ")),
    ];
    out.extend(r.warnings.iter().map(|w| format!("UYARI: {}", w)));
    out
}

fn times(lines: &[Line]) -> String {
    if lines.is_empty() { return String::new(); }
    format!("{} – {}", crate::time::format_ass(lines.iter().map(|x| x.start).min().unwrap_or(0)), crate::time::format_ass(lines.iter().map(|x| x.end).max().unwrap_or(0)))
}

fn csv_field(s: &str) -> String {
    if s.contains([';', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

/// `;`-separated CSV with a BOM (opens correctly in Excel with Turkish regional settings).
pub fn to_csv(r: &CompareResult, only_flags: bool) -> String {
    let mut o = String::from("\u{feff}");
    let row = |cols: &[String]| cols.iter().map(|c| csv_field(c)).collect::<Vec<_>>().join(";") + "\r\n";
    o.push_str(&row(&["Grup", "Tür", "Kaynak zamanı", "Aday satır(lar)ı", "Aday zamanı", "Kaynak", "Aday", "Bayraklar"].map(String::from)));
    for (k, g) in r.groups.iter().enumerate() {
        if only_flags && g.flags.is_empty() { continue; }
        o.push_str(&row(&[(k + 1).to_string(), g.kind(), times(&g.src), g.cand.iter().map(|c| (c.row + 1).to_string()).collect::<Vec<_>>().join(","),
            times(&g.cand), g.src_text(" / "), g.cand_text(" / "), g.flags.iter().map(|f| f.label()).collect::<Vec<_>>().join(" | ")]));
    }
    o
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#x27;")
}

const HTML_CSS: &str = r#"
:root { --bg:#ffffff; --fg:#1d1e22; --muted:#6b6c73; --line:#e3e4e8; --row:#f7f8fa; --head:#eef0f4;
        --chip:#e8e9ec; --err:#b3261e; --warn:#8a5a00; --info:#285ea8; }
@media (prefers-color-scheme: dark) {
  :root { --bg:#16171a; --fg:#e3e3e6; --muted:#9a9ba3; --line:#2c2d33; --row:#1b1c20; --head:#22242a;
          --chip:#2a2b30; --err:#ff9b9b; --warn:#ffd27a; --info:#8ab4ff; } }
body { margin:0; padding:16px; background:var(--bg); color:var(--fg);
       font:14px/1.45 "Segoe UI", system-ui, -apple-system, sans-serif; }
h1 { font-size:20px; margin:0 0 8px; }
.sum { color:var(--muted); margin:0 0 4px; }
.warn { color:var(--warn); font-weight:600; }
.bar { position:sticky; top:0; background:var(--bg); padding:8px 0; border-bottom:1px solid var(--line);
       display:flex; flex-wrap:wrap; gap:6px; z-index:1; }
.bar button { border:1px solid var(--line); background:var(--chip); color:var(--fg); border-radius:14px;
              padding:3px 10px; cursor:pointer; font:inherit; }
.bar button.on { outline:2px solid var(--info); }
table { border-collapse:collapse; width:100%; margin-top:8px; table-layout:fixed; }
th, td { border-bottom:1px solid var(--line); padding:6px 8px; vertical-align:top; text-align:left;
         overflow-wrap:anywhere; }
th { background:var(--head); position:sticky; top:46px; }
tr:nth-child(even) td { background:var(--row); }
td.n { width:3.5em; color:var(--muted); } td.k { width:3em; color:var(--muted); }
td.t { width:9.5em; color:var(--muted); font-variant-numeric:tabular-nums; font-size:12px; }
.chip { display:inline-block; border-radius:10px; padding:1px 8px; margin:0 4px 3px 0; font-size:12px;
        font-weight:600; background:var(--chip); }
.c-EKSİK, .c-FAZLA, .c-BOŞ { color:var(--err); } .c-İNGİLİZCE\? { color:var(--err); }
.c-UZUNLUK, .c-ZAMAN { color:var(--warn); } .c-SAYI { color:var(--info); }
.d { color:var(--muted); font-size:12px; }
.row { color:var(--muted); font-size:12px; margin-right:4px; }
"#;

const HTML_JS: &str = r#"
const rows = [...document.querySelectorAll('tbody tr')];
function show(code) {
  document.querySelectorAll('.bar button').forEach(b => b.classList.toggle('on', b.dataset.f === code));
  rows.forEach(r => {
    const f = r.dataset.flags;
    r.style.display = code === 'all' ? '' : code === 'flagged' ? (f ? '' : 'none')
                     : (f.split(' ').includes(code) ? '' : 'none');
  });
}
document.querySelectorAll('.bar button').forEach(b => b.addEventListener('click', () => show(b.dataset.f)));
show(document.body.dataset.start);
"#;

/// Self-contained HTML report with filter buttons.
pub fn to_html(r: &CompareResult, src_name: &str, cand_name: &str, only_flags: bool) -> String {
    let base = |p: &str| std::path::Path::new(p).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| p.to_string());
    let c = r.counts();
    let mut o = vec![
        "<!doctype html><html lang='tr'><head><meta charset='utf-8'>".to_string(),
        "<meta name='viewport' content='width=device-width, initial-scale=1'>".into(),
        format!("<title>Karşılaştırma — {}</title>", esc(&base(cand_name))),
        format!("<style>{}</style></head>", HTML_CSS),
        format!("<body data-start='{}'>", if only_flags { "flagged" } else { "all" }),
        "<h1>Kaynak ↔ aday karşılaştırması</h1>".into(),
    ];
    for ln in summary_lines(r, &base(src_name), &base(cand_name)) {
        o.push(format!("<p class='{}'>{}</p>", if ln.starts_with("UYARI") { "sum warn" } else { "sum" }, esc(&ln)));
    }
    o.push("<div class='bar'>".into());
    o.push(format!("<button data-f='all'>Tümü ({})</button>", r.groups.len()));
    o.push(format!("<button data-f='flagged'>Bayraklı ({})</button>", r.flagged().len()));
    for k in FLAG_ORDER {
        let n = c.get(k).copied().unwrap_or(0);
        if n > 0 { o.push(format!("<button data-f='{}' title='{}'>{} ({})</button>", esc(k), esc(flag_help(k)), esc(k), n)); }
    }
    o.push("</div>".into());
    o.push("<table><thead><tr><th style='width:3.5em'>#</th><th style='width:3em'>Tür</th><th style='width:9.5em'>Zaman (kaynak)</th><th>Kaynak</th><th>Aday</th><th style='width:22%'>Bayraklar</th></tr></thead><tbody>".into());
    for (k, g) in r.groups.iter().enumerate() {
        if only_flags && g.flags.is_empty() { continue; }
        let src = g.src.iter().map(|x| esc(&x.plain)).collect::<Vec<_>>().join("<br>");
        let cand = g.cand.iter().map(|x| format!("<span class='row'>{}</span>{}", x.row + 1, if x.plain.is_empty() { "<i>(boş)</i>".to_string() } else { esc(&x.plain) })).collect::<Vec<_>>().join("<br>");
        let chips: String = g.flags.iter().map(|f| format!("<div><span class='chip c-{}'>{}</span><span class='d'>{}</span></div>", esc(f.code), esc(f.code), esc(&f.detail))).collect();
        let mut codes: Vec<&str> = Vec::new();
        for c in g.codes() { if !codes.contains(&c) { codes.push(c); } }
        let t = if g.src.is_empty() { times(&g.cand) } else { times(&g.src) };
        o.push(format!("<tr data-flags='{}'><td class='n'>{}</td><td class='k'>{}</td><td class='t'>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>", esc(&codes.join(" ")), k + 1, g.kind(), esc(&t), src, cand, chips));
    }
    o.push(format!("</tbody></table><script>{}</script></body></html>", HTML_JS));
    o.join("\n")
}

/// Marked copy of the candidate: `{QC: …}` in front of flagged lines; source lines missing in
/// the candidate (EKSİK) appended as comments at the candidate's time (shift applied).
pub fn marked_document(cand: &Script, r: &CompareResult) -> Script {
    let mut doc = cand.clone();
    for (row, flags) in r.flags_by_cand_row() {
        if let Some(e) = doc.events.get_mut(row) {
            let body = flags.iter().map(|f| f.label()).collect::<Vec<_>>().join("; ").replace('{', "(").replace('}', ")");
            e.text = format!("{{QC: {}}}{}", body, e.text);
        }
    }
    doc.events.extend(missing_as_comments(cand, r));
    doc
}

/// EKSİK source lines as comment events on the candidate's time line (for "add missing").
pub fn missing_as_comments(cand: &Script, r: &CompareResult) -> Vec<Event> {
    let mut styles: HashMap<&str, usize> = HashMap::new();
    for e in cand.events.iter().filter(|e| !e.comment) { *styles.entry(e.style.as_str()).or_insert(0) += 1; }
    let style = styles.into_iter().max_by_key(|(s, n)| (*n, std::cmp::Reverse(s.to_string()))).map(|(s, _)| s.to_string()).unwrap_or_else(|| "Default".into());
    let mut out = Vec::new();
    for g in r.groups.iter().filter(|g| !g.src.is_empty() && g.cand.is_empty()) {
        for s in &g.src {
            let mut e = Event::new(crate::time::snap_cs(s.start + r.shift_ms), crate::time::snap_cs(s.end + r.shift_ms), &style, &format!("{{QC: EKSİK}}{}", s.text));
            e.comment = true;
            out.push(e);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const EN: [&str; 10] = ["Where are you going?", "I told you not to come back here.", "We need to find the key before dawn.",
        "That's the only way out.", "Do you really think so?", "Look at the sky tonight.", "He said he would wait for us.",
        "Let's go, everyone!", "I can't believe this is happening.", "Thank you for everything."];
    const TR: [&str; 10] = ["Nereye gidiyorsun?", "Buraya geri gelme demiştim.", "Şafaktan önce anahtarı bulmalıyız.",
        "Tek çıkış yolu bu.", "Gerçekten öyle mi düşünüyorsun?", "Bu gece gökyüzüne bak.", "Bizi bekleyeceğini söyledi.",
        "Hadi gidelim, millet!", "Bunun olduğuna inanamıyorum.", "Her şey için teşekkürler."];

    /// Small deterministic generator (the Python tests used `random.Random(7)`; values differ, rules do not).
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 { self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0 }
        fn range(&mut self, lo: i64, hi: i64) -> i64 { lo + (self.next() % ((hi - lo + 1) as u64)) as i64 }
        fn choice(&mut self, v: &[i64]) -> i64 { v[(self.next() % v.len() as u64) as usize] }
    }

    fn ev(s: i64, e: i64, t: &str) -> Event { Event::new(s, e, "Default", t) }

    /// Source lines + a Turkish candidate of the same episode, shifted. Known defects: line 10
    /// missing (EKSİK), 20 split in two (1:2), 30+31 merged (2:1), 40 left in English, 50 empty,
    /// 60 a number missing, 70 0.9 s late (ZAMAN), a credit line at the end (FAZLA).
    fn make_pair(shift: i64, n: usize, seed: u64) -> (Vec<Event>, Vec<Event>) {
        let mut rnd = Rng(seed * 2654435761 + 1);
        let (mut src, mut cand) = (Vec::new(), Vec::new());
        let mut t = 5000;
        for i in 0..n {
            let dur = rnd.range(1200, 3600);
            let k = i % EN.len();
            src.push(ev(t, t + dur, &format!("{}{}", EN[k], if i == 60 { format!(" It's {} yen.", i + 1000) } else { String::new() })));
            t += dur + rnd.choice(&[150, 300, 600, 1200, 2500, 4500]);
        }
        for (i, s) in src.iter().enumerate() {
            let k = i % TR.len();
            let (mut st, en) = (s.start + shift, s.end + shift);
            match i {
                10 | 31 => continue,
                20 => {
                    let mid = (st + en) / 2;
                    let half = TR[k].chars().count() / 2;
                    cand.push(ev(st, mid, &TR[k].chars().take(half).collect::<String>()));
                    cand.push(ev(mid, en, &TR[k].chars().skip(half).collect::<String>()));
                    continue;
                }
                30 => { cand.push(ev(st, src[31].end + shift, &format!("{} {}", TR[k], TR[(k + 1) % TR.len()]))); continue; }
                _ => {}
            }
            let text = match i {
                40 => "We need to find the key, and you know that.".to_string(),
                50 => "{\\i1}{\\i0}".to_string(),
                60 => format!("{} Bin yen tutuyor.", TR[k]),
                _ => TR[k].to_string(),
            };
            if i == 70 { st += 900; }
            cand.push(ev(st, en, &text));
        }
        let last = src.last().unwrap().end;
        cand.push(ev(last + shift + 3000, last + shift + 7000, "Çeviri: Osi — iyi seyirler"));
        (src, cand)
    }

    fn group_of_src(r: &CompareResult, i: usize) -> &Group {
        r.groups.iter().find(|g| g.src.iter().any(|x| x.row == i)).unwrap_or_else(|| panic!("source {} not grouped", i))
    }

    #[test]
    fn finds_shift_despite_splits_and_merges() {
        for shift in [2350, -1780, 0, 11420] {
            let (src, cand) = make_pair(shift, 160, 7);
            let r = compare(&src, &cand, &CompareOptions::default());
            assert!(r.shift_auto);
            assert!((r.shift_ms - shift).abs() <= 20, "{} vs {}", shift, r.shift_ms);
            assert!(r.shift_score > 0.6, "{}", r.shift_score);
            assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        }
    }

    #[test]
    fn wrong_pair_warns_and_manual_shift() {
        let (src, _) = make_pair(0, 160, 1);
        let (_, cand) = make_pair(0, 160, 99);
        let r = compare(&src, &cand, &CompareOptions::default());
        assert!(r.warnings.iter().any(|w| w.contains("Zayıf ilinti")), "{:?}", r.warnings);
        let (src, cand) = make_pair(2350, 160, 7);
        let r = compare(&src, &cand, &CompareOptions { shift_ms: Some(2350), ..Default::default() });
        assert!(!r.shift_auto);
        assert_eq!(r.shift_ms, 2350);
        let r0 = compare(&src, &cand, &CompareOptions { shift_ms: Some(0), ..Default::default() });
        assert!(r0.counts()["EKSİK"] > 20);
        assert_eq!(find_shift(&[], &[], 15000), (0, 0.0));
    }

    #[test]
    fn each_flag_and_kinds() {
        let (src, cand) = make_pair(2350, 160, 7);
        let r = compare(&src, &cand, &CompareOptions::default());
        assert_eq!(group_of_src(&r, 20).kind(), "1:2");
        assert_eq!(group_of_src(&r, 30).kind(), "2:1");
        assert!(std::ptr::eq(group_of_src(&r, 30), group_of_src(&r, 31)));
        assert_eq!(r.kinds().get("1:2"), Some(&1));
        assert_eq!(r.kinds().get("2:1"), Some(&1));
        assert_eq!(group_of_src(&r, 10).codes(), vec!["EKSİK"]);
        assert!(group_of_src(&r, 40).codes().contains(&"İNGİLİZCE?"));
        assert!(group_of_src(&r, 50).codes().contains(&"BOŞ"));
        let g60 = group_of_src(&r, 60);
        let f = g60.flags.iter().find(|f| f.code == "SAYI").expect("SAYI");
        assert!(f.detail.contains("1060"), "{}", f.detail);
        let g70 = group_of_src(&r, 70);
        assert_eq!(g70.codes(), vec!["ZAMAN"]);
        assert!(g70.flags[0].detail.contains("+0.90"), "{}", g70.flags[0].detail);
        let extra: Vec<&Group> = r.groups.iter().filter(|g| g.src.is_empty()).collect();
        assert_eq!(extra.len(), 1);
        assert_eq!(extra[0].codes(), vec!["FAZLA"]);
        let c = r.counts();
        assert_eq!((c["EKSİK"], c["FAZLA"], c["BOŞ"], c["ZAMAN"]), (1, 1, 1, 1));
        assert!(group_of_src(&r, 30).codes().is_empty(), "{:?}", group_of_src(&r, 30).flags);
        assert!(group_of_src(&r, 20).codes().is_empty(), "{:?}", group_of_src(&r, 20).flags);
        assert!(r.groups.iter().filter(|g| !g.src.is_empty() && !g.cand.is_empty() && g.flags.is_empty()).count() > 140);
    }

    #[test]
    fn copy_of_source_length_and_english() {
        let src = vec![ev(0, 2000, "Giorno Giovanna"), ev(3000, 5000, "This is a long sentence to translate.")];
        let cand = vec![ev(0, 2000, "Giorno Giovanna"), ev(3000, 5000, "Evet.")];
        let r = compare(&src, &cand, &CompareOptions { shift_ms: Some(0), ..Default::default() });
        assert!(r.groups[0].codes().contains(&"İNGİLİZCE?"));
        assert!(r.groups[1].codes().contains(&"UZUNLUK"));
        let loose = compare(&src, &cand, &CompareOptions { shift_ms: Some(0), sim: 1.01, ratio_min: 0.1, ..Default::default() });
        assert!(loose.groups[0].codes().is_empty() && loose.groups[1].codes().is_empty());
        assert!(!english_words("I told you not to come back here.").is_empty());
        assert!(english_words("Buraya geri gelme demiştim.").is_empty());
        assert!(english_words("Hey, OK, her şey on dakikada biter!").is_empty());
        assert!(english_words("Oh, hey.").is_empty());
    }

    #[test]
    fn similarity_matches_difflib() {
        // values from Python's difflib.SequenceMatcher(None, a, b, autojunk=False).ratio()
        assert!((similarity("abcd", "bcde") - 0.75).abs() < 1e-9);
        assert!((similarity("Hello there", "hello, there!") - 1.0).abs() < 1e-9);
        assert!((similarity("kitten", "sitting") - 8.0 / 13.0).abs() < 1e-9);
        assert_eq!(similarity("", "x"), 0.0);
    }

    #[test]
    fn typeset_and_style_skipping() {
        let src = vec![ev(0, 2000, "Hello there."), ev(0, 2000, "{\\pos(10,10)}SHOP"), ev(3000, 4000, "Sign\\N \\N \\N")];
        let cand = vec![ev(0, 2000, "Merhaba."), ev(0, 2000, "{\\pos(10,10)}DÜKKÂN")];
        let r = compare(&src, &cand, &CompareOptions { shift_ms: Some(0), ..Default::default() });
        assert_eq!((r.src_count, r.cand_count), (1, 1));
        assert_eq!(r.src_skipped.typeset, 2);
        let r2 = compare(&src, &cand, &CompareOptions { shift_ms: Some(0), include_typeset: true, ..Default::default() });
        assert_eq!((r2.src_count, r2.cand_count), (3, 2));
        let mut s2 = vec![ev(0, 2000, "La la la"), ev(3000, 5000, "Hi.")];
        s2[0].style = "OP".into();
        let mut c2 = vec![ev(3000, 5000, "Selam."), ev(0, 2000, "La la la")];
        c2[1].style = "Karaoke".into();
        let r = compare(&s2, &c2, &CompareOptions { shift_ms: Some(0), skip_styles_src: "^OP$".into(), skip_styles_cand: "Karaoke|OP|ED".into(), ..Default::default() });
        assert_eq!((r.src_count, r.cand_count), (1, 1));
        assert_eq!(r.counts()["EKSİK"] + r.counts()["FAZLA"], 0);
    }

    #[test]
    fn reports_and_marked_copy() {
        let (src, cand) = make_pair(2350, 160, 7);
        let r = compare(&src, &cand, &CompareOptions::default());
        let h = to_html(&r, "kaynak_en.srt", "aday_tr.ass", false);
        assert!(h.contains("<!doctype html>") && h.contains("EKSİK (1)") && h.contains("+2.35 sn (aday kaynaktan GEÇ)") && h.contains("Çeviri: Osi"));
        assert!(to_html(&r, "a", "b", true).len() < h.len() / 3);
        let c = to_csv(&r, false);
        assert!(c.starts_with('\u{feff}'));
        assert!(c.matches("\r\n").count() > 150);
        assert_eq!(to_csv(&r, true).matches("\r\n").count(), 1 + r.flagged().len());
        let mut doc = Script::new();
        doc.events = cand.clone();
        let m = marked_document(&doc, &r);
        assert_eq!(m.events.len(), cand.len() + 1);
        let added = m.events.last().unwrap();
        assert!(added.comment && added.text.starts_with("{QC: EKSİK}"));
        // the missing source line lands on the candidate's time line
        assert!((added.start - (src[10].start + 2350)).abs() <= 5);
        assert!(m.events.iter().any(|e| e.text.starts_with("{QC: ZAMAN +0.90 sn}")));
        // untouched lines are identical, the original document is not modified
        assert_eq!(doc.events, cand);
        assert_eq!(m.events.iter().filter(|e| e.text.starts_with("{QC:")).count(), r.flags_by_cand_row().len() + 1);
    }

    #[test]
    fn plain_and_visible_text() {
        assert_eq!(plain_of("{\\i1}Merhaba{\\i0}\\Nnasılsın?"), "Merhaba nasılsın?");
        assert_eq!(plain_of("{\\p1}m 0 0 l 10 0{\\p0}Yazı"), "Yazı");
        assert_eq!(visible_len("Şafak  sökmeden"), 13);
        assert_eq!(visible_len("c\u{0327}"), 1);
        assert_eq!(numbers_in("It's 1,060 yen at 10:30"), ["1030", "1060"].iter().map(|s| s.to_string()).collect());
    }
}
