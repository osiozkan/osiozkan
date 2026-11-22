//! Lua'daki `subs` nesnesi ve satır tablosu dönüşümleri.
use mlua::{AnyUserData, Lua, MetaMethod, Result, Table, UserData, UserDataMethods, Value, Variadic};
use osisub_core::ass::{self, Dialogue, Entry, Style};
use osisub_core::color::Color;
use std::cell::RefCell;
use std::rc::Rc;

pub type SharedScript = Rc<RefCell<ass::Script>>;

type ExtraCache = Rc<RefCell<Option<std::collections::BTreeMap<u32, (String, String)>>>>;

/// Lua `subs` nesnesi. İkinci alan Extradata önbelleğidir (her yazmada temizlenir);
/// `line.extra` okumak böylece satır başına tüm belgeyi taramaz.
/// Üçüncü alan görünür dizin → ham dizin eşlemesidir: Aegisub gibi Lua yalnızca
/// info/style/dialogue satırlarını görür (Script Info `;` yorumları, `[Aegisub Project Garbage]`,
/// `[Fonts]`, Extradata gizli). Yoksa karaskel.collect_head ilk yorumda durup "No styles were found" verir.
pub struct Subs(pub SharedScript, ExtraCache, VisMap);

type VisMap = Rc<RefCell<Option<(usize, Vec<usize>)>>>;

impl Subs {
    pub fn new(script: SharedScript) -> Subs {
        Subs(script, Rc::new(RefCell::new(None)), Rc::new(RefCell::new(None)))
    }
}

/// Lua'ya görünen satır mı (Aegisub 3.x `subs` içeriği).
pub fn lua_visible(e: &Entry) -> bool {
    !matches!(e, Entry::Unknown { .. })
}

/// Lua'ya görünen satırların ham dizinleri (artan sıralı; Lua dizini = konum + 1).
pub fn visible_raw(script: &ass::Script) -> Vec<usize> {
    script.entries.iter().enumerate().filter(|(_, e)| lua_visible(e)).map(|(i, _)| i).collect()
}

/// Ham (0 tabanlı) dizin → Lua dizini (1 tabanlı); gizli satırsa `None`.
pub fn lua_index_of(script: &ass::Script, raw: usize) -> Option<usize> {
    if raw >= script.entries.len() || !lua_visible(&script.entries[raw]) { return None; }
    Some(script.entries[..raw].iter().filter(|e| lua_visible(e)).count() + 1)
}

/// Lua dizini (1 tabanlı) → ham (0 tabanlı) dizin.
pub fn raw_index_of(script: &ass::Script, lua_i: usize) -> Option<usize> {
    if lua_i == 0 { return None; }
    script.entries.iter().enumerate().filter(|(_, e)| lua_visible(e)).nth(lua_i - 1).map(|(i, _)| i)
}

fn with_map<R>(map: &VisMap, sc: &ass::Script, f: impl FnOnce(&[usize]) -> R) -> R {
    let mut m = map.borrow_mut();
    if m.as_ref().is_none_or(|(n, _)| *n != sc.entries.len()) {
        let v: Vec<usize> = sc.entries.iter().enumerate().filter(|(_, e)| lua_visible(e)).map(|(i, _)| i).collect();
        *m = Some((sc.entries.len(), v));
    }
    f(&m.as_ref().unwrap().1)
}

/// Lua dizini (1 tabanlı) → ham dizin; aralık dışıysa `None`.
fn raw_at(map: &VisMap, sc: &ass::Script, i: i64) -> Option<usize> {
    with_map(map, sc, |v| if i >= 1 { v.get(i as usize - 1).copied() } else { None })
}

/// Lua'da `i`'nci satırdan önce ekleme konumu (ham); `i` sondan sonraysa belge sonu.
fn raw_insert_pos(map: &VisMap, sc: &ass::Script, i: i64) -> usize {
    raw_at(map, sc, i.max(1)).unwrap_or(sc.entries.len())
}

/// Ham `p` konumuna satır eklendi: eşlemeyi yeniden kurmadan güncelle.
fn map_inserted(map: &VisMap, p: usize, visible: bool) {
    if let Some((n, v)) = map.borrow_mut().as_mut() {
        let k = v.partition_point(|&x| x < p);
        for x in &mut v[k..] { *x += 1; }
        if visible { v.insert(k, p); }
        *n += 1;
    }
}

/// Ham `p` konumundaki satır silindi.
fn map_removed(map: &VisMap, p: usize) {
    if let Some((n, v)) = map.borrow_mut().as_mut() {
        let k = v.partition_point(|&x| x < p);
        if v.get(k) == Some(&p) { v.remove(k); }
        for x in &mut v[k..] { *x -= 1; }
        *n -= 1;
    }
}

