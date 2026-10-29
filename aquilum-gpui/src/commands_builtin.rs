use std::sync::Arc;
use crate::commands::{Command, CommandRegistry};

/// Регистрирует метаданные встроенных команд приложения в реестр команд.
pub fn register_builtin_commands(registry: &CommandRegistry) {
    // 1. Сохранить текущую заметку (Ctrl+S)
    registry.register(Command {
        id: "editor:save".into(),
        name: "Save current note".into(),
        owner: None,
        hotkey: Some("Ctrl+S".into()),
        callback: Arc::new(|| {}),
    });

    // 2. Новая пустая вкладка (Ctrl+N)
    registry.register(Command {
        id: "workspace:new-tab".into(),
        name: "New empty tab".into(),
        owner: None,
        hotkey: Some("Ctrl+N".into()),
        callback: Arc::new(|| {}),
    });

    // 3. Закрыть текущую вкладку (Ctrl+W)
    registry.register(Command {
        id: "workspace:close-tab".into(),
        name: "Close active tab".into(),
        owner: None,
        hotkey: Some("Ctrl+W".into()),
        callback: Arc::new(|| {}),
    });

    // 4. Переключить левый сайдбар (Ctrl+B)
    registry.register(Command {
        id: "view:toggle-left-sidebar".into(),
        name: "Toggle left sidebar".into(),
        owner: None,
        hotkey: Some("Ctrl+B".into()),
        callback: Arc::new(|| {}),
    });

    // 5. Переключить правый сайдбар
    registry.register(Command {
        id: "view:toggle-right-sidebar".into(),
        name: "Toggle right sidebar".into(),
        owner: None,
        hotkey: None,
        callback: Arc::new(|| {}),
    });

    // Переключить вертикальную панель (Ribbon)
    registry.register(Command {
        id: "view:toggle-ribbon".into(),
        name: "Toggle vertical ribbon rail".into(),
        owner: None,
        hotkey: None,
        callback: Arc::new(|| {}),
    });

    // 6. Переключить настройки (Ctrl+,)
    registry.register(Command {
        id: "app:open-settings".into(),
        name: "Open settings".into(),
        owner: None,
        hotkey: Some("Ctrl+,".into()),
        callback: Arc::new(|| {}),
    });

    // 7. Переключить перенос строк (Word Wrap)
    registry.register(Command {
        id: "editor:toggle-word-wrap".into(),
        name: "Toggle word wrap".into(),
        owner: None,
        hotkey: None,
        callback: Arc::new(|| {}),
    });

    // 8. Палитра команд (Ctrl+P)
    registry.register(Command {
        id: "app:open-command-palette".into(),
        name: "Open command palette".into(),
        owner: None,
        hotkey: Some("Ctrl+P".into()),
        callback: Arc::new(|| {}),
    });

    // 9. Меню быстрого перехода / поиск файлов (Ctrl+O)
    registry.register(Command {
        id: "app:open-search-dialog".into(),
        name: "Quick switcher: Open search".into(),
        owner: None,
        hotkey: Some("Ctrl+O".into()),
        callback: Arc::new(|| {}),
    });

    // 10. Разделить справа (Ctrl+\)
    registry.register(Command {
        id: "workspace:split-right".into(),
        name: "Split right".into(),
        owner: None,
        hotkey: Some("Ctrl+\\".into()),
        callback: Arc::new(|| {}),
    });

    // 11. Разделить снизу (Ctrl+Shift+\)
    registry.register(Command {
        id: "workspace:split-down".into(),
        name: "Split down".into(),
        owner: None,
        hotkey: Some("Ctrl+Shift+\\".into()),
        callback: Arc::new(|| {}),
    });

    // 12. Экспорт в PDF
    registry.register(Command {
        id: "app:export-pdf".into(),
        name: "Export to PDF".into(),
        owner: None,
        hotkey: None,
        callback: Arc::new(|| {}),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_commands_registration() {
        let registry = CommandRegistry::new();
        register_builtin_commands(&registry);

        let commands = registry.all();
        assert!(commands.len() >= 9);

        assert!(registry.search("save").iter().any(|c| c.id == "editor:save"));
        assert!(registry.search("settings").iter().any(|c| c.id == "app:open-settings"));
        assert!(registry.search("sidebar").iter().any(|c| c.id == "view:toggle-left-sidebar"));
        assert!(registry.search("sidebar").iter().any(|c| c.id == "view:toggle-right-sidebar"));
        assert!(registry.search("new tab").iter().any(|c| c.id == "workspace:new-tab"));
        assert!(registry.search("palette").iter().any(|c| c.id == "app:open-command-palette"));
        assert!(registry.search("switcher").iter().any(|c| c.id == "app:open-search-dialog"));
        assert!(registry.search("pdf").iter().any(|c| c.id == "app:export-pdf"));
    }
}
