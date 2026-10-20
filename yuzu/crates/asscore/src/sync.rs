//! Piecewise sync to speech (Osi's bsync) or to a correctly timed reference subtitle (the bsnap role).
//!
//! Rules (as bsync):
//! * The offset is NOT constant through an episode.
//! * It only changes at real scene breaks (≥ 4 s gap in the subtitles).
//! * Screen text (ending in `\N \N \N`) and signs (`\pos`, `\move`, drawings) are no anchors, but
//!   move with their segment.
//! * OP/ED vocals are suppressed (singing must not count as speech).
//! * No line goes before 0 or past the end of the audio.
//! * Only start/end times change.
//!
//! Pipeline: speech activity (10 ms; from audio or from a reference's dialogue coverage) →
//! OP/ED suppression (OP/ED lines and uninterrupted "speech" longer than `oped_max_ms`) →
//! segments at gaps ≥ `scene_gap_ms`, blocks at gaps ≥ `block_gap_ms` → coarse offset per block
//! (cross-correlation, ±`coarse_range_ms`) → fine alignment by dynamic programming: one offset
//! per segment within ±`fine_range_ms` of its block's, score = anchor overlap with speech
//! (silence penalised), offset changes cost `change_penalty`, segments may not swap order →
//! confidence = best score minus the best score ≥ 300 ms away, per anchor ms.

use crate::dsp;
use crate::model::Event;
use regex::Regex;

pub const RES: i64 = 10;

#[derive(Clone, Debug)]
pub struct SyncOptions {
    pub coarse_range_ms: i64,
    pub fine_range_ms: i64,
    pub step_ms: i64,
    pub scene_gap_ms: i64,
    pub block_gap_ms: i64,
    /// 1 ms of offset change costs this many ms of overlap.
    pub change_penalty: f64,
    /// Penalty per ms of a line over silence.
    pub silence_weight: f64,
    /// Gaussian smoothing (σ, ms) of the activity: the overlap plateau becomes a peak whose
    /// centre balances lead-in and lead-out.
    pub blur_ms: i64,
    pub oped_regex: String,
    /// Uninterrupted activity longer than this is treated as a song (0 = off).
    pub oped_max_ms: i64,
    /// Only these rows (None: the whole document).
    pub rows: Option<Vec<usize>>,
}

