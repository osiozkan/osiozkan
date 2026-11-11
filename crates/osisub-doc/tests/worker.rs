use osisub_core::ass::Script;
use osisub_doc::worker::{Cmd, Event, Worker};
use osisub_lua::DV;
use std::time::Duration;

fn recv(w: &Worker) -> Event {
    w.events.recv_timeout(Duration::from_secs(10)).expect("olay gelmedi")
}

#[test]
fn load_run_dialog() {
    let w = Worker::spawn(vec![], vec![]);
    w.send(Cmd::Load("../osisub-lua/tests/fixtures/sample.lua".into()));
    let Event::Loaded(Ok((info, macros))) = recv(&w) else { panic!("yüklenemedi") };
    assert_eq!(info.name, "Test makrosu");
    assert_eq!(macros.len(), 3);

    // Diyalog içeren makro: arayüz yanıtı beklenir.
    w.send(Cmd::Run { id: 2, script: Script::new_default(), selection: vec![], active: 0, props: vec![] });
    let Event::Dialog(spec) = recv(&w) else { panic!("diyalog bekleniyordu") };
    assert_eq!(spec.controls.len(), 2);
    let mut r = spec.defaults();
    assert_eq!(r.values["n"], DV::Num(7.0));
    r.button = Some("OK".into());
    w.reply_dialog(r);
    loop {
        match recv(&w) {
            Event::Done(Ok(d)) => {
                assert!(d.cancelled);
                break;
            }
            Event::Done(Err(e)) => panic!("{:?}", e),
            _ => {}
        }
    }
}

#[test]
fn media_state_visible_to_lua() {
    use osisub_core::ass::{Dialogue, Script};
    let w = Worker::spawn(vec![], vec![]);
    {
        let mut m = w.media.lock().unwrap();
        m.fps = Some(24.0);
        m.video_size = Some((1920, 1080));
        m.audio_selection = Some((1500, 2500));
    }
    w.send(Cmd::Load("../osisub-lua/tests/fixtures/media.lua".into()));
    let Event::Loaded(Ok(_)) = recv(&w) else { panic!("yüklenemedi") };
    let script = Script::from_dialogues(vec![Dialogue::default()]);
    w.send(Cmd::Run { id: 0, script, selection: vec![], active: 0, props: vec![("video_position".into(), "42".into())] });
    let mut log = String::new();
    loop {
        match recv(&w) {
            Event::Log(m) => log.push_str(&m),
            Event::Done(Ok(_)) => break,
            Event::Done(Err(e)) => panic!("{:?}", e),
            _ => {}
        }
    }
    // Aegisub gibi START anlambilimi: ms_from_frame(24) = 23 ile 24. karenin ortası (958..1000 → 979)
    assert_eq!(log, "fps:24,979 size:1920x1080 aud:1500-2500 pos:42");
}

#[test]
fn get_frame_from_real_video() {
    use osisub_core::ass::{Dialogue, Script};
    if !osisub_media::ffmpeg_available() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("osisub-wgf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let v = dir.join("red.mkv");
    let ok = std::process::Command::new(osisub_media::ffmpeg_bin()).args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=c=0xFF2010:s=64x36:r=24:d=1"]).arg(&v).status().map(|s| s.success()).unwrap_or(false);
    if !ok {
        return;
    }
    let info = osisub_media::probe(&v).unwrap();
    let lua = dir.join("gf.lua");
    std::fs::write(&lua, "aegisub.register_macro('gf', '', function() local f = aegisub.get_frame(3) aegisub.log('%dx%d %s', f:width(), f:height(), f:getPixelFormatted(10, 10)) end)").unwrap();
    let w = Worker::spawn(vec![], vec![]);
    {
        let mut m = w.media.lock().unwrap();
        m.fps = Some(24.0);
        m.video = Some((v.clone(), info));
    }
    w.send(Cmd::Load(lua));
    let Event::Loaded(Ok(_)) = recv(&w) else { panic!("yüklenemedi") };
    w.send(Cmd::Run { id: 0, script: Script::from_dialogues(vec![Dialogue::default()]), selection: vec![], active: 0, props: vec![] });
    let mut log = String::new();
    loop {
        match recv(&w) {
            Event::Log(m) => log.push_str(&m),
            Event::Done(Ok(_)) => break,
            Event::Done(Err(e)) => panic!("{:?}", e),
            _ => {}
        }
    }
    // renk (R=FF,G=20,B=10) → "&HBBGGRR&"; YUV dönüşümü ±birkaç birim kaydırabilir
    assert!(log.starts_with("64x36 &H"), "{log}");
    let hex = &log[8..14];
    let (b, g, r) = (u8::from_str_radix(&hex[0..2], 16).unwrap(), u8::from_str_radix(&hex[2..4], 16).unwrap(), u8::from_str_radix(&hex[4..6], 16).unwrap());
    assert!(r > 0xF0 && g < 0x30 && b < 0x28, "{log}");
    let _ = std::fs::remove_dir_all(&dir);
}
