//! Video önizleme + ses dalga formu paneli.
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, TextureHandle};
use osisub_core::ass::Entry;
use osisub_core::time;
use osisub_doc::Document;
use osisub_media::{Frame, MediaInfo, Waveform};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

enum Msg {
    Info(Result<MediaInfo, String>),
    Wave(Result<Waveform, String>),
    Frame { time: i64, rev: u64, res: Result<Frame, String> },
    Keyframes(Vec<i64>),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VideoTool {
    None,
    Pos,
    Clip,
    Rotate,
    Scale,
}

#[derive(Clone, Copy, PartialEq)]
enum Drag {
    None,
    Start,
    End,
    Body(i64),
}

pub struct Media {
    pub path: Option<PathBuf>,
    pub info: Option<MediaInfo>,
    wave: Option<Arc<Waveform>>,
    pub status: String,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    tex: Option<TextureHandle>,
    frame_busy: bool,
    frame_shown: (i64, u64),
    pub playhead: i64,
    view_start: f64,
    ms_per_px: f64,
    drag: Drag,
    last_active: usize,
    tmp_dir: PathBuf,
    player: Option<Child>,
    pub snap: bool,
    pub keyframes: Vec<i64>,
    pub snap_kf: bool,
    /// Videoda \pos sürükleme aracı.
    pub tool: VideoTool,
    vt_drag: Option<(u8, (f32, f32))>,
    pub spectrum: bool,
    spec_tex: Option<((i64, i64, usize, usize), TextureHandle)>,
    stream: Option<osisub_media::PlayStream>,
    audio: Option<osisub_media::audio::AudioPlayer>,
    stream_frames: std::collections::VecDeque<(usize, Vec<u8>)>,
    stream_count: usize,
    play_started: Option<std::time::Instant>,
    play_begin_at: Option<std::time::Instant>,
    play_from: i64,
    play_dur: i64,
}

impl Media {
    pub fn new() -> Media {
        let (tx, rx) = channel();
        let tmp_dir = std::env::temp_dir().join(format!("osisub_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp_dir);
        Media {
            path: None,
            info: None,
            wave: None,
            status: String::new(),
            tx,
            rx,
            tex: None,
            frame_busy: false,
            frame_shown: (-1, u64::MAX),
            playhead: 0,
            view_start: 0.0,
            ms_per_px: 20.0,
            drag: Drag::None,
            last_active: usize::MAX,
            tmp_dir,
            player: None,
            snap: true,
            keyframes: vec![],
            snap_kf: true,
            tool: VideoTool::None,
            vt_drag: None,
            spectrum: false,
            spec_tex: None,
            stream: None,
            audio: None,
            stream_frames: Default::default(),
            stream_count: 0,
            play_started: None,
            play_begin_at: None,
            play_from: 0,
            play_dur: 0,
        }
    }

    /// Önizleme için gerekli her şey yüklendi mi (ekran görüntüsü modu bekler).
    pub fn ready(&self) -> bool {
        match &self.info {
            Some(i) => (!i.has_video || self.tex.is_some()) && (!i.has_audio || self.wave.is_some()),
            None => false,
        }
    }

    /// Playhead'i `n` kare ileri/geri alır (fps yoksa 40 ms).
    pub fn step_frames(&mut self, n: i64) {
        let step = self.fps().map(|f| 1000.0 / f).unwrap_or(40.0);
        let cur = self.fps().map(|f| (self.playhead as f64 * f / 1000.0).round()).unwrap_or(0.0);
        self.playhead = match self.fps() {
            Some(f) => (((cur as i64 + n) as f64) * 1000.0 / f).round() as i64,
            None => self.playhead + (n as f64 * step) as i64,
        }
        .max(0);
    }

    pub fn waveform(&self) -> Option<Arc<Waveform>> {
        self.wave.clone()
    }

    pub fn is_open(&self) -> bool {
        self.path.is_some()
    }

    pub fn fps(&self) -> Option<f64> {
        self.info.as_ref().map(|i| i.fps).filter(|f| *f > 0.0)
    }

    pub fn open(&mut self, p: PathBuf, ctx: &egui::Context) {
        self.stop_player();
        self.info = None;
        self.wave = None;
        self.keyframes.clear();
        self.tex = None;
        self.frame_shown = (-1, u64::MAX);
        self.view_start = 0.0;
        self.status = "Ortam çözümleniyor…".into();
        self.path = Some(p.clone());
        let tx = self.tx.clone();
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let info = osisub_media::probe(&p);
            let has_audio = info.as_ref().map(|i| i.has_audio).unwrap_or(false);
            let info_has_video = info.as_ref().map(|i| i.has_video).unwrap_or(false);
            let _ = tx.send(Msg::Info(info));
            ctx2.request_repaint();
            if has_audio {
                let _ = tx.send(Msg::Wave(osisub_media::load_waveform(&p)));
                ctx2.request_repaint();
            }
            if info_has_video {
                if let Ok(k) = osisub_media::keyframes(&p) {
                    let _ = tx.send(Msg::Keyframes(k));
                    ctx2.request_repaint();
                }
            }
        });
    }

