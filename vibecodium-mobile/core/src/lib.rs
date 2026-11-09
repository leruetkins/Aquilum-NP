//! Platform-independent core for the vibecodium Android front end.
//!
//! `vault` and `line_index` are the very same source files the GPUI desktop
//! application builds (included by path, not copied), so security rules and
//! line addressing cannot drift between the two front ends. Everything above
//! them is a small JSON API plus a JNI bridge.

#[path = "../../../aquilum-gpui/src/vault.rs"]
pub mod vault;

#[path = "../../../aquilum-gpui/src/trash.rs"]
pub mod trash;

/// The desktop vault delegates attachment bookkeeping to `crate::images`.
/// The mobile core has no attachment UI yet, so it provides the same function
/// signatures with no-ops; this keeps `vault.rs` and `trash.rs` reused verbatim
/// instead of forked.
mod images {
    use std::path::Path;

    pub fn folder() -> String {
        "attachments".to_string()
    }

    pub fn invalidate_ref_index() {}

    pub fn trash_note_attachments(
        _vault_root: &Path,
        _note_path: &Path,
        _content: &str,
        _folder: &str,
        _exclude_notes: &[&Path],
    ) {
    }

    pub fn move_note_attachments(
        _vault_root: &Path,
        _old_note: &Path,
        _new_note: &Path,
        _folder: &str,
    ) -> usize {
        0
    }
}

mod api;
mod jni_bridge;
mod markdown;

#[cfg(target_os = "android")]
pub mod surface;

/// Minimal color surface used by the desktop markdown modules that were
/// written against GPUI. The mobile core never paints; it only needs the type
/// to compile the shared parser/grammar sources.
pub mod gpui {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Rgba {
        pub r: f32,
        pub g: f32,
        pub b: f32,
        pub a: f32,
    }

    pub fn rgb(hex: u32) -> Rgba {
        Rgba {
            r: ((hex >> 16) & 0xff) as f32 / 255.0,
            g: ((hex >> 8) & 0xff) as f32 / 255.0,
            b: (hex & 0xff) as f32 / 255.0,
            a: 1.0,
        }
    }

    pub fn rgba(hex: u32) -> Rgba {
        Rgba {
            r: ((hex >> 24) & 0xff) as f32 / 255.0,
            g: ((hex >> 16) & 0xff) as f32 / 255.0,
            b: ((hex >> 8) & 0xff) as f32 / 255.0,
            a: (hex & 0xff) as f32 / 255.0,
        }
    }
}

/// The desktop callout labels are localized through `crate::i18n::t`. The
/// mobile model only needs a stable display label, so the last key segment is
/// title-cased and the Android side localizes its own UI strings.
pub mod i18n {
    pub fn t(key: &str, _lang: &str) -> String {
        let name = key.rsplit('.').next().unwrap_or(key);
        let mut chars = name.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().chain(chars).collect(),
            None => String::new(),
        }
    }
}

pub use api::*;
