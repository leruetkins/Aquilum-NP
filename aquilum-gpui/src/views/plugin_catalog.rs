use serde::{Deserialize, Serialize};

/// Catalog metadata for a community plugin published in the registry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogPluginEntry {
    /// Unique plugin identifier (e.g. "dataview", "calendar").
    pub id: String,
    /// Human-friendly display name.
    pub name: String,
    /// Author name or GitHub handle.
    pub author: String,
    /// Brief description of plugin capabilities.
    pub description: String,
    /// GitHub repository relative path (e.g. "blacksmithgu/obsidian-dataview").
    pub repo: String,
    /// Latest release version tag (e.g. "0.5.67").
    #[serde(default = "default_version")]
    pub version: String,
    /// Download count for popularity ranking.
    #[serde(default)]
    pub downloads: u64,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

/// Filter and sorting criteria for the catalog browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogSortOrder {
    MostPopular,
    Alphabetical,
    RecentlyUpdated,
}

/// State of the Community Plugins Catalog modal dialog.
#[derive(Debug, Clone)]
pub struct PluginCatalogState {
    pub is_open: bool,
    pub search_query: String,
    pub sort_order: CatalogSortOrder,
    pub selected_plugin_id: Option<String>,
    pub entries: Vec<CatalogPluginEntry>,
    pub is_loading: bool,
    pub status_message: Option<String>,
}

impl Default for PluginCatalogState {
    fn default() -> Self {
        Self {
            is_open: false,
            search_query: String::new(),
            sort_order: CatalogSortOrder::MostPopular,
            selected_plugin_id: None,
            entries: Self::default_catalog(),
            is_loading: false,
            status_message: None,
        }
    }
}

impl PluginCatalogState {
    /// Default built-in registry entries available offline or while loading remote registry.
    pub fn default_catalog() -> Vec<CatalogPluginEntry> {
        vec![
            CatalogPluginEntry {
                id: "dataview".to_string(),
                name: "Dataview".to_string(),
                author: "blacksmithgu".to_string(),
                description: "Complex queries, JavaScript APIs, and dynamic tables over your markdown vault.".to_string(),
                repo: "blacksmithgu/obsidian-dataview".to_string(),
                version: "0.5.67".to_string(),
                downloads: 1450000,
            },
            CatalogPluginEntry {
                id: "calendar".to_string(),
                name: "Calendar".to_string(),
                author: "liamcain".to_string(),
                description: "Simple calendar view in the sidebar to visualize and navigate daily notes.".to_string(),
                repo: "liamcain/obsidian-calendar-plugin".to_string(),
                version: "1.5.10".to_string(),
                downloads: 890000,
            },
            CatalogPluginEntry {
                id: "kanban".to_string(),
                name: "Kanban".to_string(),
                author: "mgmeyers".to_string(),
                description: "Create Kanban boards inside notes with drag-and-drop cards and lists.".to_string(),
                repo: "mgmeyers/obsidian-kanban".to_string(),
                version: "2.0.51".to_string(),
                downloads: 980000,
            },
            CatalogPluginEntry {
                id: "tasks".to_string(),
                name: "Tasks".to_string(),
                author: "obsidian-tasks-group".to_string(),
                description: "Task management, recurring task schedules, and checklist queries across all notes.".to_string(),
                repo: "obsidian-tasks-group/obsidian-tasks".to_string(),
                version: "7.19.0".to_string(),
                downloads: 720000,
            },
            CatalogPluginEntry {
                id: "linter".to_string(),
                name: "Linter".to_string(),
                author: "platers".to_string(),
                description: "Format and lint markdown files according to customizable style rules.".to_string(),
                repo: "platers/obsidian-linter".to_string(),
                version: "1.26.0".to_string(),
                downloads: 540000,
            },
        ]
    }