fn vis_len(map: &VisMap, sc: &ass::Script) -> usize {
    with_map(map, sc, |v| v.len())
}

/// Satırın `extra` tablosu: Extradata kimliklerinden anahtar → değer.
fn extra_table(lua: &Lua, d: &Dialogue, script: Option<(&ass::Script, &ExtraCache)>) -> Result<Table> {
    let t = lua.create_table()?;
    if d.extra.is_empty() {
        return Ok(t);
    }
    let Some((sc, cache)) = script else { return Ok(t) };
    let mut c = cache.borrow_mut();
    let all = c.get_or_insert_with(|| sc.extradata());
    for id in &d.extra {
        if let Some((k, v)) = all.get(id) {
            t.set(k.as_str(), v.as_str())?;
        }
    }
    Ok(t)
}

/// Lua `extra` tablosunu okur (dize anahtarlı, dize/sayı değerli girdiler).
fn read_extra(t: &Table) -> Result<std::collections::BTreeMap<String, String>> {
    let mut m = std::collections::BTreeMap::new();
    if let Ok(Value::Table(x)) = t.get::<Value>("extra") {
        for pair in x.pairs::<Value, Value>() {
            let (k, v) = pair?;
            let Value::String(k) = k else { continue };
            let v = match v {
                Value::String(s) => s.to_str()?.to_string(),
                Value::Integer(n) => n.to_string(),
                Value::Number(n) => n.to_string(),
                Value::Boolean(b) => b.to_string(),
                _ => continue,
            };
            m.insert(k.to_str()?.to_string(), v);
        }
    }
    Ok(m)
}

