//! The `subs` object handed to macros: behaves like Aegisub's subtitle file userdata.

use crate::convert::{entry_to_table, table_to_entry};
use kaku_core::ass::{Entry, Script};
use mlua::{MetaMethod, MultiValue, Result, UserData, UserDataMethods, Value};
use std::cell::RefCell;
use std::rc::Rc;

pub type Shared = Rc<RefCell<Script>>;

pub struct Subs(pub Shared);

fn idx_err(i: i64, n: usize) -> mlua::Error {
    mlua::Error::runtime(format!("Requested out-of-range line from subtitle file: {} (have {})", i, n))
}

fn collect_entries(args: &MultiValue) -> Result<Vec<Entry>> {
    args.iter().map(table_to_entry).collect()
}

impl UserData for Subs {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Len, |_, this, ()| Ok(this.0.borrow().entries.len()));
        m.add_meta_method(MetaMethod::Index, |lua, this, key: Value| {
            match key {
                Value::Integer(_) | Value::Number(_) => {
                    let i = match key { Value::Integer(i) => i, Value::Number(f) => f as i64, _ => 0 };
                    let sc = this.0.borrow();
                    if i < 1 || i as usize > sc.entries.len() {
                        return Err(idx_err(i, sc.entries.len()));
                    }
                    Ok(Value::Table(entry_to_table(lua, &sc.entries[i as usize - 1])?))
                }
                Value::String(s) => {
                    let name = s.to_str()?.to_string();
                    let sh = this.0.clone();
                    let f = match name.as_str() {
                        "n" => return Ok(Value::Integer(this.0.borrow().entries.len() as i64)),
                        "insert" => lua.create_function(move |_, args: MultiValue| {
                            let mut it = args.into_iter();
                            let at = match it.next() { Some(Value::Integer(i)) => i, Some(Value::Number(f)) => f as i64, _ => return Err(mlua::Error::runtime("insert: index expected")) };
                            let rest: MultiValue = it.collect();
                            let new = collect_entries(&rest)?;
                            let mut sc = sh.borrow_mut();
                            let pos = ((at - 1).max(0) as usize).min(sc.entries.len());
                            for (k, e) in new.into_iter().enumerate() {
                                sc.entries.insert(pos + k, e);
                            }
                            Ok(())
                        })?,
                        "append" => lua.create_function(move |_, args: MultiValue| {
                            let new = collect_entries(&args)?;
                            sh.borrow_mut().entries.extend(new);
                            Ok(())
                        })?,
                        "delete" => lua.create_function(move |_, args: MultiValue| {
                            let mut idxs: Vec<usize> = Vec::new();
                            for a in args.iter() {
                                match a {
                                    Value::Integer(i) => idxs.push(*i as usize),
                                    Value::Number(f) => idxs.push(*f as usize),
                                    Value::Table(t) => {
                                        for v in t.sequence_values::<i64>() {
                                            idxs.push(v? as usize);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            idxs.sort_unstable();
                            idxs.dedup();
                            let mut sc = sh.borrow_mut();
                            for &i in idxs.iter().rev() {
                                if i >= 1 && i <= sc.entries.len() {
                                    sc.entries.remove(i - 1);
                                }
                            }
                            Ok(())
                        })?,
                        "deleterange" => lua.create_function(move |_, (a, b): (i64, i64)| {
                            let mut sc = sh.borrow_mut();
                            let (a, b) = (a.max(1) as usize, (b.max(0) as usize).min(sc.entries.len()));
                            if a <= b {
                                sc.entries.drain(a - 1..b);
                            }
                            Ok(())
                        })?,
                        "script_resolution" => lua.create_function(move |_, ()| {
                            let (x, y) = sh.borrow().play_res();
                            Ok((x, y))
                        })?,
                        _ => return Ok(Value::Nil),
                    };
                    Ok(Value::Function(f))
                }
                _ => Ok(Value::Nil),
            }
        });
        m.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (Value, Value)| {
            let i = match key { Value::Integer(i) => i, Value::Number(f) => f as i64, _ => return Err(mlua::Error::runtime("subs index must be a number")) };
            let mut sc = this.0.borrow_mut();
            let len = sc.entries.len();
            if i == 0 {
                // Append at end.
                sc.entries.push(table_to_entry(&val)?);
            } else if i < 0 {
                let pos = (-i - 1) as usize;
                if pos > len {
                    return Err(idx_err(i, len));
                }
                sc.entries.insert(pos, table_to_entry(&val)?);
            } else {
                if i as usize > len {
                    return Err(idx_err(i, len));
                }
                match val {
                    Value::Nil => {
                        sc.entries.remove(i as usize - 1);
                    }
                    v => sc.entries[i as usize - 1] = table_to_entry(&v)?,
                }
            }
            Ok(())
        });
    }
}
