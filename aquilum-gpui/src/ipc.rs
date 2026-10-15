use std::path::PathBuf;
use crate::settings::{APP_URL_SCHEME, APP_IPC_PIPE_NAME, get_setting_by_id};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VibecodiumAction {
    OpenSettings {
        tab: Option<String>,
        setting_id: Option<String>,
    },
    OpenFile(PathBuf),
    FocusApp,
}

pub fn parse_vibecodium_url(url: &str) -> Option<VibecodiumAction> {
    let trimmed = url.trim();
    let prefix = format!("{}://", APP_URL_SCHEME);
    if !trimmed.starts_with(&prefix) {
        return None;
    }
    let rest = &trimmed[prefix.len()..];
    let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return Some(VibecodiumAction::OpenSettings {
            tab: None,
            setting_id: None,
        });
    }

    match parts[0] {
        "settings" => {
            if parts.len() == 1 {
                Some(VibecodiumAction::OpenSettings {
                    tab: None,
                    setting_id: None,
                })
            } else {
                let id = parts[1];
                if let Some(s) = get_setting_by_id(id) {
                    Some(VibecodiumAction::OpenSettings {
                        tab: Some(s.tab_id.to_string()),
                        setting_id: Some(s.id.to_string()),
                    })
                } else {
                    Some(VibecodiumAction::OpenSettings {
                        tab: None,
                        setting_id: Some(id.to_string()),
                    })
                }
            }
        }
        _ => Some(VibecodiumAction::FocusApp),
    }
}

pub fn parse_command_line_action(arg: &str) -> VibecodiumAction {
    let trimmed = arg.trim().trim_matches('"');
    if let Some(action) = parse_vibecodium_url(trimmed) {
        action
    } else {
        let path = PathBuf::from(trimmed);
        if path.exists() {
            VibecodiumAction::OpenFile(path)
        } else {
            VibecodiumAction::FocusApp
        }
    }
}

#[cfg(target_os = "windows")]
pub mod platform {
    use super::*;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::sync::mpsc;
    use std::io::Write;

    type HANDLE = *mut std::ffi::c_void;
    type BOOL = i32;
    type DWORD = u32;

    const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;
    const PIPE_ACCESS_DUPLEX: DWORD = 0x00000003;
    const PIPE_TYPE_MESSAGE: DWORD = 0x00000004;
    const PIPE_READMODE_MESSAGE: DWORD = 0x00000002;
    const PIPE_WAIT: DWORD = 0x00000000;
    const PIPE_UNLIMITED_INSTANCES: DWORD = 255;

    extern "system" {
        fn CreateNamedPipeW(
            lpName: *const u16,
            dwOpenMode: DWORD,
            dwPipeMode: DWORD,
            nMaxInstances: DWORD,
            nOutBufferSize: DWORD,
            nInBufferSize: DWORD,
            nDefaultTimeOut: DWORD,
            lpSecurityAttributes: *mut std::ffi::c_void,
        ) -> HANDLE;

        fn ConnectNamedPipe(hNamedPipe: HANDLE, lpOverlapped: *mut std::ffi::c_void) -> BOOL;
        fn DisconnectNamedPipe(hNamedPipe: HANDLE) -> BOOL;
        fn CloseHandle(hObject: HANDLE) -> BOOL;
        fn ReadFile(
            hFile: HANDLE,
            lpBuffer: *mut u8,
            nNumberOfBytesToRead: DWORD,
            lpNumberOfBytesRead: *mut DWORD,
            lpOverlapped: *mut std::ffi::c_void,
        ) -> BOOL;
    }

    /// Attempts to send payload to an already running primary instance.
    /// Returns true if successfully delivered.
    pub fn try_send_to_primary_instance(payload: &str) -> bool {
        if let Ok(mut file) = std::fs::OpenOptions::new().write(true).open(APP_IPC_PIPE_NAME) {
            let _ = file.write_all(payload.as_bytes());
            let _ = file.flush();
            true
        } else {
            false
        }
    }

