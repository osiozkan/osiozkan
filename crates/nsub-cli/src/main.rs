//! `nightsub` command line: convert, inspect, QC, fix, time-shift and run Aegisub Lua macros headlessly.

use nsub_core::formats::{self, Format};
use nsub_core::{fixes, ops, qc, Script};
use nsub_lua::{Automation, Options};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

const HELP: &str = "\
nightsub — anime subtitle toolkit (Aegisub-compatible automation, Subtitle Edit style tools)

USAGE:
  nightsub info <file>
  nightsub convert <in> <out> [--fps N]            format chosen by <out> extension
  nightsub check <file> [--cps N] [--min-dur MS] [--max-dur MS] [--line-len N] [--lines N] [--gap MS]
  nightsub fix <file> -o <out> [--overlaps] [--min-dur MS] [--spaces] [--gap MS] [--cps N] [--case upper|lower|sentence|title|invert]
                [--hi] [--balance N] [--unbreak] [--split N] [--merge-dups MS] [--merge-short N] [--snap FPS]
  nightsub shift <file> <+/-ms> -o <out>
  nightsub sync <file> <old1> <new1> <old2> <new2> -o <out>   (times in ms or H:MM:SS.cc)
  nightsub replace <file> <pattern> <with> -o <out> [--regex] [--case] [--word] [--style S] [--skip-tags]
  nightsub macros <script.lua>...                  list macros registered by Lua scripts
  nightsub run <file> --script <x.lua> [--script ...] --macro <name> [-o out] [--sel 1-5,9]
                [--fps N] [--answer name=value]... [--button LABEL] [--include DIR]...
";

struct Args {
    pos: Vec<String>,
    flags: HashMap<String, Vec<String>>,
}

fn parse_args(raw: &[String], valued: &[&str]) -> Args {
    let mut a = Args { pos: vec![], flags: HashMap::new() };
    let mut i = 0;
    while i < raw.len() {
        let r = &raw[i];
        if r == "-o" || (r.starts_with("--") && r.len() > 2) {
            let k = r.trim_start_matches('-').to_string();
            let k = if k == "o" { "out".to_string() } else { k };
            if valued.contains(&k.as_str()) && i + 1 < raw.len() {
                a.flags.entry(k).or_default().push(raw[i + 1].clone());
                i += 2;
                continue;
            }
            a.flags.entry(k).or_default();
        } else if r.starts_with('-') && r.len() > 1 && r[1..].parse::<f64>().is_err() {
            a.flags.entry(r.trim_start_matches('-').to_string()).or_default();
        } else {
            a.pos.push(r.clone());
        }
        i += 1;
    }
    a
}

impl Args {
    fn has(&self, k: &str) -> bool {
        self.flags.contains_key(k)
    }
    fn get(&self, k: &str) -> Option<&str> {
        self.flags.get(k).and_then(|v| v.last()).map(|s| s.as_str())
    }
    fn num<T: std::str::FromStr>(&self, k: &str) -> Result<Option<T>, String> {
        match self.get(k) {
            None => Ok(None),
            Some(v) => v.parse().map(Some).map_err(|_| format!("--{k}: invalid number '{v}'")),
        }
    }
    fn pos(&self, i: usize, what: &str) -> Result<&str, String> {
        self.pos.get(i).map(|s| s.as_str()).ok_or_else(|| format!("missing argument: {what}"))
    }
}

fn load(path: &str) -> Result<Script, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    formats::load(&bytes, Some(path)).map_err(|e| format!("{path}: {e}"))
}

fn save(script: &Script, path: &str) -> Result<(), String> {
    let fmt = Format::from_extension(path).unwrap_or(Format::Ass);
    std::fs::write(path, formats::save(script, fmt)).map_err(|e| format!("{path}: {e}"))
}

fn parse_time(s: &str) -> Result<i64, String> {
    if let Ok(v) = s.parse::<i64>() {
        return Ok(v);
    }
    nsub_core::time::parse_clock(s).ok_or_else(|| format!("bad time '{s}'"))
}

fn parse_selection(spec: &str, max: usize) -> Result<Vec<usize>, String> {
    let mut out = vec![];
    for part in spec.split(',').filter(|p| !p.is_empty()) {
        if let Some((a, b)) = part.split_once('-') {
            let (a, b): (usize, usize) = (a.parse().map_err(|_| "bad --sel")?, b.parse().map_err(|_| "bad --sel")?);
            out.extend(a..=b.min(max));
        } else {
            out.push(part.parse().map_err(|_| "bad --sel")?);
        }
    }
    Ok(out)
}

