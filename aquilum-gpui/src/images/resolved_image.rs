use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use super::annotate::{caption_height, visible_crop};
use super::config::{folder, vault_root};
use super::edit::{edit_path, load_edit, radius_px, CropRect, ImageEdit};
use super::import::{attachments_dir, write_new};
use super::reference::{parse_image_reference, ImageReference};
use super::resolve::{image_size, resolve_attachment, text_width};

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
