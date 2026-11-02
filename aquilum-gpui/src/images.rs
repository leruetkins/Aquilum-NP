//! Image attachments for notes.
//!
//! A note keeps only `![[file-name.png]]`; the bytes live in an `attachments`
//! folder next to the note. The folder name is configurable (`files.folder`).
//! Resolution order when a reference is opened, copied or exported is the one
//! from `TODO/images/images.md`.

use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

pub const DEFAULT_FOLDER: &str = "attachments";

/// Extensions a file may claim before it is validated. The extension is never
/// trusted: the bytes are decoded (header only) before the file is imported.
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "ico", "avif",
];

// ───────────────────────────── Import ─────────────────────────────

/// Where the bytes of one paste/drop come from.
#[derive(Debug, Clone)]
pub enum ImageImportSource {
    /// Raster data from the clipboard, in the format the platform reported.
    Clipboard { bytes: Vec<u8>, format: gpui::ImageFormat },
    /// Files from Explorer, the clipboard file list, or a file picker.
    Files(Vec<PathBuf>),
}

#[derive(Debug, Clone)]
pub struct ImportedImage {
    pub name: String,
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone)]
pub enum ImportError {
    Unsupported(String),
    Damaged(String),
    Io(String),
}

impl ImportError {
    pub fn message(&self, language: &str) -> String {
        let prefix = match self {
            Self::Unsupported(_) => crate::i18n::t("editor.image.error_unsupported", language),
            Self::Damaged(_) => crate::i18n::t("editor.image.error_damaged", language),
            Self::Io(_) => crate::i18n::t("editor.image.error_io", language),
        };
        match self {
            Self::Unsupported(name) | Self::Damaged(name) => format!("{prefix}: {name}"),
            Self::Io(err) => format!("{prefix}: {err}"),
        }
    }
}

pub fn attachments_dir(note_path: &Path, folder: &str) -> PathBuf {
    let base = note_path.parent().unwrap_or_else(|| Path::new("."));
    base.join(folder_name(folder))
}

fn folder_name(folder: &str) -> String {
    let trimmed = folder.trim().trim_matches(['/', '\\']);
    if trimmed.is_empty() {
        DEFAULT_FOLDER.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Imports every source into `<note dir>/<folder>/`. Files are copied byte for
/// byte; clipboard rasters are encoded as PNG. Each source is reported
/// separately so one damaged file does not cancel the rest.
pub fn import_images(
    note_path: &Path,
    folder: &str,
    source: ImageImportSource,
) -> Vec<Result<ImportedImage, ImportError>> {
    let dir = attachments_dir(note_path, folder);
    match source {
        ImageImportSource::Clipboard { bytes, format } => {
            vec![import_clipboard(&dir, &bytes, format)]
        }
        ImageImportSource::Files(paths) => paths
            .into_iter()
            .map(|path| import_file(&dir, &path))
            .collect(),
    }
}

fn import_clipboard(
    dir: &Path,
    bytes: &[u8],
    format: gpui::ImageFormat,
) -> Result<ImportedImage, ImportError> {
    let format = match format {
        gpui::ImageFormat::Png => image::ImageFormat::Png,
        gpui::ImageFormat::Jpeg => image::ImageFormat::Jpeg,
        gpui::ImageFormat::Webp => image::ImageFormat::WebP,
        gpui::ImageFormat::Gif => image::ImageFormat::Gif,
        gpui::ImageFormat::Bmp => image::ImageFormat::Bmp,
        gpui::ImageFormat::Tiff => image::ImageFormat::Tiff,
        gpui::ImageFormat::Svg => {
            return Err(ImportError::Unsupported("svg".to_string()));
        }
    };
    let Some(raster) = image::load_from_memory_with_format(bytes, format).ok() else {
        return Err(ImportError::Damaged("clipboard".to_string()));
    };
    let (width, height) = (raster.width(), raster.height());
    let mut png = Vec::new();
    if raster
        .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .is_err()
    {
        return Err(ImportError::Damaged("clipboard".to_string()));
    }
    let (name, path) = write_new(dir, "png", &png)?;
    Ok(ImportedImage { name, path, width, height })
}

fn import_file(dir: &Path, source: &Path) -> Result<ImportedImage, ImportError> {
    let label = source
        .file_name()
        .map_or_else(|| source.display().to_string(), |n| n.to_string_lossy().into_owned());
    let reader = image::ImageReader::open(source)
        .map_err(|_| ImportError::Damaged(label.clone()))?;
    let reader = reader
        .with_guessed_format()
        .map_err(|_| ImportError::Damaged(label.clone()))?;
    let Some(format) = reader.format() else {
        return Err(ImportError::Unsupported(label));
    };
    let (width, height) = reader
        .into_dimensions()
        .map_err(|_| ImportError::Damaged(label.clone()))?;
    let ext = format
        .extensions_str()
        .first()
        .copied()
        .unwrap_or("png");
    let bytes = std::fs::read(source).map_err(|err| ImportError::Io(err.to_string()))?;
    let (name, path) = write_new(dir, ext, &bytes)?;
    Ok(ImportedImage { name, path, width, height })
}

/// Writes `bytes` under a fresh `date-time-suffix.ext` name. `create_new`
/// makes the suffix collision-proof instead of relying on a pre-check.
fn write_new(dir: &Path, ext: &str, bytes: &[u8]) -> Result<(String, PathBuf), ImportError> {
    std::fs::create_dir_all(dir).map_err(|err| ImportError::Io(err.to_string()))?;
    for _ in 0..16 {
        let name = new_name(ext);
        let path = dir.join(&name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                use std::io::Write;
                file.write_all(bytes)
                    .map_err(|err| ImportError::Io(err.to_string()))?;
                invalidate_index();
                remember_dimensions(&path);
                return Ok((name, path));
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(ImportError::Io(err.to_string())),
        }
    }
    Err(ImportError::Io("no unique attachment name".to_string()))
}

fn new_name(ext: &str) -> String {
    let now = time::OffsetDateTime::now_local()
        .unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    let suffix = &uuid::Uuid::new_v4().simple().to_string()[..4];
    format!(
        "{:04}-{:02}-{:02}-{:02}{:02}-{}.{}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        suffix,
        ext
    )
}

// ───────────────────────────── Resolution ─────────────────────────────

/// Finds the bytes of `name` for the note at `note_path`.
///
/// 1. `<note dir>/<folder>/name`
/// 2. `<vault>/<folder>/.trash/name`
/// 3. `<vault>/<folder>/name`
/// 4. any file with this name anywhere in the vault (cached scan)
pub fn resolve_attachment(
    vault_root: Option<&Path>,
    note_path: &Path,
    folder: &str,
    name: &str,
) -> Option<PathBuf> {
    let name = name.trim().trim_matches('"');
    if name.is_empty() {
        return None;
    }
    let rel = Path::new(name);
    if rel.is_absolute() {
        return rel.is_file().then(|| rel.to_path_buf());
    }
    let folder = folder_name(folder);
    let key = (note_path.to_path_buf(), folder.clone(), name.to_string());
    if let Ok(cache) = resolved().lock() {
        if let Some(hit) = cache.get(&key) {
            return hit.clone();
        }
    }
    let found = resolve_uncached(vault_root, note_path, &folder, name, rel);
    if let Ok(mut cache) = resolved().lock() {
        cache.insert(key, found.clone());
    }
    found
}

fn resolve_uncached(
    vault_root: Option<&Path>,
    note_path: &Path,
    folder: &str,
    name: &str,
    rel: &Path,
) -> Option<PathBuf> {
    let beside = note_path.parent().unwrap_or_else(|| Path::new(".")).join(folder).join(rel);
    if beside.is_file() {
        return Some(beside);
    }
    let Some(root) = vault_root else {
        return None;
    };
    let trash = root.join(folder).join(".trash").join(rel);
    if trash.is_file() {
        return Some(trash);
    }
    let shared = root.join(folder).join(rel);
    if shared.is_file() {
        return Some(shared);
    }
    if rel.components().count() == 1 {
        let key = name.to_lowercase();
        if let Some((_, found)) = vault_index(root).into_iter().find(|(k, _)| *k == key) {
            return Some(found);
        }
    }
    None
}

type ResolveKey = (PathBuf, String, String);

fn resolved() -> &'static Mutex<HashMap<ResolveKey, Option<PathBuf>>> {
    static RESOLVED: OnceLock<Mutex<HashMap<ResolveKey, Option<PathBuf>>>> = OnceLock::new();
    RESOLVED.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Drops remembered resolutions. Called after imports and after the file
/// explorer rescans, so a deleted attachment is not shown as present.
pub fn invalidate_resolution() {
    if let Ok(mut cache) = resolved().lock() {
        cache.clear();
    }
    if let Ok(mut cache) = dimensions().lock() {
        cache.clear();
    }
    if let Ok(mut cache) = edit_cache().lock() {
        cache.clear();
    }
    invalidate_resolved_images();
    crate::image_render::invalidate_caches();
}

fn vault_index(root: &Path) -> Vec<(String, PathBuf)> {
    let mut guard = index()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((cached, entries)) = guard.as_ref() {
        if cached == root {
            return entries.clone();
        }
    }
    let mut entries = Vec::new();
    scan_dir(root, root, 0, &mut entries);
    *guard = Some((root.to_path_buf(), entries.clone()));
    entries
}

fn scan_dir(root: &Path, dir: &Path, depth: usize, out: &mut Vec<(String, PathBuf)>) {
    if depth > 12 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "target" || name == "node_modules" || name == "vendor" {
            continue;
        }
        if path.is_dir() {
            scan_dir(root, &path, depth + 1, out);
        } else if is_image_path(&path) && path.starts_with(root) {
            out.push((name.to_lowercase(), path));
        }
    }
}

fn index() -> &'static Mutex<Option<(PathBuf, Vec<(String, PathBuf)>)>> {
    static INDEX: OnceLock<Mutex<Option<(PathBuf, Vec<(String, PathBuf)>)>>> = OnceLock::new();
    INDEX.get_or_init(|| Mutex::new(None))
}

