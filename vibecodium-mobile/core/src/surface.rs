//! Native Android surface used by the hybrid UI.
//!
//! The Android shell keeps pickers, IME ownership, sync and navigation in
//! Kotlin. This module paints the explorer and the source editor into an
//! `ANativeWindow` owned by a `SurfaceView`, and reports taps back as a small
//! JSON action. It deliberately has no Compose/View dependency.

use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::c_void;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use fontdue::Font;
use jni::objects::{JClass, JObject, JString};
use jni::sys::{jboolean, jfloat, jint, jlong, jstring};
use jni::JNIEnv;
use serde::Deserialize;
use serde_json::json;

#[repr(C)]
struct ANativeWindow {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Default)]
struct ANativeWindowBuffer {
    width: i32,
    height: i32,
    stride: i32,
    format: i32,
    bits: *mut c_void,
    reserved: [u32; 6],
}

#[cfg(target_os = "android")]
#[link(name = "android")]
extern "C" {
    fn ANativeWindow_fromSurface(env: *mut jni::sys::JNIEnv, surface: jni::sys::jobject) -> *mut ANativeWindow;
    fn ANativeWindow_release(window: *mut ANativeWindow);
    fn ANativeWindow_setBuffersGeometry(window: *mut ANativeWindow, width: i32, height: i32, format: i32) -> i32;
    fn ANativeWindow_lock(window: *mut ANativeWindow, buffer: *mut ANativeWindowBuffer, dirty: *mut c_void) -> i32;
    fn ANativeWindow_unlockAndPost(window: *mut ANativeWindow) -> i32;
}

#[cfg(target_os = "android")]
#[link(name = "log")]
extern "C" {
    fn __android_log_print(prio: i32, tag: *const u8, fmt: *const u8, ...) -> i32;
}

/// Host builds have no liblog; diagnostics become no-ops so the surface's
/// tests can link without the Android NDK.
#[cfg(target_os = "android")]
macro_rules! android_log {
    ($prio:expr, $tag:expr, $fmt:expr $(, $arg:expr)* $(,)?) => {
        unsafe { __android_log_print($prio, $tag, $fmt $(, $arg)*) }
    };
}

#[cfg(not(target_os = "android"))]
macro_rules! android_log {
    ($($tt:tt)*) => {
        0i32
    };
}

#[cfg(not(target_os = "android"))]
#[allow(non_snake_case)]
unsafe fn ANativeWindow_fromSurface(_env: *mut jni::sys::JNIEnv, _surface: jni::sys::jobject) -> *mut ANativeWindow { std::ptr::null_mut() }
#[cfg(not(target_os = "android"))]
#[allow(non_snake_case)]
unsafe fn ANativeWindow_release(_window: *mut ANativeWindow) {}
#[cfg(not(target_os = "android"))]
#[allow(non_snake_case)]
unsafe fn ANativeWindow_setBuffersGeometry(_window: *mut ANativeWindow, _width: i32, _height: i32, _format: i32) -> i32 { 0 }
#[cfg(not(target_os = "android"))]
#[allow(non_snake_case)]
unsafe fn ANativeWindow_lock(_window: *mut ANativeWindow, _buffer: *mut ANativeWindowBuffer, _dirty: *mut c_void) -> i32 { 0 }
#[cfg(not(target_os = "android"))]
#[allow(non_snake_case)]
unsafe fn ANativeWindow_unlockAndPost(_window: *mut ANativeWindow) -> i32 { 0 }

const WINDOW_FORMAT_RGBA_8888: i32 = 1;

/// Colors supplied by the Kotlin theme layer. Every field is 0xRRGGBB;
/// alpha is handled by the painter where it matters. The palette is global
/// because one app theme applies to both the tree and editor surfaces.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct SurfacePalette {
    background: u32,
    surface: u32,
    surface_alt: u32,
    text: u32,
    text_muted: u32,
    text_faint: u32,
    accent: u32,
    accent_hover: u32,
    border: u32,
    selection: u32,
    code_background: u32,
    code_normal: u32,
    code_comment: u32,
    code_keyword: u32,
    code_string: u32,
    code_number: u32,
    code_function: u32,
    code_type: u32,
    code_punctuation: u32,
    heading: u32,
    marker: u32,
    link: u32,
    tag: u32,
    inline_code: u32,
    inline_code_background: u32,
    highlight: u32,
    tree_background: u32,
    tree_text: u32,
    tree_muted: u32,
    tree_folder: u32,
    active_row: u32,
    tag_background: u32,
    editor_background: u32,
    scrollbar: u32,
}

impl Default for SurfacePalette {
    fn default() -> Self {
        Self {
            background: 0x000000,
            surface: 0x1e1e1e,
            surface_alt: 0x282828,
            text: 0xdadada,
            text_muted: 0xb3b3b3,
            text_faint: 0x666666,
            accent: 0x8a5cf5,
            accent_hover: 0xa68af9,
            border: 0x333333,
            selection: 0x23173d,
            code_background: 0x111111,
            code_normal: 0xdadada,
            code_comment: 0x666666,
            code_keyword: 0xfa99cd,
            code_string: 0x44cf6e,
            code_number: 0xe9973f,
            code_function: 0xe0de71,
            code_type: 0x53dfdd,
            code_punctuation: 0xb3b3b3,
            heading: 0xdadada,
            marker: 0x666666,
            link: 0x8a5cf5,
            tag: 0x8a5cf5,
            inline_code: 0x53dfdd,
            inline_code_background: 0x111111,
            highlight: 0x2a2140,
            tree_background: 0x282828,
            tree_text: 0xdadada,
            tree_muted: 0x8a8a8a,
            tree_folder: 0x8a5cf5,
            active_row: 0x333333,
            tag_background: 0x2a2a2a,
            editor_background: 0x000000,
            scrollbar: 0x6a6a6a,
        }
    }
}

static THEME_PALETTE: OnceLock<Mutex<SurfacePalette>> = OnceLock::new();

fn palette() -> SurfacePalette {
    THEME_PALETTE
        .get_or_init(|| Mutex::new(SurfacePalette::default()))
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default()
}

fn set_palette(value: SurfacePalette) {
    if let Ok(mut guard) = THEME_PALETTE
        .get_or_init(|| Mutex::new(SurfacePalette::default()))
        .lock()
    {
        *guard = value;
    }
}





#[inline(always)]
fn logical(rgb: u32) -> u32 {
    let r = (rgb >> 16) & 0xff;
    let g = (rgb >> 8) & 0xff;
    let b = rgb & 0xff;
    r | (g << 8) | (b << 16) | (0xff << 24)
}

#[inline(always)]
fn blend(dst: u32, rgb: u32, alpha: u8) -> u32 {
    if alpha == 0 {
        return dst;
    }
    let color = logical(rgb);
    if alpha == 255 {
        return color;
    }
    let a = u32::from(alpha);
    let inv = 255 - a;
    let dr = dst & 0xff;
    let dg = (dst >> 8) & 0xff;
    let db = (dst >> 16) & 0xff;
    let sr = color & 0xff;
    let sg = (color >> 8) & 0xff;
    let sb = (color >> 16) & 0xff;
    let r = (sr * a + dr * inv + 128) / 255;
    let g = (sg * a + dg * inv + 128) / 255;
    let b = (sb * a + db * inv + 128) / 255;
    r | (g << 8) | (b << 16) | (0xff << 24)
}

fn fill(pixels: &mut [u32], width: usize, height: usize, rgb: u32) {
    let color = logical(rgb);
    let count = (width * height).min(pixels.len());
    pixels[..count].fill(color);
}

fn rect(pixels: &mut [u32], width: usize, height: usize, x: f32, y: f32, w: f32, h: f32, rgb: u32) {
    let x0 = x.floor().max(0.0) as usize;
    let y0 = y.floor().max(0.0) as usize;
    let x1 = (x + w).ceil().min(width as f32).max(0.0) as usize;
    let y1 = (y + h).ceil().min(height as f32).max(0.0) as usize;
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let color = logical(rgb);
    for row in y0..y1 {
        for column in x0..x1 {
            pixels[row * width + column] = color;
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn line(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    thickness: f32,
    rgb: u32,
) {
    let steps = ((x1 - x0).abs().max((y1 - y0).abs())).ceil() as i32;
    if steps <= 0 {
        return;
    }
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let x = x0 + (x1 - x0) * t;
        let y = y0 + (y1 - y0) * t;
        rect(
            pixels,
            width,
            height,
            x - thickness / 2.0,
            y - thickness / 2.0,
            thickness,
            thickness,
            rgb,
        );
    }
}

// Lucide path data mirrored from the desktop client (`src/icons.rs`) and
// Obsidian 1.15.0's icon registry (`assets/public/app.js`).
const CHEVRON_RIGHT_SVG: &str = "M8.59 16.59 13.17 12 8.59 7.41 10 6l6 6-6 6-1.41-1.41z";
const CHEVRON_DOWN_SVG: &str = "M7.41 8.59 12 13.17l4.59-4.58L18 10l-6 6-6-6 1.41-1.41z";
const FOLDER_SVG: &str = "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z M2 10h20";
const FOLDER_OPEN_SVG: &str = "m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2";
const FILE_TEXT_SVG: &str = "M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z M14 2v5a1 1 0 0 0 1 1h5 M10 9H8 M16 13H8 M16 17H8";

// Callout glyphs, mirrored from the desktop `icons.rs` set (24x24 stroke).
const CALLOUT_NOTE_SVG: &str = "M12 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7 M18.375 2.625a1 1 0 0 1 3 3l-9.013 9.014a2 2 0 0 1-.853.505l-2.873.84a.5.5 0 0 1-.62-.62l.84-2.873a2 2 0 0 1 .506-.852z";
const CALLOUT_INFO_SVG: &str = "M12 2a10 10 0 1 0 0 20a10 10 0 1 0 0 -20 M12 16v-4 M12 8h0.1";
const CALLOUT_TIP_SVG: &str = "M13 2L3 14L12 14L11 22L21 10L12 10L13 2Z";
const CALLOUT_WARNING_SVG: &str = "M21.73 18l-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3 M12 9v4 M12 17h0.1";
const CALLOUT_DANGER_SVG: &str = "M12 2a10 10 0 1 0 0 20a10 10 0 1 0 0 -20 M12 8v4 M12 16h0.1";
const CALLOUT_QUOTE_SVG: &str = "M16 3a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2 1 1 0 0 1 1 1v1a2 2 0 0 1-2 2 1 1 0 0 0-1 1v2a1 1 0 0 0 1 1 6 6 0 0 0 6-6V5a2 2 0 0 0-2-2z M5 3a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2 1 1 0 0 1 1 1v1a2 2 0 0 1-2 2 1 1 0 0 0-1 1v2a1 1 0 0 0 1 1 6 6 0 0 0 6-6V5a2 2 0 0 0-2-2z";

type IconPoints = Vec<Vec<(f32, f32)>>;

fn svg_number(bytes: &[u8], index: &mut usize) -> Option<f32> {
    while *index < bytes.len() {
        match bytes[*index] {
            b' ' | b',' | b'\n' | b'\r' | b'\t' => *index += 1,
            _ => break,
        }
    }
    let start = *index;
    if *index < bytes.len() && (bytes[*index] == b'-' || bytes[*index] == b'+') {
        *index += 1;
    }
    let mut seen_dot = false;
    while *index < bytes.len() {
        match bytes[*index] {
            b'0'..=b'9' => *index += 1,
            b'.' if !seen_dot => {
                seen_dot = true;
                *index += 1;
            }
            b'e' | b'E' => {
                *index += 1;
                if *index < bytes.len() && (bytes[*index] == b'-' || bytes[*index] == b'+') {
                    *index += 1;
                }
            }
            _ => break,
        }
    }
    if *index == start {
        return None;
    }
    std::str::from_utf8(&bytes[start..*index]).ok()?.parse().ok()
}

fn cubic_points(
    from: (f32, f32),
    c1: (f32, f32),
    c2: (f32, f32),
    to: (f32, f32),
    out: &mut Vec<(f32, f32)>,
) {
    const STEPS: usize = 12;
    for step in 1..=STEPS {
        let t = step as f32 / STEPS as f32;
        let u = 1.0 - t;
        let x = u * u * u * from.0 + 3.0 * u * u * t * c1.0 + 3.0 * u * t * t * c2.0 + t * t * t * to.0;
        let y = u * u * u * from.1 + 3.0 * u * u * t * c1.1 + 3.0 * u * t * t * c2.1 + t * t * t * to.1;
        out.push((x, y));
    }
}

fn quad_points(from: (f32, f32), control: (f32, f32), to: (f32, f32), out: &mut Vec<(f32, f32)>) {
    const STEPS: usize = 8;
    for step in 1..=STEPS {
        let t = step as f32 / STEPS as f32;
        let u = 1.0 - t;
        out.push((
            u * u * from.0 + 2.0 * u * t * control.0 + t * t * to.0,
            u * u * from.1 + 2.0 * u * t * control.1 + t * t * to.1,
        ));
    }
}

fn arc_points(
    from: (f32, f32),
    rx: f32,
    ry: f32,
    rotation_deg: f32,
    large: bool,
    sweep: bool,
    to: (f32, f32),
) -> Vec<(f32, f32)> {
    let mut rx = rx.abs();
    let mut ry = ry.abs();
    if rx < 0.0001 || ry < 0.0001 {
        return vec![to];
    }
    let phi = rotation_deg.to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();
    let dx = (from.0 - to.0) / 2.0;
    let dy = (from.1 - to.1) / 2.0;
    let x1 = cos_phi * dx + sin_phi * dy;
    let y1 = -sin_phi * dx + cos_phi * dy;
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        let scale = lambda.sqrt();
        rx *= scale;
        ry *= scale;
    }
    let rx2 = rx * rx;
    let ry2 = ry * ry;
    let numerator = (rx2 * ry2 - rx2 * y1 * y1 - ry2 * x1 * x1).max(0.0);
    let denominator = rx2 * y1 * y1 + ry2 * x1 * x1;
    let mut coefficient = if denominator > 0.0 {
        (numerator / denominator).sqrt()
    } else {
        0.0
    };
    if large == sweep {
        coefficient = -coefficient;
    }
    let cx1 = coefficient * rx * y1 / ry;
    let cy1 = -coefficient * ry * x1 / rx;
    let cx = cos_phi * cx1 - sin_phi * cy1 + (from.0 + to.0) / 2.0;
    let cy = sin_phi * cx1 + cos_phi * cy1 + (from.1 + to.1) / 2.0;
    let start_angle = ((y1 - cy1) / ry).atan2((x1 - cx1) / rx);
    let end_angle = ((-y1 - cy1) / ry).atan2((-x1 - cx1) / rx);
    let mut delta = end_angle - start_angle;
    if sweep && delta < 0.0 {
        delta += std::f32::consts::TAU;
    } else if !sweep && delta > 0.0 {
        delta -= std::f32::consts::TAU;
    }
    let steps = ((delta.abs() / (std::f32::consts::PI / 12.0)).ceil() as usize).max(2);
    let mut points = Vec::with_capacity(steps);
    for step in 1..=steps {
        let angle = start_angle + delta * step as f32 / steps as f32;
        let (sin_a, cos_a) = angle.sin_cos();
        points.push((
            cx + cos_phi * rx * cos_a - sin_phi * ry * sin_a,
            cy + sin_phi * rx * cos_a + cos_phi * ry * sin_a,
        ));
    }
    points
}

fn flatten_path(data: &str) -> IconPoints {
    let bytes = data.as_bytes();
    let mut index = 0usize;
    let mut command: Option<u8> = None;
    let mut current = (0.0f32, 0.0f32);
    let mut subpath_start = current;
    let mut last_cubic: Option<(f32, f32)> = None;
    let mut last_quad: Option<(f32, f32)> = None;
    let mut paths: IconPoints = Vec::new();
    let mut path: Vec<(f32, f32)> = Vec::new();
    let is_letter = |byte: u8| byte.is_ascii_alphabetic();

    while index < bytes.len() {
        while index < bytes.len() && matches!(bytes[index], b' ' | b',' | b'\n' | b'\r' | b'\t') {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }
        if is_letter(bytes[index]) {
            let next = bytes[index];
            index += 1;
            if matches!(next, b'Z' | b'z') {
                if path.len() > 1 && path.last() != path.first() {
                    path.push(*path.first().unwrap_or(&current));
                }
                if path.len() > 1 {
                    paths.push(std::mem::take(&mut path));
                } else {
                    path.clear();
                }
                current = subpath_start;
                last_cubic = None;
                last_quad = None;
                command = None;
                continue;
            }
            command = Some(next);
        }
        let Some(next) = command else {
            index += 1;
            continue;
        };
        let relative = next.is_ascii_lowercase();
        let add = |point: (f32, f32)| {
            if relative {
                (current.0 + point.0, current.1 + point.1)
            } else {
                point
            }
        };
        let fail = |command: &mut Option<u8>, index: &mut usize| {
            *command = None;
            *index += 1;
        };
        match next {
            b'M' | b'm' => {
                let (Some(x), Some(y)) = (svg_number(bytes, &mut index), svg_number(bytes, &mut index)) else {
                    fail(&mut command, &mut index);
                    continue;
                };
                let point = add((x, y));
                if !path.is_empty() {
                    paths.push(std::mem::take(&mut path));
                }
                current = point;
                subpath_start = point;
                path.push(point);
                last_cubic = None;
                last_quad = None;
                command = Some(if relative { b'l' } else { b'L' });
            }
            b'L' | b'l' => {
                let (Some(x), Some(y)) = (svg_number(bytes, &mut index), svg_number(bytes, &mut index)) else {
                    fail(&mut command, &mut index);
                    continue;
                };
                let point = add((x, y));
                path.push(point);
                current = point;
                last_cubic = None;
                last_quad = None;
            }
            b'H' | b'h' => {
                let Some(x) = svg_number(bytes, &mut index) else {
                    fail(&mut command, &mut index);
                    continue;
                };
                let point = (if relative { current.0 + x } else { x }, current.1);
                path.push(point);
                current = point;
                last_cubic = None;
                last_quad = None;
            }
            b'V' | b'v' => {
                let Some(y) = svg_number(bytes, &mut index) else {
                    fail(&mut command, &mut index);
                    continue;
                };
                let point = (current.0, if relative { current.1 + y } else { y });
                path.push(point);
                current = point;
                last_cubic = None;
                last_quad = None;
            }
            b'C' | b'c' => {
                let numbers: Vec<f32> = (0..6).filter_map(|_| svg_number(bytes, &mut index)).collect();
                if numbers.len() != 6 {
                    fail(&mut command, &mut index);
                    continue;
                }
                let c1 = add((numbers[0], numbers[1]));
                let c2 = add((numbers[2], numbers[3]));
                let end = add((numbers[4], numbers[5]));
                cubic_points(current, c1, c2, end, &mut path);
                current = end;
                last_cubic = Some(c2);
                last_quad = None;
            }
            b'S' | b's' => {
                let numbers: Vec<f32> = (0..4).filter_map(|_| svg_number(bytes, &mut index)).collect();
                if numbers.len() != 4 {
                    fail(&mut command, &mut index);
                    continue;
                }
                let c1 = last_cubic
                    .map(|control| (2.0 * current.0 - control.0, 2.0 * current.1 - control.1))
                    .unwrap_or(current);
                let c2 = add((numbers[0], numbers[1]));
                let end = add((numbers[2], numbers[3]));
                cubic_points(current, c1, c2, end, &mut path);
                current = end;
                last_cubic = Some(c2);
                last_quad = None;
            }
            b'Q' | b'q' => {
                let numbers: Vec<f32> = (0..4).filter_map(|_| svg_number(bytes, &mut index)).collect();
                if numbers.len() != 4 {
                    fail(&mut command, &mut index);
                    continue;
                }
                let control = add((numbers[0], numbers[1]));
                let end = add((numbers[2], numbers[3]));
                quad_points(current, control, end, &mut path);
                current = end;
                last_quad = Some(control);
                last_cubic = None;
            }
            b'T' | b't' => {
                let (Some(x), Some(y)) = (svg_number(bytes, &mut index), svg_number(bytes, &mut index)) else {
                    fail(&mut command, &mut index);
                    continue;
                };
                let control = last_quad
                    .map(|prev| (2.0 * current.0 - prev.0, 2.0 * current.1 - prev.1))
                    .unwrap_or(current);
                let end = add((x, y));
                quad_points(current, control, end, &mut path);
                current = end;
                last_quad = Some(control);
                last_cubic = None;
            }
            b'A' | b'a' => {
                let numbers: Vec<f32> = (0..7).filter_map(|_| svg_number(bytes, &mut index)).collect();
                if numbers.len() != 7 {
                    fail(&mut command, &mut index);
                    continue;
                }
                let end = add((numbers[5], numbers[6]));
                let points = arc_points(
                    current,
                    numbers[0],
                    numbers[1],
                    numbers[2],
                    numbers[3] != 0.0,
                    numbers[4] != 0.0,
                    end,
                );
                path.extend(points);
                current = end;
                last_cubic = None;
                last_quad = None;
            }
            _ => {
                command = None;
                index += 1;
            }
        }
    }
    if path.len() > 1 {
        paths.push(path);
    }
    paths
}

fn chevron_paths(expanded: bool) -> &'static [Vec<(f32, f32)>] {
    static RIGHT: OnceLock<IconPoints> = OnceLock::new();
    static DOWN: OnceLock<IconPoints> = OnceLock::new();
    if expanded {
        DOWN.get_or_init(|| flatten_path(CHEVRON_DOWN_SVG)).as_slice()
    } else {
        RIGHT.get_or_init(|| flatten_path(CHEVRON_RIGHT_SVG)).as_slice()
    }
}

fn icon_paths(folder: bool, expanded: bool) -> &'static [Vec<(f32, f32)>] {
    static FOLDER: OnceLock<IconPoints> = OnceLock::new();
    static FOLDER_OPEN: OnceLock<IconPoints> = OnceLock::new();
    static FILE_TEXT: OnceLock<IconPoints> = OnceLock::new();
    if folder {
        if expanded {
            FOLDER_OPEN.get_or_init(|| flatten_path(FOLDER_OPEN_SVG)).as_slice()
        } else {
            FOLDER.get_or_init(|| flatten_path(FOLDER_SVG)).as_slice()
        }
    } else {
        FILE_TEXT.get_or_init(|| flatten_path(FILE_TEXT_SVG)).as_slice()
    }
}

/// The glyph a callout header shows for its family, matching the desktop set.
fn callout_icon_paths(
    kind: crate::markdown::CalloutKind,
) -> &'static [Vec<(f32, f32)>] {
    use crate::markdown::CalloutKind;
    static NOTE: OnceLock<IconPoints> = OnceLock::new();
    static INFO: OnceLock<IconPoints> = OnceLock::new();
    static TIP: OnceLock<IconPoints> = OnceLock::new();
    static WARNING: OnceLock<IconPoints> = OnceLock::new();
    static DANGER: OnceLock<IconPoints> = OnceLock::new();
    static QUOTE: OnceLock<IconPoints> = OnceLock::new();
    match kind {
        CalloutKind::Note => NOTE.get_or_init(|| flatten_path(CALLOUT_NOTE_SVG)).as_slice(),
        CalloutKind::Info => INFO.get_or_init(|| flatten_path(CALLOUT_INFO_SVG)).as_slice(),
        CalloutKind::Tip => TIP.get_or_init(|| flatten_path(CALLOUT_TIP_SVG)).as_slice(),
        CalloutKind::Warning => WARNING
            .get_or_init(|| flatten_path(CALLOUT_WARNING_SVG))
            .as_slice(),
        CalloutKind::Danger => DANGER
            .get_or_init(|| flatten_path(CALLOUT_DANGER_SVG))
            .as_slice(),
        CalloutKind::Quote => QUOTE
            .get_or_init(|| flatten_path(CALLOUT_QUOTE_SVG))
            .as_slice(),
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_icon(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    size: f32,
    thickness: f32,
    paths: &[Vec<(f32, f32)>],
    rgb: u32,
) {
    let scale = size / 24.0;
    for path in paths {
        for pair in path.windows(2) {
            line(
                pixels,
                width,
                height,
                x + pair[0].0 * scale,
                y + pair[0].1 * scale,
                x + pair[1].0 * scale,
                y + pair[1].1 * scale,
                thickness,
                rgb,
            );
        }
    }
}

/// Filled SVG path rasterizer for the disclosed-file chevron. SVG icons in
/// the editor are usually stroked; this one follows the filled Material
/// chevron used by the RecyclerView search rows.
fn fill_icon(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    size: f32,
    paths: &[Vec<(f32, f32)>],
    rgb: u32,
) {
    let scale = size / 24.0;
    for path in paths {
        if path.len() < 3 {
            continue;
        }
        let points: Vec<(f32, f32)> = path
            .iter()
            .map(|point| (x + point.0 * scale, y + point.1 * scale))
            .collect();
        let min_y = points
            .iter()
            .map(|point| point.1)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as usize;
        let max_y = points
            .iter()
            .map(|point| point.1)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .min(height as f32) as usize;
        for py in min_y..max_y {
            let scan_y = py as f32 + 0.5;
            let mut crossings = Vec::new();
            for index in 0..points.len() {
                let a = points[index];
                let b = points[(index + 1) % points.len()];
                if (a.1 <= scan_y) != (b.1 <= scan_y) {
                    let t = (scan_y - a.1) / (b.1 - a.1);
                    crossings.push(a.0 + (b.0 - a.0) * t);
                }
            }
            crossings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let mut index = 0;
            while index + 1 < crossings.len() {
                let start = crossings[index].floor().max(0.0) as usize;
                let end = crossings[index + 1].ceil().min(width as f32) as usize;
                if end > start {
                    rect(
                        pixels,
                        width,
                        height,
                        start as f32,
                        py as f32,
                        (end - start) as f32,
                        1.0,
                        rgb,
                    );
                }
                index += 2;
            }
        }
    }
}

#[derive(Default)]
struct IconRaster {
    extent: usize,
    pixels: Vec<u32>,
}

#[derive(Default)]
struct EmojiRaster {
    width: usize,
    height: usize,
    pixels: Vec<u32>,
}

fn is_emoji_start(c: char) -> bool {
    let u = c as u32;
    matches!(
        u,
        0x1F300..=0x1F5FF // Misc Symbols and Pictographs (e.g. 🎁, 🔔, 💎, 🔥)
        | 0x1F600..=0x1F64F // Emoticons (e.g. 😀, 😂, 😎)
        | 0x1F680..=0x1F6FF // Transport and Map (e.g. 🚀, 🚗)
        | 0x1F700..=0x1F77F // Alchemical Symbols
        | 0x1F780..=0x1F7FF // Geometric Shapes Extended (🟠, 🟢, 🟫)
        | 0x1F800..=0x1F8FF // Supplemental Arrows-C
        | 0x1F900..=0x1F9FF // Supplemental Symbols and Pictographs (e.g. 🥳, 🤖, 🧠)
        | 0x1FA00..=0x1FA6F // Chess Symbols
        | 0x1FA70..=0x1FAFF // Symbols and Pictographs Extended-A (e.g. 🩺, 🪟)
        | 0x1F1E6..=0x1F1FF // Regional Indicator Symbols (Flags)
        | 0x231A..=0x231B   // Watch, Hourglass
        | 0x23E9..=0x23EC   // Fast forward, rewind
        | 0x23F0            // Alarm clock
        | 0x23F3            // Hourglass done
        | 0x23F8..=0x23FA   // Pause, stop, record
        | 0x25AA..=0x25AB   // Black/white small square
        | 0x25FB..=0x25FE   // Medium squares
        | 0x2600..=0x26FF   // Misc symbols (☀️, ☁️, ☂️, ⚡, ⚠️, ☕, ⚽, ⚾, ⛄, ⛅, ⛽)
        | 0x2702..=0x27B0   // Dingbats (✂️, ✈️, ✉️, ✌️, ✍️, ✏️, ✒️, ✔️, ✖️, ✝️, ✡️, ✳️, ✴️, ❄️, ❇️, ❌, ❎, ❓, ❔, ❕, ❗, ❣️, ❤️, ➕, ➖, ➗, ➡️)
        | 0x2934..=0x2935   // Curving arrows
        | 0x2B05..=0x2B07   // Black arrows
        | 0x2B1B..=0x2B1C   // Black/white large square
        | 0x2B50            // Star ⭐
        | 0x2B55            // Heavy large circle ⭕
        | 0x3030            // Wavy dash
        | 0x303D            // Part alternation mark
        | 0x3297            // Circled ideograph congratulation
        | 0x3299            // Circled ideograph secret
    )
}

fn is_cjk(c: char) -> bool {
    let u = c as u32;
    matches!(
        u,
        0x1100..=0x115F   // Hangul Jamo
        | 0x2329..=0x232A // Angle brackets
        | 0x2E80..=0x302F // CJK Radicals, Kangxi, CJK Symbols
        | 0x3040..=0x4DBF // Hiragana, Katakana, Bopomofo, CJK Ext A
        | 0x4E00..=0xA4CF // CJK Unified Ideographs, Yi
        | 0xAC00..=0xD7A3 // Hangul Syllables
        | 0xF900..=0xFAFF // CJK Compatibility Ideographs
        | 0xFE10..=0xFE19 // Vertical Forms
        | 0xFE30..=0xFE4F // CJK Compatibility Forms
        | 0xFF01..=0xFF60 // Fullwidth Forms
        | 0xFFE0..=0xFFE6 // Fullwidth Signs
        | 0x20000..=0x2FA1F // CJK Extensions
    )
}

fn emoji_cluster_len(chars: &[char], start: usize) -> usize {
    if start >= chars.len() {
        return 0;
    }
    let c = chars[start];
    let is_start = is_emoji_start(c);
    let has_fe0f = start + 1 < chars.len() && chars[start + 1] == '\u{fe0f}';
    if !is_start && !has_fe0f {
        return 0;
    }
    let mut len = 1;
    let is_flag_start = (0x1F1E6..=0x1F1FF).contains(&(c as u32));
    while start + len < chars.len() {
        let next_c = chars[start + len];
        let next_u = next_c as u32;
        let is_continuation = next_c == '\u{fe0f}'
            || next_c == '\u{fe0e}'
            || (0x1f3fb..=0x1f3ff).contains(&next_u)
            || (0xe0020..=0xe007f).contains(&next_u)
            || (is_flag_start && (0x1f1e6..=0x1f1ff).contains(&next_u) && len == 1);
        if is_continuation {
            len += 1;
            if is_flag_start && (0x1f1e6..=0x1f1ff).contains(&next_u) {
                break;
            }
        } else if next_c == '\u{200d}' {
            len += 1;
            if start + len < chars.len() {
                len += 1;
            }
        } else {
            break;
        }
    }
    len
}

fn line_cell_offsets(chars: &[char], hidden: &[bool]) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(chars.len() + 1);
    let mut current_cell = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        offsets.push(current_cell);
        if hidden.get(i).copied().unwrap_or(false) {
            i += 1;
            continue;
        }
        let cluster_len = emoji_cluster_len(chars, i);
        if cluster_len > 0 {
            current_cell += 2;
            for _ in 1..cluster_len {
                i += 1;
                offsets.push(current_cell);
            }
            i += 1;
        } else {
            let c = chars[i];
            if c == '\r' || c == '\u{fe0f}' || c == '\u{fe0e}' {
                // zero-width continuation or carriage return
            } else if is_cjk(c) {
                current_cell += 2;
            } else {
                current_cell += 1;
            }
            i += 1;
        }
    }
    offsets.push(current_cell);
    offsets
}

/// Pixel x of every char boundary in one logical line. Hidden Markdown markers
/// take no space, emoji clusters advance as one unit, and continuation chars
/// report the position after the cluster, matching `line_cell_offsets`.
/// `out` is reused so a scroll frame does not allocate per visible line.
fn line_x_offsets(
    out: &mut Vec<f32>,
    painter: &mut Painter,
    chars: &[char],
    hidden: &[bool],
    px: f32,
    face_at: impl Fn(usize) -> Face,
) {
    out.clear();
    out.reserve(chars.len() + 1);
    let mut x = 0.0f32;
    let mut i = 0usize;
    while i < chars.len() {
        out.push(x);
        if hidden.get(i).copied().unwrap_or(false) {
            i += 1;
            continue;
        }
        let (advance, len) = painter.advance_at(chars, i, px, face_at(i));
        x += advance;
        for _ in 1..len {
            i += 1;
            out.push(x);
        }
        i += 1;
    }
    out.push(x);
}

fn prev_grapheme_char_count(chars: &[char], pos: usize) -> usize {
    if pos == 0 || chars.is_empty() {
        return 0;
    }
    let pos = pos.min(chars.len());
    let search_start = pos.saturating_sub(12);
    for p in search_start..pos {
        let len = emoji_cluster_len(chars, p);
        if len > 0 && p + len == pos {
            return len;
        }
    }
    1
}

fn next_grapheme_char_count(chars: &[char], pos: usize) -> usize {
    if pos >= chars.len() {
        return 0;
    }
    let len = emoji_cluster_len(chars, pos);
    if len > 0 {
        len
    } else {
        1
    }
}

fn snap_to_cluster_start(chars: &[char], pos: usize) -> usize {
    if pos == 0 || pos >= chars.len() {
        return pos.min(chars.len());
    }
    let search_start = pos.saturating_sub(12);
    for p in search_start..pos {
        let len = emoji_cluster_len(chars, p);
        if len > 0 && p + len > pos {
            return p;
        }
    }
    pos
}

fn snap_to_cluster_end(chars: &[char], pos: usize) -> usize {
    if pos == 0 || pos >= chars.len() {
        return pos.min(chars.len());
    }
    let search_start = pos.saturating_sub(12);
    for p in search_start..pos {
        let len = emoji_cluster_len(chars, p);
        if len > 0 && p + len > pos {
            return p + len;
        }
    }
    pos
}

fn snap_to_cluster_boundary(chars: &[char], pos: usize) -> usize {
    if pos == 0 || pos >= chars.len() {
        return pos.min(chars.len());
    }
    let search_start = pos.saturating_sub(12);
    for p in search_start..pos {
        let len = emoji_cluster_len(chars, p);
        if len > 0 && p + len > pos {
            if pos - p >= (len + 1) / 2 {
                return p + len;
            } else {
                return p;
            }
        }
    }
    pos
}

/// Font face a run of text is drawn with. `Mono` also selects the fixed-cell
/// advance that keeps code columns and indentation aligned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Face {
    Ui = 0,
    Mono = 1,
    Serif = 2,
}

impl Face {
    fn is_mono(self) -> bool {
        matches!(self, Face::Mono)
    }
}

struct FontSet {
    ui: Option<Font>,
    mono: Option<Font>,
    serif: Option<Font>,
    symbols: Option<Font>,
    symbols2: Option<Font>,
    cjk: Option<Font>,
}

static SYSTEM_FONTS: OnceLock<Arc<FontSet>> = OnceLock::new();

struct Painter {
    fonts: Arc<FontSet>,
    glyphs: HashMap<(u8, char, u32), (fontdue::Metrics, Vec<u8>)>,
    icons: HashMap<(usize, u32, u32), IconRaster>,
    emojis: HashMap<(String, u32), EmojiRaster>,
    metrics_cache: HashMap<(u32, u8), (f32, f32, f32)>,
    char_advances: HashMap<(u8, char, u32), f32>,
}

fn load_font(paths: &[&str]) -> Option<Font> {
    for path in paths {
        if !Path::new(path).is_file() {
            continue;
        }
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(font) = Font::from_bytes(bytes, fontdue::FontSettings::default()) {
                return Some(font);
            }
        }
    }
    None
}

/// System fonts are immutable and expensive to read/parse; every surface
/// shares one copy instead of re-reading the TTFs on each Painter creation.
fn system_fonts() -> Arc<FontSet> {
    SYSTEM_FONTS
        .get_or_init(|| {
            Arc::new(FontSet {
                ui: load_font(&[
                    "/system/fonts/Roboto-Regular.ttf",
                    "/system/fonts/RobotoStatic-Regular.ttf",
                    "/system/fonts/NotoSans-Regular.ttf",
                ]),
                mono: load_font(&[
                    "/system/fonts/RobotoMono-Regular.ttf",
                    "/system/fonts/DroidSansMono.ttf",
                    "/system/fonts/NotoSansMono-Regular.ttf",
                ]),
                serif: load_font(&[
                    "/system/fonts/NotoSerif-Regular.ttf",
                    "/system/fonts/NotoSerifStatic-Regular.ttf",
                    "/system/fonts/DroidSerif-Regular.ttf",
                ]),
                symbols: load_font(&[
                    "/system/fonts/NotoSansSymbols-Regular-Subsetted.ttf",
                    "/system/fonts/NotoSansSymbols-Regular.ttf",
                ]),
                symbols2: load_font(&[
                    "/system/fonts/NotoSansSymbols-Regular-Subsetted2.ttf",
                    "/system/fonts/NotoSansSymbols2-Regular.ttf",
                ]),
                cjk: None,
            })
        })
        .clone()
}

