//! Native modules exposed to scripts: `unicode`, `re` (native half) and `lfs`.
use fancy_regex::Regex;
use mlua::{Lua, MultiValue, Result, Table, UserData, UserDataRef, Value};
use std::cell::RefCell;
use std::rc::Rc;
use subforge_core::turkish;

fn lstr(s: &mlua::LuaString) -> String {
    String::from_utf8_lossy(&s.as_bytes()).into_owned()
}

/// Byte width of the UTF-8 character starting at byte `i` (1-based), as Aegisub's `unicode.charwidth`.
fn char_width(b: Option<u8>) -> usize {
    match b {
        None => 1,
        Some(b) if b < 128 => 1,
        Some(b) if b < 224 => 2,
        Some(b) if b < 240 => 3,
        Some(_) => 4,
    }
}

pub fn install_unicode(lua: &Lua) -> Result<Table> {
    let t = lua.create_table()?;
    t.set(
        "codepoint",
        lua.create_function(|_, s: mlua::LuaString| Ok(lstr(&s).chars().next().map(|c| c as u32).unwrap_or(0)))?,
    )?;
    t.set("len", lua.create_function(|_, s: mlua::LuaString| Ok(lstr(&s).chars().count()))?)?;
    // Aegisub: iterator yielding (char, index).
    t.set(
        "chars",
        lua.create_function(|lua, s: mlua::LuaString| {
            let st = lstr(&s);
            let chars: Vec<String> = st.chars().map(|c| c.to_string()).collect();
            let idx = Rc::new(RefCell::new(0usize));
            lua.create_function(move |lua, ()| {
                let mut i = idx.borrow_mut();
                let r = chars.get(*i).cloned();
                *i += 1;
                Ok(match r {
                    Some(c) => MultiValue::from_iter([Value::String(lua.create_string(c)?), Value::Integer(*i as _)]),
                    None => MultiValue::new(),
                })
            })
        })?,
    )?;
    for (names, f) in [
        (["to_upper_case", "to_upper"], (|s: &str| s.to_uppercase()) as fn(&str) -> String),
        (["to_lower_case", "to_lower"], |s| s.to_lowercase()),
        (["to_fold_case", "to_fold"], |s| s.to_lowercase()),
    ] {
        for n in names {
            t.set(n, lua.create_function(move |_, s: mlua::LuaString| Ok(f(&lstr(&s))))?)?;
        }
    }
    // Turkish-aware variants (SubForge extension).
    t.set("to_upper_tr", lua.create_function(|_, s: String| Ok(turkish::upper(&s)))?)?;
    t.set("to_lower_tr", lua.create_function(|_, s: String| Ok(turkish::lower(&s)))?)?;
    t.set("title_tr", lua.create_function(|_, s: String| Ok(turkish::title(&s)))?)?;
    t.set(
        "charwidth",
        lua.create_function(|_, (s, i): (mlua::LuaString, Option<usize>)| {
            let i = i.unwrap_or(1).max(1);
            Ok(char_width(s.as_bytes().get(i - 1).copied()))
        })?,
    )?;
    Ok(t)
}

// ------------------------------------------------------------------ re
//
// Native half of `aegisub.re`: compile/search/match/replace over fancy-regex with
// Boost-perl defaults. The Lua half (`lua/aegisub_re.lua`) ports Aegisub's re.moon.

struct ReObj(Rc<Regex>);

impl UserData for ReObj {}

const F_ICASE: i64 = 1;
const F_NO_MOD_M: i64 = 16;
const F_NO_MOD_S: i64 = 32;
const F_MOD_X: i64 = 128;

const RE_FLAGS: [(&str, i64); 9] = [
    ("ICASE", F_ICASE),
    ("NOSUB", 2),
    ("COLLATE", 4),
    ("NEWLINE_ALT", 8),
    ("NO_MOD_M", F_NO_MOD_M),
    ("NO_MOD_S", F_NO_MOD_S),
    ("MOD_S", 64),
    ("MOD_X", F_MOD_X),
    ("NO_EMPTY_SUBEXPRESSIONS", 256),
];