pub fn invalidate_index() {
    *index()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    invalidate_resolution();
    invalidate_resolved_images();
    invalidate_dimensions();
}

/// Pixel dimensions change when a file is cropped, compressed or replaced on
/// disk; the size cache must not outlive the file.
pub fn invalidate_dimensions() {
    if let Ok(mut cache) = dimensions().lock() {
        cache.clear();
    }
}

pub fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| IMAGE_EXTENSIONS.iter().any(|known| known.eq_ignore_ascii_case(ext)))
}

// ───────────────────────────── Dimensions ─────────────────────────────

fn dimensions() -> &'static Mutex<HashMap<PathBuf, (u32, u32)>> {
    static DIMENSIONS: OnceLock<Mutex<HashMap<PathBuf, (u32, u32)>>> = OnceLock::new();
    DIMENSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Natural size of the file, cached until its length or mtime changes.
pub fn image_size(path: &Path) -> Option<(u32, u32)> {
    if let Ok(cache) = dimensions().lock() {
        if let Some(size) = cache.get(path) {
            return Some(*size);
        }
    }
    let size = image::image_dimensions(path).ok()?;
    if let Ok(mut cache) = dimensions().lock() {
        cache.insert(path.to_path_buf(), size);
    }
    Some(size)
}

fn remember_dimensions(path: &Path) {
    let _ = image_size(path);
}

/// The editor's monospace advance, published once per frame by `AppView`.
/// Image height is computed while the visual window is built, where the text
/// system is not available; both the height index and the paint call this, so
/// they always agree.
static CHAR_WIDTH_BITS: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(8.0f32.to_bits());

pub fn set_char_width(width: f32) {
    if width.is_finite() && width > 0.0 {
        CHAR_WIDTH_BITS.store(width.to_bits(), std::sync::atomic::Ordering::Relaxed);
    }
}

fn char_width() -> f32 {
    f32::from_bits(CHAR_WIDTH_BITS.load(std::sync::atomic::Ordering::Relaxed))
}

/// Width available to an image block for `wrap_cols` characters.
pub fn text_width(wrap_cols: usize) -> f32 {
    (wrap_cols as f32 * char_width()).max(160.0)
}

// ───────────────────────────── References ─────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageReference {
    pub target: String,
    /// `![[name]]` — a bare name resolved through `resolve_attachment`.
    pub wiki: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

impl ImageReference {
    /// Markup to write back into the note.
    pub fn markup(&self) -> String {
        match (self.width, self.height) {
            (Some(w), Some(h)) => format!("![[{}|{}x{}]]", self.target, w, h),
            (Some(w), None) => format!("![[{}|{}]]", self.target, w),
            _ if self.wiki => format!("![[{}]]", self.target),
            _ => format!("![]({})", self.target),
        }
    }

    pub fn with_size(&self, width: Option<u32>, height: Option<u32>) -> Self {
        Self {
            width,
            height,
            ..self.clone()
        }
    }

    pub fn with_target(&self, target: String) -> Self {
        Self {
            target,
            ..self.clone()
        }
    }

    /// Displayed size in px inside a text column of `max_width`.
    pub fn display_size(&self, natural: (u32, u32), max_width: f32) -> (f32, f32) {
        let (nw, nh) = (natural.0.max(1) as f32, natural.1.max(1) as f32);
        let explicit_w = self.width.filter(|w| *w > 0).map(|w| w as f32);
        let explicit_h = self.height.filter(|h| *h > 0).map(|h| h as f32);
        let (mut w, mut h) = match (explicit_w, explicit_h) {
            (Some(w), Some(h)) => (w, h),
            (Some(w), None) => (w, w * nh / nw),
            (None, Some(h)) => (h * nw / nh, h),
            (None, None) => (nw, nh),
        };
        let max_width = max_width.max(1.0);
        if w > max_width {
            h *= max_width / w;
            w = max_width;
        }
        (w.max(1.0), h.max(1.0))
    }
}

