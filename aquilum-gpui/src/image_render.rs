//! Pixel work for image attachments: display copies, text/vector
//! rasterization (tiny-skia + cosmic-text), baking edits into exported copies,
//! destructive crop and compression.
//!
//! The vector geometry itself lives in `images.rs`; this module only turns it
//! into pixels, so the editor and the exported file draw the same shapes.

use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use cosmic_text::{Attrs, Buffer, Color as TextColor, Family, FontSystem, Metrics, Shaping, SwashCache};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

use crate::images::{self, Annotation, ImageEdit, Point, Shape};

// ───────────────────────────── Decoded originals ─────────────────────────────

fn decoded() -> &'static Mutex<HashMap<PathBuf, Arc<image::RgbaImage>>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<image::RgbaImage>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn invalidate_decoded() {
    if let Ok(mut cache) = decoded().lock() {
        cache.clear();
    }
}

/// Drops every pixel cache after an external file/sidecar change.
pub fn invalidate_caches() {
    invalidate_decoded();
    crate::images::invalidate_dimensions();
    if let Ok(mut cache) = display_cache().lock() {
        cache.clear();
    }
    if let Ok(mut cache) = text_cache().lock() {
        cache.clear();
    }
}

pub fn load_rgba(path: &Path) -> Option<Arc<image::RgbaImage>> {
    if let Ok(cache) = decoded().lock() {
        if let Some(image) = cache.get(path) {
            return Some(Arc::clone(image));
        }
    }
    let image = Arc::new(image::open(path).ok()?.to_rgba8());
    if let Ok(mut cache) = decoded().lock() {
        cache.insert(path.to_path_buf(), Arc::clone(&image));
    }
    Some(image)
}

// ───────────────────────────── Display copies ─────────────────────────────

type DisplayKey = (PathBuf, u32, u32, u32, u32);

fn display_cache() -> &'static Mutex<HashMap<DisplayKey, Arc<gpui::Image>>> {
    static CACHE: OnceLock<Mutex<HashMap<DisplayKey, Arc<gpui::Image>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A cropped copy for the editor. GPUI can only paint a whole image quad, so a
/// non-destructive crop is materialised here and cached; the original file is
/// never touched.
pub fn cropped_display(path: &Path, crop: images::CropRect) -> Option<Arc<gpui::Image>> {
    let rgba = load_rgba(path)?;
    let crop = crop.clamp_to((rgba.width(), rgba.height()));
    let key = (
        path.to_path_buf(),
        crop.x as u32,
        crop.y as u32,
        crop.w as u32,
        crop.h as u32,
    );
    if let Ok(cache) = display_cache().lock() {
        if let Some(image) = cache.get(&key) {
            return Some(Arc::clone(image));
        }
    }
    let sub = image::imageops::crop_imm(
        &*rgba,
        crop.x as u32,
        crop.y as u32,
        crop.w.max(1.0) as u32,
        crop.h.max(1.0) as u32,
    )
    .to_image();
    let png = encode_rgba(&sub, EncodedFormat::Png, 92).ok()?;
    let image = Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Png, png));
    if let Ok(mut cache) = display_cache().lock() {
        cache.insert(key, Arc::clone(&image));
    }
    Some(image)
}

// ───────────────────────────── Text ─────────────────────────────

fn font_system() -> &'static Mutex<FontSystem> {
    static SYSTEM: OnceLock<Mutex<FontSystem>> = OnceLock::new();
    SYSTEM.get_or_init(|| {
        let mut system = FontSystem::new();
        system
            .db_mut()
            .load_font_data(crate::fonts::ROBOTO_MONO_REGULAR.to_vec());
        Mutex::new(system)
    })
}

fn blend_rgba(dst: &mut [u8], width: u32, height: u32, x: i32, y: i32, src: [u8; 4]) {
    if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 || src[3] == 0 {
        return;
    }
    let index = ((y as u32 * width + x as u32) * 4) as usize;
    let sa = src[3] as f32 / 255.0;
    let da = dst[index + 3] as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= f32::EPSILON {
        return;
    }
    for channel in 0..3 {
        dst[index + channel] = ((src[channel] as f32 * sa
            + dst[index + channel] as f32 * da * (1.0 - sa))
            / out_a)
            .round() as u8;
    }
    dst[index + 3] = (out_a * 255.0).round() as u8;
}

