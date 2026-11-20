use std::cell::RefCell;
use gpui::{
    div, prelude::*, px, Context, IntoElement, MouseButton, MouseDownEvent,
    Styled, Window,
};
use crate::document_buffer::{DocumentBuffer, DocumentId};
use crate::icons;
use crate::theme::Theme;
use crate::views::app_view::AppView;

/// One heading shown in the right-sidebar outline panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlineItem {
    pub level: usize,
    pub title: String,
    pub line: usize,
}

/// Outline panel state: whether it is shown plus the heading cache of the
/// active note, re-scanned only when the buffer version changes.
#[derive(Default)]
pub struct OutlineState {
    pub open: bool,
    pub cache: RefCell<Option<(DocumentId, u64, Vec<OutlineItem>)>>,
}

impl AppView {
    /// Toggles the collapsed state of the right sidebar.
    pub fn toggle_right_sidebar(&mut self, cx: &mut Context<Self>) {
        self.right_sidebar_collapsed = !self.right_sidebar_collapsed;
        cx.notify();
    }

    /// Shows or hides the outline panel, expanding the sidebar when shown.
    pub fn toggle_outline(&mut self, cx: &mut Context<Self>) {
        self.outline.open = !self.outline.open;
        if self.outline.open {
            self.right_sidebar_collapsed = false;
        }
        cx.notify();
    }

    /// Headings of `doc` for the outline panel; the scan is cached by `(id, version)`.
    pub fn outline_items(&self, doc: &DocumentBuffer) -> Vec<OutlineItem> {
        if let Some((id, version, items)) = self.outline.cache.borrow().as_ref() {
            if *id == doc.id && *version == doc.version {
                return items.clone();
            }
        }
        let items = collect_outline(doc);
        *self.outline.cache.borrow_mut() = Some((doc.id, doc.version, items.clone()));
        items
    }

