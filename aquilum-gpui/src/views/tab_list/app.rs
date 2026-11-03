use gpui::{px, AppContext, Context, SharedString, Window};
use crate::icons;
use crate::views::app_view::AppView;
use crate::views::tabs_menu::format_bookmark_label;
use super::model::*;
use super::popup::TabListPopup;

/// Title shown for a tab: the file stem, the localized "New tab" label for
/// empty-tab sentinels, or the "untitled" placeholder as a last resort.
#[allow(dead_code)]
fn tab_title(path: &std::path::Path, new_tab: &str, untitled: &str) -> SharedString {
    SharedString::from(crate::views::new_tab::tab_display_title(path, new_tab, untitled))
}

impl AppView {
    /// Opens the tabs popup for the currently open tabs in the active leaf.
    pub fn open_tabs_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active_leaf = self.state.workspace.active_leaf;
        self.open_leaf_tabs_popup(active_leaf, 0.0, 0.0, window, cx);
        self.tabs_popup_pos = None;
    }

    /// Opens the tabs popup for a specific leaf/window at the specified anchor coordinates.
    pub fn open_leaf_tabs_popup(
        &mut self,
        leaf_id: crate::workspace::LeafId,
        click_x: f32,
        click_y: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let leaf = match self.state.leaves.get(&leaf_id) {
            Some(l) => l,
            None => return,
        };
        if leaf.tabs.is_empty() {
            return;
        }
        self.state.workspace.active_leaf = leaf_id;

        let untitled = self.t("common.untitled");
        let new_tab = self.t("tabs.newTab");
        let tabs: Vec<TabEntry> = leaf
            .tabs
            .iter()
            .enumerate()
            .map(|(id, tab)| {
                let (title, icon) = match &tab.content {
                    crate::workspace::TabContent::Document { id } => {
                        let t = self
                            .state
                            .documents
                            .get(id)
                            .map(|d| d.title.clone())
                            .unwrap_or_else(|| untitled.clone());
                        (t, icons::ICON_FILE_TEXT)
                    }
                    crate::workspace::TabContent::Empty => (new_tab.clone(), icons::ICON_FILE_TEXT),
                    crate::workspace::TabContent::Plugin { view_id } => {
                        (view_id.clone(), icons::ICON_PLUG)
                    }
                };
                TabEntry {
                    id,
                    title: SharedString::from(title),
                    subtitle: None,
                    icon,
                    font_family: None,
                    dot_color: None,
                    trailing_icon: None,
                }
            })
            .collect();

        let active_ix = leaf.active_tab_index.min(leaf.tabs.len().saturating_sub(1));
        let theme = self.theme.clone();
        let header = TabListHeader {
            fold: SharedString::from(self.t("tabs.fold_tabs")),
            bookmark: SharedString::from(format_bookmark_label(
                leaf.tabs.len(),
                &self.config.ui.language,
            )),
            close_all: SharedString::from(self.t("tabs.close_all")),
        };

        let popup = cx.new(|cx| {
            TabListPopup::new(tabs, Some(active_ix), theme, Some(header), None, None, cx)
        });
        // `subscribe_in` is used instead of `subscribe`: activating a tab needs a window.
        let subscription =
            cx.subscribe_in(&popup, window, move |this, _popup, event: &TabListEvent, window, cx| {
                match event {
                    TabListEvent::Activate(id) => {
                        this.select_leaf_tab(leaf_id, *id, window, cx);
                        this.close_tabs_popup(cx);
                    }
                    TabListEvent::FoldTabs => {
                        this.state.status_text = this.t("tabs.fold_tabs_mode");
                        this.close_tabs_popup(cx);
                    }
                    TabListEvent::BookmarkTabs => {
                        this.state.status_text = this.t("tabs.tabs_added_to_bookmarks");
                        this.close_tabs_popup(cx);
                    }
                    TabListEvent::CloseAllTabs => {
                        this.close_leaf_all_tabs(leaf_id, window, cx);
                        this.close_tabs_popup(cx);
                    }
                    TabListEvent::Dismiss => this.close_tabs_popup(cx),
                }
            });

        window.focus(&popup.read(cx).focus_handle());
        self.tabs_popup = Some(popup);
        self.tabs_popup_leaf = Some(leaf_id);
        self.tabs_popup_pos = Some((click_x, click_y));
        self.tabs_popup_subscription = Some(subscription);
        cx.notify();
    }

    /// Closes the tabs popup, dropping its subscription with it.
    pub fn close_tabs_popup(&mut self, cx: &mut Context<Self>) {
        if self.tabs_popup.take().is_none() {
            self.tabs_popup_leaf = None;
            self.tabs_popup_pos = None;
            return;
        }
        self.tabs_popup_leaf = None;
        self.tabs_popup_pos = None;
        self.tabs_popup_subscription = None;
        cx.notify();
    }

    /// Whether the tabs popup is currently shown.
    pub fn tabs_popup_is_open(&self) -> bool {
        self.tabs_popup.is_some()
    }

    /// Opens the locale picker: the same smart popup, but with locales and
    /// no action header. The parent anchors it to the language button (see
    /// the settings UI tab), so unlike the tabs popup it needs no free-space
    /// math: the list viewport is capped to ~10 rows.
    pub fn open_locale_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::i18n::{LocaleInfo, SUPPORTED_LOCALES};

        let current = LocaleInfo::normalize(&self.config.ui.language);
        let tabs: Vec<TabEntry> = SUPPORTED_LOCALES
            .iter()
            .enumerate()
            .map(|(id, loc)| TabEntry {
                id,
                title: SharedString::from(loc.native_name),
                subtitle: Some(SharedString::from(loc.english_name)),
                icon: icons::ICON_GLOBE,
                font_family: None,
                dot_color: None,
                trailing_icon: None,
            })
            .collect();
        let active_ix = SUPPORTED_LOCALES
            .iter()
            .position(|loc| loc.code == current)
            .unwrap_or(0);
        let theme = self.theme.clone();

        let popup = cx.new(|cx| {
            TabListPopup::new(
                tabs,
                Some(active_ix),
                theme,
                None,
                Some(px(ROW_HEIGHT * 10.0)),
                Some(50.0),
                cx,
            )
        });
        let subscription = cx.subscribe_in(
            &popup,
            window,
            |this, _popup, event: &TabListEvent, _window, cx| {
                match event {
                    TabListEvent::Activate(id) => {
                        if let Some(loc) = SUPPORTED_LOCALES.get(*id) {
                            this.config.ui.language = loc.code.to_string();
                            this.save_settings(cx);
                        }
                        this.close_locale_popup(cx);
                    }
                    // The locale picker has no header actions.
                    TabListEvent::FoldTabs
                    | TabListEvent::BookmarkTabs
                    | TabListEvent::CloseAllTabs => {}
                    TabListEvent::Dismiss => this.close_locale_popup(cx),
                }
            },
        );

        window.focus(&popup.read(cx).focus_handle());
        self.locale_popup = Some(popup);
        self.locale_popup_subscription = Some(subscription);
        cx.notify();
    }

    /// Closes the locale picker, dropping its subscription with it.
    pub fn close_locale_popup(&mut self, cx: &mut Context<Self>) {
        if self.locale_popup.take().is_none() {
            return;
        }
        self.locale_popup_subscription = None;
        cx.notify();
    }

    /// Whether the locale picker is currently shown.
    pub fn locale_popup_is_open(&self) -> bool {
        self.locale_popup.is_some()
    }

    /// Opens the editor font picker: the same smart popup, each row previewed
    /// in its own font. The list is short, so no height cap is needed.
    pub fn open_font_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.config.editor.font.font_family.clone();
        let tabs: Vec<TabEntry> = crate::fonts::EDITOR_FONT_CHOICES
            .iter()
            .enumerate()
            .map(|(id, name)| TabEntry {
                id,
                title: SharedString::from(*name),
                subtitle: None,
                icon: icons::ICON_TYPE,
                font_family: Some(SharedString::from(*name)),
                dot_color: None,
                trailing_icon: None,
            })
            .collect();
        let active_ix = crate::fonts::EDITOR_FONT_CHOICES
            .iter()
            .position(|name| *name == current)
            .unwrap_or(0);
        let theme = self.theme.clone();

        let popup = cx.new(|cx| TabListPopup::new(tabs, Some(active_ix), theme, None, None, None, cx));
        let subscription = cx.subscribe_in(
            &popup,
            window,
            |this, _popup, event: &TabListEvent, _window, cx| {
                match event {
                    TabListEvent::Activate(id) => {
                        if let Some(name) = crate::fonts::EDITOR_FONT_CHOICES.get(*id) {
                            this.config.editor.font.font_family = name.to_string();
                            this.save_settings(cx);
                        }
                        this.close_font_popup(cx);
                    }
                    // The font picker has no header actions.
                    TabListEvent::FoldTabs
                    | TabListEvent::BookmarkTabs
                    | TabListEvent::CloseAllTabs => {}
                    TabListEvent::Dismiss => this.close_font_popup(cx),
                }
            },
        );

        window.focus(&popup.read(cx).focus_handle());
        self.font_popup = Some(popup);
        self.font_popup_subscription = Some(subscription);
        cx.notify();
    }

    /// Closes the font picker, dropping its subscription with it.
    pub fn close_font_popup(&mut self, cx: &mut Context<Self>) {
        if self.font_popup.take().is_none() {
            return;
        }
        self.font_popup_subscription = None;
        cx.notify();
    }

    /// Whether the font picker is currently shown.
    pub fn font_popup_is_open(&self) -> bool {
        self.font_popup.is_some()
    }

    /// Opens the plugin select picker: the same smart popup, anchored to the select button.
    pub fn open_plugin_select(
        &mut self,
        plugin_id: String,
        view_id: u64,
        node_id: String,
        options: Vec<crate::plugins::ui::SelectOption>,
        current_value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active_ix = options.iter().position(|o| o.value == current_value).unwrap_or(0);
        let tabs: Vec<TabEntry> = options
            .iter()
            .enumerate()
            .map(|(id, opt)| TabEntry {
                id,
                title: SharedString::from(opt.label.clone()),
                subtitle: None,
                icon: icons::ICON_CHEVRON_RIGHT,
                font_family: None,
                dot_color: None,
                trailing_icon: None,
            })
            .collect();

        let theme = self.theme.clone();
        let max_rows = options.len().clamp(1, 10) as f32;
        let popup = cx.new(|cx| {
            TabListPopup::new(
                tabs,
                Some(active_ix),
                theme,
                None,
                Some(px(ROW_HEIGHT * max_rows)),
                Some(50.0),
                cx,
            )
        });

        let p_id_activate = plugin_id.clone();
        let n_id_activate = node_id.clone();
        let opts_activate = options.clone();

        let subscription = cx.subscribe_in(
            &popup,
            window,
            move |this, _popup, event: &TabListEvent, _window, cx| {
                match event {
                    TabListEvent::Activate(id) => {
                        if let Some(opt) = opts_activate.get(*id) {
                            let chosen_val = opt.value.clone();
                            if let Some(tree) = this.plugin_views.get_mut(&(p_id_activate.clone(), view_id)) {
                                tree.update_input_value(&n_id_activate, &chosen_val);
                            }
                            let _ = this.state.plugin_host.send_event(
                                &p_id_activate,
                                crate::plugins::api::HostEvent::UiEvent {
                                    view_id,
                                    node_id: n_id_activate.clone(),
                                    event: "change".to_string(),
                                    value: Some(chosen_val),
                                },
                            );
                        }
                        this.close_plugin_select(cx);
                    }
                    TabListEvent::Dismiss => this.close_plugin_select(cx),
                    _ => {}
                }
            },
        );

        window.focus(&popup.read(cx).focus_handle());
        self.active_plugin_select = Some((plugin_id, view_id, node_id));
        self.plugin_select_popup = Some(popup);
        self.plugin_select_subscription = Some(subscription);
        cx.notify();
    }

    /// Closes the plugin select picker, dropping its subscription.
    pub fn close_plugin_select(&mut self, cx: &mut Context<Self>) {
        if self.plugin_select_popup.take().is_none() && self.active_plugin_select.take().is_none() {
            return;
        }
        self.active_plugin_select = None;
        self.plugin_select_subscription = None;
        cx.notify();
    }

    /// Whether the plugin select picker is currently shown for this node.
    pub fn is_plugin_select_open(&self, plugin_id: &str, view_id: u64, node_id: &str) -> bool {
        self.active_plugin_select.as_ref() == Some(&(plugin_id.to_string(), view_id, node_id.to_string()))
            && self.plugin_select_popup.is_some()
    }
}
