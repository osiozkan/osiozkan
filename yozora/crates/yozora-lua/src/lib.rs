//! Aegisub Automation 4 (Lua) uyumluluk katmanı.
//!
//! Her betik kendi LuaJIT durumunda çalışır (Aegisub gibi). `subs` nesnesi, `aegisub.*` API'si,
//! `utils.lua`, `karaskel.lua`, `unicode.lua` ve `re` modülü sağlanır. Başsız (GUI'siz) modda
//! diyaloglar varsayılan değerleri ya da `params` ile verilen değerleri döndürür.
mod fonts;
mod native;

use mlua::{Lua, Table, Value};
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use yozora_core::{Color, Document, Event, InfoLine, Style};

const PRELUDE: &str = include_str!("../lua/prelude.lua");
const UTILS: &str = include_str!("../lua/utils.lua");
const KARASKEL: &str = include_str!("../lua/karaskel.lua");
const UNICODE: &str = include_str!("../lua/unicode.lua");

#[derive(Debug)]
pub struct ScriptError(pub String);
impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ScriptError {}
impl From<mlua::Error> for ScriptError {
    fn from(e: mlua::Error) -> Self {
        ScriptError(e.to_string())
    }
}
type Result<T> = std::result::Result<T, ScriptError>;

#[derive(Debug, Clone, Default)]
pub struct HostOptions {
    pub fps: Option<f64>,
    /// Başsız diyalog değerleri: kontrol adı -> değer. `__button` basılacak düğmeyi seçer.
    pub params: HashMap<String, String>,
    /// Anahtar kareler (ms).
    pub keyframes_ms: Vec<i64>,
    pub video_size: Option<(u32, u32)>,
    pub subs_path: Option<PathBuf>,
    pub include_dirs: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroKind {
    Macro,
    Filter,
}

#[derive(Debug, Clone)]
pub struct MacroInfo {
    /// Betik içindeki 1 tabanlı sıra numarası.
    pub index: usize,
    pub kind: MacroKind,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Default)]
pub struct ScriptMeta {
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub level: i64,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct RunOutput {
    /// Makronun döndürdüğü yeni seçim (olay indeksleri, 0 tabanlı). `None` ise değişmedi.
    pub selection: Option<Vec<usize>>,
    pub active: Option<usize>,
    pub log: Vec<LogEntry>,
    pub cancelled: bool,
}

pub struct Script {
    lua: Lua,
    funcs: Table,
    pub path: PathBuf,
    pub meta: ScriptMeta,
    pub macros: Vec<MacroInfo>,
}

impl Script {
    pub fn load(path: &Path, opts: &HostOptions) -> Result<Script> {
        let src = std::fs::read(path).map_err(|e| ScriptError(format!("{}: {}", path.display(), e)))?;
        let src = String::from_utf8_lossy(&src).into_owned();
        let src = src.strip_prefix('\u{feff}').unwrap_or(&src).to_string();

        // SAFETY: betikler kullanıcı tarafından güvenilir kabul edilir (Aegisub'da da kısıtsız çalışır);
        // ffi/debug kütüphaneleri bazı betikler için gereklidir.
        let lua = unsafe { Lua::unsafe_new() };
        let native = native::build(&lua, path, opts)?;
        let chunk = lua.load(PRELUDE).set_name("=yozora-prelude");
        let funcs: Table = chunk.call(native)?;
        let g = lua.globals();
        g.set("__yozora", funcs.clone())?;

        let chunk = lua.load(&src).set_name(format!("@{}", path.display()));
        chunk.exec()?;

        let get = |k: &str| -> String { g.get::<Option<String>>(k).ok().flatten().unwrap_or_default() };
        let meta = ScriptMeta {
            name: get("script_name"),
            description: get("script_description"),
            author: get("script_author"),
            version: get("script_version"),
        };
        let describe: mlua::Function = funcs.get("describe")?;
        let list: Table = describe.call(())?;
        let mut macros = Vec::new();
        for (i, v) in list.sequence_values::<Table>().enumerate() {
            let t = v?;
            let kind: String = t.get(1)?;
            macros.push(MacroInfo {
                index: i + 1,
                kind: if kind == "filter" { MacroKind::Filter } else { MacroKind::Macro },
                name: t.get(2)?,
                description: t.get(3)?,
            });
        }
        Ok(Script { lua, funcs, path: path.to_path_buf(), meta, macros })
    }

