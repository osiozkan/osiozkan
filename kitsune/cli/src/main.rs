use kitsune_core::{ass, qc, srt, timing, Document};
use kitsune_lua::Host;
use std::path::Path;
use std::process::ExitCode;

const HELP: &str = "kitsune — anime altyazı aracı (çekirdek CLI)

KULLANIM:
  kitsune check <dosya> [--cps N] [--width N]      kalite kontrol
  kitsune convert <girdi> <çıktı>                  .ass / .srt dönüştür
  kitsune shift <girdi> <çıktı> <ms>               tüm zamanları kaydır
  kitsune fix <girdi> <çıktı> [--gap MS] [--min MS] çakışma/kısa süre düzelt
  kitsune lua-list <betik.lua>                     betikteki makroları listele
  kitsune lua-run <betik.lua> <girdi> <çıktı> [--macro N] [--select A-B]
";

fn load(p: &str) -> Result<Document, String> {
    let s = std::fs::read_to_string(p).map_err(|e| format!("{}: {}", p, e))?;
    Ok(if p.to_lowercase().ends_with(".srt") { srt::parse(&s) } else { ass::parse(&s) })
}

fn save(p: &str, d: &Document) -> Result<(), String> {
    let s = if p.to_lowercase().ends_with(".srt") { srt::write(d) } else { ass::write(d) };
    std::fs::write(p, s).map_err(|e| format!("{}: {}", p, e))
}

fn opt(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn run(args: Vec<String>) -> Result<(), String> {
    let a: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    match a.as_slice() {
        ["check", f, ..] => {
            let d = load(f)?;
            let mut cfg = qc::Config::default();
            if let Some(v) = opt(&args, "--cps").and_then(|v| v.parse().ok()) { cfg.max_cps = v; }
            if let Some(v) = opt(&args, "--width").and_then(|v| v.parse().ok()) { cfg.max_line_chars = v; }
            let issues = qc::check(&d, &cfg);
            for i in &issues {
                println!("#{:<4} {:?}/{}: {}", i.event + 1, i.severity, i.code, i.message);
            }
            println!("{} satır, {} sorun", d.events.len(), issues.len());
            if issues.iter().any(|i| i.severity == qc::Severity::Error) {
                return Err("hata düzeyinde sorunlar var".into());
            }
            Ok(())
        }
        ["convert", i, o] => save(o, &load(i)?),
        ["shift", i, o, ms] => {
            let mut d = load(i)?;
            timing::shift(&mut d, &[], ms.parse().map_err(|_| "ms sayı olmalı")?);
            save(o, &d)
        }
        ["fix", i, o, ..] => {
            let mut d = load(i)?;
            let gap = opt(&args, "--gap").and_then(|v| v.parse().ok()).unwrap_or(0);
            let min = opt(&args, "--min").and_then(|v| v.parse().ok()).unwrap_or(500);
            let a = timing::fix_overlaps(&mut d, gap);
            let b = timing::extend_short(&mut d, min, gap);
            println!("{} çakışma, {} kısa satır düzeltildi", a, b);
            save(o, &d)
        }
        ["lua-list", s] => {
            let h = Host::new(Document::new()).map_err(|e| e.to_string())?;
            h.load_script(Path::new(s)).map_err(|e| e.to_string())?;
            for (n, m) in h.macros().iter().enumerate() {
                println!("{}: {} — {}", n, m.name, m.description);
            }
            Ok(())
        }
        ["lua-run", s, i, o, ..] => {
            let h = Host::new(load(i)?).map_err(|e| e.to_string())?;
            h.load_script(Path::new(s)).map_err(|e| e.to_string())?;
            let n: usize = opt(&args, "--macro").and_then(|v| v.parse().ok()).unwrap_or(0);
            let total = h.document().events.len();
            let sel: Vec<usize> = match opt(&args, "--select") {
                Some(r) => {
                    let (x, y) = r.split_once('-').unwrap_or((&r, &r));
                    let x: usize = x.parse().map_err(|_| "geçersiz --select")?;
                    let y: usize = y.parse().map_err(|_| "geçersiz --select")?;
                    (x.max(1) - 1..y.min(total)).collect()
                }
                None => (0..total).collect(),
            };
            let r = h.run(n, &sel, sel.first().copied());
            for l in h.take_log() {
                eprint!("{}", l);
            }
            r.map_err(|e| e.to_string())?;
            save(o, &h.document())
        }
        _ => {
            print!("{}", HELP);
            Ok(())
        }
    }
}

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hata: {}", e);
            ExitCode::FAILURE
        }
    }
}
