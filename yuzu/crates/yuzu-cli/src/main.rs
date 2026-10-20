//! `yuzu` command line: convert, shift, check and run Aegisub-style Lua macros headlessly.

use asscore::{ass, ops, subrip, Script};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use yuzu_lua::*;

struct CliHost {
    answers: HashMap<String, DValue>,
    script: Option<PathBuf>,
}

impl Host for CliHost {
    fn log(&self, level: i32, msg: &str) {
        let tag = match level { 0 => "FATAL", 1 => "ERROR", 2 => "WARN", _ => "" };
        if tag.is_empty() { eprint!("{}", msg) } else { eprint!("[{}] {}", tag, msg) }
    }
    fn dialog(&self, spec: &DialogSpec) -> DialogResult {
        let mut values = default_dialog_values(spec);
        for (k, v) in &self.answers { if values.contains_key(k) { values.insert(k.clone(), v.clone()); } }
        let button = self.answers.get("__button").and_then(|v| if let DValue::Str(s) = v { Some(s.clone()) } else { None })
            .or_else(|| spec.ok.clone()).unwrap_or_else(|| spec.buttons.first().cloned().unwrap_or_default());
        DialogResult::Ok { button, values }
    }
    fn project(&self) -> ProjectInfo {
        ProjectInfo { script_path: self.script.clone(), ..Default::default() }
    }
}

fn usage() -> ! {
    eprintln!(
        "yuzu {} — anime subtitle toolkit\n\n\
USAGE:\n  yuzu info <file>\n  yuzu convert <in> <out>            (ass|srt|vtt by extension)\n  yuzu shift <in> <out> <±ms> [--start|--end]\n  yuzu qc <file> [--cps N] [--cpl N]\n  yuzu fix <in> <out>                  (dedupe, sort, clean tags, trim)\n  yuzu resample <in> <out> <W> <H>\n  yuzu macros <script.lua>...          (list macros)\n  yuzu run <script.lua> <macro-name|index> <in> <out> [--sel a-b,c] [--set key=value]...\n\n\
Osi's tools:\n  yuzu compare <source> <candidate> [-o report.html] [--csv x.csv] [--marked y.ass] [--only-flags]\n               [--shift auto|SEC] [--shift-range SEC] [--ratio-min X] [--ratio-max X] [--sim X]\n               [--max-start-diff SEC] [--min-overlap X] [--min-len N] [--include-typeset]\n               [--skip-styles-src RE] [--skip-styles-cand RE]      (aligncompare)\n  yuzu sync-ref <reference> <in> <out> [--scene-gap SEC] [--range SEC] [--oped RE]   (piecewise sync)\n  yuzu glossary <terms.tsv> <file> [--ref reference] [--fix out.ass]                 (term check)\n",
        env!("CARGO_PKG_VERSION")
    );
    std::process::exit(2)
}

pub fn read(path: &str) -> Script {
    let bytes = std::fs::read(path).unwrap_or_else(|e| { eprintln!("{}: {}", path, e); std::process::exit(1) });
    { let t = asscore::decode_bytes(&bytes); asscore::formats::load_any(path, &t, &Default::default()).unwrap_or_else(|| asscore::load_str(path, &t)) }
}

pub fn write(path: &str, sc: &Script) {
    let lower = path.to_ascii_lowercase();
    if let Some(f) = asscore::formats::detect(path, "").filter(|f| *f != asscore::formats::Format::Ass) {
        let out = asscore::formats::save_any(f, sc, &Default::default());
        std::fs::write(path, out).unwrap_or_else(|e| { eprintln!("{}: {}", path, e); std::process::exit(1) });
        return;
    }
    let out = if lower.ends_with(".srt") { subrip::write_srt(sc) } else if lower.ends_with(".vtt") { subrip::write_vtt(sc) } else { format!("\u{feff}{}", ass::write(sc)) };
    std::fs::write(path, out).unwrap_or_else(|e| { eprintln!("{}: {}", path, e); std::process::exit(1) });
}

