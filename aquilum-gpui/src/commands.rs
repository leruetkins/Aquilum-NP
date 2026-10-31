use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub type CommandCallback = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
pub struct Command {
    pub id: String,
    pub name: String,
    /// None для встроенных команд, Some(plugin_id) для плагинных команд.
    pub owner: Option<String>,
    pub callback: CommandCallback,
    /// Горячая клавиша для отображения в палитре (например, "Ctrl+S").
    pub hotkey: Option<String>,
    /// Иконка для отображения в тулбаре и палитре команд.
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

/// Центральный реестр команд приложения и плагинов.
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

    /// Зарегистрировать команду. Если команда с таким id существовала, она перезаписывается.
    pub fn register(&self, cmd: Command) -> String {
        let id = cmd.id.clone();
        self.commands.lock().unwrap().insert(id.clone(), cmd);
        id
    }

    /// Найти команду по id.
    pub fn get(&self, id: &str) -> Option<Command> {
        let cmds = self.commands.lock().unwrap();
        cmds.get(id).cloned()
    }

    /// Снять регистрацию команды по идентификатору.
    pub fn unregister(&self, id: &str) {
        self.commands.lock().unwrap().remove(id);
    }

    /// Удалить все команды, зарегистрированные конкретным плагином (при его выгрузке).
    pub fn unregister_all_of(&self, plugin_id: &str) {
        self.commands
            .lock()
            .unwrap()
            .retain(|_, cmd| cmd.owner.as_deref() != Some(plugin_id));
    }

    /// Получить список всех команд (для палитры команд).
    pub fn all(&self) -> Vec<Command> {
        let cmds = self.commands.lock().unwrap();
        let mut list: Vec<Command> = cmds.values().cloned().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    /// Фильтрация команд по поисковому запросу (все слова запроса должны присутствовать в имени или id).
    pub fn search(&self, query: &str) -> Vec<Command> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return self.all();
        }
        let words: Vec<&str> = q.split_whitespace().collect();
        let cmds = self.commands.lock().unwrap();
        let mut list: Vec<Command> = cmds
            .values()
            .filter(|cmd| {
                let name = cmd.name.to_lowercase();
                let id = cmd.id.to_lowercase();
                words.iter().all(|w| name.contains(w) || id.contains(w))
            })
            .cloned()
            .collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    /// Выполнить команду по ее идентификатору. Возвращает true, если команда найдена и вызвана.
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
        assert_eq!(registry.search("action").len(), 1);
        assert_eq!(registry.search("missing").len(), 0);

        assert!(!executed.load(Ordering::SeqCst));
        assert!(registry.execute("test:action"));
        assert!(executed.load(Ordering::SeqCst));

        // Вызов несуществующей
        assert!(!registry.execute("non:existent"));
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

        // Выгружаем plugin-a
        registry.unregister_all_of("plugin-a");
        assert_eq!(registry.all().len(), 2);
        assert!(registry.commands.lock().unwrap().contains_key("builtin:save"));
        assert!(registry.commands.lock().unwrap().contains_key("plugin:cmd2"));
        assert!(!registry.commands.lock().unwrap().contains_key("plugin:cmd1"));
    }
}
