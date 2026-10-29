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
    assert_eq!(log, "fps:24,1000 size:1920x1080 aud:1500-2500 pos:42");
}
