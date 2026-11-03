use std::collections::BTreeMap;
use std::sync::OnceLock;
use serde::{Deserialize, Serialize};

/// High-level compatibility level of an Obsidian plugin with the Aquilum GPUI host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CompatibilityStatus {
    /// 100% of requested APIs are natively supported in Aquilum.
    Full,
    /// 60%–99% of APIs are supported; minor fallback stubs or easily extensible methods.
    Partial(f32),
    /// Critical incompatibilities detected (raw DOM, Electron, child_process, private Obsidian internals).
    Incompatible,
    /// The scan recognized no known API calls at all: the plugin may use newer
    /// API names the pattern list does not cover, so its compatibility is unknown.
    Unknown,
}

/// Detailed compatibility report produced by the static analyzer or runtime audit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompatibilityReport {
    pub status: CompatibilityStatus,
    /// Calculated compatibility score between 0.0 (incompatible) and 1.0 (perfect).
    pub score: f32,
    /// Obsidian APIs identified in the bundle that are fully implemented.
    pub supported_calls: Vec<String>,
    /// APIs called that are safe stubs or not yet implemented.
    pub missing_calls: Vec<String>,
    /// Incompatible API calls or forbidden module imports.
    pub incompatible_calls: Vec<String>,
    /// Warnings and notes for the user or plugin author.
    pub warnings: Vec<String>,
}

impl CompatibilityReport {
    /// Returns the badge display text (e.g., "100% Совместим", "75% Совместим", "Не поддерживается").
    pub fn badge_label(&self) -> String {
        match self.status {
            CompatibilityStatus::Full => "100% Совместим".to_string(),
            CompatibilityStatus::Partial(pct) => format!("{:.0}% Совместим", pct * 100.0),
            CompatibilityStatus::Incompatible => "Не поддерживается".to_string(),
            CompatibilityStatus::Unknown => "Совместимость неизвестна".to_string(),
        }
    }
}

/// Patterns and classifiers for static bundle analysis.
struct ApiPattern {
    pattern: &'static str,
    name: &'static str,
    category: PatternCategory,
}

enum PatternCategory {
    Supported,
    MissingOrStub,
    Incompatible,
}

