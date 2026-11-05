#![allow(dead_code)]

use std::borrow::Cow;
use gpui::{px, Hsla, Result, SharedString, Styled, Svg};

pub const CHEVRON_RIGHT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 18 6-6-6-6"/></svg>"#;
pub const CHEVRON_DOWN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"#;
pub const CHEVRONS_UP_DOWN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m7 15 5 5 5-5"/><path d="m7 9 5-5 5 5"/></svg>"#;
pub const CHEVRONS_DOWN_UP: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m7 4 5 5 5-5"/><path d="m7 20 5-5 5 5"/></svg>"#;
pub const FOLDER: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 8 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/></svg>"#;
pub const FOLDER_PLUS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 10v6"/><path d="M9 13h6"/><path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 8 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/></svg>"#;
pub const FOLDER_OPEN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.5 6A2 2 0 0 1 18.5 20H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.69.9H18a2 2 0 0 1 2 2v2"/></svg>"#;
pub const FILE_TEXT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/><path d="M10 9H8"/><path d="M16 13H8"/><path d="M16 17H8"/></svg>"#;
pub const PLUS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14"/><path d="M12 5v14"/></svg>"#;
pub const SAVE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15.2 3a2 2 0 0 1 1.4.6l3.8 3.8a2 2 0 0 1 .6 1.4V19a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z"/><path d="M17 21v-7a1 1 0 0 0-1-1H8a1 1 0 0 0-1 1v7"/><path d="M7 3v4a1 1 0 0 0 1 1h7"/></svg>"#;
pub const EYE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2.062 12.348a1 1 0 0 1 0-.696 10.75 10.75 0 0 1 19.876 0 1 1 0 .696 10.75 10.75 0 0 1-19.876 0"/><circle cx="12" cy="12" r="3"/></svg>"#;
pub const PENCIL: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z"/><path d="m15 5 4 4"/></svg>"#;
pub const REFRESH: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M8 16H3v5"/></svg>"#;
pub const SEARCH: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/></svg>"#;
pub const GLOBE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"/><path d="M2 12h20"/></svg>"#;
pub const BOLT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/></svg>"#;
pub const CODE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>"#;
pub const CLOSE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>"#;
pub const PANEL_LEFT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="18" x="3" y="3" rx="2"/><path d="M9 3v18"/></svg>"#;
pub const PANEL_RIGHT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="18" x="3" y="3" rx="2"/><path d="M15 3v18"/></svg>"#;
pub const LAYERS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="12 2 2 7 12 12 22 7 12 2"/><polyline points="2 17 12 22 22 17"/><polyline points="2 12 12 17 22 12"/></svg>"#;
pub const BOOKMARK: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m19 21-7-4-7 4V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v16z"/></svg>"#;
pub const BOOKMARK_FILLED: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="black" stroke="none"><path d="m19 21-7-4-7 4V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v16z"/></svg>"#;
pub const SETTINGS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>"#;
pub const SLIDERS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="21" x2="14" y1="4" y2="4"/><line x1="10" x2="3" y1="4" y2="4"/><line x1="21" x2="12" y1="12" y2="12"/><line x1="8" x2="3" y1="12" y2="12"/><line x1="21" x2="16" y1="20" y2="20"/><line x1="12" x2="3" y1="20" y2="20"/><line x1="14" x2="14" y1="2" y2="6"/><line x1="8" x2="8" y1="10" y2="14"/><line x1="16" x2="16" y1="18" y2="22"/></svg>"#;
pub const MONITOR: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="20" height="14" x="2" y="3" rx="2"/><line x1="8" x2="16" y1="21" y2="21"/><line x1="12" x2="12" y1="17" y2="21"/></svg>"#;
pub const SUN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="4"/><path d="M12 2v2"/><path d="M12 20v2"/><path d="m4.93 4.93 1.41 1.41"/><path d="m17.66 17.66 1.41 1.41"/><path d="M2 12h2"/><path d="M20 12h2"/><path d="m6.34 17.66-1.41 1.41"/><path d="m19.07 4.93-1.41 1.41"/></svg>"#;
pub const MOON: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/></svg>"#;
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
pub const PIPETTE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m14 7 3 3"/><path d="m5 16-1 4 4-1 10.7-10.7a2.4 2.4 0 0 0 0-3.4l-1.6-1.6a2.4 2.4 0 0 0-3.4 0Z"/><path d="m16 5 3 3"/><path d="m2 22 3-3"/></svg>"#;
pub const MINIMIZE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12"><rect fill="currentColor" width="10" height="1" x="1" y="6"/></svg>"#;
pub const MAXIMIZE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12" fill="none"><rect width="9" height="9" x="1.5" y="1.5" stroke="currentColor" stroke-width="1"/></svg>"#;
pub const RESTORE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12" fill="none"><path d="M1.5 3.5H8.5V10.5H1.5V3.5Z" stroke="currentColor" stroke-width="1"/><path d="M4 2H10V8H9V9H11V1H3V3H4V2Z" fill="currentColor"/></svg>"#;
pub const WINDOW_CLOSE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12"><path fill="currentColor" fill-rule="evenodd" d="M10.052 10.968 1.03 1.93l.849-.848 9.023 9.037-.849.848Z"/><path fill="currentColor" fill-rule="evenodd" d="M1.023 10.112 10.06 1.09l.848.85-9.037 9.023-.848-.85Z"/></svg>"#;
pub const NOTE_CREATE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.375 2.625a1 1 0 0 1 3 3l-9.013 9.014a2 2 0 0 1-.853.505l-2.873.84a.5.5 0 0 1-.62-.62l.84-2.873a2 2 0 0 1 .506-.852z"/></svg>"#;
pub const CALENDAR: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 2v4"/><path d="M16 2v4"/><rect width="18" height="18" x="3" y="4" rx="2"/><path d="M3 10h18"/></svg>"#;
pub const INFO: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/></svg>"#;
pub const TRIANGLE_ALERT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/><path d="M12 9v4"/><path d="M12 17h.01"/></svg>"#;
pub const CIRCLE_ALERT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="12" x2="12" y1="8" y2="12"/><line x1="12" x2="12.01" y1="16" y2="16"/></svg>"#;
pub const QUOTE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M16 3a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2 1 1 0 0 1 1 1v1a2 2 0 0 1-2 2 1 1 0 0 0-1 1v2a1 1 0 0 0 1 1 6 6 0 0 0 6-6V5a2 2 0 0 0-2-2z"/><path d="M5 3a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2 1 1 0 0 1 1 1v1a2 2 0 0 1-2 2 1 1 0 0 0-1 1v2a1 1 0 0 0 1 1 6 6 0 0 0 6-6V5a2 2 0 0 0-2-2z"/></svg>"#;
pub const TERMINAL: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 17 10 11 4 5"/><line x1="12" x2="20" y1="19" y2="19"/></svg>"#;
pub const ARROW_LEFT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m12 19-7-7 7-7"/><path d="M19 12H5"/></svg>"#;
pub const ARROW_RIGHT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14"/><path d="m12 5 7 7-7 7"/></svg>"#;
pub const MORE_VERTICAL: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="1"/><circle cx="12" cy="5" r="1"/><circle cx="12" cy="19" r="1"/></svg>"#;
pub const TAB_ACTIVE_LEFT_FILL: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 36" width="16" height="36"><path d="M 16 0 H 14 C 10.686 0 8 2.686 8 6 V 26 C 8 30.418 4.418 34 0 34 V 36 H 16 Z" fill="currentColor"/></svg>"#;
pub const TAB_ACTIVE_LEFT_STROKE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 36" width="16" height="36" fill="none"><path d="M 0 33.5 L 0.5 33.5 C 4.918 33.5 8.5 30.142 8.5 26 V 6 C 8.5 2.962 10.962 0.5 14 0.5 H 16" stroke="currentColor" stroke-width="1"/></svg>"#;
pub const TAB_ACTIVE_RIGHT_FILL: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 36" width="16" height="36"><path d="M 0 0 H 2 C 5.314 0 8 2.686 8 6 V 26 C 8 30.418 11.582 34 16 34 V 36 H 0 Z" fill="currentColor"/></svg>"#;
pub const TAB_ACTIVE_RIGHT_STROKE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 36" width="16" height="36" fill="none"><path d="M 0 0.5 H 2 C 5.038 0.5 7.5 2.962 7.5 6 V 26 C 7.5 30.142 11.082 33.5 15.5 33.5 L 16 33.5" stroke="currentColor" stroke-width="1"/></svg>"#;
pub const COPY: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>"#;
pub const FOLDER_INPUT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 9V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.69.9H20a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2v-1"/><path d="M2 13h10"/><path d="m9 10 3 3-3 3"/></svg>"#;
pub const EXTERNAL_LINK: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15 3h6v6"/><path d="M10 14 21 3"/><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/></svg>"#;
pub const CANVAS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="7" height="7" x="3" y="3" rx="1"/><rect width="7" height="7" x="14" y="3" rx="1"/><rect width="7" height="7" x="14" y="14" rx="1"/><rect width="7" height="7" x="3" y="14" rx="1"/></svg>"#;
pub const DRAWING: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4.5 16.5c-1.5 1.26-2 5-2 5s3.74-.5 5-2c.71-.84.7-2.13-.09-2.91a2.18 2.18 0 0 0-2.91-.09z"/><path d="m12 15-3-3a22 22 0 0 1 2-3.95A12.88 12.88 0 0 1 22 2c0 2.72-.78 7.5-6 11a22.35 22.35 0 0 1-4 2z"/><path d="M9 12H4s.55-3.03 2-4c1.62-1.08 5 0 5 0"/><path d="M12 15v5s3.03-.55 4-2c1.08-1.62 0-5 0-5"/></svg>"#;
pub const HASH: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="4" x2="20" y1="9" y2="9"/><line x1="4" x2="20" y1="15" y2="15"/><line x1="10" x2="8" y1="3" y2="21"/><line x1="16" x2="14" y1="3" y2="21"/></svg>"#;
pub const DOWNLOAD: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" x2="12" y1="15" y2="3"/></svg>"#;
pub const MOVE_ARROWS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="5 9 2 12 5 15"/><polyline points="9 5 12 2 15 5"/><polyline points="15 19 12 22 9 19"/><polyline points="19 9 22 12 19 15"/><line x1="2" x2="22" y1="12" y2="12"/><line x1="12" x2="12" y1="2" y2="22"/></svg>"#;
pub const SPLIT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="18" x="3" y="3" rx="2"/><path d="M12 3v18"/></svg>"#;
pub const APP_WINDOW: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="4" width="20" height="16" rx="2"/><path d="M10 4v4"/><path d="M2 8h20"/><path d="M6 4v4"/></svg>"#;
pub const MERGE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="18" cy="18" r="3"/><circle cx="6" cy="6" r="3"/><path d="M6 21V9a9 9 0 0 0 9 9"/></svg>"#;
pub const LINK: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>"#;
pub const VAULT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="18" x="3" y="3" rx="2"/><circle cx="12" cy="12" r="4"/><line x1="12" x2="12" y1="8" y2="10"/><line x1="12" x2="12" y1="14" y2="16"/><line x1="8" x2="10" y1="12" y2="12"/><line x1="14" x2="16" y1="12" y2="12"/></svg>"#;
pub const COMPUTER: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="16" height="12" x="4" y="3" rx="2"/><path d="M2 18h20"/><path d="M12 15v3"/></svg>"#;
pub const PIN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 17v5"/><path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V6a1 1 0 0 0-1-1H8a2 2 0 0 0-2 2z"/><path d="M5 8V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v3"/></svg>"#;
pub const ALIGN_LEFT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="21" x2="3" y1="6" y2="6"/><line x1="15" x2="3" y1="12" y2="12"/><line x1="17" x2="3" y1="18" y2="18"/></svg>"#;
pub const ALIGN_CENTER: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="21" x2="3" y1="6" y2="6"/><line x1="17" x2="7" y1="12" y2="12"/><line x1="19" x2="5" y1="18" y2="18"/></svg>"#;
pub const ALIGN_RIGHT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="21" x2="3" y1="6" y2="6"/><line x1="21" x2="9" y1="12" y2="12"/><line x1="21" x2="7" y1="18" y2="18"/></svg>"#;
pub const TABLE_MERGE_CELLS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="18" x="3" y="3" rx="2"/><path d="M3 12h18"/><path d="M12 3v18"/></svg>"#;
pub const TABLE_SPLIT_CELLS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="18" x="3" y="3" rx="2"/><path d="M3 12h18"/><path d="M12 3v9"/></svg>"#;
pub const TABLE_DELETE_ROW: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="18" x="3" y="3" rx="2"/><path d="M3 9h18"/><path d="M3 15h18"/></svg>"#;
pub const TABLE_DELETE_COL: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="18" x="3" y="3" rx="2"/><path d="M9 3v18"/><path d="M15 3v18"/></svg>"#;
pub const SPLIT_DOWN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="18" x="3" y="3" rx="2"/><path d="M3 12h18"/></svg>"#;
pub const LOCK: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="11" x="3" y="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>"#;
pub const CIRCLE_PLUS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="M8 12h8"/><path d="M12 8v8"/></svg>"#;
pub const SORT_ASC: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m3 8 4-4 4 4"/><path d="M7 4v16"/><path d="M11 12h4"/><path d="M11 16h7"/><path d="M11 20h10"/></svg>"#;
pub const SORT_DESC: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m3 16 4 4 4-4"/><path d="M7 20V4"/><path d="M11 4h10"/><path d="M11 8h7"/><path d="M11 12h4"/></svg>"#;

