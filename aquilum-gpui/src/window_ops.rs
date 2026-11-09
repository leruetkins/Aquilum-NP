//! Window operations GPUI does not expose directly (always-on-top, maximize
//! after mapping, OS-driven move).

/// Puts the window above all others.
///
/// Windows: `SetWindowPos` with `HWND_TOPMOST`. Linux/X11: an EWMH
/// `_NET_WM_STATE` client message with `_NET_WM_STATE_ABOVE`. Wayland has no
/// protocol for this, so the call is a no-op there.
pub fn set_always_on_top(window: &gpui::Window, on_top: bool) {
    #[cfg(target_os = "windows")]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use windows_sys::Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{
                SetWindowPos, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
            },
        };

        // `gpui::Window` has an inherent `window_handle()` returning an
        // `AnyWindowHandle`, so the trait method is called explicitly.
        if let Ok(handle) = <gpui::Window as HasWindowHandle>::window_handle(window) {
            if let RawWindowHandle::Win32(h) = handle.as_ref() {
                let hwnd: HWND = h.hwnd.get() as HWND;
                unsafe {
                    SetWindowPos(
                        hwnd,
                        if on_top { HWND_TOPMOST } else { HWND_NOTOPMOST },
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    set_always_on_top_x11(window, on_top);

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        let _ = (window, on_top);
    }
}

/// Maximizes the window.
///
/// GPUI applies `WindowBounds::Maximized` before the platform window is
/// mapped. On Windows that is enough; on X11 the window manager ignores the
/// `_NET_WM_STATE` request for a window it does not manage yet, so the
/// restore path calls this after the first frame instead. The X11 request uses
/// `_NET_WM_STATE_ADD`, which is idempotent.
pub fn maximize_window(window: &gpui::Window) {
    #[cfg(target_os = "windows")]
    window.zoom_window();

    #[cfg(target_os = "linux")]
    {
        if !maximize_x11(window) {
            // Wayland has no xcb handle; `zoom_window` sets xdg_toplevel
            // maximized, which is valid before the surface is mapped.
            window.zoom_window();
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    let _ = window;
}

/// Hands the window to the OS or window manager to move it with the mouse
/// until the button is released.
///
/// Windows: posted as a non-client caption drag (the modal move loop must not
/// run inside a view callback). Linux: `_NET_WM_MOVERESIZE` on X11 and
/// `xdg_toplevel.move` on Wayland.
pub fn start_window_drag(window: &gpui::Window) {
    window.start_window_move();
}

#[cfg(target_os = "linux")]
use x11rb::protocol::xproto::{self, ClientMessageEvent};

/// Returns the xcb window id, the root window it lives on and a wrapper around
/// GPUI's xcb connection. `None` on Wayland or when the handles are missing.
#[cfg(target_os = "linux")]
fn x11_window_connection(
    window: &gpui::Window,
) -> Option<(
    xproto::Window,
    xproto::Window,
    x11rb::xcb_ffi::XCBConnection,
)> {
    use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
    use x11rb::protocol::xproto::ConnectionExt as _;

    let window_handle = <gpui::Window as HasWindowHandle>::window_handle(window).ok()?;
    let window_id = match window_handle.as_raw() {
        RawWindowHandle::Xcb(h) => h.window.get(),
        _ => return None,
    };

    let display_handle = <gpui::Window as HasDisplayHandle>::display_handle(window).ok()?;
    let connection = match display_handle.as_raw() {
        RawDisplayHandle::Xcb(d) => d.connection?,
        _ => return None,
    };

    // Safety: the pointer belongs to the live GPUI window's xcb connection,
    // which outlives this call; `false` keeps ownership with GPUI.
    let conn = unsafe {
        x11rb::xcb_ffi::XCBConnection::from_raw_xcb_connection(connection.as_ptr(), false)
    }
    .ok()?;

    // The top-level window's parent is the root of the screen it lives on,
    // which is not necessarily screen 0.
    let root = conn.query_tree(window_id).ok()?.reply().ok()?.root;
    Some((window_id, root, conn))
}

#[cfg(target_os = "linux")]
fn intern_atom(conn: &x11rb::xcb_ffi::XCBConnection, name: &[u8]) -> Option<xproto::Atom> {
    use x11rb::protocol::xproto::ConnectionExt as _;

    conn.intern_atom(false, name)
        .ok()?
        .reply()
        .ok()
        .map(|reply| reply.atom)
}

#[cfg(target_os = "linux")]
fn send_wm_state(
    conn: &x11rb::xcb_ffi::XCBConnection,
    root: xproto::Window,
    event: ClientMessageEvent,
) {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{ConnectionExt as _, EventMask};

    let _ = conn.send_event(
        false,
        root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        event,
    );
    let _ = conn.flush();
}

#[cfg(target_os = "linux")]
fn set_always_on_top_x11(window: &gpui::Window, on_top: bool) {
    let Some((window_id, root, conn)) = x11_window_connection(window) else {
        return;
    };
    let Some(wm_state) = intern_atom(&conn, b"_NET_WM_STATE") else {
        return;
    };
    let Some(above) = intern_atom(&conn, b"_NET_WM_STATE_ABOVE") else {
        return;
    };

    send_wm_state(
        &conn,
        root,
        net_wm_state_event(window_id, wm_state, above, 0, on_top),
    );
}

/// Returns false when the window has no xcb handle (Wayland).
#[cfg(target_os = "linux")]
fn maximize_x11(window: &gpui::Window) -> bool {
    let Some((window_id, root, conn)) = x11_window_connection(window) else {
        return false;
    };
    let Some(wm_state) = intern_atom(&conn, b"_NET_WM_STATE") else {
        return true;
    };
    let Some(vertical) = intern_atom(&conn, b"_NET_WM_STATE_MAXIMIZED_VERT") else {
        return true;
    };
    let Some(horizontal) = intern_atom(&conn, b"_NET_WM_STATE_MAXIMIZED_HORZ") else {
        return true;
    };

    send_wm_state(
        &conn,
        root,
        net_wm_state_event(window_id, wm_state, vertical, horizontal, true),
    );
    true
}

/// Builds an EWMH `_NET_WM_STATE` client message.
///
/// `add` maps to `_NET_WM_STATE_ADD` and `false` to `_NET_WM_STATE_REMOVE`;
/// a zero `second` property means "none".
#[cfg(target_os = "linux")]
fn net_wm_state_event(
    window_id: xproto::Window,
    wm_state: xproto::Atom,
    first: xproto::Atom,
    second: xproto::Atom,
    add: bool,
) -> xproto::ClientMessageEvent {
    let action: u32 = if add { 1 } else { 0 };
    // Data: [action, first property, second property, source indication].
    ClientMessageEvent::new(32, window_id, wm_state, [action, first, second, 1, 0])
}

#[cfg(all(test, target_os = "linux"))]
mod tests;
