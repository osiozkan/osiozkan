//! Lua'ya sunulan yerel işlevler.
use crate::{embedded_libs, fonts, HostOptions, Result, ScriptError};
use fancy_regex::{Regex, RegexBuilder};
use mlua::{Lua, Table, Value};
use std::path::Path;

pub fn build(lua: &Lua, script_path: &Path, opts: &HostOptions) -> Result<Table> {
    let n = lua.create_table()?;

    // Seçenekler
    let o = lua.create_table()?;
    if let Some(fps) = opts.fps {
        o.set("fps", fps)?;
    }
    let params = lua.create_table()?;
    for (k, v) in &opts.params {
        params.set(k.as_str(), v.as_str())?;
    }
    o.set("params", params)?;
    let script_dir = script_path.parent().map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_else(|| ".".into());
    o.set("script_dir", script_dir)?;
    if let Some(sp) = &opts.subs_path {
        o.set("subs_name", sp.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default())?;
        o.set("script_dir_subs", sp.parent().map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_else(|| ".".into()))?;
    }
    let kf = lua.create_table()?;
    if let Some(fps) = opts.fps {
        for (i, ms) in opts.keyframes_ms.iter().enumerate() {
            kf.set(i + 1, yozora_core::time::frame_from_ms(*ms, fps))?;
        }
    }
    o.set("keyframes", kf)?;
    if let Some((w, h)) = opts.video_size {
        o.set("video_w", w)?;
        o.set("video_h", h)?;
    }
    let inc = lua.create_table()?;
    for (i, d) in opts.include_dirs.iter().enumerate() {
        inc.set(i + 1, d.to_string_lossy().replace('\\', "/"))?;
    }
    o.set("include_dirs", inc)?;
    n.set("options", o)?;

    // Gömülü kitaplıklar
    let emb = lua.create_table()?;
    for (name, src) in embedded_libs() {
        emb.set(name, src)?;
    }
    n.set("embedded", emb)?;

    n.set("progress", lua.create_function(|_, (_k, _v): (String, Value)| Ok(()))?)?;

    n.set(
        "text_extents",
        lua.create_function(|_, (style, text): (Table, String)| {
            let name: String = style.get::<Option<String>>("fontname")?.unwrap_or_else(|| "Arial".into());
            let g = |k: &str, d: f64| style.get::<Option<f64>>(k).ok().flatten().unwrap_or(d);
            let b = |k: &str| matches!(style.get::<Value>(k), Ok(Value::Boolean(true)));
            let m = fonts::StyleMetrics {
                fontname: &name,
                fontsize: g("fontsize", 40.0),
                bold: b("bold"),
                italic: b("italic"),
                scale_x: g("scale_x", 100.0),
                scale_y: g("scale_y", 100.0),
                spacing: g("spacing", 0.0),
            };
            Ok(fonts::extents(&m, &text))
        })?,
    )?;

    // Unicode
    n.set("codepoint", lua.create_function(|_, s: String| Ok(s.chars().next().map(|c| c as u32).unwrap_or(0)))?)?;
    n.set(
        "charwidth",
        lua.create_function(|_, s: String| {
            let u = s.chars().next().map(|c| c as u32).unwrap_or(0);
            let wide = (0x1100..=0x115F).contains(&u)
                || (0x2E80..=0xA4CF).contains(&u)
                || (0xAC00..=0xD7A3).contains(&u)
                || (0xF900..=0xFAFF).contains(&u)
                || (0xFE30..=0xFE6F).contains(&u)
                || (0xFF00..=0xFF60).contains(&u);
            Ok(if wide { 2 } else { 1 })
        })?,
    )?;
    n.set("upper", lua.create_function(|_, s: String| Ok(s.to_uppercase()))?)?;
    n.set("lower", lua.create_function(|_, s: String| Ok(s.to_lowercase()))?)?;
    n.set("fold", lua.create_function(|_, s: String| Ok(s.to_lowercase()))?)?;

    // Düzenli ifadeler
    n.set("re", build_re(lua)?)?;
    lua.globals().set("__yozora_native", n.clone())?;
    Ok(n)
}

fn compile(pat: &str, flags: i64) -> std::result::Result<Regex, mlua::Error> {
    RegexBuilder::new(pat)
        .case_insensitive(flags & 1 != 0)
        .build()
        .map_err(|e| mlua::Error::RuntimeError(format!("re: geçersiz desen '{pat}': {e}")))
}

fn match_table(lua: &Lua, s: &str, start: usize, end: usize) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("str", &s[start..end])?;
    t.set("first", start + 1)?;
    t.set("last", end)?;
    Ok(t)
}

