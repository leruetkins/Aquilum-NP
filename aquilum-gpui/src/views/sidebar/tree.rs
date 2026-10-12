//! Virtualized file-tree component of the sidebar.
//!
//! A vault can hold thousands of notes, so the tree is never rendered as a tree of
//! elements. Once per frame [`AppView::render_sidebar_tree`] flattens it into a list of
//! [`SidebarRow`] values (respecting collapsed folders and the active search filter),
//! wraps that list in a [`VisualRowHeightIndex`], and lays out only the rows that
//! intersect the viewport — bracketed by two spacers that keep the scrollable content as
//! tall as the whole list, so the scrollbar stays proportional.

use std::path::PathBuf;

use gpui::{div, point, prelude::*, px, AnyElement, Context, MouseButton, SharedString, Window};

use crate::icons;
use crate::markdown::VisualRowHeightIndex;
use crate::state::FileNode;
use crate::theme::Theme;
use crate::views::app_view::AppView;

/// Fixed height of one sidebar tree row, in pixels.
///
/// The file tree is virtualized: the whole tree is flattened into a list of rows once per
/// frame and only the rows intersecting the viewport are laid out. A fixed row height
/// makes the geometry a plain stride — row `i` occupies
/// `[i * SIDEBAR_ROW_H, (i + 1) * SIDEBAR_ROW_H)` — and keeps the two spacer heights
/// exact, so the scrollbar still reflects the full list.
///
/// The value reproduces the row stride used before virtualization: the label line box
/// (`text_xs` = 12px at gpui's default `phi()` line height -> 19px) plus 8px of vertical
/// padding (`py_1`), plus the 2px the container gap used to add. Text rhythm and total
/// content height are therefore unchanged, while the explicit height makes the geometry
/// exact instead of relying on text metrics.
const SIDEBAR_ROW_H: f32 = 29.0;

/// Pixels of rows rendered above and below the viewport, so fast scrolling does not
/// expose blank edges before the next frame catches up.
const SIDEBAR_OVERSCAN_PX: f32 = 300.0;

/// One flattened line of the sidebar file tree.
///
/// Built by [`AppView::flatten_sidebar_tree`] and turned into an element by
/// [`AppView::render_sidebar_row`]. The tree is flattened instead of walked recursively
/// during render, so collapsed folders and the active search filter cost nothing beyond
/// this cheap data row.
struct SidebarRow {
    is_folder: bool,
    /// Folders only: whether the folder is currently expanded.
    is_expanded: bool,
    /// Files only: whether this row is the active note.
    is_selected: bool,
    name: String,
    path: PathBuf,
    depth: usize,
}