/// Lays out and rasterises plain text with the bundled monospace font. One
/// mechanism is used for the screen and for exports, so Cyrillic and wrapping
/// look the same everywhere.
pub fn rasterize_text(text: &str, font_size: f32, color: [u8; 4], max_width: f32) -> Option<(u32, u32, Vec<u8>)> {
    if text.is_empty() || font_size <= 0.0 {
        return None;
    }
    let mut system = font_system().lock().ok()?;
    let metrics = Metrics::new(font_size, (font_size * 1.3).max(font_size + 2.0));
    let mut buffer = Buffer::new(&mut system, metrics);
    buffer.set_size(&mut system, Some(max_width.max(font_size).max(8.0)), None);
    let attrs = Attrs::new()
        .family(Family::Name("Roboto Mono"))
        .color(TextColor::rgba(color[0], color[1], color[2], color[3]));
    buffer.set_text(&mut system, text, &attrs, Shaping::Advanced);
    buffer.shape_until_scroll(&mut system, false);

    let mut width = 1.0_f32;
    let mut height = 1.0_f32;
    for run in buffer.layout_runs() {
        width = width.max(run.line_w);
        height = height.max(run.line_top + run.line_height);
    }
    let w = (width.ceil() as u32 + 2).max(1);
    let h = (height.ceil() as u32 + 2).max(1);
    let mut canvas = vec![0u8; (w as usize) * (h as usize) * 4];
    let mut cache = SwashCache::new();

    for run in buffer.layout_runs() {
        for glyph in run.glyphs {
            let physical = glyph.physical((0.0, 0.0), 1.0);
            let Some(image) = cache.get_image(&mut system, physical.cache_key) else {
                continue;
            };
            let origin_x = physical.x as i32 + image.placement.left;
            let origin_y = physical.y as i32 + image.placement.top;
            match image.content {
                cosmic_text::SwashContent::Mask | cosmic_text::SwashContent::SubpixelMask => {
                    let channels = if matches!(image.content, cosmic_text::SwashContent::Mask) { 1 } else { 4 };
                    for row in 0..image.placement.height as i32 {
                        for col in 0..image.placement.width as i32 {
                            let index = ((row * image.placement.width as i32 + col) * channels) as usize;
                            let coverage = image.data.get(index).copied().unwrap_or(0);
                            if coverage == 0 {
                                continue;
                            }
                            let alpha = (color[3] as u16 * coverage as u16 / 255) as u8;
                            blend_rgba(
                                &mut canvas,
                                w,
                                h,
                                origin_x + col,
                                origin_y + row,
                                [color[0], color[1], color[2], alpha],
                            );
                        }
                    }
                }
                cosmic_text::SwashContent::Color => {
                    for row in 0..image.placement.height as i32 {
                        for col in 0..image.placement.width as i32 {
                            let index = ((row * image.placement.width as i32 + col) * 4) as usize;
                            let Some(pixel) = image.data.get(index..index + 4) else {
                                continue;
                            };
                            blend_rgba(
                                &mut canvas,
                                w,
                                h,
                                origin_x + col,
                                origin_y + row,
                                [pixel[0], pixel[1], pixel[2], pixel[3]],
                            );
                        }
                    }
                }
            }
        }
    }
    Some((w, h, canvas))
}

/// Text annotation image reused by the painter. Cached by content so a static
/// note does not re-shape text every frame.
type TextKey = (String, u32, [u8; 4], u32);

fn text_cache() -> &'static Mutex<HashMap<TextKey, (Arc<gpui::Image>, u32, u32)>> {
    static CACHE: OnceLock<Mutex<HashMap<TextKey, (Arc<gpui::Image>, u32, u32)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn gpui_text_image(
    text: &str,
    font_size: f32,
    color: [u8; 4],
    max_width: f32,
) -> Option<(Arc<gpui::Image>, u32, u32)> {
    let cache = text_cache;
    let key = (
        text.to_string(),
        (font_size * 10.0).round() as u32,
        color,
        max_width.round() as u32,
    );
    if let Ok(map) = cache().lock() {
        if let Some(hit) = map.get(&key) {
            return Some(hit.clone());
        }
    }
    let (w, h, rgba) = rasterize_text(text, font_size, color, max_width)?;
    let png = encode_rgba(
        &image::RgbaImage::from_raw(w, h, rgba)?,
        EncodedFormat::Png,
        92,
    )
    .ok()?;
    let image = Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Png, png));
    let value = (image, w, h);
    if let Ok(mut map) = cache().lock() {
        map.insert(key, value.clone());
    }
    Some(value)
}