const API_PATTERNS: &[ApiPattern] = &[
    // Supported core APIs
    ApiPattern { pattern: "Notice", name: "new Notice()", category: PatternCategory::Supported },
    ApiPattern { pattern: "addCommand", name: "plugin.addCommand()", category: PatternCategory::Supported },
    ApiPattern { pattern: "addRibbonIcon", name: "plugin.addRibbonIcon()", category: PatternCategory::Supported },
    ApiPattern { pattern: "addSettingTab", name: "plugin.addSettingTab()", category: PatternCategory::Supported },
    ApiPattern { pattern: "Setting", name: "new Setting()", category: PatternCategory::Supported },
    ApiPattern { pattern: "PluginSettingTab", name: "PluginSettingTab", category: PatternCategory::Supported },
    ApiPattern { pattern: "registerMarkdownCodeBlockProcessor", name: "registerMarkdownCodeBlockProcessor()", category: PatternCategory::Supported },
    ApiPattern { pattern: "loadData", name: "plugin.loadData()", category: PatternCategory::Supported },
    ApiPattern { pattern: "saveData", name: "plugin.saveData()", category: PatternCategory::Supported },
    ApiPattern { pattern: "vault.read", name: "app.vault.read()", category: PatternCategory::Supported },
    ApiPattern { pattern: "vault.modify", name: "app.vault.modify()", category: PatternCategory::Supported },
    ApiPattern { pattern: "vault.create", name: "app.vault.create()", category: PatternCategory::Supported },
    ApiPattern { pattern: "vault.delete", name: "app.vault.delete()", category: PatternCategory::Supported },
    ApiPattern { pattern: "vault.process", name: "app.vault.process()", category: PatternCategory::Supported },
    ApiPattern { pattern: "vault.getMarkdownFiles", name: "app.vault.getMarkdownFiles()", category: PatternCategory::Supported },
    ApiPattern { pattern: "vault.getFiles", name: "app.vault.getFiles()", category: PatternCategory::Supported },
    ApiPattern { pattern: "workspace.getActiveFile", name: "app.workspace.getActiveFile()", category: PatternCategory::Supported },
    ApiPattern { pattern: "workspace.openFile", name: "app.workspace.openFile()", category: PatternCategory::Supported },
    ApiPattern { pattern: "normalizePath", name: "normalizePath()", category: PatternCategory::Supported },
    ApiPattern { pattern: "requestUrl", name: "requestUrl()", category: PatternCategory::Supported },
    ApiPattern { pattern: "Menu", name: "new Menu()", category: PatternCategory::Supported },
    ApiPattern { pattern: "editor-menu", name: "workspace.on('editor-menu')", category: PatternCategory::Supported },
    ApiPattern { pattern: "setSubmenu", name: "MenuItem.setSubmenu()", category: PatternCategory::Supported },
    ApiPattern { pattern: "metadataCache.getFileCache", name: "app.metadataCache.getFileCache()", category: PatternCategory::Supported },
    ApiPattern { pattern: "ItemView", name: "ItemView", category: PatternCategory::Supported },
    ApiPattern { pattern: "WorkspaceLeaf", name: "WorkspaceLeaf", category: PatternCategory::Supported },

    // Missing or safe stubs
    ApiPattern { pattern: "requestSaveHistory", name: "app.workspace.requestSaveHistory()", category: PatternCategory::MissingOrStub },
    ApiPattern { pattern: "vault.adapter", name: "app.vault.adapter", category: PatternCategory::MissingOrStub },
    ApiPattern { pattern: "vault.copy", name: "app.vault.copy()", category: PatternCategory::MissingOrStub },
    ApiPattern { pattern: "splitActiveLeaf", name: "app.workspace.splitActiveLeaf()", category: PatternCategory::MissingOrStub },

    // Incompatible calls requiring Electron / raw DOM / Node processes
    ApiPattern { pattern: "require(\"electron\")", name: "require('electron')", category: PatternCategory::Incompatible },
    ApiPattern { pattern: "require('electron')", name: "require('electron')", category: PatternCategory::Incompatible },
    ApiPattern { pattern: "require(\"child_process\")", name: "require('child_process')", category: PatternCategory::Incompatible },
    ApiPattern { pattern: "require('child_process')", name: "require('child_process')", category: PatternCategory::Incompatible },
    ApiPattern { pattern: "ipcRenderer", name: "Electron ipcRenderer", category: PatternCategory::Incompatible },
    ApiPattern { pattern: "document.createElement", name: "document.createElement()", category: PatternCategory::Incompatible },
    ApiPattern { pattern: "document.getElementById", name: "document.getElementById()", category: PatternCategory::Incompatible },
    ApiPattern { pattern: "innerHTML", name: "element.innerHTML", category: PatternCategory::Incompatible },
];

/// Statically scans a plugin's `main.js` script to determine API compatibility.
pub fn analyze_plugin_script(script_source: &str) -> CompatibilityReport {
    let mut supported = Vec::new();
    let mut missing = Vec::new();
    let mut incompatible = Vec::new();
    let mut warnings = Vec::new();

    for pat in API_PATTERNS {
        if script_source.contains(pat.pattern) {
            match pat.category {
                PatternCategory::Supported => {
                    if !supported.contains(&pat.name.to_string()) {
                        supported.push(pat.name.to_string());
                    }
                }
                PatternCategory::MissingOrStub => {
                    if !missing.contains(&pat.name.to_string()) {
                        missing.push(pat.name.to_string());
                    }
                }
                PatternCategory::Incompatible => {
                    if !incompatible.contains(&pat.name.to_string()) {
                        incompatible.push(pat.name.to_string());
                    }
                }
            }
        }
    }

    if !incompatible.is_empty() {
        warnings.push(format!(
            "Обнаружены несовместимые интерфейсы ({}), требующие веб-браузер или среду Electron.",
            incompatible.join(", ")
        ));
        let score = if supported.is_empty() { 0.0 } else { 0.25 };
        CompatibilityReport {
            status: CompatibilityStatus::Incompatible,
            score,
            supported_calls: supported,
            missing_calls: missing,
            incompatible_calls: incompatible,
            warnings,
        }
    } else if missing.is_empty() {
        let status = if supported.is_empty() {
            // Nothing recognized at all: the bundle may be built against a newer
            // API or heavily minified, so "100% compatible" would be a lie.
            CompatibilityStatus::Unknown
        } else {
            CompatibilityStatus::Full
        };
        CompatibilityReport {
            status,
            score: if supported.is_empty() { 0.0 } else { 1.0 },
            supported_calls: supported,
            missing_calls: missing,
            incompatible_calls: incompatible,
            warnings,
        }
    } else {
        let total_features = (supported.len() + missing.len()) as f32;
        let score = if total_features > 0.0 {
            (supported.len() as f32 / total_features).clamp(0.1, 0.99)
        } else {
            1.0
        };

        warnings.push(format!(
            "Используются заглушки или методы в разработке: {}",
            missing.join(", ")
        ));

        CompatibilityReport {
            status: CompatibilityStatus::Partial(score),
            score,
            supported_calls: supported,
            missing_calls: missing,
            incompatible_calls: incompatible,
            warnings,
        }
    }
}

