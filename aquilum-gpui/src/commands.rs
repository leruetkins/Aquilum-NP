use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub type CommandCallback = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
pub struct Command {
    pub id: String,
    pub name: String,
    /// None for built-in commands, Some(plugin_id) for plugin commands.
    pub owner: Option<String>,
    pub callback: CommandCallback,
    /// Hotkey for display in the palette (e.g., "Ctrl+S").
    pub hotkey: Option<String>,
    /// Icon for display in the toolbar and command palette.
    pub icon: Option<String>,
}

impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Command")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("owner", &self.owner)
            .field("hotkey", &self.hotkey)
            .field("icon", &self.icon)
            .finish()
    }
}

/// Central registry of app and plugin commands.
pub struct CommandRegistry {
    commands: Mutex<HashMap<String, Command>>,
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: Mutex::new(HashMap::new()),
        }
    }

    /// Register a command. If a command with the same id existed, it is overwritten.
    pub fn register(&self, cmd: Command) -> String {
        let id = cmd.id.clone();
        self.commands.lock().unwrap().insert(id.clone(), cmd);
        id
    }

    /// Find a command by id.
    pub fn get(&self, id: &str) -> Option<Command> {
        let cmds = self.commands.lock().unwrap();
        cmds.get(id).cloned()
    }

    /// Unregister a command by identifier.
    pub fn unregister(&self, id: &str) {
        self.commands.lock().unwrap().remove(id);
    }

    /// Remove all commands registered by a particular plugin (when it is unloaded).
    pub fn unregister_all_of(&self, plugin_id: &str) {
        self.commands
            .lock()
            .unwrap()
            .retain(|_, cmd| cmd.owner.as_deref() != Some(plugin_id));
    }

    /// Get the list of all commands (for the command palette).
    pub fn all(&self) -> Vec<Command> {
        let cmds = self.commands.lock().unwrap();
        let mut list: Vec<Command> = cmds.values().cloned().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    /// Stable palette numbers for built-in commands, in id order. Plugin
    /// commands are not numbered: their set changes at runtime, so a number
    /// would not stay meaningful.
    pub fn numbers(&self) -> HashMap<String, usize> {
        let mut ids: Vec<String> = {
            let cmds = self.commands.lock().unwrap();
            cmds.values()
                .filter(|cmd| cmd.owner.is_none())
                .map(|cmd| cmd.id.clone())
                .collect()
        };
        ids.sort();
        ids.into_iter()
            .enumerate()
            .map(|(index, id)| (id, index + 1))
            .collect()
    }

    /// Filter commands by search query. Every query word must appear in the
    /// localized command name, the registered English name, the id, or the
    /// built-in command number, so the palette matches both the interface
    /// language and English and can be used by number.
    pub fn search(&self, query: &str, lang: &str) -> Vec<Command> {
        let q = query.trim().to_lowercase();
        let commands: Vec<Command> = {
            let cmds = self.commands.lock().unwrap();
            cmds.values().cloned().collect()
        };
        let numbers = self.numbers();
        let mut list: Vec<(Command, String, String)> = commands
            .into_iter()
            .map(|cmd| {
                let label = crate::command_i18n::display_name(&cmd.id, &cmd.name, lang);
                let number = numbers
                    .get(&cmd.id)
                    .map(|number| number.to_string())
                    .unwrap_or_default();
                (cmd, label, number)
            })
            .collect();
        if !q.is_empty() {
            let words: Vec<&str> = q.split_whitespace().collect();
            let numeric_query = q.chars().all(|c| c.is_ascii_digit());
            list.retain(|(cmd, label, number)| {
                let label = label.to_lowercase();
                let name = cmd.name.to_lowercase();
                let id = cmd.id.to_lowercase();
                let number_matches = |word: &str| {
                    numeric_query
                        && !number.is_empty()
                        && number.chars().all(|c| c.is_ascii_digit())
                        && number.starts_with(word)
                };
                words.iter().all(|w| {
                    label.contains(w) || name.contains(w) || id.contains(w) || number_matches(w)
                })
            });
        }
        let numeric_query = !q.is_empty() && q.chars().all(|c| c.is_ascii_digit());
        if numeric_query {
            // Number matches first, then the remaining name/id matches.
            list.sort_by(|(a, a_label, a_number), (b, b_label, b_number)| {
                a_number
                    .is_empty()
                    .cmp(&b_number.is_empty())
                    .then_with(|| {
                        let a_num = a_number.parse::<usize>().unwrap_or(usize::MAX);
                        let b_num = b_number.parse::<usize>().unwrap_or(usize::MAX);
                        a_num.cmp(&b_num)
                    })
                    .then_with(|| a_label.to_lowercase().cmp(&b_label.to_lowercase()))
                    .then_with(|| a.name.cmp(&b.name))
            });
        } else {
            list.sort_by(|(a, a_label, _), (b, b_label, _)| {
                a_label
                    .to_lowercase()
                    .cmp(&b_label.to_lowercase())
                    .then_with(|| a.name.cmp(&b.name))
            });
        }
        list.into_iter().map(|(cmd, _, _)| cmd).collect()
    }

    /// Execute a command by its identifier. Returns true if the command was found and invoked.
    pub fn execute(&self, id: &str) -> bool {
        let callback = {
            let cmds = self.commands.lock().unwrap();
            cmds.get(id).map(|c| c.callback.clone())
        };
        if let Some(cb) = callback {
            cb();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn test_command_registration_and_execution() {
        let registry = CommandRegistry::new();
        let executed = Arc::new(AtomicBool::new(false));

        let flag = executed.clone();
        registry.register(Command {
            id: "test:action".into(),
            name: "Test Action".into(),
            owner: Some("test-plugin".into()),
            callback: Arc::new(move || {
                flag.store(true, Ordering::SeqCst);
            }),
            hotkey: Some("Ctrl+Shift+T".into()),
            icon: None,
        });

        assert_eq!(registry.all().len(), 1);
        assert_eq!(registry.search("action", "en").len(), 1);
        assert_eq!(registry.search("missing", "en").len(), 0);

        assert!(!executed.load(Ordering::SeqCst));
        assert!(registry.execute("test:action"));
        assert!(executed.load(Ordering::SeqCst));

        // Calling a nonexistent one
        assert!(!registry.execute("non:existent"));
    }

    #[test]
    fn test_search_matches_command_numbers() {
        let registry = CommandRegistry::new();
        for (id, owner) in [("core:a", None), ("core:b", None), ("aaa:plugin", Some("p"))] {
            registry.register(Command {
                id: id.into(),
                name: id.into(),
                owner: owner.map(str::to_string),
                callback: Arc::new(|| {}),
                hotkey: None,
                icon: None,
            });
        }

        // core:a is #1, core:b is #2; plugin commands are not numbered.
        let hits = registry.search("2", "en");
        assert_eq!(hits.first().map(|cmd| cmd.id.as_str()), Some("core:b"));
        assert!(!hits.iter().any(|cmd| cmd.id == "aaa:plugin"));
        assert!(registry.search("3", "en").is_empty());
    }

    #[test]
    fn test_command_cleanup_on_plugin_unload() {
        let registry = CommandRegistry::new();

        registry.register(Command {
            id: "builtin:save".into(),
            name: "Save".into(),
            owner: None,
            callback: Arc::new(|| {}),
            hotkey: None,
            icon: None,
        });

        registry.register(Command {
            id: "plugin:cmd1".into(),
            name: "Cmd 1".into(),
            owner: Some("plugin-a".into()),
            callback: Arc::new(|| {}),
            hotkey: None,
            icon: None,
        });

        registry.register(Command {
            id: "plugin:cmd2".into(),
            name: "Cmd 2".into(),
            owner: Some("plugin-b".into()),
            callback: Arc::new(|| {}),
            hotkey: None,
            icon: None,
        });

        assert_eq!(registry.all().len(), 3);

        // Unload plugin-a
        registry.unregister_all_of("plugin-a");
        assert_eq!(registry.all().len(), 2);
        assert!(registry.commands.lock().unwrap().contains_key("builtin:save"));
        assert!(registry.commands.lock().unwrap().contains_key("plugin:cmd2"));
        assert!(!registry.commands.lock().unwrap().contains_key("plugin:cmd1"));
    }

    #[test]
    fn test_palette_numbers_cover_only_builtins() {
        let registry = CommandRegistry::new();
        for (id, owner) in [
            ("zzz:plugin", Some("p")),
            ("core:b", None),
            ("aaa:plugin", Some("p")),
            ("core:a", None),
        ] {
            registry.register(Command {
                id: id.into(),
                name: id.into(),
                owner: owner.map(str::to_string),
                callback: Arc::new(|| {}),
                hotkey: None,
                icon: None,
            });
        }

        let numbers = registry.numbers();
        assert_eq!(numbers.len(), 2);
        assert_eq!(numbers["core:a"], 1);
        assert_eq!(numbers["core:b"], 2);
        assert!(!numbers.contains_key("aaa:plugin"));
        assert!(!numbers.contains_key("zzz:plugin"));
    }
}