// ───────────────────────────── Shapes ─────────────────────────────

fn shape_bounds(shapes: &[Shape], width: f32) -> Option<(f32, f32, f32, f32)> {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for shape in shapes {
        for p in &shape.points {
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }
    }
    if !min_x.is_finite() {
        return None;
    }
    let pad = width + 4.0;
    Some((min_x - pad, min_y - pad, max_x + pad, max_y + pad))
}

fn composite_pixmap(
    dst: &mut [u8],
    width: u32,
    height: u32,
    pixmap: &Pixmap,
    origin_x: i32,
    origin_y: i32,
) {
    for (index, pixel) in pixmap.pixels().iter().enumerate() {
        let color = pixel.demultiply();
        if color.alpha() == 0 {
            continue;
        }
        let x = (index % pixmap.width() as usize) as i32 + origin_x;
        let y = (index / pixmap.width() as usize) as i32 + origin_y;
        blend_rgba(
            dst,
            width,
            height,
            x,
            y,
            [color.red(), color.green(), color.blue(), color.alpha()],
        );
    }
}

fn draw_annotations(
    canvas: &mut [u8],
    width: u32,
    height: u32,
    annotations: &[Annotation],
    offset: (f32, f32),
) {
    let mut labels = 0usize;
    for annotation in annotations {
        let mut color = images::parse_hex_color(&annotation.color);
        let mut stroke_width = annotation.width.max(0.5);
        if matches!(annotation.kind, images::AnnotationKind::Highlight) {
            color[3] = color[3].min(140);
            stroke_width *= 3.0;
        }
        let shapes = images::annotation_geometry(annotation);
        for shape in &shapes {
            let Some((min_x, min_y, max_x, max_y)) = shape_bounds(std::slice::from_ref(shape), stroke_width) else {
                continue;
            };
            let w = (max_x - min_x).ceil().max(1.0) as u32;
            let h = (max_y - min_y).ceil().max(1.0) as u32;
            let Some(mut pixmap) = Pixmap::new(w, h) else {
                continue;
            };
            let mut builder = PathBuilder::new();
            let local = |p: &Point| ((p.x + offset.0 - min_x).round(), (p.y + offset.1 - min_y).round());
            if let Some(first) = shape.points.first() {
                let (x, y) = local(first);
                builder.move_to(x as f32, y as f32);
                for p in shape.points.iter().skip(1) {
                    let (x, y) = local(p);
                    builder.line_to(x as f32, y as f32);
                }
                if shape.closed {
                    builder.close();
                }
            }
            let Some(path) = builder.finish() else {
                continue;
            };
            let mut paint = Paint::default();
            paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
            paint.anti_alias = true;
            if shape.filled {
                pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
            } else {
                let mut stroke = Stroke::default();
                stroke.width = stroke_width;
                stroke.line_cap = tiny_skia::LineCap::Round;
                stroke.line_join = tiny_skia::LineJoin::Round;
                pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
            }
            composite_pixmap(
                canvas,
                width,
                height,
                &pixmap,
                min_x.floor() as i32,
                min_y.floor() as i32,
            );
        }

        if annotation.is_textual() {
            let point = annotation.points.first().copied().unwrap_or_default();
            let mut text = annotation.text.clone().unwrap_or_default();
            if matches!(annotation.kind, images::AnnotationKind::Label) {
                labels += 1;
                if text.trim().is_empty() {
                    text = labels.to_string();
                }
            }
            if text.is_empty() {
                continue;
            }
            let text_color = if matches!(annotation.kind, images::AnnotationKind::Label) {
                [255, 255, 255, 255]
            } else {
                color
            };
            let max_width = (width as f32 - point.x - offset.0).max(annotation.font_size);
            if let Some((tw, th, rgba)) =
                rasterize_text(&text, annotation.font_size, text_color, max_width)
            {
                let mut x = point.x + offset.0;
                let mut y = point.y + offset.1;
                if matches!(annotation.kind, images::AnnotationKind::Label) {
                    x -= tw as f32 / 2.0;
                    y -= th as f32 / 2.0;
                }
                for row in 0..th {
                    for col in 0..tw {
                        let index = ((row * tw + col) * 4) as usize;
                        let Some(pixel) = rgba.get(index..index + 4) else {
                            continue;
                        };
                        blend_rgba(
                            canvas,
                            width,
                            height,
                            x.round() as i32 + col as i32,
                            y.round() as i32 + row as i32,
                            [pixel[0], pixel[1], pixel[2], pixel[3]],
                        );
                    }
                }
            }
        }
    }
}