// ──────────────────────────────────────────────────────── реестр корпуса

/// Запись реестра совместимости: результат статического скана doctor'а по реальному
/// корпусу плагинов Obsidian. `catalog_id` — id плагина из community-plugins.json.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogCompatRecord {
    pub status: CompatibilityStatus,
    /// Доля распознанных поддержанных вызовов от всех распознанных (0..1).
    pub coverage: f32,
    /// Экспорты obsidian, которых нет у хоста (API новее 1.7.5 и т.п.).
    pub missing_exports: Vec<String>,
    /// Члены app.*, которых нет у хоста.
    pub missing_app: Vec<String>,
    /// Движковые блокеры (canvas / codemirror / node / ...).
    pub blockers: Vec<String>,
}

/// Компактные формы JSON-реестра.
#[derive(Deserialize)]
struct RegistryRecord {
    s: String,
    #[serde(default)]
    c: f32,
    #[serde(default)]
    e: Vec<String>,
    #[serde(default)]
    a: Vec<String>,
    #[serde(default)]
    b: Vec<String>,
}

#[derive(Deserialize)]
struct RegistryFile {
    #[serde(default)]
    generated: String,
    #[serde(default)]
    counts: BTreeMap<String, u32>,
    plugins: BTreeMap<String, RegistryRecord>,
}

/// Сгенерированный реестр: `node make-compat-registry.cjs <doctor-report> <out>`.
/// Инструмент живёт в REA `.workbuddy-ai/tools/plugin-probe/`; обновлять файл стоит
/// после заметных изменений поверхности хоста или нового скана корпуса.
static COMPAT_REGISTRY_JSON: &str = include_str!("compat_registry.json");

/// Разобранный реестр, один раз на процесс: каталогу нужен lookup на каждую строку
/// списка, а JSON на ~80КБ не должен парситься в цикле рендера.
static COMPAT_REGISTRY: OnceLock<RegistryFile> = OnceLock::new();

fn compat_registry() -> &'static RegistryFile {
    COMPAT_REGISTRY.get_or_init(|| {
        serde_json::from_str(COMPAT_REGISTRY_JSON).expect("compat_registry.json must deserialize")
    })
}

/// Возвращает запись совместимости по id плагина из каталога, если он был в скане корпуса.
pub fn catalog_compat(plugin_id: &str) -> Option<CatalogCompatRecord> {
    let file = compat_registry();
    let record = file.plugins.get(plugin_id)?;
    let status = match record.s.as_str() {
        "ok" => CompatibilityStatus::Full,
        "partial" => CompatibilityStatus::Partial(record.c.clamp(0.0, 1.0)),
        "blocked" => CompatibilityStatus::Incompatible,
        _ => CompatibilityStatus::Unknown,
    };
    Some(CatalogCompatRecord {
        status,
        coverage: record.c,
        missing_exports: record.e.clone(),
        missing_app: record.a.clone(),
        blockers: record.b.clone(),
    })
}

/// Число плагинов, покрытых реестром (для подписи в каталоге).
pub fn catalog_compat_stats() -> (u32, u32, u32, u32) {
    let file = compat_registry();
    let total = file.plugins.len() as u32;
    (
        total,
        *file.counts.get("ok").unwrap_or(&0),
        *file.counts.get("partial").unwrap_or(&0),
        *file.counts.get("blocked").unwrap_or(&0),
    )
}

#[cfg(test)]
mod tests;
