use semver::Version;
use serde::{Deserialize, Serialize};

/// Permissions requested by a plugin in its manifest.json.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PluginPermissions {
    /// Level of vault access permitted: "none", "read", "write".
    #[serde(default)]
    pub vault: VaultAccess,
    /// Whitelist of external HTTP domains the plugin can connect to.
    #[serde(default)]
    pub network: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VaultAccess {
    None,
    Read,
    Write,
}

impl Default for VaultAccess {
    fn default() -> Self {
        Self::Write
    }
}

/// Metadata structure matching Obsidian's manifest.json with added security fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Unique plugin identifier (alphanumeric, hyphens and underscores).
    pub id: String,
    /// Human-readable plugin name.
    pub name: String,
    /// Semver version of the plugin ("1.0.0").
    pub version: String,
    /// Minimum compatible version of Vibecodium / host API ("0.1.0").
    #[serde(rename = "minAppVersion", default)]
    pub min_app_version: Option<String>,
    /// Brief description of the plugin's purpose.
    #[serde(default)]
    pub description: String,
    /// Author name.
    #[serde(default)]
    pub author: String,
    /// Declared permission requirements.
    #[serde(default)]
    pub permissions: PluginPermissions,
}

impl PluginManifest {
    /// Validates the plugin manifest fields.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() {
            return Err("Plugin 'id' cannot be empty".into());
        }
        if self.id.len() > 64 {
            return Err(format!("Plugin 'id' is too long ({} > 64 chars)", self.id.len()));
        }

        // Strict id format: starts with alphanumeric, contains only alphanumeric, dashes, underscores
        let first_char = self.id.chars().next().unwrap();
        if !first_char.is_ascii_alphanumeric() {
            return Err(format!(
                "Invalid plugin id '{}': must start with an alphanumeric character",
                self.id
            ));
        }

        if !self
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(format!(
                "Invalid plugin id '{}': only alphanumeric characters, dashes and underscores are allowed",
                self.id
            ));
        }

        // Check against Windows reserved device names (CON, PRN, AUX, NUL, COM1-9, LPT1-9)
        let upper_id = self.id.to_ascii_uppercase();
        let stem = upper_id.split('.').next().unwrap_or(&upper_id);
        const RESERVED_NAMES: &[&str] = &[
            "CON", "PRN", "AUX", "NUL",
            "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
            "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ];
        if RESERVED_NAMES.contains(&stem) {
            return Err(format!(
                "Invalid plugin id '{}': matches reserved Windows device name",
                self.id
            ));
        }

        if self.name.trim().is_empty() {
            return Err("Plugin 'name' cannot be empty".into());
        }
        if Version::parse(&self.version).is_err() {
            return Err(format!("Invalid plugin version '{}': must be valid semver", self.version));
        }
        if let Some(ref min_v) = self.min_app_version {
            if Version::parse(min_v).is_err() {
                return Err(format!("Invalid minAppVersion '{}': must be valid semver", min_v));
            }
        }
        Ok(())
    }

    /// Checks if the plugin is compatible with the given host version.
    pub fn is_compatible_with_host(&self, host_version_str: &str) -> bool {
        let Ok(host_v) = Version::parse(host_version_str) else {
            return false;
        };
        if let Some(ref min_v_str) = self.min_app_version {
            if let Ok(min_v) = Version::parse(min_v_str) {
                return host_v >= min_v;
            }
        }
        true
    }
}

use sha2::{Digest, Sha256};

/// Calculates deterministic SHA-256 hex digest of a byte slice.
pub fn compute_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// A stored user security grant for a specific plugin bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginGrant {
    pub plugin_id: String,
    pub bundle_sha256: String,
    pub granted_permissions: PluginPermissions,
    pub granted_at_unix_ms: u64,
}

/// Persistent store for user-approved plugin permissions located in AppData.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginGrantsStore {
    /// Map key: "{plugin_id}:{bundle_sha256}"
    pub grants: std::collections::HashMap<String, PluginGrant>,
}

impl PluginGrantsStore {
    pub fn grant_key(plugin_id: &str, bundle_sha256: &str) -> String {
        format!("{}:{}", plugin_id, bundle_sha256)
    }

    pub fn is_trusted(&self, plugin_id: &str, bundle_sha256: &str) -> bool {
        let key = Self::grant_key(plugin_id, bundle_sha256);
        self.grants.contains_key(&key)
    }

    pub fn get_grant(&self, plugin_id: &str, bundle_sha256: &str) -> Option<&PluginGrant> {
        let key = Self::grant_key(plugin_id, bundle_sha256);
        self.grants.get(&key)
    }

    pub fn add_grant(
        &mut self,
        plugin_id: &str,
        bundle_sha256: &str,
        permissions: PluginPermissions,
    ) {
        let key = Self::grant_key(plugin_id, bundle_sha256);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        self.grants.insert(
            key,
            PluginGrant {
                plugin_id: plugin_id.to_string(),
                bundle_sha256: bundle_sha256.to_string(),
                granted_permissions: permissions,
                granted_at_unix_ms: now,
            },
        );
    }

    pub fn load_from_file(path: &std::path::Path) -> Self {
        if let Ok(data) = std::fs::read_to_string(path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save_to_file(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_validation() {
        let manifest = PluginManifest {
            id: "my-sample-plugin".into(),
            name: "My Sample Plugin".into(),
            version: "1.0.0".into(),
            min_app_version: Some("0.1.0".into()),
            description: "A great plugin".into(),
            author: "Author".into(),
            permissions: PluginPermissions {
                vault: VaultAccess::Write,
                network: vec!["api.github.com".into()],
            },
        };

        assert!(manifest.validate().is_ok());
        assert!(manifest.is_compatible_with_host("0.1.0"));
        assert!(manifest.is_compatible_with_host("1.0.0"));
        assert!(!manifest.is_compatible_with_host("0.0.9"));
    }

    #[test]
    fn test_manifest_invalid_id() {
        let mut manifest = PluginManifest {
            id: "invalid id with spaces!".into(),
            name: "Plugin".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        assert!(manifest.validate().is_err());
        manifest.id = "valid-id_123".into();
        assert!(manifest.validate().is_ok());

        // Reserved Windows names must fail
        manifest.id = "con".into();
        assert!(manifest.validate().is_err());
        manifest.id = "aux".into();
        assert!(manifest.validate().is_err());
        manifest.id = "com1".into();
        assert!(manifest.validate().is_err());

        // Leading dash must fail
        manifest.id = "-invalid-start".into();
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_plugin_grants_store_and_sha256() {
        let bundle_bytes = b"console.log('Hello Obsidian Plugin');";
        let hash = compute_sha256(bundle_bytes);
        assert_eq!(hash.len(), 64);

        let mut store = PluginGrantsStore::default();
        let plugin_id = "sample-plugin";

        assert!(!store.is_trusted(plugin_id, &hash));

        store.add_grant(
            plugin_id,
            &hash,
            PluginPermissions {
                vault: VaultAccess::Read,
                network: vec!["api.example.com".into()],
            },
        );

        assert!(store.is_trusted(plugin_id, &hash));
        assert!(!store.is_trusted(plugin_id, "other_hash"));
        assert!(!store.is_trusted("other_plugin", &hash));

        let grant = store.get_grant(plugin_id, &hash).unwrap();
        assert_eq!(grant.granted_permissions.vault, VaultAccess::Read);
        assert_eq!(grant.granted_permissions.network, vec!["api.example.com".to_string()]);
    }
}
