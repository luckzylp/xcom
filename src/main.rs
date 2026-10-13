//! XCOM 串口调试助手 — a cross-platform serial port assistant.
//!
//! A Rust + egui reimplementation of the ALIENTEK ATK-XCOM tool, with no
//! advertising and with an adaptive character-encoding detector.

// Do not open a console window alongside the GUI on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod encoding;
mod fonts;
mod hex;
mod logstore;
mod multi;
mod protocol;
mod serial;
mod settings;
mod theme;
mod ui;

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 780.0])
            .with_min_inner_size([880.0, 540.0])
            // A stable app id lets the compositor associate the window with its
            // desktop entry (and therefore its icon).
            .with_app_id("xcom")
            .with_title("XCOM 串口调试助手"),
        ..Default::default()
    };

    eframe::run_native(
        "XCOM 串口调试助手",
        native_options,
        Box::new(|cc| {
            // Fonts must be installed before the first frame is laid out.
            match fonts::install(&cc.egui_ctx) {
                fonts::FontStatus::Loaded(family) => log::info!("已加载中文字体: {family}"),
                fonts::FontStatus::Missing => {
                    log::warn!("未找到中文字体，界面中文可能显示为方块")
                }
            }
            Ok(Box::new(app::XcomApp::new(cc)))
        }),
    )
}
