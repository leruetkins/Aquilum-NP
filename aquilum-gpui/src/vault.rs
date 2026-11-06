use std::path::{Component, Path, PathBuf};

/// File information compatible with TFile from the Obsidian API.
/// Used in Snapshot for synchronous plugin getters.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileInfo {
    /// Relative path from the vault root using forward slashes ('/').
    pub path: String,
    /// File name with extension.
    pub name: String,
    /// Name without extension.
    pub basename: String,
    /// Extension without the dot ("md", "txt", ...).
    pub extension: String,
}

impl FileInfo {
    pub fn from_path(root: &Path, abs: &Path) -> Option<Self> {
        let rel = abs.strip_prefix(root).ok()?;
        let name = abs.file_name()?.to_string_lossy().into_owned();
        let ext = abs.extension().unwrap_or_default().to_string_lossy().into_owned();
        let basename = abs.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        Some(Self {
            path: rel.to_string_lossy().replace('\\', "/"),
            name,
            basename,
            extension: ext,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultError {
    InvalidPath(String),
    AlreadyExists(String),
    NotFound(String),
    Io(String),
}

impl std::fmt::Display for VaultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPath(s) => write!(f, "invalid path: {s}"),
            Self::AlreadyExists(s) => write!(f, "already exists: {s}"),
            Self::NotFound(s) => write!(f, "not found: {s}"),
            Self::Io(s) => write!(f, "io error: {s}"),
        }
    }
}

impl std::error::Error for VaultError {}

/// Vault core with no GPUI dependency.
/// Manages the note file structure, path validation, and plugin data.
pub struct VaultCore {
    /// Absolute path to the vault root.
    pub root: PathBuf,
    /// Vault name.
    pub name: String,
    markdown_files_cache: std::sync::Mutex<Option<std::sync::Arc<[FileInfo]>>>,
}

impl VaultCore {
    pub fn open(path: PathBuf) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Workspace".to_owned());
        Self {
            root: path,
            name,
            markdown_files_cache: std::sync::Mutex::new(None),
        }
    }

    /// Safe path resolution relative to the vault root.
    ///
    /// Normalization algorithm:
    /// 1. Rejects null bytes and absolute paths.
    /// 2. Lexically resolves `.` and `..` without accessing the filesystem.
    /// 3. Finds the nearest existing ancestor and calls canonicalize to expand symlinks.
    /// 4. Appends the remaining path and verifies that the result does not escape the vault root.
    /// 5. Blocks direct access to `.vault/` (except explicit plugin data calls).
    pub fn resolve(&self, rel: &str) -> Result<PathBuf, VaultError> {
        self.resolve_internal(rel, false)
    }

    fn resolve_internal(&self, rel: &str, allow_reserved: bool) -> Result<PathBuf, VaultError> {
        if rel.contains('\0') {
            return Err(VaultError::InvalidPath("contains null byte".into()));
        }
        let rel_path = Path::new(rel);
        if rel_path.is_absolute() || rel_path.has_root() {
            return Err(VaultError::InvalidPath("path must be relative".into()));
        }

        // Lexical normalization without filesystem access
        let mut stack: Vec<&std::ffi::OsStr> = Vec::new();
        for c in rel_path.components() {
            match c {
                Component::Normal(s) => stack.push(s),
                Component::CurDir => {}
                Component::ParentDir => {
                    if stack.pop().is_none() {
                        return Err(VaultError::InvalidPath("escapes vault root via ..".into()));
                    }
                }
                Component::Prefix(_) | Component::RootDir => {
                    return Err(VaultError::InvalidPath("absolute component not allowed".into()));
                }
            }
        }

        let joined = stack.iter().fold(self.root.clone(), |p, s| p.join(s));

        // Expand symlinks at the nearest existing ancestor
        let mut existing = joined.as_path();
        let mut tail: Vec<std::ffi::OsString> = Vec::new();
        let canon_prefix = loop {
            match existing.canonicalize() {
                Ok(p) => break p,
                Err(_) => {
                    tail.push(
                        existing
                            .file_name()
                            .ok_or_else(|| VaultError::InvalidPath("invalid path structure".into()))?
                            .to_owned(),
                    );
                    existing = existing
                        .parent()
                        .ok_or_else(|| VaultError::InvalidPath("no valid parent directory".into()))?;
                }
            }
        };

        let mut resolved = canon_prefix;
        for part in tail.into_iter().rev() {
            resolved.push(part);
        }

        // Canonicalize the vault root for prefix comparison
        let root_canon = match self.root.canonicalize() {
            Ok(p) => p,
            Err(_) => self.root.clone(),
        };

        if !resolved.starts_with(&root_canon) {
            return Err(VaultError::InvalidPath("resolved path escapes vault".into()));
        }

        // Deny access to the hidden service directory .vault
        if !allow_reserved && resolved.starts_with(root_canon.join(".vault")) {
            return Err(VaultError::InvalidPath("access to .vault is reserved".into()));
        }

        Ok(resolved)
    }