pub const UNDO: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 7v6h6"/><path d="M21 17a9 9 0 0 0-9-9 9 9 0 0 0-6 2.3L3 13"/></svg>"#;
pub const REDO: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 7v6h-6"/><path d="M3 17a9 9 0 0 1 9-9 9 9 0 0 1 6 2.3l3 2.7"/></svg>"#;
pub const ERASER: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m7 21-4.3-4.3c-1-1-1-2.5 0-3.4l9.6-9.6c1-1 2.5-1 3.4 0l5.6 5.6c1 1 1 2.5 0 3.4L13 21"/><path d="M22 21H7"/><path d="m5 11 9 9"/></svg>"#;
pub const BOLD: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 12h9a4 4 0 0 1 0 8H7a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1h7a4 4 0 0 1 0 8"/></svg>"#;
pub const ITALIC: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="19" x2="10" y1="4" y2="4"/><line x1="14" x2="5" y1="20" y2="20"/><line x1="15" x2="9" y1="4" y2="20"/></svg>"#;
pub const STRIKETHROUGH: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M16 4H9a3 3 0 0 0-2.83 4"/><path d="M14 12a4 4 0 0 1 0 8H6"/><line x1="4" x2="20" y1="12" y2="12"/></svg>"#;
pub const UNDERLINE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 4v6a6 6 0 0 0 12 0V4"/><line x1="4" x2="20" y1="20" y2="20"/></svg>"#;
pub const HIGHLIGHTER: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 11-6 6v3h9l3-3"/><path d="m22 12-4.6 4.6a2 2 0 0 1-2.8 0l-5.2-5.2a2 2 0 0 1 0-2.8L14 4"/></svg>"#;
pub const HEADING_1: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 12h8"/><path d="M4 18V6"/><path d="M12 18V6"/><path d="m17 12 3-2v8"/></svg>"#;
pub const HEADING_2: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 12h8"/><path d="M4 18V6"/><path d="M12 18V6"/><path d="M21 18h-4c0-4 4-3 4-6 0-1.5-2-2.5-4-1"/></svg>"#;
pub const HEADING_3: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 12h8"/><path d="M4 18V6"/><path d="M12 18V6"/><path d="M17.5 10.5c1.7-1 3.5 0 3.5 1.5a2 2 0 0 1-2 2"/><path d="M17 17.5c2 1.5 4 .3 4-1.5a2 2 0 0 0-2-2"/></svg>"#;
pub const HEADING_4: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 12h8"/><path d="M4 18V6"/><path d="M12 18V6"/><path d="M17 10v4h4"/><path d="M21 10v8"/></svg>"#;
pub const HEADING_5: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 12h8"/><path d="M4 18V6"/><path d="M12 18V6"/><path d="M17 13v-3h4"/><path d="M17 17.7c.4.2.8.3 1.3.3 1.5 0 2.7-1.1 2.7-2.5s-1.2-2.5-2.7-2.5h-1.3"/></svg>"#;
pub const HEADING_6: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 12h8"/><path d="M4 18V6"/><path d="M12 18V6"/><circle cx="19" cy="15" r="2.5"/><path d="M20 10.8A3.5 3.5 0 0 0 16.5 14"/></svg>"#;
pub const HEADING_N: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 12h8"/><path d="M4 18V6"/><path d="M12 18V6"/><path d="M17 18V12l4 6V12"/></svg>"#;
pub const TABLE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3v18"/><rect width="18" height="18" x="3" y="3" rx="2"/><path d="M3 9h18"/><path d="M3 15h18"/></svg>"#;
pub const LIST_BULLET: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="8" x2="21" y1="6" y2="6"/><line x1="8" x2="21" y1="12" y2="12"/><line x1="8" x2="21" y1="18" y2="18"/><line x1="3" x2="3.01" y1="6" y2="6"/><line x1="3" x2="3.01" y1="12" y2="12"/><line x1="3" x2="3.01" y1="18" y2="18"/></svg>"#;
pub const LIST_NUMBERED: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="10" x2="21" y1="6" y2="6"/><line x1="10" x2="21" y1="12" y2="12"/><line x1="10" x2="21" y1="18" y2="18"/><path d="M4 6h1v4"/><path d="M4 10h2"/><path d="M6 18H4c0-1 2-2 2-3s-1-1.5-2-1"/></svg>"#;
pub const LIST_TASK: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="5" width="6" height="6" rx="1"/><path d="m3 17 2 2 4-4"/><line x1="13" x2="21" y1="6" y2="6"/><line x1="13" x2="21" y1="12" y2="12"/><line x1="13" x2="21" y1="18" y2="18"/></svg>"#;
pub const INDENT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 8 7 12 3 16"/><line x1="21" x2="11" y1="12" y2="12"/><line x1="21" x2="11" y1="6" y2="6"/><line x1="21" x2="11" y1="18" y2="18"/></svg>"#;
pub const OUTDENT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="7 8 3 12 7 16"/><line x1="21" x2="11" y1="12" y2="12"/><line x1="21" x2="11" y1="6" y2="6"/><line x1="21" x2="11" y1="18" y2="18"/></svg>"#;
pub const MESSAGE_SQUARE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>"#;
pub const FULLSCREEN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 3 21 3 21 9"/><polyline points="9 21 3 21 3 15"/><line x1="21" x2="14" y1="3" y2="10"/><line x1="3" x2="10" y1="21" y2="14"/></svg>"#;
pub const MORE_HORIZONTAL: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/><circle cx="5" cy="12" r="1"/></svg>"#;
pub const INLINE_CODE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m18 16 4-4-4-4"/><path d="m6 8-4 4 4 4"/><path d="m14.5 4-5 16"/></svg>"#;
pub const CODE_BLOCK: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="black" stroke="none"><path d="M20 3H4c-1.103 0-2 .897-2 2v14c0 1.103.897 2 2 2h16c1.103 0 2-.897 2-2V5c0-1.103-.897-2-2-2zM4 19V7h16l.002 12H4z"/><path d="M9.293 9.293L5.586 13l3.707 3.707l1.414-1.414L8.414 13l2.293-2.293zm5.414 0l-1.414 1.414L15.586 13l-2.293 2.293l1.414 1.414L18.414 13z"/></svg>"#;
pub const PILCROW: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M13 12h1"/><path d="M18 12h1"/><path d="M20 7H9a5 5 0 0 0 0 10h4"/><path d="M19 3v18"/><path d="M15 3v18"/></svg>"#;


