mod app;
mod audio;
mod crash_logger;
mod i18n;
mod session;
mod ui;
mod undo;
mod vst_host;

fn main() -> eframe::Result<()> {
    crash_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 700.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title("ToneDock"),
        ..Default::default()
    };

    eframe::run_native(
        "ToneDock",
        options,
        Box::new(|cc| Ok(Box::new(app::ToneDockApp::new(cc)))),
    )
}
