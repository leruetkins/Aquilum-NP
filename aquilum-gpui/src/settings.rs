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
pub struct FilesSettings {
    pub folder: String,
    pub close_deleted_tabs: bool,
    pub keep_modified_on_delete: bool,
}

impl Default for FilesSettings {
    fn default() -> Self {
        Self {
            folder: "Files".to_string(),
            close_deleted_tabs: true,
            keep_modified_on_delete: true,
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
    pub title_ru: &'static str,
    pub title_en: &'static str,
    pub desc_ru: &'static str,
    pub desc_en: &'static str,
    pub keywords: &'static [&'static str],
    pub aliases: &'static [&'static str],
    pub setting_type: SettingType,
}

impl SettingDescriptor {
    pub fn url(&self) -> String {
        format!("{}://settings/{}", APP_URL_SCHEME, self.id)
    }

    pub fn title(&self, is_ru: bool) -> &'static str {
        if is_ru { self.title_ru } else { self.title_en }
    }

    pub fn description(&self, is_ru: bool) -> &'static str {
        if is_ru { self.desc_ru } else { self.desc_en }
    }
}

pub static SETTINGS: &[SettingDescriptor] = &[
    SettingDescriptor {
        id: "ui.theme",
        tab_id: "ui",
        title_ru: "Тема оформления",
        title_en: "Appearance Theme",
        desc_ru: "Выбор цветовой темы интерфейса: системная, тёмная или светлая",
        desc_en: "Color theme for the user interface: system, dark, or light",
        keywords: &["theme", "dark", "light", "тема", "тёмная", "светлая"],
        aliases: &["appearance.theme"],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "ui.language",
        tab_id: "ui",
        title_ru: "Язык интерфейса",
        title_en: "Interface Language",
        desc_ru: "Язык локализации интерфейса приложения (ru / en)",
        desc_en: "Interface localization language (ru / en)",
        keywords: &["language", "locale", "язык", "локализация", "русский", "english"],
        aliases: &["system.language"],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "ui.primaryColor",
        tab_id: "ui",
        title_ru: "Основной акцентный цвет",
        title_en: "Primary Accent Color",
        desc_ru: "Пользовательский акцентный цвет для кнопок, выделений и активных вкладок",
        desc_en: "Custom brand accent color for buttons, selections, and active tabs",
        keywords: &["color", "accent", "цвет", "акцент", "hex"],
        aliases: &["appearance.accent"],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "ui.sidebarSmartScroll",
        tab_id: "ui",
        title_ru: "Умная прокрутка сайдбара",
        title_en: "Sidebar Smart Scroll",
        desc_ru: "Автоматически центрировать активный файл в проводнике при переключении заметок",
        desc_en: "Automatically center the active file in the sidebar tree when switching notes",
        keywords: &["sidebar", "scroll", "сайдбар", "прокрутка", "проводник"],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "ui.swapTabBarButtons",
        tab_id: "ui",
        title_ru: "Поменять местами кнопки на панели вкладок",
        title_en: "Swap Tab Bar Creation Buttons",
        desc_ru: "Кнопка «+» слева (создает вкладку крайней слева), а кнопка новой заметки справа (создает заметку крайней справа)",
        desc_en: "New tab button '+' on the left (opens tab at far left), new note button on the right (opens tab at far right)",
        keywords: &["tabs", "buttons", "swap", "вкладки", "кнопки", "плюс", "заметка"],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "editor.fontSizeBase",
        tab_id: "editor",
        title_ru: "Размер шрифта редактора",
        title_en: "Editor Font Size",
        desc_ru: "Базовый кегль шрифта в окне редактирования заметок (px)",
        desc_en: "Base font size in the note editor viewport (px)",
        keywords: &["font", "size", "editor", "шрифт", "размер", "кегль"],
        aliases: &["editor.font.font_size_base"],
        setting_type: SettingType::U32,
    },
    SettingDescriptor {
        id: "editor.fontFamily",
        tab_id: "editor",
        title_ru: "Семейство шрифта редактора",
        title_en: "Editor Font Family",
        desc_ru: "Гарнитура шрифта для отображения и редактирования заметок",
        desc_en: "Typeface family for displaying and editing note contents",
        keywords: &["font", "family", "гарнитура", "моноширинный", "monospace"],
        aliases: &["editor.font.font_family"],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "editor.wordWrap",
        tab_id: "editor",
        title_ru: "Перенос строк по словам",
        title_en: "Word Wrap",
        desc_ru: "Мягкий перенос длинных строк по границе видимой области редактора",
        desc_en: "Wrap long lines gracefully at the editor viewport edge",
        keywords: &["wrap", "word", "lines", "перенос", "строки"],
        aliases: &["editor.wrap"],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "editor.liveTabs",
        tab_id: "editor",
        title_ru: "Количество активных вкладок",
        title_en: "Live Tabs Count",
        desc_ru: "Максимальное число одновременно удерживаемых в памяти вкладок редактора",
        desc_en: "Maximum number of editor tabs retained active in memory",
        keywords: &["tabs", "live", "вкладки", "память"],
        aliases: &[],
        setting_type: SettingType::Usize,
    },
    SettingDescriptor {
        id: "editor.hideCurrentLineOnSelection",
        tab_id: "editor",
        title_ru: "Скрывать подсветку строки при выделении",
        title_en: "Hide Current Line Highlight on Selection",
        desc_ru: "Не подсвечивать текущую строку, когда выделен диапазон текста",
        desc_en: "Hide current line highlight when a text range is selected",
        keywords: &["selection", "highlight", "выделение", "подсветка"],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "editor.wheelSmoothScroll",
        tab_id: "editor",
        title_ru: "Плавная прокрутка колесом (ease 90 мс)",
        title_en: "Smooth Wheel Scroll (90 ms ease)",
        desc_ru: "Короткая анимация колеса с жёсткой посадкой вместо мгновенных шагов. Выкл — текст останавливается сразу",
        desc_en: "Short wheel animation with a hard landing instead of instant steps. Off means the text stops dead",
        keywords: &["scroll", "smooth", "wheel", "прокрутка", "плавная", "колесо", "ease"],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "editor.scrollbarRuler",
        tab_id: "editor",
        title_ru: "Метки курсора и выделения на скроллбаре",
        title_en: "Scrollbar Ruler Marks",
        desc_ru: "Тонкая линия курсора и область выделения поверх ползунка редактора",
        desc_en: "Thin cursor line and selection span over the editor scrollbar thumb",
        keywords: &["scrollbar", "ruler", "cursor", "selection", "скроллбар", "курсор", "выделение", "метки"],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "editor.wideScrollbar",
        tab_id: "editor",
        title_ru: "Широкий скроллбар (как в Zed)",
        title_en: "Wide Scrollbar (Zed-style)",
        desc_ru: "Широкая лента 16 px с видимым треком и квадратным ползунком на всю ширину вместо узкой пилюли",
        desc_en: "Wide 16 px lane with a visible track and a full-width square thumb instead of the narrow pill",
        keywords: &["scrollbar", "wide", "zed", "скроллбар", "широкий", "ползунок"],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "editor.noteWidth",
        tab_id: "ui",
        title_ru: "Ширина заметки",
        title_en: "Note Column Width",
        desc_ru: "Ширина текстовой колонки заметки в процентах от ширины окна (0–100%)",
        desc_en: "Width of the note content column as a percentage of the window (0-100%)",
        keywords: &["width", "note", "slider", "ширина", "заметка", "ползунок"],
        aliases: &["ui.noteWidth"],
        setting_type: SettingType::U32,
    },
    SettingDescriptor {
        id: "editor.noteAlign",
        tab_id: "ui",
        title_ru: "Выравнивание заметки",
        title_en: "Note Alignment",
        desc_ru: "Горизонтальное выравнивание колонки заметки: по центру или по левому краю",
        desc_en: "Horizontal alignment of the note column: centered or left-aligned",
        keywords: &["align", "center", "left", "выравнивание", "центр", "левый край"],
        aliases: &["ui.noteAlign"],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "editor.titleAlign",
        tab_id: "ui",
        title_ru: "Положение заголовка (по окну)",
        title_en: "Note Title Alignment (Window)",
        desc_ru: "Положение имени файла по ширине окна: слева или по центру",
        desc_en: "Position of the file title relative to the window: left or centered",
        keywords: &["title", "align", "filename", "заголовок", "имя файла", "окно"],
        aliases: &["ui.titleAlign", "editor.titleAlignWindow"],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "editor.titleAlignEditor",
        tab_id: "ui",
        title_ru: "Положение заголовка (по редактору)",
        title_en: "Note Title Alignment (Editor)",
        desc_ru: "Положение имени файла по ширине заметки: по левому краю или по середине",
        desc_en: "Position of the file title relative to the note column: left edge or centered",
        keywords: &["title", "align", "editor", "заметка", "редактор", "колонка"],
        aliases: &["ui.titleAlignEditor"],
        setting_type: SettingType::String,
    },
    SettingDescriptor {
        id: "reader.lineHeight",
        tab_id: "reader",
        title_ru: "Высота строки в режиме чтения",
        title_en: "Reader Line Height",
        desc_ru: "Межстрочный интервал текста в режиме комфортного чтения",
        desc_en: "Line height multiplier for comfortable reading mode",
        keywords: &["reader", "line", "height", "чтение", "интервал"],
        aliases: &[],
        setting_type: SettingType::F32,
    },
    SettingDescriptor {
        id: "reader.maxWidthCh",
        tab_id: "reader",
        title_ru: "Максимальная ширина колонки чтения",
        title_en: "Reader Max Column Width",
        desc_ru: "Ограничение ширины текстового блока в символах (ch)",
        desc_en: "Maximum text container column width in characters (ch)",
        keywords: &["width", "reader", "ширина", "колонка", "символы"],
        aliases: &[],
        setting_type: SettingType::U32,
    },
    SettingDescriptor {
        id: "communityPlugins.safeMode",
        tab_id: "community-plugins",
        title_ru: "Безопасный режим сторонних плагинов",
        title_en: "Community Plugins Safe Mode",
        desc_ru: "Блокирует загрузку и выполнение сторонних плагинов для защиты системы",
        desc_en: "Blocks community plugins execution for system stability and data protection",
        keywords: &["safe", "mode", "plugins", "безопасный", "режим", "плагины"],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "mcp.enabled",
        tab_id: "mcp",
        title_ru: "Включить локальный MCP сервер",
        title_en: "Enable Local MCP Server",
        desc_ru: "Model Context Protocol сервер для интеграции с внешними AI-агентами",
        desc_en: "Model Context Protocol server for integration with external AI coding assistants",
        keywords: &["mcp", "ai", "server", "сервер", "агенты"],
        aliases: &[],
        setting_type: SettingType::Bool,
    },
    SettingDescriptor {
        id: "trash.retentionDays",
        tab_id: "trash",
        title_ru: "Срок хранения файлов в корзине",
        title_en: "Trash Retention Days",
        desc_ru: "Количество дней хранения удалённых заметок до окончательной очистки",
        desc_en: "Number of days deleted notes remain in trash before purge",
        keywords: &["trash", "retention", "days", "корзина", "дни", "очистка"],
        aliases: &[],
        setting_type: SettingType::U32,
    },
];

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

