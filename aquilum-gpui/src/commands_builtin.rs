use std::sync::Arc;
use crate::commands::{Command, CommandRegistry};

/// Registers metadata for the application's built-in commands in the command registry.
pub fn register_builtin_commands(registry: &CommandRegistry) {
    // 1. Save the current note (Ctrl+S)
    registry.register(Command {
        id: "editor:save".into(),
        name: "Save current note".into(),
        owner: None,
        hotkey: Some("Ctrl+S".into()),
        icon: Some("save".into()),
        callback: Arc::new(|| {}),
    });

    // 2. New empty tab (Ctrl+N)
    registry.register(Command {
        id: "workspace:new-tab".into(),
        name: "New empty tab".into(),
        owner: None,
        hotkey: Some("Ctrl+N".into()),
        icon: Some("plus".into()),
        callback: Arc::new(|| {}),
    });

    // 3. Close the current tab (Ctrl+W)
    registry.register(Command {
        id: "workspace:close-tab".into(),
        name: "Close active tab".into(),
        owner: None,
        hotkey: Some("Ctrl+W".into()),
        icon: Some("close".into()),
        callback: Arc::new(|| {}),
    });

    // 4. Toggle the left sidebar (Ctrl+B)
    registry.register(Command {
        id: "view:toggle-left-sidebar".into(),
        name: "Toggle left sidebar".into(),
        owner: None,
        hotkey: Some("Ctrl+\\".into()),
        icon: Some("panel-left".into()),
        callback: Arc::new(|| {}),
    });

    // 5. Toggle the right sidebar
    registry.register(Command {
        id: "view:toggle-right-sidebar".into(),
        name: "Toggle right sidebar".into(),
        owner: None,
        hotkey: None,
        icon: Some("panel-right".into()),
        callback: Arc::new(|| {}),
    });

    // Toggle the vertical panel (Ribbon)
    registry.register(Command {
        id: "view:toggle-ribbon".into(),
        name: "Toggle vertical ribbon rail".into(),
        owner: None,
        hotkey: None,
        icon: None,
        callback: Arc::new(|| {}),
    });

    // 6. Toggle settings (Ctrl+,)
    registry.register(Command {
        id: "app:open-settings".into(),
        name: "Open settings".into(),
        owner: None,
        hotkey: Some("Ctrl+,".into()),
        icon: Some("settings".into()),
        callback: Arc::new(|| {}),
    });

    // 7. Toggle line wrapping (Word Wrap)
    registry.register(Command {
        id: "editor:toggle-word-wrap".into(),
        name: "Toggle word wrap".into(),
        owner: None,
        hotkey: None,
        icon: None,
        callback: Arc::new(|| {}),
    });

    // 8. Command palette (Ctrl+P)
    registry.register(Command {
        id: "app:open-command-palette".into(),
        name: "Open command palette".into(),
        owner: None,
        hotkey: Some("Ctrl+P".into()),
        icon: None,
        callback: Arc::new(|| {}),
    });

    // 9. Quick switcher menu / file search (Ctrl+O)
    registry.register(Command {
        id: "app:open-search-dialog".into(),
        name: "Quick switcher: Open search".into(),
        owner: None,
        hotkey: Some("Ctrl+O".into()),
        icon: Some("search".into()),
        callback: Arc::new(|| {}),
    });

    // 10. Split right (Ctrl+\)
    registry.register(Command {
        id: "workspace:split-right".into(),
        name: "Split right".into(),
        owner: None,
        hotkey: Some("Ctrl+\\".into()),
        icon: Some("split".into()),
        callback: Arc::new(|| {}),
    });

    // 11. Split down (Ctrl+Shift+\)
    registry.register(Command {
        id: "workspace:split-down".into(),
        name: "Split down".into(),
        owner: None,
        hotkey: Some("Ctrl+Shift+\\".into()),
        icon: Some("split-down".into()),
        callback: Arc::new(|| {}),
    });

    // 12. Export to PDF
    registry.register(Command {
        id: "app:export-pdf".into(),
        name: "Export to PDF".into(),
        owner: None,
        hotkey: None,
        icon: None,
        callback: Arc::new(|| {}),
    });

    // ─────────────────────────────────────────────────────────────────────────
    // Formatting Commands (Editor Toolbar & Keybindings)
    // ─────────────────────────────────────────────────────────────────────────

    registry.register(Command {
        id: "editor:undo".into(),
        name: "Undo".into(),
        owner: None,
        hotkey: Some("Ctrl+Z".into()),
        icon: Some("undo".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:redo".into(),
        name: "Redo".into(),
        owner: None,
        hotkey: Some("Ctrl+Y".into()),
        icon: Some("redo".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:clear-formatting".into(),
        name: "Clear formatting".into(),
        owner: None,
        hotkey: None,
        icon: Some("eraser".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:heading-1".into(),
        name: "Heading 1".into(),
        owner: None,
        hotkey: None,
        icon: Some("h1".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:heading-2".into(),
        name: "Heading 2".into(),
        owner: None,
        hotkey: None,
        icon: Some("h2".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:heading-3".into(),
        name: "Heading 3".into(),
        owner: None,
        hotkey: None,
        icon: Some("h3".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:heading-4".into(),
        name: "Heading 4".into(),
        owner: None,
        hotkey: None,
        icon: Some("h4".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:heading-5".into(),
        name: "Heading 5".into(),
        owner: None,
        hotkey: None,
        icon: Some("h5".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:heading-6".into(),
        name: "Heading 6".into(),
        owner: None,
        hotkey: None,
        icon: Some("h6".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:bold".into(),
        name: "Bold".into(),
        owner: None,
        hotkey: Some("Ctrl+B".into()),
        icon: Some("bold".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:italic".into(),
        name: "Italic".into(),
        owner: None,
        hotkey: Some("Ctrl+I".into()),
        icon: Some("italic".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:strikethrough".into(),
        name: "Strikethrough".into(),
        owner: None,
        hotkey: None,
        icon: Some("strikethrough".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:underline".into(),
        name: "Underline".into(),
        owner: None,
        hotkey: Some("Ctrl+U".into()),
        icon: Some("underline".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:highlight".into(),
        name: "Highlight".into(),
        owner: None,
        hotkey: None,
        icon: Some("highlighter".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:inline-code".into(),
        name: "Inline code".into(),
        owner: None,
        hotkey: None,
        icon: Some("code".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:code-block".into(),
        name: "Code block".into(),
        owner: None,
        hotkey: None,
        icon: Some("code-block".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:insert-callout".into(),
        name: "Insert callout".into(),
        owner: None,
        hotkey: None,
        icon: Some("comment".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:list-bullet".into(),
        name: "Bullet list".into(),
        owner: None,
        hotkey: None,
        icon: Some("list".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:list-numbered".into(),
        name: "Numbered list".into(),
        owner: None,
        hotkey: None,
        icon: Some("list-ordered".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:list-task".into(),
        name: "Task list".into(),
        owner: None,
        hotkey: None,
        icon: Some("check-circle".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:toggle-task".into(),
        name: "Toggle checkbox task".into(),
        owner: None,
        hotkey: None,
        icon: Some("check-circle".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:indent".into(),
        name: "Indent".into(),
        owner: None,
        hotkey: Some("Tab".into()),
        icon: Some("indent".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:outdent".into(),
        name: "Outdent".into(),
        owner: None,
        hotkey: Some("Shift+Tab".into()),
        icon: Some("outdent".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:insert-table".into(),
        name: "Insert table".into(),
        owner: None,
        hotkey: None,
        icon: Some("table".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:insert-image".into(),
        name: "Insert image…".into(),
        owner: None,
        hotkey: None,
        icon: Some("image".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:unused-attachments".into(),
        name: "Find unused attachments".into(),
        owner: None,
        hotkey: None,
        icon: Some("paperclip".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "note:export-md".into(),
        name: "Export note to Markdown zip".into(),
        owner: None,
        hotkey: None,
        icon: Some("download".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:attach-file".into(),
        name: "Attach file or link".into(),
        owner: None,
        hotkey: None,
        icon: Some("paperclip".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:align-left".into(),
        name: "Align left".into(),
        owner: None,
        hotkey: None,
        icon: Some("align-left".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:align-center".into(),
        name: "Align center".into(),
        owner: None,
        hotkey: None,
        icon: Some("align-center".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:align-right".into(),
        name: "Align right".into(),
        owner: None,
        hotkey: None,
        icon: Some("align-right".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:toggle-fullscreen".into(),
        name: "Toggle fullscreen".into(),
        owner: None,
        hotkey: Some("F11".into()),
        icon: Some("fullscreen".into()),
        callback: Arc::new(|| {}),
    });

    registry.register(Command {
        id: "editor:toggle-invisibles".into(),
        name: "Toggle non-printable characters".into(),
        owner: None,
        hotkey: None,
        icon: Some("pilcrow".into()),
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
        assert!(registry.search("bold").iter().any(|c| c.id == "editor:bold"));
        assert!(registry.search("italic").iter().any(|c| c.id == "editor:italic"));
        assert!(registry.search("undo").iter().any(|c| c.id == "editor:undo"));
        assert!(registry.search("heading").iter().any(|c| c.id == "editor:heading-1"));
        assert!(registry.search("table").iter().any(|c| c.id == "editor:insert-table"));
        assert!(registry.search("invisibles").iter().any(|c| c.id == "editor:toggle-invisibles"));
    }
}
