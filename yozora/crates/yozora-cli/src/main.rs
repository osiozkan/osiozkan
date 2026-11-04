use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use yozora_core::convert::{self, Format};
use yozora_core::qc::{self, QcConfig, Severity};
use yozora_core::{time, timing, Document};
use yozora_lua::{HostOptions, MacroKind, Script};

#[derive(Parser)]
#[command(name = "yozora", version, about = "Yozora — anime altyazı aracı (komut satırı)")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Dosya hakkında özet bilgi.
    Info { file: PathBuf },
    /// Biçim dönüştür (ass/ssa/srt/vtt/sub/txt; uzantıdan anlaşılır).
    Convert {
        input: PathBuf,
        output: PathBuf,
        #[arg(long, default_value_t = 23.976)]
        fps: f64,
    },
    /// Kalite kontrol raporu.
    Qc {
        file: PathBuf,
        #[arg(long, default_value_t = 20.0)]
        cps: f64,
        #[arg(long, default_value_t = 42)]
        max_line: usize,
        #[arg(long, default_value_t = 2)]
        max_lines: usize,
        #[arg(long, default_value_t = 500)]
        min_dur: i64,
        #[arg(long, default_value_t = 8000)]
        max_dur: i64,
        #[arg(long, default_value_t = 0)]
        min_gap: i64,
        #[arg(long)]
        json: bool,
        /// Hata düzeyinde sorun varsa 1 ile çık.
        #[arg(long)]
        strict: bool,
    },
    /// Zamanları kaydır (örn. --by -1.5s, 250ms).
    Shift {
        input: PathBuf,
        output: PathBuf,
        #[arg(long, allow_hyphen_values = true)]
        by: String,
        /// Yalnızca bu zamandan sonrakiler (ASS zamanı ya da süre).
        #[arg(long)]
        after: Option<String>,
    },
    /// Timing post-processor (lead-in/out, anahtar kare yakalama, bağlama).
    Tpp {
        input: PathBuf,
        output: PathBuf,
        #[arg(long, default_value_t = 120)]
        lead_in: i64,
        #[arg(long, default_value_t = 500)]
        lead_out: i64,
        #[arg(long, default_value_t = 350)]
        snap_start_before: i64,
        #[arg(long, default_value_t = 0)]
        snap_start_after: i64,
        #[arg(long, default_value_t = 0)]
        snap_end_before: i64,
        #[arg(long, default_value_t = 900)]
        snap_end_after: i64,
        #[arg(long, default_value_t = 0)]
        link_gap: i64,
        #[arg(long, default_value_t = 0)]
        link_bias: i64,
        /// Anahtar kare dosyası (satır başına bir kare numarası ya da "# keyframe format v1" XviD stats değil).
        #[arg(long)]
        keyframes: Option<PathBuf>,
        #[arg(long, default_value_t = 23.976)]
        fps: f64,
    },
    /// Aegisub uyumlu Lua betikleri.
    Lua {
        #[command(subcommand)]
        cmd: LuaCmd,
    },
}

#[derive(Subcommand)]
enum LuaCmd {
    /// Betikteki makroları listele.
    List { scripts: Vec<PathBuf> },
    /// Bir makroyu/süzgeci çalıştır.
    Run {
        script: PathBuf,
        /// Makro adı (tam eşleşme ya da alt dize) ya da 1 tabanlı sıra numarası.
        #[arg(long = "macro", short = 'm')]
        macro_name: String,
        /// Girdi altyazı dosyası.
        file: PathBuf,
        /// Çıktı (verilmezse yazılmaz, yalnızca günlük gösterilir).
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Seçili satırlar (1 tabanlı olay numaraları): "1,3,5-9". Varsayılan: hepsi.
        #[arg(long)]
        select: Option<String>,
        /// Başsız diyalog değeri: ad=değer (tekrarlanabilir). `__button=Ad` düğmeyi seçer.
        #[arg(long = "param", short = 'p')]
        params: Vec<String>,
        #[arg(long)]
        fps: Option<f64>,
    },
}

fn read_doc(path: &Path, fps: f64) -> Result<Document, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    let text = String::from_utf8_lossy(&bytes);
    let fmt = Format::from_path(&path.to_string_lossy()).ok_or_else(|| format!("{}: bilinmeyen biçim", path.display()))?;
    Ok(convert::load(&text, fmt, fps))
}

fn write_doc(path: &Path, doc: &Document, fps: f64) -> Result<(), String> {
    let fmt = Format::from_path(&path.to_string_lossy()).ok_or_else(|| format!("{}: bilinmeyen biçim", path.display()))?;
    std::fs::write(path, convert::save(doc, fmt, fps)).map_err(|e| format!("{}: {}", path.display(), e))
}

fn parse_selection(spec: &str, n: usize) -> Result<Vec<usize>, String> {
    let mut out = Vec::new();
    for part in spec.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => (a.trim().parse::<usize>(), b.trim().parse::<usize>()),
            None => (part.parse::<usize>(), part.parse::<usize>()),
        };
        let (Ok(a), Ok(b)) = (a, b) else { return Err(format!("geçersiz seçim: {part}")) };
        for i in a.max(1)..=b.min(n) {
            out.push(i - 1);
        }
    }
    Ok(out)
}

fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!("hata: {e}");
        std::process::exit(2);
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.cmd {
        Cmd::Info { file } => {
            let d = read_doc(&file, 23.976)?;
            let (rx, ry) = d.play_res();
            let comments = d.events.iter().filter(|e| e.comment).count();
            let end = d.events.iter().map(|e| e.end).max().unwrap_or(0);
            println!("Çözünürlük : {rx}x{ry}");
            println!("Stiller    : {}", d.styles.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", "));
            println!("Satırlar   : {} ({} yorum)", d.events.len(), comments);
            println!("Son zaman  : {}", time::format_ass(end));
            for w in &d.warnings {
                println!("uyarı: {w}");
            }
        }
        Cmd::Convert { input, output, fps } => {
            let d = read_doc(&input, fps)?;
            write_doc(&output, &d, fps)?;
            eprintln!("{} satır yazıldı -> {}", d.events.len(), output.display());
        }
        Cmd::Qc { file, cps, max_line, max_lines, min_dur, max_dur, min_gap, json, strict } => {
            let d = read_doc(&file, 23.976)?;
            let cfg = QcConfig { max_cps: cps, min_duration_ms: min_dur, max_duration_ms: max_dur, max_line_chars: max_line, max_lines, min_gap_ms: min_gap };
            let issues = qc::check(&d, &cfg);
            if json {
                println!("{}", serde_json::to_string_pretty(&issues).unwrap());
            } else {
                for i in &issues {
                    let sev = match i.severity {
                        Severity::Error => "HATA",
                        Severity::Warning => "UYARI",
                        Severity::Info => "bilgi",
                    };
                    println!("#{:<5} {:<6} {:<18} {}", i.line, sev, i.code, i.message);
                }
                println!("— {} sorun", issues.len());
            }
            if strict && issues.iter().any(|i| i.severity == Severity::Error) {
                std::process::exit(1);
            }
        }
        Cmd::Shift { input, output, by, after } => {
            let mut d = read_doc(&input, 23.976)?;
            let delta = time::parse_duration(&by).ok_or("geçersiz --by")?;
            let after = match after {
                Some(a) => time::parse_duration(&a).ok_or("geçersiz --after")?,
                None => i64::MIN,
            };
            let which: Vec<usize> = (0..d.events.len()).filter(|&i| d.events[i].start >= after).collect();
            timing::shift(&mut d, &which, delta, true, true);
            write_doc(&output, &d, 23.976)?;
            eprintln!("{} satır kaydırıldı", which.len());
        }
        Cmd::Tpp { input, output, lead_in, lead_out, snap_start_before, snap_start_after, snap_end_before, snap_end_after, link_gap, link_bias, keyframes, fps } => {
            let mut d = read_doc(&input, fps)?;
            let kfs: Vec<i64> = match keyframes {
                Some(p) => std::fs::read_to_string(&p)
                    .map_err(|e| e.to_string())?
                    .lines()
                    .filter_map(|l| l.trim().parse::<i64>().ok())
                    .map(|f| time::ms_from_frame(f, fps))
                    .collect(),
                None => vec![],
            };
            let opt = timing::TppOptions { lead_in, lead_out, snap_start_before, snap_start_after, snap_end_before, snap_end_after, link_gap, link_bias };
            let which: Vec<usize> = (0..d.events.len()).collect();
            timing::tpp(&mut d, &which, &opt, &kfs);
            write_doc(&output, &d, fps)?;
        }
        Cmd::Lua { cmd } => lua(cmd)?,
    }
    Ok(())
}

fn lua(cmd: LuaCmd) -> Result<(), String> {
    match cmd {
        LuaCmd::List { scripts } => {
            for p in scripts {
                match Script::load(&p, &HostOptions::default()) {
                    Ok(s) => {
                        println!("{} — {} {}", p.display(), s.meta.name, s.meta.version);
                        for m in &s.macros {
                            let k = if m.kind == MacroKind::Filter { "süzgeç" } else { "makro" };
                            println!("  {}. [{}] {} — {}", m.index, k, m.name, m.description);
                        }
                    }
                    Err(e) => println!("{} — YÜKLENEMEDİ: {}", p.display(), e),
                }
            }
        }
        LuaCmd::Run { script, macro_name, file, output, select, params, fps } => {
            let mut doc = read_doc(&file, fps.unwrap_or(23.976))?;
            let mut map = HashMap::new();
            for p in params {
                let (k, v) = p.split_once('=').ok_or_else(|| format!("--param ad=değer biçiminde olmalı: {p}"))?;
                map.insert(k.to_string(), v.to_string());
            }
            let opts = HostOptions { fps, params: map, subs_path: Some(file.clone()), ..Default::default() };
            let s = Script::load(&script, &opts).map_err(|e| format!("betik yüklenemedi: {e}"))?;
            let idx = match macro_name.parse::<usize>() {
                Ok(n) if n >= 1 && n <= s.macros.len() => n,
                _ => s
                    .macros
                    .iter()
                    .find(|m| m.name == macro_name)
                    .or_else(|| s.macros.iter().find(|m| m.name.to_lowercase().contains(&macro_name.to_lowercase())))
                    .map(|m| m.index)
                    .ok_or_else(|| format!("makro bulunamadı: {macro_name}"))?,
            };
            let sel = match select {
                Some(spec) => parse_selection(&spec, doc.events.len())?,
                None => (0..doc.events.len()).collect(),
            };
            let active = sel.first().copied().unwrap_or(0);
            let out = s.run(idx, &mut doc, &sel, active).map_err(|e| format!("betik hatası:\n{e}"))?;
            for l in &out.log {
                eprintln!("[{}] {}", l.level, l.message.trim_end());
            }
            if out.cancelled {
                eprintln!("makro iptal etti; değişiklik yapılmadı");
                return Ok(());
            }
            if let Some(o) = output {
                write_doc(&o, &doc, fps.unwrap_or(23.976))?;
                eprintln!("yazıldı: {}", o.display());
            }
        }
    }
    Ok(())
}