// ───────────────────────────── Rounded corners ─────────────────────────────

fn apply_round_mask(rgba: &mut image::RgbaImage, radius: f32) {
    let (w, h) = rgba.dimensions();
    let radius = radius.clamp(0.0, (w.min(h) as f32) / 2.0);
    if radius <= 0.5 {
        return;
    }
    let Some(mut mask) = Pixmap::new(w, h) else {
        return;
    };
    let mut builder = PathBuilder::new();
    let r = radius;
    builder.move_to(r, 0.0);
    builder.line_to(w as f32 - r, 0.0);
    builder.quad_to(w as f32, 0.0, w as f32, r);
    builder.line_to(w as f32, h as f32 - r);
    builder.quad_to(w as f32, h as f32, w as f32 - r, h as f32);
    builder.line_to(r, h as f32);
    builder.quad_to(0.0, h as f32, 0.0, h as f32 - r);
    builder.line_to(0.0, r);
    builder.quad_to(0.0, 0.0, r, 0.0);
    builder.close();
    let Some(path) = builder.finish() else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    paint.anti_alias = true;
    mask.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    let bytes: &mut [u8] = rgba.as_mut();
    for (index, pixel) in mask.pixels().iter().enumerate() {
        let coverage = pixel.alpha();
        if coverage == 255 {
            continue;
        }
        let offset = index * 4 + 3;
        if let Some(alpha) = bytes.get_mut(offset) {
            *alpha = ((*alpha as u16 * coverage as u16) / 255) as u8;
        }
    }
}

// ───────────────────────────── Encoding ─────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodedFormat {
    Png,
    Jpeg,
    WebP,
}

pub fn encode_rgba(
    rgba: &image::RgbaImage,
    format: EncodedFormat,
    quality: u8,
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    match format {
        EncodedFormat::Png => {
            rgba.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
                .map_err(|err| err.to_string())?;
        }
        EncodedFormat::WebP => {
            rgba.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::WebP)
                .map_err(|err| err.to_string())?;
        }
        EncodedFormat::Jpeg => {
            let rgb = image::DynamicImage::ImageRgba8(rgba.clone()).to_rgb8();
            let mut cursor = Cursor::new(&mut bytes);
            let mut encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, quality.clamp(1, 100));
            encoder.encode(
                rgb.as_raw(),
                rgb.width(),
                rgb.height(),
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|err| err.to_string())?;
        }
    }
    Ok(bytes)
}

pub fn optimize_png(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let options = oxipng::Options::from_preset(4);
    oxipng::optimize_from_memory(bytes, &options).map_err(|err| err.to_string())
}

fn save_like(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_extension("tmp-save");
    std::fs::write(&temp, bytes).map_err(|err| err.to_string())?;
    std::fs::rename(&temp, path).map_err(|err| {
        let _ = std::fs::remove_file(&temp);
        err.to_string()
    })
}

fn save_rgba_like(path: &Path, rgba: &image::RgbaImage) -> Result<(), String> {
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_default();
    let format = match ext.as_str() {
        "jpg" | "jpeg" => EncodedFormat::Jpeg,
        "webp" => EncodedFormat::WebP,
        _ => EncodedFormat::Png,
    };
    let bytes = encode_rgba(rgba, format, 92)?;
    save_like(path, &bytes)
}

// ───────────────────────────── Bake / flatten ─────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct BakeOptions {
    /// Destructive crop applied in the copy. Export and "save as" use this;
    /// flatten keeps the original crop and writes annotations only.
    pub apply_crop: bool,
    pub apply_corner_radius: bool,
    /// Text and position ("above"/"below") drawn into the extra strip.
    pub caption: Option<(String, String)>,
    /// Strip background; `None` keeps it transparent.
    pub caption_background: Option<[u8; 4]>,
}