fn build_re(lua: &Lua) -> Result<Table> {
    let re = lua.create_table()?;
    re.set("ICASE", 1)?;
    re.set("NOSUB", 2)?;
    re.set("EXTENDED", 4)?;
    re.set("NO_MOD_M", 8)?;
    re.set("NO_MOD_S", 16)?;

    // find(str, pat, flags): tüm eşleşmelerin listesi (nil = yok)
    re.set(
        "find",
        lua.create_function(|lua, (s, pat, flags): (String, String, Option<i64>)| {
            let rx = compile(&pat, flags.unwrap_or(0))?;
            let out = lua.create_table()?;
            let mut n = 0;
            for m in rx.find_iter(&s) {
                let m = m.map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                n += 1;
                out.set(n, match_table(lua, &s, m.start(), m.end())?)?;
            }
            Ok(if n == 0 { Value::Nil } else { Value::Table(out) })
        })?,
    )?;

    // match(str, pat, flags): ilk eşleşmenin grupları ([1] = tümü)
    re.set(
        "match",
        lua.create_function(|lua, (s, pat, flags): (String, String, Option<i64>)| {
            let rx = compile(&pat, flags.unwrap_or(0))?;
            let caps = rx.captures(&s).map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            let Some(caps) = caps else { return Ok(Value::Nil) };
            let out = lua.create_table()?;
            for i in 0..caps.len() {
                match caps.get(i) {
                    Some(m) => out.set(i + 1, match_table(lua, &s, m.start(), m.end())?)?,
                    None => {
                        let t = lua.create_table()?;
                        t.set("str", "")?;
                        t.set("first", 0)?;
                        t.set("last", -1)?;
                        out.set(i + 1, t)?
                    }
                }
            }
            Ok(Value::Table(out))
        })?,
    )?;

    // sub(str, pat, repl, count?, flags?): `\1`/`$1` geri başvuruları
    re.set(
        "sub",
        lua.create_function(|_, (s, pat, repl, count, flags): (String, String, Value, Option<i64>, Option<i64>)| {
            let rx = compile(&pat, flags.unwrap_or(0))?;
            let max = count.filter(|c| *c > 0).map(|c| c as usize).unwrap_or(usize::MAX);
            let repl_s = match &repl {
                Value::String(v) => v.to_string_lossy().to_string(),
                _ => return Err(mlua::Error::RuntimeError("re.sub: değiştirme dizesi bekleniyor".into())),
            };
            let mut out = String::new();
            let mut last = 0usize;
            let mut done = 0usize;
            for caps in rx.captures_iter(&s) {
                if done >= max {
                    break;
                }
                let caps = caps.map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                let m = caps.get(0).unwrap();
                out.push_str(&s[last..m.start()]);
                let rb = repl_s.as_bytes();
                let mut k = 0;
                while k < rb.len() {
                    let c = rb[k];
                    if (c == b'\\' || c == b'$') && k + 1 < rb.len() && rb[k + 1].is_ascii_digit() {
                        let g = (rb[k + 1] - b'0') as usize;
                        if let Some(gm) = caps.get(g) {
                            out.push_str(gm.as_str());
                        }
                        k += 2;
                    } else if c == b'\\' && k + 1 < rb.len() && (rb[k + 1] == b'\\' || rb[k + 1] == b'$') {
                        out.push(rb[k + 1] as char);
                        k += 2;
                    } else {
                        // UTF-8 güvenli kopya
                        let ch = repl_s[k..].chars().next().unwrap();
                        out.push(ch);
                        k += ch.len_utf8();
                    }
                }
                last = m.end();
                done += 1;
            }
            out.push_str(&s[last..]);
            Ok((out, done))
        })?,
    )?;
    let sub: mlua::Function = re.get("sub")?;
    re.set("gsub", sub)?;

    // Lua yardımcıları: compile nesnesi, split
    lua.load(
        r#"
        local re = ...
        function re.compile(pat, flags)
          local o = {}
          function o:find(s) return re.find(s, pat, flags) end
          function o:match(s) return re.match(s, pat, flags) end
          function o:sub(s, repl, count) return re.sub(s, pat, repl, count, flags) end
          o.gsub = o.sub
          return o
        end
        function re.split(s, pat, flags)
          local out, pos = {}, 1
          local ms = re.find(s, pat, flags) or {}
          for _, m in ipairs(ms) do
            out[#out + 1] = s:sub(pos, m.first - 1)
            pos = m.last + 1
          end
          out[#out + 1] = s:sub(pos)
          return out
        end
        "#,
    )
    .call::<()>(re.clone())
    .map_err(|e: mlua::Error| ScriptError(e.to_string()))?;
    Ok(re)
}
