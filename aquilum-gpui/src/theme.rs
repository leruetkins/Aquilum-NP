use gpui::{rgb, Rgba};

pub struct Theme {
    pub bg_app: Rgba,
    pub bg_sidebar: Rgba,
    pub bg_editor: Rgba,
    pub bg_card: Rgba,
    pub bg_hover: Rgba,
    pub bg_selected: Rgba,
    pub border: Rgba,
    pub border_subtle: Rgba,
    pub text_primary: Rgba,
    pub text_secondary: Rgba,
    pub text_muted: Rgba,
    pub accent: Rgba,
    pub accent_hover: Rgba,
    #[allow(dead_code)]
    pub tag_bg: Rgba,
    pub code_bg: Rgba,
    pub success: Rgba,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg_app: rgb(0x181825),         // Catppuccin Mantle
            bg_sidebar: rgb(0x11111b),     // Catppuccin Crust
            bg_editor: rgb(0x1e1e2e),      // Catppuccin Base
            bg_card: rgb(0x24273a),
            bg_hover: rgb(0x313244),       // Surface0
            bg_selected: rgb(0x45475a),    // Surface1
            border: rgb(0x313244),
            border_subtle: rgb(0x27273a),
            text_primary: rgb(0xcdd6f4),   // Text
            text_secondary: rgb(0xa6adc8), // Subtext0
            text_muted: rgb(0x6c7086),     // Overlay0
            accent: rgb(0x89b4fa),         // Blue
            accent_hover: rgb(0xb4befe),   // Lavender
            tag_bg: rgb(0x313244),
            code_bg: rgb(0x181825),
            success: rgb(0xa6e3a1),        // Green
        }
    }
}
