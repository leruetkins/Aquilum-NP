use gpui::ScrollDelta;
use crate::icons;

/// Line-based wheel ticks (mice) are divided down by this: one notch moves
/// roughly one UI line instead of the system default (usually 3 lines ≈
/// half a settings card). Pixel deltas (trackpads) pass through untouched.
const SETTINGS_WHEEL_LINES_DIVISOR: f32 = 3.0;

/// Scales a raw wheel delta for the settings list into logical pixels.
pub(super) fn scaled_wheel_dy(delta: ScrollDelta, line_h: f32) -> f32 {
    match delta {
        ScrollDelta::Pixels(p) => f32::from(p.y),
        ScrollDelta::Lines(l) => l.y * line_h / SETTINGS_WHEEL_LINES_DIVISOR,
    }
}

/// Helper to determine whether a command is a plugin command or built-in/internal.
pub(super) fn is_plugin_command(cmd_id: &str, owner: Option<&str>) -> bool {
    if let Some(owner) = owner {
        if !owner.is_empty() {
            return true;
        }
    }
    if let Some((prefix, _)) = cmd_id.split_once(':') {
        !matches!(
            prefix,
            "editor" | "workspace" | "view" | "app" | "markdown" | "core" | "file" | "folder" | "window"
        )
    } else {
        false
    }
}

/// Helper returning the icon indicating whether a command is a plugin or internal/built-in.
pub(super) fn command_type_icon(is_plugin: bool) -> &'static str {
    if is_plugin {
        icons::ICON_PLUG
    } else {
        icons::ICON_APP_WINDOW
    }
}

/// Represents the navigation tabs available in the Settings modal window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsTab {
    Ui,
    Editor,
    Reader,
    Search,
    Templates,
    Files,
    Sync,
    Trash,
    System,
    Shortcuts,
    CorePlugins,
    CommunityPlugins,
    PluginSettings(String),
}

impl SettingsTab {
    /// Returns the i18n key for this settings tab.
    pub fn key(&self) -> &'static str {
        match self {
            Self::Ui => "settings.tabs.ui",
            Self::Editor => "settings.tabs.editor",
            Self::Reader => "settings.tabs.reader",
            Self::Search => "settings.tabs.search",
            Self::Templates => "settings.tabs.templates",
            Self::Files => "settings.tabs.files",
            Self::Sync => "settings.tabs.sync",
            Self::Trash => "settings.tabs.trash",
            Self::System => "settings.tabs.system",
            Self::Shortcuts => "settings.tabs.shortcuts",
            Self::CorePlugins => "settings.tabs.core_plugins",
            Self::CommunityPlugins => "settings.tabs.community_plugins",
            Self::PluginSettings(_) => "settings.tabs.plugin_settings",
        }
    }

    /// Returns the user-facing localized display title for the tab.
    pub fn title(&self, lang: &str) -> String {
        match self {
            Self::PluginSettings(name) if name.is_empty() => {
                crate::i18n::t("settings.tabs.plugin_settings", lang)
            }
            Self::PluginSettings(name) if name == "editor-toolbar" => {
                crate::i18n::t("settings.tabs.editor_toolbar", lang)
            }
            Self::PluginSettings(name) => name.clone(),
            _ => crate::i18n::t(self.key(), lang),
        }
    }

    /// Returns the SVG icon path corresponding to this settings tab.
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Ui => icons::ICON_MONITOR,
            Self::Editor => icons::ICON_TYPE,
            Self::Reader => icons::ICON_BOOK_OPEN,
            Self::Search => icons::ICON_SEARCH,
            Self::Templates => icons::ICON_FILE_TEXT,
            Self::Files => icons::ICON_PAPERCLIP,
            Self::Sync => icons::ICON_REFRESH,
            Self::Trash => icons::ICON_TRASH_2,
            Self::System => icons::ICON_INFO,
            Self::Shortcuts => icons::ICON_KEYBOARD,
            Self::CorePlugins => icons::ICON_PLUG,
            Self::CommunityPlugins => icons::ICON_GLOBE,
            Self::PluginSettings(name) => {
                if name == "editor-toolbar" {
                    icons::ICON_TYPE
                } else {
                    icons::ICON_SETTINGS
                }
            }
        }
    }

    /// Returns a slice containing all settings tabs in display order.
    pub fn all() -> &'static [SettingsTab] {
        &[
            Self::Ui,
            Self::Editor,
            Self::Reader,
            Self::Search,
            Self::Templates,
            Self::Files,
            Self::Sync,
            Self::Trash,
            Self::System,
            Self::Shortcuts,
            Self::CorePlugins,
            Self::CommunityPlugins,
        ]
    }
}
