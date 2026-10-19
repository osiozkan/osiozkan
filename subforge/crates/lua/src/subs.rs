//! The `subs` object handed to macros: a userdata over the shared document.
use crate::convert::{line_to_table, table_to_line};
use crate::doc::{Doc, Line};
use mlua::{AnyUserData, Lua, MetaMethod, MultiValue, Result, UserData, UserDataMethods, Value, Variadic};
use std::cell::RefCell;
use std::rc::Rc;

pub struct Subs(pub Rc<RefCell<Doc>>);

fn idx_of(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(i) => Some(*i as i64),
        Value::Number(n) if n.fract() == 0.0 => Some(*n as i64),
        _ => None,
    }
}

fn line_from_value(v: Value) -> Result<Line> {
    match v {
        Value::Table(t) => table_to_line(&t),
        other => Err(mlua::Error::runtime(format!("expected a line table, got {}", other.type_name()))),
    }
}

impl UserData for Subs {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Len, |_, this, ()| Ok(this.0.borrow().lines.len()));

        m.add_meta_method(MetaMethod::Index, |lua, this, key: Value| -> Result<Value> {
            if let Some(i) = idx_of(&key) {
                let doc = this.0.borrow();
                if i < 1 || i as usize > doc.lines.len() {
                    return Err(mlua::Error::runtime(format!("Requested out-of-range line from subtitle file: {i}")));
                }
                return Ok(Value::Table(line_to_table(lua, &doc.lines[i as usize - 1])?));
            }
            let Value::String(s) = key else { return Ok(Value::Nil) };
            let name = s.to_str()?.to_string();
            let doc = this.0.clone();
            let f = match name.as_str() {
                "n" => return Ok(Value::Integer(doc.borrow().lines.len() as _)),
                "insert" => lua.create_function(move |_, args: Variadic<Value>| {
                    let mut it = args.into_iter();
                    let at = it.next().and_then(|v| idx_of(&v)).ok_or_else(|| mlua::Error::runtime("subs.insert: first argument must be a line index"))?;
                    let mut d = doc.borrow_mut();
                    let at = (at.max(1) as usize - 1).min(d.lines.len());
                    let new: Vec<Line> = it.map(line_from_value).collect::<Result<_>>()?;
                    for (k, l) in new.into_iter().enumerate() {
                        d.lines.insert(at + k, l);
                    }
                    Ok(())
                })?,
                "append" => lua.create_function(move |_, args: Variadic<Value>| {
                    let new: Vec<Line> = args.into_iter().map(line_from_value).collect::<Result<_>>()?;
                    doc.borrow_mut().lines.extend(new);
                    Ok(())
                })?,
                "delete" => lua.create_function(move |_, args: Variadic<Value>| {
                    let mut idx: Vec<usize> = Vec::new();
                    for a in args {
                        match &a {
                            Value::Table(t) => {
                                for v in t.sequence_values::<i64>() {
                                    idx.push(v? as usize);
                                }
                            }
                            v => idx.push(idx_of(v).ok_or_else(|| mlua::Error::runtime("subs.delete: indices must be numbers"))? as usize),
                        }
                    }
                    idx.sort_unstable();
                    idx.dedup();
                    let mut d = doc.borrow_mut();
                    for &i in idx.iter().rev() {
                        if i >= 1 && i <= d.lines.len() {
                            d.lines.remove(i - 1);
                        }
                    }
                    Ok(())
                })?,
                "deleterange" => lua.create_function(move |_, (a, b): (i64, i64)| {
                    let mut d = doc.borrow_mut();
                    let (a, b) = (a.max(1) as usize, (b.max(0) as usize).min(d.lines.len()));
                    if a <= b {
                        d.lines.drain(a - 1..b);
                    }
                    Ok(())
                })?,
                "script_resolution" => lua.create_function(move |_, ()| {
                    let (x, y) = doc.borrow().play_res();
                    Ok((x, y))
                })?,
                _ => return Ok(Value::Nil),
            };
            Ok(Value::Function(f))
        });

        m.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (Value, Value)| {
            let i = idx_of(&key).ok_or_else(|| mlua::Error::runtime("subs: only integer indices can be assigned"))?;
            let mut d = this.0.borrow_mut();
            if i < 1 || i as usize > d.lines.len() + 1 {
                return Err(mlua::Error::runtime(format!("Requested out-of-range line from subtitle file: {i}")));
            }
            if val.is_nil() {
                if (i as usize) <= d.lines.len() {
                    d.lines.remove(i as usize - 1);
                }
                return Ok(());
            }
            let line = line_from_value(val)?;
            if i as usize == d.lines.len() + 1 {
                d.lines.push(line);
            } else {
                d.lines[i as usize - 1] = line;
            }
            Ok(())
        });
    }
}

pub fn make_subs(lua: &Lua, doc: Rc<RefCell<Doc>>) -> Result<AnyUserData> {
    lua.create_userdata(Subs(doc))
}

#[allow(dead_code)]
pub fn unused(_: MultiValue) {}
