//! Runs the (non-Send) Lua runtime on its own thread so macros can block on GUI dialogs.
use osiosub_core::ass::AssFile;
use osiosub_lua::{DialogResult, DialogSpec, Host, MacroInfo, MacroResult, ScriptRuntime};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

pub type VideoMeta = Arc<Mutex<Option<(i32, i32, f64)>>>;

pub enum ToWorker {
    Load(PathBuf),
    Run { idx: usize, file: AssFile, sel: Vec<usize>, active: usize },
    Quit,
}

pub enum FromWorker {
    Loaded(Result<usize, String>, Vec<MacroInfo>),
    Dialog(DialogSpec),
    Log(String),
    Done(MacroResult),
}

struct GuiHost {
    tx: Sender<FromWorker>,
    replies: Receiver<DialogResult>,
    ctx: egui::Context,
    video: VideoMeta,
}

impl Host for GuiHost {
    fn dialog(&mut self, spec: &DialogSpec) -> DialogResult {
        let _ = self.tx.send(FromWorker::Dialog(spec.clone()));
        self.ctx.request_repaint();
        self.replies.recv().unwrap_or(DialogResult { button: None, values: Default::default() })
    }
    fn video_size(&self) -> Option<(i32, i32)> { self.video.lock().unwrap().map(|v| (v.0, v.1)) }
    fn fps(&self) -> Option<f64> { self.video.lock().unwrap().map(|v| v.2) }
    fn log(&mut self, msg: &str) {
        let _ = self.tx.send(FromWorker::Log(msg.to_string()));
        self.ctx.request_repaint();
    }
}

pub struct Worker {
    pub tx: Sender<ToWorker>,
    pub rx: Receiver<FromWorker>,
    pub reply: Sender<DialogResult>,
    pub video: VideoMeta,
}

pub fn spawn(ctx: egui::Context) -> Worker {
    let (to_tx, to_rx) = channel::<ToWorker>();
    let (from_tx, from_rx) = channel::<FromWorker>();
    let (reply_tx, reply_rx) = channel::<DialogResult>();
    let video: VideoMeta = Arc::new(Mutex::new(None));
    let video2 = video.clone();
    std::thread::spawn(move || {
        let host = GuiHost { tx: from_tx.clone(), replies: reply_rx, ctx: ctx.clone(), video: video2 };
        let mut rt = match ScriptRuntime::new(Box::new(host)) { Ok(r) => r, Err(_) => return };
        while let Ok(msg) = to_rx.recv() {
            match msg {
                ToWorker::Load(p) => {
                    let r = rt.load_file(&p);
                    let all = rt.macros();
                    let _ = from_tx.send(FromWorker::Loaded(r.map(|v| v.len()), all));
                }
                ToWorker::Run { idx, file, sel, active } => {
                    let res = rt.run(idx, &file, &sel, active);
                    let _ = from_tx.send(FromWorker::Done(res));
                }
                ToWorker::Quit => break,
            }
            ctx.request_repaint();
        }
    });
    Worker { tx: to_tx, rx: from_rx, reply: reply_tx, video }
}