    /// Makroyu belgeye uygular. `selection` olay indeksleridir (0 tabanlı).
    /// Hata ya da iptal durumunda belge değiştirilmez.
    pub fn run(&self, macro_index: usize, doc: &mut Document, selection: &[usize], active: usize) -> Result<RunOutput> {
        let (lines, offset) = doc_to_lua(&self.lua, doc)?;
        let sel = self.lua.create_table()?;
        for (i, s) in selection.iter().enumerate() {
            sel.set(i + 1, s + offset + 1)?;
        }
        let run: mlua::Function = self.funcs.get("run")?;
        let res: Table = run.call((macro_index, lines, sel, active + offset + 1))?;
        let log = self.take_log();
        let ok: bool = res.get("ok")?;
        if !ok {
            if res.get::<Option<bool>>("cancelled")?.unwrap_or(false) {
                return Ok(RunOutput { log, cancelled: true, ..Default::default() });
            }
            let err: String = res.get::<Option<String>>("err")?.unwrap_or_else(|| "bilinmeyen hata".into());
            return Err(ScriptError(err));
        }
        let new_lines: Table = res.get("lines")?;
        let ev_map = lua_to_doc(&new_lines, doc)?;
        let conv = |v: i64| -> Option<usize> { ev_map.get(&v).copied() };
        let selection = match res.get::<Option<Table>>("sel")? {
            Some(t) => Some(t.sequence_values::<i64>().filter_map(|v| v.ok().and_then(conv)).collect()),
            None => None,
        };
        let active = res.get::<Option<i64>>("active")?.and_then(conv);
        Ok(RunOutput { selection, active, log, cancelled: false })
    }

    /// Makronun `validate` işlevini çalıştırır (yoksa `true`).
    pub fn validate(&self, macro_index: usize, doc: &Document, selection: &[usize], active: usize) -> Result<bool> {
        let (lines, offset) = doc_to_lua(&self.lua, doc)?;
        let sel = self.lua.create_table()?;
        for (i, s) in selection.iter().enumerate() {
            sel.set(i + 1, s + offset + 1)?;
        }
        let f: mlua::Function = self.funcs.get("validate")?;
        Ok(f.call((macro_index, lines, sel, active + offset + 1))?)
    }

