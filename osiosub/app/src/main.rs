mod app;
mod worker;

fn main() -> eframe::Result {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]).with_title("OsioSub"),
        ..Default::default()
    };
    let arg = std::env::args().nth(1);
    eframe::run_native("OsioSub", opts, Box::new(move |cc| Ok(Box::new(app::OsioApp::new(cc, arg)))))
}
