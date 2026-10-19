mod state;
mod theme;
mod ui_edit;
mod ui_grid;
mod ui_menu;
mod ui_video;
mod ui_wave;
mod ui_windows;

fn main() -> eframe::Result<()> {
    let arg = std::env::args().nth(1);
    let opts = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1440.0, 900.0]).with_min_inner_size([900.0, 560.0]).with_title("SubForge").with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native("SubForge", opts, Box::new(move |cc| Ok(Box::new(state::App::new(cc, arg.clone())))))
}
