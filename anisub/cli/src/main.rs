use anisub_core::ass::Script;
use anisub_core::lua::{HostConfig, LuaScript};
use anisub_core::srt;
use std::path::{Path, PathBuf};

fn usage() -> ! {
    eprintln!(
        "anisub - anime subtitle toolkit\n\n\
         USAGE:\n  anisub macros <script.lua>\n  anisub run <subs.(ass|srt)> <script.lua> [--macro NAME|INDEX] [--fps F] [-o out.ass]\n  anisub convert <in.(ass|srt)> <out.(ass|srt)>\n  anisub info <subs.(ass|srt)>\n  anisub qc <subs.(ass|srt)>\n  anisub shift <subs.(ass|srt)> <ms>"
    );
    std::process::exit(2)
}

fn load_subs(p: &Path) -> Script {
    let bytes = std::fs::read(p).unwrap_or_else(|e| die(&format!("{}: {e}", p.display())));
    let text = String::from_utf8_lossy(&bytes);
    if p.extension().map(|e| e.eq_ignore_ascii_case("srt")).unwrap_or(false) { srt::parse(&text) } else { Script::parse(&text) }
}

fn save_subs(p: &Path, s: &Script) {
    let out = if p.extension().map(|e| e.eq_ignore_ascii_case("srt")).unwrap_or(false) { srt::write(s) } else { s.to_ass_string() };
    std::fs::write(p, out).unwrap_or_else(|e| die(&format!("{}: {e}", p.display())));
}

fn die(m: &str) -> ! {
    eprintln!("error: {m}");
    std::process::exit(1)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else { usage() };
    match cmd.as_str() {
        "macros" => {
            let path = args.get(1).unwrap_or_else(|| usage());
            let s = LuaScript::load(Path::new(path), &HostConfig::default()).unwrap_or_else(|e| die(&e));
            println!("{} — {} ({} {})", s.name, s.description, s.author, s.version);
            for (i, m) in s.macros().iter().enumerate() {
                println!("[{i}] {}{} — {}", m.name, if m.is_filter { " (filter)" } else { "" }, m.description);
            }
        }
        "run" => {
            let subs_path = PathBuf::from(args.get(1).unwrap_or_else(|| usage()));
            let lua_path = PathBuf::from(args.get(2).unwrap_or_else(|| usage()));
            let mut macro_sel: Option<String> = None;
            let mut out: Option<PathBuf> = None;
            let mut cfg = HostConfig::default();
            let mut i = 3;
            while i < args.len() {
                match args[i].as_str() {
                    "--macro" => { macro_sel = args.get(i + 1).cloned(); i += 1; }
                    "--fps" => { cfg.fps = args.get(i + 1).and_then(|v| v.parse().ok()); i += 1; }
                    "-o" => { out = args.get(i + 1).map(PathBuf::from); i += 1; }
                    _ => usage(),
                }
                i += 1;
            }
            let mut subs = load_subs(&subs_path);
            let script = LuaScript::load(&lua_path, &cfg).unwrap_or_else(|e| die(&e));
            let macros = script.macros();
            if macros.is_empty() { die("script registered no macros"); }
            let idx = match macro_sel {
                None => 0,
                Some(s) => s.parse::<usize>().ok().or_else(|| macros.iter().position(|m| m.name == s)).unwrap_or_else(|| die("macro not found")),
            };
            let n = subs.lines.len();
            let sel: Vec<usize> = subs.lines.iter().enumerate().filter(|(_, l)| matches!(l.kind, anisub_core::ass::LineKind::Dialogue(_))).map(|(i, _)| i).collect();
            let active = sel.first().copied().unwrap_or(0);
            let _ = n;
            match script.run(idx, &mut subs, &sel, active) {
                Ok(r) => {
                    if !r.log.is_empty() { print!("{}", r.log); }
                    save_subs(&out.unwrap_or(subs_path), &subs);
                }
                Err(e) => die(&e),
            }
        }
        "convert" => {
            let s = load_subs(Path::new(args.get(1).unwrap_or_else(|| usage())));
            save_subs(Path::new(args.get(2).unwrap_or_else(|| usage())), &s);
        }
        "qc" => {
            let s = load_subs(Path::new(args.get(1).unwrap_or_else(|| usage())));
            let issues = anisub_core::qc::check(&s, &anisub_core::qc::QcConfig::default());
            for i in &issues { println!("line {}: [{:?}] {} — {}", i.line + 1, i.severity, i.code, i.message); }
            println!("{} issue(s)", issues.len());
            if issues.iter().any(|i| i.severity == anisub_core::qc::Severity::Error) { std::process::exit(1); }
        }
        "shift" => {
            let p = Path::new(args.get(1).unwrap_or_else(|| usage()));
            let ms: i64 = args.get(2).and_then(|v| v.parse().ok()).unwrap_or_else(|| usage());
            let mut s = load_subs(p);
            anisub_core::timing::shift(&mut s, &[], ms);
            save_subs(p, &s);
        }
        "info" => {
            let s = load_subs(Path::new(args.get(1).unwrap_or_else(|| usage())));
            let (x, y) = s.play_res();
            println!("resolution: {x}x{y}\nstyles: {}\nevents: {}", s.styles().count(), s.events().count());
        }
        _ => usage(),
    }
}