impl Default for Painter {
    fn default() -> Self {
        Self::new()
    }
}

impl Painter {
    fn new() -> Self {
        Self {
            fonts: system_fonts(),
            glyphs: HashMap::new(),
            icons: HashMap::new(),
            emojis: HashMap::new(),
            metrics_cache: HashMap::new(),
            char_advances: HashMap::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_filled_icon(
        &mut self,
        target: &mut [u32],
        target_width: usize,
        target_height: usize,
        x: f32,
        y: f32,
        size: f32,
        paths: &'static [Vec<(f32, f32)>],
        rgb: u32,
    ) {
        let pad = 2usize;
        let extent = size.ceil() as usize + pad * 2 + 2;
        let key = (paths.as_ptr() as usize, size.to_bits(), 0u32);
        if !self.icons.contains_key(&key) {
            let mut mask = vec![0u32; extent * extent];
            fill_icon(
                &mut mask,
                extent,
                extent,
                pad as f32,
                pad as f32,
                size,
                paths,
                0xffffff,
            );
            self.icons.insert(key, IconRaster { extent, pixels: mask });
        }
        let Some(raster) = self.icons.get(&key) else {
            return;
        };
        let color = logical(rgb);
        let origin_x = x.round() as i32 - pad as i32;
        let origin_y = y.round() as i32 - pad as i32;
        for row in 0..raster.extent {
            let target_y = origin_y + row as i32;
            if target_y < 0 || target_y >= target_height as i32 {
                continue;
            }
            for column in 0..raster.extent {
                let target_x = origin_x + column as i32;
                if target_x < 0 || target_x >= target_width as i32 {
                    continue;
                }
                if raster.pixels[row * raster.extent + column] != 0 {
                    target[target_y as usize * target_width + target_x as usize] = color;
                }
            }
        }
    }

    fn draw_stroke_icon(
        &mut self,
        target: &mut [u32],
        target_width: usize,
        target_height: usize,
        x: f32,
        y: f32,
        size: f32,
        thickness: f32,
        paths: &'static [Vec<(f32, f32)>],
        rgb: u32,
    ) {
        let pad = thickness.ceil() as usize + 2;
        let extent = size.ceil() as usize + pad * 2 + 2;
        let key = (paths.as_ptr() as usize, size.to_bits(), thickness.to_bits());
        if !self.icons.contains_key(&key) {
            let mut mask = vec![0u32; extent * extent];
            draw_icon(
                &mut mask,
                extent,
                extent,
                pad as f32,
                pad as f32,
                size,
                thickness,
                paths,
                0xffffff,
            );
            self.icons.insert(key, IconRaster { extent, pixels: mask });
        }
        let Some(raster) = self.icons.get(&key) else {
            return;
        };
        let color = logical(rgb);
        let origin_x = x.round() as i32 - pad as i32;
        let origin_y = y.round() as i32 - pad as i32;
        for row in 0..raster.extent {
            let target_y = origin_y + row as i32;
            if target_y < 0 || target_y >= target_height as i32 {
                continue;
            }
            for column in 0..raster.extent {
                let target_x = origin_x + column as i32;
                if target_x < 0 || target_x >= target_width as i32 {
                    continue;
                }
                if raster.pixels[row * raster.extent + column] != 0 {
                    target[target_y as usize * target_width + target_x as usize] = color;
                }
            }
        }
    }

    fn font(&self, face: Face) -> Option<&Font> {
        match face {
            Face::Mono => self.fonts.mono.as_ref().or(self.fonts.ui.as_ref()),
            Face::Serif => self
                .fonts
                .serif
                .as_ref()
                .or(self.fonts.ui.as_ref())
                .or(self.fonts.mono.as_ref()),
            Face::Ui => self.fonts.ui.as_ref().or(self.fonts.mono.as_ref()),
        }
    }

    fn font_for_char(&self, character: char, face: Face) -> Option<&Font> {
        let primary = match face {
            Face::Mono => self.fonts.mono.as_ref().or(self.fonts.ui.as_ref()),
            Face::Serif => self
                .fonts
                .serif
                .as_ref()
                .or(self.fonts.ui.as_ref())
                .or(self.fonts.mono.as_ref()),
            Face::Ui => self.fonts.ui.as_ref().or(self.fonts.mono.as_ref()),
        };
        if let Some(font) = primary {
            if font.lookup_glyph_index(character) != 0 {
                return Some(font);
            }
        }
        let secondary = if face.is_mono() {
            self.fonts.ui.as_ref()
        } else {
            self.fonts.mono.as_ref()
        };
        if let Some(font) = secondary {
            if font.lookup_glyph_index(character) != 0 {
                return Some(font);
            }
        }
        for fallback in [&self.fonts.symbols, &self.fonts.symbols2, &self.fonts.cjk] {
            if let Some(font) = fallback.as_ref() {
                if font.lookup_glyph_index(character) != 0 {
                    return Some(font);
                }
            }
        }
        primary
    }

    fn font_metrics(&mut self, px: f32, face: Face) -> (f32, f32, f32) {
        let key = (px.to_bits(), face as u8);
        if let Some(&cached) = self.metrics_cache.get(&key) {
            return cached;
        }
        let advance = self
            .font(face)
            .map(|f| f.metrics('M', px).advance_width)
            .unwrap_or(px * 0.6);
        let (ascent, descent) = self
            .font(face)
            .and_then(|f| f.horizontal_line_metrics(px))
            .map(|line| (line.ascent.round(), line.descent.abs().round()))
            .unwrap_or((px * 0.8, px * 0.25));
        self.metrics_cache.insert(key, (advance, ascent, descent));
        (advance, ascent, descent)
    }

    /// Ascent and descent (both positive) of the face at `px`. Their sum is the
    /// text box the glyphs are drawn into, which is what must be centered in a
    /// row so descenders sit inside the highlight and caret.
    fn font_line_span(&mut self, px: f32, face: Face) -> (f32, f32) {
        let (_, ascent, descent) = self.font_metrics(px, face);
        (ascent, descent)
    }

    fn advance(&mut self, px: f32, face: Face) -> f32 {
        self.font_metrics(px, face).0
    }

    /// Advance of one non-grapheme character. It mirrors exactly what `draw`
    /// moves the cursor by, so layout and glyphs cannot drift apart.
    fn advance_of_char(&mut self, character: char, px: f32, face: Face) -> f32 {
        if character == '\r' || character == '\u{fe0f}' || character == '\u{fe0e}' {
            return 0.0;
        }
        if face.is_mono() {
            let cell = self.advance(px, face);
            return if is_cjk(character) { 2.0 * cell } else { cell };
        }
        let character = if character == '\t' { ' ' } else { character };
        let key = (face as u8, character, px.round() as u32);
        if let Some(&cached) = self.char_advances.get(&key) {
            return cached;
        }
        let advance = self
            .font_for_char(character, face)
            .map(|font| font.metrics(character, px).advance_width)
            .unwrap_or(px * 0.6);
        self.char_advances.insert(key, advance);
        advance
    }

    /// Advance and char length of the grapheme starting at `index`. Emoji
    /// clusters are measured as one unit, matching `draw`.
    fn advance_at(&mut self, chars: &[char], index: usize, px: f32, face: Face) -> (f32, usize) {
        let cluster_len = emoji_cluster_len(chars, index);
        if cluster_len > 0 {
            let advance = if face.is_mono() {
                2.0 * self.advance(px, face)
            } else {
                px
            };
            return (advance, cluster_len);
        }
        let advance = self.advance_of_char(chars[index], px, face);
        (advance, 1)
    }

    fn width(&mut self, text: &str, px: f32, face: Face) -> f32 {
        if face.is_mono() {
            let advance = self.advance(px, Face::Mono);
            if text.is_ascii() {
                return text.len() as f32 * advance;
            }
            let chars: Vec<char> = text.chars().filter(|&c| c != '\r').collect();
            let offsets = line_cell_offsets(&chars, &[]);
            let total_cells = offsets.last().copied().unwrap_or(0);
            return total_cells as f32 * advance;
        }
        let Some(font) = self.font(face) else {
            return text.chars().count() as f32 * px * 0.6;
        };
        text.chars().filter(|&c| c != '\r').map(|c| {
            if is_emoji_start(c) {
                px
            } else {
                let f = self.font_for_char(c, face).unwrap_or(font);
                f.metrics(c, px).advance_width
            }
        }).sum()
    }

    fn draw(
        &mut self,
        pixels: &mut [u32],
        width: usize,
        height: usize,
        x: f32,
        y: f32,
        text: &str,
        px: f32,
        rgb: u32,
        face: Face,
        italic: bool,
    ) {
        let (cell_advance, ascent, _) = self.font_metrics(px, face);
        let baseline = y + ascent;
        let mut cursor = x;
        // A tab has no glyph; layout treats it as a space, so drawing must too.
        let tab_advance = if face.is_mono() {
            cell_advance
        } else {
            self.advance_of_char(' ', px, face)
        };

        if text.is_ascii() {
            let px_key = px.round() as u32;
            for &byte in text.as_bytes() {
                if byte == b'\r' {
                    continue;
                }
                let character = byte as char;
                let key = (face as u8, character, px_key);
                if !self.glyphs.contains_key(&key) {
                    let raster = self
                        .font_for_char(character, face)
                        .map(|font| font.rasterize(character, px))
                        .unwrap_or_else(|| (fontdue::Metrics::default(), Vec::new()));
                    self.glyphs.insert(key, raster);
                }
                let Some((metrics, bitmap)) = self.glyphs.get(&key) else {
                    cursor += if character == '\t' {
                        tab_advance
                    } else if face.is_mono() {
                        cell_advance
                    } else {
                        0.0
                    };
                    continue;
                };
                let gx = (cursor + metrics.xmin as f32).round() as i32;
                let gy = (baseline - metrics.ymin as f32 - metrics.height as f32).round() as i32;
                for row in 0..metrics.height {
                    let py = gy + row as i32;
                    if py < 0 || py >= height as i32 {
                        continue;
                    }
                    let py = py as usize;
                    // Synthetic oblique: every glyph row is shifted by its
                    // distance above the baseline, so no italic font is needed.
                    let lean = if italic {
                        ((baseline - (gy + row as i32) as f32) * 0.20).round().max(0.0) as i32
                    } else {
                        0
                    };
                    for column in 0..metrics.width {
                        let alpha = bitmap[row * metrics.width + column];
                        if alpha == 0 {
                            continue;
                        }
                        let px_x = gx + column as i32 + lean;
                        if px_x < 0 || px_x >= width as i32 {
                            continue;
                        }
                        let index = py * width + px_x as usize;
                        pixels[index] = blend(pixels[index], rgb, alpha);
                    }
                }
                cursor += if character == '\t' {
                    tab_advance
                } else if face.is_mono() {
                    cell_advance
                } else {
                    metrics.advance_width
                };
            }
            return;
        }

        let chars: Vec<char> = text.chars().collect();
        let mut i = 0usize;
        while i < chars.len() {
            let character = chars[i];
            if character == '\r' || character == '\u{fe0f}' || character == '\u{fe0e}' {
                i += 1;
                continue;
            }
            let cluster_len = emoji_cluster_len(&chars, i);
            if cluster_len > 0 {
                let emoji_str: String = chars[i..i + cluster_len].iter().collect();
                let px_key = px.round() as u32;
                let key = (emoji_str.clone(), px_key);
                if !self.emojis.contains_key(&key) {
                    if let Some((ew, eh, epixels)) = crate::jni_bridge::rasterize_emoji(&emoji_str, px) {
                        self.emojis.insert(key.clone(), EmojiRaster {
                            width: ew,
                            height: eh,
                            pixels: epixels,
                        });
                    }
                }
                if let Some(raster) = self.emojis.get(&key) {
                    // Layout measures a proportional emoji as one `px` square;
                    // keep the glyph advance identical so the caret stays put.
                    let slot_w = if face.is_mono() {
                        2.0 * cell_advance
                    } else {
                        px
                    };
                    let offset_x = ((slot_w - raster.width as f32) / 2.0).max(0.0);
                    let gx = (cursor + offset_x).round() as i32;
                    let gy = (baseline - raster.height as f32 * 0.85).round() as i32;
                    for row in 0..raster.height {
                        let py = gy + row as i32;
                        if py < 0 || py >= height as i32 {
                            continue;
                        }
                        for col in 0..raster.width {
                            let px_x = gx + col as i32;
                            if px_x < 0 || px_x >= width as i32 {
                                continue;
                            }
                            let p = raster.pixels[row * raster.width + col];
                            let alpha = ((p >> 24) & 0xff) as u8;
                            if alpha == 0 {
                                continue;
                            }
                            let pr = (p >> 16) & 0xff;
                            let pg = (p >> 8) & 0xff;
                            let pb = p & 0xff;
                            let color = (pr << 16) | (pg << 8) | pb;
                            let index = py as usize * width + px_x as usize;
                            pixels[index] = blend(pixels[index], color, alpha);
                        }
                    }
                    cursor += slot_w;
                    i += cluster_len;
                    continue;
                }
            }
            let key = (face as u8, character, px.round() as u32);
            if !self.glyphs.contains_key(&key) {
                let raster = self
                    .font_for_char(character, face)
                    .map(|font| font.rasterize(character, px))
                    .unwrap_or_else(|| (fontdue::Metrics::default(), Vec::new()));
                self.glyphs.insert(key, raster);
            }
            let Some((metrics, bitmap)) = self.glyphs.get(&key) else {
                i += 1;
                continue;
            };
            let cjk = is_cjk(character);
            let slot_w = if face.is_mono() && cjk {
                2.0 * cell_advance
            } else if face.is_mono() {
                cell_advance
            } else {
                metrics.advance_width
            };
            let offset_x = if face.is_mono() && cjk {
                ((slot_w - metrics.advance_width) / 2.0).max(0.0)
            } else {
                0.0
            };
            let gx = (cursor + metrics.xmin as f32 + offset_x).round() as i32;
            let gy = (baseline - metrics.ymin as f32 - metrics.height as f32).round() as i32;
            for row in 0..metrics.height {
                let py = gy + row as i32;
                if py < 0 || py >= height as i32 {
                    continue;
                }
                let py = py as usize;
                let lean = if italic {
                    ((baseline - (gy + row as i32) as f32) * 0.20).round().max(0.0) as i32
                } else {
                    0
                };
                for column in 0..metrics.width {
                    let alpha = bitmap[row * metrics.width + column];
                    if alpha == 0 {
                        continue;
                    }
                    let px_x = gx + column as i32 + lean;
                    if px_x < 0 || px_x >= width as i32 {
                        continue;
                    }
                    let index = py * width + px_x as usize;
                    pixels[index] = blend(pixels[index], rgb, alpha);
                }
            }
            cursor += if face.is_mono() {
                if cjk { 2.0 * cell_advance } else { cell_advance }
            } else {
                metrics.advance_width
            };
            i += 1;
        }
    }
}

/// Top of a text box that vertically centers the font's ascent and descent in
/// a row. Accounts for descenders, unlike a fixed top pad.
fn centered_text_y(painter: &mut Painter, y: f32, row_h: f32, font: f32, face: Face) -> f32 {
    let (ascent, descent) = painter.font_line_span(font, face);
    y + ((row_h - (ascent + descent)) * 0.5).max(0.0)
}

#[derive(Debug, Clone, Deserialize, Default)]
struct TreeRow {
    #[serde(default)]
    name: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    folder: bool,
    #[serde(default)]
    depth: usize,
    #[serde(default)]
    expanded: bool,
    #[serde(default)]
    has_children: bool,
    #[serde(default)]
    note: bool,
    #[serde(default)]
    extension: String,
    /// First matching character in the displayed name; `match_len` 0 = none.
    #[serde(default)]
    match_start: usize,
    #[serde(default)]
    match_len: usize,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct TreeDoc {
    #[serde(default)]
    rows: Vec<TreeRow>,
    #[serde(default)]
    active: Option<String>,
}

#[derive(Default)]
struct TreeState {
    doc: TreeDoc,
    scroll: f32,
    scroll_x: f32,
    drag: Option<TreeDrag>,
}

#[derive(Clone, Copy)]
struct TreeDrag {
    start_x: f32,
    start_y: f32,
    start_scroll_x: f32,
    start_scroll: f32,
    tap: bool,
    horizontal: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default)]
struct WrapLine {
    line: usize,
    start: usize,
    end: usize,
    font: f32,
    /// Row height in device pixels. Headings that outgrow the body pitch get
    /// a taller row, the same rule the desktop layout uses.
    height: f32,
}

const HISTORY_CAP: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EditSelection {
    cursor: usize,
    anchor: usize,
}

/// One atomic text mutation in char offsets.
#[derive(Clone, Debug)]
struct TextChange {
    at: usize,
    removed: String,
    inserted: String,
}

#[derive(Clone, Debug)]
struct UndoStep {
    changes: Vec<TextChange>,
    sel_before: EditSelection,
    sel_after: EditSelection,
}

/// Undo/redo stacks mirroring the desktop `session_core::undo`: atomic steps
/// without coalescing, redo cleared on a new edit, and a pure deletion that
/// exactly reverts the last insertion annuls both steps.
#[derive(Default)]
struct History {
    undo: VecDeque<UndoStep>,
    redo: VecDeque<UndoStep>,
}

impl History {
    fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    fn record(&mut self, step: UndoStep) {
        if let (Some(last), [change]) = (self.undo.back(), step.changes.as_slice()) {
            if last.changes.len() == 1 {
                let previous = &last.changes[0];
                if previous.removed.is_empty()
                    && !previous.inserted.is_empty()
                    && change.inserted.is_empty()
                    && !change.removed.is_empty()
                    && change.at == previous.at
                    && change.removed == previous.inserted
                {
                    self.undo.pop_back();
                    self.redo.clear();
                    return;
                }
            }
        }
        self.redo.clear();
        self.undo.push_back(step);
        while self.undo.len() > HISTORY_CAP {
            self.undo.pop_front();
        }
    }

    fn take(&mut self, redo: bool) -> Option<UndoStep> {
        let (from, to) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        let step = from.pop_back()?;
        to.push_back(step.clone());
        Some(step)
    }

    fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[derive(Default)]
struct EditorState {
    text: String,
    selection: usize,
    anchor: usize,
    scroll: f32,
    scroll_x: f32,
    band_origin: f32,
    language: String,
    drag: Option<(f32, f32, f32, f32, bool)>,
    model: Vec<crate::markdown::MarkdownLine>,
    model_dirty: bool,
    wrap: Vec<WrapLine>,
    wrap_first: Vec<usize>,
    wrap_count: Vec<usize>,
    /// Cumulative top offset per visual row and the total content height.
    wrap_tops: Vec<f32>,
    wrap_height: f32,
    /// Canonical wrap (all markup hidden) the active-line exceptions build on.
    wrap_canonical: Vec<WrapLine>,
    wrap_canonical_first: Vec<usize>,
    wrap_canonical_count: Vec<usize>,
    wrap_canonical_tops: Vec<f32>,
    wrap_canonical_height: f32,
    /// Caret line, selection and visible line window the effective wrap was
    /// built for.
    wrap_paint_key: (usize, usize, usize, usize, usize),
    wrap_has_exceptions: bool,
    wrap_width: f32,
    wrap_dirty: bool,
    bottom_inset: f32,
    top_inset: f32,
    caret_visible: bool,
    insertion_handle_visible: bool,
    preview: bool,
    starts: Vec<usize>,
    byte_starts: Vec<usize>,
    starts_dirty: bool,
    /// Measured table geometry keyed by the table's header line. Valid while
    /// `table_layout_width`/`table_layout_font` still match the current wrap.
    table_layouts: HashMap<usize, TableLayout>,
    table_layout_width: f32,
    table_layout_font: f32,
    /// Code blocks the reader folded in reading mode, keyed by the fence
    /// block id. Long blocks start folded; a tap on their header unfolds them.
    code_collapsed: HashSet<usize>,
    /// One entry per source line: the callout block that covers it, if any.
    /// Built with the model so a frame never re-derives it.
    callout_blocks: Vec<Option<crate::markdown::CalloutBlock>>,
    /// Tables the user switched to raw source with the header button. Every
    /// other table is a grid and edits its cells in place.
    raw_tables: HashSet<usize>,
    /// Tables touched by a non-empty selection; they show their source until
    /// the selection collapses, like an active block on the desktop.
    selection_tables: HashSet<usize>,
    /// Horizontal pan of each wide table, keyed by its header line. One
    /// table sliding must not move the others.
    table_scroll: HashMap<usize, f32>,
    /// Character ranges of the active in-page find query. Painted behind the
    /// text; the range at `find_active` uses the accent colour.
    find_ranges: Vec<(usize, usize)>,
    find_active: Option<usize>,
    history: History,
}

struct SurfaceState {
    window: *mut ANativeWindow,
    width: usize,
    height: usize,
    /// Real viewport height. `height` is temporarily reduced while a cache
    /// band is painted, so layout math must not read it.
    viewport_height: usize,
    density: f32,
    /// Editor body text size in dp, scaled by [`SurfaceState::density`].
    editor_font_dp: f32,
    /// Editor line pitch in dp; derived from the font size when it changes.
    editor_line_dp: f32,
    /// Size of fenced code in dp. Independent from the body size, so code can
    /// be set smaller than prose.
    code_font_dp: f32,
    /// Face used for Markdown prose (and other non-code text).
    text_face: Face,
    /// Face used for fenced/inline code and for code files.
    code_face: Face,
    /// Per-level heading size, colour and format pushed by the settings screen.
    headings: [HeadingStyle; 6],
    /// Tints the caret line while editing, like the desktop editor. Paint-only
    /// state: toggling it never re-wraps the document.
    current_line_highlight: bool,
    /// Extends the gutter wash across every visual row of a wrapped caret
    /// line. Off by default: only the row holding the caret is tinted.
    gutter_wrapped_line_highlight: bool,
    background: u32,
    painter: Painter,
    tree: TreeState,
    editor: EditorState,
    pixels: Vec<u32>,
    frame_valid: bool,
    suspend_render: bool,
    /// 0 = none, 1 = tree, 2 = editor. Used to restore the background when a
    /// palette arrives after the surface was attached.
    content_kind: u8,
    last_mode: u8,
    last_scroll: f32,
    last_width: usize,
    last_height: usize,
    /// Gutter width of the previous frame; a change at a digit boundary is
    /// logged because it also re-wraps the whole document.
    last_gutter: f32,
}

unsafe impl Send for SurfaceState {}

fn surfaces() -> &'static Mutex<HashMap<i32, Arc<Mutex<SurfaceState>>>> {
    static SURFACES: OnceLock<Mutex<HashMap<i32, Arc<Mutex<SurfaceState>>>>> = OnceLock::new();
    SURFACES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Lock-free channel between the UI/input thread and the render thread.
/// Touch moves must never wait on a paint in progress; otherwise the scroll
/// advances in bursts and the motion stutters.
struct InputState {
    density_bits: AtomicU32,
    mode: AtomicU8,
    drag_active: AtomicBool,
    moved: AtomicBool,
    start_x_bits: AtomicU32,
    start_y_bits: AtomicU32,
    start_scroll_bits: AtomicU32,
    start_scroll_x_bits: AtomicU32,
    last_y_bits: AtomicU32,
    last_x_bits: AtomicU32,
    scroll_bits: AtomicU32,
    scroll_x_bits: AtomicU32,
    scroll_dirty: AtomicBool,
    scroll_x_dirty: AtomicBool,
    max_y_bits: AtomicU32,
    max_x_bits: AtomicU32,
    /// 0 = undecided, 1 = vertical, 2 = horizontal.
    axis: AtomicU8,
}

impl InputState {
    fn new(density: f32) -> Self {
        Self {
            density_bits: AtomicU32::new(density.to_bits()),
            mode: AtomicU8::new(0),
            drag_active: AtomicBool::new(false),
            moved: AtomicBool::new(false),
            start_x_bits: AtomicU32::new(0),
            start_y_bits: AtomicU32::new(0),
            start_scroll_bits: AtomicU32::new(0),
            start_scroll_x_bits: AtomicU32::new(0),
            last_y_bits: AtomicU32::new(0),
            last_x_bits: AtomicU32::new(0),
            scroll_bits: AtomicU32::new(0),
            scroll_x_bits: AtomicU32::new(0),
            scroll_dirty: AtomicBool::new(false),
            scroll_x_dirty: AtomicBool::new(false),
            max_y_bits: AtomicU32::new(0),
            max_x_bits: AtomicU32::new(0),
            axis: AtomicU8::new(0),
        }
    }
}

fn inputs() -> &'static Mutex<HashMap<i32, Arc<InputState>>> {
    static INPUTS: OnceLock<Mutex<HashMap<i32, Arc<InputState>>>> = OnceLock::new();
    INPUTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn input_of(id: i32) -> Option<Arc<InputState>> {
    inputs().lock().ok()?.get(&id).cloned()
}

/// Applies a pending lock-free scroll request into the state, if any.
fn apply_pending_scroll(input: &InputState, state: &mut SurfaceState) {
    let mode = input.mode.load(Ordering::Relaxed);
    if input.scroll_dirty.swap(false, Ordering::Relaxed) {
        let y = f32::from_bits(input.scroll_bits.load(Ordering::Relaxed));
        match mode {
            1 => state.tree_set_scroll(y),
            _ => state.editor_set_scroll(y, 0.0),
        }
    }
    if mode == 1 && input.scroll_x_dirty.swap(false, Ordering::Relaxed) {
        let x = f32::from_bits(input.scroll_x_bits.load(Ordering::Relaxed));
        state.tree_set_scroll_x(x);
    }
}

fn drain_input(id: i32) {
    let Some(input) = input_of(id) else {
        return;
    };
    let _ = with_surface(id, |state| apply_pending_scroll(&input, state));
}

fn row_height(density: f32) -> f32 {
    (36.0 * density).round().max(1.0)
}

/// Characters of a row name that fit [max_width] while leaving room for the
/// trailing ellipsis. Returns the number of characters to draw before it.
fn fit_name_chars(painter: &mut Painter, chars: &[char], font: f32, max_width: f32) -> usize {
    let ellipsis_width = painter.width("…", font, Face::Ui);
    if max_width <= ellipsis_width {
        return 0;
    }
    let budget = max_width - ellipsis_width;
    let mut used = 0.0;
    let mut count = 0;
    let mut glyph = String::new();
    for ch in chars {
        glyph.clear();
        glyph.push(*ch);
        let advance = painter.width(&glyph, font, Face::Ui);
        if used + advance > budget {
            break;
        }
        used += advance;
        count += 1;
    }
    // Never leave a space dangling in front of the ellipsis.
    while count > 0 && chars[count - 1] == ' ' {
        count -= 1;
    }
    count
}

/// Width of the file-type pill painted on the right of a row, or 0 when the
/// row shows none. Bitmaps (anything but a folder or markdown) get one.
fn tree_tag_width(painter: &mut Painter, row: &TreeRow, density: f32) -> f32 {
    let extension = row.extension.to_ascii_uppercase();
    if row.folder || extension.is_empty() || extension == "MD" {
        0.0
    } else {
        painter.width(&extension, 9.0 * density, Face::Ui) + 10.0 * density
    }
}

/// Body text size the editor layout was tuned at; settings scale from this.
const EDITOR_FONT_DP: f32 = 15.0;

/// Line pitch that goes with [`EDITOR_FONT_DP`].
const EDITOR_LINE_DP: f32 = 22.0;

/// Default size of fenced code, one step under the body text so a block reads
/// as a quotation of code rather than as prose.
const CODE_FONT_DP: f32 = 13.0;

/// Empty space on each side of the line-number column, in dp.
const GUTTER_PAD_DP: f32 = 8.0;

/// Smallest line-number column, so a one-digit file still has room to breathe.
const GUTTER_MIN_DP: f32 = 24.0;

/// Per-level heading style pushed from the settings screen. `size_dp` is stated
/// at [`EDITOR_FONT_DP`] and scaled with the configured body size, matching the
/// desktop typography; `color` is opaque RGB.
#[derive(Clone, Copy, Debug, PartialEq)]
struct HeadingStyle {
    size_dp: f32,
    color: u32,
    bold: bool,
    italic: bool,
}

/// Defaults keep the sizes and palette the editor was tuned for.
const DEFAULT_HEADINGS: [HeadingStyle; 6] = [
    HeadingStyle { size_dp: 19.0, color: 0x0072b2, bold: true, italic: false },
    HeadingStyle { size_dp: 18.0, color: 0x56b4e9, bold: true, italic: false },
    HeadingStyle { size_dp: 17.0, color: 0x009e73, bold: true, italic: false },
    HeadingStyle { size_dp: 16.0, color: 0xe69f00, bold: true, italic: false },
    HeadingStyle { size_dp: 15.0, color: 0xd55e00, bold: true, italic: false },
    HeadingStyle { size_dp: 14.0, color: 0xcc79a7, bold: true, italic: false },
];

fn heading_at(headings: &[HeadingStyle; 6], level: u8) -> HeadingStyle {
    headings[(level.clamp(1, 6) - 1) as usize]
}

/// Row height for text drawn at `font_px`. Body text keeps the configured
/// pitch; a heading tall enough to need more room grows its row on the same
/// 4 dp grid the desktop layout uses.
fn visual_line_height(font_px: f32, base_line_px: f32, density: f32) -> f32 {
    let unrounded = (font_px * 1.35).max(base_line_px);
    if (unrounded - base_line_px).abs() < 0.5 {
        return base_line_px;
    }
    let step = (4.0 * density).max(1.0);
    ((unrounded / step).ceil() * step).max(base_line_px)
}

/// Horizontal alignment a table column asks for in its separator row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TableAlign {
    Left,
    Center,
    Right,
}

/// Measured geometry of one markdown table, shared by all of its rows so the
/// columns stay aligned while the table scrolls.
#[derive(Clone, Debug, Default)]
struct TableLayout {
    cols: usize,
    aligns: Vec<TableAlign>,
    /// Painted width of each column in device pixels, padding included.
    widths: Vec<f32>,
    total: f32,
    /// Left offset the table inherits from its source indentation, so a table
    /// nested in a list stays under its item instead of jumping to the margin.
    indent: f32,
}

fn is_table_kind(kind: &str) -> bool {
    matches!(kind, "table" | "table_row" | "table_separator")
}

/// First and last source line of the table block containing `line`.
fn table_bounds(model: &[crate::markdown::MarkdownLine], line: usize) -> Option<(usize, usize)> {
    if !model
        .get(line)
        .map(|row| is_table_kind(&row.kind))
        .unwrap_or(false)
    {
        return None;
    }
    let mut start = line;
    while start > 0 && is_table_kind(&model[start - 1].kind) {
        start -= 1;
    }
    let mut end = line;
    while end + 1 < model.len() && is_table_kind(&model[end + 1].kind) {
        end += 1;
    }
    Some((start, end))
}

/// Header line of the table containing `line`, if any.
fn table_header_at(model: &[crate::markdown::MarkdownLine], line: usize) -> Option<usize> {
    table_bounds(model, line).map(|(start, _)| start)
}

fn table_align(cell: &str) -> TableAlign {
    let trimmed = cell.trim();
    match (trimmed.starts_with(':'), trimmed.ends_with(':')) {
        (true, true) => TableAlign::Center,
        (false, true) => TableAlign::Right,
        _ => TableAlign::Left,
    }
}

/// Outer char ranges of the cells between the row's pipes, following GFM:
/// escaped `\|` does not split, and leading/trailing pipes are optional.
fn table_cell_bounds(line: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = line.chars().collect();
    let n = chars.len();
    let mut pipes = Vec::new();
    let mut i = 0usize;
    while i < n {
        match chars[i] {
            '\\' => i += 2,
            '|' => {
                pipes.push(i);
                i += 1;
            }
            _ => i += 1,
        }
    }
    let first_ns = chars.iter().position(|c| !c.is_whitespace()).unwrap_or(n);
    let last_ns = chars.iter().rposition(|c| !c.is_whitespace());
    let lead = pipes.first() == Some(&first_ns);
    let trail =
        last_ns.is_some() && pipes.last().copied() == last_ns && (pipes.len() > 1 || !lead);
    let mut bounds = Vec::new();
    let mut seg_start = if lead { first_ns + 1 } else { 0 };
    for &p in pipes.iter().skip(lead as usize) {
        bounds.push((seg_start, p));
        seg_start = p + 1;
    }
    if !trail {
        bounds.push((seg_start.min(n), n));
    }
    bounds
}

/// Text typed into a grid cell never breaks the row: a pipe is escaped and a
/// newline becomes a space (desktop turns it into `<br>` for its renderer).
fn sanitize_table_cell_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_backslash = false;
    for ch in text.replace("\r\n", "\n").chars() {
        match ch {
            '|' if !prev_backslash => out.push_str("\\|"),
            '\n' | '\r' => out.push(' '),
            other => out.push(other),
        }
        prev_backslash = ch == '\\' && !prev_backslash;
    }
    out
}

/// Content ranges of the cells, trimmed of the padding spaces. An empty cell
/// gets a point one char inside its outer range, exactly like the desktop
/// `split_row` model.
fn table_cell_ranges(line: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = line.chars().collect();
    table_cell_bounds(line)
        .into_iter()
        .map(|(s, e)| {
            let mut cs = s;
            let mut ce = e;
            while cs < ce && chars[cs].is_whitespace() {
                cs += 1;
            }
            while ce > cs && chars[ce - 1].is_whitespace() {
                ce -= 1;
            }
            if cs == ce {
                let point = (s + 1).min(e);
                (point, point)
            } else {
                (cs, ce)
            }
        })
        .collect()
}

/// Outer ranges, padding included. The caret uses them so a trailing space
/// typed into a cell still belongs to that cell.
fn table_cell_wide_ranges(line: &str) -> Vec<(usize, usize)> {
    table_cell_bounds(line)
}

/// Column of a tap inside a table row and its left box offset. A row can have
/// fewer cells than the table has columns; a tap in those phantom columns
/// belongs to the row's last real cell, never to the start of the line.
fn table_column_at(local: f32, widths: &[f32], actual_cols: usize) -> (usize, f32) {
    let cols = actual_cols.min(widths.len()).max(1);
    let mut cell_x = 0.0f32;
    for col in 0..cols {
        let width = widths.get(col).copied().unwrap_or(0.0);
        if local < cell_x + width || col + 1 == cols {
            return (col, cell_x);
        }
        cell_x += width;
    }
    (cols - 1, cell_x)
}

/// Painted x of the caret inside a table row, including the padding spaces of
/// the cell. Returns `None` only when the layout has no cell at all.
#[allow(clippy::too_many_arguments)]
fn table_caret_x(
    painter: &mut Painter,
    model: &[crate::markdown::MarkdownLine],
    line: usize,
    layout: &TableLayout,
    cursor: usize,
    text_x: f32,
    pad: f32,
    font: f32,
    face: Face,
) -> f32 {
    let bounds = table_cell_bounds(&model[line].raw);
    let contents = table_cell_ranges(&model[line].raw);
    let chars: Vec<char> = model[line].raw.chars().collect();
    let mut cell_x = text_x;
    let mut last_text_end = text_x + pad * 0.5;
    for col in 0..layout.cols {
        let width = layout.widths.get(col).copied().unwrap_or(0.0);
        let (outer_start, outer_end) = bounds.get(col).copied().unwrap_or((0, 0));
        let (content_start, _content_end) = contents
            .get(col)
            .copied()
            .unwrap_or((outer_start, outer_start));
        let full = cell_visible_text(
            model[line]
                .cells
                .get(col)
                .map(String::as_str)
                .unwrap_or(""),
        );
        let inner = (width - pad).max(0.0);
        let text_width = painter.width(&full, font, face);
        let align_offset = match layout.aligns.get(col).copied().unwrap_or(TableAlign::Left) {
            TableAlign::Left => 0.0,
            TableAlign::Center => ((inner - text_width) * 0.5).max(0.0),
            TableAlign::Right => (inner - text_width).max(0.0),
        };
        let text_origin = cell_x + pad * 0.5 + align_offset;
        last_text_end = text_origin + text_width;
        if cursor >= outer_start && cursor <= outer_end {
            // An empty cell's caret sits at the cell's text start; padding
            // spaces never pull it towards the right border.
            if full.is_empty() || cursor <= content_start {
                return text_origin;
            }
            let end = cursor.min(outer_end);
            let from = content_start.min(end);
            let prefix: String = chars[from..end].iter().collect();
            return text_origin + painter.width(&cell_visible_text(&prefix), font, face);
        }
        cell_x += width;
    }
    // The caret is at a pipe or the row end: stay at the last cell's text end
    // instead of jumping to the table border.
    last_text_end
}

/// Maps a tap inside one table cell to a source char offset in its raw line.
///
/// The mapping follows the painted text, not the raw characters: alignment
/// moves where the text starts, and inline markers take no painted width.
/// Tapping left of the text snaps to the cell start, right of it to the cell
/// end (or to the end of the row for the last column).
#[allow(clippy::too_many_arguments)]
fn cell_tap_offset(
    painter: &mut Painter,
    cell_raw: &str,
    range: (usize, usize),
    width: f32,
    pad: f32,
    align: TableAlign,
    target_x: f32,
    font: f32,
    face: Face,
) -> usize {
    use crate::markdown::SyntaxSpan;
    let visible = cell_visible_text(cell_raw);
    let full_width = painter.width(&visible, font, face);
    let inner = (width - pad).max(0.0);
    let align_offset = match align {
        TableAlign::Left => 0.0,
        TableAlign::Center => ((inner - full_width) * 0.5).max(0.0),
        TableAlign::Right => (inner - full_width).max(0.0),
    };
    let text_start = pad * 0.5 + align_offset;
    if target_x <= text_start {
        return range.0;
    }
    if target_x >= text_start + full_width {
        // A tap right of the painted text stays at the end of the cell's
        // content; it never moves past the trailing pipe.
        return range.1;
    }
    // Painted char under the target, by cumulative advances.
    let mut advance_sum = 0.0f32;
    let mut visible_index = 0usize;
    for character in visible.chars() {
        let advance = painter.advance_of_char(character, font, face);
        if target_x < text_start + advance_sum + advance * 0.5 {
            break;
        }
        advance_sum += advance;
        visible_index += 1;
    }
    // Map the painted index back through the inline markers.
    let mut raw_consumed = 0usize;
    let mut painted = 0usize;
    for span in crate::markdown::parse_inline_syntax_spans(cell_raw) {
        let text = match &span {
            SyntaxSpan::Marker(text)
            | SyntaxSpan::Text(text)
            | SyntaxSpan::Bold(text)
            | SyntaxSpan::Italic(text)
            | SyntaxSpan::BoldItalic(text)
            | SyntaxSpan::InlineCode(text)
            | SyntaxSpan::Strikethrough(text)
            | SyntaxSpan::Underline(text)
            | SyntaxSpan::Highlight(text)
            | SyntaxSpan::Tag(text) => text.as_str(),
            SyntaxSpan::Link { text, .. } => text.as_str(),
            SyntaxSpan::Grammar { text, .. } => text.as_str(),
        };
        let count = text.chars().count();
        if matches!(span, SyntaxSpan::Marker(_)) {
            raw_consumed += count;
            continue;
        }
        if painted + count >= visible_index {
            raw_consumed += visible_index - painted;
            break;
        }
        painted += count;
        raw_consumed += count;
    }
    range.0 + raw_consumed.min(range.1.saturating_sub(range.0))
}

/// A cell as the reader sees it: inline markup stripped, link text kept.
fn cell_visible_text(cell: &str) -> String {
    use crate::markdown::SyntaxSpan;
    let mut out = String::new();
    for span in crate::markdown::parse_inline_syntax_spans(cell) {
        match span {
            SyntaxSpan::Marker(_) => {}
            SyntaxSpan::Text(text)
            | SyntaxSpan::Bold(text)
            | SyntaxSpan::Italic(text)
            | SyntaxSpan::BoldItalic(text)
            | SyntaxSpan::InlineCode(text)
            | SyntaxSpan::Strikethrough(text)
            | SyntaxSpan::Underline(text)
            | SyntaxSpan::Highlight(text)
            | SyntaxSpan::Tag(text) => out.push_str(&text),
            SyntaxSpan::Link { text, .. } => out.push_str(&text),
            SyntaxSpan::Grammar { text, .. } => out.push_str(&text),
        }
    }
    out
}

/// Measures every column width once per table and distributes the row to the
/// full content width. A table narrower than the viewport spreads its columns;
/// one that is too wide shrinks them proportionally so it still fits a phone.
#[allow(clippy::too_many_arguments)]
fn build_table_layout(
    painter: &mut Painter,
    model: &[crate::markdown::MarkdownLine],
    start: usize,
    end: usize,
    content_width: f32,
    font: f32,
    face: Face,
    density: f32,
) -> TableLayout {
    let mut cols = 0usize;
    for row in start..=end {
        cols = cols.max(model[row].cells.len());
    }
    if cols == 0 {
        return TableLayout::default();
    }
    let mut aligns = vec![TableAlign::Left; cols];
    if start + 1 <= end && model[start + 1].kind == "table_separator" {
        for (index, cell) in model[start + 1].cells.iter().take(cols).enumerate() {
            aligns[index] = table_align(cell);
        }
    }
    // A table indented inside a list item keeps that offset; its columns share
    // what is left of the content width.
    let indent_chars = model[start]
        .raw
        .chars()
        .take_while(|character| *character == ' ' || *character == '\t')
        .count();
    let indent = painter.width(&" ".repeat(indent_chars), font, face);
    let available = (content_width - indent).max(content_width * 0.5);
    let pad = (12.0 * density).round();
    let mut widths = vec![0.0f32; cols];
    for row in start..=end {
        if model[row].kind == "table_separator" {
            continue;
        }
        for (index, cell) in model[row].cells.iter().take(cols).enumerate() {
            let text = cell_visible_text(cell);
            let measured = painter.width(&text, font, face) + pad;
            widths[index] = widths[index].max(measured);
        }
    }
    let min = (28.0 * density).round();
    for width in widths.iter_mut() {
        *width = width.max(min);
    }
    let total: f32 = widths.iter().sum();
    // A table wider than the viewport keeps its measured columns and scrolls
    // horizontally instead of squeezing the text; a narrow one spreads out to
    // fill the row.
    if total < available {
        let extra = (available - total) / cols as f32;
        for width in widths.iter_mut() {
            *width += extra;
        }
    }
    let total = widths.iter().sum();
    TableLayout {
        cols,
        aligns,
        widths,
        total,
        indent,
    }
}

/// Paints one source line of a table: the header row, the rule that closes it
/// and a body row. The separator source line itself paints nothing — its rule
/// is the header's bottom border.
#[allow(clippy::too_many_arguments)]
fn paint_table_row(
    pixels: &mut [u32],
    surface_width: usize,
    surface_height: usize,
    painter: &mut Painter,
    model: &[crate::markdown::MarkdownLine],
    line: usize,
    layout: &TableLayout,
    x_shift: f32,
    cursor_char: Option<usize>,
    text_x: f32,
    y: f32,
    row_h: f32,
    font: f32,
    face: Face,
    density: f32,
    palette: &SurfacePalette,
) {
    if layout.cols == 0 || layout.total <= 0.0 {
        return;
    }
    let kind = model[line].kind.as_str();
    if kind == "table_separator" {
        return;
    }
    let header = kind == "table";
    let text_x = text_x + layout.indent - x_shift;
    let hair = (1.0 * density).max(1.0);
    let pad = (12.0 * density).round();
    let text_y = centered_text_y(painter, y, row_h, font, face);
    if header {
        rect(
            pixels,
            surface_width,
            surface_height,
            text_x,
            y,
            layout.total,
            row_h,
            palette.surface_alt,
        );
    }
    let mut cell_x = text_x;
    for col in 0..layout.cols {
        let width = layout.widths.get(col).copied().unwrap_or(0.0);
        let cell = model[line]
            .cells
            .get(col)
            .map(String::as_str)
            .unwrap_or("");
        let text = cell_visible_text(cell);
        let inner = (width - pad).max(0.0);
        let mut shown = text;
        let mut text_width = painter.width(&shown, font, face);
        if text_width > inner && !shown.is_empty() {
            let ellipsis = painter.advance_of_char('…', font, face);
            let mut fitted = String::new();
            let mut used = 0.0f32;
            for character in shown.chars() {
                let advance = painter.advance_of_char(character, font, face);
                if used + advance > (inner - ellipsis).max(0.0) {
                    fitted.push('…');
                    break;
                }
                used += advance;
                fitted.push(character);
            }
            text_width = painter.width(&fitted, font, face);
            shown = fitted;
        }
        let align_offset = match layout.aligns.get(col).copied().unwrap_or(TableAlign::Left) {
            TableAlign::Left => 0.0,
            TableAlign::Center => ((inner - text_width) * 0.5).max(0.0),
            TableAlign::Right => (inner - text_width).max(0.0),
        };
        let x = cell_x + pad * 0.5 + align_offset;
        let color = if header { palette.heading } else { palette.text };
        painter.draw(
            pixels,
            surface_width,
            surface_height,
            x,
            text_y,
            &shown,
            font,
            color,
            face,
            false,
        );
        if header {
            painter.draw(
                pixels,
                surface_width,
                surface_height,
                x + (0.6 * density).round(),
                text_y,
                &shown,
                font,
                color,
                face,
                false,
            );
        }
        cell_x += width;
    }
    // Grid: column separators plus the rule under every row.
    let mut x = text_x;
    for col in 0..layout.cols {
        rect(
            pixels,
            surface_width,
            surface_height,
            x - hair * 0.5,
            y,
            hair,
            row_h,
            palette.border,
        );
        x += layout.widths.get(col).copied().unwrap_or(0.0);
    }
    rect(
        pixels,
        surface_width,
        surface_height,
        x - hair * 0.5,
        y,
        hair,
        row_h,
        palette.border,
    );
    rect(
        pixels,
        surface_width,
        surface_height,
        text_x,
        y + row_h - hair,
        layout.total,
        hair,
        palette.border,
    );
    if header {
        rect(
            pixels,
            surface_width,
            surface_height,
            text_x,
            y,
            layout.total,
            hair,
            palette.border,
        );
    }
    // The cell being edited shows a caret; the grid otherwise has no cursor.
    if let Some(cursor) = cursor_char {
        let caret_x = table_caret_x(
            painter,
            model,
            line,
            layout,
            cursor,
            text_x,
            pad,
            font,
            face,
        );
        rect(
            pixels,
            surface_width,
            surface_height,
            caret_x,
            y + 2.0 * density,
            2.0 * density,
            (row_h - 4.0 * density).max(2.0),
            palette.accent,
        );
    }
}

/// Whether the theme background is dark, which picks the callout colour set.
fn palette_is_dark(palette: &SurfacePalette) -> bool {
    let rgb = palette.editor_background;
    let r = (rgb >> 16) & 0xff;
    let g = (rgb >> 8) & 0xff;
    let b = rgb & 0xff;
    r + g + b < 0x180
}

fn callout_rgb(kind: crate::markdown::CalloutKind, palette: &SurfacePalette) -> u32 {
    crate::markdown::callout_hex(kind, palette_is_dark(palette))
}

/// Callout extents for the whole document, computed once with the model.
fn callout_block_map(
    model: &[crate::markdown::MarkdownLine],
) -> Vec<Option<crate::markdown::CalloutBlock>> {
    let lines: Vec<String> = model.iter().map(|row| row.raw.clone()).collect();
    let states: Vec<Option<crate::markdown::CodeBlockState>> = model
        .iter()
        .map(|row| {
            if is_code_kind(&row.kind) {
                Some(crate::markdown::CodeBlockState {
                    kind: crate::markdown::CodeBlockKind::InsideCode,
                    lang: row.language.clone(),
                    block_id: row.block_id.unwrap_or(0),
                })
            } else {
                None
            }
        })
        .collect();
    crate::markdown::callout_blocks(&lines, &states)
}

/// First and last source line of the callout block covering `line`.
fn callout_block_extent(
    blocks: &[Option<crate::markdown::CalloutBlock>],
    line: usize,
) -> Option<(usize, usize)> {
    blocks.get(line).copied().flatten()?;
    let is_first = |index: usize| {
        blocks
            .get(index)
            .copied()
            .flatten()
            .map(|block| block.first)
            .unwrap_or(true)
    };
    let is_last = |index: usize| {
        blocks
            .get(index)
            .copied()
            .flatten()
            .map(|block| block.last)
            .unwrap_or(true)
    };
    let mut start = line;
    while start > 0 && !is_first(start) && blocks.get(start - 1).copied().flatten().is_some() {
        start -= 1;
    }
    let mut end = line;
    while end + 1 < blocks.len()
        && !is_last(end)
        && blocks.get(end + 1).copied().flatten().is_some()
    {
        end += 1;
    }
    Some((start, end))
}

/// Opaque mix of a colour over the editor background, used for the callout
/// wash: the desktop paints its translucent family colour over the same
/// surface, so pre-mixing keeps the frame cheap.
fn mixed_rgb(base: u32, over: u32, alpha: u8) -> u32 {
    let a = u32::from(alpha);
    let inv = 255 - a;
    let channel =
        |shift: u32| (((over >> shift) & 0xff) * a + ((base >> shift) & 0xff) * inv) / 255;
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

fn callout_wash_rgb(kind: crate::markdown::CalloutKind, palette: &SurfacePalette) -> u32 {
    let alpha = if palette_is_dark(palette) { 0x24 } else { 0x1a };
    mixed_rgb(
        palette.editor_background,
        callout_rgb(kind, palette),
        alpha,
    )
}

/// The whole callout block: a family-tinted wash with rounded ends and a
/// family-coloured bar down its left edge, the way the desktop draws it.
#[allow(clippy::too_many_arguments)]
fn paint_callout_block(
    pixels: &mut [u32],
    surface_width: usize,
    surface_height: usize,
    x: f32,
    top: f32,
    width: f32,
    height: f32,
    kind: crate::markdown::CalloutKind,
    palette: &SurfacePalette,
    density: f32,
) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let radius = (6.0 * density).round();
    rounded_box(
        pixels,
        surface_width,
        surface_height,
        x,
        top,
        width,
        height,
        radius,
        callout_wash_rgb(kind, palette),
    );
    let bar_width = (crate::markdown::CALLOUT_BAR_WIDTH * density)
        .round()
        .max(2.0);
    let bar_radius = (bar_width * 0.5).min(radius);
    rounded_box_radii(
        pixels,
        surface_width,
        surface_height,
        x,
        top,
        bar_width,
        height,
        // Only the outer (left) edge is rounded; the right edge stays flush
        // with the block text.
        [bar_radius, 0.0, 0.0, bar_radius],
        callout_rgb(kind, palette),
    );
}

/// Header glyph and, for an untitled callout, its type label. The raw marker
/// cells stay reserved as blanks so the title keeps its column.
#[allow(clippy::too_many_arguments)]
fn paint_callout_header(
    pixels: &mut [u32],
    surface_width: usize,
    surface_height: usize,
    painter: &mut Painter,
    raw: &str,
    callout: &crate::markdown::CalloutPrefix<'_>,
    kind: crate::markdown::CalloutKind,
    x: f32,
    y: f32,
    row_h: f32,
    font_px: f32,
    density: f32,
    palette: &SurfacePalette,
    titled: bool,
    face: Face,
) {
    let color = callout_rgb(kind, palette);
    let prefix_bytes = crate::markdown::quote_prefix(raw).map_or(0, |prefix| prefix.bytes);
    let prefix = &raw[..prefix_bytes.min(raw.len())];
    let prefix_width = painter.width(prefix, font_px, face);
    let icon_size = (font_px * 0.95).round().max(10.0 * density);
    let icon_x = x + prefix_width;
    let icon_y = y + ((row_h - icon_size) * 0.5).max(0.0);
    let thickness = (2.0 / 24.0 * icon_size).max(1.0 * density);
    painter.draw_stroke_icon(
        pixels,
        surface_width,
        surface_height,
        icon_x,
        icon_y,
        icon_size,
        thickness,
        callout_icon_paths(kind),
        color,
    );
    if !titled {
        let label = crate::markdown::callout_label(callout, "en");
        let label_px = (font_px * 0.82).max(9.0 * density);
        let label_x = icon_x + icon_size + (6.0 * density).round();
        let label_y = centered_text_y(painter, y, row_h, label_px, face);
        painter.draw(
            pixels,
            surface_width,
            surface_height,
            label_x,
            label_y,
            &label,
            label_px,
            color,
            face,
            false,
        );
        painter.draw(
            pixels,
            surface_width,
            surface_height,
            label_x + (0.6 * density).round(),
            label_y,
            &label,
            label_px,
            color,
            face,
            false,
        );
    }
}

/// Blocks longer than this start folded in reading mode; the header unfolds
/// them with a tap, so a long note does not turn into a wall of code.
const CODE_FOLD_LINES: usize = 12;

fn is_code_kind(kind: &str) -> bool {
    matches!(kind, "code" | "code_fence")
}

/// First and last source line of the fenced block containing `line`, plus the
/// block id the model assigns to every fence pair.
fn code_block_bounds(
    model: &[crate::markdown::MarkdownLine],
    line: usize,
) -> Option<(usize, usize)> {
    let row = model.get(line)?;
    if !is_code_kind(&row.kind) {
        return None;
    }
    let id = row.block_id?;
    let mut start = line;
    while start > 0 && model[start - 1].block_id == Some(id) {
        start -= 1;
    }
    let mut end = line;
    while end + 1 < model.len() && model[end + 1].block_id == Some(id) {
        end += 1;
    }
    Some((start, end))
}

fn code_block_line_count(
    model: &[crate::markdown::MarkdownLine],
    start: usize,
    end: usize,
) -> usize {
    (start..=end)
        .filter(|index| model.get(*index).map(|row| row.kind == "code").unwrap_or(false))
        .count()
}

/// Opening and closing fence lines of the block.
fn code_block_fences(
    model: &[crate::markdown::MarkdownLine],
    start: usize,
    end: usize,
) -> (usize, usize) {
    let open = (start..=end)
        .find(|index| {
            model
                .get(*index)
                .map(|row| row.kind == "code_fence" && row.is_opening == Some(true))
                .unwrap_or(false)
        })
        .unwrap_or(start);
    let close = (start..=end)
        .rev()
        .find(|index| {
            model
                .get(*index)
                .map(|row| row.kind == "code_fence" && row.is_opening == Some(false))
                .unwrap_or(false)
        })
        .unwrap_or(end);
    (open, close)
}

/// The code a block holds, ready for the clipboard. Every source line keeps
/// its own trailing newline, like the desktop copy button.
fn code_block_body(
    model: &[crate::markdown::MarkdownLine],
    start: usize,
    end: usize,
) -> String {
    let mut body = String::new();
    for index in start..=end {
        if let Some(row) = model.get(index) {
            if row.kind == "code" {
                body.push_str(&row.content);
                body.push('\n');
            }
        }
    }
    body
}

fn is_opening_fence(model: &[crate::markdown::MarkdownLine], line: usize) -> bool {
    model
        .get(line)
        .map(|row| row.kind == "code_fence" && row.is_opening == Some(true))
        .unwrap_or(false)
}

/// Reading-mode fold state: a long block is folded unless the reader unfolded
/// it, so the model rebuild only has to seed the ones that start folded.
fn default_folded_code(model: &[crate::markdown::MarkdownLine]) -> HashSet<usize> {
    let mut folded = HashSet::new();
    let mut index = 0usize;
    while index < model.len() {
        if is_code_kind(&model[index].kind) {
            if let Some((start, end)) = code_block_bounds(model, index) {
                if code_block_line_count(model, start, end) > CODE_FOLD_LINES {
                    if let Some(id) = model[start].block_id {
                        folded.insert(id);
                    }
                }
                index = end + 1;
                continue;
            }
        }
        index += 1;
    }
    folded
}

/// Header band of a fenced block: language on the left, copy control on the
/// right. It is taller than a code row so the controls get breathing room.
fn code_header_height(font_px: f32, base_line_px: f32, density: f32) -> f32 {
    visual_line_height(font_px, base_line_px, density) + (10.0 * density).round()
}

fn row_height_for(
    model: &[crate::markdown::MarkdownLine],
    line: usize,
    font_px: f32,
    header_font_px: f32,
    base_line_px: f32,
    density: f32,
) -> f32 {
    if is_opening_fence(model, line) {
        // The header carries the language label, which is sized from the body
        // font, so the row cannot follow the smaller code size.
        code_header_height(header_font_px, base_line_px, density)
    } else {
        // A code row keeps the body's text/line proportion at its own size, so
        // code set smaller than prose also takes less vertical space. The
        // closing fence is an empty row of the same block and follows it.
        let is_code = model
            .get(line)
            .map(|row| is_code_kind(&row.kind))
            .unwrap_or(false);
        let line_base = if is_code && header_font_px > 0.0 {
            (base_line_px * font_px / header_font_px).max(6.0)
        } else {
            base_line_px
        };
        visual_line_height(font_px, line_base, density)
    }
}

fn code_label_px(font_px: f32, density: f32) -> f32 {
    (font_px * 0.82).max(9.0 * density)
}

/// Copy control geometry inside a code header row.
fn code_copy_pill(
    painter: &mut Painter,
    text_x: f32,
    content_width: f32,
    label_px: f32,
    density: f32,
) -> (f32, f32, f32) {
    let text_width = painter.width("Copy", label_px, Face::Ui);
    let pad_x = (8.0 * density).round();
    let width = text_width + 2.0 * pad_x;
    let height = label_px + (8.0 * density).round();
    let x = text_x + content_width - (10.0 * density).round() - width;
    (x, width, height)
}

/// Source toggle geometry on a table header row: `</>` at the right edge.
fn table_source_button(
    painter: &mut Painter,
    text_x: f32,
    content_width: f32,
    label_px: f32,
    density: f32,
) -> (f32, f32, f32) {
    let text_width = painter.width("</>", label_px, Face::Ui);
    let pad_x = (6.0 * density).round();
    let width = text_width + 2.0 * pad_x;
    let height = label_px + (8.0 * density).round();
    let x = text_x + content_width - (4.0 * density).round() - width;
    (x, width, height)
}

#[allow(clippy::too_many_arguments)]
fn paint_table_source_button(
    pixels: &mut [u32],
    surface_width: usize,
    surface_height: usize,
    painter: &mut Painter,
    text_x: f32,
    content_width: f32,
    y: f32,
    row_h: f32,
    code_font: f32,
    density: f32,
    palette: &SurfacePalette,
) {
    let label_px = code_label_px(code_font, density);
    let (x, width, height) =
        table_source_button(painter, text_x, content_width, label_px, density);
    let by = y + ((row_h - height) * 0.5).max(0.0);
    rounded_box(
        pixels,
        surface_width,
        surface_height,
        x,
        by,
        width,
        height,
        height * 0.5,
        palette.border,
    );
    let text_width = painter.width("</>", label_px, Face::Ui);
    painter.draw(
        pixels,
        surface_width,
        surface_height,
        x + (width - text_width) * 0.5,
        by + ((height - label_px) * 0.5).max(0.0),
        "</>",
        label_px,
        palette.text,
        Face::Ui,
        false,
    );
}

/// Obsidian-style task checkbox: a rounded square with a thin border when
/// open, filled with the accent and a contrasting check when done.
#[allow(clippy::too_many_arguments)]
fn paint_task_checkbox(
    pixels: &mut [u32],
    surface_width: usize,
    surface_height: usize,
    x: f32,
    y: f32,
    row_h: f32,
    size: f32,
    checked: bool,
    palette: &SurfacePalette,
    density: f32,
) {
    let size = size.max(8.0 * density);
    let top = y + ((row_h - size) * 0.5).max(0.0);
    let radius = (3.0 * density).min(size * 0.5);
    if checked {
        rounded_box(
            pixels,
            surface_width,
            surface_height,
            x,
            top,
            size,
            size,
            radius,
            palette.accent,
        );
        let accent = palette.accent;
        let r = (accent >> 16) & 0xff;
        let g = (accent >> 8) & 0xff;
        let b = accent & 0xff;
        let check = if r + g + b > 0x1e0 { 0x000000 } else { 0xffffff };
        let thickness = (2.0 * density).max(1.0);
        let s = size;
        line(
            pixels,
            surface_width,
            surface_height,
            x + 0.26 * s,
            top + 0.52 * s,
            x + 0.44 * s,
            top + 0.70 * s,
            thickness,
            check,
        );
        line(
            pixels,
            surface_width,
            surface_height,
            x + 0.44 * s,
            top + 0.70 * s,
            x + 0.76 * s,
            top + 0.30 * s,
            thickness,
            check,
        );
    } else {
        let border = (1.5 * density).max(1.0);
        rounded_box(
            pixels,
            surface_width,
            surface_height,
            x,
            top,
            size,
            size,
            radius,
            palette.text_faint,
        );
        if size > border * 2.0 {
            rounded_box(
                pixels,
                surface_width,
                surface_height,
                x + border,
                top + border,
                size - border * 2.0,
                size - border * 2.0,
                (radius - border * 0.5).max(1.0),
                palette.surface_alt,
            );
        }
    }
}

/// Rounded rectangle with anti-aliased edges; `rect` only draws squares and
/// the block needs a real box.
///
/// The interior is filled one contiguous row slice at a time: a code block can
/// cover the whole viewport, and evaluating the corner distance field for
/// every pixel of it cost tens of milliseconds per frame. Only the rounded
/// bands and their two boundary pixels need the exact coverage.
#[allow(clippy::too_many_arguments)]
fn rounded_box(
    pixels: &mut [u32],
    surface_width: usize,
    surface_height: usize,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radius: f32,
    rgb: u32,
) {
    let r = radius.min(width * 0.5).min(height * 0.5).max(0.0);
    rounded_box_radii(
        pixels,
        surface_width,
        surface_height,
        x,
        y,
        width,
        height,
        [r, r, r, r],
        rgb,
    );
}

/// Rounded rectangle with a radius per corner: top-left, top-right,
/// bottom-right, bottom-left. A callout bar rounds only its left side, so its
/// right edge stays flush with the block text.
#[allow(clippy::too_many_arguments)]
fn rounded_box_radii(
    pixels: &mut [u32],
    surface_width: usize,
    surface_height: usize,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radii: [f32; 4],
    rgb: u32,
) {
    if width <= 0.0 || height <= 0.0 || surface_width == 0 {
        return;
    }
    let color = logical(rgb);
    let limit = (width * 0.5).min(height * 0.5).max(0.0);
    let [top_left, top_right, bottom_right, bottom_left] =
        radii.map(|radius| radius.min(limit).max(0.0));
    let y0 = y.floor().max(0.0) as usize;
    let y1 = ((y + height).ceil().min(surface_height as f32)).max(0.0) as usize;
    if y0 >= y1 {
        return;
    }
    let right_edge = (x + width).min(surface_width as f32);
    let edge_inset = |radius: f32, distance: f32| {
        if radius <= 0.0 || distance >= radius {
            0.0
        } else {
            radius - (radius * radius - distance * distance).max(0.0).sqrt()
        }
    };
    for row in y0..y1 {
        let py = row as f32 + 0.5 - y;
        // `y1` is rounded up; the last candidate row can lie entirely below
        // the box and would paint a one-pixel line past its bottom edge.
        if py < 0.0 || py >= height {
            continue;
        }
        let left_inset = if py < top_left {
            edge_inset(top_left, top_left - py)
        } else if py > height - bottom_left {
            edge_inset(bottom_left, py - (height - bottom_left))
        } else {
            0.0
        };
        let right_inset = if py < top_right {
            edge_inset(top_right, top_right - py)
        } else if py > height - bottom_right {
            edge_inset(bottom_right, py - (height - bottom_right))
        } else {
            0.0
        };
        let left = (x + left_inset).max(0.0);
        let right = (x + width - right_inset).min(right_edge);
        if right <= left {
            continue;
        }
        let row_start = row * surface_width;
        let solid_start = left.ceil() as usize;
        let solid_end = (right.floor() as usize).min(surface_width);
        // Partial pixels on both edges keep the corner arc smooth.
        if left > 0.0 && left < surface_width as f32 {
            let column = left.floor() as usize;
            let alpha = ((column as f32 + 1.0 - left).clamp(0.0, 1.0) * 255.0) as u8;
            let index = row_start + column;
            if alpha > 0 && index < pixels.len() {
                pixels[index] = blend(pixels[index], color, alpha);
            }
        }
        if right < surface_width as f32 {
            let column = right.floor() as usize;
            let alpha = ((right - column as f32).clamp(0.0, 1.0) * 255.0) as u8;
            let index = row_start + column;
            if alpha > 0 && index < pixels.len() {
                pixels[index] = blend(pixels[index], color, alpha);
            }
        }
        if solid_end > solid_start {
            let start = row_start + solid_start;
            let end = (row_start + solid_end).min(pixels.len());
            if start < end {
                pixels[start..end].fill(color);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_code_header(
    pixels: &mut [u32],
    surface_width: usize,
    surface_height: usize,
    painter: &mut Painter,
    model: &[crate::markdown::MarkdownLine],
    open_line: usize,
    y: f32,
    row_h: f32,
    text_x: f32,
    content_width: f32,
    font_px: f32,
    density: f32,
    palette: &SurfacePalette,
    folded: bool,
    line_count: usize,
    show_label: bool,
) {
    let label_px = code_label_px(font_px, density);
    let pad = (10.0 * density).round();
    let text_y = centered_text_y(painter, y, row_h, label_px, Face::Ui);
    if show_label {
        // Desktop treatment: the language in accent, with a `<>` glyph after
        // it. A fence without a language reads as CODE.
        let language = model
            .get(open_line)
            .and_then(|row| row.language.clone())
            .filter(|language| !language.is_empty())
            .unwrap_or_else(|| "code".to_string())
            .to_uppercase();
        let bold_offset = (0.6 * density).round();
        painter.draw(
            pixels,
            surface_width,
            surface_height,
            text_x + pad,
            text_y,
            &language,
            label_px,
            palette.accent,
            Face::Ui,
            false,
        );
        painter.draw(
            pixels,
            surface_width,
            surface_height,
            text_x + pad + bold_offset,
            text_y,
            &language,
            label_px,
            palette.accent,
            Face::Ui,
            false,
        );
        let language_width = painter.width(&language, label_px, Face::Ui);
        let mut icon_x = text_x + pad + language_width + (8.0 * density).round();
        painter.draw(
            pixels,
            surface_width,
            surface_height,
            icon_x,
            text_y,
            "<>",
            label_px,
            palette.text_muted,
            Face::Ui,
            false,
        );
        if folded {
            icon_x += painter.width("<>", label_px, Face::Ui) + (6.0 * density).round();
            painter.draw(
                pixels,
                surface_width,
                surface_height,
                icon_x,
                text_y,
                &format!("· {line_count} lines"),
                label_px,
                palette.text_muted,
                Face::Ui,
                false,
            );
        }
    }
    let (pill_x, pill_w, pill_h) =
        code_copy_pill(painter, text_x, content_width, label_px, density);
    let pill_y = y + ((row_h - pill_h) * 0.5).max(0.0);
    rounded_box(
        pixels,
        surface_width,
        surface_height,
        pill_x,
        pill_y,
        pill_w,
        pill_h,
        pill_h * 0.5,
        palette.surface_alt,
    );
    let copy_width = painter.width("Copy", label_px, Face::Ui);
    painter.draw(
        pixels,
        surface_width,
        surface_height,
        pill_x + (pill_w - copy_width) * 0.5,
        pill_y + ((pill_h - label_px) * 0.5).max(0.0),
        "Copy",
        label_px,
        palette.text,
        Face::Ui,
        false,
    );
}

/// Index of the visual row containing `y` (a document offset in pixels), or the
/// nearest preceding row.
fn visual_at_y(tops: &[f32], y: f32) -> usize {
    if tops.is_empty() {
        return 0;
    }
    let index = tops.partition_point(|&top| top <= y);
    index.saturating_sub(1).min(tops.len() - 1)
}

fn accumulate_tops(wrap: &[WrapLine]) -> (Vec<f32>, f32) {
    let mut tops = Vec::with_capacity(wrap.len());
    let mut y = 0.0;
    for chunk in wrap {
        tops.push(y);
        y += chunk.height;
    }
    (tops, y)
}

fn line_starts(text: &str) -> (Vec<usize>, Vec<usize>) {
    let mut char_starts = vec![0];
    let mut byte_starts = vec![0];
    for (char_idx, (byte_idx, character)) in text.char_indices().enumerate() {
        if character == '\n' {
            char_starts.push(char_idx + 1);
            byte_starts.push(byte_idx + 1);
        }
    }
    (char_starts, byte_starts)
}

fn byte_at(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(byte, _)| byte)
        .unwrap_or(text.len())
}

fn char_count(text: &str) -> usize {
    text.chars().count()
}

fn line_of(starts: &[usize], selection: usize) -> usize {
    match starts.binary_search(&selection) {
        Ok(index) => index,
        Err(index) => index.saturating_sub(1),
    }
}

fn logical_line<'a>(text: &'a str, byte_starts: &[usize], line: usize) -> &'a str {
    let Some(&start) = byte_starts.get(line) else {
        return "";
    };
    let mut end = if line + 1 < byte_starts.len() {
        byte_starts[line + 1].saturating_sub(1)
    } else {
        text.len()
    };
    if end > start && text.as_bytes().get(end - 1) == Some(&b'\r') {
        end -= 1;
    }
    &text[start..end]
}

fn rounded_rect(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    size: f32,
    radii: [f32; 4],
    rgb: u32,
) {
    let color = logical(rgb);
    let x0 = x.floor().max(0.0) as usize;
    let y0 = y.floor().max(0.0) as usize;
    let x1 = ((x + size).ceil().min(width as f32)).max(0.0) as usize;
    let y1 = ((y + size).ceil().min(height as f32)).max(0.0) as usize;
    for row in y0..y1 {
        for column in x0..x1 {
            let px = column as f32 + 0.5 - x;
            let py = row as f32 + 0.5 - y;
            if px < 0.0 || py < 0.0 || px >= size || py >= size {
                continue;
            }
            let corner = |cx: f32, cy: f32, radius: f32, left: bool, top: bool| {
                if radius <= 0.0 {
                    return true;
                }
                let in_x = if left { px < cx } else { px > cx };
                let in_y = if top { py < cy } else { py > cy };
                if in_x && in_y {
                    (px - cx) * (px - cx) + (py - cy) * (py - cy) <= radius * radius
                } else {
                    true
                }
            };
            let inside = corner(radii[0], radii[0], radii[0], true, true)
                && corner(size - radii[1], radii[1], radii[1], false, true)
                && corner(size - radii[2], size - radii[2], radii[2], false, false)
                && corner(radii[3], size - radii[3], radii[3], true, false);
            if inside {
                pixels[row * width + column] = color;
            }
        }
    }
}

/// Material-style teardrop handle:
/// - `kind == 0`: start handle (left). Sharp tip at (tip_x, tip_y) pointing up-right, circular bulb hangs down-left.
/// - `kind == 1`: end handle (right). Sharp tip at (tip_x, tip_y) pointing up-left, circular bulb hangs down-right.
/// - `kind == 2`: insertion handle (center). Sharp tip at (tip_x, tip_y) pointing up, vertical symmetric teardrop hangs straight down.
fn draw_teardrop_handle(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    tip_x: f32,
    tip_y: f32,
    density: f32,
    kind: i32,
    rgb: u32,
) {
    if kind == 2 {
        let radius = 10.0 * density;
        let h_dist = 1.38 * radius;
        let cx = tip_x;
        let cy = tip_y + h_dist;
        let l_tan = (h_dist * h_dist - radius * radius).sqrt();
        let y_tan = cy - (radius * radius) / h_dist;
        let sin_theta = radius / h_dist;
        let cos_theta = l_tan / h_dist;
        let aa = (0.75 * density).max(0.5);

        let min_x = (cx - radius - aa).floor().max(0.0) as usize;
        let max_x = (cx + radius + aa).ceil().min(width as f32) as usize;
        let min_y = (tip_y - aa).floor().max(0.0) as usize;
        let max_y = (cy + radius + aa).ceil().min(height as f32) as usize;

        for row in min_y..max_y {
            let py = row as f32 + 0.5;
            let row_offset = row * width;
            let dy = py - tip_y;

            for col in min_x..max_x {
                let px = col as f32 + 0.5;
                let dx = (px - tip_x).abs();

                let alpha = if dy < 0.0 {
                    if dy <= -aa {
                        0
                    } else {
                        let w = aa.max(density);
                        if dx > w {
                            0
                        } else {
                            let h_alpha = ((w - dx) / w).clamp(0.0, 1.0);
                            let v_alpha = ((dy + aa) / aa).clamp(0.0, 1.0);
                            (h_alpha * v_alpha * 255.0) as u8
                        }
                    }
                } else if py <= y_tan {
                    let dist = dy * sin_theta - dx * cos_theta;
                    if dist >= aa {
                        255
                    } else if dist <= -aa {
                        0
                    } else {
                        (((dist + aa) / (2.0 * aa)) * 255.0).clamp(0.0, 255.0) as u8
                    }
                } else {
                    let d = ((px - cx) * (px - cx) + (py - cy) * (py - cy)).sqrt();
                    let dist = radius - d;
                    if dist >= aa {
                        255
                    } else if dist <= -aa {
                        0
                    } else {
                        (((dist + aa) / (2.0 * aa)) * 255.0).clamp(0.0, 255.0) as u8
                    }
                };

                if alpha > 0 {
                    let index = row_offset + col;
                    pixels[index] = blend(pixels[index], rgb, alpha);
                }
            }
        }
        return;
    }

    let radius = 11.0 * density;
    let (cx, cy) = match kind {
        0 => (tip_x - radius, tip_y + radius),
        _ => (tip_x + radius, tip_y + radius),
    };

    let min_x = ((cx - radius).min(tip_x)).floor().max(0.0) as usize;
    let max_x = ((cx + radius).max(tip_x)).ceil().min(width as f32) as usize;
    let min_y = tip_y.floor().max(0.0) as usize;
    let max_y = (cy + radius).ceil().min(height as f32) as usize;

    let r = radius;
    let aa = (0.75 * density).max(0.5);

    for row in min_y..max_y {
        let py = row as f32 + 0.5;
        let row_offset = row * width;
        for col in min_x..max_x {
            let px = col as f32 + 0.5;
            let dx = px - cx;
            let dy = py - cy;
            let d2 = dx * dx + dy * dy;

            let in_point = match kind {
                0 => (cx <= px && px <= tip_x) && (tip_y <= py && py <= cy),
                _ => (tip_x <= px && px <= cx) && (tip_y <= py && py <= cy),
            };

            let index = row_offset + col;
            if in_point {
                pixels[index] = blend(pixels[index], rgb, 255);
            } else if d2 <= (r - aa) * (r - aa) {
                pixels[index] = blend(pixels[index], rgb, 255);
            } else if d2 <= (r + aa) * (r + aa) {
                let dist = d2.sqrt();
                let alpha = (((r + aa - dist) / (2.0 * aa)) * 255.0).clamp(0.0, 255.0) as u8;
                pixels[index] = blend(pixels[index], rgb, alpha);
            }
        }
    }
}

/// Face for one char of a logical line: fenced and inline code use the code
/// face, everything else keeps the line's face.
fn char_face(
    model: &[crate::markdown::MarkdownLine],
    line: usize,
    index: usize,
    line_face: Face,
    code_face: Face,
) -> Face {
    let Some(model_line) = model.get(line) else {
        return line_face;
    };
    if matches!(model_line.kind.as_str(), "code" | "code_fence") {
        return code_face;
    }
    let content_start = model_line.prefix_chars;
    for span in &model_line.spans {
        if span.kind == "inline_code" {
            let start = content_start + span.start;
            let end = content_start + span.end;
            if index >= start && index < end {
                return code_face;
            }
        }
    }
    line_face
}

fn is_revealable_kind(kind: &str) -> bool {
    matches!(
        kind,
        "link"
            | "bold"
            | "italic"
            | "bold_italic"
            | "inline_code"
            | "strikethrough"
            | "underline"
            | "highlight"
            | "grammar"
    )
}

/// Raw char ranges of the inline elements the caret or selection reveals.
///
/// Mirrors the desktop `inline_reveal`: only the element under the caret (or
/// touched by the selection) shows its markers, not the whole line.
fn reveal_ranges(
    model: &[crate::markdown::MarkdownLine],
    line: usize,
    char_count: usize,
    cursor_char: Option<usize>,
    selection: Option<(usize, usize)>,
) -> Vec<(usize, usize)> {
    let Some(model_line) = model.get(line) else {
        return Vec::new();
    };
    if matches!(model_line.kind.as_str(), "code" | "code_fence" | "rule") {
        return Vec::new();
    }
    let base = model_line.prefix_chars.min(char_count);
    let spans = &model_line.spans;
    let is_marker = |index: usize| {
        spans
            .get(index)
            .map(|span| span.kind == "marker")
            .unwrap_or(false)
    };
    let mut elements: Vec<(usize, usize)> = Vec::new();
    let mut index = 0usize;
    while index < spans.len() {
        if index + 2 < spans.len()
            && is_marker(index)
            && is_revealable_kind(&spans[index + 1].kind)
            && is_marker(index + 2)
        {
            let start = (base + spans[index].start).min(char_count);
            let end = (base + spans[index + 2].end).min(char_count);
            elements.push((start, end));
            index += 3;
            continue;
        }
        if index + 1 < spans.len()
            && is_marker(index)
            && spans[index].text == "\\"
            && spans[index + 1].kind == "text"
        {
            let start = (base + spans[index].start).min(char_count);
            let end = (base + spans[index + 1].end).min(char_count);
            elements.push((start, end));
            index += 2;
            continue;
        }
        index += 1;
    }
    let task = if model_line.kind == "task" {
        Some((0usize, model_line.prefix_chars.min(char_count)))
    } else {
        None
    };
    let mut ranges = Vec::new();
    if let Some((start, end)) = selection {
        if start < end {
            if let Some(range) = task {
                if start < range.1 && range.0 < end {
                    ranges.push(range);
                }
            }
            for &(a, b) in &elements {
                if start < b && a < end {
                    ranges.push((a, b));
                }
            }
        }
    }
    if let Some(cursor) = cursor_char {
        if let Some(range) = task {
            if range.0 <= cursor && cursor < range.1 {
                ranges.push(range);
            }
        }
        for &(a, b) in &elements {
            // The end boundary counts as inside: a caret right after the
            // visible text of `[word](url)` sits before the closing marker.
            if a <= cursor && cursor <= b {
                ranges.push((a, b));
            }
        }
    }
    ranges
}

/// Whether a raw char is hidden when nothing is revealed on the line: block
/// prefixes and every inline marker cost no wrap width, exactly like `draw`.
fn char_hidden(
    model: &[crate::markdown::MarkdownLine],
    line: usize,
    index: usize,
    char_count: usize,
) -> bool {
    let Some(model_line) = model.get(line) else {
        return false;
    };
    let prefix = model_line.prefix_chars.min(char_count);
    let block = match model_line.kind.as_str() {
        "heading" => index < prefix,
        "quote" => index > 0 && index < prefix,
        "task" => {
            let start = (model_line.indent + 2).min(char_count);
            index >= start && index < prefix
        }
        "code_fence" => true,
        _ => false,
    };
    if block {
        return true;
    }
    model_line.spans.iter().any(|span| {
        if span.kind != "marker" {
            return false;
        }
        let start = (prefix + span.start).min(char_count);
        let end = (prefix + span.end).min(char_count);
        index >= start && index < end
    })
}

fn canonical_hidden(
    model: &[crate::markdown::MarkdownLine],
    line: usize,
    char_count: usize,
) -> Vec<bool> {
    (0..char_count)
        .map(|index| char_hidden(model, line, index, char_count))
        .collect()
}

/// Whether a task's raw `- [ ]` marker column is showing. Like the desktop
/// live preview, the caret or a selection has to touch the marker itself:
/// editing the text keeps the painted checkbox in place.
fn task_prefix_revealed(
    line_start: usize,
    prefix_chars: usize,
    cursor: Option<usize>,
    selection: Option<(usize, usize)>,
) -> bool {
    if prefix_chars == 0 {
        return true;
    }
    let end = line_start + prefix_chars;
    if let Some(cursor) = cursor {
        if cursor >= line_start && cursor < end {
            return true;
        }
    }
    if let Some((start, finish)) = selection {
        if start < finish && start < end && line_start < finish {
            return true;
        }
    }
    false
}

/// An idle task row keeps its whole marker column as visible blanks, so the
/// painted checkbox reserves the same width as the raw `- [ ]` source and
/// never overlaps the text that follows it.
fn clear_closed_task_prefix(
    model: &[crate::markdown::MarkdownLine],
    line: usize,
    line_start: usize,
    hidden: &mut [bool],
    cursor: Option<usize>,
    selection: Option<(usize, usize)>,
) {
    let Some(model_line) = model.get(line) else {
        return;
    };
    if model_line.kind != "task" {
        return;
    }
    let prefix = model_line.prefix_chars.min(hidden.len());
    if prefix == 0 || task_prefix_revealed(line_start, prefix, cursor, selection) {
        return;
    }
    for value in hidden.iter_mut().take(prefix) {
        *value = false;
    }
}

/// Hidden chars for one line in its current paint state.
fn compute_hidden(
    model: &[crate::markdown::MarkdownLine],
    line: usize,
    chars: &[char],
    line_start: usize,
    cursor_line: Option<usize>,
    cursor: usize,
    selection: (usize, usize),
    preview: bool,
) -> Vec<bool> {
    let char_count = chars.len();
    if preview {
        let mut hidden = canonical_hidden(model, line, char_count);
        clear_closed_task_prefix(model, line, line_start, &mut hidden, None, None);
        return hidden;
    }
    let sel_start = selection.0.max(line_start);
    let sel_end = selection.1.min(line_start + char_count);
    let has_selection = sel_start < sel_end;
    let is_cursor = cursor_line == Some(line);
    // Editing any line of a fenced block reveals that block's own fences, so
    // the caret does not have to sit on the opening or closing line to see the
    // ``` markers. The reading-mode fold path never reaches here.
    let active_code_block = cursor_line
        .and_then(|cursor| model.get(cursor))
        .filter(|row| is_code_kind(&row.kind))
        .and_then(|row| row.block_id);
    if !is_cursor && !has_selection {
        if let (Some(block), Some(model_line)) = (active_code_block, model.get(line)) {
            if model_line.kind == "code_fence" && model_line.block_id == Some(block) {
                return vec![false; char_count];
            }
        }
        let mut hidden = canonical_hidden(model, line, char_count);
        clear_closed_task_prefix(model, line, line_start, &mut hidden, None, None);
        return hidden;
    }
    // The active line keeps its block prefix raw; only the inline markers of
    // the element under the caret/selection appear.
    let mut hidden = vec![false; char_count];
    let Some(model_line) = model.get(line) else {
        return hidden;
    };
    if model_line.kind == "code_fence" {
        return hidden;
    }
    let cursor_char = if is_cursor {
        Some(cursor.saturating_sub(line_start))
    } else {
        None
    };
    let selection_chars = if has_selection {
        Some((sel_start - line_start, sel_end - line_start))
    } else {
        None
    };
    let ranges = reveal_ranges(model, line, char_count, cursor_char, selection_chars);
    let base = model_line.prefix_chars.min(char_count);
    for span in &model_line.spans {
        if span.kind != "marker" {
            continue;
        }
        let start = (base + span.start).min(char_count);
        let end = (base + span.end).min(char_count);
        let shown = ranges.iter().any(|&(a, b)| start < b && a < end);
        if !shown {
            for value in hidden[start..end].iter_mut() {
                *value = true;
            }
        }
    }
    hidden
}

/// Wrap one logical line into raw char ranges. `hidden` chars (collapsed
/// Markdown markup) take no width, so a long link destination cannot push the
/// visible text into the next row.
#[allow(clippy::too_many_arguments)]
fn wrap_line_chunks(
    painter: &mut Painter,
    chars: &[char],
    hidden: &[bool],
    content_width: f32,
    first_indent: f32,
    continuation_indent: f32,
    font: f32,
    line_face: Face,
    model: &[crate::markdown::MarkdownLine],
    line_index: usize,
    code_face: Face,
) -> Vec<(usize, usize)> {
    if chars.is_empty() {
        return vec![(0, 0)];
    }
    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < chars.len() {
        // A quote's text starts after its marker column, and its continuation
        // rows hang under that text with a little extra gap, so the callout
        // bar never runs into the wrapped prose.
        let indent = if start == 0 { first_indent } else { continuation_indent };
        let limit = (content_width - indent).max(1.0);
        let mut end = start;
        let mut width = 0.0f32;
        while end < chars.len() {
            let face = char_face(model, line_index, end, line_face, code_face);
            let (mut step, len) = painter.advance_at(chars, end, font, face);
            if hidden.get(end).copied().unwrap_or(false) {
                step = 0.0;
            }
            if width + step > limit && end > start {
                break;
            }
            width += step;
            end += len;
        }
        if end < chars.len() {
            if let Some(space) = (start..end)
                .rev()
                .find(|&index| chars[index] == ' ' || chars[index] == '\t')
            {
                if space > start {
                    end = space + 1;
                }
            }
        }
        chunks.push((start, end));
        start = end;
    }
    chunks
}

/// Painted width of a line's marker column: quotes pad their marker cells,
/// bullets pad the indent, the dot and its space. Continuation rows hang under
/// that column.
fn line_text_indent(
    painter: &mut Painter,
    model: &[crate::markdown::MarkdownLine],
    line: usize,
    font: f32,
    face: Face,
) -> f32 {
    let Some(row) = model.get(line) else {
        return 0.0;
    };
    match row.kind.as_str() {
        "quote" => {
            let prefix = row.prefix_chars.min(64);
            painter.width(&" ".repeat(prefix), font, face)
        }
        "bullet" => {
            let indent = row.indent.min(64);
            let mut text = " ".repeat(indent);
            text.push('●');
            text.push(' ');
            painter.width(&text, font, face)
        }
        "task" => {
            let prefix = row.prefix_chars.min(64);
            painter.width(&" ".repeat(prefix), font, face)
        }
        _ => 0.0,
    }
}

fn wrap_layout(
    text: &str,
    starts: &[usize],
    content_width: f32,
    painter: &mut Painter,
    line_fonts: &[(f32, Face)],
    model: &[crate::markdown::MarkdownLine],
    code_face: Face,
    fallback_font_px: f32,
    base_line_px: f32,
    density: f32,
    preview: bool,
    folded_code: &HashSet<usize>,
    raw_tables: &HashSet<usize>,
    selection_tables: &HashSet<usize>,
) -> Vec<WrapLine> {
    let all: Vec<char> = text.chars().collect();
    let mut rows = Vec::with_capacity(starts.len());
    for (line_index, &line_start) in starts.iter().enumerate() {
        let (font, line_face) = line_fonts
            .get(line_index)
            .copied()
            .unwrap_or((fallback_font_px, Face::Mono));
        let line_end = if line_index + 1 < starts.len() {
            starts[line_index + 1].saturating_sub(1)
        } else {
            all.len()
        };
        let chars = &all[line_start.min(all.len())..line_end.min(all.len())];
        // A table painted as a grid takes exactly one visual row per source
        // line, and none of its raw pipes may take part in wrapping.
        let table_kind =
            model.get(line_index).map(|line| line.kind.as_str());
        let table = table_kind.filter(|kind| is_table_kind(kind));
        // The table holding the caret keeps its raw source lines so it can be
        // edited; every other table is painted as a grid.
        let raw_table_here = table.is_some()
            && table_header_at(model, line_index)
                .map(|header| {
                    raw_tables.contains(&header) || selection_tables.contains(&header)
                })
                .unwrap_or(false);
        let hidden = if table.is_some() && !raw_table_here {
            vec![true; chars.len()]
        } else {
            canonical_hidden(model, line_index, chars.len())
        };
        // A folded reading-mode block keeps only its header row; every code
        // line and the closing fence collapse to zero height.
        let folded = if preview {
            model
                .get(line_index)
                .filter(|row| is_code_kind(&row.kind))
                .and_then(|row| row.block_id)
                .map(|id| folded_code.contains(&id))
                .unwrap_or(false)
        } else {
            false
        };
        let height = match table {
            // The raw table keeps ordinary source rows.
            Some(_) if raw_table_here => visual_line_height(font, base_line_px, density),
            // The header's own bottom border is the separator, so the source
            // separator line needs no vertical space of its own.
            Some("table_separator") => 0.0,
            Some(_) => visual_line_height(font, base_line_px, density) + (10.0 * density).round(),
            None if folded => {
                if is_opening_fence(model, line_index) {
                    code_header_height(fallback_font_px, base_line_px, density)
                } else {
                    0.0
                }
            }
            None => row_height_for(
                model,
                line_index,
                font,
                fallback_font_px,
                base_line_px,
                density,
            ),
        };
        let text_indent = line_text_indent(painter, model, line_index, font, line_face);
        let (first_indent, continuation_indent) = (text_indent, text_indent);
        for (start, end) in wrap_line_chunks(
            painter,
            chars,
            &hidden,
            content_width,
            first_indent,
            continuation_indent,
            font,
            line_face,
            model,
            line_index,
            code_face,
        ) {
            rows.push(WrapLine {
                line: line_index,
                start,
                end,
                font,
                height,
            });
        }
    }
    rows
}

fn heading_font(
    line: Option<&crate::markdown::MarkdownLine>,
    base: f32,
    headings: &[HeadingStyle; 6],
    editor_font_dp: f32,
    density: f32,
) -> f32 {
    let Some(line) = line else {
        return base;
    };
    if line.kind != "heading" {
        return base;
    }
    let style = heading_at(headings, line.level);
    let scale = editor_font_dp / EDITOR_FONT_DP;
    (style.size_dp * scale * density).round().max(6.0)
}

fn model_for_editor(text: &str) -> Vec<crate::markdown::MarkdownLine> {
    let line_count = text.bytes().filter(|byte| *byte == b'\n').count() + 1;
    if text.chars().count() > 120_000 || line_count > 4_000 {
        Vec::new()
    } else {
        let mut model = crate::markdown::model(text);
        // `str::lines()` drops the empty line after a trailing newline, while
        // the editor keeps a start offset for it. The renderer only uses the
        // model when its length matches the line starts, so keep them aligned;
        // otherwise marker hiding and heading styling are disabled for the
        // whole note.
        if !model.is_empty() && text.ends_with('\n') {
            model.push(blank_markdown_line());
        }
        model
    }
}

fn blank_markdown_line() -> crate::markdown::MarkdownLine {
    crate::markdown::MarkdownLine {
        raw: String::new(),
        kind: "text".to_string(),
        level: 0,
        indent: 0,
        marker: String::new(),
        content: String::new(),
        checked: None,
        num: None,
        language: None,
        is_opening: None,
        block_id: None,
        callout: None,
        prefix_chars: 0,
        spans: Vec::new(),
        cells: Vec::new(),
        target: None,
        alt: None,
    }
}

fn blank_range(chars: &mut [char], start: usize, end: usize) {
    let start = start.min(chars.len());
    let end = end.min(chars.len());
    for character in &mut chars[start..end] {
        *character = ' ';
    }
}

fn inline_color(kind: &str, base: u32, palette: &SurfacePalette) -> u32 {
    match kind {
        "inline_code" => palette.inline_code,
        "link" => palette.link,
        "tag" => palette.tag,
        "grammar" => palette.text_muted,
        "highlight" => palette.heading,
        _ => base,
    }
}

fn keyword(language: &str, word: &str) -> bool {
    const RUST: &[&str] = &[
        "fn", "let", "mut", "pub", "use", "mod", "struct", "enum", "impl", "trait", "match",
        "if", "else", "for", "while", "loop", "return", "self", "Self", "crate", "async",
        "await", "move", "ref", "type", "const", "static", "where", "unsafe", "extern", "dyn",
        "in", "as", "break", "continue", "true", "false", "Some", "None", "Ok", "Err",
    ];
    const KOTLIN: &[&str] = &[
        "fun", "val", "var", "class", "object", "interface", "if", "else", "when", "for",
        "while", "return", "import", "package", "private", "public", "protected", "internal",
        "override", "suspend", "data", "sealed", "enum", "companion", "null", "true", "false",
        "is", "as", "in", "out", "by", "lateinit", "const", "typealias", "try", "catch",
        "finally", "throw", "this", "super",
    ];
    const JS: &[&str] = &[
        "function", "const", "let", "var", "if", "else", "for", "while", "return", "import",
        "export", "class", "new", "this", "async", "await", "try", "catch", "finally", "throw",
        "typeof", "interface", "type", "extends", "implements", "true", "false", "null",
        "undefined",
    ];
    const PYTHON: &[&str] = &[
        "def", "class", "import", "from", "if", "elif", "else", "for", "while", "return", "in",
        "is", "not", "and", "or", "None", "True", "False", "try", "except", "finally", "with",
        "as", "lambda", "yield", "async", "await", "pass", "break", "continue", "global",
        "nonlocal",
    ];
    const C: &[&str] = &[
        "auto", "break", "case", "char", "const", "continue", "default", "do", "double", "else",
        "enum", "extern", "float", "for", "goto", "if", "inline", "int", "long", "register",
        "restrict", "return", "short", "signed", "sizeof", "static", "struct", "switch",
        "typedef", "union", "unsigned", "void", "volatile", "while", "class", "public",
        "private", "protected", "virtual", "override", "final", "namespace", "using", "template",
        "typename", "new", "delete", "this", "throw", "try", "catch", "bool", "true", "false",
        "nullptr", "null", "package", "import", "extends", "implements", "interface", "abstract",
    ];
    let language = language.to_ascii_lowercase();
    let words: &[&str] = if language.contains("rust") {
        RUST
    } else if language.contains("kotlin") || language == "kt" {
        KOTLIN
    } else if language.contains("python") || language == "py" {
        PYTHON
    } else if language.contains("js")
        || language.contains("javascript")
        || language.contains("ts")
        || language.contains("jsx")
        || language.contains("tsx")
    {
        JS
    } else {
        C
    };
    words.contains(&word)
}

fn code_colors(line: &str, language: &str, palette: &SurfacePalette) -> Vec<u32> {
    let chars: Vec<char> = line.chars().collect();
    let mut colors = vec![palette.code_normal; chars.len()];
    let language = language.to_ascii_lowercase();
    let hash_comments = language.contains("py")
        || language.contains("sh")
        || language.contains("yaml")
        || language.contains("toml");
    let mut index = 0;
    while index < chars.len() {
        let character = chars[index];
        let slash_comment = !hash_comments
            && character == '/'
            && chars.get(index + 1) == Some(&'/');
        if slash_comment || (hash_comments && character == '#') {
            for color in colors.iter_mut().skip(index) {
                *color = palette.code_comment;
            }
            break;
        }
        if character == '"' || character == '\'' || character == '`' {
            let quote = character;
            let start = index;
            index += 1;
            while index < chars.len() {
                if chars[index] == '\\' {
                    index += 2;
                    continue;
                }
                if chars[index] == quote {
                    index += 1;
                    break;
                }
                index += 1;
            }
            for color in colors.iter_mut().take(index.min(chars.len())).skip(start) {
                *color = palette.code_string;
            }
            continue;
        }
        if character.is_ascii_digit() {
            let start = index;
            index += 1;
            while index < chars.len() && (chars[index].is_ascii_alphanumeric() || chars[index] == '.') {
                index += 1;
            }
            for color in colors.iter_mut().take(index).skip(start) {
                *color = palette.code_number;
            }
            continue;
        }
        if character.is_alphanumeric() || character == '_' {
            let start = index;
            index += 1;
            while index < chars.len() && (chars[index].is_alphanumeric() || chars[index] == '_') {
                index += 1;
            }
            let word: String = chars[start..index].iter().collect();
            let color = if keyword(&language, &word) {
                palette.code_keyword
            } else {
                let next = chars[index..].iter().find(|character| !character.is_whitespace());
                if next == Some(&'(') {
                    palette.code_function
                } else if word.chars().next().is_some_and(|character| character.is_uppercase()) {
                    palette.code_type
                } else {
                    palette.code_normal
                }
            };
            for color_slot in colors.iter_mut().take(index).skip(start) {
                *color_slot = color;
            }
            continue;
        }
        index += 1;
    }
    colors
}

impl SurfaceState {
    fn new(window: *mut ANativeWindow, width: usize, height: usize, density: f32) -> Self {
        Self {
            window,
            width,
            height,
            viewport_height: height,
            density: if density > 0.0 { density } else { 1.0 },
            editor_font_dp: EDITOR_FONT_DP,
            editor_line_dp: EDITOR_LINE_DP,
            code_font_dp: CODE_FONT_DP,
            text_face: Face::Mono,
            code_face: Face::Mono,
            headings: DEFAULT_HEADINGS,
            current_line_highlight: true,
            gutter_wrapped_line_highlight: false,
            background: palette().editor_background,
            content_kind: 0,
            painter: Painter::new(),
            tree: TreeState::default(),
            editor: EditorState {
                language: "\0".to_string(),
                caret_visible: true,
                insertion_handle_visible: true,
                starts: vec![0],
                ..EditorState::default()
            },
            pixels: Vec::new(),
            frame_valid: false,
            suspend_render: false,
            last_mode: 0,
            last_scroll: 0.0,
            last_width: 0,
            last_height: 0,
            last_gutter: -1.0,
        }
    }

    fn invalidate(&mut self) {
        self.frame_valid = false;
    }

    /// Body text size in device pixels. Every editor glyph, caret and gutter
    /// metric derives from this one number.
    fn font_px(&self) -> f32 {
        (self.editor_font_dp * self.density).round().max(6.0)
    }

    /// Pixel size of fenced code rows and their fences.
    fn code_font_px(&self) -> f32 {
        (self.code_font_dp * self.density).round().max(6.0)
    }

    /// Size a logical editor line is painted at: headings keep their own scale,
    /// fenced code follows the code size, prose follows the body size.
    fn font_for_line(&self, line: usize) -> f32 {
        match self.editor.model.get(line).map(|row| row.kind.as_str()) {
            Some("code") | Some("code_fence") => self.code_font_px(),
            _ => heading_font(
                self.editor.model.get(line),
                self.font_px(),
                &self.headings,
                self.editor_font_dp,
                self.density,
            ),
        }
    }

    /// Line pitch in device pixels.
    fn line_px(&self) -> f32 {
        (self.editor_line_dp * self.density).round().max(6.0)
    }

    /// Line-number type size in device pixels. It keeps the 12/15 proportion
    /// to the body text that the layout was tuned at.
    fn number_font_px(&self) -> f32 {
        (12.0 * self.font_px() / EDITOR_FONT_DP).round().max(8.0)
    }

    /// Width of the line-number column in device pixels. Only the widest label
    /// and its padding are reserved, so short files get a narrow gutter
    /// instead of a full monospace cell for every potential digit.
    fn gutter_width(&mut self) -> f32 {
        self.ensure_starts();
        let px = self.number_font_px();
        let mut digit = 0.0f32;
        for character in '0'..='9' {
            let mut buf = [0u8; 4];
            digit = digit.max(self.painter.width(character.encode_utf8(&mut buf), px, Face::Ui));
        }
        let digits = self.editor.starts.len().max(1).to_string().len() as f32;
        (digits * digit + 2.0 * GUTTER_PAD_DP * self.density)
            .max(GUTTER_MIN_DP * self.density)
            .round()
    }

    /// Defers the frame until the caller has published all state for a new surface.
    fn refresh(&mut self) {
        if self.suspend_render {
            self.invalidate();
        } else {
            self.render();
        }
    }

    fn current_mode(&self) -> u8 {
        if !self.tree.doc.rows.is_empty() {
            1
        } else if !self.editor.text.is_empty() || self.editor.language != "\0" {
            2
        } else {
            0
        }
    }

    fn render(&mut self) {
        self.render_to_window();
    }

    /// Paints directly into the locked window buffer. Copying the whole
    /// RGBA frame a second time was the most expensive part of a scroll frame.
    fn render_to_window(&mut self) {
        if self.window.is_null() || self.width == 0 || self.height == 0 {
            return;
        }
        let t0 = std::time::Instant::now();
        let mut buffer = ANativeWindowBuffer::default();
        let locked = unsafe {
            ANativeWindow_lock(self.window, &mut buffer, std::ptr::null_mut())
        };
        let t_lock = t0.elapsed();
        if locked != 0 || buffer.bits.is_null() {
            return;
        }
        let t1 = std::time::Instant::now();
        self.paint_locked(&buffer);
        let t_paint = t1.elapsed();
        let t2 = std::time::Instant::now();
        unsafe {
            ANativeWindow_unlockAndPost(self.window);
        }
        let t_post = t2.elapsed();
        android_log!(
            3, // DEBUG
            b"VibecodiumRust\0".as_ptr(),
            b"render breakdown: lock=%.2fms, paint=%.2fms, post=%.2fms\0".as_ptr(),
            t_lock.as_secs_f64() * 1000.0,
            t_paint.as_secs_f64() * 1000.0,
            t_post.as_secs_f64() * 1000.0,
        );
    }

    fn paint_locked(&mut self, buffer: &ANativeWindowBuffer) {
        if self.width == 0 || self.height == 0 || buffer.bits.is_null() {
            return;
        }
        let mode = self.current_mode();
        match mode {
            1 => {
                let max_y = (self.tree.doc.rows.len() as f32 * row_height(self.density)
                    - self.height as f32)
                    .max(0.0);
                self.tree.scroll = self.tree.scroll.min(max_y);
            }
            2 => {
                let max_y = self.editor_scroll_metrics().2;
                self.editor.scroll = self.editor.scroll.min(max_y);
            }
            _ => {}
        }
        let target = match mode {
            1 => self.tree.scroll,
            2 => self.editor.scroll,
            _ => 0.0,
        };
        let length = self.width * self.height;
        let stride = buffer.stride.max(0) as usize;
        let buffer_width = buffer.width.max(0) as usize;
        let buffer_height = buffer.height.max(0) as usize;
        let copy_width = self.width.min(buffer_width);
        let copy_height = self.height.min(buffer_height);
        let target_ptr = buffer.bits as *mut u32;

        if stride == self.width && copy_width == self.width && copy_height == self.height {
            let buffer_slice = unsafe { std::slice::from_raw_parts_mut(target_ptr, length) };
            self.paint_full(buffer_slice, mode, target, self.height, mode != 2);
        } else {
            if self.pixels.len() != length {
                self.pixels.resize(length, 0);
            }
            let mut pixels = std::mem::take(&mut self.pixels);
            self.paint_full(&mut pixels, mode, target, self.height, mode != 2);
            self.pixels = pixels;
            unsafe {
                for row in 0..copy_height {
                    std::ptr::copy_nonoverlapping(
                        self.pixels.as_ptr().add(row * self.width),
                        target_ptr.add(row * stride),
                        copy_width,
                    );
                }
            }
        }

        self.frame_valid = true;
        self.last_mode = mode;
        self.last_scroll = target;
        self.last_width = self.width;
        self.last_height = self.height;
    }

    fn paint_full(
        &mut self,
        pixels: &mut [u32],
        mode: u8,
        scroll: f32,
        height: usize,
        scrollbar: bool,
    ) {
        fill(pixels, self.width, height, self.background);
        let saved_height = self.height;
        let saved_editor_scroll = self.editor.scroll;
        let saved_editor_inset = self.editor.bottom_inset;
        let saved_tree_scroll = self.tree.scroll;
        self.height = height;
        match mode {
            1 => {
                self.tree.scroll = scroll;
                self.draw_tree(pixels);
            }
            2 => {
                self.editor.scroll = scroll;
                self.draw_editor(pixels);
            }
            _ => {}
        }
        if scrollbar {
            self.draw_scrollbar(pixels, self.width, mode);
        }
        self.height = saved_height;
        self.editor.scroll = saved_editor_scroll;
        self.editor.bottom_inset = saved_editor_inset;
        self.tree.scroll = saved_tree_scroll;
    }

    fn draw_scrollbar(&self, pixels: &mut [u32], line_width: usize, mode: u8) {
        let palette = palette();
        let density = self.density;
        let (content, viewport, scroll, max_scroll) = match mode {
            1 => {
                let content = self.tree.doc.rows.len() as f32 * row_height(density);
                let max = (content - self.height as f32).max(0.0);
                (content, self.height as f32, self.tree.scroll.round(), max)
            }
            2 => {
                let content = self.editor.top_inset.round()
                    + self.editor.wrap_height
                    + self.editor.bottom_inset.round();
                let max = (content - self.height as f32).max(0.0);
                (content, self.height as f32, self.editor.scroll.round(), max)
            }
            _ => return,
        };
        if max_scroll <= 0.5 || content <= 0.0 {
            return;
        }
        let bar_width = 3.0 * density;
        let x = self.width as f32 - 2.0 * density - bar_width;
        let track_top = 6.0 * density;
        let track_height = (self.height as f32 - 12.0 * density).max(1.0);
        let ratio = (viewport / content).clamp(0.0, 1.0);
        let thumb_height = (track_height * ratio).max(28.0 * density).min(track_height);
        let progress = (scroll / max_scroll).clamp(0.0, 1.0);
        let y = track_top + (track_height - thumb_height) * progress;
        rect(
            pixels,
            line_width,
            self.height,
            x - density,
            0.0,
            bar_width + 2.0 * density,
            self.height as f32,
            self.background,
        );
        rect(
            pixels,
            line_width,
            self.height,
            x,
            y,
            bar_width,
            thumb_height,
            palette.scrollbar,
        );
    }

    fn tree_viewport(&self) -> String {
        let max_y = (self.tree.doc.rows.len() as f32 * row_height(self.density)
            - self.height as f32)
            .max(0.0);
        json!({ "scroll": self.tree.scroll, "max_y": max_y }).to_string()
    }

    fn tree_set_scroll(&mut self, y: f32) {
        let max_y = (self.tree.doc.rows.len() as f32 * row_height(self.density)
            - self.height as f32)
            .max(0.0);
        self.tree.scroll = y.round().clamp(0.0, max_y);
    }

    fn tree_set_scroll_x(&mut self, x: f32) {
        let max_x = self.tree_max_x();
        self.tree.scroll_x = x.round().clamp(0.0, max_x);
    }

    /// Widest visible row content beyond the viewport. Rows off-screen are
    /// ignored so a long name elsewhere in the tree cannot shift the whole
    /// list out of view. The tail of a row is measured before the type pill
    /// and the scrollbar, so scrolling all the way right reveals the whole
    /// name instead of stopping behind them.
    fn tree_max_x(&mut self) -> f32 {
        let density = self.density;
        let name_font = 17.0 * density;
        let row_h = row_height(density);
        let width = self.width as f32;
        let rows_len = self.tree.doc.rows.len();
        let first = ((self.tree.scroll / row_h).floor().max(0.0) as usize).min(rows_len);
        let last = (((self.tree.scroll + self.height as f32) / row_h).ceil() as usize)
            .min(rows_len);
        let mut painter = std::mem::take(&mut self.painter);
        let mut max_x: f32 = 0.0;
        for row in &self.tree.doc.rows[first..last] {
            let indent = 10.0 * density + row.depth as f32 * 16.0 * density;
            let text_x = indent + if row.folder { 39.0 } else { 35.0 } * density;
            let mut name_right = width - 8.0 * density;
            let tag_width = tree_tag_width(&mut painter, row, density);
            if tag_width > 0.0 {
                name_right = name_right.min(width - tag_width - 13.0 * density);
            }
            let name_width = painter.width(&row.name, name_font, Face::Ui);
            // The half-pixel slack keeps a name that exactly fills the row
            // from ending in an ellipsis at full scroll.
            max_x = max_x.max(text_x + name_width - name_right + 0.5 * density);
        }
        self.painter = painter;
        max_x.max(0.0)
    }

    fn draw_tree(&mut self, pixels: &mut [u32]) {
        let palette = palette();
        let row_h = row_height(self.density);
        let density = self.density;
        let max_x = self.tree_max_x();
        self.tree.scroll_x = self.tree.scroll_x.clamp(0.0, max_x);
        let scroll = self.tree.scroll.round();
        let scroll_x = self.tree.scroll_x.round();
        let active = self.tree.doc.active.clone();
        let rows = std::mem::take(&mut self.tree.doc.rows);
        let mut painter = std::mem::take(&mut self.painter);
        let name_font = 17.0 * density;
        let icon_size = 16.0 * density;
        let icon_thickness = 1.0 * density;
        let chevron_size = 18.0 * density;
        for (index, row) in rows.iter().enumerate() {
            let y = index as f32 * row_h - scroll;
            if y + row_h < 0.0 || y > self.height as f32 {
                continue;
            }
            if active.as_deref() == Some(row.path.as_str()) {
                rect(pixels, self.width, self.height, 0.0, y, self.width as f32, row_h, palette.active_row);
            }
            let center_y = y + row_h / 2.0;
            let indent = 10.0 * density + row.depth as f32 * 16.0 * density - scroll_x;
            let chevron_x = indent;
            let icon_x = indent + if row.folder { 17.0 } else { 13.0 } * density;
            let text_x = indent + if row.folder { 39.0 } else { 35.0 } * density;
            if row.folder && row.has_children {
                painter.draw_filled_icon(
                    pixels,
                    self.width,
                    self.height,
                    chevron_x,
                    center_y - chevron_size / 2.0,
                    chevron_size,
                    chevron_paths(row.expanded),
                    palette.text_faint,
                );
            }
            painter.draw_stroke_icon(
                pixels,
                self.width,
                self.height,
                icon_x,
                center_y - icon_size / 2.0,
                icon_size,
                icon_thickness,
                icon_paths(row.folder, row.expanded),
                if row.folder { palette.tree_folder } else { palette.tree_muted },
            );
            let extension = row.extension.to_ascii_uppercase();
            let tag_width = tree_tag_width(&mut painter, row, density);
            let show_tag = tag_width > 0.0;
            let tag_x = self.width as f32 - tag_width - 8.0 * density;
            let name = row.name.clone();
            let name_color = if row.folder || row.note { palette.tree_text } else { palette.tree_muted };
            let name_y = y + (row_h - name_font) / 2.0;
            let chars: Vec<char> = name.chars().collect();
            // Names stop before the type tag and the scrollbar strip; anything
            // that does not fit ends with an ellipsis instead of running under
            // them.
            let mut name_right = self.width as f32 - 8.0 * density;
            if show_tag {
                name_right = name_right.min(tag_x - 5.0 * density);
            }
            let name_budget = (name_right - text_x).max(0.0);
            let visible = if painter.width(&name, name_font, Face::Ui) <= name_budget {
                chars.len()
            } else {
                fit_name_chars(&mut painter, &chars, name_font, name_budget)
            };
            let ellipsis = visible < chars.len();
            let start = row.match_start.min(visible);
            let end = (row.match_start + row.match_len).min(visible);
            let mut x = text_x;
            if row.match_len > 0 && start < end {
                let prefix: String = chars[..start].iter().collect();
                let matched: String = chars[start..end].iter().collect();
                let suffix: String = chars[end..visible].iter().collect();
                if !prefix.is_empty() {
                    painter.draw(pixels, self.width, self.height, x, name_y, &prefix, name_font, name_color, Face::Ui, false);
                    x += painter.width(&prefix, name_font, Face::Ui);
                }
                let matched_width = painter.width(&matched, name_font, Face::Ui);
                rect(
                    pixels,
                    self.width,
                    self.height,
                    x,
                    name_y,
                    matched_width,
                    name_font,
                    palette.selection,
                );
                painter.draw(
                    pixels,
                    self.width,
                    self.height,
                    x,
                    name_y,
                    &matched,
                    name_font,
                    palette.text,
                    Face::Ui,
                    false,
                );
                x += matched_width;
                if !suffix.is_empty() {
                    painter.draw(pixels, self.width, self.height, x, name_y, &suffix, name_font, name_color, Face::Ui, false);
                    x += painter.width(&suffix, name_font, Face::Ui);
                }
            } else {
                let visible_name: String = chars[..visible].iter().collect();
                if !visible_name.is_empty() {
                    painter.draw(
                        pixels,
                        self.width,
                        self.height,
                        x,
                        name_y,
                        &visible_name,
                        name_font,
                        name_color,
                        Face::Ui,
                        false,
                    );
                    x += painter.width(&visible_name, name_font, Face::Ui);
                }
            }
            if ellipsis {
                painter.draw(
                    pixels,
                    self.width,
                    self.height,
                    x,
                    name_y,
                    "…",
                    name_font,
                    name_color,
                    Face::Ui,
                    false,
                );
            }
            if show_tag {
                rect(
                    pixels,
                    self.width,
                    self.height,
                    tag_x,
                    y + (row_h - 18.0 * density) / 2.0,
                    tag_width,
                    18.0 * density,
                    palette.tag_background,
                );
                painter.draw(
                    pixels,
                    self.width,
                    self.height,
                    tag_x + 5.0 * density,
                    y + (row_h - 9.0 * density) / 2.0,
                    &extension,
                    9.0 * density,
                    palette.tree_muted,
                    Face::Ui,
                    false,
                );
            }
        }
        self.tree.doc.rows = rows;
        self.painter = painter;
    }

    fn ensure_starts(&mut self) {
        if self.editor.starts_dirty {
            let (char_starts, byte_starts) = line_starts(&self.editor.text);
            self.editor.starts = char_starts;
            self.editor.byte_starts = byte_starts;
            self.editor.starts_dirty = false;
        }
    }

    fn editor_metrics(&mut self) -> (f32, f32, f32) {
        let advance = self.painter.advance(self.font_px(), Face::Mono);
        let gutter = self.gutter_width();
        let content = (self.width as f32 - gutter - 8.0 * self.density).max(advance);
        (advance, gutter, content)
    }

    fn ensure_model(&mut self) {
        if self.editor.model_dirty {
            self.editor.model = if self.editor.language.is_empty() {
                model_for_editor(&self.editor.text)
            } else {
                Vec::new()
            };
            self.editor.table_layouts.clear();
            self.editor.code_collapsed = default_folded_code(&self.editor.model);
            self.editor.callout_blocks = callout_block_map(&self.editor.model);
            self.editor.model_dirty = false;
        }
    }

    /// Face used for a logical editor line: code files and fenced Markdown
    /// code use the code face, prose uses the text face.
    fn line_face(&self, line: usize) -> Face {
        if !self.editor.language.is_empty() {
            return self.code_face;
        }
        match self.editor.model.get(line).map(|line| line.kind.as_str()) {
            Some("code") | Some("code_fence") => self.code_face,
            _ => self.text_face,
        }
    }

    /// Line window visible in the current scroll, mapped through the canonical
    /// layout. Stable regardless of active-line exceptions.
    fn visible_line_window(&self) -> (usize, usize) {
        let line_count = self.editor.starts.len();
        if line_count == 0 || self.editor.wrap_canonical.is_empty() {
            return (0, line_count.saturating_sub(1));
        }
        let scroll_top = (self.editor.scroll - self.editor.top_inset).max(0.0);
        let first_visual = visual_at_y(&self.editor.wrap_canonical_tops, scroll_top);
        let viewport_bottom = scroll_top + self.viewport_height as f32;
        let last_visual = (visual_at_y(&self.editor.wrap_canonical_tops, viewport_bottom) + 2)
            .min(self.editor.wrap_canonical.len().saturating_sub(1));
        let mut first_line = 0usize;
        for line in 0..line_count {
            let start = self
                .editor
                .wrap_canonical_first
                .get(line)
                .copied()
                .unwrap_or(0);
            let count = self
                .editor
                .wrap_canonical_count
                .get(line)
                .copied()
                .unwrap_or(0);
            if start + count > first_visual {
                first_line = line;
                break;
            }
        }
        let mut last_line = first_line;
        for line in first_line..line_count {
            let start = self
                .editor
                .wrap_canonical_first
                .get(line)
                .copied()
                .unwrap_or(0);
            if start >= last_visual {
                break;
            }
            last_line = line;
        }
        (first_line, last_line)
    }

    fn paint_key(&self) -> (usize, usize, usize, usize, usize) {
        let (sel_start, sel_end) = if self.editor.preview {
            (0, 0)
        } else {
            self.selected_range()
        };
        let (first_line, last_line) = self.visible_line_window();
        let cursor_line = if self.editor.preview {
            usize::MAX
        } else {
            line_of(&self.editor.starts, self.editor.selection)
        };
        (cursor_line, sel_start, sel_end, first_line, last_line)
    }

    fn ensure_wrap(&mut self, content_width: f32) {
        self.ensure_starts();
        self.ensure_model();
        let (sel_start, sel_end) = self.selected_range();
        let mut selection_tables = HashSet::new();
        if sel_start < sel_end {
            let first = line_of(&self.editor.starts, sel_start);
            let last = line_of(&self.editor.starts, sel_end.saturating_sub(1));
            for line in first..=last {
                if let Some(header) = table_header_at(&self.editor.model, line) {
                    selection_tables.insert(header);
                }
            }
        }
        if self.editor.selection_tables != selection_tables {
            self.editor.selection_tables = selection_tables;
            self.editor.wrap_dirty = true;
        }
        let content_changed = self.editor.wrap_dirty
            || (self.editor.wrap_width - content_width).abs() >= 0.5
            || self.editor.wrap_canonical.is_empty();
        if !content_changed {
            let key = self.paint_key();
            if self.editor.wrap_paint_key == key {
                return;
            }
            self.apply_wrap_paint(content_width);
            self.editor.wrap_paint_key = key;
            return;
        }
        let base = self.font_px();
        let base_line = self.line_px();
        let line_fonts: Vec<(f32, Face)> = (0..self.editor.starts.len())
            .map(|line| (self.font_for_line(line), self.line_face(line)))
            .collect();
        let wrap = wrap_layout(
            &self.editor.text,
            &self.editor.starts,
            content_width,
            &mut self.painter,
            &line_fonts,
            &self.editor.model,
            self.code_face,
            base,
            base_line,
            self.density,
            self.editor.preview,
            &self.editor.code_collapsed,
            &self.editor.raw_tables,
            &self.editor.selection_tables,
        );
        let mut first = vec![0; self.editor.starts.len()];
        let mut count = vec![0; self.editor.starts.len()];
        for (visual, chunk) in wrap.iter().enumerate() {
            if let Some(value) = count.get_mut(chunk.line) {
                if *value == 0 {
                    first[chunk.line] = visual;
                }
                *value += 1;
            }
        }
        let (canonical_tops, canonical_height) = accumulate_tops(&wrap);
        self.editor.wrap_canonical = wrap;
        self.editor.wrap_canonical_first = first;
        self.editor.wrap_canonical_count = count;
        self.editor.wrap_canonical_tops = canonical_tops;
        self.editor.wrap_canonical_height = canonical_height;
        self.editor.wrap_width = content_width;
        self.editor.wrap_dirty = false;
        self.editor.wrap = self.editor.wrap_canonical.clone();
        self.editor.wrap_first = self.editor.wrap_canonical_first.clone();
        self.editor.wrap_count = self.editor.wrap_canonical_count.clone();
        self.editor.wrap_tops = self.editor.wrap_canonical_tops.clone();
        self.editor.wrap_height = self.editor.wrap_canonical_height;
        self.editor.wrap_has_exceptions = false;
        let key = self.paint_key();
        self.apply_wrap_paint(content_width);
        self.editor.wrap_paint_key = key;
    }

    fn restore_canonical_wrap(&mut self) {
        if !self.editor.wrap_has_exceptions {
            return;
        }
        self.editor.wrap = self.editor.wrap_canonical.clone();
        self.editor.wrap_first = self.editor.wrap_canonical_first.clone();
        self.editor.wrap_count = self.editor.wrap_canonical_count.clone();
        self.editor.wrap_tops = self.editor.wrap_canonical_tops.clone();
        self.editor.wrap_height = self.editor.wrap_canonical_height;
        self.editor.wrap_has_exceptions = false;
    }

    /// Re-wrap the caret line and the visible selected lines for the markup
    /// they reveal, leaving every other line canonical. Only the line being
    /// edited can reflow, matching the desktop behaviour.
    fn apply_wrap_paint(&mut self, content_width: f32) {
        let line_count = self.editor.starts.len();
        if line_count == 0 || self.editor.wrap_canonical.is_empty() {
            return;
        }
        if self.editor.preview {
            self.restore_canonical_wrap();
            return;
        }
        let (first_line, last_line) = self.visible_line_window();
        let (sel_start, sel_end) = self.selected_range();
        let cursor_line =
            line_of(&self.editor.starts, self.editor.selection).min(line_count.saturating_sub(1));
        let mut exception = vec![false; line_count];
        if cursor_line >= first_line && cursor_line <= last_line {
            exception[cursor_line] = true;
        }
        if sel_end > sel_start {
            let sel_first = line_of(&self.editor.starts, sel_start);
            let sel_last = line_of(&self.editor.starts, sel_end.saturating_sub(1).max(sel_start));
            let lo = sel_first.max(first_line);
            let hi = sel_last.min(last_line);
            for line in lo..=hi.min(line_count.saturating_sub(1)) {
                exception[line] = true;
            }
        }
        if self.editor.model.is_empty() {
            exception.fill(false);
        }
        if !exception.iter().any(|flag| *flag) {
            self.restore_canonical_wrap();
            return;
        }
        let base = self.font_px();
        let base_line = self.line_px();
        let mut effective: Vec<WrapLine> = Vec::with_capacity(self.editor.wrap_canonical.len());
        let mut new_first = vec![0usize; line_count];
        let mut new_count = vec![0usize; line_count];
        for line in 0..line_count {
            new_first[line] = effective.len();
            // A grid table line stays one row even while its cell holds the
            // caret; rebuilding it from the visible raw pipes would wrap it
            // into several rows and stretch the whole table.
            let grid_table = table_header_at(&self.editor.model, line)
                .map(|header| !self.table_is_raw(header))
                .unwrap_or(false);
            if exception[line] && !grid_table {
                let raw = logical_line(&self.editor.text, &self.editor.byte_starts, line);
                let chars: Vec<char> = raw.chars().collect();
                let line_start = self.editor.starts.get(line).copied().unwrap_or(0);
                let hidden = compute_hidden(
                    &self.editor.model,
                    line,
                    &chars,
                    line_start,
                    Some(cursor_line),
                    self.editor.selection,
                    (sel_start, sel_end),
                    false,
                );
                let font = self.font_for_line(line);
                let line_face = self.line_face(line);
                let code_face = self.code_face;
                let text_indent =
                    line_text_indent(&mut self.painter, &self.editor.model, line, font, line_face);
                let (first_indent, continuation_indent) = (text_indent, text_indent);
                for (start, end) in wrap_line_chunks(
                    &mut self.painter,
                    &chars,
                    &hidden,
                    content_width,
                    first_indent,
                    continuation_indent,
                    font,
                    line_face,
                    &self.editor.model,
                    line,
                    code_face,
                ) {
                    effective.push(WrapLine {
                        line,
                        start,
                        end,
                        font,
                        height: row_height_for(
                            &self.editor.model,
                            line,
                            font,
                            base,
                            base_line,
                            self.density,
                        ),
                    });
                }
            } else {
                let start = self.editor.wrap_canonical_first[line];
                let count = self.editor.wrap_canonical_count[line];
                effective.extend_from_slice(&self.editor.wrap_canonical[start..start + count]);
            }
            new_count[line] = effective.len() - new_first[line];
        }
        let (effective_tops, effective_height) = accumulate_tops(&effective);
        self.editor.wrap = effective;
        self.editor.wrap_tops = effective_tops;
        self.editor.wrap_height = effective_height;
        self.editor.wrap_first = new_first;
        self.editor.wrap_count = new_count;
        self.editor.wrap_has_exceptions = true;
    }

    fn visual_for_position(&self, starts: &[usize], position: usize) -> (usize, usize) {
        let position = position.min(char_count(&self.editor.text));
        let line = line_of(starts, position);
        let column = position.saturating_sub(starts.get(line).copied().unwrap_or(0));
        let first = self.editor.wrap_first.get(line).copied().unwrap_or(0);
        let count = self.editor.wrap_count.get(line).copied().unwrap_or(1).max(1);
        for offset in 0..count {
            let visual = first + offset;
            let Some(chunk) = self.editor.wrap.get(visual) else {
                break;
            };
            if column < chunk.end || offset + 1 == count {
                return (visual, column.saturating_sub(chunk.start));
            }
        }
        (first, column)
    }

    fn draw_editor(&mut self, pixels: &mut [u32]) {
        let palette = palette();
        let density = self.density;
        let font_px = self.font_px();
        if self.editor.model_dirty {
            self.editor.model = if self.editor.language.is_empty() {
                model_for_editor(&self.editor.text)
            } else {
                Vec::new()
            };
            self.editor.table_layouts.clear();
            self.editor.code_collapsed = default_folded_code(&self.editor.model);
            self.editor.callout_blocks = callout_block_map(&self.editor.model);
            self.editor.model_dirty = false;
        }
        let advance = self.painter.advance(font_px, Face::Mono);
        let number_font_px = self.number_font_px();
        let gutter = self.gutter_width();
        if (self.last_gutter - gutter).abs() > 0.5 {
            if self.last_gutter >= 0.0 {
                android_log!(
                    3,
                    b"VibecodiumRust\0".as_ptr(),
                    b"gutter %.1f -> %.1f lines=%d scroll=%.1f band=%.1f height=%d viewport=%d\0"
                        .as_ptr(),
                    self.last_gutter as f64,
                    gutter as f64,
                    self.editor.starts.len() as i32,
                    self.editor.scroll as f64,
                    self.editor.band_origin as f64,
                    self.height as i32,
                    self.viewport_height as i32,
                );
            }
            self.last_gutter = gutter;
        }
        let content_width = (self.width as f32 - gutter - 8.0 * density).max(advance);
        self.ensure_wrap(content_width);
        let starts = std::mem::take(&mut self.editor.starts);
        let byte_starts = std::mem::take(&mut self.editor.byte_starts);
        let total_visual = self.editor.wrap.len();
        let text_x = gutter;
        let scroll = if self.editor.band_origin >= 0.0 {
            self.editor.band_origin.round()
        } else {
            self.editor.scroll.round()
        };
        let top_inset = self.editor.top_inset.round();
        let first = visual_at_y(&self.editor.wrap_tops, (scroll - top_inset).max(0.0));
        let viewport_bottom = (scroll - top_inset).max(0.0) + self.height as f32;
        let last = (visual_at_y(&self.editor.wrap_tops, viewport_bottom) + 2).min(total_visual);
        let (selection_start, selection_end) = self.selected_range();
        let cursor_line = if self.editor.preview {
            usize::MAX
        } else {
            line_of(&starts, self.editor.selection)
        };
        let (cursor_visual, cursor_offset) =
            self.visual_for_position(&starts, self.editor.selection);
        // The desktop hides the wash while a selection is active, so the
        // selected span is the only highlight on screen.
        let show_current_line = self.current_line_highlight
            && !self.editor.preview
            && selection_start == selection_end;
        // Same token the floating header pills use, so the active line reads
        // as part of the top toolbar in both light and dark themes.
        let current_line_wash = palette.surface_alt;
        let line_faces: Vec<Face> = (0..starts.len()).map(|line| self.line_face(line)).collect();
        let headings = self.headings;
        let model = std::mem::take(&mut self.editor.model);
        let has_model = model.len() == starts.len();
        let mut painter = std::mem::take(&mut self.painter);
        let mut prepared_line = usize::MAX;
        let mut prepared_heading: Option<HeadingStyle> = None;
        let mut painted_rule = false;
        let mut task_prefix_open = false;
        let mut bullet_prefix_open = false;
        let mut line_start = 0usize;
        let mut chars: Vec<char> = Vec::new();
        let mut colors: Vec<u32> = Vec::new();
        let mut hidden: Vec<bool> = Vec::new();
        let mut faces: Vec<Face> = Vec::new();
        let mut x_offsets: Vec<f32> = Vec::new();
        let mut overlays: Vec<(usize, usize, u32)> = Vec::new();
        let mut continuation_indent = 0.0f32;
        let (start_visual, start_offset) =
            self.visual_for_position(&starts, selection_start);
        let (end_visual, end_offset) = self.visual_for_position(&starts, selection_end);
        // Copy only the find ranges that can touch the visible lines; the
        // full list can be thousands of entries and this loop runs per frame.
        let first_line = self
            .editor
            .wrap
            .get(first)
            .map(|chunk| chunk.line)
            .unwrap_or(0);
        let last_line = self
            .editor
            .wrap
            .get(last.saturating_sub(1))
            .map(|chunk| chunk.line)
            .unwrap_or_else(|| starts.len().saturating_sub(1));
        let visible_start = starts.get(first_line).copied().unwrap_or(0);
        let visible_end = starts.get(last_line + 1).copied().unwrap_or(usize::MAX);
        let find_active = self.editor.find_active;
        let find_ranges: Vec<(usize, usize, usize)> = self
            .editor
            .find_ranges
            .iter()
            .enumerate()
            .filter(|(_, (range_start, range_end))| {
                *range_end > visible_start && *range_start < visible_end
            })
            .map(|(index, &(range_start, range_end))| (index, range_start, range_end))
            .collect();
        let mut start_handle: Option<(f32, f32)> = None;
        let mut end_handle: Option<(f32, f32)> = None;
        let mut insertion_handle: Option<(f32, f32)> = None;
        let mut painted_code_block: Option<usize> = None;
        let mut painted_callout: Option<usize> = None;
        for visual in first..last {
            let chunk = self.editor.wrap[visual];
            let row_h = chunk.height;
            let y = top_inset + self.editor.wrap_tops.get(visual).copied().unwrap_or(0.0) - scroll;
            let row_face = line_faces.get(chunk.line).copied().unwrap_or(Face::Mono);
            let text_y = centered_text_y(&mut painter, y, row_h, chunk.font, row_face);
            // Folded code rows and table separators own no vertical space.
            if row_h <= 0.0 {
                continue;
            }
            // One wash and one family-coloured bar behind the whole callout
            // block, painted when its first visible row is reached.
            if has_model {
                if let Some((start, end)) =
                    callout_block_extent(&self.editor.callout_blocks, chunk.line)
                {
                    if painted_callout != Some(start) {
                        painted_callout = Some(start);
                        let kind = self.editor.callout_blocks[start]
                            .map(|block| block.kind)
                            .unwrap_or(crate::markdown::CalloutKind::Note);
                        let first_visual = self
                            .editor
                            .wrap_first
                            .get(start)
                            .copied()
                            .unwrap_or(visual);
                        let last_visual = self
                            .editor
                            .wrap_first
                            .get(end)
                            .copied()
                            .unwrap_or(visual)
                            + self
                                .editor
                                .wrap_count
                                .get(end)
                                .copied()
                                .unwrap_or(1)
                                .saturating_sub(1);
                        let top = top_inset
                            + self
                                .editor
                                .wrap_tops
                                .get(first_visual)
                                .copied()
                                .unwrap_or(0.0)
                            - scroll;
                        let bottom = top_inset
                            + self
                                .editor
                                .wrap_tops
                                .get(last_visual)
                                .copied()
                                .unwrap_or(0.0)
                            + self
                                .editor
                                .wrap
                                .get(last_visual)
                                .map(|row| row.height)
                                .unwrap_or(0.0)
                            - scroll;
                        paint_callout_block(
                            pixels,
                            self.width,
                            self.height,
                            text_x,
                            top,
                            content_width,
                            (bottom - top).max(0.0),
                            kind,
                            &palette,
                            density,
                        );
                    }
                }
            }
            // Editing a callout reveals its raw markers. The family bar is
            // decorative then, so the caret row paints the wash over it.
            if !self.editor.preview && chunk.line == cursor_line {
                if let Some(block) = self.editor.callout_blocks.get(chunk.line).copied().flatten()
                {
                    let bar_width = (crate::markdown::CALLOUT_BAR_WIDTH * density)
                        .round()
                        .max(2.0);
                    rect(
                        pixels,
                        self.width,
                        self.height,
                        text_x,
                        y,
                        bar_width,
                        row_h,
                        callout_wash_rgb(block.kind, &palette),
                    );
                }
            }
            // One rounded box behind the whole fenced block, painted when its
            // first visible row is reached.
            if has_model {
                if let Some(line) = model.get(chunk.line) {
                    if is_code_kind(&line.kind) {
                        if let Some((start, end)) = code_block_bounds(&model, chunk.line) {
                            let id = model[start].block_id.unwrap_or(usize::MAX);
                            if painted_code_block != Some(id) {
                                painted_code_block = Some(id);
                                let first_visual = self
                                    .editor
                                    .wrap_first
                                    .get(start)
                                    .copied()
                                    .unwrap_or(visual);
                                let last_visual = self
                                    .editor
                                    .wrap_first
                                    .get(end)
                                    .copied()
                                    .unwrap_or(visual)
                                    + self
                                        .editor
                                        .wrap_count
                                        .get(end)
                                        .copied()
                                        .unwrap_or(1)
                                        .saturating_sub(1);
                                let top = top_inset
                                    + self
                                        .editor
                                        .wrap_tops
                                        .get(first_visual)
                                        .copied()
                                        .unwrap_or(0.0)
                                    - scroll;
                                let bottom = top_inset
                                    + self
                                        .editor
                                        .wrap_tops
                                        .get(last_visual)
                                        .copied()
                                        .unwrap_or(0.0)
                                    + self
                                        .editor
                                        .wrap
                                        .get(last_visual)
                                        .map(|row| row.height)
                                        .unwrap_or(0.0)
                                    - scroll;
                                rounded_box(
                                    pixels,
                                    self.width,
                                    self.height,
                                    text_x,
                                    top,
                                    content_width,
                                    (bottom - top).max(0.0),
                                    (8.0 * density).round(),
                                    palette.code_background,
                                );
                            }
                        }
                    }
                }
            }
            // The caret row wash sits above code and callout backgrounds but
            // below inline, find and selection overlays, like the desktop row
            // highlight. Only the visual row holding the caret is tinted, so a
            // wrapped paragraph does not light up as one tall block.
            if show_current_line && visual == cursor_visual {
                rect(
                    pixels,
                    self.width,
                    self.height,
                    0.0,
                    y,
                    self.width as f32,
                    row_h,
                    current_line_wash,
                );
            }
            // Tables are painted as a grid from the measured layout. The table
            // holding the caret falls through to its raw source lines so it can
            // be edited, exactly like the desktop editor.
            if has_model {
                let header = model
                    .get(chunk.line)
                    .filter(|line| is_table_kind(&line.kind))
                    .and_then(|_| table_bounds(&model, chunk.line))
                    .map(|(start, _)| start);
                if let Some(start) = header.filter(|start| !self.table_is_raw(*start)) {
                    if chunk.start != 0 {
                        prepared_line = usize::MAX;
                        continue;
                    }
                    let table_font = self.code_font_px();
                    if (self.editor.table_layout_width - content_width).abs() >= 0.5
                        || (self.editor.table_layout_font - table_font).abs() >= 0.5
                    {
                        self.editor.table_layouts.clear();
                        self.editor.table_layout_width = content_width;
                        self.editor.table_layout_font = table_font;
                    }
                    if !self.editor.table_layouts.contains_key(&start) {
                        let (_, end) = table_bounds(&model, chunk.line).unwrap_or((start, start));
                        let layout = build_table_layout(
                            &mut painter,
                            &model,
                            start,
                            end,
                            content_width,
                            self.code_font_px(),
                            self.code_face,
                            density,
                        );
                        self.editor.table_layouts.insert(start, layout);
                    }
                    if let Some(layout) = self.editor.table_layouts.get(&start).cloned() {
                        let shift = self
                            .editor
                            .table_scroll
                            .get(&start)
                            .copied()
                            .unwrap_or(0.0)
                            .min((layout.indent + layout.total - content_width).max(0.0));
                        let cursor_char = if chunk.line == cursor_line {
                            Some(
                                self.editor.selection.saturating_sub(
                                    starts.get(chunk.line).copied().unwrap_or(0),
                                ),
                            )
                        } else {
                            None
                        };
                        paint_table_row(
                            pixels,
                            self.width,
                            self.height,
                            &mut painter,
                            &model,
                            chunk.line,
                            &layout,
                            shift,
                            cursor_char,
                            text_x,
                            y,
                            row_h,
                            self.code_font_px(),
                            self.code_face,
                            density,
                            &palette,
                        );
                    }
                    if chunk.line == start {
                        paint_table_source_button(
                            pixels,
                            self.width,
                            self.height,
                            &mut painter,
                            text_x,
                            content_width,
                            y,
                            row_h,
                            self.code_font_px(),
                            density,
                            &palette,
                        );
                    }
                    prepared_line = usize::MAX;
                    continue;
                }
            }
            if chunk.line != prepared_line {
                prepared_line = chunk.line;
                painted_rule = false;
                prepared_heading = if has_model {
                    model
                        .get(chunk.line)
                        .filter(|line| line.kind == "heading")
                        .map(|line| heading_at(&headings, line.level))
                } else {
                    None
                };
                line_start = starts.get(chunk.line).copied().unwrap_or(0);
                let raw = logical_line(&self.editor.text, &byte_starts, chunk.line);
                chars.clear();
                chars.extend(raw.chars());
                colors.clear();
                colors.resize(chars.len(), palette.code_normal);
                hidden = compute_hidden(
                    &model,
                    chunk.line,
                    &chars,
                    line_start,
                    if self.editor.preview {
                        None
                    } else {
                        Some(cursor_line)
                    },
                    self.editor.selection,
                    (selection_start, selection_end),
                    self.editor.preview,
                );
                task_prefix_open = model
                    .get(chunk.line)
                    .filter(|line| line.kind == "task")
                    .is_some_and(|line| {
                        task_prefix_revealed(
                            line_start,
                            line.prefix_chars,
                            if self.editor.preview {
                                None
                            } else {
                                Some(self.editor.selection)
                            },
                            Some((selection_start, selection_end)),
                        )
                    });
                bullet_prefix_open = model
                    .get(chunk.line)
                    .filter(|line| line.kind == "bullet")
                    .is_some_and(|line| {
                        task_prefix_revealed(
                            line_start,
                            line.prefix_chars,
                            if self.editor.preview {
                                None
                            } else {
                                Some(self.editor.selection)
                            },
                            Some((selection_start, selection_end)),
                        )
                    });
                overlays.clear();
                if !self.editor.language.is_empty() {
                    colors = code_colors(raw, &self.editor.language, &palette);
                } else if has_model {
                    if let Some(line) = model.get(chunk.line) {
                        let base = match line.kind.as_str() {
                            "heading" => heading_at(&headings, line.level).color,
                            "quote" => palette.text_muted,
                            "code" => palette.code_string,
                            "code_fence" => palette.text_muted,
                            _ => palette.code_normal,
                        };
                        colors.clear();
                        colors.resize(chars.len(), base);
                        if line.kind == "heading" {
                            // The revealed `#` markers stay muted, like the
                            // desktop heading row.
                            let length = line.prefix_chars.min(chars.len());
                            for value in colors.iter_mut().take(length) {
                                *value = palette.marker;
                            }
                        }
                        if line.kind == "code" {
                            if let Some(language) = line.language.as_deref() {
                                colors = code_colors(raw, language, &palette);
                            }
                        }
                        if chunk.line != cursor_line
                            || (line.kind == "task" && !task_prefix_open)
                            || (line.kind == "bullet" && !bullet_prefix_open)
                        {
                            match line.kind.as_str() {
                                "heading" => {
                                    let length = line.prefix_chars.min(chars.len());
                                    for value in hidden.iter_mut().take(length) {
                                        *value = true;
                                    }
                                }
                                "task" => {
                                    // The checkbox is painted as a real square
                                    // below; the marker cells stay blank and
                                    // reserve the column. Editing the text keeps
                                    // the box, like the desktop live preview:
                                    // only a caret in the marker shows raw.
                                    if !task_prefix_open {
                                        blank_range(&mut chars, 0, line.prefix_chars);
                                        if line.indent + 1 < chars.len() {
                                            chars[line.indent] = ' ';
                                            chars[line.indent + 1] = ' ';
                                        }
                                        if line.checked == Some(true) {
                                            for color in colors[line.prefix_chars.min(chars.len())..].iter_mut() {
                                                *color = palette.marker;
                                            }
                                        }
                                    }
                                }
                                "bullet" => {
                                    blank_range(&mut chars, 0, line.prefix_chars);
                                    if line.prefix_chars >= 2 {
                                        let marker = line.prefix_chars - 2;
                                        if marker + 1 < chars.len() {
                                            // A filled circle reads heavier than
                                            // the source dash, and the accent
                                            // colour separates it from prose.
                                            chars[marker] = '●';
                                            chars[marker + 1] = ' ';
                                            colors[marker] = palette.accent;
                                        }
                                    }
                                }
                                "quote" => {
                                    let length = line.prefix_chars.min(chars.len());
                                    let callout_block = self
                                        .editor
                                        .callout_blocks
                                        .get(chunk.line)
                                        .copied()
                                        .flatten();
                                    for value in hidden.iter_mut().take(length) {
                                        *value = true;
                                    }
                                    if callout_block.is_some() {
                                        // A callout draws its wash and family
                                        // bar for the whole block; the marker
                                        // cells stay as blanks so the title
                                        // keeps its column.
                                        for index in 0..length {
                                            chars[index] = ' ';
                                            hidden[index] = false;
                                        }
                                    } else {
                                        // Every plain quote level keeps its own
                                        // bar in the marker cells.
                                        for index in 0..length {
                                            if chars[index] == '>' {
                                                chars[index] = '│';
                                                hidden[index] = false;
                                                colors[index] = palette.marker;
                                            }
                                        }
                                    }
                                }
                                "rule" => {
                                    // A rule paints a divider once the caret
                                    // leaves the line; on the active line its
                                    // raw `---` stays editable.
                                    hidden.fill(true);
                                    painted_rule = true;
                                }
                                "code_fence" => {
                                    // Editing any line of a block reveals both
                                    // of its fences, so the caret does not have
                                    // to sit on the marker line itself.
                                    let active_block = model
                                        .get(cursor_line)
                                        .filter(|row| is_code_kind(&row.kind))
                                        .and_then(|row| row.block_id);
                                    let revealed = line.block_id.is_some()
                                        && line.block_id == active_block;
                                    if !revealed {
                                        hidden.fill(true);
                                    }
                                }
                                _ => {}
                            }
                        }
                        let content_start = line.prefix_chars.min(chars.len());
                        for span in &line.spans {
                            let start = content_start + span.start;
                            let end = content_start + span.end;
                            if span.kind == "marker" {
                                if chunk.line != cursor_line {
                                    for index in start.min(chars.len())..end.min(chars.len()) {
                                        hidden[index] = true;
                                    }
                                }
                                continue;
                            }
                            for index in start.min(chars.len())..end.min(chars.len()) {
                                colors[index] = inline_color(&span.kind, base, &palette);
                            }
                            if (span.kind == "highlight" || span.kind == "inline_code") && end > start {
                                let color = if span.kind == "inline_code" {
                                    palette.inline_code_background
                                } else {
                                    palette.highlight
                                };
                                overlays.push((start.min(chars.len()), end.min(chars.len()), color));
                            }
                        }
                        // Plain title text resets its colour to the quote base
                        // above, so the callout header reapplies its own tint
                        // after the span pass.
                        if line.kind == "quote" && chunk.line != cursor_line {
                            if let Some(callout) = crate::markdown::callout_prefix(raw) {
                                let color = callout_rgb(callout.kind, &palette);
                                let start = line.prefix_chars.min(chars.len());
                                for value in colors[start..].iter_mut() {
                                    *value = color;
                                }
                            }
                        }
                    }
                }
                let line_face = line_faces.get(chunk.line).copied().unwrap_or(Face::Mono);
                continuation_indent = if has_model {
                    line_text_indent(&mut painter, &model, chunk.line, chunk.font, line_face)
                } else {
                    0.0
                };
                faces.clear();
                faces.resize(chars.len(), line_face);
                if let Some(line) = model.get(chunk.line) {
                    let content_start = line.prefix_chars.min(chars.len());
                    for span in &line.spans {
                        if span.kind == "inline_code" {
                            let start = (content_start + span.start).min(chars.len());
                            let end = (content_start + span.end).min(chars.len());
                            for value in faces[start..end].iter_mut() {
                                *value = self.code_face;
                            }
                        }
                    }
                }
                x_offsets.clear();
                line_x_offsets(&mut x_offsets, &mut painter, &chars, &hidden, chunk.font, |index| {
                    faces.get(index).copied().unwrap_or(line_face)
                });
            }
            let chunk_start = chunk.start.min(chars.len());
            let chunk_end = chunk.end.min(chars.len());
            let base_x = x_offsets.get(chunk_start).copied().unwrap_or(0.0);
            let local_x = |index: usize| x_offsets.get(index).copied().unwrap_or(base_x) - base_x;
            // A quote's continuation rows hang under its text, past the marker
            // column, so the callout bar cannot overlap the wrapped prose.
            let row_shift = if chunk.start > 0 { continuation_indent } else { 0.0 };
            let absolute_start = line_start + chunk.start;
            let absolute_end = line_start + chunk.end;
            if painted_rule {
                let thickness = (1.0 * density).max(1.0);
                rect(
                    pixels,
                    self.width,
                    self.height,
                    text_x,
                    y + (row_h - thickness) * 0.5,
                    content_width,
                    thickness,
                    palette.border,
                );
            }
            for (overlay_start, overlay_end, color) in &overlays {
                let start = (*overlay_start).max(chunk_start);
                let end = (*overlay_end).min(chunk_end);
                let offset_start = local_x(start);
                let offset_end = local_x(end);
                if offset_end > offset_start {
                    rect(
                        pixels,
                        self.width,
                        self.height,
                        text_x + row_shift + offset_start,
                        y + 2.0 * density,
                        offset_end - offset_start,
                        row_h - 4.0 * density,
                        *color,
                    );
                }
            }
            for (idx, find_start, find_end) in &find_ranges {
                let start = (*find_start).max(absolute_start).min(absolute_end);
                let end = (*find_end).max(absolute_start).min(absolute_end);
                if end > start {
                    let raw_start = (start - line_start).clamp(chunk_start, chunk_end);
                    let raw_end = (end - line_start).clamp(chunk_start, chunk_end);
                    let offset_start = local_x(raw_start);
                    let offset_end = local_x(raw_end);
                    if offset_end > offset_start {
                        let color = if Some(*idx) == find_active {
                            palette.accent
                        } else {
                            palette.highlight
                        };
                        rect(
                            pixels,
                            self.width,
                            self.height,
                            text_x + row_shift + offset_start,
                            y + 2.0 * density,
                            offset_end - offset_start,
                            (row_h - 4.0 * density).max(2.0),
                            color,
                        );
                    }
                }
            }
            if selection_end > selection_start {
                let start = selection_start.max(absolute_start).min(absolute_end);
                let end = selection_end.max(absolute_start).min(absolute_end);
                if end > start {
                    let raw_start = (start - line_start).clamp(chunk_start, chunk_end);
                    let raw_end = (end - line_start).clamp(chunk_start, chunk_end);
                    let offset_start = local_x(raw_start);
                    let offset_end = local_x(raw_end);
                    if offset_end > offset_start {
                        let pad = 1.5 * density;
                        let left = (text_x + row_shift + offset_start - pad).max(gutter);
                        let right = text_x + row_shift + offset_end + pad;
                        rect(
                            pixels,
                            self.width,
                            self.height,
                            left,
                            y,
                            (right - left).max(2.0 * density),
                            row_h,
                            palette.selection,
                        );
                    }
                }
            }
            let mut index = chunk_start;
            while index < chunk_end {
                if hidden[index] {
                    index += 1;
                    continue;
                }
                let color = colors[index];
                let face = faces.get(index).copied().unwrap_or(Face::Mono);
                let mut segment = String::new();
                let start_idx = index;
                while index < chunk_end {
                    if hidden[index] {
                        break;
                    }
                    if colors[index] != color || faces[index] != face {
                        break;
                    }
                    segment.push(chars[index]);
                    index += 1;
                }
                let segment_x = text_x + row_shift + local_x(start_idx);
                let italic = prepared_heading.is_some_and(|style| style.italic);
                painter.draw(
                    pixels,
                    self.width,
                    self.height,
                    segment_x,
                    text_y,
                    &segment,
                    chunk.font,
                    color,
                    face,
                    italic,
                );
                if prepared_heading.is_some_and(|style| style.bold) {
                    painter.draw(
                        pixels,
                        self.width,
                        self.height,
                        segment_x + (0.6 * density).round(),
                        text_y,
                        &segment,
                        chunk.font,
                        color,
                        face,
                        italic,
                    );
                }
            }
            // Header chrome of a fenced block: language, fold count and the
            // copy control. Editing any line of the block reveals its raw
            // fences, but the copy control stays on the header row.
            if has_model {
                if let Some(line) = model.get(chunk.line) {
                    if line.kind == "code_fence" && line.is_opening == Some(true) {
                        if let Some((start, end)) = code_block_bounds(&model, chunk.line) {
                            let active_block = if self.editor.preview {
                                None
                            } else {
                                model
                                    .get(cursor_line)
                                    .filter(|row| is_code_kind(&row.kind))
                                    .and_then(|row| row.block_id)
                            };
                            let revealed = !self.editor.preview
                                && line.block_id.is_some()
                                && line.block_id == active_block;
                            let folded = self.editor.preview
                                && line
                                    .block_id
                                    .map(|id| self.editor.code_collapsed.contains(&id))
                                    .unwrap_or(false);
                            let lines = code_block_line_count(&model, start, end);
                            paint_code_header(
                                pixels,
                                self.width,
                                self.height,
                                &mut painter,
                                &model,
                                chunk.line,
                                y,
                                row_h,
                                text_x,
                                content_width,
                                font_px,
                                density,
                                &palette,
                                folded,
                                lines,
                                !revealed,
                            );
                        }
                    }
                }
            }
            // Callout header: the family glyph, plus the type label for an
            // untitled callout. The body rows keep just the wash and the bar.
            if has_model && chunk.line != cursor_line && chunk.start == 0 {
                if let Some(block) = self.editor.callout_blocks.get(chunk.line).copied().flatten() {
                    if block.first {
                        let raw = logical_line(&self.editor.text, &byte_starts, chunk.line);
                        if let Some(callout) = crate::markdown::callout_prefix(raw) {
                            // An untitled callout shows its type after the
                            // glyph; a titled one already carries its own
                            // heading text on the row.
                            let titled = !callout.title.trim().is_empty();
                            paint_callout_header(
                                pixels,
                                self.width,
                                self.height,
                                &mut painter,
                                raw,
                                &callout,
                                block.kind,
                                text_x,
                                y,
                                row_h,
                                font_px,
                                density,
                                &palette,
                                titled,
                                self.text_face,
                            );
                        }
                    }
                }
            }
            // The source toggle stays on the header row of a raw table too.
            if has_model && chunk.start == 0 {
                if let Some(header) = model
                    .get(chunk.line)
                    .filter(|row| is_table_kind(&row.kind))
                    .and_then(|_| table_header_at(&model, chunk.line))
                {
                    if header == chunk.line && self.table_is_raw(header) {
                        paint_table_source_button(
                            pixels,
                            self.width,
                            self.height,
                            &mut painter,
                            text_x,
                            content_width,
                            y,
                            row_h,
                            self.code_font_px(),
                            density,
                            &palette,
                        );
                    }
                }
            }
            // Task checkbox on the first visual row while the marker column is
            // collapsed; a caret inside the marker shows its raw `[x]` instead.
            if has_model && chunk.start == 0 && !task_prefix_open {
                if let Some(line) = model.get(chunk.line) {
                    if line.kind == "task" {
                        if let Some(task) = crate::markdown::task_prefix(&line.raw) {
                            let index = task.indent_chars.min(chars.len().saturating_sub(1));
                            let cell_width =
                                (local_x(index + 1) - local_x(index)).max(1.0);
                            // Desktop: a box two cells wide, clamped to 12..18pt.
                            let size =
                                // Obsidian: --checkbox-size is 1.15em.
                                (chunk.font * 1.15).max(16.0 * density);
                            let box_cells = task.checkbox.chars().count() as f32;
                            // Keep the box inside its own marker cells: the
                            // trailing prefix space then reads as a real gap
                            // between the box and the text.
                            let box_x = text_x
                                + row_shift
                                + local_x(index)
                                + task.list_chars() as f32 * cell_width
                                + (box_cells * cell_width - size).min(0.0);
                            paint_task_checkbox(
                                pixels,
                                self.width,
                                self.height,
                                box_x,
                                y,
                                row_h,
                                size,
                                line.checked == Some(true),
                                &palette,
                                density,
                            );
                        }
                    }
                }
            }
            if visual == cursor_visual && selection_end == selection_start {
                let cursor_col = chunk_start + cursor_offset;
                let x = text_x + row_shift + local_x(cursor_col);
                if self.editor.caret_visible {
                    rect(
                        pixels,
                        self.width,
                        self.height,
                        x,
                        y,
                        2.0 * density,
                        row_h,
                        palette.accent,
                    );
                }
                if self.editor.insertion_handle_visible {
                    insertion_handle = Some((x + 1.0 * density, y + row_h));
                }
            }
            if selection_end > selection_start {
                if visual == start_visual {
                    let col = chunk_start + start_offset;
                    start_handle = Some((text_x + row_shift + local_x(col), y + row_h));
                }
                if visual == end_visual {
                    let col = chunk_start + end_offset;
                    end_handle = Some((text_x + row_shift + local_x(col), y + row_h));
                }
            }
        }
        rect(pixels, self.width, self.height, 0.0, 0.0, gutter, self.height as f32, palette.editor_background);
        for visual in first..last {
            let chunk = self.editor.wrap[visual];
            let y = top_inset + self.editor.wrap_tops.get(visual).copied().unwrap_or(0.0) - scroll;
            // The gutter is repainted above the content, so the caret line wash
            // is laid down again here. A wrapped line owns several visual rows:
            // by default only the caret row keeps the gutter tinted; the
            // setting extends it to the whole logical line.
            if show_current_line
                && (visual == cursor_visual
                    || (self.gutter_wrapped_line_highlight && chunk.line == cursor_line))
            {
                rect(
                    pixels,
                    self.width,
                    self.height,
                    0.0,
                    y,
                    gutter,
                    chunk.height,
                    current_line_wash,
                );
            }
            if chunk.start != 0 {
                continue;
            }
            let number = (chunk.line + 1).to_string();
            let number_width = painter.width(&number, number_font_px, Face::Ui);
            let number_y = y + ((chunk.height - number_font_px) * 0.5).max(0.0);
            painter.draw(
                pixels,
                self.width,
                self.height,
                gutter - (GUTTER_PAD_DP * density).round() - number_width,
                number_y,
                &number,
                number_font_px,
                palette.marker,
                Face::Ui,
                false,
            );
        }
        if let Some((x, y)) = insertion_handle {
            if x >= gutter && y >= 0.0 && y <= self.height as f32 {
                draw_teardrop_handle(
                    pixels,
                    self.width,
                    self.height,
                    x,
                    y,
                    density,
                    2,
                    palette.accent,
                );
            }
        }
        if let Some((x, y)) = start_handle {
            if x >= gutter && y >= 0.0 && y <= self.height as f32 {
                draw_teardrop_handle(
                    pixels,
                    self.width,
                    self.height,
                    x,
                    y,
                    density,
                    0,
                    palette.accent,
                );
            }
        }
        if let Some((x, y)) = end_handle {
            if x >= gutter && y >= 0.0 && y <= self.height as f32 {
                draw_teardrop_handle(
                    pixels,
                    self.width,
                    self.height,
                    x,
                    y,
                    density,
                    1,
                    palette.accent,
                );
            }
        }
        self.editor.model = model;
        self.editor.starts = starts;
        self.editor.byte_starts = byte_starts;
        self.painter = painter;
    }

    fn tree_touch(&mut self, x: f32, y: f32, action: i32) -> Option<String> {
        let row_h = row_height(self.density);
        match action {
            0 => {
                self.tree.drag = Some(TreeDrag {
                    start_x: x,
                    start_y: y,
                    start_scroll_x: self.tree.scroll_x,
                    start_scroll: self.tree.scroll,
                    tap: true,
                    horizontal: None,
                });
                None
            }
            1 => {
                if let Some(mut drag) = self.tree.drag {
                    let dx = x - drag.start_x;
                    let dy = y - drag.start_y;
                    let threshold = 8.0 * self.density;
                    if drag.horizontal.is_none() && (dx.abs() > threshold || dy.abs() > threshold) {
                        drag.horizontal = Some(dx.abs() > dy.abs());
                    }
                    if let Some(horizontal) = drag.horizontal {
                        let was_tap = drag.tap;
                        drag.tap = false;
                        if horizontal {
                            let delta = if was_tap { dx - dx.signum() * threshold } else { dx };
                            let max_x = self.tree_max_x();
                            self.tree.scroll_x = (drag.start_scroll_x - delta).clamp(0.0, max_x);
                        } else {
                            let delta = if was_tap { dy - dy.signum() * threshold } else { dy };
                            let max_y = (self.tree.doc.rows.len() as f32 * row_h - self.height as f32).max(0.0);
                            self.tree.scroll = (drag.start_scroll - delta).clamp(0.0, max_y);
                        }
                    }
                    self.tree.drag = Some(drag);
                }
                None
            }
            2 => {
                let drag = self.tree.drag.take()?;
                if !drag.tap {
                    return None;
                }
                let index = ((y + self.tree.scroll) / row_h).floor().max(0.0) as usize;
                let row = self.tree.doc.rows.get(index)?;
                let kind = if row.folder {
                    "toggle"
                } else if row.note {
                    "open_note"
                } else {
                    "open_file"
                };
                Some(json!({ "kind": kind, "path": row.path }).to_string())
            }
            _ => None,
        }
    }

    /// Tap routing for the code-block chrome. Returns an action for Kotlin when
    /// the tap hit the copy control or folded/unfolded a reading-mode block,
    /// and `None` when the tap is an ordinary caret placement.
    fn editor_code_tap(&mut self, x: f32, y: f32) -> Option<String> {
        let (_advance, gutter, content) = self.editor_metrics();
        self.ensure_wrap(content);
        self.ensure_model();
        if self.editor.wrap.is_empty() {
            return None;
        }
        let scroll = self.editor.scroll;
        let top_inset = self.editor.top_inset.round();
        let visual = visual_at_y(&self.editor.wrap_tops, (y + scroll - top_inset).max(0.0))
            .min(self.editor.wrap.len().saturating_sub(1));
        let chunk = *self.editor.wrap.get(visual)?;
        if chunk.height <= 0.0 {
            return None;
        }
        // A tap on the painted checkbox toggles it, like the desktop. While the
        // marker column is open (the caret sits in it) the raw source is
        // editable and the tap is an ordinary caret placement.
        let line_start = self
            .editor
            .starts
            .get(chunk.line)
            .copied()
            .unwrap_or(0);
        let marker_open = self
            .editor
            .model
            .get(chunk.line)
            .filter(|line| line.kind == "task")
            .is_some_and(|line| {
                task_prefix_revealed(
                    line_start,
                    line.prefix_chars,
                    Some(self.editor.selection),
                    Some(self.selected_range()),
                )
            });
        if chunk.start == 0 && !marker_open {
            if let Some(line) = self.editor.model.get(chunk.line) {
                if line.kind == "task" {
                    if let Some(task) = crate::markdown::task_prefix(&line.raw) {
                        let font = self.font_for_line(chunk.line);
                        let face = self.line_face(chunk.line);
                        let cell = self.painter.advance_of_char(' ', font, face).max(1.0);
                        let indent = task.indent_chars.min(64);
                        let indent_w = self.painter.width(&" ".repeat(indent), font, face);
                        let size = (font * 1.15).max(16.0 * self.density);
                        let box_cells = task.checkbox.chars().count() as f32;
                        let bx = gutter
                            + indent_w
                            + task.list_chars() as f32 * cell
                            + (box_cells * cell - size).min(0.0);
                        let row_top = top_inset
                            + self
                                .editor
                                .wrap_tops
                                .get(visual)
                                .copied()
                                .unwrap_or(0.0)
                            - scroll;
                        let by = row_top + ((chunk.height - size) * 0.5).max(0.0);
                        if x >= bx && x <= bx + size && y >= by && y <= by + size {
                            if let Some((range, replacement)) =
                                crate::markdown::task_toggle(&line.raw)
                            {
                                let start = line_start
                                    + line.raw[..range.start].chars().count();
                                let end = start + line.raw[range.clone()].chars().count();
                                let (saved_selection, saved_anchor) =
                                    (self.editor.selection, self.editor.anchor);
                                self.replace_range(start, end, replacement);
                                // Toggling is not a caret move: the checkbox
                                // stays visible and the row does not reveal.
                                self.editor.selection = saved_selection;
                                self.editor.anchor = saved_anchor;
                                self.invalidate();
                                return Some(json!({ "kind": "task_toggle" }).to_string());
                            }
                        }
                    }
                }
            }
        }
        // A table grid tap chooses a caret inside the tapped cell and keeps
        // the grid; only tables switched to source take the normal caret path.
        if self
            .editor
            .model
            .get(chunk.line)
            .map(|row| is_table_kind(&row.kind))
            .unwrap_or(false)
        {
            let header = table_header_at(&self.editor.model, chunk.line)?;
            let label_px = code_label_px(self.code_font_px(), self.density);
            let (bx, bw, bh) =
                table_source_button(&mut self.painter, gutter, content, label_px, self.density);
            let header_visual = self
                .editor
                .wrap_first
                .get(header)
                .copied()
                .unwrap_or(visual);
            let header_top = top_inset
                + self
                    .editor
                    .wrap_tops
                    .get(header_visual)
                    .copied()
                    .unwrap_or(0.0)
                - scroll;
            let header_h = self
                .editor
                .wrap
                .get(header_visual)
                .map(|row| row.height)
                .unwrap_or(0.0);
            let by = header_top + ((header_h - bh) * 0.5).max(0.0);
            if x >= bx && x <= bx + bw && y >= by && y <= by + bh {
                if !self.editor.raw_tables.insert(header) {
                    self.editor.raw_tables.remove(&header);
                }
                self.editor.wrap_dirty = true;
                self.invalidate();
                return Some(json!({ "kind": "table_source" }).to_string());
            }
            if self.table_is_raw(header) {
                return None;
            }
            let (start, end) = table_bounds(&self.editor.model, chunk.line)?;
            let code_font = self.code_font_px();
            let code_face = self.code_face;
            let density = self.density;
            let layout = build_table_layout(
                &mut self.painter,
                &self.editor.model,
                start,
                end,
                content,
                code_font,
                code_face,
                density,
            );
            let raw = self.editor.model[chunk.line].raw.clone();
            let shift = self
                .editor
                .table_scroll
                .get(&header)
                .copied()
                .unwrap_or(0.0)
                .min((layout.indent + layout.total - content).max(0.0));
            let pad = (12.0 * density).round();
            let local = x - gutter - layout.indent + shift;
            let ranges = table_cell_ranges(&raw);
            let (column, cell_x) = table_column_at(local, &layout.widths, ranges.len());
            let width = layout.widths.get(column).copied().unwrap_or(0.0);
            let (range_start, range_end) = ranges
                .get(column)
                .copied()
                .unwrap_or((0, raw.chars().count()));
            // `target` is measured from the cell box; `cell_tap_offset` adds
            // the half padding itself, so it must not be subtracted here.
            let target = local - cell_x;
            let cell_raw: String = raw
                .chars()
                .skip(range_start)
                .take(range_end.saturating_sub(range_start))
                .collect();
            let offset = cell_tap_offset(
                &mut self.painter,
                &cell_raw,
                (range_start, range_end),
                width,
                pad,
                layout.aligns.get(column).copied().unwrap_or(TableAlign::Left),
                target,
                code_font,
                code_face,
            );
            let line_start = self
                .editor
                .starts
                .get(chunk.line)
                .copied()
                .unwrap_or(0);
            self.editor.selection = line_start + offset;
            self.editor.anchor = self.editor.selection;
            self.editor.insertion_handle_visible = true;
            self.invalidate();
            return Some(json!({ "kind": "table_cell" }).to_string());
        }
        let (start, end, open, block_id) = {
            let line = self.editor.model.get(chunk.line)?;
            if !is_code_kind(&line.kind) {
                return None;
            }
            let (start, end) = code_block_bounds(&self.editor.model, chunk.line)?;
            let (open, _) = code_block_fences(&self.editor.model, start, end);
            (start, end, open, line.block_id)
        };
        if chunk.line != open {
            return None;
        }
        let font = self.font_px();
        let label_px = code_label_px(font, self.density);
        let (pill_x, pill_w, pill_h) =
            code_copy_pill(&mut self.painter, gutter, content, label_px, self.density);
        let row_top =
            top_inset + self.editor.wrap_tops.get(visual).copied().unwrap_or(0.0) - scroll;
        let pill_y = row_top + ((chunk.height - pill_h) * 0.5).max(0.0);
        if x >= pill_x && x <= pill_x + pill_w && y >= pill_y && y <= pill_y + pill_h {
            let body = code_block_body(&self.editor.model, start, end);
            return Some(json!({ "kind": "copy_code", "text": body }).to_string());
        }
        if self.editor.preview {
            if let Some(id) = block_id {
                if !self.editor.code_collapsed.insert(id) {
                    self.editor.code_collapsed.remove(&id);
                }
            }
            self.editor.wrap_dirty = true;
            self.invalidate();
            return Some(json!({ "kind": "code_toggle" }).to_string());
        }
        None
    }

    fn editor_touch(&mut self, x: f32, y: f32, action: i32) -> bool {
        match action {
            0 => {
                self.editor.drag = Some((x, y, self.editor.scroll_x, self.editor.scroll, true));
                true
            }
            1 => {
                if let Some((start_x, start_y, start_scroll_x, start_scroll, tap)) = self.editor.drag {
                    let dx = x - start_x;
                    let dy = y - start_y;
                    let threshold = 8.0 * self.density;
                    if dx.abs() > threshold || dy.abs() > threshold || !tap {
                        if dx.abs() <= dy.abs() || !tap {
                            let (_advance, _, content) = self.editor_metrics();
                            self.ensure_wrap(content);
                            let max_y = (self.editor.top_inset + self.editor.wrap_height
                                + self.editor.bottom_inset
                                - self.height as f32)
                                .max(0.0);
                            let delta_y = if tap {
                                if dy.abs() > threshold {
                                    dy - dy.signum() * threshold
                                } else {
                                    0.0
                                }
                            } else {
                                dy
                            };
                            self.editor.scroll = (self.editor.scroll - delta_y).clamp(0.0, max_y);
                        }
                        self.editor.drag =
                            Some((x, y, start_scroll_x, start_scroll, false));
                        return true;
                    }
                    if tap {
                        self.editor.drag =
                            Some((start_x, start_y, start_scroll_x, start_scroll, true));
                    }
                }
                false
            }
            2 => {
                let drag = self.editor.drag.take();
                if drag.map(|value| !value.4).unwrap_or(true) {
                    return false;
                }
                self.invalidate();
                let position = self.editor_position_at(x, y);
                self.editor.selection = position;
                self.editor.anchor = position;
                self.editor.insertion_handle_visible = true;
                true
            }
            _ => false,
        }
    }

    fn editor_scroll_metrics(&mut self) -> (f32, f32, f32, f32) {
        let (advance, gutter, content) = self.editor_metrics();
        self.ensure_wrap(content);
        let max_y = (self.editor.top_inset + self.editor.wrap_height
            + self.editor.bottom_inset
            - self.viewport_height as f32)
            .max(0.0);
        (gutter, advance, max_y, 0.0)
    }

    /// Whether a table renders its raw source right now.
    fn table_is_raw(&self, header: usize) -> bool {
        self.editor.raw_tables.contains(&header)
            || self.editor.selection_tables.contains(&header)
    }

    fn editor_set_scroll(&mut self, y: f32, _x: f32) {
        let (_, _, max_y, _) = self.editor_scroll_metrics();
        self.editor.scroll = y.round().clamp(0.0, max_y);
        // The editor has no global horizontal scroll; each wide table keeps
        // its own pan in `table_scroll`.
        self.editor.scroll_x = 0.0;
    }

    /// Switches the table under the caret between grid and raw source.
    fn toggle_table_source(&mut self) -> bool {
        let (_, _, content) = self.editor_metrics();
        self.ensure_wrap(content);
        self.ensure_model();
        let line = line_of(&self.editor.starts, self.editor.selection)
            .min(self.editor.starts.len().saturating_sub(1));
        let header = table_header_at(&self.editor.model, line).or_else(|| {
            let scroll = self.editor.scroll + self.viewport_height as f32 * 0.5;
            let visual = visual_at_y(&self.editor.wrap_tops, scroll.max(0.0))
                .min(self.editor.wrap.len().saturating_sub(1));
            let center = self.editor.wrap.get(visual)?.line;
            table_header_at(&self.editor.model, center)
        });
        let Some(header) = header else {
            return false;
        };
        if !self.editor.raw_tables.insert(header) {
            self.editor.raw_tables.remove(&header);
        }
        self.editor.wrap_dirty = true;
        self.invalidate();
        true
    }

    /// Current horizontal pan and overflow of the table under `y`.
    fn table_scroll_state_at(&mut self, y: f32) -> (f32, f32) {
        let (_advance, _gutter, content) = self.editor_metrics();
        self.ensure_wrap(content);
        self.ensure_model();
        if self.editor.wrap.is_empty() {
            return (0.0, 0.0);
        }
        let scroll = self.editor.scroll;
        let top_inset = self.editor.top_inset.round();
        let visual = visual_at_y(
            &self.editor.wrap_tops,
            (y + scroll - top_inset).max(0.0),
        )
        .min(self.editor.wrap.len() - 1);
        let line = self.editor.wrap[visual].line;
        let code_font = self.code_font_px();
        let code_face = self.code_face;
        let density = self.density;
        let model = &self.editor.model;
        if !model
            .get(line)
            .map(|row| is_table_kind(&row.kind))
            .unwrap_or(false)
        {
            return (0.0, 0.0);
        }
        if table_header_at(model, line)
            .map(|header| self.table_is_raw(header))
            .unwrap_or(false)
        {
            return (0.0, 0.0);
        }
        let Some((start, end)) = table_bounds(model, line) else {
            return (0.0, 0.0);
        };
        let layout = build_table_layout(
            &mut self.painter,
            model,
            start,
            end,
            content,
            code_font,
            code_face,
            density,
        );
        let max = (layout.indent + layout.total - content).max(0.0);
        let x = self
            .editor
            .table_scroll
            .get(&start)
            .copied()
            .unwrap_or(0.0)
            .min(max);
        (x, max)
    }

    /// Applies a horizontal pan to the table under `y`.
    fn set_table_scroll_at(&mut self, y: f32, x: f32) {
        let (_, max) = self.table_scroll_state_at(y);
        let (_advance, _gutter, content) = self.editor_metrics();
        self.ensure_wrap(content);
        let scroll = self.editor.scroll;
        let top_inset = self.editor.top_inset.round();
        if self.editor.wrap.is_empty() {
            return;
        }
        let visual = visual_at_y(
            &self.editor.wrap_tops,
            (y + scroll - top_inset).max(0.0),
        )
        .min(self.editor.wrap.len() - 1);
        let line = self.editor.wrap[visual].line;
        if let Some(header) = table_header_at(&self.editor.model, line) {
            self.editor.table_scroll.insert(header, x.clamp(0.0, max));
            self.invalidate();
        }
    }

    fn editor_viewport(&mut self) -> String {
        let (_, _, max_y, _) = self.editor_scroll_metrics();
        json!({
            "scroll": self.editor.scroll,
            "scroll_x": 0.0,
            "max_y": max_y,
            "max_x": 0.0,
        })
        .to_string()
    }

    fn insert(&mut self, text: &str) {
        self.invalidate();
        self.editor.insertion_handle_visible = false;
        if text.is_empty() {
            return;
        }
        let sanitized;
        let text = if self.grid_table_cell_bounds().is_some() {
            sanitized = sanitize_table_cell_text(text);
            sanitized.as_str()
        } else {
            text
        };
        if text.is_empty() {
            return;
        }
        let sel_before = self.selection_now();
        let (start, end) = self.selected_range();
        let removed = self.remove_range(start, end);
        let at = self.editor.selection;
        let byte = byte_at(&self.editor.text, at);
        self.editor.text.insert_str(byte, text);
        self.editor.selection += text.chars().count();
        self.editor.anchor = self.editor.selection;
        self.record_edit(
            vec![TextChange {
                at,
                removed,
                inserted: text.to_string(),
            }],
            sel_before,
        );
        self.mark_edited();
        self.scroll_to_cursor();
    }

    /// Absolute content start and outer end (before the trailing pipe) of the
    /// grid cell holding the caret. Deletions and typing stay inside it.
    fn grid_table_cell_bounds(&self) -> Option<(usize, usize)> {
        let line = line_of(&self.editor.starts, self.editor.selection);
        let row = self.editor.model.get(line)?;
        if !is_table_kind(&row.kind) {
            return None;
        }
        let header = table_header_at(&self.editor.model, line)?;
        if self.table_is_raw(header) {
            return None;
        }
        let line_start = *self.editor.starts.get(line)?;
        let pos = self.editor.selection.saturating_sub(line_start);
        let contents = table_cell_ranges(&row.raw);
        let bounds = table_cell_bounds(&row.raw);
        let index = contents
            .iter()
            .position(|(start, end)| pos >= *start && pos <= *end)
            .or_else(|| {
                contents
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, (start, end))| {
                        if pos < *start {
                            start - pos
                        } else {
                            pos.saturating_sub(*end)
                        }
                    })
                    .map(|(index, _)| index)
            })?;
        let (content_start, _) = contents.get(index).copied()?;
        let (_, outer_end) = bounds.get(index).copied()?;
        Some((line_start + content_start, line_start + outer_end))
    }

