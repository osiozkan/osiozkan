//! Fonts Collector: copy needed font files to a folder and/or zip.

use crate::locate::{locate_all, Located};
use crate::used::used_fonts;
use crate::Error;
use asscore::Script;
use fontdb::Database;
use std::collections::HashSet;
use std::io::Write;
use std::path::PathBuf;

/// What `collect` should do.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Copy font files into this folder (created if needed).
    pub dest_dir: Option<PathBuf>,
    /// Write a deflate-compressed `.zip` containing the font files.
    pub zip_path: Option<PathBuf>,
    /// Only plan: nothing is written and no destination is required.
    pub dry_run: bool,
    /// Also copy substitute fonts (family not found). Off by default.
    pub include_fallback: bool,
}

/// Outcome for one font use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// File written (to the folder and/or zip).
    Copied,
    /// Dry run: file would be written.
    Planned,
    /// Same file already taken by another font use (e.g. bold and regular share a file).
    Duplicate,
    /// Only a substitute exists and `include_fallback` is off.
    Substituted,
    /// No file found.
    Missing,
}

/// One row of the report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub located: Located,
    pub status: Status,
    /// Name the file gets in the destination (None if nothing is copied).
    pub dest_name: Option<String>,
}

/// Collector result.
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub entries: Vec<Entry>,
    /// Unique source files that are (or would be) copied, in order.
    pub files: Vec<PathBuf>,
    pub warnings: Vec<String>,
    pub dry_run: bool,
}

impl Report {
    /// Entries for which no font file was found.
    pub fn missing(&self) -> Vec<&Entry> {
        self.entries.iter().filter(|e| e.status == Status::Missing).collect()
    }
    /// True if every used font resolved to a copied font.
    pub fn is_complete(&self) -> bool {
        self.entries.iter().all(|e| matches!(e.status, Status::Copied | Status::Planned | Status::Duplicate))
    }
    /// Human readable listing, one line per font, then warnings.
    pub fn listing(&self) -> String {
        let mut s = String::new();
        for e in &self.entries {
            let flags = format!("{}{}", if e.located.bold { " bold" } else { "" }, if e.located.italic { " italic" } else { "" });
            let what = match e.status {
                Status::Copied => "copied",
                Status::Planned => "would copy",
                Status::Duplicate => "duplicate",
                Status::Substituted => "substitute only (not copied)",
                Status::Missing => "MISSING",
            };
            let path = e.located.file.as_ref().map(|f| format!(" <- {}", f.path.display())).unwrap_or_default();
            s.push_str(&format!("{}{}: {}{}\n", e.located.family, flags, what, path));
        }
        for w in &self.warnings {
            s.push_str(&format!("warning: {w}\n"));
        }
        s
    }
}

/// Find the fonts `script` uses, resolve them in `db` and copy/zip them as `options` says.
pub fn collect(script: &Script, options: &Options, db: &Database) -> Result<Report, Error> {
    let uses = used_fonts(script);
    collect_located(&locate_all(&uses, db), options)
}