    pub fn stop_player(&mut self) {
        if let Some(mut c) = self.player.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    pub fn is_playing(&self) -> bool {
        self.player.is_some() || self.stream.is_some() || self.audio.is_some() || self.play_begin_at.is_some()
    }

    fn stop_all(&mut self) {
        self.stop_player();
        self.audio = None;
        self.stream = None;
        self.stream_frames.clear();
        self.play_begin_at = None;
        self.play_started = None;
    }

    /// Etkin satırı (ses + varsa video) oynatır; çalarken tekrar çağrılırsa durdurur.
    pub fn toggle_play(&mut self, doc: &Document, ctx: &egui::Context) {
        if self.is_playing() {
            self.stop_all();
            return;
        }
        let Some(p) = self.path.clone() else { return };
        let (s, e) = match doc.script.entries.get(doc.active) {
            Some(Entry::Dialogue(d)) => (d.start_time, d.end_time),
            _ => (self.playhead, self.playhead + 5000),
        };
        let (from, dur) = ((s - 300).max(0), e - s + 600);
        // Önce uygulama içi ses motoru (hassas ses saati); olmazsa harici ffplay.
        match osisub_media::audio::AudioPlayer::start(&p, from, dur) {
            Ok(a) => self.audio = Some(a),
            Err(why) => {
                let child = Command::new("ffplay")
                    .args(["-v", "quiet", "-nodisp", "-autoexit", "-ss", &format!("{:.3}", from as f64 / 1000.0), "-t", &format!("{:.3}", dur as f64 / 1000.0)])
                    .arg(&p)
                    .stdin(Stdio::null())
                    .spawn();
                match child {
                    Ok(c) => {
                        self.player = Some(c);
                        self.status = format!("Uygulama içi ses yok ({}); ffplay kullanılıyor", why);
                    }
                    Err(_) => self.status = format!("Ses çalınamadı: {}; ffplay da bulunamadı", why),
                }
            }
        }
        if let Some(info) = self.info.clone().filter(|i| i.has_video) {
            let ass = self.tmp_dir.join("play.ass");
            let _ = std::fs::write(&ass, doc.script.to_ass_string());
            match osisub_media::spawn_play(&p, Some(&ass), from, dur, 640, &info) {
                Ok(st) => {
                    self.stream = Some(st);
                    self.stream_frames.clear();
                    self.stream_count = 0;
                    self.play_started = None;
                }
                Err(e) => self.status = e,
            }
        }
        self.play_from = from;
        self.play_dur = dur;
        self.play_begin_at = Some(std::time::Instant::now());
        ctx.request_repaint();
    }

    /// Oynatma sırasında kare/zaman ilerletme.
    fn tick_play(&mut self, ctx: &egui::Context) {
        let Some(begin) = self.play_begin_at else { return };
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
        if let Some(st) = &self.stream {
            while let Ok(buf) = st.frames.try_recv() {
                if self.play_started.is_none() {
                    self.play_started = Some(std::time::Instant::now());
                }
                self.stream_frames.push_back((self.stream_count, buf));
                self.stream_count += 1;
            }
        }
        let (elapsed, ended) = match &self.audio {
            // Ses saati: konum aygıtın gerçekten çaldığı örneklerden gelir.
            Some(a) => {
                self.playhead = a.position_ms();
                (((self.playhead - self.play_from).max(0)) as f64 / 1000.0, a.finished() && begin.elapsed().as_millis() > 500)
            }
            None => {
                let clock = self.play_started.unwrap_or(begin);
                self.playhead = self.play_from + (begin.elapsed().as_secs_f64() * 1000.0) as i64;
                (clock.elapsed().as_secs_f64(), begin.elapsed().as_millis() as i64 > self.play_dur + 800)
            }
        };
        if let Some(st) = &self.stream {
            let target = (elapsed * st.fps) as usize;
            let (w, h) = (st.width as usize, st.height as usize);
            let mut latest: Option<Vec<u8>> = None;
            while let Some((idx, _)) = self.stream_frames.front() {
                if *idx <= target {
                    latest = self.stream_frames.pop_front().map(|x| x.1);
                } else {
                    break;
                }
            }
            if let Some(buf) = latest {
                let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &buf);
                match &mut self.tex {
                    Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                    None => self.tex = Some(ctx.load_texture("frame", img, egui::TextureOptions::LINEAR)),
                }
            }
        }
        if ended || begin.elapsed().as_millis() as i64 > self.play_dur + 3000 {
            self.stop_all();
            self.frame_shown = (-1, u64::MAX);
        }
    }

