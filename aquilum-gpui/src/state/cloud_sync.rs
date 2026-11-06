use super::*;
use crate::settings::SyncSettings;

impl AppState {
    /// Dropbox settings for one workspace. The per-workspace map wins; the
    /// global default is only a fallback for a workspace that has never been
    /// configured.
    pub fn sync_settings_for(&self, root: &Path) -> SyncSettings {
        let config = self.settings.get_config();
        config
            .sync_vaults
            .get(&Self::sync_key(root))
            .copied()
            .unwrap_or(config.sync)
    }

    /// Persists settings for the current workspace and re-binds the service.
    pub fn set_sync_settings(&self, root: &Path, settings: SyncSettings) {
        let mut config = self.settings.get_config();
        config.sync_vaults.insert(Self::sync_key(root), settings);
        if let Err(error) = self.settings.update_config(config) {
            eprintln!("[sync] settings were not saved: {error}");
        }
        self.bind_sync();
    }

    /// Gives the sync service the current vault and its settings. Called on
    /// workspace open/switch and after a settings change.
    pub fn bind_sync(&self) {
        let vault = self
            .workspace_path
            .as_ref()
            .map(|root| (root.clone(), self.sync_settings_for(root)));
        self.sync.set_vault(vault);
    }

    fn sync_key(root: &Path) -> String {
        let key = Self::normalize_path(root).to_string_lossy().replace('\\', "/");
        #[cfg(windows)]
        let key = key.to_lowercase();
        key
    }
}
