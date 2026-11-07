use crate::host::{DialogControl, DialogSpec, DlgVal, Host};
use crate::lines::{flatten, from_table, to_table, unflatten, Line};
use crate::modules;
use mlua::{Function, Lua, MetaMethod, MultiValue, RegistryKey, Result as LResult, Table, UserData, UserDataMethods, Value, Variadic};
use osiosub_core::ass::AssFile;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct MacroInfo {
    pub name: String,
    pub description: String,
    pub kind: Kind,
    pub script: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind { Macro, Filter }

struct Entry { info: MacroInfo, process: RegistryKey, validate: Option<RegistryKey>, is_active: Option<RegistryKey> }

#[derive(Debug, Default)]
pub struct MacroResult {
    pub file: Option<AssFile>,
    /// Selection as 0-based *event* indices.
    pub selection: Option<Vec<usize>>,
    pub active: Option<usize>,
    pub cancelled: bool,
    pub log: String,
    pub error: Option<String>,
}

struct Shared {
    lines: Vec<Line>,
    base: AssFile,
    host: Box<dyn Host>,
    log: String,
    entries: Vec<Entry>,
    cur_script: String,
    cancel_flag: bool,
    props: (f64, f64),
}

pub struct ScriptRuntime {
    lua: Lua,
    sh: Rc<RefCell<Shared>>,
    pub script_dirs: Vec<PathBuf>,
}

struct Subs(Rc<RefCell<Shared>>);

fn idx(v: &Value) -> Option<i64> {
    match v { Value::Integer(i) => Some(*i as i64), Value::Number(n) => Some(*n as i64), _ => None }
}

impl UserData for Subs {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Len, |_, this, ()| Ok(this.0.borrow().lines.len()));
        m.add_meta_method(MetaMethod::Index, |lua, this, key: Value| -> LResult<Value> {
            if let Some(i) = idx(&key) {
                let sh = this.0.borrow();
                if i < 1 || i as usize > sh.lines.len() { return Err(mlua::Error::runtime(format!("Requested out-of-range line from subs file: {}", i))); }
                return Ok(Value::Table(to_table(lua, &sh.lines[i as usize - 1])?));
            }
            let name = match &key { Value::String(s) => s.to_str()?.to_string(), _ => return Ok(Value::Nil) };
            let rc = this.0.clone();
            let f: Function = match name.as_str() {
                "n" => return Ok(Value::Integer(this.0.borrow().lines.len() as _)),
                "append" => lua.create_function(move |_, args: Variadic<Table>| {
                    let mut sh = rc.borrow_mut();
                    for t in args.iter() { sh.lines.push(from_table(t)?); }
                    Ok(())
                })?,
                "insert" => lua.create_function(move |_, (at, args): (i64, Variadic<Table>)| {
                    let mut sh = rc.borrow_mut();
                    let mut pos = (at.max(1) as usize - 1).min(sh.lines.len());
                    for t in args.iter() { let l = from_table(t)?; sh.lines.insert(pos, l); pos += 1; }
                    Ok(())
                })?,
                "delete" => lua.create_function(move |_, args: Variadic<i64>| {
                    let mut sh = rc.borrow_mut();
                    let mut v: Vec<i64> = args.iter().cloned().collect();
                    v.sort_unstable(); v.dedup();
                    for i in v.into_iter().rev() {
                        if i >= 1 && (i as usize) <= sh.lines.len() { sh.lines.remove(i as usize - 1); }
                    }
                    Ok(())
                })?,
                "deleterange" => lua.create_function(move |_, (a, b): (i64, i64)| {
                    let mut sh = rc.borrow_mut();
                    let (a, b) = (a.max(1) as usize, (b.max(0) as usize).min(sh.lines.len()));
                    if a <= b { sh.lines.drain(a - 1..b); }
                    Ok(())
                })?,
                "script_resolution" => lua.create_function(move |_, ()| {
                    let sh = rc.borrow();
                    let mut x: Option<i32> = None; let mut y: Option<i32> = None;
                    for l in &sh.lines { if let Line::Info(k, v) = l {
                        if k.eq_ignore_ascii_case("PlayResX") { x = v.trim().parse().ok(); }
                        if k.eq_ignore_ascii_case("PlayResY") { y = v.trim().parse().ok(); }
                    } }
                    Ok(match (x, y) { (Some(x), Some(y)) => (x, y), (Some(x), None) => (x, x * 3 / 4), (None, Some(y)) => (y * 4 / 3, y), _ => (384, 288) })
                })?,
                "SetUndoPoint" => lua.create_function(|_, _: MultiValue| Ok(()))?,
                _ => return Ok(Value::Nil),
            };
            Ok(Value::Function(f))
        });
        m.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (Value, Value)| {
            let Some(i) = idx(&key) else { return Err(mlua::Error::runtime("subs: integer index expected")) };
            let mut sh = this.0.borrow_mut();
            if let Value::Table(t) = val {
                let l = from_table(&t)?;
                if i >= 1 && (i as usize) <= sh.lines.len() { sh.lines[i as usize - 1] = l; }
                else if i == 0 || i as usize == sh.lines.len() + 1 { sh.lines.push(l); }                                // subs[0] = line : append
                else if i < 0 {                                                      // subs[-n] = line : insert before n
                    let p = ((-i) as usize - 1).min(sh.lines.len());
                    sh.lines.insert(p, l);
                } else { return Err(mlua::Error::runtime(format!("subs: line {} out of range", i))); }
            } else if i >= 1 && (i as usize) <= sh.lines.len() {
                sh.lines.remove(i as usize - 1);                                    // subs[i] = nil : delete
            }
            Ok(())
        });
    }
}

