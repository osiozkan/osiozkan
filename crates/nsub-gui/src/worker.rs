//! Runs Lua automation on its own thread so long macros keep the UI responsive and
//! `aegisub.dialog.display` can block until the user answers.

use nsub_core::Script;
use nsub_lua::dialog::{DialogRequest, DialogResponse, Progress};
use nsub_lua::{Automation, MacroInfo, Options, RunResult};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

pub enum Cmd {
    Load { scripts: Vec<PathBuf>, doc_path: Option<PathBuf>, fps: f64 },
    Run { id: usize, script: Box<Script>, sel: Vec<usize>, active: usize },
    DialogAnswer(DialogResponse),
}

pub enum Msg {
    Loaded { macros: Vec<MacroInfo>, errors: Vec<String> },
    Dialog(DialogRequest),
    Progress(Progress),
    Finished(Result<(Box<Script>, RunResult), String>),
}

pub type Waker = Arc<dyn Fn() + Send + Sync>;

pub struct Worker {
    tx: Sender<Cmd>,
    pub rx: Receiver<Msg>,
    pub cancel: Arc<AtomicBool>,
}

impl Worker {
    pub fn spawn(waker: Option<Waker>) -> Worker {
        let (tx, cmd_rx) = channel::<Cmd>();
        let (msg_tx, rx) = channel::<Msg>();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel2 = cancel.clone();
        std::thread::Builder::new()
            .name("lua-worker".into())
            .spawn(move || run_loop(cmd_rx, msg_tx, cancel2, waker))
            .expect("spawn worker");
        Worker { tx, rx, cancel }
    }

    pub fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }

    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn run_loop(cmd_rx: Receiver<Cmd>, msg_tx: Sender<Msg>, cancel: Arc<AtomicBool>, waker: Option<Waker>) {
    let cmd_rx = Rc::new(cmd_rx);
    let wake = {
        let w = waker.clone();
        move || {
            if let Some(w) = &w {
                w()
            }
        }
    };
    let mut auto: Option<Automation> = None;
    let mut macros: Vec<MacroInfo> = vec![];
    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            Cmd::Load { scripts, doc_path, fps } => {
                let dialog_tx = msg_tx.clone();
                let dialog_rx = cmd_rx.clone();
                let dialog_wake = wake.clone();
                let progress_tx = msg_tx.clone();
                let progress_wake = wake.clone();
                let opts = Options {
                    fps,
                    echo_log: false,
                    script_path: doc_path.clone(),
                    cancel_flag: cancel.clone(),
                    dialog_handler: Some(Rc::new(move |req| {
                        let _ = dialog_tx.send(Msg::Dialog(req));
                        dialog_wake();
                        loop {
                            match dialog_rx.recv() {
                                Ok(Cmd::DialogAnswer(r)) => return r,
                                Ok(_) => continue,
                                Err(_) => return DialogResponse::default(),
                            }
                        }
                    })),
                    progress_handler: Some(Rc::new(move |p| {
                        let _ = progress_tx.send(Msg::Progress(p));
                        progress_wake();
                    })),
                    ..Default::default()
                };
                macros.clear();
                let mut errors = vec![];
                match Automation::new(opts) {
                    Ok(mut a) => {
                        let mut fm = nsub_fonts::FontMetrics::system();
                        if let Some(dir) = doc_path.as_ref().and_then(|p| p.parent()) {
                            fm.add_dir(dir);
                        }
                        a.set_metrics(Box::new(fm));
                        for s in &scripts {
                            match a.load_script(s) {
                                Ok(m) => macros.extend(m),
                                Err(e) => errors.push(e),
                            }
                        }
                        auto = Some(a);
                    }
                    Err(e) => errors.push(format!("Lua init failed: {e}")),
                }
                let _ = msg_tx.send(Msg::Loaded { macros: macros.clone(), errors });
                wake();
            }
            Cmd::Run { id, mut script, sel, active } => {
                cancel.store(false, Ordering::Relaxed);
                let res = match auto.as_ref() {
                    None => Err("automation not initialised".to_string()),
                    Some(a) => match a.validate(id, &script, &sel, active) {
                        Ok(false) => Err("not applicable to the current selection".to_string()),
                        Err(e) => Err(e),
                        Ok(true) => a.run(id, &mut script, &sel, active).map(|r| (script, r)),
                    },
                };
                let _ = msg_tx.send(Msg::Finished(res));
                wake();
            }
            Cmd::DialogAnswer(_) => {}
        }
    }
}