    fn pump(&mut self, ctx: &egui::Context, doc: &Document) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Info(Ok(i)) => {
                    self.status = format!("{}×{} @ {:.3} fps, {}", i.width, i.height, i.fps, time::format_ass(i.duration_ms));
                    self.info = Some(i);
                }
                Msg::Info(Err(e)) => self.status = format!("Ortam açılamadı: {}", e),
                Msg::Wave(Ok(w)) => self.wave = Some(Arc::new(w)),
                Msg::Keyframes(k) => self.keyframes = k,
                Msg::Wave(Err(e)) => self.status = format!("Ses okunamadı: {}", e),
                Msg::Frame { time, rev, res } => {
                    self.frame_busy = false;
                    match res {
                        Ok(f) => {
                            let img = egui::ColorImage::from_rgba_unmultiplied([f.width as usize, f.height as usize], &f.rgba);
                            self.tex = Some(ctx.load_texture("frame", img, egui::TextureOptions::LINEAR));
                            self.frame_shown = (time, rev);
                        }
                        Err(e) => {
                            self.status = e;
                            self.frame_shown = (time, rev);
                        }
                    }
                }
            }
        }
        if let (Some(p), Some(info)) = (self.path.clone(), self.info.clone()) {
            if info.has_video && !self.frame_busy && !self.is_playing() && self.frame_shown != (self.playhead, doc.rev) {
                self.frame_busy = true;
                let (t, rev) = (self.playhead, doc.rev);
                let ass = self.tmp_dir.join("preview.ass");
                let _ = std::fs::write(&ass, doc.script.to_ass_string());
                let tx = self.tx.clone();
                let ctx2 = ctx.clone();
                std::thread::spawn(move || {
                    let res = osisub_media::render_frame(&p, Some(&ass), t, 960, &info);
                    let _ = tx.send(Msg::Frame { time: t, rev, res });
                    ctx2.request_repaint();
                });
            }
        }
        if let Some(c) = self.player.as_mut() {
            if matches!(c.try_wait(), Ok(Some(_))) {
                self.player = None;
            }
        }
        self.tick_play(ctx);
    }

    fn heat(v: u8) -> Color32 {
        let t = v as f32 / 255.0;
        let stops: [(f32, [f32; 3]); 5] = [(0.0, [8.0, 8.0, 24.0]), (0.3, [70.0, 20.0, 110.0]), (0.55, [190.0, 50.0, 90.0]), (0.8, [250.0, 150.0, 40.0]), (1.0, [252.0, 240.0, 150.0])];
        for w in stops.windows(2) {
            let (a, b) = (w[0], w[1]);
            if t <= b.0 {
                let f = (t - a.0) / (b.0 - a.0);
                return Color32::from_rgb((a.1[0] + (b.1[0] - a.1[0]) * f) as u8, (a.1[1] + (b.1[1] - a.1[1]) * f) as u8, (a.1[2] + (b.1[2] - a.1[2]) * f) as u8);
            }
        }
        Color32::WHITE
    }

    fn snap_ms(&self, ms: i64) -> i64 {
        if self.snap_kf {
            let tol = (self.ms_per_px * 6.0) as i64;
            if let Some(k) = self.keyframes.iter().copied().filter(|k| (k - ms).abs() <= tol).min_by_key(|k| (k - ms).abs()) {
                return k;
            }
        }
        match (self.snap, self.fps()) {
            (true, Some(f)) => time::snap_to_frame(ms, f),
            _ => ms,
        }
    }

    /// Playhead'i başlangıç/bitiş olarak ayarlar (kısayollar için).
    pub fn set_active_start(&self, doc: &mut Document) {
        if let Some(Entry::Dialogue(d)) = doc.script.entries.get(doc.active).cloned() {
            doc.set_times(doc.active, self.playhead.min(d.end_time), d.end_time);
        }
    }

    pub fn set_active_end(&self, doc: &mut Document) {
        if let Some(Entry::Dialogue(d)) = doc.script.entries.get(doc.active).cloned() {
            doc.set_times(doc.active, d.start_time, self.playhead.max(d.start_time));
        }
    }

    /// Paneli çizer. Etkin satır değişirse playhead satır başına gider.
    pub fn ui(&mut self, ui: &mut egui::Ui, doc: &mut Document) {
        let ctx = ui.ctx().clone();
        self.pump(&ctx, doc);
        if self.last_active != doc.active {
            self.last_active = doc.active;
            if let Some(Entry::Dialogue(d)) = doc.script.entries.get(doc.active) {
                self.playhead = d.start_time + 20;
                let (w_ms, _) = (self.ms_per_px * 600.0, 0);
                if (d.start_time as f64) < self.view_start || (d.end_time as f64) > self.view_start + w_ms {
                    self.view_start = (d.start_time as f64 - w_ms * 0.25).max(0.0);
                }
            }
        }
        ui.horizontal_top(|ui| {
            let h = ui.available_height();
            // Video
            let vid_w = (h * 16.0 / 9.0).min(ui.available_width() * 0.45);
            let mut img_rect: Option<Rect> = None;
            ui.allocate_ui(egui::vec2(vid_w, h), |ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(vid_w, h), Sense::hover());
                ui.painter().rect_filled(rect, 4.0, Color32::BLACK);
                if let Some(t) = &self.tex {
                    let sz = t.size_vec2();
                    let scale = (rect.width() / sz.x).min(rect.height() / sz.y);
                    let img = Rect::from_center_size(rect.center(), sz * scale);
                    ui.painter().image(t.id(), img, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    img_rect = Some(img);
                } else {
                    let msg = if self.info.as_ref().map_or(false, |i| !i.has_video) { "Yalnızca ses" } else if self.is_open() { "Kare yükleniyor…" } else { "Video yok" };
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, msg, egui::FontId::proportional(14.0), Color32::GRAY);
                }
                ui.painter().text(rect.left_bottom() + egui::vec2(6.0, -6.0), egui::Align2::LEFT_BOTTOM, time::format_ass(self.playhead), egui::FontId::monospace(12.0), Color32::from_rgba_unmultiplied(255, 255, 255, 190));
            });
            if let (Some(img), true) = (img_rect, self.tool != VideoTool::None) {
                self.tool_ui(ui, img, doc);
            }
            // Dalga formu
            self.wave_ui(ui, doc);
        });
    }

    /// Etkin satırın konumu: `\\pos` ya da stil/hizalamadan türetilen varsayılan.
    fn current_pos(doc: &Document, d: &osisub_core::ass::Dialogue) -> (f32, f32) {
        let (rx, ry) = doc.script.res();
        let (rx, ry) = (rx as f32, ry as f32);
        if let Some(t) = osisub_core::overrides::get_initial(&d.text, "pos") {
            let a = t.args();
            return (a.first().and_then(|v| v.trim().parse().ok()).unwrap_or(rx / 2.0), a.get(1).and_then(|v| v.trim().parse().ok()).unwrap_or(ry / 2.0));
        }
        let st = doc.script.style_by_name(&d.style).cloned().unwrap_or_default();
        let an = osisub_core::overrides::get_initial(&d.text, "an").and_then(|t| t.number()).map(|n| n as i32).unwrap_or(st.align);
        let (ml, mr, mv) = (if d.margin_l > 0 { d.margin_l } else { st.margin_l } as f32, if d.margin_r > 0 { d.margin_r } else { st.margin_r } as f32, if d.margin_t > 0 { d.margin_t } else { st.margin_t } as f32);
        let x = match (an - 1) % 3 { 0 => ml, 1 => (rx - mr + ml) / 2.0, _ => rx - mr };
        let y = match (an - 1) / 3 { 0 => ry - mv, 1 => ry / 2.0, _ => mv };
        (x, y)
    }

    fn set_tag(doc: &mut Document, at: usize, text: &str, name: &str, arg: &str, desc: &str) {
        let nt = osisub_core::overrides::set_initial(text, name, arg);
        if nt != text {
            let key = format!("vt:{}:{}", name, at);
            doc.edit_keyed(desc, Some(&key), |tx| tx.dialogue_mut(at, |dd| dd.text = nt));
        }
    }

    /// Videodaki tipografi araçları: konum, kırpma, döndürme, ölçek.
    fn tool_ui(&mut self, ui: &mut egui::Ui, img: Rect, doc: &mut Document) {
        let at = doc.active;
        let Some(Entry::Dialogue(d)) = doc.script.entries.get(at).cloned() else { return };
        let (rx, ry) = doc.script.res();
        let (rx, ry) = (rx as f32, ry as f32);
        let to_screen = |x: f32, y: f32| Pos2::new(img.left() + x / rx * img.width(), img.top() + y / ry * img.height());
        let to_script = |p: Pos2| ((p.x - img.left()) / img.width() * rx, (p.y - img.top()) / img.height() * ry);
        let col = Color32::from_rgb(0xe5, 0x6b, 0x9a);
        let p = ui.painter_at(img);
        let cur = Self::current_pos(doc, &d);
        let c = to_screen(cur.0, cur.1);
        let r = ui.interact(img, egui::Id::unique("video_tool"), Sense::click_and_drag());
        let ptr = r.interact_pointer_pos();
        if r.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        match self.tool {
            VideoTool::None => {}
            VideoTool::Pos => {
                if osisub_core::overrides::get_initial(&d.text, "move").is_some() {
                    p.text(img.left_top() + egui::vec2(6.0, 6.0), egui::Align2::LEFT_TOP, "\\move var: konum aracı devre dışı", egui::FontId::proportional(12.0), Color32::from_rgb(0xe0, 0xa8, 0x4a));
                    return;
                }
                p.circle_stroke(c, 8.0, Stroke::new(2.0, col));
                p.line_segment([c - egui::vec2(14.0, 0.0), c + egui::vec2(14.0, 0.0)], Stroke::new(1.0, col));
                p.line_segment([c - egui::vec2(0.0, 14.0), c + egui::vec2(0.0, 14.0)], Stroke::new(1.0, col));
                p.text(c + egui::vec2(12.0, 10.0), egui::Align2::LEFT_TOP, format!("{:.0}, {:.0}", cur.0, cur.1), egui::FontId::monospace(11.0), col);
                if r.dragged() || r.clicked() {
                    if let Some(pos) = ptr {
                        let (x, y) = to_script(pos);
                        Self::set_tag(doc, at, &d.text, "pos", &format!("({},{})", x.round().clamp(0.0, rx), y.round().clamp(0.0, ry)), "Konum");
                    }
                }
            }
            VideoTool::Clip => {
                let existing: Option<(f32, f32, f32, f32)> = osisub_core::overrides::get_initial(&d.text, "clip").and_then(|t| {
                    let a: Vec<f32> = t.args().iter().filter_map(|v| v.trim().parse().ok()).collect();
                    (a.len() == 4).then(|| (a[0], a[1], a[2], a[3]))
                });
                if let Some((x1, y1, x2, y2)) = existing {
                    let (a, b) = (to_screen(x1.min(x2), y1.min(y2)), to_screen(x1.max(x2), y1.max(y2)));
                    let dim = Color32::from_rgba_unmultiplied(0, 0, 0, 120);
                    p.rect_filled(Rect::from_min_max(img.min, Pos2::new(img.right(), a.y)), 0.0, dim);
                    p.rect_filled(Rect::from_min_max(Pos2::new(img.left(), b.y), img.max), 0.0, dim);
                    p.rect_filled(Rect::from_min_max(Pos2::new(img.left(), a.y), Pos2::new(a.x, b.y)), 0.0, dim);
                    p.rect_filled(Rect::from_min_max(Pos2::new(b.x, a.y), Pos2::new(img.right(), b.y)), 0.0, dim);
                    p.rect_stroke(Rect::from_min_max(a, b), 0.0, Stroke::new(1.5, col), egui::StrokeKind::Inside);
                    for corner in [a, b, Pos2::new(a.x, b.y), Pos2::new(b.x, a.y)] {
                        p.rect_filled(Rect::from_center_size(corner, egui::vec2(8.0, 8.0)), 1.0, col);
                    }
                }
                if r.drag_started() {
                    if let Some(pos) = ptr {
                        // Mevcut köşeye yakınsa onu taşı (modlar: 1=sol-üst, 2=sağ-alt, 3=sol-alt, 4=sağ-üst), değilse yeni dikdörtgen.
                        let mut mode = 0u8;
                        let anchor;
                        if let Some((x1, y1, x2, y2)) = existing {
                            let (l, t, rr, bb) = (x1.min(x2), y1.min(y2), x1.max(x2), y1.max(y2));
                            let corners = [(1u8, (l, t), (rr, bb)), (2, (rr, bb), (l, t)), (3, (l, bb), (rr, t)), (4, (rr, t), (l, bb))];
                            let hit = corners.iter().find(|(_, c, _)| to_screen(c.0, c.1).distance(pos) < 10.0);
                            if let Some((m, _, opp)) = hit {
                                mode = *m;
                                anchor = *opp;
                            } else {
                                anchor = to_script(pos);
                            }
                        } else {
                            anchor = to_script(pos);
                        }
                        self.vt_drag = Some((mode, anchor));
                    }
                }
                if r.dragged() {
                    if let (Some((_, anchor)), Some(pos)) = (self.vt_drag, ptr) {
                        let (x, y) = to_script(pos);
                        let (x, y) = (x.round().clamp(0.0, rx), y.round().clamp(0.0, ry));
                        let (ax, ay) = (anchor.0.round(), anchor.1.round());
                        Self::set_tag(doc, at, &d.text, "clip", &format!("({},{},{},{})", ax.min(x), ay.min(y), ax.max(x), ay.max(y)), "Kırpma");
                    }
                }
                if r.drag_stopped() {
                    self.vt_drag = None;
                }
                if r.secondary_clicked() {
                    let nt = osisub_core::overrides::remove_initial(&d.text, "clip");
                    if nt != d.text {
                        doc.edit("Kırpmayı kaldır", |tx| tx.dialogue_mut(at, |dd| dd.text = nt));
                    }
                }
            }
            VideoTool::Rotate => {
                let org = osisub_core::overrides::get_initial(&d.text, "org").map(|t| t.args()).and_then(|a| Some((a.first()?.trim().parse::<f32>().ok()?, a.get(1)?.trim().parse::<f32>().ok()?))).unwrap_or(cur);
                let o = to_screen(org.0, org.1);
                let frz = osisub_core::overrides::get_initial(&d.text, "frz").and_then(|t| t.number()).unwrap_or(0.0) as f32;
                let rad = 70.0;
                let hpos = o + egui::vec2(rad * (-frz).to_radians().cos(), rad * (-frz).to_radians().sin());
                p.circle_stroke(o, rad, Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xe5, 0x6b, 0x9a, 120)));
                p.line_segment([o, hpos], Stroke::new(2.0, col));
                p.circle_filled(hpos, 6.0, col);
                p.circle_filled(o, 4.0, Color32::WHITE);
                p.text(o + egui::vec2(8.0, 8.0), egui::Align2::LEFT_TOP, format!("frz {:.1}°", frz), egui::FontId::monospace(11.0), col);
                if r.dragged() {
                    if let Some(pos) = ptr {
                        let v = pos - o;
                        let mut a = -(v.y.atan2(v.x)).to_degrees();
                        if ui.input(|i| i.modifiers.shift) {
                            a = (a / 15.0).round() * 15.0;
                        }
                        Self::set_tag(doc, at, &d.text, "frz", &format!("{:.1}", a).trim_end_matches('0').trim_end_matches('.').to_string(), "Döndür");
                    }
                }
                if r.secondary_clicked() {
                    let nt = osisub_core::overrides::remove_initial(&d.text, "frz");
                    if nt != d.text {
                        doc.edit("Dönmeyi kaldır", |tx| tx.dialogue_mut(at, |dd| dd.text = nt));
                    }
                }
            }
            VideoTool::Scale => {
                let sx = osisub_core::overrides::get_initial(&d.text, "fscx").and_then(|t| t.number()).unwrap_or(100.0) as f32;
                p.circle_stroke(c, 100.0 / 100.0 * 60.0, Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xe5, 0x6b, 0x9a, 120)));
                let h = c + egui::vec2(sx / 100.0 * 60.0, 0.0);
                p.line_segment([c, h], Stroke::new(2.0, col));
                p.circle_filled(h, 6.0, col);
                p.text(c + egui::vec2(8.0, 8.0), egui::Align2::LEFT_TOP, format!("ölçek {:.0}%", sx), egui::FontId::monospace(11.0), col);
                if r.dragged() {
                    if let Some(pos) = ptr {
                        let v = ((pos - c).length() / 60.0 * 100.0).round().clamp(1.0, 1000.0);
                        let t1 = osisub_core::overrides::set_initial(&d.text, "fscx", &format!("{}", v));
                        let t2 = osisub_core::overrides::set_initial(&t1, "fscy", &format!("{}", v));
                        if t2 != d.text {
                            let key = format!("vt:fsc:{}", at);
                            doc.edit_keyed("Ölçek", Some(&key), |tx| tx.dialogue_mut(at, |dd| dd.text = t2));
                        }
                    }
                }
            }
        }
    }

    fn wave_ui(&mut self, ui: &mut egui::Ui, doc: &mut Document) {
        let size = egui::vec2(ui.available_width(), ui.available_height());
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 4.0, Color32::from_rgb(0x14, 0x15, 0x1b));
        let w_px = rect.width().max(1.0) as usize;
        let x_of = |ms: f64, vs: f64, mpp: f64| rect.left() + ((ms - vs) / mpp) as f32;
        let (vs, mpp) = (self.view_start, self.ms_per_px);
        let ms_of = |x: f32| vs + (x - rect.left()) as f64 * mpp;

        // Kaydır / yakınlaştır
        if resp.hovered() {
            let (scroll, zoom, ctrl) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.modifiers.command));
            if ctrl && scroll.y != 0.0 || zoom != 1.0 {
                let f = if zoom != 1.0 { 1.0 / zoom as f64 } else { (1.0 - scroll.y as f64 * 0.003).clamp(0.5, 2.0) };
                let anchor = resp.hover_pos().map(|p| ms_of(p.x)).unwrap_or(vs);
                let new_mpp = (self.ms_per_px * f).clamp(0.5, 200.0);
                self.view_start = (anchor - (anchor - vs) * new_mpp / mpp).max(0.0);
                self.ms_per_px = new_mpp;
            } else if scroll != egui::Vec2::ZERO {
                self.view_start = (self.view_start - (scroll.x as f64 + scroll.y as f64) * mpp).max(0.0);
            }
        }
        let (vs, mpp) = (self.view_start, self.ms_per_px);
        let ms_of = |x: f32| vs + (x - rect.left()) as f64 * mpp;
        let (v_end, mid) = (vs + w_px as f64 * mpp, rect.center().y);

        let act = doc.active;
        // Altyazı satırları (spektrumda üstte, dalga formunda altta çizilir)
        let draw_lines = |p: &egui::Painter, doc: &Document| {
        let (act, sel) = (doc.active, doc.selection.clone());
        for (i, e) in doc.script.entries.iter().enumerate() {
            if let Entry::Dialogue(d) = e {
                if (d.end_time as f64) < vs || (d.start_time as f64) > v_end {
                    continue;
                }
                let r = Rect::from_min_max(Pos2::new(x_of(d.start_time as f64, vs, mpp).max(rect.left()), rect.top()), Pos2::new(x_of(d.end_time as f64, vs, mpp).min(rect.right()), rect.bottom()));
                let col = if i == act { Color32::from_rgba_unmultiplied(0xe5, 0x6b, 0x9a, 70) } else if sel.binary_search(&i).is_ok() { Color32::from_rgba_unmultiplied(0xe5, 0x6b, 0x9a, 36) } else { Color32::from_rgba_unmultiplied(120, 160, 255, 22) };
                p.rect_filled(r, 0.0, col);
                p.line_segment([Pos2::new(r.left(), r.top()), Pos2::new(r.left(), r.bottom())], Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 160, 255, 90)));
                if r.width() > 40.0 && i == act {
                    p.text(r.left_top() + egui::vec2(4.0, 2.0), egui::Align2::LEFT_TOP, osisub_core::tags::strip_tags(&d.text).chars().take(40).collect::<String>(), egui::FontId::proportional(11.0), Color32::from_gray(220));
                }
            }
        }

        };
        if !self.spectrum {
            draw_lines(&p, doc);
        }
        // Spektrum
        if let (true, Some(w)) = (self.spectrum, self.wave.clone()) {
            let bins = (rect.height() as usize).clamp(32, 256);
            let key = ((vs * 10.0) as i64, (mpp * 1000.0) as i64, w_px, bins);
            if self.spec_tex.as_ref().map(|t| t.0) != Some(key) {
                let raw = osisub_media::spectrogram(&w, vs, mpp, w_px, bins);
                let pixels: Vec<Color32> = raw.iter().map(|&v| Self::heat(v)).collect();
                let img = egui::ColorImage { size: [w_px, bins], source_size: egui::vec2(w_px as f32, bins as f32), pixels };
                let tex = ui.ctx().load_texture("spectrum", img, egui::TextureOptions::LINEAR);
                self.spec_tex = Some((key, tex));
            }
            if let Some((_, t)) = &self.spec_tex {
                p.image(t.id(), Rect::from_min_size(rect.min, egui::vec2(w_px as f32, rect.height())), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
            draw_lines(&p, doc);
        } else if let Some(w) = &self.wave {
            let peaks = w.peaks(vs, mpp, w_px);
            let amp = rect.height() * 0.45 / 32768.0;
            for (i, (lo, hi)) in peaks.iter().enumerate() {
                let x = rect.left() + i as f32 + 0.5;
                p.line_segment([Pos2::new(x, mid - *hi as f32 * amp), Pos2::new(x, mid - *lo as f32 * amp - 1.0)], Stroke::new(1.0, Color32::from_rgb(0x6f, 0xb8, 0xd8)));
            }
        } else if self.is_open() && self.info.as_ref().map_or(true, |i| i.has_audio) {
            p.text(rect.center(), egui::Align2::CENTER_CENTER, "Ses yükleniyor…", egui::FontId::proportional(13.0), Color32::GRAY);
        } else {
            p.text(rect.center(), egui::Align2::CENTER_CENTER, "Ses yok — Video menüsünden ortam açın", egui::FontId::proportional(13.0), Color32::GRAY);
        }
        p.line_segment([Pos2::new(rect.left(), mid), Pos2::new(rect.right(), mid)], Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 25)));

        // Saniye çizgileri
        let step = [100.0, 250.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 30000.0, 60000.0].iter().copied().find(|s| s / mpp > 70.0).unwrap_or(120000.0);
        let mut t = (vs / step).floor() * step;
        while t < v_end {
            let x = x_of(t, vs, mpp);
            if x >= rect.left() {
                p.line_segment([Pos2::new(x, rect.bottom() - 8.0), Pos2::new(x, rect.bottom())], Stroke::new(1.0, Color32::from_gray(110)));
                p.text(Pos2::new(x + 3.0, rect.bottom() - 2.0), egui::Align2::LEFT_BOTTOM, time::format_ass(t as i64).trim_end_matches(|c: char| c == '0').trim_end_matches('.').to_string(), egui::FontId::monospace(9.0), Color32::from_gray(130));
            }
            t += step;
        }

        // Anahtar kareler
        for &k in &self.keyframes {
            if (k as f64) >= vs && (k as f64) <= v_end {
                let x = x_of(k as f64, vs, mpp);
                p.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.top() + 7.0)], Stroke::new(1.0, Color32::from_rgb(0x7a, 0xd9, 0x8a)));
            }
        }

        // Playhead
        let px = x_of(self.playhead as f64, vs, mpp);
        if px >= rect.left() && px <= rect.right() {
            p.line_segment([Pos2::new(px, rect.top()), Pos2::new(px, rect.bottom())], Stroke::new(1.0, Color32::from_rgb(0xff, 0xd0, 0x4a)));
        }

        // Etkin satır tutamaçları ve sürükleme
        let Some(Entry::Dialogue(d)) = doc.script.entries.get(act).cloned() else { return };
        let (xs, xe) = (x_of(d.start_time as f64, vs, mpp), x_of(d.end_time as f64, vs, mpp));
        for x in [xs, xe] {
            if x >= rect.left() - 2.0 && x <= rect.right() + 2.0 {
                p.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(2.0, Color32::from_rgb(0xe5, 0x6b, 0x9a)));
            }
        }
        if resp.drag_started() {
            if let Some(pos) = resp.interact_pointer_pos() {
                self.drag = if (pos.x - xs).abs() <= 7.0 { Drag::Start } else if (pos.x - xe).abs() <= 7.0 { Drag::End } else if pos.x > xs && pos.x < xe && ui.input(|i| i.modifiers.alt) { Drag::Body(ms_of(pos.x) as i64 - d.start_time) } else { Drag::None };
            }
        }
        if resp.dragged() && self.drag != Drag::None {
            if let Some(pos) = resp.interact_pointer_pos() {
                let m = self.snap_ms(ms_of(pos.x) as i64).max(0);
                match self.drag {
                    Drag::Start => doc.set_times(act, m.min(d.end_time), d.end_time),
                    Drag::End => doc.set_times(act, d.start_time, m.max(d.start_time)),
                    Drag::Body(off) => {
                        let ns = self.snap_ms(m - off).max(0);
                        doc.set_times(act, ns, ns + d.duration());
                    }
                    Drag::None => {}
                }
                self.playhead = m;
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if resp.drag_stopped() {
            self.drag = Drag::None;
        }
        if resp.hovered() {
            if let Some(pos) = resp.hover_pos() {
                if (pos.x - xs).abs() <= 7.0 || (pos.x - xe).abs() <= 7.0 {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
            }
        }
        // Tıklama: sol = playhead (Shift = başlangıç), sağ = bitiş
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let m = self.snap_ms(ms_of(pos.x) as i64).max(0);
                if ui.input(|i| i.modifiers.shift) {
                    doc.set_times(act, m.min(d.end_time), d.end_time);
                }
                self.playhead = m;
            }
        }
        if resp.secondary_clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let m = self.snap_ms(ms_of(pos.x) as i64).max(0);
                doc.set_times(act, d.start_time, m.max(d.start_time));
            }
        }
    }
}

impl Drop for Media {
    fn drop(&mut self) {
        self.stop_all();
        let _ = std::fs::remove_dir_all(&self.tmp_dir);
    }
}
