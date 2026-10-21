use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use subforge::ass::Document;
use subforge::automation::{RunOptions, Script};
use subforge::{ops, srt};

#[derive(Parser)]
#[command(name = "subforge", version, about = "Anime subtitle toolkit: ASS core, Lua automation (Aegisub-compatible), timing & QA tools")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show script info, styles and event statistics
    Info { file: PathBuf },
    /// Convert between ass/ssa/srt/vtt/txt (format from output extension)
    Convert { input: PathBuf, output: PathBuf },
    /// Shift all (or selected) events by milliseconds (negative allowed)
    Shift { input: PathBuf, ms: i64, #[arg(short, long)] output: Option<PathBuf> },
    /// Convert timing between frame rates (e.g. 23.976 -> 25)
    Fps { input: PathBuf, from: f64, to: f64, #[arg(short, long)] output: Option<PathBuf> },
    /// Two-point resync: map a1->b1 and a2->b2 (milliseconds)
    Resync { input: PathBuf, a1: i64, b1: i64, a2: i64, b2: i64, #[arg(short, long)] output: Option<PathBuf> },
    /// Snap all times to frame boundaries
    Snap { input: PathBuf, fps: f64, #[arg(short, long)] output: Option<PathBuf> },
    /// Check reading speed, durations, overlaps, line length, styles...
    Qa {
        input: PathBuf,
        #[arg(long, default_value_t = 20.0)] max_cps: f64,
        #[arg(long, default_value_t = 42)] max_line_len: usize,
        #[arg(long, default_value_t = 500)] min_duration: i64,
    },
    /// Apply automatic fixes: overlaps, short lines, sort
    Fix { input: PathBuf, #[arg(long, default_value_t = 500)] min_duration: i64, #[arg(short, long)] output: Option<PathBuf> },
    /// Find & replace in event text
    Replace {
        input: PathBuf,
        find: String,
        with: String,
        #[arg(long)] regex: bool,
        #[arg(short, long)] ignore_case: bool,
        #[arg(short, long)] output: Option<PathBuf>,
    },
    /// Start the local editor UI (open http://ADDR/ in a browser)
    Serve {
        /// optional subtitle file to open
        file: Option<PathBuf>,
        #[arg(long, default_value = "127.0.0.1:8765")] addr: String,
    },
    /// List macros/filters registered by a Lua automation script
    Macros { script: PathBuf },
    /// Run an Aegisub-style Lua macro/filter on a subtitle file
    Run {
        input: PathBuf,
        script: PathBuf,
        /// macro name (see `macros`)
        name: String,
        /// comma separated 1-based line indices in the subs list; default: all dialogue lines
        #[arg(long)] select: Option<String>,
        /// answers for aegisub.dialog.display controls: name=value (repeatable)
        #[arg(short, long = "param")] params: Vec<String>,
        #[arg(long)] fps: Option<f64>,
        /// extra include() search directory (repeatable), e.g. Aegisub's automation/include
        #[arg(short = 'I', long = "include")] include: Vec<PathBuf>,
        #[arg(short, long)] output: Option<PathBuf>,
    },
}

fn ext(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn read_text(p: &Path) -> Result<String> {
    let bytes = std::fs::read(p).with_context(|| format!("reading {}", p.display()))?;
    Ok(match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => String::from_utf8_lossy(e.as_bytes()).to_string(),
    })
}

fn load(p: &Path) -> Result<Document> {
    let s = read_text(p)?;
    Ok(match ext(p).as_str() {
        "srt" | "vtt" => srt::parse_srt(&s),
        _ => Document::parse(&s),
    })
}

fn render(doc: &Document, p: &Path) -> String {
    match ext(p).as_str() {
        "srt" => srt::to_srt(doc),
        "vtt" => srt::to_vtt(doc),
        "txt" => doc.events().filter(|e| !e.comment).map(|e| subforge::tags::visible_text(&e.text)).collect::<Vec<_>>().join("\n") + "\n",
        _ => doc.to_string(),
    }
}

fn save(doc: &Document, input: &Path, output: &Option<PathBuf>) -> Result<()> {
    let target = output.clone().unwrap_or_else(|| input.to_path_buf());
    std::fs::write(&target, render(doc, &target))?;
    eprintln!("wrote {}", target.display());
    Ok(())
}

fn main() {
    if let Err(e) = real_main() {
        eprintln!("error: {:#}", e);
        std::process::exit(1);
    }
}

fn real_main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Info { file } => {
            let d = load(&file)?;
            let (rx, ry) = d.res();
            println!("Title: {}", d.info("Title").unwrap_or("-"));
            println!("PlayRes: {}x{}", rx, ry);
            println!("Styles: {}", d.styles().map(|s| s.name.clone()).collect::<Vec<_>>().join(", "));
            let ev: Vec<_> = d.events().collect();
            let dialogue = ev.iter().filter(|e| !e.comment).count();
            println!("Events: {} ({} dialogue, {} comment)", ev.len(), dialogue, ev.len() - dialogue);
            let kara = ev.iter().filter(|e| e.text.contains("\\k")).count();
            println!("Karaoke lines: {}", kara);
            if let (Some(a), Some(b)) = (ev.iter().map(|e| e.start).min(), ev.iter().map(|e| e.end).max()) {
                println!("Span: {} - {}", subforge::time::fmt_ass_time(a), subforge::time::fmt_ass_time(b));
            }
        }
        Cmd::Convert { input, output } => {
            let d = load(&input)?;
            std::fs::write(&output, render(&d, &output))?;
        }
        Cmd::Shift { input, ms, output } => {
            let mut d = load(&input)?;
            ops::shift(&mut d, ms, None);
            save(&d, &input, &output)?;
        }
        Cmd::Fps { input, from, to, output } => {
            let mut d = load(&input)?;
            ops::change_framerate(&mut d, from, to);
            save(&d, &input, &output)?;
        }
        Cmd::Resync { input, a1, b1, a2, b2, output } => {
            let mut d = load(&input)?;
            ops::resync_two_point(&mut d, a1, b1, a2, b2);
            save(&d, &input, &output)?;
        }
        Cmd::Snap { input, fps, output } => {
            let mut d = load(&input)?;
            ops::snap_to_frames(&mut d, fps);
            save(&d, &input, &output)?;
        }
        Cmd::Qa { input, max_cps, max_line_len, min_duration } => {
            let d = load(&input)?;
            let o = ops::QaOptions { max_cps, max_line_len, min_duration_ms: min_duration, ..Default::default() };
            let issues = ops::qa(&d, &o);
            for i in &issues {
                println!("#{:<5} {:<14} {}", i.event, i.kind, i.message);
            }
            println!("{} issue(s)", issues.len());
            if !issues.is_empty() {
                std::process::exit(1);
            }
        }
        Cmd::Fix { input, min_duration, output } => {
            let mut d = load(&input)?;
            d.sort_events_by_start();
            let a = ops::fix_overlaps(&mut d);
            let b = ops::extend_short(&mut d, min_duration);
            eprintln!("fixed {} overlap(s), extended {} short line(s)", a, b);
            save(&d, &input, &output)?;
        }
        Cmd::Replace { input, find, with, regex, ignore_case, output } => {
            let mut d = load(&input)?;
            let n = ops::find_replace(&mut d, &find, &with, regex, ignore_case)?;
            eprintln!("{} event(s) changed", n);
            save(&d, &input, &output)?;
        }
        Cmd::Serve { file, addr } => {
            let initial = match file {
                Some(f) => Some((f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), read_text(&f)?)),
                None => None,
            };
            subforge::server::serve(&addr, initial)?;
        }
        Cmd::Macros { script } => {
            let doc = Rc::new(RefCell::new(Document::new_default()));
            let s = Script::load(&script, doc, &RunOptions::default())?;
            let m = s.meta();
            if let Some(n) = m.get("script_name") {
                println!("{} - {}", n, m.get("script_description").cloned().unwrap_or_default());
            }
            for x in s.list()? {
                println!("{} {}: {}", if x.is_filter { "[filter]" } else { "[macro] " }, x.name, x.description);
            }
        }
        Cmd::Run { input, script, name, select, params, fps, include, output } => {
            let doc = Rc::new(RefCell::new(load(&input)?));
            let mut opts = RunOptions { fps, include_dirs: include, ..Default::default() };
            if let Some(s) = select {
                opts.selection = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            }
            let mut pm = HashMap::new();
            for p in params {
                match p.split_once('=') {
                    Some((k, v)) => {
                        pm.insert(k.to_string(), v.to_string());
                    }
                    None => bail!("--param expects name=value, got '{}'", p),
                }
            }
            opts.params = pm;
            let s = Script::load(&script, doc.clone(), &opts)?;
            let r = s.run(&name, &opts)?;
            for l in &r.log {
                eprint!("{}", l);
                if !l.ends_with('\n') {
                    eprintln!();
                }
            }
            if r.cancelled {
                eprintln!("macro cancelled; document unchanged");
                std::process::exit(2);
            }
            save(&doc.borrow(), &input, &output)?;
        }
    }
    Ok(())
}