fn build(pattern: &str, flags: i64) -> std::result::Result<Regex, String> {
    // Boost perl syntax: `.` matches newlines and ^/$ match at line boundaries by default.
    let mut f = String::new();
    if flags & F_NO_MOD_M == 0 {
        f.push('m');
    }
    if flags & F_NO_MOD_S == 0 {
        f.push('s');
    }
    if flags & F_ICASE != 0 {
        f.push('i');
    }
    if flags & F_MOD_X != 0 {
        f.push('x');
    }
    let full = if f.is_empty() { pattern.to_string() } else { format!("(?{f}){pattern}") };
    Regex::new(&full).map_err(|e| e.to_string())
}

#[derive(Clone, Copy, PartialEq)]
enum Case {
    None,
    Upper,
    Lower,
}

fn push_cased(out: &mut String, t: &str, mode: Case, one: &mut Option<Case>) {
    for (i, c) in t.chars().enumerate() {
        let m = if i == 0 { one.take().unwrap_or(mode) } else { mode };
        match m {
            Case::None => out.push(c),
            Case::Upper => out.extend(c.to_uppercase()),
            Case::Lower => out.extend(c.to_lowercase()),
        }
    }
}

/// Expand a Boost/perl replacement format (`$1`, `${1}`, `$&`, `\1`, `\n`, `\U..\E`...).
fn expand(caps: &fancy_regex::Captures, fmt: &str, out: &mut String) {
    let mut mode = Case::None;
    let mut one: Option<Case> = None;
    let group = |n: usize| caps.get(n).map(|m| m.as_str()).unwrap_or("");
    let cs: Vec<char> = fmt.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c == '$' && i + 1 < cs.len() {
            let n = cs[i + 1];
            if n.is_ascii_digit() {
                let (mut j, mut v) = (i + 1, 0usize);
                while j < cs.len() && cs[j].is_ascii_digit() && j < i + 3 {
                    v = v * 10 + cs[j].to_digit(10).unwrap() as usize;
                    j += 1;
                }
                push_cased(out, group(v), mode, &mut one);
                i = j;
                continue;
            }
            if n == '&' {
                push_cased(out, group(0), mode, &mut one);
                i += 2;
                continue;
            }
            if n == '{' {
                if let Some(end) = cs[i..].iter().position(|&x| x == '}') {
                    let num: String = cs[i + 2..i + end].iter().collect();
                    if let Ok(v) = num.parse::<usize>() {
                        push_cased(out, group(v), mode, &mut one);
                        i += end + 1;
                        continue;
                    }
                }
            }
        }
        if c == '\\' && i + 1 < cs.len() {
            let n = cs[i + 1];
            i += 2;
            match n {
                '0'..='9' => push_cased(out, group(n.to_digit(10).unwrap() as usize), mode, &mut one),
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                'U' => mode = Case::Upper,
                'L' => mode = Case::Lower,
                'E' => mode = Case::None,
                'u' => one = Some(Case::Upper),
                'l' => one = Some(Case::Lower),
                other => push_cased(out, &other.to_string(), mode, &mut one),
            }
            continue;
        }
        push_cased(out, &c.to_string(), mode, &mut one);
        i += 1;
    }
}

