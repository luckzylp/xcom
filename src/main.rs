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
mod i18n;
mod logstore;
mod multi;
mod protocol;
mod serial;
mod settings;
mod theme;
mod ui;

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // The window title is set before the app is constructed, so the language
    // has to be resolved here rather than in `XcomApp::new`.
    let settings = settings::Settings::load();
    i18n::set_lang(settings.lang.unwrap_or_else(i18n::system_lang));
    log::info!(
        "界面语言 {:?}，窗口标题: {}",
        i18n::lang(),
        i18n::window_title()
    );

    // Alpha is what makes the rounded corners possible: the pixels we never
    // paint stay see-through.  Only needed where we supply the frame ourselves.
    let transparent = ui::window::PAINTS_OWN_BACKGROUND;

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 780.0])
            .with_min_inner_size([880.0, 540.0])
            .with_transparent(transparent)
            // A stable app id lets the compositor associate the window with its
            // desktop entry (and therefore its icon).
            .with_app_id("xcom")
            .with_title(i18n::window_title()),
        ..Default::default()
    };

    eframe::run_native(
        &i18n::window_title(),
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