    fn delete(&mut self, before: usize, after: usize) {
        self.invalidate();
        self.editor.insertion_handle_visible = false;
        let sel_before = self.selection_now();
        let (selection_start, selection_end) = self.selected_range();
        if selection_end > selection_start {
            let removed = self.remove_range(selection_start, selection_end);
            self.editor.selection = selection_start;
            self.editor.anchor = selection_start;
            self.record_edit(
                vec![TextChange {
                    at: selection_start,
                    removed,
                    inserted: String::new(),
                }],
                sel_before,
            );
            self.mark_edited();
            self.scroll_to_cursor();
            return;
        }
        let (mut before, mut after) = (before, after);
        if let Some((content_start, outer_end)) = self.grid_table_cell_bounds() {
            let cursor = self.editor.selection;
            let start = cursor.saturating_sub(before).max(content_start);
            let end = (cursor + after).min(outer_end);
            before = cursor.saturating_sub(start);
            after = end.saturating_sub(cursor);
            if before == 0 && after == 0 {
                return;
            }
        }
        let chars: Vec<char> = self.editor.text.chars().collect();
        let pos = self.editor.selection.min(chars.len());
        let (start, end) = if before == 1 && after == 0 {
            let count = prev_grapheme_char_count(&chars, pos);
            (pos.saturating_sub(count), pos)
        } else if before == 0 && after == 1 {
            let count = next_grapheme_char_count(&chars, pos);
            (pos, (pos + count).min(chars.len()))
        } else {
            let s = snap_to_cluster_start(&chars, pos.saturating_sub(before));
            let e = snap_to_cluster_end(&chars, (pos + after).min(chars.len()));
            (s, e)
        };
        if start >= end {
            return;
        }
        let removed = self.remove_range(start, end);
        self.editor.selection = start;
        self.editor.anchor = start;
        self.record_edit(
            vec![TextChange {
                at: start,
                removed,
                inserted: String::new(),
            }],
            sel_before,
        );
        self.mark_edited();
        self.scroll_to_cursor();
    }

