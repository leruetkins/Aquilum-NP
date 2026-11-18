use std::borrow::Cow;

pub static ROBOTO_MONO_REGULAR: &[u8] = include_bytes!("../assets/fonts/roboto_mono_regular.ttf");
pub static ROBOTO_MONO_BOLD: &[u8] = include_bytes!("../assets/fonts/roboto_mono_bold.ttf");
pub static ROBOTO_MONO_ITALIC: &[u8] = include_bytes!("../assets/fonts/roboto_mono_italic.ttf");
pub static ROBOTO_MONO_BOLD_ITALIC: &[u8] = include_bytes!("../assets/fonts/roboto_mono_bold_italic.ttf");
pub static DROID_SANS_MONO: &[u8] = include_bytes!("../assets/fonts/droid_sans_mono.ttf");
pub static DROID_SANS_MONO_BOLD: &[u8] = include_bytes!("../assets/fonts/droid_sans_mono_bold.ttf");
pub static INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/inter_regular.ttf");
pub static INTER_MEDIUM: &[u8] = include_bytes!("../assets/fonts/inter_medium.ttf");
pub static INTER_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/inter_semibold.ttf");
pub static INTER_BOLD: &[u8] = include_bytes!("../assets/fonts/inter_bold.ttf");
pub static INTER_ITALIC: &[u8] = include_bytes!("../assets/fonts/inter_italic.ttf");
pub static INTER_BOLD_ITALIC: &[u8] = include_bytes!("../assets/fonts/inter_bolditalic.ttf");

/// Available monospace editor font families.
pub const EDITOR_FONT_CHOICES: &[&str] = &[
    "Roboto Mono",
    "Droid Sans Mono",
    "Consolas",
    "Cascadia Code",
    "Cascadia Mono",
    "Courier New",
    "DejaVu Sans Mono",
];

/// Available interface font families. "Inter" is bundled with the app.
pub const UI_FONT_CHOICES: &[&str] = &["Inter", "Segoe UI"];

/// Font family used by interface text that does not pick a family explicitly.
///
/// Set from `ui.fontFamily` at startup and whenever the setting changes, so
/// the many UI call sites that used to hardcode "Segoe UI" stay consistent.
static UI_FONT_FAMILY: std::sync::OnceLock<std::sync::RwLock<&'static str>> =
    std::sync::OnceLock::new();

fn ui_font_cell() -> &'static std::sync::RwLock<&'static str> {
    UI_FONT_FAMILY.get_or_init(|| std::sync::RwLock::new("Segoe UI"))
}

pub fn set_ui_font_family(family: &str) {
    let family = family.trim();
    let family = if family.is_empty() { "Segoe UI" } else { family };
    if let Ok(mut current) = ui_font_cell().write() {
        if *current != family {
            *current = Box::leak(family.to_string().into_boxed_str());
        }
    }
}

/// The configured interface font family. Leaked on change (settings are
/// changed rarely) so call sites can keep using `&'static str` APIs.
pub fn ui_font_family() -> &'static str {
    *ui_font_cell()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Registers all bundled embedded fonts into the GPUI TextSystem.
pub fn register_embedded_fonts(cx: &gpui::App) {
    let fonts: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(ROBOTO_MONO_REGULAR),
        Cow::Borrowed(ROBOTO_MONO_BOLD),
        Cow::Borrowed(ROBOTO_MONO_ITALIC),
        Cow::Borrowed(ROBOTO_MONO_BOLD_ITALIC),
        Cow::Borrowed(DROID_SANS_MONO),
        Cow::Borrowed(DROID_SANS_MONO_BOLD),
        Cow::Borrowed(INTER_REGULAR),
        Cow::Borrowed(INTER_MEDIUM),
        Cow::Borrowed(INTER_SEMIBOLD),
        Cow::Borrowed(INTER_BOLD),
        Cow::Borrowed(INTER_ITALIC),
        Cow::Borrowed(INTER_BOLD_ITALIC),
    ];
    if let Err(err) = cx.text_system().add_fonts(fonts) {
        eprintln!("[Fonts] Failed to load embedded fonts: {err}");
    }
}
