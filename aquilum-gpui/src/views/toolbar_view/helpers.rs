use crate::formatting::ActiveFormatting;
use crate::toolbar::ToolbarItem;
use crate::views::app_view::AppView;

impl AppView {
    /// Helper to get command name by ID.
    pub(super) fn get_command_name(&self, command_id: &str) -> Option<String> {
        self.state.commands.get(command_id).map(|cmd| cmd.name.clone())
    }

    /// Helper to get command hotkey string by ID.
    pub(super) fn get_command_hotkey(&self, command_id: &str) -> Option<String> {
        self.state.commands.get(command_id).and_then(|cmd| cmd.hotkey.clone())
    }

    /// Label shown for a toolbar item: an explicit stored tooltip (a custom or
    /// plugin-provided label) wins, then the localized name of a built-in
    /// command or group, then the registered command name. `None` means the
    /// item has no label and should not show a tooltip.
    pub(crate) fn toolbar_item_label(&self, item: &ToolbarItem) -> Option<String> {
        crate::toolbar_i18n::resolve_label(
            &self.config.ui.language,
            &item.id,
            item.command_id(),
            item.tooltip(),
            item.command_id()
                .and_then(|cmd| self.get_command_name(cmd))
                .as_deref(),
        )
    }
}

/// Checks if a command ID is currently disabled.
pub(super) fn is_command_disabled(command_id: &str, fmt: &ActiveFormatting) -> bool {
    match command_id {
        "editor:undo" => !fmt.can_undo,
        "editor:redo" => !fmt.can_redo,
        _ => false,
    }
}
