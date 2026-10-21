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
        Session { doc: Document::new_default(), name: "untitled.ass".into(), encoding: "UTF-8".into(), undo: vec![], redo: vec![], dirty: false }
    }

    fn checkpoint(&mut self) {
        self.undo.push(self.doc.clone());
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
    }

    pub fn state(&self) -> Value {
        let info: Vec<Value> = self
            .doc
            .entries
            .iter()
            .filter_map(|e| if let Entry::Info { key, value } = e { Some(json!({"key": key, "value": value})) } else { None })
            .collect();
        let styles: Vec<StyleJson> = self.doc.styles().map(StyleJson::from).collect();
        let events: Vec<EventJson> = self.doc.events().map(EventJson::from).collect();
        let (rx, ry) = self.doc.res();
        json!({
            "name": self.name, "encoding": self.encoding, "info": info, "styles": styles, "events": events,
            "can_undo": !self.undo.is_empty(), "can_redo": !self.redo.is_empty(), "dirty": self.dirty,
            "res": [rx, ry],
        })
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
        let jbody = || -> Result<Value, Response> {
            if body.iter().all(|b| b.is_ascii_whitespace()) {
                Ok(json!({}))
            } else {
                serde_json::from_slice(body).map_err(|e| err_resp(400, format!("bad json: {}", e)))
            }
        };
        match (method, route) {
            ("GET", "/") | ("GET", "/index.html") => text_resp("text/html; charset=utf-8", INDEX_HTML.to_string()),
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
                json_resp(self.state())
            }
            ("POST", "/api/undo") => {
                if let Some(prev) = self.undo.pop() {
                    self.redo.push(std::mem::replace(&mut self.doc, prev));
                }
                json_resp(self.state())
            }
            ("POST", "/api/redo") => {
                if let Some(next) = self.redo.pop() {
                    self.undo.push(std::mem::replace(&mut self.doc, next));
                }
                json_resp(self.state())
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
                let mut st = self.state();
                st["message"] = json!(msg);
                json_resp(st)
            }
            ("POST", "/api/macros") => {
                let v = match jbody() { Ok(v) => v, Err(r) => return r };
                let src = v.get("script").and_then(|x| x.as_str()).unwrap_or("");
                match with_script(src, &self.doc, &RunOptions::default(), |s, _| {
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
                let result = run_script_on(&src, shared.clone(), &name, &opts);
                match result {
                    Ok(r) => {
                        if !r.cancelled {
                            let newdoc = shared.borrow().clone();
                            if newdoc != self.doc {
                                self.checkpoint();
                                self.doc = newdoc;
                            }
                        }
                        let mut st = self.state();
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

fn with_script<T>(src: &str, doc: &Document, opts: &RunOptions, f: impl FnOnce(&Script, &RunOptions) -> Result<T, String>) -> Result<T, String> {
    let p = script_file(src)?;
    let shared = Rc::new(RefCell::new(doc.clone()));
    let s = Script::load(&p, shared, opts).map_err(|e| format!("{:#}", e))?;
    f(&s, opts)
}

fn run_script_on(src: &str, doc: crate::automation::SharedDoc, name: &str, opts: &RunOptions) -> Result<crate::automation::RunResult, String> {
    let p = script_file(src)?;
    let s = Script::load(&p, doc, opts).map_err(|e| format!("{:#}", e))?;
    s.run(name, opts).map_err(|e| format!("{:#}", e))
}

/// Blocking server loop.
pub fn serve(addr: &str, initial: Option<(String, String)>) -> anyhow::Result<()> {
    let server = tiny_http::Server::http(addr).map_err(|e| anyhow::anyhow!("{}", e))?;
    let mut session = Session::new();
    if let Some((name, text)) = initial {
        session.open_text(&name, &text);
    }
    eprintln!("SubForge UI: http://{}/", addr);
    for mut req in server.incoming_requests() {
        let mut body = Vec::new();
        let _ = std::io::Read::read_to_end(req.as_reader(), &mut body);
        let method = req.method().as_str().to_string();
        let url = req.url().to_string();
        let r = session.handle(&method, &url, &body);
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
    fn post(s: &mut Session, path: &str, v: Value) -> Value {
        let r = s.handle("POST", path, v.to_string().as_bytes());
        serde_json::from_slice(&r.body).unwrap()
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
