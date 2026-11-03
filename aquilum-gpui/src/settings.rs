use serde::{Deserialize, Serialize};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

// =========================================================================
// Settings Models
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Bm25fParams {
    pub k1: f32,
    pub k3: f32,
    pub b_title: f32,
    pub b_body: f32,
    pub title_weight: f32,
}

impl Default for Bm25fParams {
    fn default() -> Self {
        Self {
            k1: 1.2,
            k3: 8.0,
            b_title: 0.3,
            b_body: 0.75,
            title_weight: 2.5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AnalysisSettings {
    pub enable_bm25f: bool,
    pub enable_adamic_adar: bool,
    pub enable_wikixiv: bool,
    pub bm25f_params: Bm25fParams,
}

impl Default for AnalysisSettings {
    fn default() -> Self {
        Self {
            enable_bm25f: true,
            enable_adamic_adar: true,
            enable_wikixiv: true,
            bm25f_params: Bm25fParams::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchIndexSettings {
    pub candidate_pool_size: usize,
    pub max_query_terms: usize,
}

impl Default for SearchIndexSettings {
    fn default() -> Self {
        Self {
            candidate_pool_size: 256,
            max_query_terms: 64,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FontSettings {
    pub font_family: String,
    pub font_weight: u32,
    pub font_size_base: u32,
}

impl Default for FontSettings {
    fn default() -> Self {
        Self {
            font_family: "Inter".to_string(),
            font_weight: 400,
            font_size_base: 14,
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct HeadingStyle {
    pub font_size: u32,
    pub color_name: String,
    #[serde(default = "default_true")]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
}

impl HeadingStyle {
    pub fn new(font_size: u32, color_name: &str) -> Self {
        Self {
            font_size,
            color_name: color_name.to_string(),
            bold: true,
            italic: false,
        }
    }

    pub fn with_style(font_size: u32, color_name: &str, bold: bool, italic: bool) -> Self {
        Self {
            font_size,
            color_name: color_name.to_string(),
            bold,
            italic,
        }
    }
}

impl Default for HeadingStyle {
    fn default() -> Self {
        Self {
            font_size: 16,
            color_name: "blue".to_string(),
            bold: true,
            italic: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct HeadingsSettings {
    pub h1: HeadingStyle,
    pub h2: HeadingStyle,
    pub h3: HeadingStyle,
    pub h4: HeadingStyle,
    pub h5: HeadingStyle,
    pub h6: HeadingStyle,
}

impl Default for HeadingsSettings {
    fn default() -> Self {
        Self {
            h1: HeadingStyle::new(22, "blue"),
            h2: HeadingStyle::new(19, "sky_blue"),
            h3: HeadingStyle::new(17, "bluish_green"),
            h4: HeadingStyle::new(15, "orange"),
            h5: HeadingStyle::new(14, "vermilion"),
            h6: HeadingStyle::new(13, "reddish_purple"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum NoteAlignment {
    #[default]
    Center,
    Left,
}

impl NoteAlignment {
    pub fn is_left(&self) -> bool {
        matches!(self, Self::Left)
    }

    pub fn is_center(&self) -> bool {
        matches!(self, Self::Center)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TitleAlignment {
    #[serde(alias = "left")]
    WindowLeft,
    #[serde(alias = "center")]
    WindowCenter,
    EditorLeft,
    EditorCenter,
}

impl Default for TitleAlignment {
    fn default() -> Self {
        Self::WindowLeft
    }
}

impl TitleAlignment {
    pub fn is_window_left(&self) -> bool {
        matches!(self, Self::WindowLeft)
    }

    pub fn is_window_center(&self) -> bool {
        matches!(self, Self::WindowCenter)
    }

    pub fn is_editor_left(&self) -> bool {
        matches!(self, Self::EditorLeft)
    }

    pub fn is_editor_center(&self) -> bool {
        matches!(self, Self::EditorCenter)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EditorSettings {
    pub save_debounce_ms: u32,
    pub line_height: f32,
    pub max_width_ch: u32,
    pub word_wrap: bool,
    pub smart_dashes: bool,
    pub list_callouts: bool,
    pub auto_link_title: bool,
    pub live_tabs: u32,
    pub link_suggest: bool,
    pub link_suggest_min_chars: u32,
    pub headings: HeadingsSettings,
    pub hide_current_line_on_selection: bool,
    /// Short wheel ease (~90 ms, hard landing) instead of instant steps.
    /// Off by default: when the wheel stops, the text stops.
    pub wheel_smooth_scroll: bool,
    /// Overview-ruler marks on the editor scrollbar: cursor line + selection
    /// span. On by default.
    pub scrollbar_ruler: bool,
    /// Wide Zed-style editor scrollbar: a 16 px lane with a visible track and
    /// a full-width square thumb instead of the floating pill. Off by default.
    pub wide_scrollbar: bool,
    /// Whether to keep cursor column stable when clicking on collapsed markdown tags or headings.
    /// Default is true.
    #[serde(default = "default_true")]
    pub exact_reveal_cursor: bool,
    /// Whether to use adaptive debouncing and deadline based on document size.
    /// On by default.
    pub adaptive_save: bool,
    /// Automatically save dirty notes when the application window loses focus.
    /// On by default.
    pub save_on_blur: bool,
    /// Whether to display invisible characters (spaces, line breaks, EOF) in red.
    /// Off by default.
    #[serde(default)]
    pub show_invisibles: bool,
    /// Note content width percentage (0..=100, where 100 is full width). Default is 100.
    pub editor_width_pct: u32,
    /// Note alignment inside the editor pane (Center or Left). Default is Center.
    pub note_align: NoteAlignment,
    /// Note title alignment (WindowLeft, WindowCenter, EditorLeft, EditorCenter). Default is WindowLeft.
    #[serde(default)]
    pub title_align: TitleAlignment,
    #[serde(flatten)]
    pub font: FontSettings,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            save_debounce_ms: 1000,
            line_height: 1.8,
            max_width_ch: 65,
            word_wrap: true,
            smart_dashes: true,
            list_callouts: true,
            auto_link_title: true,
            live_tabs: 3,
            link_suggest: true,
            link_suggest_min_chars: 2,
            headings: HeadingsSettings::default(),
            hide_current_line_on_selection: true,
            wheel_smooth_scroll: false,
            scrollbar_ruler: true,
            wide_scrollbar: false,
            exact_reveal_cursor: true,
            adaptive_save: true,
            save_on_blur: true,
            show_invisibles: false,
            editor_width_pct: 100,
            note_align: NoteAlignment::Center,
            title_align: TitleAlignment::WindowLeft,
            font: FontSettings {
                font_family: "iA Writer Mono".to_string(),
                font_weight: 400,
                font_size_base: 16,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReaderSettings {
    pub line_height: f32,
    pub max_width_ch: u32,
    pub margin_px: u32,
    pub justify: bool,
    pub hyphenate: bool,
    pub flow: String,
    #[serde(flatten)]
    pub font: FontSettings,
}

impl Default for ReaderSettings {
    fn default() -> Self {
        Self {
            line_height: 1.6,
            max_width_ch: 65,
            margin_px: 48,
            justify: true,
            hyphenate: true,
            flow: "paginated".to_string(),
            font: FontSettings {
                font_family: "iA Writer Quattro".to_string(),
                font_weight: 400,
                font_size_base: 18,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UiSettings {
    pub theme: String,
    /// Preset id used when the light appearance is active.
    pub light_theme: String,
    /// Preset id used when the dark appearance is active.
    pub dark_theme: String,
    pub language: String,
    pub primary_color: String,
    pub sidebar_smart_scroll: bool,
    pub swap_tab_bar_buttons: bool,
    #[serde(flatten)]
    pub font: FontSettings,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            light_theme: "light".to_string(),
            dark_theme: "dark".to_string(),
            language: String::new(),
            primary_color: "#1471eb".to_string(),
            sidebar_smart_scroll: false,
            swap_tab_bar_buttons: true,
            font: FontSettings {
                font_family: "Inter".to_string(),
                font_weight: 400,
                font_size_base: 14,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TrashSettings {
    pub retention_days: u32,
}

impl Default for TrashSettings {
    fn default() -> Self {
        Self {
            retention_days: 30,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HistorySettings {
    pub retention_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct McpSettings {
    pub enabled: bool,
    pub port: u16,
    pub token: String,
    pub allow_write: bool,
}

impl Default for McpSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 8787,
            token: String::new(),
            allow_write: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TemplateSettings {
    pub folder: String,
}

impl Default for TemplateSettings {
    fn default() -> Self {
        Self { folder: "Templates".to_string() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ImagesSettings {
    /// Default corner radius, percent of the shorter visible side (0–50).
    pub corner_radius: u32,
    /// `{n}` is the figure number, `{text}` the caption text.
    pub caption_template: String,
    /// `below` or `above`.
    pub caption_position: String,
    pub caption_numbered: bool,
    /// Open the image editor right after an import.
    pub open_editor_after_insert: bool,
    /// Above this size the import toast suggests compression.
    pub large_file_mb: u32,
    /// Attachment trash retention.
    pub trash_days: u32,
}

impl Default for ImagesSettings {
    fn default() -> Self {
        Self {
            corner_radius: 0,
            caption_template: "Рис. {n}. {text}".to_string(),
            caption_position: "below".to_string(),
            caption_numbered: true,
            open_editor_after_insert: false,
            large_file_mb: 20,
            trash_days: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FilesSettings {
    pub folder: String,
    pub close_deleted_tabs: bool,
    pub keep_modified_on_delete: bool,
    /// Show the attachment folder as a regular node in the file explorer.
    /// Off by default: the folder is storage, not a note folder.
    pub show_attachments_folder: bool,
    /// Automatically move attachments to trash when their references are removed from notes.
    #[serde(default = "default_true")]
    pub auto_trash_attachments: bool,
}

impl Default for FilesSettings {
    fn default() -> Self {
        Self {
            folder: "attachments".to_string(),
            close_deleted_tabs: true,
            keep_modified_on_delete: true,
            show_attachments_folder: false,
            auto_trash_attachments: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UpdateSettings {
    pub auto: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self { auto: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CommunityPluginSettings {
    /// Safe mode disables execution of all community plugins.
    pub safe_mode: bool,
}

impl Default for CommunityPluginSettings {
    fn default() -> Self {
        Self { safe_mode: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    pub analysis: AnalysisSettings,
    pub mcp: McpSettings,
    pub trash: TrashSettings,
    pub history: HistorySettings,
    pub search: SearchIndexSettings,
    pub editor: EditorSettings,
    pub reader: ReaderSettings,
    pub ui: UiSettings,
    pub templates: TemplateSettings,
    pub files: FilesSettings,
    pub images: ImagesSettings,
    pub updates: UpdateSettings,
    pub community_plugins: CommunityPluginSettings,
}

// =========================================================================
// Settings Manager
// =========================================================================

pub struct SettingsManager {
    config_path: PathBuf,
    current_config: RwLock<AppConfig>,
}

impl SettingsManager {
    pub fn new(app_data_dir: &Path) -> Self {
        let config_path = app_data_dir.join("settings.json");
        let current_config = Self::load_or_default(&config_path);
        Self {
            config_path,
            current_config: RwLock::new(current_config),
        }
    }

    fn load_or_default(path: &Path) -> AppConfig {
        match fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(config) => return config,
                Err(error) => {
                    eprintln!("[settings] не удалось распарсить {}: {error}", path.display());
                }
            },
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => eprintln!("[settings] не удалось прочитать {}: {error}", path.display()),
        }
        let default_config = AppConfig::default();
        if let Err(error) = write_config(path, &default_config) {
            eprintln!("[settings] не удалось записать настройки по умолчанию: {error}");
        }
        default_config
    }

    pub fn get_config(&self) -> AppConfig {
        self.current_config.read().unwrap().clone()
    }

    pub fn update_config(&self, new_config: AppConfig) -> Result<(), String> {
        write_config(&self.config_path, &new_config)?;
        *self.current_config.write().unwrap() = new_config;
        Ok(())
    }
}

fn write_config(path: &Path, config: &AppConfig) -> Result<(), String> {
    let json = serde_json::to_string_pretty(config).map_err(|error| error.to_string())?;
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, json.as_bytes()).map_err(|e| e.to_string())?;
    fs::rename(&temp_path, path).map_err(|e| e.to_string())?;
    Ok(())
}

// =========================================================================
// Multi-Window Shared Settings Model & Events
// =========================================================================

pub const APP_URL_SCHEME: &str = "vibecodium";
pub const APP_IPC_PIPE_NAME: &str = r"\\.\pipe\vibecodium-ipc";

#[derive(Debug, Clone)]
pub enum SettingsEvent {
    ConfigChanged,
    ThemeChanged,
    PluginToggled { plugin_id: String, enabled: bool },
    WordWrapToggled(bool),
}

pub struct SettingsModel {
    pub config: AppConfig,
    pub manager: std::sync::Arc<SettingsManager>,
}

impl SettingsModel {
    pub fn new(manager: std::sync::Arc<SettingsManager>) -> Self {
        let config = manager.get_config();
        Self { config, manager }
    }

    pub fn save(&mut self, cx: &mut gpui::Context<Self>) {
        if let Err(e) = self.manager.update_config(self.config.clone()) {
            eprintln!("[settings] error saving config: {}", e);
        }
        cx.emit(SettingsEvent::ConfigChanged);
        cx.notify();
    }

    pub fn set_theme(&mut self, theme_name: String, cx: &mut gpui::Context<Self>) {
        self.config.ui.theme = theme_name;
        self.save(cx);
        cx.emit(SettingsEvent::ThemeChanged);
    }

    pub fn set_word_wrap(&mut self, word_wrap: bool, cx: &mut gpui::Context<Self>) {
        self.config.editor.word_wrap = word_wrap;
        self.save(cx);
        cx.emit(SettingsEvent::WordWrapToggled(word_wrap));
    }

    pub fn set_show_invisibles(&mut self, show: bool, cx: &mut gpui::Context<Self>) {
        self.config.editor.show_invisibles = show;
        self.save(cx);
    }
}

impl gpui::EventEmitter<SettingsEvent> for SettingsModel {}

// =========================================================================
// Setting Registry & JsonPath Accessor
// =========================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingType {
    Bool,
    String,
    U32,
    Usize,
    F32,
}

#[derive(Debug, Clone)]
pub struct SettingDescriptor {
    pub id: &'static str,
    pub tab_id: &'static str,
    pub title_key: &'static str,
    pub desc_key: &'static str,
    pub keywords: &'static [&'static str],
    pub aliases: &'static [&'static str],
    pub setting_type: SettingType,
}

impl SettingDescriptor {
    pub fn url(&self) -> String {
        format!("{}://settings/{}", APP_URL_SCHEME, self.id)
    }

    pub fn title(&self, lang: &str) -> String {
        crate::i18n::t(self.title_key, lang)
    }

    pub fn description(&self, lang: &str) -> String {
        crate::i18n::t(self.desc_key, lang)
    }
}

pub static SETTINGS: &[SettingDescriptor] = &[
    SettingDescriptor {
        id: "ui.theme",
        tab_id: "ui",
        title_key: "settings.registry.ui.theme.title",
        desc_key: "settings.registry.ui.theme.desc",
        keywords: &["theme", "dark", "light", "тема", "тёмная", "светлая"],
        aliases: &["appearance.theme"],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "ui.lightTheme",
        tab_id: "ui",
        title_key: "settings.registry.ui.lightTheme.title",
        desc_key: "settings.registry.ui.lightTheme.desc",
        keywords: &["theme", "light", "palette", "тема", "светлая", "палитра"],
        aliases: &[],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "ui.darkTheme",
        tab_id: "ui",
        title_key: "settings.registry.ui.darkTheme.title",
        desc_key: "settings.registry.ui.darkTheme.desc",
        keywords: &["theme", "dark", "palette", "тема", "тёмная", "темная", "палитра"],
        aliases: &[],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "ui.language",
        tab_id: "ui",
        title_key: "settings.registry.ui.language.title",
        desc_key: "settings.registry.ui.language.desc",
        keywords: &["language", "locale", "язык", "локализация", "русский", "english"],
        aliases: &["system.language"],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "ui.primaryColor",
        tab_id: "ui",
        title_key: "settings.registry.ui.primaryColor.title",
        desc_key: "settings.registry.ui.primaryColor.desc",
        keywords: &["color", "accent", "цвет", "акцент", "hex"],
        aliases: &["appearance.accent"],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "ui.sidebarSmartScroll",
        tab_id: "ui",
        title_key: "settings.registry.ui.sidebarSmartScroll.title",
        desc_key: "settings.registry.ui.sidebarSmartScroll.desc",
        keywords: &["sidebar", "scroll", "сайдбар", "прокрутка", "проводник"],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "ui.swapTabBarButtons",
        tab_id: "ui",
        title_key: "settings.registry.ui.swapTabBarButtons.title",
        desc_key: "settings.registry.ui.swapTabBarButtons.desc",
        keywords: &["tabs", "buttons", "swap", "вкладки", "кнопки", "плюс", "заметка"],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "editor.fontSizeBase",
        tab_id: "editor",
        title_key: "settings.registry.editor.fontSizeBase.title",
        desc_key: "settings.registry.editor.fontSizeBase.desc",
        keywords: &["font", "size", "editor", "шрифт", "размер", "кегль"],
        aliases: &["editor.font.font_size_base"],
        setting_type: SettingType::U32,
    },
    SettingDescriptor {
        id: "editor.fontFamily",
        tab_id: "editor",
        title_key: "settings.registry.editor.fontFamily.title",
        desc_key: "settings.registry.editor.fontFamily.desc",
        keywords: &["font", "family", "гарнитура", "моноширинный", "monospace"],
        aliases: &["editor.font.font_family"],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "editor.wordWrap",
        tab_id: "editor",
        title_key: "settings.registry.editor.wordWrap.title",
        desc_key: "settings.registry.editor.wordWrap.desc",
        keywords: &["wrap", "word", "lines", "перенос", "строки"],
        aliases: &["editor.wrap"],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "editor.liveTabs",
        tab_id: "editor",
        title_key: "settings.registry.editor.liveTabs.title",
        desc_key: "settings.registry.editor.liveTabs.desc",
        keywords: &["tabs", "live", "вкладки", "память"],
        aliases: &[],
        setting_type: SettingType::Usize,
    },
    SettingDescriptor {
        id: "editor.hideCurrentLineOnSelection",
        tab_id: "editor",
        title_key: "settings.registry.editor.hideCurrentLineOnSelection.title",
        desc_key: "settings.registry.editor.hideCurrentLineOnSelection.desc",
        keywords: &["selection", "highlight", "выделение", "подсветка"],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "editor.wheelSmoothScroll",
        tab_id: "editor",
        title_key: "settings.registry.editor.wheelSmoothScroll.title",
        desc_key: "settings.registry.editor.wheelSmoothScroll.desc",
        keywords: &["scroll", "smooth", "wheel", "прокрутка", "плавная", "колесо", "ease"],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "editor.scrollbarRuler",
        tab_id: "editor",
        title_key: "settings.registry.editor.scrollbarRuler.title",
        desc_key: "settings.registry.editor.scrollbarRuler.desc",
        keywords: &["scrollbar", "ruler", "cursor", "selection", "скроллбар", "курсор", "выделение", "метки"],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "editor.wideScrollbar",
        tab_id: "editor",
        title_key: "settings.registry.editor.wideScrollbar.title",
        desc_key: "settings.registry.editor.wideScrollbar.desc",
        keywords: &["scrollbar", "wide", "zed", "скроллбар", "широкий", "ползунок"],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "editor.noteWidth",
        tab_id: "ui",
        title_key: "settings.registry.editor.noteWidth.title",
        desc_key: "settings.registry.editor.noteWidth.desc",
        keywords: &["width", "note", "slider", "ширина", "заметка", "ползунок"],
        aliases: &["ui.noteWidth"],
        setting_type: SettingType::U32,
    },
    SettingDescriptor {
        id: "editor.noteAlign",
        tab_id: "ui",
        title_key: "settings.registry.editor.noteAlign.title",
        desc_key: "settings.registry.editor.noteAlign.desc",
        keywords: &["align", "center", "left", "выравнивание", "центр", "левый край"],
        aliases: &["ui.noteAlign"],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "editor.titleAlign",
        tab_id: "ui",
        title_key: "settings.registry.editor.titleAlign.title",
        desc_key: "settings.registry.editor.titleAlign.desc",
        keywords: &["title", "align", "filename", "заголовок", "имя файла", "окно"],
        aliases: &["ui.titleAlign", "editor.titleAlignWindow"],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "editor.titleAlignEditor",
        tab_id: "ui",
        title_key: "settings.registry.editor.titleAlignEditor.title",
        desc_key: "settings.registry.editor.titleAlignEditor.desc",
        keywords: &["title", "align", "editor", "заметка", "редактор", "колонка"],
        aliases: &["ui.titleAlignEditor"],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "reader.lineHeight",
        tab_id: "reader",
        title_key: "settings.registry.reader.lineHeight.title",
        desc_key: "settings.registry.reader.lineHeight.desc",
        keywords: &["reader", "line", "height", "чтение", "интервал"],
        aliases: &[],
        setting_type: SettingType::F32,
    },
    SettingDescriptor {
        id: "reader.maxWidthCh",
        tab_id: "reader",
        title_key: "settings.registry.reader.maxWidthCh.title",
        desc_key: "settings.registry.reader.maxWidthCh.desc",
        keywords: &["width", "reader", "ширина", "колонка", "символы"],
        aliases: &[],
        setting_type: SettingType::U32,
    },
    SettingDescriptor {
        id: "communityPlugins.safeMode",
        tab_id: "community-plugins",
        title_key: "settings.registry.communityPlugins.safeMode.title",
        desc_key: "settings.registry.communityPlugins.safeMode.desc",
        keywords: &["safe", "mode", "plugins", "безопасный", "режим", "плагины"],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "mcp.enabled",
        tab_id: "mcp",
        title_key: "settings.registry.mcp.enabled.title",
        desc_key: "settings.registry.mcp.enabled.desc",
        keywords: &["mcp", "ai", "server", "сервер", "агенты"],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "trash.retentionDays",
        tab_id: "trash",
        title_key: "settings.registry.trash.retentionDays.title",
        desc_key: "settings.registry.trash.retentionDays.desc",
        keywords: &["trash", "retention", "days", "корзина", "дни", "очистка"],
        aliases: &[],
        setting_type: SettingType::U32,
    },

    SettingDescriptor {
        id: "files.autoTrashAttachments",
        tab_id: "files",
        title_key: "settings.registry.files.autoTrashAttachments.title",
        desc_key: "settings.registry.files.autoTrashAttachments.desc",
        keywords: &["attachments", "trash", "auto", "вложения", "корзина", "изображения", "удаление"],
        aliases: &["files.autoTrash"],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "files.attachmentsFolder",
        tab_id: "files",
        title_key: "settings.files.attachments_folder",
        desc_key: "settings.files.attachments_folder_desc",
        keywords: &["attachments", "folder", "name", "вложения", "папка", "имя", "изображения"],
        aliases: &["files.attachmentsPath"],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "files.showAttachmentsFolder",
        tab_id: "files",
        title_key: "settings.files.show_attachments_folder",
        desc_key: "settings.files.show_attachments_folder_desc",
        keywords: &["attachments", "folder", "tree", "вложения", "папка", "проводник"],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "files.closeDeletedTabs",
        tab_id: "files",
        title_key: "settings.files.close_deleted_tabs",
        desc_key: "settings.files.close_deleted_tabs_desc",
        keywords: &["delete", "tabs", "close", "удаление", "вкладки", "закрыть"],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "files.keepModifiedOnDelete",
        tab_id: "files",
        title_key: "settings.files.keep_modified_on_delete",
        desc_key: "settings.files.keep_modified_on_delete_desc",
        keywords: &["delete", "modified", "dirty", "удаление", "изменённые", "несохранённые"],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "images.cornerRadius",
        tab_id: "files",
        title_key: "editor.image.images_radius",
        desc_key: "editor.image.images_radius_desc",
        keywords: &["image", "radius", "corners", "изображение", "скругление", "углы"],
        aliases: &[],
        setting_type: SettingType::U32,
    },
    SettingDescriptor {
        id: "images.captionTemplate",
        tab_id: "files",
        title_key: "editor.image.caption_template",
        desc_key: "editor.image.caption_template_desc",
        keywords: &["image", "caption", "template", "подпись", "шаблон", "изображение"],
        aliases: &[],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "images.captionPosition",
        tab_id: "files",
        title_key: "editor.image.caption_position_default",
        desc_key: "editor.image.caption_position_default_desc",
        keywords: &["image", "caption", "position", "подпись", "положение", "сверху", "снизу"],
        aliases: &[],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "images.captionNumbered",
        tab_id: "files",
        title_key: "editor.image.caption_numbered_default",
        desc_key: "editor.image.caption_numbered_default_desc",
        keywords: &["image", "caption", "number", "подпись", "нумерация", "изображение"],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "images.openEditorAfterInsert",
        tab_id: "files",
        title_key: "editor.image.open_editor_after_insert",
        desc_key: "editor.image.open_editor_after_insert_desc",
        keywords: &["image", "editor", "insert", "изображение", "редактор", "вставка"],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "images.largeFileMb",
        tab_id: "files",
        title_key: "editor.image.large_file_mb",
        desc_key: "editor.image.large_file_mb_desc",
        keywords: &["image", "large", "size", "изображение", "размер", "порог"],
        aliases: &[],
        setting_type: SettingType::U32,
    },
    SettingDescriptor {
        id: "images.trashDays",
        tab_id: "files",
        title_key: "editor.image.trash_days",
        desc_key: "editor.image.trash_days_desc",
        keywords: &["image", "trash", "retention", "корзина", "хранение", "дни"],
        aliases: &[],
        setting_type: SettingType::U32,
    },
    SettingDescriptor {
        id: "images.clearTrash",
        tab_id: "files",
        title_key: "editor.image.clear_trash",
        desc_key: "editor.image.clear_trash_desc",
        keywords: &["image", "trash", "clear", "корзина", "очистить", "изображения"],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "editorToolbar.alignment",
        tab_id: "editor-toolbar",
        title_key: "settings.toolbar.alignment",
        desc_key: "settings.toolbar.desc",
        keywords: &["toolbar", "align", "position", "панель", "выравнивание", "расположение"],
        aliases: &["toolbar.alignment"],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "ui.fontFamily",
        tab_id: "ui",
        title_key: "settings.ui.font_family",
        desc_key: "settings.ui.font_family_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "ui.fontWeight",
        tab_id: "ui",
        title_key: "settings.ui.font_weight",
        desc_key: "settings.ui.font_weight_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "ui.fontSize",
        tab_id: "ui",
        title_key: "settings.ui.font_size",
        desc_key: "settings.ui.font_size_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::U32,
    },
    SettingDescriptor {
        id: "editor.heading1",
        tab_id: "editor",
        title_key: "settings.editor.h1_title",
        desc_key: "settings.editor.h1_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "editor.heading2",
        tab_id: "editor",
        title_key: "settings.editor.h2_title",
        desc_key: "settings.editor.h2_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "editor.heading3",
        tab_id: "editor",
        title_key: "settings.editor.h3_title",
        desc_key: "settings.editor.h3_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "editor.heading4",
        tab_id: "editor",
        title_key: "settings.editor.h4_title",
        desc_key: "settings.editor.h4_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "editor.heading5",
        tab_id: "editor",
        title_key: "settings.editor.h5_title",
        desc_key: "settings.editor.h5_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "editor.heading6",
        tab_id: "editor",
        title_key: "settings.editor.h6_title",
        desc_key: "settings.editor.h6_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "editor.lineHeight",
        tab_id: "editor",
        title_key: "settings.editor.line_height",
        desc_key: "settings.editor.line_height_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::F32,
    },
    SettingDescriptor {
        id: "editor.maxWidth",
        tab_id: "editor",
        title_key: "settings.editor.max_width",
        desc_key: "settings.editor.max_width_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::U32,
    },
    SettingDescriptor {
        id: "editor.smartDashes",
        tab_id: "editor",
        title_key: "settings.editor.smart_dashes",
        desc_key: "settings.editor.smart_dashes_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "editor.listCallouts",
        tab_id: "editor",
        title_key: "settings.editor.list_callouts",
        desc_key: "settings.editor.list_callouts_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "editor.linkSuggest",
        tab_id: "editor",
        title_key: "settings.editor.link_suggest",
        desc_key: "settings.editor.link_suggest_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "editor.debounce",
        tab_id: "editor",
        title_key: "settings.editor.debounce",
        desc_key: "settings.editor.debounce_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::U32,
    },
    SettingDescriptor {
        id: "reader.fontFamily",
        tab_id: "reader",
        title_key: "settings.reader.font_family",
        desc_key: "settings.reader.font_family_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "reader.justify",
        tab_id: "reader",
        title_key: "settings.reader.justify",
        desc_key: "settings.reader.justify_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "reader.hyphenate",
        tab_id: "reader",
        title_key: "settings.reader.hyphenate",
        desc_key: "settings.reader.hyphenate_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "search.candidatePool",
        tab_id: "search",
        title_key: "settings.search.candidate_pool",
        desc_key: "settings.search.candidate_pool_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::U32,
    },
    SettingDescriptor {
        id: "search.maxTerms",
        tab_id: "search",
        title_key: "settings.search.max_terms",
        desc_key: "settings.search.max_terms_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::U32,
    },
    SettingDescriptor {
        id: "templates.folder",
        tab_id: "templates",
        title_key: "settings.templates.folder",
        desc_key: "settings.templates.folder_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "analysis.bm25f",
        tab_id: "analysis",
        title_key: "settings.analysis.bm25f",
        desc_key: "settings.analysis.bm25f_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "analysis.adamicAdar",
        tab_id: "analysis",
        title_key: "settings.analysis.adamic_adar",
        desc_key: "settings.analysis.adamic_adar_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "analysis.wikixiv",
        tab_id: "analysis",
        title_key: "settings.analysis.wikixiv",
        desc_key: "settings.analysis.wikixiv_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "mcp.port",
        tab_id: "mcp",
        title_key: "settings.mcp.port",
        desc_key: "settings.mcp.port_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::U32,
    },
    SettingDescriptor {
        id: "mcp.allowWrite",
        tab_id: "mcp",
        title_key: "settings.mcp.allow_write",
        desc_key: "settings.mcp.allow_write_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "history.retention",
        tab_id: "history",
        title_key: "settings.history.retention",
        desc_key: "settings.history.retention_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::U32,
    },
    SettingDescriptor {
        id: "trash.autoDelete",
        tab_id: "trash",
        title_key: "settings.trash.auto_delete",
        desc_key: "settings.trash.auto_delete_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "trash.folder",
        tab_id: "trash",
        title_key: "settings.trash.folder",
        desc_key: "settings.trash.folder_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "corePlugins.commandPalette",
        tab_id: "core-plugins",
        title_key: "settings.plugins.core.command_palette.name",
        desc_key: "settings.plugins.core.command_palette.desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "corePlugins.wordCounter",
        tab_id: "core-plugins",
        title_key: "settings.plugins.core.word_counter.name",
        desc_key: "settings.plugins.core.word_counter.desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "corePlugins.editorToolbar",
        tab_id: "core-plugins",
        title_key: "settings.plugins.core.editor_toolbar.name",
        desc_key: "settings.plugins.core.editor_toolbar.desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "corePlugins.fileRecovery",
        tab_id: "core-plugins",
        title_key: "settings.plugins.core.file_recovery.name",
        desc_key: "settings.plugins.core.file_recovery.desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "editor.exactRevealCursor",
        tab_id: "editor",
        title_key: "settings.editor.exact_reveal_cursor",
        desc_key: "settings.editor.exact_reveal_cursor_desc",
        keywords: &[],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
];

// Lookup
pub fn get_setting_by_id(id_or_alias: &str) -> Option<&'static SettingDescriptor> {
    SETTINGS.iter().find(|s| {
        s.id.eq_ignore_ascii_case(id_or_alias)
            || s.aliases.iter().any(|a| a.eq_ignore_ascii_case(id_or_alias))
    })
}

pub fn get_config_value(config: &AppConfig, path: &str) -> Option<serde_json::Value> {
    let json = serde_json::to_value(config).ok()?;
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = &json;
    for part in parts {
        current = current.get(part)?;
    }
    Some(current.clone())
}

pub fn set_config_value(config: &mut AppConfig, path: &str, value: serde_json::Value) -> Result<(), String> {
    let mut json = serde_json::to_value(&*config).map_err(|e| e.to_string())?;
    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty() {
        return Err("Empty path".to_string());
    }
    let mut current = &mut json;
    for &part in &parts[..parts.len() - 1] {
        if !current.is_object() {
            return Err(format!("Expected object at part {}", part));
        }
        current = current.get_mut(part).ok_or_else(|| format!("Key not found: {}", part))?;
    }
    let last = parts[parts.len() - 1];
    if let Some(target) = current.get_mut(last) {
        *target = value;
    } else {
        return Err(format!("Leaf key not found: {}", last));
    }
    *config = serde_json::from_value(json).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn is_setting_modified(config: &AppConfig, path: &str) -> bool {
    let default_config = AppConfig::default();
    let cur = get_config_value(config, path);
    let def = get_config_value(&default_config, path);
    cur != def
}

/// Finds settings matching `query` for the settings search box.
///
/// Titles and descriptions are matched in the requested language plus the
/// English and Russian translations, so a query typed in either language is
/// still found when the interface uses a third one.
pub fn search_settings(query: &str, lang: &str) -> Vec<&'static SettingDescriptor> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }

    // Direct link or ID match
    let direct_id = if let Some(stripped) = q.strip_prefix(&format!("{}://settings/", APP_URL_SCHEME)) {
        stripped
    } else {
        &q
    };

    if let Some(s) = get_setting_by_id(direct_id) {
        return vec![s];
    }

    let terms: Vec<&str> = q.split_whitespace().collect();
    let mut matches = Vec::new();

    for setting in SETTINGS {
        let id_lower = setting.id.to_lowercase();
        let mut titles = Vec::with_capacity(3);
        let mut descs = Vec::with_capacity(3);
        for code in [lang, "en", "ru"] {
            titles.push(crate::i18n::t(setting.title_key, code).to_lowercase());
            descs.push(crate::i18n::t(setting.desc_key, code).to_lowercase());
        }

        let mut score = 0i32;

        if id_lower == q || titles.iter().any(|title| title == &q) {
            score += 1000;
        } else if id_lower.contains(&q) || titles.iter().any(|title| title.contains(&q)) {
            score += 500;
        }

        let all_matched = terms.iter().all(|term| {
            id_lower.contains(term)
                || titles.iter().any(|title| title.contains(term))
                || descs.iter().any(|desc| desc.contains(term))
                || setting.keywords.iter().any(|kw| kw.to_lowercase().contains(term))
                || setting.aliases.iter().any(|al| al.to_lowercase().contains(term))
        });

        if all_matched {
            for term in &terms {
                if id_lower.contains(term) { score += 50; }
                if titles.iter().any(|title| title.contains(term)) { score += 40; }
                if setting.keywords.iter().any(|kw| kw.to_lowercase().contains(term)) { score += 30; }
                if descs.iter().any(|desc| desc.contains(term)) { score += 10; }
            }
            matches.push((score, setting));
        }
    }

    matches.sort_by(|a, b| b.0.cmp(&a.0));
    matches.into_iter().map(|(_, s)| s).collect()
}

#[cfg(test)]
mod tests;