/// Parses a line that consists only of one image reference.
///
/// Returns `None` for ordinary text, for markup inside a longer line, and for
/// remote URLs (those are not attachments).
pub fn parse_image_reference(line: &str) -> Option<ImageReference> {
    let trimmed = line.trim();
    if !trimmed.starts_with('!') {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix("![[") {
        let inner = rest.strip_suffix("]]")?;
        if inner.contains('[') || inner.contains(']') || inner.contains('\n') {
            return None;
        }
        let mut parts = inner.split('|');
        let target = parts.next()?.trim();
        if target.is_empty() {
            return None;
        }
        let (mut width, mut height) = (None, None);
        if let Some(spec) = parts.next() {
            let spec = spec.trim();
            if let Some((w, h)) = spec.split_once('x').or_else(|| spec.split_once('X')) {
                width = w.trim().parse::<u32>().ok().filter(|v| *v > 0);
                height = h.trim().parse::<u32>().ok().filter(|v| *v > 0);
            } else if let Ok(w) = spec.parse::<u32>() {
                if w > 0 {
                    width = Some(w);
                }
            }
            // A non-numeric part is an Obsidian-style caption; captions are
            // stored in `.edit.json`, so it is only a hint here and ignored.
        }
        return Some(ImageReference {
            target: target.to_string(),
            wiki: true,
            width,
            height,
        });
    }
    if let Some(rest) = trimmed.strip_prefix("![") {
        let (alt, url) = rest.split_once("](")?;
        let url = url.strip_suffix(')')?;
        let _ = alt;
        if url.starts_with("http://") || url.starts_with("https://") || url.is_empty() {
            return None;
        }
        return Some(ImageReference {
            target: url.trim().to_string(),
            wiki: false,
            width: None,
            height: None,
        });
    }
    None
}

/// `![[name.png]]` written at the caret, one per line.
pub fn insert_text_for(images: &[ImportedImage]) -> String {
    images
        .iter()
        .map(|image| format!("![[{}]]", image.name))
        .collect::<Vec<_>>()
        .join("\n")
}

// ───────────────────────────── Annotations ─────────────────────────────

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationKind {
    #[default]
    Arrow,
    Line,
    Rect,
    Ellipse,
    Highlight,
    Pencil,
    Text,
    Label,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Annotation {
    pub kind: AnnotationKind,
    /// Coordinates are original image pixels. Rect/ellipse/line/arrow use the
    /// first and the last point; pencil/highlight keep every sample.
    pub points: Vec<Point>,
    /// `#rrggbb` or `#rrggbbaa`.
    pub color: String,
    /// Stroke width in original pixels.
    pub width: f32,
    pub filled: bool,
    pub text: Option<String>,
    pub font_size: f32,
}

impl Default for Annotation {
    fn default() -> Self {
        Self {
            kind: AnnotationKind::Arrow,
            points: Vec::new(),
            color: "#e5484d".to_string(),
            width: 3.0,
            filled: false,
            text: None,
            font_size: 18.0,
        }
    }
}

impl Annotation {
    pub fn with_points(kind: AnnotationKind, points: Vec<Point>) -> Self {
        Self {
            kind,
            points,
            ..Self::default()
        }
    }

    pub fn is_textual(&self) -> bool {
        matches!(self.kind, AnnotationKind::Text | AnnotationKind::Label)
    }

    pub fn bounds(&self) -> Option<RectF> {
        if self.points.is_empty() {
            return None;
        }
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for p in &self.points {
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }
        let pad = if self.is_textual() {
            self.font_size
        } else {
            self.width.max(2.0)
        };
        Some(RectF {
            x: min_x - pad,
            y: min_y - pad,
            w: (max_x - min_x) + pad * 2.0,
            h: (max_y - min_y) + pad * 2.0,
        })
    }

    pub fn hit(&self, point: Point, tolerance: f32) -> bool {
        let Some(bounds) = self.bounds() else {
            return false;
        };
        bounds.inflate(tolerance).contains(point)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RectF {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl RectF {
    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x <= self.right() && p.y >= self.y && p.y <= self.bottom()
    }

    pub fn inflate(&self, by: f32) -> Self {
        Self {
            x: self.x - by,
            y: self.y - by,
            w: self.w + by * 2.0,
            h: self.h + by * 2.0,
        }
    }

    pub fn intersects(&self, other: &RectF) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }
}

/// A stroke or fill in image coordinates. GPUI and tiny-skia both consume this
/// same list, so the editor and the exported file cannot disagree.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub points: Vec<Point>,
    pub closed: bool,
    pub filled: bool,
}

pub fn parse_hex_color(value: &str) -> [u8; 4] {
    let hex = value.trim().trim_start_matches('#');
    let parse = |s: &str| u8::from_str_radix(s, 16).unwrap_or(0);
    match hex.len() {
        3 => {
            let r = parse(&hex[0..1]);
            let g = parse(&hex[1..2]);
            let b = parse(&hex[2..3]);
            [r * 17, g * 17, b * 17, 255]
        }
        6 => [parse(&hex[0..2]), parse(&hex[2..4]), parse(&hex[4..6]), 255],
        8 => [
            parse(&hex[0..2]),
            parse(&hex[2..4]),
            parse(&hex[4..6]),
            parse(&hex[6..8]),
        ],
        _ => [229, 72, 77, 255],
    }
}

pub fn color_to_hex(rgba: [u8; 4]) -> String {
    if rgba[3] == 255 {
        format!("#{:02x}{:02x}{:02x}", rgba[0], rgba[1], rgba[2])
    } else {
        format!("#{:02x}{:02x}{:02x}{:02x}", rgba[0], rgba[1], rgba[2], rgba[3])
    }
}

fn ellipse_points(rect: RectF) -> Vec<Point> {
    const SEGMENTS: usize = 48;
    let cx = rect.x + rect.w / 2.0;
    let cy = rect.y + rect.h / 2.0;
    let rx = rect.w.abs() / 2.0;
    let ry = rect.h.abs() / 2.0;
    (0..SEGMENTS)
        .map(|i| {
            let angle = i as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
            Point::new(cx + rx * angle.cos(), cy + ry * angle.sin())
        })
        .collect()
}

fn arrow_head(shaft_start: Point, tip: Point, width: f32) -> (Point, Vec<Point>) {
    let dx = tip.x - shaft_start.x;
    let dy = tip.y - shaft_start.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.5 {
        return (tip, Vec::new());
    }
    let ux = dx / len;
    let uy = dy / len;
    let head = (width * 3.6).clamp(8.0, 44.0);
    let base = Point::new(tip.x - ux * head, tip.y - uy * head);
    let nx = -uy;
    let ny = ux;
    let half = head * 0.45;
    let left = Point::new(base.x + nx * half, base.y + ny * half);
    let right = Point::new(base.x - nx * half, base.y - ny * half);
    (base, vec![tip, left, right])
}

/// Strokes and fills for one annotation. Text and labels are drawn separately
/// from `text`/`font_size`.
pub fn annotation_geometry(annotation: &Annotation) -> Vec<Shape> {
    let first = annotation.points.first().copied();
    let last = annotation.points.last().copied();
    match annotation.kind {
        AnnotationKind::Line => match (first, last) {
            (Some(a), Some(b)) => vec![Shape {
                points: vec![a, b],
                closed: false,
                filled: false,
            }],
            _ => Vec::new(),
        },
        AnnotationKind::Arrow => match (first, last) {
            (Some(a), Some(b)) => {
                let (base, head) = arrow_head(a, b, annotation.width);
                let mut shapes = vec![Shape {
                    points: vec![a, base],
                    closed: false,
                    filled: false,
                }];
                if head.len() == 3 {
                    shapes.push(Shape {
                        points: head,
                        closed: true,
                        filled: true,
                    });
                }
                shapes
            }
            _ => Vec::new(),
        },
        AnnotationKind::Rect => match (first, last) {
            (Some(a), Some(b)) => {
                let rect = RectF {
                    x: a.x.min(b.x),
                    y: a.y.min(b.y),
                    w: (b.x - a.x).abs(),
                    h: (b.y - a.y).abs(),
                };
                vec![Shape {
                    points: vec![
                        Point::new(rect.x, rect.y),
                        Point::new(rect.right(), rect.y),
                        Point::new(rect.right(), rect.bottom()),
                        Point::new(rect.x, rect.bottom()),
                    ],
                    closed: true,
                    filled: annotation.filled,
                }]
            }
            _ => Vec::new(),
        },
        AnnotationKind::Ellipse => match (first, last) {
            (Some(a), Some(b)) => {
                let rect = RectF {
                    x: a.x.min(b.x),
                    y: a.y.min(b.y),
                    w: (b.x - a.x).abs(),
                    h: (b.y - a.y).abs(),
                };
                vec![Shape {
                    points: ellipse_points(rect),
                    closed: true,
                    filled: annotation.filled,
                }]
            }
            _ => Vec::new(),
        },
        AnnotationKind::Highlight | AnnotationKind::Pencil => {
            if annotation.points.len() < 2 {
                Vec::new()
            } else {
                vec![Shape {
                    points: annotation.points.clone(),
                    closed: false,
                    filled: false,
                }]
            }
        }
        AnnotationKind::Label => {
            let p = first.unwrap_or(Point::new(0.0, 0.0));
            let r = annotation.font_size.max(8.0);
            let rect = RectF {
                x: p.x - r,
                y: p.y - r,
                w: r * 2.0,
                h: r * 2.0,
            };
            vec![Shape {
                points: ellipse_points(rect),
                closed: true,
                filled: true,
            }]
        }
        AnnotationKind::Text => Vec::new(),
    }
}

/// Shifts every annotation after a destructive crop and drops annotations that
/// fell completely outside the frame.
pub fn shift_annotations_after_crop(annotations: &mut Vec<Annotation>, crop: CropRect) {
    let frame = RectF {
        x: 0.0,
        y: 0.0,
        w: crop.w,
        h: crop.h,
    };
    for annotation in annotations.iter_mut() {
        for p in &mut annotation.points {
            p.x -= crop.x;
            p.y -= crop.y;
        }
    }
    annotations.retain(|annotation| annotation.bounds().is_some_and(|b| b.intersects(&frame)));
}

pub fn scale_annotations(annotations: &mut [Annotation], factor: f32) {
    if !(factor.is_finite() && factor > 0.0) {
        return;
    }
    for annotation in annotations {
        for p in &mut annotation.points {
            p.x *= factor;
            p.y *= factor;
        }
        annotation.width *= factor;
        annotation.font_size *= factor;
    }
}

/// Crop actually applied for display, or `None` when the stored rectangle is
/// the whole picture (so a stale full-size crop cannot change the layout).
pub fn visible_crop(edit: &ImageEdit, natural: (u32, u32)) -> Option<CropRect> {
    let crop = edit.crop.as_ref()?.clamp_to(natural);
    let full = (crop.w >= natural.0 as f32 - 0.5) && (crop.h >= natural.1 as f32 - 0.5);
    if full && crop.x <= 0.5 && crop.y <= 0.5 {
        None
    } else {
        Some(crop)
    }
}

/// Height of a caption block (above or below the picture), including the gap.
pub fn caption_height(edit: &ImageEdit, display_width: f32) -> f32 {
    let Some(caption) = edit.caption.as_ref() else {
        return 0.0;
    };
    if caption.text.trim().is_empty() {
        return 0.0;
    }
    let char_w = 6.2_f32;
    let per_line = (display_width.max(80.0) / char_w).floor().max(1.0);
    let lines = (caption.text.chars().count() as f32 / per_line).ceil().max(1.0);
    lines * 18.0 + 6.0
}

// ───────────────────────────── Edit sidecar ─────────────────────────────

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

fn edit_cache() -> &'static Mutex<HashMap<PathBuf, ImageEdit>> {
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

/// Copies `name` (and its `.edit.json`) into the target note's attachments
/// under a fresh name. Used when a reference is pasted into another note.
pub fn copy_attachment(
    vault_root: Option<&Path>,
    note_path: &Path,
    folder: &str,
    name: &str,
) -> Option<String> {
    copy_attachment_to(vault_root, note_path, note_path, folder, name)
}

/// Copies an attachment (and its `.edit.json`) from one note's attachment
/// folder into another note's, keeping every pixel and edit intact under a
/// fresh unique name.
pub fn copy_attachment_to(
    vault_root: Option<&Path>,
    from_note: &Path,
    to_note: &Path,
    folder: &str,
    name: &str,
) -> Option<String> {
    let source = resolve_attachment(vault_root, from_note, folder, name)?;
    let bytes = std::fs::read(&source).ok()?;
    let ext = source
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("png");
    let dir = attachments_dir(to_note, folder);
    let (new_name, new_path) = write_new(&dir, ext, &bytes).ok()?;
    let source_edit = edit_path(&source);
    if source_edit.is_file() {
        let target_edit = edit_path(&new_path);
        let _ = std::fs::copy(&source_edit, &target_edit);
    }
    Some(new_name)
}

/// One image reference resolved to a file on disk and to its painted size.
#[derive(Debug, Clone)]
pub struct ResolvedImage {
    pub path: PathBuf,
    pub reference: ImageReference,
    pub natural: (u32, u32),
    /// Size of the region actually shown (crop size, or the whole picture).
    pub visible: (u32, u32),
    /// Crop applied for display; `None` when the whole picture is shown.
    pub crop: Option<CropRect>,
    pub show_original: bool,
    pub width: f32,
    pub height: f32,
    /// Widest the picture may be painted in the current text column.
    pub max_width: f32,
    pub edit: ImageEdit,
}

impl ResolvedImage {
    pub fn radius_px(&self) -> f32 {
        radius_px(&self.edit, self.width, self.height)
    }
}

type ResolvedKey = (PathBuf, String, usize, bool);

fn resolved_images() -> &'static Mutex<HashMap<ResolvedKey, std::sync::Arc<ResolvedImage>>> {
    static CACHE: OnceLock<Mutex<HashMap<ResolvedKey, std::sync::Arc<ResolvedImage>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Drops painted-image geometry. Called when a sidecar changes or the vault
/// rescans, so the next frame re-reads the radius and the file size.
pub fn invalidate_resolved_images() {
    if let Ok(mut cache) = resolved_images().lock() {
        cache.clear();
    }
}

pub fn resolve_for_note(
    note_path: &Path,
    line: &str,
    wrap_cols: usize,
    show_original: bool,
) -> Option<std::sync::Arc<ResolvedImage>> {
    let key = (note_path.to_path_buf(), line.to_string(), wrap_cols, show_original);
    if let Ok(cache) = resolved_images().lock() {
        if let Some(image) = cache.get(&key) {
            return Some(image.clone());
        }
    }
    let reference = parse_image_reference(line)?;
    let root = vault_root();
    let path = resolve_attachment(root.as_deref(), note_path, &folder(), &reference.target)?;
    let natural = image_size(&path)?;
    let edit = load_edit(&path);
    let crop = if show_original { None } else { visible_crop(&edit, natural) };
    let visible = crop.map_or(natural, |crop| crop.visible_size());
    let max_width = text_width(wrap_cols);
    let (width, height) = reference.display_size(visible, max_width);
    let image = std::sync::Arc::new(ResolvedImage {
        path,
        reference,
        natural,
        visible,
        crop,
        show_original,
        width,
        height,
        max_width,
        edit,
    });
    if let Ok(mut cache) = resolved_images().lock() {
        cache.insert(key, std::sync::Arc::clone(&image));
    }
    Some(image)
}

/// Height of an image block, in px, for the visual-row height index.
pub fn block_height(
    note_path: &Path,
    line: &str,
    wrap_cols: usize,
    show_original: bool,
) -> Option<f32> {
    let reference = parse_image_reference(line)?;
    let root = vault_root();
    let path = resolve_attachment(root.as_deref(), note_path, &folder(), &reference.target)?;
    let natural = image_size(&path)?;
    let edit = load_edit(&path);
    let crop = if show_original { None } else { visible_crop(&edit, natural) };
    let visible = crop.map_or(natural, |crop| crop.visible_size());
    let (width, height) = reference.display_size(visible, text_width(wrap_cols));
    // Captions are hidden while the full original is shown for editing; the
    // row then matches the picture exactly so the floating frame lines up.
    let caption = if show_original {
        0.0
    } else {
        // The caption slot is always at least as tall as the inline editor, so
        // opening it never moves the picture or the text below.
        let content = (caption_height(&edit, width) - 6.0).max(0.0);
        content.max(CAPTION_SLOT_H) + 6.0 + CAPTION_SEPARATION
    };
    Some((height + caption + BLOCK_PADDING * 2.0 + TOOLBAR_RESERVE).max(MIN_BLOCK_HEIGHT))
}

/// Height of the inline caption editor content (field row + breathing room).
pub const CAPTION_EDITOR_H: f32 = 40.0;
/// The caption slot always reserves this much room below/above the picture, so
/// switching between the static caption and the editor does not reflow.
pub const CAPTION_SLOT_H: f32 = CAPTION_EDITOR_H;
/// Extra distance between the picture and its caption (on top of the flex gap).
pub const CAPTION_SEPARATION: f32 = 4.0;

/// Vertical room around the image inside its row.
pub const BLOCK_PADDING: f32 = 8.0;

/// Height of the floating image action bar. Buttons must fit inside.
pub const TOOLBAR_H: f32 = 34.0;
/// Empty band above every picture. The bar is painted inside it, so hovering
/// or selecting a picture never overlaps the text that precedes it.
pub const TOOLBAR_BAND: f32 = 38.0;
/// Band plus the flex gap between the band and the picture/caption. This is
/// what the row height has to add to `height + caption`.
pub const TOOLBAR_RESERVE: f32 = TOOLBAR_BAND + 6.0;
pub const MIN_BLOCK_HEIGHT: f32 = 32.0;

// The vault root and folder name are process-wide: the editor window is built
// from view state that has no access to the vault, and a single app instance
// works with a single vault at a time.
static VAULT_ROOT: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
static FOLDER: OnceLock<Mutex<String>> = OnceLock::new();

fn vault_cell() -> &'static Mutex<Option<PathBuf>> {
    VAULT_ROOT.get_or_init(|| Mutex::new(None))
}

fn folder_cell() -> &'static Mutex<String> {
    FOLDER.get_or_init(|| Mutex::new(DEFAULT_FOLDER.to_string()))
}

