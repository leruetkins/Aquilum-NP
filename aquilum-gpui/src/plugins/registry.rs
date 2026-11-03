//! Obsidian Community Plugins Registry Client.
//!
//! Handles background fetching, disk caching of community-plugins.json and stats,
//! and downloading real plugin files (manifest.json, main.js, styles.css, README.md)
//! directly from GitHub Releases and raw repositories.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const REGISTRY_PLUGINS_URL: &str =
    "https://raw.githubusercontent.com/obsidianmd/obsidian-releases/master/community-plugins.json";
pub const REGISTRY_STATS_URL: &str =
    "https://raw.githubusercontent.com/obsidianmd/obsidian-releases/master/community-plugin-stats.json";
pub const USER_AGENT: &str = "Aquilum-Vibecodium/1.0 (Windows NT 10.0; Win64; x64)";

/// Catalog entry for a community plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogPluginEntry {
    /// Unique plugin identifier (e.g. "dataview", "calendar").
    pub id: String,
    /// Human-friendly display name.
    pub name: String,
    /// Author name or GitHub handle.
    pub author: String,
    /// Brief description of plugin capabilities.
    pub description: String,
    /// GitHub repository relative path (e.g. "blacksmithgu/obsidian-dataview").
    pub repo: String,
    /// Branch override (if specified in registry).
    #[serde(default)]
    pub branch: Option<String>,
    /// Latest release version tag (defaults to "1.0.0" until manifest is fetched).
    #[serde(default = "default_version")]
    pub version: String,
    /// Download count for popularity ranking.
    #[serde(default)]
    pub downloads: u64,
    /// Unix timestamp in milliseconds of the latest release update from community stats.
    #[serde(default)]
    pub updated: Option<u64>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

/// Statistics payload for community plugins.
#[derive(Debug, Deserialize)]
struct PluginStatsPayload {
    #[serde(default)]
    downloads: Option<u64>,
    #[serde(default)]
    updated: Option<u64>,
}

/// Returns the cache file path for the community plugins catalog on disk.
pub fn get_catalog_cache_path() -> PathBuf {
    crate::state::app_data_dir()
        .join("cache")
        .join("community-plugins.json")
}

/// Loads cached community plugins from disk if available.
pub fn load_cached_community_plugins() -> Option<Vec<CatalogPluginEntry>> {
    let cache_file = get_catalog_cache_path();
    if cache_file.is_file() {
        if let Ok(content) = std::fs::read_to_string(&cache_file) {
            if let Ok(entries) = serde_json::from_str::<Vec<CatalogPluginEntry>>(&content) {
                if !entries.is_empty() {
                    return Some(entries);
                }
            }
        }
    }
    None
}

/// Saves the community plugins list to disk cache.
pub fn save_cached_community_plugins(entries: &[CatalogPluginEntry]) -> Result<(), String> {
    let cache_file = get_catalog_cache_path();
    if let Some(parent) = cache_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = serde_json::to_string(entries).map_err(|e| e.to_string())?;
    std::fs::write(&cache_file, json).map_err(|e| e.to_string())?;
    Ok(())
}

/// Creates a standard ureq agent with appropriate timeouts and redirect policies.
fn create_http_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(15))
        .redirects(5)
        .build()
}

/// Fetches the live community plugins registry from GitHub along with popularity stats.
pub fn fetch_remote_community_plugins() -> Result<Vec<CatalogPluginEntry>, String> {
    let agent = create_http_agent();

    // 1. Fetch community-plugins.json
    let resp = agent
        .get(REGISTRY_PLUGINS_URL)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/json")
        .call()
        .map_err(|e| format!("Failed to fetch plugins list: {}", e))?;

    let plugins_text = resp
        .into_string()
        .map_err(|e| format!("Failed to read plugins response body: {}", e))?;

    let mut entries: Vec<CatalogPluginEntry> = serde_json::from_str(&plugins_text)
        .map_err(|e| format!("Failed to parse community-plugins.json: {}", e))?;

    // 2. Fetch stats (optional: best-effort, failures ignored)
    if let Ok(stats_resp) = agent
        .get(REGISTRY_STATS_URL)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/json")
        .call()
    {
        if let Ok(stats_text) = stats_resp.into_string() {
            if let Ok(stats_map) = serde_json::from_str::<HashMap<String, PluginStatsPayload>>(&stats_text) {
                for entry in &mut entries {
                    if let Some(stat) = stats_map.get(&entry.id) {
                        if let Some(dl) = stat.downloads {
                            entry.downloads = dl;
                        }
                        if let Some(up) = stat.updated {
                            entry.updated = Some(up);
                        }
                    }
                }
            }
        }
    }

    // 3. Cache to disk
    let _ = save_cached_community_plugins(&entries);

    Ok(entries)
}

