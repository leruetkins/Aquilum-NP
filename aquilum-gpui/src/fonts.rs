use std::borrow::Cow;

pub static ROBOTO_MONO_REGULAR: &[u8] = include_bytes!("../assets/fonts/roboto_mono_regular.ttf");
pub static ROBOTO_MONO_BOLD: &[u8] = include_bytes!("../assets/fonts/roboto_mono_bold.ttf");
pub static ROBOTO_MONO_ITALIC: &[u8] = include_bytes!("../assets/fonts/roboto_mono_italic.ttf");
pub static ROBOTO_MONO_BOLD_ITALIC: &[u8] = include_bytes!("../assets/fonts/roboto_mono_bold_italic.ttf");
pub static DROID_SANS_MONO: &[u8] = include_bytes!("../assets/fonts/droid_sans_mono.ttf");
pub static DROID_SANS_MONO_BOLD: &[u8] = include_bytes!("../assets/fonts/droid_sans_mono_bold.ttf");

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

/// Registers all bundled embedded fonts into the GPUI TextSystem.
pub fn register_embedded_fonts(cx: &gpui::App) {
    let fonts: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(ROBOTO_MONO_REGULAR),
        Cow::Borrowed(ROBOTO_MONO_BOLD),
        Cow::Borrowed(ROBOTO_MONO_ITALIC),
        Cow::Borrowed(ROBOTO_MONO_BOLD_ITALIC),
        Cow::Borrowed(DROID_SANS_MONO),
        Cow::Borrowed(DROID_SANS_MONO_BOLD),
    ];
    if let Err(err) = cx.text_system().add_fonts(fonts) {
        eprintln!("[Fonts] Failed to load embedded fonts: {err}");
    }
}