pub fn set_vault_root(root: Option<PathBuf>) {
    *vault_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = root;
    invalidate_index();
}

pub fn vault_root() -> Option<PathBuf> {
    vault_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

pub fn set_folder(folder: &str) {
    let normalized = folder_name(folder);
    let mut guard = folder_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if *guard != normalized {
        *guard = normalized;
    }
}

pub fn folder() -> String {
    folder_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbaImage::new(width, height)
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        bytes
    }

    #[test]
    fn names_are_timestamped_and_unique() {
        let first = new_name("png");
        let second = new_name("png");
        assert!(first.ends_with(".png"));
        assert!(first.len() > 20);
        assert_ne!(first, second);
    }

    #[test]
    fn a_wiki_reference_parses_with_and_without_size() {
        let plain = parse_image_reference("![[2026-10-31-1515-k3x9.png]]").unwrap();
        assert_eq!(plain.target, "2026-10-31-1515-k3x9.png");
        assert_eq!((plain.width, plain.height), (None, None));
        let sized = parse_image_reference("  ![[a.png|300x200]]  ").unwrap();
        assert_eq!((sized.width, sized.height), (Some(300), Some(200)));
        assert_eq!(sized.markup(), "![[a.png|300x200]]");
        let width = parse_image_reference("![[a.png|300]]").unwrap();
        assert_eq!(width.markup(), "![[a.png|300]]");
    }

    #[test]
    fn text_and_remote_urls_are_not_attachments() {
        assert!(parse_image_reference("text ![[a.png]]").is_none());
        assert!(parse_image_reference("![alt](https://example.com/a.png)").is_none());
        assert!(parse_image_reference("![alt](attachments/a.png)").is_some());
    }

    #[test]
    fn display_size_keeps_the_ratio_and_fits_the_column() {
        let image = parse_image_reference("![[a.png]]").unwrap();
        let (w, h) = image.display_size((2000, 1000), 600.0);
        assert_eq!((w, h), (600.0, 300.0));
        let sized = image.with_size(Some(300), None);
        let (w, h) = sized.display_size((2000, 1000), 600.0);
        assert_eq!((w, h), (300.0, 150.0));
        let both = image.with_size(Some(300), Some(400));
        assert_eq!(both.display_size((2000, 1000), 600.0), (300.0, 400.0));
    }

    #[test]
    fn clipboard_raster_is_stored_as_png() {
        let dir = tempfile::tempdir().unwrap();
        let note = dir.path().join("Note.md");
        std::fs::write(&note, "").unwrap();
        let imported = import_images(
            &note,
            "attachments",
            ImageImportSource::Clipboard { bytes: png(4, 3), format: gpui::ImageFormat::Png },
        );
        let image = imported.into_iter().next().unwrap().unwrap();
        assert_eq!((image.width, image.height), (4, 3));
        assert!(image.path.is_file());
        assert!(image.path.parent().unwrap().ends_with("attachments"));
    }

    #[test]
    fn a_file_is_copied_byte_for_byte_and_resolved_beside_the_note() {
        let dir = tempfile::tempdir().unwrap();
        let note = dir.path().join("sub").join("Note.md");
        std::fs::create_dir_all(note.parent().unwrap()).unwrap();
        std::fs::write(&note, "").unwrap();
        let source = dir.path().join("source.png");
        std::fs::write(&source, png(2, 2)).unwrap();
        let imported = import_images(&note, "attachments", ImageImportSource::Files(vec![source]))
            .into_iter()
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(std::fs::read(&imported.path).unwrap(), png(2, 2));
        let resolved = resolve_attachment(Some(dir.path()), &note, "attachments", &imported.name).unwrap();
        assert_eq!(resolved, imported.path);
    }

    #[test]
    fn a_damaged_file_is_reported_and_other_files_still_import() {
        let dir = tempfile::tempdir().unwrap();
        let note = dir.path().join("Note.md");
        std::fs::write(&note, "").unwrap();
        let bad = dir.path().join("bad.png");
        std::fs::write(&bad, b"not an image").unwrap();
        let good_source = dir.path().join("good.png");
        std::fs::write(&good_source, png(1, 1)).unwrap();
        let results = import_images(
            &note,
            "attachments",
            ImageImportSource::Files(vec![bad, good_source]),
        );
        assert!(results[0].is_err());
        assert!(results[1].is_ok());
    }

    #[test]
    fn edit_sidecar_is_dropped_when_it_becomes_empty() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("a.png");
        std::fs::write(&image, png(1, 1)).unwrap();
        set_corner_radius(&image, Some(8.0)).unwrap();
        assert!(edit_path(&image).is_file());
        assert_eq!(load_edit(&image).corner_radius, Some(8.0));
        set_corner_radius(&image, None).unwrap();
        assert!(!edit_path(&image).is_file());
    }

    #[test]
    fn a_dib_is_wrapped_into_a_decodable_bmp() {
        let width = 2u32;
        let height = 2u32;
        let stride = ((width * 3).div_ceil(4) * 4) as usize;
        let mut dib = vec![0u8; 40 + stride * height as usize];
        dib[0..4].copy_from_slice(&40u32.to_le_bytes());
        dib[4..8].copy_from_slice(&width.to_le_bytes());
        dib[8..12].copy_from_slice(&height.to_le_bytes());
        dib[12..14].copy_from_slice(&1u16.to_le_bytes());
        dib[14..16].copy_from_slice(&24u16.to_le_bytes());
        let bmp = dib_to_bmp(&dib).unwrap();
        assert_eq!(&bmp[0..2], b"BM");
        assert_eq!(u32::from_le_bytes(bmp[2..6].try_into().unwrap()) as usize, bmp.len());
        assert_eq!(u32::from_le_bytes(bmp[10..14].try_into().unwrap()), 14 + 40);
        let decoded = image::load_from_memory_with_format(&bmp, image::ImageFormat::Bmp).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (2, 2));
    }

    #[test]
    fn radius_is_a_share_of_the_shorter_side() {
        let edit = ImageEdit { corner_radius: Some(10.0), ..Default::default() };
        assert_eq!(radius_px(&edit, 400.0, 200.0), 20.0);
    }

    #[test]
    fn copying_between_notes_keeps_pixels_and_sidecar_under_a_new_name() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let from = root.join("a").join("NoteA.md");
        let to = root.join("b").join("NoteB.md");
        std::fs::create_dir_all(from.parent().unwrap()).unwrap();
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        let source_dir = attachments_dir(&from, "Files");
        std::fs::create_dir_all(&source_dir).unwrap();
        let source = source_dir.join("pic.png");
        std::fs::write(&source, b"pixels").unwrap();
        std::fs::write(edit_path(&source), b"{\"version\":1}").unwrap();

        let copied = copy_attachment_to(Some(root), &from, &to, "Files", "pic.png").unwrap();
        assert_ne!(copied, "pic.png");
        let target = attachments_dir(&to, "Files").join(&copied);
        assert_eq!(std::fs::read(&target).unwrap(), b"pixels");
        assert!(edit_path(&target).is_file());
        assert!(source.is_file());
    }

    #[test]
    fn saving_without_a_reference_moves_the_pair_to_the_trash() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let note = root.join("Test").join("test1.md");
        std::fs::create_dir_all(note.parent().unwrap()).unwrap();
        std::fs::write(&note, "").unwrap();
        let source_dir = attachments_dir(&note, "Files");
        std::fs::create_dir_all(&source_dir).unwrap();
        let source = source_dir.join("pic.png");
        std::fs::write(&source, b"pixels").unwrap();
        std::fs::write(edit_path(&source), b"{\"version\":1}").unwrap();

        let mut previous = std::collections::HashSet::new();
        previous.insert("pic.png".to_string());
        reconcile_saved_references(
            Some(root),
            &note,
            "Files",
            &previous,
            &std::collections::HashSet::new(),
        );

        assert!(!source.exists());
        let trashed = trash_dir(root, "Files").join("pic.png");
        assert_eq!(std::fs::read(&trashed).unwrap(), b"pixels");
        assert!(edit_path(&trashed).is_file());
    }

    #[test]
    fn moving_between_notes_preserves_the_name_and_the_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let from = root.join("a").join("NoteA.md");
        let to = root.join("b").join("NoteB.md");
        std::fs::create_dir_all(from.parent().unwrap()).unwrap();
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::write(&from, "").unwrap();
        std::fs::write(&to, "").unwrap();
        let source_dir = attachments_dir(&from, "Files");
        std::fs::create_dir_all(&source_dir).unwrap();
        let source = source_dir.join("pic.png");
        std::fs::write(&source, b"pixels").unwrap();
        std::fs::write(edit_path(&source), b"{\"version\":1}").unwrap();

        let moved = move_attachment(Some(root), &from, &to, "Files", "pic.png").unwrap();
        assert_eq!(moved, "pic.png");
        let target = attachments_dir(&to, "Files").join("pic.png");
        assert_eq!(std::fs::read(&target).unwrap(), b"pixels");
        assert!(edit_path(&target).is_file());
        assert!(!source.exists());
    }

    #[test]
    fn referenced_elsewhere_streams_files_and_ignores_case() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let note1 = root.join("Note1.md");
        let note2 = root.join("Note2.md");
        let huge = root.join("Huge.md");

        std::fs::write(&note1, "# Title\nSome text\n![[Target.PNG]]\nMore text").unwrap();
        std::fs::write(&note2, "# Other\n![[other.png]]").unwrap();

        // Simulate a large file (> 256KB) filled with filler lines to ensure
        // buffer boundary traversal works properly and does not read whole files.
        let mut filler = String::with_capacity(300 * 1024);
        for i in 0..10_000 {
            filler.push_str("ordinary line of text without any image references\n");
            if i == 5000 {
                filler.push_str("![[shared.png]]\n");
            }
        }
        std::fs::write(&huge, &filler).unwrap();

        // 1. Found in Note1 (case-insensitive)
        assert!(referenced_elsewhere(root, "target.png", &[]));
        assert!(referenced_elsewhere(root, "Target.PNG", &[]));

        // 2. Excluded note is skipped
        assert!(!referenced_elsewhere(root, "target.png", &[&note1]));

        // 3. Found deep inside a large file across chunks
        assert!(referenced_elsewhere(root, "shared.png", &[]));

        // 4. Non-existent reference is not found
        assert!(!referenced_elsewhere(root, "nonexistent.png", &[]));
    }
}

