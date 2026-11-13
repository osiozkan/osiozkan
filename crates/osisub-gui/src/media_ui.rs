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
    /// `time` anından başlayan ardışık kareler (ilk kare = istenen kare)
    /// `first`: kare hızı biliniyorsa ilk çizilen karenin numarası (istenen karenin birkaç kare öncesi)
    Frame { time: i64, rev: u64, first: Option<i64>, res: Result<Vec<Frame>, String> },
    Keyframes(Vec<i64>),
    /// Videodan algılanan değişken kare hızı (VFR)
    Vfr(osisub_core::vfr::Framerate),
    /// Pipetle videodan alınan renk
    Picked(Result<[u8; 3], String>),
    /// Sahte video üretildi: (Aegisub `?dummy:` dizesi, sonuç)
    Dummy(String, Result<PathBuf, String>),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VideoTool {
    None,
    Pos,
    Move,
    Clip,
    VClip,
    Rotate,
    /// \frx/\fry (Aegisub Rotate XY)
    RotateXY,
    Scale,
    /// `\p` çizim noktalarını düzenleme (\an7, döndürmesiz)
    Drawing,
    /// 4 köşeyi sürükleyerek perspektife oturtma (\frx\fry\frz\fscx\fscy\fax\org)
    Perspective,
}

#[derive(Clone, Copy, PartialEq)]
enum Drag {
    None,
    Start,
    End,
    /// karaoke: `i` ile `i+1` hecesi arasındaki sınır
    Syl(usize),
    Body(i64),
}

/// Etkin satıra göre oynatılacak bölüm (Aegisub audio/play/selection/*).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlayPart {
    Selection,
    Before,
    After,
    Begin,
    End,
}

impl PlayPart {
    /// (başlangıç, süre) ms
    pub fn range(self, start: i64, end: i64) -> (i64, i64) {
        let dur = (end - start).max(0);
        let half = dur.min(500);
        match self {
            PlayPart::Selection => (start, dur),
            PlayPart::Before => ((start - 500).max(0), start.min(500)),
            PlayPart::After => (end, 500),
            PlayPart::Begin => (start, half),
            PlayPart::End => (end - half, half),
        }
    }
}

