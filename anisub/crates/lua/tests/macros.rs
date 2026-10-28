use anisub_core::ass::{AssFile, Line};
use anisub_lua::Runtime;

const FILE: &str = "[Script Info]\nPlayResX: 1280\nPlayResY: 720\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,20,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,{\\k50}ka{\\k50}ra\nDialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,second\n";

fn rt() -> Runtime {
    Runtime::new(AssFile::parse(FILE)).unwrap()
}

#[test]
fn macro_edits_lines() {
    let rt = rt();
    rt.load_source(
        r#"
        script_name = "t"
        include("utils.lua")
        aegisub.register_macro("shift", "d", function(subs, sel)
            for _, i in ipairs(sel) do
                local l = subs[i]
                l.start_time = l.start_time + 500
                l.text = l.text:upper()
                subs[i] = l
            end
            local n = #subs
            subs:insert(n + 1, {class="dialogue", start_time=0, end_time=100, text="new", style="Default"})
            aegisub.log("done %d", n)
            return sel
        end)
        "#,
        "t.lua",
    )
    .unwrap();
    assert_eq!(rt.macros().unwrap().len(), 1);
    let first_ev = rt.subs.borrow().lines.iter().position(|l| matches!(l, Line::Dialogue(_))).unwrap() as i64 + 1;
    let r = rt.run("shift", vec![first_ev], first_ev).unwrap();
    assert_eq!(r.selection, Some(vec![first_ev]));
    let f = rt.subs.borrow();
    let ev: Vec<_> = f.events().collect();
    assert_eq!(ev.len(), 3);
    assert_eq!(ev[0].start, 1500);
    assert!(ev[0].text.contains("KA"));
    assert_eq!(ev[2].text, "new");
    drop(f);
    assert!(rt.take_log().unwrap().starts_with("done "));
}

#[test]
fn karaskel_preproc() {
    let rt = rt();
    rt.load_source(
        r#"
        include("karaskel.lua")
        aegisub.register_macro("k", "", function(subs, sel)
            local meta, styles = karaskel.collect_head(subs)
            assert(meta.res_x == 1280 and styles.Default.height > 0)
            for i = 1, #subs do
                local l = subs[i]
                if l.class == "dialogue" then
                    karaskel.preproc_line(subs, meta, styles, l)
                    aegisub.log("%d|%d|%s|%s", l.kara.n, l.kara[2] and l.kara[2].start_time or -1, l.text_stripped, l.halign)
                    break
                end
            end
        end)
        "#,
        "k.lua",
    )
    .unwrap();
    rt.run("k", vec![], 1).unwrap();
    assert_eq!(rt.take_log().unwrap(), "2|500|kara|center");
}

#[test]
fn errors_and_cancel() {
    let rt = rt();
    rt.load_source(
        r#"aegisub.register_macro("c", "", function() aegisub.cancel() end)
           aegisub.register_macro("e", "", function() error("boom") end)"#,
        "x.lua",
    )
    .unwrap();
    assert!(rt.run("c", vec![], 1).unwrap().cancelled);
    assert!(rt.run("e", vec![], 1).is_err());
    assert!(rt.run("nope", vec![], 1).is_err());
}

#[test]
fn text_extents_uses_real_fonts() {
    let rt = rt();
    rt.load_source(
        r#"aegisub.register_macro("w", "", function()
            local st = { fontname = "DejaVu Sans", fontsize = 40, scale_x = 100, scale_y = 100, spacing = 0 }
            local w1, h = aegisub.text_extents(st, "iiii")
            local w2 = aegisub.text_extents(st, "WWWW")
            st.spacing = 2
            local w3 = aegisub.text_extents(st, "iiii")
            aegisub.log("%s|%s|%s", w2 > w1 * 2, h, math.abs(w3 - w1 - 8) < 1e-6)
        end)"#,
        "w.lua",
    )
    .unwrap();
    rt.run("w", vec![], 1).unwrap();
    assert_eq!(rt.take_log().unwrap(), "true|40|true");
}

#[test]
fn luajit_compat() {
    let rt = rt();
    rt.load_source(
        r#"aegisub.register_macro("j", "", function()
            aegisub.log("%s|%d|%s", tostring(jit ~= nil), bit.band(0xF0, 0x3C), type(require("ffi")))
        end)"#,
        "j.lua",
    )
    .unwrap();
    rt.run("j", vec![], 1).unwrap();
    assert_eq!(rt.take_log().unwrap(), "true|48|table");
}
