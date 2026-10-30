use kitsune_core::{ass, Document};
use kitsune_lua::Host;
use std::path::Path;

fn sample() -> Document {
    ass::parse(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/ornek.ass")).unwrap())
}

#[test]
fn macro_modifies_selection_and_appends() {
    let h = Host::new(sample()).unwrap();
    let n = h.load_script(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/ornek.lua"))).unwrap();
    assert_eq!(n, 1);
    let sel = h.run(0, &[0], Some(0)).unwrap();
    assert_eq!(sel, vec![0]);
    let d = h.document();
    assert_eq!(d.events.len(), 3);
    assert_eq!(d.events[0].text, "{\\i1}Merhaba dünya{\\i0}");
    assert_eq!(d.events[0].end, 3200);
    assert_eq!(h.is_valid(0, &[]).unwrap(), false);
}

#[test]
fn cancel_and_error_roll_back() {
    let dir = std::env::temp_dir().join("kitsune_test_cancel.lua");
    std::fs::write(&dir, "aegisub.register_macro('c','',function(s) s[1]={class='info',key='X',value='1'} s.delete(2) aegisub.cancel() end)").unwrap();
    let h = Host::new(sample()).unwrap();
    h.load_script(&dir).unwrap();
    let before = h.document();
    assert!(matches!(h.run(0, &[], None), Err(kitsune_lua::RunError::Cancelled)));
    assert_eq!(h.document(), before);
}

#[test]
fn utils_color_helpers() {
    let h = Host::new(Document::new()).unwrap();
    let f = std::env::temp_dir().join("kitsune_test_util.lua");
    std::fs::write(&f, "aegisub.register_macro('u','',function(s) local r,g,b=util.extract_color('&H0000FF00&') assert(r==0 and g==255 and b==0) assert(util.ass_color(1,2,3)=='&H030201&') assert(unicode.len('şğü')==3) assert(('  x '):trim()=='x') end)").unwrap();
    h.load_script(&f).unwrap();
    h.run(0, &[], None).unwrap();
}
