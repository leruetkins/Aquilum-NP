#![allow(dead_code)]

use std::borrow::Cow;
use gpui::{px, Hsla, Result, SharedString, Styled, Svg};

pub const CHEVRON_RIGHT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 18 6-6-6-6"/></svg>"#;
pub const CHEVRON_DOWN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"#;
pub const FOLDER: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 8 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/></svg>"#;
pub const FOLDER_OPEN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.5 6A2 2 0 0 1 18.5 20H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.69.9H18a2 2 0 0 1 2 2v2"/></svg>"#;
pub const FILE_TEXT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/><path d="M10 9H8"/><path d="M16 13H8"/><path d="M16 17H8"/></svg>"#;
pub const PLUS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14"/><path d="M12 5v14"/></svg>"#;
pub const SAVE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15.2 3a2 2 0 0 1 1.4.6l3.8 3.8a2 2 0 0 1 .6 1.4V19a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z"/><path d="M17 21v-7a1 1 0 0 0-1-1H8a1 1 0 0 0-1 1v7"/><path d="M7 3v4a1 1 0 0 0 1 1h7"/></svg>"#;
pub const EYE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2.062 12.348a1 1 0 0 1 0-.696 10.75 10.75 0 0 1 19.876 0 1 1 0 .696 10.75 10.75 0 0 1-19.876 0"/><circle cx="12" cy="12" r="3"/></svg>"#;
pub const PENCIL: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z"/><path d="m15 5 4 4"/></svg>"#;
pub const REFRESH: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M8 16H3v5"/></svg>"#;
pub const SEARCH: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/></svg>"#;
pub const BOLT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/></svg>"#;
pub const CODE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>"#;
pub const CLOSE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>"#;
pub const PANEL_LEFT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="18" x="3" y="3" rx="2"/><path d="M9 3v18"/></svg>"#;
pub const SETTINGS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>"#;
pub const MONITOR: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="20" height="14" x="2" y="3" rx="2"/><line x1="8" x2="16" y1="21" y2="21"/><line x1="12" x2="12" y1="17" y2="21"/></svg>"#;
pub const TYPE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 7 4 4 20 4 20 7"/><line x1="9" x2="15" y1="20" y2="20"/><line x1="12" x2="12" y1="4" y2="20"/></svg>"#;
pub const BOOK_OPEN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z"/><path d="M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z"/></svg>"#;
pub const PLUG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 22v-5"/><path d="M9 8V2"/><path d="M15 8V2"/><path d="M18 8v5a4 4 0 0 1-4 4h-4a4 4 0 0 1-4-4V8Z"/></svg>"#;
pub const HISTORY: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/><path d="M12 7v5l4 2"/></svg>"#;
pub const TRASH_2: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/><line x1="10" x2="10" y1="11" y2="17"/><line x1="14" x2="14" y1="11" y2="17"/></svg>"#;
pub const KEYBOARD: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="20" height="16" x="2" y="4" rx="2"/><path d="M6 8h.001"/><path d="M10 8h.001"/><path d="M14 8h.001"/><path d="M18 8h.001"/><path d="M6 12h.001"/><path d="M10 12h.001"/><path d="M14 12h.001"/><path d="M18 12h.001"/><path d="M7 16h10"/></svg>"#;
pub const POWER: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2v10"/><path d="M18.4 6.6a9 9 0 1 1-12.77.04"/></svg>"#;
pub const ANALYSIS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="16" y="16" width="6" height="6" rx="1"/><rect x="2" y="16" width="6" height="6" rx="1"/><rect x="9" y="2" width="6" height="6" rx="1"/><path d="M5 16v-3a1 1 0 0 1 1-1h12a1 1 0 0 1 1 1v3"/><path d="M12 12V8"/></svg>"#;
pub const PAPERCLIP: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48"/></svg>"#;
pub const CHECK: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>"#;
pub const MINIMIZE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round"><line x1="5" y1="12" x2="19" y2="12"/></svg>"#;
pub const MAXIMIZE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round"><rect width="14" height="14" x="5" y="5" rx="1"/></svg>"#;
pub const RESTORE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round"><rect width="11" height="11" x="8" y="5" rx="1"/><path d="M5 9v10h10"/></svg>"#;
pub const NOTE_CREATE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.375 2.625a1 1 0 0 1 3 3l-9.013 9.014a2 2 0 0 1-.853.505l-2.873.84a.5.5 0 0 1-.62-.62l.84-2.873a2 2 0 0 1 .506-.852z"/></svg>"#;
pub const CALENDAR: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 2v4"/><path d="M16 2v4"/><rect width="18" height="18" x="3" y="4" rx="2"/><path d="M3 10h18"/></svg>"#;
pub const TERMINAL: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 17 10 11 4 5"/><line x1="12" x2="20" y1="19" y2="19"/></svg>"#;