pub struct Media {
    pub path: Option<PathBuf>,
    /// Açık ortam bir Aegisub sahte videosuysa onun `?dummy:` dizesi (projeye bu yazılır)
    pub dummy: Option<String>,
    pub info: Option<MediaInfo>,
    wave: Option<Arc<Waveform>>,
    pub status: String,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    tex: Option<TextureHandle>,
    frame_busy: bool,
    frame_shown: (i64, u64),
    frame_cache: std::collections::VecDeque<((i64, u64), TextureHandle)>,
    rev_seen: u64,
    rev_changed_at: std::time::Instant,
    pub playhead: i64,
    view_start: f64,
    ms_per_px: f64,
    drag: Drag,
    last_active: usize,
    tmp_dir: PathBuf,
    player: Option<Child>,
    pub snap: bool,
    pub keyframes: Vec<i64>,
    /// Yüklü timecode dosyası (VFR); varsa kare hesapları bunu kullanır.
    pub timecodes: Option<Arc<osisub_core::vfr::Framerate>>,
    /// `timecodes` videodan otomatik mi geldi (yeni video açılınca sıfırlanır)
    pub timecodes_auto: bool,
    pub snap_kf: bool,
    /// Videoda \pos sürükleme aracı.
    pub tool: VideoTool,
    vt_drag: Option<(u8, (f32, f32))>,
    /// \move aracı: sürükleme başındaki değer
    vt_move0: Option<osisub_core::overrides::Move>,
    /// vektör \clip aracı: sürüklenen nokta (komut, nokta)
    vt_vdrag: Option<(usize, usize)>,
    /// perspektif aracı: son oturtulan dörtgen (satır, satır metni, köşeler) ve sürüklenen köşe (4 = tümü)
    vt_quad: Option<(usize, String, [(f64, f64); 4])>,
    vt_qdrag: Option<(usize, [(f64, f64); 4], (f64, f64))>,
    /// perspektif aracı için uygulamanın doldurduğu: etkin satırın kutusu (satır, metin, w, h) ve kamera uzaklığı
    pub persp_box: Option<(usize, String, (f64, f64))>,
    pub persp_dist: f64,
    /// perspektif aracı LayoutRes'siz betiğe değer yazdı: uygulama LayoutRes'i ekler
    pub persp_need_layout: bool,
    /// önizleme klasörüne çıkarılmış gömülü yazı tiplerinin anahtarı (ad:boyut;…)
    fonts_key: String,
    /// video önizleme yakınlaştırma (1 = sığdır) ve kaydırma (piksel)
    pub video_zoom: f32,
    /// en-boy oranı geçersiz kılma (Aegisub "Override AR"); None = otomatik (anamorfikte SAR'dan)
    pub ar_override: Option<f64>,
    /// videoda güvenli alan kılavuzu (eylem %90, başlık %80)
    pub safe_area: bool,
    pub video_pan: egui::Vec2,
    pub spectrum: bool,
    /// Oynatma hızı ve ses seviyesi (bir sonraki oynatmada geçerli)
    pub play_opts: osisub_media::audio::PlayOpts,
    /// Video ayrı pencerede (Aegisub "Detach video")
    pub detached: bool,
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
            dummy: None,
            info: None,
            wave: None,
            status: String::new(),
            tx,
            rx,
            tex: None,
            frame_busy: false,
            frame_shown: (-1, u64::MAX),
            frame_cache: Default::default(),
            rev_seen: u64::MAX,
            rev_changed_at: std::time::Instant::now(),
            playhead: 0,
            view_start: 0.0,
            ms_per_px: 20.0,
            drag: Drag::None,
            last_active: usize::MAX,
            tmp_dir,
            player: None,
            snap: true,
            keyframes: vec![],
            timecodes: None,
            timecodes_auto: false,
            snap_kf: true,
            tool: VideoTool::None,
            vt_drag: None,
            vt_move0: None,
            vt_quad: None,
            vt_qdrag: None,
            persp_box: None,
            safe_area: false,
            ar_override: None,
            persp_dist: osisub_core::perspective::DIST_PX,
            persp_need_layout: false,
            vt_vdrag: None,
            fonts_key: String::new(),
            video_zoom: 1.0,
            video_pan: egui::Vec2::ZERO,
            spectrum: false,
            play_opts: Default::default(),
            detached: false,
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
        self.playhead = match self.framerate() {
            // Önce en yakın kare başlangıcına oturt, sonra n kare git (VFR'de kare süreleri farklı olabilir).
            Some(fr) => fr.time_at(fr.frame_at(fr.snap(self.playhead)) + n),
            None => self.playhead + n * 40,
        }
        .max(0);
    }

    /// Kare önbelleğini boşaltır ve kareyi yeniden ister (en-boy oranı değişince).
    pub fn invalidate_frames(&mut self) {
        self.frame_cache.clear();
        self.frame_shown = (i64::MIN, u64::MAX);
    }

    /// Kare çizimi için ortam bilgisi (en-boy oranı geçersiz kılması uygulanmış).
    pub fn render_info(&self) -> Option<osisub_media::MediaInfo> {
        let mut i = self.info.clone()?;
        if let Some(ar) = self.ar_override { i.dar = ar; }
        Some(i)
    }

    /// Önceki/sonraki anahtar kareye git (`dir` < 0 geri). Anahtar kare yoksa yerinde kalır.
    pub fn step_keyframe(&mut self, dir: i64) -> bool {
        let cur = self.playhead;
        let k = if dir > 0 { self.keyframes.iter().copied().filter(|&k| k > cur + 1).min() } else { self.keyframes.iter().copied().filter(|&k| k < cur - 1).max() };
        match k {
            Some(k) => { self.playhead = k; true }
            None => false,
        }
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

    /// Kare hızı: timecode dosyası yüklüyse o (VFR), yoksa videonun sabit hızı.
    pub fn framerate(&self) -> Option<Arc<osisub_core::vfr::Framerate>> {
        self.timecodes.clone().or_else(|| self.fps().map(|f| Arc::new(osisub_core::vfr::Framerate::cfr(f))))
    }

    /// Aegisub sahte videosunu (`?dummy:…`) üretip açar (önbellekte varsa hemen).
    pub fn open_dummy(&mut self, d: &osisub_core::dummy::DummyVideo, ctx: &egui::Context) {
        let Some(dir) = osisub_core::paths::data_dir().map(|p| p.join("dummy")) else {
            self.status = "Sahte video: veri klasörü bulunamadı".into();
            return;
        };
        let out = dir.join(d.cache_name());
        let (src, key, tx, ctx2) = (d.lavfi_source(), d.to_aegisub(), self.tx.clone(), ctx.clone());
        self.status = if out.is_file() { "Sahte video açılıyor…".into() } else { format!("Sahte video hazırlanıyor ({} kare; bir kez, önbelleğe alınır)…", d.frames) };
        std::thread::spawn(move || {
            let r = osisub_media::render_lavfi(&src, &out);
            let _ = tx.send(Msg::Dummy(key, r));
            ctx2.request_repaint();
        });
    }

    pub fn open(&mut self, p: PathBuf, ctx: &egui::Context) {
        self.dummy = None;
        self.stop_player();
        self.info = None;
        self.wave = None;
        self.keyframes.clear();
        if self.timecodes_auto {
            self.timecodes = None;
            self.timecodes_auto = false;
        }
        self.tex = None;
        self.frame_cache.clear();
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
                if let Some(fr) = osisub_media::frame_timestamps(&p).ok().and_then(osisub_core::vfr::Framerate::from_timestamps) {
                    let _ = tx.send(Msg::Vfr(fr));
                    ctx2.request_repaint();
                }
                if let Ok(k) = osisub_media::keyframes(&p) {
                    let _ = tx.send(Msg::Keyframes(k));
                    ctx2.request_repaint();
                }
            }
        });
    }

    /// Gömülü yazı tiplerini (değiştiyse) önizleme klasörüne çıkarır; libass `fontsdir` ile okur.
    fn sync_preview_fonts(&mut self, doc: &Document) {
        let list = osisub_core::attach::list(&doc.script);
        let key: String = list.iter().filter(|a| a.section == osisub_core::attach::SEC_FONTS).map(|a| format!("{}:{};", a.name, a.size)).collect();
        if key == self.fonts_key {
            return;
        }
        self.fonts_key = key;
        let dir = self.tmp_dir.join(".osisub-fonts");
        let _ = std::fs::remove_dir_all(&dir);
        let fonts: Vec<_> = list.iter().filter(|a| a.section == osisub_core::attach::SEC_FONTS).collect();
        if fonts.is_empty() {
            return;
        }
        let _ = std::fs::create_dir_all(&dir);
        for a in fonts {
            if let Some(data) = osisub_core::attach::extract(&doc.script, &a.name) {
                let safe: String = a.name.chars().map(|c| if c == '/' || c == '\\' { '_' } else { c }).collect();
                let _ = std::fs::write(dir.join(safe), data);
            }
        }
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
        let (s, e) = match doc.script.entries.get(doc.active) {
            Some(Entry::Dialogue(d)) => (d.start_time, d.end_time),
            _ => (self.playhead, self.playhead + 5000),
        };
        self.play_range(doc, ctx, (s - 300).max(0), e - s + 600);
    }

    /// Aegisub ses kipi oynatmaları (etkin satıra göre): seçim, başlangıçtan önceki/bitişten
    /// sonraki 500 ms, seçimin ilk/son 500 ms'si. Çalarken çağrılırsa durdurur.
    pub fn play_part(&mut self, doc: &Document, ctx: &egui::Context, part: PlayPart) {
        if self.is_playing() {
            self.stop_all();
            return;
        }
        let Some(Entry::Dialogue(d)) = doc.script.entries.get(doc.active) else { return };
        let (from, dur) = part.range(d.start_time, d.end_time);
        if dur > 0 {
            self.play_range(doc, ctx, from, dur);
        }
    }

    pub fn stop(&mut self) {
        if self.is_playing() {
            self.stop_all();
        }
    }

    /// Playhead'den itibaren (satır sınırı olmadan) oynatır; çalarken çağrılırsa durdurur.
    pub fn toggle_play_from_playhead(&mut self, doc: &Document, ctx: &egui::Context) {
        if self.is_playing() {
            self.stop_all();
            return;
        }
        let end = self.info.as_ref().map(|i| i.duration_ms).filter(|d| *d > 0).unwrap_or(self.playhead + 60_000);
        let from = self.playhead.max(0);
        // çok uzun akışlar için üst sınır: 10 dk (yeniden basınca devam eder)
        self.play_range(doc, ctx, from, (end - from).clamp(500, 600_000));
    }

    fn play_range(&mut self, doc: &Document, ctx: &egui::Context, from: i64, dur: i64) {
        let Some(p) = self.path.clone() else { return };
        // Önce uygulama içi ses motoru (hassas ses saati); olmazsa harici ffplay.
        match osisub_media::audio::AudioPlayer::start(&p, from, dur, self.play_opts) {
            Ok(a) => self.audio = Some(a),
            Err(why) => {
                let mut cmd = Command::new(osisub_media::ffplay_bin());
                cmd.args(["-v", "quiet", "-nodisp", "-autoexit", "-ss", &format!("{:.3}", from as f64 / 1000.0), "-t", &format!("{:.3}", dur as f64 / 1000.0)]);
                let af = self.play_opts.filter();
                if !af.is_empty() { cmd.args(["-af", &af]); }
                let child = cmd.arg(&p).stdin(Stdio::null()).spawn();
                match child {
                    Ok(c) => {
                        self.player = Some(c);
                        self.status = format!("Uygulama içi ses yok ({}); ffplay kullanılıyor", why);
                    }
                    Err(_) => self.status = format!("Ses çalınamadı: {}; ffplay da bulunamadı", why),
                }
            }
        }
        if let Some(info) = self.render_info().filter(|i| i.has_video) {
            let ass = self.tmp_dir.join("play.ass");
            let (lo, hi) = (from - 1000, from + dur + 1000);
            self.sync_preview_fonts(doc);
            let _ = std::fs::write(&ass, doc.script.to_ass_string_filtered(|d| d.end_time >= lo && d.start_time <= hi, true));
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
        // Oynatma kareleri dokuyu yerinde günceller: önbellekteki bir kareyle paylaşılan dokuyu bozmasın.
        self.tex = None;
        self.frame_shown = (-1, u64::MAX);
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
                let sp = self.play_opts.speed.clamp(0.25, 4.0);
                // ses aygıtı yoksa saat (hızla ölçekli): oynatma aralığının sonunu geçmesin
                self.playhead = self.play_from + ((begin.elapsed().as_secs_f64() * 1000.0 * sp) as i64).min(self.play_dur);
                (clock.elapsed().as_secs_f64() * sp, (begin.elapsed().as_millis() as f64 * sp) as i64 > self.play_dur + 800)
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
        if ended || (begin.elapsed().as_millis() as f64 * self.play_opts.speed.clamp(0.25, 4.0)) as i64 > self.play_dur + 3000 {
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
                    // Kare hızı artık belli: etkin satırın ilk görünen karesine yeniden konumlan
                    self.last_active = usize::MAX;
                }
                Msg::Info(Err(e)) => self.status = format!("Ortam açılamadı: {}", e),
                Msg::Dummy(key, Ok(p)) => {
                    self.open(p, ctx);
                    self.dummy = Some(key);
                }
                Msg::Dummy(_, Err(e)) => self.status = format!("Sahte video: {}", e),
                Msg::Picked(Ok(c)) => {
                    self.status = format!("Pipet: {}", osisub_core::color::Color { r: c[0], g: c[1], b: c[2], a: 0 }.to_tag_string());
                    crate::color_ui::deliver(ctx, c);
                }
                Msg::Picked(Err(e)) => {
                    self.status = format!("Pipet: {}", e);
                    crate::color_ui::cancel(ctx);
                }
                Msg::Wave(Ok(w)) => self.wave = Some(Arc::new(w)),
                Msg::Keyframes(k) => self.keyframes = k,
                Msg::Vfr(fr) => {
                    self.last_active = usize::MAX;
                    // Kullanıcının elle yüklediği timecode varsa ona dokunma
                    if self.timecodes.is_none() || self.timecodes_auto {
                        self.status = format!("Değişken kare hızı (VFR) algılandı: ortalama {:.3} fps — kare hesapları videonun zaman damgalarını kullanıyor", fr.fps());
                        self.timecodes = Some(Arc::new(fr));
                        self.timecodes_auto = true;
                    }
                }
                Msg::Wave(Err(e)) => self.status = format!("Ses okunamadı: {}", e),
                Msg::Frame { time, rev, first, res } => {
                    self.frame_busy = false;
                    match res {
                        Ok(frames) => {
                            // Önbellek anahtarları: istenen zaman + çevredeki karelerin tam başlangıç zamanları
                            // (kare adımı playhead'i tam kare başlangıcına koyar → ileri/geri adım önbellekten gelir).
                            let fr = self.framerate();
                            let base = first;
                            let shown = match (&fr, first) { (Some(f), Some(b)) => (f.frame_at(time) - b).max(0) as usize, _ => 0 };
                            for (i, f) in frames.iter().enumerate() {
                                let img = egui::ColorImage::from_rgba_unmultiplied([f.width as usize, f.height as usize], &f.rgba);
                                let tex = ctx.load_texture("frame", img, egui::TextureOptions::LINEAR);
                                let mut keys = vec![];
                                if i == shown.min(frames.len() - 1) {
                                    keys.push(time);
                                    self.tex = Some(tex.clone());
                                    self.frame_shown = (time, rev);
                                }
                                if let (Some(fr), Some(b)) = (&fr, base) {
                                    let k = fr.time_at(b + i as i64);
                                    if !keys.contains(&k) { keys.push(k); }
                                }
                                for k in keys {
                                    self.frame_cache.retain(|(key, _)| *key != (k, rev));
                                    self.frame_cache.push_back(((k, rev), tex.clone()));
                                }
                            }
                            while self.frame_cache.len() > 48 {
                                self.frame_cache.pop_front();
                            }
                        }
                        Err(e) => {
                            self.status = e;
                            self.frame_shown = (time, rev);
                        }
                    }
                }
            }
        }
        if self.rev_seen != doc.rev {
            self.rev_seen = doc.rev;
            self.rev_changed_at = std::time::Instant::now();
        }
        // Belge değiştiyse yazma bitene kadar (150 ms) bekle; yalnızca playhead değiştiyse hemen çiz.
        let settling = self.frame_shown.1 != doc.rev && self.rev_changed_at.elapsed() < std::time::Duration::from_millis(150);
        if settling {
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }
        // Önbellekte varsa ffmpeg çalıştırmadan göster.
        if !settling && self.frame_shown != (self.playhead, doc.rev) && !self.is_playing() {
            if let Some((_, t)) = self.frame_cache.iter().find(|(k, _)| *k == (self.playhead, doc.rev)) {
                self.tex = Some(t.clone());
                self.frame_shown = (self.playhead, doc.rev);
            }
        }
        if let (Some(p), Some(info)) = (self.path.clone(), self.render_info()) {
            if info.has_video && !self.frame_busy && !settling && !self.is_playing() && self.frame_shown != (self.playhead, doc.rev) {
                self.frame_busy = true;
                let (t, rev) = (self.playhead, doc.rev);
                let ass = self.tmp_dir.join("preview.ass");
                let t0 = self.playhead;
                self.sync_preview_fonts(doc);
                let _ = std::fs::write(&ass, doc.script.to_ass_string_filtered(|d| d.start_time <= t0 + 1500 && d.end_time >= t0 - 1000, true));
                let tx = self.tx.clone();
                let ctx2 = ctx.clone();
                // Playhead'de ekranda olan kare (mpv/Aegisub gibi): o karenin başlangıcından iste.
                // Playhead'deki kare (mpv/Aegisub gibi) ve çevresi: 3 kare öncesinden 5 kare sonrasına
                let (t_render, first, count) = match self.framerate() {
                    Some(fr) => {
                        let f0 = fr.frame_at(t);
                        let b = (f0 - 3).max(0);
                        (fr.time_at(b), Some(b), (f0 - b) as usize + 6)
                    }
                    None => (t, None, 6),
                };
                std::thread::spawn(move || {
                    let res = osisub_media::render_frames(&p, Some(&ass), t_render, count, 960, &info);
                    let _ = tx.send(Msg::Frame { time: t, rev, first, res });
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
                // tam kare zamanı yerine ASS-güvenli sınır (yoksa satır sahne değişiminden bir kare sonra başlayabilir)
                return self.framerate().map_or(k, |fr| fr.time_at_start(fr.frame_at(k)));
            }
        }
        match (self.snap, self.framerate()) {
            (true, Some(fr)) => fr.snap(ms),
            _ => ms,
        }
    }

    /// Playhead'i başlangıç/bitiş olarak ayarlar (kısayollar için).
    /// F6: satır videoda görünen karede başlasın (Aegisub: TimeAtFrame(kare, START)).
    pub fn set_active_start(&self, doc: &mut Document) {
        if let Some(Entry::Dialogue(d)) = doc.script.entries.get(doc.active).cloned() {
            let t = self.framerate().map_or(self.playhead, |fr| fr.time_at_start(fr.frame_at(self.playhead)));
            doc.set_times(doc.active, t.min(d.end_time), d.end_time);
        }
    }

    /// F7: satır videoda görünen kare DAHİL görünsün (Aegisub: TimeAtFrame(kare, END)).
    pub fn set_active_end(&self, doc: &mut Document) {
        if let Some(Entry::Dialogue(d)) = doc.script.entries.get(doc.active).cloned() {
            let t = self.framerate().map_or(self.playhead, |fr| fr.time_at_end(fr.frame_at(self.playhead)));
            doc.set_times(doc.active, d.start_time, t.max(d.start_time));
        }
    }

    /// Paneli çizer. Etkin satır değişirse playhead satır başına gider.
    pub fn ui(&mut self, ui: &mut egui::Ui, doc: &mut Document) {
        let ctx = ui.ctx().clone();
        self.pump(&ctx, doc);
        if self.last_active != doc.active {
            self.last_active = doc.active;
            if let Some(Entry::Dialogue(d)) = doc.script.entries.get(doc.active) {
                // Aegisub gibi: satırın göründüğü ilk kareye git (kare hızı yoksa biraz içeri)
                self.playhead = match self.framerate() {
                    Some(fr) => fr.time_at(fr.frame_at_start(d.start_time)),
                    None => d.start_time + 20,
                };
                let (w_ms, _) = (self.ms_per_px * 600.0, 0);
                if (d.start_time as f64) < self.view_start || (d.end_time as f64) > self.view_start + w_ms {
                    self.view_start = (d.start_time as f64 - w_ms * 0.25).max(0.0);
                }
            }
        }
        if self.detached && self.info.as_ref().map_or(false, |i| i.has_video) {
            // video ayrı pencerede: panelde yalnızca dalga formu
            self.wave_ui(ui, doc);
            return;
        }
        ui.horizontal_top(|ui| {
            let h = ui.available_height();
            // Video
            let vid_w = (h * 16.0 / 9.0).min(ui.available_width() * 0.45);
            self.video_ui(ui, doc, vid_w, h);
            // Dalga formu
            self.wave_ui(ui, doc);
        });
    }

    /// Video önizlemesi ve video araçları (`w`×`h` alanında; ayrı pencerede de kullanılır).
    pub fn video_ui(&mut self, ui: &mut egui::Ui, doc: &mut Document, vid_w: f32, h: f32) {
            let mut img_rect: Option<(Rect, Rect)> = None;
        ui.allocate_ui(egui::vec2(vid_w, h), |ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(vid_w, h), Sense::hover());
            ui.painter().rect_filled(rect, 4.0, Color32::BLACK);
            // Yakınlaştır/kaydır: Ctrl+tekerlek (imleç etrafında), orta tuşla sürükle; orta çift tık = sıfırla
            let hover = ui.rect_contains_pointer(rect);
            if hover {
                let (zoom_d, scroll, ctrl, mid_down, mid_dbl, delta, pos) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta.y, i.modifiers.command, i.pointer.middle_down(), i.pointer.button_double_clicked(egui::PointerButton::Middle), i.pointer.delta(), i.pointer.hover_pos()));
                let f = if zoom_d != 1.0 { zoom_d.clamp(0.85, 1.18) } else if ctrl && scroll != 0.0 { (1.0 + scroll * 0.003).clamp(0.85, 1.18) } else { 1.0 };
                if f != 1.0 {
                    let nz = (self.video_zoom * f).clamp(1.0, 16.0);
                    let f = nz / self.video_zoom;
                    if let Some(c) = pos {
                        let center = rect.center() + self.video_pan;
                        let nc = c - (c - center) * f;
                        self.video_pan = nc - rect.center();
                    }
                    self.video_zoom = nz;
                    if nz == 1.0 { self.video_pan = egui::Vec2::ZERO; }
                }
                if mid_down && delta != egui::Vec2::ZERO && self.video_zoom > 1.0 {
                    self.video_pan += delta;
                }
                if mid_dbl {
                    self.video_zoom = 1.0;
                    self.video_pan = egui::Vec2::ZERO;
                }
            }
            if let Some(t) = &self.tex {
                let sz = t.size_vec2();
                let scale = (rect.width() / sz.x).min(rect.height() / sz.y) * self.video_zoom;
                let img = Rect::from_center_size(rect.center() + self.video_pan, sz * scale);
                ui.painter_at(rect).image(t.id(), img, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                img_rect = Some((img, img.intersect(rect)));
                if self.safe_area {
                    let p = ui.painter_at(rect);
                    for (k, a) in [(0.9, 150u8), (0.8, 110u8)] {
                        let r = Rect::from_center_size(img.center(), img.size() * k);
                        p.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, a)), egui::StrokeKind::Middle);
                    }
                    let c = img.center();
                    p.line_segment([c - egui::vec2(10.0, 0.0), c + egui::vec2(10.0, 0.0)], Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 120)));
                    p.line_segment([c - egui::vec2(0.0, 10.0), c + egui::vec2(0.0, 10.0)], Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 120)));
                }
                if self.video_zoom > 1.0 {
                    ui.painter().text(rect.right_top() + egui::vec2(-6.0, 6.0), egui::Align2::RIGHT_TOP, format!("{:.0}%", self.video_zoom * 100.0), egui::FontId::monospace(12.0), Color32::from_rgba_unmultiplied(255, 255, 255, 200));
                }
            } else {
                let msg = if self.info.as_ref().map_or(false, |i| !i.has_video) { "Yalnızca ses" } else if self.is_open() { "Kare yükleniyor…" } else { "Video yok" };
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, msg, egui::FontId::proportional(14.0), Color32::GRAY);
            }
            ui.painter().text(rect.left_bottom() + egui::vec2(6.0, -6.0), egui::Align2::LEFT_BOTTOM, match &self.timecodes { Some(t) => format!("{}  kare {}  VFR{}", time::format_ass(self.playhead), t.frame_at(self.playhead), if self.timecodes_auto { "" } else { " (timecode)" }), None => time::format_ass(self.playhead) }, egui::FontId::monospace(12.0), Color32::from_rgba_unmultiplied(255, 255, 255, 190));
        });
        if crate::color_ui::eyedropper_waiting(ui.ctx()) {
            // Pipet kipi: tıklama video araçlarına gitmez
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                crate::color_ui::cancel(ui.ctx());
            } else if let Some((img, vis)) = img_rect {
                let resp = ui.interact(vis, egui::Id::unique("eyedropper"), Sense::click());
                let resp = resp.on_hover_cursor(egui::CursorIcon::Crosshair);
                if let (true, Some(p)) = (resp.clicked(), resp.interact_pointer_pos()) {
                    self.pick_color(ui.ctx(), ((p.x - img.left()) / img.width()).clamp(0.0, 1.0), ((p.y - img.top()) / img.height()).clamp(0.0, 1.0));
                }
            }
            return;
        }
        if let (Some((img, vis)), true) = (img_rect, self.tool != VideoTool::None) {
            self.tool_ui(ui, img, vis, doc);
        }
    }

    /// Pipet: o anki karenin (altyazısız, tam çözünürlük) göreli (u, v) noktasındaki rengi arka planda okur.
    fn pick_color(&mut self, ctx: &egui::Context, u: f32, v: f32) {
        let (Some(path), Some(info)) = (self.path.clone(), self.info.clone()) else { return };
        let (t, tx, ctx2) = (self.playhead, self.tx.clone(), ctx.clone());
        self.status = "Pipet: renk okunuyor…".into();
        std::thread::spawn(move || {
            let r = osisub_media::render_frame(&path, None, t, info.width.max(16), &info).map(|f| {
                let (w, h) = (f.width as usize, f.height as usize);
                let x = ((u * w as f32) as usize).min(w.saturating_sub(1));
                let y = ((v * h as f32) as usize).min(h.saturating_sub(1));
                crate::color_ui::sample(&f.rgba, w, h, x, y)
            });
            let _ = tx.send(Msg::Picked(r));
            ctx2.request_repaint();
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
    /// `img`: videonun ekrandaki tam dikdörtgeni (yakınlaştırmada panelden taşabilir), `vis`: görünen kısmı.
    fn tool_ui(&mut self, ui: &mut egui::Ui, img: Rect, vis: Rect, doc: &mut Document) {
        let at = doc.active;
        let Some(Entry::Dialogue(d)) = doc.script.entries.get(at).cloned() else { return };
        let (rx, ry) = doc.script.res();
        let (rx, ry) = (rx as f32, ry as f32);
        let to_screen = |x: f32, y: f32| Pos2::new(img.left() + x / rx * img.width(), img.top() + y / ry * img.height());
        let to_script = |p: Pos2| ((p.x - img.left()) / img.width() * rx, (p.y - img.top()) / img.height() * ry);
        let col = Color32::from_rgb(0xe5, 0x6b, 0x9a);
        let p = ui.painter_at(vis);
        let cur = Self::current_pos(doc, &d);
        let c = to_screen(cur.0, cur.1);
        let r = ui.interact(vis, egui::Id::unique("video_tool"), Sense::click_and_drag());
        let ptr = r.interact_pointer_pos();
        let press = ui.input(|i| i.pointer.press_origin());
        if r.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        match self.tool {
            VideoTool::None => {}
            VideoTool::Pos => {
                if osisub_core::overrides::get_initial(&d.text, "move").is_some() {
                    p.text(vis.left_top() + egui::vec2(6.0, 6.0), egui::Align2::LEFT_TOP, "\\move var: konum aracı devre dışı", egui::FontId::proportional(12.0), Color32::from_rgb(0xe0, 0xa8, 0x4a));
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
            VideoTool::Move => {
                use osisub_core::overrides::Move;
                let mv = Move::get(&d.text).unwrap_or(Move { from: (cur.0 as f64, cur.1 as f64), to: (cur.0 as f64, cur.1 as f64), times: None });
                let (a, b) = (to_screen(mv.from.0 as f32, mv.from.1 as f32), to_screen(mv.to.0 as f32, mv.to.1 as f32));
                p.line_segment([a, b], Stroke::new(1.5, col));
                if a.distance(b) > 12.0 {
                    // ok başı
                    let dir = (b - a).normalized();
                    let n = egui::vec2(-dir.y, dir.x);
                    p.line_segment([b, b - dir * 12.0 + n * 6.0], Stroke::new(1.5, col));
                    p.line_segment([b, b - dir * 12.0 - n * 6.0], Stroke::new(1.5, col));
                }
                p.circle_stroke(a, 8.0, Stroke::new(2.0, col));
                p.rect_stroke(Rect::from_center_size(b, egui::vec2(14.0, 14.0)), 1.0, Stroke::new(2.0, Color32::from_rgb(0x6b, 0xc5, 0xe5)), egui::StrokeKind::Middle);
                let times = mv.times.map(|(t1, t2)| format!(" ({}–{} ms)", t1, t2)).unwrap_or_default();
                p.text(vis.left_top() + egui::vec2(6.0, 6.0), egui::Align2::LEFT_TOP, format!("\\move {:.0},{:.0} → {:.0},{:.0}{}", mv.from.0, mv.from.1, mv.to.0, mv.to.1, times), egui::FontId::monospace(11.0), col);
                p.text(vis.left_top() + egui::vec2(6.0, 22.0), egui::Align2::LEFT_TOP, "daire: başlangıç · kare: bitiş · boş alan: ikisi · sağ tık: \\pos", egui::FontId::proportional(11.0), Color32::from_rgba_unmultiplied(255, 255, 255, 170));
                if r.drag_started() {
                    // drag_started eşik aşıldıktan sonra gelir: isabet testi için basma noktası kullanılır.
                    if let Some(pos) = press.or(ptr) {
                        // mod 1 = başlangıç, 2 = bitiş, 3 = ikisi birden (tutulan nokta = ilk tıklama)
                        let mode = if b.distance(pos) < 12.0 { 2 } else if a.distance(pos) < 12.0 { 1 } else { 3 };
                        self.vt_drag = Some((mode, to_script(pos)));
                        self.vt_move0 = Some(mv);
                    }
                }
                if r.dragged() {
                    if let (Some((mode, anchor)), Some(m0), Some(pos)) = (self.vt_drag, self.vt_move0, ptr) {
                        let (x, y) = to_script(pos);
                        let (x, y) = (x.clamp(0.0, rx) as f64, y.clamp(0.0, ry) as f64);
                        let mut m = m0;
                        match mode {
                            1 => m.from = (x.round(), y.round()),
                            2 => m.to = (x.round(), y.round()),
                            _ => {
                                let (dx, dy) = ((x - anchor.0 as f64).round(), (y - anchor.1 as f64).round());
                                m.from = (m0.from.0 + dx, m0.from.1 + dy);
                                m.to = (m0.to.0 + dx, m0.to.1 + dy);
                            }
                        }
                        let nt = m.apply(&d.text);
                        if nt != d.text {
                            let key = format!("vt:move:{}", at);
                            doc.edit_keyed("Hareket", Some(&key), |tx| tx.dialogue_mut(at, |dd| dd.text = nt));
                        }
                    }
                }
                if r.drag_stopped() {
                    self.vt_drag = None;
                    self.vt_move0 = None;
                }
                if r.secondary_clicked() {
                    let nt = Move::to_pos(&d.text);
                    if nt != d.text {
                        doc.edit("\\move → \\pos", |tx| tx.dialogue_mut(at, |dd| dd.text = nt));
                    }
                }
            }
            VideoTool::VClip => {
                use osisub_core::drawing;
                // Mevcut vektör kırpma: \clip(çizim) ya da \clip(ölçek,çizim); \iclip korunur
                let (tag_name, existing) = ["clip", "iclip"].iter().find_map(|n| osisub_core::overrides::get_initial(&d.text, n).map(|t| (*n, t))).map(|(n, t)| (n, Some(t))).unwrap_or(("clip", None));
                let (scale, mut cmds) = match &existing {
                    Some(t) => {
                        let a = t.args();
                        let draw_arg = a.last().map(|s| s.trim().to_string()).unwrap_or_default();
                        if draw_arg.chars().any(|c| c.is_ascii_alphabetic()) {
                            let sc = if a.len() == 2 { a[0].trim().parse::<i32>().unwrap_or(1).max(1) } else { 1 };
                            (sc, drawing::parse(&draw_arg))
                        } else {
                            (1, vec![]) // dikdörtgen kırpma: vektör aracı yeni çizim başlatır
                        }
                    }
                    None => (1, vec![]),
                };
                let mul = 2f64.powi(scale - 1);
                let scr = |p: (f64, f64)| to_screen((p.0 / mul) as f32, (p.1 / mul) as f32);
                for shape in drawing::polylines(&cmds) {
                    let pts: Vec<Pos2> = shape.iter().map(|&p| scr(p)).collect();
                    for w in pts.windows(2) {
                        p.line_segment([w[0], w[1]], Stroke::new(1.5, col));
                    }
                    if pts.len() > 2 {
                        p.line_segment([*pts.last().unwrap(), pts[0]], Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xe5, 0x6b, 0x9a, 120)));
                    }
                }
                let handles = drawing::points(&cmds);
                for (ci, _, q) in &handles {
                    let s = scr(*q);
                    let is_ctrl = cmds[*ci].kind == 'b';
                    if is_ctrl { p.circle_stroke(s, 4.0, Stroke::new(1.5, Color32::from_rgb(0x6b, 0xc5, 0xe5))); } else { p.rect_filled(Rect::from_center_size(s, egui::vec2(7.0, 7.0)), 1.0, col); }
                }
                p.text(vis.left_top() + egui::vec2(6.0, 6.0), egui::Align2::LEFT_TOP, format!("\\{} vektör: {} nokta · tıkla: nokta ekle · Ctrl+tık: eğri · sürükle: taşı · sağ tık: noktayı sil (boşta: kırpmayı kaldır)", tag_name, handles.len()), egui::FontId::proportional(11.0), Color32::from_rgba_unmultiplied(255, 255, 255, 190));
                let hit = |q: Pos2| handles.iter().find(|(_, _, h)| scr(*h).distance(q) < 9.0).map(|(ci, pi, _)| (*ci, *pi));
                let mut changed = false;
                if r.drag_started() {
                    self.vt_vdrag = press.or(ptr).and_then(hit);
                }
                if r.dragged() {
                    if let (Some((ci, pi)), Some(pos)) = (self.vt_vdrag, ptr) {
                        let (x, y) = to_script(pos);
                        if let Some(pt) = cmds.get_mut(ci).and_then(|c| c.pts.get_mut(pi)) {
                            *pt = ((x as f64 * mul).round(), (y as f64 * mul).round());
                            changed = true;
                        }
                    }
                }
                if r.drag_stopped() {
                    self.vt_vdrag = None;
                }
                if r.clicked() {
                    if let Some(pos) = ptr {
                        match hit(pos) {
                            None => {
                                let (x, y) = to_script(pos);
                                drawing::push_point(&mut cmds, ((x as f64 * mul).round(), (y as f64 * mul).round()));
                                changed = true;
                            }
                            // Ctrl+tık: bu noktayla biten düz parçayı eğriye çevir
                            Some((ci, pi)) if ui.input(|i| i.modifiers.command) => changed |= drawing::line_to_bezier(&mut cmds, ci, pi),
                            _ => {}
                        }
                    }
                }
                if r.secondary_clicked() {
                    match ptr.and_then(hit) {
                        Some((ci, pi)) => { drawing::remove_point(&mut cmds, ci, pi); changed = true; }
                        None => {
                            let nt = osisub_core::overrides::remove_initial(&d.text, tag_name);
                            if nt != d.text { doc.edit("Vektör kırpmayı kaldır", |tx| tx.dialogue_mut(at, |dd| dd.text = nt)); }
                        }
                    }
                }
                if changed {
                    let body = drawing::to_string(&cmds);
                    let nt = if body.is_empty() {
                        osisub_core::overrides::remove_initial(&d.text, tag_name)
                    } else {
                        let arg = if scale > 1 { format!("({},{})", scale, body) } else { format!("({})", body) };
                        osisub_core::overrides::set_initial(&d.text, tag_name, &arg)
                    };
                    if nt != d.text {
                        let key = format!("vt:vclip:{}", at);
                        doc.edit_keyed("Vektör kırpma", Some(&key), |tx| tx.dialogue_mut(at, |dd| dd.text = nt));
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
                    // drag_started eşik aşıldıktan sonra gelir: isabet testi için basma noktası kullanılır.
                    if let Some(pos) = press.or(ptr) {
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
                p.circle_stroke(o, 9.0, Stroke::new(1.5, Color32::WHITE));
                p.text(o + egui::vec2(8.0, 8.0), egui::Align2::LEFT_TOP, format!("frz {:.1}°", frz), egui::FontId::monospace(11.0), col);
                p.text(vis.left_top() + egui::vec2(6.0, 6.0), egui::Align2::LEFT_TOP, "tutamaç: açı (Shift = 15°) · beyaz nokta: \\org taşı · sağ tık: \\frz kaldır", egui::FontId::proportional(11.0), Color32::from_rgba_unmultiplied(255, 255, 255, 170));
                if r.drag_started() {
                    // 1 = \org taşı, 0 = açı
                    let on_org = press.or(ptr).map_or(false, |q| q.distance(o) < 10.0);
                    self.vt_drag = Some((if on_org { 1 } else { 0 }, org));
                }
                if r.drag_stopped() {
                    self.vt_drag = None;
                }
                if r.dragged() && matches!(self.vt_drag, Some((1, _))) {
                    if let Some(pos) = ptr {
                        let (x, y) = to_script(pos);
                        Self::set_tag(doc, at, &d.text, "org", &format!("({},{})", x.round(), y.round()), "Döndürme merkezi");
                    }
                } else if r.dragged() {
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
            VideoTool::RotateXY => {
                let get = |n: &str| osisub_core::overrides::get_initial(&d.text, n).and_then(|t| t.number()).unwrap_or(0.0) as f32;
                let (frx, fry) = (get("frx"), get("fry"));
                let org = osisub_core::overrides::get_initial(&d.text, "org").map(|t| t.args()).and_then(|a| Some((a.first()?.trim().parse::<f32>().ok()?, a.get(1)?.trim().parse::<f32>().ok()?))).unwrap_or(cur);
                let o = to_screen(org.0, org.1);
                // Döndürülmüş bir kare taslağı: frx dikey, fry yatay sıkıştırma olarak gösterilir
                let (w, h) = (60.0 * fry.to_radians().cos().abs().max(0.05), 40.0 * frx.to_radians().cos().abs().max(0.05));
                p.rect_stroke(Rect::from_center_size(o, egui::vec2(w * 2.0, h * 2.0)), 0.0, Stroke::new(1.5, col), egui::StrokeKind::Middle);
                p.line_segment([o - egui::vec2(80.0, 0.0), o + egui::vec2(80.0, 0.0)], Stroke::new(1.0, Color32::from_rgba_unmultiplied(0x6b, 0xc5, 0xe5, 160)));
                p.line_segment([o - egui::vec2(0.0, 60.0), o + egui::vec2(0.0, 60.0)], Stroke::new(1.0, Color32::from_rgba_unmultiplied(0x6b, 0xc5, 0xe5, 160)));
                p.circle_filled(o, 4.0, Color32::WHITE);
                p.text(o + egui::vec2(8.0, 8.0), egui::Align2::LEFT_TOP, format!("frx {:.1}°  fry {:.1}°", frx, fry), egui::FontId::monospace(11.0), col);
                p.text(vis.left_top() + egui::vec2(6.0, 6.0), egui::Align2::LEFT_TOP, "yatay sürükle: \\fry · dikey: \\frx · Shift: 15° adım · sağ tık: sıfırla", egui::FontId::proportional(11.0), Color32::from_rgba_unmultiplied(255, 255, 255, 170));
                if r.drag_started() {
                    self.vt_drag = Some((0, (frx, fry)));
                }
                if r.dragged() {
                    if let (Some((_, (x0, y0))), Some(origin), Some(pos)) = (self.vt_drag, press, ptr) {
                        let dv = pos - origin;
                        let snap = ui.input(|i| i.modifiers.shift);
                        let q = |v: f32| { let v = if snap { (v / 15.0).round() * 15.0 } else { (v * 10.0).round() / 10.0 }; format!("{}", v) };
                        let ny = y0 + dv.x * 0.5;
                        let nx = x0 - dv.y * 0.5;
                        let t1 = osisub_core::overrides::set_initial(&d.text, "fry", &q(ny));
                        let t2 = osisub_core::overrides::set_initial(&t1, "frx", &q(nx));
                        if t2 != d.text {
                            let key = format!("vt:frxy:{}", at);
                            doc.edit_keyed("3B döndür", Some(&key), |tx| tx.dialogue_mut(at, |dd| dd.text = t2));
                        }
                    }
                }
                if r.drag_stopped() {
                    self.vt_drag = None;
                }
                if r.secondary_clicked() {
                    let nt = osisub_core::overrides::remove_initial(&osisub_core::overrides::remove_initial(&d.text, "frx"), "fry");
                    if nt != d.text {
                        doc.edit("3B dönmeyi kaldır", |tx| tx.dialogue_mut(at, |dd| dd.text = nt));
                    }
                }
            }
            VideoTool::Drawing => {
                use osisub_core::{drawing, overrides as ov};
                let pscale = ov::get_initial(&d.text, "p").and_then(|t| t.number()).unwrap_or(0.0);
                let warn = |msg: &str| { p.text(vis.left_top() + egui::vec2(6.0, 6.0), egui::Align2::LEFT_TOP, msg, egui::FontId::proportional(12.0), Color32::from_rgb(0xe0, 0xa8, 0x4a)); };
                if pscale <= 0.0 { warn("Çizim aracı: satır \\p1 çizimi değil"); return; }
                let num = |n: &str| ov::get_initial(&d.text, n).and_then(|t| t.number());
                let st = doc.script.style_by_name(&d.style).cloned().unwrap_or_default();
                let an = num("an").map(|v| v as i32).unwrap_or(st.align);
                let rot = num("frz").or(num("fr")).unwrap_or(st.angle) != 0.0 || num("frx").unwrap_or(0.0) != 0.0 || num("fry").unwrap_or(0.0) != 0.0;
                if an != 7 || rot { warn("Çizim aracı yalnızca \\an7 ve döndürmesiz çizimlerde çalışır"); return; }
                let k = 1.0 / 2f64.powf(pscale - 1.0);
                let (sx, sy) = (num("fscx").unwrap_or(st.scale_x) / 100.0 * k, num("fscy").unwrap_or(st.scale_y) / 100.0 * k);
                let (px, py) = (cur.0 as f64, cur.1 as f64);
                // çizim metni: \p bloğundan sonraki ilk metin parçası
                let segs = ov::parse(&d.text);
                let Some(ti) = segs.iter().position(|g| matches!(g, ov::Seg::Text(t) if !t.trim().is_empty())) else { warn("Çizim boş"); return; };
                let ov::Seg::Text(body) = &segs[ti] else { return };
                let mut cmds = drawing::parse(body);
                let scr = |q: (f64, f64)| to_screen((px + q.0 * sx) as f32, (py + q.1 * sy) as f32);
                let back = |s: Pos2| { let (x, y) = to_script(s); (((x as f64 - px) / sx * 2.0).round() / 2.0, ((y as f64 - py) / sy * 2.0).round() / 2.0) };
                for shape in drawing::polylines(&cmds) {
                    let pts: Vec<Pos2> = shape.iter().map(|&q| scr(q)).collect();
                    for w in pts.windows(2) { p.line_segment([w[0], w[1]], Stroke::new(1.5, col)); }
                    if pts.len() > 2 { p.line_segment([*pts.last().unwrap(), pts[0]], Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xe5, 0x6b, 0x9a, 120))); }
                }
                let handles = drawing::points(&cmds);
                for (ci, _, q) in &handles {
                    let s = scr(*q);
                    if cmds[*ci].kind == 'b' { p.circle_stroke(s, 4.0, Stroke::new(1.5, Color32::from_rgb(0x6b, 0xc5, 0xe5))); } else { p.rect_filled(Rect::from_center_size(s, egui::vec2(7.0, 7.0)), 1.0, col); }
                }
                p.text(vis.left_top() + egui::vec2(6.0, 6.0), egui::Align2::LEFT_TOP, format!("\\p çizim: {} nokta · tıkla: ekle · Ctrl+tık: eğri · sürükle: taşı · sağ tık: sil", handles.len()), egui::FontId::proportional(11.0), Color32::from_rgba_unmultiplied(255, 255, 255, 190));
                let hit = |q: Pos2| handles.iter().find(|(_, _, h)| scr(*h).distance(q) < 9.0).map(|(ci, pi, _)| (*ci, *pi));
                let mut changed = false;
                if r.drag_started() { self.vt_vdrag = press.or(ptr).and_then(hit); }
                if r.dragged() {
                    if let (Some((ci, pi)), Some(pos)) = (self.vt_vdrag, ptr) {
                        if let Some(pt) = cmds.get_mut(ci).and_then(|c| c.pts.get_mut(pi)) { *pt = back(pos); changed = true; }
                    }
                }
                if r.drag_stopped() { self.vt_vdrag = None; }
                if r.clicked() {
                    if let Some(pos) = ptr {
                        match hit(pos) {
                            None => { drawing::push_point(&mut cmds, back(pos)); changed = true; }
                            Some((ci, pi)) if ui.input(|i| i.modifiers.command) => changed |= drawing::line_to_bezier(&mut cmds, ci, pi),
                            _ => {}
                        }
                    }
                }
                if r.secondary_clicked() {
                    if let Some((ci, pi)) = ptr.and_then(hit) { drawing::remove_point(&mut cmds, ci, pi); changed = true; }
                }
                if changed {
                    let mut segs2 = segs.clone();
                    segs2[ti] = ov::Seg::Text(drawing::to_string(&cmds));
                    let nt = ov::to_string(&segs2);
                    if nt != d.text {
                        let key = format!("vt:draw:{}", at);
                        doc.edit_keyed("Çizim", Some(&key), |tx| tx.dialogue_mut(at, |dd| dd.text = nt));
                    }
                }
            }
            VideoTool::Perspective => {
                use osisub_core::{overrides as ov, perspective as pp};
                let Some((bw, bh)) = self.persp_box.as_ref().filter(|b| b.0 == at && b.1 == d.text).map(|b| b.2) else {
                    p.text(vis.left_top() + egui::vec2(6.0, 6.0), egui::Align2::LEFT_TOP, "Perspektif: satır kutusu ölçülemedi (yazı tipi bulunamadı ya da boş satır)", egui::FontId::proportional(12.0), Color32::from_rgb(0xe0, 0xa8, 0x4a));
                    return;
                };
                let dist = self.persp_dist;
                let num = |n: &str| ov::get_initial(&d.text, n).and_then(|t| t.number());
                let pair = |n: &str| ov::get_initial(&d.text, n).map(|t| t.args()).and_then(|a| Some((a.first()?.trim().parse::<f64>().ok()?, a.get(1)?.trim().parse::<f64>().ok()?)));
                // dörtgen: son sürüklemeden, yoksa satırın perspektif etiketlerinden, yoksa 4 noktalı kırpmadan, yoksa konum çevresinde kutu
                let quad: [(f64, f64); 4] = match &self.vt_quad {
                    Some((l, t, q)) if *l == at && *t == d.text => *q,
                    _ => {
                        let an7 = num("an") == Some(7.0);
                        if let (true, Some(pos), Some(org)) = (an7, pair("pos"), pair("org")) {
                            let pr = pp::Params { pos, frx: num("frx").unwrap_or(0.0), fry: num("fry").unwrap_or(0.0), frz: num("frz").or(num("fr")).unwrap_or(0.0), fscx: num("fscx").unwrap_or(100.0), fscy: num("fscy").unwrap_or(100.0), fax: num("fax").unwrap_or(0.0) };
                            pp::corners(&pr, org, dist, bw, bh)
                        } else if let Some(q) = ov::get_initial(&d.text, "clip").and_then(|t| t.args().last().cloned()).and_then(|dr| pp::quad_from_clip(&dr)) {
                            q
                        } else {
                            let (cx, cy) = (cur.0 as f64, cur.1 as f64);
                            [(cx - bw / 2.0, cy - bh / 2.0), (cx + bw / 2.0, cy - bh / 2.0), (cx + bw / 2.0, cy + bh / 2.0), (cx - bw / 2.0, cy + bh / 2.0)]
                        }
                    }
                };
                let sq: Vec<Pos2> = quad.iter().map(|q| to_screen(q.0 as f32, q.1 as f32)).collect();
                for i in 0..4 {
                    p.line_segment([sq[i], sq[(i + 1) % 4]], Stroke::new(1.5, col));
                }
                // iç ızgara (perspektifi görmek için): kenar orta noktaları
                let mid = |a: Pos2, b: Pos2| a + (b - a) * 0.5;
                let thin = Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xe5, 0x6b, 0x9a, 110));
                p.line_segment([mid(sq[0], sq[1]), mid(sq[3], sq[2])], thin);
                p.line_segment([mid(sq[0], sq[3]), mid(sq[1], sq[2])], thin);
                for (i, s) in sq.iter().enumerate() {
                    p.rect_filled(Rect::from_center_size(*s, egui::vec2(9.0, 9.0)), 1.0, if i == 0 { Color32::from_rgb(0x6b, 0xc5, 0xe5) } else { col });
                }
                p.text(vis.left_top() + egui::vec2(6.0, 6.0), egui::Align2::LEFT_TOP, "Perspektif: köşeleri sürükle (mavi = sol üst) · içeriden sürükle: taşı · sağ tık: perspektifi kaldır", egui::FontId::proportional(11.0), Color32::from_rgba_unmultiplied(255, 255, 255, 190));
                let inside = |q: Pos2| (0..4).all(|i| { let (a, b) = (sq[i], sq[(i + 1) % 4]); (b.x - a.x) * (q.y - a.y) - (b.y - a.y) * (q.x - a.x) >= 0.0 });
                if r.drag_started() {
                    if let Some(pos) = press.or(ptr) {
                        let hit = (0..4).find(|&i| sq[i].distance(pos) < 10.0).or_else(|| inside(pos).then_some(4));
                        let (x, y) = to_script(pos);
                        self.vt_qdrag = hit.map(|h| (h, quad, (x as f64, y as f64)));
                    }
                }
                if r.dragged() {
                    if let (Some((h, q0, anchor)), Some(pos)) = (self.vt_qdrag, ptr) {
                        let (x, y) = to_script(pos);
                        let (x, y) = (x as f64, y as f64);
                        let mut q = q0;
                        if h < 4 {
                            q[h] = ((x * 2.0).round() / 2.0, (y * 2.0).round() / 2.0);
                        } else {
                            let (dx, dy) = ((x - anchor.0).round(), (y - anchor.1).round());
                            for c in q.iter_mut() { *c = (c.0 + dx, c.1 + dy); }
                        }
                        if let Some((pr, org, _err)) = pp::fit(&q, bw, bh, dist) {
                            let mut text = d.text.clone();
                            for n in ["pos", "move", "org", "frx", "fry", "frz", "fr", "fscx", "fscy", "fax", "fay", "an", "a"] {
                                text = ov::remove_initial(&text, n);
                            }
                            for t in ov::parse_block(&pp::tags(&pr, org)) {
                                text = ov::set_initial(&text, &t.name, &t.arg);
                            }
                            if text != d.text {
                                let key = format!("vt:persp:{}", at);
                                let t2 = text.clone();
                                doc.edit_keyed("Perspektif", Some(&key), |tx| tx.dialogue_mut(at, |dd| dd.text = t2));
                                self.persp_need_layout = true;
                            }
                            self.vt_quad = Some((at, text, q));
                        }
                    }
                }
                if r.drag_stopped() {
                    self.vt_qdrag = None;
                }
                if r.secondary_clicked() {
                    let mut text = d.text.clone();
                    for n in ["org", "frx", "fry", "fax", "fay"] {
                        text = ov::remove_initial(&text, n);
                    }
                    if text != d.text {
                        doc.edit("Perspektifi kaldır", |tx| tx.dialogue_mut(at, |dd| dd.text = text));
                    }
                    self.vt_quad = None;
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
        // Karaoke kipi: \k heceleri dalga formunda; iç sınırlar sürüklenebilir (Aegisub karaoke modu)
        let (k_lead, k_syls) = osisub_core::karaoke::parse_with_lead(&d.text);
        let mut k_bounds: Vec<f32> = vec![]; // iç sınırların x konumları (i: i ile i+1 arası)
        if k_syls.len() > 1 {
            let mut t = d.start_time;
            let top = rect.top() + 4.0;
            for (i, s) in k_syls.iter().enumerate() {
                let t2 = t + s.dur * 10;
                let (xa, xb) = (x_of(t as f64, vs, mpp), x_of(t2 as f64, vs, mpp));
                if xb > rect.left() && xa < rect.right() && xb - xa > 8.0 {
                    p.text(Pos2::new((xa + xb) / 2.0, top + 14.0), egui::Align2::CENTER_TOP, osisub_core::tags::strip_tags(&s.text).trim(), egui::FontId::proportional(12.0), Color32::from_rgb(0xf0, 0xd0, 0x60));
                }
                if i + 1 < k_syls.len() {
                    k_bounds.push(xb);
                    if xb >= rect.left() && xb <= rect.right() {
                        p.line_segment([Pos2::new(xb, top), Pos2::new(xb, rect.bottom())], Stroke::new(1.0, Color32::from_rgb(0xf0, 0xd0, 0x60)));
                    }
                }
                t = t2;
            }
        }
        if resp.drag_started() {
            if let Some(pos) = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos()) {
                let syl = k_bounds.iter().enumerate().filter(|(_, x)| (pos.x - **x).abs() <= 5.0).min_by(|a, b| (pos.x - a.1).abs().total_cmp(&(pos.x - b.1).abs())).map(|(i, _)| i);
                self.drag = if (pos.x - xs).abs() <= 7.0 { Drag::Start } else if (pos.x - xe).abs() <= 7.0 { Drag::End } else if let Some(i) = syl { Drag::Syl(i) } else if pos.x > xs && pos.x < xe && ui.input(|i| i.modifiers.alt) { Drag::Body(ms_of(pos.x) as i64 - d.start_time) } else { Drag::None };
            }
        }
        // Karaoke: heceye çift tıkla → yalnızca o heceyi oynat
        if resp.double_clicked() && k_syls.len() > 1 {
            if let Some(pos) = resp.interact_pointer_pos() {
                let m = ms_of(pos.x) as i64;
                let mut t = d.start_time;
                for s in &k_syls {
                    let t2 = t + s.dur * 10;
                    if m >= t && m < t2 {
                        if self.is_playing() { self.stop_all(); }
                        self.play_range(doc, ui.ctx(), t, (t2 - t).max(50));
                        break;
                    }
                    t = t2;
                }
            }
        }
        if let (true, Drag::Syl(i)) = (resp.dragged(), self.drag) {
            if let Some(pos) = resp.interact_pointer_pos() {
                // sınırın şimdiki zamanı (cs) → yeni zaman; toplam süre korunur
                let cur: i64 = k_syls.iter().take(i + 1).map(|s| s.dur).sum();
                let want = ((ms_of(pos.x) as i64 - d.start_time) as f64 / 10.0).round() as i64;
                let mut v = k_syls.clone();
                osisub_core::karaoke::move_boundary(&mut v, i, want - cur);
                let nt = osisub_core::karaoke::build_with_lead(&k_lead, &v);
                if nt != d.text {
                    let key = format!("kara-wave:{}", act);
                    doc.edit_keyed("Hece sınırı", Some(&key), |tx| tx.dialogue_mut(act, |dd| dd.text = nt));
                }
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
        }
        if resp.dragged() && self.drag != Drag::None && !matches!(self.drag, Drag::Syl(_)) {
            if let Some(pos) = resp.interact_pointer_pos() {
                let m = self.snap_ms(ms_of(pos.x) as i64).max(0);
                match self.drag {
                    Drag::Start => doc.set_times(act, m.min(d.end_time), d.end_time),
                    Drag::End => doc.set_times(act, d.start_time, m.max(d.start_time)),
                    Drag::Body(off) => {
                        let ns = self.snap_ms(m - off).max(0);
                        doc.set_times(act, ns, ns + d.duration());
                    }
                    Drag::Syl(_) | Drag::None => {}
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
                if (pos.x - xs).abs() <= 7.0 || (pos.x - xe).abs() <= 7.0 || k_bounds.iter().any(|x| (pos.x - x).abs() <= 5.0) {
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

#[cfg(test)]
mod play_part_tests {
    use super::PlayPart;
    #[test]
    fn ranges() {
        assert_eq!(PlayPart::Selection.range(1000, 3000), (1000, 2000));
        assert_eq!(PlayPart::Before.range(1000, 3000), (500, 500));
        assert_eq!(PlayPart::Before.range(200, 3000), (0, 200));
        assert_eq!(PlayPart::After.range(1000, 3000), (3000, 500));
        assert_eq!(PlayPart::Begin.range(1000, 1300), (1000, 300));
        assert_eq!(PlayPart::End.range(1000, 3000), (2500, 500));
    }
}
