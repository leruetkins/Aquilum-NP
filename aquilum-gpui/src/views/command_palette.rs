//! Obsidian-style Command Palette modal dialog (`Ctrl+P`).
//!
//! Provides a centered search overlay to filter and execute application and
//! plugin commands. Designed identically to Obsidian's Command Palette:
//! - Centered card with search filter input, caret blinking, and close button.
//! - Filtered list of registered commands with hotkey badges (e.g. `Ctrl + O`, `Ctrl + B`).
//! - Bottom keyboard navigation hints footer: `↑↓ навигация   ↵ использовать   esc отмена`.
//! - Up/Down arrow wrap navigation, Enter to run, Escape to dismiss.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    div, prelude::*, px, Context, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, SharedString, Styled, Window,
};

use crate::blink_manager::BlinkTarget;
use crate::commands::Command;
use crate::icons;
use crate::theme::Theme;
use crate::views::app_view::AppView;
use crate::views::goto_search::{
    highlight_query_terms, highlight_terms, word_range_at,
};
use crate::views::scrollbar::ScrollbarDrag;

/// State of the Command Palette modal dialog.
#[derive(Clone, Debug, Default)]
pub struct CommandPaletteState {
    pub is_open: bool,
    pub query: String,
    pub cursor_pos: usize,
    pub selection_anchor: Option<usize>,
    pub is_mouse_selecting: bool,
    pub filtered_commands: Vec<Command>,
    pub selected_ix: usize,
    /// Overlay scrollbar drag state for the command list.
    pub scrollbar_drag: ScrollbarDrag,
    /// Window x of the input text box; measured via canvas for click mapping.
    pub box_origin_x: Rc<Cell<f32>>,
}

impl CommandPaletteState {
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        if let Some(anchor) = self.selection_anchor {
            if anchor != self.cursor_pos {
                return Some((anchor.min(self.cursor_pos), anchor.max(self.cursor_pos)));
            }
        }
        None
    }
}

impl AppView {
    /// Opens the Command Palette modal with cleared query and focused input (Ctrl+P / Ctrl+Shift+P).
    pub fn open_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.command_palette.is_open {
            self.close_command_palette(window, cx);
            return;
        }

        // Close other popups / context menus
        self.close_tabs_popup(cx);
        self.close_tab_context_menu(cx);
        self.file_context_menu.is_open = false;
        self.folder_context_menu.is_open = false;
        if self.search_dialog.is_open {
            self.close_search_dialog(window, cx);
        }
        if self.plugin_catalog.is_open {
            self.close_plugin_catalog(window, cx);
        }

        let all_commands = self.state.commands.all();
        self.command_palette = CommandPaletteState {
            is_open: true,
            filtered_commands: all_commands,
            selected_ix: 0,
            box_origin_x: self.command_palette.box_origin_x.clone(),
            ..Default::default()
        };