    /// Returns filtered and sorted list of catalog entries according to current search query.
    pub fn filtered_entries(&self) -> Vec<CatalogPluginEntry> {
        let q = self.search_query.trim().to_lowercase();
        let mut list: Vec<CatalogPluginEntry> = self
            .entries
            .iter()
            .filter(|e| {
                if q.is_empty() {
                    true
                } else {
                    e.name.to_lowercase().contains(&q)
                        || e.id.to_lowercase().contains(&q)
                        || e.author.to_lowercase().contains(&q)
                        || e.description.to_lowercase().contains(&q)
                }
            })
            .cloned()
            .collect();

        match self.sort_order {
            CatalogSortOrder::MostPopular => {
                list.sort_by(|a, b| b.downloads.cmp(&a.downloads));
            }
            CatalogSortOrder::Alphabetical => {
                list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            }
            CatalogSortOrder::RecentlyUpdated => {
                list.sort_by(|a, b| b.version.cmp(&a.version));
            }
        }
        list
    }
}

use crate::icons;
use crate::views::app_view::AppView;
use gpui::prelude::*;
use gpui::{
    div, px, rgba, Context, ElementId, InteractiveElement, IntoElement, KeyDownEvent,
    MouseButton, ParentElement, SharedString, StatefulInteractiveElement, Styled,
};

impl AppView {
    /// Opens the Community Plugins catalog modal.
    pub fn open_plugin_catalog(&mut self, cx: &mut Context<Self>) {
        self.plugin_catalog.is_open = true;
        self.plugin_catalog.search_query.clear();
        if self.plugin_catalog.selected_plugin_id.is_none() {
            if let Some(first) = self.plugin_catalog.entries.first() {
                self.plugin_catalog.selected_plugin_id = Some(first.id.clone());
            }
        }
        cx.notify();
    }

    /// Closes the Community Plugins catalog modal.
    pub fn close_plugin_catalog(&mut self, cx: &mut Context<Self>) {
        self.plugin_catalog.is_open = false;
        cx.notify();
    }

    /// Installs a plugin into `<vault>/.vault/plugins/<id>/` from the catalog entry.
    pub fn install_plugin_from_catalog(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        let Some(entry) = self.plugin_catalog.entries.iter().find(|e| e.id == plugin_id).cloned() else {
            return;
        };
        let Some(vault) = &self.state.vault else {
            self.state.status_text = "Cannot install plugin: No vault opened".to_string();
            cx.notify();
            return;
        };

        let plugin_dir = vault.root.join(".vault").join("plugins").join(&entry.id);
        if let Err(e) = std::fs::create_dir_all(&plugin_dir) {
            self.state.status_text = format!("Failed to create plugin directory: {}", e);
            cx.notify();
            return;
        }

        // Generate manifest.json
        let manifest_json = serde_json::json!({
            "id": entry.id,
            "name": entry.name,
            "version": entry.version,
            "minAppVersion": "0.1.0",
            "description": entry.description,
            "author": entry.author,
            "permissions": {
                "vault": "read",
                "network": []
            }
        });

        // Generate main.js stub
        let main_js = format!(
            r#"/* {} v{} by {} */
class Plugin {{
    onload() {{
        new Notice("{} is active!");
    }}
    onunload() {{}}
}}
module.exports = Plugin;
"#,
            entry.name, entry.version, entry.author, entry.name
        );

        let _ = std::fs::write(
            plugin_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest_json).unwrap_or_default(),
        );
        let _ = std::fs::write(plugin_dir.join("main.js"), main_js);

        // Enable plugin in community-plugins.json
        let _ = vault.enable_community_plugin(&entry.id);

        // Reload plugins if safe mode is off
        if !self.config.community_plugins.safe_mode {
            self.load_installed_plugins_for_vault();
        }

