//! Minimal settings surface for the search engine included by path.
//!
//! `search/bm25.rs` is shared with the desktop build and reads its parameters
//! from `crate::settings::Bm25fParams`. The mobile app has no settings UI yet,
//! so this is the same struct with the desktop defaults; keep the values in
//! sync with `aquilum-gpui/src/settings.rs`.

#[derive(Debug, Clone, Copy)]
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