        window.focus(&self.command_palette_focus_handle);
        self.command_palette_blink.enable(BlinkTarget::CommandPalette, cx);
        cx.notify();
    }

    /// Closes the Command Palette modal and returns focus to the editor.
    pub fn close_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.command_palette.is_open {
            return;
        }
        self.command_palette.is_open = false;
        self.command_palette.selection_anchor = None;
        self.command_palette.is_mouse_selecting = false;
        self.command_palette_blink.disable(cx);
        window.focus(&self.focus_handle);
        cx.notify();
    }

    /// Re-filters commands according to the current query.
    pub fn run_command_palette_filter(&mut self) {
        let hits = self.state.commands.search(&self.command_palette.query);
        self.command_palette.filtered_commands = hits;
        self.command_palette.selected_ix = self
            .command_palette
            .selected_ix
            .min(self.command_palette.filtered_commands.len().saturating_sub(1));
    }

    /// Executes the currently selected command or closes the palette.
    pub fn execute_selected_command(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(cmd) = self
            .command_palette
            .filtered_commands
            .get(self.command_palette.selected_ix)
            .cloned()
        {
            let cmd_id = cmd.id.clone();
            self.close_command_palette(window, cx);
            self.execute_command(&cmd_id, window, cx);
        } else {
            self.close_command_palette(window, cx);
        }
    }

    /// Central command execution dispatcher.
    pub fn execute_command(&mut self, cmd_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        println!("[command-palette] execute_command requested: '{}'", cmd_id);
        // 1. Try registered callback first
        let executed = self.state.commands.execute(cmd_id);
        println!("[command-palette] Command '{}' registered callback executed = {}", cmd_id, executed);

        // 2. Execute built-in host actions
        match cmd_id {
            "editor:save" => {
                self.state.save_active_note();
                cx.notify();
            }
            "workspace:new-tab" => {
                self.open_empty_tab(window, cx);
            }
            "workspace:close-tab" => {
                if let Some(doc) = self.state.active_document() {
                    let path = doc.path.clone();
                    self.close_tab(&path, cx);
                }
            }
            "view:toggle-left-sidebar" => {
                self.toggle_sidebar(cx);
            }
            "view:toggle-right-sidebar" => {
                self.toggle_right_sidebar(cx);
            }
            "view:toggle-ribbon" => {
                self.toggle_ribbon_rail_visibility(cx);
            }
            "app:open-settings" => {
                self.open_settings(None, cx);
            }
            "editor:toggle-word-wrap" => {
                self.toggle_word_wrap(cx);
            }
            "app:open-command-palette" => {
                self.open_command_palette(window, cx);
            }
            "app:open-search-dialog" => {
                self.open_search_dialog(window, cx);
            }
            _ => {
                if !executed {
                    self.state.status_text = format!("Unknown command: {}", cmd_id);
                    cx.notify();
                }
            }
        }
    }

    /// Handles keyboard events inside the Command Palette input.
    pub fn handle_command_palette_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.command_palette_blink.pause_and_show(BlinkTarget::CommandPalette, cx);
        let key = &event.keystroke.key;
        let ctrl = event.keystroke.modifiers.control;
        let shift = event.keystroke.modifiers.shift;
        let alt = event.keystroke.modifiers.alt;

        // Escape dismisses palette
        if key == "escape" {
            self.close_command_palette(window, cx);
            return;
        }

        // Arrow navigation
        if key == "down" || key == "up" {
            let len = self.command_palette.filtered_commands.len();
            if len > 0 {
                let delta = if key == "down" { 1isize } else { -1 };
                let cur = self.command_palette.selected_ix as isize;
                self.command_palette.selected_ix =
                    (cur + delta).rem_euclid(len as isize) as usize;
                cx.notify();
            }
            return;
        }

        // Enter executes command
        if key == "enter" {
            self.execute_selected_command(window, cx);
            return;
        }

        // Home / End
        if key == "home" || key == "end" {
            let total = self.command_palette.query.chars().count();
            let pos = if key == "home" { 0 } else { total };
            if shift {
                if self.command_palette.selection_anchor.is_none() {
                    self.command_palette.selection_anchor = Some(self.command_palette.cursor_pos);
                }
            } else {
                self.command_palette.selection_anchor = None;
            }
            self.command_palette.cursor_pos = pos;
            cx.notify();
            return;
        }

        // Left / Right cursor movement
        if key == "left" || key == "right" {
            let total = self.command_palette.query.chars().count();
            if shift && self.command_palette.selection_anchor.is_none() {
                self.command_palette.selection_anchor = Some(self.command_palette.cursor_pos);
            }
            if key == "left" {
                if shift {
                    self.command_palette.cursor_pos = self.command_palette.cursor_pos.saturating_sub(1);
                } else if let Some((start, _)) = self.command_palette.selection_range() {
                    self.command_palette.cursor_pos = start;
                    self.command_palette.selection_anchor = None;
                } else {
                    self.command_palette.cursor_pos = self.command_palette.cursor_pos.saturating_sub(1);
                }
            } else if shift {
                self.command_palette.cursor_pos = (self.command_palette.cursor_pos + 1).min(total);
            } else if let Some((_, end)) = self.command_palette.selection_range() {
                self.command_palette.cursor_pos = end;
                self.command_palette.selection_anchor = None;
            } else {
                self.command_palette.cursor_pos = (self.command_palette.cursor_pos + 1).min(total);
            }
            cx.notify();
            return;
        }

        // Ctrl+A select all
        if ctrl && (key.eq_ignore_ascii_case("a") || key == "ф" || key == "Ф") {
            self.command_palette.selection_anchor = Some(0);
            self.command_palette.cursor_pos = self.command_palette.query.chars().count();
            cx.notify();
            return;
        }

        // Ctrl+C copy
        if ctrl && (key.eq_ignore_ascii_case("c") || key == "с" || key == "С") {
            if let Some((s, e)) = self.command_palette.selection_range() {
                let text: String = self.command_palette.query.chars().skip(s).take(e - s).collect();
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
            }
            return;
        }

        // Ctrl+X cut
        if ctrl && (key.eq_ignore_ascii_case("x") || key == "ч" || key == "Ч") {
            if let Some((s, e)) = self.command_palette.selection_range() {
                let text: String = self.command_palette.query.chars().skip(s).take(e - s).collect();
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                let mut chars: Vec<char> = self.command_palette.query.chars().collect();
                chars.drain(s..e);
                self.command_palette.query = chars.into_iter().collect();
                self.command_palette.cursor_pos = s;
                self.command_palette.selection_anchor = None;
                self.run_command_palette_filter();
                cx.notify();
            }
            return;
        }

        // Ctrl+V paste
        if ctrl && (key.eq_ignore_ascii_case("v") || key == "м" || key == "М") {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    let filtered: String = text
                        .chars()
                        .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
                        .collect();
                    if !filtered.is_empty() {
                        self.insert_command_palette_text(&filtered);
                        self.run_command_palette_filter();
                        cx.notify();
                    }
                }
            }
            return;
        }

        if ctrl || alt || event.keystroke.modifiers.platform {
            return;
        }

        // Backspace / Delete
        if key == "backspace" || key == "delete" {
            let mut chars: Vec<char> = self.command_palette.query.chars().collect();
            if let Some((s, e)) = self.command_palette.selection_range() {
                chars.drain(s..e);
                self.command_palette.cursor_pos = s;
                self.command_palette.selection_anchor = None;
            } else if key == "backspace" {
                let pos = self.command_palette.cursor_pos.min(chars.len());
                if pos > 0 {
                    chars.remove(pos - 1);
                    self.command_palette.cursor_pos = pos - 1;
                } else {
                    return;
                }
            } else {
                let pos = self.command_palette.cursor_pos.min(chars.len());
                if pos < chars.len() {
                    chars.remove(pos);
                } else {
                    return;
                }
            }
            self.command_palette.query = chars.into_iter().collect();
            self.run_command_palette_filter();
            cx.notify();
            return;
        }

        // Plain text input
        let text_to_insert = if key == "space" {
            Some(" ".to_string())
        } else if let Some(ch) = &event.keystroke.key_char {
            Some(ch.to_string())
        } else if key.chars().count() == 1 {
            Some(key.clone())
        } else {
            None
        };
        if let Some(text) = text_to_insert {
            let filtered: String = text
                .chars()
                .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
                .collect();
            if !filtered.is_empty() {
                self.insert_command_palette_text(&filtered);
                self.run_command_palette_filter();
                cx.notify();
            }
        }
    }

    /// Inserts text into the Command Palette search query.
    fn insert_command_palette_text(&mut self, text: &str) {
        let mut chars: Vec<char> = self.command_palette.query.chars().collect();
        let pos = if let Some((s, e)) = self.command_palette.selection_range() {
            chars.drain(s..e);
            self.command_palette.selection_anchor = None;
            s
        } else {
            self.command_palette.cursor_pos.min(chars.len())
        };
        for (i, c) in text.chars().enumerate() {
            chars.insert(pos + i, c);
        }
        self.command_palette.cursor_pos = pos + text.chars().count();
        self.command_palette.query = chars.into_iter().collect();
    }

    /// Maps a click x (window coords) to a query char index.
    pub fn command_palette_click_pos(&self, click_x: f32, window: &Window) -> usize {
        let query = &self.command_palette.query;
        if query.is_empty() {
            return 0;
        }
        let box_x = self.command_palette.box_origin_x.get();
        let rel_x = (click_x - box_x).max(0.0);
        let run = gpui::TextRun {
            len: query.len(),
            font: gpui::font("Segoe UI"),
            color: Default::default(),
            underline: Default::default(),
            strikethrough: None,
            background_color: None,
        };
        let shaped = window.text_system().shape_line(
            SharedString::from(query.clone()),
            px(14.0),
            &[run],
            None,
        );
        let byte_idx = shaped.closest_index_for_x(px(rel_x));
        query[..byte_idx.min(query.len())].chars().count()
    }

    /// Follows the pointer while input selection is being dragged.
    pub fn handle_command_palette_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.command_palette.is_open || !self.command_palette.is_mouse_selecting {
            return;
        }
        let x: f32 = event.position.x.into();
        let new_pos = self.command_palette_click_pos(x, window);
        if self.command_palette.cursor_pos != new_pos {
            self.command_palette.cursor_pos = new_pos;
            self.command_palette_blink.pause_and_show(BlinkTarget::CommandPalette, cx);
            cx.notify();
        }
    }

    /// Ends an input drag selection in command palette.
    pub fn handle_command_palette_mouse_up(&mut self, cx: &mut Context<Self>) {
        if self.command_palette.is_mouse_selecting {
            self.command_palette.is_mouse_selecting = false;
            if self.command_palette.selection_anchor == Some(self.command_palette.cursor_pos) {
                self.command_palette.selection_anchor = None;
            }
            cx.notify();
        }
    }

    /// Renders the Obsidian-style Command Palette modal overlay.
    pub fn render_command_palette(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let window_size = window.bounds().size;
        let window_w: f32 = window_size.width.into();
        let window_h: f32 = window_size.height.into();
        let card_w = (620.0f32).min(window_w - 48.0).max(320.0);
        let card_h = (520.0f32).min(window_h - 100.0).max(340.0);

        let query = self.command_palette.query.clone();
        let terms = highlight_query_terms(&query);
        let selected_ix = self.command_palette.selected_ix;
        let pixel_scale = window.scale_factor();

        // Input caret geometry
        let query_chars: Vec<char> = query.chars().collect();
        let cursor_pos = self.command_palette.cursor_pos.min(query_chars.len());
        let selection_range = self.command_palette.selection_range();
        let run = gpui::TextRun {
            len: query.len(),
            font: gpui::font("Segoe UI"),
            color: Default::default(),
            underline: Default::default(),
            strikethrough: None,
            background_color: None,
        };
        let shaped = window.text_system().shape_line(
            SharedString::from(query.clone()),
            px(14.0),
            &[run],
            None,
        );
        let cursor_x = if query.is_empty() {
            0.0
        } else {
            let byte_index: usize = query_chars[..cursor_pos].iter().map(|c| c.len_utf8()).sum();
            crate::snap::snap_x(f32::from(shaped.x_for_index(byte_index)), pixel_scale)
        };
        let (sel_x1, sel_w) = if let Some((s, e)) = selection_range {
            if !query.is_empty() {
                let b1: usize = query_chars[..s.min(query_chars.len())].iter().map(|c| c.len_utf8()).sum();
                let b2: usize = query_chars[..e.min(query_chars.len())].iter().map(|c| c.len_utf8()).sum();
                crate::snap::snapped_span(
                    f32::from(shaped.x_for_index(b1)),
                    f32::from(shaped.x_for_index(b2)),
                    pixel_scale,
                )
            } else {
                (0.0, 0.0)
            }
        } else {
            (0.0, 0.0)
        };
        let is_input_focused = self.command_palette_focus_handle.is_focused(window);
        let caret_visible = is_input_focused && self.command_palette_blink.visible();

        // Localized placeholder: "Выберите команду..." in Russian, "Type a command..." in English
        let placeholder_text = if self.config.ui.language.starts_with("ru") {
            "Выберите команду..."
        } else {
            "Type a command..."
        };

        // Navigation footer strings
        let is_ru = self.config.ui.language.starts_with("ru");
        let (nav_hint, use_hint, cancel_hint) = if is_ru {
            ("навигация", "использовать", "отмена")
        } else {
            ("navigate", "use", "dismiss")
        };

        div()
            .id("command-palette-overlay")
            .absolute()
            .inset_0()
            .cursor(gpui::CursorStyle::Arrow)
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                if this.command_palette.scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    // Follow scrollbar drag
                    let _ = y;
                } else {
                    this.handle_command_palette_mouse_move(event, window, cx);
                }
                cx.stop_propagation();
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.handle_command_palette_mouse_up(cx);
                this.command_palette.scrollbar_drag.end();
            }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.handle_command_palette_mouse_up(cx);
                this.command_palette.scrollbar_drag.end();
            }))
            .on_scroll_wheel(|_, _, cx| {
                cx.stop_propagation();
            })
            // Backdrop
            .child(
                div()
                    .id("command-palette-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(gpui::rgba(0x00000066))
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                        this.close_command_palette(window, cx);
                    }))
                    .on_mouse_down(MouseButton::Right, cx.listener(|this, _, window, cx| {
                        this.close_command_palette(window, cx);
                    })),
            )
            // Center Dialog Card
            .child(
                div()
                    .id("command-palette-center")
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .id("command-palette-card")
                            .w(px(card_w))
                            .h(px(card_h))
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .bg(theme.bg_card)
                            .border_1()
                            .border_color(theme.border)
                            .rounded_xl()
                            .shadow_xl()
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
                            // ---- Header: Search Input + Close Button ----
                            .child(
                                div()
                                    .id("command-palette-header")
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .px_3()
                                    .h(px(48.0))
                                    .flex_shrink_0()
                                    .border_b_1()
                                    .border_color(theme.border)
                                    .child(
                                        div()
                                            .id("command-palette-input")
                                            .relative()
                                            .flex_1()
                                            .min_w_0()
                                            .h(px(22.0))
                                            .track_focus(&self.command_palette_focus_handle)
                                            .cursor(gpui::CursorStyle::IBeam)
                                            .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                                cx.stop_propagation();
                                                window.focus(&this.command_palette_focus_handle);
                                                this.command_palette_blink.pause_and_show(BlinkTarget::CommandPalette, cx);
                                                let click_x: f32 = event.position.x.into();
                                                let pos = this.command_palette_click_pos(click_x, window);
                                                let total_chars = this.command_palette.query.chars().count();
                                                if event.click_count >= 3 {
                                                    this.command_palette.selection_anchor = Some(0);
                                                    this.command_palette.cursor_pos = total_chars;
                                                    this.command_palette.is_mouse_selecting = false;
                                                } else if event.click_count == 2 {
                                                    let (s, e) = word_range_at(&this.command_palette.query, pos);
                                                    this.command_palette.selection_anchor = Some(s);
                                                    this.command_palette.cursor_pos = e;
                                                    this.command_palette.is_mouse_selecting = false;
                                                } else if event.modifiers.shift {
                                                    if this.command_palette.selection_anchor.is_none() {
                                                        this.command_palette.selection_anchor = Some(this.command_palette.cursor_pos);
                                                    }
                                                    this.command_palette.cursor_pos = pos;
                                                    this.command_palette.is_mouse_selecting = true;
                                                } else {
                                                    this.command_palette.cursor_pos = pos;
                                                    this.command_palette.selection_anchor = Some(pos);
                                                    this.command_palette.is_mouse_selecting = true;
                                                }
                                                cx.notify();
                                            }))
                                            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                                                cx.stop_propagation();
                                                this.handle_command_palette_key(event, window, cx);
                                            }))
                                            .child(
                                                gpui::canvas(
                                                    {
                                                        let cell = self.command_palette.box_origin_x.clone();
                                                        move |bounds, _, _| {
                                                             cell.set(f32::from(bounds.origin.x));
                                                        }
                                                    },
                                                    |_, _, _, _| {},
                                                )
                                                .absolute()
                                                .size_full(),
                                            )
                                            .when(selection_range.is_some() && sel_w > 0.0, |s| {
                                                s.child(
                                                    div()
                                                        .absolute()
                                                        .left(px(sel_x1))
                                                        .top(px(0.0))
                                                        .w(px(sel_w))
                                                        .h(px(22.0))
                                                        .bg(theme.selection_bg)
                                                        .rounded_xs(),
                                                )
                                            })
                                            .child(if query.is_empty() {
                                                div()
                                                    .font_family("Segoe UI")
                                                    .text_size(px(14.0))
                                                    .line_height(px(22.0))
                                                    .text_color(theme.text_muted)
                                                    .child(placeholder_text.to_string())
                                            } else {
                                                div()
                                                    .font_family("Segoe UI")
                                                    .text_size(px(14.0))
                                                    .line_height(px(22.0))
                                                    .text_color(theme.text_primary)
                                                    .child(query.clone())
                                            })
                                            .when(caret_visible, |s| {
                                                s.child(
                                                    div()
                                                        .absolute()
                                                        .left(px(cursor_x))
                                                        .top(px(2.0))
                                                        .w(px(1.5))
                                                        .h(px(18.0))
                                                        .bg(theme.accent)
                                                        .rounded_xs(),
                                                )
                                            }),
                                    )
                                    // Close 'X' button
                                    .child(
                                        div()
                                            .id("command-palette-close-btn")
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .w(px(24.0))
                                            .h(px(24.0))
                                            .rounded_md()
                                            .cursor_pointer()
                                            .hover(|b| b.bg(theme.bg_hover))
                                            .child(icons::icon(icons::ICON_CLOSE, 14.0, theme.text_secondary))
                                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                                cx.stop_propagation();
                                            })
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.close_command_palette(window, cx);
                                            })),
                                    ),
                            )
                            // ---- Body: List of Commands ----
                            .child(
                                div()
                                    .id("command-palette-list-viewport")
                                    .relative()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_h_0()
                                    .child(
                                        div()
                                            .id("command-palette-scroll")
                                            .track_scroll(&self.command_palette_scroll_handle)
                                            .flex()
                                            .flex_col()
                                            .flex_1()
                                            .min_h_0()
                                            .overflow_scroll()
                                            .p_2()
                                            .gap_1()
                                            .children(self.command_palette.filtered_commands.iter().enumerate().map(|(ix, cmd)| {
                                                let is_selected = ix == selected_ix;
                                                let cmd_id_click = cmd.id.clone();
                                                let cmd_name = cmd.name.clone();
                                                let hotkey_opt = cmd.hotkey.clone();

                                                div()
                                                    .id(SharedString::from(format!("command-item-{}", ix)))
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .justify_between()
                                                    .px_3()
                                                    .py_1p5()
                                                    .rounded_md()
                                                    .cursor_pointer()
                                                    .when(is_selected, |d| d.bg(theme.bg_hover))
                                                    .hover(|h| h.bg(theme.bg_hover))
                                                    .on_hover(cx.listener(move |this, is_hovered: &bool, _, cx| {
                                                        if *is_hovered && this.command_palette.selected_ix != ix {
                                                            this.command_palette.selected_ix = ix;
                                                            cx.notify();
                                                        }
                                                    }))
                                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                                        cx.stop_propagation();
                                                    })
                                                    .on_click(cx.listener(move |this, _, window, cx| {
                                                        cx.stop_propagation();
                                                        this.close_command_palette(window, cx);
                                                        this.execute_command(&cmd_id_click, window, cx);
                                                    }))
                                                    // Command name on left with term highlighting
                                                    .child(
                                                        div()
                                                            .flex_1()
                                                            .min_w_0()
                                                            .truncate()
                                                            .text_sm()
                                                            .child(highlight_terms(&cmd_name, &terms, theme.text_primary, is_selected, theme)),
                                                    )
                                                    // Hotkey badge / pill on right if present
                                                    .when_some(hotkey_opt, |row, hotkey| {
                                                        row.child(
                                                            div()
                                                                .flex_shrink_0()
                                                                .px_2()
                                                                .py_0p5()
                                                                .rounded(px(4.0))
                                                                .bg(if is_selected { theme.bg_card } else { theme.bg_sidebar })
                                                                .border_1()
                                                                .border_color(theme.border)
                                                                .text_xs()
                                                                .font_family("Segoe UI")
                                                                .text_color(theme.text_secondary)
                                                                .child(format_hotkey_badge(&hotkey)),
                                                        )
                                                    })
                                            })),
                                    ),
                            )
                            // ---- Footer: Obsidian-style hints ----
                            .child(
                                div()
                                    .id("command-palette-footer")
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_center()
                                    .gap_4()
                                    .h(px(32.0))
                                    .flex_shrink_0()
                                    .border_t_1()
                                    .border_color(theme.border)
                                    .text_xs()
                                    .text_color(theme.text_muted)
                                    .child(format!("↑↓ {}", nav_hint))
                                    .child(format!("↵ {}", use_hint))
                                    .child(format!("esc {}", cancel_hint)),
                            ),
                    ),
            )
    }
}

/// Formats hotkeys like "Ctrl+P" or "Ctrl+Alt+Left" into spaced pill text "Ctrl + P" or "Ctrl + Alt + ←".
fn format_hotkey_badge(raw: &str) -> String {
    let parts: Vec<&str> = raw.split('+').map(|s| s.trim()).collect();
    let formatted: Vec<String> = parts
        .iter()
        .map(|&p| match p.to_lowercase().as_str() {
            "left" => "←".to_string(),
            "right" => "→".to_string(),
            "up" => "↑".to_string(),
            "down" => "↓".to_string(),
            _ => p.to_string(),
        })
        .collect();
    formatted.join(" + ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_hotkey_badge() {
        assert_eq!(format_hotkey_badge("Ctrl+O"), "Ctrl + O");
        assert_eq!(format_hotkey_badge("Ctrl+Alt+Left"), "Ctrl + Alt + ←");
        assert_eq!(format_hotkey_badge("Ctrl+B"), "Ctrl + B");
        assert_eq!(format_hotkey_badge("Ctrl+Shift+P"), "Ctrl + Shift + P");
    }
}
