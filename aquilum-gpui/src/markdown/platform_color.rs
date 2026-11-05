#[cfg(target_os = "windows")]
use super::colors::*;

/// Opens the native OS Color Picker dialog (e.g. ChooseColorW on Windows) to visually pick any color.
#[cfg(target_os = "windows")]
pub fn open_system_color_picker(initial_hex: &str) -> Option<String> {
    #[repr(C)]
    struct CHOOSECOLORW {
        l_struct_size: u32,
        hwnd_owner: *mut std::ffi::c_void,
        h_instance: *mut std::ffi::c_void,
        rgb_result: u32,
        lp_cust_colors: *mut u32,
        flags: u32,
        l_cust_data: isize,
        lpfn_hook: Option<unsafe extern "system" fn(*mut std::ffi::c_void, u32, usize, isize) -> usize>,
        lp_template_name: *const u16,
    }

    #[link(name = "comdlg32")]
    extern "system" {
        fn ChooseColorW(lpcc: *mut CHOOSECOLORW) -> i32;
    }

    static CUSTOM_COLORS: std::sync::Mutex<[u32; 16]> = std::sync::Mutex::new([0x00FFFFFF; 16]);

    let init_rgb = parse_hex_color(initial_hex)
        .or_else(|| {
            let p = find_heading_preset(initial_hex);
            let (r, g, b) = p.rgb;
            Some(gpui::rgb(((r as u32) << 16) | ((g as u32) << 8) | (b as u32)))
        })
        .unwrap_or(gpui::rgb(0x0072B2));

    let r = (init_rgb.r * 255.0).round() as u32;
    let g = (init_rgb.g * 255.0).round() as u32;
    let b = (init_rgb.b * 255.0).round() as u32;
    let win_rgb = (r & 0xFF) | ((g & 0xFF) << 8) | ((b & 0xFF) << 16);

    const CC_RGBINIT: u32 = 0x00000001;
    const CC_FULLOPEN: u32 = 0x00000002;
    const CC_ANYCOLOR: u32 = 0x00000100;

    let mut custom_colors = CUSTOM_COLORS.lock().unwrap();

    let mut cc = CHOOSECOLORW {
        l_struct_size: std::mem::size_of::<CHOOSECOLORW>() as u32,
        hwnd_owner: std::ptr::null_mut(),
        h_instance: std::ptr::null_mut(),
        rgb_result: win_rgb,
        lp_cust_colors: custom_colors.as_mut_ptr(),
        flags: CC_RGBINIT | CC_FULLOPEN | CC_ANYCOLOR,
        l_cust_data: 0,
        lpfn_hook: None,
        lp_template_name: std::ptr::null(),
    };

    let result = unsafe { ChooseColorW(&mut cc) };
    if result != 0 {
        let res_r = cc.rgb_result & 0xFF;
        let res_g = (cc.rgb_result >> 8) & 0xFF;
        let res_b = (cc.rgb_result >> 16) & 0xFF;
        Some(format!("#{:02X}{:02X}{:02X}", res_r, res_g, res_b))
    } else {
        None
    }
}

/// Samples the pixel color at the current mouse cursor position on Windows.
#[cfg(target_os = "windows")]
pub fn sample_screen_color_at_cursor() -> Option<String> {
    #[repr(C)]
    struct POINT {
        x: i32,
        y: i32,
    }
    #[link(name = "user32")]
    extern "system" {
        fn GetCursorPos(lp_point: *mut POINT) -> i32;
        fn GetDC(h_wnd: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        fn ReleaseDC(h_wnd: *mut std::ffi::c_void, h_dc: *mut std::ffi::c_void) -> i32;
    }
    #[link(name = "gdi32")]
    extern "system" {
        fn GetPixel(h_dc: *mut std::ffi::c_void, x: i32, y: i32) -> u32;
    }

    let mut pt = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut pt) } == 0 {
        return None;
    }
    let hdc = unsafe { GetDC(std::ptr::null_mut()) };
    if hdc.is_null() {
        return None;
    }
    let pixel = unsafe { GetPixel(hdc, pt.x, pt.y) };
    unsafe { ReleaseDC(std::ptr::null_mut(), hdc) };

    if pixel == 0xFFFFFFFF {
        return None;
    }
    let r = pixel & 0xFF;
    let g = (pixel >> 8) & 0xFF;
    let b = (pixel >> 16) & 0xFF;
    Some(format!("#{:02X}{:02X}{:02X}", r, g, b))
}

#[cfg(not(target_os = "windows"))]
pub fn sample_screen_color_at_cursor() -> Option<String> {
    None
}

#[cfg(not(target_os = "windows"))]
pub fn open_system_color_picker(_initial_hex: &str) -> Option<String> {
    None
}
