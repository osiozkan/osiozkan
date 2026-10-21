//! Local HTTP server that powers the SubForge editor UI. All editing logic lives in Rust;
//! the UI is a thin client. `Session::handle` is socket-free so it can be unit tested.

use crate::ass::{Document, Entry, Event, Style};
use crate::automation::{RunOptions, Script};
use crate::{ops, srt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub const INDEX_HTML: &str = include_str!("../ui/index.html");
pub const RENDER_JS: &str = include_str!("../ui/render.js");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EventJson {
    pub comment: bool,
    pub layer: i32,
    pub start: i64,
    pub end: i64,
    pub style: String,
    pub actor: String,
    pub ml: i32,
    pub mr: i32,
    pub mv: i32,
    pub effect: String,
    pub text: String,
}

impl From<&Event> for EventJson {
    fn from(e: &Event) -> Self {
        EventJson {
            comment: e.comment,
            layer: e.layer,
            start: e.start,
            end: e.end,
            style: e.style.clone(),
            actor: e.actor.clone(),
            ml: e.margin_l,
            mr: e.margin_r,
            mv: e.margin_v,
            effect: e.effect.clone(),
            text: e.text.clone(),
        }
    }
}
impl From<EventJson> for Event {
    fn from(e: EventJson) -> Self {
        Event {
            comment: e.comment,
            layer: e.layer,
            start: e.start,
            end: e.end,
            style: e.style,
            actor: e.actor,
            margin_l: e.ml,
            margin_r: e.mr,
            margin_v: e.mv,
            effect: e.effect,
            text: e.text,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StyleJson {
    pub name: String,
    pub fontname: String,
    pub fontsize: f64,
    pub colors: [String; 4],
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
    pub scale_x: f64,
    pub scale_y: f64,
    pub spacing: f64,
    pub angle: f64,
    pub border_style: i32,
    pub outline: f64,
    pub shadow: f64,
    pub align: i32,
    pub ml: i32,
    pub mr: i32,
    pub mv: i32,
    pub encoding: i32,
}

impl From<&Style> for StyleJson {
    fn from(s: &Style) -> Self {
        StyleJson {
            name: s.name.clone(),
            fontname: s.fontname.clone(),
            fontsize: s.fontsize,
            colors: [0, 1, 2, 3].map(|i| crate::ass::fmt_color(s.colors[i])),
            bold: s.bold,
            italic: s.italic,
            underline: s.underline,
            strikeout: s.strikeout,
            scale_x: s.scale_x,
            scale_y: s.scale_y,
            spacing: s.spacing,
            angle: s.angle,
            border_style: s.border_style,
            outline: s.outline,
            shadow: s.shadow,
            align: s.align,
            ml: s.margin_l,
            mr: s.margin_r,
            mv: s.margin_v,
            encoding: s.encoding,
        }
    }
}
impl From<StyleJson> for Style {
    fn from(s: StyleJson) -> Self {
        Style {
            name: s.name,
            fontname: s.fontname,
            fontsize: s.fontsize,
            colors: [0, 1, 2, 3].map(|i| crate::ass::parse_color(&s.colors[i])),
            bold: s.bold,
            italic: s.italic,
            underline: s.underline,
            strikeout: s.strikeout,
            scale_x: s.scale_x,
            scale_y: s.scale_y,
            spacing: s.spacing,
            angle: s.angle,
            border_style: s.border_style,
            outline: s.outline,
            shadow: s.shadow,
            align: s.align,
            margin_l: s.ml,
            margin_r: s.mr,
            margin_v: s.mv,
            encoding: s.encoding,
        }
    }
}

pub struct Session {
    pub doc: Document,
    pub name: String,
    pub encoding: String,
    pub path: Option<std::path::PathBuf>,
    pub reference: Option<Document>,
    last_autosave: Option<std::time::Instant>,
    undo: Vec<Document>,
    redo: Vec<Document>,
    pub dirty: bool,
}

pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

fn json_resp(v: Value) -> Response {
    Response { status: 200, content_type: "application/json; charset=utf-8", body: v.to_string().into_bytes() }
}
fn err_resp(status: u16, msg: impl ToString) -> Response {
    Response { status, content_type: "application/json; charset=utf-8", body: json!({"error": msg.to_string()}).to_string().into_bytes() }
}
fn text_resp(ct: &'static str, s: String) -> Response {
    Response { status: 200, content_type: ct, body: s.into_bytes() }
}

/// Position in `entries` of the n-th event / style.
fn nth_pos(doc: &Document, n: usize, events: bool) -> Option<usize> {
    doc.entries
        .iter()
        .enumerate()
        .filter(|(_, e)| if events { matches!(e, Entry::Event(_)) } else { matches!(e, Entry::Style(_)) })
        .nth(n)
        .map(|(i, _)| i)
}

fn as_i(v: &Value, k: &str) -> Option<usize> {
    v.get(k).and_then(|x| x.as_u64()).map(|x| x as usize)
}

/// Apply one patch operation. Errors leave the doc untouched by the caller (it works on a clone).
fn apply_op(doc: &mut Document, op: &Value) -> Result<(), String> {
    let kind = op.get("op").and_then(|v| v.as_str()).ok_or("op missing")?;
    match kind {
        "set_event" => {
            let i = as_i(op, "i").ok_or("i missing")?;
            let pos = nth_pos(doc, i, true).ok_or("event index out of range")?;
            let ev: EventJson = serde_json::from_value(op.get("event").cloned().ok_or("event missing")?).map_err(|e| e.to_string())?;
            doc.entries[pos] = Entry::Event(ev.into());
        }
        "insert_event" => {
            // insert before event index `at` (or append when at >= count)
            let at = as_i(op, "at").unwrap_or(usize::MAX);
            let ev: EventJson = serde_json::from_value(op.get("event").cloned().ok_or("event missing")?).map_err(|e| e.to_string())?;
            let pos = nth_pos(doc, at, true).unwrap_or(doc.entries.len());
            doc.entries.insert(pos, Entry::Event(ev.into()));
        }
        "delete_events" => {
            let mut idx: Vec<usize> = op.get("idx").and_then(|v| v.as_array()).ok_or("idx missing")?.iter().filter_map(|x| x.as_u64()).map(|x| x as usize).collect();
            idx.sort_unstable();
            idx.dedup();
            let mut pos: Vec<usize> = idx.iter().filter_map(|i| nth_pos(doc, *i, true)).collect();
            pos.sort_unstable();
            for p in pos.into_iter().rev() {
                doc.entries.remove(p);
            }
        }
        "set_style" => {
            let i = as_i(op, "i").ok_or("i missing")?;
            let pos = nth_pos(doc, i, false).ok_or("style index out of range")?;
            let st: StyleJson = serde_json::from_value(op.get("style").cloned().ok_or("style missing")?).map_err(|e| e.to_string())?;
            let old = if let Entry::Style(s) = &doc.entries[pos] { s.name.clone() } else { String::new() };
            let new_name = st.name.clone();
            doc.entries[pos] = Entry::Style(st.into());
            if op.get("rename_events").and_then(|v| v.as_bool()).unwrap_or(true) && old != new_name {
                for e in doc.events_mut() {
                    if e.style == old {
                        e.style = new_name.clone();
                    }
                }
            }
        }
        "add_style" => {
            let mut st = Style::default();
            let base = op.get("name").and_then(|v| v.as_str()).unwrap_or("New style");
            let mut name = base.to_string();
            let mut n = 1;
            while doc.style(&name).is_some() {
                n += 1;
                name = format!("{} {}", base, n);
            }
            st.name = name;
            let pos = doc.entries.iter().rposition(|e| matches!(e, Entry::Style(_) | Entry::Info { .. })).map(|p| p + 1).unwrap_or(0);
            doc.entries.insert(pos, Entry::Style(st));
        }
        "delete_style" => {
            let i = as_i(op, "i").ok_or("i missing")?;
            let pos = nth_pos(doc, i, false).ok_or("style index out of range")?;
            doc.entries.remove(pos);
        }
        "set_info" => {
            let k = op.get("key").and_then(|v| v.as_str()).ok_or("key missing")?;
            let v = op.get("value").and_then(|v| v.as_str()).ok_or("value missing")?;
            doc.set_info(k, v);
        }
        other => return Err(format!("unknown op '{}'", other)),
    }
    Ok(())
}

impl Session {
    pub fn new() -> Session {
        Session { doc: Document::new_default(), name: "untitled.ass".into(), encoding: "UTF-8".into(), path: None, reference: None, last_autosave: None, undo: vec![], redo: vec![], dirty: false }
    }

    fn checkpoint(&mut self) {
        self.undo.push(self.doc.clone());
        // bound memory: at most 200 steps and ~3M cloned entries in total
        while self.undo.len() > 200 || (self.undo.len() > 1 && self.undo.len() * self.doc.entries.len() > 3_000_000) {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
    }

    pub fn state(&self) -> Value {
        self.state_inner(true)
    }

    fn state_inner(&self, with_events: bool) -> Value {
        let info: Vec<Value> = self
            .doc
            .entries
            .iter()
            .filter_map(|e| if let Entry::Info { key, value } = e { Some(json!({"key": key, "value": value})) } else { None })
            .collect();
        let styles: Vec<StyleJson> = self.doc.styles().map(StyleJson::from).collect();
        let events: Vec<EventJson> = if with_events { self.events_json() } else { vec![] };
        let (rx, ry) = self.doc.res();
        json!({
            "name": self.name, "encoding": self.encoding, "path": self.path.as_ref().map(|p| p.display().to_string()), "info": info, "styles": styles, "events": events,
            "can_undo": !self.undo.is_empty(), "can_redo": !self.redo.is_empty(), "dirty": self.dirty,
            "res": [rx, ry],
        })
    }

    fn events_json(&self) -> Vec<EventJson> {
        self.doc.events().map(EventJson::from).collect()
    }

    /// State with the events replaced by a splice (`from`, `del`, `ins`) relative to `before`:
    /// common prefix/suffix are skipped so edits on huge files stay cheap to ship.
    fn delta(&self, before: &[EventJson]) -> Value {
        let now = self.events_json();
        let mut a = 0;
        while a < before.len() && a < now.len() && before[a] == now[a] {
            a += 1;
        }
        let mut b = 0;
        while b < before.len() - a && b < now.len() - a && before[before.len() - 1 - b] == now[now.len() - 1 - b] {
            b += 1;
        }
        let mut st = self.state_without_events();
        st["splice"] = json!({"before": before.len(), "from": a, "del": before.len() - a - b, "ins": &now[a..now.len() - b]});
        st
    }

    fn state_without_events(&self) -> Value {
        let mut st = self.state_inner(false);
        st["events"] = Value::Null;
        st
    }

    /// Crash-recovery copy of unsaved work (at most one write every 10 s).
    fn maybe_autosave(&mut self) {
        if !self.dirty {
            return;
        }
        if self.last_autosave.map(|t| t.elapsed().as_secs() < 10).unwrap_or(false) {
            return;
        }
        self.last_autosave = Some(std::time::Instant::now());
        let dir = std::env::var_os("SUBFORGE_AUTOSAVE_DIR")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".cache/subforge/autosave")))
            .unwrap_or_else(|| std::env::temp_dir().join("subforge-autosave"));
        if std::fs::create_dir_all(&dir).is_ok() {
            let stem = std::path::Path::new(&self.name).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "untitled".into());
            let _ = std::fs::write(dir.join(format!("{}.autosave.ass", stem)), self.doc.to_string());
        }
    }

    pub fn open_text(&mut self, name: &str, text: &str) {
        self.checkpoint();
        let lower = name.to_lowercase();
        self.doc = if lower.ends_with(".srt") || lower.ends_with(".vtt") { srt::parse_srt(text) } else { Document::parse(text) };
        self.name = name.to_string();
        self.undo.clear();
        self.redo.clear();
        self.dirty = false;
    }

    pub fn handle(&mut self, method: &str, path: &str, body: &[u8]) -> Response {
        let (route, query) = path.split_once('?').unwrap_or((path, ""));
        let q: HashMap<String, String> = query
            .split('&')
            .filter_map(|kv| kv.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let before: Option<Vec<EventJson>> = match route {
            "/api/patch" | "/api/undo" | "/api/redo" | "/api/tool" | "/api/macro/run" if method == "POST" => Some(self.events_json()),
            _ => None,
        };
        let before = before.as_deref().unwrap_or(&[]);
        let r = self.route(method, route, &q, body, before);
        if method == "POST" && matches!(route, "/api/patch" | "/api/undo" | "/api/redo" | "/api/tool" | "/api/macro/run") {
            self.maybe_autosave();
        }
        r
    }

    fn route(&mut self, method: &str, route: &str, q: &HashMap<String, String>, body: &[u8], before: &[EventJson]) -> Response {
        let jbody = || -> Result<Value, Response> {
            if body.iter().all(|b| b.is_ascii_whitespace()) {
                Ok(json!({}))
            } else {
                serde_json::from_slice(body).map_err(|e| err_resp(400, format!("bad json: {}", e)))
            }
        };
        match (method, route) {
            ("GET", "/") | ("GET", "/index.html") => text_resp("text/html; charset=utf-8", INDEX_HTML.to_string()),
            ("GET", "/render.js") => text_resp("text/javascript; charset=utf-8", RENDER_JS.to_string()),
            ("GET", "/favicon.ico") => Response { status: 204, content_type: "image/x-icon", body: vec![] },
            ("GET", "/api/state") => json_resp(self.state()),
            ("POST", "/api/open") => {
                let v = match jbody() { Ok(v) => v, Err(r) => return r };
                let name = v.get("name").and_then(|x| x.as_str()).unwrap_or("untitled.ass");
                let text = v.get("text").and_then(|x| x.as_str()).unwrap_or("");
                self.open_text(name, text);
                json_resp(self.state())
            }
            ("POST", "/api/open_raw") => {
                // raw file bytes in the body; ?name=file.ass&encoding=windows-1254 (optional override)
                let name = q.get("name").map(|n| url_decode(n)).unwrap_or_else(|| "untitled.ass".into());
                let hint = q.get("encoding").map(|e| url_decode(e)).filter(|e| !e.is_empty() && e != "auto");
                let d = crate::encoding::decode(body, hint.as_deref());
                self.open_text(&name, &d.text);
                self.encoding = d.encoding.to_string();
                let mut st = self.state();
                st["decode_errors"] = json!(d.had_errors);
                json_resp(st)
            }
            ("POST", "/api/open_path") => {
                let v = match jbody() { Ok(v) => v, Err(r) => return r };
                let Some(path) = v.get("path").and_then(|x| x.as_str()) else { return err_resp(400, "path missing") };
                let hint = v.get("encoding").and_then(|x| x.as_str()).filter(|e| !e.is_empty() && *e != "auto");
                let bytes = match std::fs::read(path) { Ok(b) => b, Err(e) => return err_resp(400, format!("{}: {}", path, e)) };
                let d = crate::encoding::decode(&bytes, hint);
                let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string());
                self.open_text(&name, &d.text);
                self.encoding = d.encoding.to_string();
                self.path = Some(std::path::PathBuf::from(path));
                let mut st = self.state();
                st["decode_errors"] = json!(d.had_errors);
                json_resp(st)
            }
            ("POST", "/api/save_path") => {
                let v = match jbody() { Ok(v) => v, Err(r) => return r };
                let path = v.get("path").and_then(|x| x.as_str()).map(std::path::PathBuf::from).or_else(|| self.path.clone());
                let Some(path) = path else { return err_resp(400, "no path: use Save As") };
                let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                let text = match ext.as_str() {
                    "srt" => srt::to_srt(&self.doc),
                    "vtt" => srt::to_vtt(&self.doc),
                    "txt" => self.doc.events().filter(|e| !e.comment).map(|e| crate::tags::visible_text(&e.text)).collect::<Vec<_>>().join("\n"),
                    _ => self.doc.to_string(),
                };
                let enc = v.get("encoding").and_then(|x| x.as_str()).unwrap_or("utf-8");
                let bom = v.get("bom").and_then(|x| x.as_bool()).unwrap_or(ext == "ass" || ext == "ssa");
                // keep one backup of the previous version next to the file
                if path.exists() && v.get("backup").and_then(|x| x.as_bool()).unwrap_or(true) {
                    let mut bak = path.clone().into_os_string();
                    bak.push(".bak");
                    let _ = std::fs::copy(&path, std::path::PathBuf::from(bak));
                }
                if let Err(e) = std::fs::write(&path, crate::encoding::encode(&text, enc, bom)) {
                    return err_resp(400, format!("{}: {}", path.display(), e));
                }
                if ext == "ass" || ext == "ssa" {
                    self.name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    self.path = Some(path);
                    self.dirty = false;
                }
                json_resp(self.state())
            }
            ("GET", "/api/fs") => {
                let dir = q.get("path").map(|p| url_decode(p)).filter(|p| !p.is_empty());
                let dir = dir.map(std::path::PathBuf::from).or_else(|| self.path.as_ref().and_then(|p| p.parent().map(|x| x.to_path_buf()))).or_else(|| std::env::current_dir().ok()).unwrap_or_else(|| std::path::PathBuf::from("/"));
                let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
                let mut entries: Vec<Value> = match std::fs::read_dir(&dir) {
                    Ok(rd) => rd
                        .flatten()
                        .filter_map(|e| {
                            let name = e.file_name().to_string_lossy().to_string();
                            if name.starts_with('.') {
                                return None;
                            }
                            let is_dir = e.path().is_dir();
                            Some(json!({"name": name, "dir": is_dir}))
                        })
                        .collect(),
                    Err(e) => return err_resp(400, format!("{}: {}", dir.display(), e)),
                };
                entries.sort_by(|a, b| {
                    (b["dir"].as_bool(), a["name"].as_str().map(|s| s.to_lowercase())).partial_cmp(&(a["dir"].as_bool(), b["name"].as_str().map(|s| s.to_lowercase()))).unwrap_or(std::cmp::Ordering::Equal)
                });
                json_resp(json!({"path": dir.display().to_string(), "parent": dir.parent().map(|p| p.display().to_string()), "entries": entries}))
            }
            ("POST", "/api/ref") => {
                // reference subtitle (e.g. the source-language track) for translation / bilingual merge
                let name = q.get("name").map(|n| url_decode(n)).unwrap_or_default();
                let hint = q.get("encoding").map(|e| url_decode(e)).filter(|e| !e.is_empty() && e != "auto");
                let d = crate::encoding::decode(body, hint.as_deref());
                let lower = name.to_lowercase();
                let doc = if lower.ends_with(".srt") || lower.ends_with(".vtt") { srt::parse_srt(&d.text) } else { Document::parse(&d.text) };
                let evs: Vec<EventJson> = doc.events().filter(|e| !e.comment).map(EventJson::from).collect();
                self.reference = Some(doc);
                json_resp(json!({"name": name, "events": evs, "encoding": d.encoding}))
            }
            ("POST", "/api/ref/clear") => {
                self.reference = None;
                json_resp(json!({"ok": true}))
            }
            ("POST", "/api/new") => {
                *self = Session::new();
                json_resp(self.state())
            }
            ("POST", "/api/patch") => {
                let v = match jbody() { Ok(v) => v, Err(r) => return r };
                let Some(ops_v) = v.get("ops").and_then(|x| x.as_array()) else { return err_resp(400, "ops missing") };
                let mut work = self.doc.clone();
                for op in ops_v {
                    if let Err(e) = apply_op(&mut work, op) {
                        return err_resp(400, e);
                    }
                }
                if work != self.doc {
                    self.checkpoint();
                    self.doc = work;
                }
                json_resp(self.delta(before))
            }
            ("POST", "/api/undo") => {
                if let Some(prev) = self.undo.pop() {
                    self.redo.push(std::mem::replace(&mut self.doc, prev));
                }
                json_resp(self.delta(before))
            }
            ("POST", "/api/redo") => {
                if let Some(next) = self.redo.pop() {
                    self.undo.push(std::mem::replace(&mut self.doc, next));
                }
                json_resp(self.delta(before))
            }
            ("GET", "/api/export") => {
                let fmt = q.get("fmt").map(|s| s.as_str()).unwrap_or("ass");
                let enc = q.get("enc").map(|e| url_decode(e)).unwrap_or_else(|| "utf-8".into());
                let text = match fmt {
                    "srt" => srt::to_srt(&self.doc),
                    "vtt" => srt::to_vtt(&self.doc),
                    "txt" => self.doc.events().filter(|e| !e.comment).map(|e| crate::tags::visible_text(&e.text)).collect::<Vec<_>>().join("\n"),
                    _ => self.doc.to_string(),
                };
                let bom = q.get("bom").map(|b| b != "0").unwrap_or(fmt == "ass");
                Response { status: 200, content_type: "application/octet-stream", body: crate::encoding::encode(&text, &enc, bom) }
            }
            ("GET", "/api/export_text") => {
                let fmt = q.get("fmt").map(|s| s.as_str()).unwrap_or("ass");
                match fmt {
                    "srt" => text_resp("text/plain; charset=utf-8", srt::to_srt(&self.doc)),
                    "vtt" => text_resp("text/vtt; charset=utf-8", srt::to_vtt(&self.doc)),
                    "txt" => text_resp(
                        "text/plain; charset=utf-8",
                        self.doc.events().filter(|e| !e.comment).map(|e| crate::tags::visible_text(&e.text)).collect::<Vec<_>>().join("\n"),
                    ),
                    _ => text_resp("text/plain; charset=utf-8", self.doc.to_string()),
                }
            }
            ("POST", "/api/saved") => {
                self.dirty = false;
                json_resp(self.state())
            }
            ("GET", "/api/stats") => {
                let st = crate::cleanup::stats(&self.doc);
                json_resp(json!({"lines": st.lines, "comments": st.comments, "words": st.words, "chars": st.chars, "total_ms": st.total_ms, "avg_cps": st.avg_cps, "max_cps": st.max_cps, "avg_line_len": st.avg_line_len}))
            }
            ("GET", "/api/fonts") => {
                let mut db = crate::fonts::FontDb::new();
                let list: Vec<Value> = crate::fontcollect::resolve(&self.doc, &mut db)
                    .into_iter()
                    .map(|r| json!({"family": r.key.family, "bold": r.key.bold, "italic": r.key.italic, "uses": r.uses, "installed": r.path.is_some(), "path": r.path.map(|p| p.display().to_string())}))
                    .collect();
                json_resp(json!({"fonts": list}))
            }
            ("POST", "/api/qa") => {
                let v = match jbody() { Ok(v) => v, Err(r) => return r };
                let mut o = ops::QaOptions::default();
                if let Some(x) = v.get("max_cps").and_then(|x| x.as_f64()) { o.max_cps = x; }
                if let Some(x) = v.get("min_duration").and_then(|x| x.as_i64()) { o.min_duration_ms = x; }
                if let Some(x) = v.get("max_line_len").and_then(|x| x.as_u64()) { o.max_line_len = x as usize; }
                let issues: Vec<Value> = ops::qa(&self.doc, &o).into_iter().map(|i| json!({"event": i.event, "kind": i.kind, "message": i.message})).collect();
                json_resp(json!({"issues": issues}))
            }
            ("POST", "/api/tool") => {
                let v = match jbody() { Ok(v) => v, Err(r) => return r };
                let name = v.get("name").and_then(|x| x.as_str()).unwrap_or("");
                let num = |k: &str| v.get(k).and_then(|x| x.as_f64());
                let sel: Option<Vec<usize>> = v.get("selection").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_u64()).map(|x| x as usize).collect());
                let mut work = self.doc.clone();
                let mut msg = String::new();
                match name {
                    "shift" => ops::shift(&mut work, num("ms").unwrap_or(0.0) as i64, sel.as_deref()),
                    "fps" => ops::change_framerate(&mut work, num("from").unwrap_or(23.976), num("to").unwrap_or(25.0)),
                    "snap" => ops::snap_to_frames(&mut work, num("fps").unwrap_or(23.976)),
                    "resync" => ops::resync_two_point(&mut work, num("a1").unwrap_or(0.0) as i64, num("b1").unwrap_or(0.0) as i64, num("a2").unwrap_or(1.0) as i64, num("b2").unwrap_or(1.0) as i64),
                    "sort" => work.sort_events_by_start(),
                    "fix" => {
                        work.sort_events_by_start();
                        let a = ops::fix_overlaps(&mut work);
                        let b = ops::extend_short(&mut work, num("min_duration").unwrap_or(500.0) as i64);
                        msg = format!("fixed {} overlap(s), extended {} short line(s)", a, b);
                    }
                    "strip_tags" => ops::strip_all_tags(&mut work),
                    "merge_ref" => {
                        let Some(r) = &self.reference else { return err_resp(400, "load a reference subtitle first") };
                        let stack = v.get("stack").and_then(|x| x.as_bool()).unwrap_or(true);
                        let style = v.get("style").and_then(|x| x.as_str()).unwrap_or("");
                        let style = if !stack && style.is_empty() { "Secondary" } else { style };
                        msg = format!("{} line(s) merged", ops::merge_reference(&mut work, r, stack, style));
                    }
                    "resample" => {
                        let (x, y) = (num("x").unwrap_or(0.0) as i32, num("y").unwrap_or(0.0) as i32);
                        if x <= 0 || y <= 0 {
                            return err_resp(400, "resample needs positive x and y");
                        }
                        msg = if crate::resample::resample(&mut work, x, y) { format!("resampled to {}x{}", x, y) } else { "already at that resolution".into() };
                    }
                    "fix_common" => msg = format!("{} event(s) fixed", crate::cleanup::fix_common(&mut work, &crate::cleanup::FixOptions::all())),
                    "remove_hi" => {
                        let (a, b) = crate::cleanup::remove_hearing_impaired(&mut work, true, true);
                        msg = format!("{} changed, {} removed", a, b);
                    }
                    "case" => {
                        use crate::cleanup::Casing;
                        let mode = match v.get("mode").and_then(|x| x.as_str()).unwrap_or("sentence") {
                            "upper" => Casing::Upper,
                            "lower" => Casing::Lower,
                            "title" => Casing::Title,
                            _ => Casing::Sentence,
                        };
                        let tr = v.get("turkish").and_then(|x| x.as_bool()).unwrap_or(false);
                        crate::cleanup::change_case_doc(&mut work, mode, tr, sel.as_deref());
                    }
                    "rebalance" => msg = format!("{} event(s) rebalanced", crate::cleanup::rebalance_doc(&mut work, num("max").unwrap_or(42.0) as usize)),
                    "merge_short" => msg = format!("{} merged", crate::cleanup::merge_short(&mut work, num("gap").unwrap_or(250.0) as i64, num("max").unwrap_or(42.0) as usize)),
                    "split_long" => msg = format!("{} split", crate::cleanup::split_long(&mut work, num("max").unwrap_or(84.0) as usize)),
                    "replace" => {
                        let find = v.get("find").and_then(|x| x.as_str()).unwrap_or("");
                        let with = v.get("with").and_then(|x| x.as_str()).unwrap_or("");
                        let rx = v.get("regex").and_then(|x| x.as_bool()).unwrap_or(false);
                        let ic = v.get("ignore_case").and_then(|x| x.as_bool()).unwrap_or(false);
                        match ops::find_replace(&mut work, find, with, rx, ic) {
                            Ok(n) => msg = format!("{} event(s) changed", n),
                            Err(e) => return err_resp(400, e),
                        }
                    }
                    other => return err_resp(400, format!("unknown tool '{}'", other)),
                }
                if work != self.doc {
                    self.checkpoint();
                    self.doc = work;
                }
                let mut st = self.delta(before);
                st["message"] = json!(msg);
                json_resp(st)
            }
            ("POST", "/api/macros") => {
                let v = match jbody() { Ok(v) => v, Err(r) => return r };
                let src = v.get("script").and_then(|x| x.as_str()).unwrap_or("");
                let spath = v.get("path").and_then(|x| x.as_str());
                match with_script(src, spath, &self.doc, &RunOptions::default(), |s, _| {
                    let meta = s.meta();
                    let list = s.list().map_err(|e| e.to_string())?;
                    Ok(json!({"meta": meta, "macros": list.iter().map(|m| json!({"name": m.name, "description": m.description, "filter": m.is_filter})).collect::<Vec<_>>()}))
                }) {
                    Ok(j) => json_resp(j),
                    Err(e) => err_resp(400, e),
                }
            }
            ("POST", "/api/macro/run") => {
                let v = match jbody() { Ok(v) => v, Err(r) => return r };
                let src = v.get("script").and_then(|x| x.as_str()).unwrap_or("").to_string();
                let spath = v.get("path").and_then(|x| x.as_str()).map(|s| s.to_string());
                let name = v.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
                let mut opts = RunOptions::default();
                if let Some(a) = v.get("selection").and_then(|x| x.as_array()) {
                    opts.selection = a.iter().filter_map(|x| x.as_u64()).map(|x| x as usize).collect();
                }
                if let Some(o) = v.get("params").and_then(|x| x.as_object()) {
                    for (k, val) in o {
                        opts.params.insert(k.clone(), val.as_str().map(|s| s.to_string()).unwrap_or_else(|| val.to_string()));
                    }
                }
                opts.fps = v.get("fps").and_then(|x| x.as_f64());
                let shared = Rc::new(RefCell::new(self.doc.clone()));
                let result = run_script_on(&src, spath.as_deref(), shared.clone(), &name, &opts);
                match result {
                    Ok(r) => {
                        if !r.cancelled {
                            let newdoc = shared.borrow().clone();
                            if newdoc != self.doc {
                                self.checkpoint();
                                self.doc = newdoc;
                            }
                        }
                        let mut st = self.delta(before);
                        st["log"] = json!(r.log);
                        st["cancelled"] = json!(r.cancelled);
                        json_resp(st)
                    }
                    Err(e) => err_resp(400, e),
                }
            }
            _ => err_resp(404, "not found"),
        }
    }
}

fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
                out.push(b'%');
                i += 1;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

fn script_file(src: &str) -> Result<std::path::PathBuf, String> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    src.hash(&mut h);
    let dir = std::env::temp_dir().join("subforge-scripts");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let p = dir.join(format!("{:x}.lua", h.finish()));
    std::fs::write(&p, src).map_err(|e| e.to_string())?;
    Ok(p)
}

fn script_path(src: &str, path: Option<&str>) -> Result<std::path::PathBuf, String> {
    match path {
        Some(p) if !p.is_empty() => Ok(std::path::PathBuf::from(p)),
        _ => script_file(src),
    }
}

fn with_script<T>(src: &str, path: Option<&str>, doc: &Document, opts: &RunOptions, f: impl FnOnce(&Script, &RunOptions) -> Result<T, String>) -> Result<T, String> {
    let p = script_path(src, path)?;
    let shared = Rc::new(RefCell::new(doc.clone()));
    let s = Script::load(&p, shared, opts).map_err(|e| format!("{:#}", e))?;
    f(&s, opts)
}

fn run_script_on(src: &str, path: Option<&str>, doc: crate::automation::SharedDoc, name: &str, opts: &RunOptions) -> Result<crate::automation::RunResult, String> {
    let p = script_path(src, path)?;
    let s = Script::load(&p, doc, opts).map_err(|e| format!("{:#}", e))?;
    s.run(name, opts).map_err(|e| format!("{:#}", e))
}