/// Applies crop, annotations and the corner radius to a copy of the picture.
pub fn bake_rgba(
    path: &Path,
    edit: &ImageEdit,
    options: &BakeOptions,
) -> Result<image::RgbaImage, String> {
    let original = load_rgba(path).ok_or_else(|| "cannot read image".to_string())?;
    let natural = (original.width(), original.height());
    let crop = if options.apply_crop {
        images::visible_crop(edit, natural)
    } else {
        None
    };
    let (offset_x, offset_y, width, height) = crop.map_or((0.0, 0.0, natural.0, natural.1), |crop| {
        (
            crop.x,
            crop.y,
            crop.w.round().max(1.0) as u32,
            crop.h.round().max(1.0) as u32,
        )
    });
    let mut out = image::RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let sx = (x as f32 + offset_x).round() as u32;
            let sy = (y as f32 + offset_y).round() as u32;
            if sx < natural.0 && sy < natural.1 {
                out.put_pixel(x, y, *original.get_pixel(sx, sy));
            }
        }
    }
    {
        let width = out.width();
        let height = out.height();
        let mut canvas: Vec<u8> = out.as_raw().clone();
        draw_annotations(
            &mut canvas,
            width,
            height,
            &edit.annotations,
            (-offset_x, -offset_y),
        );
        out = image::RgbaImage::from_raw(width, height, canvas)
            .ok_or_else(|| "bad annotation canvas".to_string())?;
    }
    if options.apply_corner_radius {
        let radius = edit.corner_radius.unwrap_or(0.0).clamp(0.0, 50.0) / 100.0
            * out.width().min(out.height()) as f32;
        apply_round_mask(&mut out, radius);
    }
    if let Some((text, position)) = options.caption.as_ref() {
        out = burn_caption(out, text, position, options.caption_background);
    }
    Ok(out)
}

fn burn_caption(
    image: image::RgbaImage,
    text: &str,
    position: &str,
    background: Option<[u8; 4]>,
) -> image::RgbaImage {
    let width = image.width();
    let font_size = (width as f32 / 30.0).clamp(12.0, 42.0);
    let Some((text_w, text_h, text_rgba)) =
        rasterize_text(text, font_size, [32, 32, 36, 255], (width as f32 - 24.0).max(40.0))
    else {
        return image;
    };
    let strip_h = text_h + (font_size * 0.6) as u32 + 8;
    let total_h = image.height() + strip_h;
    let mut out = image::RgbaImage::new(width, total_h);
    let above = position == "above";
    let y_offset = if above { strip_h } else { 0 };
    if above {
        for y in 0..image.height() {
            for x in 0..width {
                out.put_pixel(x, y + y_offset, *image.get_pixel(x, y));
            }
        }
    } else {
        for y in 0..image.height() {
            for x in 0..width {
                out.put_pixel(x, y, *image.get_pixel(x, y));
            }
        }
    }
    let bg = background.unwrap_or([0, 0, 0, 0]);
    if bg[3] > 0 {
        let strip_y = if above { 0 } else { image.height() };
        for y in strip_y..strip_y + strip_h {
            for x in 0..width {
                if x < width && y < total_h {
                    out.put_pixel(x, y, image::Rgba(bg));
                }
            }
        }
    }
    let text_x = (width as i32 - text_w as i32) / 2;
    let text_y = if above {
        ((strip_h as i32 - text_h as i32) / 2).max(0)
    } else {
        image.height() as i32 + ((strip_h as i32 - text_h as i32) / 2).max(0)
    };
    let mut canvas = out.as_raw().clone();
    for row in 0..text_h {
        for col in 0..text_w {
            let index = ((row * text_w + col) * 4) as usize;
            let Some(pixel) = text_rgba.get(index..index + 4) else {
                continue;
            };
            blend_rgba(
                &mut canvas,
                width,
                total_h,
                text_x + col as i32,
                text_y + row as i32,
                [pixel[0], pixel[1], pixel[2], pixel[3]],
            );
        }
    }
    image::RgbaImage::from_raw(width, total_h, canvas).unwrap_or_else(|| image::RgbaImage::new(width, total_h))
}

/// Writes annotations into the original file (crop and radius stay metadata).
pub fn flatten_file(path: &Path, edit: &mut ImageEdit) -> Result<(), String> {
    let original = load_rgba(path).ok_or_else(|| "cannot read image".to_string())?;
    let (w, h) = original.dimensions();
    let mut canvas = original.as_raw().clone();
    draw_annotations(&mut canvas, w, h, &edit.annotations, (0.0, 0.0));
    let rgba = image::RgbaImage::from_raw(w, h, canvas)
        .ok_or_else(|| "bad annotation canvas".to_string())?;
    save_rgba_like(path, &rgba)?;
    edit.annotations.clear();
    images::save_edit(path, edit).map_err(|err| err.to_string())?;
    invalidate_caches();
    Ok(())
}

