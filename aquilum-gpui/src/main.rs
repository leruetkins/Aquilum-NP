#![windows_subsystem = "windows"]

pub mod icons;
pub mod markdown;
pub mod state;
pub mod theme;
pub mod views;

use gpui::{App, AppContext, Application, WindowOptions};
use views::app_view::AppView;

fn main() {
    Application::new()
        .with_assets(icons::MemoryAssetSource)
        .run(|cx: &mut App| {
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
                    let view = cx.new(|cx| AppView::new(cx));
                    let weak = view.downgrade();
                    window.on_window_should_close(cx, move |window, cx| {
                        if let Some(view) = weak.upgrade() {
                            let _ = view.update(cx, |app, _cx| {
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