fn str_field(t: &Table, k: &str) -> Option<String> {
    match t.get::<Value>(k).ok()? {
        Value::String(s) => s.to_str().ok().map(|s| s.to_string()),
        Value::Number(n) => Some(n.to_string()),
        Value::Integer(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Aegisub: `lua_tointeger` keser, `AssTime` en yakın santisaniyeye yuvarlar ve [0, 10 sa) aralığına sıkıştırır
/// (`!retime("abs",1234.7,…)!` → 1230). Okununca aynı değer döner.
fn ass_time(v: f64) -> i64 {
    let t = if v.is_finite() { v.trunc().clamp(-1e12, 1e12) as i64 } else { 0 };
    let r = (t + 5) - (t + 5).rem_euclid(10);
    r.clamp(0, 10 * 3600 * 1000 - 1)
}

fn num_field(t: &Table, k: &str) -> Option<f64> {
    match t.get::<Value>(k).ok()? {
        Value::Number(n) => Some(n),
        Value::Integer(n) => Some(n as f64),
        Value::String(s) => s.to_str().ok()?.trim().parse().ok(),
        _ => None,
    }
}

fn bool_field(t: &Table, k: &str) -> Option<bool> {
    match t.get::<Value>(k).ok()? {
        Value::Boolean(b) => Some(b),
        Value::Number(n) => Some(n != 0.0),
        Value::Integer(n) => Some(n != 0),
        _ => None,
    }
}

fn section_name(sec: &str) -> String {
    format!("[{}]", sec)
}

fn entry_to_table_in(lua: &Lua, e: &Entry, script: Option<(&ass::Script, &ExtraCache)>) -> Result<Table> {
    let t = lua.create_table()?;
    t.set("section", section_name(e.section()))?;
    match e {
        Entry::Info { key, value } => {
            t.set("class", "info")?;
            t.set("key", key.as_str())?;
            t.set("value", value.as_str())?;
            t.set("raw", format!("{}: {}", key, value))?;
        }
        Entry::Style(s) => {
            t.set("class", "style")?;
            t.set("name", s.name.as_str())?;
            t.set("fontname", s.fontname.as_str())?;
            t.set("fontsize", s.fontsize)?;
            t.set("color1", format!("{}&", s.color1))?; // Aegisub: GetAssStyleFormatted() + "&"
            t.set("color2", format!("{}&", s.color2))?; // Aegisub: GetAssStyleFormatted() + "&"
            t.set("color3", format!("{}&", s.color3))?; // Aegisub: GetAssStyleFormatted() + "&"
            t.set("color4", format!("{}&", s.color4))?; // Aegisub: GetAssStyleFormatted() + "&"
            t.set("bold", s.bold)?;
            t.set("italic", s.italic)?;
            t.set("underline", s.underline)?;
            t.set("strikeout", s.strikeout)?;
            t.set("scale_x", s.scale_x)?;
            t.set("scale_y", s.scale_y)?;
            t.set("spacing", s.spacing)?;
            t.set("angle", s.angle)?;
            t.set("borderstyle", s.borderstyle)?;
            t.set("outline", s.outline)?;
            t.set("shadow", s.shadow)?;
            t.set("align", s.align)?;
            t.set("margin_l", s.margin_l)?;
            t.set("margin_r", s.margin_r)?;
            t.set("margin_t", s.margin_t)?;
            t.set("margin_b", s.margin_b)?;
            t.set("margin_v", s.margin_t)?;
            t.set("encoding", s.encoding)?;
            t.set("relative_to", 2)?;
            t.set("raw", ass::style_line(s))?;
        }
        Entry::Dialogue(d) => {
            t.set("class", "dialogue")?;
            t.set("comment", d.comment)?;
            t.set("layer", d.layer)?;
            t.set("start_time", d.start_time)?;
            t.set("end_time", d.end_time)?;
            t.set("style", d.style.as_str())?;
            t.set("actor", d.actor.as_str())?;
            t.set("margin_l", d.margin_l)?;
            t.set("margin_r", d.margin_r)?;
            t.set("margin_t", d.margin_t)?;
            t.set("margin_b", d.margin_b)?;
            t.set("margin_v", d.margin_t)?;
            t.set("effect", d.effect.as_str())?;
            t.set("text", d.text.as_str())?;
            t.set("extra", extra_table(lua, d, script)?)?;
            t.set("raw", ass::dialogue_line(d))?;
        }
        Entry::Unknown { raw, .. } => {
            t.set("class", "unknown")?;
            t.set("raw", raw.as_str())?;
        }
    }
    Ok(t)
}

pub fn table_to_entry(t: &Table, default_section: &str) -> Result<Entry> {
    let class = str_field(t, "class").unwrap_or_default();
    let int = |k: &str, d: i32| num_field(t, k).map(|v| v as i32).unwrap_or(d);
    Ok(match class.as_str() {
        "info" => Entry::Info {
            key: str_field(t, "key").unwrap_or_default(),
            value: str_field(t, "value").unwrap_or_default(),
        },
        "style" => {
            let base = Style::default();
            let col = |k: &str, d: &str| str_field(t, k).map(|v| Color::parse(&v).map(|c| c.to_style_string()).unwrap_or(v)).unwrap_or(d.to_string());
            let mt = if t.contains_key("margin_t")? { int("margin_t", 10) } else { int("margin_v", 10) };
            let mb = if t.contains_key("margin_b")? { int("margin_b", mt) } else { mt };
            Entry::Style(Style {
                name: str_field(t, "name").unwrap_or(base.name),
                fontname: str_field(t, "fontname").unwrap_or(base.fontname),
                fontsize: num_field(t, "fontsize").unwrap_or(base.fontsize),
                color1: col("color1", &base.color1),
                color2: col("color2", &base.color2),
                color3: col("color3", &base.color3),
                color4: col("color4", &base.color4),
                bold: bool_field(t, "bold").unwrap_or(false),
                italic: bool_field(t, "italic").unwrap_or(false),
                underline: bool_field(t, "underline").unwrap_or(false),
                strikeout: bool_field(t, "strikeout").unwrap_or(false),
                scale_x: num_field(t, "scale_x").unwrap_or(100.0),
                scale_y: num_field(t, "scale_y").unwrap_or(100.0),
                spacing: num_field(t, "spacing").unwrap_or(0.0),
                angle: num_field(t, "angle").unwrap_or(0.0),
                borderstyle: int("borderstyle", 1),
                outline: num_field(t, "outline").unwrap_or(2.0),
                shadow: num_field(t, "shadow").unwrap_or(2.0),
                align: int("align", 2),
                margin_l: int("margin_l", 10),
                margin_r: int("margin_r", 10),
                margin_t: mt,
                margin_b: mb,
                encoding: int("encoding", 1),
            })
        }
        "dialogue" => {
            let mt = if t.contains_key("margin_t")? { int("margin_t", 0) } else { int("margin_v", 0) };
            let mb = if t.contains_key("margin_b")? { int("margin_b", mt) } else { mt };
            Entry::Dialogue(Dialogue {
                comment: bool_field(t, "comment").unwrap_or(false),
                layer: int("layer", 0),
                start_time: ass_time(num_field(t, "start_time").unwrap_or(0.0)),
                end_time: ass_time(num_field(t, "end_time").unwrap_or(0.0)),
                style: str_field(t, "style").unwrap_or_else(|| "Default".into()),
                actor: str_field(t, "actor").unwrap_or_default(),
                margin_l: int("margin_l", 0),
                margin_r: int("margin_r", 0),
                margin_t: mt,
                margin_b: mb,
                effect: str_field(t, "effect").unwrap_or_default(),
                text: str_field(t, "text").unwrap_or_default(),
                extra: Vec::new(),
            })
        }
        // Aegisub: sınıfsız ya da bilinmeyen sınıflı tablo hatadır (boş/çöp satır yazılmasın)
        "" => return Err(mlua::Error::runtime("Table lacks 'class' field")),
        c if c != "unknown" => return Err(mlua::Error::runtime(format!("Found line with unknown class: {}", c))),
        _ => {
            let section = str_field(t, "section")
                .map(|s| s.trim_start_matches('[').trim_end_matches(']').to_string())
                .unwrap_or_else(|| default_section.to_string());
            Entry::Unknown { section, raw: str_field(t, "raw").unwrap_or_default() }
        }
    })
}

/// Tablodan girdi; diyalogsa `extra` tablosu belgenin Extradata bölümüne işlenir.
fn table_to_entry_in(t: &Table, default_section: &str, script: &mut ass::Script, cache: &ExtraCache) -> Result<Entry> {
    let mut e = table_to_entry(t, default_section)?;
    if let Entry::Dialogue(d) = &mut e {
        let m = read_extra(t)?;
        if !m.is_empty() {
            d.extra = script.intern_extra(&m);
        }
    }
    *cache.borrow_mut() = None;
    Ok(e)
}

fn idx(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(i) => Some(*i),
        Value::Number(n) => Some(*n as i64),
        _ => None,
    }
}

fn last_section(script: &ass::Script) -> String {
    script.entries.last().map(|e| e.section().to_string()).unwrap_or_else(|| ass::SEC_EVENTS.to_string())
}

impl UserData for Subs {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_function(MetaMethod::Index, |lua, (ud, key): (AnyUserData, Value)| -> Result<Value> {
            let (shared, cache, map) = {
                let b = ud.borrow::<Subs>()?;
                (b.0.clone(), b.1.clone(), b.2.clone())
            };
            match &key {
                Value::String(s) => {
                    let name = s.to_str()?.to_string();
                    let sh = shared.clone();
                    let ca = cache.clone();
                    let mp = map.clone();
                    let f = match name.as_str() {
                        "n" => return Ok(Value::Integer(vis_len(&map, &shared.borrow()) as i64)),
                        "append" => lua.create_function(move |lua, items: Variadic<Table>| {
                            let mut sc = sh.borrow_mut();
                            for t in items {
                                let sec = last_section(&sc);
                                let e = table_to_entry_in(&t, &sec, &mut sc, &ca)?;
                                sc.append_in_section(e);
                                *mp.borrow_mut() = None;
                            }
                            let _ = lua;
                            Ok(())
                        })?,
                        "insert" => lua.create_function(move |_, (i, items): (i64, Variadic<Table>)| {
                            let mut sc = sh.borrow_mut();
                            let mut pos = raw_insert_pos(&mp, &sc, i);
                            for t in items {
                                let sec = last_section(&sc);
                                let e = table_to_entry_in(&t, &sec, &mut sc, &ca)?;
                                let p = pos.min(sc.entries.len());
                                let vis = lua_visible(&e);
                                sc.entries.insert(p, e);
                                map_inserted(&mp, p, vis);
                                pos += 1;
                            }
                            Ok(())
                        })?,
                        "script_resolution" => lua.create_function(move |_, ()| Ok(sh.borrow().script_resolution()))?,
                        "delete" => lua.create_function(move |_, items: Variadic<Value>| {
                            *ca.borrow_mut() = None;
                            let mut sc = sh.borrow_mut();
                            // Aegisub: tek tablo bağımsız değişkeni → içindeki indekslerin hepsi
                            let num = |x: &Value| match x { Value::Integer(i) => Some(*i), Value::Number(n) => Some(*n as i64), _ => None };
                            let v: Vec<i64> = match items.first() {
                                Some(Value::Table(tb)) if items.len() == 1 => tb.clone().sequence_values::<Value>().filter_map(|x| x.ok()).filter_map(|x| num(&x)).collect(),
                                _ => items.iter().map(|x| num(x).ok_or_else(|| mlua::Error::runtime("subs.delete: indeks sayı olmalı"))).collect::<mlua::Result<_>>()?,
                            };
                            let mut raw: Vec<usize> = v.into_iter().filter_map(|i| raw_at(&mp, &sc, i)).collect();
                            raw.sort_unstable();
                            raw.dedup();
                            for r in raw.into_iter().rev() {
                                sc.entries.remove(r);
                                map_removed(&mp, r);
                            }
                            Ok(())
                        })?,
                        "deleterange" => lua.create_function(move |_, (a, b): (i64, i64)| {
                            *ca.borrow_mut() = None;
                            let mut sc = sh.borrow_mut();
                            let raw: Vec<usize> = with_map(&mp, &sc, |v| {
                                let a = (a.max(1) - 1) as usize;
                                let b = (b.max(0) as usize).min(v.len());
                                if a < b { v[a..b].to_vec() } else { vec![] }
                            });
                            for r in raw.into_iter().rev() {
                                sc.entries.remove(r);
                                map_removed(&mp, r);
                            }
                            Ok(())
                        })?,
                        _ => return Ok(Value::Nil),
                    };
                    Ok(Value::Function(f))
                }
                other => {
                    let Some(i) = idx(other) else { return Ok(Value::Nil) };
                    let sc = shared.borrow();
                    match raw_at(&map, &sc, i) {
                        Some(r) => Ok(Value::Table(entry_to_table_in(lua, &sc.entries[r], Some((&sc, &cache)))?)),
                        None => Ok(Value::Nil),
                    }
                }
            }
        });
        methods.add_meta_function(MetaMethod::NewIndex, |_, (ud, key, val): (AnyUserData, Value, Value)| {
            let (shared, cache, map) = {
                let b = ud.borrow::<Subs>()?;
                (b.0.clone(), b.1.clone(), b.2.clone())
            };
            let Some(i) = idx(&key) else { return Ok(()) };
            let mut sc = shared.borrow_mut();
            *cache.borrow_mut() = None;
            match val {
                Value::Nil => {
                    if let Some(r) = raw_at(&map, &sc, i) {
                        sc.entries.remove(r);
                        map_removed(&map, r);
                    }
                }
                Value::Table(t) => {
                    let sec = last_section(&sc);
                    let e = table_to_entry_in(&t, &sec, &mut sc, &cache)?;
                    if let Some(r) = raw_at(&map, &sc, i) {
                        let keep = lua_visible(&e);
                        sc.entries[r] = e;
                        if !keep { *map.borrow_mut() = None; }
                    } else if i < 0 {
                        // Aegisub: negatif dizin = |i|'nci satırdan ÖNCE ekle.
                        let at = raw_insert_pos(&map, &sc, -i);
                        let vis = lua_visible(&e);
                        sc.entries.insert(at, e);
                        map_inserted(&map, at, vis);
                    } else {
                        let vis = lua_visible(&e);
                        sc.entries.push(e);
                        map_inserted(&map, sc.entries.len() - 1, vis);
                    }
                }
                _ => return Err(mlua::Error::runtime("subs: atama için tablo ya da nil gerekli")),
            }
            Ok(())
        });
        methods.add_meta_method(MetaMethod::Len, |_, this, ()| Ok(vis_len(&this.2, &this.0.borrow()) as i64));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aegisub_zaman_yuvarlama() {
        assert_eq!(ass_time(1234.7), 1230);
        assert_eq!(ass_time(1236.0), 1240);
        assert_eq!(ass_time(1235.0), 1240);
        assert_eq!(ass_time(-20.0), 0);
        assert_eq!(ass_time(f64::NAN), 0);
        assert_eq!(ass_time(1e15), 10 * 3600 * 1000 - 1);
    }

    #[test]
    fn entry_table_roundtrip() {
        let lua = Lua::new();
        let entries = vec![
            Entry::Info { key: "Title".into(), value: "x: y".into() },
            Entry::Style(Style { name: "A b".into(), fontname: "Noto".into(), fontsize: 22.5, bold: true, align: 7, margin_l: 3, margin_r: 4, margin_t: 5, margin_b: 5, outline: 1.25, ..Default::default() }),
            Entry::Dialogue(Dialogue { comment: true, layer: 2, start_time: 1230, end_time: 4560, style: "A b".into(), actor: "Ali".into(), margin_l: 1, margin_r: 2, margin_t: 3, margin_b: 3, effect: "karaoke".into(), text: "{\\k10}a, b\\Nç".into(), extra: vec![] }),
            Entry::Unknown { section: "Aegisub Project Garbage".into(), raw: "Video File: x.mkv".into() },
        ];
        for e in entries {
            let t = entry_to_table_in(&lua, &e, None).unwrap();
            let back = table_to_entry(&t, e.section()).unwrap();
            assert_eq!(back, e);
        }
    }
}
