#[cfg(windows)]
use std::io::Cursor;
use std::path::{Path, PathBuf};


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
