//! Karaoke line model: syllable splitting, joining, tap-timing and kana/romaji syllabification.

use crate::tags::{self, Syllable};

#[derive(Clone, Debug, PartialEq)]
pub struct KaraLine {
    /// Override blocks (e.g. `{\an8}`) that precede the first syllable.
    pub prefix: String,
    pub syls: Vec<Syllable>,
}

impl KaraLine {
    pub fn from_text(text: &str) -> KaraLine {
        let mut syls = tags::karaoke_syllables(text);
        let mut prefix = String::new();
        if syls.len() > 1 && syls[0].duration_cs == 0 && !syls[0].text.is_empty() && tags::strip_tags(&syls[0].text).is_empty() {
            prefix = syls.remove(0).text;
        }
        if syls.is_empty() {
            syls.push(Syllable { duration_cs: 0, kind: "kf".into(), text: tags::strip_tags(text), extra_tags: String::new() });
        }
        KaraLine { prefix, syls }
    }

    pub fn to_text(&self) -> String {
        format!("{}{}", self.prefix, tags::build_karaoke(&self.syls))
    }

    /// Plain text with `|` between syllables (editing aid).
    pub fn markup(&self) -> String {
        self.syls.iter().map(|s| tags::strip_tags(&s.text)).collect::<Vec<_>>().join("|")
    }

    /// Re-split according to a `|` markup; durations of unchanged syllables are kept, others share their parent's time.
    pub fn apply_markup(&mut self, markup: &str) {
        let total = self.total_cs();
        let kind = self.syls.first().map(|s| s.kind.clone()).unwrap_or_else(|| "kf".into());
        let parts: Vec<&str> = markup.split('|').collect();
        let old: Vec<String> = self.syls.iter().map(|s| tags::strip_tags(&s.text)).collect();
        if parts.len() == old.len() && parts.iter().zip(&old).all(|(a, b)| a == b) { return; }
        // Map old syllable boundaries (in chars) onto the new text, assuming the visible text is unchanged.
        let flat_old: String = old.concat();
        let flat_new: String = parts.concat();
        let mut new_syls: Vec<Syllable> = parts.iter().map(|p| Syllable { duration_cs: 0, kind: kind.clone(), text: p.to_string(), extra_tags: String::new() }).collect();
        if flat_old == flat_new {
            // time at each char boundary from the old layout, linear inside a syllable
            let mut bounds: Vec<(usize, i64)> = vec![(0, 0)];
            let (mut c, mut t) = (0usize, 0i64);
            for (i, s) in self.syls.iter().enumerate() { c += old[i].chars().count(); t += s.duration_cs; bounds.push((c, t)); }
            let time_at = |pos: usize| -> i64 {
                for w in bounds.windows(2) {
                    if pos >= w[0].0 && pos <= w[1].0 {
                        let span = (w[1].0 - w[0].0).max(1) as i64;
                        return w[0].1 + (w[1].1 - w[0].1) * (pos - w[0].0) as i64 / span;
                    }
                }
                total
            };
            let mut pos = 0usize;
            for s in new_syls.iter_mut() {
                let n = s.text.chars().count();
                s.duration_cs = time_at(pos + n) - time_at(pos);
                pos += n;
            }
            for (k, s) in new_syls.iter_mut().enumerate() { if let Some(o) = self.syls.get(k) { if o.text == s.text { s.extra_tags = o.extra_tags.clone(); } } }
        } else {
            let n = new_syls.len().max(1) as i64;
            for s in new_syls.iter_mut() { s.duration_cs = total / n; }
        }
        self.syls = new_syls;
        self.fit_total(total);
    }

    pub fn total_cs(&self) -> i64 {
        self.syls.iter().map(|s| s.duration_cs).sum()
    }

    /// Plain text of syllable `i` (no override blocks).
    pub fn plain(&self, i: usize) -> String {
        tags::strip_tags(&self.syls[i].text)
    }

