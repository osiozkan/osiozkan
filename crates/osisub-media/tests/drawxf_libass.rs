//! core::drawxf modelini libass çıktısıyla karşılaştırır (ffmpeg yoksa atlanır).
use osisub_core::ass::{Dialogue, Script};
use osisub_core::drawxf::DrawXf;

const HEAD: &str = "[Script Info]\nPlayResX: 1280\nPlayResY: 720\nScaledBorderAndShadow: yes\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,DejaVu Sans,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,0,0,2,10,10,10,1\n";

fn render_bbox(tags: &str, dr: &str) -> (f64, f64, f64, f64) {
    let dir = std::env::temp_dir().join(format!("osisub-dxf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let a = dir.join("d.ass");
    std::fs::write(&a, format!("{HEAD}\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:00.00,0:00:05.00,Default,,0,0,0,,{{{}\\p1}}{}\n", tags, dr)).unwrap();
    let out = std::process::Command::new(osisub_media::ffmpeg_bin())
        .args(["-v", "error", "-f", "lavfi", "-i", "color=black:s=1280x720", "-vf"])
        .arg(format!("ass={},format=gray", a.display()))
        .args(["-frames:v", "1", "-f", "rawvideo", "-"])
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let px = out.stdout;
    assert_eq!(px.len(), 1280 * 720);
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for y in 0..720 {
        for x in 0..1280 {
            if px[y * 1280 + x] > 128 {
                x0 = x0.min(x as f64);
                y0 = y0.min(y as f64);
                x1 = x1.max(x as f64 + 1.0);
                y1 = y1.max(y as f64 + 1.0);
            }
        }
    }
    (x0, y0, x1, y1)
}

fn predicted(tags: &str, dr: &str) -> (f64, f64, f64, f64) {
    let s = Script::parse(HEAD);
    let d = Dialogue { text: format!("{{{}\\p1}}{}", tags, dr), ..Default::default() };
    let pos = osisub_core::overrides::get_initial(&d.text, "pos").map(|t| t.args()).map(|a| (a[0].trim().parse().unwrap(), a[1].trim().parse().unwrap())).unwrap();
    let t = DrawXf::from_line(&s, &d, pos, dr, osisub_core::perspective::dist_for(720.0, 720.0)).unwrap();
    // kenarları örnekle
    let pts = osisub_core::drawing::polylines(&osisub_core::drawing::parse(dr)).concat();
    let mut b = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for i in 0..pts.len() {
        let (a, c) = (pts[i], pts[(i + 1) % pts.len()]);
        for k in 0..=20 {
            let f = k as f64 / 20.0;
            let q = t.fwd(a.0 + (c.0 - a.0) * f, a.1 + (c.1 - a.1) * f);
            b = (b.0.min(q.0), b.1.min(q.1), b.2.max(q.0), b.3.max(q.1));
        }
    }
    b
}

#[test]
fn drawing_transform_matches_libass() {
    if !osisub_media::ffmpeg_available() {
        eprintln!("ffmpeg yok: atlandı");
        return;
    }
    let dr = "m 50 50 l 250 50 250 130 50 130";
    let mut bad = vec![];
    for tags in [
        "\\an5\\pos(640,360)",
        "\\an5\\pos(640,360)\\frz30",
        "\\an3\\pos(640,360)\\frz-20\\org(500,300)",
        "\\an5\\pos(640,360)\\fax0.4",
        "\\an2\\pos(640,360)\\fay0.2\\fscx150",
        "\\an5\\pos(640,360)\\frx30\\fry-25",
        "\\an7\\pos(400,200)\\frx20\\fry20\\frz10\\org(640,360)\\fscy70",
    ] {
        let r = render_bbox(tags, dr);
        let p = predicted(tags, dr);
        let err = [(r.0 - p.0).abs(), (r.1 - p.1).abs(), (r.2 - p.2).abs(), (r.3 - p.3).abs()].into_iter().fold(0.0, f64::max);
        if err > 2.0 {
            bad.push(format!("{tags}: libass {r:?} model {p:?}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
