//! Native Android surface used by the hybrid UI.
//!
//! The Android shell keeps pickers, IME ownership, sync and navigation in
//! Kotlin. This module paints the explorer and the source editor into an
//! `ANativeWindow` owned by a `SurfaceView`, and reports taps back as a small
//! JSON action. It deliberately has no Compose/View dependency.

use std::collections::HashMap;
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

#[link(name = "log")]
extern "C" {
    fn __android_log_print(prio: i32, tag: *const u8, fmt: *const u8, ...) -> i32;
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
const CHEVRON_RIGHT_SVG: &str = "m9 18 6-6-6-6";
const CHEVRON_DOWN_SVG: &str = "m6 9 6 6 6-6";
const FOLDER_SVG: &str = "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z M2 10h20";
const FOLDER_OPEN_SVG: &str = "m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2";
const FILE_TEXT_SVG: &str = "M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z M14 2v5a1 1 0 0 0 1 1h5 M10 9H8 M16 13H8 M16 17H8";

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

#[derive(Default)]
struct Painter {
    ui_font: Option<Font>,
    mono_font: Option<Font>,
    symbols_font: Option<Font>,
    symbols2_font: Option<Font>,
    cjk_font: Option<Font>,
    glyphs: HashMap<(u8, char, u32), (fontdue::Metrics, Vec<u8>)>,
    icons: HashMap<(usize, u32, u32), IconRaster>,
    emojis: HashMap<(String, u32), EmojiRaster>,
    metrics_cache: HashMap<(u32, bool), (f32, f32)>,
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

impl Painter {
    fn new() -> Self {
        let ui_font = load_font(&[
            "/system/fonts/Roboto-Regular.ttf",
            "/system/fonts/RobotoStatic-Regular.ttf",
            "/system/fonts/NotoSans-Regular.ttf",
        ]);
        let mono_font = load_font(&[
            "/system/fonts/RobotoMono-Regular.ttf",
            "/system/fonts/DroidSansMono.ttf",
            "/system/fonts/NotoSansMono-Regular.ttf",
        ]);
        let symbols_font = load_font(&[
            "/system/fonts/NotoSansSymbols-Regular-Subsetted.ttf",
            "/system/fonts/NotoSansSymbols-Regular.ttf",
        ]);
        let symbols2_font = load_font(&[
            "/system/fonts/NotoSansSymbols-Regular-Subsetted2.ttf",
            "/system/fonts/NotoSansSymbols2-Regular.ttf",
        ]);
        let cjk_font = None;
        Self {
            ui_font,
            mono_font,
            symbols_font,
            symbols2_font,
            cjk_font,
            glyphs: HashMap::new(),
            icons: HashMap::new(),
            emojis: HashMap::new(),
            metrics_cache: HashMap::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
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

    fn font(&self, mono: bool) -> Option<&Font> {
        if mono {
            self.mono_font.as_ref().or(self.ui_font.as_ref())
        } else {
            self.ui_font.as_ref().or(self.mono_font.as_ref())
        }
    }

    fn font_for_char(&self, character: char, mono: bool) -> Option<&Font> {
        let primary = if mono {
            self.mono_font.as_ref().or(self.ui_font.as_ref())
        } else {
            self.ui_font.as_ref().or(self.mono_font.as_ref())
        };
        if let Some(font) = primary {
            if font.lookup_glyph_index(character) != 0 {
                return Some(font);
            }
        }
        let secondary = if mono {
            self.ui_font.as_ref()
        } else {
            self.mono_font.as_ref()
        };
        if let Some(font) = secondary {
            if font.lookup_glyph_index(character) != 0 {
                return Some(font);
            }
        }
        for fallback in [&self.symbols_font, &self.symbols2_font, &self.cjk_font] {
            if let Some(font) = fallback.as_ref() {
                if font.lookup_glyph_index(character) != 0 {
                    return Some(font);
                }
            }
        }
        primary
    }

    fn font_metrics(&mut self, px: f32, mono: bool) -> (f32, f32) {
        let key = (px.to_bits(), mono);
        if let Some(&cached) = self.metrics_cache.get(&key) {
            return cached;
        }
        let advance = self
            .font(mono)
            .map(|f| f.metrics('M', px).advance_width)
            .unwrap_or(px * 0.6);
        let ascent = self
            .font(mono)
            .and_then(|f| f.horizontal_line_metrics(px))
            .map(|line| line.ascent)
            .unwrap_or(px * 0.8)
            .round();
        self.metrics_cache.insert(key, (advance, ascent));
        (advance, ascent)
    }

    fn advance(&mut self, px: f32, mono: bool) -> f32 {
        self.font_metrics(px, mono).0
    }

    fn width(&mut self, text: &str, px: f32, mono: bool) -> f32 {
        if mono {
            let advance = self.advance(px, true);
            if text.is_ascii() {
                return text.len() as f32 * advance;
            }
            let chars: Vec<char> = text.chars().filter(|&c| c != '\r').collect();
            let offsets = line_cell_offsets(&chars, &[]);
            let total_cells = offsets.last().copied().unwrap_or(0);
            return total_cells as f32 * advance;
        }
        let Some(font) = self.font(mono) else {
            return text.chars().count() as f32 * px * 0.6;
        };
        text.chars().filter(|&c| c != '\r').map(|c| {
            if is_emoji_start(c) {
                px
            } else {
                let f = self.font_for_char(c, mono).unwrap_or(font);
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
        mono: bool,
    ) {
        let (cell_advance, ascent) = self.font_metrics(px, mono);
        let baseline = y + ascent;
        let mut cursor = x;

        if text.is_ascii() {
            let px_key = px.round() as u32;
            for &byte in text.as_bytes() {
                if byte == b'\r' {
                    continue;
                }
                let character = byte as char;
                let key = (u8::from(mono), character, px_key);
                if !self.glyphs.contains_key(&key) {
                    let raster = self
                        .font_for_char(character, mono)
                        .map(|font| font.rasterize(character, px))
                        .unwrap_or_else(|| (fontdue::Metrics::default(), Vec::new()));
                    self.glyphs.insert(key, raster);
                }
                let Some((metrics, bitmap)) = self.glyphs.get(&key) else {
                    cursor += if mono { cell_advance } else { 0.0 };
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
                    for column in 0..metrics.width {
                        let alpha = bitmap[row * metrics.width + column];
                        if alpha == 0 {
                            continue;
                        }
                        let px_x = gx + column as i32;
                        if px_x < 0 || px_x >= width as i32 {
                            continue;
                        }
                        let index = py * width + px_x as usize;
                        pixels[index] = blend(pixels[index], rgb, alpha);
                    }
                }
                cursor += if mono { cell_advance } else { metrics.advance_width };
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
                    let slot_w = if mono { 2.0 * cell_advance } else { raster.width as f32 };
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
                    cursor += if mono { 2.0 * cell_advance } else { raster.width as f32 };
                    i += cluster_len;
                    continue;
                }
            }
            let key = (u8::from(mono), character, px.round() as u32);
            if !self.glyphs.contains_key(&key) {
                let raster = self
                    .font_for_char(character, mono)
                    .map(|font| font.rasterize(character, px))
                    .unwrap_or_else(|| (fontdue::Metrics::default(), Vec::new()));
                self.glyphs.insert(key, raster);
            }
            let Some((metrics, bitmap)) = self.glyphs.get(&key) else {
                i += 1;
                continue;
            };
            let cjk = is_cjk(character);
            let slot_w = if mono && cjk { 2.0 * cell_advance } else if mono { cell_advance } else { metrics.advance_width };
            let offset_x = if mono && cjk { ((slot_w - metrics.advance_width) / 2.0).max(0.0) } else { 0.0 };
            let gx = (cursor + metrics.xmin as f32 + offset_x).round() as i32;
            let gy = (baseline - metrics.ymin as f32 - metrics.height as f32).round() as i32;
            for row in 0..metrics.height {
                let py = gy + row as i32;
                if py < 0 || py >= height as i32 {
                    continue;
                }
                let py = py as usize;
                for column in 0..metrics.width {
                    let alpha = bitmap[row * metrics.width + column];
                    if alpha == 0 {
                        continue;
                    }
                    let px_x = gx + column as i32;
                    if px_x < 0 || px_x >= width as i32 {
                        continue;
                    }
                    let index = py * width + px_x as usize;
                    pixels[index] = blend(pixels[index], rgb, alpha);
                }
            }
            cursor += if mono {
                if cjk { 2.0 * cell_advance } else { cell_advance }
            } else {
                metrics.advance_width
            };
            i += 1;
        }
    }
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
    drag: Option<(f32, f32, bool)>,
}

#[derive(Clone, Copy, Debug, Default)]
struct WrapLine {
    line: usize,
    start: usize,
    end: usize,
    font: f32,
    advance: f32,
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
}

struct SurfaceState {
    window: *mut ANativeWindow,
    width: usize,
    height: usize,
    /// Real viewport height. `height` is temporarily reduced while a cache
    /// band is painted, so layout math must not read it.
    viewport_height: usize,
    density: f32,
    background: u32,
    painter: Painter,
    tree: TreeState,
    editor: EditorState,
    pixels: Vec<u32>,
    frame_valid: bool,
    suspend_render: bool,
    last_mode: u8,
    last_scroll: f32,
    last_width: usize,
    last_height: usize,
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
    last_y_bits: AtomicU32,
    scroll_bits: AtomicU32,
    scroll_dirty: AtomicBool,
    max_y_bits: AtomicU32,
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
            last_y_bits: AtomicU32::new(0),
            scroll_bits: AtomicU32::new(0),
            scroll_dirty: AtomicBool::new(false),
            max_y_bits: AtomicU32::new(0),
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
    if !input.scroll_dirty.swap(false, Ordering::Relaxed) {
        return;
    }
    let y = f32::from_bits(input.scroll_bits.load(Ordering::Relaxed));
    let mode = input.mode.load(Ordering::Relaxed);
    match mode {
        1 => state.tree_set_scroll(y),
        _ => state.editor_set_scroll(y, 0.0),
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

fn line_height(density: f32) -> f32 {
    (22.0 * density).round().max(1.0)
}

fn editor_font(density: f32) -> f32 {
    (15.0 * density).round().max(1.0)
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

fn wrap_layout(
    text: &str,
    starts: &[usize],
    content_width: f32,
    metrics: &[(f32, f32)],
) -> Vec<WrapLine> {
    let all: Vec<char> = text.chars().collect();
    let mut rows = Vec::with_capacity(starts.len());
    for (line_index, &line_start) in starts.iter().enumerate() {
        let (font, advance) = metrics
            .get(line_index)
            .copied()
            .unwrap_or((15.0, 9.0));
        let line_end = if line_index + 1 < starts.len() {
            starts[line_index + 1].saturating_sub(1)
        } else {
            all.len()
        };
        let chars = &all[line_start.min(all.len())..line_end.min(all.len())];
        if chars.is_empty() {
            rows.push(WrapLine {
                line: line_index,
                start: 0,
                end: 0,
                font,
                advance,
            });
            continue;
        }
        let max_cols = ((content_width / advance).floor() as usize).max(1);
        let mut start = 0usize;
        while start < chars.len() {
            let mut end = start;
            let mut current_cols = 0usize;
            while end < chars.len() {
                let cluster_len = emoji_cluster_len(chars, end);
                let col_w = if cluster_len > 0 {
                    2
                } else if is_cjk(chars[end]) {
                    2
                } else if chars[end] == '\r' || chars[end] == '\u{fe0f}' || chars[end] == '\u{fe0e}' {
                    0
                } else {
                    1
                };
                if current_cols + col_w > max_cols && end > start {
                    break;
                }
                current_cols += col_w;
                end += if cluster_len > 0 { cluster_len } else { 1 };
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
            rows.push(WrapLine {
                line: line_index,
                start,
                end,
                font,
                advance,
            });
            start = end;
        }
    }
    rows
}

fn heading_font(line: Option<&crate::markdown::MarkdownLine>, base: f32) -> f32 {
    let Some(line) = line else {
        return base;
    };
    if line.kind != "heading" {
        return base;
    }
    match line.level {
        1 => base * 1.27,
        2 => base * 1.20,
        3 => base * 1.13,
        4 => base * 1.07,
        5 => base * 1.03,
        _ => base,
    }
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

fn inline_color(kind: &str, base: u32) -> u32 {
    match kind {
        "inline_code" => 0x53dfdd,
        "link" | "tag" => 0x8a5cf5,
        "grammar" => 0xb3b3b3,
        "highlight" => 0xdadada,
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

fn code_colors(line: &str, language: &str) -> Vec<u32> {
    let chars: Vec<char> = line.chars().collect();
    let mut colors = vec![0xdadadau32; chars.len()];
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
                *color = 0x666666;
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
                *color = 0x44cf6e;
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
                *color = 0xe9973f;
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
                0xfa99cd
            } else {
                let next = chars[index..].iter().find(|character| !character.is_whitespace());
                if next == Some(&'(') {
                    0xe0de71
                } else if word.chars().next().is_some_and(|character| character.is_uppercase()) {
                    0x53dfdd
                } else {
                    0xdadada
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
            background: 0x000000,
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
        }
    }

    fn invalidate(&mut self) {
        self.frame_valid = false;
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
        unsafe {
            __android_log_print(
                3, // DEBUG
                b"VibecodiumRust\0".as_ptr(),
                b"render breakdown: lock=%.2fms, paint=%.2fms, post=%.2fms\0".as_ptr(),
                t_lock.as_secs_f64() * 1000.0,
                t_paint.as_secs_f64() * 1000.0,
                t_post.as_secs_f64() * 1000.0,
            );
        }
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
        let density = self.density;
        let (content, viewport, scroll, max_scroll) = match mode {
            1 => {
                let content = self.tree.doc.rows.len() as f32 * row_height(density);
                let max = (content - self.height as f32).max(0.0);
                (content, self.height as f32, self.tree.scroll.round(), max)
            }
            2 => {
                let content = self.editor.top_inset.round()
                    + self.editor.wrap.len() as f32 * line_height(density)
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
            0x6a6a6a,
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

    fn draw_tree(&mut self, pixels: &mut [u32]) {
        let row_h = row_height(self.density);
        let density = self.density;
        let scroll = self.tree.scroll.round();
        let active = self.tree.doc.active.clone();
        let rows = std::mem::take(&mut self.tree.doc.rows);
        let mut painter = std::mem::take(&mut self.painter);
        let name_font = 17.0 * density;
        let icon_size = 16.0 * density;
        let icon_thickness = 1.0 * density;
        let chevron_size = 10.0 * density;
        let chevron_thickness = 1.6667 * density;
        for (index, row) in rows.iter().enumerate() {
            let y = index as f32 * row_h - scroll;
            if y + row_h < 0.0 || y > self.height as f32 {
                continue;
            }
            if active.as_deref() == Some(row.path.as_str()) {
                rect(pixels, self.width, self.height, 0.0, y, self.width as f32, row_h, 0x333333);
            }
            let center_y = y + row_h / 2.0;
            let indent = 10.0 * density + row.depth as f32 * 16.0 * density;
            let chevron_x = indent;
            let icon_x = indent + 13.0 * density;
            let text_x = indent + 35.0 * density;
            if row.folder && row.has_children {
                painter.draw_stroke_icon(
                    pixels,
                    self.width,
                    self.height,
                    chevron_x,
                    center_y - chevron_size / 2.0,
                    chevron_size,
                    chevron_thickness,
                    chevron_paths(row.expanded),
                    0x8a8a8a,
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
                if row.folder { 0x8a5cf5 } else { 0x8a8a8a },
            );
            let extension = row.extension.to_ascii_uppercase();
            let show_tag = !row.folder && !extension.is_empty() && extension != "MD";
            let tag_width = if show_tag {
                painter.width(&extension, 9.0 * density, false) + 10.0 * density
            } else {
                0.0
            };
            let max_width = (self.width as f32 - text_x - tag_width - 8.0 * density).max(0.0);
            let mut name = row.name.clone();
            if painter.width(&name, name_font, false) > max_width && max_width > 0.0 {
                while name.chars().count() > 1 && painter.width(&name, name_font, false) > max_width {
                    name.pop();
                }
                name.push('…');
            }
            painter.draw(
                pixels,
                self.width,
                self.height,
                text_x,
                y + (row_h - name_font) / 2.0,
                &name,
                name_font,
                if row.folder || row.note { 0xdadada } else { 0xb3b3b3 },
                false,
            );
            if show_tag {
                let tag_x = self.width as f32 - tag_width - 8.0 * density;
                rect(
                    pixels,
                    self.width,
                    self.height,
                    tag_x,
                    y + (row_h - 18.0 * density) / 2.0,
                    tag_width,
                    18.0 * density,
                    0x2a2a2a,
                );
                painter.draw(
                    pixels,
                    self.width,
                    self.height,
                    tag_x + 5.0 * density,
                    y + (row_h - 9.0 * density) / 2.0,
                    &extension,
                    9.0 * density,
                    0xb3b3b3,
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
        let advance = self.painter.advance(editor_font(self.density), true);
        self.ensure_starts();
        let gutter = (self.editor.starts.len().max(1).to_string().len() as f32 * advance
            + 24.0 * self.density)
            .max(34.0 * self.density);
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
            self.editor.model_dirty = false;
        }
    }

    fn ensure_wrap(&mut self, content_width: f32) {
        self.ensure_starts();
        self.ensure_model();
        if !self.editor.wrap_dirty
            && (self.editor.wrap_width - content_width).abs() < 0.5
            && !self.editor.wrap.is_empty()
        {
            return;
        }
        let base = editor_font(self.density);
        let metrics: Vec<(f32, f32)> = (0..self.editor.starts.len())
            .map(|line| {
                let font = heading_font(self.editor.model.get(line), base);
                let advance = self.painter.advance(font, true);
                (font, advance)
            })
            .collect();
        let wrap = wrap_layout(
            &self.editor.text,
            &self.editor.starts,
            content_width,
            &metrics,
        );
        self.editor.wrap_first = vec![0; self.editor.starts.len()];
        self.editor.wrap_count = vec![0; self.editor.starts.len()];
        self.editor.wrap = wrap;
        for (visual, chunk) in self.editor.wrap.iter().enumerate() {
            if let Some(count) = self.editor.wrap_count.get_mut(chunk.line) {
                if *count == 0 {
                    self.editor.wrap_first[chunk.line] = visual;
                }
                *count += 1;
            }
        }
        self.editor.wrap_width = content_width;
        self.editor.wrap_dirty = false;
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
        let density = self.density;
        let line_h = line_height(density);
        let font_px = editor_font(density);
        if self.editor.model_dirty {
            self.editor.model = if self.editor.language.is_empty() {
                model_for_editor(&self.editor.text)
            } else {
                Vec::new()
            };
            self.editor.model_dirty = false;
        }
        let advance = self.painter.advance(font_px, true);
        self.ensure_starts();
        let gutter = (self.editor.starts.len().max(1).to_string().len() as f32 * advance
            + (24.0 * density).round())
            .max((34.0 * density).round())
            .round();
        let content_width = (self.width as f32 - gutter - (8.0 * density).round()).max(advance);
        self.ensure_wrap(content_width);
        let starts = std::mem::take(&mut self.editor.starts);
        let byte_starts = std::mem::take(&mut self.editor.byte_starts);
        self.editor.scroll_x = 0.0;
        let total_visual = self.editor.wrap.len();
        let text_x = gutter;
        let scroll = if self.editor.band_origin >= 0.0 {
            self.editor.band_origin.round()
        } else {
            self.editor.scroll.round()
        };
        let top_inset = self.editor.top_inset.round();
        let first = (((scroll - top_inset) / line_h)
            .floor()
            .max(0.0) as usize)
            .min(total_visual.saturating_sub(1));
        let visible = (self.height as f32 / line_h).ceil() as usize + 2;
        let last = total_visual.min(first + visible);
        let (selection_start, selection_end) = self.selected_range();
        let cursor_line = if self.editor.preview {
            usize::MAX
        } else {
            line_of(&starts, self.editor.selection)
        };
        let (cursor_visual, cursor_offset) =
            self.visual_for_position(&starts, self.editor.selection);
        let model = std::mem::take(&mut self.editor.model);
        let has_model = model.len() == starts.len();
        let mut painter = std::mem::take(&mut self.painter);
        let mut prepared_line = usize::MAX;
        let mut line_start = 0usize;
        let mut chars: Vec<char> = Vec::new();
        let mut colors: Vec<u32> = Vec::new();
        let mut hidden: Vec<bool> = Vec::new();
        let mut cell_offsets: Vec<usize> = Vec::new();
        let mut overlays: Vec<(usize, usize, u32)> = Vec::new();
        let (start_visual, start_offset) =
            self.visual_for_position(&starts, selection_start);
        let (end_visual, end_offset) = self.visual_for_position(&starts, selection_end);
        let mut start_handle: Option<(f32, f32)> = None;
        let mut end_handle: Option<(f32, f32)> = None;
        let mut insertion_handle: Option<(f32, f32)> = None;
        let text_pad_y = (3.0 * density).round();
        let number_pad_y = (4.0 * density).round();
        for visual in first..last {
            let chunk = self.editor.wrap[visual];
            let y = top_inset + visual as f32 * line_h - scroll;
            if chunk.line != prepared_line {
                prepared_line = chunk.line;
                line_start = starts.get(chunk.line).copied().unwrap_or(0);
                let raw = logical_line(&self.editor.text, &byte_starts, chunk.line);
                chars.clear();
                chars.extend(raw.chars());
                colors.clear();
                colors.resize(chars.len(), 0xdadadau32);
                hidden.clear();
                hidden.resize(chars.len(), false);
                overlays.clear();
                if !self.editor.language.is_empty() {
                    colors = code_colors(raw, &self.editor.language);
                } else if has_model {
                    if let Some(line) = model.get(chunk.line) {
                        let base = match line.kind.as_str() {
                            "heading" => 0xdadada,
                            "quote" => 0xb3b3b3,
                            "code" => 0x44cf6e,
                            _ => 0xdadada,
                        };
                        colors.clear();
                        colors.resize(chars.len(), base);
                        if line.kind == "code" {
                            if let Some(language) = line.language.as_deref() {
                                colors = code_colors(raw, language);
                            }
                        }
                        if chunk.line != cursor_line {
                            match line.kind.as_str() {
                                "heading" => {
                                    let length = line.prefix_chars.min(chars.len());
                                    for value in hidden.iter_mut().take(length) {
                                        *value = true;
                                    }
                                }
                                "task" => {
                                    blank_range(&mut chars, 0, line.prefix_chars);
                                    let icon = if line.checked == Some(true) { '☑' } else { '☐' };
                                    if line.indent < chars.len() {
                                        chars[line.indent] = icon;
                                        if line.checked == Some(true) {
                                            colors[line.indent] = 0x808080;
                                        }
                                        if line.indent + 1 < chars.len() {
                                            chars[line.indent + 1] = ' ';
                                        }
                                        let hide_start = (line.indent + 2).min(chars.len());
                                        let hide_end = line.prefix_chars.min(chars.len());
                                        for value in hidden[hide_start..hide_end].iter_mut() {
                                            *value = true;
                                        }
                                    }
                                    if line.checked == Some(true) {
                                        for color in colors[line.prefix_chars.min(chars.len())..].iter_mut() {
                                            *color = 0x808080;
                                        }
                                    }
                                }
                                "bullet" => {
                                    blank_range(&mut chars, 0, line.prefix_chars);
                                    if line.prefix_chars >= 2 {
                                        let marker = line.prefix_chars - 2;
                                        if marker + 1 < chars.len() {
                                            chars[marker] = '•';
                                            chars[marker + 1] = ' ';
                                        }
                                    }
                                }
                                "quote" => {
                                    if let Some(character) = chars.first_mut() {
                                        *character = '│';
                                    }
                                    let length = line.prefix_chars.min(chars.len());
                                    for value in hidden.iter_mut().take(length).skip(1) {
                                        *value = true;
                                    }
                                }
                                "code_fence" => {
                                    hidden.fill(true);
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
                                colors[index] = inline_color(&span.kind, base);
                            }
                            if (span.kind == "highlight" || span.kind == "inline_code") && end > start {
                                let color = if span.kind == "inline_code" { 0x111111 } else { 0x2a2140 };
                                overlays.push((start.min(chars.len()), end.min(chars.len()), color));
                            }
                        }
                    }
                }
                cell_offsets = line_cell_offsets(&chars, &hidden);
            }
            let chunk_start = chunk.start.min(chars.len());
            let chunk_end = chunk.end.min(chars.len());
            let base_cell = cell_offsets.get(chunk_start).copied().unwrap_or(0);
            let absolute_start = line_start + chunk.start;
            let absolute_end = line_start + chunk.end;
            for (overlay_start, overlay_end, color) in &overlays {
                let start = (*overlay_start).max(chunk_start);
                let end = (*overlay_end).min(chunk_end);
                let cell_start = cell_offsets
                    .get(start)
                    .copied()
                    .unwrap_or(0)
                    .saturating_sub(base_cell);
                let cell_end = cell_offsets
                    .get(end)
                    .copied()
                    .unwrap_or(0)
                    .saturating_sub(base_cell);
                if cell_end > cell_start {
                    rect(
                        pixels,
                        self.width,
                        self.height,
                        text_x + cell_start as f32 * chunk.advance,
                        y + 2.0 * density,
                        (cell_end - cell_start) as f32 * chunk.advance,
                        line_h - 4.0 * density,
                        *color,
                    );
                }
            }
            if selection_end > selection_start {
                let start = selection_start.max(absolute_start).min(absolute_end);
                let end = selection_end.max(absolute_start).min(absolute_end);
                if end > start {
                    let raw_start = (start - line_start).clamp(chunk_start, chunk_end);
                    let raw_end = (end - line_start).clamp(chunk_start, chunk_end);
                    let cell_start = cell_offsets
                        .get(raw_start)
                        .copied()
                        .unwrap_or(0)
                        .saturating_sub(base_cell);
                    let cell_end = cell_offsets
                        .get(raw_end)
                        .copied()
                        .unwrap_or(0)
                        .saturating_sub(base_cell);
                    if cell_end > cell_start {
                        let pad = 1.5 * density;
                        let left = (text_x + cell_start as f32 * chunk.advance - pad)
                            .max(gutter);
                        let right = text_x + cell_end as f32 * chunk.advance + pad;
                        rect(
                            pixels,
                            self.width,
                            self.height,
                            left,
                            y,
                            (right - left).max(2.0 * density),
                            line_h,
                            0x23173d,
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
                let mut segment = String::new();
                let start_idx = index;
                while index < chunk_end {
                    if hidden[index] {
                        break;
                    }
                    if colors[index] != color {
                        break;
                    }
                    segment.push(chars[index]);
                    index += 1;
                }
                let cell_pos = cell_offsets
                    .get(start_idx)
                    .copied()
                    .unwrap_or(0)
                    .saturating_sub(base_cell);
                let segment_x = text_x + cell_pos as f32 * chunk.advance;
                painter.draw(
                    pixels,
                    self.width,
                    self.height,
                    segment_x,
                    y + text_pad_y,
                    &segment,
                    chunk.font,
                    color,
                    true,
                );
                if chunk.font > font_px + 0.1 {
                    painter.draw(
                        pixels,
                        self.width,
                        self.height,
                        segment_x + (0.6 * density).round(),
                        y + text_pad_y,
                        &segment,
                        chunk.font,
                        color,
                        true,
                    );
                }
            }
            if visual == cursor_visual && selection_end == selection_start {
                let cursor_col = chunk_start + cursor_offset;
                let cursor_cell = cell_offsets
                    .get(cursor_col)
                    .copied()
                    .unwrap_or(cursor_col)
                    .saturating_sub(base_cell);
                let x = text_x + cursor_cell as f32 * chunk.advance;
                if self.editor.caret_visible {
                    rect(
                        pixels,
                        self.width,
                        self.height,
                        x,
                        y,
                        2.0 * density,
                        line_h,
                        0x8a5cf5,
                    );
                }
                if self.editor.insertion_handle_visible {
                    insertion_handle = Some((x + 1.0 * density, y + line_h));
                }
            }
            if selection_end > selection_start {
                if visual == start_visual {
                    let col = chunk_start + start_offset;
                    let cell = cell_offsets
                        .get(col)
                        .copied()
                        .unwrap_or(col)
                        .saturating_sub(base_cell);
                    start_handle = Some((
                        text_x + cell as f32 * chunk.advance,
                        y + line_h,
                    ));
                }
                if visual == end_visual {
                    let col = chunk_start + end_offset;
                    let cell = cell_offsets
                        .get(col)
                        .copied()
                        .unwrap_or(col)
                        .saturating_sub(base_cell);
                    end_handle = Some((
                        text_x + cell as f32 * chunk.advance,
                        y + line_h,
                    ));
                }
            }
        }
        rect(pixels, self.width, self.height, 0.0, 0.0, gutter, self.height as f32, 0x000000);
        let number_font_px = (12.0 * density).round();
        for visual in first..last {
            let chunk = self.editor.wrap[visual];
            if chunk.start != 0 {
                continue;
            }
            let y = top_inset + visual as f32 * line_h - scroll;
            let number = (chunk.line + 1).to_string();
            let number_width = painter.width(&number, number_font_px, true);
            painter.draw(
                pixels,
                self.width,
                self.height,
                gutter - (8.0 * density).round() - number_width,
                y + number_pad_y,
                &number,
                number_font_px,
                0x666666,
                true,
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
                    0x8a5cf5,
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
                    0x8a5cf5,
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
                    0x8a5cf5,
                );
            }
        }
        self.editor.model = model;
        self.editor.starts = starts;
        self.editor.byte_starts = byte_starts;
        self.painter = painter;
    }

    fn tree_touch(&mut self, _x: f32, y: f32, action: i32) -> Option<String> {
        let row_h = row_height(self.density);
        match action {
            0 => {
                self.tree.drag = Some((y, self.tree.scroll, true));
                None
            }
            1 => {
                if let Some((start_y, start_scroll, tap)) = self.tree.drag {
                    let distance = y - start_y;
                    let threshold = 8.0 * self.density;
                    if distance.abs() > threshold || !tap {
                        let delta_y = if tap {
                            distance - distance.signum() * threshold
                        } else {
                            distance
                        };
                        self.tree.drag = Some((y, start_scroll, false));
                        let max_y = (self.tree.doc.rows.len() as f32 * row_h - self.height as f32).max(0.0);
                        self.tree.scroll = (self.tree.scroll - delta_y).clamp(0.0, max_y);
                    }
                }
                None
            }
            2 => {
                let drag = self.tree.drag.take()?;
                if !drag.2 {
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

    fn editor_touch(&mut self, x: f32, y: f32, action: i32) -> bool {
        let line_h = line_height(self.density);
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
                            let max_y = (self.editor.top_inset + self.editor.wrap.len() as f32 * line_h
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
        let line_h = line_height(self.density);
        let (advance, gutter, content) = self.editor_metrics();
        self.ensure_wrap(content);
        let max_y = (self.editor.top_inset + self.editor.wrap.len() as f32 * line_h
            + self.editor.bottom_inset
            - self.viewport_height as f32)
            .max(0.0);
        (gutter, advance, max_y, 0.0)
    }

    fn editor_set_scroll(&mut self, y: f32, _x: f32) {
        let (_, _, max_y, _) = self.editor_scroll_metrics();
        self.editor.scroll = y.round().clamp(0.0, max_y);
        self.editor.scroll_x = 0.0;
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
        let (start, end) = self.selected_range();
        if end > start {
            self.delete_range(start, end);
        }
        let byte = byte_at(&self.editor.text, self.editor.selection);
        self.editor.text.insert_str(byte, text);
        self.editor.selection += text.chars().count();
        self.editor.anchor = self.editor.selection;
        self.editor.model_dirty = true;
        self.editor.wrap_dirty = true;
        self.editor.starts_dirty = true;
        self.scroll_to_cursor();
    }

    fn delete(&mut self, before: usize, after: usize) {
        self.invalidate();
        self.editor.insertion_handle_visible = false;
        let (selection_start, selection_end) = self.selected_range();
        if selection_end > selection_start {
            self.delete_range(selection_start, selection_end);
            self.editor.selection = selection_start;
            self.editor.anchor = selection_start;
            self.editor.model_dirty = true;
            self.editor.wrap_dirty = true;
            self.editor.starts_dirty = true;
            self.scroll_to_cursor();
            return;
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
        self.delete_range(start, end);
        self.editor.selection = start;
        self.editor.anchor = start;
        self.editor.model_dirty = true;
        self.editor.wrap_dirty = true;
        self.editor.starts_dirty = true;
        self.scroll_to_cursor();
    }

    fn replace_range(&mut self, start: usize, end: usize, text: &str) {
        self.invalidate();
        self.editor.insertion_handle_visible = false;
        let length = char_count(&self.editor.text);
        let start = start.min(length);
        let end = end.max(start).min(length);
        self.delete_range(start, end);
        if !text.is_empty() {
            let byte = byte_at(&self.editor.text, start);
            self.editor.text.insert_str(byte, text);
        }
        self.editor.selection = start + char_count(text);
        self.editor.anchor = self.editor.selection;
        self.editor.model_dirty = true;
        self.editor.wrap_dirty = true;
        self.editor.starts_dirty = true;
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
        if start >= end {
            return;
        }
        let start_byte = byte_at(&self.editor.text, start);
        let end_byte = byte_at(&self.editor.text, end);
        self.editor.text.replace_range(start_byte..end_byte, "");
        self.editor.selection = start;
        self.editor.anchor = start;
    }

    fn scroll_to_cursor(&mut self) {
        let line_h = line_height(self.density);
        let (_, _, content) = self.editor_metrics();
        self.ensure_wrap(content);
        let (visual, _) = self.visual_for_position(&self.editor.starts, self.editor.selection);
        let top = visual as f32 * line_h;
        let bottom = self.editor.top_inset + top + line_h + 24.0 * self.density;
        let visible = (self.viewport_height as f32 - self.editor.bottom_inset).max(line_h);
        if top < self.editor.scroll {
            self.editor.scroll = top;
        } else if bottom > self.editor.scroll + visible {
            self.editor.scroll = bottom - visible;
        }
        let max_y = (self.editor.top_inset + self.editor.wrap.len() as f32 * line_h
            + self.editor.bottom_inset
            - self.viewport_height as f32)
            .max(0.0);
        self.editor.scroll = self.editor.scroll.clamp(0.0, max_y);
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

    fn editor_position_at(&mut self, x: f32, y: f32) -> usize {
        let line_h = line_height(self.density);
        let (_, gutter, content) = self.editor_metrics();
        self.ensure_wrap(content);
        let visual = (((y + self.editor.scroll - self.editor.top_inset) / line_h)
            .floor()
            .max(0.0) as usize)
            .min(self.editor.wrap.len().saturating_sub(1));
        let Some(chunk) = self.editor.wrap.get(visual).copied() else {
            return 0;
        };
        let line_start = self.editor.starts.get(chunk.line).copied().unwrap_or(0);
        let raw = logical_line(&self.editor.text, &self.editor.byte_starts, chunk.line);
        let chars: Vec<char> = raw.chars().collect();
        let mut hidden = vec![false; chars.len()];
        let cursor_line = if self.editor.preview {
            usize::MAX
        } else {
            line_of(&self.editor.starts, self.editor.selection)
        };
        if chunk.line != cursor_line && !self.editor.model.is_empty() {
            if let Some(line) = self.editor.model.get(chunk.line) {
                match line.kind.as_str() {
                    "heading" => {
                        let length = line.prefix_chars.min(chars.len());
                        for val in hidden.iter_mut().take(length) {
                            *val = true;
                        }
                    }
                    "task" => {
                        if line.indent < chars.len() {
                            let hide_start = (line.indent + 2).min(chars.len());
                            let hide_end = line.prefix_chars.min(chars.len());
                            for val in hidden[hide_start..hide_end].iter_mut() {
                                *val = true;
                            }
                        }
                    }
                    "quote" => {
                        let length = line.prefix_chars.min(chars.len());
                        for val in hidden.iter_mut().take(length).skip(1) {
                            *val = true;
                        }
                    }
                    "code_fence" => {
                        hidden.fill(true);
                    }
                    _ => {}
                }
                let content_start = line.prefix_chars.min(chars.len());
                for span in &line.spans {
                    if span.kind == "marker" {
                        let s = (content_start + span.start).min(chars.len());
                        let e = (content_start + span.end).min(chars.len());
                        for val in hidden[s..e].iter_mut() {
                            *val = true;
                        }
                    }
                }
            }
        }
        let cell_offsets = line_cell_offsets(&chars, &hidden);
        let chunk_start = chunk.start.min(chars.len());
        let chunk_end = chunk.end.min(chars.len());
        let base_cell = cell_offsets.get(chunk_start).copied().unwrap_or(0);
        let target_cell = ((x + self.editor.scroll_x - gutter) / chunk.advance).max(0.0);

        let mut best_col = chunk_start;
        let mut best_dist = f32::MAX;
        for col in chunk_start..=chunk_end {
            if col > chunk_start && col < chunk_end {
                if cell_offsets[col] == cell_offsets[col.saturating_sub(1)] {
                    continue;
                }
            }
            let cell = cell_offsets.get(col).copied().unwrap_or(0).saturating_sub(base_cell) as f32;
            let dist = (cell - target_cell).abs();
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

    fn editor_layout_info(&mut self) -> String {
        let (_advance, _, content) = self.editor_metrics();
        self.ensure_wrap(content);
        json!({
            "row_height": line_height(self.density),
            "top": self.editor.top_inset,
            "first": self.editor.wrap_first,
            "count": self.editor.wrap_count,
            "total": self.editor.wrap.len(),
        })
        .to_string()
    }

    fn editor_selection_handles(&mut self) -> String {
        let (selection_start, selection_end) = self.selected_range();
        let line_h = line_height(self.density);
        let (_, gutter, content) = self.editor_metrics();
        self.ensure_wrap(content);
        let text_x = gutter;

        if selection_end > selection_start {
            let mut points = Vec::with_capacity(2);
            for position in [selection_start, selection_end] {
                let (visual, offset) = self.visual_for_position(&self.editor.starts, position);
                let (advance, cell) = if let Some(chunk) = self.editor.wrap.get(visual) {
                    let raw = logical_line(&self.editor.text, &self.editor.byte_starts, chunk.line);
                    let chars: Vec<char> = raw.chars().collect();
                    let cell_offsets = line_cell_offsets(&chars, &[]);
                    let chunk_start = chunk.start.min(chars.len());
                    let base_cell = cell_offsets.get(chunk_start).copied().unwrap_or(0);
                    let col = (chunk_start + offset).min(chars.len());
                    let c = cell_offsets.get(col).copied().unwrap_or(col).saturating_sub(base_cell);
                    (chunk.advance, c)
                } else {
                    (0.0, offset)
                };
                points.push(json!({
                    "x": text_x + cell as f32 * advance,
                    "y": self.editor.top_inset + visual as f32 * line_h - self.editor.scroll + line_h,
                }));
            }
            json!({
                "start": points[0],
                "end": points[1],
                "row_height": line_h,
                "size": 24.0 * self.density,
            })
            .to_string()
        } else {
            let (visual, offset) = self.visual_for_position(&self.editor.starts, self.editor.selection);
            let (advance, cell) = if let Some(chunk) = self.editor.wrap.get(visual) {
                let raw = logical_line(&self.editor.text, &self.editor.byte_starts, chunk.line);
                let chars: Vec<char> = raw.chars().collect();
                let cell_offsets = line_cell_offsets(&chars, &[]);
                let chunk_start = chunk.start.min(chars.len());
                let base_cell = cell_offsets.get(chunk_start).copied().unwrap_or(0);
                let col = (chunk_start + offset).min(chars.len());
                let c = cell_offsets.get(col).copied().unwrap_or(col).saturating_sub(base_cell);
                (chunk.advance, c)
            } else {
                (0.0, offset)
            };
            let cursor_x = text_x + cell as f32 * advance + 1.0 * self.density;
            let cursor_y = self.editor.top_inset + visual as f32 * line_h - self.editor.scroll + line_h;
            json!({
                "insertion": {
                    "x": cursor_x,
                    "y": cursor_y,
                },
                "row_height": line_h,
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
    unsafe {
        __android_log_print(
            3,
            b"VibecodiumRust\0".as_ptr(),
            b"nativeSurfaceRender: lock=%.2fms, paint=%.2fms, post=%.2fms\0".as_ptr(),
            t_lock.as_secs_f64() * 1000.0,
            t_paint.as_secs_f64() * 1000.0,
            t_post.as_secs_f64() * 1000.0,
        );
    }
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
        state.background = 0x282828;
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
        state.background = 0x000000;
        if state.editor.text != text {
            state.editor.text = text;
            state.editor.model_dirty = true;
            state.editor.wrap_dirty = true;
            state.editor.starts_dirty = true;
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
        state.editor.preview = preview != 0;
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
                    input.mode.store(mode, Ordering::Relaxed);
                    input.max_y_bits.store(max_y.to_bits(), Ordering::Relaxed);
                    input.drag_active.store(true, Ordering::Relaxed);
                    input.moved.store(false, Ordering::Relaxed);
                    input.start_x_bits.store(x.to_bits(), Ordering::Relaxed);
                    input.start_y_bits.store(y.to_bits(), Ordering::Relaxed);
                    input.start_scroll_bits.store(scroll.round().to_bits(), Ordering::Relaxed);
                    input.last_y_bits.store(y.to_bits(), Ordering::Relaxed);
                    input.scroll_bits.store(scroll.round().to_bits(), Ordering::Relaxed);
                    input.scroll_dirty.store(false, Ordering::Relaxed);
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
                            drag.2 = false;
                        }
                    }
                    if input.scroll_dirty.swap(false, Ordering::Relaxed) {
                        let scroll =
                            f32::from_bits(input.scroll_bits.load(Ordering::Relaxed));
                        if state.tree.doc.rows.is_empty() {
                            state.editor_set_scroll(scroll, 0.0);
                        } else {
                            state.tree_set_scroll(scroll);
                        }
                    }
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
    fn test_wrap_layout_emoji_boundary() {
        let text = "hello 😀 world";
        let starts = vec![0];
        let metrics = vec![(15.0, 10.0)];
        // Content width of 100.0 fits floor(100/10) = 10 cells
        // "hello " is 6 cells.
        // 😀 is 2 cells.
        // "hello 😀 " is 9 cells.
        // "world" is 5 cells, cannot fit on line 1 (9 + 5 = 14 > 10).
        // It wraps after the space at index 7: line 1 gets "hello 😀 ", line 2 gets "world".
        let rows = wrap_layout(text, &starts, 100.0, &metrics);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].start, 0);
        assert_eq!(rows[0].end, 8); // "hello 😀 "
        assert_eq!(rows[1].start, 8); // "world"
    }
}
