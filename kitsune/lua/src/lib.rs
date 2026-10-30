//! Aegisub Automation 4 uyumlu Lua ana bilgisayarı (LuaJIT yerine gömülü Lua 5.1).
use kitsune_core::{Document, Event, Style};
use mlua::{Function, Lua, MetaMethod, MultiValue, Result as LuaResult, Table, UserData, UserDataMethods, Value, Variadic};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const UTILS: &str = include_str!("lua/utils.lua");
const UNICODE: &str = include_str!("lua/unicode.lua");
const KARASKEL: &str = include_str!("lua/karaskel.lua");
const CANCEL: &str = "__kitsune_cancel__";

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Macro,
    Filter,
}

#[derive(Clone)]
pub struct MacroEntry {
    pub name: String,
    pub description: String,
    pub kind: Kind,
    process: Function,
    validate: Option<Function>,
}

#[derive(Debug)]
pub enum RunError {
    Cancelled,
    Lua(String),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::Cancelled => write!(f, "komut iptal edildi"),
            RunError::Lua(s) => write!(f, "{}", s),
        }
    }
}

impl From<mlua::Error> for RunError {
    fn from(e: mlua::Error) -> Self {
        let s = e.to_string();
        if s.contains(CANCEL) {
            RunError::Cancelled
        } else {
            RunError::Lua(s)
        }
    }
}

type Shared = Rc<RefCell<Document>>;

struct Subs(Shared);

fn color_out(c: &str) -> String {
    if c.ends_with('&') { c.to_string() } else { format!("{}&", c) }
}

fn color_in(c: String) -> String {
    c.trim_end_matches('&').to_string()
}

fn entry_table(lua: &Lua, doc: &Document, idx0: usize) -> LuaResult<Table> {
    let t = lua.create_table()?;
    let ni = doc.info.len();
    let ns = doc.styles.len();
    if idx0 < ni {
        let (k, v) = &doc.info[idx0];
        t.set("class", "info")?;
        t.set("section", "Script Info")?;
        t.set("key", k.as_str())?;
        t.set("value", v.as_str())?;
        t.set("raw", format!("{}: {}", k, v))?;
    } else if idx0 < ni + ns {
        let s = &doc.styles[idx0 - ni];
        t.set("class", "style")?;
        t.set("section", "V4+ Styles")?;
        t.set("raw", kitsune_core::ass::style_line(s))?;
        t.set("name", s.name.as_str())?;
        t.set("fontname", s.fontname.as_str())?;
        t.set("fontsize", s.fontsize)?;
        t.set("color1", color_out(&s.primary))?;
        t.set("color2", color_out(&s.secondary))?;
        t.set("color3", color_out(&s.outline_color))?;
        t.set("color4", color_out(&s.back))?;
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
        t.set("margin_v", s.margin_v)?;
        t.set("encoding", s.encoding)?;
    } else {
        let e = &doc.events[idx0 - ni - ns];
        t.set("class", "dialogue")?;
        t.set("section", "[Events]")?;
        t.set("raw", kitsune_core::ass::event_line(e))?;
        t.set("comment", e.comment)?;
        t.set("layer", e.layer)?;
        t.set("start_time", e.start)?;
        t.set("end_time", e.end)?;
        t.set("style", e.style.as_str())?;
        t.set("actor", e.actor.as_str())?;
        t.set("margin_l", e.margin_l)?;
        t.set("margin_r", e.margin_r)?;
        t.set("margin_t", e.margin_t)?;
        t.set("margin_b", e.margin_t)?;
        t.set("margin_v", e.margin_t)?;
        t.set("effect", e.effect.as_str())?;
        t.set("text", e.text.as_str())?;
    }
    Ok(t)
}

enum Entry {
    Info(String, String),
    Style(Style),
    Event(Event),
}

fn get<T: mlua::FromLua>(t: &Table, k: &str, d: T) -> T {
    t.get::<Option<T>>(k).ok().flatten().unwrap_or(d)
}

