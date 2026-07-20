mod app;
mod config;
mod dji;
mod progress;

fn main() -> eframe::Result {
    env_logger::init();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([640.0, 420.0]),
        ..Default::default()
    };

    eframe::run_native(
        "DJI2KMZ",
        native_options,
        Box::new(|_cc| Ok(Box::new(app::DjiKmzApp::default()))),
    )
}
