use std::path::{Path, PathBuf};

use gpui::{div, prelude::*, px, Context, IntoElement, MouseButton, Styled, Window};

use crate::icons;
use crate::theme::Theme;
use crate::views::app_view::AppView;

/// State for the Obsidian-style editor tab context menu.
///
/// Opened with a right click on a tab in the tab strip. Remembers which tab
/// was clicked so actions apply to it even if it is not the active one.
#[derive(Clone, Debug)]
pub struct TabContextMenuState {
    pub is_open: bool,
    pub tab_path: PathBuf,
    pub leaf_id: Option<crate::workspace::LeafId>,
    pub tab_idx: Option<usize>,
    pub position_x: f32,
    pub position_y: f32,
}

impl Default for TabContextMenuState {
    fn default() -> Self {
        Self {
            is_open: false,
            tab_path: PathBuf::new(),
            leaf_id: None,
            tab_idx: None,
            position_x: 0.0,
            position_y: 0.0,
        }
    }
}

impl AppView {
    /// Opens the tab context menu for `tab_path` at the given coordinates.
    pub fn open_tab_context_menu(
        &mut self,
        tab_path: PathBuf,
        leaf_id: Option<crate::workspace::LeafId>,
        tab_idx: Option<usize>,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        // Only one context menu at a time.
        self.file_context_menu.is_open = false;
        self.folder_context_menu.is_open = false;
        self.close_tabs_popup(cx);

        self.tab_context_menu = TabContextMenuState {
            is_open: true,
            tab_path,
            leaf_id,
            tab_idx,
            position_x: x,
            position_y: y,
        };
        cx.notify();
    }

