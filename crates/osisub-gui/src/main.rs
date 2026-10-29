//! osisub-gui: anime altyazı stüdyosu arayüzü.
mod app;
mod dialog_ui;
mod style_ui;
mod theme;

use std::path::PathBuf;

fn main() -> eframe::Result {
    let mut file: Option<PathBuf> = None;
    let mut screenshot: Option<PathBuf> = None;
    let mut scripts: Vec<PathBuf> = vec![];
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--screenshot" => screenshot = args.next().map(PathBuf::from),
            "--lua" => scripts.extend(args.next().map(PathBuf::from)),
            "-h" | "--help" => {
                println!("osisub-gui [dosya.ass|srt|vtt] [--lua betik.lua]... [--screenshot cikti.png]");
                return Ok(());
            }
            _ => file = Some(PathBuf::from(a)),
        }
    }
    let opts = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1280.0, 780.0]).with_min_inner_size([900.0, 560.0]).with_title("osisub"),
        ..Default::default()
    };
    eframe::run_native("osisub", opts, Box::new(move |cc| Ok(Box::new(app::App::new(cc, file, scripts, screenshot)))))
}