// ─────────────────────── Platform clipboard ───────────────────────
//
// GPUI's Windows reader understands only the app-registered `PNG` / `GIF` /
// `JFIF` formats and joins CF_HDROP names without separators. Screenshots and
// Explorer copies use CF_DIB and CF_HDROP, so both are read here (limitation 5
// of the spec: the platform part stays in an adapter).

#[cfg(windows)]
pub fn clipboard_raster() -> Option<(Vec<u8>, gpui::ImageFormat)> {
    let dib = win_clipboard::read_dib()?;
    let bmp = dib_to_bmp(&dib)?;
    let image = image::load_from_memory_with_format(&bmp, image::ImageFormat::Bmp).ok()?;
    let mut png = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    Some((png, gpui::ImageFormat::Png))
}

#[cfg(not(windows))]
pub fn clipboard_raster() -> Option<(Vec<u8>, gpui::ImageFormat)> {
    None
}

/// Sidecar-independent attachment payload for Cut/Copy across notes. GPUI's
/// own metadata channel is text-hash checked, which is easy to lose to a
/// CRLF normalisation; this is a plain private clipboard format.
#[cfg(windows)]
pub fn set_clipboard_attachment_meta(note: &Path, name: &str, cut: bool) {
    let json = serde_json::json!({
        "name": name,
        "note": note.to_string_lossy(),
        "cut": cut,
    })
    .to_string();
    win_clipboard::write_meta(&json);
}