    /// Closes the tab context menu.
    pub fn close_tab_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.tab_context_menu.is_open {
            self.tab_context_menu.is_open = false;
            cx.notify();
        }
    }

    /// Closes every open tab except `keep`, switching to it first if needed.
    pub fn close_other_tabs(&mut self, keep: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let active_leaf_id = self.state.workspace.active_leaf;
        if let Some(leaf) = self.state.leaves.get_mut(&active_leaf_id) {
            let keep_idx = leaf.tabs.iter().position(|t| match &t.content {
                crate::workspace::TabContent::Document { id } => {
                    self.state.documents.get(id).map(|d| d.path.as_path() == keep).unwrap_or(false)
                }
                _ => false,
            });
            if let Some(keep_i) = keep_idx {
                let keep_tab = leaf.tabs.remove(keep_i);
                leaf.tabs.clear();
                leaf.tabs.push(keep_tab);
                leaf.active_tab_index = 0;
                self.state.cleanup_orphaned_documents_and_views();
                self.select_leaf_tab(active_leaf_id, 0, window, cx);
                cx.notify();
                return;
            }
        }

        if !self.state.open_tabs.iter().any(|p| p == keep) {
            return;
        }
        let keep_path = keep.to_path_buf();
        let needs_switch = self
            .state
            .active_document()
            .map(|d| d.path != keep_path)
            .unwrap_or(true);
        if needs_switch {
            self.select_note(keep_path.clone(), false, window, cx);
        }
        let to_close: Vec<PathBuf> = self
            .state
            .open_tabs
            .iter()
            .filter(|p| *p != &keep_path)
            .cloned()
            .collect();
        for path in to_close {
            self.close_tab(&path, cx);
        }
    }

    /// Closes all tabs strictly to the right of `path` in the tab strip order.
    pub fn close_tabs_to_right(&mut self, path: &Path, cx: &mut Context<Self>) {
        let active_leaf_id = self.state.workspace.active_leaf;
        if let Some(leaf) = self.state.leaves.get_mut(&active_leaf_id) {
            let keep_idx = leaf.tabs.iter().position(|t| match &t.content {
                crate::workspace::TabContent::Document { id } => {
                    self.state.documents.get(id).map(|d| d.path.as_path() == path).unwrap_or(false)
                }
                _ => false,
            });
            if let Some(ix) = keep_idx {
                leaf.tabs.truncate(ix + 1);
                if leaf.active_tab_index > ix {
                    leaf.active_tab_index = ix;
                }
                self.state.cleanup_orphaned_documents_and_views();
                cx.notify();
                return;
            }
        }

        let Some(ix) = self.state.open_tabs.iter().position(|p| p == path) else {
            return;
        };
        let to_close: Vec<PathBuf> = self.state.open_tabs.iter().skip(ix + 1).cloned().collect();
        for tab in to_close {
            self.close_tab(&tab, cx);
        }
    }

    pub fn split_tab_right(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        self.split_tab_in_direction(path, crate::workspace::SplitDirection::Row, window, cx);
    }

    pub fn split_tab_down(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        self.split_tab_in_direction(path, crate::workspace::SplitDirection::Column, window, cx);
    }

    pub fn split_tab_in_direction(
        &mut self,
        path: &Path,
        dir: crate::workspace::SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active_leaf = self.tab_context_menu.leaf_id.unwrap_or(self.state.workspace.active_leaf);
        let is_empty = crate::state::is_empty_tab_path(path)
            || path.to_string_lossy() == "newtab"
            || self.tab_context_menu.leaf_id.and_then(|lid| self.state.leaves.get(&lid))
                .and_then(|l| self.tab_context_menu.tab_idx.and_then(|idx| l.tabs.get(idx)))
                .map(|t| matches!(t.content, crate::workspace::TabContent::Empty))
                .unwrap_or(false);

        if is_empty {
            let new_leaf_id = crate::workspace::LeafId::new();
            let mut new_leaf = crate::workspace::LeafState::new(new_leaf_id);
            new_leaf.tabs.push(crate::workspace::Tab::new_empty());
            new_leaf.active_tab_index = 0;
            self.state.workspace.split_leaf(active_leaf, dir, new_leaf_id);
            self.state.leaves.insert(new_leaf_id, new_leaf);
            self.state.invalidate_layout_caches();
            self.layout_settle_frames.set(crate::views::app_view::SCROLL_SETTLE_FRAMES);
            self.switch_to_leaf(new_leaf_id, window, cx);
            return;
        }

        let doc_id = if let Some(id) = self.state.find_document_by_path(path) {
            id
        } else {
            let title = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled".to_string());
            let content = std::fs::read_to_string(path).unwrap_or_default();
            self.state.get_or_open_document(path, title, content)
        };

        let outcome = self.state.workspace.open_file(
            &mut self.state.leaves,
            doc_id,
            active_leaf,
            crate::workspace::OpenMode::OpenInDirection(dir),
        );

        if let crate::workspace::OpenOutcome::SplitCreated { new_leaf, tab_id } = outcome {
            let mut view_state = crate::workspace::EditorViewState::new(tab_id, doc_id);
            if let (Some(active_view), Some(active_doc)) = (self.state.active_view(), self.state.active_document()) {
                if active_doc.path == path {
                    view_state = active_view.clone();
                    view_state.tab_id = tab_id;
                    view_state.doc_id = doc_id;
                    view_state.title_input = None;
                    view_state.measured_viewport_h = None;
                }
            }
            self.state.views.insert(tab_id, view_state);
            self.state.invalidate_layout_caches();
            self.layout_settle_frames.set(crate::views::app_view::SCROLL_SETTLE_FRAMES);
            self.switch_to_leaf(new_leaf, window, cx);
        }
    }

    /// Renders one menu row: icon on the left, label, hover highlight.
    fn render_tab_menu_item(
        &self,
        id: &'static str,
        icon_path: &'static str,
        label: String,
        theme: &Theme,
        cx: &Context<Self>,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> impl IntoElement {
        div()
            .id(id)
            .h(px(30.0))
            .flex()
            .flex_row()
            .items_center()
            .px_2p5()
            .rounded_md()
            .cursor_pointer()
            .hover(|h| h.bg(theme.bg_hover))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2p5()
                    .child(icons::icon(icon_path, 14.0, theme.text_secondary))
                    .child(div().text_sm().text_color(theme.text_primary).child(label)),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                on_click(this, window, cx);
            }))
    }

    /// Renders the Obsidian-like tab context menu overlay at the click position.
    pub fn render_tab_context_menu(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let window_size = window.bounds().size;
        let window_h: f32 = window_size.height.into();
        let window_w: f32 = window_size.width.into();

        // 9 rows x 30px + 3 dividers (~9px each) + panel padding (~8px).
        let menu_w = 270.0;
        let menu_h = 312.0;

        let click_y = self.tab_context_menu.position_y;
        let menu_y = if click_y + menu_h + 36.0 > window_h {
            (click_y - menu_h + 13.0).max(8.0)
        } else {
            click_y.max(8.0)
        };

        let mut menu_x = self.tab_context_menu.position_x.max(8.0);
        if menu_x + menu_w > window_w - 12.0 {
            menu_x = (window_w - menu_w - 12.0).max(8.0);
        }

        let target_path = self.tab_context_menu.tab_path.clone();

        div()
            .id("tab-context-menu-overlay")
            .absolute()
            .inset_0()
            .cursor(gpui::CursorStyle::Arrow)
            .on_mouse_move(|_, _, cx| {
                cx.stop_propagation();
            })
            .on_scroll_wheel(|_, _, cx| {
                cx.stop_propagation();
            })
            // Invisible full-screen backdrop to dismiss the menu on click.
            .child(
                div()
                    .id("tab-context-backdrop")
                    .absolute()
                    .inset_0()
                    .cursor(gpui::CursorStyle::Arrow)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.close_tab_context_menu(cx);
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, _, _, cx| {
                            this.close_tab_context_menu(cx);
                        }),
                    )
                    .on_mouse_move(|_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_scroll_wheel(|_, _, cx| {
                        cx.stop_propagation();
                    }),
            )
            // Popup panel, visually matching the screenshot: rounded, bordered, shadowed.
            .child(
                div()
                    .id("tab-context-panel")
                    .absolute()
                    .left(px(menu_x))
                    .top(px(menu_y))
                    .w(px(menu_w))
                    .bg(theme.bg_sidebar)
                    .border_1()
                    .border_color(theme.border)
                    .rounded_lg()
                    .shadow_xl()
                    .p_1p5()
                    .flex()
                    .flex_col()
                    .cursor(gpui::CursorStyle::Arrow)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_mouse_down(MouseButton::Right, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_scroll_wheel(|_, _, cx| {
                        cx.stop_propagation();
                    })
                    // --- SECTION 1: closing ---
                    .child(self.render_tab_menu_item(
                        "tab-menu-close",
                        icons::ICON_CLOSE,
                        self.t("tabs.close"),
                        theme,
                        cx,
                        {
                            let p = target_path.clone();
                            let opt_leaf = self.tab_context_menu.leaf_id;
                            let opt_idx = self.tab_context_menu.tab_idx;
                            move |this, window, cx| {
                                if let (Some(lid), Some(idx)) = (opt_leaf, opt_idx) {
                                    this.close_leaf_tab(lid, idx, window, cx);
                                } else {
                                    this.close_tab(&p, cx);
                                }
                                this.close_tab_context_menu(cx);
                            }
                        },
                    ))
                    .child(self.render_tab_menu_item(
                        "tab-menu-close-others",
                        icons::ICON_CLOSE,
                        self.t("tabs.close_others"),
                        theme,
                        cx,
                        {
                            let p = target_path.clone();
                            let opt_leaf = self.tab_context_menu.leaf_id;
                            let opt_idx = self.tab_context_menu.tab_idx;
                            move |this, window, cx| {
                                if let (Some(lid), Some(idx)) = (opt_leaf, opt_idx) {
                                    if let Some(leaf) = this.state.leaves.get_mut(&lid) {
                                        if idx < leaf.tabs.len() {
                                            let keep_tab = leaf.tabs.remove(idx);
                                            leaf.tabs.clear();
                                            leaf.tabs.push(keep_tab);
                                            leaf.active_tab_index = 0;
                                            this.state.cleanup_orphaned_documents_and_views();
                                            this.select_leaf_tab(lid, 0, window, cx);
                                            this.close_tab_context_menu(cx);
                                            return;
                                        }
                                    }
                                }
                                this.close_other_tabs(&p, window, cx);
                                this.close_tab_context_menu(cx);
                            }
                        },
                    ))
                    .child(self.render_tab_menu_item(
                        "tab-menu-close-right",
                        icons::ICON_CLOSE,
                        self.t("tabs.close_to_right"),
                        theme,
                        cx,
                        {
                            let p = target_path.clone();
                            let opt_leaf = self.tab_context_menu.leaf_id;
                            let opt_idx = self.tab_context_menu.tab_idx;
                            move |this, window, cx| {
                                if let (Some(lid), Some(idx)) = (opt_leaf, opt_idx) {
                                    let active_to_select = if let Some(leaf) = this.state.leaves.get_mut(&lid) {
                                        if idx + 1 < leaf.tabs.len() {
                                            leaf.tabs.truncate(idx + 1);
                                            if leaf.active_tab_index > idx {
                                                leaf.active_tab_index = idx;
                                            }
                                            Some(leaf.active_tab_index)
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    };
                                    this.state.cleanup_orphaned_documents_and_views();
                                    if let Some(target_idx) = active_to_select {
                                        this.select_leaf_tab(lid, target_idx, window, cx);
                                        this.close_tab_context_menu(cx);
                                        return;
                                    }
                                }
                                this.close_tabs_to_right(&p, cx);
                                this.close_tab_context_menu(cx);
                            }
                        },
                    ))
                    .child(self.render_tab_menu_item(
                        "tab-menu-close-all",
                        icons::ICON_CLOSE,
                        self.t("tabs.close_all"),
                        theme,
                        cx,
                        {
                            let opt_leaf = self.tab_context_menu.leaf_id;
                            move |this, window, cx| {
                                if let Some(lid) = opt_leaf {
                                    this.close_leaf_all_tabs(lid, window, cx);
                                } else {
                                    this.close_all_tabs(cx);
                                }
                                this.close_tab_context_menu(cx);
                            }
                        },
                    ))
                    .child(self.render_menu_divider(theme))
                    // --- SECTION 1b: splitting ---
                    .child(self.render_tab_menu_item(
                        "tab-menu-split-right",
                        icons::ICON_SPLIT,
                        self.t("tabs.split_right"),
                        theme,
                        cx,
                        {
                            let p = target_path.clone();
                            move |this, window, cx| {
                                this.split_tab_right(&p, window, cx);
                                this.close_tab_context_menu(cx);
                            }
                        },
                    ))
                    .child(self.render_tab_menu_item(
                        "tab-menu-split-down",
                        icons::ICON_SPLIT,
                        self.t("tabs.split_down"),
                        theme,
                        cx,
                        {
                            let p = target_path.clone();
                            move |this, window, cx| {
                                this.split_tab_down(&p, window, cx);
                                this.close_tab_context_menu(cx);
                            }
                        },
                    ))
                    .child(self.render_menu_divider(theme))
                    // --- SECTION 2: pin & link ---
                    .child(self.render_tab_menu_item(
                        "tab-menu-pin",
                        icons::ICON_PIN,
                        self.t("tabs.pin"),
                        theme,
                        cx,
                        {
                            let p = target_path.clone();
                            move |this, _, cx| {
                                let name = p
                                    .file_stem()
                                    .map(|s| s.to_string_lossy().to_string())
                                    .unwrap_or_else(|| this.t("common.untitled"));
                                this.state.status_text = this.t_fmt("tabs.pinned_status", &[&name]);
                                this.close_tab_context_menu(cx);
                            }
                        },
                    ))
                    .child(self.render_tab_menu_item(
                        "tab-menu-link",
                        icons::ICON_LINK,
                        self.t("tabs.link_tab"),
                        theme,
                        cx,
                        move |this, _, cx| {
                            this.state.status_text = this.t("tabs.link_tab_status");
                            this.close_tab_context_menu(cx);
                        },
                    ))
                    .child(self.render_menu_divider(theme))
                    // --- SECTION 3: new window ---
                    .child(self.render_tab_menu_item(
                        "tab-menu-new-window",
                        icons::ICON_APP_WINDOW,
                        self.t("tabs.move_to_new_window"),
                        theme,
                        cx,
                        move |this, _, cx| {
                            this.state.status_text = this.t("tabs.new_window_status");
                            this.close_tab_context_menu(cx);
                        },
                    )),
            )
    }
}
