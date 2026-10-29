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
    pub pos_tool: bool,
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
            pos_tool: false,
        }
    }

    /// Önizleme için gerekli her şey yüklendi mi (ekran görüntüsü modu bekler).
    pub fn ready(&self) -> bool {
        match &self.info {
            Some(i) => (!i.has_video || self.tex.is_some()) && (!i.has_audio || self.wave.is_some()),
            None => false,
        }
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

    fn stop_player(&mut self) {
        if let Some(mut c) = self.player.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    pub fn toggle_play(&mut self, doc: &Document) {
        if self.player.is_some() {
            self.stop_player();
            return;
        }
        let Some(p) = self.path.clone() else { return };
        let (s, e) = match doc.script.entries.get(doc.active) {
            Some(Entry::Dialogue(d)) => (d.start_time, d.end_time),
            _ => (self.playhead, self.playhead + 5000),
        };
        let child = Command::new("ffplay")
            .args(["-v", "quiet", "-nodisp", "-autoexit", "-ss", &format!("{:.3}", (s - 300).max(0) as f64 / 1000.0), "-t", &format!("{:.3}", (e - s + 600) as f64 / 1000.0)])
            .arg(p)
            .stdin(Stdio::null())
            .spawn();
        match child {
            Ok(c) => self.player = Some(c),
            Err(_) => self.status = "ffplay bulunamadı (ses oynatma için gerekli)".into(),
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
            if info.has_video && !self.frame_busy && self.frame_shown != (self.playhead, doc.rev) {
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
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }
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
            ui.allocate_ui(egui::vec2(vid_w, h), |ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(vid_w, h), Sense::hover());
                ui.painter().rect_filled(rect, 4.0, Color32::BLACK);
                if let Some(t) = &self.tex {
                    let sz = t.size_vec2();
                    let scale = (rect.width() / sz.x).min(rect.height() / sz.y);
                    let img = Rect::from_center_size(rect.center(), sz * scale);
                    ui.painter().image(t.id(), img, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    if self.pos_tool {
                        Self::pos_tool_ui(ui, img, doc);
                    }
                } else {
                    let msg = if self.info.as_ref().map_or(false, |i| !i.has_video) { "Yalnızca ses" } else if self.is_open() { "Kare yükleniyor…" } else { "Video yok" };
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, msg, egui::FontId::proportional(14.0), Color32::GRAY);
                }
                ui.painter().text(rect.left_bottom() + egui::vec2(6.0, -6.0), egui::Align2::LEFT_BOTTOM, time::format_ass(self.playhead), egui::FontId::monospace(12.0), Color32::from_rgba_unmultiplied(255, 255, 255, 190));
            });
            // Dalga formu
            self.wave_ui(ui, doc);
        });
    }

    /// Etkin satırın \pos'unu (yoksa stilden türetilen varsayılan konumu) video üzerinde sürüklemeyi sağlar.
    fn pos_tool_ui(ui: &mut egui::Ui, img: Rect, doc: &mut Document) {
        let at = doc.active;
        let Some(Entry::Dialogue(d)) = doc.script.entries.get(at).cloned() else { return };
        let (rx, ry) = doc.script.res();
        let (rx, ry) = (rx as f32, ry as f32);
        if osisub_core::overrides::get_initial(&d.text, "move").is_some() {
            ui.painter().text(img.left_top() + egui::vec2(6.0, 6.0), egui::Align2::LEFT_TOP, "\\move var: konum aracı devre dışı", egui::FontId::proportional(12.0), Color32::from_rgb(0xe0, 0xa8, 0x4a));
            return;
        }
        // Mevcut ya da varsayılan konum
        let cur: (f32, f32) = match osisub_core::overrides::get_initial(&d.text, "pos") {
            Some(t) => {
                let a = t.args();
                (a.first().and_then(|v| v.trim().parse().ok()).unwrap_or(rx / 2.0), a.get(1).and_then(|v| v.trim().parse().ok()).unwrap_or(ry / 2.0))
            }
            None => {
                let st = doc.script.style_by_name(&d.style).cloned().unwrap_or_default();
                let an = osisub_core::overrides::get_initial(&d.text, "an").and_then(|t| t.number()).map(|n| n as i32).unwrap_or(st.align);
                let (ml, mr, mv) = (if d.margin_l > 0 { d.margin_l } else { st.margin_l } as f32, if d.margin_r > 0 { d.margin_r } else { st.margin_r } as f32, if d.margin_t > 0 { d.margin_t } else { st.margin_t } as f32);
                let x = match (an - 1) % 3 { 0 => ml, 1 => (rx - mr + ml) / 2.0, _ => rx - mr };
                let y = match (an - 1) / 3 { 0 => ry - mv, 1 => ry / 2.0, _ => mv };
                (x, y)
            }
        };
        let to_screen = |x: f32, y: f32| Pos2::new(img.left() + x / rx * img.width(), img.top() + y / ry * img.height());
        let c = to_screen(cur.0, cur.1);
        let p = ui.painter_at(img);
        let col = Color32::from_rgb(0xe5, 0x6b, 0x9a);
        p.circle_stroke(c, 8.0, Stroke::new(2.0, col));
        p.line_segment([c - egui::vec2(14.0, 0.0), c + egui::vec2(14.0, 0.0)], Stroke::new(1.0, col));
        p.line_segment([c - egui::vec2(0.0, 14.0), c + egui::vec2(0.0, 14.0)], Stroke::new(1.0, col));
        p.text(c + egui::vec2(12.0, 10.0), egui::Align2::LEFT_TOP, format!("{:.0}, {:.0}", cur.0, cur.1), egui::FontId::monospace(11.0), col);
        let r = ui.interact(img, egui::Id::new("pos_tool"), Sense::click_and_drag());
        if r.dragged() || r.clicked() {
            if let Some(pos) = r.interact_pointer_pos() {
                let x = ((pos.x - img.left()) / img.width() * rx).round().clamp(0.0, rx);
                let y = ((pos.y - img.top()) / img.height() * ry).round().clamp(0.0, ry);
                let nt = osisub_core::overrides::set_initial(&d.text, "pos", &format!("({},{})", x, y));
                if nt != d.text {
                    let key = format!("pos:{}", at);
                    doc.edit_keyed("Konum", Some(&key), |tx| tx.dialogue_mut(at, |dd| dd.text = nt));
                }
            }
        }
        if r.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
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

        // Altyazı satırları
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

        // Dalga
        if let Some(w) = &self.wave {
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
        self.stop_player();
        let _ = std::fs::remove_dir_all(&self.tmp_dir);
    }
}