pub const ICON_CHEVRON_RIGHT: &str = "icons/chevron_right.svg";
pub const ICON_CHEVRON_DOWN: &str = "icons/chevron_down.svg";
pub const ICON_FOLDER: &str = "icons/folder.svg";
pub const ICON_FOLDER_OPEN: &str = "icons/folder_open.svg";
pub const ICON_FILE_TEXT: &str = "icons/file_text.svg";
pub const ICON_PLUS: &str = "icons/plus.svg";
pub const ICON_SAVE: &str = "icons/save.svg";
pub const ICON_EYE: &str = "icons/eye.svg";
pub const ICON_PENCIL: &str = "icons/pencil.svg";
pub const ICON_REFRESH: &str = "icons/refresh.svg";
pub const ICON_SEARCH: &str = "icons/search.svg";
pub const ICON_BOLT: &str = "icons/bolt.svg";
pub const ICON_CODE: &str = "icons/code.svg";
pub const ICON_CLOSE: &str = "icons/close.svg";
pub const ICON_PANEL_LEFT: &str = "icons/panel_left.svg";
pub const ICON_SETTINGS: &str = "icons/settings.svg";
pub const ICON_MONITOR: &str = "icons/monitor.svg";
pub const ICON_TYPE: &str = "icons/type.svg";
pub const ICON_BOOK_OPEN: &str = "icons/book_open.svg";
pub const ICON_PLUG: &str = "icons/plug.svg";
pub const ICON_HISTORY: &str = "icons/history.svg";
pub const ICON_TRASH_2: &str = "icons/trash_2.svg";
pub const ICON_KEYBOARD: &str = "icons/keyboard.svg";
pub const ICON_POWER: &str = "icons/power.svg";
pub const ICON_ANALYSIS: &str = "icons/analysis.svg";
pub const ICON_PAPERCLIP: &str = "icons/paperclip.svg";
pub const ICON_CHECK: &str = "icons/check.svg";
pub const ICON_MINIMIZE: &str = "icons/minimize.svg";
pub const ICON_MAXIMIZE: &str = "icons/maximize.svg";
pub const ICON_RESTORE: &str = "icons/restore.svg";
pub const ICON_NOTE_CREATE: &str = "icons/note_create.svg";
pub const ICON_CALENDAR: &str = "icons/calendar.svg";
pub const ICON_TERMINAL: &str = "icons/terminal.svg";

pub struct MemoryAssetSource;

impl gpui::AssetSource for MemoryAssetSource {
    /// Loads the in-memory SVG asset by virtual file path.
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let svg = match path {
            ICON_CHEVRON_RIGHT => Some(CHEVRON_RIGHT),
            ICON_CHEVRON_DOWN => Some(CHEVRON_DOWN),
            ICON_FOLDER => Some(FOLDER),
            ICON_FOLDER_OPEN => Some(FOLDER_OPEN),
            ICON_FILE_TEXT => Some(FILE_TEXT),
            ICON_PLUS => Some(PLUS),
            ICON_SAVE => Some(SAVE),
            ICON_EYE => Some(EYE),
            ICON_PENCIL => Some(PENCIL),
            ICON_REFRESH => Some(REFRESH),
            ICON_SEARCH => Some(SEARCH),
            ICON_BOLT => Some(BOLT),
            ICON_CODE => Some(CODE),
            ICON_CLOSE => Some(CLOSE),
            ICON_PANEL_LEFT => Some(PANEL_LEFT),
            ICON_SETTINGS => Some(SETTINGS),
            ICON_MONITOR => Some(MONITOR),
            ICON_TYPE => Some(TYPE),
            ICON_BOOK_OPEN => Some(BOOK_OPEN),
            ICON_PLUG => Some(PLUG),
            ICON_HISTORY => Some(HISTORY),
            ICON_TRASH_2 => Some(TRASH_2),
            ICON_KEYBOARD => Some(KEYBOARD),
            ICON_POWER => Some(POWER),
            ICON_ANALYSIS => Some(ANALYSIS),
            ICON_PAPERCLIP => Some(PAPERCLIP),
            ICON_CHECK => Some(CHECK),
            ICON_MINIMIZE => Some(MINIMIZE),
            ICON_MAXIMIZE => Some(MAXIMIZE),
            ICON_RESTORE => Some(RESTORE),
            ICON_NOTE_CREATE => Some(NOTE_CREATE),
            ICON_CALENDAR => Some(CALENDAR),
            ICON_TERMINAL => Some(TERMINAL),
            _ => None,
        };
        Ok(svg.map(|s| Cow::Borrowed(s.as_bytes())))
    }

    /// Lists all registered virtual SVG asset paths.
    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(vec![
            ICON_CHEVRON_RIGHT.into(),
            ICON_CHEVRON_DOWN.into(),
            ICON_FOLDER.into(),
            ICON_FOLDER_OPEN.into(),
            ICON_FILE_TEXT.into(),
            ICON_PLUS.into(),
            ICON_SAVE.into(),
            ICON_EYE.into(),
            ICON_PENCIL.into(),
            ICON_REFRESH.into(),
            ICON_SEARCH.into(),
            ICON_BOLT.into(),
            ICON_CODE.into(),
            ICON_CLOSE.into(),
            ICON_PANEL_LEFT.into(),
            ICON_SETTINGS.into(),
            ICON_MONITOR.into(),
            ICON_TYPE.into(),
            ICON_BOOK_OPEN.into(),
            ICON_PLUG.into(),
            ICON_HISTORY.into(),
            ICON_TRASH_2.into(),
            ICON_KEYBOARD.into(),
            ICON_POWER.into(),
            ICON_ANALYSIS.into(),
            ICON_PAPERCLIP.into(),
            ICON_CHECK.into(),
            ICON_MINIMIZE.into(),
            ICON_MAXIMIZE.into(),
            ICON_RESTORE.into(),
            ICON_NOTE_CREATE.into(),
            ICON_CALENDAR.into(),
            ICON_TERMINAL.into(),
        ])
    }
}

/// Renders an SVG icon with given dimensions and color.
pub fn icon(path: &'static str, size: f32, color: impl Into<Hsla>) -> Svg {
    gpui::svg()
        .path(path)
        .w(px(size))
        .h(px(size))
        .flex_shrink_0()
        .text_color(color.into())
}