    /// Split syllable `i` after `chars` characters of its plain text; duration split proportionally (at least 1 cs each when possible).
    pub fn split(&mut self, i: usize, chars: usize) {
        let s = self.syls[i].clone();
        let all: Vec<char> = s.text.chars().collect();
        let mut vis = 0usize;
        let mut in_block = false;
        let mut cut = all.len();
        for (k, c) in all.iter().enumerate() {
            if *c == '{' { in_block = true; }
            if !in_block {
                if vis == chars { cut = k; break; }
                vis += 1;
            }
            if *c == '}' { in_block = false; }
        }
        if chars == 0 || cut >= all.len() { return; }
        let a: String = all[..cut].iter().collect();
        let b: String = all[cut..].iter().collect();
        let total_vis = (vis + b.chars().filter(|_| true).count()).max(1) as i64;
        let first_vis = chars as i64;
        let mut d1 = (s.duration_cs * first_vis + total_vis / 2) / total_vis;
        if s.duration_cs >= 2 { d1 = d1.clamp(1, s.duration_cs - 1); }
        let d2 = s.duration_cs - d1;
        self.syls[i] = Syllable { duration_cs: d1, kind: s.kind.clone(), text: a, extra_tags: s.extra_tags.clone() };
        self.syls.insert(i + 1, Syllable { duration_cs: d2, kind: s.kind, text: b, extra_tags: String::new() });
    }

    /// Merge syllable `i+1` into `i`.
    pub fn join(&mut self, i: usize) {
        if i + 1 >= self.syls.len() { return; }
        let n = self.syls.remove(i + 1);
        self.syls[i].duration_cs += n.duration_cs;
        self.syls[i].text.push_str(&n.text);
    }

    /// Move the boundary between `i` and `i+1` by `delta_cs`, keeping both ≥ 0.
    pub fn move_boundary(&mut self, i: usize, delta_cs: i64) {
        if i + 1 >= self.syls.len() { return; }
        let d = delta_cs.clamp(-self.syls[i].duration_cs, self.syls[i + 1].duration_cs);
        self.syls[i].duration_cs += d;
        self.syls[i + 1].duration_cs -= d;
    }

    pub fn set_kind(&mut self, kind: &str) {
        for s in &mut self.syls { s.kind = kind.to_string(); }
    }

    /// Scale all durations so the total equals `total_cs`.
    pub fn fit_total(&mut self, total_cs: i64) {
        let cur = self.total_cs();
        if cur <= 0 || self.syls.is_empty() {
            let n = self.syls.len().max(1) as i64;
            let mut used = 0;
            for (k, s) in self.syls.iter_mut().enumerate() {
                let d = if k as i64 == n - 1 { total_cs - used } else { total_cs / n };
                s.duration_cs = d; used += d;
            }
            return;
        }
        let mut used = 0;
        let n = self.syls.len();
        for (k, s) in self.syls.iter_mut().enumerate() {
            let d = if k == n - 1 { total_cs - used } else { s.duration_cs * total_cs / cur };
            s.duration_cs = d.max(0); used += s.duration_cs;
        }
    }

    /// Equal duration for each syllable.
    pub fn distribute_evenly(&mut self, total_cs: i64) {
        for s in &mut self.syls { s.duration_cs = 0; }
        self.fit_total(total_cs);
    }

    /// Build durations from tap times (ms, absolute) given the line start; the last tap closes the final syllable.
    pub fn apply_taps(&mut self, line_start_ms: i64, taps_ms: &[i64]) {
        let mut prev = line_start_ms;
        for (k, s) in self.syls.iter_mut().enumerate() {
            match taps_ms.get(k) {
                Some(t) => { s.duration_cs = ((*t - prev) as f64 / 10.0).round().max(0.0) as i64; prev = *t; }
                None => break,
            }
        }
    }

    /// Re-split every syllable per character (CJK / per-letter).
    pub fn split_per_char(&mut self) {
        self.syls = tags::split_syllables_by_char(&self.syls);
    }

