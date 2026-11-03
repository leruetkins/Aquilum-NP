use super::*;

impl Default for WindowState {
    fn default() -> Self {
        Self {
            width: 1200.0,
            height: 800.0,
            x: None,
            y: None,
            maximized: false,
        }
    }
}

impl WindowState {
    pub fn load(dir: &Path) -> Self {
        Self::load_file(&dir.join("window-state.json"))
    }

    pub fn load_file(path: &Path) -> Self {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(state) = serde_json::from_str::<WindowState>(&content) {
                return state;
            }
        }
        Self::default()
    }

    pub fn save(&self, dir: &Path) {
        self.save_file(&dir.join("window-state.json"))
    }

    pub fn save_file(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn to_validated_bounds(&self, cx: &gpui::App, default_w: f32, default_h: f32) -> gpui::WindowBounds {
        use gpui::{px, size, Bounds, Point};
        let w = if self.width >= 300.0 { self.width } else { default_w };
        let h = if self.height >= 200.0 { self.height } else { default_h };
        let sz = size(px(w), px(h));

        let has_valid_coords = match (self.x, self.y) {
            (Some(x), Some(y)) if x.is_finite() && y.is_finite() => true,
            _ => false,
        };

        let bounds = if has_valid_coords {
            Bounds {
                origin: Point {
                    x: px(self.x.unwrap()),
                    y: px(self.y.unwrap()),
                },
                size: sz,
            }
        } else {
            Bounds::centered(None, sz, cx)
        };

        if self.maximized {
            gpui::WindowBounds::Maximized(bounds)
        } else {
            gpui::WindowBounds::Windowed(bounds)
        }
    }

    pub fn to_window_bounds(&self, cx: &gpui::App) -> gpui::WindowBounds {
        use gpui::{px, size, Bounds, Point};
        let w = if self.width >= 300.0 { self.width } else { 1200.0 };
        let h = if self.height >= 200.0 { self.height } else { 800.0 };
        let sz = size(px(w), px(h));

        let has_valid_coords = match (self.x, self.y) {
            (Some(x), Some(y)) if x.is_finite() && y.is_finite() => true,
            _ => false,
        };

        let bounds = if has_valid_coords {
            Bounds {
                origin: Point {
                    x: px(self.x.unwrap()),
                    y: px(self.y.unwrap()),
                },
                size: sz,
            }
        } else {
            Bounds::centered(None, sz, cx)
        };

        if self.maximized {
            gpui::WindowBounds::Maximized(bounds)
        } else {
            gpui::WindowBounds::Windowed(bounds)
        }
    }
}