pub const IMAGE_CROP: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="black" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M5 2v9h9M2 5h9v9"/></svg>"#;
pub const IMAGE_RESIZE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="black" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M9 2h5v5M7 14H2V9M14 2L9 7M2 14l5-5"/></svg>"#;
pub const IMAGE_ROUND: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="black" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M2 14V8a6 6 0 0 1 6-6h6"/></svg>"#;
pub const IMAGE_COMPRESS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="black" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M8 2v5M5.5 4.5L8 7l2.5-2.5M8 14V9M5.5 11.5L8 9l2.5 2.5M2 8h12"/></svg>"#;
pub const IMAGE_ANNOTATE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="black" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M3 13L13 3M7 3h6v6"/></svg>"#;
pub const IMAGE_CAPTION: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="black" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M2 11h12M2 14h8M5 3h6M8 3v5"/></svg>"#;
pub const IMAGE_DELETE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="black" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M3 4.5h10M6 4.5V3h4v1.5M4.5 4.5l.5 9h6l.5-9"/></svg>"#;

pub const ICON_IMAGE_CROP: &str = "icons/image_crop.svg";
pub const ICON_IMAGE_RESIZE: &str = "icons/image_resize.svg";
pub const ICON_IMAGE_ROUND: &str = "icons/image_round.svg";
pub const ICON_IMAGE_COMPRESS: &str = "icons/image_compress.svg";
pub const ICON_IMAGE_ANNOTATE: &str = "icons/image_annotate.svg";
pub const ICON_IMAGE_CAPTION: &str = "icons/image_caption.svg";
pub const ICON_IMAGE_DELETE: &str = "icons/image_delete.svg";
pub const ICON_ARROW_LEFT: &str = "icons/arrow_left.svg";
pub const ICON_ARROW_RIGHT: &str = "icons/arrow_right.svg";
pub const ICON_MORE_VERTICAL: &str = "icons/more_vertical.svg";
pub const ICON_TAB_ACTIVE_LEFT_FILL: &str = "icons/tab_active_left_fill.svg";
pub const ICON_TAB_ACTIVE_LEFT_STROKE: &str = "icons/tab_active_left_stroke.svg";
pub const ICON_TAB_ACTIVE_RIGHT_FILL: &str = "icons/tab_active_right_fill.svg";
pub const ICON_TAB_ACTIVE_RIGHT_STROKE: &str = "icons/tab_active_right_stroke.svg";
pub const ICON_COPY: &str = "icons/copy.svg";
pub const ICON_FOLDER_INPUT: &str = "icons/folder_input.svg";
pub const ICON_EXTERNAL_LINK: &str = "icons/external_link.svg";
pub const ICON_CANVAS: &str = "icons/canvas.svg";
pub const ICON_DRAWING: &str = "icons/drawing.svg";
pub const ICON_HASH: &str = "icons/hash.svg";
pub const ICON_DOWNLOAD: &str = "icons/download.svg";
pub const ICON_MOVE_ARROWS: &str = "icons/move_arrows.svg";
pub const ICON_SPLIT: &str = "icons/split.svg";
pub const ICON_APP_WINDOW: &str = "icons/app_window.svg";
pub const ICON_MERGE: &str = "icons/merge.svg";
pub const ICON_LINK: &str = "icons/link.svg";
pub const ICON_VAULT: &str = "icons/vault.svg";
pub const ICON_COMPUTER: &str = "icons/computer.svg";
pub const ICON_PIN: &str = "icons/pin.svg";
pub const ICON_ALIGN_LEFT: &str = "icons/align_left.svg";
pub const ICON_ALIGN_CENTER: &str = "icons/align_center.svg";
pub const ICON_ALIGN_RIGHT: &str = "icons/align_right.svg";
pub const ICON_TABLE_MERGE_CELLS: &str = "icons/table_merge_cells.svg";
pub const ICON_TABLE_SPLIT_CELLS: &str = "icons/table_split_cells.svg";
pub const ICON_TABLE_DELETE_ROW: &str = "icons/table_delete_row.svg";
pub const ICON_TABLE_DELETE_COL: &str = "icons/table_delete_col.svg";
pub const ICON_SPLIT_DOWN: &str = "icons/split_down.svg";
pub const ICON_LOCK: &str = "icons/lock.svg";
pub const ICON_CIRCLE_PLUS: &str = "icons/circle_plus.svg";
pub const ICON_SORT_ASC: &str = "icons/sort_asc.svg";
pub const ICON_SORT_DESC: &str = "icons/sort_desc.svg";
pub const ICON_UNDO: &str = "icons/undo.svg";
pub const ICON_REDO: &str = "icons/redo.svg";
pub const ICON_ERASER: &str = "icons/eraser.svg";
pub const ICON_BOLD: &str = "icons/bold.svg";
pub const ICON_ITALIC: &str = "icons/italic.svg";
pub const ICON_STRIKETHROUGH: &str = "icons/strikethrough.svg";
pub const ICON_UNDERLINE: &str = "icons/underline.svg";
pub const ICON_HIGHLIGHTER: &str = "icons/highlighter.svg";
pub const ICON_HEADING_1: &str = "icons/heading_1.svg";
pub const ICON_HEADING_2: &str = "icons/heading_2.svg";
pub const ICON_HEADING_3: &str = "icons/heading_3.svg";
pub const ICON_HEADING_4: &str = "icons/heading_4.svg";
pub const ICON_HEADING_5: &str = "icons/heading_5.svg";
pub const ICON_HEADING_6: &str = "icons/heading_6.svg";
pub const ICON_HEADING_N: &str = "icons/heading_n.svg";
pub const ICON_TABLE: &str = "icons/table.svg";
pub const ICON_LIST_BULLET: &str = "icons/list_bullet.svg";
pub const ICON_LIST_NUMBERED: &str = "icons/list_numbered.svg";
pub const ICON_LIST_TASK: &str = "icons/list_task.svg";
pub const ICON_INDENT: &str = "icons/indent.svg";
pub const ICON_OUTDENT: &str = "icons/outdent.svg";
pub const ICON_MESSAGE_SQUARE: &str = "icons/message_square.svg";
pub const ICON_FULLSCREEN: &str = "icons/fullscreen.svg";
pub const ICON_MORE_HORIZONTAL: &str = "icons/more_horizontal.svg";
pub const ICON_INLINE_CODE: &str = "icons/inline_code.svg";
pub const ICON_CODE_BLOCK: &str = "icons/code_block.svg";
pub const ICON_PILCROW: &str = "icons/pilcrow.svg";

