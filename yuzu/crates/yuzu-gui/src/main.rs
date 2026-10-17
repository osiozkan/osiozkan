mod app;
mod luadialog;
mod preview;
mod settings;
mod worker;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opts = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1380.0, 860.0]).with_title("Yuzu"),
        ..Default::default()
    };
    eframe::run_native("Yuzu", opts, Box::new(move |cc| Ok(Box::new(app::App::new(cc, args.first().cloned())))))
}
