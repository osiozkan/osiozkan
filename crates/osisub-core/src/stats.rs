//! Betik istatistikleri.
use crate::ass::Script;
use crate::{qc, tags};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub lines: usize,
    pub comments: usize,
    pub words: usize,
    pub chars: usize,
    pub total_duration_ms: i64,
    pub span_ms: i64,
    pub avg_duration_ms: f64,
    pub avg_cps: f64,
    pub max_cps: f64,
    /// KSS dağılımı: 0-5, 6-10, 11-15, 16-20, 21-25, 26+ (satır sayıları).
    pub cps_histogram: [usize; 6],
    pub styles: BTreeMap<String, usize>,
    pub actors: BTreeMap<String, usize>,
}

pub fn compute(script: &Script) -> Stats {
    let mut st = Stats::default();
    let (mut first, mut last) = (i64::MAX, 0i64);
    let mut cps_sum = 0.0;
    let mut cps_n = 0usize;
    for d in script.dialogues() {
        if d.comment {
            st.comments += 1;
            continue;
        }
        st.lines += 1;
        let vis = tags::visible_text(&d.text);
        st.words += vis.split_whitespace().count();
        st.chars += tags::visible_len(&d.text);
        st.total_duration_ms += d.duration().max(0);
        first = first.min(d.start_time);
        last = last.max(d.end_time);
        *st.styles.entry(d.style.clone()).or_default() += 1;
        if !d.actor.is_empty() {
            *st.actors.entry(d.actor.clone()).or_default() += 1;
        }
        let c = qc::cps(d);
        if c.is_finite() && !vis.trim().is_empty() {
            cps_sum += c;
            cps_n += 1;
            st.max_cps = st.max_cps.max(c);
            let b = match c as usize {
                0..=5 => 0,
                6..=10 => 1,
                11..=15 => 2,
                16..=20 => 3,
                21..=25 => 4,
                _ => 5,
            };
            st.cps_histogram[b] += 1;
        }
    }
    if st.lines > 0 {
        st.avg_duration_ms = st.total_duration_ms as f64 / st.lines as f64;
        st.span_ms = last - first;
    }
    if cps_n > 0 {
        st.avg_cps = cps_sum / cps_n as f64;
    }
    st
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Dialogue;

    #[test]
    fn basic_stats() {
        let s = Script::from_dialogues(vec![
            Dialogue { start_time: 0, end_time: 2000, text: "iki kelime".into(), actor: "Ali".into(), ..Default::default() },
            Dialogue { start_time: 3000, end_time: 4000, text: "{\\i1}tek{\\i0}".into(), ..Default::default() },
            Dialogue { start_time: 5000, end_time: 6000, text: "yorum".into(), comment: true, ..Default::default() },
        ]);
        let st = compute(&s);
        assert_eq!((st.lines, st.comments, st.words), (2, 1, 3));
        assert_eq!(st.span_ms, 4000);
        assert_eq!(st.actors["Ali"], 1);
        assert!(st.avg_cps > 2.0 && st.avg_cps < 4.0, "{}", st.avg_cps);
        assert_eq!(st.cps_histogram.iter().sum::<usize>(), 2);
    }
}
