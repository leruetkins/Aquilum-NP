use typst::foundations::Bytes;
use typst::syntax::{FileId, RootedPath, VirtualPath, VirtualRoot};
use typst_layout::PagedDocument;
use super::typst::{markdown_to_typst, markdown_to_typst_with_images, PdfImage};
use super::world::MemoryWorld;

/// Compile a raw Typst document string into PDF bytes.
pub fn compile_typst_to_pdf(typst_source: &str) -> Result<Vec<u8>, String> {
    compile_typst_to_pdf_with_files(typst_source, &[])
}

fn compile_typst_to_pdf_with_files(
    typst_source: &str,
    files: &[(String, Vec<u8>)],
) -> Result<Vec<u8>, String> {
    let mut world = MemoryWorld::new(typst_source.to_string());
    for (path, bytes) in files {
        if let Ok(vpath) = VirtualPath::new(path) {
            let id = FileId::new(RootedPath::new(VirtualRoot::Project, vpath));
            world.files.insert(id, Bytes::new(bytes.clone()));
        }
    }
    let warned = typst::compile::<PagedDocument>(&world);
    let document = warned.output.map_err(|diagnostics| {
        let mut msg = String::new();
        for diag in diagnostics {
            msg.push_str(&format!("{:?}: {}\n", diag.severity, diag.message));
        }
        msg
    })?;

    let pdf_options = typst_pdf::PdfOptions::default();
    typst_pdf::pdf(&document, &pdf_options).map_err(|errs| {
        let mut msg = String::new();
        for diag in errs {
            msg.push_str(&format!("{:?}: {}\n", diag.severity, diag.message));
        }
        msg
    })
}

/// High-level function: converts Markdown to PDF bytes with optional Heading styles.
pub fn export_markdown_to_pdf(
    title: &str,
    markdown: &str,
    headings: Option<&crate::settings::HeadingsSettings>,
) -> Result<Vec<u8>, String> {
    let typst_markup = markdown_to_typst(title, markdown, headings);
    match compile_typst_to_pdf(&typst_markup) {
        Ok(bytes) => {
            println!("[Typst Engine] Successfully compiled PDF ({} bytes)", bytes.len());
            Ok(bytes)
        }
        Err(err) => {
            eprintln!("[Typst Engine] Compilation ERROR:\n{}", err);
            eprintln!("[Typst Engine] Generated Typst markup was:\n{}", typst_markup);
            Err(err)
        }
    }
}

/// Collects every image reference in the note, bakes crop/annotations into
/// PNG copies, and formats captions with the same numbering as the editor.
pub fn build_pdf_images(
    note_path: &std::path::Path,
    content: &str,
    vault_root: Option<&std::path::Path>,
    folder: &str,
    caption_template: &str,
    default_radius: f32,
) -> Vec<PdfImage> {
    let mut images: Vec<PdfImage> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut number = 0u32;
    for line in content.lines() {
        let Some(reference) = crate::images::parse_image_reference(line.trim()) else {
            continue;
        };
        if !seen.insert(reference.target.clone()) {
            continue;
        }
        let Some(path) = crate::images::resolve_attachment(
            vault_root,
            note_path,
            folder,
            &reference.target,
        ) else {
            continue;
        };
        if !crate::images::is_image_path(&path) {
            continue;
        }
        let edit = crate::images::load_edit(&path);
        let caption = edit.caption.clone().filter(|caption| !caption.text.trim().is_empty());
        let formatted_caption = caption.as_ref().map(|caption| {
            if caption.numbered {
                number += 1;
                crate::images::format_caption(
                    caption_template,
                    Some(number),
                    &caption.text,
                    true,
                )
            } else {
                crate::images::format_caption(caption_template, None, &caption.text, false)
            }
        });
        let position = caption
            .as_ref()
            .map(|caption| {
                if caption.position.is_empty() {
                    "below".to_string()
                } else {
                    caption.position.clone()
                }
            })
            .unwrap_or_else(|| "below".to_string());
        let radius = edit
            .corner_radius
            .unwrap_or(default_radius)
            .clamp(0.0, 50.0);
        let needs_bake = edit.crop.is_some() || !edit.annotations.is_empty();
        let (archive_name, bytes) = if needs_bake {
            let options = crate::image_render::BakeOptions {
                apply_crop: true,
                apply_corner_radius: false,
                caption: None,
                caption_background: None,
            };
            let baked = match crate::image_render::bake_rgba(&path, &edit, &options) {
                Ok(baked) => baked,
                Err(_) => continue,
            };
            let stem = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            let name = format!("{stem}.png");
            match crate::image_render::encode_rgba(
                &baked,
                crate::image_render::EncodedFormat::Png,
                92,
            ) {
                Ok(bytes) => (name, bytes),
                Err(_) => continue,
            }
        } else {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| reference.target.clone());
            match std::fs::read(&path) {
                Ok(bytes) => (name, bytes),
                Err(_) => continue,
            }
        };
        images.push(PdfImage {
            target: reference.target,
            path: format!("attachments/{archive_name}"),
            bytes,
            width_px: reference.width,
            height_px: reference.height,
            radius_pct: (radius > 0.5).then_some(radius),
            caption: formatted_caption,
            position,
        });
    }
    images
}

pub fn export_markdown_to_pdf_with_images(
    title: &str,
    markdown: &str,
    headings: Option<&crate::settings::HeadingsSettings>,
    images: Vec<PdfImage>,
) -> Result<Vec<u8>, String> {
    let markup = markdown_to_typst_with_images(title, markdown, headings, &images);
    let files: Vec<(String, Vec<u8>)> = images
        .iter()
        .map(|image| (image.path.clone(), image.bytes.clone()))
        .collect();
    compile_typst_to_pdf_with_files(&markup, &files)
}