fn parse_sel(s: &str, max: usize) -> Vec<usize> {
    let mut v = Vec::new();
    for part in s.split(',').filter(|p| !p.is_empty()) {
        if let Some((a, b)) = part.split_once('-') {
            let (a, b): (usize, usize) = (a.parse().unwrap_or(1), b.parse().unwrap_or(max));
            for i in a..=b.min(max) { v.push(i.saturating_sub(1)); }
        } else if let Ok(i) = part.parse::<usize>() { v.push(i.saturating_sub(1)); }
    }
    v
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.is_empty() { usage(); }
    match a[0].as_str() {
        "info" if a.len() == 2 => {
            let s = read(&a[1]);
            let (x, y) = s.res();
            let dialog = s.events.iter().filter(|e| !e.comment).count();
            println!("lines: {} ({} dialogue, {} comments)\nstyles: {}\nresolution: {}x{}", s.events.len(), dialog, s.events.len() - dialog, s.styles.len(), x, y);
            if let Some(last) = s.events.iter().map(|e| e.end).max() { println!("duration: {}", asscore::time::format_ass(last)); }
        }
        "convert" if a.len() == 3 => write(&a[2], &read(&a[1])),
        "shift" if a.len() >= 4 => {
            let mut s = read(&a[1]);
            let ms: i64 = a[3].parse().unwrap_or_else(|_| usage());
            let which = if a.iter().any(|x| x == "--start") { 1 } else if a.iter().any(|x| x == "--end") { 2 } else { 0 };
            let idx: Vec<usize> = (0..s.events.len()).collect();
            ops::shift(&mut s, &idx, ms, which);
            write(&a[2], &s);
        }
        "qc" if a.len() >= 2 => {
            let s = read(&a[1]);
            let mut cfg = ops::QcConfig::default();
            let mut i = 2;
            while i + 1 < a.len() {
                match a[i].as_str() { "--cps" => cfg.max_cps = a[i + 1].parse().unwrap_or(cfg.max_cps), "--cpl" => cfg.max_cpl = a[i + 1].parse().unwrap_or(cfg.max_cpl), _ => {} }
                i += 2;
            }
            let issues = ops::qc(&s, &cfg);
            for is in &issues {
                println!("{:>5} {:?} [{}] {}", is.index + 1, is.severity, is.code, is.message);
            }
            eprintln!("{} issue(s)", issues.len());
            if issues.iter().any(|i| i.severity == ops::Severity::Error) { std::process::exit(1); }
        }
        "fix" if a.len() == 3 => {
            let mut s = read(&a[1]);
            let d = ops::dedupe(&mut s);
            s.sort_by_start();
            for e in s.events.iter_mut() { e.text = asscore::tags::clean_tags(e.text.trim()); }
            eprintln!("removed {} duplicate(s)", d);
            write(&a[2], &s);
        }
        "resample" if a.len() == 5 => {
            let mut s = read(&a[1]);
            ops::resample(&mut s, a[3].parse().unwrap_or_else(|_| usage()), a[4].parse().unwrap_or_else(|_| usage()));
            write(&a[2], &s);
        }
        "macros" if a.len() >= 2 => {
            for p in &a[1..] {
                let host = Rc::new(CliHost { answers: HashMap::new(), script: None });
                match LuaScript::load(Path::new(p), host, &[]) {
                    Ok(s) => {
                        println!("{} — {}", p, s.script_name);
                        for m in s.macros() { println!("  [{}] {}  {}", m.index, m.name, m.description); }
                        for f in s.filters() { println!("  filter [{}] {}  {}", f.0, f.1, f.2); }
                    }
                    Err(e) => { eprintln!("{}: {}", p, e); std::process::exit(1); }
                }
            }
        }
        "run" if a.len() >= 5 => {
            let mut doc = read(&a[3]);
            let mut answers = HashMap::new();
            let mut sel: Vec<usize> = (0..doc.events.len()).collect();
            let mut i = 5;
            while i < a.len() {
                match a[i].as_str() {
                    "--sel" if i + 1 < a.len() => { sel = parse_sel(&a[i + 1], doc.events.len()); i += 2; }
                    "--set" if i + 1 < a.len() => {
                        if let Some((k, v)) = a[i + 1].split_once('=') {
                            let dv = if v == "true" { DValue::Bool(true) } else if v == "false" { DValue::Bool(false) } else if let Ok(n) = v.parse::<f64>() { DValue::Num(n) } else { DValue::Str(v.to_string()) };
                            answers.insert(k.to_string(), dv);
                        }
                        i += 2;
                    }
                    _ => i += 1,
                }
            }
            let host = Rc::new(CliHost { answers, script: Some(PathBuf::from(&a[3])) });
            let script = LuaScript::load(Path::new(&a[1]), host, &[]).unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1) });
            let ms = script.macros();
            let m = a[2].parse::<usize>().ok().filter(|i| *i < ms.len()).or_else(|| ms.iter().find(|m| m.name == a[2]).map(|m| m.index));
            let Some(m) = m else { eprintln!("macro '{}' not found; available: {}", a[2], ms.iter().map(|m| m.name.as_str()).collect::<Vec<_>>().join(", ")); std::process::exit(1) };
            match script.run_macro(m, &mut doc, &sel, sel.first().copied()) {
                Ok(o) if o.cancelled => { eprintln!("cancelled"); std::process::exit(3); }
                Ok(_) => write(&a[4], &doc),
                Err(e) => { eprintln!("macro error: {}", e); std::process::exit(1); }
            }
        }
        "compare" if a.len() >= 3 => compare_cmd(&a[1..]),
        "sync-ref" if a.len() >= 4 => sync_ref_cmd(&a[1..]),
        "glossary" if a.len() >= 3 => glossary_cmd(&a[1..]),
        _ => usage(),
    }
}