fn dlg_val(v: &Value) -> Option<DlgVal> {
    Some(match v {
        Value::Boolean(b) => DlgVal::Bool(*b),
        Value::Integer(i) => DlgVal::Num(*i as f64),
        Value::Number(n) => DlgVal::Num(*n),
        Value::String(s) => DlgVal::Str(s.to_str().ok()?.to_string()),
        _ => return None,
    })
}
fn parse_dialog(cfg: &Table, buttons: Option<Table>) -> LResult<DialogSpec> {
    let mut spec = DialogSpec::default();
    for pair in cfg.sequence_values::<Table>() {
        let t = pair?;
        let g = |k: &str| -> Option<String> { match t.get::<Value>(k).ok()? { Value::String(s) => Some(s.to_str().ok()?.to_string()), Value::Integer(i) => Some(i.to_string()), Value::Number(n) => Some(n.to_string()), _ => None } };
        let gn = |k: &str| -> Option<f64> { match t.get::<Value>(k).ok()? { Value::Integer(i) => Some(i as f64), Value::Number(n) => Some(n), _ => None } };
        let mut c = DialogControl {
            class: g("class").unwrap_or_default(), name: g("name").unwrap_or_default(),
            x: gn("x").unwrap_or(0.0) as i32, y: gn("y").unwrap_or(0.0) as i32,
            width: gn("width").unwrap_or(1.0) as i32, height: gn("height").unwrap_or(1.0) as i32,
            label: g("label").unwrap_or_default(), text: g("text").unwrap_or_default(), hint: g("hint").unwrap_or_default(),
            min: gn("min"), max: gn("max"), step: gn("step"), ..Default::default()
        };
        if let Ok(Value::Table(items)) = t.get::<Value>("items") {
            for s in items.sequence_values::<String>() { c.items.push(s?); }
        }
        let val = t.get::<Value>("value")?;
        c.value = dlg_val(&val);
        if c.class == "dropdown" && c.value.is_none() { c.value = None; }
        if matches!(c.class.as_str(), "edit" | "textbox") && c.value.is_none() { c.value = Some(DlgVal::Str(c.text.clone())); }
        spec.controls.push(c);
    }
    spec.buttons = match buttons {
        Some(b) => b.sequence_values::<String>().collect::<LResult<Vec<_>>>()?,
        None => vec!["OK".into(), "Cancel".into()],
    };
    Ok(spec)
}

