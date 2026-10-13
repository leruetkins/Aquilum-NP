use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub type ActionCallback = Arc<dyn Fn() + Send + Sync>;

/// Represents an interactive icon in the left Obsidian-style ribbon rail (44px wide).
#[derive(Clone)]
pub struct RibbonAction {
    pub id: String,
    pub icon: &'static str,
    pub tooltip: String,
    /// None for built-in actions, Some(plugin_id) for plugin-contributed actions.
    pub owner: Option<String>,
    pub callback: ActionCallback,
}

/// Represents an item displayed in the bottom application status bar (28px height).
#[derive(Clone)]
pub struct StatusBarItem {
    pub id: String,
    pub text: String,
    /// None for built-in items, Some(plugin_id) for plugin-contributed items.
    pub owner: Option<String>,
    pub on_click: Option<ActionCallback>,
}

/// Thread-safe registry for managing ribbon actions.
#[derive(Default)]
pub struct RibbonRegistry {
    actions: Mutex<HashMap<String, RibbonAction>>,
}

impl RibbonRegistry {
    pub fn new() -> Self {
        Self {
            actions: Mutex::new(HashMap::new()),
        }
    }

    /// Register a ribbon icon. Overwrites if an action with the same id exists.
    pub fn register(&self, action: RibbonAction) -> String {
        let id = action.id.clone();
        self.actions.lock().unwrap().insert(id.clone(), action);
        id
    }

    /// Unregister a ribbon action by id.
    pub fn unregister(&self, id: &str) {
        self.actions.lock().unwrap().remove(id);
    }

    /// Remove all actions contributed by a specific plugin upon unload.
    pub fn unregister_all_of(&self, plugin_id: &str) {
        self.actions
            .lock()
            .unwrap()
            .retain(|_, action| action.owner.as_deref() != Some(plugin_id));
    }

    /// Returns a specific ribbon action by id if present.
    pub fn get(&self, id: &str) -> Option<RibbonAction> {
        self.actions.lock().unwrap().get(id).cloned()
    }

    /// Returns all registered ribbon actions.
    pub fn all(&self) -> Vec<RibbonAction> {
        let acts = self.actions.lock().unwrap();
        let mut list: Vec<RibbonAction> = acts.values().cloned().collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }
}

/// Thread-safe registry for managing bottom status bar items.
#[derive(Default)]
pub struct StatusBarRegistry {
    items: Mutex<HashMap<String, StatusBarItem>>,
}

impl StatusBarRegistry {
    pub fn new() -> Self {
        Self {
            items: Mutex::new(HashMap::new()),
        }
    }

    /// Register or update a status bar item.
    pub fn set_item(&self, item: StatusBarItem) -> String {
        let id = item.id.clone();
        self.items.lock().unwrap().insert(id.clone(), item);
        id
    }

    /// Update text of an existing status bar item.
    pub fn set_text(&self, id: &str, text: &str) {
        if let Some(item) = self.items.lock().unwrap().get_mut(id) {
            item.text = text.to_string();
        }
    }

    /// Remove a status bar item by id.
    pub fn remove(&self, id: &str) {
        self.items.lock().unwrap().remove(id);
    }

    /// Remove all status bar items contributed by a specific plugin upon unload.
    pub fn unregister_all_of(&self, plugin_id: &str) {
        self.items
            .lock()
            .unwrap()
            .retain(|_, item| item.owner.as_deref() != Some(plugin_id));
    }

    /// Returns all registered status bar items.
    pub fn all(&self) -> Vec<StatusBarItem> {
        let items = self.items.lock().unwrap();
        let mut list: Vec<StatusBarItem> = items.values().cloned().collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn test_ribbon_registry_lifecycle() {
        let registry = RibbonRegistry::new();
        let clicked = Arc::new(AtomicBool::new(false));

        let flag = clicked.clone();
        registry.register(RibbonAction {
            id: "my-plugin:sync".into(),
            icon: "cloud",
            tooltip: "Sync Vault".into(),
            owner: Some("my-plugin".into()),
            callback: Arc::new(move || {
                flag.store(true, Ordering::SeqCst);
            }),
        });

        assert_eq!(registry.all().len(), 1);
        let action = &registry.all()[0];
        assert_eq!(action.id, "my-plugin:sync");
        assert_eq!(action.tooltip, "Sync Vault");

        (action.callback)();
        assert!(clicked.load(Ordering::SeqCst));

        // Test unload cleanup
        registry.unregister_all_of("my-plugin");
        assert_eq!(registry.all().len(), 0);
    }

    #[test]
    fn test_status_bar_registry_lifecycle() {
        let registry = StatusBarRegistry::new();

        registry.set_item(StatusBarItem {
            id: "word-count".into(),
            text: "Words: 150".into(),
            owner: Some("word-counter".into()),
            on_click: None,
        });

        assert_eq!(registry.all().len(), 1);
        assert_eq!(registry.all()[0].text, "Words: 150");

        registry.set_text("word-count", "Words: 155");
        assert_eq!(registry.all()[0].text, "Words: 155");

        // Clean up
        registry.unregister_all_of("word-counter");
        assert_eq!(registry.all().len(), 0);
    }
}
