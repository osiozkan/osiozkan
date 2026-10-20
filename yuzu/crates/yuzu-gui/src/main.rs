mod app;
mod commands;
mod i18n;
mod palette;
mod prefs;
mod luadialog;
mod media;
mod preview;
mod settings;
mod worker;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opts = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1380.0, 860.0]).with_title("Yuzu"),
        ..Default::default()
    };
    // yuzu-gui [file] [--video FILE] [--reference FILE]
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1).cloned());
    let (video, reference) = (value("--video"), value("--reference"));
    let file = args.iter().enumerate().find(|(i, a)| !a.starts_with("--") && (*i == 0 || !args[*i - 1].starts_with("--"))).map(|(_, a)| a.clone());
    eframe::run_native("Yuzu", opts, Box::new(move |cc| {
        let mut app = app::App::new(cc, file, video);
        if let Some(r) = reference { app.open_reference(std::path::PathBuf::from(r)); }
        Ok(Box::new(app))
    }))
}