    /// Re-split the whole line's plain text by a strategy; keeps total duration.
    pub fn resplit(&mut self, strategy: Split) {
        let total = self.total_cs();
        let plain: String = self.syls.iter().map(|s| tags::strip_tags(&s.text)).collect();
        let kind = self.syls.first().map(|s| s.kind.clone()).unwrap_or_else(|| "kf".into());
        let pieces: Vec<String> = match strategy {
            Split::Char => plain.chars().map(|c| c.to_string()).collect(),
            Split::Word => split_keep_spaces(&plain),
            Split::Romaji => romaji_syllables(&plain),
            Split::Kana => kana_syllables(&plain),
        };
        self.syls = pieces.into_iter().map(|t| Syllable { duration_cs: 0, kind: kind.clone(), text: t, extra_tags: String::new() }).collect();
        if self.syls.is_empty() { self.syls.push(Syllable { duration_cs: total, kind, text: String::new(), extra_tags: String::new() }); }
        // weight by visible char count (spaces weigh 0)
        let weights: Vec<i64> = self.syls.iter().map(|s| s.text.chars().filter(|c| !c.is_whitespace()).count() as i64).collect();
        let wsum: i64 = weights.iter().sum::<i64>().max(1);
        let mut used = 0; let n = self.syls.len();
        for (k, s) in self.syls.iter_mut().enumerate() {
            let d = if k == n - 1 { total - used } else { total * weights[k] / wsum };
            s.duration_cs = d; used += d;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Split { Char, Word, Romaji, Kana }

fn split_keep_spaces(s: &str) -> Vec<String> {
    // words with their trailing spaces attached: "ka ra o" → ["ka ", "ra ", "o"]
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut prev_space = false;
    for c in s.chars() {
        if prev_space && !c.is_whitespace() { out.push(std::mem::take(&mut cur)); }
        cur.push(c);
        prev_space = c.is_whitespace();
    }
    if !cur.is_empty() { out.push(cur); }
    out
}

fn is_small_kana(c: char) -> bool {
    matches!(c, 'ゃ' | 'ゅ' | 'ょ' | 'ぁ' | 'ぃ' | 'ぅ' | 'ぇ' | 'ぉ' | 'ャ' | 'ュ' | 'ョ' | 'ァ' | 'ィ' | 'ゥ' | 'ェ' | 'ォ')
}

/// Split Japanese kana text into morae: small ゃゅょ attach to the previous kana, ー and ん/ッ stand alone (っ attaches forward-visible as its own mora).
pub fn kana_syllables(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in s.chars() {
        if is_small_kana(c) || c == 'ー' || c == '〜' {
            if let Some(l) = out.last_mut() { if !l.chars().all(char::is_whitespace) { l.push(c); continue; } }
        }
        if c.is_whitespace() {
            if let Some(l) = out.last_mut() { l.push(c); continue; }
        }
        out.push(c.to_string());
    }
    out
}

/// Split romaji into morae: consonant cluster + vowel (kya, shi, tsu…), lone vowels, syllabic n, geminate consonants join the next syllable.
pub fn romaji_syllables(s: &str) -> Vec<String> {
    let cs: Vec<char> = s.chars().collect();
    let is_v = |c: char| matches!(c.to_ascii_lowercase(), 'a' | 'i' | 'u' | 'e' | 'o' | 'ā' | 'ī' | 'ū' | 'ē' | 'ō');
    let is_alpha = |c: char| c.is_alphabetic();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if !is_alpha(c) {
            // punctuation/space attaches to previous syllable
            match out.last_mut() { Some(l) => l.push(c), None => out.push(c.to_string()) }
            i += 1;
            continue;
        }
        let mut syl = String::new();
        // consonant run (max 3: tsu, shi, kya, chi, sh, ch…)
        let start = i;
        while i < cs.len() && is_alpha(cs[i]) && !is_v(cs[i]) {
            // syllabic n: 'n' not followed by a vowel/y
            if cs[i].to_ascii_lowercase() == 'n' && i == start {
                let next = cs.get(i + 1).copied();
                let nn = next.map(|n| !is_alpha(n) || (!is_v(n) && n.to_ascii_lowercase() != 'y')).unwrap_or(true);
                if nn { syl.push(cs[i]); i += 1; if cs.get(i).map(|c| c.to_ascii_lowercase()) == Some('\'') { syl.push('\''); i += 1; } break; }
            }
            syl.push(cs[i]);
            i += 1;
            // geminate: "kk", "tt", "ss", "pp" → close "k" as its own mora (っ)
            if syl.chars().count() == 1 && i < cs.len() && cs[i].to_ascii_lowercase() == syl.chars().next().unwrap().to_ascii_lowercase() && !is_v(cs[i]) && syl != "n" {
                break;
            }
            if syl.chars().count() >= 3 { break; }
        }
        if i < cs.len() && is_v(cs[i]) && !(syl.len() == 1 && syl.eq_ignore_ascii_case("n") && false) {
            if !(syl.to_ascii_lowercase() == "n" && cs.get(i).map(|c| !is_v(*c)).unwrap_or(false)) {
                syl.push(cs[i]);
                i += 1;
            }
        }
        // long vowel mark attaches ("ō" handled as vowel; "ou"/"aa" remain separate morae by design)
        if syl.is_empty() { syl.push(cs[i]); i += 1; }
        out.push(syl);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_join_roundtrip() {
        let mut k = KaraLine::from_text("{\\kf40}karaoke");
        k.split(0, 4);
        assert_eq!(k.syls.len(), 2);
        assert_eq!(k.syls[0].text, "kara");
        assert_eq!(k.total_cs(), 40);
        k.join(0);
        assert_eq!(k.to_text(), "{\\kf40}karaoke");
    }
    #[test]
    fn taps() {
        let mut k = KaraLine::from_text("{\\k0}a{\\k0}b{\\k0}c");
        k.apply_taps(1000, &[1200, 1500, 2000]);
        assert_eq!(k.syls.iter().map(|s| s.duration_cs).collect::<Vec<_>>(), vec![20, 30, 50]);
    }
    #[test]
    fn romaji() {
        assert_eq!(romaji_syllables("karaoke"), vec!["ka", "ra", "o", "ke"]);
        assert_eq!(romaji_syllables("shinjitsu"), vec!["shi", "n", "ji", "tsu"]);
        assert_eq!(romaji_syllables("kyou"), vec!["kyo", "u"]);
        assert_eq!(romaji_syllables("ka ra"), vec!["ka ", "ra"]);
    }
    #[test]
    fn kana() {
        assert_eq!(kana_syllables("きょう"), vec!["きょ", "う"]);
        assert_eq!(kana_syllables("らーめん"), vec!["らー", "め", "ん"]);
    }
    #[test]
    fn resplit_keeps_total() {
        let mut k = KaraLine::from_text("{\\k100}ka ra o ke");
        k.resplit(Split::Word);
        assert_eq!(k.syls.len(), 4);
        assert_eq!(k.total_cs(), 100);
    }
    #[test]
    fn prefix_and_markup() {
        let mut k = KaraLine::from_text("{\\an8}{\\k20}ka{\\k30}rao");
        assert_eq!(k.prefix, "{\\an8}");
        assert_eq!(k.markup(), "ka|rao");
        k.apply_markup("ka|ra|o");
        assert_eq!(k.syls.len(), 3);
        assert_eq!(k.total_cs(), 50);
        assert!(k.to_text().starts_with("{\\an8}{\\k20}ka"));
    }
    #[test]
    fn fit() {
        let mut k = KaraLine::from_text("{\\k10}a{\\k30}b");
        k.fit_total(80);
        assert_eq!(k.total_cs(), 80);
    }
}