fn table_entry(t: &Table) -> LuaResult<Entry> {
    let class: String = get(t, "class", String::new());
    match class.to_ascii_lowercase().as_str() {
        "info" => Ok(Entry::Info(get(t, "key", String::new()), get(t, "value", String::new()))),
        "style" => {
            let d = Style::default();
            let mv = get(t, "margin_t", get(t, "margin_v", d.margin_v));
            Ok(Entry::Style(Style {
                name: get(t, "name", d.name),
                fontname: get(t, "fontname", d.fontname),
                fontsize: get(t, "fontsize", d.fontsize),
                primary: color_in(get(t, "color1", d.primary)),
                secondary: color_in(get(t, "color2", d.secondary)),
                outline_color: color_in(get(t, "color3", d.outline_color)),
                back: color_in(get(t, "color4", d.back)),
                bold: get(t, "bold", d.bold),
                italic: get(t, "italic", d.italic),
                underline: get(t, "underline", d.underline),
                strikeout: get(t, "strikeout", d.strikeout),
                scale_x: get(t, "scale_x", d.scale_x),
                scale_y: get(t, "scale_y", d.scale_y),
                spacing: get(t, "spacing", d.spacing),
                angle: get(t, "angle", d.angle),
                border_style: get(t, "borderstyle", d.border_style),
                outline: get(t, "outline", d.outline),
                shadow: get(t, "shadow", d.shadow),
                alignment: get(t, "align", d.alignment),
                margin_l: get(t, "margin_l", d.margin_l),
                margin_r: get(t, "margin_r", d.margin_r),
                margin_v: mv,
                encoding: get(t, "encoding", d.encoding),
            }))
        }
        "dialogue" => {
            let d = Event::default();
            let mt = get(t, "margin_t", get(t, "margin_v", d.margin_t));
            Ok(Entry::Event(Event {
                comment: get(t, "comment", false),
                layer: get(t, "layer", 0),
                start: get(t, "start_time", d.start),
                end: get(t, "end_time", d.end),
                style: get(t, "style", d.style),
                actor: get(t, "actor", String::new()),
                margin_l: get(t, "margin_l", 0),
                margin_r: get(t, "margin_r", 0),
                margin_t: mt,
                effect: get(t, "effect", String::new()),
                text: get(t, "text", String::new()),
            }))
        }
        other => Err(mlua::Error::runtime(format!("desteklenmeyen satır sınıfı: '{}'", other))),
    }
}

fn index_of(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(i) => Some(*i as i64),
        Value::Number(n) => Some(*n as i64),
        _ => None,
    }
}

fn put(doc: &mut Document, pos0: Option<usize>, e: Entry) {
    let ni = doc.info.len();
    let ns = doc.styles.len();
    match e {
        Entry::Info(k, v) => {
            let p = pos0.map(|p| p.min(ni)).unwrap_or(ni);
            doc.info.insert(p, (k, v));
        }
        Entry::Style(s) => {
            let p = pos0.map(|p| p.saturating_sub(ni).min(ns)).unwrap_or(ns);
            doc.styles.insert(p, s);
        }
        Entry::Event(ev) => {
            let ne = doc.events.len();
            let p = pos0.map(|p| p.saturating_sub(ni + ns).min(ne)).unwrap_or(ne);
            doc.events.insert(p, ev);
        }
    }
}

fn remove(doc: &mut Document, idx0: usize) {
    let ni = doc.info.len();
    let ns = doc.styles.len();
    if idx0 < ni {
        doc.info.remove(idx0);
    } else if idx0 < ni + ns {
        doc.styles.remove(idx0 - ni);
    } else if idx0 - ni - ns < doc.events.len() {
        doc.events.remove(idx0 - ni - ns);
    }
}

fn strip_self(args: Variadic<Value>) -> Vec<Value> {
    let mut v: Vec<Value> = args.into_iter().collect();
    if matches!(v.first(), Some(Value::UserData(_))) {
        v.remove(0);
    }
    v
}