// ───────────────────────────── Destructive crop ─────────────────────────────

pub fn apply_crop_file(path: &Path, edit: &mut ImageEdit) -> Result<(), String> {
    let original = load_rgba(path).ok_or_else(|| "cannot read image".to_string())?;
    let natural = (original.width(), original.height());
    let Some(crop) = images::visible_crop(edit, natural) else {
        edit.crop = None;
        return images::save_edit(path, edit).map_err(|err| err.to_string());
    };
    let sub = image::imageops::crop_imm(
        &*original,
        crop.x as u32,
        crop.y as u32,
        crop.w.max(1.0) as u32,
        crop.h.max(1.0) as u32,
    )
    .to_image();
    save_rgba_like(path, &sub)?;
    images::shift_annotations_after_crop(&mut edit.annotations, crop);
    edit.crop = None;
    images::save_edit(path, edit).map_err(|err| err.to_string())?;
    invalidate_caches();
    images::invalidate_dimensions();
    Ok(())
}

pub fn blur_region_file(
    path: &Path,
    region: images::RectF,
    pixelate: bool,
) -> Result<(), String> {
    let original = load_rgba(path).ok_or_else(|| "cannot read image".to_string())?;
    let (w, h) = original.dimensions();
    let x = region.x.max(0.0).min(w as f32) as u32;
    let y = region.y.max(0.0).min(h as f32) as u32;
    let rw = region.w.max(2.0).min((w - x) as f32).max(1.0) as u32;
    let rh = region.h.max(2.0).min((h - y) as f32).max(1.0) as u32;
    let sub = image::imageops::crop_imm(&*original, x, y, rw, rh).to_image();
    let processed = if pixelate {
        let small = image::imageops::resize(&sub, (rw / 8).max(1), (rh / 8).max(1), image::imageops::FilterType::Nearest);
        image::imageops::resize(&small, rw, rh, image::imageops::FilterType::Nearest)
    } else {
        image::imageops::blur(&sub, 8.0)
    };
    let mut out = (*original).clone();
    image::imageops::replace(&mut out, &processed, x as i64, y as i64);
    save_rgba_like(path, &out)?;
    invalidate_caches();
    Ok(())
}

// ───────────────────────────── Compression ─────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressFormat {
    Png,
    Jpeg,
    WebP,
}

impl CompressFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::WebP => "webp",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::WebP => "WebP",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompressOptions {
    pub format: CompressFormat,
    pub quality: u8,
    pub scale: f32,
    pub optimize: bool,
}

impl Default for CompressOptions {
    fn default() -> Self {
        Self {
            format: CompressFormat::Png,
            quality: 85,
            scale: 1.0,
            optimize: false,
        }
    }
}

/// Encodes a copy without touching the file. Returns the bytes and the scaled
/// pixels so callers can adjust `.edit.json` after applying.
pub fn compress_rgba(
    original: &image::RgbaImage,
    options: &CompressOptions,
) -> Result<(Vec<u8>, image::RgbaImage), String> {
    let scale = options.scale.clamp(0.05, 1.0);
    let width = ((original.width() as f32 * scale).round() as u32).max(1);
    let height = ((original.height() as f32 * scale).round() as u32).max(1);
    let scaled = if width == original.width() && height == original.height() {
        original.clone()
    } else {
        image::imageops::resize(original, width, height, image::imageops::FilterType::Lanczos3)
    };
    let format = match options.format {
        CompressFormat::Png => EncodedFormat::Png,
        CompressFormat::Jpeg => EncodedFormat::Jpeg,
        CompressFormat::WebP => EncodedFormat::WebP,
    };
    let mut bytes = encode_rgba(&scaled, format, options.quality)?;
    if options.optimize && options.format == CompressFormat::Png {
        bytes = optimize_png(&bytes).unwrap_or(bytes);
    }
    Ok((bytes, scaled))
}

