mod ass;
mod lua;
mod srt;

use anyhow::{anyhow, Result};
use std::path::Path;

const USAGE: &str = "anisub 0.1 - anime subtitle toolkit

USAGE:
  anisub info <file>                         summary of an .ass/.srt/.vtt file
  anisub convert <in> <out>                  convert between .ass .srt .vtt
  anisub macros <script.lua>                 list macros/filters registered by a script
  anisub run <script.lua> <macro> <in.ass> <out.ass> [--fps N] [--select 1,2,5-9]
";

fn load(path: &str) -> Result<ass::Document> {
    let text = String::from_utf8_lossy(&std::fs::read(path)?).into_owned();
    Ok(match ext(path).as_str() {
        "srt" | "vtt" => srt::parse(&text, (1920, 1080)),
        _ => ass::Document::parse(&text),
    })
}
fn ext(p: &str) -> String {
    Path::new(p).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase()
}
fn save(doc: &ass::Document, path: &str) -> Result<()> {
    let out = match ext(path).as_str() {
        "srt" => srt::write_srt(doc),
        "vtt" => srt::write_vtt(doc),
        _ => doc.to_string(),
    };
    std::fs::write(path, out)?;
    Ok(())
}
fn parse_sel(s: &str) -> Vec<usize> {
    let mut v = Vec::new();
    for part in s.split(',') {
        if let Some((a, b)) = part.split_once('-') {
            if let (Ok(a), Ok(b)) = (a.trim().parse::<usize>(), b.trim().parse::<usize>()) {
                v.extend(a..=b);
            }
        } else if let Ok(x) = part.trim().parse() {
            v.push(x);
        }
    }
    v
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(|s| s.as_str()).collect::<Vec<_>>().as_slice() {
        ["info", f] => {
            let d = load(f)?;
            println!("lines: {}  styles: {}  dialogue: {}", d.entries.len(), d.styles().count(), d.dialogues().count());
            if let (Some(x), Some(y)) = (d.info("PlayResX"), d.info("PlayResY")) {
                println!("resolution: {}x{}", x, y);
            }
        }
        ["convert", i, o] => save(&load(i)?, o)?,
        ["macros", script] => {
            let host = lua::Host::new(&ass::Document::default(), Some(Path::new(script)), 23.976)?;
            host.load_script(Path::new(script))?;
            for (i, m) in host.macros()?.iter().enumerate() {
                println!("{}. [{}] {} - {}", i + 1, m.kind, m.name, m.description);
            }
        }
        ["run", script, name, inp, out, rest @ ..] => {
            let mut fps = 23.976;
            let mut sel = None;
            let mut it = rest.iter();
            while let Some(a) = it.next() {
                match *a {
                    "--fps" => fps = it.next().ok_or_else(|| anyhow!("--fps needs a value"))?.parse()?,
                    "--select" => sel = Some(parse_sel(it.next().ok_or_else(|| anyhow!("--select needs a value"))?)),
                    o => return Err(anyhow!("unknown option {}", o)),
                }
            }
            let doc = load(inp)?;
            let host = lua::Host::new(&doc, Some(Path::new(script)), fps)?;
            host.load_script(Path::new(script))?;
            let result = host.run(name, sel);
            for l in host.log.borrow().iter() {
                eprintln!("{}", l);
            }
            save(&result?, out)?;
        }
        _ => print!("{}", USAGE),
    }
    Ok(())
}