pub fn search_settings(query: &str) -> Vec<&'static SettingDescriptor> {
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
        let title_ru = setting.title_ru.to_lowercase();
        let title_en = setting.title_en.to_lowercase();
        let desc_ru = setting.desc_ru.to_lowercase();
        let desc_en = setting.desc_en.to_lowercase();

        let mut score = 0i32;

        if id_lower == q || title_ru == q || title_en == q {
            score += 1000;
        } else if id_lower.contains(&q) || title_ru.contains(&q) || title_en.contains(&q) {
            score += 500;
        }

        let all_matched = terms.iter().all(|term| {
            id_lower.contains(term)
                || title_ru.contains(term)
                || title_en.contains(term)
                || desc_ru.contains(term)
                || desc_en.contains(term)
                || setting.keywords.iter().any(|kw| kw.to_lowercase().contains(term))
                || setting.aliases.iter().any(|al| al.to_lowercase().contains(term))
        });

        if all_matched {
            for term in &terms {
                if id_lower.contains(term) { score += 50; }
                if title_ru.contains(term) || title_en.contains(term) { score += 40; }
                if setting.keywords.iter().any(|kw| kw.to_lowercase().contains(term)) { score += 30; }
                if desc_ru.contains(term) || desc_en.contains(term) { score += 10; }
            }
            matches.push((score, setting));
        }
    }

    matches.sort_by(|a, b| b.0.cmp(&a.0));
    matches.into_iter().map(|(_, s)| s).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_settings_registry_ids_unique() {
        let mut ids = HashSet::new();
        for s in SETTINGS {
            assert!(ids.insert(s.id), "Duplicate setting id: {}", s.id);
            assert_eq!(s.url(), format!("{}://settings/{}", APP_URL_SCHEME, s.id));
        }
    }

    #[test]
    fn test_get_setting_by_id_and_alias() {
        let s = get_setting_by_id("ui.theme").expect("ui.theme must exist");
        assert_eq!(s.id, "ui.theme");

        let s_alias = get_setting_by_id("appearance.theme").expect("appearance.theme alias must resolve");
        assert_eq!(s_alias.id, "ui.theme");

        assert!(get_setting_by_id("non.existent.id").is_none());
    }

    #[test]
    fn test_get_and_set_config_value() {
        let mut config = AppConfig::default();
        let val = get_config_value(&config, "ui.theme").expect("ui.theme value");
        assert_eq!(val, serde_json::json!("system"));

        assert!(!is_setting_modified(&config, "ui.theme"));

        set_config_value(&mut config, "ui.theme", serde_json::json!("dark")).expect("set ui.theme");
        assert_eq!(config.ui.theme, "dark");
        assert!(is_setting_modified(&config, "ui.theme"));

        set_config_value(&mut config, "editor.wordWrap", serde_json::json!(false)).expect("set editor.wordWrap");
        assert!(!config.editor.word_wrap);
        assert!(is_setting_modified(&config, "editor.wordWrap"));
    }

    #[test]
    fn test_search_settings() {
        // Direct ID and URL match
        let res_id = search_settings("editor.wordWrap");
        assert_eq!(res_id.len(), 1);
        assert_eq!(res_id[0].id, "editor.wordWrap");

        let res_url = search_settings(&format!("{}://settings/editor.wordWrap", APP_URL_SCHEME));
        assert_eq!(res_url.len(), 1);
        assert_eq!(res_url[0].id, "editor.wordWrap");

        // Russian keyword search
        let res_ru = search_settings("перенос");
        assert!(res_ru.iter().any(|s| s.id == "editor.wordWrap"));

        // English keyword search
        let res_en = search_settings("wrap");
        assert!(res_en.iter().any(|s| s.id == "editor.wordWrap"));

        // Multi-keyword search
        let res_font = search_settings("шрифт размер");
        assert!(res_font.iter().any(|s| s.id == "editor.fontSizeBase"));
    }

    #[test]
    fn test_exact_reveal_cursor_default_and_serde() {
        let default_settings = EditorSettings::default();
        assert!(default_settings.exact_reveal_cursor);

        let json_empty = "{}";
        let parsed: EditorSettings = serde_json::from_str(json_empty).unwrap();
        assert!(parsed.exact_reveal_cursor);

        let json_false = r#"{"exactRevealCursor": false}"#;
        let parsed_false: EditorSettings = serde_json::from_str(json_false).unwrap();
        assert!(!parsed_false.exact_reveal_cursor);
    }

    #[test]
    fn test_editor_width_pct_default_and_serde() {
        let default_settings = EditorSettings::default();
        assert_eq!(default_settings.editor_width_pct, 100);

        // Deserializing empty JSON should default editor_width_pct to 100
        let json_empty = "{}";
        let parsed: EditorSettings = serde_json::from_str(json_empty).unwrap();
        assert_eq!(parsed.editor_width_pct, 100);

        // Deserializing with custom percentage (e.g. 45 from slider)
        let json_custom = r#"{"editorWidthPct": 45}"#;
        let parsed_custom: EditorSettings = serde_json::from_str(json_custom).unwrap();
        assert_eq!(parsed_custom.editor_width_pct, 45);

        // Serialization includes editorWidthPct
        let serialized = serde_json::to_string(&parsed_custom).unwrap();
        assert!(serialized.contains(r#""editorWidthPct":45"#));
    }

    #[test]
    fn test_note_align_default_and_serde() {
        let default_settings = EditorSettings::default();
        assert_eq!(default_settings.note_align, NoteAlignment::Center);

        // Deserializing empty JSON should default note_align to Center
        let json_empty = "{}";
        let parsed: EditorSettings = serde_json::from_str(json_empty).unwrap();
        assert_eq!(parsed.note_align, NoteAlignment::Center);

        // Deserializing with custom alignment "left"
        let json_left = r#"{"noteAlign": "left"}"#;
        let parsed_left: EditorSettings = serde_json::from_str(json_left).unwrap();
        assert_eq!(parsed_left.note_align, NoteAlignment::Left);

        // Deserializing with custom alignment "center"
        let json_center = r#"{"noteAlign": "center"}"#;
        let parsed_center: EditorSettings = serde_json::from_str(json_center).unwrap();
        assert_eq!(parsed_center.note_align, NoteAlignment::Center);

        // Serialization includes noteAlign
        let serialized = serde_json::to_string(&parsed_left).unwrap();
        assert!(serialized.contains(r#""noteAlign":"left""#));
    }

    #[test]
    fn test_title_align_default_and_serde() {
        let default_settings = EditorSettings::default();
        assert_eq!(default_settings.title_align, TitleAlignment::WindowLeft);

        // Deserializing empty JSON should default title_align to WindowLeft
        let json_empty = "{}";
        let parsed: EditorSettings = serde_json::from_str(json_empty).unwrap();
        assert_eq!(parsed.title_align, TitleAlignment::WindowLeft);

        // Deserializing legacy "center" and "left" aliases
        let json_legacy_center = r#"{"titleAlign": "center"}"#;
        let parsed_legacy_center: EditorSettings = serde_json::from_str(json_legacy_center).unwrap();
        assert_eq!(parsed_legacy_center.title_align, TitleAlignment::WindowCenter);

        let json_legacy_left = r#"{"titleAlign": "left"}"#;
        let parsed_legacy_left: EditorSettings = serde_json::from_str(json_legacy_left).unwrap();
        assert_eq!(parsed_legacy_left.title_align, TitleAlignment::WindowLeft);

        // Deserializing editor_left and editor_center
        let json_editor_left = r#"{"titleAlign": "editor_left"}"#;
        let parsed_editor_left: EditorSettings = serde_json::from_str(json_editor_left).unwrap();
        assert_eq!(parsed_editor_left.title_align, TitleAlignment::EditorLeft);

        let json_editor_center = r#"{"titleAlign": "editor_center"}"#;
        let parsed_editor_center: EditorSettings = serde_json::from_str(json_editor_center).unwrap();
        assert_eq!(parsed_editor_center.title_align, TitleAlignment::EditorCenter);

        // Serialization produces snake_case
        let serialized = serde_json::to_string(&parsed_editor_center).unwrap();
        assert!(serialized.contains(r#""titleAlign":"editor_center""#));
    }
}
