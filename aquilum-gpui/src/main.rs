// Release-сборка: подсистема "windows" — консольное окно не создаётся вовсе.
// Debug-сборка сохраняет консоль, чтобы видеть логи (в т.ч. FPS-монитор).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod blink_manager;
pub mod code_scan;
pub mod commands;
pub mod commands_builtin;
pub mod events;
pub mod editor_window;
pub mod fonts;
pub mod i18n;
pub mod icons;
pub mod ipc;
pub mod line_index;
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
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let payload = args[1..].join("\n");
        if crate::ipc::platform::try_send_to_primary_instance(&payload) {
            return;
        }
    }

    Application::new()
        .with_assets(icons::MemoryAssetSource)
        .run(|cx: &mut App| {
            fonts::register_embedded_fonts(cx);
            let app_data_dir = state::app_data_dir();
            let window_state = state::WindowState::load(&app_data_dir);
            let window_bounds = window_state.to_window_bounds(cx);

            let initial_settings = settings::SettingsManager::new(&app_data_dir);
            let initial_config = initial_settings.get_config();
            let initial_theme = theme::Theme::from_config(&initial_config.ui.theme, None);
            cx.set_global(theme::ActiveTheme(std::sync::Arc::new(initial_theme)));

            crate::ipc::platform::register_url_scheme();
            let (ipc_tx, ipc_rx) = std::sync::mpsc::channel();
            crate::ipc::platform::start_ipc_server(ipc_tx);

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

                    // Start background IPC receiver for deep links and single-instance communication
                    let weak_app = view.downgrade();
                    let timer_executor = cx.background_executor().clone();
                    cx.spawn(async move |cx| {
                        loop {
                            while let Ok(msg) = ipc_rx.try_recv() {
                                for line in msg.lines() {
                                    let action = crate::ipc::parse_command_line_action(line);
                                    let weak = weak_app.clone();
                                    let _ = cx.update(|cx| {
                                        if let Some(app) = weak.upgrade() {
                                            let _ = app.update(cx, |this, cx| {
                                                match action {
                                                    crate::ipc::VibecodiumAction::OpenSettings { tab, setting_id } => {
                                                        let settings_tab = tab.as_deref().map(|t| match t {
                                                            "ui" => crate::views::settings_view::SettingsTab::Ui,
                                                            "editor" => crate::views::settings_view::SettingsTab::Editor,
                                                            "reader" => crate::views::settings_view::SettingsTab::Reader,
                                                            "search" => crate::views::settings_view::SettingsTab::Search,
                                                            "templates" => crate::views::settings_view::SettingsTab::Templates,
                                                            "files" => crate::views::settings_view::SettingsTab::Files,
                                                            "analysis" => crate::views::settings_view::SettingsTab::Analysis,
                                                            "mcp" => crate::views::settings_view::SettingsTab::Mcp,
                                                            "history" => crate::views::settings_view::SettingsTab::History,
                                                            "trash" => crate::views::settings_view::SettingsTab::Trash,
                                                            "system" => crate::views::settings_view::SettingsTab::System,
                                                            "shortcuts" => crate::views::settings_view::SettingsTab::Shortcuts,
                                                            "core-plugins" => crate::views::settings_view::SettingsTab::CorePlugins,
                                                            "community-plugins" => crate::views::settings_view::SettingsTab::CommunityPlugins,
                                                            other => crate::views::settings_view::SettingsTab::PluginSettings(other.to_string()),
                                                        });
                                                        this.open_settings_with_id(settings_tab, setting_id, cx);
                                                    }
                                                    crate::ipc::VibecodiumAction::OpenFile(path) => {
                                                        this.state.open_note(path.clone());
                                                        this.apply_note_settings_for_path(&path);
                                                        cx.notify();
                                                    }
                                                    crate::ipc::VibecodiumAction::FocusApp => {
                                                        cx.notify();
                                                    }
                                                }
                                            });
                                        }
                                    });
                                }
                            }
                            timer_executor.timer(std::time::Duration::from_millis(100)).await;
                        }
                    }).detach();

                    window.on_window_should_close(cx, move |window, cx| {
                        if let Some(view) = weak.upgrade() {
                            let _ = view.update(cx, |app, cx| {
                                app.catalog_window_manager.close_if_open(cx);
                                app.settings_window_manager.close_if_open(cx);
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
