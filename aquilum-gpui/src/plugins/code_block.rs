use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Thread-safe registry for custom markdown code block processors registered by plugins.
///
/// In Obsidian, plugins register processors via:
/// `this.registerMarkdownCodeBlockProcessor("dataview", (source, el, ctx) => { ... })`
#[derive(Debug, Clone, Default)]
pub struct CodeBlockProcessorRegistry {
    /// Mapping: lowercase language tag -> plugin identifier (e.g. "dataview" -> "obsidian-dataview")
    processors: Arc<RwLock<HashMap<String, String>>>,
}

impl CodeBlockProcessorRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a custom code block processor for the specified language tag.
    pub fn register(&self, language: &str, plugin_id: &str) {
        let mut map = self.processors.write().unwrap();
        map.insert(language.to_lowercase().trim().to_string(), plugin_id.to_string());
    }

    /// Checks if a processor is registered for a given language tag and returns the owning plugin ID.
    pub fn get_plugin_for_lang(&self, language: &str) -> Option<String> {
        let key = language.to_lowercase();
        let map = self.processors.read().unwrap();
        map.get(key.trim()).cloned()
    }

    /// Checks if any processor is registered for the specified language tag.
    pub fn has_processor(&self, language: &str) -> bool {
        let key = language.to_lowercase();
        let map = self.processors.read().unwrap();
        map.contains_key(key.trim())
    }

    /// Unregisters all processors belonging to a given plugin (called automatically upon plugin unload).
    pub fn unregister_all_of(&self, plugin_id: &str) {
        let mut map = self.processors.write().unwrap();
        map.retain(|_, p| p != plugin_id);
    }

    /// Returns a list of all currently registered language tags.
    pub fn all_languages(&self) -> Vec<String> {
        let map = self.processors.read().unwrap();
        map.keys().cloned().collect()
    }

    /// Clears all registered processors.
    pub fn clear(&self) {
        let mut map = self.processors.write().unwrap();
        map.clear();
    }
}

#[cfg(test)]
mod tests;
