//! The `aegisub.*` table, the `subs` object and the embedded Lua prelude.

use crate::convert::{self, Entry};
use crate::dialog::{Control, DialogRequest, DialogResponse, Progress, Val};
use crate::{Shared, CANCEL_MARK};
use mlua::{AnyUserData, Function, Lua, MultiValue, Result, Table, UserData, UserDataMethods, Value};
use nsub_core::ass::Script;
use nsub_core::time::FrameRate;
use std::cell::RefCell;
use std::rc::Rc;

type Sh = Rc<RefCell<Shared>>;

struct Subs(Sh);

fn out_of_range(i: i64) -> mlua::Error {
    mlua::Error::runtime(format!("Requested out-of-range line from subtitle file: {}", i))
}

fn to_index(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(i) => Some(*i as i64),
        Value::Number(n) => Some(*n as i64),
        _ => None,
    }
}

fn group(e: &Entry) -> u8 {
    match e {
        Entry::Info(_) => 0,
        Entry::Style(_) => 1,
        Entry::Event(_) => 2,
    }
}

fn collect_entries(args: impl IntoIterator<Item = Value>) -> Result<Vec<Entry>> {
    let mut out = vec![];
    for a in args {
        match a {
            Value::Table(t) => out.push(convert::table_to_entry(&t).map_err(mlua::Error::runtime)?),
            other => return Err(mlua::Error::runtime(format!("expected line table, got {}", other.type_name()))),
        }
    }
    Ok(out)
}

/// Aegisub's append: new lines go after the last existing line of the same class, else at the end.
fn append_entries(entries: &mut Vec<Entry>, new: Vec<Entry>) {
    for e in new {
        let g = group(&e);
        match entries.iter().rposition(|x| group(x) == g) {
            Some(p) => entries.insert(p + 1, e),
            None => entries.push(e),
        }
    }
}

fn insert_entries(entries: &mut Vec<Entry>, before: i64, new: Vec<Entry>) -> Result<()> {
    if before < 1 || before as usize > entries.len() + 1 {
        return Err(mlua::Error::runtime("bad argument #1 to 'insert' (Out of range line index)"));
    }
    if before as usize == entries.len() + 1 {
        append_entries(entries, new);
    } else {
        let at = before as usize - 1;
        entries.splice(at..at, new);
    }
    Ok(())
}

fn delete_indices(entries: &mut Vec<Entry>, mut idx: Vec<i64>) -> Result<()> {
    for &i in &idx {
        if i < 1 || i as usize > entries.len() {
            return Err(mlua::Error::runtime("bad argument to 'delete' (Out of range line index)"));
        }
    }
    idx.sort_unstable_by(|a, b| b.cmp(a));
    idx.dedup();
    for i in idx {
        entries.remove(i as usize - 1);
    }
    Ok(())
}

