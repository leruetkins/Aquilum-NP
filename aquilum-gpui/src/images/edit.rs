use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use serde::{Deserialize, Serialize};
use super::annotate::Annotation;
use super::invalidate_resolved_images;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CropRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl CropRect {
    pub fn full(natural: (u32, u32)) -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: natural.0.max(1) as f32,
            h: natural.1.max(1) as f32,
        }
    }

    /// Keeps the frame inside the picture and enforces the minimum size.
    pub fn clamp_to(&self, natural: (u32, u32)) -> Self {
        let nw = natural.0.max(1) as f32;
        let nh = natural.1.max(1) as f32;
        let x = self.x.clamp(0.0, (nw - 1.0).max(0.0));
        let y = self.y.clamp(0.0, (nh - 1.0).max(0.0));
        let w = self.w.max(1.0).min((nw - x).max(1.0));
        let h = self.h.max(1.0).min((nh - y).max(1.0));
        Self { x, y, w, h }
    }

    pub fn visible_size(&self) -> (u32, u32) {
        (
            self.w.round().max(1.0) as u32,
            self.h.round().max(1.0) as u32,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Caption {
    pub text: String,
    #[serde(default = "default_caption_position")]
    pub position: String,
    #[serde(default)]
    pub numbered: bool,
}

impl Default for Caption {
    fn default() -> Self {
        Self {
            text: String::new(),
            position: default_caption_position(),
            numbered: false,
        }
    }
}

fn default_caption_position() -> String {
    "below".to_string()
}

/// Contents of `name.png.edit.json`. Unknown fields are preserved on save.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ImageEdit {
    pub version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crop: Option<CropRect>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<Annotation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corner_radius: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<Caption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_name: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for ImageEdit {
    fn default() -> Self {
        Self {
            version: 1,
            crop: None,
            annotations: Vec::new(),
            corner_radius: None,
            caption: None,
            source_name: None,
            extra: serde_json::Map::new(),
        }
    }
}

impl ImageEdit {
    fn is_empty(&self) -> bool {
        self.crop.is_none()
            && self.annotations.is_empty()
            && self.corner_radius.is_none()
            && self.caption.is_none()
            && self.source_name.is_none()
            && self.extra.is_empty()
    }
}

pub fn edit_path(image: &Path) -> PathBuf {
    let mut name = image
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.push_str(".edit.json");
    image.with_file_name(name)
}

pub(crate) fn edit_cache() -> &'static Mutex<HashMap<PathBuf, ImageEdit>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, ImageEdit>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn load_edit(image: &Path) -> ImageEdit {
    let path = edit_path(image);
    if let Ok(cache) = edit_cache().lock() {
        if let Some(edit) = cache.get(&path) {
            return edit.clone();
        }
    }
    let edit = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<ImageEdit>(&text).ok())
        .unwrap_or_default();
    if let Ok(mut cache) = edit_cache().lock() {
        cache.insert(path, edit.clone());
    }
    edit
}

pub fn save_edit(image: &Path, edit: &ImageEdit) -> std::io::Result<()> {
    let path = edit_path(image);
    if edit.is_empty() {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
    } else {
        let text = serde_json::to_string_pretty(edit)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
        std::fs::write(&path, text)?;
    }
    if let Ok(mut cache) = edit_cache().lock() {
        cache.remove(&path);
    }
    invalidate_resolved_images();
    EDIT_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

static DEFAULT_RADIUS_BITS: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0.0f32.to_bits());
static EDIT_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Global corner radius default, published by the view from settings.
pub fn set_default_corner_radius(percent: f32) {
    let percent = if percent.is_finite() {
        percent.clamp(0.0, 50.0)
    } else {
        0.0
    };
    DEFAULT_RADIUS_BITS.store(percent.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

pub fn default_corner_radius() -> f32 {
    f32::from_bits(DEFAULT_RADIUS_BITS.load(std::sync::atomic::Ordering::Relaxed))
}

pub fn current_corner_radius(edit: &ImageEdit) -> f32 {
    edit.corner_radius.unwrap_or_else(default_corner_radius).clamp(0.0, 50.0)
}

/// Bumped whenever a sidecar changes, so caption numbering and painted
/// geometry are recomputed without watching every file.
pub fn edit_generation() -> u64 {
    EDIT_GENERATION.load(std::sync::atomic::Ordering::Relaxed)
}

/// Radius in px for a visible area of `width`x`height`; the stored value is a
/// percentage of the shorter side (0–50).
pub fn radius_px(edit: &ImageEdit, width: f32, height: f32) -> f32 {
    current_corner_radius(edit) / 100.0 * width.min(height)
}

pub fn format_caption(template: &str, number: Option<u32>, text: &str, numbered: bool) -> String {
    let text = text.trim();
    if text.is_empty() {
        return String::new();
    }
    let Some(number) = number.filter(|_| numbered) else {
        return text.to_string();
    };
    template
        .replace("{n}", &number.to_string())
        .replace("{text}", text)
}

pub fn set_corner_radius(image: &Path, percent: Option<f32>) -> std::io::Result<()> {
    let mut edit = load_edit(image);
    let default = default_corner_radius();
    edit.corner_radius = percent
        .map(|p| p.clamp(0.0, 50.0))
        .filter(|p| (*p - default).abs() > 0.01);
    save_edit(image, &edit)
}
