use anisub_core::{self as core, ass::{AssFile, Line}, ops, qc};
use anisub_lua::Runtime;
use std::path::PathBuf;
use std::process::ExitCode;

const HELP: &str = "anisub - anime subtitle toolkit (Aegisub-compatible automation, Subtitle Edit style tools)

USAGE:
  anisub convert <in> <out>                    convert between .ass/.srt (UTF-8 out)
  anisub check <file> [--cps N] [--chars N]    QC: reading speed, line length, overlaps...
  anisub shift <in> <out> <ms>                 shift all times (negative allowed)
  anisub fix <in> <out>                        sort, merge duplicates, close small gaps
  anisub macros <script.lua>                   list macros registered by a Lua script
  anisub run <script.lua> <macro> <in> <out>   run an Automation 4 macro headless
        [--select 1,2,3] [--fps 23.976] [-I includedir]
";

fn arg_val(args: &mut Vec<String>, flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    if i + 1 >= args.len() {
        return None;
    }
    let v = args.remove(i + 1);
    args.remove(i);
    Some(v)
}

fn load(p: &str) -> Result<AssFile, String> {
    core::load(&PathBuf::from(p)).map_err(|e| format!("{p}: {e}"))
}
fn save(f: &AssFile, p: &str) -> Result<(), String> {
    core::save(f, &PathBuf::from(p)).map_err(|e| format!("{p}: {e}"))
}

fn run() -> Result<(), String> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "-h" || args[0] == "--help" {
        print!("{HELP}");
        return Ok(());
    }
    let cmd = args.remove(0);
    match cmd.as_str() {
        "convert" if args.len() == 2 => save(&load(&args[0])?, &args[1]),
        "shift" if args.len() == 3 => {
            let mut f = load(&args[0])?;
            ops::shift(&mut f, args[2].parse().map_err(|_| "bad ms value")?);
            save(&f, &args[1])
        }
        "fix" if args.len() == 2 => {
            let mut f = load(&args[0])?;
            ops::sort_by_start(&mut f);
            let m = ops::merge_duplicates(&mut f);
            ops::close_gaps(&mut f, 250);
            eprintln!("merged {m} duplicate events");
            save(&f, &args[1])
        }
        "check" => {
            let mut rules = qc::Rules::default();
            if let Some(v) = arg_val(&mut args, "--cps") {
                rules.max_cps = v.parse().map_err(|_| "bad --cps")?;
            }
            if let Some(v) = arg_val(&mut args, "--chars") {
                rules.max_line_chars = v.parse().map_err(|_| "bad --chars")?;
            }
            let f = load(args.first().ok_or("missing file")?)?;
            let issues = qc::check(&f, &rules);
            for i in &issues {
                println!("#{:<5} {:<9} {}", i.event, i.kind, i.detail);
            }
            if issues.is_empty() { Ok(()) } else { Err(format!("{} issue(s)", issues.len())) }
        }
        "macros" if args.len() == 1 => {
            let rt = Runtime::new(AssFile::new_default()).map_err(|e| e.to_string())?;
            rt.load_script(&PathBuf::from(&args[0])).map_err(|e| e.to_string())?;
            for m in rt.macros().map_err(|e| e.to_string())? {
                println!("[{}] {} - {}", m.kind, m.name, m.description);
            }
            Ok(())
        }
        "run" => {
            let select = arg_val(&mut args, "--select");
            let fps = arg_val(&mut args, "--fps");
            let mut incs = Vec::new();
            while let Some(d) = arg_val(&mut args, "-I") {
                incs.push(d);
            }
            if args.len() != 4 {
                return Err("usage: anisub run <script.lua> <macro> <in> <out>".into());
            }
            let f = load(&args[2])?;
            let ev_idx: Vec<i64> = f
                .lines
                .iter()
                .enumerate()
                .filter_map(|(i, l)| matches!(l, Line::Dialogue(_)).then_some(i as i64 + 1))
                .collect();
            let sel: Vec<i64> = match select {
                Some(s) => s.split(',').filter_map(|x| x.trim().parse().ok()).collect(),
                None => ev_idx.clone(),
            };
            let active = sel.first().copied().unwrap_or(1);
            let rt = Runtime::new(f).map_err(|e| e.to_string())?;
            for d in incs {
                rt.add_include_dir(d);
            }
            if let Some(v) = fps {
                rt.set_fps(v.parse().map_err(|_| "bad --fps")?).map_err(|e| e.to_string())?;
            }
            rt.load_script(&PathBuf::from(&args[0])).map_err(|e| e.to_string())?;
            let res = rt.run(&args[1], sel, active).map_err(|e| e.to_string());
            let log = rt.take_log().unwrap_or_default();
            if !log.is_empty() {
                eprintln!("{log}");
            }
            let res = res?;
            if res.cancelled {
                return Err("macro cancelled".into());
            }
            let out = rt.subs.borrow().clone();
            save(&out, &args[3])
        }
        _ => Err(format!("unknown or malformed command '{cmd}'\n\n{HELP}")),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
