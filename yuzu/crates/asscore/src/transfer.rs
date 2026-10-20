//! Transfer from a reference subtitle (Osi's TR-EN merge): write the reference's lines (e.g. the
//! Turkish translation) into the matching lines of the document (e.g. the styled English .ass).
//!
//! Rules: the document owns styles/actors/positions; times stay its own (optionally the
//! reference's). A document line without a match is NEVER deleted — it stays and is reported.
//! A reference line without a host is still inserted as a new line and reported. Ambiguous,
//! multiple and weak matches are reported, not hidden.
//!
//! Match measure: shared time / the shorter line's duration. Each reference line goes to the
//! document line with the highest ratio; below `min_overlap` it has no host; if the second best
//! also passes `ambiguous` the match is ambiguous.
//!
//! Known limits: differently split lines (two EN, one TR) give one host only — the other is
//! reported as "no reference"; a large sync difference breaks matching (sync first); signs in a
//! separate style should be left out with `host_styles`.

use crate::model::Event;
use crate::tags;

#[derive(Clone, Debug)]
pub struct TransferOptions {
    pub min_overlap: f64,
    pub weak_below: f64,
    pub ambiguous: f64,
    /// None = every non-comment line can host.
    pub host_styles: Option<Vec<String>>,
    /// Keep leading override blocks of the host line (`{\an8}` …).
    pub keep_leading_tags: bool,
    /// Append the host's original text as a `{EN: …}` comment block.
    pub keep_original: bool,
    pub use_ref_timing: bool,
    pub insert_orphans: bool,
    /// None = style of the nearest host line.
    pub orphan_style: Option<String>,
    pub join_sep: String,
}