    /// Reads a text file from the vault. The path is relative.
    pub fn read_file(&self, rel_path: &str) -> Result<String, VaultError> {
        let abs = self.resolve(rel_path)?;
        if !abs.exists() {
            return Err(VaultError::NotFound(rel_path.to_string()));
        }
        let meta = std::fs::metadata(&abs).map_err(|e| VaultError::Io(e.to_string()))?;
        if meta.len() > 10 * 1024 * 1024 {
            // Read at most 1 MB for oversized files to prevent QuickJS heap exhaustion or freezing
            use std::io::Read;
            let mut file = std::fs::File::open(&abs).map_err(|e| VaultError::Io(e.to_string()))?;
            let mut buffer = vec![0u8; 1024 * 1024];
            let n = file.read(&mut buffer).map_err(|e| VaultError::Io(e.to_string()))?;
            buffer.truncate(n);
            return Ok(String::from_utf8_lossy(&buffer).into_owned());
        }
        std::fs::read_to_string(&abs).map_err(|e| VaultError::Io(e.to_string()))
    }

    /// Writes/overwrites a file in the vault. The path is relative.
    pub fn modify_file(&self, rel_path: &str, data: &str) -> Result<(), VaultError> {
        let abs = self.resolve(rel_path)?;
        if let Some(parent) = abs.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&abs, data).map_err(|e| VaultError::Io(e.to_string()))
    }

    /// Creates a new file. Returns the absolute path. Errors if the file already exists.
    pub fn create_file(&self, rel_path: &str, data: &str) -> Result<PathBuf, VaultError> {
        let abs = self.resolve(rel_path)?;
        if abs.exists() {
            return Err(VaultError::AlreadyExists(rel_path.to_string()));
        }
        if let Some(parent) = abs.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&abs, data).map_err(|e| VaultError::Io(e.to_string()))?;
        self.invalidate_markdown_cache();
        Ok(abs)
    }

    /// Upper bound for plugin binary operations: base64-in-JSON transport
    /// is acceptable for images and attachments, but not for gigabyte-sized files.
    const BINARY_LIMIT: u64 = 50 * 1024 * 1024;

    /// Reads a file as bytes (base64 encoding is done by the bridge).
    pub fn read_file_binary(&self, rel_path: &str) -> Result<Vec<u8>, VaultError> {
        let abs = self.resolve(rel_path)?;
        if !abs.exists() {
            return Err(VaultError::NotFound(rel_path.to_string()));
        }
        let meta = std::fs::metadata(&abs).map_err(|e| VaultError::Io(e.to_string()))?;
        if meta.len() > Self::BINARY_LIMIT {
            return Err(VaultError::Io(format!(
                "file too large for vault.readBinary ({} bytes, limit 50 MB)",
                meta.len()
            )));
        }
        std::fs::read(&abs).map_err(|e| VaultError::Io(e.to_string()))
    }

    /// Overwrites or creates a binary file.
    pub fn modify_file_binary(&self, rel_path: &str, data: &[u8]) -> Result<(), VaultError> {
        if data.len() as u64 > Self::BINARY_LIMIT {
            return Err(VaultError::Io(format!(
                "payload too large for vault.modifyBinary ({} bytes, limit 50 MB)",
                data.len()
            )));
        }
        let abs = self.resolve(rel_path)?;
        if let Some(parent) = abs.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&abs, data).map_err(|e| VaultError::Io(e.to_string()))
    }

    /// Creates a new binary file. Errors if the file already exists.
    pub fn create_file_binary(&self, rel_path: &str, data: &[u8]) -> Result<PathBuf, VaultError> {
        if data.len() as u64 > Self::BINARY_LIMIT {
            return Err(VaultError::Io(format!(
                "payload too large for vault.createBinary ({} bytes, limit 50 MB)",
                data.len()
            )));
        }
        let abs = self.resolve(rel_path)?;
        if abs.exists() {
            return Err(VaultError::AlreadyExists(rel_path.to_string()));
        }
        if let Some(parent) = abs.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&abs, data).map_err(|e| VaultError::Io(e.to_string()))?;
        self.invalidate_markdown_cache();
        Ok(abs)
    }

    /// Deletes a file.
    pub fn delete_file(&self, rel_path: &str) -> Result<(), VaultError> {
        let abs = self.resolve(rel_path)?;
        if !abs.exists() {
            return Err(VaultError::NotFound(rel_path.to_string()));
        }
        let note_content = crate::trash::note_content(&abs);
        std::fs::remove_file(&abs).map_err(|e| VaultError::Io(e.to_string()))?;
        if let Some(content) = note_content {
            crate::images::trash_note_attachments(
                &self.root,
                &abs,
                &content,
                &crate::images::folder(),
                &[abs.as_path()],
            );
        }
        self.invalidate_markdown_cache();
        Ok(())
    }

    /// Safe deletion: the file is moved to the vault trash (`.trash/<rel_path>`)
    /// instead of being permanently deleted. `.trash` is a hidden directory, so the trash
    /// does not appear in the snapshot or the file tree.
    pub fn trash_file(&self, rel_path: &str) -> Result<(), VaultError> {
        let abs = self.resolve(rel_path)?;
        if !abs.exists() {
            return Err(VaultError::NotFound(rel_path.to_string()));
        }
        crate::trash::trash_note(&self.root, &abs, rel_path)
            .map_err(|error| VaultError::Io(error.to_string()))?;
        self.invalidate_markdown_cache();
        Ok(())
    }

    /// Renames / moves a file inside the vault.
    pub fn rename_file(&self, old_rel: &str, new_rel: &str) -> Result<PathBuf, VaultError> {
        let old_abs = self.resolve(old_rel)?;
        let new_abs = self.resolve(new_rel)?;
        if !old_abs.exists() {
            return Err(VaultError::NotFound(old_rel.to_string()));
        }
        if new_abs.exists() {
            return Err(VaultError::AlreadyExists(new_rel.to_string()));
        }
        if let Some(parent) = new_abs.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let is_note = crate::trash::note_content(&old_abs).is_some();
        std::fs::rename(&old_abs, &new_abs).map_err(|e| VaultError::Io(e.to_string()))?;
        if is_note {
            crate::images::move_note_attachments(
                &self.root,
                &old_abs,
                &new_abs,
                &crate::images::folder(),
            );
        }
        self.invalidate_markdown_cache();
        Ok(new_abs)
    }

    /// Invalidates the markdown file list cache.
    pub fn invalidate_markdown_cache(&self) {
        if let Ok(mut cache) = self.markdown_files_cache.lock() {
            *cache = None;
        }
        crate::images::invalidate_ref_index();

    }

    /// Cached list of all markdown files for `Snapshot`, shared by `Arc` so
    /// publishing a snapshot never copies the whole list (`publish_snapshot`
    /// runs on every save).
    pub fn list_markdown_files(&self) -> std::sync::Arc<[FileInfo]> {
        if let Ok(cache) = self.markdown_files_cache.lock() {
            if let Some(ref files) = *cache {
                return files.clone();
            }
        }
        let mut result = Vec::new();
        self.walk_dir(&self.root, &mut result);
        let arc: std::sync::Arc<[FileInfo]> = result.into();
        if let Ok(mut cache) = self.markdown_files_cache.lock() {
            *cache = Some(std::sync::Arc::clone(&arc));
        }
        arc
    }

    fn walk_dir(&self, dir: &Path, out: &mut Vec<FileInfo>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                // Skip hidden folders and common massive build/dependency directories
                if !name.starts_with('.')
                    && name != "target"
                    && name != "node_modules"
                    && name != "dist"
                    && name != "build"
                    && name != "vendor"
                {
                    self.walk_dir(&path, out);
                }
            } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                if let Some(info) = FileInfo::from_path(&self.root, &path) {
                    out.push(info);
                }
            }
        }
    }

    /// Path to the plugin settings file: <vault>/.vault/plugins/<id>/data.json
    pub fn plugin_data_path(&self, plugin_id: &str) -> PathBuf {
        self.root.join(".vault").join("plugins").join(plugin_id).join("data.json")
    }

    /// Reads plugin data from data.json
    pub fn load_plugin_data(&self, plugin_id: &str) -> Option<serde_json::Value> {
        let path = self.plugin_data_path(plugin_id);
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Saves plugin settings to data.json (atomically via creation of the parent folder)
    pub fn save_plugin_data(&self, plugin_id: &str, data: &serde_json::Value) -> Result<(), String> {
        let path = self.plugin_data_path(plugin_id);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    /// Path to the list of enabled third-party plugins: <vault>/.vault/community-plugins.json
    pub fn community_plugins_path(&self) -> PathBuf {
        self.root.join(".vault").join("community-plugins.json")
    }

    /// Reads the list of enabled third-party plugin IDs.
    pub fn load_community_plugins(&self) -> Vec<String> {
        let path = self.community_plugins_path();
        let Ok(content) = std::fs::read_to_string(path) else {
            return Vec::new();
        };
        serde_json::from_str(&content).unwrap_or_default()
    }

    /// Saves the list of enabled third-party plugin IDs.
    pub fn save_community_plugins(&self, plugin_ids: &[String]) -> Result<(), String> {
        let path = self.community_plugins_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(plugin_ids).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    /// Enables a plugin in community-plugins.json.
    pub fn enable_community_plugin(&self, plugin_id: &str) -> Result<(), String> {
        let mut list = self.load_community_plugins();
        if !list.iter().any(|id| id == plugin_id) {
            list.push(plugin_id.to_string());
            self.save_community_plugins(&list)?;
        }
        Ok(())
    }

    /// Disables a plugin in community-plugins.json.
    pub fn disable_community_plugin(&self, plugin_id: &str) -> Result<(), String> {
        let mut list = self.load_community_plugins();
        list.retain(|id| id != plugin_id);
        self.save_community_plugins(&list)
    }

    /// Checks whether a plugin is enabled in community-plugins.json.
    pub fn is_community_plugin_enabled(&self, plugin_id: &str) -> bool {
        self.load_community_plugins().iter().any(|id| id == plugin_id)
    }

    /// Path to active core plugins list: <vault>/.vault/core-plugins.json
    pub fn core_plugins_path(&self) -> PathBuf {
        self.root.join(".vault").join("core-plugins.json")
    }

    /// Reads the list of active core plugin IDs.
    /// Defaults to default core plugins (command-palette, word-counter) if file does not exist.
    pub fn load_core_plugins(&self) -> Vec<String> {
        let path = self.core_plugins_path();
        let default_list = vec![
            "command-palette".to_string(),
            "word-counter".to_string(),
            "editor-toolbar".to_string(),
        ];
        let Ok(content) = std::fs::read_to_string(path) else {
            return default_list;
        };
        serde_json::from_str(&content).unwrap_or(default_list)
    }

    /// Saves the list of active core plugin IDs.
    pub fn save_core_plugins(&self, plugin_ids: &[String]) -> Result<(), String> {
        let path = self.core_plugins_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(plugin_ids).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    /// Enables a core plugin in core-plugins.json.
    pub fn enable_core_plugin(&self, plugin_id: &str) -> Result<(), String> {
        let mut list = self.load_core_plugins();
        if !list.iter().any(|id| id == plugin_id) {
            list.push(plugin_id.to_string());
            self.save_core_plugins(&list)?;
        }
        Ok(())
    }

    /// Disables a core plugin in core-plugins.json.
    pub fn disable_core_plugin(&self, plugin_id: &str) -> Result<(), String> {
        let mut list = self.load_core_plugins();
        list.retain(|id| id != plugin_id);
        self.save_core_plugins(&list)
    }

    /// Checks if a core plugin is active.
    pub fn is_core_plugin_enabled(&self, plugin_id: &str) -> bool {
        self.load_core_plugins().iter().any(|id| id == plugin_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempVault {
        dir: PathBuf,
    }

    impl TempVault {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("aquilum_vault_test_{}_{}", name, uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            Self { dir }
        }
    }

    impl Drop for TempVault {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn test_file_info_conversion() {
        let root = Path::new("D:/notes");
        let file = Path::new("D:/notes/folder/My Note.md");
        let info = FileInfo::from_path(root, file).unwrap();

        assert_eq!(info.path, "folder/My Note.md");
        assert_eq!(info.name, "My Note.md");
        assert_eq!(info.basename, "My Note");
        assert_eq!(info.extension, "md");
    }

    #[test]
    fn test_vault_create_read_modify_delete() {
        let temp = TempVault::new("crud");
        let vault = VaultCore::open(temp.dir.clone());

        // Create
        let path = vault.create_file("test.md", "hello world").unwrap();
        assert!(path.exists());

        // Duplicate create error
        assert!(matches!(vault.create_file("test.md", "duplicate"), Err(VaultError::AlreadyExists(_))));

        // Read
        let content = vault.read_file("test.md").unwrap();
        assert_eq!(content, "hello world");

        // Modify
        vault.modify_file("test.md", "updated content").unwrap();
        assert_eq!(vault.read_file("test.md").unwrap(), "updated content");

        // Rename
        let new_path = vault.rename_file("test.md", "renamed.md").unwrap();
        assert!(new_path.exists());
        assert!(!path.exists());
        assert_eq!(vault.read_file("renamed.md").unwrap(), "updated content");

        // Delete
        vault.delete_file("renamed.md").unwrap();
        assert!(matches!(vault.read_file("renamed.md"), Err(VaultError::NotFound(_))));
    }

    #[test]
    fn test_resolve_security_traversal() {
        let temp = TempVault::new("security");
        let vault = VaultCore::open(temp.dir.clone());

        // Null bytes blocked
        assert!(matches!(vault.resolve("sub/evil\0.md"), Err(VaultError::InvalidPath(_))));

        // Absolute path blocked
        #[cfg(windows)]
        assert!(matches!(vault.resolve("C:\\Windows\\evil.txt"), Err(VaultError::InvalidPath(_))));
        assert!(matches!(vault.resolve("/etc/passwd"), Err(VaultError::InvalidPath(_))));

        // Parent traversal .. blocked
        assert!(matches!(vault.resolve("../outside.txt"), Err(VaultError::InvalidPath(_))));
        assert!(matches!(vault.resolve("folder/../../outside.txt"), Err(VaultError::InvalidPath(_))));

        // Access to reserved .vault blocked
        assert!(matches!(vault.resolve(".vault/secret.json"), Err(VaultError::InvalidPath(_))));
        assert!(matches!(vault.resolve("sub/../.vault/data.json"), Err(VaultError::InvalidPath(_))));

        // Valid nested path allowed
        let valid = vault.resolve("deep/nested/note.md").unwrap();
        assert!(valid.starts_with(temp.dir.canonicalize().unwrap()));
    }

    #[test]
    fn test_plugin_data_persistence() {
        let temp = TempVault::new("plugindata");
        let vault = VaultCore::open(temp.dir.clone());

        let data = serde_json::json!({
            "setting1": "value1",
            "count": 42
        });

        vault.save_plugin_data("word-counter", &data).unwrap();
        let loaded = vault.load_plugin_data("word-counter").unwrap();
        assert_eq!(loaded["count"], 42);
        assert_eq!(loaded["setting1"], "value1");

        // Non-existent plugin returns None
        assert!(vault.load_plugin_data("other-plugin").is_none());
    }

    #[test]
    fn test_community_plugin_list_persistence() {
        let temp = TempVault::new("commplugins");
        let vault = VaultCore::open(temp.dir.clone());

        assert_eq!(vault.load_community_plugins().len(), 0);
        assert!(!vault.is_community_plugin_enabled("dataview"));

        vault.enable_community_plugin("dataview").unwrap();
        assert!(vault.is_community_plugin_enabled("dataview"));
        assert_eq!(vault.load_community_plugins(), vec!["dataview"]);

        vault.enable_community_plugin("calendar").unwrap();
        assert_eq!(vault.load_community_plugins(), vec!["dataview", "calendar"]);

        vault.disable_community_plugin("dataview").unwrap();
        assert!(!vault.is_community_plugin_enabled("dataview"));
        assert_eq!(vault.load_community_plugins(), vec!["calendar"]);
    }

    #[test]
    fn test_core_plugin_list_persistence() {
        let temp = TempVault::new("coreplugins");
        let vault = VaultCore::open(temp.dir.clone());

        // Default list when file does not exist
        let defaults = vault.load_core_plugins();
        assert!(defaults.contains(&"command-palette".to_string()));
        assert!(defaults.contains(&"word-counter".to_string()));
        assert!(vault.is_core_plugin_enabled("command-palette"));
        assert!(!vault.is_core_plugin_enabled("file-recovery"));

        vault.enable_core_plugin("file-recovery").unwrap();
        assert!(vault.is_core_plugin_enabled("file-recovery"));

        vault.disable_core_plugin("word-counter").unwrap();
        assert!(!vault.is_core_plugin_enabled("word-counter"));
        assert!(vault.is_core_plugin_enabled("command-palette"));
    }
}