#[cfg(not(windows))]
pub fn set_clipboard_attachment_meta(_note: &Path, _name: &str, _cut: bool) {}

#[cfg(windows)]
pub fn clipboard_attachment_meta() -> Option<serde_json::Value> {
    let text = win_clipboard::read_meta()?;
    serde_json::from_str(&text).ok()
}

#[cfg(not(windows))]
pub fn clipboard_attachment_meta() -> Option<serde_json::Value> {
    None
}

/// File list from CF_HDROP (Explorer's Ctrl+C). `None` when the clipboard holds
/// no file list at all.
#[cfg(windows)]
pub fn clipboard_file_paths() -> Option<Vec<PathBuf>> {
    win_clipboard::read_files()
}

#[cfg(not(windows))]
pub fn clipboard_file_paths() -> Option<Vec<PathBuf>> {
    None
}

/// Wraps a `CF_DIB` payload into a BMP file so the `image` crate can decode it.
/// The DIB has no `BITMAPFILEHEADER`; the pixel array starts after the info
/// header and its palette.
pub fn dib_to_bmp(dib: &[u8]) -> Option<Vec<u8>> {
    if dib.len() < 16 {
        return None;
    }
    let header_size = u32::from_le_bytes(dib[0..4].try_into().ok()?) as usize;
    if header_size < 12 || header_size > dib.len() {
        return None;
    }
    let palette_entry = if header_size == 12 { 3 } else { 4 };
    let bit_count_offset = if header_size == 12 { 10 } else { 14 };
    if dib.len() < bit_count_offset + 2 {
        return None;
    }
    let bit_count =
        u16::from_le_bytes(dib[bit_count_offset..bit_count_offset + 2].try_into().ok()?) as usize;
    let colors_used = if header_size >= 40 && dib.len() >= 40 {
        u32::from_le_bytes(dib[32..36].try_into().ok()?) as usize
    } else {
        0
    };
    let palette_count = if bit_count <= 8 {
        if colors_used > 0 {
            colors_used
        } else {
            1usize << bit_count
        }
    } else if colors_used > 0 {
        colors_used
    } else {
        0
    };
    let pixel_offset = 14 + header_size + palette_count * palette_entry;
    let file_size = 14 + dib.len();
    let mut bmp = Vec::with_capacity(file_size);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&(file_size as u32).to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes());
    bmp.extend_from_slice(&(pixel_offset as u32).to_le_bytes());
    bmp.extend_from_slice(dib);
    Some(bmp)
}

#[cfg(windows)]
mod win_clipboard {
    use std::path::PathBuf;
    use std::time::Duration;

    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
        RegisterClipboardFormatW, SetClipboardData,
    };
    use windows_sys::Win32::System::Ole::{CF_DIB, CF_HDROP};
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
    };
    use windows_sys::Win32::UI::Shell::DragQueryFileW;

    fn open() -> bool {
        for _ in 0..5 {
            if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
                return true;
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        false
    }

    pub(super) fn read_dib() -> Option<Vec<u8>> {
        if !open() {
            return None;
        }
        let result = (|| {
            if unsafe { IsClipboardFormatAvailable(CF_DIB as u32) } == 0 {
                return None;
            }
            let handle = unsafe { GetClipboardData(CF_DIB as u32) };
            if handle.is_null() {
                return None;
            }
            let size = unsafe { GlobalSize(handle) };
            if size == 0 {
                return None;
            }
            let ptr = unsafe { GlobalLock(handle) };
            if ptr.is_null() {
                return None;
            }
            let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, size).to_vec() };
            unsafe { GlobalUnlock(handle) };
            Some(bytes)
        })();
        unsafe { CloseClipboard() };
        result
    }

    pub(super) fn read_files() -> Option<Vec<PathBuf>> {
        if !open() {
            return None;
        }
        let result = (|| {
            if unsafe { IsClipboardFormatAvailable(CF_HDROP as u32) } == 0 {
                return None;
            }
            let handle = unsafe { GetClipboardData(CF_HDROP as u32) };
            if handle.is_null() {
                return None;
            }
            let mut paths = Vec::new();
            let count = unsafe { DragQueryFileW(handle, u32::MAX, std::ptr::null_mut(), 0) };
            for index in 0..count {
                let len = unsafe { DragQueryFileW(handle, index, std::ptr::null_mut(), 0) };
                if len == 0 {
                    continue;
                }
                let mut buffer = vec![0u16; len as usize + 1];
                let written = unsafe { DragQueryFileW(handle, index, buffer.as_mut_ptr(), buffer.len() as u32) };
                if written == 0 {
                    continue;
                }
                buffer.truncate(written as usize);
                paths.push(PathBuf::from(String::from_utf16_lossy(&buffer)));
            }
            (!paths.is_empty()).then_some(paths)
        })();
        unsafe { CloseClipboard() };
        result
    }

    fn meta_format() -> u32 {
        static FORMAT: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
        *FORMAT.get_or_init(|| unsafe {
            RegisterClipboardFormatW(windows_sys::w!("VibecodiumAttachment"))
        })
    }

    pub(super) fn read_meta() -> Option<String> {
        if !open() {
            return None;
        }
        let result = (|| {
            let format = meta_format();
            if unsafe { IsClipboardFormatAvailable(format) } == 0 {
                return None;
            }
            let handle = unsafe { GetClipboardData(format) };
            if handle.is_null() {
                return None;
            }
            let size = unsafe { GlobalSize(handle) };
            if size < 2 {
                return None;
            }
            let ptr = unsafe { GlobalLock(handle) };
            if ptr.is_null() {
                return None;
            }
            let data = unsafe {
                std::slice::from_raw_parts(ptr as *const u16, size / 2)
            };
            let len = data.iter().position(|c| *c == 0).unwrap_or(data.len());
            let out = String::from_utf16_lossy(&data[..len]);
            unsafe { GlobalUnlock(handle) };
            Some(out)
        })();
        unsafe { CloseClipboard() };
        result
    }

    pub(super) fn write_meta(json: &str) -> bool {
        if !open() {
            return false;
        }
        let result = (|| {
            let wide: Vec<u16> = json.encode_utf16().chain(Some(0)).collect();
            let bytes = wide.len() * 2;
            let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes) };
            if handle.is_null() {
                return false;
            }
            let ptr = unsafe { GlobalLock(handle) };
            if ptr.is_null() {
                unsafe { GlobalFree(handle) };
                return false;
            }
            unsafe {
                std::ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, ptr as *mut u8, bytes);
                GlobalUnlock(handle);
            }
            let stored = unsafe { SetClipboardData(meta_format(), handle) };
            if stored.is_null() {
                unsafe { GlobalFree(handle) };
                return false;
            }
            true
        })();
        unsafe { CloseClipboard() };
        result
    }
}
// ───────────────────────────── References & lifecycle ─────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefScan {
    Normal,
    Bang,
    Open,
    Wiki,
    WikiSize,
    Close,
    AfterClose,
    Inline,
}

