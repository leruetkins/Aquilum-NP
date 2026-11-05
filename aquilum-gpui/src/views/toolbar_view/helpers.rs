use crate::formatting::ActiveFormatting;
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
}

/// Checks if a command ID is currently disabled.
pub(super) fn is_command_disabled(command_id: &str, fmt: &ActiveFormatting) -> bool {
    match command_id {
        "editor:undo" => !fmt.can_undo,
        "editor:redo" => !fmt.can_redo,
        _ => false,
    }
}