impl ScriptRuntime {
    pub fn new(host: Box<dyn Host>) -> LResult<Self> {
        let lua = Lua::new();
        let sh = Rc::new(RefCell::new(Shared {
            lines: vec![], base: AssFile::default(), host, log: String::new(), entries: vec![],
            cur_script: String::new(), cancel_flag: false, props: (0.0, 0.0),
        }));
        let rt = ScriptRuntime { lua, sh, script_dirs: vec![] };
        rt.install()?;
        Ok(rt)
    }

    fn install(&self) -> LResult<()> {
        let lua = &self.lua;
        let g = lua.globals();
        let au = lua.create_table()?;
        let sh = self.sh.clone();

        // register_macro(name, description, process, validate, is_active)
        {
            let sh = sh.clone();
            au.set("register_macro", lua.create_function(move |lua, (name, desc, process, validate, active): (String, Option<String>, Function, Option<Function>, Option<Function>)| {
                let mut s = sh.borrow_mut();
                let script = s.cur_script.clone();
                s.entries.push(Entry {
                    info: MacroInfo { name, description: desc.unwrap_or_default(), kind: Kind::Macro, script },
                    process: lua.create_registry_value(process)?,
                    validate: validate.map(|f| lua.create_registry_value(f)).transpose()?,
                    is_active: active.map(|f| lua.create_registry_value(f)).transpose()?,
                });
                Ok(())
            })?)?;
        }
        {
            let sh = sh.clone();
            au.set("register_filter", lua.create_function(move |lua, (name, desc, _prio, process, _cfg): (String, Option<String>, Option<i64>, Function, Option<Function>)| {
                let mut s = sh.borrow_mut();
                let script = s.cur_script.clone();
                s.entries.push(Entry {
                    info: MacroInfo { name, description: desc.unwrap_or_default(), kind: Kind::Filter, script },
                    process: lua.create_registry_value(process)?, validate: None, is_active: None,
                });
                Ok(())
            })?)?;
        }
        {
            let sh = sh.clone();
            au.set("cancel", lua.create_function(move |_, ()| -> LResult<()> {
                sh.borrow_mut().cancel_flag = true;
                Err(mlua::Error::runtime("__aegisub_cancel__"))
            })?)?;
        }
        // log([level,] fmt, ...)
        let logf = {
            let sh = sh.clone();
            lua.create_function(move |lua, args: MultiValue| {
                let mut v: Vec<Value> = args.into_iter().collect();
                if matches!(v.first(), Some(Value::Integer(_)) | Some(Value::Number(_))) && v.len() >= 2 { v.remove(0); }
                let fmt: Function = lua.globals().get::<Table>("string")?.get("format")?;
                let s: String = if v.len() > 1 { fmt.call(MultiValue::from_iter(v))? } else {
                    match v.first() { Some(Value::String(s)) => s.to_str()?.to_string(), Some(Value::Integer(i)) => i.to_string(), Some(Value::Number(n)) => n.to_string(), _ => String::new() }
                };
                let mut sh = sh.borrow_mut();
                sh.log.push_str(&s);
                sh.host.log(&s);
                Ok(())
            })?
        };
        au.set("log", logf.clone())?;
        let dbg = lua.create_table()?;
        dbg.set("out", logf)?;
        au.set("debug", dbg)?;

        // progress
        let pr = lua.create_table()?;
        for name in ["set", "task", "title"] {
            let sh = sh.clone();
            let n = name.to_string();
            pr.set(name, lua.create_function(move |_, a: MultiValue| {
                let mut s = sh.borrow_mut();
                match (n.as_str(), a.iter().next()) {
                    ("set", Some(Value::Number(p))) => s.host.progress(*p, "", ""),
                    ("set", Some(Value::Integer(p))) => s.host.progress(*p as f64, "", ""),
                    ("task", Some(Value::String(t))) => { let t = t.to_str()?.to_string(); s.host.progress(-1.0, &t, "") }
                    ("title", Some(Value::String(t))) => { let t = t.to_str()?.to_string(); s.host.progress(-1.0, "", &t) }
                    _ => {}
                }
                Ok(())
            })?)?;
        }
        pr.set("is_cancelled", lua.create_function(|_, ()| Ok(false))?)?;
        au.set("progress", pr)?;

        // dialog
        let dl = lua.create_table()?;
        {
            let sh = sh.clone();
            dl.set("display", lua.create_function(move |lua, (cfg, buttons, _ids): (Table, Option<Table>, Option<Table>)| {
                let spec = parse_dialog(&cfg, buttons)?;
                let res = sh.borrow_mut().host.dialog(&spec);
                let out = lua.create_table()?;
                for (k, v) in &res.values {
                    match v {
                        DlgVal::Str(s) => out.set(k.as_str(), s.as_str())?,
                        DlgVal::Num(n) => out.set(k.as_str(), *n)?,
                        DlgVal::Bool(b) => out.set(k.as_str(), *b)?,
                    }
                }
                let btn: Value = match res.button { Some(b) => Value::String(lua.create_string(&b)?), None => Value::Boolean(false) };
                Ok((btn, out))
            })?)?;
        }
        dl.set("open", lua.create_function(|_, _: MultiValue| Ok(Value::Nil))?)?;
        dl.set("save", lua.create_function(|_, _: MultiValue| Ok(Value::Nil))?)?;
        au.set("dialog", dl)?;

        // text_extents(style_table, text)
        {
            let sh = sh.clone();
            au.set("text_extents", lua.create_function(move |_, (st, text): (Table, String)| {
                let style = match from_table(&{ st.set("class", "style")?; st })? { Line::Style(s) => s, _ => unreachable!() };
                let (w, h, d, e) = sh.borrow().host.text_extents(&style, &text);
                Ok((w, h, d, e))
            })?)?;
        }
        {
            let sh = sh.clone();
            au.set("video_size", lua.create_function(move |_, ()| {
                Ok(match sh.borrow().host.video_size() { Some((w, h)) => (Some(w), Some(h), Some(0), Some(1)), None => (None, None, None, None) })
            })?)?;
        }
        {
            let sh = sh.clone();
            au.set("frame_from_ms", lua.create_function(move |_, ms: f64| {
                Ok(sh.borrow().host.fps().map(|f| osiosub_core::time::FrameRate::new(f).frame_from_ms(ms as i64)))
            })?)?;
        }
        {
            let sh = sh.clone();
            au.set("ms_from_frame", lua.create_function(move |_, fr: f64| {
                Ok(sh.borrow().host.fps().map(|f| osiosub_core::time::FrameRate::new(f).ms_from_frame(fr as i64)))
            })?)?;
        }
        {
            let sh = sh.clone();
            au.set("keyframes", lua.create_function(move |lua, ()| {
                let t = lua.create_table()?;
                for (i, k) in sh.borrow().host.keyframes().iter().enumerate() { t.set(i + 1, *k)?; }
                Ok(t)
            })?)?;
        }
        {
            let sh = sh.clone();
            au.set("project_properties", lua.create_function(move |lua, ()| {
                let t = lua.create_table()?;
                let s = sh.borrow();
                let (rx, ry) = s.base.play_res();
                t.set("video_zoom", 1.0)?; t.set("ar_mode", 0)?; t.set("ar_value", 0.0)?;
                t.set("script_resolution_x", rx)?; t.set("script_resolution_y", ry)?;
                t.set("automation_scripts", "")?; t.set("export_encoding", "UTF-8")?;
                let _ = s.props;
                Ok(t)
            })?)?;
        }
        au.set("decode_path", lua.create_function(|_, p: String| Ok(p.replace("?user", ".").replace("?data", ".").replace("?temp", "/tmp")))?)?;
        au.set("file_name", lua.create_function(|_, ()| Ok(Value::Nil))?)?;
        au.set("lua_supported_version", lua.create_function(|_, _: MultiValue| Ok(true))?)?;
        au.set("gettext", lua.create_function(|_, s: String| Ok(s))?)?;
        au.set("get_audio_selection", lua.create_function(|_, ()| Ok(Value::Nil))?)?;
        au.set("set_undo_point", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
        {
            let sh = sh.clone();
            let cb = lua.create_table()?;
            let sh2 = sh.clone();
            cb.set("get", lua.create_function(move |_, ()| Ok(sh2.borrow().host.clipboard_get()))?)?;
            cb.set("set", lua.create_function(move |_, s: String| Ok(sh.borrow_mut().host.clipboard_set(&s)))?)?;
            au.set("clipboard", cb)?;
        }
        g.set("aegisub", au)?;
        g.set("tr", lua.create_function(|_, s: String| Ok(s))?)?;
        modules::install(lua)?;

        // include(name): script dirs (package.path), then the bundled unmodified Aegisub include files
        let include = lua.create_function(move |lua, name: String| -> LResult<MultiValue> {
            let pkg: Table = lua.globals().get("package")?;
            let path: String = pkg.get("path")?;
            let base = name.strip_suffix(".lua").unwrap_or(&name).to_string();
            let mut src: Option<(String, String)> = None;
            for pat in path.split(';') {
                if pat.is_empty() { continue; }
                let cand = pat.replace('?', &base.replace('.', "/"));
                if let Ok(bytes) = std::fs::read(&cand) { src = Some((cand, String::from_utf8_lossy(&bytes).trim_start_matches('\u{feff}').to_string())); break; }
            }
            if src.is_none() {
                if let Some(e) = modules::embedded_include(&format!("{}.lua", base)) { src = Some((format!("{}.lua", base), e.trim_start_matches('\u{feff}').to_string())); }
            }
            let Some((chunk, code)) = src else { return Err(mlua::Error::runtime(format!("Lua include not found: {}", name))) };
            lua.load(code).set_name(format!("@{}", chunk)).call::<MultiValue>(())
        })?;
        g.set("include", include)?;
        Ok(())
    }

    fn set_search_path(&self, extra: &Path) -> LResult<()> {
        let pkg: Table = self.lua.globals().get("package")?;
        let mut p = String::new();
        let mut dirs = vec![extra.to_path_buf()];
        dirs.extend(self.script_dirs.iter().cloned());
        for d in dirs {
            let d = d.to_string_lossy().replace('\\', "/");
            p.push_str(&format!("{d}/?.lua;{d}/?/init.lua;{d}/?/?.lua;"));
        }
        let old: String = pkg.get("path")?;
        pkg.set("path", format!("{}{}", p, old))
    }

    /// Load a .lua/.moon-free script file and register its macros. Returns names registered.
    pub fn load_file(&mut self, path: &Path) -> Result<Vec<MacroInfo>, String> {
        let src = std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))?;
        let src = String::from_utf8_lossy(&src).trim_start_matches('\u{feff}').to_string();
        let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        self.set_search_path(&dir).map_err(|e| e.to_string())?;
        self.load_source(&src, &path.to_string_lossy())
    }

    pub fn load_source(&mut self, src: &str, chunk: &str) -> Result<Vec<MacroInfo>, String> {
        let before = self.sh.borrow().entries.len();
        self.sh.borrow_mut().cur_script = chunk.to_string();
        // Make `include`d module state fresh per script is not required; globals like script_name are read after exec.
        self.lua.load(src).set_name(format!("@{}", chunk)).exec().map_err(|e| e.to_string())?;
        let s = self.sh.borrow();
        Ok(s.entries[before..].iter().map(|e| e.info.clone()).collect())
    }

    pub fn macros(&self) -> Vec<MacroInfo> { self.sh.borrow().entries.iter().map(|e| e.info.clone()).collect() }

    fn sync_in(&self, file: &AssFile) {
        let mut s = self.sh.borrow_mut();
        s.lines = flatten(file);
        s.base = file.clone();
        s.log.clear();
        s.cancel_flag = false;
    }

    fn to_sub_index(file: &AssFile, ev: usize) -> i64 { (file.info.len() + file.styles.len() + ev + 1) as i64 }

    pub fn validate(&self, i: usize, file: &AssFile, selection: &[usize], active: usize) -> bool {
        self.sync_in(file);
        let sh = self.sh.borrow();
        let Some(e) = sh.entries.get(i) else { return false };
        let Some(vk) = &e.validate else { return true };
        let Ok(f) = self.lua.registry_value::<Function>(vk) else { return true };
        let subs = match self.lua.create_userdata(Subs(self.sh.clone())) { Ok(s) => s, Err(_) => return true };
        let sel = self.lua.create_sequence_from(selection.iter().map(|&x| Self::to_sub_index(file, x))).unwrap();
        drop(sh);
        f.call::<bool>((subs, sel, Self::to_sub_index(file, active))).unwrap_or(true)
    }

    /// Run macro `i` against `file`. `selection`/`active` are 0-based event indices.
    pub fn run(&self, i: usize, file: &AssFile, selection: &[usize], active: usize) -> MacroResult {
        self.sync_in(file);
        let mut res = MacroResult::default();
        let (pk, kind) = {
            let sh = self.sh.borrow();
            match sh.entries.get(i) { Some(e) => (self.lua.registry_value::<Function>(&e.process), e.info.kind), None => { res.error = Some("no such macro".into()); return res; } }
        };
        let f = match pk { Ok(f) => f, Err(e) => { res.error = Some(e.to_string()); return res; } };
        let subs = match self.lua.create_userdata(Subs(self.sh.clone())) { Ok(s) => s, Err(e) => { res.error = Some(e.to_string()); return res; } };
        let n_head = file.info.len() + file.styles.len();
        let sel: Vec<i64> = selection.iter().map(|&x| Self::to_sub_index(file, x)).collect();
        let sel_t = self.lua.create_sequence_from(sel).unwrap();
        let ret: LResult<MultiValue> = match kind {
            Kind::Macro => f.call((subs.clone(), sel_t, Self::to_sub_index(file, active))),
            Kind::Filter => {
                let cfg = self.lua.create_table().unwrap();
                f.call((subs.clone(), cfg))
            }
        };
        match ret {
            Ok(vals) => {
                let mut it = vals.into_iter();
                if let Some(Value::Table(t)) = it.next() {
                    let sel: Vec<usize> = t.sequence_values::<i64>().filter_map(|v| v.ok()).filter(|&v| v as usize > n_head).map(|v| v as usize - n_head - 1).collect();
                    res.selection = Some(sel);
                    if let Some(a) = it.next().as_ref().and_then(idx) { if a as usize > n_head { res.active = Some(a as usize - n_head - 1); } }
                }
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("__aegisub_cancel__") || self.sh.borrow().cancel_flag { res.cancelled = true; }
                else { res.error = Some(msg); }
            }
        }
        let sh = self.sh.borrow();
        res.log = sh.log.clone();
        if !res.cancelled && res.error.is_none() {
            let mut out = unflatten(&sh.lines, &sh.base);
            // an empty style list would be invalid
            if out.styles.is_empty() { out.styles.push(Default::default()); }
            res.file = Some(out);
        }
        res
    }

    /// Evaluate a snippet (for tests / REPL).
    pub fn eval_string(&self, code: &str) -> Result<String, String> {
        let v: Value = self.lua.load(code).eval().map_err(|e| e.to_string())?;
        Ok(match v { Value::String(s) => s.to_str().map(|s| s.to_string()).unwrap_or_default(), Value::Integer(i) => i.to_string(), Value::Number(n) => n.to_string(), Value::Boolean(b) => b.to_string(), Value::Nil => "nil".into(), _ => format!("{:?}", v) })
    }
}
