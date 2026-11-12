//! Locale keys for built-in toolbar labels.
//!
//! Toolbar tooltips are not persisted in `.vault/plugins/editor-toolbar/data.json`;
//! they are resolved from the command (or item) id at render time, so tooltips
//! follow the current interface language on every locale switch. Command labels
//! come from `command_i18n`, so the toolbar and the command palette share them.

/// Locale key for the label of a built-in toolbar dropdown group, keyed by its
/// stable item id (group dropdowns have no command of their own).
pub fn item_label_key(item_id: &str) -> Option<&'static str> {
    Some(match item_id {
        "tb-hn-dropdown" => "editor.toolbar.more_headings",
        "tb-callout-dropdown" => "editor.context.insert_callout",
        "tb-indent-dropdown" => "editor.context.formatting",
        "tb-lists-dropdown" => "editor.toolbar.lists",
        "tb-align-dropdown" => "editor.toolbar.alignment",
        _ => return None,
    })
}

/// Resolves the display label for a toolbar item. An explicit stored tooltip
/// (a custom or plugin-provided label) wins, then the localized name of a
/// built-in command or group, then the registered command name. `None` means
/// the item has no label and should not show a tooltip.
pub fn resolve_label(
    lang: &str,
    item_id: &str,
    command_id: Option<&str>,
    stored_tooltip: Option<&str>,
    command_name: Option<&str>,
) -> Option<String> {
    if let Some(tooltip) = stored_tooltip.map(str::trim).filter(|tip| !tip.is_empty()) {
        return Some(tooltip.to_string());
    }
    if let Some(key) = command_id.and_then(crate::command_i18n::label_key) {
        return Some(crate::i18n::t(key, lang));
    }
    if let Some(key) = item_label_key(item_id) {
        return Some(crate::i18n::t(key, lang));
    }
    command_name.map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::editor_toolbar::default_toolbar_items;
    use crate::toolbar::{ToolbarItem, ToolbarItemKind};

    fn assert_defaults_resolve(items: &[ToolbarItem]) {
        for item in items {
            if let ToolbarItemKind::Dropdown { items, .. } = &item.kind {
                assert_defaults_resolve(items);
            }
            if item.is_separator() {
                continue;
            }
            let label = resolve_label("en", &item.id, item.command_id(), item.tooltip(), None)
                .unwrap_or_else(|| panic!("no label for toolbar item {}", item.id));
            assert!(
                !label.starts_with("editor."),
                "missing locale key while resolving {}: {label}",
                item.id
            );
        }
    }

    #[test]
    fn every_default_item_resolves_to_a_localized_label() {
        assert_defaults_resolve(&default_toolbar_items());
    }

    #[test]
    fn stored_tooltip_wins_over_the_localized_name() {
        let label = resolve_label("de", "tb-custom", Some("editor:bold"), Some("Custom"), None);
        assert_eq!(label.as_deref(), Some("Custom"));
    }

    #[test]
    fn known_command_uses_the_interface_language() {
        let expected = crate::i18n::t("editor.context.bold", "de");
        let label = resolve_label("de", "tb-bold", Some("editor:bold"), None, Some("Bold"));
        assert_eq!(label.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn unknown_command_falls_back_to_its_registered_name() {
        let label = resolve_label("ru", "tb-plugin", Some("plugin:action"), None, Some("My action"));
        assert_eq!(label.as_deref(), Some("My action"));
    }
}