pub const ICON_CHEVRON_RIGHT: &str = "icons/chevron_right.svg";
pub const ICON_CHEVRON_DOWN: &str = "icons/chevron_down.svg";
pub const ICON_CHEVRONS_UP_DOWN: &str = "icons/chevrons_up_down.svg";
pub const ICON_CHEVRONS_DOWN_UP: &str = "icons/chevrons_down_up.svg";
pub const ICON_FOLDER: &str = "icons/folder.svg";
pub const ICON_FOLDER_OPEN: &str = "icons/folder_open.svg";
pub const ICON_FOLDER_PLUS: &str = "icons/folder_plus.svg";
pub const ICON_FILE_TEXT: &str = "icons/file_text.svg";
pub const ICON_PLUS: &str = "icons/plus.svg";
pub const ICON_SAVE: &str = "icons/save.svg";
pub const ICON_EYE: &str = "icons/eye.svg";
pub const ICON_PENCIL: &str = "icons/pencil.svg";
pub const ICON_REFRESH: &str = "icons/refresh.svg";
pub const ICON_SEARCH: &str = "icons/search.svg";
pub const ICON_GLOBE: &str = "icons/globe.svg";
pub const ICON_BOLT: &str = "icons/bolt.svg";
pub const ICON_CODE: &str = "icons/code.svg";
pub const ICON_CLOSE: &str = "icons/close.svg";
pub const ICON_PANEL_LEFT: &str = "icons/panel_left.svg";
pub const ICON_PANEL_RIGHT: &str = "icons/panel_right.svg";
pub const ICON_LAYERS: &str = "icons/layers.svg";
pub const ICON_BOOKMARK: &str = "icons/bookmark.svg";
pub const ICON_BOOKMARK_FILLED: &str = "icons/bookmark_filled.svg";
pub const ICON_SETTINGS: &str = "icons/settings.svg";
pub const ICON_SLIDERS: &str = "icons/sliders.svg";
pub const ICON_SUN: &str = "icons/sun.svg";
pub const ICON_MOON: &str = "icons/moon.svg";
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
pub const ICON_PIPETTE: &str = "icons/pipette.svg";
pub const ICON_MINIMIZE: &str = "icons/minimize.svg";
pub const ICON_MAXIMIZE: &str = "icons/maximize.svg";
pub const ICON_RESTORE: &str = "icons/restore.svg";
pub const ICON_NOTE_CREATE: &str = "icons/note_create.svg";
pub const ICON_CALENDAR: &str = "icons/calendar.svg";
pub const ICON_INFO: &str = "icons/info.svg";
pub const ICON_TRIANGLE_ALERT: &str = "icons/triangle_alert.svg";
pub const ICON_CIRCLE_ALERT: &str = "icons/circle_alert.svg";
pub const ICON_QUOTE: &str = "icons/quote.svg";
pub const ICON_TERMINAL: &str = "icons/terminal.svg";
pub const ICON_WINDOW_CLOSE: &str = "icons/window_close.svg";