/// Names referenced by `![[name]]` and `![[name|size]]` in one text.
pub fn references_in_text(text: &str) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    scan_references(text.chars(), &mut out);
    out
}

/// Same scan over the document rope. Only image references allocate.
pub fn references_in_rope(rope: &crop::Rope) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    scan_references(rope.chars(), &mut out);
    out
}

fn scan_references(chars: impl Iterator<Item = char>, out: &mut std::collections::HashSet<String>) {
    let mut state = RefScan::Normal;
    let mut buffer = String::new();
    let finish = |buffer: &mut String, out: &mut std::collections::HashSet<String>| {
        let name = buffer.trim();
        if !name.is_empty() && !name.starts_with("http://") && !name.starts_with("https://") {
            out.insert(name.to_string());
        }
        buffer.clear();
    };
    for ch in chars {
        state = match state {
            RefScan::Normal => {
                if ch == '!' {
                    RefScan::Bang
                } else {
                    RefScan::Normal
                }
            }
            RefScan::Bang => {
                if ch == '[' {
                    RefScan::Open
                } else if ch == '!' {
                    RefScan::Bang
                } else {
                    RefScan::Normal
                }
            }
            RefScan::Open => {
                if ch == '[' {
                    buffer.clear();
                    RefScan::Wiki
                } else if ch == ']' {
                    RefScan::AfterClose
                } else {
                    RefScan::Normal
                }
            }
            RefScan::Wiki => match ch {
                '|' => RefScan::WikiSize,
                ']' => RefScan::Close,
                '\n' => {
                    buffer.clear();
                    RefScan::Normal
                }
                other => {
                    buffer.push(other);
                    RefScan::Wiki
                }
            },
            RefScan::WikiSize => match ch {
                ']' => RefScan::Close,
                '\n' => {
                    buffer.clear();
                    RefScan::Normal
                }
                _ => RefScan::WikiSize,
            },
            RefScan::Close => {
                if ch == ']' {
                    finish(&mut buffer, out);
                } else {
                    buffer.clear();
                }
                RefScan::Normal
            }
            RefScan::AfterClose => {
                if ch == '(' {
                    buffer.clear();
                    RefScan::Inline
                } else {
                    RefScan::Normal
                }
            }
            RefScan::Inline => match ch {
                ')' => {
                    finish(&mut buffer, out);
                    RefScan::Normal
                }
                '\n' => {
                    buffer.clear();
                    RefScan::Normal
                }
                other => {
                    buffer.push(other);
                    RefScan::Inline
                }
            },
        };
    }
}

fn trash_dir(vault_root: &Path, folder: &str) -> PathBuf {
    vault_root.join(folder_name(folder)).join(".trash")
}

fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(name);
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().map(|s| s.to_string_lossy().into_owned());
    for n in 1..10_000u32 {
        let candidate_name = match &ext {
            Some(ext) => format!("{stem} {n}.{ext}"),
            None => format!("{stem} {n}"),
        };
        let candidate = dir.join(candidate_name);
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(name)
}

/// Moves one attachment (and its sidecar) into the global attachment trash.
pub fn trash_pair(vault_root: &Path, folder: &str, source: &Path) -> Option<PathBuf> {
    let name = source.file_name()?.to_string_lossy().into_owned();
    let dir = trash_dir(vault_root, folder);
    std::fs::create_dir_all(&dir).ok()?;
    let target = unique_path(&dir, &name);
    std::fs::rename(source, &target).ok()?;
    let source_edit = edit_path(source);
    if source_edit.is_file() {
        let target_edit = edit_path(&target);
        let _ = std::fs::rename(&source_edit, &target_edit);
    }
    invalidate_index();
    Some(target)
}

/// Returns a trashed attachment next to the note when the note references it
/// again (undo after a save).
pub fn restore_if_trashed(
    vault_root: &Path,
    note_path: &Path,
    folder: &str,
    name: &str,
) -> Option<PathBuf> {
    let destination = attachments_dir(note_path, folder).join(name);
    if destination.exists() {
        return Some(destination);
    }
    let trashed = trash_dir(vault_root, folder).join(name);
    if !trashed.is_file() {
        return None;
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).ok()?;
    }
    std::fs::rename(&trashed, &destination).ok()?;
    let trashed_edit = edit_path(&trashed);
    if trashed_edit.is_file() {
        let destination_edit = edit_path(&destination);
        let _ = std::fs::rename(&trashed_edit, &destination_edit);
    }
    invalidate_index();
    Some(destination)
}

fn is_note_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()).map(|ext| ext.to_ascii_lowercase()).as_deref(),
        Some("md") | Some("markdown") | Some("txt")
    )
}

fn walk_markdown(root: &Path, dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 16 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "target" || name == "node_modules" || name == "vendor" {
            continue;
        }
        if path.is_dir() {
            walk_markdown(root, &path, depth + 1, out);
        } else if path.starts_with(root) && is_note_file(&path) {
            // Skip binary/oversized files (> 50 MB, matching vault.rs BINARY_LIMIT).
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if size <= 50 * 1024 * 1024 {
                out.push(path);
            }
        }
    }
}

fn is_image_reference_at(buf: &[u8], pos: usize, needle_len: usize) -> bool {
    // 1. Boundary check: character immediately preceding needle cannot be part of the filename
    if pos > 0 {
        let prev = buf[pos - 1];
        if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'-' || prev == b'.' {
            return false;
        }
    }
    // 2. Boundary check: character immediately following needle cannot be part of the filename
    let end_pos = pos + needle_len;
    if end_pos < buf.len() {
        let next = buf[end_pos];
        if next.is_ascii_alphanumeric() || next == b'_' || next == b'-' || next == b'.' {
            return false;
        }
    }

    // 3. Look backwards up to 1024 bytes for `![` on the same line
    let lookback_start = pos.saturating_sub(1024);
    let prefix = &buf[lookback_start..pos];
    let Some(bang_bracket_idx) = memchr::memmem::rfind(prefix, b"![") else {
        return false;
    };
    let between = &prefix[bang_bracket_idx + 2..];
    // No newlines allowed between `![` and the reference name
    if memchr::memchr(b'\n', between).is_some() || memchr::memchr(b'\r', between).is_some() {
        return false;
    }

    // 4. Look forwards up to 512 bytes on the same line for closing `]` or `)`
    let lookahead_end = (end_pos + 512).min(buf.len());
    let suffix = &buf[end_pos..lookahead_end];
    let suffix = if let Some(newline_idx) = memchr::memchr(b'\n', suffix) {
        &suffix[..newline_idx]
    } else {
        suffix
    };

    suffix.contains(&b']') || suffix.contains(&b')')
}

/// Checks whether a single markdown file references `name`, without loading
/// the whole file into memory, using streaming SIMD scanning.
pub fn note_references_attachment(path: &Path, name: &str) -> bool {
    let needle = name.as_bytes();
    if needle.is_empty() {
        return false;
    }
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };

    let mut needle_lower = needle.to_vec();
    needle_lower.make_ascii_lowercase();
    let finder = memchr::memmem::Finder::new(&needle_lower);

    let mut buf = [0u8; 64 * 1024];
    let mut lower_buf = [0u8; 64 * 1024];
    let min_overlap = 2048.max(needle.len() + 64);
    if min_overlap >= buf.len() {
        return false;
    }
    let mut overlap = 0;

    loop {
        let read = match std::io::Read::read(&mut file, &mut buf[overlap..]) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => return false,
        };
        let valid = overlap + read;
        if valid >= needle.len() {
            // An attachment in a note is always referenced via `![` (markdown or wikilink).
            // If the buffer does not even contain `![`, it cannot contain any image references.
            if memchr::memmem::find(&buf[..valid], b"![").is_some() {
                lower_buf[..valid].copy_from_slice(&buf[..valid]);
                lower_buf[..valid].make_ascii_lowercase();

                let mut search_from = 0;
                while search_from + needle.len() <= valid {
                    if let Some(rel_pos) = finder.find(&lower_buf[search_from..valid]) {
                        let pos = search_from + rel_pos;
                        if is_image_reference_at(&lower_buf[..valid], pos, needle.len()) {
                            return true;
                        }
                        search_from = pos + 1;
                    } else {
                        break;
                    }
                }
            }
            overlap = min_overlap.min(valid);
            buf.copy_within(valid - overlap..valid, 0);
        } else {
            overlap = valid;
        }
    }
    false
}

