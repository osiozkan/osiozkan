//! SubInspector.Inspector karşılığı: gerçek libass (ffmpeg) çizimiyle uçtan uca.
use osisub_core::ass::Script;
use osisub_lua::{Engine, HeadlessHost};

const ASS: &str = "[Script Info]\nPlayResX: 640\nPlayResY: 360\nScaledBorderAndShadow: yes\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Kutu,DejaVu Sans,30,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,0,0,7,0,0,0,1\nStyle: Metin,DejaVu Sans,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,3,0,5,0,0,0,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:02.00,Kutu,,0,0,0,,{\\pos(100,50)\\p1}m 0 0 l 40 0 40 30 0 30\nDialogue: 0,0:00:00.00,0:00:01.00,Kutu,,0,0,0,,{\\move(0,0,200,0)\\p1}m 0 0 l 10 0 10 10 0 10\nDialogue: 0,0:00:00.00,0:00:05.00,Metin,,0,0,0,,Merhaba\n";

#[test]
fn get_bounds_like_assfoundation() {
    if !osisub_media::ffmpeg_available() {
        eprintln!("ffmpeg yok: atlandı");
        return;
    }
    let renderer: osisub_lua::BoundsRenderer = std::sync::Arc::new(|a: &str, w: u32, h: u32, t: &[i64]| osisub_media::render_coverage(a, w, h, t, &std::env::temp_dir().join("osisub-inspector-test")));
    let mut e = Engine::new(Box::new(HeadlessHost { quiet: true, renderer: Some(renderer), ..Default::default() })).unwrap();
    e.set_script(Script::parse(ASS));
    let f = std::env::temp_dir().join(format!("osisub_si_{}.lua", std::process::id()));
    std::fs::write(
        &f,
        r#"
local SubInspector = require("SubInspector.Inspector")
aegisub.register_macro("si", "", function(subs, sel)
  local si = assert(SubInspector(subs))
  local lines = {}
  for i = 1, #subs do if subs[i].class == "dialogue" then lines[#lines + 1] = subs[i] end end
  -- tek satır: ilk kare
  local b, t = si:getBounds({ lines[1] })
  assert(#b == 1 and t[1] == 1000, "anlar")
  aegisub.log("kutu %d %d %d %d %s %d\n", b[1].x, b[1].y, b[1].w, b[1].h, tostring(b[1].solid), #b[1].hash)
  -- her kare: \move ile ilerleyen kare (24 kare)
  lines[2].si_exhaustive = true
  local b2, t2 = si:getBounds({ lines[2] })
  aegisub.log("hareket %d kare ilk x %d son x %d ilk an %d\n", #b2, b2[1].x, b2[#b2].x, t2[1])
  -- aynı görünüm → aynı özet (konumdan bağımsız değil: kutu taşınınca da aynı)
  local l3 = lines[1]; l3.text = "{\\pos(300,200)\\p1}m 0 0 l 40 0 40 30 0 30"
  local b3 = si:getBounds({ l3 })
  aegisub.log("ozet %s\n", tostring(b3[1].hash == b[1].hash))
  -- metin (kenarlıklı, ortalanmış) satırı
  local b4 = si:getBounds({ lines[3] })
  aegisub.log("metin %d %d %d %d\n", b4[1].x, b4[1].y, b4[1].w, b4[1].h)
  -- görünmeyen satır → false
  local l5 = lines[1]; l5.text = "{\\alpha&HFF&\\p1}m 0 0 l 10 0 10 10"
  aegisub.log("bos %s\n", tostring(si:getBounds({ l5 })[1]))
  local ok, err = si:getBounds({})
  aegisub.log("hata %s\n", tostring(ok == nil and err ~= nil))
end)
"#,
    )
    .unwrap();
    e.load_script(&f).unwrap();
    e.run_macro(0, &[], 0).unwrap();
    let log = e.take_log();
    let _ = std::fs::remove_file(&f);
    eprintln!("{log}");
    let nums = |key: &str| -> Vec<i64> { log.lines().find(|l| l.starts_with(key)).unwrap().split_whitespace().filter_map(|w| w.parse().ok()).collect() };
    let k = nums("kutu ");
    assert!((k[0] - 100).abs() <= 1 && (k[1] - 50).abs() <= 1 && (k[2] - 40).abs() <= 2 && (k[3] - 30).abs() <= 2, "{log}");
    assert!(log.contains(" 16\n"), "özet 16 hane: {log}");
    let h = nums("hareket ");
    assert_eq!(h[0], 24, "{log}");
    // son kare 959 ms → x ≈ 191.8
    assert!(h[1] <= 1 && (h[2] - 191).abs() <= 2 && h[3] == 0, "{log}");
    assert!(log.contains("ozet true"), "{log}");
    let m = nums("metin ");
    // ortalanmış (\an5) metin ekran ortasında
    assert!(((m[0] + m[2] / 2) - 320).abs() <= 3 && ((m[1] + m[3] / 2) - 180).abs() <= 6 && m[2] > 120, "{log}");
    assert!(log.contains("bos false"), "{log}");
    assert!(log.contains("hata true"), "{log}");
}