impl UserData for Subs {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method("__len", |_, this, ()| Ok(this.0.borrow().entries.len()));
        m.add_meta_method("__index", |lua, this, key: Value| -> Result<Value> {
            if let Some(i) = to_index(&key) {
                let sh = this.0.borrow();
                return match sh.entries.get((i - 1) as usize) {
                    Some(e) if i >= 1 => Ok(Value::Table(convert::entry_to_table(lua, e)?)),
                    _ => Err(out_of_range(i)),
                };
            }
            let Value::String(name) = key else {
                return Err(mlua::Error::runtime(format!("Attempt to index a Subtitle File object with value of type '{}'.", key.type_name())));
            };
            let sh = this.0.clone();
            let name = name.to_str()?.to_string();
            let f: Function = match name.as_str() {
                "n" => return Ok(Value::Integer(this.0.borrow().entries.len() as _)),
                "insert" => lua.create_function(move |_, args: MultiValue| {
                    let mut it = args.into_iter();
                    let idx = it.next().as_ref().and_then(to_index).ok_or_else(|| mlua::Error::runtime("bad argument #1 to 'insert' (number expected)"))?;
                    let new = collect_entries(it)?;
                    insert_entries(&mut sh.borrow_mut().entries, idx, new)
                })?,
                "append" => lua.create_function(move |_, args: MultiValue| {
                    let new = collect_entries(args)?;
                    append_entries(&mut sh.borrow_mut().entries, new);
                    Ok(())
                })?,
                "delete" => lua.create_function(move |_, args: MultiValue| {
                    let idx: Vec<i64> = match args.iter().next() {
                        Some(Value::Table(t)) if args.len() == 1 => t.sequence_values::<f64>().filter_map(|v| v.ok()).map(|n| n as i64).collect(),
                        _ => args.iter().filter_map(to_index).collect(),
                    };
                    delete_indices(&mut sh.borrow_mut().entries, idx)
                })?,
                "deleterange" => lua.create_function(move |_, args: MultiValue| {
                    let v: Vec<i64> = args.iter().filter_map(to_index).collect();
                    let mut s = sh.borrow_mut();
                    let a = (v.first().copied().unwrap_or(1).max(1) - 1) as usize;
                    let b = (v.get(1).copied().unwrap_or(0).max(0) as usize).min(s.entries.len());
                    if a < b {
                        s.entries.drain(a..b);
                    }
                    Ok(())
                })?,
                "script_resolution" => lua.create_function(move |_, ()| Ok(sh.borrow().res))?,
                _ => return Err(mlua::Error::runtime(format!("Invalid indexing in Subtitle File object: '{name}'"))),
            };
            Ok(Value::Function(f))
        });
        m.add_meta_method("__newindex", |_, this, (key, val): (Value, Value)| {
            let n = to_index(&key).ok_or_else(|| mlua::Error::runtime("subs index must be a number"))?;
            let mut s = this.0.borrow_mut();
            if n < 0 {
                let new = collect_entries([val])?;
                return insert_entries(&mut s.entries, -n, new);
            }
            if n == 0 {
                let new = collect_entries([val])?;
                append_entries(&mut s.entries, new);
                return Ok(());
            }
            match val {
                Value::Nil => delete_indices(&mut s.entries, vec![n]),
                Value::Table(t) => {
                    if n as usize > s.entries.len() {
                        return Err(out_of_range(n));
                    }
                    let e = convert::table_to_entry(&t).map_err(mlua::Error::runtime)?;
                    s.entries[n as usize - 1] = e;
                    Ok(())
                }
                _ => Err(mlua::Error::runtime("Can't convert a non-table value to AssEntry")),
            }
        });
    }
}

pub(crate) fn make_subs(lua: &Lua, sh: &Sh) -> Result<AnyUserData> {
    lua.create_userdata(Subs(sh.clone()))
}

/// Remember script-level facts (resolution, info) for `video_size`, `project_properties`.
pub(crate) fn set_script_context(sh: &Sh, script: &Script) {
    let (w, h) = script.resolution();
    let mut s = sh.borrow_mut();
    s.res = (w, h);
    s.ycbcr = script.get_info("YCbCr Matrix").unwrap_or("None").to_string();
}

