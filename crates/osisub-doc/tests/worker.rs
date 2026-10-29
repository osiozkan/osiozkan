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
