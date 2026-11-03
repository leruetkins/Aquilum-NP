use std::io::Cursor;
use std::path::{Path, PathBuf};
use super::resolve::{invalidate_index, remember_dimensions};

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

pub(crate) fn folder_name(folder: &str) -> String {
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
pub(crate) fn write_new(dir: &Path, ext: &str, bytes: &[u8]) -> Result<(String, PathBuf), ImportError> {
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

pub(crate) fn new_name(ext: &str) -> String {
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