    fn replace_range(&mut self, start: usize, end: usize, text: &str) {
        self.invalidate();
        self.editor.insertion_handle_visible = false;
        let length = char_count(&self.editor.text);
        let start = start.min(length);
        let end = end.max(start).min(length);
        let sel_before = self.selection_now();
        let removed = self.remove_range(start, end);
        if !text.is_empty() {
            let byte = byte_at(&self.editor.text, start);
            self.editor.text.insert_str(byte, text);
        }
        self.editor.selection = start + char_count(text);
        self.editor.anchor = self.editor.selection;
        if !removed.is_empty() || !text.is_empty() {
            self.record_edit(
                vec![TextChange {
                    at: start,
                    removed,
                    inserted: text.to_string(),
                }],
                sel_before,
            );
        }
        self.mark_edited();
        self.scroll_to_cursor();
    }

    fn selected_range(&self) -> (usize, usize) {
        let length = char_count(&self.editor.text);
        (
            self.editor.anchor.min(self.editor.selection).min(length),
            self.editor.anchor.max(self.editor.selection).min(length),
        )
    }

    fn delete_range(&mut self, start: usize, end: usize) {
        let _ = self.remove_range(start, end);
    }

    /// Removes chars `start..end` and returns them, leaving the caret at `start`.
    fn remove_range(&mut self, start: usize, end: usize) -> String {
        if start >= end {
            return String::new();
        }
        let removed = self.slice_chars(start, end);
        let start_byte = byte_at(&self.editor.text, start);
        let end_byte = byte_at(&self.editor.text, end);
        self.editor.text.replace_range(start_byte..end_byte, "");
        self.editor.selection = start;
        self.editor.anchor = start;
        removed
    }