        self.state.status_text = format!("Installed and enabled plugin '{}'", entry.name);
        cx.notify();
    }

    /// Uninstalls an installed plugin by removing its directory and disabling it.
    pub fn uninstall_plugin_from_catalog(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        let Some(vault) = &self.state.vault else {
            return;
        };

        let _ = vault.disable_community_plugin(plugin_id);

        // Unload runtime if active
        if let Some(pos) = self.plugin_runtimes.iter().position(|r| r.manifest.id == plugin_id) {
            let handle = self.plugin_runtimes.remove(pos);
            let _ = handle.host_events_tx.unbounded_send(crate::plugins::api::HostEvent::Unload);
            self.state.plugin_host.unregister_plugin(plugin_id);
            self.state.commands.unregister_all_of(plugin_id);
            self.state.ribbon.unregister_all_of(plugin_id);
            self.state.status_bar.unregister_all_of(plugin_id);
            self.state.code_block_processors.unregister_all_of(plugin_id);
            self.code_block_views.retain(|_, (pid, _)| pid != plugin_id);
        }

        let plugin_dir = vault.root.join(".vault").join("plugins").join(plugin_id);
        let _ = std::fs::remove_dir_all(&plugin_dir);

        self.state.status_text = format!("Uninstalled plugin '{}'", plugin_id);
        cx.notify();
    }

    /// Renders the Community Plugins Catalog modal window.
    pub fn render_plugin_catalog_modal(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let _is_ru = self.config.ui.language.starts_with("ru");

        let entries = self.plugin_catalog.filtered_entries();
        let selected_id = self.plugin_catalog.selected_plugin_id.clone();
        let selected_entry = entries
            .iter()
            .find(|e| Some(&e.id) == selected_id.as_ref())
            .or_else(|| entries.first());

        div()
            .id("plugin-catalog-root")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .cursor(gpui::CursorStyle::Arrow)
            .track_focus(&self.catalog_focus_handle)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.close_plugin_catalog(cx);
                }
            }))
            // Backdrop
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .bg(rgba(0x000000aa))
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.close_plugin_catalog(cx);
                    })),
            )
            // Centered Catalog Dialog Window
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .id("plugin-catalog-dialog")
                            .flex()
                            .flex_col()
                            .w(px(920.0))
                            .h(px(640.0))
                            .max_w(px(960.0))
                            .max_h(px(720.0))
                            .rounded_xl()
                            .overflow_hidden()
                            .bg(theme.bg_sidebar)
                            .border_1()
                            .border_color(theme.border)
                            .shadow_lg()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            // Catalog Header with Search
                            .child(self.render_catalog_header(cx))
                            // Catalog Body: List (left) + Details (right)
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .flex_1()
                                    .min_h_0()
                                    .w_full()
                                    .child(self.render_catalog_list(&entries, cx))
                                    .child(self.render_catalog_details(selected_entry, cx)),
                            ),
                    ),
            )
    }

    /// Renders the title bar, search input, and sort toggles of the Catalog modal.
    fn render_catalog_header(&self, cx: &Context<Self>) -> gpui::Div {
        let theme = &self.theme;
        let is_ru = self.config.ui.language.starts_with("ru");
        let query = self.plugin_catalog.search_query.clone();

        div()
            .flex()
            .flex_col()
            .px_5()
            .py_3()
            .border_b_1()
            .border_color(theme.border_subtle)
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(icons::icon(icons::ICON_GLOBE, 18.0, theme.accent))
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(theme.text_primary)
                                    .child(if is_ru {
                                        "Каталог сторонних плагинов"
                                    } else {
                                        "Community Plugins Catalog"
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .id("btn-close-plugin-catalog")
                            .flex()
                            .items_center()
                            .justify_center()
                            .p_1()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(icons::ICON_CLOSE, 16.0, theme.text_muted))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_plugin_catalog(cx);
                            })),
                    ),
            )
            // Search Bar & Sort
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .flex_row()
                            .items_center()
                            .bg(theme.bg_editor)
                            .border_1()
                            .border_color(theme.border)
                            .rounded_md()
                            .px_3()
                            .py_1p5()
                            .gap_2()
                            .child(icons::icon(icons::ICON_SEARCH, 14.0, theme.text_muted))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(if query.is_empty() {
                                        theme.text_muted
                                    } else {
                                        theme.text_primary
                                    })
                                    .child(if query.is_empty() {
                                        if is_ru {
                                            "Поиск плагинов по названию или автору...".to_string()
                                        } else {
                                            "Search community plugins...".to_string()
                                        }
                                    } else {
                                        query
                                    }),
                            ),
                    ),
            )
    }

    /// Renders the left scrollable list of plugin entries in the catalog.
    fn render_catalog_list(&self, entries: &[CatalogPluginEntry], cx: &Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let selected_id = self.plugin_catalog.selected_plugin_id.as_deref().unwrap_or("");
        let is_ru = self.config.ui.language.starts_with("ru");

        div()
            .id("catalog-items-scroll")
            .track_scroll(&self.catalog_scroll_handle)
            .flex()
            .flex_col()
            .w(px(340.0))
            .min_w(px(340.0))
            .h_full()
            .border_r_1()
            .border_color(theme.border_subtle)
            .overflow_scroll()
            .p_2()
            .gap_1p5()
            .children(entries.iter().map(|entry| {
                let is_selected = entry.id == selected_id;
                let is_installed = self
                    .state
                    .vault
                    .as_ref()
                    .map(|v| v.root.join(".vault").join("plugins").join(&entry.id).exists())
                    .unwrap_or(false);

                let entry_id = entry.id.clone();
                let bg_color = if is_selected {
                    theme.bg_hover
                } else {
                    rgba(0x00000000)
                };

                div()
                    .id(ElementId::from(SharedString::from(format!("cat-item-{}", entry.id))))
                    .flex()
                    .flex_col()
                    .p_2p5()
                    .rounded_md()
                    .bg(bg_color)
                    .cursor_pointer()
                    .hover(|h| h.bg(theme.bg_hover))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.plugin_catalog.selected_plugin_id = Some(entry_id.clone());
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme.text_primary)
                                    .child(entry.name.clone()),
                            )
                            .when(is_installed, |parent| {
                                parent.child(
                                    div()
                                        .text_xs()
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded_sm()
                                        .bg(theme.accent)
                                        .text_color(theme.text_on_accent)
                                        .child(if is_ru { "Установлен" } else { "Installed" }),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.text_muted)
                            .child(format!("by {}", entry.author)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.text_secondary)
                            .line_clamp(2)
                            .child(entry.description.clone()),
                    )
            }))
    }

    /// Renders the right detail preview pane of the currently selected plugin.
    fn render_catalog_details(
        &self,
        entry_opt: Option<&CatalogPluginEntry>,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let theme = &self.theme;
        let is_ru = self.config.ui.language.starts_with("ru");

        let Some(entry) = entry_opt else {
            return div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(theme.text_muted)
                .child(if is_ru { "Выберите плагин из списка" } else { "Select a plugin from the list" })
                .into_any_element();
        };

        let is_installed = self
            .state
            .vault
            .as_ref()
            .map(|v| v.root.join(".vault").join("plugins").join(&entry.id).exists())
            .unwrap_or(false);

        let is_enabled = self
            .state
            .vault
            .as_ref()
            .map(|v| v.is_community_plugin_enabled(&entry.id))
            .unwrap_or(false);

        let plugin_id = entry.id.clone();
        let plugin_id_uninstall = entry.id.clone();
        let plugin_id_toggle = entry.id.clone();

        div()
            .id("catalog-details-scroll")
            .flex()
            .flex_col()
            .flex_1()
            .h_full()
            .overflow_scroll()
            .bg(theme.bg_editor)
            .p_6()
            .gap_4()
            // Title & Author Header
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(theme.text_primary)
                                    .child(entry.name.clone()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.text_muted)
                                    .child(format!(
                                        "v{} • by {} • {} downloads",
                                        entry.version, entry.author, entry.downloads
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .when(!is_installed, |parent| {
                                parent.child(
                                    div()
                                        .id("btn-install-catalog-plugin")
                                        .px_4()
                                        .py_1p5()
                                        .rounded_md()
                                        .bg(theme.accent)
                                        .cursor_pointer()
                                        .hover(|h| h.bg(theme.accent_hover))
                                        .text_sm()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(theme.text_on_accent)
                                        .child(if is_ru { "Установить" } else { "Install" })
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.install_plugin_from_catalog(&plugin_id, cx);
                                        })),
                                )
                            })
                            .when(is_installed, |parent| {
                                parent
                                    .child(
                                        div()
                                            .id("btn-toggle-catalog-plugin")
                                            .px_3()
                                            .py_1p5()
                                            .rounded_md()
                                            .bg(theme.bg_hover)
                                            .cursor_pointer()
                                            .hover(|h| h.bg(theme.border))
                                            .text_sm()
                                            .text_color(theme.text_primary)
                                            .child(if is_enabled {
                                                if is_ru { "Отключить" } else { "Disable" }
                                            } else {
                                                if is_ru { "Включить" } else { "Enable" }
                                            })
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                if let Some(vault) = &this.state.vault {
                                                    if is_enabled {
                                                        let _ = vault.disable_community_plugin(&plugin_id_toggle);
                                                        if let Some(pos) = this.plugin_runtimes.iter().position(|r| r.manifest.id == plugin_id_toggle) {
                                                            let handle = this.plugin_runtimes.remove(pos);
                                                            let _ = handle.host_events_tx.unbounded_send(crate::plugins::api::HostEvent::Unload);
                                                            this.state.plugin_host.unregister_plugin(&plugin_id_toggle);
                                                        }
                                                    } else {
                                                        let _ = vault.enable_community_plugin(&plugin_id_toggle);
                                                        this.load_installed_plugins_for_vault();
                                                    }
                                                    cx.notify();
                                                }
                                            })),
                                    )
                                    .child(
                                        div()
                                            .id("btn-uninstall-catalog-plugin")
                                            .px_3()
                                            .py_1p5()
                                            .rounded_md()
                                            .bg(rgba(0xea4335ff))
                                            .cursor_pointer()
                                            .hover(|h| h.bg(rgba(0xd93025ff)))
                                            .text_sm()
                                            .text_color(theme.text_on_accent)
                                            .child(if is_ru { "Удалить" } else { "Uninstall" })
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.uninstall_plugin_from_catalog(&plugin_id_uninstall, cx);
                                            })),
                                    )
                            }),
                    ),
            )
            // Description & Manifest permissions badge
            .child(
                div()
                    .text_sm()
                    .text_color(theme.text_primary)
                    .child(entry.description.clone()),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border_subtle)
                    .bg(theme.bg_sidebar)
                    .child(
                        div()
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text_secondary)
                            .child(if is_ru { "Запрашиваемые права (Permissions)" } else { "Requested Permissions" }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .px_2()
                                    .py_1()
                                    .rounded_sm()
                                    .bg(theme.bg_hover)
                                    .text_color(theme.text_muted)
                                    .child("vault:read"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .px_2()
                                    .py_1()
                                    .rounded_sm()
                                    .bg(theme.bg_hover)
                                    .text_color(theme.text_muted)
                                    .child(if is_ru { "Сеть: отключена" } else { "Network: none" }),
                            ),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(format!("Repository: https://github.com/{}", entry.repo)),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_default_entries() {
        let state = PluginCatalogState::default();
        assert!(!state.entries.is_empty());
        assert!(state.entries.iter().any(|e| e.id == "dataview"));
        assert!(state.entries.iter().any(|e| e.id == "calendar"));
    }

    #[test]
    fn test_catalog_search_filtering() {
        let mut state = PluginCatalogState::default();
        state.search_query = "data".to_string();
        let filtered = state.filtered_entries();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "dataview");

        state.search_query = "liamcain".to_string();
        let author_match = state.filtered_entries();
        assert_eq!(author_match.len(), 1);
        assert_eq!(author_match[0].id, "calendar");

        state.search_query = "non-existent-plugin-query".to_string();
        assert!(state.filtered_entries().is_empty());
    }

    #[test]
    fn test_catalog_sorting_orders() {
        let mut state = PluginCatalogState::default();
        state.sort_order = CatalogSortOrder::Alphabetical;
        let alpha = state.filtered_entries();
        for i in 0..alpha.len() - 1 {
            assert!(alpha[i].name.to_lowercase() <= alpha[i + 1].name.to_lowercase());
        }

        state.sort_order = CatalogSortOrder::MostPopular;
        let popular = state.filtered_entries();
        for i in 0..popular.len() - 1 {
            assert!(popular[i].downloads >= popular[i + 1].downloads);
        }
    }
}