fn make_token() -> String {
    use std::io::Read;
    let mut buf = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        if f.read_exact(&mut buf).is_ok() {
            return buf.iter().map(|b| format!("{:02x}", b)).collect();
        }
    }
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::new();
    for i in 0..2u8 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u8(i);
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
        h.write_u32(std::process::id());
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

/// Blocking server loop. The API is protected against other web pages (CSRF / DNS rebinding):
/// requests need the per-run token embedded in the served page, and a loopback `Host` header.
pub fn serve(addr: &str, initial: Option<(String, String)>) -> anyhow::Result<()> {
    let server = tiny_http::Server::http(addr).map_err(|e| anyhow::anyhow!("{}", e))?;
    let mut session = Session::new();
    if let Some((name, text)) = initial {
        session.open_text(&name, &text);
    }
    let token = make_token();
    let port = addr.rsplit(':').next().unwrap_or("8765").to_string();
    let loopback = addr.starts_with("127.") || addr.starts_with("localhost") || addr.starts_with("[::1]");
    let allowed_hosts = [format!("127.0.0.1:{}", port), format!("localhost:{}", port), format!("[::1]:{}", port)];
    eprintln!("SubForge UI: http://{}/", addr);
    for mut req in server.incoming_requests() {
        let mut body = Vec::new();
        let _ = std::io::Read::read_to_end(req.as_reader(), &mut body);
        let method = req.method().as_str().to_string();
        let url = req.url().to_string();
        let header = |name: &str| -> Option<String> {
            req.headers().iter().find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name)).map(|h| h.value.as_str().to_string())
        };
        let host_ok = !loopback || header("host").map(|h| allowed_hosts.iter().any(|a| a.eq_ignore_ascii_case(&h))).unwrap_or(false);
        let origin_ok = header("origin").map(|o| allowed_hosts.iter().any(|a| o.eq_ignore_ascii_case(&format!("http://{}", a)))).unwrap_or(true);
        let route = url.split('?').next().unwrap_or("");
        let r = if !host_ok || !origin_ok {
            err_resp(403, "forbidden host/origin")
        } else if route.starts_with("/api/") && header("x-subforge-token").as_deref() != Some(token.as_str()) {
            err_resp(403, "missing or wrong token")
        } else {
            let mut r = session.handle(&method, &url, &body);
            if route == "/" || route == "/index.html" {
                r.body = String::from_utf8_lossy(&r.body).replace("__SF_TOKEN__", &token).into_bytes();
            }
            r
        };
        let hdr = tiny_http::Header::from_bytes(&b"Content-Type"[..], r.content_type.as_bytes()).unwrap();
        let _ = req.respond(tiny_http::Response::from_data(r.body).with_status_code(r.status).with_header(hdr));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        let mut s = Session::new();
        s.open_text("a.ass", &std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sample.ass")).unwrap());
        s
    }
    /// Post and, like the UI client, fold any events splice into a full state value.
    fn post(s: &mut Session, path: &str, v: Value) -> Value {
        let r = s.handle("POST", path, v.to_string().as_bytes());
        let mut st: Value = serde_json::from_slice(&r.body).unwrap();
        if st.get("splice").is_some() {
            let full = s.state();
            let sp = st["splice"].clone();
            // verify the splice reproduces the real event list from the previous one
            assert!(sp["from"].as_u64().unwrap() <= sp["before"].as_u64().unwrap());
            st["events"] = full["events"].clone();
        }
        st
    }

    #[test]
    fn state_patch_undo() {
        let mut s = session();
        let st = s.state();
        assert_eq!(st["events"].as_array().unwrap().len(), 3);
        let mut ev = st["events"][1].clone();
        ev["text"] = json!("Changed");
        let st = post(&mut s, "/api/patch", json!({"ops": [{"op": "set_event", "i": 1, "event": ev}]}));
        assert_eq!(st["events"][1]["text"], "Changed");
        assert_eq!(st["can_undo"], true);
        let st = post(&mut s, "/api/undo", json!({}));
        assert_eq!(st["events"][1]["text"], "Hello  world");
        let st = post(&mut s, "/api/redo", json!({}));
        assert_eq!(st["events"][1]["text"], "Changed");
    }

    fn apply_splice(events: &mut Vec<Value>, sp: &Value) {
        assert_eq!(sp["before"].as_u64().unwrap() as usize, events.len(), "client out of sync");
        let from = sp["from"].as_u64().unwrap() as usize;
        let del = sp["del"].as_u64().unwrap() as usize;
        let ins: Vec<Value> = sp["ins"].as_array().unwrap().clone();
        events.splice(from..from + del, ins);
    }

    #[test]
    fn splices_reproduce_full_state() {
        let mut s = session();
        let mut mirror: Vec<Value> = s.state()["events"].as_array().unwrap().clone();
        let ev = mirror[0].clone();
        let steps: Vec<(&str, Value)> = vec![
            ("/api/patch", json!({"ops": [{"op": "insert_event", "at": 1, "event": ev}]})),
            ("/api/patch", json!({"ops": [{"op": "delete_events", "idx": [0, 2]}]})),
            ("/api/tool", json!({"name": "shift", "ms": 250})),
            ("/api/undo", json!({})),
            ("/api/undo", json!({})),
            ("/api/redo", json!({})),
            ("/api/tool", json!({"name": "replace", "find": "o", "with": "0"})),
            ("/api/tool", json!({"name": "sort"})),
        ];
        for (path, body) in steps {
            let r = s.handle("POST", path, body.to_string().as_bytes());
            let st: Value = serde_json::from_slice(&r.body).unwrap();
            apply_splice(&mut mirror, &st["splice"]);
            assert_eq!(Value::Array(mirror.clone()), s.state()["events"], "after {} {}", path, body);
        }
    }

    #[test]
    fn open_raw_detects_turkish_codepage() {
        let mut s = Session::new();
        let text = "[Script Info]\nTitle: T\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,Şimdi gidiyoruz çocuklar, İstanbul'a. Işık ğüöç\n";
        let bytes = crate::encoding::encode(text, "windows-1254", false);
        let r = s.handle("POST", "/api/open_raw?name=tr.ass", &bytes);
        let v: Value = serde_json::from_slice(&r.body).unwrap();
        assert_eq!(v["events"][0]["text"], "Şimdi gidiyoruz çocuklar, İstanbul'a. Işık ğüöç", "enc={}", v["encoding"]);
        let r = s.handle("GET", "/api/export?fmt=ass&enc=windows-1254&bom=0", b"");
        assert_eq!(r.body, crate::encoding::encode(&s.doc.to_string(), "windows-1254", false));
        let r = s.handle("POST", "/api/open_raw?name=x.ass&encoding=windows-1254", &bytes);
        let v: Value = serde_json::from_slice(&r.body).unwrap();
        assert_eq!(v["encoding"], "windows-1254");
    }

    #[test]
    fn insert_delete_and_style_rename() {
        let mut s = session();
        let st = s.state();
        let ev = st["events"][0].clone();
        let st = post(&mut s, "/api/patch", json!({"ops": [{"op": "insert_event", "at": 0, "event": ev}, {"op": "delete_events", "idx": [3]}]}));
        assert_eq!(st["events"].as_array().unwrap().len(), 3);
        let mut sty = st["styles"][0].clone();
        sty["name"] = json!("Main");
        let st = post(&mut s, "/api/patch", json!({"ops": [{"op": "set_style", "i": 0, "style": sty}]}));
        assert_eq!(st["events"][0]["style"], "Main");
        let bad = s.handle("POST", "/api/patch", json!({"ops": [{"op": "set_event", "i": 99, "event": {}}]}).to_string().as_bytes());
        assert_eq!(bad.status, 400);
    }

    #[test]
    fn tools_qa_export_macros() {
        let mut s = session();
        let st = post(&mut s, "/api/tool", json!({"name": "shift", "ms": 1000}));
        assert_eq!(st["events"][0]["start"], 2000);
        let st = post(&mut s, "/api/tool", json!({"name": "replace", "find": "hello", "with": "Bye", "ignore_case": true}));
        assert_eq!(st["message"], "1 event(s) changed");
        let q = post(&mut s, "/api/qa", json!({}));
        assert!(q["issues"].as_array().unwrap().iter().any(|i| i["kind"] == "whitespace") || q["issues"].is_array());
        let r = s.handle("GET", "/api/export_text?fmt=srt", b"");
        assert!(String::from_utf8(r.body).unwrap().contains("-->"));
        let script = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/test_macro.lua")).unwrap();
        let m = post(&mut s, "/api/macros", json!({"script": script}));
        assert_eq!(m["macros"].as_array().unwrap().len(), 5);
        let st = post(&mut s, "/api/macro/run", json!({"script": script, "name": "Kara dup"}));
        assert_eq!(st["events"].as_array().unwrap().len(), 4);
        assert!(st["log"].as_array().unwrap().len() >= 1);
        let st = post(&mut s, "/api/undo", json!({}));
        assert_eq!(st["events"].as_array().unwrap().len(), 3);
    }
}
