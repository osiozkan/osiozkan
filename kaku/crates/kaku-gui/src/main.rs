mod app;
mod highlight;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]).with_title("Kaku"),
        ..Default::default()
    };
    eframe::run_native(
        "Kaku",
        opts,
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            let mut a = app::KakuApp::new();
            if let Some(p) = args.first() {
                a.open_path(std::path::Path::new(p));
            }
            Ok(Box::new(a))
        }),
    )
}