impl UserData for Subs {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Len, |_, this, ()| Ok(this.0.borrow().entry_count()));
        m.add_meta_method(MetaMethod::Index, |lua, this, key: Value| {
            if let Some(i) = index_of(&key) {
                let d = this.0.borrow();
                if i < 1 || i as usize > d.entry_count() {
                    return Err(mlua::Error::runtime(format!("satır indeksi aralık dışında: {}", i)));
                }
                return Ok(Value::Table(entry_table(lua, &d, i as usize - 1)?));
            }
            let Value::String(k) = &key else { return Ok(Value::Nil) };
            let doc = this.0.clone();
            let f: Function = match &*k.to_str()? {
                "n" => return Ok(Value::Integer(doc.borrow().entry_count() as _)),
                "append" => lua.create_function(move |_, a: Variadic<Value>| {
                    for v in strip_self(a) {
                        if let Value::Table(t) = v {
                            put(&mut doc.borrow_mut(), None, table_entry(&t)?);
                        }
                    }
                    Ok(())
                })?,
                "insert" => lua.create_function(move |_, a: Variadic<Value>| {
                    let a = strip_self(a);
                    let Some(at) = a.first().and_then(index_of) else {
                        return Err(mlua::Error::runtime("insert: indeks gerekli"));
                    };
                    let mut pos = (at.max(1) - 1) as usize;
                    for v in &a[1..] {
                        if let Value::Table(t) = v {
                            put(&mut doc.borrow_mut(), Some(pos), table_entry(t)?);
                            pos += 1;
                        }
                    }
                    Ok(())
                })?,
                "delete" => lua.create_function(move |_, a: Variadic<Value>| {
                    let mut ix: Vec<usize> = strip_self(a)
                        .iter()
                        .filter_map(index_of)
                        .filter(|i| *i >= 1)
                        .map(|i| i as usize - 1)
                        .collect();
                    ix.sort_unstable_by(|a, b| b.cmp(a));
                    ix.dedup();
                    for i in ix {
                        remove(&mut doc.borrow_mut(), i);
                    }
                    Ok(())
                })?,
                "deleterange" => lua.create_function(move |_, a: Variadic<Value>| {
                    let a = strip_self(a);
                    if let (Some(x), Some(y)) = (a.first().and_then(index_of), a.get(1).and_then(index_of)) {
                        for i in (x.max(1)..=y).rev() {
                            remove(&mut doc.borrow_mut(), i as usize - 1);
                        }
                    }
                    Ok(())
                })?,
                _ => return Ok(Value::Nil),
            };
            Ok(Value::Function(f))
        });
        m.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (Value, Value)| {
            let Some(i) = index_of(&key) else {
                return Err(mlua::Error::runtime("yalnızca sayısal indekse atama yapılabilir"));
            };
            let Value::Table(t) = val else {
                return Err(mlua::Error::runtime("satır tablosu bekleniyor"));
            };
            let mut d = this.0.borrow_mut();
            let n = d.entry_count();
            if i < 1 || i as usize > n + 1 {
                return Err(mlua::Error::runtime(format!("satır indeksi aralık dışında: {}", i)));
            }
            let entry = table_entry(&t)?;
            if i as usize == n + 1 {
                put(&mut d, None, entry);
                return Ok(());
            }
            let idx0 = i as usize - 1;
            let (ni, ns) = (d.info.len(), d.styles.len());
            match (entry, idx0) {
                (Entry::Info(k, v), x) if x < ni => d.info[x] = (k, v),
                (Entry::Style(s), x) if x >= ni && x < ni + ns => d.styles[x - ni] = s,
                (Entry::Event(e), x) if x >= ni + ns => d.events[x - ni - ns] = e,
                _ => return Err(mlua::Error::runtime("satır sınıfı bulunduğu bölümle uyuşmuyor")),
            }
            Ok(())
        });
    }
}

pub struct Host {
    lua: Lua,
    doc: Shared,
    macros: Rc<RefCell<Vec<MacroEntry>>>,
    log: Rc<RefCell<Vec<String>>>,
    script_dir: Rc<RefCell<PathBuf>>,
}

/// Başsız ortamda `aegisub.dialog.display` için varsayılan yanıt verir.
fn dialog_defaults(lua: &Lua, dialog: Table, buttons: Option<Table>) -> LuaResult<(Value, Table)> {
    let res = lua.create_table()?;
    for item in dialog.sequence_values::<Table>() {
        let item = item?;
        let name: Option<String> = item.get("name").ok();
        if let Some(n) = name {
            let class: String = get(&item, "class", String::new());
            let v: Value = item.get("value").unwrap_or(Value::Nil);
            let v = match (class.as_str(), v) {
                ("checkbox", Value::Nil) => Value::Boolean(false),
                ("intedit" | "floatedit" | "slider", Value::Nil) => Value::Integer(0),
                ("edit" | "textbox" | "dropdown", Value::Nil) => Value::String(lua.create_string("")?),
                (_, v) => v,
            };
            res.set(n, v)?;
        }
    }
    let first = match buttons {
        Some(b) => b.get::<Value>(1)?,
        None => Value::Boolean(true),
    };
    Ok((first, res))
}

impl Host {
    pub fn new(doc: Document) -> LuaResult<Host> {
        let lua = Lua::new();
        let h = Host {
            lua,
            doc: Rc::new(RefCell::new(doc)),
            macros: Rc::new(RefCell::new(Vec::new())),
            log: Rc::new(RefCell::new(Vec::new())),
            script_dir: Rc::new(RefCell::new(PathBuf::from("."))),
        };
        h.install()?;
        Ok(h)
    }

    pub fn document(&self) -> Document {
        self.doc.borrow().clone()
    }

    pub fn set_document(&self, d: Document) {
        *self.doc.borrow_mut() = d;
    }

    pub fn macros(&self) -> Vec<MacroEntry> {
        self.macros.borrow().clone()
    }

