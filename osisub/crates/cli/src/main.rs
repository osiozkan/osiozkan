use osisub_core::{checks, ops, srt, Script};
use osisub_lua::Automation;
use std::process::ExitCode;

const HELP: &str = "osisub 0.1 - anime subtitle toolkit (Aegisub-compatible Lua automation)

USAGE:
  osisub info FILE
  osisub convert IN OUT                  (.ass .ssa .srt .vtt by extension)
  osisub check FILE [--max-cps N] [--max-cpl N] [--min-dur MS] [--max-dur MS] [--min-gap MS]
  osisub shift FILE MS [-o OUT]          (MS may be negative)
  osisub fix FILE [-o OUT]               (sort, trim overlaps, merge duplicates)
  osisub macros SCRIPT.lua
  osisub run SCRIPT.lua MACRO FILE [-o OUT] [--select 1,2,3] [--active N] [--set name=value]... [--fps F] [--res WxH]
";

fn load(path: &str) -> Result<Script, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let text = String::from_utf8_lossy(&bytes).to_string();
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "srt" => Ok(srt::parse_srt(&text)),
        "vtt" => Ok(srt::parse_vtt(&text)),
        _ => Script::parse(&text).map_err(|e| format!("{path}: {e}")),
    }
}

fn save(sc: &Script, path: &str) -> Result<(), String> {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let out = match ext.as_str() {
        "srt" => srt::to_srt(sc),
        "vtt" => srt::to_vtt(sc),
        _ => sc.to_ass(),
    };
    std::fs::write(path, out).map_err(|e| format!("{path}: {e}"))
}

fn opt(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("");
    let rest = &args[args.len().min(1)..];
    let out_path = opt(&args, "-o");
    match cmd {
        "info" => {
            let sc = load(rest.first().ok_or("missing FILE")?)?;
            let (w, h) = sc.resolution();
            println!("resolution: {w}x{h}\nstyles: {}\nevents: {} ({} comments)", sc.styles().count(), sc.events().count(), sc.events().filter(|e| e.comment).count());
            if let Some(last) = sc.events().map(|e| e.end).max() {
                println!("duration: {}", osisub_core::time::format_ass_time(last));
            }
        }
        "convert" => {
            let (i, o) = (rest.first().ok_or("missing IN")?, rest.get(1).ok_or("missing OUT")?);
            save(&load(i)?, o)?;
        }
        "check" => {
            let sc = load(rest.first().ok_or("missing FILE")?)?;
            let mut cfg = checks::CheckConfig::default();
            if let Some(v) = opt(&args, "--max-cps").and_then(|v| v.parse().ok()) { cfg.max_cps = v; }
            if let Some(v) = opt(&args, "--max-cpl").and_then(|v| v.parse().ok()) { cfg.max_chars_per_line = v; }
            if let Some(v) = opt(&args, "--min-dur").and_then(|v| v.parse().ok()) { cfg.min_duration_ms = v; }
            if let Some(v) = opt(&args, "--max-dur").and_then(|v| v.parse().ok()) { cfg.max_duration_ms = v; }
            if let Some(v) = opt(&args, "--min-gap").and_then(|v| v.parse().ok()) { cfg.min_gap_ms = v; }
            let issues = checks::run(&sc, &cfg);
            for i in &issues {
                println!("#{} {:?}: {}", i.entry + 1, i.kind, i.message);
            }
            println!("{} issue(s)", issues.len());
            if !issues.is_empty() {
                return Err(String::new());
            }
        }
        "shift" => {
            let f = rest.first().ok_or("missing FILE")?;
            let ms: i64 = rest.get(1).ok_or("missing MS")?.parse().map_err(|_| "MS must be an integer")?;
            let mut sc = load(f)?;
            ops::shift(&mut sc, ms, None);
            save(&sc, out_path.as_deref().unwrap_or(f))?;
        }
        "fix" => {
            let f = rest.first().ok_or("missing FILE")?;
            let mut sc = load(f)?;
            ops::sort_by_start(&mut sc);
            let o = ops::fix_overlaps(&mut sc);
            let m = ops::merge_duplicates(&mut sc);
            eprintln!("trimmed {o} overlap(s), merged {m} duplicate(s)");
            save(&sc, out_path.as_deref().unwrap_or(f))?;
        }
        "macros" => {
            let a = Automation::new(Script::new_default()).map_err(|e| e.to_string())?;
            a.load_file(rest.first().ok_or("missing SCRIPT")?).map_err(|e| e.to_string())?;
            for m in a.list_macros() {
                println!("{}{} — {}", if m.is_filter { "[filter] " } else { "" }, m.name, m.description);
            }
        }
        "run" => {
            let (script, name, file) = (rest.first().ok_or("missing SCRIPT")?, rest.get(1).ok_or("missing MACRO")?, rest.get(2).ok_or("missing FILE")?);
            let sc = load(file)?;
            let a = Automation::new(sc).map_err(|e| e.to_string())?;
            if let Some(f) = opt(&args, "--fps").and_then(|v| v.parse().ok()) {
                let (w, h) = opt(&args, "--res").and_then(|r| r.split_once('x').map(|(a, b)| (a.parse().unwrap_or(1920), b.parse().unwrap_or(1080)))).unwrap_or((1920, 1080));
                a.set_video(w, h, f);
            }
            let sets: Vec<(String, String)> = args.windows(2).filter(|w| w[0] == "--set").filter_map(|w| w[1].split_once('=').map(|(k, v)| (k.to_string(), v.to_string()))).collect();
            a.set_dialog_values(&sets).map_err(|e| e.to_string())?;
            a.load_file(script).map_err(|e| e.to_string())?;
            let n = a.script().entries.len();
            let sel: Vec<usize> = match opt(&args, "--select") {
                Some(s) => s.split(',').filter_map(|x| x.trim().parse().ok()).collect(),
                None => a.script().entries.iter().enumerate().filter(|(_, e)| matches!(e, osisub_core::Entry::Dialogue(_))).map(|(i, _)| i + 1).collect(),
            };
            let active = opt(&args, "--active").and_then(|v| v.parse().ok()).or(sel.first().copied()).unwrap_or(n.min(1));
            let res = a.run(name, &sel, active);
            for l in a.logs() {
                eprintln!("{l}");
            }
            res.map_err(|e| e.to_string())?;
            save(&a.take_script(), out_path.as_deref().unwrap_or(file))?;
        }
        _ => {
            print!("{HELP}");
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("error: {e}");
            }
            ExitCode::FAILURE
        }
    }
}
