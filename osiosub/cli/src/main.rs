//! `osiosub` command line: convert, check, shift, and run Aegisub Lua macros headlessly.
use osiosub_core::{ass::AssFile, qc, textfmt, timing};
use osiosub_lua::{DefaultHost, ScriptRuntime};
use std::{env, fs, path::Path, process::exit};

const HELP: &str = "osiosub <command>
  convert <in> <out>                 convert between .ass/.srt/.vtt/.txt
  check <file>                       run QC (CPS, line length, overlaps, typography)
  shift <in> <out> <ms>              shift all lines by ms (negative allowed)
  fix <in> <out>                     typography fixes + sort + close tiny gaps
  macros <script.lua>                list macros registered by a script
  run <script.lua> <in> <out> [macro-index|name] [sel-first-last]
                                     run an Aegisub automation macro on a file
";

fn read(path: &str) -> AssFile {
    let bytes = fs::read(path).unwrap_or_else(|e| { eprintln!("{path}: {e}"); exit(1) });
    let text = String::from_utf8_lossy(&bytes).into_owned();
    textfmt::open_any(path, &text).unwrap_or_else(|e| { eprintln!("{path}: {e}"); exit(1) })
}

fn write(path: &str, f: &AssFile) {
    let l = path.to_ascii_lowercase();
    let s = if l.ends_with(".srt") { textfmt::to_srt(f) } else if l.ends_with(".vtt") { textfmt::to_vtt(f) } else if l.ends_with(".txt") { textfmt::to_txt(f) } else { f.to_string() };
    fs::write(path, s).unwrap_or_else(|e| { eprintln!("{path}: {e}"); exit(1) });
}

fn main() {
    let a: Vec<String> = env::args().skip(1).collect();
    match a.first().map(|s| s.as_str()) {
        Some("convert") if a.len() == 3 => write(&a[2], &read(&a[1])),
        Some("check") if a.len() == 2 => {
            let f = read(&a[1]);
            let issues = qc::check(&f, &qc::QcConfig::default());
            for i in &issues { println!("{:>5}  {:?}  [{}] {}", i.line + 1, i.severity, i.code, i.message); }
            println!("{} issue(s) in {} line(s)", issues.len(), f.events.len());
            if issues.iter().any(|i| i.severity == qc::Severity::Error) { exit(2); }
        }
        Some("shift") if a.len() == 4 => {
            let mut f = read(&a[1]);
            let d: i64 = a[3].parse().unwrap_or_else(|_| { eprintln!("bad ms"); exit(1) });
            let all: Vec<usize> = (0..f.events.len()).collect();
            timing::shift(&mut f, &all, d, true, true);
            write(&a[2], &f);
        }
        Some("fix") if a.len() == 3 => {
            let mut f = read(&a[1]);
            for e in &mut f.events { e.text = osiosub_core::tags::map_plain(&e.text, |s| s.replace("...", "…").replace("  ", " ")); e.text = e.text.trim().to_string(); }
            timing::sort_by_start(&mut f);
            let n = timing::close_gaps(&mut f, 100);
            eprintln!("closed {n} gap(s)");
            write(&a[2], &f);
        }
        Some("macros") if a.len() == 2 => {
            let mut rt = ScriptRuntime::new(Box::new(DefaultHost)).unwrap();
            match rt.load_file(Path::new(&a[1])) {
                Ok(m) => for (i, m) in m.iter().enumerate() { println!("{i}: {} — {}", m.name, m.description); },
                Err(e) => { eprintln!("{e}"); exit(1) }
            }
        }
        Some("run") if a.len() >= 4 => {
            let mut rt = ScriptRuntime::new(Box::new(DefaultHost)).unwrap();
            let ms = rt.load_file(Path::new(&a[1])).unwrap_or_else(|e| { eprintln!("{e}"); exit(1) });
            let which = a.get(4).map(|s| s.as_str()).unwrap_or("0");
            let idx = which.parse::<usize>().ok().or_else(|| ms.iter().position(|m| m.name == which)).unwrap_or_else(|| { eprintln!("macro not found"); exit(1) });
            let f = read(&a[2]);
            let sel: Vec<usize> = match a.get(5).and_then(|s| s.split_once('-')) {
                Some((x, y)) => (x.parse::<usize>().unwrap_or(1) - 1..y.parse::<usize>().unwrap_or(f.events.len())).collect(),
                None => (0..f.events.len()).collect(),
            };
            let r = rt.run(idx, &f, &sel, sel.first().copied().unwrap_or(0));
            if !r.log.is_empty() { eprintln!("{}", r.log); }
            if let Some(e) = r.error { eprintln!("error: {e}"); exit(1) }
            if r.cancelled { eprintln!("cancelled"); exit(3) }
            write(&a[3], &r.file.unwrap());
        }
        _ => { print!("{HELP}"); exit(if a.is_empty() { 0 } else { 1 }); }
    }
}