    /// Renders the outline toggle in the right-sidebar header; it matches a leaf tab.
    fn render_outline_toggle_button(&self, theme: &Theme, cx: &Context<Self>) -> impl IntoElement {
        let is_open = self.outline.open;
        div()
            .id("btn-right-sidebar-outline")
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.0))
            .px(px(6.0))
            .py(px(3.0))
            .flex_shrink_0()
            .rounded(px(4.0))
            .cursor_pointer()
            .when(is_open, |d| d.bg(theme.bg_card))
            .when(!is_open, |d| d.hover(|h| h.bg(theme.bg_hover)))
            .child(icons::icon(
                icons::ICON_LIST,
                13.0,
                if is_open { theme.accent } else { theme.text_muted },
            ))
            .child(
                div()
                    .text_xs()
                    .text_color(if is_open {
                        theme.text_primary
                    } else {
                        theme.text_muted
                    })
                    .child(self.t("app.outline.title")),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.toggle_outline(cx);
            }))
    }

    /// Renders the active note's headings; a row click moves the caret there.
    fn render_outline_panel(&self, theme: &Theme, cx: &Context<Self>) -> impl IntoElement {
        let items = self
            .state
            .active_document()
            .map(|doc| self.outline_items(doc))
            .unwrap_or_default();

        let mut panel = div()
            .id("right-sidebar-outline")
            .flex()
            .flex_col()
            .flex_1()
            .overflow_y_scroll()
            .p(px(4.0))
            .rounded(px(4.0))
            .bg(theme.bg_card)
            .child(
                div()
                    .h(px(22.0))
                    .flex()
                    .items_center()
                    .px_2p5()
                    .text_xs()
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(theme.text_muted)
                    .child(self.t("app.outline.title")),
            );

        if items.is_empty() {
            panel = panel.child(
                div()
                    .px_2p5()
                    .py_1()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(self.t("app.outline.empty")),
            );
        }

        for item in items {
            let line = item.line;
            let indent = 6.0 + item.level.saturating_sub(1) as f32 * 12.0;
            panel = panel.child(
                div()
                    .id(gpui::ElementId::from(gpui::SharedString::from(format!(
                        "outline-item-{line}"
                    ))))
                    .flex()
                    .items_center()
                    .pl(px(indent))
                    .pr_2()
                    .py(px(4.0))
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|h| h.bg(theme.bg_hover))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.jump_to_doc_line(line, cx);
                        window.focus(&this.focus_handle);
                        this.reset_editor_cursor_blink(cx);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_xs()
                            .text_color(theme.text_secondary)
                            .child(item.title.clone()),
                    ),
            );
        }

        panel
    }

    /// Renders the 1px resizer divider between the editor and right sidebar with interactive hitbox.
    pub fn render_right_resizer(&self, theme: &Theme, cx: &Context<Self>) -> impl IntoElement {
        let is_resizer_active = self.is_resizing_right_sidebar || self.is_hovering_right_resizer;
        let resizer_color = if is_resizer_active {
            theme.accent
        } else {
            theme.border
        };

        div()
            .id("right-sidebar-resizer")
            .relative()
            .flex_shrink_0()
            .w(px(1.0))
            .h_full()
            .bg(resizer_color)
            .child(
                div()
                    .id("right-sidebar-resizer-hitbox")
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(-3.0))
                    .w(px(7.0))
                    .cursor(gpui::CursorStyle::ResizeColumn)
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        this.is_hovering_right_resizer = *hovered;
                        cx.notify();
                    }))
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                        if event.click_count == 2 {
                            this.toggle_right_sidebar(cx);
                        } else {
                            this.is_resizing_right_sidebar = true;
                            window.set_custom_drag_cursor_style(Some(gpui::CursorStyle::ResizeColumn), cx);
                            cx.notify();
                        }
                    })),
            )
    }

    /// Renders the right sidebar container (Obsidian-style).
    pub fn render_right_sidebar(&self, theme: &Theme, window: &Window, cx: &Context<Self>) -> impl IntoElement {
        let mut container = div()
            .id("right-sidebar")
            .flex()
            .flex_col()
            .w(px(self.right_sidebar_width.clamp(160.0, 600.0)))
            .flex_shrink_0()
            .overflow_hidden()
            .h_full()
            .bg(theme.bg_sidebar)
            // The adjacent `right-sidebar-resizer` already draws this border;
            // drawing another one here would double the separator (and stay
            // gray while the resizer turns accent).
            .p(px(8.0))
            .gap(px(6.0));

        let right_leaves: Vec<&crate::plugins::api::PluginLeafView> = self
            .plugin_leaf_views
            .values()
            .filter(|l| l.location == "right" || l.location.is_empty())
            .collect();

        // 1. Header: built-in outline toggle plus right-leaf tabs.
        let mut tab_bar = div()
            .id("right-sidebar-tabs")
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.0))
            .pb(px(4.0))
            .border_b_1()
            .border_color(theme.border);

        tab_bar = tab_bar.child(self.render_outline_toggle_button(theme, cx));

        let active_leaf = if right_leaves.is_empty() {
            None
        } else {
            let active_leaf_id = self
                .active_right_leaf
                .as_deref()
                .unwrap_or(&right_leaves[0].leaf_id);

            let active_leaf = right_leaves
                .iter()
                .find(|l| l.leaf_id == active_leaf_id)
                .copied()
                .unwrap_or(right_leaves[0]);

            for leaf in &right_leaves {
                let is_active = !self.outline.open && leaf.leaf_id == active_leaf.leaf_id;
                let leaf_id_select = leaf.leaf_id.clone();
                let leaf_id_close = leaf.leaf_id.clone();
                let icon_str = leaf.icon.as_deref().unwrap_or("layout");
                let icon_asset = crate::icons::map_obsidian_icon(icon_str);
                let title = leaf.title.clone();

                tab_bar = tab_bar.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(4.0))
                        .px(px(6.0))
                        .py(px(3.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .when(is_active, |d| d.bg(theme.bg_card))
                        .when(!is_active, |d| d.hover(|h| h.bg(theme.bg_hover)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                this.outline.open = false;
                                this.select_right_leaf(&leaf_id_select, cx);
                            }),
                        )
                        .child(crate::icons::icon(
                            icon_asset,
                            13.0,
                            if is_active {
                                theme.accent
                            } else {
                                theme.text_muted
                            },
                        ))
                        .child(
                            div()
                                .text_xs()
                                .max_w(px(120.0))
                                .overflow_hidden()
                                .text_color(if is_active {
                                    theme.text_primary
                                } else {
                                    theme.text_muted
                                })
                                .child(title),
                        )
                        .child(
                            div()
                                .px(px(2.0))
                                .rounded(px(2.0))
                                .text_xs()
                                .text_color(theme.text_muted)
                                .hover(|h| h.bg(theme.bg_hover).text_color(theme.text_primary))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.close_plugin_leaf(&leaf_id_close, cx);
                                    }),
                                )
                                .child("×"),
                        ),
                );
            }

            Some(active_leaf)
        };

        container = container.child(tab_bar);

        if self.outline.open {
            container = container.child(self.render_outline_panel(theme, cx));
        } else if let Some(active_leaf) = active_leaf {
            // 2. Action buttons header if active leaf has actions
            if !active_leaf.actions.is_empty() {
                let mut actions_bar = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_end()
                    .gap(px(4.0))
                    .py(px(2.0));

                for act in &active_leaf.actions {
                    let act_id = act.id.clone();
                    let act_plugin_id = active_leaf.plugin_id.clone();
                    let act_icon = crate::icons::map_obsidian_icon(&act.icon);
                    actions_bar = actions_bar.child(
                        div()
                            .p(px(3.0))
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    this.trigger_plugin_action(&act_plugin_id, &act_id, cx);
                                }),
                            )
                            .child(crate::icons::icon(act_icon, 13.0, theme.text_muted)),
                    );
                }
                container = container.child(actions_bar);
            }

            // 3. Active leaf view content
            container = container.child(
                div()
                    .id("right-sidebar-leaf-content")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_scroll()
                    .p(px(4.0))
                    .rounded(px(4.0))
                    .bg(theme.bg_card)
                    .child(crate::plugins::ui::render_ui_node(
                        &active_leaf.tree,
                        &active_leaf.plugin_id,
                        0,
                        theme,
                        self.active_plugin_input.as_ref(),
                        &self.plugin_input_focus_handle,
                        self.active_plugin_select.as_ref(),
                        self.plugin_select_popup.as_ref(),
                        self.plugin_input_blink.visible(),
                        window,
                        cx,
                        None,
                    )),
            );
        } else {
            container = container.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .h_full()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child("No active plugin views"),
            );
        }

        container
    }
}

