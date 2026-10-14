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
        Self::Read
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
}
