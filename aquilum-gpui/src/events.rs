use std::sync::Mutex;

/// События vault (файловые операции).
/// Рассылаются подписчикам плагинов и внутренних сервисов хоста.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultEvent {
    /// Файл изменён (сохранение заметки).
    Modify { path: String },
    /// Файл создан.
    Create { path: String },
    /// Файл удалён.
    Delete { path: String },
    /// Файл переименован.
    Rename { old_path: String, new_path: String },
}

/// События рабочей области.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceEvent {
    /// Открыт файл (или None при закрытии последнего файла / переходе на пустую вкладку).
    FileOpen { path: Option<String> },
    /// Изменилась активная вкладка.
    ActiveLeafChange,
    /// Vault открыт или переключён на другой путь.
    VaultOpen { path: String },
}

type VaultHandler = Box<dyn Fn(&VaultEvent) + Send + 'static>;
type WorkspaceHandler = Box<dyn Fn(&WorkspaceEvent) + Send + 'static>;

/// Потокобезопасная шина событий для связывания ядра с плагинами и UI.
pub struct EventBus {
    vault_handlers: Mutex<Vec<(u64, VaultHandler)>>,
    workspace_handlers: Mutex<Vec<(u64, WorkspaceHandler)>>,
    next_id: Mutex<u64>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            vault_handlers: Mutex::new(Vec::new()),
            workspace_handlers: Mutex::new(Vec::new()),
            next_id: Mutex::new(1),
        }
    }

    fn next_id(&self) -> u64 {
        let mut id = self.next_id.lock().unwrap();
        let val = *id;
        *id += 1;
        val
    }

    /// Подписаться на события vault. Возвращает id подписки для последующей отписки.
    pub fn on_vault<F>(&self, handler: F) -> u64
    where
        F: Fn(&VaultEvent) + Send + 'static,
    {
        let id = self.next_id();
        self.vault_handlers.lock().unwrap().push((id, Box::new(handler)));
        id
    }

    /// Подписаться на события workspace. Возвращает id подписки.
    pub fn on_workspace<F>(&self, handler: F) -> u64
    where
        F: Fn(&WorkspaceEvent) + Send + 'static,
    {
        let id = self.next_id();
        self.workspace_handlers.lock().unwrap().push((id, Box::new(handler)));
        id
    }

    /// Отписаться от событий по идентификатору подписки.
    pub fn off(&self, handler_id: u64) {
        self.vault_handlers.lock().unwrap().retain(|(id, _)| *id != handler_id);
        self.workspace_handlers.lock().unwrap().retain(|(id, _)| *id != handler_id);
    }

    /// Отправить событие vault всем зарегистрированным слушателям.
    pub fn emit_vault(&self, event: &VaultEvent) {
        let handlers = self.vault_handlers.lock().unwrap();
        for (_, handler) in handlers.iter() {
            handler(event);
        }
    }

    /// Отправить событие workspace всем зарегистрированным слушателям.
    pub fn emit_workspace(&self, event: &WorkspaceEvent) {
        let handlers = self.workspace_handlers.lock().unwrap();
        for (_, handler) in handlers.iter() {
            handler(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_event_bus_vault_events() {
        let bus = EventBus::new();
        let counter = Arc::new(AtomicUsize::new(0));

        let c = counter.clone();
        let sub_id = bus.on_vault(move |ev| {
            if let VaultEvent::Modify { path } = ev {
                if path == "Notes/Test.md" {
                    c.fetch_add(1, Ordering::SeqCst);
                }
            }
        });

        bus.emit_vault(&VaultEvent::Create { path: "Notes/Other.md".into() });
        assert_eq!(counter.load(Ordering::SeqCst), 0);

        bus.emit_vault(&VaultEvent::Modify { path: "Notes/Test.md".into() });
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        // Отписываемся
        bus.off(sub_id);
        bus.emit_vault(&VaultEvent::Modify { path: "Notes/Test.md".into() });
        assert_eq!(counter.load(Ordering::SeqCst), 1); // Не изменился
    }

    #[test]
    fn test_event_bus_workspace_events() {
        let bus = EventBus::new();
        let last_file = Arc::new(Mutex::new(None));

        let f = last_file.clone();
        let sub_id = bus.on_workspace(move |ev| {
            if let WorkspaceEvent::FileOpen { path } = ev {
                *f.lock().unwrap() = path.clone();
            }
        });

        bus.emit_workspace(&WorkspaceEvent::FileOpen { path: Some("readme.md".into()) });
        assert_eq!(*last_file.lock().unwrap(), Some("readme.md".into()));

        bus.emit_workspace(&WorkspaceEvent::FileOpen { path: None });
        assert_eq!(*last_file.lock().unwrap(), None);

        bus.off(sub_id);
        bus.emit_workspace(&WorkspaceEvent::FileOpen { path: Some("ignored.md".into()) });
        assert_eq!(*last_file.lock().unwrap(), None);
    }
}