/// Helper to download text from primary URL or fallback URL.
fn download_file_with_fallback(
    agent: &ureq::Agent,
    urls: &[String],
) -> Result<String, String> {
    for url in urls {
        match agent
            .get(url)
            .set("User-Agent", USER_AGENT)
            .set("Accept", "*/*")
            .call()
        {
            Ok(resp) => {
                if let Ok(body) = resp.into_string() {
                    if !body.trim().is_empty() {
                        return Ok(body);
                    }
                }
            }
            Err(_) => continue,
        }
    }
    Err("File could not be found at any specified URL".to_string())
}

/// Downloads all plugin release files (manifest.json, main.js, styles.css, README.md)
/// directly from GitHub Releases or raw repository content into `target_dir`.
pub fn download_plugin_release(
    repo: &str,
    target_dir: &Path,
    on_progress: impl Fn(&str),
) -> Result<(), String> {
    let agent = create_http_agent();
    let clean_repo = repo.trim().trim_matches('/');

    std::fs::create_dir_all(target_dir)
        .map_err(|e| format!("Failed to create plugin directory: {}", e))?;

    // 1. Download manifest.json
    on_progress("Скачивание manifest.json...");
    let manifest_urls = vec![
        format!("https://github.com/{}/releases/latest/download/manifest.json", clean_repo),
        format!("https://raw.githubusercontent.com/{}/master/manifest.json", clean_repo),
        format!("https://raw.githubusercontent.com/{}/main/manifest.json", clean_repo),
    ];
    let manifest_content = download_file_with_fallback(&agent, &manifest_urls)
        .map_err(|e| format!("Could not download manifest.json for {}: {}", clean_repo, e))?;

    // Validate that manifest is valid JSON
    let _parsed_val: serde_json::Value = serde_json::from_str(&manifest_content)
        .map_err(|e| format!("Invalid manifest.json received: {}", e))?;

    std::fs::write(target_dir.join("manifest.json"), manifest_content)
        .map_err(|e| format!("Failed to write manifest.json: {}", e))?;

    // 2. Download main.js
    on_progress("Скачивание main.js...");
    let main_urls = vec![
        format!("https://github.com/{}/releases/latest/download/main.js", clean_repo),
        format!("https://raw.githubusercontent.com/{}/master/main.js", clean_repo),
        format!("https://raw.githubusercontent.com/{}/main/main.js", clean_repo),
    ];
    let main_content = download_file_with_fallback(&agent, &main_urls)
        .map_err(|e| format!("Could not download main.js for {}: {}", clean_repo, e))?;

    std::fs::write(target_dir.join("main.js"), main_content)
        .map_err(|e| format!("Failed to write main.js: {}", e))?;

    // 3. Download styles.css (optional)
    on_progress("Скачивание styles.css...");
    let css_urls = vec![
        format!("https://github.com/{}/releases/latest/download/styles.css", clean_repo),
        format!("https://raw.githubusercontent.com/{}/master/styles.css", clean_repo),
        format!("https://raw.githubusercontent.com/{}/main/styles.css", clean_repo),
    ];
    if let Ok(css_content) = download_file_with_fallback(&agent, &css_urls) {
        let _ = std::fs::write(target_dir.join("styles.css"), css_content);
    }

    // 4. Download README.md (documentation)
    on_progress("Скачивание README.md...");
    let readme_urls = vec![
        format!("https://raw.githubusercontent.com/{}/master/README.md", clean_repo),
        format!("https://raw.githubusercontent.com/{}/main/README.md", clean_repo),
        format!("https://raw.githubusercontent.com/{}/master/readme.md", clean_repo),
        format!("https://raw.githubusercontent.com/{}/main/readme.md", clean_repo),
        format!("https://github.com/{}/releases/latest/download/README.md", clean_repo),
    ];
    if let Ok(readme_content) = download_file_with_fallback(&agent, &readme_urls) {
        let _ = std::fs::write(target_dir.join("README.md"), readme_content);
    }

    Ok(())
}

/// Fetches the README.md content directly from GitHub for any repository (raw or release).
pub fn fetch_readme_for_plugin(repo: &str) -> Result<String, String> {
    let agent = create_http_agent();
    let clean_repo = repo.trim().trim_matches('/');
    let readme_urls = vec![
        format!("https://raw.githubusercontent.com/{}/master/README.md", clean_repo),
        format!("https://raw.githubusercontent.com/{}/main/README.md", clean_repo),
        format!("https://raw.githubusercontent.com/{}/master/readme.md", clean_repo),
        format!("https://raw.githubusercontent.com/{}/main/readme.md", clean_repo),
        format!("https://github.com/{}/releases/latest/download/README.md", clean_repo),
    ];
    download_file_with_fallback(&agent, &readme_urls)
}

#[cfg(test)]
mod tests;
