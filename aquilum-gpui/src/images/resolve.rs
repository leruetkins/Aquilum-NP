use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use super::edit_cache;
use super::import::{folder_name, IMAGE_EXTENSIONS};
use super::{invalidate_ref_index, invalidate_resolved_images};


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
    invalidate_ref_index();
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

pub(crate) fn remember_dimensions(path: &Path) {
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