/// Collects every image reference from a markdown file without loading the
/// whole file into memory.
pub fn references_in_file(path: &Path, out: &mut std::collections::HashSet<String>) {
    let Ok(file) = std::fs::File::open(path) else {
        return;
    };
    use std::io::BufRead;
    let mut reader = std::io::BufReader::new(file);
    let mut line = String::new();
    while let Ok(n) = reader.read_line(&mut line) {
        if n == 0 {
            break;
        }
        if line.len() <= 4096 && (line.contains("![[") || line.contains("![")) {
            scan_references(line.chars(), out);
        }
        line.clear();
    }
}

fn is_excluded_note(path: &Path, exclude: &[&Path]) -> bool {
    exclude.iter().any(|excluded| {
        #[cfg(windows)]
        {
            excluded.as_os_str().eq_ignore_ascii_case(path.as_os_str())
        }
        #[cfg(not(windows))]
        {
            *excluded == path
        }
    })
}

/// True when some other note in the vault references `name`.
pub fn referenced_elsewhere(vault_root: &Path, name: &str, exclude: &[&Path]) -> bool {
    let mut notes = Vec::new();
    walk_markdown(vault_root, vault_root, 0, &mut notes);
    for path in notes {
        if is_excluded_note(&path, exclude) {
            continue;
        }
        if note_references_attachment(&path, name) {
            return true;
        }
    }
    false
}

static CUT_PROTECTED: OnceLock<Mutex<std::collections::HashSet<(PathBuf, String)>>> =
    OnceLock::new();

fn cut_protected_cell() -> &'static Mutex<std::collections::HashSet<(PathBuf, String)>> {
    CUT_PROTECTED.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

/// A Cut copied the reference away: keep the pair until it is pasted into
/// another note (then it is moved), instead of trashing it on the next save.
pub fn protect_cut(note_path: &Path, name: &str) {
    cut_protected_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert((note_path.to_path_buf(), name.to_string()));
}

pub fn release_cut(note_path: &Path, name: &str) {
    cut_protected_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&(note_path.to_path_buf(), name.to_string()));
}

fn is_cut_protected(note_path: &Path, name: &str) -> bool {
    cut_protected_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .contains(&(note_path.to_path_buf(), name.to_string()))
}

/// A save removed `name`: move the pair to the trash unless another note still
/// uses it or it came from a shared folder.
pub fn defer_removed_reference(
    vault_root: &Path,
    note_path: &Path,
    folder: &str,
    name: &str,
    exclude_notes: &[&Path],
) {
    let beside = attachments_dir(note_path, folder).join(name);
    if !beside.is_file() {
        return;
    }
    if is_cut_protected(note_path, name) {
        return;
    }
    if referenced_elsewhere(vault_root, name, exclude_notes) {
        return;
    }
    let _ = trash_pair(vault_root, folder, &beside);
}

/// Applies the attachment lifecycle after a successful save: references that
/// disappeared from the saved text go to the trash (unless another note still
/// uses them), and references that came back are restored from the trash.
pub fn reconcile_saved_references(
    vault_root: Option<&Path>,
    note_path: &Path,
    folder: &str,
    previous: &std::collections::HashSet<String>,
    current: &std::collections::HashSet<String>,
) {
    let Some(vault_root) = vault_root else {
        return;
    };
    let exclude = [note_path];
    for name in previous.difference(current) {
        defer_removed_reference(vault_root, note_path, folder, name, &exclude);
    }
    for name in current {
        restore_if_trashed(vault_root, note_path, folder, name);
    }
}

/// Moves the pairs referenced by a note from `old_note`'s attachments to
/// `new_note`'s attachments. Shared files stay where they are.
pub fn move_note_attachments(
    vault_root: &Path,
    old_note: &Path,
    new_note: &Path,
    folder: &str,
) -> usize {
    let mut names = std::collections::HashSet::new();
    references_in_file(new_note, &mut names);
    if names.is_empty() {
        return 0;
    }
    let old_dir = attachments_dir(old_note, folder);
    let new_dir = attachments_dir(new_note, folder);
    if old_dir == new_dir {
        return 0;
    }
    let mut moved = 0;
    for name in names {
        let source = old_dir.join(&name);
        if !source.is_file() {
            continue;
        }
        if referenced_elsewhere(vault_root, &name, &[old_note, new_note]) {
            continue;
        }
        if std::fs::create_dir_all(&new_dir).is_err() {
            continue;
        }
        let target = if new_dir.join(&name).exists() {
            continue;
        } else {
            new_dir.join(&name)
        };
        if std::fs::rename(&source, &target).is_ok() {
            let source_edit = edit_path(&source);
            if source_edit.is_file() {
                let _ = std::fs::rename(&source_edit, edit_path(&target));
            }
            moved += 1;
        }
    }
    if moved > 0 {
        invalidate_index();
    }
    moved
}

/// Sends every attachment referenced by a deleted note to the trash.
pub fn trash_note_attachments(
    vault_root: &Path,
    note_path: &Path,
    content: &str,
    folder: &str,
    exclude_notes: &[&Path],
) {
    for name in references_in_text(content) {
        defer_removed_reference(vault_root, note_path, folder, &name, exclude_notes);
    }
}

/// Attachment files in the vault that no note references.
pub fn unused_attachments(vault_root: &Path, folder: &str) -> Vec<PathBuf> {
    let mut notes = Vec::new();
    walk_markdown(vault_root, vault_root, 0, &mut notes);
    let mut used = std::collections::HashSet::new();
    for path in notes {
        references_in_file(&path, &mut used);
    }
    let mut files = Vec::new();
    let mut dirs = vec![vault_root.join(folder_name(folder))];
    scan_attachment_dirs(vault_root, vault_root, &folder_name(folder), 0, &mut dirs);
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".edit.json") {
                continue;
            }
            if is_image_path(&path) && !used.contains(&name) {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn scan_attachment_dirs(
    root: &Path,
    dir: &Path,
    folder: &str,
    depth: usize,
    out: &mut Vec<PathBuf>,
) {
    if depth > 16 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "target" || name == "node_modules" || name == "vendor" {
            continue;
        }
        if name == folder {
            out.push(path.clone());
        }
        scan_attachment_dirs(root, &path, folder, depth + 1, out);
    }
}

/// Removes attachment trash files older than `days`.
pub fn clean_trash(vault_root: &Path, folder: &str, days: u32) -> usize {
    let dir = trash_dir(vault_root, folder);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return 0;
    };
    let limit = std::time::Duration::from_secs(days.max(1) as u64 * 24 * 60 * 60);
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|time| time.elapsed().ok())
            .is_some_and(|age| age > limit);
        if stale && std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Moves a pair to another note's attachments when no other note references
/// it. Returns the name in the target folder.
pub fn move_attachment(
    vault_root: Option<&Path>,
    from_note: &Path,
    to_note: &Path,
    folder: &str,
    name: &str,
) -> Option<String> {
    let folder = folder_name(folder);
    let source = attachments_dir(from_note, &folder).join(name);
    if !source.is_file() {
        return None;
    }
    let target_dir = attachments_dir(to_note, &folder);
    // Both notes keep attachments in the same folder: nothing to move and no
    // need to scan the whole vault for other references.
    if source.parent() == Some(target_dir.as_path()) {
        return Some(name.to_string());
    }
    let root = vault_root?;
    if referenced_elsewhere(root, name, &[from_note, to_note]) {
        return None;
    }
    std::fs::create_dir_all(&target_dir).ok()?;
    let target = if target_dir.join(name).exists() {
        unique_path(&target_dir, name)
    } else {
        target_dir.join(name)
    };
    std::fs::rename(&source, &target).ok()?;
    let source_edit = edit_path(&source);
    if source_edit.is_file() {
        let _ = std::fs::rename(&source_edit, edit_path(&target));
    }
    invalidate_index();
    Some(target.file_name()?.to_string_lossy().into_owned())
}

/// Deletes every file in the attachment trash.
pub fn clear_trash(vault_root: &Path, folder: &str) -> usize {
    let dir = trash_dir(vault_root, folder);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        if std::fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}
