#![windows_subsystem = "windows"]

pub mod icons;
pub mod markdown;
pub mod state;
pub mod theme;
pub mod views;

use gpui::{
    App, AppContext, Application, Bounds, WindowBounds, WindowOptions, px, size,
};
use views::app_view::AppView;

fn main() {
    Application::new()
        .with_assets(icons::MemoryAssetSource)
        .run(|cx: &mut App| {
            let bounds = Bounds::centered(None, size(px(1200.0), px(800.0)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Aquilum".into()),
                        appears_transparent: true,
                        traffic_light_position: None,
                    }),
                    ..Default::default()
                },
                |_, cx| cx.new(|cx| AppView::new(cx)),
            )
            .unwrap();
            cx.activate(true);
        });
}
