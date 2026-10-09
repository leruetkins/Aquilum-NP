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

pub struct MemoryAssetSource;

impl gpui::AssetSource for MemoryAssetSource {
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
            _ => None,
        };
        Ok(svg.map(|s| Cow::Borrowed(s.as_bytes())))
    }

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
        ])
    }
}

pub fn icon(path: &'static str, size: f32, color: impl Into<Hsla>) -> Svg {
    gpui::svg()
        .path(path)
        .w(px(size))
        .h(px(size))
        .flex_shrink_0()
        .text_color(color.into())
}
