//! Lua motorunu ayrı bir iş parçacığında çalıştırır. Motor `!Send` olduğundan iş parçacığı içinde
//! yaratılır; iletişim kutuları arayüze iletilip yanıt beklenir.
use osisub_core::ass::Script;
use osisub_lua::{DialogResult, DialogSpec, Engine, Host, MacroInfo, ScriptInfo};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// Arayüzün güncellediği, betiklerin okuduğu ortam bilgisi.
#[derive(Clone, Debug, Default)]
pub struct MediaState {
    pub fps: Option<f64>,
    pub video_size: Option<(i32, i32)>,
    pub audio_selection: Option<(i64, i64)>,
    /// Arayüz iptal isterse `true`; betikler `aegisub.progress.is_cancelled()` ile görür.
    pub cancel: bool,
}

pub enum Cmd {
    Load(PathBuf),
    Run { id: usize, script: Script, selection: Vec<usize>, active: usize, props: Vec<(String, String)> },
    Validate { id: usize, script: Script, selection: Vec<usize>, active: usize },
    Shutdown,
}

#[derive(Debug)]
pub struct RunDone {
    pub id: usize,
    pub script: Script,
    /// 0 tabanlı entry indeksleri.
    pub selection: Option<Vec<usize>>,
    pub active: Option<usize>,
    pub undo_desc: Option<String>,
    pub cancelled: bool,
    pub log: String,
}

pub enum Event {
    Loaded(Result<(ScriptInfo, Vec<MacroInfo>), String>),
    Dialog(DialogSpec),
    Progress { fraction: f64, task: String, title: String },
    Done(Result<RunDone, (String, String)>), // hata: (mesaj, günlük)
    Valid { id: usize, ok: bool },
    Log(String),
}

struct ChannelHost {
    media: Arc<Mutex<MediaState>>,
    tx: Sender<Event>,
    resp: Receiver<DialogResult>,
    task: String,
    title: String,
}

impl Host for ChannelHost {
    fn dialog(&mut self, spec: &DialogSpec) -> DialogResult {
        if self.tx.send(Event::Dialog(spec.clone())).is_err() {
            return DialogResult::default();
        }
        self.resp.recv().unwrap_or_default()
    }
    fn fps(&self) -> Option<f64> {
        self.media.lock().ok().and_then(|m| m.fps)
    }
    fn video_size(&self) -> Option<(i32, i32)> {
        self.media.lock().ok().and_then(|m| m.video_size)
    }
    fn keyframes(&self) -> Vec<i64> {
        self.media.lock().map(|m| m.keyframes.clone()).unwrap_or_default()
    }
    fn audio_selection(&self) -> Option<(i64, i64)> {
        self.media.lock().ok().and_then(|m| m.audio_selection)
    }
    fn is_cancelled(&mut self) -> bool {
        self.media.lock().map(|m| m.cancel).unwrap_or(false)
    }
    fn progress(&mut self, fraction: f64) {
        let _ = self.tx.send(Event::Progress { fraction, task: self.task.clone(), title: self.title.clone() });
    }
    fn progress_task(&mut self, t: &str) {
        self.task = t.to_string();
    }
    fn progress_title(&mut self, t: &str) {
        self.title = t.to_string();
    }
    fn log(&mut self, _level: i64, msg: &str) {
        let _ = self.tx.send(Event::Log(msg.to_string()));
    }
}

pub struct Worker {
    cmd: Sender<Cmd>,
    pub events: Receiver<Event>,
    dialog_reply: Sender<DialogResult>,
    handle: Option<std::thread::JoinHandle<()>>,
    pub media: Arc<Mutex<MediaState>>,
}

impl Worker {
    pub fn spawn(include_dirs: Vec<PathBuf>, font_dirs: Vec<PathBuf>) -> Worker {
        let (cmd_tx, cmd_rx) = channel::<Cmd>();
        let (ev_tx, ev_rx) = channel::<Event>();
        let (dlg_tx, dlg_rx) = channel::<DialogResult>();
        let media: Arc<Mutex<MediaState>> = Arc::new(Mutex::new(MediaState::default()));
        let media_t = media.clone();
        let handle = std::thread::spawn(move || {
            let host = ChannelHost { media: media_t, tx: ev_tx.clone(), resp: dlg_rx, task: String::new(), title: String::new() };
            let mut engine = match Engine::new(Box::new(host)) {
                Ok(e) => e,
                Err(e) => {
                    let _ = ev_tx.send(Event::Done(Err((e.to_string(), String::new()))));
                    return;
                }
            };
            for d in include_dirs {
                engine.add_include_dir(d);
            }
            for d in font_dirs {
                engine.add_font_dir(d);
            }
            while let Ok(c) = cmd_rx.recv() {
                match c {
                    Cmd::Shutdown => break,
                    Cmd::Load(p) => {
                        let r = engine.load_script(&p).map(|info| {
                            let ms: Vec<MacroInfo> = engine.macros().into_iter().filter(|m| info.macros.contains(&m.id)).collect();
                            (info, ms)
                        });
                        let _ = ev_tx.send(Event::Loaded(r.map_err(|e| e.to_string())));
                    }
                    Cmd::Validate { id, script, selection, active } => {
                        engine.set_script(script);
                        let sel: Vec<usize> = selection.iter().map(|i| i + 1).collect();
                        let ok = engine.validate_macro(id, &sel, active + 1);
                        let _ = ev_tx.send(Event::Valid { id, ok });
                    }
                    Cmd::Run { id, script, selection, active, props } => {
                        engine.set_script(script);
                        for (k, v) in props {
                            engine.set_property(&k, &v);
                        }
                        let sel: Vec<usize> = selection.iter().map(|i| i + 1).collect();
                        let r = engine.run_macro(id, &sel, active + 1);
                        let log = engine.take_log();
                        let ev = match r {
                            Err(e) => Err((e.to_string(), log)),
                            Ok(o) => Ok(RunDone {
                                id,
                                script: engine.take_script(),
                                selection: o.selection.map(|s| s.into_iter().filter(|&i| i >= 1).map(|i| i - 1).collect()),
                                active: o.active.map(|a| a.saturating_sub(1)),
                                undo_desc: o.undo_desc,
                                cancelled: o.cancelled,
                                log,
                            }),
                        };
                        let _ = ev_tx.send(Event::Done(ev));
                    }
                }
            }
        });
        Worker { cmd: cmd_tx, events: ev_rx, dialog_reply: dlg_tx, handle: Some(handle), media }
    }

    pub fn send(&self, c: Cmd) {
        let _ = self.cmd.send(c);
    }

    pub fn reply_dialog(&self, r: DialogResult) {
        let _ = self.dialog_reply.send(r);
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.cmd.send(Cmd::Shutdown);
        // Bir iletişim kutusunda bekliyorsa serbest bırak.
        let _ = self.dialog_reply.send(DialogResult::default());
        // Takılmış bir betik arayüzü kilitlemesin: iş parçacığını beklemeden bırak.
        // Çalışan betik bitince (ya da süreç kapanınca) iş parçacığı kendiliğinden sonlanır.
        drop(self.handle.take());
    }
}
