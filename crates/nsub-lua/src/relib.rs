//! Native helpers behind the Lua ports of `aegisub.re`, `aegisub.unicode` and `aegisub.lfs`.

use fancy_regex::Regex;
use mlua::{AnyUserData, Lua, Result, UserData, Value};

struct Re(Regex);
impl UserData for Re {}
struct Groups(Vec<Option<(usize, usize)>>);
impl UserData for Groups {}

// Boost-style flag bits, matching the order in lua/aegisub/re.lua (value = 2^(index-1)).
const F_ICASE: i64 = 1;
const F_NO_MOD_M: i64 = 16;
const F_NO_MOD_S: i64 = 32;
const F_MOD_S: i64 = 64;
const F_MOD_X: i64 = 128;

fn text<'a>(s: &'a mlua::LuaString, buf: &'a mut Vec<u8>) -> Result<&'a str> {
    *buf = s.as_bytes().to_vec();
    std::str::from_utf8(buf).map_err(|_| mlua::Error::runtime("input is not valid UTF-8"))
}

fn expand(rep: &str, caps: &fancy_regex::Captures, out: &mut String) {
    let b: Vec<char> = rep.chars().collect();
    let mut i = 0;
    let group = |n: usize, out: &mut String| {
        if let Some(m) = caps.get(n) {
            out.push_str(m.as_str());
        }
    };
    while i < b.len() {
        let c = b[i];
        if (c == '$' || c == '\\') && i + 1 < b.len() {
            let n = b[i + 1];
            if n.is_ascii_digit() {
                group(n.to_digit(10).unwrap() as usize, out);
                i += 2;
                continue;
            }
            if c == '$' && n == '&' {
                group(0, out);
                i += 2;
                continue;
            }
            if c == '$' && n == '{' {
                if let Some(end) = b[i + 2..].iter().position(|&x| x == '}') {
                    let num: String = b[i + 2..i + 2 + end].iter().collect();
                    if let Ok(k) = num.parse::<usize>() {
                        group(k, out);
                        i += end + 3;
                        continue;
                    }
                }
            }
            if c == '$' && n == '$' {
                out.push('$');
                i += 2;
                continue;
            }
            if c == '\\' && (n == '\\' || n == '$') {
                out.push(n);
                i += 2;
                continue;
            }
            if c == '\\' && n == 'n' {
                out.push('\n');
                i += 2;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
}

pub(crate) fn install(lua: &Lua) -> Result<()> {
    let g = lua.globals();
    let re = lua.create_table()?;

    re.set(
        "compile",
        lua.create_function(|lua, (pat, flags): (mlua::LuaString, i64)| -> Result<(Value, Option<String>)> {
            let mut p = String::new();
            let mut mods = String::new();
            if flags & F_ICASE != 0 {
                mods.push('i');
            }
            if flags & F_NO_MOD_M == 0 {
                mods.push('m');
            }
            if flags & F_NO_MOD_S == 0 || flags & F_MOD_S != 0 {
                mods.push('s');
            }
            if flags & F_MOD_X != 0 {
                mods.push('x');
            }
            if !mods.is_empty() {
                p.push_str(&format!("(?{mods})"));
            }
            p.push_str(&String::from_utf8_lossy(&pat.as_bytes()));
            match Regex::new(&p) {
                Ok(r) => Ok((Value::UserData(lua.create_userdata(Re(r))?), None)),
                Err(e) => Ok((Value::Nil, Some(e.to_string()))),
            }
        })?,
    )?;

    // search(re, str, start0) -> first, last   (1-based, inclusive)
    re.set(
        "search",
        lua.create_function(|_, (re, s, start): (AnyUserData, mlua::LuaString, usize)| {
            let re = re.borrow::<Re>()?;
            let mut buf = vec![];
            let t = text(&s, &mut buf)?;
            if start > t.len() || !t.is_char_boundary(start) {
                return Ok(mlua::MultiValue::new());
            }
            match re.0.find_from_pos(t, start).map_err(|e| mlua::Error::runtime(e.to_string()))? {
                Some(m) => Ok(mlua::MultiValue::from_vec(vec![Value::Integer(m.start() as i64 + 1), Value::Integer(m.end() as i64)])),
                None => Ok(mlua::MultiValue::new()),
            }
        })?,
    )?;

    // match(re, str, start0) -> groups | nil
    re.set(
        "match",
        lua.create_function(|lua, (re, s, start): (AnyUserData, mlua::LuaString, usize)| {
            let re = re.borrow::<Re>()?;
            let mut buf = vec![];
            let t = text(&s, &mut buf)?;
            if start > t.len() || !t.is_char_boundary(start) {
                return Ok(Value::Nil);
            }
            match re.0.captures_from_pos(t, start).map_err(|e| mlua::Error::runtime(e.to_string()))? {
                Some(c) => {
                    let v: Vec<Option<(usize, usize)>> = (0..c.len()).map(|i| c.get(i).map(|m| (m.start() + 1, m.end()))).collect();
                    Ok(Value::UserData(lua.create_userdata(Groups(v))?))
                }
                None => Ok(Value::Nil),
            }
        })?,
    )?;

    // group(groups, idx) -> first, last | nil
    re.set(
        "group",
        lua.create_function(|_, (m, idx): (AnyUserData, usize)| {
            let m = m.borrow::<Groups>()?;
            Ok(match m.0.get(idx).copied().flatten() {
                Some((a, b)) => mlua::MultiValue::from_vec(vec![Value::Integer(a as i64), Value::Integer(b as i64)]),
                None => mlua::MultiValue::new(),
            })
        })?,
    )?;

    // replace(re, repl, str, max_count) -> string
    re.set(
        "replace",
        lua.create_function(|_, (re, rep, s, max): (AnyUserData, mlua::LuaString, mlua::LuaString, i64)| {
            let re = re.borrow::<Re>()?;
            let (mut b1, mut b2) = (vec![], vec![]);
            let rep = text(&rep, &mut b1)?;
            let t = text(&s, &mut b2)?;
            let mut out = String::new();
            let mut last = 0;
            let mut n = 0;
            for c in re.0.captures_iter(t) {
                if n >= max {
                    break;
                }
                let c = c.map_err(|e| mlua::Error::runtime(e.to_string()))?;
                let m = c.get(0).unwrap();
                out.push_str(&t[last..m.start()]);
                expand(rep, &c, &mut out);
                last = m.end();
                n += 1;
            }
            out.push_str(&t[last..]);
            Ok(out)
        })?,
    )?;
    g.set("__nsub_re", re)?;

    g.set(
        "__nsub_case",
        lua.create_function(|_, (kind, s): (String, mlua::LuaString)| {
            let t = String::from_utf8_lossy(&s.as_bytes()).to_string();
            Ok(match kind.as_str() {
                "upper" => t.to_uppercase(),
                "lower" | "fold" => t.to_lowercase(),
                "upper_tr" => nsub_core::text::upper_tr(&t),
                "lower_tr" => nsub_core::text::lower_tr(&t),
                _ => t,
            })
        })?,
    )?;

    g.set(
        "__nsub_fs",
        lua.create_function(|lua, args: mlua::MultiValue| -> Result<mlua::MultiValue> {
            let mut it = args.into_iter();
            let op = match it.next() {
                Some(Value::String(s)) => s.to_string_lossy().to_string(),
                _ => return Err(mlua::Error::runtime("fs: missing op")),
            };
            let path = match it.next() {
                Some(Value::String(s)) => s.to_string_lossy().to_string(),
                _ => String::new(),
            };
            let ok = |b: bool| -> Result<mlua::MultiValue> { Ok(mlua::MultiValue::from_vec(vec![if b { Value::Number(0.0) } else { Value::Nil }])) };
            match op.as_str() {
                "stat" => match std::fs::metadata(&path) {
                    Ok(m) => {
                        let mtime = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0.0, |d| d.as_secs() as f64);
                        Ok(mlua::MultiValue::from_vec(vec![
                            Value::String(lua.create_string(if m.is_dir() { "directory" } else { "file" })?),
                            Value::Number(mtime),
                            Value::Number(m.len() as f64),
                        ]))
                    }
                    Err(_) => Ok(mlua::MultiValue::new()),
                },
                "cwd" => Ok(mlua::MultiValue::from_vec(vec![Value::String(
                    lua.create_string(std::env::current_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_default())?,
                )])),
                "chdir" => ok(std::env::set_current_dir(&path).is_ok()),
                "mkdir" => ok(std::fs::create_dir(&path).is_ok()),
                "rmdir" => ok(std::fs::remove_dir(&path).is_ok()),
                "touch" => ok(std::fs::OpenOptions::new().create(true).append(true).open(&path).is_ok()),
                "list" => match std::fs::read_dir(&path) {
                    Ok(rd) => {
                        let t = lua.create_table()?;
                        t.raw_push(".")?;
                        t.raw_push("..")?;
                        for e in rd.flatten() {
                            t.raw_push(e.file_name().to_string_lossy().to_string())?;
                        }
                        Ok(mlua::MultiValue::from_vec(vec![Value::Table(t)]))
                    }
                    Err(_) => Ok(mlua::MultiValue::new()),
                },
                other => Err(mlua::Error::runtime(format!("fs: unknown op {other}"))),
            }
        })?,
    )?;
    Ok(())
}