    fn slice_chars(&self, start: usize, end: usize) -> String {
        let length = char_count(&self.editor.text);
        let start = start.min(length);
        let end = end.min(length).max(start);
        let start_byte = byte_at(&self.editor.text, start);
        let end_byte = byte_at(&self.editor.text, end);
        self.editor.text[start_byte..end_byte].to_string()
    }

    fn selection_now(&self) -> EditSelection {
        EditSelection {
            cursor: self.editor.selection,
            anchor: self.editor.anchor,
        }
    }

    fn record_edit(&mut self, changes: Vec<TextChange>, sel_before: EditSelection) {
        if changes.is_empty() {
            return;
        }
        self.editor.history.record(UndoStep {
            changes,
            sel_before,
            sel_after: self.selection_now(),
        });
    }

    fn mark_edited(&mut self) {
        self.editor.model_dirty = true;
        self.editor.wrap_dirty = true;
        self.editor.starts_dirty = true;
    }

    /// Applies one recorded change. Undo restores `removed`, redo `inserted`.
    fn apply_change(&mut self, change: &TextChange, redo: bool) {
        let length = char_count(&self.editor.text);
        let at = change.at.min(length);
        let replaced = if redo { &change.removed } else { &change.inserted };
        let replacement = if redo { &change.inserted } else { &change.removed };
        let replaced_len = char_count(replaced);
        let start_byte = byte_at(&self.editor.text, at);
        let end_byte = byte_at(&self.editor.text, (at + replaced_len).min(length));
        self.editor.text.replace_range(start_byte..end_byte, replacement);
    }

