//! XCOM 串口调试助手 — a cross-platform serial port assistant.

// Do not open a console window alongside the GUI on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod encoding;
mod fonts;
mod hex;
mod highlight;
mod i18n;
mod logfile;
mod logstore;
mod multi;
mod protocol;
mod serial;
mod settings;
mod theme;
mod ui;

use gpui_kit::{AppContext as _, TitlebarOptions, WindowBounds, WindowOptions, px, size};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let started = std::time::Instant::now();

    let settings = settings::Settings::load();
    i18n::set_lang(settings.lang.unwrap_or_else(i18n::system_lang));

    let window_size = settings
        .window_size
        .map(|[w, h]| size(px(w.max(880.0)), px(h.max(540.0))))
        .unwrap_or_else(|| size(px(1200.0), px(780.0)));
    let startup = settings.clone();
    let title = i18n::window_title();

    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            app::init(cx);

            theme::apply(cx, startup.theme);
            theme::set_base_font(
                cx,
                startup.fonts.ui_family.as_deref(),
                startup.fonts.ui_size,
            );

            let options = WindowOptions {
                window_bounds: Some(WindowBounds::centered(window_size, cx)),
                window_min_size: Some(size(px(880.0), px(540.0))),
                titlebar: Some(TitlebarOptions {
                    title: Some(title.clone().into()),
                    ..Default::default()
                }),
                app_id: Some("xcom".to_owned()),
                ..Default::default()
            };
            let settings = startup.clone();
            let (handle, view) = gpui_kit::open_window(options, cx, move |window, cx| {
                cx.new(|cx| app::XcomApp::new(window, cx, settings.clone()))
            })
            .expect("failed to open the main window");

            let weak = view.downgrade();
            cx.on_app_quit(move |cx| {
                let _ = weak.update(cx, |app, _| app.save_on_exit());
                async {}
            })
            .detach();

            let weak = view.downgrade();
            cx.on_window_closed(move |cx, _window| {
                let _ = weak.update(cx, |app, _| app.save_on_exit());
            })
            .detach();

            log::info!(
                "界面语言 {:?}，窗口标题: {}",
                i18n::lang(),
                i18n::window_title()
            );
            let _ = handle;
            log::info!("启动完成用时 {:?}", started.elapsed());
        });
}
