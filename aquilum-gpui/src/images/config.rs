use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use super::import::{folder_name, DEFAULT_FOLDER};
use super::{invalidate_index, warm_ref_index};


// The vault root and folder name are process-wide: the editor window is built
// from view state that has no access to the vault, and a single app instance
// works with a single vault at a time.
static VAULT_ROOT: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
static FOLDER: OnceLock<Mutex<String>> = OnceLock::new();

fn vault_cell() -> &'static Mutex<Option<PathBuf>> {
    VAULT_ROOT.get_or_init(|| Mutex::new(None))
}

fn folder_cell() -> &'static Mutex<String> {
    FOLDER.get_or_init(|| Mutex::new(DEFAULT_FOLDER.to_string()))
}

pub fn set_vault_root(root: Option<PathBuf>) {
    *vault_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = root.clone();
    invalidate_index();
    if let Some(r) = root {
        warm_ref_index(r);
    }
}

pub fn vault_root() -> Option<PathBuf> {
    vault_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

pub fn set_folder(folder: &str) {
    let normalized = folder_name(folder);
    let mut guard = folder_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if *guard != normalized {
        *guard = normalized;
    }
}

pub fn folder() -> String {
    folder_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}