    fn after_history_change(&mut self) {
        self.editor.insertion_handle_visible = false;
        self.mark_edited();
        self.invalidate();
        self.scroll_to_cursor();
    }

    fn undo(&mut self) -> bool {
        let Some(step) = self.editor.history.take(false) else {
            return false;
        };
        for change in step.changes.iter().rev() {
            self.apply_change(change, false);
        }
        self.editor.selection = step.sel_before.cursor;
        self.editor.anchor = step.sel_before.anchor;
        self.after_history_change();
        true
    }

    fn redo(&mut self) -> bool {
        let Some(step) = self.editor.history.take(true) else {
            return false;
        };
        for change in step.changes.iter() {
            self.apply_change(change, true);
        }
        self.editor.selection = step.sel_after.cursor;
        self.editor.anchor = step.sel_after.anchor;
        self.after_history_change();
        true
    }

    fn scroll_to_cursor(&mut self) {
        let line_h = self.line_px();
        let (_, _, content) = self.editor_metrics();
        self.ensure_wrap(content);
        let (visual, _) = self.visual_for_position(&self.editor.starts, self.editor.selection);
        let top = self.editor.wrap_tops.get(visual).copied().unwrap_or(0.0);
        let row_h = self
            .editor
            .wrap
            .get(visual)
            .map(|chunk| chunk.height)
            .unwrap_or(line_h);
        let bottom = self.editor.top_inset + top + row_h + 24.0 * self.density;
        let visible = (self.viewport_height as f32 - self.editor.bottom_inset).max(line_h);
        if top < self.editor.scroll {
            self.editor.scroll = top;
        } else if bottom > self.editor.scroll + visible {
            self.editor.scroll = bottom - visible;
        }
        let max_y = (self.editor.top_inset + self.editor.wrap_height
            + self.editor.bottom_inset
            - self.viewport_height as f32)
            .max(0.0);
        self.editor.scroll = self.editor.scroll.clamp(0.0, max_y);
        self.editor.scroll_x = 0.0;
    }

    /// Scrolls so the selected line is vertically centered in the usable
    /// viewport. Used by search results; the regular reveal keeps a caret
    /// merely visible, which reads as "jumped to the top of the screen".
    fn scroll_to_cursor_centered(&mut self) {
        let line_h = self.line_px();
        let (_, _, content) = self.editor_metrics();
        self.ensure_wrap(content);
        let (visual, _) = self.visual_for_position(&self.editor.starts, self.editor.selection);
        let top = self.editor.wrap_tops.get(visual).copied().unwrap_or(0.0);
        let row_h = self
            .editor
            .wrap
            .get(visual)
            .map(|chunk| chunk.height)
            .unwrap_or(line_h);
        let visible = (self.viewport_height as f32
            - self.editor.top_inset
            - self.editor.bottom_inset)
            .max(line_h);
        let target = top + row_h * 0.5 - visible * 0.5;
        let max_y = (self.editor.top_inset + self.editor.wrap_height
            + self.editor.bottom_inset
            - self.viewport_height as f32)
            .max(0.0);
        self.editor.scroll = target.clamp(0.0, max_y);
        self.editor.scroll_x = 0.0;
    }

    fn editor_select_word_at(&mut self, x: f32, y: f32) -> bool {
        self.invalidate();
        let position = self.editor_position_at(x, y);
        let chars: Vec<char> = self.editor.text.chars().collect();
        let is_word = |character: char| character.is_alphanumeric() || character == '_';
        let mut start = position;
        while start > 0 && is_word(chars[start - 1]) {
            start -= 1;
        }
        let mut end = position;
        while end < chars.len() && is_word(chars[end]) {
            end += 1;
        }
        if start == end {
            let cluster_len = emoji_cluster_len(&chars, position);
            if cluster_len > 0 {
                start = position;
                end = position + cluster_len;
            } else {
                let prev_len = prev_grapheme_char_count(&chars, position);
                if prev_len > 1 || (position > 0 && emoji_cluster_len(&chars, position - prev_len) > 0) {
                    start = position - prev_len;
                    end = position;
                } else {
                    return false;
                }
            }
        }
        self.editor.anchor = start;
        self.editor.selection = end;
        self.editor.drag = None;
        self.scroll_to_cursor();
        true
    }

    /// Markdown markers hidden on a non-cursor line. Hit-testing and the
    /// selection handles share this, so taps and grips agree with the glyphs
    /// the editor actually paints.
    fn hidden_markers(&self, line: usize, chars: &[char]) -> Vec<bool> {
        let line_start = self.editor.starts.get(line).copied().unwrap_or(0);
        let cursor_line = if self.editor.preview {
            None
        } else {
            Some(line_of(&self.editor.starts, self.editor.selection))
        };
        compute_hidden(
            &self.editor.model,
            line,
            chars,
            line_start,
            cursor_line,
            self.editor.selection,
            self.selected_range(),
            self.editor.preview,
        )
    }