    pub fn take_log(&self) -> Vec<String> {
        std::mem::take(&mut *self.log.borrow_mut())
    }

    fn install(&self) -> LuaResult<()> {
        let lua = &self.lua;
        let g = lua.globals();

        // Yerleşik modüller.
        let preload: Table = lua.globals().get::<Table>("package")?.get("preload")?;
        for (name, src) in [("utils", UTILS), ("unicode", UNICODE), ("karaskel", KARASKEL)] {
            let f = lua.load(src).set_name(name).into_function()?;
            preload.set(name, f)?;
        }
        let utf8 = lua.create_function(|lua, ()| lua.globals().get::<Table>("package")?.get::<Table>("loaded")?.get::<Value>("unicode"))?;
        preload.set("utf8", utf8)?;
        // `util` adı, Aegisub'da global `util` tablosu olarak da erişilir (require "utils" sonrası).
        let aeg = lua.create_table()?;

        let macros = self.macros.clone();
        aeg.set(
            "register_macro",
            lua.create_function(move |_, (name, desc, process, validate, _active): (String, Option<String>, Function, Option<Function>, Value)| {
                macros.borrow_mut().push(MacroEntry { name, description: desc.unwrap_or_default(), kind: Kind::Macro, process, validate });
                Ok(())
            })?,
        )?;
        let macros = self.macros.clone();
        aeg.set(
            "register_filter",
            lua.create_function(move |_, (name, desc, _prio, process, _cfg): (String, Option<String>, Value, Function, Value)| {
                macros.borrow_mut().push(MacroEntry { name, description: desc.unwrap_or_default(), kind: Kind::Filter, process, validate: None });
                Ok(())
            })?,
        )?;

        let log = self.log.clone();
        let out = lua.create_function(move |lua, args: MultiValue| {
            let mut v: Vec<Value> = args.into_iter().collect();
            if matches!(v.first(), Some(Value::Integer(_) | Value::Number(_))) && v.len() > 1 {
                v.remove(0);
            }
            let Some(Value::String(fmt)) = v.first().cloned() else { return Ok(()) };
            let fmt = fmt.to_str()?.to_string();
            let string: Table = lua.globals().get("string")?;
            let format: Function = string.get("format")?;
            let line: String = if v.len() > 1 {
                format.call(MultiValue::from_iter(v.into_iter()))?
            } else {
                fmt
            };
            log.borrow_mut().push(line);
            Ok(())
        })?;
        let dbg = lua.create_table()?;
        dbg.set("out", out.clone())?;
        aeg.set("debug", dbg)?;
        aeg.set("log", out)?;

        let prog = lua.create_table()?;
        for n in ["set", "task", "title"] {
            prog.set(n, lua.create_function(|_, _: Variadic<Value>| Ok(()))?)?;
        }
        prog.set("is_cancelled", lua.create_function(|_, _: Variadic<Value>| Ok(false))?)?;
        aeg.set("progress", prog)?;

        aeg.set("cancel", lua.create_function(|_, ()| -> LuaResult<()> { Err(mlua::Error::runtime(CANCEL)) })?)?;
        aeg.set("gettext", lua.create_function(|_, s: String| Ok(s))?)?;
        aeg.set("decode_path", lua.create_function(|_, s: String| Ok(s))?)?;
        aeg.set("frame_from_ms", lua.create_function(|_, _: Variadic<Value>| Ok(Value::Nil))?)?;
        aeg.set("ms_from_frame", lua.create_function(|_, _: Variadic<Value>| Ok(Value::Nil))?)?;
        aeg.set("video_size", lua.create_function(|_, _: Variadic<Value>| Ok(Value::Nil))?)?;
        aeg.set("keyframes", lua.create_function(|lua, ()| lua.create_table())?)?;
        aeg.set("project_properties", lua.create_function(|lua, ()| lua.create_table())?)?;
        aeg.set(
            "text_extents",
            lua.create_function(|_, (style, text): (Table, String)| {
                // Yaklaşık ölçü: gerçek yazı tipi metrikleri yol haritasında.
                let size: f64 = get(&style, "fontsize", 40.0);
                let sx: f64 = get(&style, "scale_x", 100.0) / 100.0;
                let sy: f64 = get(&style, "scale_y", 100.0) / 100.0;
                let spacing: f64 = get(&style, "spacing", 0.0);
                let mut w = 0.0;
                for c in text.chars() {
                    let cw = if (c as u32) >= 0x2E80 { 1.0 } else if c == ' ' { 0.3 } else { 0.55 };
                    w += (size * cw + spacing) * sx;
                }
                let h = size * sy;
                Ok((w, h, h * 0.2, 0.0))
            })?,
        )?;

        let dlg = lua.create_table()?;
        dlg.set(
            "display",
            lua.create_function(|lua, (d, b, _ids): (Table, Option<Table>, Value)| dialog_defaults(lua, d, b))?,
        )?;
        dlg.set("open", lua.create_function(|_, _: Variadic<Value>| Ok(Value::Nil))?)?;
        dlg.set("save", lua.create_function(|_, _: Variadic<Value>| Ok(Value::Nil))?)?;
        aeg.set("dialog", dlg)?;
        g.set("aegisub", aeg)?;

        // include(): önce betik klasörü, sonra yerleşik modüller.
        let dir = self.script_dir.clone();
        g.set(
            "include",
            lua.create_function(move |lua, name: String| {
                let p = dir.borrow().join(&name);
                if p.is_file() {
                    let src = std::fs::read_to_string(&p).map_err(mlua::Error::external)?;
                    return lua.load(&src).set_name(name.as_str()).exec();
                }
                let req: Function = lua.globals().get("require")?;
                let stem = name.trim_end_matches(".lua").to_string();
                let m: Value = req.call(stem.clone())?;
                if !matches!(m, Value::Nil) {
                    lua.globals().set(stem, m)?;
                }
                Ok(())
            })?,
        )?;
        // Aegisub betikleri `util`/`karaskel`'i global olarak bekler.
        lua.load("util = require('utils'); unicode = require('unicode'); karaskel = require('karaskel')").exec()?;
        Ok(())
    }

