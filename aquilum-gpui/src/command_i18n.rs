//! Locale keys and display labels for built-in commands.
//!
//! Command registry names are English and are registered once, so they are not
//! translated in place: the palette resolves a label from the command id at
//! render time. This keeps the interface language switch live and lets search
//! match both the localized and the English name.

/// Locale key for a built-in command, or `None` for plugin commands.
pub fn label_key(command_id: &str) -> Option<&'static str> {
    Some(match command_id {
        "editor:save" => "settings.shortcuts.save_note",
        "workspace:new-tab" => "settings.shortcuts.new_note",
        "workspace:close-tab" => "command.close_tab",
        "view:toggle-left-sidebar" => "settings.shortcuts.toggle_sidebar",
        "view:toggle-right-sidebar" => "command.toggle_right_sidebar",
        "view:toggle-ribbon" => "command.toggle_ribbon",
        "app:open-settings" => "settings.title",
        "editor:toggle-word-wrap" => "settings.shortcuts.toggle_word_wrap",
        "app:open-command-palette" => "settings.shortcuts.open_command_palette",
        "app:open-search-dialog" => "command.quick_switcher",
        "workspace:split-right" => "command.split_right",
        "workspace:split-down" => "command.split_down",
        "app:export-pdf" => "command.export_pdf",

        "editor:undo" => "editor.toolbar.undo",
        "editor:redo" => "editor.toolbar.redo",
        "editor:clear-formatting" => "editor.toolbar.clear_formatting",
        "editor:heading-1" => "editor.context.heading_1",
        "editor:heading-2" => "editor.context.heading_2",
        "editor:heading-3" => "editor.context.heading_3",
        "editor:heading-4" => "editor.context.heading_4",
        "editor:heading-5" => "editor.context.heading_5",
        "editor:heading-6" => "editor.context.heading_6",
        "editor:bold" => "editor.context.bold",
        "editor:italic" => "editor.context.italic",
        "editor:strikethrough" => "editor.context.strikethrough",
        "editor:underline" => "editor.toolbar.underline",
        "editor:highlight" => "editor.context.highlight",
        "editor:inline-code" => "editor.context.inline_code",
        "editor:code-block" => "editor.context.insert_code_block",
        "editor:insert-callout" => "editor.context.insert_callout",
        "editor:list-bullet" => "editor.context.bullet_list",
        "editor:list-numbered" => "editor.context.numbered_list",
        "editor:list-task" => "editor.context.task_list",
        "editor:toggle-task" => "editor.toolbar.toggle_task",
        "editor:indent" => "editor.toolbar.indent",
        "editor:outdent" => "editor.toolbar.outdent",
        "editor:insert-table" => "editor.context.insert_table",
        "editor:insert-image" => "editor.context.insert_image",
        "editor:unused-attachments" => "command.unused_attachments",
        "note:export-md" => "command.export_markdown_zip",
        "editor:attach-file" => "editor.toolbar.attach_file",
        "editor:align-left" => "editor.context.table_align_left",
        "editor:align-center" => "editor.context.table_align_center",
        "editor:align-right" => "editor.context.table_align_right",
        "editor:toggle-fullscreen" => "editor.toolbar.fullscreen",
        "editor:toggle-invisibles" => "editor.context.show_invisibles",
        _ => return None,
    })
}

/// Localized display name of a command, falling back to its registered name.
pub fn display_name(command_id: &str, registered_name: &str, lang: &str) -> String {
    display_labels(command_id, registered_name, lang).0
}

/// Primary and secondary labels for a command row: the interface-language name,
/// plus the English name when the translation differs from it. English UI and
/// untranslated entries keep the registered name and show no second line.
pub fn display_labels(
    command_id: &str,
    registered_name: &str,
    lang: &str,
) -> (String, Option<String>) {
    let Some(key) = label_key(command_id) else {
        return (registered_name.to_string(), None);
    };
    let english = crate::i18n::t(key, "en");
    let label = crate::i18n::t(key, lang);
    if label.is_empty() || label == english || label == registered_name {
        return (registered_name.to_string(), None);
    }
    (label, Some(registered_name.to_string()))
}

/// Second line of a palette row: the English name (when the translation
/// differs) followed by the command id.
pub fn secondary_line(english_name: Option<&str>, command_id: &str) -> String {
    match english_name {
        Some(name) => format!("{name} · {command_id}"),
        None => command_id.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::CommandRegistry;
    use crate::commands_builtin::register_builtin_commands;

    #[test]
    fn every_builtin_command_has_a_locale_key() {
        let registry = CommandRegistry::new();
        register_builtin_commands(&registry);
        for cmd in registry.all() {
            let key = label_key(&cmd.id)
                .unwrap_or_else(|| panic!("missing locale key for built-in command {}", cmd.id));
            assert_ne!(
                crate::i18n::t(key, "en"),
                key,
                "missing en.json entry for {key} (command {})",
                cmd.id
            );
        }
    }

    #[test]
    fn plugin_commands_have_no_key() {
        assert_eq!(label_key("my-plugin:action"), None);
    }

    #[test]
    fn russian_label_is_shown_with_the_english_original() {
        let (primary, secondary) = display_labels("editor:bold", "Bold", "ru");
        assert_eq!(primary, crate::i18n::t("editor.context.bold", "ru"));
        assert_eq!(secondary.as_deref(), Some("Bold"));
    }

    #[test]
    fn english_ui_has_no_duplicate_second_line() {
        let (primary, secondary) = display_labels("editor:bold", "Bold", "en");
        assert_eq!(primary, "Bold");
        assert_eq!(secondary, None);
    }

    #[test]
    fn english_ui_keeps_the_registered_name() {
        let (primary, secondary) = display_labels("editor:save", "Save current note", "en");
        assert_eq!(primary, "Save current note");
        assert_eq!(secondary, None);
    }

    #[test]
    fn secondary_line_shows_english_name_and_id() {
        assert_eq!(
            secondary_line(Some("Bold"), "editor:bold"),
            "Bold · editor:bold"
        );
        assert_eq!(secondary_line(None, "editor:bold"), "editor:bold");
    }
}