    fn editor_position_at(&mut self, x: f32, y: f32) -> usize {
        let (_, gutter, content) = self.editor_metrics();
        self.ensure_wrap(content);
        let visual = visual_at_y(
            &self.editor.wrap_tops,
            (y + self.editor.scroll - self.editor.top_inset).max(0.0),
        )
        .min(self.editor.wrap.len().saturating_sub(1));
        let Some(chunk) = self.editor.wrap.get(visual).copied() else {
            return 0;
        };
        let line_start = self.editor.starts.get(chunk.line).copied().unwrap_or(0);
        let raw = logical_line(&self.editor.text, &self.editor.byte_starts, chunk.line);
        let chars: Vec<char> = raw.chars().collect();
        let hidden = self.hidden_markers(chunk.line, &chars);
        let line_face = self.line_face(chunk.line);
        let code_face = self.code_face;
        let model = &self.editor.model;
        let continuation_indent =
            line_text_indent(&mut self.painter, model, chunk.line, chunk.font, line_face);
        let mut x_offsets = Vec::new();
        line_x_offsets(&mut x_offsets, &mut self.painter, &chars, &hidden, chunk.font, |index| {
            char_face(model, chunk.line, index, line_face, code_face)
        });
        let chunk_start = chunk.start.min(chars.len());
        let chunk_end = chunk.end.min(chars.len());
        let base_x = x_offsets.get(chunk_start).copied().unwrap_or(0.0);
        let row_shift = if chunk.start > 0 {
            continuation_indent
        } else {
            0.0
        };
        let target_x = (x - gutter - row_shift).max(0.0);

        let mut best_col = chunk_start;
        let mut best_dist = f32::MAX;
        for col in chunk_start..=chunk_end {
            if col > chunk_start && col < chunk_end {
                if x_offsets[col] == x_offsets[col.saturating_sub(1)] {
                    continue;
                }
            }
            let offset = x_offsets.get(col).copied().unwrap_or(base_x) - base_x;
            let dist = (offset - target_x).abs();
            if dist < best_dist {
                best_dist = dist;
                best_col = col;
            }
        }
        (line_start + best_col).min(char_count(&self.editor.text))
    }

    fn editor_state(&self) -> String {
        json!({
            "text": self.editor.text,
            "selection": self.editor.selection,
            "anchor": self.editor.anchor,
        })
        .to_string()
    }

    fn editor_selection_state(&self) -> String {
        json!({
            "selection": self.editor.selection,
            "anchor": self.editor.anchor,
        })
        .to_string()
    }

    fn editor_select_at(&mut self, x: f32, y: f32, extend: bool) -> bool {
        self.invalidate();
        let position = self.editor_position_at(x, y);
        if !extend {
            self.editor.anchor = position;
        }
        self.editor.selection = position;
        self.editor.drag = None;
        self.scroll_to_cursor();
        true
    }

    /// Top offset and total height of every logical line in the effective
    /// layout. A heading line can occupy several visual rows, all of them at
    /// the heading's taller height.
    fn line_extents(&self) -> (Vec<f32>, Vec<f32>) {
        let line_count = self.editor.wrap_first.len();
        let mut tops = Vec::with_capacity(line_count);
        let mut heights = Vec::with_capacity(line_count);
        for line in 0..line_count {
            let first = self.editor.wrap_first.get(line).copied().unwrap_or(0);
            let count = self.editor.wrap_count.get(line).copied().unwrap_or(0);
            let top = self
                .editor
                .wrap_tops
                .get(first)
                .copied()
                .unwrap_or(self.editor.wrap_height);
            let height = self
                .editor
                .wrap
                .get(first)
                .map(|chunk| chunk.height * count as f32)
                .unwrap_or(0.0);
            tops.push(top);
            heights.push(height);
        }
        (tops, heights)
    }

    fn editor_layout_info(&mut self) -> String {
        let (_advance, _, content) = self.editor_metrics();
        self.ensure_wrap(content);
        let (tops, heights) = self.line_extents();
        json!({
            "row_height": self.line_px(),
            "top": self.editor.top_inset,
            "first": self.editor.wrap_first,
            "count": self.editor.wrap_count,
            "tops": tops,
            "heights": heights,
            "total": self.editor.wrap.len(),
        })
        .to_string()
    }

    fn editor_selection_handles(&mut self) -> String {
        let (selection_start, selection_end) = self.selected_range();
        let line_h = self.line_px();
        let (_, gutter, content) = self.editor_metrics();
        self.ensure_wrap(content);
        let text_x = gutter;

        if selection_end > selection_start {
            let mut points = Vec::with_capacity(2);
            let mut handle_row = line_h;
            for position in [selection_start, selection_end] {
                let (visual, offset) = self.visual_for_position(&self.editor.starts, position);
                let top = self.editor.wrap_tops.get(visual).copied().unwrap_or(0.0);
                let mut row_h = line_h;
                let mut handle_x = text_x;
                if let Some(chunk) = self.editor.wrap.get(visual).copied() {
                    row_h = chunk.height;
                    let raw = logical_line(&self.editor.text, &self.editor.byte_starts, chunk.line);
                    let chars: Vec<char> = raw.chars().collect();
                    let hidden = self.hidden_markers(chunk.line, &chars);
                    let line_face = self.line_face(chunk.line);
                    let code_face = self.code_face;
                    let model = &self.editor.model;
                    let mut x_offsets = Vec::new();
                    line_x_offsets(&mut x_offsets, &mut self.painter, &chars, &hidden, chunk.font, |index| {
                        char_face(model, chunk.line, index, line_face, code_face)
                    });
                    let chunk_start = chunk.start.min(chars.len());
                    let base_x = x_offsets.get(chunk_start).copied().unwrap_or(0.0);
                    let col = (chunk_start + offset).min(chars.len());
                    handle_x = text_x + x_offsets.get(col).copied().unwrap_or(base_x) - base_x;
                }
                points.push(json!({
                    "x": handle_x,
                    "y": self.editor.top_inset + top - self.editor.scroll + row_h,
                }));
                handle_row = handle_row.max(row_h);
            }
            json!({
                "start": points[0],
                "end": points[1],
                "row_height": handle_row,
                "size": 24.0 * self.density,
            })
            .to_string()
        } else {
            let (visual, offset) = self.visual_for_position(&self.editor.starts, self.editor.selection);
            let top = self.editor.wrap_tops.get(visual).copied().unwrap_or(0.0);
            let mut row_h = line_h;
            let mut cursor_x = text_x + 1.0 * self.density;
            if let Some(chunk) = self.editor.wrap.get(visual).copied() {
                row_h = chunk.height;
                let raw = logical_line(&self.editor.text, &self.editor.byte_starts, chunk.line);
                let chars: Vec<char> = raw.chars().collect();
                let hidden = self.hidden_markers(chunk.line, &chars);
                let line_face = self.line_face(chunk.line);
                let code_face = self.code_face;
                let model = &self.editor.model;
                let mut x_offsets = Vec::new();
                line_x_offsets(&mut x_offsets, &mut self.painter, &chars, &hidden, chunk.font, |index| {
                    char_face(model, chunk.line, index, line_face, code_face)
                });
                let chunk_start = chunk.start.min(chars.len());
                let base_x = x_offsets.get(chunk_start).copied().unwrap_or(0.0);
                let col = (chunk_start + offset).min(chars.len());
                cursor_x = text_x + 1.0 * self.density
                    + x_offsets.get(col).copied().unwrap_or(base_x)
                    - base_x;
            }
            let cursor_y = self.editor.top_inset + top - self.editor.scroll + row_h;
            json!({
                "insertion": {
                    "x": cursor_x,
                    "y": cursor_y,
                },
                "row_height": row_h,
                "size": 24.0 * self.density,
            })
            .to_string()
        }
    }