pub(crate) fn install(lua: &Lua, sh: &Sh) -> Result<()> {
    let g = lua.globals();
    let a = lua.create_table()?;

    a.set("lua_supported_version", 4)?;
    a.set("__init_version", "3.3.3-nightsub")?;

    {
        let sh = sh.clone();
        g.set(
            "__nsub_log",
            lua.create_function(move |_, s: String| {
                let mut x = sh.borrow_mut();
                if x.opts.echo_log {
                    eprint!("{}", s);
                }
                x.log.push_str(&s);
                Ok(())
            })?,
        )?;
    }
    a.set(
        "cancel",
        lua.create_function(|_, ()| -> Result<()> { Err(mlua::Error::runtime(CANCEL_MARK)) })?,
    )?;
    a.set("set_undo_point", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    {
        let sh = sh.clone();
        a.set(
            "text_extents",
            lua.create_function(move |_, (style, text): (Table, String)| {
                let st = match convert::table_to_entry(&style) {
                    Ok(Entry::Style(s)) => s,
                    _ => return Err(mlua::Error::runtime("text_extents: first argument must be a style table")),
                };
                let x = sh.borrow().metrics.extents(&st, &text);
                Ok((x.width, x.height, x.descent, x.extlead))
            })?,
        )?;
    }
    {
        let sh1 = sh.clone();
        let sh = sh1.clone();
        a.set(
            "frame_from_ms",
            lua.create_function(move |_, ms: f64| {
                let fps = sh.borrow().opts.fps;
                Ok(if fps > 0.0 { Value::Integer(FrameRate::new(fps).frame_from_ms(ms as i64) as _) } else { Value::Nil })
            })?,
        )?;
        let sh = sh_clone(&sh1);
        a.set(
            "ms_from_frame",
            lua.create_function(move |_, f: f64| {
                let fps = sh.borrow().opts.fps;
                Ok(if fps > 0.0 { Value::Integer(FrameRate::new(fps).ms_from_frame(f as i64) as _) } else { Value::Nil })
            })?,
        )?;
    }
    {
        let sh = sh.clone();
        a.set(
            "video_size",
            lua.create_function(move |_, ()| {
                let (w, h) = sh.borrow().res;
                Ok((w, h, w as f64 / h.max(1) as f64, 0))
            })?,
        )?;
    }
    {
        let sh = sh.clone();
        a.set(
            "project_properties",
            lua.create_function(move |lua, ()| {
                let s = sh.borrow();
                let t = lua.create_table()?;
                t.set("automation_scripts", "")?;
                t.set("export_encoding", "UTF-8")?;
                t.set("export_filters", "")?;
                t.set("last_style_storage", "Default")?;
                t.set("timecodes_file", "")?;
                t.set("video_file", s.opts.video_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default())?;
                t.set("audio_file", s.opts.audio_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default())?;
                t.set("video_zoom", 1.0)?;
                t.set("ar_mode", 0)?;
                t.set("ar_value", 0.0)?;
                t.set("scroll_position", 0)?;
                t.set("active_row", 0)?;
                t.set("ycbcr_matrix", s.ycbcr.as_str())?;
                Ok(t)
            })?,
        )?;
    }
    {
        let sh = sh.clone();
        a.set(
            "file_name",
            lua.create_function(move |_, with_path: Option<bool>| {
                let s = sh.borrow();
                Ok(s.opts.script_path.as_ref().map(|p| {
                    if with_path.unwrap_or(false) {
                        p.to_string_lossy().to_string()
                    } else {
                        p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
                    }
                }))
            })?,
        )?;
    }
    {
        let sh = sh.clone();
        a.set(
            "decode_path",
            lua.create_function(move |_, p: String| {
                let s = sh.borrow();
                let dir_of = |o: &Option<std::path::PathBuf>| o.as_ref().and_then(|p| p.parent()).map(|d| d.to_string_lossy().to_string());
                let user = std::env::var("HOME").map(|h| format!("{h}/.nightsub")).unwrap_or_else(|_| ".".into());
                let tmp = std::env::temp_dir().to_string_lossy().to_string();
                let mut out = p.clone();
                for (tok, val) in [
                    ("?script", dir_of(&s.opts.script_path)),
                    ("?video", dir_of(&s.opts.video_path)),
                    ("?audio", dir_of(&s.opts.audio_path)),
                    ("?user", Some(user.clone())),
                    ("?data", Some(user)),
                    ("?temp", Some(tmp)),
                ] {
                    if out.starts_with(tok) {
                        out = format!("{}{}", val.unwrap_or_default(), &out[tok.len()..]);
                    }
                }
                Ok(out)
            })?,
        )?;
    }
    a.set(
        "parse_karaoke_data",
        lua.create_function(|lua, line: Table| {
            let (start, text) = match convert::table_to_entry(&line) {
                Ok(Entry::Event(e)) => (e.start, e.text),
                _ => return Err(mlua::Error::runtime("bad argument #1 to 'parse_karaoke_data' (Subtitle line must be a dialogue line)")),
            };
            let out = lua.create_table()?;
            let first = lua.create_table()?;
            for (k, v) in [("duration", 0i64), ("start_time", 0), ("end_time", 0)] {
                first.set(k, v)?;
            }
            for k in ["tag", "text", "text_stripped"] {
                first.set(k, "")?;
            }
            out.raw_set(0, first)?;
            for (i, syl) in nsub_core::kara::parse_syllables(&text, start).iter().enumerate() {
                let t = lua.create_table()?;
                t.set("duration", syl.duration)?;
                t.set("start_time", syl.start_time - start)?;
                t.set("end_time", syl.start_time + syl.duration - start)?;
                t.set("tag", syl.tag_type.as_str())?;
                t.set("text", syl.get_text(false))?;
                t.set("text_stripped", syl.text.as_str())?;
                out.raw_set(i + 1, t)?;
            }
            Ok(out)
        })?,
    )?;
    a.set("get_audio_selection", lua.create_function(|_, ()| Ok(Value::Nil))?)?;
    a.set("gettext", lua.create_function(|_, s: String| Ok(s))?)?;
    {
        let sh1 = sh.clone();
        a.set(
            "is_cancelled",
            lua.create_function(move |_, ()| Ok(sh1.borrow().opts.cancel_flag.load(std::sync::atomic::Ordering::Relaxed)))?,
        )?;
        let sh2 = sh.clone();
        g.set(
            "__nsub_progress",
            lua.create_function(move |_, (kind, num, text): (String, Option<f64>, Option<String>)| {
                let h = sh2.borrow().opts.progress_handler.clone();
                if let Some(h) = h {
                    let mut p = Progress::default();
                    match kind.as_str() {
                        "set" => p.fraction = num,
                        "task" => p.task = text,
                        _ => p.title = text,
                    }
                    h(p);
                }
                Ok(())
            })?,
        )?;
    }

    // register_macro / register_filter
    for (fname, kind) in [("register_macro", "macro"), ("register_filter", "filter")] {
        let sh = sh.clone();
        a.set(
            fname,
            lua.create_function(move |lua, args: MultiValue| {
                let reg: Table = lua.named_registry_value("nsub_macros")?;
                let t = lua.create_table()?;
                let v: Vec<Value> = args.into_iter().collect();
                let s = |i: usize| match v.get(i) {
                    Some(Value::String(s)) => s.to_string_lossy().to_string(),
                    _ => String::new(),
                };
                t.set("kind", kind)?;
                t.set("name", s(0))?;
                t.set("description", s(1))?;
                t.set("script_file", sh.borrow().current_script_file.as_str())?;
                if kind == "macro" {
                    for (i, k) in [(2, "process"), (3, "validate"), (4, "is_active")] {
                        if let Some(Value::Function(f)) = v.get(i) {
                            t.set(k, f.clone())?;
                        }
                    }
                } else {
                    // register_filter(name, description, priority, process, config)
                    if let Some(Value::Function(f)) = v.get(3) {
                        t.set("process", f.clone())?;
                    }
                    if let Some(Value::Function(f)) = v.get(4) {
                        t.set("config", f.clone())?;
                    }
                }
                reg.raw_set(reg.raw_len() + 1, t)?;
                Ok(())
            })?,
        )?;
    }

    // clipboard
    {
        let c = lua.create_table()?;
        let sh1 = sh.clone();
        c.set("get", lua.create_function(move |_, ()| Ok(sh1.borrow().clipboard.clone()))?)?;
        let sh2 = sh.clone();
        c.set(
            "set",
            lua.create_function(move |_, s: String| {
                sh2.borrow_mut().clipboard = s;
                Ok(true)
            })?,
        )?;
        a.set("clipboard", c)?;
    }

    // dialogs: delegated to the host's handler when present, else answered headlessly
    {
        let d = lua.create_table()?;
        let sh1 = sh.clone();
        d.set(
            "display",
            lua.create_function(move |lua, (dialog, buttons, ids): (Table, Option<Table>, Option<Table>)| {
                let req = build_request(&dialog, buttons.as_ref(), ids.as_ref())?;
                let handler = sh1.borrow().opts.dialog_handler.clone();
                let resp = match handler {
                    Some(h) => h(req.clone()),
                    None => headless_answer(&sh1.borrow(), &req),
                };
                let res = lua.create_table()?;
                for c in &req.controls {
                    if c.name.is_empty() || c.class == "label" || c.class == "image" {
                        continue;
                    }
                    let v = resp.values.get(&c.name).cloned().unwrap_or_else(|| c.value.clone());
                    res.set(c.name.as_str(), val_to_lua(lua, &c.class, v)?)?;
                }
                let button: Value = match (&resp.button, req.buttons.is_empty()) {
                    (Some(b), true) => Value::Boolean(b == "OK"),
                    (Some(b), false) => Value::String(lua.create_string(b)?),
                    (None, true) => Value::Boolean(false),
                    (None, false) => match &req.cancel_id {
                        Some(c) => Value::String(lua.create_string(c)?),
                        None => Value::Boolean(false),
                    },
                };
                Ok((button, res))
            })?,
        )?;
        d.set("open", lua.create_function(|_, _: MultiValue| Ok(Value::Nil))?)?;
        d.set("save", lua.create_function(|_, _: MultiValue| Ok(Value::Nil))?)?;
        a.set("dialog", d)?;
    }

    g.set("aegisub", a)?;
    Ok(())
}

fn sh_clone(s: &Sh) -> Sh {
    s.clone()
}

const PRELUDE: &str = include_str!("lua/prelude.lua");

macro_rules! embed {
    ($($key:literal => $path:literal),* $(,)?) => {
        &[$(($key, include_str!($path))),*]
    };
}

const EMBEDDED: &[(&str, &str)] = embed! {
    "include/utils.lua" => "lua/include/utils.lua",
    "include/utils-auto4.lua" => "lua/include/utils-auto4.lua",
    "include/karaskel.lua" => "lua/include/karaskel.lua",
    "include/karaskel-auto4.lua" => "lua/include/karaskel-auto4.lua",
    "include/cleantags.lua" => "lua/include/cleantags.lua",
    "include/clipboard.lua" => "lua/include/clipboard.lua",
    "include/unicode.lua" => "lua/include/unicode.lua",
    "include/unicode-monkeypatch.lua" => "lua/include/unicode-monkeypatch.lua",
    "include/re.lua" => "lua/include/re.lua",
    "include/lfs.lua" => "lua/include/lfs.lua",
    "aegisub/util" => "lua/aegisub/util.lua",
    "aegisub/unicode" => "lua/aegisub/unicode.lua",
    "aegisub/re" => "lua/aegisub/re.lua",
    "aegisub/lfs" => "lua/aegisub/lfs.lua",
    "aegisub/clipboard" => "lua/aegisub/clipboard.lua",
    "aegisub/argcheck" => "lua/aegisub/argcheck.lua",
    "aegisub/ffi" => "lua/aegisub/ffi.lua",
    "depctrl" => "lua/depctrl.lua",
};

pub(crate) fn install_prelude(lua: &Lua, sh: &Sh) -> Result<()> {
    let inc: Vec<String> = sh.borrow().opts.include_dirs.iter().map(|p| p.to_string_lossy().replace('\\', "/")).collect();
    lua.globals().set("__nsub_include_dirs", lua.create_sequence_from(inc)?)?;
    let embedded = lua.create_table()?;
    for (name, src) in EMBEDDED {
        embedded.set(*name, *src)?;
    }
    lua.globals().set("__nsub_embedded", embedded)?;
    lua.load(PRELUDE).set_name("=prelude").exec()
}

fn lua_to_val(v: Value) -> Val {
    match v {
        Value::Nil => Val::None,
        Value::Boolean(b) => Val::Bool(b),
        Value::Integer(i) => Val::Num(i as f64),
        Value::Number(n) => Val::Num(n),
        Value::String(s) => Val::Str(s.to_string_lossy().to_string()),
        _ => Val::None,
    }
}

fn val_to_lua(lua: &Lua, class: &str, v: Val) -> Result<Value> {
    Ok(match (class, v) {
        ("checkbox", Val::Bool(b)) => Value::Boolean(b),
        ("checkbox", Val::Num(n)) => Value::Boolean(n != 0.0),
        ("checkbox", Val::Str(s)) => Value::Boolean(matches!(s.as_str(), "1" | "true" | "yes" | "on")),
        ("checkbox", Val::None) => Value::Boolean(false),
        ("intedit", Val::Num(n)) => Value::Integer(n as _),
        ("intedit", Val::Str(s)) => Value::Integer(s.trim().parse::<f64>().unwrap_or(0.0) as _),
        ("floatedit", Val::Num(n)) => Value::Number(n),
        ("floatedit", Val::Str(s)) => Value::Number(s.trim().parse().unwrap_or(0.0)),
        ("intedit" | "floatedit", _) => Value::Integer(0),
        (_, Val::Str(s)) => Value::String(lua.create_string(s)?),
        (_, Val::Num(n)) => Value::Number(n),
        (_, Val::Bool(b)) => Value::Boolean(b),
        (_, Val::None) => Value::String(lua.create_string("")?),
    })
}

fn opt_num(t: &Table, k: &str) -> Option<f64> {
    match t.get::<Value>(k) {
        Ok(Value::Number(n)) => Some(n),
        Ok(Value::Integer(i)) => Some(i as f64),
        _ => None,
    }
}

fn build_request(dialog: &Table, buttons: Option<&Table>, ids: Option<&Table>) -> Result<DialogRequest> {
    let mut req = DialogRequest::default();
    for c in dialog.sequence_values::<Table>() {
        let c = c?;
        let class: String = c.get::<String>("class").unwrap_or_default().to_ascii_lowercase();
        let s = |k: &str| c.get::<Option<String>>(k).ok().flatten().unwrap_or_default();
        let mut value = lua_to_val(c.get::<Value>("value").unwrap_or(Value::Nil));
        if value == Val::None && matches!(class.as_str(), "edit" | "textbox") {
            value = lua_to_val(c.get::<Value>("text").unwrap_or(Value::Nil));
        }
        if value == Val::None && class == "dropdown" {
            if let Ok(items) = c.get::<Table>("items") {
                value = lua_to_val(items.get::<Value>(1).unwrap_or(Value::Nil));
            }
        }
        let items = c.get::<Table>("items").map(|t| t.sequence_values::<String>().filter_map(|v| v.ok()).collect()).unwrap_or_default();
        req.controls.push(Control {
            class,
            name: s("name"),
            label: s("label"),
            hint: s("hint"),
            value,
            items,
            min: opt_num(&c, "min"),
            max: opt_num(&c, "max"),
            step: opt_num(&c, "step"),
            x: opt_num(&c, "x").unwrap_or(0.0) as i32,
            y: opt_num(&c, "y").unwrap_or(0.0) as i32,
            width: opt_num(&c, "width").unwrap_or(1.0) as i32,
            height: opt_num(&c, "height").unwrap_or(1.0) as i32,
        });
    }
    if let Some(b) = buttons {
        req.buttons = b.sequence_values::<String>().filter_map(|v| v.ok()).collect();
    }
    if let Some(i) = ids {
        req.ok_id = i.get::<Option<String>>("ok").ok().flatten();
        req.cancel_id = i.get::<Option<String>>("cancel").ok().flatten();
    }
    Ok(req)
}

fn headless_answer(s: &Shared, req: &DialogRequest) -> DialogResponse {
    let mut r = DialogResponse::default();
    for c in &req.controls {
        if c.name.is_empty() {
            continue;
        }
        if let Some(x) = s.opts.dialog_answers.get(&c.name) {
            let v = match c.class.as_str() {
                "checkbox" => Val::Bool(matches!(x.as_str(), "1" | "true" | "yes" | "on")),
                "intedit" | "floatedit" => Val::Num(x.parse().unwrap_or(0.0)),
                _ => Val::Str(x.clone()),
            };
            r.values.insert(c.name.clone(), v);
        }
    }
    r.button = Some(match (&s.opts.dialog_button, req.buttons.first()) {
        (Some(b), _) => b.clone(),
        (None, Some(b)) => b.clone(),
        (None, None) => "OK".to_string(),
    });
    r
}