/// Replaces the attachment with its compressed copy. If the format changes the
/// file gets a new extension; the caller rewrites the note reference.
pub fn apply_compression(
    path: &Path,
    edit: &mut ImageEdit,
    options: &CompressOptions,
) -> Result<PathBuf, String> {
    let original = load_rgba(path).ok_or_else(|| "cannot read image".to_string())?;
    let (_, scaled) = compress_rgba(&original, options)?;
    let (bytes, _) = compress_rgba(&original, options)?;
    let old_ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_default();
    let new_ext = options.format.extension();
    let target = if old_ext == new_ext {
        path.to_path_buf()
    } else {
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let candidate = path.with_file_name(format!("{stem}.{new_ext}"));
        if candidate.exists() {
            let suffix = &uuid::Uuid::new_v4().simple().to_string()[..4];
            path.with_file_name(format!("{stem}-{suffix}.{new_ext}"))
        } else {
            candidate
        }
    };
    save_like(&target, &bytes)?;
    let factor = scaled.width() as f32 / original.width().max(1) as f32;
    if let Some(crop) = edit.crop.as_mut() {
        crop.x *= factor;
        crop.y *= factor;
        crop.w *= factor;
        crop.h *= factor;
    }
    images::scale_annotations(&mut edit.annotations, factor);
    let old_edit = images::edit_path(path);
    let new_edit = images::edit_path(&target);
    if target != path && old_edit.exists() {
        let _ = std::fs::rename(&old_edit, &new_edit);
    }
    images::save_edit(&target, edit).map_err(|err| err.to_string())?;
    if target != path {
        let _ = std::fs::remove_file(path);
    }
    invalidate_caches();
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::images::{Annotation, AnnotationKind, ImageEdit, Point};

    fn canvas(w: u32, h: u32) -> image::RgbaImage {
        image::RgbaImage::from_pixel(w, h, image::Rgba([40, 40, 220, 255]))
    }

    #[test]
    fn a_filled_rectangle_changes_its_interior_pixels() {
        let mut bytes = canvas(40, 40).into_raw();
        let annotation = Annotation {
            kind: AnnotationKind::Rect,
            points: vec![Point::new(5.0, 5.0), Point::new(30.0, 30.0)],
            color: "#ffffff".to_string(),
            width: 3.0,
            filled: true,
            text: None,
            font_size: 16.0,
        };
        draw_annotations(&mut bytes, 40, 40, &[annotation], (0.0, 0.0));
        let image = image::RgbaImage::from_raw(40, 40, bytes).unwrap();
        assert_eq!(image.get_pixel(15, 15)[0], 255);
        assert_eq!(image.get_pixel(1, 1)[0], 40);
    }

    #[test]
    fn rounded_mask_clears_the_corner_alpha() {
        let mut image = canvas(100, 60);
        apply_round_mask(&mut image, 20.0);
        assert_eq!(image.get_pixel(0, 0)[3], 0);
        assert_eq!(image.get_pixel(50, 30)[3], 255);
    }

    #[test]
    fn text_rasterization_produces_cyrillic_pixels() {
        let (w, h, rgba) = rasterize_text("Привет", 20.0, [0, 0, 0, 255], 200.0).unwrap();
        assert!(w > 10 && h > 10);
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        assert!(rgba.chunks_exact(4).any(|p| p[3] > 0));
    }

    #[test]
    fn baking_applies_crop_and_annotations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.png");
        canvas(80, 40).save(&path).unwrap();
        let edit = ImageEdit {
            crop: Some(crate::images::CropRect { x: 10.0, y: 5.0, w: 40.0, h: 20.0 }),
            annotations: vec![Annotation {
                kind: AnnotationKind::Line,
                points: vec![Point::new(10.0, 10.0), Point::new(40.0, 30.0)],
                ..Default::default()
            }],
            ..Default::default()
        };
        let baked = bake_rgba(
            &path,
            &edit,
            &BakeOptions { apply_crop: true, ..Default::default() },
        )
        .unwrap();
        assert_eq!(baked.dimensions(), (40, 20));
    }
}

/// Encodes a preview copy for the compression dialog without writing.
pub fn compress_preview(
    path: &Path,
    options: &CompressOptions,
) -> Result<(Vec<u8>, u32, u32, CompressFormat), String> {
    let original = load_rgba(path).ok_or_else(|| "cannot read image".to_string())?;
    let (bytes, scaled) = compress_rgba(&original, options)?;
    Ok((bytes, scaled.width(), scaled.height(), options.format))
}