impl AppView {
    /// Renders the scrollable, virtualized file tree.
    ///
    /// Returns the whole `#sidebar-tree-scroll` container: the caller only decides where
    /// it goes. Everything else — flattening, resolving the scroll offset, and slicing the
    /// visible range — happens here.
    pub(crate) fn render_sidebar_tree(
        &self,
        active_path: Option<&PathBuf>,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let mut sidebar_rows: Vec<SidebarRow> = Vec::new();
        let mut selected_row_index: Option<usize> = None;
        self.flatten_sidebar_tree(
            &self.state.file_tree,
            0,
            active_path,
            false,
            &mut sidebar_rows,
            &mut selected_row_index,
        );

        // Uniform row height turns the geometry into a plain stride:
        // row `i` spans [i * SIDEBAR_ROW_H, (i + 1) * SIDEBAR_ROW_H).
        let height_index = VisualRowHeightIndex::new(vec![SIDEBAR_ROW_H; sidebar_rows.len()]);

        // Real viewport height, so the visible slice is exact rather than guessed from the
        // window height. Falls back to the old estimate on the very first frame, before the
        // tree has been laid out.
        let sidebar_viewport_h = {
            let h = f32::from(self.sidebar_scroll_handle.bounds().size.height);
            if h > 0.0 {
                h
            } else {
                (f32::from(window.bounds().size.height) - 144.0).max(100.0)
            }
        };

        // Current scroll position measured from the top of the list (>= 0).
        let mut sidebar_scroll_top = -f32::from(self.sidebar_scroll_handle.offset().y);

        // Resolve programmatic scroll requests *before* computing the visible range, so the
        // slice we render matches the offset we are about to apply. Virtualization shifts
        // the child indices (two spacers are inserted), so `scroll_to_top_of_item` can no
        // longer be used — the target offset is computed from the height index directly.
        let mut scroll_request = false;
        if !self.initial_sidebar_scroll_done.get() {
            self.initial_sidebar_scroll_done.set(true);
            if self.state.session.sidebar_scroll_top < 0.0 {
                sidebar_scroll_top = -self.state.session.sidebar_scroll_top;
                scroll_request = true;
            } else if let Some(ix) = selected_row_index {
                let half = ((sidebar_viewport_h / SIDEBAR_ROW_H) as usize / 2).max(1);
                sidebar_scroll_top = height_index.top_of(ix.saturating_sub(half));
                scroll_request = true;
            }
        } else if self.need_scroll_sidebar_to_active.get() {
            self.need_scroll_sidebar_to_active.set(false);
            if let Some(ix) = selected_row_index {
                let half = ((sidebar_viewport_h / SIDEBAR_ROW_H) as usize / 2).max(1);
                sidebar_scroll_top = height_index.top_of(ix.saturating_sub(half));
                scroll_request = true;
            }
        }

        if scroll_request {
            // `max_offset` stays 0 until the tree has been laid out once, so only clamp
            // against it when it is meaningful; GPUI's own prepaint clamps the offset
            // against the real content size anyway.
            let max_y = f32::from(self.sidebar_scroll_handle.max_offset().height);
            sidebar_scroll_top = if max_y > 0.0 {
                sidebar_scroll_top.clamp(0.0, max_y)
            } else {
                sidebar_scroll_top.max(0.0)
            };
            self.sidebar_scroll_handle
                .set_offset(point(px(0.0), px(-sidebar_scroll_top)));
        }

        let visible_rows = height_index.visible_range(sidebar_scroll_top, sidebar_viewport_h, SIDEBAR_OVERSCAN_PX);
        let top_spacer_h = height_index.top_of(visible_rows.start);
        let bottom_spacer_h = (height_index.total() - height_index.top_of(visible_rows.end)).max(0.0);

        // The two spacers keep the scrollable content as tall as the full list, so the
        // scrollbar stays proportional to the whole tree.
        let mut tree_elements: Vec<AnyElement> = Vec::with_capacity(visible_rows.len() + 2);
        if top_spacer_h > 0.0 {
            tree_elements.push(
                div()
                    .id("sidebar-tree-top-spacer")
                    .w_full()
                    .h(px(top_spacer_h))
                    .flex_shrink_0()
                    .into_any_element(),
            );
        }
        for ix in visible_rows {
            tree_elements.push(self.render_sidebar_row(&sidebar_rows[ix], theme, cx));
        }
        if bottom_spacer_h > 0.0 {
            tree_elements.push(
                div()
                    .id("sidebar-tree-bottom-spacer")
                    .w_full()
                    .h(px(bottom_spacer_h))
                    .flex_shrink_0()
                    .into_any_element(),
            );
        }

        div()
            .id("sidebar-tree-scroll")
            .track_scroll(&self.sidebar_scroll_handle)
            .flex()
            .flex_col()
            .flex_1()
            .overflow_scroll()
            .children(tree_elements)
            .into_any_element()
    }

    /// Flattens the file tree into a list of rows, honouring collapsed folders and the
    /// active search filter, and records the index of the active note's row.
    ///
    /// This replaces the old recursive element builder: instead of constructing a full
    /// `Div` subtree for every node in the vault (thousands of them, all laid out every
    /// frame), we produce one cheap data row per visible node and let the caller lay out
    /// only the slice that is actually on screen.
    fn flatten_sidebar_tree(
        &self,
        nodes: &[FileNode],
        depth: usize,
        active_path: Option<&PathBuf>,
        parent_matched: bool,
        out: &mut Vec<SidebarRow>,
        selected: &mut Option<usize>,
    ) {
        let query = self.state.search_query.trim().to_lowercase();
        let is_filtering = !query.is_empty();

        for node in nodes {
            match node {
                FileNode::Folder { name, path, children } => {
                    let folder_matches_self = is_filtering && name.to_lowercase().contains(&query);
                    let folder_has_matching_descendant = is_filtering && children.iter().any(|c| c.matches_query(&query));

                    // When filtering, hide the folder unless it, its parent, or a descendant matches.
                    if is_filtering && !parent_matched && !folder_matches_self && !folder_has_matching_descendant {
                        continue;
                    }

                    // Auto-expand while filtering when the folder itself or a descendant matches.
                    let is_expanded = self.state.expanded_folders.contains(path)
                        || (is_filtering && (folder_matches_self || folder_has_matching_descendant));

                    out.push(SidebarRow {
                        is_folder: true,
                        is_expanded,
                        is_selected: false,
                        name: name.clone(),
                        path: path.clone(),
                        depth,
                    });

                    if is_expanded {
                        let next_parent_matched = parent_matched || folder_matches_self;
                        self.flatten_sidebar_tree(children, depth + 1, active_path, next_parent_matched, out, selected);
                    }
                }
                FileNode::File { name, path } => {
                    if is_filtering && !parent_matched && !name.to_lowercase().contains(&query) {
                        continue;
                    }

                    let is_selected = active_path.map(|p| p == path).unwrap_or(false);
                    if is_selected {
                        *selected = Some(out.len());
                    }

                    out.push(SidebarRow {
                        is_folder: false,
                        is_expanded: false,
                        is_selected,
                        name: name.clone(),
                        path: path.clone(),
                        depth,
                    });
                }
            }
        }
    }