/// Parses one line as an ATX heading; returns its level, plain-text title and line number.
fn parse_outline_line(line: &str, line_no: usize) -> Option<OutlineItem> {
    let trimmed = line.trim_start();
    let level = crate::formatting::detect_heading_level(trimmed)?;
    let mut body = trimmed.get(level + 1..).unwrap_or("").trim_end();
    // A trailing `###` closes the heading only when it stands on its own;
    // otherwise it is part of the text (so `# C#` keeps the sharp).
    if body.ends_with('#') {
        let no_hashes = body.trim_end_matches('#');
        if no_hashes.chars().last().is_some_and(|c| c.is_whitespace()) {
            body = no_hashes.trim_end();
        }
    }
    let title = crate::markdown::strip_inline_markdown(body.trim());
    if title.trim().is_empty() {
        return None;
    }
    Some(OutlineItem {
        level,
        title,
        line: line_no,
    })
}

/// Collects the heading outline of `doc`, skipping fenced code blocks.
fn collect_outline(doc: &DocumentBuffer) -> Vec<OutlineItem> {
    let mut items = Vec::new();
    let mut in_fence = false;
    for line_no in 0..doc.line_index.lines() {
        let text = doc.line_text(line_no);
        let trimmed = text.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if let Some(item) = parse_outline_line(trimmed, line_no) {
            items.push(item);
        }
    }
    items
}

#[cfg(test)]
mod tests;