    /// Betiği yükler; kayıtlı makro/filtre sayısını döner.
    pub fn load_script(&self, path: &Path) -> Result<usize, RunError> {
        let src = std::fs::read_to_string(path).map_err(|e| RunError::Lua(e.to_string()))?;
        let dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
        let pkg: Table = self.lua.globals().get("package")?;
        let old: String = pkg.get("path")?;
        pkg.set("path", format!("{0}/?.lua;{0}/?/init.lua;{1}", dir.display(), old))?;
        *self.script_dir.borrow_mut() = dir;
        let before = self.macros.borrow().len();
        self.lua.load(&src).set_name(path.to_string_lossy().as_ref()).exec()?;
        Ok(self.macros.borrow().len() - before)
    }

    /// Makroyu çalıştırır. `selection` 0 tabanlı olay indeksleridir; hata/iptalde belge geri alınır.
    pub fn run(&self, macro_idx: usize, selection: &[usize], active: Option<usize>) -> Result<Vec<usize>, RunError> {
        let entry = self.macros.borrow().get(macro_idx).cloned().ok_or_else(|| RunError::Lua("makro bulunamadı".into()))?;
        let snapshot = self.doc.borrow().clone();
        let off = {
            let d = self.doc.borrow();
            d.info.len() + d.styles.len()
        };
        let subs = self.lua.create_userdata(Subs(self.doc.clone()))?;
        let sel = self.lua.create_table()?;
        for (i, s) in selection.iter().enumerate() {
            sel.set(i + 1, s + off + 1)?;
        }
        let act = active.map(|a| (a + off + 1) as i64).unwrap_or(0);
        let res: Result<MultiValue, mlua::Error> = if entry.kind == Kind::Filter {
            entry.process.call((subs, Value::Nil))
        } else {
            entry.process.call((subs, sel, act))
        };
        match res {
            Ok(rv) => {
                let mut out = Vec::new();
                if let Some(Value::Table(t)) = rv.iter().next() {
                    for v in t.sequence_values::<i64>().flatten() {
                        if v as usize > off {
                            out.push(v as usize - off - 1);
                        }
                    }
                }
                Ok(out)
            }
            Err(e) => {
                *self.doc.borrow_mut() = snapshot;
                Err(e.into())
            }
        }
    }

    /// `validate` işlevi varsa sonucu döner (yoksa true).
    pub fn is_valid(&self, macro_idx: usize, selection: &[usize]) -> Result<bool, RunError> {
        let Some(e) = self.macros.borrow().get(macro_idx).cloned() else { return Ok(false) };
        let Some(v) = e.validate else { return Ok(true) };
        let off = {
            let d = self.doc.borrow();
            d.info.len() + d.styles.len()
        };
        let subs = self.lua.create_userdata(Subs(self.doc.clone()))?;
        let sel = self.lua.create_table()?;
        for (i, s) in selection.iter().enumerate() {
            sel.set(i + 1, s + off + 1)?;
        }
        Ok(v.call::<bool>((subs, sel, 0))?)
    }
}

impl MacroEntry {
    pub fn kind(&self) -> Kind {
        self.kind
    }
}
