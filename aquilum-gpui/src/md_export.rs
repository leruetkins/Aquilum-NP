//! Markdown export to a zip: the note plus only the attachments it uses.
//! Image copies are baked (crop/annotations/radius/caption) so the archive
//! opens in any Markdown editor without the vault.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::image_render::BakeOptions;
use crate::images::ImageEdit;
use crate::images;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptionFormat {
    Italic,
    Html,
    Alt,
}

#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub caption_format: CaptionFormat,
    pub embed_caption: bool,
    pub round_corners: bool,
    /// Leave `![[...]]` untouched.
    pub keep_obsidian: bool,
    pub caption_template: String,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            caption_format: CaptionFormat::Italic,
            embed_caption: false,
            round_corners: true,
            keep_obsidian: false,
            caption_template: "Рис. {n}. {text}".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExportReport {
    pub path: PathBuf,
    pub included: usize,
    pub missing: usize,
}

fn percent_encode(path: &str) -> String {
    let mut out = String::new();
    for byte in path.as_bytes() {
        let ch = *byte as char;
        let keep = ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~' | '/');
        if keep {
            out.push(ch);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn image_line_reference(line: &str) -> Option<(usize, usize, String)> {
    // Returns (start, end, inner) for `![[...]]` with nothing but whitespace around it.
    let trimmed = line.trim();
    let start = line.find(trimmed).unwrap_or(0);
    let inner = trimmed.strip_prefix("![[")?.strip_suffix("]]")?;
    Some((start, start + trimmed.len(), inner.to_string()))
}

fn split_inner(inner: &str) -> (String, Option<String>) {
    let mut parts = inner.split('|');
    let target = parts.next().unwrap_or("").trim().to_string();
    let spec = parts.next().map(|part| part.trim().to_string());
    (target, spec)
}

fn size_spec(spec: &Option<String>) -> Option<(Option<u32>, Option<u32>)> {
    let spec = spec.as_deref()?;
    if let Some((w, h)) = spec.split_once('x').or_else(|| spec.split_once('X')) {
        let w = w.trim().parse::<u32>().ok();
        let h = h.trim().parse::<u32>().ok();
        if w.is_some() || h.is_some() {
            return Some((w, h));
        }
    } else if let Ok(w) = spec.trim().parse::<u32>() {
        return Some((Some(w), None));
    }
    None
}

fn caption_markup(
    format: CaptionFormat,
    position: &str,
    image_html: &str,
    text: &str,
) -> String {
    match format {
        CaptionFormat::Italic => {
            let caption = format!("*{text}*");
            if position == "above" {
                format!("{caption}\n\n{image_html}")
            } else {
                format!("{image_html}\n\n{caption}")
            }
        }
        CaptionFormat::Html => {
            if position == "above" {
                format!("<figure>\n<figcaption>{text}</figcaption>\n{image_html}\n</figure>")
            } else {
                format!("<figure>\n{image_html}\n<figcaption>{text}</figcaption>\n</figure>")
            }
        }
        CaptionFormat::Alt => image_html.to_string(),
    }
}

pub fn build_zip(
    note_path: &Path,
    vault_root: Option<&Path>,
    folder: &str,
    options: &ExportOptions,
    destination: &Path,
) -> Result<ExportReport, String> {
    let content = std::fs::read_to_string(note_path).map_err(|err| err.to_string())?;
    let note_stem = note_path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "note".to_string());
    let note_file_name = format!("{note_stem}.md");

    // Resolve every unique attachment first so the archive has one copy per
    // name and the text rewrite uses final archive names.
    let mut archive_names: HashMap<String, String> = HashMap::new();
    let mut files: HashMap<String, Vec<u8>> = HashMap::new();
    let mut missing = 0usize;

    let mut lines: Vec<String> = content.lines().map(|line| line.to_string()).collect();
    let mut figure_number = 0u32;

    for line in lines.iter_mut() {
        if options.keep_obsidian {
            continue;
        }
        let Some((start, end, inner)) = image_line_reference(line) else {
            continue;
        };
        let (target, spec) = split_inner(&inner);
        let Some(path) = images::resolve_attachment(vault_root, note_path, folder, &target) else {
            missing += 1;
            continue;
        };
        if !images::is_image_path(&path) {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(target.clone());
            if !files.contains_key(&name) {
                let bytes = std::fs::read(&path).map_err(|err| err.to_string())?;
                files.insert(name.clone(), bytes);
            }
            let href = format!("attachments/{}", percent_encode(&name));
            *line = line.replacen(&format!("![[{}]]", inner), &format!("[{}]({})", name, href), 1);
            continue;
        }

        let edit = images::load_edit(&path);
        let caption = edit.caption.clone().filter(|caption| !caption.text.trim().is_empty());
        let number = if caption.as_ref().is_some_and(|caption| caption.numbered) {
            figure_number += 1;
            Some(figure_number)
        } else {
            None
        };
        let caption_text = caption.as_ref().map(|caption| {
            images::format_caption(
                &options.caption_template,
                number,
                &caption.text,
                caption.numbered,
            )
        });

        let has_edit = edit.crop.is_some()
            || !edit.annotations.is_empty()
            || edit.corner_radius.is_some()
            || (options.embed_caption && caption_text.is_some());
        let original_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();

        let archive_name = if has_edit {
            let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            format!("{stem}.png")
        } else {
            original_name.clone()
        };
        archive_names.entry(target.clone()).or_insert_with(|| archive_name.clone());

        if !files.contains_key(&archive_name) {
            if has_edit {
                let bake_edit = ImageEdit {
                    crop: edit.crop,
                    annotations: edit.annotations.clone(),
                    corner_radius: edit.corner_radius,
                    caption: None,
                    source_name: edit.source_name.clone(),
                    version: edit.version,
                    extra: edit.extra.clone(),
                };
                let options_bake = BakeOptions {
                    apply_crop: true,
                    apply_corner_radius: options.round_corners,
                    caption: if options.embed_caption {
                        caption_text.clone().map(|text| {
                            let position = caption
                                .as_ref()
                                .map(|caption| caption.position.clone())
                                .unwrap_or_else(|| "below".to_string());
                            (text, position)
                        })
                    } else {
                        None
                    },
                    caption_background: None,
                };
                let baked = crate::image_render::bake_rgba(&path, &bake_edit, &options_bake)?;
                let bytes = crate::image_render::encode_rgba(
                    &baked,
                    crate::image_render::EncodedFormat::Png,
                    92,
                )?;
                files.insert(archive_name.clone(), bytes);
            } else {
                let bytes = std::fs::read(&path).map_err(|err| err.to_string())?;
                files.insert(archive_name.clone(), bytes);
            }
        }

        let href = format!("attachments/{}", percent_encode(&archive_name));
        let size = size_spec(&spec);
        let alt = caption_text.clone().unwrap_or_default();
        let image_html = if let Some((width, height)) = size {
            let mut attrs = String::new();
            if let Some(width) = width {
                attrs.push_str(&format!(" width=\"{width}\""));
            }
            if let Some(height) = height {
                attrs.push_str(&format!(" height=\"{height}\""));
            }
            format!("<img src=\"{href}\" alt=\"{alt}\"{attrs}>")
        } else {
            format!("![{alt}]({href})")
        };

        let replacement = if let Some(text) = caption_text.as_ref().filter(|_| !options.embed_caption) {
            let position = caption
                .as_ref()
                .map(|caption| caption.position.clone())
                .unwrap_or_else(|| "below".to_string());
            match options.caption_format {
                CaptionFormat::Alt => image_html.clone(),
                format => caption_markup(format, &position, &image_html, text),
            }
        } else {
            image_html
        };
        *line = format!("{}{}{}", &line[..start], replacement, &line[end..]);
    }

    let rewritten = lines.join("\n");
    let mut zip_bytes = Vec::new();
    {
        use zip::write::SimpleFileOptions;
        let cursor = std::io::Cursor::new(&mut zip_bytes);
        let mut writer = zip::ZipWriter::new(cursor);
        let deflated = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        writer
            .start_file(&note_file_name, deflated)
            .map_err(|err| err.to_string())?;
        writer
            .write_all(rewritten.as_bytes())
            .map_err(|err| err.to_string())?;
        for (name, bytes) in files {
            writer
                .start_file(format!("attachments/{name}"), stored)
                .map_err(|err| err.to_string())?;
            writer.write_all(&bytes).map_err(|err| err.to_string())?;
        }
        writer.finish().map_err(|err| err.to_string())?;
    }
    std::fs::write(destination, &zip_bytes).map_err(|err| err.to_string())?;
    Ok(ExportReport {
        path: destination.to_path_buf(),
        included: archive_names.len(),
        missing,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_contains_baked_images_and_standard_markdown() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path();
        let attachments = vault.join("attachments");
        std::fs::create_dir_all(&attachments).unwrap();
        let note = vault.join("Note.md");
        std::fs::write(&note, "# Заметка

![[a.png]]
").unwrap();
        let image = attachments.join("a.png");
        image::RgbaImage::from_pixel(40, 20, image::Rgba([10, 20, 30, 255]))
            .save(&image)
            .unwrap();
        std::fs::write(
            images::edit_path(&image),
            r#"{"version":1,"crop":{"x":10,"y":5,"w":20,"h":10}}"#,
        )
        .unwrap();
        let zip_path = vault.join("out.zip");
        let report = build_zip(
            &note,
            Some(vault),
            "attachments",
            &ExportOptions::default(),
            &zip_path,
        )
        .unwrap();
        assert_eq!(report.included, 1);
        let file = std::fs::File::open(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let mut md = String::new();
        std::io::Read::read_to_string(&mut archive.by_name("Note.md").unwrap(), &mut md).unwrap();
        assert!(md.contains("!["));
        assert!(md.contains("attachments/a.png"));
        assert!(!md.contains("![["));
        let mut baked = Vec::new();
        std::io::Read::read_to_end(
            &mut archive.by_name("attachments/a.png").unwrap(),
            &mut baked,
        )
        .unwrap();
        let decoded = image::load_from_memory(&baked).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (20, 10));
    }

    #[test]
    fn percent_encoding_keeps_the_folder_slash() {
        assert_eq!(percent_encode("attachments/пример файл.png"), "attachments/%D0%BF%D1%80%D0%B8%D0%BC%D0%B5%D1%80%20%D1%84%D0%B0%D0%B9%D0%BB.png");
    }
}
