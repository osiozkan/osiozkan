//! osisub-gui: anime altyazı stüdyosu arayüzü.
mod app;
mod dialog_ui;
mod media_ui;
mod settings;
mod style_ui;
mod theme;

use std::path::PathBuf;

/// Komut satırı seçenekleri.
#[derive(Default)]
pub struct Args {
    pub file: Option<PathBuf>,
    pub scripts: Vec<PathBuf>,
    pub screenshot: Option<PathBuf>,
    pub tab: Option<String>,
    pub media: Option<PathBuf>,
    pub open_win: Option<String>,
    pub line: Option<usize>,
    pub reference: Option<PathBuf>,
    /// Ekran görüntüsünü bu kadar saniye sonra al (xdotool senaryoları için).
    pub shot_delay: Option<f32>,
}

fn main() -> eframe::Result {
    let mut a = Args::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--screenshot" => a.screenshot = args.next().map(PathBuf::from),
            "--lua" => a.scripts.extend(args.next().map(PathBuf::from)),
            "--media" => a.media = args.next().map(PathBuf::from),
            "--open" => a.open_win = args.next(),
            "--line" => a.line = args.next().and_then(|v| v.parse().ok()),
            "--tab" => a.tab = args.next(),
            "--shot-delay" => a.shot_delay = args.next().and_then(|v| v.parse().ok()),
            "--ref" => a.reference = args.next().map(PathBuf::from),
            "-h" | "--help" => {
                println!("osisub-gui [dosya.ass|srt|vtt] [--lua betik.lua]... [--media video.mkv] [--ref referans.srt] [--tab styles|karaoke|spell|quality|automation|info] [--line N] [--open find|timing|resample|fonts|fixes|vad|pos|play|spectrum|spell] [--screenshot cikti.png]");
                return Ok(());
            }
            _ => a.file = Some(PathBuf::from(arg)),
        }
    }
    let opts = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1280.0, 780.0]).with_min_inner_size([900.0, 560.0]).with_title("osisub"),
        ..Default::default()
    };
    eframe::run_native("osisub", opts, Box::new(move |cc| Ok(Box::new(app::App::new(cc, a)))))
}