pub fn install_re_impl(lua: &Lua) -> Result<Table> {
    let t = lua.create_table()?;
    t.set(
        "compile",
        lua.create_function(|lua, (pattern, flags): (mlua::LuaString, i64)| -> Result<MultiValue> {
            Ok(match build(&lstr(&pattern), flags) {
                Ok(r) => MultiValue::from_iter([Value::UserData(lua.create_userdata(ReObj(Rc::new(r)))?)]),
                Err(e) => MultiValue::from_iter([Value::Nil, Value::String(lua.create_string(e)?)]),
            })
        })?,
    )?;
    // search(re, str, start0) -> first, last (1-based, inclusive) of the first match at/after byte start0
    t.set(
        "search",
        lua.create_function(|_, (re, s, start): (UserDataRef<ReObj>, mlua::LuaString, usize)| {
            let st = lstr(&s);
            if start > st.len() || !st.is_char_boundary(start) {
                return Ok((None, None));
            }
            Ok(match re.0.captures_from_pos(&st, start) {
                Ok(Some(c)) => {
                    let m = c.get(0).unwrap();
                    (Some(m.start() + 1), Some(m.end()))
                }
                _ => (None, None),
            })
        })?,
    )?;
    // match(re, str, start0) -> { {first,last}, ... } for the groups of the first match (cut at the
    // first unmatched group, as Aegisub does), or nil
    t.set(
        "match",
        lua.create_function(|lua, (re, s, start): (UserDataRef<ReObj>, mlua::LuaString, usize)| {
            let st = lstr(&s);
            if start > st.len() || !st.is_char_boundary(start) {
                return Ok(Value::Nil);
            }
            let Ok(Some(c)) = re.0.captures_from_pos(&st, start) else { return Ok(Value::Nil) };
            let out = lua.create_table()?;
            for (i, g) in c.iter().enumerate() {
                let Some(g) = g else { break };
                let e = lua.create_table()?;
                e.set(1, g.start() + 1)?;
                e.set(2, g.end())?;
                out.set(i + 1, e)?;
            }
            Ok(Value::Table(out))
        })?,
    )?;
    t.set(
        "replace",
        lua.create_function(|_, (re, repl, s, max): (UserDataRef<ReObj>, mlua::LuaString, mlua::LuaString, i64)| {
            let (repl, st) = (lstr(&repl), lstr(&s));
            let (mut out, mut last, mut left) = (String::new(), 0, max);
            for c in re.0.captures_iter(&st) {
                let Ok(c) = c else { break };
                if left <= 0 {
                    break;
                }
                let m = c.get(0).unwrap();
                out.push_str(&st[last..m.start()]);
                expand(&c, &repl, &mut out);
                last = m.end();
                left -= 1;
            }
            out.push_str(&st[last..]);
            Ok(out)
        })?,
    )?;
    let flags = lua.create_table()?;
    for (n, v) in RE_FLAGS {
        flags.set(n, v)?;
    }
    t.set("flags", flags)?;
    Ok(t)
}

/// Minimal LuaFileSystem (`lfs`): enough for scripts that probe paths or list folders.
pub fn install_lfs(lua: &Lua) -> Result<Table> {
    use std::time::UNIX_EPOCH;
    let t = lua.create_table()?;
    t.set(
        "attributes",
        lua.create_function(|lua, (path, attr): (String, Option<String>)| {
            let Ok(md) = std::fs::metadata(&path) else { return Ok((Value::Nil, Some("cannot obtain information from file".to_string()))) };
            let r = lua.create_table()?;
            r.set("mode", if md.is_dir() { "directory" } else if md.is_file() { "file" } else { "other" })?;
            r.set("size", md.len())?;
            let ts = |t: std::io::Result<std::time::SystemTime>| t.ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
            r.set("modification", ts(md.modified()))?;
            r.set("access", ts(md.accessed()))?;
            r.set("change", ts(md.modified()))?;
            r.set("permissions", if md.permissions().readonly() { "r--r--r--" } else { "rw-rw-rw-" })?;
            if let Some(a) = attr {
                return Ok((r.get::<Value>(a)?, None));
            }
            Ok((Value::Table(r), None))
        })?,
    )?;
    t.set(
        "dir",
        lua.create_function(|lua, path: String| {
            let mut names: Vec<String> = vec![".".into(), "..".into()];
            if let Ok(rd) = std::fs::read_dir(&path) {
                names.extend(rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()));
            } else {
                return Err(mlua::Error::runtime(format!("cannot open {path}")));
            }
            let idx = Rc::new(RefCell::new(0usize));
            lua.create_function(move |_, ()| {
                let mut i = idx.borrow_mut();
                let r = names.get(*i).cloned();
                *i += 1;
                Ok(r)
            })
        })?,
    )?;
    t.set("mkdir", lua.create_function(|_, p: String| Ok(std::fs::create_dir(&p).is_ok()))?)?;
    t.set("rmdir", lua.create_function(|_, p: String| Ok(std::fs::remove_dir(&p).is_ok()))?)?;
    t.set("currentdir", lua.create_function(|_, ()| Ok(std::env::current_dir().map(|p| p.to_string_lossy().to_string()).ok()))?)?;
    t.set("chdir", lua.create_function(|_, p: String| Ok(std::env::set_current_dir(p).is_ok()))?)?;
    t.set("touch", lua.create_function(|_, p: String| Ok(std::fs::OpenOptions::new().create(true).append(true).open(p).is_ok()))?)?;
    Ok(t)
}

pub fn preload(lua: &Lua, name: &str, value: Table) -> Result<()> {
    let package: Table = lua.globals().get("package")?;
    let loaded: Table = package.get("loaded")?;
    loaded.set(name, value)?;
    Ok(())
}
