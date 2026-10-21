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
    /// input encoding (default: auto-detect; e.g. windows-1254 for legacy Turkish files)
    #[arg(long, global = true)]
    in_encoding: Option<String>,
    /// output encoding (default utf-8)
    #[arg(long, global = true, default_value = "utf-8")]
    out_encoding: String,
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
    /// Rescale to a new script resolution (styles, margins, \pos \fs \bord \clip, drawings...)
    Resample { input: PathBuf, x: i32, y: i32, #[arg(short, long)] output: Option<PathBuf> },
    /// List fonts used (styles + \fn tags); --collect DIR copies the font files that are installed
    Fonts { input: PathBuf, #[arg(long)] collect: Option<PathBuf>, #[arg(long = "font-dir")] font_dir: Vec<PathBuf> },
    /// Text cleanup tools (Subtitle Edit style)
    Clean {
        input: PathBuf,
        /// fix spacing/punctuation/ellipsis/dash errors
        #[arg(long)] fix: bool,
        /// remove [sounds], (descriptions), ♪ music ♪ and SPEAKER: labels
        #[arg(long)] hi: bool,
        /// change case: upper|lower|title|sentence
        #[arg(long)] case: Option<String>,
        /// use Turkish i/İ ı/I casing rules
        #[arg(long)] turkish: bool,
        /// re-break lines longer than N chars into two balanced lines
        #[arg(long)] rebalance: Option<usize>,
        /// merge neighbours when gap <= MS and merged text <= 42 chars
        #[arg(long)] merge_gap: Option<i64>,
        /// split events longer than N chars
        #[arg(long)] split: Option<usize>,
        #[arg(short, long)] output: Option<PathBuf>,
    },
    /// Show document statistics
    Stats { input: PathBuf },
    /// Start the local editor UI (open http://ADDR/ in a browser)
    Serve {
        /// optional subtitle file to open
        file: Option<PathBuf>,
        #[arg(long, default_value = "127.0.0.1:8765")] addr: String,
    },
    /// List macros/filters registered by a Lua automation script
    Macros {
        script: PathBuf,
        #[arg(short = 'I', long = "include")] include: Vec<PathBuf>,
    },
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

thread_local! {
    static ENC: std::cell::RefCell<(Option<String>, String)> = const { std::cell::RefCell::new((None, String::new())) };
}

fn read_text(p: &Path) -> Result<String> {
    let bytes = std::fs::read(p).with_context(|| format!("reading {}", p.display()))?;
    let hint = ENC.with(|e| e.borrow().0.clone());
    let d = subforge::encoding::decode(&bytes, hint.as_deref());
    if d.encoding != "UTF-8" {
        eprintln!("note: {} decoded as {}{}", p.display(), d.encoding, if d.had_errors { " (some bytes invalid)" } else { "" });
    }
    Ok(d.text)
}

fn write_text(p: &Path, text: &str) -> Result<()> {
    let enc = ENC.with(|e| e.borrow().1.clone());
    let bom = ext(p) == "ass" || ext(p) == "ssa";
    std::fs::write(p, subforge::encoding::encode(text, &enc, bom))?;
    Ok(())
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
    write_text(&target, &render(doc, &target))?;
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
    ENC.with(|e| *e.borrow_mut() = (cli.in_encoding.clone(), cli.out_encoding.clone()));
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
            write_text(&output, &render(&d, &output))?;
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
        Cmd::Resample { input, x, y, output } => {
            let mut d = load(&input)?;
            let (ox, oy) = d.res();
            if subforge::resample::resample(&mut d, x, y) {
                eprintln!("resampled {}x{} -> {}x{}", ox, oy, x, y);
            } else {
                eprintln!("already {}x{}", x, y);
            }
            save(&d, &input, &output)?;
        }
        Cmd::Fonts { input, collect, font_dir } => {
            let d = load(&input)?;
            let mut db = subforge::fonts::FontDb::new();
            for f in font_dir {
                db.add_dir(f);
            }
            if let Some(dir) = collect {
                let (copied, missing) = subforge::fontcollect::collect(&d, &mut db, &dir)?;
                for c in &copied {
                    println!("copied {}", c.display());
                }
                for m in &missing {
                    println!("MISSING {} (bold={}, italic={})", m.family, m.bold, m.italic);
                }
                if !missing.is_empty() {
                    std::process::exit(1);
                }
            } else {
                for r in subforge::fontcollect::resolve(&d, &mut db) {
                    println!(
                        "{:<30} {}{} {:>5} line(s)  {}",
                        r.key.family,
                        if r.key.bold { "B" } else { "-" },
                        if r.key.italic { "I" } else { "-" },
                        r.uses,
                        r.path.map(|p| p.display().to_string()).unwrap_or_else(|| "MISSING".into())
                    );
                }
            }
        }
        Cmd::Clean { input, fix, hi, case, turkish, rebalance, merge_gap, split, output } => {
            use subforge::cleanup as c;
            let mut d = load(&input)?;
            if hi {
                let (a, b) = c::remove_hearing_impaired(&mut d, true, true);
                eprintln!("hearing-impaired: {} changed, {} removed", a, b);
            }
            if fix {
                eprintln!("typography: {} event(s) fixed", c::fix_common(&mut d, &c::FixOptions::all()));
            }
            if let Some(m) = case {
                let mode = match m.as_str() {
                    "upper" => c::Casing::Upper,
                    "lower" => c::Casing::Lower,
                    "title" => c::Casing::Title,
                    "sentence" => c::Casing::Sentence,
                    o => bail!("unknown case mode '{}'", o),
                };
                c::change_case_doc(&mut d, mode, turkish, None);
            }
            if let Some(g) = merge_gap {
                eprintln!("merged {} pair(s)", c::merge_short(&mut d, g, 42));
            }
            if let Some(n) = split {
                eprintln!("split {} long event(s)", c::split_long(&mut d, n));
            }
            if let Some(n) = rebalance {
                eprintln!("rebalanced {} event(s)", c::rebalance_doc(&mut d, n));
            }
            save(&d, &input, &output)?;
        }
        Cmd::Stats { input } => {
            let d = load(&input)?;
            let s = subforge::cleanup::stats(&d);
            println!("lines {} (+{} comments), words {}, chars {}", s.lines, s.comments, s.words, s.chars);
            println!("total display time {:.1}s, avg CPS {:.1}, max CPS {:.1}, avg line length {:.1}", s.total_ms as f64 / 1000.0, s.avg_cps, s.max_cps, s.avg_line_len);
        }
        Cmd::Serve { file, addr } => {
            let initial = match file {
                Some(f) => Some((f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), read_text(&f)?)),
                None => None,
            };
            subforge::server::serve(&addr, initial)?;
        }
        Cmd::Macros { script, include } => {
            let doc = Rc::new(RefCell::new(Document::new_default()));
            let s = Script::load(&script, doc, &RunOptions { include_dirs: include, ..Default::default() })?;
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
