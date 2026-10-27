#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
mod highlight;
mod i18n;
mod media_pane;

fn main() -> eframe::Result {
    i18n::load_pref();
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
            let mut it = args.iter();
            while let Some(arg) = it.next() {
                if arg == "--media" {
                    if let Some(m) = it.next() {
                        a.media.load(std::path::PathBuf::from(m), &cc.egui_ctx);
                    }
                } else if arg == "--line" {
                    if let Some(n) = it.next().and_then(|x| x.parse::<usize>().ok()) {
                        a.goto_line(n.saturating_sub(1));
                    }
                } else {
                    a.open_path(std::path::Path::new(arg));
                }
            }
            Ok(Box::new(a))
        }),
    )
}
