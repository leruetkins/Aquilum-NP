// Release build: "windows" subsystem — no console window is created at all.
// Debug build keeps the console so logs are visible (including the FPS monitor).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod blink_manager;
pub mod code_scan;
pub mod commands;
pub mod commands_builtin;
pub mod events;
pub mod editor_window;
pub mod fs_watcher;
pub mod fonts;
pub mod i18n;
pub mod icons;
pub mod images;
pub mod image_render;
pub mod ipc;
pub mod line_index;
pub mod markdown;
pub mod markdown_doc;
pub mod md_export;
pub mod perf;
pub mod pdf_export;
pub mod plugins;
pub mod ribbon;
pub mod save_manager;
pub mod search;
pub mod settings;
pub mod snap;
pub mod state;
pub mod sync;
pub mod trash;
pub mod document_buffer;
pub mod formatting;
pub mod table;
pub mod theme;
pub mod toolbar;
pub mod vault;
pub mod views;
pub mod window_ops;
pub mod workspace;

use gpui::{App, AppContext, Application, WindowDecorations, WindowOptions};
use views::app_view::AppView;

/// Vault passed on the command line: `--vault <path>` or `--vault=<path>`.
/// A launch with it is ephemeral: the session is not written back, so the
/// default (last used) vault stays untouched.
fn parse_vault_arg(args: &[String]) -> Option<std::path::PathBuf> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let raw = arg.trim_matches('"');
        if let Some(value) = raw.strip_prefix("--vault=") {
            return (!value.is_empty()).then(|| std::path::PathBuf::from(value.trim_matches('"')));
        }
        if raw == "--vault" {
            let value = iter.next()?.trim_matches('"');
            return (!value.is_empty()).then(|| std::path::PathBuf::from(value));
        }
    }
    None
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let vault_override = parse_vault_arg(&args);

    // A vault argument is handled by this process; forwarding it to the
    // primary instance would start the default vault there instead.
    if vault_override.is_none() && !args.is_empty() {
        let payload = args.join("\n");
        if crate::ipc::platform::try_send_to_primary_instance(&payload) {
            return;
        }
    }

    Application::new()
        .with_assets(icons::MemoryAssetSource)
        .run(move |cx: &mut App| {
            fonts::register_embedded_fonts(cx);
            let app_data_dir = state::app_data_dir();
            let window_state = state::WindowState::load(&app_data_dir);
            let restore_maximized = window_state.maximized;
            let window_bounds = window_state.to_window_bounds(cx);

            let initial_settings = settings::SettingsManager::new(&app_data_dir);
            let initial_config = initial_settings.get_config();
            let initial_theme = theme::Theme::from_ui(&initial_config.ui, None);
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
                    window_decorations: Some(WindowDecorations::Client),
                    app_id: Some("vibecodium".to_string()),
                    ..Default::default()
                },
                move |window, cx| {
                    let (api_tx, api_rx) = futures::channel::mpsc::unbounded();
                    let api_tx_clone = api_tx.clone();

                    let initial_vault = vault_override.clone();
                    let view = cx.new(|cx| {
                        let mut app = AppView::new(cx, initial_vault.clone());
                        app.plugin_api_tx = Some(api_tx_clone);
                        app.load_installed_plugins_for_vault();
                        // Focus editor by default and activate caret blinking immediately
                        window.focus(&app.focus_handle);
                        app.editor_blink.enable(crate::blink_manager::BlinkTarget::Editor, cx);
                        app.init_window_observers(window, cx);
                        // Restored window state is applied after the first frame:
                        // on X11 the window manager ignores requests sent before
                        // the window is mapped.
                        let restore_pin = app.always_on_top;
                        if restore_pin || restore_maximized {
                            window.on_next_frame(move |window, _| {
                                if restore_maximized && !window.is_maximized() {
                                    crate::window_ops::maximize_window(window);
                                }
                                if restore_pin {
                                    crate::window_ops::set_always_on_top(window, true);
                                }
                            });
                        }
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
                            let weak = weak_app.clone();
                            let _ = cx.update(|cx| {
                                if let Some(app) = weak.upgrade() {
                                    let _ = app.update(cx, |this, cx| {
                                        this.poll_fs_changes(cx);
                                        this.poll_search_replies(cx);
                                        this.poll_sync_status(cx);
                                    });
                                }
                            });
                            timer_executor.timer(std::time::Duration::from_millis(150)).await;
                        }
                    }).detach();

                    window.on_window_should_close(cx, move |window, cx| {
                        if let Some(view) = weak.upgrade() {
                            let _ = view.update(cx, |app, cx| {
                                app.prepare_shutdown(window, cx);
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

#[cfg(test)]
mod tests {
    use super::parse_vault_arg;
    use std::path::PathBuf;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_vault_argument() {
        assert_eq!(
            parse_vault_arg(&args(&["--vault", "D:/notes"])),
            Some(PathBuf::from("D:/notes"))
        );
        assert_eq!(
            parse_vault_arg(&args(&["--vault=D:/notes"])),
            Some(PathBuf::from("D:/notes"))
        );
        assert_eq!(parse_vault_arg(&args(&["--vault"])), None);
        assert_eq!(parse_vault_arg(&args(&["--other", "x"])), None);
        assert_eq!(parse_vault_arg(&[]), None);
    }
}