fn flag_value<'a>(a: &'a [String], name: &str) -> Option<&'a str> {
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1)).map(|s| s.as_str())
}

fn num<T: std::str::FromStr>(a: &[String], name: &str, d: T) -> T {
    flag_value(a, name).map(|v| v.parse().unwrap_or_else(|_| { eprintln!("{}: bad value '{}'", name, v); std::process::exit(2) })).unwrap_or(d)
}

/// aligncompare: source ↔ candidate check with reports.
fn compare_cmd(a: &[String]) {
    use asscore::compare::{self, CompareOptions};
    let (src_path, cand_path) = (&a[0], &a[1]);
    let o = CompareOptions {
        shift_ms: match flag_value(a, "--shift") { None | Some("auto") => None, Some(v) => Some((v.parse::<f64>().unwrap_or_else(|_| usage()) * 1000.0).round() as i64) },
        shift_range_ms: (num(a, "--shift-range", 15.0f64) * 1000.0) as i64,
        ratio_min: num(a, "--ratio-min", 0.45), ratio_max: num(a, "--ratio-max", 2.2), sim: num(a, "--sim", 0.85),
        max_start_diff_ms: (num(a, "--max-start-diff", 0.5f64) * 1000.0) as i64, min_overlap: num(a, "--min-overlap", 0.25), min_len: num(a, "--min-len", 10usize),
        skip_styles_src: flag_value(a, "--skip-styles-src").unwrap_or("").to_string(), skip_styles_cand: flag_value(a, "--skip-styles-cand").unwrap_or("").to_string(),
        include_typeset: a.iter().any(|x| x == "--include-typeset"),
    };
    let only = a.iter().any(|x| x == "--only-flags");
    let (src, cand) = (read(src_path), read(cand_path));
    let r = compare::compare(&src.events, &cand.events, &o);
    for line in compare::summary_lines(&r, src_path, cand_path) { println!("{}", line); }
    let save = |p: &str, data: String| std::fs::write(p, data).unwrap_or_else(|e| { eprintln!("{}: {}", p, e); std::process::exit(1) });
    if let Some(p) = flag_value(a, "-o") { save(p, compare::to_html(&r, src_path, cand_path, only)); eprintln!("rapor: {}", p); }
    if let Some(p) = flag_value(a, "--csv") { save(p, compare::to_csv(&r, only)); eprintln!("csv: {}", p); }
    if let Some(p) = flag_value(a, "--marked") {
        if std::fs::canonicalize(p).ok() == std::fs::canonicalize(cand_path).ok() && Path::new(p).exists() { eprintln!("--marked aday dosyanın üstüne yazamaz"); std::process::exit(2); }
        write(p, &compare::marked_document(&cand, &r));
        eprintln!("işaretli kopya: {}", p);
    }
}