fn run() -> Result<ExitCode, String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = raw.first().cloned() else {
        print!("{HELP}");
        return Ok(ExitCode::SUCCESS);
    };
    let rest = &raw[1..];
    let valued = ["o", "out", "fps", "cps", "min-dur", "max-dur", "line-len", "lines", "gap", "script", "macro", "sel", "answer", "button", "include", "style", "video", "case", "balance", "split", "merge-dups", "merge-short", "snap"];
    let a = parse_args(rest, &valued);
    match cmd.as_str() {
        "help" | "--help" | "-h" => print!("{HELP}"),
        "info" => {
            let s = load(a.pos(0, "file")?)?;
            let (w, h) = s.resolution();
            println!("resolution: {w}x{h}\nstyles: {}\nevents: {} ({} comments)", s.styles.len(), s.events.len(), s.events.iter().filter(|e| e.comment).count());
            if let Some(last) = s.events.iter().map(|e| e.end).max() {
                println!("last end: {}", nsub_core::time::format_ass(last));
            }
            for w in &s.warnings {
                println!("warning: {w}");
            }
        }
        "convert" => {
            let s = load(a.pos(0, "in")?)?;
            let out = a.pos(1, "out")?;
            let fmt = Format::from_extension(out).ok_or("unknown output extension")?;
            let fps = a.num::<f64>("fps")?.unwrap_or(23.976);
            let text = if fmt == Format::MicroDvd { formats::write_microdvd(&s, fps) } else { formats::save(&s, fmt) };
            std::fs::write(out, text).map_err(|e| e.to_string())?;
            println!("wrote {out} ({} events)", s.events.len());
        }
        "check" => {
            let s = load(a.pos(0, "file")?)?;
            let mut cfg = qc::QcConfig::default();
            if let Some(v) = a.num("cps")? { cfg.max_cps = v }
            if let Some(v) = a.num("min-dur")? { cfg.min_duration_ms = v }
            if let Some(v) = a.num("max-dur")? { cfg.max_duration_ms = v }
            if let Some(v) = a.num("line-len")? { cfg.max_line_len = v }
            if let Some(v) = a.num("lines")? { cfg.max_lines = v }
            if let Some(v) = a.num("gap")? { cfg.min_gap_ms = v }
            let issues = qc::check(&s, &cfg);
            for i in &issues {
                println!("#{:<5} {:?}: {}", i.index + 1, i.kind, i.message);
            }
            println!("{} issue(s)", issues.len());
            if !issues.is_empty() {
                return Ok(ExitCode::from(1));
            }
        }
        "fix" => {
            let mut s = load(a.pos(0, "file")?)?;
            let out = a.get("out").ok_or("-o <out> required")?;
            let mut n = 0;
            if a.has("overlaps") { n += qc::fix_overlaps(&mut s, 0) }
            if let Some(ms) = a.num::<i64>("min-dur")? { n += qc::fix_min_duration(&mut s, ms) }
            if a.has("spaces") { n += qc::fix_double_spaces(&mut s) }
            if let Some(ms) = a.num::<i64>("gap")? { n += fixes::apply_min_gap(&mut s, ms) }
            if let Some(c) = a.num::<f64>("cps")? { n += fixes::fix_cps(&mut s, c, a.num::<i64>("gap")?.unwrap_or(0), 7000) }
            if let Some(c) = a.get("case") {
                let mode = match c {
                    "upper" => fixes::Case::Upper,
                    "lower" => fixes::Case::Lower,
                    "sentence" => fixes::Case::Sentence,
                    "title" => fixes::Case::Title,
                    "invert" => fixes::Case::Invert,
                    other => return Err(format!("unknown --case '{other}'")),
                };
                n += fixes::change_case(&mut s, mode, None);
            }
            if a.has("hi") { n += fixes::remove_hearing_impaired(&mut s) }
            if a.has("unbreak") { n += fixes::unbreak(&mut s) }
            if let Some(l) = a.num::<usize>("balance")? { n += fixes::balance_all(&mut s, l) }
            if let Some(l) = a.num::<usize>("split")? { n += fixes::split_long(&mut s, l) }
            if let Some(ms) = a.num::<i64>("merge-dups")? { n += fixes::merge_duplicates(&mut s, ms) }
            if let Some(l) = a.num::<usize>("merge-short")? { n += fixes::merge_short(&mut s, l, 500) }
            if let Some(f) = a.num::<f64>("snap")? { fixes::snap_to_frames(&mut s, f); n += 1 }
            save(&s, out)?;
            println!("{n} fix(es) applied -> {out}");
        }
        "shift" => {
            let mut s = load(a.pos(0, "file")?)?;
            let d: i64 = a.pos(1, "ms")?.parse().map_err(|_| "bad ms")?;
            ops::shift(&mut s, d, None);
            save(&s, a.get("out").ok_or("-o <out> required")?)?;
        }
        "sync" => {
            let mut s = load(a.pos(0, "file")?)?;
            let t = |i| parse_time(a.pos(i, "time").unwrap_or(""));
            ops::sync_two_points(&mut s, t(1)?, t(2)?, t(3)?, t(4)?)?;
            save(&s, a.get("out").ok_or("-o <out> required")?)?;
        }
        "replace" => {
            let mut s = load(a.pos(0, "file")?)?;
            let r = ops::Replace {
                pattern: a.pos(1, "pattern")?.to_string(),
                with: a.pos(2, "with")?.to_string(),
                regex: a.has("regex"),
                case_sensitive: a.has("case"),
                whole_word: a.has("word"),
                style: a.get("style").map(String::from),
                skip_tags: a.has("skip-tags"),
            };
            let n = ops::find_replace(&mut s, &r)?;
            save(&s, a.get("out").ok_or("-o <out> required")?)?;
            println!("{n} line(s) changed");
        }
        "macros" => {
            let mut auto = Automation::new(Options::default())?;
            for p in &a.pos {
                for m in auto.load_script(&PathBuf::from(p))? {
                    println!("{:<40} {}", m.name, m.description);
                }
            }
        }
        "run" => {
            let path = a.pos(0, "file")?.to_string();
            let mut s = load(&path)?;
            let mut opts = Options::default();
            if let Some(f) = a.num("fps")? { opts.fps = f }
            opts.dialog_button = a.get("button").map(String::from);
            opts.include_dirs = a.flags.get("include").cloned().unwrap_or_default().into_iter().map(PathBuf::from).collect();
            opts.script_path = Some(PathBuf::from(&path));
            opts.video_path = a.get("video").map(PathBuf::from);
            for kv in a.flags.get("answer").cloned().unwrap_or_default() {
                let (k, v) = kv.split_once('=').ok_or("--answer expects name=value")?;
                opts.dialog_answers.insert(k.to_string(), v.to_string());
            }
            let mut fm = nsub_fonts::FontMetrics::system();
            if let Some(dir) = std::path::Path::new(&path).parent() {
                fm.add_dir(dir);
            }
            let mut auto = Automation::new(opts)?;
            auto.set_metrics(Box::new(fm));
            let mut all = vec![];
            for sc in a.flags.get("script").ok_or("--script required")? {
                all.extend(auto.load_script(&PathBuf::from(sc))?);
            }
            let want = a.get("macro").ok_or("--macro required")?;
            let m = all
                .iter()
                .find(|m| m.name == want)
                .or_else(|| all.iter().find(|m| m.name.ends_with(&format!("/{want}"))))
                .ok_or_else(|| format!("macro '{want}' not found; available: {}", all.iter().map(|m| m.name.as_str()).collect::<Vec<_>>().join(", ")))?;
            let first_ev = s.info.len() + s.styles.len() + 1;
            let n_flat = first_ev + s.events.len();
            let sel = match a.get("sel") {
                Some(spec) => parse_selection(spec, n_flat)?,
                None => (first_ev..n_flat).collect(),
            };
            let active = sel.first().copied().unwrap_or(first_ev);
            if m.has_validate && !auto.validate(m.id, &s, &sel, active)? {
                return Err(format!("macro '{}' rejected the current selection (validation failed)", m.name));
            }
            let res = auto.run(m.id, &mut s, &sel, active)?;
            if res.canceled {
                println!("macro cancelled; nothing written");
                return Ok(ExitCode::from(2));
            }
            let out = a.get("out").unwrap_or(&path).to_string();
            save(&s, &out)?;
            println!("ok -> {out}");
        }
        other => return Err(format!("unknown command '{other}'\n\n{HELP}")),
    }
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    match run() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}
