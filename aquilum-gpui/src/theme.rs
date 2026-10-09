use gpui::{rgb, rgba, Rgba, WindowAppearance};

/// Holds all color definitions for the active UI theme.
#[derive(Clone, Debug)]
pub struct Theme {
    /// Flag indicating whether the theme is dark or light.
    pub is_dark: bool,
    /// Background color for the root application shell and window frame.
    pub bg_app: Rgba,
    /// Background color for the sidebar directory tree and panels.
    pub bg_sidebar: Rgba,
    /// Background color for the primary note editor viewport.
    pub bg_editor: Rgba,
    /// Background color for settings cards and elevated cards.
    pub bg_card: Rgba,
    /// Background color for hovered buttons and items.
    pub bg_hover: Rgba,
    /// Background color for active or selected items.
    pub bg_selected: Rgba,
    /// Primary border color for frames and dividers.
    pub border: Rgba,
    /// Subtle secondary border color.
    pub border_subtle: Rgba,
    /// Main high-contrast foreground text color.
    pub text_primary: Rgba,
    /// Secondary muted text color for descriptions and metadata.
    pub text_secondary: Rgba,
    /// Subtle muted text and icon color.
    pub text_muted: Rgba,
    /// Brand accent color for buttons, active tabs, and highlights.
    pub accent: Rgba,
    /// Hover state color for accented interactive elements.
    pub accent_hover: Rgba,
    /// Text color to be displayed directly on top of accent-colored backgrounds.
    pub text_on_accent: Rgba,
    /// Background color for active text selection in editor.
    pub selection_bg: Rgba,
    /// Tag chip background color.
    #[allow(dead_code)]
    pub tag_bg: Rgba,
    /// Background color for inline code and code block fencing.
    pub code_bg: Rgba,
    /// Color for success indicators, inline code text, and green badges.
    pub success: Rgba,
}

impl Theme {
    /// Returns the default dark theme (Catppuccin Mocha palette).
    pub fn dark() -> Self {
        Self {
            is_dark: true,
            bg_app: rgb(0x181825),         // Catppuccin Mantle
            bg_sidebar: rgb(0x11111b),     // Catppuccin Crust
            bg_editor: rgb(0x1e1e2e),      // Catppuccin Base
            bg_card: rgb(0x24273a),        // Macchiato Surface
            bg_hover: rgb(0x313244),       // Surface0
            bg_selected: rgb(0x45475a),    // Surface1
            border: rgb(0x313244),
            border_subtle: rgb(0x27273a),
            text_primary: rgb(0xcdd6f4),   // Text
            text_secondary: rgb(0xa6adc8), // Subtext0
            text_muted: rgb(0x6c7086),     // Overlay0
            accent: rgb(0x89b4fa),         // Blue
            accent_hover: rgb(0xb4befe),   // Lavender
            text_on_accent: rgb(0x11111b), // Dark text on light accent
            selection_bg: rgba(0x89b4fa44),
            tag_bg: rgb(0x313244),
            code_bg: rgb(0x181825),
            success: rgb(0xa6e3a1),        // Green
        }
    }

    /// Returns the modern light theme (Catppuccin Latte clean palette).
    pub fn light() -> Self {
        Self {
            is_dark: false,
            bg_app: rgb(0xf2f4f8),         // Soft clean canvas
            bg_sidebar: rgb(0xe8ecf2),     // Subtle contrasting sidebar
            bg_editor: rgb(0xffffff),      // Crisp white editor surface
            bg_card: rgb(0xffffff),        // White card background
            bg_hover: rgb(0xe0e5ee),       // Gentle hover highlight
            bg_selected: rgb(0xd2d9e6),    // Selected item background
            border: rgb(0xd0d7e3),         // Crisp light border
            border_subtle: rgb(0xe2e7f0),  // Subtle divider
            text_primary: rgb(0x1e2030),   // High contrast deep charcoal
            text_secondary: rgb(0x4c4f69), // Slate subtext
            text_muted: rgb(0x8c8fa1),     // Muted icon and caption gray
            accent: rgb(0x1e66f5),         // Vibrant royal blue
            accent_hover: rgb(0x1855c4),   // Deep royal blue
            text_on_accent: rgb(0xffffff), // Crisp white on royal blue
            selection_bg: rgba(0x1e66f533),
            tag_bg: rgb(0xe8ecf2),
            code_bg: rgb(0xf2f4f8),        // Light code background
            success: rgb(0x40a02b),        // Forest green
        }
    }

    /// Resolves the theme based on the configuration preference and OS window appearance.
    pub fn from_config(theme_name: &str, appearance: Option<WindowAppearance>) -> Self {
        match theme_name {
            "light" => Self::light(),
            "dark" => Self::dark(),
            _ => {
                // "system" preference: resolve based on operating system window appearance
                if let Some(app) = appearance {
                    match app {
                        WindowAppearance::Light | WindowAppearance::VibrantLight => Self::light(),
                        _ => Self::dark(),
                    }
                } else {
                    Self::dark()
                }
            }
        }
    }
}

impl Default for Theme {
    /// Provides the default Dark theme for fallback initialization.
    fn default() -> Self {
        Self::dark()
    }
}