impl Default for SyncOptions {
    fn default() -> Self {
        SyncOptions {
            coarse_range_ms: 60_000, fine_range_ms: 8_000, step_ms: 40, scene_gap_ms: 4_000, block_gap_ms: 60_000,
            change_penalty: 0.15, silence_weight: 0.3, blur_ms: 150,
            oped_regex: r"(?i)(^|[^a-z])(op|ed|opening|ending|song|şarkı|insert|karaoke)([^a-z]|$)".into(),
            oped_max_ms: 45_000, rows: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Segment {
    pub rows: Vec<usize>,
    pub start: i64,
    pub end: i64,
    pub anchors: Vec<usize>,
    pub anchor_ms: i64,
    pub block: usize,
    pub shift: i64,
    pub confidence: f64,
    pub notes: Vec<String>,
}

pub fn is_screen_text(e: &Event) -> bool {
    static R: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    R.get_or_init(|| Regex::new(r"(\\N\s*){3}$").unwrap()).is_match(e.text.trim_end())
}

pub fn is_sign(e: &Event) -> bool {
    static R: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    R.get_or_init(|| Regex::new(r"\\(pos|move|p[1-9])").unwrap()).is_match(&e.text)
}

pub fn is_oped(e: &Event, rx: &Regex) -> bool {
    rx.is_match(&e.style) || rx.is_match(&e.actor) || rx.is_match(&e.effect)
}

fn rx_of(p: &str) -> Regex { Regex::new(if p.is_empty() { "$^" } else { p }).unwrap_or_else(|_| Regex::new("$^").unwrap()) }

/// Activity (10 ms) covering the given spans.
pub fn activity_from_segments(segs: &[(i64, i64)], total_ms: i64) -> Vec<f32> {
    let n = (total_ms / RES + 1).max(1) as usize;
    let mut a = vec![0f32; n];
    for &(s, e) in segs {
        let (s, e) = (s.div_euclid(RES).max(0) as usize, e.div_euclid(RES).clamp(0, n as i64) as usize);
        for v in a.iter_mut().take(e).skip(s) { *v = 1.0; }
    }
    a
}

/// A reference subtitle's dialogue coverage, used instead of speech (signs, screen text and
/// OP/ED lines of the reference do not count).
pub fn activity_from_reference(reference: &[Event], total_ms: i64, oped_regex: &str) -> Vec<f32> {
    let rx = rx_of(oped_regex);
    let segs: Vec<(i64, i64)> = reference.iter().filter(|e| !e.comment && !is_screen_text(e) && !is_sign(e) && !is_oped(e, &rx)).map(|e| (e.start, e.end)).collect();
    activity_from_segments(&segs, total_ms)
}

/// Length of the time axis in reference mode (instead of the audio's end).
pub fn reference_total_ms(reference: &[Event], events: &[Event], margin_ms: i64) -> i64 {
    reference.iter().chain(events).map(|e| e.end).max().unwrap_or(0) + margin_ms
}

/// Remove OP/ED ranges and overlong uninterrupted runs (songs) from the activity.
pub fn suppress(act: &[f32], ranges: &[(i64, i64)], max_run_ms: i64) -> (Vec<f32>, Vec<(i64, i64, String)>) {
    let mut a = act.to_vec();
    let mut removed = Vec::new();
    for &(s, e) in ranges {
        let (s0, e0) = ((s / RES).max(0) as usize, ((e / RES).max(0) as usize).min(a.len()));
        for v in a.iter_mut().take(e0).skip(s0) { *v = 0.0; }
        removed.push((s, e, "OP/ED satırları".to_string()));
    }
    if max_run_ms > 0 {
        let n = a.len();
        let gap_ok = (300 / RES) as usize;
        let mut i = 0;
        while i < n {
            if a[i] <= 0.5 { i += 1; continue; }
            let (mut j, mut last) = (i, i);
            while j < n {
                if a[j] > 0.5 { last = j; } else if j - last > gap_ok { break; }
                j += 1;
            }
            if ((last - i + 1) as i64) * RES >= max_run_ms {
                for v in a.iter_mut().take(last + 1).skip(i) { *v = 0.0; }
                removed.push((i as i64 * RES, (last as i64 + 1) * RES, "kesintisiz uzun ses (şarkı?)".to_string()));
            }
            i = last + 1;
        }
    }
    (a, removed)
}

pub fn build_segments(events: &[Event], rows: &[usize], o: &SyncOptions) -> Vec<Segment> {
    let rx = rx_of(&o.oped_regex);
    let mut items: Vec<usize> = rows.iter().copied().filter(|&r| r < events.len() && !events[r].comment).collect();
    items.sort_by_key(|&r| (events[r].start, events[r].end));
    let mut segs: Vec<Segment> = Vec::new();
    let (mut cur, mut cur_end): (Vec<usize>, i64) = (Vec::new(), 0);
    for r in items {
        let e = &events[r];
        if !cur.is_empty() && e.start - cur_end >= o.scene_gap_ms {
            segs.push(Segment { start: events[cur[0]].start, end: cur_end, rows: std::mem::take(&mut cur), ..Default::default() });
        }
        cur_end = if cur.is_empty() { e.end } else { cur_end.max(e.end) };
        cur.push(r);
    }
    if !cur.is_empty() { segs.push(Segment { start: events[cur[0]].start, end: cur_end, rows: cur, ..Default::default() }); }
    let mut block = 0;
    for i in 0..segs.len() {
        if i > 0 && segs[i].start - segs[i - 1].end >= o.block_gap_ms { block += 1; }
        let s = &mut segs[i];
        s.block = block;
        s.anchors = s.rows.iter().copied().filter(|&r| !(is_screen_text(&events[r]) || is_sign(&events[r]) || is_oped(&events[r], &rx))).collect();
        s.anchor_ms = s.anchors.iter().map(|&r| events[r].end - events[r].start).sum();
    }
    segs
}

fn coarse(events: &[Event], segs: &[Segment], speech_w: &[f64], o: &SyncOptions) -> Vec<i64> {
    let n = speech_w.len();
    let r = (o.coarse_range_ms / RES) as usize;
    let blocks = segs.iter().map(|s| s.block).max().map(|b| b + 1).unwrap_or(0);
    (0..blocks).map(|b| {
        let mut sub = vec![0f64; n];
        let mut any = false;
        for s in segs.iter().filter(|s| s.block == b) {
            for &row in &s.anchors {
                let e = &events[row];
                let (a, z) = ((e.start / RES).max(0) as usize, ((e.end / RES).max(0) as usize).min(n));
                for v in sub.iter_mut().take(z).skip(a) { *v = 1.0; any = true; }
            }
        }
        if !any { return 0; }
        let c = dsp::xcorr(&sub, speech_w, r);
        let k = c.iter().enumerate().fold((0usize, f64::MIN), |acc, (i, &v)| if v > acc.1 { (i, v) } else { acc }).0;
        (k as i64 - r as i64) * RES
    }).collect()
}

fn scores(events: &[Event], seg: &Segment, cands: &[i64], cum: &[f64], n: usize) -> Vec<f64> {
    let mut sc = vec![0f64; cands.len()];
    if seg.anchors.is_empty() { return sc; }
    for (k, &c) in cands.iter().enumerate() {
        let off = c.div_euclid(RES);
        let mut tot = 0.0;
        for &row in &seg.anchors {
            let e = &events[row];
            let a = (e.start.div_euclid(RES) + off).clamp(0, n as i64) as usize;
            let b = (e.end.div_euclid(RES) + off).clamp(0, n as i64) as usize;
            tot += cum[b] - cum[a];
        }
        sc[k] = tot * RES as f64;
    }
    sc
}

/// Plan the offsets. Returns the segments (each with its proposed `shift`) and what was
/// suppressed as OP/ED. `progress` gets 0..1.
pub fn plan(events: &[Event], speech_act: &[f32], o: &SyncOptions, mut progress: impl FnMut(f64)) -> (Vec<Segment>, Vec<(i64, i64, String)>) {
    let rows: Vec<usize> = o.rows.clone().unwrap_or_else(|| (0..events.len()).collect());
    let mut segs = build_segments(events, &rows, o);
    if segs.is_empty() { return (segs, vec![]); }
    let rx = rx_of(&o.oped_regex);
    let oped: Vec<(i64, i64)> = rows.iter().filter(|&&r| r < events.len() && !events[r].comment && is_oped(&events[r], &rx)).map(|&r| (events[r].start, events[r].end)).collect();
    let (act, removed) = suppress(speech_act, &oped, o.oped_max_ms);
    let mut act: Vec<f64> = act.iter().map(|&v| v as f64).collect();
    if o.blur_ms > 0 { act = dsp::convolve_same(&act, &dsp::gaussian(o.blur_ms as f64 / RES as f64, true)); }
    let speech_w: Vec<f64> = act.iter().map(|&v| v - o.silence_weight * (1.0 - v)).collect();
    let n = speech_w.len();
    let mut cum = vec![0f64; n + 1];
    for i in 0..n { cum[i + 1] = cum[i] + speech_w[i]; }
    let coarse = coarse(events, &segs, &speech_w, o);
    let offs: Vec<i64> = (0..).map(|k| -o.fine_range_ms + k * o.step_ms).take_while(|&v| v <= o.fine_range_ms).collect();
    let cand: Vec<Vec<i64>> = segs.iter().map(|s| offs.iter().map(|d| coarse[s.block] + d).collect()).collect();
    let mut sc: Vec<Vec<f64>> = Vec::with_capacity(segs.len());
    for (i, s) in segs.iter().enumerate() {
        sc.push(scores(events, s, &cand[i], &cum, n));
        progress(0.5 * (i + 1) as f64 / segs.len() as f64);
    }
    // dynamic programming: best total score minus offset-change penalties, order preserved
    let lam = o.change_penalty;
    let mut best = sc[0].clone();
    let mut back: Vec<Vec<usize>> = Vec::new();
    for i in 1..segs.len() {
        let (pc, cc) = (&cand[i - 1], &cand[i]);
        let gap = segs[i].start - segs[i - 1].end;
        let mut nb = vec![f64::NEG_INFINITY; cc.len()];
        let mut arg = vec![0usize; cc.len()];
        for (j, &c) in cc.iter().enumerate() {
            for (p, &pv) in pc.iter().enumerate() {
                if c < pv - gap || best[p] == f64::NEG_INFINITY { continue; }
                let v = best[p] - lam * (c - pv).abs() as f64;
                if v > nb[j] { nb[j] = v; arg[j] = p; }
            }
            nb[j] += sc[i][j];
        }
        best = nb;
        back.push(arg);
        progress(0.5 + 0.5 * i as f64 / segs.len() as f64);
    }
    let mut idx = vec![best.iter().enumerate().fold((0usize, f64::NEG_INFINITY), |a, (i, &v)| if v > a.1 { (i, v) } else { a }).0];
    for arg in back.iter().rev() { let last = *idx.last().unwrap(); idx.push(arg[last]); }
    idx.reverse();
    for (i, s) in segs.iter_mut().enumerate() {
        s.shift = cand[i][idx[i]];
        if !s.anchors.is_empty() && s.anchor_ms > 0 {
            let far = cand[i].iter().zip(&sc[i]).filter(|(c, _)| (*c - s.shift).abs() >= 300).map(|(_, v)| *v).fold(f64::NEG_INFINITY, f64::max);
            let second = if far.is_finite() { far } else { sc[i].iter().cloned().fold(f64::INFINITY, f64::min) };
            s.confidence = ((sc[i][idx[i]] - second) / s.anchor_ms as f64).max(0.0);
        } else {
            s.notes.push("çapa satırı yok — komşu parçaları izledi".into());
        }
        if (s.shift - coarse[s.block]).abs() >= o.fine_range_ms - o.step_ms {
            s.notes.push("kayma ince aralığın sınırında — aralığı büyütmek gerekebilir".into());
        }
    }
    progress(1.0);
    (segs, removed)
}

/// New times per row from the segment offsets, kept within 0..total_ms.
/// Returns ((row, start, end) for moved rows, warnings).
pub fn changes(events: &[Event], segs: &[Segment], total_ms: i64) -> (Vec<(usize, i64, i64)>, Vec<String>) {
    let mut out = Vec::new();
    let mut clamped = 0;
    for s in segs.iter().filter(|s| s.shift != 0) {
        for &r in &s.rows {
            let e = &events[r];
            let (mut st, mut en) = (e.start + s.shift, e.end + s.shift);
            if st < 0 || en > total_ms {
                clamped += 1;
                st = st.max(0).min((total_ms - 10).max(0));
                en = en.max(st + 10).min(total_ms);
            }
            out.push((r, st, en));
        }
    }
    let warns = if clamped > 0 { vec![format!("{} satır 0 ile ses sonu arasına kırpıldı", clamped)] } else { vec![] };
    (out, warns)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 { self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0 }
        fn range(&mut self, lo: i64, hi: i64) -> i64 { lo + (self.next() % ((hi - lo) as u64)) as i64 }
    }

    fn make_scenes(seed: u64, n_scenes: usize, per: usize, start: i64, gap: i64) -> (Vec<Vec<(i64, i64)>>, i64) {
        let mut rng = Rng(seed.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(7));
        let mut t = start;
        let mut scenes = Vec::new();
        for _ in 0..n_scenes {
            let mut lines = Vec::new();
            for _ in 0..per {
                let d = rng.range(800, 3000);
                lines.push((t, t + d));
                t += d + rng.range(150, 900);
            }
            scenes.push(lines);
            t += gap;
        }
        (scenes, t + 5000)
    }

    fn subs_for(scenes: &[Vec<(i64, i64)>], shifts: &[i64]) -> Vec<Event> {
        let mut evs = Vec::new();
        for (sc, sh) in scenes.iter().zip(shifts) {
            for &(a, b) in sc { evs.push(Event::new(a - 100 - sh, b + 200 - sh, "Default", "satır")); }
        }
        evs
    }

    #[test]
    fn piecewise_shifts_recovered() {
        let (scenes, total) = make_scenes(3, 6, 10, 5000, 8000);
        let all: Vec<(i64, i64)> = scenes.iter().flatten().copied().collect();
        let act = activity_from_segments(&all, total);
        let shifts = [1200, 1200, -800, -800, 2500, 300];
        let mut evs = subs_for(&scenes, &shifts);
        let a0 = scenes[2][3].0;
        evs.push(Event::new(a0 + 800, a0 + 3800, "Default", "{\\an8}Dükkan\\N \\N \\N"));
        evs.push(Event::new(a0 + 900, a0 + 3000, "Default", "{\\pos(100,100)}TABELA"));
        evs.sort_by_key(|e| e.start);
        let (segs, _) = plan(&evs, &act, &SyncOptions::default(), |_| {});
        assert_eq!(segs.len(), 6);
        for (sg, sh) in segs.iter().zip(shifts) {
            assert!((sg.shift - (sh - 50)).abs() < 120, "{} vs {}", sg.shift, sh);
            assert!(sg.confidence > 0.02, "{}", sg.confidence);
        }
        assert_eq!(segs[2].rows.len(), 12);
        assert_eq!(segs[2].anchors.len(), 10);
        let (ch, warns) = changes(&evs, &segs, total);
        assert_eq!(ch.len(), evs.len());
        assert!(warns.is_empty());
    }

    #[test]
    fn scene_gap_rule_and_order() {
        let (scenes, total) = make_scenes(3, 2, 10, 5000, 2500);
        let all: Vec<(i64, i64)> = scenes.iter().flatten().copied().collect();
        let (segs, _) = plan(&subs_for(&scenes, &[300, -300]), &activity_from_segments(&all, total), &SyncOptions::default(), |_| {});
        assert_eq!(segs.len(), 1);
        let (scenes, total) = make_scenes(3, 3, 10, 5000, 4200);
        let all: Vec<(i64, i64)> = scenes.iter().flatten().copied().collect();
        let (segs, _) = plan(&subs_for(&scenes, &[0, 0, 0]), &activity_from_segments(&all, total), &SyncOptions::default(), |_| {});
        for w in segs.windows(2) { assert!(w[0].end + w[0].shift <= w[1].start + w[1].shift); }
    }

    #[test]
    fn oped_and_song_suppression() {
        let (scenes, total) = make_scenes(3, 3, 10, 95_000, 8000);
        let mut all: Vec<(i64, i64)> = scenes.iter().flatten().copied().collect();
        all.push((1000, 90_000));
        let act = activity_from_segments(&all, total);
        let mut evs = subs_for(&scenes, &[700, 700, 700]);
        for i in 0..6 { let mut e = Event::new(2000 + 5000 * i, 6000 + 5000 * i, "OP", "la la"); e.style = "OP".into(); evs.push(e); }
        evs.sort_by_key(|e| e.start);
        let (segs, removed) = plan(&evs, &act, &SyncOptions::default(), |_| {});
        assert!(removed.iter().any(|r| r.2.contains("OP/ED")));
        assert!(removed.iter().any(|r| r.2.contains("şarkı")));
        let op = segs.iter().find(|s| s.rows.iter().any(|&r| evs[r].style == "OP")).unwrap();
        assert!(op.anchors.is_empty());
        for s in segs.iter().filter(|s| !s.anchors.is_empty()) { assert!((s.shift - 650).abs() < 120, "{}", s.shift); }
        let (a, rem) = suppress(&vec![1.0; 10_000], &[], 45_000);
        assert_eq!(a.iter().sum::<f32>(), 0.0);
        assert_eq!(rem.len(), 1);
    }

    #[test]
    fn clamp_and_reference_mode() {
        let evs = vec![Event::new(100, 900, "Default", "a"), Event::new(9800, 10_500, "Default", "b")];
        let sg = vec![Segment { rows: vec![0], start: 100, end: 900, anchors: vec![0], shift: -500, ..Default::default() },
                      Segment { rows: vec![1], start: 9800, end: 10_500, anchors: vec![1], shift: 300, ..Default::default() }];
        let (ch, warns) = changes(&evs, &sg, 10_000);
        assert_eq!(ch[0], (0, 0, 400));
        assert!(ch[1].2 <= 10_000);
        assert!(!warns.is_empty());
        // reference mode: a correctly timed reference instead of speech
        let (scenes, _) = make_scenes(5, 4, 8, 5000, 9000);
        let reference: Vec<Event> = scenes.iter().flatten().map(|&(a, b)| Event::new(a, b, "Default", "ref")).collect();
        let shifts = [900, 900, -600, 1500];
        let evs = subs_for(&scenes, &shifts);
        let total = reference_total_ms(&reference, &evs, 60_000);
        let act = activity_from_reference(&reference, total, &SyncOptions::default().oped_regex);
        let (segs, _) = plan(&evs, &act, &SyncOptions { oped_max_ms: 0, ..Default::default() }, |_| {});
        assert_eq!(segs.len(), 4);
        for (s, sh) in segs.iter().zip(shifts) { assert!((s.shift - (sh - 50)).abs() < 120, "{} vs {}", s.shift, sh); }
    }
}