pub struct MemoryAssetSource;

impl gpui::AssetSource for MemoryAssetSource {
    /// Loads the in-memory SVG asset by virtual file path.
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let svg = match path {
            ICON_CHEVRON_RIGHT => Some(CHEVRON_RIGHT),
            ICON_CHEVRON_DOWN => Some(CHEVRON_DOWN),
            ICON_CHEVRONS_UP_DOWN => Some(CHEVRONS_UP_DOWN),
            ICON_CHEVRONS_DOWN_UP => Some(CHEVRONS_DOWN_UP),
            ICON_FOLDER => Some(FOLDER),
            ICON_FOLDER_OPEN => Some(FOLDER_OPEN),
            ICON_FOLDER_PLUS => Some(FOLDER_PLUS),
            ICON_FILE_TEXT => Some(FILE_TEXT),
            ICON_PLUS => Some(PLUS),
            ICON_SAVE => Some(SAVE),
            ICON_EYE => Some(EYE),
            ICON_PENCIL => Some(PENCIL),
            ICON_REFRESH => Some(REFRESH),
            ICON_SEARCH => Some(SEARCH),
            ICON_GLOBE => Some(GLOBE),
            ICON_BOLT => Some(BOLT),
            ICON_CODE => Some(CODE),
            ICON_CLOSE => Some(CLOSE),
            ICON_PANEL_LEFT => Some(PANEL_LEFT),
            ICON_PANEL_RIGHT => Some(PANEL_RIGHT),
            ICON_LAYERS => Some(LAYERS),
            ICON_BOOKMARK => Some(BOOKMARK),
            ICON_BOOKMARK_FILLED => Some(BOOKMARK_FILLED),
            ICON_SETTINGS => Some(SETTINGS),
            ICON_SLIDERS => Some(SLIDERS),
            ICON_SUN => Some(SUN),
            ICON_MOON => Some(MOON),
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
            ICON_PIPETTE => Some(PIPETTE),
            ICON_MINIMIZE => Some(MINIMIZE),
            ICON_MAXIMIZE => Some(MAXIMIZE),
            ICON_RESTORE => Some(RESTORE),
            ICON_NOTE_CREATE => Some(NOTE_CREATE),
            ICON_CALENDAR => Some(CALENDAR),
            ICON_INFO => Some(INFO),
            ICON_TRIANGLE_ALERT => Some(TRIANGLE_ALERT),
            ICON_CIRCLE_ALERT => Some(CIRCLE_ALERT),
            ICON_QUOTE => Some(QUOTE),
            ICON_TERMINAL => Some(TERMINAL),
            ICON_IMAGE_CROP => Some(IMAGE_CROP),
            ICON_IMAGE_RESIZE => Some(IMAGE_RESIZE),
            ICON_IMAGE_ROUND => Some(IMAGE_ROUND),
            ICON_IMAGE_COMPRESS => Some(IMAGE_COMPRESS),
            ICON_IMAGE_ANNOTATE => Some(IMAGE_ANNOTATE),
            ICON_IMAGE_CAPTION => Some(IMAGE_CAPTION),
            ICON_IMAGE_DELETE => Some(IMAGE_DELETE),
            ICON_ARROW_LEFT => Some(ARROW_LEFT),
            ICON_ARROW_RIGHT => Some(ARROW_RIGHT),
            ICON_MORE_VERTICAL => Some(MORE_VERTICAL),
            ICON_TAB_ACTIVE_LEFT_FILL => Some(TAB_ACTIVE_LEFT_FILL),
            ICON_TAB_ACTIVE_LEFT_STROKE => Some(TAB_ACTIVE_LEFT_STROKE),
            ICON_TAB_ACTIVE_RIGHT_FILL => Some(TAB_ACTIVE_RIGHT_FILL),
            ICON_TAB_ACTIVE_RIGHT_STROKE => Some(TAB_ACTIVE_RIGHT_STROKE),
            ICON_WINDOW_CLOSE => Some(WINDOW_CLOSE),
            ICON_COPY => Some(COPY),
            ICON_FOLDER_INPUT => Some(FOLDER_INPUT),
            ICON_EXTERNAL_LINK => Some(EXTERNAL_LINK),
            ICON_CANVAS => Some(CANVAS),
            ICON_DRAWING => Some(DRAWING),
            ICON_HASH => Some(HASH),
            ICON_DOWNLOAD => Some(DOWNLOAD),
            ICON_MOVE_ARROWS => Some(MOVE_ARROWS),
            ICON_SPLIT => Some(SPLIT),
            ICON_APP_WINDOW => Some(APP_WINDOW),
            ICON_MERGE => Some(MERGE),
            ICON_LINK => Some(LINK),
            ICON_VAULT => Some(VAULT),
            ICON_COMPUTER => Some(COMPUTER),
            ICON_PIN => Some(PIN),
            ICON_ALIGN_LEFT => Some(ALIGN_LEFT),
            ICON_ALIGN_CENTER => Some(ALIGN_CENTER),
            ICON_ALIGN_RIGHT => Some(ALIGN_RIGHT),
            ICON_TABLE_MERGE_CELLS => Some(TABLE_MERGE_CELLS),
            ICON_TABLE_SPLIT_CELLS => Some(TABLE_SPLIT_CELLS),
            ICON_TABLE_DELETE_ROW => Some(TABLE_DELETE_ROW),
            ICON_TABLE_DELETE_COL => Some(TABLE_DELETE_COL),
            ICON_SPLIT_DOWN => Some(SPLIT_DOWN),
            ICON_LOCK => Some(LOCK),
            ICON_CIRCLE_PLUS => Some(CIRCLE_PLUS),
            ICON_SORT_ASC => Some(SORT_ASC),
            ICON_SORT_DESC => Some(SORT_DESC),
            ICON_UNDO => Some(UNDO),
            ICON_REDO => Some(REDO),
            ICON_ERASER => Some(ERASER),
            ICON_BOLD => Some(BOLD),
            ICON_ITALIC => Some(ITALIC),
            ICON_STRIKETHROUGH => Some(STRIKETHROUGH),
            ICON_UNDERLINE => Some(UNDERLINE),
            ICON_HIGHLIGHTER => Some(HIGHLIGHTER),
            ICON_HEADING_1 => Some(HEADING_1),
            ICON_HEADING_2 => Some(HEADING_2),
            ICON_HEADING_3 => Some(HEADING_3),
            ICON_HEADING_4 => Some(HEADING_4),
            ICON_HEADING_5 => Some(HEADING_5),
            ICON_HEADING_6 => Some(HEADING_6),
            ICON_HEADING_N => Some(HEADING_N),
            ICON_TABLE => Some(TABLE),
            ICON_LIST_BULLET => Some(LIST_BULLET),
            ICON_LIST_NUMBERED => Some(LIST_NUMBERED),
            ICON_LIST_TASK => Some(LIST_TASK),
            ICON_INDENT => Some(INDENT),
            ICON_OUTDENT => Some(OUTDENT),
            ICON_MESSAGE_SQUARE => Some(MESSAGE_SQUARE),
            ICON_FULLSCREEN => Some(FULLSCREEN),
            ICON_MORE_HORIZONTAL => Some(MORE_HORIZONTAL),
            ICON_INLINE_CODE => Some(INLINE_CODE),
            ICON_CODE_BLOCK => Some(CODE_BLOCK),
            ICON_PILCROW => Some(PILCROW),
            _ => None,
        };
        if let Some(s) = svg {
            return Ok(Some(Cow::Borrowed(s.as_bytes())));
        }
        if let Some(custom) = crate::toolbar::get_custom_icon(path) {
            return Ok(Some(Cow::Owned(custom.into_bytes())));
        }
        Ok(None)
    }

    /// Lists all registered virtual SVG asset paths.
    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(vec![
            ICON_CHEVRON_RIGHT.into(),
            ICON_CHEVRON_DOWN.into(),
            ICON_CHEVRONS_UP_DOWN.into(),
            ICON_CHEVRONS_DOWN_UP.into(),
            ICON_FOLDER.into(),
            ICON_FOLDER_OPEN.into(),
            ICON_FOLDER_PLUS.into(),
            ICON_FILE_TEXT.into(),
            ICON_PLUS.into(),
            ICON_SAVE.into(),
            ICON_EYE.into(),
            ICON_PENCIL.into(),
            ICON_REFRESH.into(),
            ICON_SEARCH.into(),
            ICON_GLOBE.into(),
            ICON_BOLT.into(),
            ICON_CODE.into(),
            ICON_CLOSE.into(),
            ICON_PANEL_LEFT.into(),
            ICON_PANEL_RIGHT.into(),
            ICON_LAYERS.into(),
            ICON_BOOKMARK.into(),
            ICON_BOOKMARK_FILLED.into(),
            ICON_SETTINGS.into(),
            ICON_SLIDERS.into(),
            ICON_SUN.into(),
            ICON_MOON.into(),
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
            ICON_INFO.into(),
            ICON_TRIANGLE_ALERT.into(),
            ICON_CIRCLE_ALERT.into(),
            ICON_QUOTE.into(),
            ICON_TERMINAL.into(),
            ICON_IMAGE_CROP.into(),
            ICON_IMAGE_RESIZE.into(),
            ICON_IMAGE_ROUND.into(),
            ICON_IMAGE_COMPRESS.into(),
            ICON_IMAGE_ANNOTATE.into(),
            ICON_IMAGE_CAPTION.into(),
            ICON_IMAGE_DELETE.into(),
            ICON_ARROW_LEFT.into(),
            ICON_ARROW_RIGHT.into(),
            ICON_MORE_VERTICAL.into(),
            ICON_TAB_ACTIVE_LEFT_FILL.into(),
            ICON_TAB_ACTIVE_LEFT_STROKE.into(),
            ICON_TAB_ACTIVE_RIGHT_FILL.into(),
            ICON_TAB_ACTIVE_RIGHT_STROKE.into(),
            ICON_WINDOW_CLOSE.into(),
            ICON_COPY.into(),
            ICON_FOLDER_INPUT.into(),
            ICON_EXTERNAL_LINK.into(),
            ICON_CANVAS.into(),
            ICON_DRAWING.into(),
            ICON_HASH.into(),
            ICON_DOWNLOAD.into(),
            ICON_MOVE_ARROWS.into(),
            ICON_SPLIT.into(),
            ICON_APP_WINDOW.into(),
            ICON_MERGE.into(),
            ICON_LINK.into(),
            ICON_VAULT.into(),
            ICON_COMPUTER.into(),
            ICON_PIN.into(),
            ICON_ALIGN_LEFT.into(),
            ICON_ALIGN_CENTER.into(),
            ICON_ALIGN_RIGHT.into(),
            ICON_TABLE_MERGE_CELLS.into(),
            ICON_TABLE_SPLIT_CELLS.into(),
            ICON_TABLE_DELETE_ROW.into(),
            ICON_TABLE_DELETE_COL.into(),
            ICON_SPLIT_DOWN.into(),
            ICON_LOCK.into(),
            ICON_CIRCLE_PLUS.into(),
            ICON_SORT_ASC.into(),
            ICON_SORT_DESC.into(),
            ICON_INLINE_CODE.into(),
            ICON_CODE_BLOCK.into(),
            ICON_PILCROW.into(),
        ])
    }
}