    /// Renders a single flattened sidebar row. Row height is fixed ([`SIDEBAR_ROW_H`]) so
    /// the caller can place it from the row index alone.
    fn render_sidebar_row(&self, row: &SidebarRow, theme: &Theme, cx: &Context<Self>) -> AnyElement {
        if row.is_folder {
            let is_folder_selected = self.folder_context_menu.is_open && self.folder_context_menu.folder_path == row.path;

            let chevron = if row.is_expanded {
                icons::icon(icons::ICON_CHEVRON_DOWN, 12.0, theme.text_muted)
            } else {
                icons::icon(icons::ICON_CHEVRON_RIGHT, 12.0, theme.text_muted)
            };
            let folder_svg = if row.is_expanded {
                icons::icon(icons::ICON_FOLDER_OPEN, 14.0, theme.accent)
            } else {
                icons::icon(icons::ICON_FOLDER, 14.0, theme.accent)
            };
            let indent = (row.depth * 14) as f32 + 4.0;
            let label_color = if is_folder_selected { theme.accent } else { theme.text_primary };

            let label = crate::views::sidebar::highlight_matches(
                &row.name,
                &self.state.search_query,
                label_color,
                is_folder_selected,
                theme,
            );

            let folder_path = row.path.clone();
            let folder_path_context = row.path.clone();

            div()
                .id(SharedString::from(format!("fld-{}", row.path.to_string_lossy())))
                .flex()
                .flex_row()
                .items_center()
                .gap_1p5()
                .h(px(SIDEBAR_ROW_H))
                .flex_shrink_0()
                .pl(px(indent))
                .pr_2()
                .rounded_md()
                .cursor_pointer()
                .when(is_folder_selected, |s| {
                    s.bg(theme.bg_selected)
                        .text_color(theme.accent)
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                })
                .when(!is_folder_selected, |s| {
                    s.when(!self.folder_context_menu.is_open && !self.file_context_menu.is_open, |d| {
                        d.hover(|h| h.bg(theme.bg_hover))
                    })
                    .text_color(theme.text_primary)
                })
                .child(chevron)
                .child(folder_svg)
                .child(
                    div()
                        .text_xs()
                        .text_color(label_color)
                        .truncate()
                        .child(label),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.state.toggle_folder(folder_path.clone());
                    cx.notify();
                }))
                .on_mouse_down(MouseButton::Right, cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let x: f32 = event.position.x.into();
                    let y: f32 = event.position.y.into();
                    this.open_folder_context_menu(folder_path_context.clone(), x, y, cx);
                }))
                .into_any_element()
        } else {
            let is_selected = row.is_selected;
            let indent = (row.depth * 14 + 18) as f32 + 4.0;
            let display_name = row.name.strip_suffix(".md").unwrap_or(&row.name).to_string();
            let base_color = if is_selected { theme.accent } else { theme.text_secondary };

            let label = crate::views::sidebar::highlight_matches(
                &display_name,
                &self.state.search_query,
                base_color,
                is_selected,
                theme,
            );

            let file_svg = icons::icon(
                icons::ICON_FILE_TEXT,
                13.0,
                if is_selected { theme.accent } else { theme.text_muted },
            );

            let file_path = row.path.clone();

            div()
                .id(SharedString::from(format!("fil-{}", row.path.to_string_lossy())))
                .flex()
                .flex_row()
                .items_center()
                .gap_1p5()
                .h(px(SIDEBAR_ROW_H))
                .flex_shrink_0()
                .pl(px(indent))
                .pr_2()
                .rounded_md()
                .cursor_pointer()
                .when(is_selected, |s| {
                    s.bg(theme.bg_selected)
                        .text_color(theme.accent)
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                })
                .when(!is_selected, |s| {
                    s.when(!self.folder_context_menu.is_open && !self.file_context_menu.is_open, |d| {
                        d.hover(|h| h.bg(theme.bg_hover))
                    })
                    .text_color(theme.text_primary)
                })
                .child(file_svg)
                .child(
                    div()
                        .text_xs()
                        .truncate()
                        .child(label),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.select_note(file_path.clone(), false, window, cx);
                }))
                .on_mouse_down(MouseButton::Right, cx.listener({
                    let file_path = row.path.clone();
                    move |this, event: &gpui::MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        let x: f32 = event.position.x.into();
                        let y: f32 = event.position.y.into();
                        this.select_note(file_path.clone(), false, window, cx);
                        this.open_file_context_menu(file_path.clone(), x, y, cx);
                    }
                }))
                .into_any_element()
        }
    }
}