    fn editor_drag_handle(&mut self, which: i32, x: f32, y: f32) -> bool {
        self.invalidate();
        if which == 2 {
            let position = self.editor_position_at(x, (y - 1.0).max(0.0));
            self.editor.anchor = position;
            self.editor.selection = position;
            self.editor.insertion_handle_visible = true;
            self.scroll_to_cursor();
            return true;
        }
        let (selection_start, selection_end) = self.selected_range();
        if selection_end <= selection_start {
            return false;
        }
        // Kotlin sends the grip's tip, which sits on the row boundary; move
        // just above it so the character row that owns the grip is used.
        let position = self.editor_position_at(x, (y - 1.0).max(0.0));
        let fixed = if which == 0 {
            selection_end
        } else {
            selection_start
        };
        self.editor.anchor = fixed;
        self.editor.selection = position;
        self.editor.drag = None;
        self.scroll_to_cursor();
        true
    }
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorSelectionState(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
) -> jstring {
    let result = with_surface(id, |state| state.editor_selection_state()).unwrap_or_default();
    to_java(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorGetSelection(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
) -> jlong {
    with_surface(id, |state| {
        let sel = (state.editor.selection as u64) & 0xffffffff;
        let anc = (state.editor.anchor as u64) & 0xffffffff;
        ((sel << 32) | anc) as i64
    })
    .unwrap_or(0)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorLayoutInfo(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
) -> jstring {
    let result = with_surface(id, |state| state.editor_layout_info()).unwrap_or_default();
    to_java(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorSelectionHandles(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
) -> jstring {
    let result = with_surface(id, |state| state.editor_selection_handles()).unwrap_or_default();
    to_java(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorDragHandle(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    which: jint,
    x: jfloat,
    y: jfloat,
) -> jboolean {
    with_surface(id, |state| state.editor_drag_handle(which, x, y))
        .unwrap_or(false) as jboolean
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorSetTopInset(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    inset: jfloat,
) {
    let _ = with_surface(id, |state| {
        state.invalidate();
        state.editor.top_inset = inset.max(0.0);
    });
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorSetBottomInset(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    inset: jfloat,
) {
    let _ = with_surface(id, |state| {
        state.invalidate();
        let previous = state.editor.bottom_inset;
        let next = inset.max(0.0);
        state.editor.bottom_inset = next;
        if next > previous + 0.5 {
            state.scroll_to_cursor();
        } else {
            let (_, _, max_y, _) = state.editor_scroll_metrics();
            state.editor.scroll = state.editor.scroll.min(max_y);
        }
    });
}

/// Reveals the caret without changing the viewport. Floating keyboards do not
/// report an IME inset, so the regular inset-driven reveal never runs for them.
#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorScrollToCursor(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
) {
    let _ = with_surface(id, |state| {
        state.invalidate();
        state.scroll_to_cursor();
    });
}

/// Centers the caret's line in the visible editor area. Search results use
/// this instead of the edge-reveal above.
#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorScrollToCursorCentered(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
) {
    let _ = with_surface(id, |state| {
        state.invalidate();
        state.scroll_to_cursor_centered();
    });
}

/// Replaces the in-page find highlight list. `active` is the index of the
/// accent-coloured match, or -1 for none.
#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorSetFindHighlights(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    ranges: JString<'_>,
    active: jint,
) {
    let Ok(raw) = env.get_string(&ranges) else {
        return;
    };
    let ranges: String = raw.into();
    let parsed = serde_json::from_str::<Vec<(usize, usize)>>(&ranges).unwrap_or_default();
    let _ = with_surface(id, |state| {
        state.editor.find_ranges = parsed;
        state.editor.find_active = if active >= 0 {
            Some(active as usize)
        } else {
            None
        };
        state.invalidate();
    });
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorClearFindHighlights(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
) {
    let _ = with_surface(id, |state| {
        state.editor.find_ranges.clear();
        state.editor.find_active = None;
        state.invalidate();
    });
}

/// Changes the editor's type size. Called from the settings screen, never
/// during a scroll: wrap and scroll metrics are rewritten here once, so the
/// next band paint stays a plain blit.
#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorSetFontSize(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    font_dp: jfloat,
    line_dp: jfloat,
) {
    let _ = with_surface(id, |state| {
        let font = font_dp.clamp(8.0, 32.0);
        let line = line_dp.clamp(8.0, 64.0);
        if (state.editor_font_dp - font).abs() < 0.01
            && (state.editor_line_dp - line).abs() < 0.01
        {
            return;
        }
        state.editor_font_dp = font;
        state.editor_line_dp = line;
        // Glyph rasters and metrics are keyed by pixel size; the old set would
        // otherwise stay resident next to the new one.
        state.painter.glyphs.clear();
        state.painter.metrics_cache.clear();
        state.painter.char_advances.clear();
        state.editor.wrap_dirty = true;
        let content = state.editor_metrics().2;
        state.ensure_wrap(content);
        let (_, _, max_y, _) = state.editor_scroll_metrics();
        state.editor.scroll = state.editor.scroll.min(max_y);
        state.invalidate();
    });
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorSetCodeFontSize(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    font_dp: jfloat,
) {
    let _ = with_surface(id, |state| {
        let font = font_dp.clamp(7.0, 28.0);
        if (state.code_font_dp - font).abs() < 0.01 {
            return;
        }
        state.code_font_dp = font;
        // Only fenced rows change height, but the whole wrap has to be rebuilt
        // because the bands and the scroll range follow it.
        state.editor.wrap_dirty = true;
        let content = state.editor_metrics().2;
        state.ensure_wrap(content);
        let (_, _, max_y, _) = state.editor_scroll_metrics();
        state.editor.scroll = state.editor.scroll.min(max_y);
        state.invalidate();
    });
}

fn face_from_jni(value: i32) -> Face {
    match value {
        1 => Face::Mono,
        2 => Face::Serif,
        _ => Face::Ui,
    }
}

#[derive(Deserialize)]
struct HeadingStyleJson {
    size: f32,
    color: u32,
    #[serde(default = "default_heading_bold")]
    bold: bool,
    #[serde(default)]
    italic: bool,
}

fn default_heading_bold() -> bool {
    true
}

/// Applies the per-level heading styles. A size change re-wraps the document,
/// exactly like a body-font change; colour and format only repaint.
#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorSetHeadings(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    json: JString<'_>,
) {
    let Ok(raw) = env.get_string(&json) else {
        return;
    };
    let raw: String = raw.into();
    let Ok(styles) = serde_json::from_str::<Vec<HeadingStyleJson>>(&raw) else {
        return;
    };
    if styles.len() != 6 {
        return;
    }
    let mut next = DEFAULT_HEADINGS;
    for (slot, style) in next.iter_mut().zip(styles) {
        *slot = HeadingStyle {
            size_dp: style.size.clamp(6.0, 64.0),
            color: style.color & 0x00ff_ffff,
            bold: style.bold,
            italic: style.italic,
        };
    }
    let _ = with_surface(id, |state| {
        if state.headings == next {
            return;
        }
        let size_changed = state
            .headings
            .iter()
            .zip(next.iter())
            .any(|(old, new)| (old.size_dp - new.size_dp).abs() > 0.01);
        state.headings = next;
        if size_changed {
            state.editor.wrap_dirty = true;
            let content = state.editor_metrics().2;
            state.ensure_wrap(content);
            let (_, _, max_y, _) = state.editor_scroll_metrics();
            state.editor.scroll = state.editor.scroll.min(max_y);
        }
        state.invalidate();
    });
}

/// Selects the text/code faces. Layout and glyphs are invalidated here once,
/// so a later frame is still a plain band paint.
#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorSetFonts(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    text_face: jint,
    code_face: jint,
) {
    let _ = with_surface(id, |state| {
        let text = face_from_jni(text_face);
        let code = face_from_jni(code_face);
        if state.text_face == text && state.code_face == code {
            return;
        }
        state.text_face = text;
        state.code_face = code;
        state.painter.glyphs.clear();
        state.painter.metrics_cache.clear();
        state.painter.char_advances.clear();
        state.editor.wrap_dirty = true;
        let content = state.editor_metrics().2;
        state.ensure_wrap(content);
        let (_, _, max_y, _) = state.editor_scroll_metrics();
        state.editor.scroll = state.editor.scroll.min(max_y);
        state.invalidate();
    });
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorSetBand(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    origin: jfloat,
) {
    let _ = with_surface(id, |state| {
        state.editor.band_origin = origin.max(0.0);
    });
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeSurfaceSetViewportHeight(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    height: jint,
) {
    let _ = with_surface(id, |state| {
        state.viewport_height = height.max(1) as usize;
    });
}

fn to_java(env: &mut JNIEnv<'_>, value: String) -> jstring {
    env.new_string(value)
        .map(|text| text.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

fn with_surface<T>(id: i32, action: impl FnOnce(&mut SurfaceState) -> T) -> Option<T> {
    let handle = surfaces().lock().ok()?.get(&id).cloned()?;
    let mut surface = handle.lock().ok()?;
    Some(action(&mut surface))
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeSurfaceAttach(
    env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    surface: JObject<'_>,
    width: jint,
    height: jint,
    density: jfloat,
) {
    let window = unsafe {
        ANativeWindow_fromSurface(env.get_native_interface(), surface.as_raw())
    };
    if window.is_null() {
        return;
    }
    let width = width.max(1) as usize;
    let height = height.max(1) as usize;
    unsafe {
        ANativeWindow_setBuffersGeometry(window, width as i32, height as i32, WINDOW_FORMAT_RGBA_8888);
    }
    let surface_ref = match surfaces().lock() {
        Ok(mut guard) => guard
            .entry(id)
            .or_insert_with(|| {
                Arc::new(Mutex::new(SurfaceState::new(
                    window,
                    width,
                    height,
                    density,
                )))
            })
            .clone(),
        Err(_) => return,
    };
    let mut state = match surface_ref.lock() {
        Ok(state) => state,
        Err(_) => return,
    };
    let old_window = state.window;
    state.window = window;
    state.width = width;
    state.height = height;
    state.viewport_height = height;
    state.density = if density > 0.0 { density } else { 1.0 };
    state.frame_valid = false;
    if !old_window.is_null() && old_window != window {
        unsafe {
            ANativeWindow_release(old_window);
        }
    }
    drop(state);
    if let Ok(mut input_guard) = inputs().lock() {
        input_guard.entry(id).or_insert_with(|| {
            Arc::new(InputState::new(
                if density > 0.0 { density } else { 1.0 },
            ))
        });
    }
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeSurfaceDetach(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
) {
    if let Ok(guard) = surfaces().lock() {
        if let Some(handle) = guard.get(&id) {
            if let Ok(mut surface) = handle.lock() {
                if !surface.window.is_null() {
                    unsafe {
                        ANativeWindow_release(surface.window);
                    }
                    surface.window = std::ptr::null_mut();
                }
            }
        }
    }
    if let Ok(mut guard) = inputs().lock() {
        guard.remove(&id);
    }
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeSurfaceResize(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    width: jint,
    height: jint,
) {
    let _ = with_surface(id, |state| {
        state.invalidate();
        state.width = width.max(1) as usize;
        state.height = height.max(1) as usize;
        state.viewport_height = state.height;
        unsafe {
            ANativeWindow_setBuffersGeometry(
                state.window,
                state.width as i32,
                state.height as i32,
                WINDOW_FORMAT_RGBA_8888,
            );
        }
        state.refresh();
    });
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeSurfaceBeginUpdate(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
) {
    let _ = with_surface(id, |state| state.suspend_render = true);
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeSurfaceEndUpdate(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
) {
    let _ = with_surface(id, |state| {
        state.suspend_render = false;
        state.render();
    });
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeSurfaceRender(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
) -> jfloat {
    let t0 = std::time::Instant::now();
    let (window, origin) = with_surface(id, |state| (state.window, state.editor.band_origin))
        .unwrap_or((std::ptr::null_mut(), 0.0));
    if window.is_null() {
        return origin;
    }
    let input = input_of(id);
    let mut buffer = ANativeWindowBuffer::default();
    let locked = unsafe {
        ANativeWindow_lock(window, &mut buffer, std::ptr::null_mut())
    };
    let t_lock = t0.elapsed();
    if locked != 0 || buffer.bits.is_null() {
        return origin;
    }
    let t1 = std::time::Instant::now();
    let rendered_origin = with_surface(id, |state| {
        // Apply the newest touch position only once the buffer is available:
        // locking can wait for the compositor, and a frame drawn with the
        // position from before that wait reads as a missed scroll tick.
        if let Some(input) = &input {
            apply_pending_scroll(input, state);
        }
        state.paint_locked(&buffer);
        if let Some(input) = &input {
            let mode = state.current_mode();
            input.mode.store(mode, Ordering::Relaxed);
            let max_y = match mode {
                1 => (state.tree.doc.rows.len() as f32 * row_height(state.density)
                    - state.height as f32)
                    .max(0.0),
                2 => state.editor_scroll_metrics().2,
                _ => 0.0,
            };
            input.max_y_bits.store(max_y.to_bits(), Ordering::Relaxed);
        }
        state.editor.band_origin
    })
    .unwrap_or(origin);
    let t_paint = t1.elapsed();
    let t2 = std::time::Instant::now();
    unsafe {
        ANativeWindow_unlockAndPost(window);
    }
    let t_post = t2.elapsed();
    android_log!(
        3,
        b"VibecodiumRust\0".as_ptr(),
        b"nativeSurfaceRender: lock=%.2fms, paint=%.2fms, post=%.2fms\0".as_ptr(),
        t_lock.as_secs_f64() * 1000.0,
        t_paint.as_secs_f64() * 1000.0,
        t_post.as_secs_f64() * 1000.0,
    );
    rendered_origin
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeSurfaceScroll(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    y: jfloat,
) {
    if let Some(input) = input_of(id) {
        input.scroll_bits.store(y.to_bits(), Ordering::Relaxed);
        input.scroll_dirty.store(true, Ordering::Relaxed);
    }
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeSurfaceInvalidate(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
) {
    // Only the cache is dropped; the caller paints once on the render thread,
    // so a full repaint never happens at an arbitrary phase of the frame.
    let _ = with_surface(id, |state| state.invalidate());
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeThemeSetPalette(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    json: JString<'_>,
) {
    let Ok(text) = env.get_string(&json) else {
        return;
    };
    let text: String = text.into();
    let Ok(value) = serde_json::from_str::<SurfacePalette>(&text) else {
        return;
    };
    set_palette(value.clone());
    if let Ok(map) = surfaces().lock() {
        for surface in map.values() {
            if let Ok(mut state) = surface.lock() {
                state.background = match state.content_kind {
                    1 => value.tree_background,
                    _ => value.editor_background,
                };
                state.invalidate();
            }
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeSurfaceSetTree(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    json: JString<'_>,
) {
    let Ok(text) = env.get_string(&json) else {
        return;
    };
    let text: String = text.into();
    let doc = serde_json::from_str::<TreeDoc>(&text).unwrap_or_default();
    let _ = with_surface(id, |state| {
        state.invalidate();
        state.background = palette().tree_background;
        state.content_kind = 1;
        state.tree.doc = doc;
        let max = (state.tree.doc.rows.len() as f32 * row_height(state.density)
            - state.height as f32)
            .max(0.0);
        state.tree.scroll = state.tree.scroll.min(max);
        state.invalidate();
    });
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeTreeViewport(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
) -> jstring {
    drain_input(id);
    let result = with_surface(id, |state| state.tree_viewport()).unwrap_or_default();
    to_java(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeTreeSetScroll(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    y: jfloat,
) {
        if let Some(input) = input_of(id) {
        input.scroll_dirty.store(false, Ordering::Relaxed);
    }
    let _ = with_surface(id, |state| state.tree_set_scroll(y));
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeTreeSetScrollX(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    x: jfloat,
) {
    let _ = with_surface(id, |state| state.tree_set_scroll_x(x));
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeTreeScroll(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
) -> jfloat {
    drain_input(id);
    with_surface(id, |state| state.tree.scroll).unwrap_or(0.0)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeSurfaceSetEditor(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    text: JString<'_>,
    selection: jint,
    language: JString<'_>,
) {
    let Ok(text) = env.get_string(&text) else {
        return;
    };
    let text: String = text.into();
    let language = env
        .get_string(&language)
        .map(String::from)
        .unwrap_or_default();
    let _ = with_surface(id, |state| {
        state.invalidate();
        state.background = palette().editor_background;
        state.content_kind = 2;
        if state.editor.text != text {
            state.editor.text = text;
            state.editor.model_dirty = true;
            state.editor.wrap_dirty = true;
            state.editor.starts_dirty = true;
            // A different document was loaded: its undo history and table pans
            // are separate.
            state.editor.history.clear();
            state.editor.table_scroll.clear();
            state.editor.raw_tables.clear();
        }
        if state.editor.language != language {
            state.editor.language = language;
            state.editor.model_dirty = true;
        }
        let chars: Vec<char> = state.editor.text.chars().collect();
        state.editor.selection = snap_to_cluster_boundary(&chars, (selection.max(0) as usize).min(chars.len()));
        state.invalidate();
    });
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorSetCaretVisible(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    visible: jboolean,
) {
    let _ = with_surface(id, |state| {
        state.invalidate();
        state.editor.caret_visible = visible != 0;
    });
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorSetInsertionHandleVisible(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    visible: jboolean,
) {
    let _ = with_surface(id, |state| {
        state.invalidate();
        state.editor.insertion_handle_visible = visible != 0;
    });
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorSetPreview(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    preview: jboolean,
) {
    let _ = with_surface(id, |state| {
        state.invalidate();
        let preview = preview != 0;
        if state.editor.preview != preview {
            // Reading mode collapses table source lines and paints them as a
            // grid, so the canonical wrap has to be rebuilt.
            state.editor.preview = preview;
            state.editor.wrap_dirty = true;
        }
    });
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorSetCurrentLineHighlight(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    enabled: jboolean,
) {
    let _ = with_surface(id, |state| {
        let enabled = enabled != 0;
        if state.current_line_highlight != enabled {
            state.current_line_highlight = enabled;
            state.invalidate();
        }
    });
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorSetGutterWrappedLineHighlight(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    enabled: jboolean,
) {
    let _ = with_surface(id, |state| {
        let enabled = enabled != 0;
        if state.gutter_wrapped_line_highlight != enabled {
            state.gutter_wrapped_line_highlight = enabled;
            state.invalidate();
        }
    });
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeSurfaceTouch(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    x: jfloat,
    y: jfloat,
    action: jint,
) -> jstring {
    let result = match action {
        // DOWN: take the lock briefly, seed the input channel.
        0 => {
            let result = with_surface(id, |state| {
                // Fold in a fling tick that was queued just before this touch,
                // so the drag base matches what the last frame showed.
                if let Some(input) = input_of(id) {
                    apply_pending_scroll(&input, state);
                }
                let touch = if state.tree.doc.rows.is_empty() {
                    state.editor_touch(x, y, action);
                    None
                } else {
                    state.tree_touch(x, y, action)
                };
                if let Some(input) = input_of(id) {
                    let mode: u8 = if state.tree.doc.rows.is_empty() { 2 } else { 1 };
                    let max_y = if state.tree.doc.rows.is_empty() {
                        state.editor_scroll_metrics().2
                    } else {
                        (state.tree.doc.rows.len() as f32 * row_height(state.density)
                            - state.height as f32)
                            .max(0.0)
                    };
                    let scroll = if state.tree.doc.rows.is_empty() {
                        state.editor.scroll
                    } else {
                        state.tree.scroll
                    };
                    let max_x = if state.tree.doc.rows.is_empty() {
                        0.0
                    } else {
                        state.tree_max_x()
                    };
                    let scroll_x = if state.tree.doc.rows.is_empty() {
                        0.0
                    } else {
                        state.tree.scroll_x
                    };
                    input.mode.store(mode, Ordering::Relaxed);
                    input.max_y_bits.store(max_y.to_bits(), Ordering::Relaxed);
                    input.max_x_bits.store(max_x.to_bits(), Ordering::Relaxed);
                    input.drag_active.store(true, Ordering::Relaxed);
                    input.moved.store(false, Ordering::Relaxed);
                    input.axis.store(0, Ordering::Relaxed);
                    input.start_x_bits.store(x.to_bits(), Ordering::Relaxed);
                    input.start_y_bits.store(y.to_bits(), Ordering::Relaxed);
                    input.start_scroll_bits.store(scroll.round().to_bits(), Ordering::Relaxed);
                    input.start_scroll_x_bits
                        .store(scroll_x.round().to_bits(), Ordering::Relaxed);
                    input.last_y_bits.store(y.to_bits(), Ordering::Relaxed);
                    input.last_x_bits.store(x.to_bits(), Ordering::Relaxed);
                    input.scroll_bits.store(scroll.round().to_bits(), Ordering::Relaxed);
                    input.scroll_x_bits.store(scroll_x.round().to_bits(), Ordering::Relaxed);
                    input.scroll_dirty.store(false, Ordering::Relaxed);
                    input.scroll_x_dirty.store(false, Ordering::Relaxed);
                }
                touch
            })
            .flatten()
            .unwrap_or_default();
            result
        }
        // MOVE: update the scroll without touching the paint mutex.
        1 => {
            if let Some(input) = input_of(id) {
                if !input.drag_active.load(Ordering::Relaxed) {
                    return to_java(&mut env, String::new());
                }
                let density = f32::from_bits(input.density_bits.load(Ordering::Relaxed));
                let threshold = 8.0 * density;
                let start_x = f32::from_bits(input.start_x_bits.load(Ordering::Relaxed));
                let start_y = f32::from_bits(input.start_y_bits.load(Ordering::Relaxed));
                let dx = x - start_x;
                let dy = y - start_y;
                let was_moved = input.moved.load(Ordering::Relaxed);
                if !was_moved {
                    if dy.abs() > threshold || dx.abs() > threshold {
                        input.moved.store(true, Ordering::Relaxed);
                        let horizontal = input.mode.load(Ordering::Relaxed) == 1
                            && dx.abs() > dy.abs();
                        input.axis.store(if horizontal { 2 } else { 1 }, Ordering::Relaxed);
                        if horizontal {
                            let delta_x = dx - dx.signum() * threshold;
                            let start_scroll_x = f32::from_bits(
                                input.start_scroll_x_bits.load(Ordering::Relaxed),
                            );
                            let max_x = f32::from_bits(input.max_x_bits.load(Ordering::Relaxed));
                            let next_scroll = (start_scroll_x - delta_x).clamp(0.0, max_x);
                            input.scroll_x_bits.store(next_scroll.to_bits(), Ordering::Relaxed);
                            input.last_x_bits.store(x.to_bits(), Ordering::Relaxed);
                            input.scroll_x_dirty.store(true, Ordering::Relaxed);
                        } else {
                            let delta_y = if dy.abs() > threshold {
                                dy - dy.signum() * threshold
                            } else {
                                0.0
                            };
                            let last_scroll =
                                f32::from_bits(input.scroll_bits.load(Ordering::Relaxed));
                            let max_y = f32::from_bits(input.max_y_bits.load(Ordering::Relaxed));
                            let next_scroll = (last_scroll - delta_y).clamp(0.0, max_y);
                            input.scroll_bits.store(next_scroll.to_bits(), Ordering::Relaxed);
                            input.last_y_bits.store(y.to_bits(), Ordering::Relaxed);
                            input.scroll_dirty.store(true, Ordering::Relaxed);
                        }
                    }
                } else if input.axis.load(Ordering::Relaxed) == 2 {
                    let last_x = f32::from_bits(input.last_x_bits.load(Ordering::Relaxed));
                    let delta_x = x - last_x;
                    let last_scroll_x =
                        f32::from_bits(input.scroll_x_bits.load(Ordering::Relaxed));
                    let max_x = f32::from_bits(input.max_x_bits.load(Ordering::Relaxed));
                    let next_scroll = (last_scroll_x - delta_x).clamp(0.0, max_x);
                    input.scroll_x_bits.store(next_scroll.to_bits(), Ordering::Relaxed);
                    input.last_x_bits.store(x.to_bits(), Ordering::Relaxed);
                    input.scroll_x_dirty.store(true, Ordering::Relaxed);
                } else {
                    let last_y = f32::from_bits(input.last_y_bits.load(Ordering::Relaxed));
                    let delta_y = y - last_y;
                    let last_scroll =
                        f32::from_bits(input.scroll_bits.load(Ordering::Relaxed));
                    let max_y = f32::from_bits(input.max_y_bits.load(Ordering::Relaxed));
                    let next_scroll = (last_scroll - delta_y).clamp(0.0, max_y);
                    input.scroll_bits.store(next_scroll.to_bits(), Ordering::Relaxed);
                    input.last_y_bits.store(y.to_bits(), Ordering::Relaxed);
                    input.scroll_dirty.store(true, Ordering::Relaxed);
                }
                String::new()
            } else {
                String::new()
            }
        }
        // UP: fold pending scroll in, break the tap flag, then run the
        // original tap/fling decision under the lock.
        2 => {
            let result = with_surface(id, |state| {
                if let Some(input) = input_of(id) {
                    input.drag_active.store(false, Ordering::Relaxed);
                    if input.moved.load(Ordering::Relaxed) {
                        if let Some(drag) = state.editor.drag.as_mut() {
                            drag.4 = false;
                        }
                        if let Some(drag) = state.tree.drag.as_mut() {
                            drag.tap = false;
                        }
                    }
                    apply_pending_scroll(&input, state);
                }
                if state.tree.doc.rows.is_empty() {
                    state.editor_touch(x, y, action);
                    None
                } else {
                    state.tree_touch(x, y, action)
                }
            })
            .flatten()
            .unwrap_or_default();
            result
        }
        _ => String::new(),
    };
    to_java(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorInsert(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    text: JString<'_>,
) -> jstring {
    let Ok(text) = env.get_string(&text) else {
        return to_java(&mut env, String::new());
    };
    let text: String = text.into();
    let result = with_surface(id, |state| {
        state.insert(&text);
        state.editor_state()
    })
    .unwrap_or_default();
    to_java(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorReplace(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    start: jint,
    end: jint,
    text: JString<'_>,
) -> jstring {
    let value = env
        .get_string(&text)
        .map(String::from)
        .unwrap_or_default();
    let result = with_surface(id, |state| {
        state.replace_range(start.max(0) as usize, end.max(0) as usize, &value);
        state.editor_state()
    })
    .unwrap_or_default();
    to_java(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorDelete(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    before: jint,
    after: jint,
) -> jstring {
    let result = with_surface(id, |state| {
        state.delete(before.max(0) as usize, after.max(0) as usize);
        state.editor_state()
    })
    .unwrap_or_default();
    to_java(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorUndo(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
) -> jstring {
    let result = with_surface(id, |state| {
        state.undo();
        state.editor_state()
    })
    .unwrap_or_default();
    to_java(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorRedo(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
) -> jstring {
    let result = with_surface(id, |state| {
        state.redo();
        state.editor_state()
    })
    .unwrap_or_default();
    to_java(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorCanUndo(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
) -> jboolean {
    with_surface(id, |state| state.editor.history.can_undo()).unwrap_or(false) as jboolean
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorCanRedo(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
) -> jboolean {
    with_surface(id, |state| state.editor.history.can_redo()).unwrap_or(false) as jboolean
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorSelection(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    start: jint,
    end: jint,
) {
    let _ = with_surface(id, |state| {
        state.invalidate();
        let chars: Vec<char> = state.editor.text.chars().collect();
        let length = chars.len();
        let start = snap_to_cluster_boundary(&chars, (start.max(0) as usize).min(length));
        let end = snap_to_cluster_boundary(&chars, (end.max(0) as usize).min(length));
        state.editor.anchor = start;
        state.editor.selection = end;
    });
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorTapAction(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    x: jfloat,
    y: jfloat,
) -> jstring {
    let result = with_surface(id, |state| state.editor_code_tap(x, y))
        .flatten()
        .unwrap_or_default();
    to_java(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorSelectAt(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    x: jfloat,
    y: jfloat,
    extend: jboolean,
) {
    let _ = with_surface(id, |state| {
        state.editor_select_at(x, y, extend != 0);
    });
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorSelectWord(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    x: jfloat,
    y: jfloat,
) {
    let _ = with_surface(id, |state| {
        state.editor_select_word_at(x, y);
    });
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorViewport(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
) -> jstring {
    drain_input(id);
    let result = with_surface(id, |state| state.editor_viewport()).unwrap_or_default();
    to_java(&mut env, result)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorTableScrollMax(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    y: jfloat,
) -> jfloat {
    drain_input(id);
    with_surface(id, |state| state.table_scroll_state_at(y).1).unwrap_or(0.0)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorTableScrollX(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    y: jfloat,
) -> jfloat {
    drain_input(id);
    with_surface(id, |state| state.table_scroll_state_at(y).0).unwrap_or(0.0)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorSetTableScroll(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    y: jfloat,
    x: jfloat,
) {
    let _ = with_surface(id, |state| state.set_table_scroll_at(y, x));
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorToggleTableSource(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
) -> jboolean {
    with_surface(id, |state| state.toggle_table_source()).unwrap_or(false) as jboolean
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorSetScroll(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
    y: jfloat,
    x: jfloat,
) {
    if let Some(input) = input_of(id) {
        input.scroll_dirty.store(false, Ordering::Relaxed);
    }
    let _ = with_surface(id, |state| {
        state.editor_set_scroll(y, x);
    });
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorScroll(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
) -> jfloat {
    drain_input(id);
    with_surface(id, |state| state.editor.scroll).unwrap_or(0.0)
}

#[no_mangle]
pub extern "system" fn Java_md_vibecodium_mobile_CoreBridge_nativeEditorState(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    id: jint,
) -> jstring {
    let result = with_surface(id, |state| state.editor_state()).unwrap_or_default();
    to_java(&mut env, result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emoji_cluster_len() {
        let single: Vec<char> = "😀".chars().collect();
        assert_eq!(emoji_cluster_len(&single, 0), 1);

        let heart: Vec<char> = "❤️".chars().collect();
        assert_eq!(heart.len(), 2);
        assert_eq!(emoji_cluster_len(&heart, 0), 2);

        let warning: Vec<char> = "⚠️".chars().collect();
        assert_eq!(warning.len(), 2);
        assert_eq!(emoji_cluster_len(&warning, 0), 2);

        let flag: Vec<char> = "🇷🇺".chars().collect();
        assert_eq!(flag.len(), 2);
        assert_eq!(emoji_cluster_len(&flag, 0), 2);

        let family: Vec<char> = "👨‍👩‍👧‍👦".chars().collect();
        assert_eq!(family.len(), 7);
        assert_eq!(emoji_cluster_len(&family, 0), 7);

        let tone: Vec<char> = "👍🏽".chars().collect();
        assert_eq!(tone.len(), 2);
        assert_eq!(emoji_cluster_len(&tone, 0), 2);

        let text: Vec<char> = "abc".chars().collect();
        assert_eq!(emoji_cluster_len(&text, 0), 0);
    }

    #[test]
    fn test_line_cell_offsets_emojis() {
        // "a😀b"
        let line1: Vec<char> = "a😀b".chars().collect();
        let off1 = line_cell_offsets(&line1, &[]);
        assert_eq!(off1, vec![0, 1, 3, 4]);

        // "a❤️b" where ❤️ is \u{2764}\u{fe0f}
        let line2: Vec<char> = "a❤️b".chars().collect();
        assert_eq!(line2.len(), 4);
        let off2 = line_cell_offsets(&line2, &[]);
        assert_eq!(off2, vec![0, 1, 3, 3, 4]);

        // "a👨‍👩‍👧‍👦b"
        let line3: Vec<char> = "a👨‍👩‍👧‍👦b".chars().collect();
        let off3 = line_cell_offsets(&line3, &[]);
        assert_eq!(off3, vec![0, 1, 3, 3, 3, 3, 3, 3, 3, 4]);

        // "中文"
        let line4: Vec<char> = "中文".chars().collect();
        let off4 = line_cell_offsets(&line4, &[]);
        assert_eq!(off4, vec![0, 2, 4]);
    }

    #[test]
    fn test_grapheme_delete_counts() {
        let text1: Vec<char> = "a❤️b".chars().collect();
        // Cursor after ❤️ (index 3)
        assert_eq!(prev_grapheme_char_count(&text1, 3), 2);
        // Cursor after b (index 4)
        assert_eq!(prev_grapheme_char_count(&text1, 4), 1);

        let text2: Vec<char> = "a👨‍👩‍👧‍👦b".chars().collect();
        // Cursor after family (index 8)
        assert_eq!(prev_grapheme_char_count(&text2, 8), 7);
    }

    #[test]
    fn test_snap_to_cluster_boundary() {
        let text: Vec<char> = "a❤️b".chars().collect();
        assert_eq!(snap_to_cluster_boundary(&text, 0), 0);
        assert_eq!(snap_to_cluster_boundary(&text, 1), 1);
        // index 2 is between \u{2764} and \u{fe0f} inside ❤️
        assert_eq!(snap_to_cluster_boundary(&text, 2), 3);
        assert_eq!(snap_to_cluster_boundary(&text, 3), 3);
        assert_eq!(snap_to_cluster_boundary(&text, 4), 4);
    }

    #[test]
    fn heading_rows_grow_on_the_four_dp_grid() {
        let base = 22.0;
        assert_eq!(visual_line_height(15.0, base, 1.0), base);
        assert_eq!(visual_line_height(16.0, base, 1.0), base);
        assert_eq!(visual_line_height(17.0, base, 1.0), 24.0);
        assert_eq!(visual_line_height(18.0, base, 1.0), 28.0);
        assert_eq!(visual_line_height(19.0, base, 1.0), 28.0);
        assert_eq!(visual_line_height(40.0, base, 1.0), 56.0);
    }

    #[test]
    fn test_wrap_layout_emoji_boundary() {
        let text = "hello 😀 world";
        let starts = vec![0];
        let mut painter = Painter::new();
        // Without system fonts the painter falls back to 0.6 * px per cell.
        let px = 15.0;
        let line_fonts = vec![(px, Face::Mono)];
        // Content width of 90px fits 10 cells of 9px.
        // "hello " is 6 cells, 😀 is 2 cells -> 8 cells.
        // "world" is 5 cells, cannot fit on line 1 (8 + 5 = 13 > 10).
        // It wraps after the space at index 7: line 1 gets "hello 😀 ", line 2 gets "world".
        let rows = wrap_layout(
            text,
            &starts,
            90.0,
            &mut painter,
            &line_fonts,
            &[],
            Face::Mono,
            px,
            22.0,
            1.0,
            false,
            &HashSet::new(),
            &HashSet::new(),
            &HashSet::new(),
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].start, 0);
        assert_eq!(rows[0].end, 8); // "hello 😀 "
        assert_eq!(rows[1].start, 8); // "world"
    }

    #[test]
    fn test_line_x_offsets_mono_matches_cells() {
        let mut painter = Painter::new();
        let px = 15.0;
        let cell = painter.advance(px, Face::Mono);
        for text in ["a😀b", "a❤️b", "a👨‍👩‍👧‍👦b", "中文"] {
            let chars: Vec<char> = text.chars().collect();
            let cells = line_cell_offsets(&chars, &[]);
            let mut offsets = Vec::new();
            line_x_offsets(&mut offsets, &mut painter, &chars, &[], px, |_| Face::Mono);
            assert_eq!(offsets.len(), cells.len(), "{text}");
            for (x, cell_index) in offsets.iter().zip(cells.iter()) {
                assert!(
                    (x - *cell_index as f32 * cell).abs() < 0.01,
                    "{text}: {offsets:?} vs {cells:?}"
                );
            }
        }
    }

    #[test]
    fn test_line_x_offsets_proportional_uses_measured_widths() {
        let mut painter = Painter::new();
        let px = 15.0;
        let chars: Vec<char> = "a😀b".chars().collect();
        let mut offsets = Vec::new();
        line_x_offsets(&mut offsets, &mut painter, &chars, &[], px, |_| Face::Ui);
        // Without system fonts every non-emoji glyph falls back to 0.6 * px;
        // the emoji keeps its square advance in proportional mode.
        assert_eq!(offsets, vec![0.0, 9.0, 24.0, 33.0]);
    }

    #[test]
    fn test_fit_name_chars_reserves_room_for_ellipsis() {
        let mut painter = Painter::new();
        let px = 10.0;
        let chars: Vec<char> = "abcdefghijklmnop".chars().collect();
        // Each fallback glyph is 0.6 * px = 6, and so is the ellipsis.
        assert_eq!(fit_name_chars(&mut painter, &chars, px, 6.0 + 10.0 * 6.0), 10);
        // A row too narrow for any name still reserves only the ellipsis.
        assert_eq!(fit_name_chars(&mut painter, &chars, px, 6.0), 0);
        assert_eq!(fit_name_chars(&mut painter, &chars, px, 5.0), 0);
    }

    #[test]
    fn test_fit_name_chars_drops_space_before_ellipsis() {
        let mut painter = Painter::new();
        let px = 10.0;
        let chars: Vec<char> = "abc def ".chars().collect();
        assert_eq!(fit_name_chars(&mut painter, &chars, px, 6.0 + 8.0 * 6.0), 7);
    }

    #[test]
    fn table_bounds_cover_header_separator_and_rows() {
        let model = crate::markdown::model("| A | B |\n| --- | --- |\n| 1 | 2 |\n\ntail\n");
        assert_eq!(table_bounds(&model, 0), Some((0, 2)));
        assert_eq!(table_bounds(&model, 1), Some((0, 2)));
        assert_eq!(table_bounds(&model, 2), Some((0, 2)));
        assert_eq!(table_bounds(&model, 3), None);
    }

    #[test]
    fn table_alignment_reads_the_separator_colons() {
        assert_eq!(table_align("---"), TableAlign::Left);
        assert_eq!(table_align(":---"), TableAlign::Left);
        assert_eq!(table_align("---:"), TableAlign::Right);
        assert_eq!(table_align(":---:"), TableAlign::Center);
    }

    #[test]
    fn table_cells_paint_their_text_without_markup() {
        assert_eq!(cell_visible_text("**bold** text"), "bold text");
        assert_eq!(cell_visible_text("[link](https://x)"), "link");
        assert_eq!(cell_visible_text("`code`"), "code");
        assert_eq!(cell_visible_text("plain"), "plain");
    }

    #[test]
    fn table_layout_fills_the_view_and_keeps_alignment() {
        let model = crate::markdown::model("| A | B |\n| :--- | ---: |\n| 1 | 2 |\n");
        let mut painter = Painter::new();
        let layout = build_table_layout(&mut painter, &model, 0, 2, 300.0, 15.0, Face::Mono, 1.0);
        assert_eq!(layout.cols, 2);
        assert_eq!(layout.aligns, vec![TableAlign::Left, TableAlign::Right]);
        assert!(layout.widths.iter().all(|width| *width > 0.0));
        assert!((layout.total - 300.0).abs() < 0.01);
    }

    #[test]
    fn table_layout_keeps_a_nested_tables_indent() {
        let model = crate::markdown::model("   | A | B |\n   | --- | --- |\n   | 1 | 2 |");
        let mut painter = Painter::new();
        let layout = build_table_layout(&mut painter, &model, 0, 2, 300.0, 15.0, Face::Mono, 1.0);
        assert!(layout.indent > 0.0, "indent {layout:?}");
        assert!(
            (layout.indent + layout.total - 300.0).abs() < 0.01,
            "indented table still fills the remaining width: {layout:?}"
        );
    }

    #[test]
    fn selecting_a_table_shows_its_source() {
        let text = "| A | B |\n| --- | --- |\n| 1 | 2 |";
        let starts = line_starts(text).0;
        let model = crate::markdown::model(text);
        let mut painter = Painter::new();
        let line_fonts: Vec<(f32, Face)> = starts.iter().map(|_| (15.0, Face::Mono)).collect();
        let mut selected = HashSet::new();
        selected.insert(0usize);
        let rows = wrap_layout(
            text,
            &starts,
            300.0,
            &mut painter,
            &line_fonts,
            &model,
            Face::Mono,
            15.0,
            22.0,
            1.0,
            false,
            &HashSet::new(),
            &HashSet::new(),
            &selected,
        );
        assert!(
            rows.iter().all(|row| row.height > 0.0),
            "a selected table keeps all source rows visible: {rows:?}"
        );
    }

    #[test]
    fn short_rows_never_map_taps_to_the_line_start() {
        let widths = [200.0f32, 200.0, 200.0];
        assert_eq!(table_column_at(450.0, &widths, 1), (0, 0.0));
        assert_eq!(table_column_at(450.0, &widths, 2), (1, 200.0));
        assert_eq!(table_column_at(450.0, &widths, 3), (2, 400.0));
        assert_eq!(table_column_at(-5.0, &widths, 3), (0, 0.0));

        let text = "| A | B | C |\n| --- | --- | --- |\n| only |";
        let model = crate::markdown::model(text);
        let mut painter = Painter::new();
        let layout = build_table_layout(&mut painter, &model, 0, 2, 600.0, 15.0, Face::Mono, 1.0);
        let ranges = table_cell_ranges(&model[2].raw);
        assert_eq!(ranges.len(), 1, "the row has one real cell");
        let (column, _cell_x) =
            table_column_at(layout.widths.iter().sum::<f32>() * 0.9, &layout.widths, ranges.len());
        assert_eq!(column, 0, "a phantom column resolves to the real cell");
        let (range_start, range_end) = ranges[0];
        let cell_raw: String = model[2]
            .raw
            .chars()
            .skip(range_start)
            .take(range_end.saturating_sub(range_start))
            .collect();
        let offset = cell_tap_offset(
            &mut painter,
            &cell_raw,
            (range_start, range_end),
            layout.widths[0],
            12.0,
            layout.aligns[0],
            900.0,
            15.0,
            Face::Mono,
        );
        assert!(offset >= range_start && offset <= range_end);
        assert!(offset > 0, "the tap must not collapse to the line start");
    }

    #[test]
    fn empty_cells_put_the_caret_at_their_text_start() {
        let text = "| A | B | C |\n| :--- | :---: | ---: |\n|  |  |  |";
        let model = crate::markdown::model(text);
        let mut painter = Painter::new();
        let layout = build_table_layout(&mut painter, &model, 0, 2, 600.0, 15.0, Face::Mono, 1.0);
        let text_x = 100.0;
        let pad = 12.0;
        let bounds = table_cell_bounds(&model[2].raw);
        let contents = table_cell_ranges(&model[2].raw);
        let mut cell_x = text_x;
        for col in 0..layout.cols {
            let width = layout.widths.get(col).copied().unwrap_or(0.0);
            let inner = (width - pad).max(0.0);
            let align_offset = match layout.aligns.get(col).copied().unwrap_or(TableAlign::Left) {
                TableAlign::Left => 0.0,
                TableAlign::Center => inner * 0.5,
                TableAlign::Right => inner,
            };
            let expected = cell_x + pad * 0.5 + align_offset;
            let content_point = contents.get(col).copied().unwrap_or((0, 0)).0;
            let outer_end = bounds.get(col).copied().unwrap_or((0, 0)).1;
            for cursor in [content_point, outer_end.saturating_sub(1), outer_end] {
                let x = table_caret_x(
                    &mut painter,
                    &model,
                    2,
                    &layout,
                    cursor,
                    text_x,
                    pad,
                    15.0,
                    Face::Mono,
                );
                assert!(
                    (x - expected).abs() < 0.01,
                    "empty cell caret is not at its text start: col {col} cursor {cursor} x {x} expected {expected}"
                );
            }
            cell_x += width;
        }
    }

    #[test]
    fn table_cell_input_cannot_break_the_row() {
        assert_eq!(sanitize_table_cell_text("a|b"), r"a\|b");
        assert_eq!(sanitize_table_cell_text("a\nb"), "a b");
        assert_eq!(sanitize_table_cell_text("plain"), "plain");
    }

    #[test]
    fn task_marker_reveals_only_from_the_marker_column() {
        let prefix = 6;
        assert!(task_prefix_revealed(0, prefix, Some(0), None));
        assert!(task_prefix_revealed(0, prefix, Some(5), None));
        assert!(!task_prefix_revealed(0, prefix, Some(6), None));
        assert!(!task_prefix_revealed(0, prefix, Some(20), None));
        assert!(task_prefix_revealed(10, prefix, None, Some((10, 20))));
        assert!(task_prefix_revealed(10, prefix, None, Some((12, 30))));
        assert!(!task_prefix_revealed(10, prefix, None, Some((16, 30))));
        assert!(!task_prefix_revealed(10, prefix, None, Some((16, 16))));
    }

    #[test]
    fn idle_task_prefix_reserves_its_marker_column() {
        let model = model_for_editor("- [ ] привет\n");
        let chars: Vec<char> = model[0].raw.chars().collect();
        let mut hidden = canonical_hidden(&model, 0, chars.len());
        assert!(hidden[2], "the idle marker cells collapse by default");
        clear_closed_task_prefix(&model, 0, 0, &mut hidden, None, None);
        let prefix = model[0].prefix_chars;
        assert_eq!(prefix, 6);
        assert!(
            hidden[..prefix].iter().all(|value| !*value),
            "the checkbox column keeps the width of its raw source"
        );
    }

    #[test]
    fn task_checkbox_paints_like_obsidian() {
        let palette = SurfacePalette::default();
        let (w, h) = (80usize, 60usize);
        let mut open = vec![0u32; w * h];
        paint_task_checkbox(&mut open, w, h, 10.0, 0.0, 60.0, 40.0, false, &palette, 1.0);
        // The border is the faint text colour; the inside is the editor bg.
        let border = open[30 * w + 10];
        assert_ne!(border, 0, "the open checkbox has a border");
        assert_eq!(
            open[30 * w + 30],
            logical(palette.surface_alt),
            "the open checkbox is hollow"
        );

        let mut done = vec![0u32; w * h];
        paint_task_checkbox(&mut done, w, h, 10.0, 0.0, 60.0, 40.0, true, &palette, 1.0);
        assert_eq!(
            done[30 * w + 30],
            logical(palette.accent),
            "the done checkbox is filled with the accent"
        );
        assert!(
            done.iter().any(|pixel| *pixel == logical(0xffffff)),
            "the done checkbox carries a light check mark"
        );
    }

    #[test]
    fn trailing_spaces_keep_the_caret_in_their_cell() {
        let text = "| a  | b  | c  |\n| --- | --- | --- |\n| d  | e  | f  |";
        let model = crate::markdown::model(text);
        let mut painter = Painter::new();
        let layout = build_table_layout(&mut painter, &model, 0, 2, 600.0, 15.0, Face::Mono, 1.0);
        let text_x = 100.0;
        let pad = 12.0;
        let edge = text_x + layout.total - pad * 0.5;
        for line in [0usize, 2] {
            let wide = table_cell_wide_ranges(&model[line].raw);
            let mut cell_x = text_x;
            for col in 0..layout.cols {
                let width = layout.widths.get(col).copied().unwrap_or(0.0);
                let (start, end) = wide.get(col).copied().unwrap_or((0, 0));
                for cursor in [start, (start + end) / 2, end.saturating_sub(1), end] {
                    let x = table_caret_x(
                        &mut painter,
                        &model,
                        line,
                        &layout,
                        cursor,
                        text_x,
                        pad,
                        15.0,
                        Face::Mono,
                    );
                    println!("line={line} col={col} cursor={cursor} caret_x={x:.1} cell={cell_x:.1}..{}", cell_x + width);
                    assert!(
                        x >= cell_x - 1.0 && x <= cell_x + width - pad * 0.5 + 1.0,
                        "caret escaped its cell: line {line} col {col} x {x} cell {cell_x}..{}",
                        cell_x + width
                    );
                    assert!(
                        x < edge,
                        "caret flew to the table edge: line {line} col {col} x {x}"
                    );
                }
                cell_x += width;
            }
        }
    }

    #[test]
    fn every_table_cell_maps_taps_to_its_own_text() {
        let text = "\
| Left | Center | Right |
| :--- | :----: | ----: |
| a | b | c |
| **bold** | `code` | [link](https://x) |
| a long cell value | mid | 1 |

| A | B |
| --- | --- |
|   | 2 |
| only |
";
        let model = crate::markdown::model(text);
        let mut painter = Painter::new();
        let font = 15.0;
        let face = Face::Mono;
        let density = 1.0;
        let pad = 12.0;
        let text_x = 100.0;
        let content_width = 600.0;
        let mut checked = 0usize;
        let mut index = 0usize;
        while index < model.len() {
            if model[index].kind != "table" {
                index += 1;
                continue;
            }
            let (start, end) = table_bounds(&model, index).unwrap();
            let layout = build_table_layout(
                &mut painter,
                &model,
                start,
                end,
                content_width,
                font,
                face,
                density,
            );
            assert!(layout.cols > 0);
            for line in start..=end {
                if model[line].kind == "table_separator" {
                    continue;
                }
                let ranges = table_cell_ranges(&model[line].raw);
                let raw_len = model[line].raw.chars().count();
                let mut cell_x = text_x;
                for col in 0..layout.cols {
                    let width = layout.widths.get(col).copied().unwrap_or(0.0);
                    let (range_start, range_end) =
                        ranges.get(col).copied().unwrap_or((0, raw_len));
                    let cell_raw: String = model[line]
                        .raw
                        .chars()
                        .skip(range_start)
                        .take(range_end.saturating_sub(range_start))
                        .collect();
                    let align = layout
                        .aligns
                        .get(col)
                        .copied()
                        .unwrap_or(TableAlign::Left);
                    let visible = cell_visible_text(&cell_raw);
                    let full_width = painter.width(&visible, font, face);
                    let inner = (width - pad).max(0.0);
                    let align_offset = match align {
                        TableAlign::Left => 0.0,
                        TableAlign::Center => ((inner - full_width) * 0.5).max(0.0),
                        TableAlign::Right => (inner - full_width).max(0.0),
                    };
                    let text_start = pad * 0.5 + align_offset;
                    let mut last_caret = f32::MIN;
                    for fraction in [0.0f32, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
                        let target_x = (width * fraction).clamp(0.0, width);
                        let offset = cell_tap_offset(
                            &mut painter,
                            &cell_raw,
                            (range_start, range_end),
                            width,
                            pad,
                            align,
                            target_x,
                            font,
                            face,
                        );
                        let offset_in_cell = offset
                            .saturating_sub(range_start)
                            .min(range_end.saturating_sub(range_start));
                        let prefix_raw: String =
                            cell_raw.chars().take(offset_in_cell).collect();
                        let prefix = cell_visible_text(&prefix_raw);
                        let caret_x =
                            cell_x + text_start + painter.width(&prefix, font, face);
                        println!(
                            "line={line} col={col} tap={target_x:.1}/{width:.1} offset={offset} caret_x={caret_x:.1} cell_x={cell_x:.1}"
                        );
                        assert!(
                            offset >= range_start && offset <= raw_len,
                            "offset outside the cell: line {line} col {col} tap {target_x} offset {offset} range {range_start}..{range_end}"
                        );
                        assert!(
                            caret_x >= cell_x - 1.0 && caret_x <= cell_x + width + 1.0,
                            "caret outside the cell: line {line} col {col} caret {caret_x} cell {cell_x}..{}",
                            cell_x + width
                        );
                        assert!(
                            caret_x + 0.5 >= last_caret,
                            "caret went backwards: line {line} col {col} {caret_x} < {last_caret}"
                        );
                        // Inside the painted text the caret must land next to
                        // the tapped point, not a padding width away from it.
                        if full_width > 0.0
                            && target_x >= text_start
                            && target_x <= text_start + full_width
                        {
                            let max_error =
                                painter.advance_of_char('M', font, face).max(4.0) + 2.0;
                            assert!(
                                (caret_x - (cell_x + target_x)).abs() <= max_error,
                                "caret is off the tap: line {line} col {col} tap {target_x} caret_x {caret_x} text_start {text_start} full {full_width}"
                            );
                        }
                        last_caret = caret_x;
                        checked += 1;
                    }
                    let start_offset = cell_tap_offset(
                        &mut painter,
                        &cell_raw,
                        (range_start, range_end),
                        width,
                        pad,
                        align,
                        0.0,
                        font,
                        face,
                    );
                    assert_eq!(start_offset, range_start, "a tap left of the text starts the cell");
                    if visible.is_empty() {
                        // An empty cell keeps the caret at its own start for
                        // every tap, even in the last column.
                        for fraction in [0.25f32, 0.5, 0.75, 1.0] {
                            let target = (width * fraction).clamp(0.0, width);
                            let empty_offset = cell_tap_offset(
                                &mut painter,
                                &cell_raw,
                                (range_start, range_end),
                                width,
                                pad,
                                align,
                                target,
                                font,
                                face,
                            );
                            assert_eq!(
                                empty_offset, range_start,
                                "empty cell tap moved the caret: line {line} col {col} target {target}"
                            );
                        }
                    }
                    let after_text = text_start + full_width + 1.0;
                    if after_text < width && !visible.is_empty() {
                        let end_offset = cell_tap_offset(
                            &mut painter,
                            &cell_raw,
                            (range_start, range_end),
                            width,
                            pad,
                            align,
                            after_text,
                            font,
                            face,
                        );
                        assert_eq!(end_offset, range_end, "past a cell goes to its end");
                    }
                    cell_x += width;
                }
            }
            index = end + 1;
        }
        assert!(checked > 40, "the test should click every cell: {checked}");

        // Fill every cell with a marker, rebuild the document and check that
        // the text and the caret follow the new content.
        let lines: Vec<String> = text.lines().map(String::from).collect();
        let mut filled = lines.clone();
        let mut table_index = 0usize;
        let mut at = 0usize;
        while at < model.len() {
            if model[at].kind != "table" {
                at += 1;
                continue;
            }
            let (start, end) = table_bounds(&model, at).unwrap();
            let cols = build_table_layout(
                &mut painter,
                &model,
                start,
                end,
                content_width,
                font,
                face,
                density,
            )
            .cols;
            for line in start..=end {
                if model[line].kind == "table_separator" {
                    continue;
                }
                let indent: String = lines[line]
                    .chars()
                    .take_while(|character| *character == ' ')
                    .collect();
                let mut row = indent;
                for col in 0..cols {
                    row.push_str(&format!("| F{table_index}_{line}_{col} "));
                }
                row.push('|');
                filled[line] = row;
            }
            table_index += 1;
            at = end + 1;
        }
        let filled_text = filled.join("
");
        let filled_model = crate::markdown::model(&filled_text);
        let mut filled_checks = 0usize;
        let mut check_table = 0usize;
        let mut at = 0usize;
        while at < filled_model.len() {
            if filled_model[at].kind != "table" {
                at += 1;
                continue;
            }
            let (start, end) = table_bounds(&filled_model, at).unwrap();
            let layout = build_table_layout(
                &mut painter,
                &filled_model,
                start,
                end,
                content_width,
                font,
                face,
                density,
            );
            for line in start..=end {
                if filled_model[line].kind == "table_separator" {
                    continue;
                }
                let ranges = table_cell_ranges(&filled_model[line].raw);
                let raw_len = filled_model[line].raw.chars().count();
                let mut cell_x = text_x;
                for col in 0..layout.cols {
                    let width = layout.widths.get(col).copied().unwrap_or(0.0);
                    let (range_start, range_end) =
                        ranges.get(col).copied().unwrap_or((0, raw_len));
                    let cell_raw: String = filled_model[line]
                        .raw
                        .chars()
                        .skip(range_start)
                        .take(range_end.saturating_sub(range_start))
                        .collect();
                    assert!(
                        cell_raw.contains(&format!("F{check_table}_{line}_{col}")),
                        "cell was not filled: line {line} col {col} {cell_raw:?}"
                    );
                    let align = layout
                        .aligns
                        .get(col)
                        .copied()
                        .unwrap_or(TableAlign::Left);
                    let full = cell_visible_text(&cell_raw);
                    let full_width = painter.width(&full, font, face);
                    let inner = (width - pad).max(0.0);
                    let align_offset = match align {
                        TableAlign::Left => 0.0,
                        TableAlign::Center => ((inner - full_width) * 0.5).max(0.0),
                        TableAlign::Right => (inner - full_width).max(0.0),
                    };
                    let center = (width * 0.5).clamp(0.0, width);
                    let offset = cell_tap_offset(
                        &mut painter,
                        &cell_raw,
                        (range_start, range_end),
                        width,
                        pad,
                        align,
                        center,
                        font,
                        face,
                    );
                    assert!(
                        offset >= range_start && offset <= raw_len,
                        "filled cell offset outside: line {line} col {col} offset {offset}"
                    );
                    let caret_x = cell_x
                        + pad * 0.5
                        + align_offset
                        + painter.width(
                            &cell_visible_text(
                                &cell_raw
                                    .chars()
                                    .take(offset.saturating_sub(range_start))
                                    .collect::<String>(),
                            ),
                            font,
                            face,
                        );
                    assert!(
                        caret_x >= cell_x - 1.0 && caret_x <= cell_x + width + 1.0,
                        "filled caret outside cell: line {line} col {col} {caret_x}..{}",
                        cell_x + width
                    );
                    println!(
                        "filled line={line} col={col} text={full:?} tap={center:.1} offset={offset} caret_x={caret_x:.1}"
                    );
                    filled_checks += 1;
                    cell_x += width;
                }
            }
            check_table += 1;
            at = end + 1;
        }
        assert!(filled_checks > 10, "every filled cell should be checked");
    }

    #[test]
    fn callout_blocks_cover_header_and_body_rows() {
        use crate::markdown::CalloutKind;
        let model = crate::markdown::model(
            "> [!warning] Careful\n> body one\n> body two\n\nplain\n",
        );
        let blocks = callout_block_map(&model);
        assert_eq!(
            blocks[0].map(|block| (block.kind, block.first, block.last)),
            Some((CalloutKind::Warning, true, false))
        );
        assert_eq!(
            blocks[1].map(|block| (block.kind, block.first, block.last)),
            Some((CalloutKind::Warning, false, false))
        );
        assert_eq!(
            blocks[2].map(|block| (block.kind, block.first, block.last)),
            Some((CalloutKind::Warning, false, true))
        );
        assert!(blocks[3].is_none());
        assert_eq!(callout_block_extent(&blocks, 1), Some((0, 2)));
    }

    #[test]
    fn every_callout_family_has_an_icon() {
        use crate::markdown::CalloutKind;
        for kind in [
            CalloutKind::Note,
            CalloutKind::Info,
            CalloutKind::Tip,
            CalloutKind::Warning,
            CalloutKind::Danger,
            CalloutKind::Quote,
        ] {
            assert!(
                !callout_icon_paths(kind).is_empty(),
                "no glyph for {kind:?}"
            );
        }
    }

    #[test]
    fn table_header_maps_every_row_to_its_header() {
        let model = crate::markdown::model("| A | B |\n| --- | --- |\n| 1 | 2 |\n\ntail\n");
        assert_eq!(table_header_at(&model, 0), Some(0));
        assert_eq!(table_header_at(&model, 2), Some(0));
        assert_eq!(table_header_at(&model, 3), None);
    }

    #[test]
    fn editing_a_table_switches_it_to_raw_rows() {
        let text = "| A | B |\n| --- | --- |\n| 1 | 2 |";
        let starts = line_starts(text).0;
        let model = crate::markdown::model(text);
        let mut painter = Painter::new();
        let line_fonts: Vec<(f32, Face)> = starts.iter().map(|_| (15.0, Face::Mono)).collect();
        let grid = wrap_layout(
            text,
            &starts,
            300.0,
            &mut painter,
            &line_fonts,
            &model,
            Face::Mono,
            15.0,
            22.0,
            1.0,
            false,
            &HashSet::new(),
            &HashSet::new(),
            &HashSet::new(),
        );
        let mut raw_tables = HashSet::new();
        raw_tables.insert(0usize);
        let raw = wrap_layout(
            text,
            &starts,
            300.0,
            &mut painter,
            &line_fonts,
            &model,
            Face::Mono,
            15.0,
            22.0,
            1.0,
            false,
            &HashSet::new(),
            &raw_tables,
            &HashSet::new(),
        );
        assert_eq!(grid[1].height, 0.0, "the separator collapses in grid mode");
        assert!(
            raw.iter().all(|row| row.height > 0.0),
            "the edited table keeps every source row: {raw:?}"
        );
    }

    #[test]
    fn bullet_continuation_rows_hang_under_the_text() {
        let text = "- aaaaaaaa aaaaaaaa aaaaaaaa aaaaaaaa aaaaaaaa aaaaaaaa";
        let starts = line_starts(text).0;
        let model = crate::markdown::model(text);
        let mut painter = Painter::new();
        let line_fonts = vec![(15.0, Face::Mono)];
        let rows = wrap_layout(
            text,
            &starts,
            200.0,
            &mut painter,
            &line_fonts,
            &model,
            Face::Mono,
            15.0,
            22.0,
            1.0,
            false,
            &HashSet::new(),
            &HashSet::new(),
            &HashSet::new(),
        );
        assert!(rows.len() > 1, "the bullet wraps");
        let indent = line_text_indent(&mut painter, &model, 0, 15.0, Face::Mono);
        assert!(indent >= 9.0, "the bullet column has width: {indent}");
        let cell = painter.advance(15.0, Face::Mono);
        let max_chars = ((200.0 - indent) / cell).floor() as usize + 1;
        for row in &rows {
            let len = row.end - row.start;
            assert!(
                len <= max_chars,
                "a bullet row ignores the hanging indent: {len} > {max_chars}"
            );
        }
    }

    #[test]
    fn quote_wrap_reserves_the_marker_column_and_gap() {
        let chars: Vec<char> = "x ".repeat(80).chars().collect();
        let hidden = vec![false; chars.len()];
        let mut painter = Painter::new();
        let plain = wrap_line_chunks(
            &mut painter,
            &chars,
            &hidden,
            200.0,
            0.0,
            0.0,
            15.0,
            Face::Mono,
            &[],
            0,
            Face::Mono,
        );
        let quoted = wrap_line_chunks(
            &mut painter,
            &chars,
            &hidden,
            200.0,
            18.0,
            34.0,
            15.0,
            Face::Mono,
            &[],
            0,
            Face::Mono,
        );
        assert!(quoted[0].1 < plain[0].1, "quote keeps clear of its marker column");
        if quoted.len() > 1 {
            let first = quoted[0].1 - quoted[0].0;
            let second = quoted[1].1 - quoted[1].0;
            assert!(second <= first, "continuation rows are narrower");
        }
    }

    #[test]
    fn callout_bar_rounds_only_its_left_corners() {
        let (width, height) = (20usize, 20usize);
        let mut pixels = vec![0u32; width * height];
        rounded_box_radii(
            &mut pixels,
            width,
            height,
            0.0,
            0.0,
            20.0,
            20.0,
            [10.0, 0.0, 0.0, 10.0],
            0xffffff,
        );
        assert_eq!(pixels[0], 0, "top-left corner is rounded away");
        assert_ne!(pixels[width - 1], 0, "top-right stays square");
        assert_eq!(pixels[(height - 1) * width], 0, "bottom-left is rounded");
        assert_ne!(
            pixels[(height - 1) * width + width - 1],
            0,
            "bottom-right stays square"
        );
        assert_ne!(pixels[10 * width + 10], 0, "interior is filled");
    }

    #[test]
    fn rounded_box_fills_the_interior_and_honours_the_corners() {
        let (width, height) = (40usize, 30usize);
        let mut pixels = vec![0u32; width * height];
        rounded_box(
            &mut pixels,
            width,
            height,
            0.0,
            0.0,
            40.0,
            30.0,
            10.0,
            0xffffff,
        );
        assert_ne!(pixels[15 * width + 20], 0, "interior is filled");
        assert_ne!(pixels[15 * width + 2], 0, "left edge is filled");
        assert_eq!(pixels[0], 0, "corner stays empty");
        assert_eq!(pixels[(height - 1) * width], 0, "bottom corner stays empty");
    }

    #[test]
    fn code_block_bounds_group_fences_and_lines() {
        let model = crate::markdown::model("```rust\nlet a = 1;\nlet b = 2;\n```\n\ntail\n");
        assert_eq!(code_block_bounds(&model, 0), Some((0, 3)));
        assert_eq!(code_block_bounds(&model, 2), Some((0, 3)));
        assert_eq!(code_block_bounds(&model, 4), None);
        assert_eq!(code_block_fences(&model, 0, 3), (0, 3));
        assert_eq!(code_block_line_count(&model, 0, 3), 2);
        assert_eq!(code_block_body(&model, 0, 3), "let a = 1;\nlet b = 2;\n");
    }

    #[test]
    fn long_code_blocks_start_folded() {
        let mut text = String::from("```rust\n");
        for index in 0..=CODE_FOLD_LINES {
            text.push_str(&format!("let x{index} = 1;\n"));
        }
        text.push_str("```\n");
        let model = crate::markdown::model(&text);
        assert_eq!(default_folded_code(&model).len(), 1);
    }

    #[test]
    fn folded_code_lines_collapse_to_the_header() {
        let mut text = String::from("```rust\n");
        for index in 0..=CODE_FOLD_LINES {
            text.push_str(&format!("let x{index} = 1;\n"));
        }
        text.push_str("```");
        let starts = line_starts(&text).0;
        let model = crate::markdown::model(&text);
        let folded = default_folded_code(&model);
        let mut painter = Painter::new();
        let line_fonts: Vec<(f32, Face)> = starts.iter().map(|_| (15.0, Face::Mono)).collect();
        let rows = wrap_layout(
            &text,
            &starts,
            300.0,
            &mut painter,
            &line_fonts,
            &model,
            Face::Mono,
            15.0,
            22.0,
            1.0,
            true,
            &folded,
            &HashSet::new(),
            &HashSet::new(),
        );
        assert!(rows[0].height > 22.0, "the header keeps its height");
        assert!(
            rows[1..rows.len() - 1]
                .iter()
                .all(|row| row.height == 0.0),
            "folded code lines take no space: {rows:?}"
        );
        assert!(rows[rows.len() - 1].height == 0.0, "closing fence folds too");
    }

    #[test]
    fn reading_mode_collapses_each_table_line_to_one_row() {
        let text = "| A | B |\n| --- | --- |\n| 1 | 2 |";
        let (starts, _) = line_starts(text);
        let model = crate::markdown::model(text);
        let mut painter = Painter::new();
        let line_fonts: Vec<(f32, Face)> = starts.iter().map(|_| (15.0, Face::Mono)).collect();
        let rows = wrap_layout(
            text,
            &starts,
            200.0,
            &mut painter,
            &line_fonts,
            &model,
            Face::Mono,
            15.0,
            22.0,
            1.0,
            true,
            &HashSet::new(),
            &HashSet::new(),
            &HashSet::new(),
        );
        assert_eq!(rows.len(), 3, "one visual row per source line: {rows:?}");
        assert_eq!(rows[1].height, 0.0, "the separator line has no height");
        assert!(rows[0].height > 22.0, "table rows carry vertical padding");
    }
}
// touch