/// Renders an SVG icon with given dimensions (square) and color.
pub fn icon(path: &'static str, size: f32, color: impl Into<Hsla>) -> Svg {
    gpui::svg()
        .path(path)
        .w(px(size))
        .h(px(size))
        .flex_shrink_0()
        .text_color(color.into())
}

/// Renders an SVG icon with explicit width, height and color.
pub fn svg_sized(path: &'static str, width: f32, height: f32, color: impl Into<Hsla>) -> Svg {
    gpui::svg()
        .path(path)
        .w(px(width))
        .h(px(height))
        .flex_shrink_0()
        .text_color(color.into())
}

/// Maps Obsidian / Lucide icon names (e.g. "square-pen", "document", "pencil") to registered SVG icon assets.
pub fn map_obsidian_icon(name: &str) -> &'static str {
    match name.to_lowercase().as_str() {
        "square-pen" | "create-new" | "note" | "pencil" | "edit" | "pen" => ICON_NOTE_CREATE,
        "plus" | "plus-with-circle" | "add" => ICON_PLUS,
        "search" | "magnifying-glass" => ICON_SEARCH,
        "document" | "file" | "file-text" => ICON_FILE_TEXT,
        "folder" | "folder-open" => ICON_FOLDER,
        "folder-plus" => ICON_FOLDER_PLUS,
        "chevrons-up-down" | "unfold" => ICON_CHEVRONS_UP_DOWN,
        "chevrons-down-up" | "fold" => ICON_CHEVRONS_DOWN_UP,
        "calendar" => ICON_CALENDAR,
        "code-block" | "codeblock-glyph" | "code-square" | "window-code" => ICON_CODE_BLOCK,
        "inline-code" | "code-glyph" | "code-xml" | "code-slash" => ICON_INLINE_CODE,
        "code" => ICON_CODE_BLOCK,
        "terminal" => ICON_INLINE_CODE,
        "console" | "cli" | "prompt" | "terminal-square" => ICON_TERMINAL,
        "history" | "clock" => ICON_HISTORY,
        "trash" | "trash-2" => ICON_TRASH_2,
        "settings" | "gear" => ICON_SETTINGS,
        "sliders" | "sliders-horizontal" | "equalizer" | "settings-2" => ICON_SLIDERS,
        "sun" | "light-mode" | "sun-medium" => ICON_SUN,
        "moon" | "dark-mode" | "moon-star" => ICON_MOON,
        "bookmark" => ICON_BOOKMARK,
        "bolt" | "zap" | "sparkles" => ICON_BOLT,
        "refresh" | "reload" => ICON_REFRESH,
        "eye" | "preview" => ICON_EYE,
        "paperclip" | "attachment" => ICON_PAPERCLIP,
        "image" | "photo" | "picture" | "gallery" => ICON_CANVAS,
        "link" => ICON_LINK,
        "external-link" => ICON_EXTERNAL_LINK,
        "copy" | "clipboard" | "clipboard-list" => ICON_COPY,
        "scissors" | "cut" => ICON_PENCIL,
        "check" | "check-square" => ICON_CHECK,
        "undo" => ICON_UNDO,
        "redo" => ICON_REDO,
        "eraser" | "clear-formatting" => ICON_ERASER,
        "bold" => ICON_BOLD,
        "italic" => ICON_ITALIC,
        "strikethrough" => ICON_STRIKETHROUGH,
        "underline" => ICON_UNDERLINE,
        "highlight" | "highlighter" => ICON_HIGHLIGHTER,
        "h1" | "heading-1" => ICON_HEADING_1,
        "h2" | "heading-2" => ICON_HEADING_2,
        "h3" | "heading-3" => ICON_HEADING_3,
        "h4" | "heading-4" => ICON_HEADING_4,
        "h5" | "heading-5" => ICON_HEADING_5,
        "h6" | "heading-6" => ICON_HEADING_6,
        "hn" | "heading-n" | "headings" => ICON_HEADING_N,
        "table" => ICON_TABLE,
        "task" | "toggle-task" | "checkbox" => ICON_CHECK,
        "callout" | "comment" | "message-square" => ICON_MESSAGE_SQUARE,
        "list-bullet" | "bullet-list" => ICON_LIST_BULLET,
        "list-numbered" | "numbered-list" => ICON_LIST_NUMBERED,
        "list-task" | "task-list" => ICON_LIST_TASK,
        "indent" => ICON_INDENT,
        "outdent" => ICON_OUTDENT,
        "fullscreen" | "toggle-fullscreen" => ICON_FULLSCREEN,
        "more-horizontal" | "overflow" => ICON_MORE_HORIZONTAL,
        "pilcrow" | "invisibles" | "whitespace" => ICON_PILCROW,
        "type" | "text" | "quote" | "list" | "list-ordered" => ICON_TYPE,
        "hash" | "heading" => ICON_HASH,
        "split" => ICON_SPLIT,
        "split-down" => ICON_SPLIT_DOWN,
        "lock" | "encrypted" => ICON_LOCK,
        "plus-circle" | "circle-plus" => ICON_CIRCLE_PLUS,
        "app-window" => ICON_APP_WINDOW,
        "vault" => ICON_VAULT,
        "pin" => ICON_PIN,
        "canvas" => ICON_CANVAS,
        "align-left" => ICON_ALIGN_LEFT,
        "align-center" => ICON_ALIGN_CENTER,
        "align-right" => ICON_ALIGN_RIGHT,
        "table-merge-cells" | "merge-cells" => ICON_TABLE_MERGE_CELLS,
        "table-split-cells" | "split-cells" => ICON_TABLE_SPLIT_CELLS,
        "table-delete-row" | "delete-row" | "rows" => ICON_TABLE_DELETE_ROW,
        "table-delete-col" | "delete-col" | "columns" => ICON_TABLE_DELETE_COL,
        _ => ICON_NOTE_CREATE,
    }
}

