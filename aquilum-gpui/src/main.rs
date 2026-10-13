// Release-сборка: подсистема "windows" — консольное окно не создаётся вовсе.
// Debug-сборка сохраняет консоль, чтобы видеть логи (в т.ч. FPS-монитор).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod blink_manager;
pub mod commands;
pub mod commands_builtin;
pub mod events;
pub mod fonts;
pub mod i18n;
pub mod icons;
pub mod markdown;
pub mod markdown_doc;
pub mod perf;
pub mod plugins;
pub mod ribbon;
pub mod settings;
pub mod snap;
pub mod state;
pub mod theme;
pub mod vault;
pub mod views;

use gpui::{App, AppContext, Application, WindowOptions};
use views::app_view::AppView;

fn main() {
    Application::new()
        .with_assets(icons::MemoryAssetSource)
        .run(|cx: &mut App| {
            fonts::register_embedded_fonts(cx);
            let app_data_dir = state::app_data_dir();
            let window_state = state::WindowState::load(&app_data_dir);
            let window_bounds = window_state.to_window_bounds(cx);

            cx.open_window(
                WindowOptions {
                    window_bounds: Some(window_bounds),
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Vibecodium".into()),
                        appears_transparent: true,
                        traffic_light_position: None,
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let (api_tx, api_rx) = futures::channel::mpsc::unbounded();
                    let api_tx_clone = api_tx.clone();

                    let view = cx.new(|cx| {
                        let mut app = AppView::new(cx);
                        app.plugin_api_tx = Some(api_tx_clone);
                        app.load_installed_plugins_for_vault();
                        // Focus editor by default and activate caret blinking immediately
                        window.focus(&app.focus_handle);
                        app.editor_blink.enable(crate::blink_manager::BlinkTarget::Editor, cx);
                        app
                    });
                    let weak = view.downgrade();

                    // Start the GPUI plugin bridge background listener
                    crate::plugins::bridge::start_plugin_bridge(weak.clone(), api_rx, cx);

                    window.on_window_should_close(cx, move |window, cx| {
                        if let Some(view) = weak.upgrade() {
                            let _ = view.update(cx, |app, _cx| {
                                app.unload_all_plugins();
                                app.save_on_exit(window);
                            });
                        }
                        true
                    });
                    view
                },
            )
            .unwrap();
            cx.activate(true);
        });
}
