//! Minimal LuaFileSystem (`lfs`) used by many Aegisub scripts.

use mlua::{Lua, Table, Value};
use std::time::UNIX_EPOCH;

fn secs(t: std::io::Result<std::time::SystemTime>) -> i64 {
    t.ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub fn install(lua: &Lua, sf: &Table) -> mlua::Result<()> {
    let t = lua.create_table()?;
    t.set(
        "attributes",
        lua.create_function(|lua, (path, field): (String, Option<String>)| {
            let Ok(m) = std::fs::metadata(&path) else { return Ok((Value::Nil, Some("cannot obtain information from file".to_string()))) };
            let a = lua.create_table()?;
            a.set("mode", if m.is_dir() { "directory" } else if m.is_file() { "file" } else { "other" })?;
            a.set("size", m.len())?;
            a.set("modification", secs(m.modified()))?;
            a.set("access", secs(m.accessed()))?;
            a.set("change", secs(m.modified()))?;
            if let Some(f) = field {
                return Ok((a.get::<Value>(f)?, None));
            }
            Ok((Value::Table(a), None))
        })?,
    )?;
    t.set(
        "dir",
        lua.create_function(|lua, path: String| {
            let mut names: Vec<String> = vec![".".into(), "..".into()];
            let rd = std::fs::read_dir(&path).map_err(|e| mlua::Error::runtime(format!("cannot open {}: {}", path, e)))?;
            names.extend(rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()));
            let idx = std::cell::Cell::new(0usize);
            lua.create_function(move |_, ()| {
                let i = idx.get();
                idx.set(i + 1);
                Ok(names.get(i).cloned())
            })
        })?,
    )?;
    t.set("mkdir", lua.create_function(|_, p: String| Ok(match std::fs::create_dir(&p) { Ok(_) => (true, None), Err(e) => (false, Some(e.to_string())) }))?)?;
    t.set("rmdir", lua.create_function(|_, p: String| Ok(match std::fs::remove_dir(&p) { Ok(_) => (true, None), Err(e) => (false, Some(e.to_string())) }))?)?;
    t.set("currentdir", lua.create_function(|_, ()| Ok(std::env::current_dir().ok().map(|p| p.to_string_lossy().to_string())))?)?;
    t.set("chdir", lua.create_function(|_, p: String| Ok(std::env::set_current_dir(&p).is_ok()))?)?;
    t.set("touch", lua.create_function(|_, _p: String| Ok(true))?)?;
    sf.set("lfs", t)?;
    Ok(())
}