impl Default for TransferOptions {
    fn default() -> Self {
        TransferOptions { min_overlap: 0.30, weak_below: 0.60, ambiguous: 0.50, host_styles: None, keep_leading_tags: true, keep_original: false,
            use_ref_timing: false, insert_orphans: true, orphan_style: None, join_sep: "\\N".into() }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum IssueKind { Multi, Ambiguous, Weak, NoRef, Orphan, OrphanSkipped }

impl IssueKind {
    pub fn label(&self) -> &'static str {
        match self {
            IssueKind::Multi => "Birden çok satır tek satıra birleştirildi",
            IssueKind::Ambiguous => "Belirsiz: satır birden çok satırla çakışıyor",
            IssueKind::Weak => "Zayıf eşleşme (az çakışma)",
            IssueKind::NoRef => "Karşılıksız satır (olduğu gibi bırakıldı)",
            IssueKind::Orphan => "Ev sahibi yok: yeni satır olarak eklendi",
            IssueKind::OrphanSkipped => "Ev sahibi yok: eklenmedi",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Issue {
    pub kind: IssueKind,
    /// Row in the result.
    pub row: Option<usize>,
    /// Reference row.
    pub reference: Option<usize>,
    pub detail: String,
}

#[derive(Clone, Debug, Default)]
pub struct Plan {
    pub events: Vec<Event>,
    pub issues: Vec<Issue>,
    pub filled: usize,
    pub orphans: usize,
    pub no_ref: usize,
}

/// (ratio, shared ms) between two lines.
pub fn overlap_ratio(a: &Event, b: &Event) -> (f64, i64) {
    let ov = a.end.min(b.end) - a.start.max(b.start);
    if ov <= 0 { return (0.0, 0); }
    (ov as f64 / (a.end - a.start).min(b.end - b.start).max(1) as f64, ov)
}

/// Index for looking up the reference line that overlaps a document line most (grid column).
pub struct RefIndex {
    order: Vec<usize>,
    starts: Vec<i64>,
    max_dur: i64,
}

impl RefIndex {
    pub fn new(refs: &[Event]) -> RefIndex {
        let mut order: Vec<usize> = (0..refs.len()).filter(|&i| !refs[i].comment).collect();
        order.sort_by_key(|&i| refs[i].start);
        let starts = order.iter().map(|&i| refs[i].start).collect();
        let max_dur = refs.iter().map(|e| e.end - e.start).max().unwrap_or(0);
        RefIndex { order, starts, max_dur }
    }

    /// Best reference line for `[start, end)` shifted by `-offset` (offset = document later than the reference).
    pub fn best(&self, refs: &[Event], start: i64, end: i64, offset: i64) -> Option<(usize, f64)> {
        let q = Event::new(start - offset, end - offset, "", "");
        let lo = self.starts.partition_point(|&s| s < q.start - self.max_dur);
        let hi = self.starts.partition_point(|&s| s <= q.end);
        let mut best: Option<(usize, f64, i64)> = None;
        for &j in &self.order[lo..hi] {
            let (r, o) = overlap_ratio(&q, &refs[j]);
            if r > 0.0 && best.map(|b| (r, o) > (b.1, b.2)).unwrap_or(true) { best = Some((j, r, o)); }
        }
        best.map(|(j, r, _)| (j, r))
    }
}

fn leading_tags(text: &str) -> &str {
    let mut end = 0;
    let b = text.as_bytes();
    while end < b.len() && b[end] == b'{' {
        match text[end..].find('}') { Some(c) => end += c + 1, None => break }
    }
    &text[..end]
}

pub fn plan_transfer(hosts_in: &[Event], refs: &[Event], o: &TransferOptions) -> Plan {
    let eligible = |e: &Event| !e.comment && o.host_styles.as_ref().map(|s| s.contains(&e.style)).unwrap_or(true);
    let hosts: Vec<usize> = (0..hosts_in.len()).filter(|&i| eligible(&hosts_in[i])).collect();
    let mut by_start = hosts.clone();
    by_start.sort_by_key(|&i| hosts_in[i].start);
    let h_starts: Vec<i64> = by_start.iter().map(|&i| hosts_in[i].start).collect();
    let max_h = hosts.iter().map(|&i| hosts_in[i].end - hosts_in[i].start).max().unwrap_or(0);
    let mut assign: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    let mut orphans = Vec::new();
    let mut pre: Vec<(IssueKind, usize, usize, String)> = Vec::new();
    for (j, rf) in refs.iter().enumerate() {
        if rf.comment || tags::strip_tags(&rf.text).trim().is_empty() { continue; }
        let lo = h_starts.partition_point(|&s| s < rf.start - max_h);
        let hi = h_starts.partition_point(|&s| s <= rf.end);
        let mut scored: Vec<(f64, i64, usize)> = by_start[lo..hi].iter().filter_map(|&i| { let (r, ov) = overlap_ratio(&hosts_in[i], rf); (r > 0.0).then_some((r, ov, i)) }).collect();
        scored.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        if scored.is_empty() || scored[0].0 < o.min_overlap { orphans.push(j); continue; }
        let (r0, _, i0) = scored[0];
        assign.entry(i0).or_default().push(j);
        if scored.len() > 1 && scored[1].0 >= o.ambiguous {
            pre.push((IssueKind::Ambiguous, i0, j, format!("{} aday; en iyi oran %{}, ikinci %{}", scored.len(), (r0 * 100.0) as i64, (scored[1].0 * 100.0) as i64)));
        } else if r0 < o.weak_below {
            pre.push((IssueKind::Weak, i0, j, format!("çakışma oranı %{}", (r0 * 100.0) as i64)));
        }
    }
    // result lines: (event, origin) origin = Ok(host) / Err(orphan ref)
    let mut merged: Vec<(Event, Result<usize, usize>)> = Vec::new();
    for (i, ev) in hosts_in.iter().enumerate() {
        let mut ev = ev.clone();
        if let Some(js) = assign.get_mut(&i) {
            js.sort_by_key(|&j| refs[j].start);
            let body = js.iter().map(|&j| refs[j].text.as_str()).collect::<Vec<_>>().join(&o.join_sep);
            let lead = if o.keep_leading_tags { leading_tags(&ev.text).to_string() } else { String::new() };
            let mut text = lead + &body;
            if o.keep_original { text.push_str(&format!("{{EN: {}}}", tags::strip_tags(&ev.text).replace('{', "(").replace('}', ")"))); }
            ev.text = text;
            if o.use_ref_timing {
                ev.start = js.iter().map(|&j| refs[j].start).min().unwrap_or(ev.start);
                ev.end = js.iter().map(|&j| refs[j].end).max().unwrap_or(ev.end);
            }
        }
        merged.push((ev, Ok(i)));
    }
    if o.insert_orphans {
        let mut sorted = orphans.clone();
        sorted.sort_by_key(|&j| refs[j].start);
        for j in sorted {
            let rf = &refs[j];
            let style = o.orphan_style.clone().unwrap_or_else(|| {
                let k = h_starts.partition_point(|&s| s < rf.start);
                [k.wrapping_sub(1), k].iter().filter(|&&x| x < by_start.len()).map(|&x| by_start[x])
                    .min_by_key(|&i| (hosts_in[i].start - rf.start).abs()).map(|i| hosts_in[i].style.clone()).unwrap_or_else(|| "Default".into())
            });
            let pos = merged.iter().position(|(e, _)| !e.comment && e.start > rf.start).unwrap_or(merged.len());
            merged.insert(pos, (Event::new(rf.start, rf.end, &style, &rf.text), Err(j)));
        }
    }
    let row_of_host: std::collections::HashMap<usize, usize> = merged.iter().enumerate().filter_map(|(r, (_, o))| o.ok().map(|i| (i, r))).collect();
    let row_of_orphan: std::collections::HashMap<usize, usize> = merged.iter().enumerate().filter_map(|(r, (_, o))| o.err().map(|j| (j, r))).collect();
    let mut issues = Vec::new();
    for (i, js) in &assign {
        if js.len() > 1 { issues.push(Issue { kind: IssueKind::Multi, row: row_of_host.get(i).copied(), reference: Some(js[0]), detail: format!("{} satır \\N ile birleştirildi", js.len()) }); }
    }
    for (k, i, j, d) in pre { issues.push(Issue { kind: k, row: row_of_host.get(&i).copied(), reference: Some(j), detail: d }); }
    let no_ref: Vec<usize> = hosts.iter().copied().filter(|i| !assign.contains_key(i)).collect();
    for &i in &no_ref {
        issues.push(Issue { kind: IssueKind::NoRef, row: row_of_host.get(&i).copied(), reference: None, detail: tags::strip_tags(&hosts_in[i].text).chars().take(60).collect() });
    }
    for &j in &orphans {
        let detail: String = tags::strip_tags(&refs[j].text).chars().take(60).collect();
        if o.insert_orphans { issues.push(Issue { kind: IssueKind::Orphan, row: row_of_orphan.get(&j).copied(), reference: Some(j), detail }); }
        else { issues.push(Issue { kind: IssueKind::OrphanSkipped, row: None, reference: Some(j), detail }); }
    }
    issues.sort_by_key(|x| (x.row.unwrap_or(usize::MAX), format!("{:?}", x.kind)));
    Plan { filled: assign.len(), orphans: orphans.len(), no_ref: no_ref.len(), events: merged.into_iter().map(|(e, _)| e).collect(), issues }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(s: i64, e: i64, t: &str) -> Event { Event::new(s, e, "Default", t) }

    #[test]
    fn transfer_rules() {
        let mut hosts = vec![ev(0, 2000, "{\\an8}Hello."), ev(2500, 4000, "How are you?"), ev(5000, 6000, "Unmatched line"), ev(8000, 9000, "Bye.")];
        hosts[3].style = "Sign".into();
        let refs = vec![ev(100, 1900, "Merhaba."), ev(2600, 3200, "Nasılsın?"), ev(3200, 3900, "İyi misin?"), ev(7000, 7500, "Kimsesiz satır"), ev(8000, 9000, "Hoşça kal.")];
        let p = plan_transfer(&hosts, &refs, &TransferOptions::default());
        assert_eq!(p.events[0].text, "{\\an8}Merhaba.");
        assert_eq!(p.events[1].text, "Nasılsın?\\Nİyi misin?");
        assert_eq!(p.events[2].text, "Unmatched line", "never deleted");
        // orphan inserted by time, before the line that starts later, with the nearest host's style
        assert_eq!(p.events[3].text, "Kimsesiz satır");
        assert_eq!(p.events[3].style, "Sign");
        assert_eq!(p.events[4].text, "Hoşça kal.");
        assert_eq!((p.filled, p.orphans, p.no_ref), (3, 1, 1));
        assert!(p.issues.iter().any(|i| i.kind == IssueKind::Multi && i.row == Some(1)));
        assert!(p.issues.iter().any(|i| i.kind == IssueKind::NoRef && i.row == Some(2)));
        // host styles limit
        let p2 = plan_transfer(&hosts, &refs, &TransferOptions { host_styles: Some(vec!["Default".into()]), insert_orphans: false, ..Default::default() });
        assert_eq!(p2.events.len(), 4);
        assert_eq!(p2.events[3].text, "Bye.");
        assert!(p2.issues.iter().any(|i| i.kind == IssueKind::OrphanSkipped));
    }

    #[test]
    fn ref_index_uses_offset() {
        let refs = vec![ev(1000, 2000, "a"), ev(3000, 4000, "b")];
        let idx = RefIndex::new(&refs);
        assert_eq!(idx.best(&refs, 1000, 2000, 0).map(|b| b.0), Some(0));
        // document 2 s later than the reference
        assert_eq!(idx.best(&refs, 5000, 6000, 2000).map(|b| b.0), Some(1));
        assert_eq!(idx.best(&refs, 9000, 9500, 0), None);
    }
}