/// Piecewise sync of <in> to a correctly timed reference (the bsnap role).
fn sync_ref_cmd(a: &[String]) {
    use asscore::sync::{self, SyncOptions};
    let (reference, mut doc) = (read(&a[0]), read(&a[1]));
    let mut o = SyncOptions { oped_max_ms: 0, ..Default::default() };
    o.scene_gap_ms = (num(a, "--scene-gap", 4.0f64) * 1000.0) as i64;
    o.fine_range_ms = (num(a, "--range", 8.0f64) * 1000.0) as i64;
    if let Some(re) = flag_value(a, "--oped") { o.oped_regex = re.to_string(); }
    let total = sync::reference_total_ms(&reference.events, &doc.events, 60_000);
    let act = sync::activity_from_reference(&reference.events, total, &o.oped_regex);
    let (segs, _) = sync::plan(&doc.events, &act, &o, |_| {});
    for (k, s) in segs.iter().enumerate() {
        println!("{:>3}  {}  {:>3} satır  kayma {:+.2} sn  güven {:>3.0}%  {}", k + 1, asscore::time::format_ass(s.start), s.rows.len(), s.shift as f64 / 1000.0, s.confidence * 100.0, s.notes.join("; "));
    }
    let (changes, warns) = sync::changes(&doc.events, &segs, i64::MAX / 4);
    for w in warns { eprintln!("UYARI: {}", w); }
    for (r, st, en) in &changes { doc.events[*r].start = *st; doc.events[*r].end = *en; }
    eprintln!("{} satır kaydırıldı", changes.len());
    write(&a[2], &doc);
}

/// Term check against a glossary; optionally fix forbidden variants.
fn glossary_cmd(a: &[String]) {
    use asscore::glossary::{self, Glossary};
    use asscore::transfer::RefIndex;
    let g = Glossary::load(Path::new(&a[0])).unwrap_or_else(|e| { eprintln!("{}: {}", a[0], e); std::process::exit(1) });
    let mut doc = read(&a[1]);
    let reference = flag_value(a, "--ref").map(read);
    let idx = reference.as_ref().map(|r| RefIndex::new(&r.events));
    let issues = glossary::check(&g, &doc.events, |i| {
        let (r, idx) = (reference.as_ref()?, idx.as_ref()?);
        let e = &doc.events[i];
        idx.best(&r.events, e.start, e.end, 0).map(|(j, _)| asscore::tags::display_text(&r.events[j].text))
    });
    for (row, msg, _) in &issues { println!("{:>5}  {}", row + 1, msg); }
    eprintln!("{} bulgu", issues.len());
    if let Some(out) = flag_value(a, "--fix") {
        for e in doc.events.iter_mut() {
            let parts: Vec<asscore::tags::Part> = asscore::tags::split_parts(&e.text).into_iter().map(|p| match p { asscore::tags::Part::Text(t) => asscore::tags::Part::Text(g.fix_forbidden(&t)), b => b }).collect();
            e.text = asscore::tags::join_parts(&parts);
        }
        write(out, &doc);
    }
}