    fn take_log(&self) -> Vec<LogEntry> {
        let Ok(f) = self.funcs.get::<mlua::Function>("take_log") else { return vec![] };
        let Ok(t) = f.call::<Table>(()) else { return vec![] };
        t.sequence_values::<Table>()
            .filter_map(|v| v.ok())
            .map(|e| LogEntry { level: e.get(1).unwrap_or(1), message: e.get(2).unwrap_or_default() })
            .collect()
    }
}

// ───────────────────── Belge <-> Lua dönüşümü ─────────────────────

/// Döndürür: (satır dizisi, olaylardan önceki satır sayısı).
fn doc_to_lua(lua: &Lua, doc: &Document) -> Result<(Table, usize)> {
    let lines = lua.create_table()?;
    let mut n = 0usize;
    for l in &doc.info {
        if let InfoLine::Pair { key, value } = l {
            let t = lua.create_table()?;
            t.set("class", "info")?;
            t.set("section", "Script Info")?;
            t.set("key", key.as_str())?;
            t.set("value", value.as_str())?;
            n += 1;
            lines.set(n, t)?;
        }
    }
    for s in &doc.styles {
        let t = lua.create_table()?;
        t.set("class", "style")?;
        t.set("section", "V4+ Styles")?;
        t.set("name", s.name.as_str())?;
        t.set("fontname", s.fontname.as_str())?;
        t.set("fontsize", s.fontsize)?;
        t.set("color1", s.primary.to_style_string())?;
        t.set("color2", s.secondary.to_style_string())?;
        t.set("color3", s.outline_color.to_style_string())?;
        t.set("color4", s.back.to_style_string())?;
        t.set("bold", s.bold)?;
        t.set("italic", s.italic)?;
        t.set("underline", s.underline)?;
        t.set("strikeout", s.strikeout)?;
        t.set("scale_x", s.scale_x)?;
        t.set("scale_y", s.scale_y)?;
        t.set("spacing", s.spacing)?;
        t.set("angle", s.angle)?;
        t.set("borderstyle", s.border_style)?;
        t.set("outline", s.outline)?;
        t.set("shadow", s.shadow)?;
        t.set("align", s.alignment)?;
        t.set("margin_l", s.margin_l)?;
        t.set("margin_r", s.margin_r)?;
        t.set("margin_t", s.margin_v)?;
        t.set("margin_b", s.margin_v)?;
        t.set("encoding", s.encoding)?;
        t.set("relative_to", 2)?;
        n += 1;
        lines.set(n, t)?;
    }
    let offset = n;
    for e in &doc.events {
        let t = lua.create_table()?;
        t.set("class", "dialogue")?;
        t.set("section", "Events")?;
        t.set("comment", e.comment)?;
        t.set("layer", e.layer)?;
        t.set("start_time", e.start)?;
        t.set("end_time", e.end)?;
        t.set("style", e.style.as_str())?;
        t.set("actor", e.actor.as_str())?;
        t.set("margin_l", e.margin_l)?;
        t.set("margin_r", e.margin_r)?;
        t.set("margin_t", e.margin_v)?;
        t.set("margin_b", e.margin_v)?;
        t.set("effect", e.effect.as_str())?;
        t.set("text", e.text.as_str())?;
        t.set("extra", lua.create_table()?)?;
        n += 1;
        lines.set(n, t)?;
    }
    Ok((lines, offset))
}

fn s(t: &Table, k: &str) -> String {
    match t.get::<Value>(k) {
        Ok(Value::String(v)) => v.to_string_lossy().to_string(),
        Ok(Value::Integer(i)) => i.to_string(),
        Ok(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}
fn f(t: &Table, k: &str, d: f64) -> f64 {
    t.get::<Option<f64>>(k).ok().flatten().unwrap_or(d)
}
fn i(t: &Table, k: &str, d: i32) -> i32 {
    f(t, k, d as f64).round() as i32
}
fn b(t: &Table, k: &str) -> bool {
    match t.get::<Value>(k) {
        Ok(Value::Boolean(v)) => v,
        Ok(Value::Integer(n)) => n != 0,
        Ok(Value::Number(n)) => n != 0.0,
        _ => false,
    }
}
fn col(t: &Table, k: &str, d: Color) -> Color {
    Color::parse(&s(t, k)).unwrap_or(d)
}

/// Lua satırlarını belgeye yazar; (Lua indeksi -> olay indeksi) eşlemesini döndürür.
fn lua_to_doc(lines: &Table, doc: &mut Document) -> Result<HashMap<i64, usize>> {
    let mut info: Vec<InfoLine> = doc.info.iter().filter(|l| matches!(l, InfoLine::Raw(_))).cloned().collect();
    let mut styles = Vec::new();
    let mut events = Vec::new();
    let mut ev_map: HashMap<i64, usize> = HashMap::new();
    for (n, v) in lines.sequence_values::<Table>().enumerate() {
        let t = v?;
        match s(&t, "class").as_str() {
            "info" => {
                info.push(InfoLine::Pair { key: s(&t, "key"), value: s(&t, "value") });
            }
            "style" => {
                let d = Style::default();
                styles.push(Style {
                    name: s(&t, "name"),
                    fontname: s(&t, "fontname"),
                    fontsize: f(&t, "fontsize", d.fontsize),
                    primary: col(&t, "color1", d.primary),
                    secondary: col(&t, "color2", d.secondary),
                    outline_color: col(&t, "color3", d.outline_color),
                    back: col(&t, "color4", d.back),
                    bold: b(&t, "bold"),
                    italic: b(&t, "italic"),
                    underline: b(&t, "underline"),
                    strikeout: b(&t, "strikeout"),
                    scale_x: f(&t, "scale_x", 100.0),
                    scale_y: f(&t, "scale_y", 100.0),
                    spacing: f(&t, "spacing", 0.0),
                    angle: f(&t, "angle", 0.0),
                    border_style: i(&t, "borderstyle", 1),
                    outline: f(&t, "outline", 2.0),
                    shadow: f(&t, "shadow", 2.0),
                    alignment: i(&t, "align", 2),
                    margin_l: i(&t, "margin_l", 0),
                    margin_r: i(&t, "margin_r", 0),
                    margin_v: i(&t, "margin_t", i(&t, "margin_v", 0)),
                    encoding: i(&t, "encoding", 1),
                });
            }
            "dialogue" => {
                ev_map.insert(n as i64 + 1, events.len());
                events.push(Event {
                    comment: b(&t, "comment"),
                    layer: i(&t, "layer", 0),
                    start: f(&t, "start_time", 0.0).round() as i64,
                    end: f(&t, "end_time", 0.0).round() as i64,
                    style: s(&t, "style"),
                    actor: s(&t, "actor"),
                    margin_l: i(&t, "margin_l", 0),
                    margin_r: i(&t, "margin_r", 0),
                    margin_v: i(&t, "margin_t", i(&t, "margin_v", 0)),
                    effect: s(&t, "effect"),
                    text: s(&t, "text"),
                });
            }
            _ => {}
        }
    }
    doc.info = info;
    doc.styles = styles;
    doc.events = events;
    Ok(ev_map)
}

/// Gömülü kitaplık kaynakları (include arama yolunun son durağı).
pub(crate) fn embedded_libs() -> Vec<(&'static str, &'static str)> {
    vec![
        ("utils.lua", UTILS),
        ("utils-auto4.lua", UTILS),
        ("karaskel.lua", KARASKEL),
        ("karaskel-auto4.lua", KARASKEL),
        ("unicode.lua", UNICODE),
    ]
}

#[cfg(test)]
mod tests;