/// Like [`collect`] for already located fonts.
pub fn collect_located(items: &[Located], options: &Options) -> Result<Report, Error> {
    if !options.dry_run && options.dest_dir.is_none() && options.zip_path.is_none() {
        return Err(Error::NoOutput);
    }
    let mut rep = Report { dry_run: options.dry_run, ..Report::default() };
    let mut seen_paths: HashSet<PathBuf> = HashSet::new();
    let mut seen_names: HashSet<String> = HashSet::new();
    let mut plan: Vec<(PathBuf, String)> = Vec::new();
    for l in items {
        let flags = format!("{}{}", if l.bold { " bold" } else { "" }, if l.italic { " italic" } else { "" });
        let (status, dest_name) = match &l.file {
            None => {
                rep.warnings.push(format!("font \"{}\"{} not found", l.family, flags));
                (Status::Missing, None)
            }
            Some(_) if !l.exact && !options.include_fallback => {
                rep.warnings.push(format!("font \"{}\"{} not found; only a substitute is installed", l.family, flags));
                (Status::Substituted, None)
            }
            Some(f) => {
                if !l.exact {
                    rep.warnings.push(format!("font \"{}\"{} not found; copying substitute {}", l.family, flags, f.path.display()));
                }
                let base = f.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "font".into());
                if seen_paths.contains(&f.path) {
                    (Status::Duplicate, Some(base))
                } else {
                    if f.is_collection {
                        rep.warnings.push(format!("{} is a font collection (TTC); the whole file is copied", f.path.display()));
                    }
                    let mut name = base.clone();
                    let mut n = 1;
                    while !seen_names.insert(name.to_lowercase()) {
                        name = match base.rsplit_once('.') {
                            Some((stem, ext)) => format!("{stem}_{n}.{ext}"),
                            None => format!("{base}_{n}"),
                        };
                        n += 1;
                    }
                    seen_paths.insert(f.path.clone());
                    rep.files.push(f.path.clone());
                    plan.push((f.path.clone(), name.clone()));
                    (if options.dry_run { Status::Planned } else { Status::Copied }, Some(name))
                }
            }
        };
        rep.entries.push(Entry { located: l.clone(), status, dest_name });
    }
    if !options.dry_run {
        if let Some(dir) = &options.dest_dir {
            std::fs::create_dir_all(dir)?;
            for (src, name) in &plan {
                std::fs::copy(src, dir.join(name))?;
            }
        }
        if let Some(zp) = &options.zip_path {
            if let Some(parent) = zp.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)?;
            }
            let mut zw = zip::ZipWriter::new(std::fs::File::create(zp)?);
            let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            for (src, name) in &plan {
                zw.start_file(name.as_str(), opts)?;
                zw.write_all(&std::fs::read(src)?)?;
            }
            zw.finish()?;
        }
    }
    Ok(rep)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locate::FontFile;
    use std::io::Read;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("yuzu-fonts-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn loc(fam: &str, bold: bool, path: Option<&PathBuf>, exact: bool, coll: bool) -> Located {
        Located {
            family: fam.into(),
            bold,
            italic: false,
            file: path.map(|p| FontFile { path: p.clone(), index: 0, face_family: fam.into(), is_collection: coll }),
            exact,
        }
    }

    #[test]
    fn needs_an_output_unless_dry_run() {
        assert!(matches!(collect_located(&[], &Options::default()), Err(Error::NoOutput)));
        assert!(collect_located(&[], &Options { dry_run: true, ..Default::default() }).is_ok());
    }

    #[test]
    fn copy_zip_dedupe_missing_and_ttc() {
        let src = tmp("src");
        let a = src.join("a.ttf");
        let c = src.join("c.ttc");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        let a2 = src.join("sub").join("a.ttf");
        std::fs::write(&a, b"AAAA").unwrap();
        std::fs::write(&a2, b"BBBB").unwrap();
        std::fs::write(&c, b"ttcfCCCC").unwrap();
        let items = vec![
            loc("A", false, Some(&a), true, false),
            loc("A", true, Some(&a), true, false), // same file -> duplicate
            loc("A2", false, Some(&a2), true, false), // same file name, different path
            loc("C", false, Some(&c), true, true),
            loc("Gone", false, None, false, false),
            loc("Sub", false, Some(&a), false, false),
        ];
        // dry run writes nothing
        let out = tmp("out");
        let zp = out.join("fonts.zip");
        let dry = collect_located(&items, &Options { dest_dir: Some(out.join("d")), zip_path: Some(zp.clone()), dry_run: true, ..Default::default() }).unwrap();
        assert!(!out.join("d").exists() && !zp.exists());
        assert_eq!(dry.entries[0].status, Status::Planned);
        assert!(dry.listing().contains("would copy"));

        let rep = collect_located(&items, &Options { dest_dir: Some(out.join("d")), zip_path: Some(zp.clone()), ..Default::default() }).unwrap();
        let st: Vec<Status> = rep.entries.iter().map(|e| e.status).collect();
        assert_eq!(st, vec![Status::Copied, Status::Duplicate, Status::Copied, Status::Copied, Status::Missing, Status::Substituted]);
        assert_eq!(rep.entries[2].dest_name.as_deref(), Some("a_1.ttf"));
        assert_eq!(rep.files.len(), 3);
        assert_eq!(rep.missing().len(), 1);
        assert!(!rep.is_complete());
        assert!(rep.warnings.iter().any(|w| w.contains("TTC")));
        assert!(rep.warnings.iter().any(|w| w.contains("\"Gone\"")));
        assert_eq!(std::fs::read(out.join("d/a.ttf")).unwrap(), b"AAAA");
        assert_eq!(std::fs::read(out.join("d/a_1.ttf")).unwrap(), b"BBBB");
        assert_eq!(std::fs::read(out.join("d/c.ttc")).unwrap(), b"ttcfCCCC");

        let mut z = zip::ZipArchive::new(std::fs::File::open(&zp).unwrap()).unwrap();
        assert_eq!(z.len(), 3);
        let mut s = Vec::new();
        z.by_name("c.ttc").unwrap().read_to_end(&mut s).unwrap();
        assert_eq!(s, b"ttcfCCCC");

        // include_fallback copies the substitute (as a duplicate path here)
        let only_sub = vec![loc("Sub", false, Some(&c), false, false)];
        let r = collect_located(&only_sub, &Options { dry_run: true, include_fallback: true, ..Default::default() }).unwrap();
        assert_eq!(r.entries[0].status, Status::Planned);
        assert!(r.warnings.iter().any(|w| w.contains("substitute")));
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn collect_with_empty_db_reports_missing() {
        let mut s = Script::new();
        s.events.push(asscore::Event::new(0, 1, "Default", "hi"));
        let r = collect(&s, &Options { dry_run: true, ..Default::default() }, &Database::new()).unwrap();
        assert_eq!(r.missing().len(), 1);
        assert!(r.listing().contains("MISSING"));
    }
}
