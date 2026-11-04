use super::*;
use std::io::Write;

fn script(src: &str) -> (tempdir::Dir, PathBuf) {
    let d = tempdir::Dir::new();
    let p = d.path.join("test.lua");
    std::fs::File::create(&p).unwrap().write_all(src.as_bytes()).unwrap();
    (d, p)
}

mod tempdir {
    use std::path::PathBuf;
    pub struct Dir {
        pub path: PathBuf,
    }
    impl Dir {
        pub fn new() -> Dir {
            use std::sync::atomic::{AtomicU32, Ordering};
            static N: AtomicU32 = AtomicU32::new(0);
            let p = std::env::temp_dir().join(format!("yozora-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
            std::fs::create_dir_all(&p).unwrap();
            Dir { path: p }
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

fn sample() -> Document {
    let mut d = Document::new_default();
    for i in 0..3 {
        d.events.push(Event { start: i * 1000, end: i * 1000 + 900, text: format!("satir {}", i + 1), ..Event::default() });
    }
    d
}

#[test]
fn macro_edits_text_and_selection() {
    let (_d, p) = script(
        r#"
script_name = "Test"
script_author = "me"
function proc(subs, sel, active)
  for _, i in ipairs(sel) do
    local l = subs[i]
    l.text = l.text:upper() .. "!"
    subs[i] = l
  end
  aegisub.debug.out("n=%d", #sel)
  return {sel[1]}, sel[1]
end
aegisub.register_macro("Büyüt", "açıklama", proc)
"#,
    );
    let s = Script::load(&p, &HostOptions::default()).unwrap();
    assert_eq!(s.meta.name, "Test");
    assert_eq!(s.macros.len(), 1);
    assert_eq!(s.macros[0].name, "Büyüt");
    let mut d = sample();
    let out = s.run(1, &mut d, &[1, 2], 1).unwrap();
    assert_eq!(d.events[0].text, "satir 1");
    assert_eq!(d.events[1].text, "SATIR 2!");
    assert_eq!(d.events[2].text, "SATIR 3!");
    assert_eq!(out.selection, Some(vec![1]));
    assert_eq!(out.active, Some(1));
    assert_eq!(out.log[0].message, "n=2");
}

#[test]
fn insert_delete_and_len() {
    let (_d, p) = script(
        r#"
aegisub.register_macro("m", "", function(subs, sel)
  local n = #subs
  local first_dialogue
  for i = 1, #subs do if subs[i].class == "dialogue" then first_dialogue = i break end end
  local l = subs[first_dialogue]
  l.text = "eklenen"
  subs.insert(first_dialogue, l)
  subs.delete(#subs)
  subs.append({class="dialogue", comment=false, layer=0, start_time=9000, end_time=9500, style="Default", actor="", margin_l=0, margin_r=0, margin_t=0, effect="", text="sonda"})
  assert(subs.n == #subs)
end)
"#,
    );
    let s = Script::load(&p, &HostOptions::default()).unwrap();
    let mut d = sample();
    s.run(1, &mut d, &[], 0).unwrap();
    let texts: Vec<_> = d.events.iter().map(|e| e.text.as_str()).collect();
    assert_eq!(texts, vec!["eklenen", "satir 1", "satir 2", "sonda"]);
}

#[test]
fn error_and_cancel_leave_doc_untouched() {
    let (_d, p) = script(
        r#"
aegisub.register_macro("boom", "", function(subs) local l = subs[#subs]; l.text="x"; subs[#subs]=l; error("patladı") end)
aegisub.register_macro("cancel", "", function(subs) aegisub.cancel() end)
"#,
    );
    let s = Script::load(&p, &HostOptions::default()).unwrap();
    let mut d = sample();
    let before = d.clone();
    let e = s.run(1, &mut d, &[], 0).unwrap_err();
    assert!(e.0.contains("patladı"), "{}", e.0);
    assert_eq!(d, before);
    let o = s.run(2, &mut d, &[], 0).unwrap();
    assert!(o.cancelled);
    assert_eq!(d, before);
}

#[test]
fn utils_and_karaskel() {
    let (_d, p) = script(
        r#"
include("utils.lua")
include("karaskel.lua")
aegisub.register_macro("k", "", function(subs, sel)
  local r, g, b, a = extract_color("&H00FF8040")
  assert(r == 0x40 and g == 0x80 and b == 0xFF and a == 0, "extract_color")
  assert(ass_color(1, 2, 3) == "&H030201&")
  assert(interpolate_color(0.5, "&H000000&", "&HFFFFFF&") == "&H808080&", interpolate_color(0.5, "&H000000&", "&HFFFFFF&"))
  assert(string.headtail("a b c") == "a")
  local meta, styles = karaskel.collect_head(subs)
  assert(meta.res_x == 1920 and styles.n == 1 and styles["Default"])
  for _, i in ipairs(sel) do
    local l = subs[i]
    l.text = "{\\k10}ka{\\k20}ra{\\k30}oke"
    karaskel.preproc_line(subs, meta, styles, l)
    assert(l.kara.n == 3, "kara.n=" .. l.kara.n)
    assert(l.kara[2].duration == 20 and l.kara[2].text_stripped == "ra")
    assert(l.kara[3].start_time == 300 and l.kara[3].end_time == 600)
    assert(l.kara[2].left > l.kara[1].left and l.width > 0 and l.halign == "center")
    assert(math.abs(l.kara[3].right - l.right) < 1e-6)
  end
end)
"#,
    );
    let s = Script::load(&p, &HostOptions::default()).unwrap();
    let mut d = sample();
    let o = s.run(1, &mut d, &[0], 0);
    assert!(o.is_ok(), "{:?}", o.err());
}

#[test]
fn regex_unicode_dialog() {
    let (_d, p) = script(
        r#"
include("unicode.lua")
aegisub.register_macro("m", "", function(subs, sel)
  assert(re.sub("a1b22", "\\d+", "X") == "aXbX")
  assert(re.sub("a1b22", "(\\d+)", "<\\1>") == "a<1>b<22>")
  local m = re.match("12:34", "(\\d+):(\\d+)")
  assert(m[2].str == "12" and m[3].str == "34")
  assert(#re.find("a b c", "\\w") == 3)
  assert(unicode.len("çğş") == 3)
  assert(unicode.to_upper_case("abc") == "ABC")
  local btn, res = aegisub.dialog.display({
    {class="intedit", name="shift", value=5, x=0, y=0},
    {class="checkbox", name="flag", value=false, x=0, y=1},
    {class="edit", name="txt", value="d", x=0, y=2},
  }, {"Uygula", "İptal"})
  assert(btn == "Uygula")
  local l = subs[#subs]
  l.text = string.format("%d|%s|%s", res.shift, tostring(res.flag), res.txt)
  subs[#subs] = l
end)
"#,
    );
    let mut opts = HostOptions::default();
    opts.params.insert("shift".into(), "42".into());
    opts.params.insert("flag".into(), "evet".into());
    let s = Script::load(&p, &opts).unwrap();
    let mut d = sample();
    s.run(1, &mut d, &[], 0).unwrap();
    assert_eq!(d.events.last().unwrap().text, "42|true|d");
}

#[test]
fn scripts_are_isolated() {
    let (_d1, p1) = script("x = 1\naegisub.register_macro('a','',function() end)");
    let (_d2, p2) = script("assert(x == nil)\naegisub.register_macro('b','',function() end)");
    Script::load(&p1, &HostOptions::default()).unwrap();
    Script::load(&p2, &HostOptions::default()).unwrap();
}