/// Resolves an icon name or path to a valid virtual asset path or custom icon name.
pub fn resolve_toolbar_icon(name: &str) -> SharedString {
    if crate::toolbar::has_custom_icon(name) {
        return name.to_string().into();
    }
    if name.starts_with("icons/") {
        return name.to_string().into();
    }
    map_obsidian_icon(name).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::AssetSource;

    #[test]
    fn test_inline_code_and_code_block_icons() {
        let source = MemoryAssetSource;

        // Check virtual asset loading
        let inline_bytes = source.load(ICON_INLINE_CODE).expect("load inline-code").expect("bytes");
        assert_eq!(inline_bytes.as_ref(), INLINE_CODE.as_bytes());

        let block_bytes = source.load(ICON_CODE_BLOCK).expect("load code-block").expect("bytes");
        assert_eq!(block_bytes.as_ref(), CODE_BLOCK.as_bytes());

        // Check list contains both
        let list = source.list("").expect("list");
        assert!(list.iter().any(|p| p.as_ref() == ICON_INLINE_CODE));
        assert!(list.iter().any(|p| p.as_ref() == ICON_CODE_BLOCK));

        // Check icon name mappings
        assert_eq!(map_obsidian_icon("inline-code"), ICON_INLINE_CODE);
        assert_eq!(map_obsidian_icon("code-glyph"), ICON_INLINE_CODE);
        assert_eq!(map_obsidian_icon("code-xml"), ICON_INLINE_CODE);
        assert_eq!(map_obsidian_icon("code-slash"), ICON_INLINE_CODE);
        assert_eq!(map_obsidian_icon("terminal"), ICON_INLINE_CODE);

        assert_eq!(map_obsidian_icon("code-block"), ICON_CODE_BLOCK);
        assert_eq!(map_obsidian_icon("codeblock-glyph"), ICON_CODE_BLOCK);
        assert_eq!(map_obsidian_icon("code-square"), ICON_CODE_BLOCK);
        assert_eq!(map_obsidian_icon("window-code"), ICON_CODE_BLOCK);
        assert_eq!(map_obsidian_icon("code"), ICON_CODE_BLOCK);

        // Check resolve_toolbar_icon
        assert_eq!(resolve_toolbar_icon("inline-code").as_ref(), ICON_INLINE_CODE);
        assert_eq!(resolve_toolbar_icon("code-block").as_ref(), ICON_CODE_BLOCK);
    }
}