    /// Spawns background thread serving IPC messages on the named pipe.
    pub fn start_ipc_server(tx: mpsc::Sender<String>) {
        std::thread::spawn(move || {
            let pipe_name_w: Vec<u16> = OsStr::new(APP_IPC_PIPE_NAME)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();

            loop {
                let handle = unsafe {
                    CreateNamedPipeW(
                        pipe_name_w.as_ptr(),
                        PIPE_ACCESS_DUPLEX,
                        PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
                        PIPE_UNLIMITED_INSTANCES,
                        4096,
                        4096,
                        0,
                        std::ptr::null_mut(),
                    )
                };

                if handle == INVALID_HANDLE_VALUE {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    continue;
                }

                let connected = unsafe { ConnectNamedPipe(handle, std::ptr::null_mut()) != 0 };
                if connected {
                    let mut buf = vec![0u8; 4096];
                    let mut bytes_read: DWORD = 0;
                    let success = unsafe {
                        ReadFile(
                            handle,
                            buf.as_mut_ptr(),
                            buf.len() as DWORD,
                            &mut bytes_read,
                            std::ptr::null_mut(),
                        ) != 0
                    };

                    if success && bytes_read > 0 {
                        if let Ok(msg) = std::str::from_utf8(&buf[..bytes_read as usize]) {
                            let _ = tx.send(msg.to_string());
                        }
                    }
                    unsafe {
                        DisconnectNamedPipe(handle);
                    }
                }
                unsafe {
                    CloseHandle(handle);
                }
            }
        });
    }

    /// Registers vibecodium:// protocol handler in Windows registry (HKCU).
    pub fn register_url_scheme() {
        if let Ok(current_exe) = std::env::current_exe() {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;

            let exe_str = current_exe.to_string_lossy();
            let cmd = format!("\"{}\" \"%1\"", exe_str);

            let _ = std::process::Command::new("reg")
                .creation_flags(CREATE_NO_WINDOW)
                .args(&["add", &format!(r"HKCU\Software\Classes\{}", APP_URL_SCHEME), "/ve", "/t", "REG_SZ", "/d", "URL:Vibecodium Protocol", "/f"])
                .output();

            let _ = std::process::Command::new("reg")
                .creation_flags(CREATE_NO_WINDOW)
                .args(&["add", &format!(r"HKCU\Software\Classes\{}", APP_URL_SCHEME), "/v", "URL Protocol", "/t", "REG_SZ", "/d", "", "/f"])
                .output();

            let _ = std::process::Command::new("reg")
                .creation_flags(CREATE_NO_WINDOW)
                .args(&["add", &format!(r"HKCU\Software\Classes\{}\shell\open\command", APP_URL_SCHEME), "/ve", "/t", "REG_SZ", "/d", &cmd, "/f"])
                .output();
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub mod platform {
    use super::*;
    use std::sync::mpsc;

    pub fn try_send_to_primary_instance(_payload: &str) -> bool {
        false
    }

    pub fn start_ipc_server(_tx: mpsc::Sender<String>) {}

    pub fn register_url_scheme() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_vibecodium_url() {
        let url = "vibecodium://settings/editor.wordWrap";
        let action = parse_vibecodium_url(url).expect("must parse");
        assert_eq!(
            action,
            VibecodiumAction::OpenSettings {
                tab: Some("editor".to_string()),
                setting_id: Some("editor.wordWrap".to_string()),
            }
        );

        let url_general = "vibecodium://settings";
        let action_gen = parse_vibecodium_url(url_general).expect("must parse settings root");
        assert_eq!(
            action_gen,
            VibecodiumAction::OpenSettings {
                tab: None,
                setting_id: None,
            }
        );

        let invalid = "http://localhost:3000";
        assert!(parse_vibecodium_url(invalid).is_none());
    }
}
