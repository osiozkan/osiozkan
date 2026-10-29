//! osisub-gui: anime altyazı stüdyosu arayüzü.
mod app;
mod dialog_ui;
mod media_ui;
mod settings;
mod style_ui;
mod theme;

use std::path::PathBuf;

fn main() -> eframe::Result {
    let mut file: Option<PathBuf> = None;
    let mut screenshot: Option<PathBuf> = None;
    let mut scripts: Vec<PathBuf> = vec![];
    let mut tab: Option<String> = None;
    let mut media: Option<PathBuf> = None;
    let mut open_win: Option<String> = None;
    let mut line: Option<usize> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--screenshot" => screenshot = args.next().map(PathBuf::from),
            "--media" => media = args.next().map(PathBuf::from),
            "--open" => open_win = args.next(),
            "--line" => line = args.next().and_then(|v| v.parse().ok()),
            "--tab" => tab = args.next(),
            "--lua" => scripts.extend(args.next().map(PathBuf::from)),
            "-h" | "--help" => {
                println!("osisub-gui [dosya.ass|srt|vtt] [--lua betik.lua]... [--media video.mkv] [--tab styles|quality|automation|info] [--screenshot cikti.png]");
                return Ok(());
            }
            _ => file = Some(PathBuf::from(a)),
        }
    }
    let opts = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1280.0, 780.0]).with_min_inner_size([900.0, 560.0]).with_title("osisub"),
        ..Default::default()
    };
    eframe::run_native("osisub", opts, Box::new(move |cc| Ok(Box::new(app::App::new(cc, file, scripts, screenshot, tab, media, open_win, line)))))
}
