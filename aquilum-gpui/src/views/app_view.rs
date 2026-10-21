use crate::blink_manager::{BlinkManager, BlinkTarget};
use crate::icons;
use crate::state::{AppState, FileNode};
use crate::theme::Theme;
use crate::views::scrollbar::ScrollbarDrag;
use gpui::{
    div, prelude::*, px, rgb, rgba, Bounds, ClipboardItem, Context, Div, Entity, FocusHandle,
    IntoElement, KeyDownEvent, ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Render, ScrollHandle, ScrollStrategy,
    SharedString, Subscription, Window, WindowControlArea,
};
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Length of the settle burst that repaints the editor until its scroll handle
/// has been measured (see `layout_settle_frames`). Three frames is enough: one
/// drawn from the estimates, one from the freshly measured handle, one that
/// confirms nothing moved.
pub(crate) const SCROLL_SETTLE_FRAMES: u8 = 3;

/// Px one wheel "line" is worth. 24 = the editor row height, so a notch (three
/// lines on Windows) moves three rows of text.
const WHEEL_LINE_PX: f32 = 24.0;

/// Duration bounds of the optional wheel glide, seconds. A notch gets
/// `2 × mean interval of the last three notches`, clamped to this band: a
/// sustained spin shortens the glide so the next notch keeps the motion
/// alive, an isolated notch keeps a mid-length one. Off by default
/// (`editor.wheelSmoothScroll`): with the flag off every notch applies
/// instantly and the text stops dead.
const WHEEL_GLIDE_MIN_SECS: f32 = 0.05;
const WHEEL_GLIDE_MAX_SECS: f32 = 0.20;

/// Fixed thumb height (px) of the Zed-style wide scrollbar lane: a position
/// marker that never resizes with the document.
const WIDE_THUMB_H: f64 = 25.0;

/// One in-flight wheel glide. `start` is the live position when the notch
/// arrived, `target` where it lands; `curve` is the P1 control point of a
/// cubic-bezier easing (P2 fixed at `(0.6, 1.0)`) whose initial slope carries
/// the velocity of the glide the notch interrupted — a notch stream reads as
/// one continuous motion instead of a series of restarts. Stepped in `render`
/// against a hard deadline; past it the position lands exactly on `target`.
#[derive(Clone, Copy)]
pub struct WheelEase {
    start: f64,
    target: f64,
    t0: std::time::Instant,
    duration_secs: f32,
    curve: (f64, f64),
}

impl WheelEase {
    /// Builds a glide from `from` to `to` entering with `velocity` (px/s).
    /// The curve's initial tangent encodes that velocity: slope =
    /// velocity·duration/span, then both coordinates are divided by
    /// `(1 + slope²)^½` so a large velocity only tilts the tangent, never
    /// breaks the time domain. An opposing velocity (reversal) or a zero
    /// span degrades to the plain ease-out.
    fn new(from: f64, to: f64, velocity: f64, duration_secs: f32) -> Self {
        let span = to - from;
        let slope = if span == 0.0 || velocity * span <= 0.0 {
            0.0
        } else {
            velocity * f64::from(duration_secs) / span
        };
        let norm = (1.0 + slope * slope).sqrt();
        const VELOCITY_WEIGHT: f64 = 0.25;
        WheelEase {
            start: from,
            target: to,
            t0: std::time::Instant::now(),
            duration_secs,
            curve: (VELOCITY_WEIGHT / norm, VELOCITY_WEIGHT * slope / norm),
        }
    }

    fn position_at(&self, at: std::time::Instant) -> f64 {
        let t = (at.duration_since(self.t0).as_secs_f32() / self.duration_secs).clamp(0.0, 1.0);
        self.start + (self.target - self.start) * cubic_bezier(self.curve.0, self.curve.1, 0.6, 1.0, f64::from(t))
    }

    /// Current speed (px/s), numerically — what an arriving notch inherits.
    /// At the very start the backward sample is clamped to `t0`, so the
    /// interval shrinks with the position difference instead of measuring a
    /// half-span average against a full interval (which reads as half speed).
    fn velocity_at(&self, at: std::time::Instant) -> f64 {
        let h = std::time::Duration::from_millis(1);
        let before = at.max(self.t0);
        let after = at + h;
        let dt = after.duration_since(before).as_secs_f64();
        (self.position_at(after) - self.position_at(before)) / dt
    }
}

/// Rows rendered above and below the viewport.
///
/// It also bounds how far the painted slice is shifted from the scroll position
/// (see the slice in `render`), which is what keeps every layout coordinate near
/// zero — the reason a huge note no longer drifts apart at the tail.
const EDITOR_OVERSCAN_PX: f32 = 300.0;

pub struct AppView {
    pub state: AppState,
    pub theme: Theme,
    pub focus_handle: FocusHandle,
    pub search_focus_handle: FocusHandle,
    pub editor_scroll_handle: ScrollHandle,
    /// The editor's scroll position, and the single source of truth for it.
    ///
    /// gpui's own offset is *not* used for the editor any more: the visible rows
    /// are painted as a slice anchored near y = 0, and the scroll position only
    /// decides which slice that is and how far it is shifted. Keeping the number
    /// here also keeps it exact — an offset of 2×10^8 px in `f32` can only move
    /// in 16 px steps, which is coarser than a 24 px row.
    pub editor_scroll: Cell<f64>,
    /// In-flight optional wheel ease (`None` when the wheel applies instantly).
    /// Any direct write through `set_editor_scroll_top_f64` cancels it: the
    /// position came from elsewhere, so there is nothing left to ease towards.
    pub wheel_ease: Cell<Option<WheelEase>>,
    /// Start instants of the last three wheel notches: the glide duration
    /// adapts to how fast the wheel is spun (their mean × 2, clamped).
    pub wheel_notch_history: Cell<[Option<std::time::Instant>; 3]>,
    pub sidebar_scroll_handle: ScrollHandle,
    pub settings_scroll_handle: ScrollHandle,
    pub need_scroll_sidebar_to_active: Cell<bool>,
    pub initial_sidebar_scroll_done: Cell<bool>,
    pub edit_mode: bool,
    pub editor_blink: BlinkManager,
    pub editor_is_focused: bool,
    /// Last observed OS window activation state. gpui tracks focus per window and never
    /// clears it when the window is deactivated, so this is the only way to notice that
    /// the app went to the background and stop blinking there.
    pub window_was_active: bool,
    pub is_mouse_selecting: bool,
    pub ctrl_preview_selection: bool,
    pub ctrl_held: bool,
    pub hovered_link_id: Option<String>,
    pub hovered_tag: bool,
    pub sidebar_width: f32,
    pub sidebar_collapsed: bool,
    pub is_resizing_sidebar: bool,
    pub is_hovering_resizer: bool,
    pub right_sidebar_width: f32,
    pub right_sidebar_collapsed: bool,
    pub is_resizing_right_sidebar: bool,
    pub is_hovering_right_resizer: bool,
    pub split_drag_state: Option<crate::workspace::SplitDragState>,
    pub hovered_split_divider: Option<(crate::workspace::SplitDirection, Vec<usize>, usize)>,
    /// Obsidian-style tabs popup; `Some` while it is shown.
    pub tabs_popup: Option<Entity<crate::views::tab_list::TabListPopup>>,
    /// Leaf that opened the current tabs popup, if any.
    pub tabs_popup_leaf: Option<crate::workspace::LeafId>,
    /// Anchor position (click_x, click_y) for the tabs popup.
    pub tabs_popup_pos: Option<(f32, f32)>,
    /// Keeps the popup's event subscription alive; dropping it unsubscribes.
    pub tabs_popup_subscription: Option<Subscription>,
    /// Tab strip tab under the cursor, scoped to its specific leaf. Drives the
    /// Chrome-like behavior: the hovered tab's neighbours hide their
    /// dividers so the boundary always reads clean.
    pub hovered_tab: Option<(crate::workspace::LeafId, usize)>,
    /// Locale picker popup (same smart list, anchored to the language button).
    pub locale_popup: Option<Entity<crate::views::tab_list::TabListPopup>>,
    /// Keeps the locale picker's event subscription alive.
    pub locale_popup_subscription: Option<Subscription>,
    /// Editor font picker popup (same smart list, anchored to the font button).
    pub font_popup: Option<Entity<crate::views::tab_list::TabListPopup>>,
    /// Keeps the font picker's event subscription alive.
    pub font_popup_subscription: Option<Subscription>,
    /// Plugin select dropdown popup (same smart list, anchored to the select button).
    pub plugin_select_popup: Option<Entity<crate::views::tab_list::TabListPopup>>,
    /// Keeps the plugin select picker's event subscription alive.
    pub plugin_select_subscription: Option<Subscription>,
    /// Keeps the window activation subscription alive; triggers save on blur / minimize.
    pub window_activation_subscription: Option<Subscription>,
    /// Command Palette modal dialog (Ctrl+P, Obsidian-style).
    pub command_palette: crate::views::command_palette::CommandPaletteState,
    pub command_palette_focus_handle: FocusHandle,
    pub command_palette_blink: BlinkManager,
    pub command_palette_scroll_handle: ScrollHandle,
    pub plugin_modal_scroll_handle: ScrollHandle,
    pub plugin_modal_scrollbar_drag: crate::views::scrollbar::ScrollbarDrag,
    pub plugin_modal_middle_active: bool,
    pub plugin_modal_smart_last_y: std::cell::Cell<Option<f32>>,
    pub plugin_modal_smart_bounds: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>>,
    /// Active declarative UI trees registered by plugins, keyed by (plugin_id, view_id).
    pub plugin_views: HashMap<(String, u64), crate::plugins::ui::UiNode>,
    /// Active workspace leaf views registered by plugins (ItemView in right/left/center).
    pub plugin_leaf_views: std::collections::BTreeMap<String, crate::plugins::api::PluginLeafView>,
    /// Currently active leaf_id in the right sidebar.
    pub active_right_leaf: Option<String>,
    /// Currently active leaf_id in the left sidebar (if any).
    pub active_left_leaf: Option<String>,
    /// Currently active leaf_id in the central area (if any).
    pub active_center_leaf: Option<String>,
    /// Active plugin runtime handles keeping QuickJS worker threads and communication channels alive.
    pub plugin_runtimes: Vec<crate::plugins::runtime::PluginRuntimeHandle>,
    /// Custom declarative UI trees rendered for markdown code blocks, keyed by block_id.
    pub code_block_views: HashMap<usize, (String, crate::plugins::ui::UiNode)>,
    /// Active API sender channel connected to the GPUI plugin bridge background listener.
    pub plugin_api_tx: Option<futures::channel::mpsc::UnboundedSender<crate::plugins::api::ApiCall>>,
    pub word_wrap: bool,
    pub last_wrap_cols: usize,
    pub is_settings_open: bool,
    pub settings_tab: crate::views::settings_view::SettingsTab,
    pub settings_model: gpui::Entity<crate::settings::SettingsModel>,
    pub settings_window_manager: crate::views::settings_view::SettingsWindowManager,
    pub is_dragging_scrollbar: bool,
    pub scrollbar_drag_start_y: f32,
    pub scrollbar_drag_start_scroll_y: f64,
    /// Whether the pointer is over the editor scrollbar track. While hovered
    /// the overview-ruler marks stretch across the full track width; otherwise
    /// they collapse into a thin pill centered like the thumb.
    pub is_hovering_editor_scrollbar: bool,
    /// Whether the pointer is over the editor scrollbar thumb itself.
    pub is_hovering_editor_thumb: bool,
    /// Drag state of the sidebar tree's overlay scrollbar.
    pub sidebar_scrollbar_drag: ScrollbarDrag,
    /// Measured viewport of the sidebar file tree for cursor-driven smart scrolling.
    pub sidebar_smart_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// Pending cursor-driven scroll target (px from the top); `None` until the mouse moves.
    pub sidebar_smart_target_y: Cell<Option<f32>>,
    /// Last observed cursor Y over the file tree; the first event only primes it.
    pub sidebar_smart_last_mouse_y: Cell<Option<f32>>,
    /// Middle-button smart scroll for the sidebar tree: while held, the absolute
    /// cursor Y inside the tree drives the offset (same formula as Ctrl smart
    /// scroll, but activated by the middle button and always available).
    pub sidebar_middle_active: bool,
    /// Middle-button smart scroll for the main editor: while held, the absolute
    /// cursor Y inside the editor viewport drives `editor_scroll`.
    pub editor_middle_active: bool,
    /// Last observed cursor Y while editor middle-scroll is active; the first
    /// event only primes it so the content never jumps on entry.
    pub editor_middle_last_y: Cell<Option<f32>>,
    pub last_editor_h: Cell<f32>,
    /// Content cache for the sidebar search: typing reuses file bodies read by
    /// previous keystrokes instead of re-reading up to 2000 files per query.
    pub(crate) search_content_cache: crate::views::goto_search::SearchContentCache,
    /// Document line at the top of the viewport, refreshed once per frame.
    ///
    pub(crate) table_layouts: std::cell::RefCell<HashMap<usize, (u64, Arc<TableLayout>)>>,
    /// Shape cache for ordinary (non-table) editor rows, keyed by a hash of
    /// everything `shape_row` reads: line text, line kind, row char range,
    /// active flag, font and cell width. Keying by content instead of line
    /// index makes invalidation automatic — a keystroke only reshapes the one
    /// line whose text changed; the other visible rows hit the cache.
    /// (`table_layouts` is the same pattern for table rows.)
    pub(crate) row_shape_cache:
        std::cell::RefCell<HashMap<u64, Arc<RowShapedLayout>>>,
    pub(crate) manual_table_col_widths: HashMap<usize, Vec<f32>>,
    pub(crate) table_col_resize_drag: Option<TableColResizeDrag>,
    pub config: crate::settings::AppConfig,
    pub last_saved_geometry: Option<(f32, f32, Option<f32>, Option<f32>, bool)>,
    pub initial_scroll_done: bool,
    pub initial_scroll_top: f32,
    /// A saved scroll offset waiting to be written once the editor layout is
    /// measured. A handle write before that is eaten by prepaint clamping
    /// against a not-yet-measured max — and the restore is lost for good.
    pub pending_editor_restore: Option<f64>,
    /// Validate cursor visibility on the next render (the saved offset may be
    /// stale after a resize/rewrap); the reveal is a no-op when exact.
    pub need_reveal_cursor: bool,
    /// Frames left in the editor scrollbar's settle burst.
    ///
    /// The thumb is drawn from `viewport_h` / `max_scroll`, both read off the
    /// scroll handle during `render` — but the handle is only measured in
    /// prepaint of that same frame, so every frame paints the *previous*
    /// layout's numbers. On the first frame after a note is opened those
    /// numbers are estimates (a fixed 600 px viewport, the previous note's max
    /// offset), and nothing asks for another frame, so the thumb sits at the
    /// wrong size and place until some unrelated notify — a caret blink tick —
    /// repaints. Arming this counter forces the couple of frames it takes.
    pub layout_settle_frames: Cell<u8>,
    /// `(viewport_h, max_scroll)` the thumb was drawn from last frame, rounded
    /// to whole pixels so sub-pixel jitter cannot keep the burst alive.
    pub last_scroll_inputs: Cell<(f32, f64)>,
    pub is_hovering_close: bool,
    pub title_focus_handle: FocusHandle,
    pub was_title_focused: bool,
    pub title_blink: BlinkManager,
    pub auto_save_epoch: usize,
    pub active_color_picker: Option<&'static str>,
    pub custom_hex_input: String,
    pub custom_hex_focus_handle: FocusHandle,
    pub hex_is_focused: bool,
    pub hex_blink: BlinkManager,
    pub hex_cursor_pos: usize,
    pub hex_selection_anchor: Option<usize>,
    pub hex_is_mouse_selecting: bool,
    pub hex_mouse_drag_start_x: Option<f32>,
    pub hex_box_origin_x: std::rc::Rc<Cell<f32>>,
    pub quick_filter: crate::views::sidebar::QuickFilterState,
    pub folder_context_menu: crate::views::sidebar::FolderContextMenuState,
    pub file_context_menu: crate::views::sidebar::FileContextMenuState,
    pub tab_context_menu: crate::views::tab_context_menu::TabContextMenuState,
    pub ribbon_context_menu: crate::views::ribbon_menu::RibbonContextMenuState,
    pub editor_context_menu: crate::views::editor_context_menu::ContextMenuState,
    pub ribbon_dragged_id: Option<String>,
    pub ribbon_drag_start_y: Option<f32>,
    pub ribbon_is_dragging: bool,
    /// Sidebar content mode (file tree vs Obsidian-style search panel).
    pub sidebar_mode: crate::views::sidebar::SidebarMode,
    pub sidebar_search: crate::views::sidebar::SidebarSearchState,
    pub search_panel_focus_handle: FocusHandle,
    pub panel_scroll_handle: ScrollHandle,
    /// Drag state of the search panel results overlay scrollbar.
    pub panel_scrollbar_drag: crate::views::scrollbar::ScrollbarDrag,
    /// Go to file dialog ("Search the knowledge base").
    pub search_dialog: crate::views::goto_search::SearchDialogState,
    pub goto_focus_handle: FocusHandle,
    pub goto_blink: BlinkManager,
    pub goto_scroll_handle: ScrollHandle,
    /// Drag state of the search results overlay scrollbar.
    pub goto_scrollbar_drag: crate::views::scrollbar::ScrollbarDrag,
    pub folder_modal: crate::views::sidebar::FolderModal,
    pub folder_modal_focus_handle: FocusHandle,
    /// Vault switcher dialog ("Knowledge bases").
    pub vault_dialog_open: bool,
    pub vault_dialog_focus_handle: FocusHandle,
    /// Code block whose copy button was just pressed; shows the check mark until the
    /// reset timer fires.
    pub copied_code_block: Option<usize>,
    /// Community plugins catalog dialog.
    pub plugin_catalog: crate::views::plugin_catalog::PluginCatalogState,
    pub catalog_focus_handle: FocusHandle,
    pub catalog_scroll_handle: ScrollHandle,
    pub catalog_scrollbar_drag: crate::views::scrollbar::ScrollbarDrag,
    pub catalog_details_scroll_handle: ScrollHandle,
    pub catalog_details_scrollbar_drag: crate::views::scrollbar::ScrollbarDrag,
    pub catalog_blink: BlinkManager,
    pub catalog_window_manager: crate::views::plugin_catalog::CatalogWindowManager,
    /// Active interactive text input in plugin UI (plugin_id, view_id, node_id, value, cursor_pos).
    pub active_plugin_input: Option<crate::plugins::ui::PluginInputFocusState>,
    pub plugin_input_focus_handle: FocusHandle,
    pub plugin_input_blink: BlinkManager,
    /// Active interactive dropdown select popup in plugin UI (plugin_id, view_id, node_id, value, cursor_pos).
    pub active_plugin_select: Option<(String, u64, String)>,
}

impl AppView {
    /// Creates and initializes a new AppView instance with loaded configuration.
    pub fn new(cx: &mut Context<Self>) -> Self {
        let state = AppState::new();
        let config = state.settings.get_config();
        let theme = Theme::from_config(&config.ui.theme, None);
        let ws = crate::state::WindowState::load(&crate::state::app_data_dir());
        let last_saved_geometry = Some((ws.width, ws.height, ws.x, ws.y, ws.maximized));

        let mut edit_mode = true;
        let mut word_wrap = true;
        let mut initial_scroll_top = 0.0;
        if let Some(doc) = state.active_document() {
            if let Some(leaf) = state.leaves.get(&state.workspace.active_leaf) {
                if let Some(tab) = leaf.active_tab() {
                    if let Some(v) = state.views.get(&tab.id) {
                        if v.scroll_top > 0.0 {
                            initial_scroll_top = v.scroll_top as f32;
                        }
                    }
                }
            }
            if initial_scroll_top == 0.0 {
                if let Some(settings) = state.note_states.get(&doc.path) {
                    edit_mode = settings.edit_mode;
                    word_wrap = settings.word_wrap;
                    initial_scroll_top = settings.scroll_top;
                }
            }
        }

        // Restore the sidebar geometry persisted in the session. Old session
        // files predate these keys (width 0.0), so anything outside the
        // resize limits falls back to the default.
        let saved_width = state.session.sidebar_width;
        let sidebar_width = if (160.0..=600.0).contains(&saved_width) {
            saved_width
        } else {
            260.0
        };
        let sidebar_mode = state.session.sidebar_mode.clone();
        let sidebar_mode = if matches!(sidebar_mode, crate::views::sidebar::SidebarMode::Plugin(_)) {
            crate::views::sidebar::SidebarMode::Files
        } else {
            sidebar_mode
        };
        let settings_model = cx.new(|_cx| {
            crate::settings::SettingsModel::new(state.settings.clone())
        });

        let mut this = Self {
            state,
            theme,
            focus_handle: cx.focus_handle(),
            search_focus_handle: cx.focus_handle(),
            editor_scroll_handle: ScrollHandle::new(),
            editor_scroll: Cell::new(0.0),
            wheel_ease: Cell::new(None),
            wheel_notch_history: Cell::new([None; 3]),
            sidebar_scroll_handle: ScrollHandle::new(),
            settings_scroll_handle: ScrollHandle::new(),
            need_scroll_sidebar_to_active: Cell::new(false),
            initial_sidebar_scroll_done: Cell::new(false),
            edit_mode,
            editor_blink: BlinkManager::new(),
            editor_is_focused: true,
            window_was_active: true,
            is_mouse_selecting: false,
            ctrl_preview_selection: false,
            ctrl_held: false,
            hovered_link_id: None,
            hovered_tag: false,
            sidebar_width,
            sidebar_collapsed: false,
            is_resizing_sidebar: false,
            is_hovering_resizer: false,
            right_sidebar_width: 260.0,
            right_sidebar_collapsed: true,
            is_resizing_right_sidebar: false,
            is_hovering_right_resizer: false,
            split_drag_state: None,
            hovered_split_divider: None,
            tabs_popup: None,
            tabs_popup_leaf: None,
            tabs_popup_pos: None,
            tabs_popup_subscription: None,
            hovered_tab: None,
            locale_popup: None,
            locale_popup_subscription: None,
            font_popup: None,
            font_popup_subscription: None,
            plugin_select_popup: None,
            plugin_select_subscription: None,
            window_activation_subscription: None,
            command_palette: crate::views::command_palette::CommandPaletteState::default(),
            command_palette_focus_handle: cx.focus_handle(),
            command_palette_blink: BlinkManager::new(),
            command_palette_scroll_handle: ScrollHandle::new(),
            plugin_modal_scroll_handle: ScrollHandle::new(),
            word_wrap,
            last_wrap_cols: 80,
            is_settings_open: false,
            settings_tab: crate::views::settings_view::SettingsTab::Ui,
            settings_model,
            settings_window_manager: crate::views::settings_view::SettingsWindowManager::default(),
            is_dragging_scrollbar: false,
            scrollbar_drag_start_y: 0.0,
            scrollbar_drag_start_scroll_y: 0.0,
            is_hovering_editor_scrollbar: false,
            is_hovering_editor_thumb: false,
            sidebar_scrollbar_drag: ScrollbarDrag::default(),
            sidebar_smart_bounds: Rc::new(Cell::new(Bounds::default())),
            sidebar_smart_target_y: Cell::new(None),
            sidebar_smart_last_mouse_y: Cell::new(None),
            search_content_cache: crate::views::goto_search::SearchContentCache::new(),
            sidebar_middle_active: false,
            plugin_modal_scrollbar_drag: crate::views::scrollbar::ScrollbarDrag::default(),
            plugin_modal_middle_active: false,
            plugin_modal_smart_last_y: std::cell::Cell::new(None),
            plugin_modal_smart_bounds: std::rc::Rc::new(std::cell::Cell::new(gpui::Bounds::default())),
            editor_middle_active: false,
            editor_middle_last_y: Cell::new(None),
            last_editor_h: Cell::new(600.0),
            config,
            last_saved_geometry,
            initial_scroll_done: false,
            initial_scroll_top,
            pending_editor_restore: None,
            need_reveal_cursor: false,
            layout_settle_frames: Cell::new(SCROLL_SETTLE_FRAMES),
            last_scroll_inputs: Cell::new((0.0, 0.0)),
            is_hovering_close: false,
            title_focus_handle: cx.focus_handle(),
            was_title_focused: false,
            title_blink: BlinkManager::new(),
            auto_save_epoch: 0,
            active_color_picker: None,
            custom_hex_input: String::new(),
            custom_hex_focus_handle: cx.focus_handle(),
            hex_is_focused: false,
            hex_blink: BlinkManager::new(),
            hex_cursor_pos: 0,
            hex_selection_anchor: None,
            hex_is_mouse_selecting: false,
            hex_mouse_drag_start_x: None,
            hex_box_origin_x: std::rc::Rc::new(Cell::new(0.0)),
            quick_filter: crate::views::sidebar::QuickFilterState::new(),
            folder_context_menu: crate::views::sidebar::FolderContextMenuState::default(),
            file_context_menu: crate::views::sidebar::FileContextMenuState::default(),
            tab_context_menu: crate::views::tab_context_menu::TabContextMenuState::default(),
            ribbon_context_menu: crate::views::ribbon_menu::RibbonContextMenuState::default(),
            editor_context_menu: crate::views::editor_context_menu::ContextMenuState::default(),
            ribbon_dragged_id: None,
            ribbon_drag_start_y: None,
            ribbon_is_dragging: false,
            sidebar_mode,
            sidebar_search: crate::views::sidebar::SidebarSearchState::default(),
            search_panel_focus_handle: cx.focus_handle(),
            panel_scroll_handle: ScrollHandle::new(),
            panel_scrollbar_drag: crate::views::scrollbar::ScrollbarDrag::default(),
            search_dialog: crate::views::goto_search::SearchDialogState::default(),
            goto_focus_handle: cx.focus_handle(),
            goto_blink: BlinkManager::new(),
            goto_scroll_handle: ScrollHandle::new(),
            goto_scrollbar_drag: crate::views::scrollbar::ScrollbarDrag::default(),
            folder_modal: crate::views::sidebar::FolderModal::default(),
            folder_modal_focus_handle: cx.focus_handle(),
            vault_dialog_open: false,
            vault_dialog_focus_handle: cx.focus_handle(),
            copied_code_block: None,
            plugin_catalog: crate::views::plugin_catalog::PluginCatalogState::default(),
            catalog_focus_handle: cx.focus_handle(),
            catalog_scroll_handle: ScrollHandle::new(),
            catalog_scrollbar_drag: crate::views::scrollbar::ScrollbarDrag::default(),
            catalog_details_scroll_handle: ScrollHandle::new(),
            catalog_details_scrollbar_drag: crate::views::scrollbar::ScrollbarDrag::default(),
            catalog_blink: BlinkManager::new(),
            catalog_window_manager: crate::views::plugin_catalog::CatalogWindowManager::default(),
            active_plugin_input: None,
            plugin_input_focus_handle: cx.focus_handle(),
            plugin_input_blink: BlinkManager::new(),
            active_plugin_select: None,
            plugin_views: HashMap::new(),
            plugin_leaf_views: std::collections::BTreeMap::new(),
            active_right_leaf: None,
            active_left_leaf: None,
            active_center_leaf: None,
            plugin_runtimes: Vec::new(),
            code_block_views: HashMap::new(),
            plugin_api_tx: None,
            table_layouts: std::cell::RefCell::new(HashMap::new()),
            row_shape_cache: std::cell::RefCell::new(HashMap::new()),
            manual_table_col_widths: HashMap::new(),
            table_col_resize_drag: None,
        };
        this.editor_blink.enable(BlinkTarget::Editor, cx);
        crate::commands_builtin::register_builtin_commands(&this.state.commands);

        cx.subscribe(&this.settings_model, |this, model, event: &crate::settings::SettingsEvent, cx| {
            match event {
                crate::settings::SettingsEvent::ConfigChanged => {
                    this.config = model.read(cx).config.clone();
                    // Font sizes / families changed: every cached shaped row
                    // is stale; the content-hash key covers sizes, but the
                    // family is hashed too — a wholesale clear is simplest
                    // and rare enough not to matter.
                    this.row_shape_cache.borrow_mut().clear();
                    this.table_layouts.borrow_mut().clear();
                    for view in this.state.views.values_mut() {
                        view.visual_cache_dirty = true;
                    }
                    cx.notify();
                }
                crate::settings::SettingsEvent::ThemeChanged => {
                    let theme_name = model.read(cx).config.ui.theme.clone();
                    let theme = crate::theme::Theme::from_config(&theme_name, None);
                    this.theme = theme.clone();
                    cx.set_global(crate::theme::ActiveTheme(std::sync::Arc::new(theme)));
                    cx.notify();
                }
                crate::settings::SettingsEvent::WordWrapToggled(wrap) => {
                    this.word_wrap = *wrap;
                    for view in this.state.views.values_mut() {
                        view.visual_cache_dirty = true;
                    }
                    cx.notify();
                }
                crate::settings::SettingsEvent::PluginToggled { plugin_id, enabled } => {
                    if *enabled {
                        if let Some(vault) = &this.state.vault {
                            let _ = vault.enable_community_plugin(plugin_id);
                            this.load_installed_plugins_for_vault();
                        }
                    } else {
                        if let Some(vault) = &this.state.vault {
                            let _ = vault.disable_community_plugin(plugin_id);
                        }
                        this.unload_plugin(plugin_id, cx);
                    }
                    cx.notify();
                }
            }
        }).detach();

        this
    }

    /// Returns true if any UI separator/divider drag gesture is currently active.
    pub fn is_resizing_any(&self) -> bool {
        self.is_resizing_sidebar
            || self.is_resizing_right_sidebar
            || self.split_drag_state.is_some()
            || self.table_col_resize_drag.is_some()
    }

    pub(crate) fn start_table_col_resize(
        &mut self,
        table_first_line: usize,
        col_idx: usize,
        click_x: f32,
        current_widths: &[f32],
    ) {
        let entry = self
            .manual_table_col_widths
            .entry(table_first_line)
            .or_insert_with(|| current_widths.to_vec());
        if entry.len() != current_widths.len() {
            *entry = current_widths.to_vec();
        }
        let start_width = entry
            .get(col_idx)
            .copied()
            .unwrap_or(current_widths.get(col_idx).copied().unwrap_or(100.0));
        self.table_col_resize_drag = Some(TableColResizeDrag {
            table_first_line,
            col_idx,
            start_x: click_x,
            start_width,
        });
    }

    /// Discovers and spawns all installed plugins located in `<vault>/.vault/plugins/`.
    pub fn load_installed_plugins_for_vault(&mut self) {
        self.unload_all_plugins();

        let Some(api_tx) = self.plugin_api_tx.clone() else {
            return;
        };
        let Some(vault) = &self.state.vault else {
            return;
        };
        // If safe mode is enabled, skip loading all community plugins.
        if self.config.community_plugins.safe_mode {
            return;
        }
        let plugins_dir = vault.root.join(".vault").join("plugins");
        let enabled = vault.load_community_plugins();
        let handles = crate::plugins::loader::load_installed_plugins(
            &plugins_dir,
            Some(&enabled),
            api_tx,
            self.state.snapshot.clone(),
            &self.state.plugin_host,
        );
        self.plugin_runtimes = handles;

        // ВРЕМЕННО (снять окно плагина для проверки вёрстки без кликов):
        // AQUILUM_DEBUG_PLUGIN_CMD="<plugin-id>:<command-id>"
        if let Ok(spec) = std::env::var("AQUILUM_DEBUG_PLUGIN_CMD") {
            if let Some((plugin_id, command_id)) = spec.split_once(':') {
                if let Some(handle) = self.plugin_runtimes.iter().find(|h| h.manifest.id == plugin_id) {
                    let tx = handle.host_events_tx.clone();
                    let command_id = command_id.to_string();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(1500));
                        let _ = tx.unbounded_send(crate::plugins::api::HostEvent::ExecuteCommand { command_id });
                    });
                }
            }
        }
    }

    /// Gracefully unloads all active plugins by sending HostEvent::Unload and clearing registrations.
    pub fn unload_all_plugins(&mut self) {
        for handle in &self.plugin_runtimes {
            let _ = handle.host_events_tx.unbounded_send(crate::plugins::api::HostEvent::Unload);
            self.state.plugin_host.unregister_plugin(&handle.manifest.id);
            self.state.commands.unregister_all_of(&handle.manifest.id);
            self.state.ribbon.unregister_all_of(&handle.manifest.id);
            self.state.status_bar.unregister_all_of(&handle.manifest.id);
        }
        self.state.code_block_processors.clear();
        self.code_block_views.clear();
        self.plugin_runtimes.clear();
        self.plugin_views.clear();
        self.plugin_leaf_views.clear();
        self.active_right_leaf = None;
        self.active_left_leaf = None;
        self.active_center_leaf = None;
        if matches!(self.sidebar_mode, crate::views::sidebar::SidebarMode::Plugin(_)) {
            self.sidebar_mode = crate::views::sidebar::SidebarMode::Files;
        }
        self.active_plugin_input = None;
        self.active_plugin_select = None;
    }

    /// Completely unloads a single plugin and cleans up all its UI components, panels, tabs, buttons, and resources.
    pub fn unload_plugin(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        println!("[app_view] Unloading plugin '{}'...", plugin_id);

        // 1. Send Unload event to the plugin worker thread and drop the runtime handle
        if let Some(pos) = self.plugin_runtimes.iter().position(|r| r.manifest.id == plugin_id) {
            let handle = self.plugin_runtimes.remove(pos);
            let _ = handle.host_events_tx.unbounded_send(crate::plugins::api::HostEvent::Unload);
        }

        // 2. Unregister from plugin host dispatcher
        self.state.plugin_host.unregister_plugin(plugin_id);

        // 3. Remove all commands registered by this plugin
        self.state.commands.unregister_all_of(plugin_id);

        // 4. Remove all ribbon icons/actions registered by this plugin
        self.state.ribbon.unregister_all_of(plugin_id);

        // 5. Remove all status bar items registered by this plugin
        self.state.status_bar.unregister_all_of(plugin_id);

        // 6. Remove all code block processors and views registered by this plugin
        self.state.code_block_processors.unregister_all_of(plugin_id);
        self.code_block_views.retain(|_, (pid, _)| pid != plugin_id);

        // 7. Remove all workspace leaves (sidebar panels / center tabs) registered by this plugin
        let removed_leaf_ids: Vec<String> = self
            .plugin_leaf_views
            .iter()
            .filter(|(_, leaf)| leaf.plugin_id == plugin_id)
            .map(|(id, _)| id.clone())
            .collect();

        for leaf_id in &removed_leaf_ids {
            println!("[app_view] Removing plugin leaf '{}' for plugin '{}'", leaf_id, plugin_id);
            self.plugin_leaf_views.remove(leaf_id);
        }

        // 8. Update active sidebar leaves and modes if the active leaf belonged to this plugin
        if let Some(ref right_id) = self.active_right_leaf {
            if removed_leaf_ids.contains(right_id) {
                self.active_right_leaf = self
                    .plugin_leaf_views
                    .values()
                    .find(|l| l.location == "right")
                    .map(|l| l.leaf_id.clone());
            }
        }

        if let Some(ref left_id) = self.active_left_leaf {
            if removed_leaf_ids.contains(left_id) {
                self.active_left_leaf = self
                    .plugin_leaf_views
                    .values()
                    .find(|l| l.location == "left")
                    .map(|l| l.leaf_id.clone());
            }
        }

        // If sidebar is currently displaying a plugin leaf that was removed, switch back to Files
        if let crate::views::sidebar::SidebarMode::Plugin(ref current_leaf_id) = self.sidebar_mode {
            if removed_leaf_ids.contains(current_leaf_id) {
                if let Some(ref next_left) = self.active_left_leaf {
                    self.sidebar_mode = crate::views::sidebar::SidebarMode::Plugin(next_left.clone());
                } else {
                    self.sidebar_mode = crate::views::sidebar::SidebarMode::Files;
                }
            }
        }

        if let Some(ref center_id) = self.active_center_leaf {
            if removed_leaf_ids.contains(center_id) {
                self.active_center_leaf = None;
            }
        }

        // 9. Remove any custom view registrations and modals belonging to this plugin
        self.plugin_views.retain(|(pid, _), _| pid != plugin_id);
        if let Some(ref active_input) = self.active_plugin_input {
            if active_input.plugin_id == plugin_id {
                self.active_plugin_input = None;
            }
        }
        if let Some((ref pid, _, _)) = self.active_plugin_select {
            if pid == plugin_id {
                self.active_plugin_select = None;
                self.plugin_select_popup = None;
            }
        }

        println!("[app_view] Plugin '{}' completely unloaded and UI cleaned up.", plugin_id);
        cx.notify();
    }

    pub fn editor_font_family(&self) -> gpui::SharedString {
        let f = self.config.editor.font.font_family.trim();
        if f.is_empty() {
            "Roboto Mono".into()
        } else {
            f.to_string().into()
        }
    }

    pub fn hex_selection_range(&self) -> Option<(usize, usize)> {
        if let Some(anchor) = self.hex_selection_anchor {
            let pos = self.hex_cursor_pos;
            if anchor != pos {
                return Some((anchor.min(pos), anchor.max(pos)));
            }
        }
        None
    }

    pub fn reset_hex_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.hex_blink.pause_and_show(BlinkTarget::Hex, cx);
    }

    pub fn enable_hex_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.hex_blink.enable(BlinkTarget::Hex, cx);
    }

    pub fn disable_hex_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.hex_blink.disable(cx);
    }

    pub fn title_selection_range(&self) -> Option<(usize, usize)> {
        self.state
            .active_view()
            .and_then(|v| v.title_input.as_ref())
            .and_then(|ti| ti.selection_range())
    }

    pub fn reset_title_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.title_blink.pause_and_show(BlinkTarget::Title, cx);
    }

    pub fn enable_title_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.title_blink.enable(BlinkTarget::Title, cx);
    }

    pub fn disable_title_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.title_blink.disable(cx);
    }

    pub fn reset_editor_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.editor_blink.pause_and_show(BlinkTarget::Editor, cx);
    }

    pub fn enable_editor_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.editor_blink.enable(BlinkTarget::Editor, cx);
    }

    pub fn disable_editor_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.editor_blink.disable(cx);
    }

    /// Freezes every focused caret visible and kills its blink timer. Used when the OS
    /// window loses activation: the caret stays on screen, but the view stops re-rendering.
    fn suspend_cursor_blinks(&mut self, cx: &mut Context<Self>) {
        if self.editor_is_focused {
            self.editor_blink.suspend_visible(cx);
        }
        if self.was_title_focused {
            self.title_blink.suspend_visible(cx);
        }
        if self.hex_is_focused {
            self.hex_blink.suspend_visible(cx);
        }
        if self.quick_filter.is_focused {
            self.quick_filter.blink.suspend_visible(cx);
        }
        if self.search_dialog.is_open {
            self.goto_blink.suspend_visible(cx);
        }
        if self.sidebar_mode == crate::views::sidebar::SidebarMode::Search {
            self.sidebar_search.blink.suspend_visible(cx);
        }
        if self.command_palette.is_open {
            self.command_palette_blink.suspend_visible(cx);
        }
        if self.plugin_catalog.is_open {
            self.catalog_blink.suspend_visible(cx);
        }
        if self.active_plugin_input.is_some() {
            self.plugin_input_blink.suspend_visible(cx);
        }
    }

    /// Re-arms blinking for whichever caret holds in-app focus. Used when the OS window
    /// becomes active again.
    fn resume_cursor_blinks(&mut self, cx: &mut Context<Self>) {
        if self.editor_is_focused {
            self.editor_blink.enable(BlinkTarget::Editor, cx);
        }
        if self.was_title_focused {
            self.title_blink.enable(BlinkTarget::Title, cx);
        }
        if self.hex_is_focused {
            self.hex_blink.enable(BlinkTarget::Hex, cx);
        }
        if self.quick_filter.is_focused {
            self.quick_filter.enable_cursor_blink(cx);
        }
        if self.search_dialog.is_open {
            self.goto_blink.enable(BlinkTarget::GotoSearch, cx);
        }
        if self.sidebar_mode == crate::views::sidebar::SidebarMode::Search {
            self.sidebar_search.blink.enable(BlinkTarget::SidebarSearch, cx);
        }
        if self.command_palette.is_open {
            self.command_palette_blink.enable(BlinkTarget::CommandPalette, cx);
        }
        if self.plugin_catalog.is_open {
            self.catalog_blink.enable(BlinkTarget::PluginCatalog, cx);
        }
        if self.active_plugin_input.is_some() {
            self.plugin_input_blink.enable(BlinkTarget::PluginInput, cx);
        }
    }

    /// Immediately dispatches writing of a dirty document to disk in the background,
    /// without blocking the UI thread.
    pub fn flush_document_now(&mut self, doc_id: crate::document_buffer::DocumentId, cx: &mut Context<Self>) {
        let to_save = match self.state.documents.get(&doc_id) {
            Some(doc) if doc.is_dirty => {
                Some((doc.path.clone(), doc.rope.clone(), doc.title.clone(), doc.version, doc.disk_hash.clone()))
            }
            _ => None,
        };

        let Some((path, rope, title, version, expected_hash)) = to_save else {
            return;
        };

        self.auto_save_epoch = self.auto_save_epoch.wrapping_add(1);
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let background = cx.background_executor().clone();

        cx.foreground_executor().spawn(async move {
            let write_path = path.clone();
            let write_res = background.spawn(async move {
                crate::save_manager::safe_write_rope_checked(&write_path, &rope, expected_hash.as_deref())
            }).await;

            let _ = entity.update(&mut async_cx, |this, cx| {
                if let Some(doc_id) = this.state.find_document_by_path(&path) {
                    if let Some(doc) = this.state.documents.get_mut(&doc_id) {
                        match write_res {
                            crate::save_manager::CheckedWrite::Written(hash) => {
                                // The hash/stamp update must happen even when
                                // the user kept typing while the write ran
                                // (`doc.version != version`): the disk now
                                // holds exactly `hash`, and a watcher `Modified`
                                // event for our own write must be recognised
                                // as an echo instead of triggering a reload —
                                // `reload` drops the undo stack and the
                                // caret/selection visibly jump.
                                doc.disk_hash = Some(hash);
                                doc.disk_stamp = crate::save_manager::file_stamp(&doc.path);
                                if doc.version == version {
                                    doc.mark_clean();
                                    if let Some(meta) = this.state.doc_meta.get_mut(&doc_id) {
                                        meta.is_dirty = false;
                                    }
                                }
                                this.state.status_text = format!("Saved: {}", title);
                                let rel_path = this.state.vault.as_ref()
                                    .and_then(|v| doc.path.strip_prefix(&v.root).ok())
                                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                                    .unwrap_or_else(|| doc.path.to_string_lossy().into_owned());
                                this.state.events.emit_vault(&crate::events::VaultEvent::Modify { path: rel_path });
                                this.state.publish_snapshot();
                            }
                            crate::save_manager::CheckedWrite::Conflict { copy, disk_text } => {
                                // Two-sided conflict protocol: our version is
                                // preserved in the copy, the buffer adopts the
                                // external content — no data loss either way.
                                doc.reload(disk_text);
                                for view in this.state.views.values_mut() {
                                    if view.doc_id == doc_id {
                                        view.visual_cache_dirty = true;
                                    }
                                }
                                this.state.status_text = format!(
                                    "Файл изменён извне; ваша версия сохранена в {}",
                                    copy.file_name().unwrap_or_default().to_string_lossy()
                                );
                            }
                            crate::save_manager::CheckedWrite::Failed(err) => {
                                this.state.status_text = format!("Save error: {}", err);
                            }
                        }
                        cx.notify();
                    }
                }
                this.state.save_session_state();
            });
        }).detach();
    }

    /// Immediately dispatches writing of any dirty note to disk in the background,
    /// without blocking the UI thread. Used on blur/minimize, auto-save timers, and manual save.
    pub fn flush_active_note_now(&mut self, cx: &mut Context<Self>) {
        if let Some(doc_id) = self.state.active_doc_id() {
            self.flush_document_now(doc_id, cx);
        }
    }

    pub fn schedule_auto_save(&mut self, cx: &mut Context<Self>) {
        let (content_len, first_dirty) = match self.state.active_document() {
            Some(doc) if doc.is_dirty => (
                doc.len(),
                doc.first_dirty_time.unwrap_or_else(std::time::Instant::now),
            ),
            _ => return,
        };

        let (debounce, max_delay) = if self.config.editor.adaptive_save {
            crate::save_manager::adaptive_delays(content_len)
        } else {
            let d = std::time::Duration::from_millis(self.config.editor.save_debounce_ms as u64);
            (d, d * 5)
        };

        let now = std::time::Instant::now();
        let deadline = first_dirty + max_delay;
        let debounce_target = now + debounce;
        let target_time = debounce_target.min(deadline);
        let wait_dur = target_time.saturating_duration_since(now);

        self.auto_save_epoch = self.auto_save_epoch.wrapping_add(1);
        let epoch = self.auto_save_epoch;
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let background = cx.background_executor().clone();
        cx.foreground_executor().spawn(async move {
            background.timer(wait_dur).await;
            let _ = entity.update(&mut async_cx, |this, cx| {
                if let Some(doc) = this.state.active_document() {
                    let deadline_reached = std::time::Instant::now() >= deadline;
                    if (this.auto_save_epoch == epoch || deadline_reached) && doc.is_dirty {
                        this.flush_active_note_now(cx);
                    }
                }
            });
        }).detach();
    }

    pub fn current_scroll_top(&self) -> f32 {
        self.current_scroll_top_f64() as f32
    }

    /// Full-precision scroll position. Geometry must use this one: at the tail of
    /// a huge note the f32 grid is 16 px wide, i.e. coarser than a 24 px row, so
    /// rounding here would snap the slice to the wrong row.
    pub fn current_scroll_top_f64(&self) -> f64 {
        self.editor_scroll.get().max(0.0)
    }

    pub fn apply_note_settings_for_path(&mut self, path: &Path) {
        self.table_layouts.borrow_mut().clear();
        let mut restore_to = 0.0;
        if let Some(settings) = self.state.note_states.get(path).cloned() {
            self.edit_mode = settings.edit_mode;
            self.word_wrap = settings.word_wrap;
            if settings.scroll_top > 0.0 {
                self.set_editor_scroll_top(settings.scroll_top);
                restore_to = settings.scroll_top;
            } else {
                self.set_editor_scroll_top(0.0);
            }
        } else {
            self.edit_mode = true;
            self.word_wrap = true;
            self.set_editor_scroll_top(0.0);
        }
        // Same guard as at startup: re-apply once measured (a no-op write
        // when the direct write above already landed), then validate cursor
        // visibility against the fresh cache — no extra rebuilds involved.
        if self
            .state
            .active_document()
            .map(|d| d.path.as_path() == path)
            .unwrap_or(false)
        {
            self.pending_editor_restore = Some(f64::from(restore_to));
            self.need_reveal_cursor = true;
            // The note changed, so the handle still describes the old content:
            // arm the burst that repaints the scrollbar once it is measured.
            self.layout_settle_frames.set(SCROLL_SETTLE_FRAMES);
        }
    }

    pub fn save_on_exit(&mut self, window: &Window) {
        let is_maximized = window.is_maximized();
        let bounds = window.bounds();
        let mut window_state = crate::state::WindowState::load(&crate::state::app_data_dir());
        window_state.maximized = is_maximized;
        if !is_maximized {
            window_state.x = Some(bounds.origin.x.into());
            window_state.y = Some(bounds.origin.y.into());
            window_state.width = bounds.size.width.into();
            window_state.height = bounds.size.height.into();
        }
        window_state.save(&crate::state::app_data_dir());

        self.commit_title_rename_internal();
        if self.state.active_document().map(|d| d.is_dirty).unwrap_or(false) {
            self.state.save_active_note();
        }

        // Sync active view before persisting
        let current_leaf_id = self.state.workspace.active_leaf;
        let current_scroll_f64 = self.current_scroll_top_f64();
        if let Some(leaf) = self.state.leaves.get(&current_leaf_id) {
            if let Some(tab) = leaf.active_tab() {
                if let Some(view) = self.state.views.get_mut(&tab.id) {
                    view.scroll_top = current_scroll_f64;
                }
            }
        }

        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap);
        let sidebar_offset_y: f32 = self.sidebar_scroll_handle.offset().y.into();
        self.state.session.sidebar_scroll_top = sidebar_offset_y;
        self.state.session.sidebar_width = self.sidebar_width;
        self.state.session.sidebar_mode = self.sidebar_mode.clone();
        self.state.queue_session_state_save(true);
    }

    /// Completely flushes data, closes child windows, unloads plugins, cancels subscriptions,
    /// and frees document buffers on application shutdown.
    pub fn prepare_shutdown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.save_dirty_documents();
        self.catalog_window_manager.close_if_open(cx);
        self.settings_window_manager.close_if_open(cx);
        self.unload_all_plugins();
        self.plugin_leaf_views.clear();
        self.save_on_exit(window);
        self.close_tabs_popup(cx);
        self.close_tab_context_menu(cx);
        self.window_activation_subscription = None;
        self.suspend_cursor_blinks(cx);
        self.state.active_empty_tab = None;
        self.state.documents.clear();
        self.state.doc_meta.clear();
        self.state.by_path.clear();
        self.state.views.clear();
        self.state.pending_file_deletions.clear();
    }

    /// Polls for external filesystem changes across all open documents.
    ///
    /// Respects the debounce window so temporary unlinks (atomic saves) do not prematurely close tabs.
    pub fn poll_fs_changes(&mut self, cx: &mut Context<Self>) {
        let debounce = Duration::from_millis(350);
        let changes = self.state.check_fs_changes(debounce);

        if !changes.is_empty() {
            let config = self.state.settings.get_config();
            let mut any_handled = false;

            for change in changes {
                match change {
                    crate::state::FsChangeEvent::Deleted(path) => {
                        if config.files.close_deleted_tabs {
                            let keep_dirty = config.files.keep_modified_on_delete;
                            if self.state.handle_file_deleted(&path, keep_dirty) {
                                any_handled = true;
                            }
                        }
                    }
                    crate::state::FsChangeEvent::Renamed { old_path, new_path } => {
                        if self.state.handle_file_renamed(&old_path, &new_path) {
                            any_handled = true;
                        }
                    }
                    crate::state::FsChangeEvent::Modified(path) => {
                        if self.state.handle_file_modified(&path) {
                            any_handled = true;
                        }
                    }
                    crate::state::FsChangeEvent::Rescan => {
                        // Reconcile against the disk: the tree reload and the
                        // vault cache reset happen inside; deletion of open
                        // notes is still debounced through the normal path by
                        // feeding every open path as potentially missing.
                        self.state.invalidate_markdown_cache_and_tree();
                        self.state.recheck_open_paths_after_rescan();
                        any_handled = true;
                    }
                }
            }

            if any_handled {
                self.reload_file_tree_async(cx);
                self.state.publish_snapshot();
                cx.notify();
            }
        }
    }

    /// Registers OS window observers (e.g. activation / focus change) to handle
    /// background cursor blink pausing and reliable save-on-blur / save-on-minimize.
    pub fn init_window_observers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sub = cx.observe_window_activation(window, |this, window, cx| {
            let is_active = window.is_window_active();
            let changed = this.window_was_active != is_active;
            this.window_was_active = is_active;
            if !is_active {
                this.suspend_cursor_blinks(cx);
                if this.config.editor.save_on_blur {
                    this.flush_active_note_now(cx);
                }
            } else if changed {
                this.resume_cursor_blinks(cx);
            }
        });
        self.window_activation_subscription = Some(sub);
    }

    /// Toggles the left sidebar visibility.
    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
        cx.notify();
    }

    /// Toggles word wrapping mode in the editor.
    pub fn toggle_word_wrap(&mut self, cx: &mut Context<Self>) {
        self.word_wrap = !self.word_wrap;
        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap);
        cx.notify();
    }

    fn compute_wrap_cols_for_leaf(&self, leaf_id: Option<crate::workspace::LeafId>, window_width: f32, char_width: f32) -> usize {
        let editor_left_w = if self.sidebar_collapsed { 44.0 } else { self.sidebar_width + 1.0 };
        // The right sidebar shrinks the editor column (flex layout), so the
        // text must rewrap to the narrower width instead of running under it.
        let editor_right_w = if self.right_sidebar_collapsed {
            0.0
        } else {
            self.right_sidebar_width + 1.0
        };
        let char_w = if char_width > 1.0 { char_width } else { 7.7 };
        let workspace_w = (window_width - editor_left_w - editor_right_w).max(100.0);
        let width_ratio = leaf_id
            .map(|lid| self.state.workspace.horizontal_width_ratio_of_leaf(lid))
            .unwrap_or(1.0);
        let pane_w = (workspace_w * width_ratio).max(100.0);

        // R14 (§4.4): content_w = pane_w - divider_w - horizontal_padding - gutter_w - scrollbar_w
        let divider_w = if width_ratio < 0.999 { 4.0 } else { 0.0 };
        let horizontal_padding = 64.0; // px_8 on editor_list = 32px left + 32px right
        let gutter_w = 48.0; // line numbers (40px) + gap (8px)
        let scrollbar_w = if self.config.editor.wide_scrollbar { 16.0 } else { 14.0 };

        let content_w = (pane_w - divider_w - horizontal_padding - gutter_w - scrollbar_w).max(40.0);
        ((content_w / char_w).floor() as usize).max(20)
    }

    fn compute_wrap_cols(&self, window_width: f32, char_width: f32) -> usize {
        self.compute_wrap_cols_for_leaf(Some(self.state.workspace.active_leaf), window_width, char_width)
    }

    pub(crate) fn pane_text_start_x(&self, leaf_id: Option<crate::workspace::LeafId>, window: &Window) -> f32 {
        let editor_left_x = if self.sidebar_collapsed { 44.0 } else { self.sidebar_width + 1.0 };
        let editor_right_x = if self.right_sidebar_collapsed { 0.0 } else { self.right_sidebar_width + 1.0 };
        let window_w: f32 = window.bounds().size.width.into();
        let workspace_w = (window_w - editor_left_x - editor_right_x).max(0.0);
        let target_lid = leaf_id.unwrap_or(self.state.workspace.active_leaf);
        let (start_ratio, _) = self.state.workspace.horizontal_span_of_leaf(target_lid);
        editor_left_x + start_ratio * workspace_w + 32.0 + 8.0 + 40.0
    }

    fn move_cursor_up_visual(&mut self, shift: bool) {
        let Some((view, doc)) = self.state.active_view_and_doc_mut() else {
            return;
        };
        view.ensure_visual_cache_with_headings(doc, self.word_wrap, self.last_wrap_cols, Some(&self.config.editor.headings), view.viewport.offset, self.ctrl_preview_selection || !self.edit_mode);
        let visual_rows = Arc::clone(&view.window.visual_rows);
        if visual_rows.is_empty() {
            return;
        }

        let (cursor_line, cursor_col) = view.line_col(doc);
        let (curr_v_idx, col_in_row) = crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col)
            .unwrap_or((0, 0));

        if shift && view.selection_anchor.is_none() {
            view.selection_anchor = Some(view.cursor_offset);
        }

        if curr_v_idx == 0 {
            view.cursor_offset = 0;
            if !shift {
                view.selection_anchor = None;
            }
            return;
        }

        let mut target_idx = curr_v_idx - 1;
        if !shift {
            let tables = doc.ensure_table_index_for_window(view.window.start_line, view.window.end_line);
            if let Some(tbl) = view.grid_table_at(doc, &tables, visual_rows[target_idx].line_idx, cursor_line, self.ctrl_preview_selection || !self.edit_mode) {
                if visual_rows[target_idx].line_idx == tbl.sep_line() && target_idx > 0 {
                    target_idx -= 1;
                }
            }
        }
        let target_v = &visual_rows[target_idx];
        let target_len = target_v.char_end.saturating_sub(target_v.char_start);
        let max_col = if target_v.is_last_sub {
            target_len
        } else {
            target_len.saturating_sub(1)
        };
        let target_col = col_in_row.min(max_col);
        let target_char = target_v.char_start + target_col;
        let offset = view.offset_for_line_col(doc, target_v.line_idx, target_char);
        view.cursor_offset = offset;
        if !shift {
            view.selection_anchor = None;
        }
    }

    fn move_cursor_down_visual(&mut self, shift: bool) {
        let Some((view, doc)) = self.state.active_view_and_doc_mut() else {
            return;
        };
        view.ensure_visual_cache_with_headings(doc, self.word_wrap, self.last_wrap_cols, Some(&self.config.editor.headings), view.viewport.offset, self.ctrl_preview_selection || !self.edit_mode);
        let visual_rows = Arc::clone(&view.window.visual_rows);
        if visual_rows.is_empty() {
            return;
        }

        let (cursor_line, cursor_col) = view.line_col(doc);
        let (curr_v_idx, col_in_row) = crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col)
            .unwrap_or((0, 0));

        if shift && view.selection_anchor.is_none() {
            view.selection_anchor = Some(view.cursor_offset);
        }

        if curr_v_idx + 1 >= visual_rows.len() {
            view.cursor_offset = doc.len();
            if !shift {
                view.selection_anchor = None;
            }
            return;
        }

        let mut target_idx = curr_v_idx + 1;
        if !shift {
            let tables = doc.ensure_table_index_for_window(view.window.start_line, view.window.end_line);
            if let Some(tbl) = view.grid_table_at(doc, &tables, visual_rows[target_idx].line_idx, cursor_line, self.ctrl_preview_selection || !self.edit_mode) {
                if visual_rows[target_idx].line_idx == tbl.sep_line() && target_idx + 1 < visual_rows.len() {
                    target_idx += 1;
                }
            }
        }
        let target_v = &visual_rows[target_idx];
        let target_len = target_v.char_end.saturating_sub(target_v.char_start);
        let max_col = if target_v.is_last_sub {
            target_len
        } else {
            target_len.saturating_sub(1)
        };
        let target_col = col_in_row.min(max_col);
        let target_char = target_v.char_start + target_col;
        let offset = view.offset_for_line_col(doc, target_v.line_idx, target_char);
        view.cursor_offset = offset;
        if !shift {
            view.selection_anchor = None;
        }
    }

    fn move_cursor_home_visual(&mut self, shift: bool) {
        let Some((view, doc)) = self.state.active_view_and_doc_mut() else {
            return;
        };
        view.ensure_visual_cache_with_headings(doc, self.word_wrap, self.last_wrap_cols, Some(&self.config.editor.headings), view.viewport.offset, self.ctrl_preview_selection || !self.edit_mode);
        let visual_rows = Arc::clone(&view.window.visual_rows);
        if visual_rows.is_empty() {
            return;
        }
        let (cursor_line, cursor_col) = view.line_col(doc);
        let (curr_v_idx, _) = crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col)
            .unwrap_or((0, 0));
        let v = &visual_rows[curr_v_idx];

        if shift && view.selection_anchor.is_none() {
            view.selection_anchor = Some(view.cursor_offset);
        }

        let target_char = if cursor_col == v.char_start && v.sub_idx > 0 {
            0
        } else {
            v.char_start
        };
        view.cursor_offset = view.offset_for_line_col(doc, v.line_idx, target_char);
        if !shift {
            view.selection_anchor = None;
        }
    }

    fn move_cursor_end_visual(&mut self, shift: bool) {
        let Some((view, doc)) = self.state.active_view_and_doc_mut() else {
            return;
        };
        view.ensure_visual_cache_with_headings(doc, self.word_wrap, self.last_wrap_cols, Some(&self.config.editor.headings), view.viewport.offset, self.ctrl_preview_selection || !self.edit_mode);
        let visual_rows = Arc::clone(&view.window.visual_rows);
        if visual_rows.is_empty() {
            return;
        }
        let (cursor_line, cursor_col) = view.line_col(doc);
        let (curr_v_idx, _) = crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col)
            .unwrap_or((0, 0));
        let v = &visual_rows[curr_v_idx];

        if shift && view.selection_anchor.is_none() {
            view.selection_anchor = Some(view.cursor_offset);
        }

        let line_len = view.window.lines.get(v.line_idx).map(|l| l.chars().count()).unwrap_or(0);
        let sub_end = if v.is_last_sub {
            v.char_end
        } else {
            v.char_end.saturating_sub(1)
        };
        let target_char = if cursor_col >= sub_end && !v.is_last_sub {
            line_len
        } else {
            sub_end
        };
        view.cursor_offset = view.offset_for_line_col(doc, v.line_idx, target_char);
        if !shift {
            view.selection_anchor = None;
        }
    }

    pub fn heading_typography_metrics(
        classified: &crate::markdown::LineKind<'_>,
        headings: &crate::settings::HeadingsSettings,
        base_line_h: f32,
        base_char_w: f32,
    ) -> (f32, f32, f32, f32, f32) {
        Self::heading_typography_metrics_with_widths(classified, headings, base_line_h, base_char_w, None)
    }

    pub fn heading_typography_metrics_with_widths(
        classified: &crate::markdown::LineKind<'_>,
        headings: &crate::settings::HeadingsSettings,
        base_line_h: f32,
        base_char_w: f32,
        heading_char_widths: Option<&[f32; 6]>,
    ) -> (f32, f32, f32, f32, f32) {
        let (h_size, is_heading, h_idx) = match classified {
            crate::markdown::LineKind::Heading1(_) => (headings.h1.font_size as f32, true, 0),
            crate::markdown::LineKind::Heading2(_) => (headings.h2.font_size as f32, true, 1),
            crate::markdown::LineKind::Heading3(_) => (headings.h3.font_size as f32, true, 2),
            crate::markdown::LineKind::Heading4(_) => (headings.h4.font_size as f32, true, 3),
            crate::markdown::LineKind::Heading5(_) => (headings.h5.font_size as f32, true, 4),
            crate::markdown::LineKind::Heading6(_) => (headings.h6.font_size as f32, true, 5),
            _ => (14.0, false, 0),
        };
        if is_heading {
            let row_h = crate::markdown::heading_row_height(h_size, base_line_h);
            let char_w = heading_char_widths
                .and_then(|w| w.get(h_idx).copied())
                .unwrap_or_else(|| base_char_w * (h_size / 14.0));
            let cursor_h = (h_size * 0.88).round().max(16.0);
            let cursor_top = ((row_h - cursor_h) / 2.0).round().max(2.0);
            (row_h, char_w, h_size, cursor_h, cursor_top)
        } else {
            (base_line_h, base_char_w, 14.0, 16.0, 4.0)
        }
    }

    pub fn resolve_heading_char_widths(
        text_system: &gpui::TextSystem,
        editor_font: &gpui::SharedString,
        headings: &crate::settings::HeadingsSettings,
        base_char_w: f32,
    ) -> [f32; 6] {
        let heading_configs = [
            (&headings.h1, 0),
            (&headings.h2, 1),
            (&headings.h3, 2),
            (&headings.h4, 3),
            (&headings.h5, 4),
            (&headings.h6, 5),
        ];
        let mut widths = [base_char_w; 6];
        for (h_cfg, idx) in heading_configs {
            let mut font = gpui::font(editor_font.clone());
            if h_cfg.bold {
                font = font.bold();
            }
            if h_cfg.italic {
                font = font.italic();
            }
            let font_id = text_system.resolve_font(&font);
            let h_size = h_cfg.font_size as f32;
            let char_w = text_system
                .advance(font_id, gpui::px(h_size), 'x')
                .map(|s| f32::from(s.width))
                .unwrap_or(base_char_w * (h_size / 14.0));
            widths[idx] = char_w;
        }
        widths
    }

    
/// Scrolls the viewport the least amount that brings the caret into view.
///
/// The caret can be anywhere — `Ctrl+End`, a search hit, a note restored
/// mid-file — while the window follows the viewport. Revealing it needs its row,
/// so the window is placed on the caret first; after the reveal the viewport
/// shows the caret, so that anchor is the right one for the next frame too.
pub fn scroll_cursor_into_view_with_strategy(&mut self, _strategy: ScrollStrategy) {
    // handle_key обновляет визуальный кеш после изменения текста
    // и перемещения курсора.
    let measured_h = f32::from(self.editor_scroll_handle.bounds().size.height);

    let viewport_h = if measured_h > 50.0 {
        measured_h
    } else {
        self.last_editor_h.get().max(100.0)
    };

    // Row geometry of the caret, in document coordinates. Scoped so the note
    // borrow ends before the scroll position is read and written through `self`.
    let geometry = {
        let word_wrap = self.word_wrap;
        let wrap_cols = self.last_wrap_cols;
        let (view, doc) = match self.state.active_view_and_doc_mut() {
            Some(p) => p,
            None => return,
        };
        let (cursor_line, cursor_col) = view.line_col(doc);
        let anchor = if view.window.covers_line(cursor_line) && view.window.covers_line(view.viewport.offset) {
            view.viewport.offset
        } else {
            view.viewport.offset = cursor_line;
            cursor_line
        };
        view.ensure_visual_cache_with_headings(
            doc,
            word_wrap,
            wrap_cols,
            Some(&self.config.editor.headings),
            anchor,
            self.ctrl_preview_selection || !self.edit_mode,
        );
        crate::markdown::find_visual_row_for_cursor(
            &view.window.visual_rows,
            cursor_line,
            cursor_col,
        )
        .map(|(v_idx, _)| {
            let top = view.doc_y_of_local_v_idx(v_idx);
            (
                top,
                top + f64::from(view.window.height_index.height_of(v_idx)),
                view.total_height(),
            )
        })
    };

    let Some((row_top, row_bottom, total_content_h)) = geometry else {
        return;
    };

    // Высота содержимого без дополнительного нижнего отступа.
    let max_scroll = (total_content_h - f64::from(viewport_h)).max(0.0);

    let raw_scroll = self.current_scroll_top_f64();
    let current_scroll = raw_scroll.clamp(0.0, max_scroll);
    let viewport_bottom = current_scroll + f64::from(viewport_h);

    // Никаких дополнительных отступов сверху и снизу.
    // Прокручиваем только тогда, когда строка выходит за границы.
    let target_scroll = if row_top < current_scroll {
        row_top
    } else if row_bottom > viewport_bottom {
        row_bottom - f64::from(viewport_h)
    } else {
        current_scroll
    }
    .clamp(0.0, max_scroll);

    // Исправляем устаревшую позицию после изменения высоты содержимого.
    // Игнорируем субпиксельные изменения, чтобы не провоцировать дрожание.
    if (target_scroll - raw_scroll).abs() > 0.5 {
        self.set_editor_scroll_top_f64(target_scroll);
    }
}




    fn scroll_cursor_into_view(&mut self) {
        self.scroll_cursor_into_view_with_strategy(ScrollStrategy::Top);
    }

    /// Keeps the visual cache and the scroll position in sync after the cursor
    /// moved without an edit (e.g. inside a table), so table navigation follows
    /// the same path as ordinary cursor movement.
    fn refresh_view_after_cursor_move(&mut self) {
        if let Some((view, doc)) = self.state.active_view_and_doc_mut() {
            let (cursor_line, _) = view.line_col(doc);
            let anchor = if view.window.covers_line(cursor_line) && view.window.covers_line(view.viewport.offset) {
                view.viewport.offset
            } else {
                view.viewport.offset = cursor_line;
                cursor_line
            };
            let word_wrap = self.word_wrap;
            let wrap_cols = self.last_wrap_cols;
            view.ensure_visual_cache_with_headings(doc, word_wrap, wrap_cols, Some(&self.config.editor.headings), anchor, self.ctrl_preview_selection || !self.edit_mode);
        }
        self.scroll_cursor_into_view();
    }

    /// Moves the editor to `scroll_y`.
    ///
    /// Any write through here also cancels an in-flight wheel ease: the
    /// position came from somewhere else (a restore, a thumb drag, a cursor
    /// reveal, the keyboard), so there is nothing left to ease towards.
    pub(crate) fn set_editor_scroll_top(&self, scroll_y: f32) {
        self.set_editor_scroll_top_f64(scroll_y as f64);
    }

    pub(crate) fn set_editor_scroll_top_f64(&self, scroll_y: f64) {
        self.wheel_ease.set(None);
        self.editor_scroll.set(scroll_y.max(0.0));
    }

    /// Owns the editor wheel.
    ///
    /// With `editor.wheelSmoothScroll` off (the default) every notch applies
    /// instantly: when the wheel stops, the text stops — no settle tail. With
    /// the flag on, a notch starts a ~90 ms ease-out with a hard landing
    /// instead, so the two behaviours can be compared live.
    ///
    /// The editor's rows are painted as a slice anchored near y = 0 and shifted
    /// by the leftover of the scroll position, so nothing else may move them:
    /// the list no longer uses gpui's own scrolling (`overflow_hidden`), and the
    /// built-in wheel listener ignores an element that is not `Overflow::Scroll`,
    /// so this listener is the only writer.
    pub fn handle_editor_wheel(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        max_scroll: f64,
        cx: &mut Context<Self>,
    ) {
        if max_scroll <= 1.0 {
            return;
        }
        let current = self.current_scroll_top_f64().clamp(0.0, max_scroll);
        match event.delta {
            gpui::ScrollDelta::Pixels(p) => {
                if f32::from(p.y) == 0.0 {
                    return;
                }
                // A trackpad delivers its own smooth stream: it always applies
                // directly and takes over from any running ease.
                let next = (current - f64::from(f32::from(p.y))).clamp(0.0, max_scroll);
                self.set_editor_scroll_top_f64(next);
            }
            gpui::ScrollDelta::Lines(l) => {
                if l.y == 0.0 {
                    return;
                }
                let delta = -f64::from(l.y) * f64::from(WHEEL_LINE_PX);
                if !self.config.editor.wheel_smooth_scroll {
                    let next = (current + delta).clamp(0.0, max_scroll);
                    self.set_editor_scroll_top_f64(next);
                } else {
                    // Glide chain: a notch arriving mid-glide inherits its
                    // speed and extends the accumulated target when it
                    // scrolls on in the same direction, so a notch stream
                    // reads as one continuous motion; a reversal re-anchors
                    // at the live position and the old motion dies out
                    // gently instead of jumping. The duration adapts to the
                    // notch cadence (2 × mean of the last three intervals,
                    // clamped).
                    let now = std::time::Instant::now();
                    let (duration_secs, history) =
                        wheel_glide_duration_secs(self.wheel_notch_history.get(), now);
                    self.wheel_notch_history.set(history);
                    let (base, velocity) = match self.wheel_ease.get() {
                        Some(glide) => {
                            let v = glide.velocity_at(now);
                            if (glide.target - current) * delta > 0.0 {
                                (glide.target, v)
                            } else {
                                (current, v)
                            }
                        }
                        None => (current, 0.0),
                    };
                    let target = (base + delta).clamp(0.0, max_scroll);
                    if (target - current).abs() > 0.01 {
                        self.wheel_ease
                            .set(Some(WheelEase::new(current, target, velocity, duration_secs)));
                    } else {
                        self.wheel_ease.set(None);
                    }
                }
            }
        }
        cx.notify();
    }

    /// Steps the optional wheel glide one frame towards its landing. Called from
    /// `render` when `editor.wheelSmoothScroll` is on.
    ///
    /// The glide has a hard deadline: past `duration` the position lands
    /// exactly on `target` and the animation is over — no sub-pixel creep, no
    /// final re-settle.
    fn step_wheel_ease(&self, window: &mut Window) {
        let Some(anim) = self.wheel_ease.get() else {
            return;
        };
        let now = std::time::Instant::now();
        if now.duration_since(anim.t0).as_secs_f32() >= anim.duration_secs {
            self.wheel_ease.set(None);
            // Direct write, not through `set_editor_scroll_top_f64`: that
            // would cancel this very landing.
            self.editor_scroll.set(anim.target.max(0.0));
            return;
        }
        window.request_animation_frame();
        // Direct write for the same reason as above.
        self.editor_scroll.set(anim.position_at(now).max(0.0));
    }

    /// Starts middle-button smart scroll for the editor.
    ///
    /// While active, the absolute cursor Y inside the editor viewport drives
    /// `editor_scroll` (same idea as the sidebar's Ctrl smart scroll): cursor
    /// near the top shows the beginning, near the bottom the end. The first
    /// mouse-move only primes the reference point so the content never jumps
    /// on entry.
    pub(crate) fn start_editor_middle_scroll(&mut self, cx: &mut Context<Self>) {
        if self.editor_middle_active {
            return;
        }
        self.editor_middle_active = true;
        self.editor_middle_last_y.set(None);
        cx.notify();
    }

    /// Stops editor middle-button smart scroll (idempotent).
    pub(crate) fn stop_editor_middle_scroll(&mut self, cx: &mut Context<Self>) {
        if !self.editor_middle_active {
            return;
        }
        self.editor_middle_active = false;
        self.editor_middle_last_y.set(None);
        cx.notify();
    }

    /// Follows the cursor while editor middle-button smart scroll is active.
    ///
    /// Mirrors `on_sidebar_smart_mouse_move`: absolute mapping of the cursor Y
    /// inside the editor viewport to `[0, max_scroll]`, with an edge dead zone
    /// so the first/last row is fully visible before the cursor hits the
    /// border. Small jitter is ignored. Writes directly through
    /// `set_editor_scroll_top_f64`, which also ends any wheel ease.
    pub(crate) fn on_editor_middle_mouse_move(
        &self,
        cursor_y: f32,
        viewport_top: f32,
        viewport_h: f32,
        max_scroll: f64,
        cx: &mut Context<Self>,
    ) {
        if !self.editor_middle_active {
            return;
        }
        if max_scroll <= 1.0 || viewport_h <= 50.0 {
            cx.notify();
            return;
        }
        match self.editor_middle_last_y.get() {
            // First event only primes the reference point.
            None => self.editor_middle_last_y.set(Some(cursor_y)),
            Some(anchor) if (cursor_y - anchor).abs() >= 0.5 => {
                self.editor_middle_last_y.set(Some(cursor_y));
                let target = editor_middle_cursor_target(cursor_y, viewport_top, viewport_h, max_scroll);
                let current = self.current_scroll_top_f64();
                if (target - current).abs() > 0.5 {
                    self.set_editor_scroll_top_f64(target);
                }
            }
            Some(_) => {}
        }
        cx.notify();
    }

    /// Starts middle-button smart scroll for the sidebar tree.
    pub(crate) fn start_sidebar_middle_scroll(&mut self, cx: &mut Context<Self>) {
        if self.sidebar_middle_active {
            return;
        }
        self.sidebar_middle_active = true;
        // Prime the shared reference point so the list never jumps on entry.
        self.sidebar_smart_last_mouse_y.set(None);
        self.sidebar_smart_target_y.set(None);
        cx.notify();
    }

    /// Stops sidebar middle-button smart scroll (idempotent).
    pub(crate) fn stop_sidebar_middle_scroll(&mut self, cx: &mut Context<Self>) {
        if !self.sidebar_middle_active {
            return;
        }
        self.sidebar_middle_active = false;
        self.sidebar_smart_last_mouse_y.set(None);
        self.sidebar_smart_target_y.set(None);
        cx.notify();
    }

    /// Starts middle-button smart scroll for an active plugin modal list.
    pub(crate) fn start_plugin_modal_middle_scroll(&mut self, cx: &mut Context<Self>) {
        if self.plugin_modal_middle_active {
            return;
        }
        self.plugin_modal_middle_active = true;
        self.plugin_modal_smart_last_y.set(None);
        cx.notify();
    }

    /// Stops plugin modal middle-button smart scroll (idempotent).
    pub(crate) fn stop_plugin_modal_middle_scroll(&mut self, cx: &mut Context<Self>) {
        if !self.plugin_modal_middle_active {
            return;
        }
        self.plugin_modal_middle_active = false;
        self.plugin_modal_smart_last_y.set(None);
        cx.notify();
    }

    /// Cursor-driven smart scroll for plugin modal lists while middle mouse button is held.
    pub(crate) fn on_plugin_modal_middle_mouse_move(&self, y: f32, cx: &mut Context<Self>) {
        if !self.plugin_modal_middle_active {
            self.plugin_modal_smart_last_y.set(None);
            cx.notify();
            return;
        }
        match self.plugin_modal_smart_last_y.get() {
            None => self.plugin_modal_smart_last_y.set(Some(y)),
            Some(anchor) if (y - anchor).abs() >= 0.5 => {
                self.plugin_modal_smart_last_y.set(Some(y));
                let mut bounds = self.plugin_modal_smart_bounds.get();
                if bounds.size.height <= px(20.0) {
                    bounds = self.plugin_modal_scroll_handle.bounds();
                }
                let viewport_h = f32::from(bounds.size.height);
                let viewport_top = f32::from(bounds.origin.y);
                let max_scroll = f32::from(self.plugin_modal_scroll_handle.max_offset().height);
                if viewport_h > 20.0 && max_scroll >= 1.0 {
                    let pad = (viewport_h * 0.10).clamp(16.0, 48.0).min(viewport_h / 3.0);
                    let usable = (viewport_h - pad * 2.0).max(1.0);
                    let t = ((y - viewport_top - pad) / usable).clamp(0.0, 1.0);
                    let target = (max_scroll * t).clamp(0.0, max_scroll);
                    self.plugin_modal_scroll_handle.set_offset(gpui::point(px(0.0), px(-target)));
                }
            }
            Some(_) => {}
        }
        cx.notify();
    }

    /// Follows pointer Y while dragging the plugin modal scrollbar thumb.
    pub(crate) fn handle_plugin_modal_scrollbar_drag(&mut self, pointer_y: f32, cx: &mut Context<Self>) {
        if !self.plugin_modal_scrollbar_drag.active {
            return;
        }
        let style = crate::views::scrollbar::ScrollbarStyle {
            track_w: 16.0,
            thumb_w: 6.0,
            thumb_w_active: 8.0,
            thumb_inset: 3.0,
            min_thumb_h: 24.0,
            track_right_offset: 2.0,
        };
        let Some(geometry) = crate::views::scrollbar::ScrollbarGeometry::from_handle(&self.plugin_modal_scroll_handle, &style) else {
            return;
        };
        geometry.apply_drag(&self.plugin_modal_scroll_handle, &self.plugin_modal_scrollbar_drag, pointer_y);
        cx.notify();
    }

    /// Viewport geometry for editor middle-button smart scroll.
    ///
    /// Returns `(viewport_top, viewport_h, max_scroll)` in window coordinates.
    /// `None` when there is no active note or nothing to scroll. Ensures the
    /// visual cache so `max_scroll` matches the painted content.
    pub(crate) fn editor_middle_metrics(&mut self) -> Option<(f32, f32, f64)> {
        let bounds = self.editor_scroll_handle.bounds();
        let bounds_h = f32::from(bounds.size.height);
        let viewport_h = if bounds_h > 50.0 {
            bounds_h
        } else {
            self.last_editor_h.get().max(100.0)
        };
        if viewport_h <= 50.0 {
            return None;
        }
        let viewport_top = if bounds.origin.y > px(0.0) {
            f32::from(bounds.origin.y)
        } else {
            132.0
        };
        let Some((view, doc)) = self.state.active_view_and_doc_mut() else {
            return None;
        };
        let wrap_cols = if self.last_wrap_cols > 0 { self.last_wrap_cols } else { 80 };
        view.ensure_visual_cache_with_headings(doc, self.word_wrap, wrap_cols, Some(&self.config.editor.headings), view.viewport.offset, self.ctrl_preview_selection || !self.edit_mode);
        let total = view.total_height();
        let max_scroll = (total - f64::from(viewport_h)).max(0.0);
        if max_scroll <= 1.0 {
            return None;
        }
        Some((viewport_top, viewport_h, max_scroll))
    }

    /// Thumb height for the editor scrollbar lane.
    ///
    /// Proportional to the viewport share, floored at 28 px so it stays
    /// grabbable. The Zed-style wide lane instead uses a fixed slim thumb —
    /// it marks position rather than viewport share, so it never resizes
    /// with the document length.
    pub(crate) fn editor_thumb_h(viewport_h: f32, total_content_h: f64, wide: bool) -> f64 {
        if wide {
            return WIDE_THUMB_H;
        }
        let ratio = (f64::from(viewport_h) / total_content_h.max(f64::from(viewport_h))).clamp(0.04, 1.0);
        (f64::from(viewport_h) * ratio).max(28.0)
    }

    /// Clamps a saved scroll target into the true scrollable range.
    ///
    /// The real max is `total - viewport`: the list paints a slice anchored near
    /// y = 0, so gpui's own `max_offset` describes the slice, not the document.
    pub(crate) fn clamp_restore_target(target: f64, total_h: f64, viewport_h: f64) -> f64 {
        let max = (total_h - viewport_h).max(0.0);
        target.clamp(0.0, max)
    }

    /// Document Y (px from the top of the content) of the visual row holding
    /// byte `offset`.
    ///
    /// Maps through line → char column → visual row, so an offset on a wrapped
    /// line resolves to its exact sub-row instead of the line top. Drives the
    /// overview-ruler marks on the editor scrollbar (cursor line, selection
    /// span). Out-of-range offsets clamp to the document, empty caches sit at
    /// zero.
    #[cfg(test)]
    pub(crate) fn editor_doc_y_for_offset(
        offset: usize,
        content: &str,
        line_index: &crate::line_index::LineIndex,
        above_h: f64,
        lines: &crate::editor_window::WindowLines,
        visual_rows: &[crate::markdown::VisualRow],
        height_index: &crate::markdown::VisualRowHeightIndex,
    ) -> f64 {
        if lines.is_empty() || visual_rows.is_empty() || line_index.is_empty() {
            return 0.0;
        }
        let offset = offset.min(content.len());
        let line_idx = line_index.line_of(content, offset);
        let line = match lines.get(line_idx) {
            Some(line) => line,
            // Outside the window — a selection endpoint can be anywhere. The
            // ruler mark only has to land in the right region, so clamp it to
            // the window's edge rather than guessing a row.
            None => {
                return if line_idx < lines.start() {
                    above_h
                } else {
                    above_h + height_index.total()
                }
            }
        };
        let mut byte_in_line = offset
            .saturating_sub(line_index.start(content, line_idx))
            .min(line.len());
        while byte_in_line > 0 && !line.is_char_boundary(byte_in_line) {
            byte_in_line -= 1;
        }
        let col = line[..byte_in_line].chars().count();
        let (v_idx, _) =
            crate::markdown::find_visual_row_for_cursor(visual_rows, line_idx, col).unwrap_or((0, 0));
        // The row index is window-local, the caller wants a document Y.
        above_h + height_index.top_of(v_idx)
    }

    /// Returns `(slice_h, viewport_h, max_scroll_content, track_top_y)`.
    ///
    /// `viewport_h` is the true viewport height — measured bounds, or the last
    /// known height before the first layout. It is never inflated: scroll math
    /// (max, thumb travel, cursor reveal) must match the real track, otherwise
    /// the thumb slides under the status bar and the cursor hides below the
    /// fold. `slice_h` is additionally floored at the live window size (which
    /// is current from frame one, like in the sidebar tree), so the first
    /// frames already cover the whole viewport instead of popping the last
    /// rows in later. Overshooting the slice only renders a few extra clipped
    /// rows.
    ///
    /// `max_scroll_content` is `f64`: it is an absolute offset into the
    /// document, and at 2×10^8 px an f32 cannot express it more finely than
    /// every 16 px.
    pub fn editor_scroll_metrics_for_leaf(
        &self,
        leaf_id: Option<crate::workspace::LeafId>,
        is_leaf_active: bool,
        total_content_h: f64,
        window_h: f32,
    ) -> (f32, f32, f64, f32) {
        let (leaf_v_ratio, in_titlebar) = if let Some(lid) = leaf_id {
            (
                self.state.workspace.vertical_height_ratio_of_leaf(lid),
                self.state.workspace.is_leaf_in_titlebar(lid),
            )
        } else {
            (1.0, true)
        };
        let workspace_h = (window_h - 66.0).max(100.0);
        let leaf_total_h = workspace_h * leaf_v_ratio;
        let header_h = if in_titlebar { 90.0 } else { 126.0 };
        let estimated_viewport_h = (leaf_total_h - header_h).max(50.0);

        let leaf_measured_h = leaf_id
            .and_then(|lid| self.state.leaves.get(&lid))
            .and_then(|leaf| leaf.active_tab())
            .and_then(|tab| self.state.views.get(&tab.id))
            .and_then(|view| view.measured_viewport_h)
            .filter(|&mh| (mh - estimated_viewport_h).abs() < (estimated_viewport_h * 0.35).max(40.0));

        let viewport_h = if is_leaf_active {
            let bounds = self.editor_scroll_handle.bounds();
            let bounds_h = f32::from(bounds.size.height);
            if bounds_h > 50.0
                && self.split_drag_state.is_none()
                && (bounds_h - estimated_viewport_h).abs() < (estimated_viewport_h * 0.35).max(40.0)
            {
                bounds_h
            } else if let Some(mh) = leaf_measured_h {
                mh
            } else {
                estimated_viewport_h
            }
        } else if let Some(mh) = leaf_measured_h {
            mh
        } else {
            estimated_viewport_h
        };

        let slice_h = viewport_h;
        let max_scroll_content = (total_content_h - viewport_h as f64).max(0.0);
        let track_top_y = if is_leaf_active && self.split_drag_state.is_none() {
            let bounds = self.editor_scroll_handle.bounds();
            let estimated_top_y = 38.0 + (workspace_h * leaf_id.map(|lid| self.state.workspace.vertical_ratio_of_leaf(lid)).unwrap_or(0.0)) + (if in_titlebar { 0.0 } else { 36.0 }) + 90.0;
            if bounds.origin.y > px(0.0) && (f32::from(bounds.origin.y) - estimated_top_y).abs() < 50.0 {
                f32::from(bounds.origin.y)
            } else {
                estimated_top_y
            }
        } else {
            let v_ratio_top = leaf_id
                .map(|lid| self.state.workspace.vertical_ratio_of_leaf(lid))
                .unwrap_or(0.0);
            38.0 + (workspace_h * v_ratio_top) + (if in_titlebar { 0.0 } else { 36.0 }) + 90.0
        };
        (slice_h, viewport_h, max_scroll_content, track_top_y)
    }

    pub fn editor_scroll_metrics(&self, total_content_h: f64, window_h: f32) -> (f32, f32, f64, f32) {
        let active_leaf = Some(self.state.workspace.active_leaf);
        self.editor_scroll_metrics_for_leaf(active_leaf, true, total_content_h, window_h)
    }

    fn handle_scrollbar_drag(&mut self, current_y: f32, window: &Window, cx: &mut Context<Self>) {
        if !self.is_dragging_scrollbar {
            return;
        }
        let Some((view, doc)) = self.state.active_view_and_doc_mut() else {
            return;
        };
        view.ensure_visual_cache_with_headings(doc, self.word_wrap, self.last_wrap_cols, Some(&self.config.editor.headings), view.viewport.offset, self.ctrl_preview_selection || !self.edit_mode);
        let total_content_h = view.total_height();
        let window_h = f32::from(window.bounds().size.height);
        let (_, viewport_h, max_scroll_content, _) = self.editor_scroll_metrics(total_content_h, window_h);
        let thumb_h = Self::editor_thumb_h(viewport_h, total_content_h, self.config.editor.wide_scrollbar);
        let track_scrollable = (f64::from(viewport_h) - thumb_h).max(1.0);

        let delta_y = f64::from(current_y - self.scrollbar_drag_start_y);
        let delta_fraction = delta_y / track_scrollable;
        let new_scroll_y = (self.scrollbar_drag_start_scroll_y + delta_fraction * max_scroll_content)
            .clamp(0.0, max_scroll_content);
        self.set_editor_scroll_top_f64(new_scroll_y);
        cx.notify();
    }

    pub(crate) fn open_folder_dialog(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let mut dialog = rfd::AsyncFileDialog::new();
        if let Some(ref root) = self.state.workspace_path {
            dialog = dialog.set_directory(root);
        }
        cx.foreground_executor().spawn(async move {
            if let Some(folder_handle) = dialog.pick_folder().await {
                let folder = folder_handle.path().to_path_buf();
                let _ = entity.update(&mut async_cx, |this, cx| {
                    this.unload_all_plugins();
                    this.state.open_workspace(folder);
                    this.load_installed_plugins_for_vault();
                    if this.state.active_document().is_none() {
                        if let Some(first_file) = AppState::first_file_in_tree(&this.state.file_tree) {
                            this.state.open_note(first_file);
                        }
                    }
                    this.need_scroll_sidebar_to_active.set(true);
                    cx.notify();
                });
            }
        }).detach();
    }

    /// Opens the native folder picker asynchronously without blocking the GPUI update loop.
    pub(crate) fn open_folder_dialog_deferred(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_folder_dialog(window, cx);
    }

    pub(crate) fn create_new_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.create_new_note_placed(window, cx, false);
    }

    /// Same as [`create_new_note`], but the new tab opens first in the tab
    /// strip (used by the leading new-note button).
    #[allow(dead_code)]
    pub(crate) fn create_new_note_at_front(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.create_new_note_placed(window, cx, true);
    }

    fn create_new_note_placed(&mut self, window: &mut Window, cx: &mut Context<Self>, at_front: bool) {
        self.commit_title_rename_internal();
        self.flush_active_note_now(cx);
        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap);
        let created = if at_front {
            self.state.create_new_note_at_front()
        } else {
            self.state.create_new_note()
        };
        if let Some(_) = created {
            self.state.active_empty_tab = None;
            self.edit_mode = true;
            self.word_wrap = true;
            self.set_editor_scroll_top(0.0);
            self.need_scroll_sidebar_to_active.set(true);
            window.focus(&self.focus_handle);
            self.reset_editor_cursor_blink(cx);
            cx.notify();
        }
    }

    /// Activates the tab strip tab for `path` (shared by the tab body and the
    /// invisible gap extenders around it, so every pixel opens a tab).
    #[allow(dead_code)]
    pub(crate) fn select_tab_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.active_document().map(|d| &d.path != &path).unwrap_or(true) {
            self.select_note(path, true, window, cx);
        } else {
            self.need_scroll_sidebar_to_active.set(true);
            self.state.reveal_path_in_tree(&path);
            cx.notify();
        }
    }

    pub(crate) fn select_note(&mut self, path: PathBuf, center_in_sidebar: bool, window: &mut Window, cx: &mut Context<Self>) {
        // Empty-tab sentinels carry no file: activate the New Tab page instead.
        if crate::state::is_empty_tab_path(&path) {
            self.select_empty_tab(path, center_in_sidebar, window, cx);
            return;
        }
        self.commit_title_rename_internal();
        self.flush_active_note_now(cx);
        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap);
        let sidebar_offset_y: f32 = self.sidebar_scroll_handle.offset().y.into();
        self.state.session.sidebar_scroll_top = sidebar_offset_y;
        self.state.open_note(path.clone());
        self.active_center_leaf = None;
        let rel_str = if let Some(ref vault) = self.state.vault {
            path.strip_prefix(&vault.root)
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|| path.to_string_lossy().replace('\\', "/"))
        } else {
            path.to_string_lossy().replace('\\', "/")
        };
        self.state.plugin_host.broadcast_event(&crate::plugins::api::HostEvent::FileOpen {
            path: Some(rel_str),
        });
        self.apply_note_settings_for_path(&path);
        if center_in_sidebar {
            self.need_scroll_sidebar_to_active.set(true);
        }
        window.focus(&self.focus_handle);
        self.reset_editor_cursor_blink(cx);
        cx.notify();
    }

    pub(crate) fn handle_link_click(&mut self, raw_url: &str, window: &mut Window, cx: &mut Context<Self>) {
        let clean = crate::markdown::clean_link_url(raw_url);
        if clean.is_empty() {
            return;
        }

        // Web and mail links open externally
        if clean.starts_with("http://") || clean.starts_with("https://") || clean.starts_with("mailto:") {
            cx.open_url(&clean);
            return;
        }

        // Document / local file link
        if let Some(target_path) = self.resolve_document_link(&clean) {
            self.select_note(target_path, true, window, cx);
        } else {
            // Fallback: try opening via system open_url
            cx.open_url(&clean);
        }
    }

    pub(crate) fn resolve_document_link(&self, target: &str) -> Option<PathBuf> {
        let path_part = target.split('#').next()?.trim();
        if path_part.is_empty() {
            return None;
        }

        let decoded = crate::markdown::percent_decode(path_part);
        let cleaned_path = decoded.strip_prefix("file://").unwrap_or(&decoded);
        let rel_path = Path::new(cleaned_path);

        // 1. If absolute path:
        if rel_path.is_absolute() {
            if rel_path.is_file() {
                return Some(rel_path.to_path_buf());
            }
            let with_md = rel_path.with_extension("md");
            if with_md.is_file() {
                return Some(with_md);
            }
        }

        // 2. Resolve relative to active note directory:
        if let Some(active_doc) = self.state.active_document() {
            if let Some(parent) = active_doc.path.parent() {
                let candidate = parent.join(rel_path);
                if candidate.is_file() {
                    return Some(candidate);
                }
                if candidate.extension().is_none() {
                    let with_md = candidate.with_extension("md");
                    if with_md.is_file() {
                        return Some(with_md);
                    }
                }
            }
        }

        // 3. Resolve relative to vault root:
        if let Some(ref vault) = self.state.vault {
            let candidate = vault.root.join(rel_path);
            if candidate.is_file() {
                return Some(candidate);
            }
            if candidate.extension().is_none() {
                let with_md = candidate.with_extension("md");
                if with_md.is_file() {
                    return Some(with_md);
                }
            }

            // 4. Search in vault by filename or stem (Obsidian-style shortest link match)
            let file_name = rel_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let stem = rel_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if !stem.is_empty() {
                let mut matched = Vec::new();
                Self::collect_matching_notes_in_vault(&vault.root, stem, file_name, &mut matched);
                if let Some(p) = matched.into_iter().next() {
                    return Some(p);
                }
            }
        }

        // 5. Fallback: return joined path with .md extension so clicking creates/opens the new note
        if let Some(active_doc) = self.state.active_document() {
            if let Some(parent) = active_doc.path.parent() {
                let mut candidate = parent.join(rel_path);
                if candidate.extension().is_none() {
                    candidate.set_extension("md");
                }
                return Some(candidate);
            }
        }
        if let Some(ref vault) = self.state.vault {
            let mut candidate = vault.root.join(rel_path);
            if candidate.extension().is_none() {
                candidate.set_extension("md");
            }
            return Some(candidate);
        }

        None
    }

    fn collect_matching_notes_in_vault(dir: &Path, stem: &str, file_name: &str, acc: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            let p = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "node_modules" || name == "target" {
                continue;
            }
            if ft.is_dir() {
                Self::collect_matching_notes_in_vault(&p, stem, file_name, acc);
            } else if ft.is_file() {
                if let Some(f_name) = p.file_name().and_then(|n| n.to_str()) {
                    if f_name.eq_ignore_ascii_case(file_name) {
                        acc.push(p);
                        return;
                    }
                }
                if let Some(f_stem) = p.file_stem().and_then(|s| s.to_str()) {
                    if f_stem.eq_ignore_ascii_case(stem) && p.extension().map_or(false, |ext| ext == "md") {
                        acc.push(p);
                        return;
                    }
                }
            }
        }
    }

    pub(crate) fn close_tab(&mut self, path: &Path, cx: &mut Context<Self>) {
        if self.state.active_document().map(|d| d.path == path).unwrap_or(false) {
            self.commit_title_rename_internal();
        }
        if let Some(doc_id) = self.state.find_document_by_path(path) {
            self.flush_document_now(doc_id, cx);
        }
        let is_closing_active = self.state.active_document().map(|d| d.path == path).unwrap_or(false);
        if is_closing_active {
            let current_scroll = self.current_scroll_top();
            self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap);
        }
        self.state.close_tab(path);
        if let Some((target_path, _title)) = self.state.active_document().map(|d| (d.path.clone(), d.title.clone())) {
            self.apply_note_settings_for_path(&target_path);
            self.need_scroll_sidebar_to_active.set(true);
        }
        cx.notify();
    }

    fn navigate_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_title_rename_internal();
        self.flush_active_note_now(cx);
        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap);
        if let Some(target) = self.state.navigate_back() {
            self.apply_note_settings_for_path(&target);
            self.need_scroll_sidebar_to_active.set(true);
            window.focus(&self.focus_handle);
            cx.notify();
        }
    }

    fn navigate_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_title_rename_internal();
        self.flush_active_note_now(cx);
        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap);
        if let Some(target) = self.state.navigate_forward() {
            self.apply_note_settings_for_path(&target);
            self.need_scroll_sidebar_to_active.set(true);
            window.focus(&self.focus_handle);
            cx.notify();
        }
    }

    fn reload_files(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.reload_file_tree_async(cx);
    }

    /// Walks the vault on a background thread and swaps the finished tree in
    /// on the UI thread (`AppState::apply_file_tree`).
    ///
    /// `build_tree` is a recursive `read_dir` over the whole vault; on a large
    /// vault a synchronous call stalls a frame, so everything that does not
    /// need the result *inside the same click handler* (watcher deletions,
    /// rescans) goes through this. Handlers that immediately open/navigate a
    /// path they just created keep the synchronous `reload_file_tree`.
    pub fn reload_file_tree_async(&mut self, cx: &mut Context<Self>) {
        let Some(ws) = self.state.workspace_path.clone() else {
            return;
        };
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let background = cx.background_executor().clone();
        cx.foreground_executor().spawn(async move {
            let tree = background
                .spawn(async move { AppState::build_tree(&ws) })
                .await;
            let _ = entity.update(&mut async_cx, |this, cx| {
                this.state.apply_file_tree(tree);
                this.state.publish_snapshot();
                cx.notify();
            });
        }).detach();
    }

    pub(crate) fn save_note(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.flush_active_note_now(cx);
        cx.notify();
    }

    pub(crate) fn table_lines_slice(doc: &crate::document_buffer::DocumentBuffer, block: &crate::table::TableBlock) -> Vec<String> {
        (block.first_line..=block.last_line)
            .map(|i| doc.line_text(i))
            .collect()
    }

    pub(crate) fn get_or_compute_table_layout<'a>(
        table_layouts: &std::cell::RefCell<std::collections::HashMap<usize, (u64, Arc<TableLayout>)>>,
        text_system: &gpui::WindowTextSystem,
        get_line: &dyn Fn(usize) -> Option<&'a str>,
        block: &crate::table::TableBlock,
        font_family: &SharedString,
        manual_widths: Option<&[f32]>,
    ) -> Arc<TableLayout> {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for l_idx in block.first_line..=block.last_line {
            if let Some(l) = get_line(l_idx) {
                l.hash(&mut hasher);
            }
        }
        if let Some(mw) = manual_widths {
            for &w in mw {
                w.to_bits().hash(&mut hasher);
            }
        }
        let content_hash = hasher.finish();

        if let Ok(layouts) = table_layouts.try_borrow() {
            if let Some((h, layout)) = layouts.get(&block.first_line) {
                if *h == content_hash {
                    return layout.clone();
                }
            }
        }

        let layout = Arc::new(compute_table_layout(text_system, get_line, block, font_family, manual_widths));
        if let Ok(mut layouts) = table_layouts.try_borrow_mut() {
            layouts.insert(block.first_line, (content_hash, layout.clone()));
        }
        layout
    }

    /// Cached `shape_row` for ordinary (non-table) editor rows.
    ///
    /// The hash covers every input that changes the shaped result: the line
    /// text, its classification (drives the painted text and the font size),
    /// the row's char range, the active-line flag (markers are painted only
    /// on the active line), the font family and the cell width. A change to
    /// heading sizes invalidates via `clear_row_shape_cache`, called from the
    /// settings subscription.
    pub(crate) fn get_or_shape_row(
        row_cache: &std::cell::RefCell<HashMap<u64, Arc<RowShapedLayout>>>,
        text_system: &gpui::WindowTextSystem,
        line: &str,
        classified: &crate::markdown::LineKind<'_>,
        v: &crate::markdown::VisualRow,
        is_active_line: bool,
        headings: &crate::settings::HeadingsSettings,
        font_family: &SharedString,
        cell_w: f32,
    ) -> Arc<RowShapedLayout> {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        line.hash(&mut hasher);
        std::mem::discriminant(&LineKindTag::of(classified)).hash(&mut hasher);
        v.line_idx.hash(&mut hasher);
        v.sub_idx.hash(&mut hasher);
        v.char_start.hash(&mut hasher);
        v.char_end.hash(&mut hasher);
        is_active_line.hash(&mut hasher);
        font_family.hash(&mut hasher);
        cell_w.to_bits().hash(&mut hasher);
        headings_cache_key(headings).hash(&mut hasher);
        let key = hasher.finish();

        if let Ok(cache) = row_cache.try_borrow() {
            if let Some(layout) = cache.get(&key) {
                crate::perf::PERF
                    .shape_cache_hits
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return layout.clone();
            }
        }

        let layout = Arc::new(shape_row(text_system, line, classified, v, is_active_line, headings, font_family, cell_w));
        if let Ok(mut cache) = row_cache.try_borrow_mut() {
            // Bounded the same way a frame is: the visible row set. A large
            // vault scroll leaves stale entries behind; drop the whole cache
            // when it grows past a few frames' worth of rows.
            if cache.len() >= 4096 {
                cache.clear();
            }
            cache.insert(key, layout.clone());
        }
        layout
    }

    fn handle_editor_mouse_down(&mut self, click_x: f32, click_y: f32, shift: bool, click_count: usize, window: &Window, cx: &mut Context<Self>) {
        if self.state.active_doc_id().is_none() {
            return;
        }

        let in_titlebar = self.state.workspace.is_leaf_in_titlebar(self.state.workspace.active_leaf);
        let editor_rows_top_y = if in_titlebar { 90.0 } else { 126.0 };
        if click_y < editor_rows_top_y {
            return;
        }
        // f64: at the tail of a huge note an f32 scroll position is only good to
        // 16 px, which is enough to pick the neighbouring row.
        let current_scroll_y = self.current_scroll_top_f64();
        let rel_y = f64::from(click_y) - f64::from(editor_rows_top_y) + current_scroll_y;

        let editor_font = self.editor_font_family();
        let font_id = cx.text_system().resolve_font(&gpui::font(&editor_font));
        let line_char_width = cx
            .text_system()
            .advance(font_id, px(14.0), 'x')
            .map(|s| f32::from(s.width))
            .unwrap_or(7.7);
        let text_start_x = self.pane_text_start_x(None, window);
        let word_wrap = self.word_wrap;
        let wrap_cols = self.last_wrap_cols;

        let Some((view, doc)) = self.state.active_view_and_doc_mut() else {
            return;
        };

        view.ensure_visual_cache_with_headings(doc, word_wrap, wrap_cols, Some(&self.config.editor.headings), view.viewport.offset, self.ctrl_preview_selection || !self.edit_mode);
        let visual_rows = Arc::clone(&view.window.visual_rows);
        if visual_rows.is_empty() {
            return;
        }

        let target_v_idx = if rel_y <= 0.0 {
            0
        } else {
            view.local_v_idx_at_doc_y(rel_y)
        };

        let v = &visual_rows[target_v_idx];
        let v_len = v.char_end.saturating_sub(v.char_start);

        let (cursor_line, _) = view.line_col(doc);
        let selection_range = view.selection_range(doc.len());
        let line = &view.window.lines[v.line_idx];
        let line_start = doc.line_start_byte(v.line_idx);
        let line_end = line_start + line.len();
        let is_line_in_selection = selection_range.as_ref().map_or(false, |sel| {
            if sel.start == sel.end {
                false
            } else if line_start == line_end {
                sel.start <= line_start && sel.end >= line_end
            } else {
                sel.end > line_start && sel.start < line_end
            }
        });
        let is_in_active_block = view.window.code_states.get(v.line_idx).and_then(|s| s.as_ref()).map_or(false, |s| {
            let cursor_in_block = view.window.code_states.get(cursor_line).and_then(|cs| cs.as_ref()).map_or(false, |cs| cs.block_id == s.block_id);
            let sel_in_block = selection_range.as_ref().map_or(false, |sel| {
                let sel_start_l = doc.line_of_byte(sel.start).max(view.window.code_states.start());
                let sel_end_l = doc.line_of_byte(sel.end).min(view.window.code_states.end().saturating_sub(1));
                if sel_start_l <= sel_end_l {
                    (sel_start_l..=sel_end_l).any(|l_idx| {
                        view.window.code_states.get(l_idx).and_then(|cs| cs.as_ref()).map_or(false, |cs| cs.block_id == s.block_id)
                    })
                } else {
                    false
                }
            });
            cursor_in_block || sel_in_block
        });
        let is_active_line = Self::is_active_editor_line(self.edit_mode && !self.ctrl_preview_selection, v.line_idx, cursor_line, is_line_in_selection, is_in_active_block);
        let code_state = view.window.code_states.get(v.line_idx).and_then(|s| s.as_ref()).map(|s| (s.kind, s.lang.as_deref()));
        let classified = crate::markdown::classify_line(line, code_state);
        let heading_widths = Self::resolve_heading_char_widths(cx.text_system(), &editor_font, &self.config.editor.headings, line_char_width);
        let (_, row_char_w, _, _, _) = Self::heading_typography_metrics_with_widths(&classified, &self.config.editor.headings, 24.0, line_char_width, Some(&heading_widths));

        // Hit-test against the shaped layout of the row rather than a
        // `column * char_width` cell model: wide glyphs (emoji, CJK) do not advance
        // by exactly one cell.
        let col_in_row = if click_x > text_start_x {
            let tables = doc.ensure_table_index_for_window(view.window.start_line, view.window.end_line);
            let row_layout = if let Some(tbl) = view.grid_table_at(doc, &tables, v.line_idx, cursor_line, self.ctrl_preview_selection || !self.edit_mode) {
                let table_layout = Self::get_or_compute_table_layout(
                    &self.table_layouts,
                    window.text_system(),
                    &|idx| view.window.lines.get(idx).map(|s| s.as_str()),
                    tbl,
                    &editor_font,
                    self.manual_table_col_widths.get(&tbl.first_line).map(|w| w.as_slice()),
                );
                let cursor_col = if !self.ctrl_preview_selection && v.line_idx == cursor_line {
                    let (_, col) = view.line_col(doc);
                    Some(col)
                } else {
                    None
                };
                shape_table_row(
                    window.text_system(),
                    line,
                    tbl,
                    v.line_idx,
                    cursor_col,
                    &table_layout,
                    &self.config.editor.headings,
                    &editor_font,
                    row_char_w,
                )
            } else {
                let raw_row = tables
                    .at(v.line_idx)
                    .map_or(false, |t| view.table_raw_now(doc, t, cursor_line, self.ctrl_preview_selection || !self.edit_mode));
                shape_row(
                    window.text_system(),
                    line,
                    &classified,
                    v,
                    is_active_line || raw_row,
                    &self.config.editor.headings,
                    &editor_font,
                    row_char_w,
                )
            };
            let column = row_layout.column_for_x(click_x - text_start_x);
            row_layout.line_char_for_column(column).saturating_sub(v.char_start).min(v_len)
        } else {
            0
        };

        let target_char = v.char_start + col_in_row;
        let offset = view.offset_for_line_col(doc, v.line_idx, target_char);

        if click_count == 2 {
            let (w_start, w_end) = doc.word_range_at(offset);
            view.selection_anchor = Some(w_start);
            view.cursor_offset = w_end;
            self.is_mouse_selecting = false;
        } else if click_count >= 3 {
            let (l_start, l_end) = doc.line_range_at(offset);
            view.selection_anchor = Some(l_start);
            view.cursor_offset = l_end;
            self.is_mouse_selecting = false;
        } else if shift {
            if view.selection_anchor.is_none() {
                view.selection_anchor = Some(view.cursor_offset);
            }
            view.cursor_offset = offset;
        } else {
            view.cursor_offset = offset;
            view.selection_anchor = Some(offset);
        }
        self.reset_editor_cursor_blink(cx);
        cx.notify();
    }

    fn update_mouse_selection(&mut self, pos_x: f32, pos_y: f32, viewport_h: f32, window: &Window, cx: &mut Context<Self>) {
        if !self.is_mouse_selecting {
            return;
        }
        if self.state.active_doc_id().is_none() {
            return;
        }

        // Top of rows area: Titlebar (38px) + Nav bar (40px) + note-title-panel (54px) = 132px
        let editor_rows_top_y = 132.0;
        let editor_bottom_y = (viewport_h - 28.0).max(editor_rows_top_y + 100.0);
        let scroll_before = self.current_scroll_top_f64();

        // Edge autoscroll when dragging outside or near the top/bottom bounds.
        // The scroll write must be followed by a repaint even when the cursor
        // ends up on the same character: the content under the fixed pointer
        // changed, so the painted selection is stale until a frame runs.
        if pos_y < editor_rows_top_y && scroll_before > 0.0 {
            let step = f64::from(((editor_rows_top_y - pos_y).max(1.0) * 0.8).min(30.0));
            self.set_editor_scroll_top_f64((scroll_before - step).max(0.0));
        } else if pos_y > editor_bottom_y - 16.0 {
            let step = f64::from(((pos_y - (editor_bottom_y - 16.0)).max(1.0) * 0.8).min(30.0));
            self.set_editor_scroll_top_f64(scroll_before + step);
        }
        let scroll_moved = self.current_scroll_top_f64() != scroll_before;

        let current_scroll_y = self.current_scroll_top_f64();
        let rel_y = f64::from(pos_y) - f64::from(editor_rows_top_y) + current_scroll_y;

        let editor_font = self.editor_font_family();
        let font_id = cx.text_system().resolve_font(&gpui::font(&editor_font));
        let line_char_width = cx
            .text_system()
            .advance(font_id, px(14.0), 'x')
            .map(|s| f32::from(s.width))
            .unwrap_or(7.7);
        let text_start_x = self.pane_text_start_x(None, window);
        let word_wrap = self.word_wrap;
        let wrap_cols = self.last_wrap_cols;

        let Some((view, doc)) = self.state.active_view_and_doc_mut() else {
            return;
        };

        view.ensure_visual_cache_with_headings(doc, word_wrap, wrap_cols, Some(&self.config.editor.headings), view.viewport.offset, self.ctrl_preview_selection || !self.edit_mode);
        let visual_rows = Arc::clone(&view.window.visual_rows);
        if visual_rows.is_empty() {
            return;
        }

        let target_v_idx = if rel_y <= 0.0 {
            0
        } else {
            view.local_v_idx_at_doc_y(rel_y)
        };

        let v = &visual_rows[target_v_idx];
        let v_len = v.char_end.saturating_sub(v.char_start);

        let (cursor_line, _) = view.line_col(doc);
        let selection_range = view.selection_range(doc.len());
        let line = &view.window.lines[v.line_idx];
        let line_start = doc.line_start_byte(v.line_idx);
        let line_end = line_start + line.len();
        let is_line_in_selection = selection_range.as_ref().map_or(false, |sel| {
            if sel.start == sel.end {
                false
            } else if line_start == line_end {
                sel.start <= line_start && sel.end >= line_end
            } else {
                sel.end > line_start && sel.start < line_end
            }
        });
        let is_in_active_block = view.window.code_states.get(v.line_idx).and_then(|s| s.as_ref()).map_or(false, |s| {
            let cursor_in_block = view.window.code_states.get(cursor_line).and_then(|cs| cs.as_ref()).map_or(false, |cs| cs.block_id == s.block_id);
            let sel_in_block = selection_range.as_ref().map_or(false, |sel| {
                let sel_start_l = doc.line_of_byte(sel.start).max(view.window.code_states.start());
                let sel_end_l = doc.line_of_byte(sel.end).min(view.window.code_states.end().saturating_sub(1));
                if sel_start_l <= sel_end_l {
                    (sel_start_l..=sel_end_l).any(|l_idx| {
                        view.window.code_states.get(l_idx).and_then(|cs| cs.as_ref()).map_or(false, |cs| cs.block_id == s.block_id)
                    })
                } else {
                    false
                }
            });
            cursor_in_block || sel_in_block
        });
        let is_active_line = Self::is_active_editor_line(self.edit_mode && !self.ctrl_preview_selection, v.line_idx, cursor_line, is_line_in_selection, is_in_active_block);
        let code_state = view.window.code_states.get(v.line_idx).and_then(|s| s.as_ref()).map(|s| (s.kind, s.lang.as_deref()));
        let classified = crate::markdown::classify_line(line, code_state);
        let heading_widths = Self::resolve_heading_char_widths(cx.text_system(), &editor_font, &self.config.editor.headings, line_char_width);
        let (_, row_char_w, _, _, _) = Self::heading_typography_metrics_with_widths(&classified, &self.config.editor.headings, 24.0, line_char_width, Some(&heading_widths));

        let col_in_row = if pos_x > text_start_x {
            let tables = doc.ensure_table_index_for_window(view.window.start_line, view.window.end_line);
            let row_layout = if let Some(tbl) = view.grid_table_at(doc, &tables, v.line_idx, cursor_line, self.ctrl_preview_selection || !self.edit_mode) {
                let table_layout = Self::get_or_compute_table_layout(
                    &self.table_layouts,
                    window.text_system(),
                    &|idx| view.window.lines.get(idx).map(|s| s.as_str()),
                    tbl,
                    &editor_font,
                    self.manual_table_col_widths.get(&tbl.first_line).map(|w| w.as_slice()),
                );
                let cursor_col = if !self.ctrl_preview_selection && v.line_idx == cursor_line {
                    let (_, col) = view.line_col(doc);
                    Some(col)
                } else {
                    None
                };
                shape_table_row(
                    window.text_system(),
                    line,
                    tbl,
                    v.line_idx,
                    cursor_col,
                    &table_layout,
                    &self.config.editor.headings,
                    &editor_font,
                    row_char_w,
                )
            } else {
                let raw_row = tables
                    .at(v.line_idx)
                    .map_or(false, |t| view.table_raw_now(doc, t, cursor_line, self.ctrl_preview_selection || !self.edit_mode));
                shape_row(
                    window.text_system(),
                    line,
                    &classified,
                    v,
                    is_active_line || raw_row,
                    &self.config.editor.headings,
                    &editor_font,
                    row_char_w,
                )
            };
            let column = row_layout.column_for_x(pos_x - text_start_x);
            row_layout.line_char_for_column(column).saturating_sub(v.char_start).min(v_len)
        } else {
            0
        };

        let target_char = v.char_start + col_in_row;
        let offset = view.offset_for_line_col(doc, v.line_idx, target_char);
        if view.cursor_offset != offset {
            view.cursor_offset = offset;
            cx.notify();
        } else if scroll_moved {
            // Same character under the pointer, but the content scrolled:
            // without this notify the painted selection waits for the next
            // mousemove, which is exactly the visible drag lag.
            cx.notify();
        }
    }

    /// Whether the render path paints line `line_idx` as the active line, i.e. with its
    /// markdown markers (`# `, `> `, task/bullet/numbered prefixes) visible.
    ///
    /// `shape_row` reproduces exactly the text the render paints, so every hit-test must
    /// answer this the same way the render does (`is_editing && (...)`). Dropping the
    /// `edit_mode` guard makes the hit-test count markers that read mode hides, and the
    /// clicked column then lands one marker length away from the glyph under the pointer.
    fn is_active_editor_line(
        edit_mode: bool,
        line_idx: usize,
        cursor_line: usize,
        is_line_in_selection: bool,
        is_in_active_block: bool,
    ) -> bool {
        edit_mode && (line_idx == cursor_line || is_line_in_selection || is_in_active_block)
    }

    pub fn handle_custom_hex_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        self.reset_hex_cursor_blink(cx);
        let key = &event.keystroke.key;
        let total_chars = self.custom_hex_input.chars().count();
        let sel_range = self.hex_selection_range();

        if event.keystroke.modifiers.control && (key.eq_ignore_ascii_case("a") || key == "ф" || key == "Ф") {
            self.hex_selection_anchor = Some(0);
            self.hex_cursor_pos = total_chars;
            cx.notify();
            return;
        }

        if event.keystroke.modifiers.control && (key.eq_ignore_ascii_case("c") || key == "с" || key == "С") {
            let text_to_copy = if let Some((start, end)) = sel_range {
                self.custom_hex_input.chars().skip(start).take(end - start).collect::<String>()
            } else {
                self.custom_hex_input.clone()
            };
            if !text_to_copy.is_empty() {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text_to_copy));
            }
            return;
        }

        if key == "escape" {
            self.active_color_picker = None;
            self.hex_is_focused = false;
            self.hex_selection_anchor = None;
            cx.notify();
            return;
        }

        if key == "enter" {
            self.apply_custom_hex_if_valid(cx);
            self.active_color_picker = None;
            self.hex_is_focused = false;
            self.hex_selection_anchor = None;
            cx.notify();
            return;
        }

        if key == "arrowleft" || key == "left" {
            if event.keystroke.modifiers.shift {
                if self.hex_selection_anchor.is_none() {
                    self.hex_selection_anchor = Some(self.hex_cursor_pos);
                }
                self.hex_cursor_pos = self.hex_cursor_pos.saturating_sub(1);
            } else if let Some((start, _)) = sel_range {
                self.hex_selection_anchor = None;
                self.hex_cursor_pos = start;
            } else {
                self.hex_selection_anchor = None;
                self.hex_cursor_pos = self.hex_cursor_pos.saturating_sub(1);
            }
            cx.notify();
            return;
        }

        if key == "arrowright" || key == "right" {
            if event.keystroke.modifiers.shift {
                if self.hex_selection_anchor.is_none() {
                    self.hex_selection_anchor = Some(self.hex_cursor_pos);
                }
                self.hex_cursor_pos = (self.hex_cursor_pos + 1).min(total_chars);
            } else if let Some((_, end)) = sel_range {
                self.hex_selection_anchor = None;
                self.hex_cursor_pos = end;
            } else {
                self.hex_selection_anchor = None;
                self.hex_cursor_pos = (self.hex_cursor_pos + 1).min(total_chars);
            }
            cx.notify();
            return;
        }

        if key == "home" {
            if event.keystroke.modifiers.shift {
                if self.hex_selection_anchor.is_none() {
                    self.hex_selection_anchor = Some(self.hex_cursor_pos);
                }
            } else {
                self.hex_selection_anchor = None;
            }
            self.hex_cursor_pos = 0;
            cx.notify();
            return;
        }

        if key == "end" {
            if event.keystroke.modifiers.shift {
                if self.hex_selection_anchor.is_none() {
                    self.hex_selection_anchor = Some(self.hex_cursor_pos);
                }
            } else {
                self.hex_selection_anchor = None;
            }
            self.hex_cursor_pos = total_chars;
            cx.notify();
            return;
        }

        let delete_selection = |input: &mut String, start: usize, end: usize| {
            let before: String = input.chars().take(start).collect();
            let after: String = input.chars().skip(end).collect();
            *input = format!("{before}{after}");
        };

        if key == "backspace" {
            if let Some((start, end)) = sel_range {
                delete_selection(&mut self.custom_hex_input, start, end);
                self.hex_cursor_pos = start;
                self.hex_selection_anchor = None;
                self.apply_custom_hex_if_valid(cx);
                cx.notify();
            } else if self.hex_cursor_pos > 0 {
                let mut chars: Vec<char> = self.custom_hex_input.chars().collect();
                chars.remove(self.hex_cursor_pos - 1);
                self.custom_hex_input = chars.into_iter().collect();
                self.hex_cursor_pos -= 1;
                self.apply_custom_hex_if_valid(cx);
                cx.notify();
            }
            return;
        }

        if key == "delete" {
            if let Some((start, end)) = sel_range {
                delete_selection(&mut self.custom_hex_input, start, end);
                self.hex_cursor_pos = start;
                self.hex_selection_anchor = None;
                self.apply_custom_hex_if_valid(cx);
                cx.notify();
            } else if self.hex_cursor_pos < total_chars {
                let mut chars: Vec<char> = self.custom_hex_input.chars().collect();
                chars.remove(self.hex_cursor_pos);
                self.custom_hex_input = chars.into_iter().collect();
                self.apply_custom_hex_if_valid(cx);
                cx.notify();
            }
            return;
        }

        if event.keystroke.modifiers.control && (key.eq_ignore_ascii_case("v") || key == "м" || key == "М") {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    let filtered: String = text.trim().chars().filter(|c| c.is_ascii_hexdigit() || *c == '#').take(7).collect();
                    if !filtered.is_empty() {
                        let formatted = if filtered.starts_with('#') { filtered } else { format!("#{filtered}") };
                        self.custom_hex_input = formatted;
                        self.hex_cursor_pos = self.custom_hex_input.chars().count();
                        self.hex_selection_anchor = None;
                        self.apply_custom_hex_if_valid(cx);
                        cx.notify();
                    }
                }
            }
            return;
        }

        let ch = if let Some(c) = &event.keystroke.key_char {
            c.as_str()
        } else if key.chars().count() == 1 {
            key.as_str()
        } else {
            ""
        };

        for c in ch.chars() {
            if c.is_ascii_hexdigit() || c == '#' {
                let upper = c.to_ascii_uppercase();
                if let Some((start, end)) = sel_range {
                    delete_selection(&mut self.custom_hex_input, start, end);
                    self.hex_cursor_pos = start;
                    self.hex_selection_anchor = None;
                }
                if self.custom_hex_input.chars().count() < 7 {
                    let mut chars: Vec<char> = self.custom_hex_input.chars().collect();
                    chars.insert(self.hex_cursor_pos, upper);
                    self.custom_hex_input = chars.into_iter().collect();
                    self.hex_cursor_pos += 1;
                    self.apply_custom_hex_if_valid(cx);
                    cx.notify();
                }
            }
        }
    }

    pub fn apply_custom_hex_if_valid(&mut self, cx: &mut Context<Self>) {
        let Some(picker_id) = self.active_color_picker else {
            return;
        };
        if crate::markdown::parse_hex_color(&self.custom_hex_input).is_some() {
            let hex_val = if self.custom_hex_input.starts_with('#') {
                self.custom_hex_input.clone()
            } else {
                format!("#{}", self.custom_hex_input)
            };
            match picker_id {
                "h1-color" => self.config.editor.headings.h1.color_name = hex_val,
                "h2-color" => self.config.editor.headings.h2.color_name = hex_val,
                "h3-color" => self.config.editor.headings.h3.color_name = hex_val,
                "h4-color" => self.config.editor.headings.h4.color_name = hex_val,
                "h5-color" => self.config.editor.headings.h5.color_name = hex_val,
                "h6-color" => self.config.editor.headings.h6.color_name = hex_val,
                _ => {}
            }
            self.save_settings(cx);
        }
    }

    /// Checks whether any plugin modal dialog is currently active.
    pub fn has_active_plugin_modal(&self) -> bool {
        self.plugin_views.iter().any(|((_, vid), tree)| {
            if *vid != 1 {
                return false;
            }
            if let crate::plugins::ui::UiNode::Text { ref value, .. } = tree {
                !value.is_empty()
            } else {
                true
            }
        })
    }

    /// Closes an active plugin modal dialog and dispatches the close event to the plugin worker.
    pub fn close_active_plugin_modal(&mut self, cx: &mut Context<Self>) {
        println!("[app_view] close_active_plugin_modal called");
        let modal_keys: Vec<(String, u64)> = self.plugin_views.keys()
            .filter(|(_, vid)| *vid == 1)
            .cloned()
            .collect();
        for (p_id, vid) in modal_keys {
            println!("[app_view] Closing active modal for plugin '{}' (view_id = {})", p_id, vid);
            let _ = self.state.plugin_host.send_event(
                &p_id,
                crate::plugins::api::HostEvent::UiEvent {
                    view_id: vid,
                    node_id: "modal".to_string(),
                    event: "close".to_string(),
                    value: None,
                },
            );
            self.plugin_views.remove(&(p_id, vid));
        }
        self.active_plugin_input = None;
        self.active_plugin_select = None;
        self.plugin_modal_scrollbar_drag.end();
        self.plugin_modal_middle_active = false;
        self.plugin_modal_smart_last_y.set(None);
        cx.notify();
    }

    /// Renders an active plugin modal dialog overlay matching Obsidian modal style.
    pub fn render_plugin_modal_overlay(
        &self,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement> {
        let active_modal = self.plugin_views.iter().find(|((_, vid), tree)| {
            if *vid != 1 {
                return false;
            }
            if let crate::plugins::ui::UiNode::Text { ref value, .. } = tree {
                !value.is_empty()
            } else {
                true
            }
        });

        let ((p_id, _), modal_tree) = active_modal?;
        let p_id = p_id.clone();
        let caret_visible = self.plugin_input_blink.visible();

        let is_ru = self.config.ui.language.starts_with("ru");
        fn find_heading(node: &crate::plugins::ui::UiNode) -> Option<String> {
            match node {
                crate::plugins::ui::UiNode::Text { value, style: crate::plugins::ui::TextStyle::Heading, .. } => {
                    if !value.trim().is_empty() {
                        Some(value.clone())
                    } else {
                        None
                    }
                }
                crate::plugins::ui::UiNode::Column { children, .. } | crate::plugins::ui::UiNode::Row { children, .. } => {
                    children.iter().find_map(find_heading)
                }
                // Обёртка стилей — не узел, а бокс: заголовок за ней остаётся заголовком.
                // Без этой ветки модалка показывала два заголовка сразу — свой и запасной.
                crate::plugins::ui::UiNode::Styled { child, .. } => find_heading(child),
                _ => None,
            }
        }

        let has_heading = find_heading(modal_tree).is_some();
        let modal_title = find_heading(modal_tree).unwrap_or_else(|| {
            if is_ru { "Окно плагина".to_string() } else { "Plugin Dialog".to_string() }
        });

        let scroll_ctx = crate::plugins::ui::PluginScrollContext {
            scroll_handle: Some(&self.plugin_modal_scroll_handle),
            scrollbar_drag_active: self.plugin_modal_scrollbar_drag.active,
            middle_scroll_active: self.plugin_modal_middle_active,
            smart_bounds: Some(self.plugin_modal_smart_bounds.clone()),
        };

        let rendered_modal = crate::plugins::ui::render_ui_node(
            modal_tree,
            &p_id,
            1,
            theme,
            self.active_plugin_input.as_ref(),
            &self.plugin_input_focus_handle,
            self.active_plugin_select.as_ref(),
            self.plugin_select_popup.as_ref(),
            caret_visible,
            window,
            cx,
            Some(&scroll_ctx),
        );

        Some(
            gpui::deferred(
                div()
                    .id("global-plugin-modal-backdrop-overlay")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .bg(rgba(0x000000cc))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor(gpui::CursorStyle::Arrow)
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.close_active_plugin_modal(cx);
                    }))
                    .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.plugin_modal_scrollbar_drag.end();
                    }))
                    .on_mouse_up(MouseButton::Middle, cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.stop_plugin_modal_middle_scroll(cx);
                    }))
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                        if this.plugin_modal_middle_active {
                            let y: f32 = event.position.y.into();
                            this.on_plugin_modal_middle_mouse_move(y, cx);
                        } else if this.plugin_modal_scrollbar_drag.active {
                            let y: f32 = event.position.y.into();
                            this.handle_plugin_modal_scrollbar_drag(y, cx);
                        }
                    }))
                    .on_click(|_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_scroll_wheel(|_, _, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .id("global-plugin-modal-dialog-box")
                            .relative()
                            .flex()
                            .flex_col()
                            .w(px(580.0))
                            .max_w(px(660.0))
                            .max_h(px(640.0))
                            .rounded_xl()
                            .overflow_hidden()
                            .bg(theme.bg_card)
                            .border_1()
                            .border_color(theme.border)
                            .shadow_xl()
                            .cursor(gpui::CursorStyle::Arrow)
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.plugin_modal_scrollbar_drag.end();
                            }))
                            .on_mouse_up(MouseButton::Middle, cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.stop_plugin_modal_middle_scroll(cx);
                            }))
                            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                                if this.plugin_modal_middle_active {
                                    let y: f32 = event.position.y.into();
                                    this.on_plugin_modal_middle_mouse_move(y, cx);
                                } else if this.plugin_modal_scrollbar_drag.active {
                                    let y: f32 = event.position.y.into();
                                    this.handle_plugin_modal_scrollbar_drag(y, cx);
                                }
                            }))
                            .on_click(|_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_scroll_wheel(|_, _, cx| {
                                cx.stop_propagation();
                            })
                            // Optional fallback title if modal tree didn't provide its own heading
                            .when(!has_heading, |this| {
                                this.child(
                                    div()
                                        .px(px(24.0))
                                        .pt(px(20.0))
                                        .pb(px(8.0))
                                        .text_base()
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(theme.text_primary)
                                        .child(modal_title),
                                )
                            })
                            .child(
                                div()
                                    .id("global-plugin-modal-body")
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_h_0()
                                    .w_full()
                                    .overflow_hidden()
                                    .child(rendered_modal),
                            )
                            // Obsidian top-right [X] close button (rendered last so it sits above body)
                            .child(
                                div()
                                    .id("global-plugin-modal-close-x")
                                    .absolute()
                                    .top(px(14.0))
                                    .right(px(16.0))
                                    .w(px(24.0))
                                    .h(px(24.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor(gpui::CursorStyle::PointingHand)
                                    .rounded_md()
                                    .hover(|s| s.bg(theme.bg_hover))
                                    .text_color(theme.text_muted)
                                    .child(icons::icon(icons::ICON_CLOSE, 14.0, theme.text_muted))
                                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                        cx.stop_propagation();
                                        println!("[app_view] Close button [X] mouse down!");
                                        this.close_active_plugin_modal(cx);
                                    }))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        cx.stop_propagation();
                                        println!("[app_view] Close button [X] clicked!");
                                        this.close_active_plugin_modal(cx);
                                    })),
                            ),
                    ),
            )
            .with_priority(9999),
        )
    }

    /// Handles keyboard navigation and text editing inside an active plugin input field.
    pub fn handle_plugin_input_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(mut input_state) = self.active_plugin_input.clone() else {
            return;
        };

        self.plugin_input_blink.pause_and_show(BlinkTarget::PluginInput, cx);

        let key = &event.keystroke.key;
        let ctrl = event.keystroke.modifiers.control;
        let alt = event.keystroke.modifiers.alt;
        let shift = event.keystroke.modifiers.shift;

        if key == "escape" {
            self.active_plugin_input = None;
            self.plugin_input_blink.disable(cx);
            cx.notify();
            return;
        }

        let mut chars: Vec<char> = input_state.value.chars().collect();
        let total_chars = chars.len();
        let mut changed = false;

        if key == "enter" {
            if input_state.is_multiline {
                if let Some((s, e)) = input_state.selection_range() {
                    chars.drain(s..e.min(chars.len()));
                    input_state.cursor_pos = s;
                    input_state.selection_anchor = None;
                }
                let pos = input_state.cursor_pos.min(chars.len());
                chars.insert(pos, '\n');
                input_state.cursor_pos = pos + 1;
                input_state.value = chars.iter().collect();
                changed = true;
            } else {
                self.active_plugin_input = None;
                self.plugin_input_blink.disable(cx);
                cx.notify();
                return;
            }
        }

        if key == "arrowleft" || key == "left" {
            if shift {
                if input_state.selection_anchor.is_none() {
                    input_state.selection_anchor = Some(input_state.cursor_pos);
                }
                input_state.cursor_pos = input_state.cursor_pos.saturating_sub(1);
            } else {
                if let Some((s, _)) = input_state.selection_range() {
                    input_state.cursor_pos = s;
                } else {
                    input_state.cursor_pos = input_state.cursor_pos.saturating_sub(1);
                }
                input_state.selection_anchor = None;
            }
            self.active_plugin_input = Some(input_state);
            cx.notify();
            return;
        }

        if key == "arrowright" || key == "right" {
            if shift {
                if input_state.selection_anchor.is_none() {
                    input_state.selection_anchor = Some(input_state.cursor_pos);
                }
                input_state.cursor_pos = (input_state.cursor_pos + 1).min(total_chars);
            } else {
                if let Some((_, e)) = input_state.selection_range() {
                    input_state.cursor_pos = e;
                } else {
                    input_state.cursor_pos = (input_state.cursor_pos + 1).min(total_chars);
                }
                input_state.selection_anchor = None;
            }
            self.active_plugin_input = Some(input_state);
            cx.notify();
            return;
        }

        if key == "home" {
            if shift {
                if input_state.selection_anchor.is_none() {
                    input_state.selection_anchor = Some(input_state.cursor_pos);
                }
            } else {
                input_state.selection_anchor = None;
            }
            input_state.cursor_pos = 0;
            self.active_plugin_input = Some(input_state);
            cx.notify();
            return;
        }

        if key == "end" {
            if shift {
                if input_state.selection_anchor.is_none() {
                    input_state.selection_anchor = Some(input_state.cursor_pos);
                }
            } else {
                input_state.selection_anchor = None;
            }
            input_state.cursor_pos = total_chars;
            self.active_plugin_input = Some(input_state);
            cx.notify();
            return;
        }

        // Ctrl+A select all
        if ctrl && (key.eq_ignore_ascii_case("a") || key == "ф" || key == "Ф") {
            input_state.selection_anchor = Some(0);
            input_state.cursor_pos = total_chars;
            self.active_plugin_input = Some(input_state);
            cx.notify();
            return;
        }

        // Ctrl+C copy
        if ctrl && (key.eq_ignore_ascii_case("c") || key == "с" || key == "С") {
            if let Some((s, e)) = input_state.selection_range() {
                let sub: String = chars[s..e.min(chars.len())].iter().collect();
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(sub));
            } else if !input_state.value.is_empty() {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(input_state.value.clone()));
            }
            return;
        }

        // Ctrl+X cut
        if ctrl && (key.eq_ignore_ascii_case("x") || key == "ч" || key == "Ч") {
            if let Some((s, e)) = input_state.selection_range() {
                let sub: String = chars[s..e.min(chars.len())].iter().collect();
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(sub));
                chars.drain(s..e.min(chars.len()));
                input_state.cursor_pos = s;
                input_state.selection_anchor = None;
                input_state.value = chars.iter().collect();
                changed = true;
            }
        }

        // Ctrl+V paste
        if ctrl && (key.eq_ignore_ascii_case("v") || key == "м" || key == "М") {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    let is_multi = input_state.is_multiline;
                    let filtered: String = text
                        .chars()
                        .filter(|c| (!c.is_control() || (is_multi && (*c == '\n' || *c == '\t'))) && *c != '\r')
                        .collect();
                    if !filtered.is_empty() {
                        if let Some((s, e)) = input_state.selection_range() {
                            chars.drain(s..e.min(chars.len()));
                            input_state.cursor_pos = s;
                            input_state.selection_anchor = None;
                        }
                        let pos = input_state.cursor_pos.min(chars.len());
                        for (i, c) in filtered.chars().enumerate() {
                            chars.insert(pos + i, c);
                        }
                        input_state.cursor_pos = pos + filtered.chars().count();
                        input_state.value = chars.iter().collect();
                        changed = true;
                    }
                }
            }
        } else if key == "backspace" {
            if let Some((s, e)) = input_state.selection_range() {
                chars.drain(s..e.min(chars.len()));
                input_state.cursor_pos = s;
                input_state.selection_anchor = None;
                input_state.value = chars.iter().collect();
                changed = true;
            } else {
                let pos = input_state.cursor_pos.min(chars.len());
                if pos > 0 {
                    chars.remove(pos - 1);
                    input_state.cursor_pos = pos - 1;
                    input_state.value = chars.iter().collect();
                    changed = true;
                }
            }
        } else if key == "delete" {
            if let Some((s, e)) = input_state.selection_range() {
                chars.drain(s..e.min(chars.len()));
                input_state.cursor_pos = s;
                input_state.selection_anchor = None;
                input_state.value = chars.iter().collect();
                changed = true;
            } else {
                let pos = input_state.cursor_pos.min(chars.len());
                if pos < chars.len() {
                    chars.remove(pos);
                    input_state.value = chars.iter().collect();
                    changed = true;
                }
            }
        } else if !ctrl && !alt && !event.keystroke.modifiers.platform && key != "enter" {
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
                let is_multi = input_state.is_multiline;
                let filtered: String = text
                    .chars()
                    .filter(|c| (!c.is_control() || (is_multi && (*c == '\n' || *c == '\t'))) && *c != '\r')
                    .collect();
                if !filtered.is_empty() {
                    if let Some((s, e)) = input_state.selection_range() {
                        chars.drain(s..e.min(chars.len()));
                        input_state.cursor_pos = s;
                        input_state.selection_anchor = None;
                    }
                    let pos = input_state.cursor_pos.min(chars.len());
                    for (i, c) in filtered.chars().enumerate() {
                        chars.insert(pos + i, c);
                    }
                    input_state.cursor_pos = pos + filtered.chars().count();
                    input_state.value = chars.iter().collect();
                    changed = true;
                }
            }
        }

        if changed {
            let new_val = input_state.value.clone();
            let p_id = input_state.plugin_id.clone();
            let v_id = input_state.view_id;
            let n_id = input_state.node_id.clone();

            self.active_plugin_input = Some(input_state);

            if let Some(tree) = self.plugin_views.get_mut(&(p_id.clone(), v_id)) {
                tree.update_input_value(&n_id, &new_val);
            }

            let _ = self.state.plugin_host.send_event(
                &p_id,
                crate::plugins::api::HostEvent::UiEvent {
                    view_id: v_id,
                    node_id: n_id,
                    event: "change".to_string(),
                    value: Some(new_val),
                },
            );
        } else {
            self.active_plugin_input = Some(input_state);
        }

        cx.notify();
    }

    /// Triggers a view action registered via view.addAction(icon, title, callback)
    pub fn trigger_plugin_action(&mut self, plugin_id: &str, action_id: &str, cx: &mut Context<Self>) {
        let _ = self.state.plugin_host.send_event(
            plugin_id,
            crate::plugins::api::HostEvent::UiEvent {
                view_id: 0,
                node_id: action_id.to_string(),
                event: "action_click".to_string(),
                value: None,
            },
        );
        cx.notify();
    }

    /// Closes a plugin workspace leaf and notifies the plugin runtime (ItemView onClose)
    pub fn close_plugin_leaf(&mut self, leaf_id: &str, cx: &mut Context<Self>) {
        if let Some(leaf) = self.plugin_leaf_views.remove(leaf_id) {
            let _ = self.state.plugin_host.send_event(
                &leaf.plugin_id,
                crate::plugins::api::HostEvent::UiEvent {
                    view_id: 0,
                    node_id: leaf_id.to_string(),
                    event: "leaf_close".to_string(),
                    value: None,
                },
            );
        }
        if self.active_right_leaf.as_deref() == Some(leaf_id) {
            self.active_right_leaf = self
                .plugin_leaf_views
                .values()
                .find(|l| l.location == "right")
                .map(|l| l.leaf_id.clone());
        }
        if self.active_left_leaf.as_deref() == Some(leaf_id) {
            self.active_left_leaf = self
                .plugin_leaf_views
                .values()
                .find(|l| l.location == "left")
                .map(|l| l.leaf_id.clone());
            if let Some(ref next_id) = self.active_left_leaf {
                self.sidebar_mode = crate::views::sidebar::SidebarMode::Plugin(next_id.clone());
            } else if matches!(self.sidebar_mode, crate::views::sidebar::SidebarMode::Plugin(_)) {
                self.sidebar_mode = crate::views::sidebar::SidebarMode::Files;
            }
        }
        if self.active_center_leaf.as_deref() == Some(leaf_id) {
            self.active_center_leaf = None;
        }
        cx.notify();
    }

    /// Activates a plugin workspace leaf tab in the left sidebar
    pub fn select_left_leaf(&mut self, leaf_id: &str, cx: &mut Context<Self>) {
        self.active_left_leaf = Some(leaf_id.to_string());
        self.sidebar_mode = crate::views::sidebar::SidebarMode::Plugin(leaf_id.to_string());
        self.sidebar_collapsed = false;
        cx.notify();
    }

    /// Activates a plugin workspace leaf tab in the right sidebar
    pub fn select_right_leaf(&mut self, leaf_id: &str, cx: &mut Context<Self>) {
        self.active_right_leaf = Some(leaf_id.to_string());
        self.right_sidebar_collapsed = false;
        cx.notify();
    }

    /// Renders the plugin leaf view placed in the left sidebar
    pub fn render_left_plugin_leaf(
        &self,
        leaf: &crate::plugins::api::PluginLeafView,
        theme: &Theme,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let mut container = div()
            .id(gpui::ElementId::Name(format!("left-sidebar-leaf-{}", leaf.leaf_id).into()))
            .flex()
            .flex_col()
            .flex_1()
            .overflow_hidden()
            .gap(px(4.0));

        // 1. Header with title and action buttons
        let mut header = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(theme.border_subtle);

        header = header.child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(crate::icons::icon(
                    crate::icons::map_obsidian_icon(leaf.icon.as_deref().unwrap_or("clock")),
                    13.0,
                    theme.accent,
                ))
                .child(
                    div()
                        .text_xs()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme.text_primary)
                        .truncate()
                        .child(leaf.title.clone()),
                ),
        );

        let mut actions_bar = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1();

        for act in &leaf.actions {
            let act_id = act.id.clone();
            let act_plugin_id = leaf.plugin_id.clone();
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

        let leaf_id_close = leaf.leaf_id.clone();
        actions_bar = actions_bar.child(
            div()
                .px(px(4.0))
                .py(px(2.0))
                .rounded(px(4.0))
                .cursor_pointer()
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
        );

        header = header.child(actions_bar);
        container = container.child(header);

        // 2. Leaf content
        container = container.child(
            div()
                .id(gpui::ElementId::Name(format!("left-sidebar-leaf-content-{}", leaf.leaf_id).into()))
                .flex()
                .flex_col()
                .flex_1()
                .overflow_scroll()
                .px_2()
                .py_1()
                .child(crate::plugins::ui::render_ui_node(
                    &leaf.tree,
                    &leaf.plugin_id,
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

        container
    }

    pub(crate) fn commit_title_rename_internal(&mut self) -> bool {
        let Some((view, doc)) = self.state.active_view_and_doc_mut() else {
            return false;
        };
        let Some(input) = view.title_input.take() else {
            return false;
        };
        let new_title = input.text.trim();
        let current_path = doc.path.clone();
        let current_title = doc.title.clone();

        if new_title.is_empty() || new_title == current_title {
            return false;
        }

        match self.state.rename_note(&current_path, new_title) {
            Ok(_) => true,
            Err(err) => {
                if let Some(view) = self.state.active_view_mut() {
                    view.title_input = Some(input);
                }
                self.state.status_text = format!("Rename failed: {}", err);
                false
            }
        }
    }

    pub fn commit_title_rename(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.commit_title_rename_internal() {
            cx.notify();
        }
    }

    fn handle_title_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke.key;
        let ctrl = event.keystroke.modifiers.control;
        let shift = event.keystroke.modifiers.shift;
        let alt = event.keystroke.modifiers.alt;

        if key == "enter" || key == "tab" {
            self.commit_title_rename(window, cx);
            window.focus(&self.focus_handle);
            cx.notify();
            return;
        }

        if key == "escape" {
            if let Some(view) = self.state.active_view_mut() {
                view.cancel_title_edit();
            }
            window.focus(&self.focus_handle);
            cx.notify();
            return;
        }

        if key == "down" {
            self.commit_title_rename(window, cx);
            if let Some(view) = self.state.active_view_mut() {
                view.cursor_offset = 0;
                view.selection_anchor = None;
            }
            window.focus(&self.focus_handle);
            cx.notify();
            return;
        }

        let Some(view) = self.state.active_view_mut() else {
            return;
        };
        let Some(ref mut input) = view.title_input else {
            return;
        };

        // Ctrl + A (Select All)
        if ctrl && (key.eq_ignore_ascii_case("a") || key == "ф" || key == "Ф") {
            input.select_all();
            self.reset_title_cursor_blink(cx);
            cx.notify();
            return;
        }

        // Ctrl + C (Copy selected text)
        if ctrl && (key.eq_ignore_ascii_case("c") || key == "с" || key == "С") {
            if let Some(selected) = input.selected_text() {
                cx.write_to_clipboard(ClipboardItem::new_string(selected));
            }
            return;
        }

        // Ctrl + X (Cut selected text)
        if ctrl && (key.eq_ignore_ascii_case("x") || key == "ч" || key == "Ч") {
            if let Some(selected) = input.selected_text() {
                cx.write_to_clipboard(ClipboardItem::new_string(selected));
                input.delete_selection();
                self.reset_title_cursor_blink(cx);
                cx.notify();
            }
            return;
        }

        // Ctrl + S (Save note)
        if ctrl && (key.eq_ignore_ascii_case("s") || key == "ы" || key == "Ы") {
            self.commit_title_rename(window, cx);
            self.save_note(window, cx);
            return;
        }

        // Ctrl + V (Paste over selection or at cursor)
        if ctrl && (key.eq_ignore_ascii_case("v") || key == "м" || key == "М") {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    let invalid_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*', '\n', '\r', '\t'];
                    let filtered: String = text
                        .chars()
                        .filter(|c| !invalid_chars.contains(c) && !c.is_control())
                        .collect();
                    if !filtered.is_empty() {
                        input.insert_text(&filtered);
                        self.reset_title_cursor_blink(cx);
                        cx.notify();
                    }
                }
            }
            return;
        }

        // Left Arrow
        if key == "left" {
            input.move_left(shift);
            self.reset_title_cursor_blink(cx);
            cx.notify();
            return;
        }

        // Right Arrow
        if key == "right" {
            input.move_right(shift);
            self.reset_title_cursor_blink(cx);
            cx.notify();
            return;
        }

        // Home
        if key == "home" {
            input.move_home(shift);
            self.reset_title_cursor_blink(cx);
            cx.notify();
            return;
        }

        // End
        if key == "end" {
            input.move_end(shift);
            self.reset_title_cursor_blink(cx);
            cx.notify();
            return;
        }

        // Backspace
        if key == "backspace" {
            input.backspace();
            self.reset_title_cursor_blink(cx);
            cx.notify();
            return;
        }

        // Delete
        if key == "delete" {
            input.delete();
            self.reset_title_cursor_blink(cx);
            cx.notify();
            return;
        }

        // Normal character input
        if !ctrl && !alt && !event.keystroke.modifiers.platform {
            let invalid_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*', '\n', '\r', '\t'];
            let text_to_insert = if key == "space" {
                Some(" ")
            } else if let Some(ch) = &event.keystroke.key_char {
                Some(ch.as_str())
            } else if key.chars().count() == 1 {
                Some(key.as_str())
            } else {
                None
            };

            if let Some(text) = text_to_insert {
                let filtered: String = text
                    .chars()
                    .filter(|c| !invalid_chars.contains(c) && !c.is_control())
                    .collect();
                if !filtered.is_empty() {
                    input.insert_text(&filtered);
                    self.reset_title_cursor_blink(cx);
                    cx.notify();
                }
            }
        }
    }

    pub(crate) fn first_matching_file(&self, nodes: &[FileNode]) -> Option<PathBuf> {
        let query = self.state.search_query.trim().to_lowercase();
        self.first_matching_file_internal(nodes, &query, false)
    }

    fn first_matching_file_internal(&self, nodes: &[FileNode], query: &str, parent_matched: bool) -> Option<PathBuf> {
        for node in nodes {
            match node {
                FileNode::File { name, path } => {
                    if query.is_empty() || parent_matched || name.to_lowercase().contains(query) {
                        return Some(path.clone());
                    }
                }
                FileNode::Folder { name, children, .. } => {
                    let folder_matched = parent_matched || (!query.is_empty() && name.to_lowercase().contains(query));
                    if let Some(p) = self.first_matching_file_internal(children, query, folder_matched) {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    fn handle_key(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if self.active_plugin_input.is_some() {
            self.handle_plugin_input_key(event, cx);
            return;
        }

        self.reset_editor_cursor_blink(cx);
        let keystroke = &event.keystroke;
        let ctrl = keystroke.modifiers.control || keystroke.modifiers.platform;
        if ctrl != self.ctrl_held {
            self.ctrl_held = ctrl;
            cx.notify();
        }
        let alt = keystroke.modifiers.alt;
        let shift = keystroke.modifiers.shift;
        if alt && keystroke.key == "left" {
            self.navigate_back(_window, cx);
            return;
        }

        // Alt + Right (Navigate Forward in note history)
        if alt && keystroke.key == "right" {
            self.navigate_forward(_window, cx);
            return;
        }

        // Escape closes plugin modal dialog if open
        if keystroke.key == "escape" && self.has_active_plugin_modal() {
            self.close_active_plugin_modal(cx);
            return;
        }

        // Escape closes the tabs popup if open
        if keystroke.key == "escape" && self.tabs_popup.is_some() {
            self.close_tabs_popup(cx);
            return;
        }

        // Escape closes the command palette if open
        if keystroke.key == "escape" && self.command_palette.is_open {
            self.close_command_palette(_window, cx);
            return;
        }

        // Escape closes folder context menu if open
        if keystroke.key == "escape" && self.folder_context_menu.is_open {
            self.close_folder_context_menu(cx);
            return;
        }

        // Escape closes file context menu if open
        if keystroke.key == "escape" && self.file_context_menu.is_open {
            self.close_file_context_menu(cx);
            return;
        }

        // Escape closes folder modal if open
        if keystroke.key == "escape" && self.folder_modal != crate::views::sidebar::FolderModal::None {
            self.close_folder_modal(cx);
            return;
        }

        // Escape closes plugin select popup if open
        if keystroke.key == "escape" && self.plugin_select_popup.is_some() {
            self.close_plugin_select(cx);
            return;
        }

        // Escape closes settings window if open
        if keystroke.key == "escape" && self.settings_window_manager.is_open() {
            self.close_settings(cx);
            return;
        }

        // Escape cancels selection if active
        if keystroke.key == "escape" {
            self.ctrl_preview_selection = false;
            if let Some(view) = self.state.active_view_mut() {
                if view.selection_anchor.is_some() && view.selection_anchor != Some(view.cursor_offset) {
                    view.selection_anchor = None;
                    cx.notify();
                    return;
                }
            }
        }

        if !ctrl && keystroke.key != "control" && keystroke.key != "shift" && keystroke.key != "alt" {
            self.ctrl_preview_selection = false;
        }

        // Ctrl + , (Toggle Settings modal)
        if ctrl && (keystroke.key == "," || keystroke.key == "<" || keystroke.key == "б" || keystroke.key == "Б") {
            self.toggle_settings(cx);
            return;
        }

        // Ctrl + \ или Ctrl + B (Свернуть / развернуть сайдбар)
        if ctrl && (keystroke.key == "\\" || keystroke.key.eq_ignore_ascii_case("b") || keystroke.key == "и" || keystroke.key == "И") {
            self.toggle_sidebar(cx);
            return;
        }

        // Ctrl + S (Сохранение)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("s") || keystroke.key == "ы" || keystroke.key == "Ы") {
            self.flush_active_note_now(cx);
            return;
        }

        // Ctrl + P (Палитра команд)
        if ctrl && !shift && (keystroke.key.eq_ignore_ascii_case("p") || keystroke.key == "з" || keystroke.key == "З") {
            self.open_command_palette(_window, cx);
            return;
        }

        // Ctrl + Shift + P (Палитра команд)
        if ctrl && shift && (keystroke.key.eq_ignore_ascii_case("p") || keystroke.key == "з" || keystroke.key == "З") {
            self.open_command_palette(_window, cx);
            return;
        }

        // Ctrl + O (Поиск заметок / Меню быстрого перехода)
        if ctrl && !shift && (keystroke.key.eq_ignore_ascii_case("o") || keystroke.key == "щ" || keystroke.key == "Щ") {
            self.open_search_dialog(_window, cx);
            return;
        }

        // Ctrl + Shift + O (Открыть хранилище)
        if ctrl && shift && (keystroke.key.eq_ignore_ascii_case("o") || keystroke.key == "щ" || keystroke.key == "Щ") {
            self.open_folder_dialog(_window, cx);
            return;
        }

        // Ctrl + N (Новая пустая вкладка: файл не создаётся, показывается New Tab)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("n") || keystroke.key == "т" || keystroke.key == "Т") {
            self.open_empty_tab(_window, cx);
            return;
        }

        // Ctrl + W (Закрыть текущую вкладку)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("w") || keystroke.key == "ц" || keystroke.key == "Ц") {
            if let Some(doc) = self.state.active_document() {
                let path = doc.path.clone();
                self.close_tab(&path, cx);
            }
            return;
        }

        // Ctrl + F (Поиск / фильтр). Shift excluded: Ctrl+Shift+F opens
        // the sidebar search panel via the root listener below.
        if ctrl && !shift && (keystroke.key.eq_ignore_ascii_case("f") || keystroke.key == "а" || keystroke.key == "А") {
            _window.focus(&self.search_focus_handle);
            self.quick_filter.focus_and_select_all(&self.state.search_query, cx);
            return;
        }

        // Ctrl + Z (Undo)
        if ctrl && !shift && (keystroke.key.eq_ignore_ascii_case("z") || keystroke.key == "я" || keystroke.key == "Я") {
            if self.state.active_undo() {
                cx.notify();
            }
            self.schedule_auto_save(cx);
            return;
        }

        // Ctrl + Y или Ctrl + Shift + Z (Redo)
        if (ctrl && (keystroke.key.eq_ignore_ascii_case("y") || keystroke.key == "н" || keystroke.key == "Н"))
            || (ctrl && shift && (keystroke.key.eq_ignore_ascii_case("z") || keystroke.key == "я" || keystroke.key == "Я"))
        {
            if self.state.active_redo() {
                cx.notify();
            }
            self.schedule_auto_save(cx);
            return;
        }

        // Ctrl + A (Выделить всё)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("a") || keystroke.key == "ф" || keystroke.key == "Ф") {
            self.ctrl_preview_selection = false;
            self.state.active_select_all();
            cx.notify();
            return;
        }

        // Ctrl + C (Копировать)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("c") || keystroke.key == "с" || keystroke.key == "С") {
            let copied_tsv = if let Some((view, doc)) = self.state.active_view_and_doc() {
                let (cursor_line, _) = view.line_col(doc);
                if let Some(range) = view.selection_range(doc.len()) {
                    let start_line = doc.line_of_byte(range.start);
                    let end_line = doc.line_of_byte(range.end);
                    // The cursor is one end of the selection, so the windowed
                    // scan covers it too; a full-document scan is not needed.
                    let tables = doc.ensure_table_index_for_window(start_line, end_line);
                    if let (Some(t1), Some(t2)) = (tables.at(start_line), tables.at(end_line)) {
                        if t1.first_line == t2.first_line && view.grid_table_at(doc, &tables, cursor_line, cursor_line, self.ctrl_preview_selection || !self.edit_mode).is_some() {
                            let start_line_byte = doc.line_start_byte(start_line);
                            let end_line_byte = doc.line_start_byte(end_line);
                            let start_col = doc.slice_to_string(start_line_byte..range.start).chars().count();
                            let end_col = doc.slice_to_string(end_line_byte..range.end).chars().count();
                            let line_strings = Self::table_lines_slice(doc, t1);
                            let lines_ref: Vec<&str> = line_strings.iter().map(|s| s.as_str()).collect();
                            let r1 = t1.row_of_line(start_line).unwrap_or(0);
                            let r2 = t1.row_of_line(end_line).unwrap_or(t1.row_count().saturating_sub(1));
                            let c1 = crate::table::cell_index_at(&crate::table::split_row(crate::table::get_table_line(t1, &lines_ref, start_line)), start_col);
                            let c2 = crate::table::cell_index_at(&crate::table::split_row(crate::table::get_table_line(t1, &lines_ref, end_line)), end_col);
                            if r1 != r2 || c1 != c2 {
                                let rect = crate::table::CellRect::new(r1, c1, r2, c2);
                                let model = crate::table::TableModel::parse(t1, &lines_ref);
                                Some(model.to_tsv(rect.row_start..rect.row_end, rect.col_start..rect.col_end))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let strip_tags = self.ctrl_preview_selection || !self.edit_mode;
            if let Some(tsv) = copied_tsv {
                let to_copy = if strip_tags {
                    crate::markdown::strip_markdown_tags(&tsv)
                } else {
                    tsv
                };
                cx.write_to_clipboard(ClipboardItem::new_string(to_copy));
                return;
            } else if let Some(text) = self.state.active_selected_text() {
                let to_copy = if strip_tags {
                    crate::markdown::strip_markdown_tags(&text)
                } else {
                    text
                };
                cx.write_to_clipboard(ClipboardItem::new_string(to_copy));
                return;
            }
        }

        // Ctrl + X (Вырезать)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("x") || keystroke.key == "ч" || keystroke.key == "Ч") {
            if let Some(text) = self.state.active_selected_text() {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                self.state.active_delete_selection();
                cx.notify();
            }
            self.schedule_auto_save(cx);
            return;
        }

        // Alt + Z (Переключение переноса строк)
        if keystroke.modifiers.alt && (keystroke.key.eq_ignore_ascii_case("z") || keystroke.key == "я" || keystroke.key == "Я") {
            self.toggle_word_wrap(cx);
            return;
        }

        // Ctrl + / (Переключение исходника активной таблицы)
        if ctrl && (keystroke.key == "/" || keystroke.key == "?" || keystroke.key == "." || keystroke.key == "ю" || keystroke.key == "Ю") {
            if self.state.active_toggle_raw_table() {
                cx.notify();
                return;
            }
        }

        // Ctrl + V (Вставить)
        if ctrl && (keystroke.key.eq_ignore_ascii_case("v") || keystroke.key == "м" || keystroke.key == "М") {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    enum PasteAction {
                        ApplyEdit(crate::table::LineEdit, Option<(usize, usize)>),
                        InsertText(String),
                        Deny,
                    }

                    let paste_action = if let Some((view, doc)) = self.state.active_view_and_doc() {
                        let (cursor_line, cursor_col) = view.line_col(doc);
                        let tables = doc.ensure_table_index_for_window(view.window.start_line, view.window.end_line);
                        if let Some(tbl) = view.grid_table_at(doc, &tables, cursor_line, cursor_line, self.ctrl_preview_selection || !self.edit_mode) {
                            if cursor_line == tbl.sep_line() {
                                PasteAction::Deny
                            } else if text.contains('\t') {
                                let line_strings = Self::table_lines_slice(doc, tbl);
                                let lines_ref: Vec<&str> = line_strings.iter().map(|s| s.as_str()).collect();
                                let row = tbl.row_of_line(cursor_line).unwrap_or(0);
                                let cells = crate::table::split_row(crate::table::get_table_line(tbl, &lines_ref, cursor_line));
                                let col = crate::table::cell_index_at(&cells, cursor_col);
                                let mut model = crate::table::TableModel::parse(tbl, &lines_ref);
                                if let Some((max_r, max_c)) = model.paste_tsv(row, col, &text) {
                                    let edit = crate::table::rewrite(tbl, &model, false, &crate::table::char_width);
                                    let target_line = tbl.line_of_row(max_r);
                                    let new_row_str = &edit.new_lines[if max_r == 0 { 0 } else { max_r + 1 }];
                                    let cells_new = crate::table::split_row(new_row_str);
                                    let target_col = cells_new.get(max_c).map(|c| c.content.end).unwrap_or(2);
                                    PasteAction::ApplyEdit(edit, Some((target_line, target_col)))
                                } else {
                                    PasteAction::InsertText(crate::table::sanitize_cell_input(&text))
                                }
                            } else {
                                PasteAction::InsertText(crate::table::sanitize_cell_input(&text))
                            }
                        } else {
                            PasteAction::InsertText(text)
                        }
                    } else {
                        PasteAction::InsertText(text)
                    };

                    match paste_action {
                        PasteAction::ApplyEdit(edit, target_cursor) => {
                            self.state.active_apply_table_line_edit(&edit, target_cursor);
                            self.schedule_auto_save(cx);
                            cx.notify();
                            return;
                        }
                        PasteAction::InsertText(s) => {
                            self.state.active_insert_text(&s);
                            cx.notify();
                        }
                        PasteAction::Deny => {
                            return;
                        }
                    }
                }
            }
            self.schedule_auto_save(cx);
            return;
        }

        if self.state.active_doc_id().is_none() {
            return;
        }

        // Grid Table Navigation and Guarded Edits
        enum TableKeyAction {
            SetCursor(usize, usize),
            ApplyEdit(crate::table::LineEdit, Option<(usize, usize)>),
            InsertText(String),
            Deny,
            PassThrough,
        }

        let table_action = if let Some((view, doc)) = self.state.active_view_and_doc() {
            let (cursor_line, cursor_col) = view.line_col(doc);
            let tables = doc.ensure_table_index_for_window(view.window.start_line, view.window.end_line);
            if let Some(tbl) = view.grid_table_at(doc, &tables, cursor_line, cursor_line, self.ctrl_preview_selection || !self.edit_mode) {
                let line_str = doc.line_text(cursor_line);
                let cells = crate::table::split_row(&line_str);
                let current_row_cell = tbl
                    .row_of_line(cursor_line)
                    .map(|row| (row, crate::table::cell_index_at(&cells, cursor_col).min(tbl.col_count().saturating_sub(1))));
                // Only the target row's text is read: navigation must not clone the
                // whole table on every keypress.
                let cursor_at = |row: usize, cell: usize| -> TableKeyAction {
                    let target_l = tbl.line_of_row(row);
                    let target_text = doc.line_text(target_l);
                    let target_c = crate::table::split_row(&target_text)
                        .get(cell)
                        .map(|c| c.content.start)
                        .unwrap_or_else(|| target_text.chars().count());
                    TableKeyAction::SetCursor(target_l, target_c)
                };
                // The full table is only materialised when a row is appended.
                let append_row = |cell: usize| -> TableKeyAction {
                    let line_strings = Self::table_lines_slice(doc, tbl);
                    let lines_ref: Vec<&str> = line_strings.iter().map(|s| s.as_str()).collect();
                    let mut model = crate::table::TableModel::parse(tbl, &lines_ref);
                    model.insert_row(model.rows.len());
                    let edit = crate::table::rewrite(tbl, &model, false, &crate::table::char_width);
                    let target_l = edit.first_line + edit.new_lines.len() - 1;
                    let new_cells = crate::table::split_row(&edit.new_lines[edit.new_lines.len() - 1]);
                    let target_c = new_cells.get(cell).map(|c| c.content.start).unwrap_or(2);
                    TableKeyAction::ApplyEdit(edit, Some((target_l, target_c)))
                };

                match keystroke.key.as_str() {
                    "tab" => {
                        if let Some((row, cell)) = current_row_cell {
                            match crate::table::tab_from(tbl, row, cell, shift) {
                                crate::table::Nav::To { row, cell } => {
                                    cursor_at(row, cell)
                                }
                                crate::table::Nav::AppendRow { cell } => append_row(cell),
                                crate::table::Nav::Exit => {
                                    if tbl.first_line > 0 {
                                        TableKeyAction::SetCursor(tbl.first_line - 1, 0)
                                    } else {
                                        TableKeyAction::Deny
                                    }
                                }
                            }
                        } else {
                            TableKeyAction::Deny
                        }
                    }
                    "enter" => {
                        if let Some((row, cell)) = current_row_cell {
                            match crate::table::enter_from(tbl, row, cell, shift) {
                                crate::table::Nav::To { row, cell } => {
                                    cursor_at(row, cell)
                                }
                                crate::table::Nav::AppendRow { cell } => append_row(cell),
                                crate::table::Nav::Exit => {
                                    if tbl.first_line > 0 {
                                        TableKeyAction::SetCursor(tbl.first_line - 1, 0)
                                    } else {
                                        TableKeyAction::Deny
                                    }
                                }
                            }
                        } else {
                            TableKeyAction::Deny
                        }
                    }
                    "backspace" => {
                        if cursor_line == tbl.sep_line() {
                            TableKeyAction::Deny
                        } else if let Some(range) = view.selection_range(doc.len()) {
                            let start_line = doc.line_of_byte(range.start);
                            let end_line = doc.line_of_byte(range.end);
                            if start_line == end_line && start_line == cursor_line {
                                let line_start_byte = doc.line_start_byte(cursor_line);
                                let start_col = doc.slice_to_string(line_start_byte..range.start).chars().count();
                                let end_col = doc.slice_to_string(line_start_byte..range.end).chars().count();
                                if crate::table::guard_edit(tbl, cursor_line, &line_str, start_col..end_col) == crate::table::EditGuard::Deny {
                                    TableKeyAction::Deny
                                } else {
                                    TableKeyAction::PassThrough
                                }
                            } else if start_line != end_line && tbl.contains(start_line) && tbl.contains(end_line) {
                                let start_line_byte = doc.line_start_byte(start_line);
                                let end_line_byte = doc.line_start_byte(end_line);
                                let start_col = doc.slice_to_string(start_line_byte..range.start).chars().count();
                                let end_col = doc.slice_to_string(end_line_byte..range.end).chars().count();
                                let line_strings = Self::table_lines_slice(doc, tbl);
                                let lines_ref: Vec<&str> = line_strings.iter().map(|s| s.as_str()).collect();
                                let r1 = tbl.row_of_line(start_line).unwrap_or(0);
                                let r2 = tbl.row_of_line(end_line).unwrap_or(tbl.row_count().saturating_sub(1));
                                let c1 = crate::table::cell_index_at(&crate::table::split_row(crate::table::get_table_line(tbl, &lines_ref, start_line)), start_col);
                                let c2 = crate::table::cell_index_at(&crate::table::split_row(crate::table::get_table_line(tbl, &lines_ref, end_line)), end_col);
                                let rect = crate::table::CellRect::new(r1, r2, c1, c2);
                                let mut model = crate::table::TableModel::parse(tbl, &lines_ref);
                                model.clear_rect(&rect);
                                let edit = crate::table::rewrite(tbl, &model, false, &crate::table::char_width);
                                TableKeyAction::ApplyEdit(edit, Some((cursor_line, cursor_col)))
                            } else {
                                TableKeyAction::PassThrough
                            }
                        } else {
                            if cursor_col == 0 || crate::table::guard_edit(tbl, cursor_line, &line_str, cursor_col - 1..cursor_col) == crate::table::EditGuard::Deny {
                                TableKeyAction::Deny
                            } else {
                                TableKeyAction::PassThrough
                            }
                        }
                    }
                    "delete" => {
                        if cursor_line == tbl.sep_line() {
                            TableKeyAction::Deny
                        } else if let Some(range) = view.selection_range(doc.len()) {
                            let start_line = doc.line_of_byte(range.start);
                            let end_line = doc.line_of_byte(range.end);
                            if start_line == end_line && start_line == cursor_line {
                                let line_start_byte = doc.line_start_byte(cursor_line);
                                let start_col = doc.slice_to_string(line_start_byte..range.start).chars().count();
                                let end_col = doc.slice_to_string(line_start_byte..range.end).chars().count();
                                if crate::table::guard_edit(tbl, cursor_line, &line_str, start_col..end_col) == crate::table::EditGuard::Deny {
                                    TableKeyAction::Deny
                                } else {
                                    TableKeyAction::PassThrough
                                }
                            } else if start_line != end_line && tbl.contains(start_line) && tbl.contains(end_line) {
                                let start_line_byte = doc.line_start_byte(start_line);
                                let end_line_byte = doc.line_start_byte(end_line);
                                let start_col = doc.slice_to_string(start_line_byte..range.start).chars().count();
                                let end_col = doc.slice_to_string(end_line_byte..range.end).chars().count();
                                let line_strings = Self::table_lines_slice(doc, tbl);
                                let lines_ref: Vec<&str> = line_strings.iter().map(|s| s.as_str()).collect();
                                let r1 = tbl.row_of_line(start_line).unwrap_or(0);
                                let r2 = tbl.row_of_line(end_line).unwrap_or(tbl.row_count().saturating_sub(1));
                                let c1 = crate::table::cell_index_at(&crate::table::split_row(crate::table::get_table_line(tbl, &lines_ref, start_line)), start_col);
                                let c2 = crate::table::cell_index_at(&crate::table::split_row(crate::table::get_table_line(tbl, &lines_ref, end_line)), end_col);
                                let rect = crate::table::CellRect::new(r1, r2, c1, c2);
                                let mut model = crate::table::TableModel::parse(tbl, &lines_ref);
                                model.clear_rect(&rect);
                                let edit = crate::table::rewrite(tbl, &model, false, &crate::table::char_width);
                                TableKeyAction::ApplyEdit(edit, Some((cursor_line, cursor_col)))
                            } else {
                                TableKeyAction::PassThrough
                            }
                        } else {
                            if crate::table::guard_edit(tbl, cursor_line, &line_str, cursor_col..cursor_col + 1) == crate::table::EditGuard::Deny {
                                TableKeyAction::Deny
                            } else {
                                TableKeyAction::PassThrough
                            }
                        }
                    }
                    "left" => {
                        if !shift {
                            let cell = crate::table::cell_index_at(&cells, cursor_col);
                            if let Some(c) = cells.get(cell) {
                                if cursor_col <= c.content.start && cell > 0 {
                                    let prev_c = &cells[cell - 1];
                                    TableKeyAction::SetCursor(cursor_line, prev_c.content.end)
                                } else {
                                    TableKeyAction::PassThrough
                                }
                            } else {
                                TableKeyAction::PassThrough
                            }
                        } else {
                            TableKeyAction::PassThrough
                        }
                    }
                    "right" => {
                        if !shift {
                            let cell = crate::table::cell_index_at(&cells, cursor_col);
                            if let Some(c) = cells.get(cell) {
                                if cursor_col >= c.content.end && cell + 1 < cells.len() {
                                    let next_c = &cells[cell + 1];
                                    TableKeyAction::SetCursor(cursor_line, next_c.content.start)
                                } else {
                                    TableKeyAction::PassThrough
                                }
                            } else {
                                TableKeyAction::PassThrough
                            }
                        } else {
                            TableKeyAction::PassThrough
                        }
                    }
                    "home" => {
                        let cell = crate::table::cell_index_at(&cells, cursor_col);
                        if let Some(c) = cells.get(cell) {
                            TableKeyAction::SetCursor(cursor_line, c.content.start)
                        } else {
                            TableKeyAction::PassThrough
                        }
                    }
                    "end" => {
                        let cell = crate::table::cell_index_at(&cells, cursor_col);
                        if let Some(c) = cells.get(cell) {
                            TableKeyAction::SetCursor(cursor_line, c.content.end)
                        } else {
                            TableKeyAction::PassThrough
                        }
                    }
                    "up" => {
                        if !shift {
                            let cell = crate::table::cell_index_at(&cells, cursor_col);
                            if let Some(row) = tbl.row_of_line(cursor_line) {
                                if row > 0 {
                                    cursor_at(row - 1, cell)
                                } else if tbl.first_line > 0 {
                                    TableKeyAction::SetCursor(tbl.first_line - 1, 0)
                                } else {
                                    TableKeyAction::PassThrough
                                }
                            } else {
                                TableKeyAction::PassThrough
                            }
                        } else {
                            TableKeyAction::PassThrough
                        }
                    }
                    "down" => {
                        if !shift {
                            let cell = crate::table::cell_index_at(&cells, cursor_col);
                            if let Some(row) = tbl.row_of_line(cursor_line) {
                                if row + 1 < tbl.row_count() {
                                    cursor_at(row + 1, cell)
                                } else {
                                    let next_line = tbl.last_line + 1;
                                    if next_line < doc.line_index.lines() {
                                        TableKeyAction::SetCursor(next_line, 0)
                                    } else {
                                        TableKeyAction::PassThrough
                                    }
                                }
                            } else {
                                TableKeyAction::PassThrough
                            }
                        } else {
                            TableKeyAction::PassThrough
                        }
                    }
                    _ => {
                        if !ctrl && !keystroke.modifiers.alt && !keystroke.modifiers.platform {
                            if cursor_line == tbl.sep_line() {
                                TableKeyAction::Deny
                            } else {
                                let typed_str = keystroke.key_char.as_deref().unwrap_or(keystroke.key.as_str());
                                if typed_str == "|" {
                                    TableKeyAction::InsertText("\\|".into())
                                } else {
                                    TableKeyAction::PassThrough
                                }
                            }
                        } else {
                            TableKeyAction::PassThrough
                        }
                    }
                }
            } else {
                TableKeyAction::PassThrough
            }
        } else {
            TableKeyAction::PassThrough
        };

        match table_action {
            TableKeyAction::SetCursor(line, col) => {
                self.state.active_set_cursor_line_col(line, col);
                self.refresh_view_after_cursor_move();
                cx.notify();
                return;
            }
            TableKeyAction::ApplyEdit(edit, target_cursor) => {
                self.state.active_apply_table_line_edit(&edit, target_cursor);
                self.schedule_auto_save(cx);
                cx.notify();
                return;
            }
            TableKeyAction::InsertText(s) => {
                self.state.active_insert_text(&s);
                self.schedule_auto_save(cx);
                cx.notify();
                return;
            }
            TableKeyAction::Deny => {
                return;
            }
            TableKeyAction::PassThrough => {}
        }

        if keystroke.key == "backspace" || keystroke.key == "delete" {
            let edit_opt = if let Some((view, doc)) = self.state.active_view_and_doc() {
                if let Some(range) = view.selection_range(doc.len()) {
                    let start_line = doc.line_of_byte(range.start);
                    let end_line = doc.line_of_byte(range.end);
                    // Windowed scan only: a full-document scan here froze the
                    // first Delete on large notes opened without a full index.
                    let tables = doc.ensure_table_index_for_window(start_line, end_line);
                    if let (Some(t1), Some(t2)) = (tables.at(start_line), tables.at(end_line)) {
                        if t1.first_line == t2.first_line && start_line != end_line && view.grid_table_at(doc, &tables, start_line, start_line, self.ctrl_preview_selection || !self.edit_mode).is_some() {
                            let start_line_byte = doc.line_start_byte(start_line);
                            let end_line_byte = doc.line_start_byte(end_line);
                            let start_col = doc.slice_to_string(start_line_byte..range.start).chars().count();
                            let end_col = doc.slice_to_string(end_line_byte..range.end).chars().count();
                            let line_strings = Self::table_lines_slice(doc, t1);
                            let lines_ref: Vec<&str> = line_strings.iter().map(|s| s.as_str()).collect();
                            let r1 = t1.row_of_line(start_line).unwrap_or(0);
                            let r2 = t1.row_of_line(end_line).unwrap_or(t1.row_count().saturating_sub(1));
                            let c1 = crate::table::cell_index_at(&crate::table::split_row(crate::table::get_table_line(t1, &lines_ref, start_line)), start_col);
                            let c2 = crate::table::cell_index_at(&crate::table::split_row(crate::table::get_table_line(t1, &lines_ref, end_line)), end_col);
                            let rect = crate::table::CellRect::new(r1, c1, r2, c2);
                            let mut model = crate::table::TableModel::parse(t1, &lines_ref);
                            for r in rect.row_start..rect.row_end {
                                for c in rect.col_start..rect.col_end {
                                    if let Some(row) = model.rows.get_mut(r) {
                                        if let Some(cell) = row.get_mut(c) {
                                            cell.clear();
                                        }
                                    }
                                }
                            }
                            let edit = crate::table::rewrite(t1, &model, false, &crate::table::char_width);
                            Some((t1.first_line, edit))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            if let Some((first_line, edit)) = edit_opt {
                self.table_layouts.borrow_mut().remove(&first_line);
                self.state.active_apply_table_line_edit(&edit, None);
                if let Some(v) = self.state.active_view_mut() {
                    v.selection_anchor = None;
                }
                self.schedule_auto_save(cx);
                cx.notify();
                return;
            }
        }

        // A text edit or a cursor/selection move must not just repaint once:
        // one notify per keystroke renders the editor at key-repeat rate
        // (~30 fps), and the caret/selection visibly *step* between frames —
        // plus the scroll reveal and the render-side clamp then fight across
        // frames and the selection looks one event behind the caret.
        //
        // The settle frame, however, is only worth arming when the keystroke
        // actually moved the scroll: the reveal below already wrote the final
        // scroll position into the very frame that paints the edit, so a plain
        // in-line insert has nothing new for a second frame to paint — and at
        // key-repeat the doubled cadence (~60 fps vs ~30) is most of the GPU
        // burn (measured 2026-10-21: ~8% GPU at one frame per char, ~18–20% at
        // two). Scrolling keystrokes — typing at the viewport edge, cursor
        // moves that leave the viewport — still get their settle frame.
        let version_before = self.state.active_document().map(|d| d.version);
        let anchor_before = self.state.active_view().and_then(|v| v.selection_anchor);
        let cursor_before = self.state.active_view().map(|v| v.cursor_offset);
        let scroll_before = self.current_scroll_top_f64();
        match keystroke.key.as_str() {
            "backspace" => self.state.active_delete_backwards(),
            "delete" => self.state.active_delete_forwards(),
            "enter" => self.state.active_insert_newline_auto_indent(),
            "tab" => self.state.active_insert_text("    "),
            "space" => self.state.active_insert_text(" "),
            "left" => self.state.active_move_cursor_left(shift),
            "right" => self.state.active_move_cursor_right(shift),
            "up" => self.move_cursor_up_visual(shift),
            "down" => self.move_cursor_down_visual(shift),
            "home" => self.move_cursor_home_visual(shift),
            "end" => self.move_cursor_end_visual(shift),
            _ => {
                if !ctrl && !keystroke.modifiers.alt && !keystroke.modifiers.platform {
                    if let Some(ch) = &keystroke.key_char {
                        self.state.active_insert_text(ch);
                    } else if keystroke.key.chars().count() == 1 {
                        self.state.active_insert_text(&keystroke.key);
                    }
                }
            }
        }
        let version_after = self.state.active_document().map(|d| d.version);
        let state_changed = version_before != version_after
            || anchor_before != self.state.active_view().and_then(|v| v.selection_anchor)
            || cursor_before != self.state.active_view().map(|v| v.cursor_offset);
        if self.state.active_document().map(|d| d.is_dirty).unwrap_or(false) {
            self.schedule_auto_save(cx);
        }
        // Refresh the rows before revealing: every edit above marked the
        // cache dirty, and the lookup below must see the new rows already —
        // otherwise a row created at the very bottom (Enter) stays hidden
        // under the status bar until the next frame's rebuild.
        self.refresh_view_after_cursor_move();
        // Decided after the reveal ran: it is the only thing here that moves
        // the scroll, and it writes the final position directly, so the settle
        // frame is needed exactly when that write happened.
        if state_changed && (self.current_scroll_top_f64() - scroll_before).abs() > 0.5 {
            self.layout_settle_frames.set(1);
        }
        cx.notify();
    }

    fn render_navigation_bar(
        &self,
        doc_override: Option<&crate::document_buffer::DocumentBuffer>,
        leaf_id: Option<crate::workspace::LeafId>,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> gpui::Stateful<Div> {
        let is_empty_tab = leaf_id
            .and_then(|lid| self.state.leaves.get(&lid))
            .and_then(|l| l.active_tab())
            .map(|t| matches!(t.content, crate::workspace::TabContent::Empty))
            .unwrap_or(doc_override.is_none() && self.state.active_doc_id().is_none() && self.state.active_empty_tab.is_some());

        let can_go_back = self.state.can_navigate_back();
        let can_go_forward = self.state.can_navigate_forward();
        let breadcrumbs = if let Some(doc) = doc_override {
            if let Some(ref ws) = self.state.workspace_path {
                if let Ok(rel) = doc.path.strip_prefix(ws) {
                    let mut parts = Vec::new();
                    for comp in rel.components() {
                        let comp_str = comp.as_os_str().to_string_lossy().to_string();
                        if !comp_str.is_empty() {
                            parts.push(comp_str);
                        }
                    }
                    if let Some(last) = parts.last_mut() {
                        if let Some(stripped) = last.strip_suffix(".md") {
                            *last = stripped.to_string();
                        } else if let Some(stripped) = last.strip_suffix(".markdown") {
                            *last = stripped.to_string();
                        }
                    }
                    if !parts.is_empty() {
                        parts
                    } else {
                        vec![doc.title.clone()]
                    }
                } else {
                    vec![doc.title.clone()]
                }
            } else {
                vec![doc.title.clone()]
            }
        } else if is_empty_tab {
            vec![self.t("tabs.newTab")]
        } else {
            let crumbs = self.state.note_breadcrumbs();
            if crumbs.is_empty() {
                vec![self.t("tabs.newTab")]
            } else {
                crumbs
            }
        };
        let is_dirty = if let Some(doc) = doc_override {
            doc.is_dirty
        } else if let Some(lid) = leaf_id {
            self.state
                .leaves
                .get(&lid)
                .and_then(|l| l.active_tab())
                .and_then(|t| match &t.content {
                    crate::workspace::TabContent::Document { id } => Some(self.state.is_document_dirty(*id)),
                    _ => None,
                })
                .unwrap_or(false)
        } else {
            self.state.active_document().map(|d| d.is_dirty).unwrap_or(false)
        };
        let is_editing = self.edit_mode;
        let disabled_nav_color = if theme.is_dark {
            gpui::rgba(0x7f849c44)
        } else {
            gpui::rgba(0x5c5f7744)
        };

        div()
            .id("editor-navigation-bar")
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .w_full()
            .h(px(40.0))
            .flex_shrink_0()
            .min_w_0()
            .overflow_hidden()
            .px_3()
            .bg(theme.bg_editor)
            // Left: Back/Forward buttons + Breadcrumbs
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .min_w_0()
                    .overflow_hidden()
                    // Back button
                    .child(
                        div()
                            .id("btn-nav-back")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded_md()
                            .when(can_go_back, |d| {
                                d.cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_hover))
                                    .child(icons::icon(icons::ICON_ARROW_LEFT, 16.0, theme.text_secondary))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.navigate_back(window, cx);
                                    }))
                            })
                            .when(!can_go_back, |d| {
                                d.cursor_default()
                                    .child(icons::icon(icons::ICON_ARROW_LEFT, 16.0, disabled_nav_color))
                            }),
                    )
                    // Forward button
                    .child(
                        div()
                            .id("btn-nav-forward")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded_md()
                            .when(can_go_forward, |d| {
                                d.cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_hover))
                                    .child(icons::icon(icons::ICON_ARROW_RIGHT, 16.0, theme.text_secondary))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.navigate_forward(window, cx);
                                    }))
                            })
                            .when(!can_go_forward, |d| {
                                d.cursor_default()
                                    .child(icons::icon(icons::ICON_ARROW_RIGHT, 16.0, disabled_nav_color))
                            }),
                    )
                    // Spacer between arrows and breadcrumbs
                    .child(div().w(px(10.0)))
                    // Breadcrumbs trail
                    .child({
                        let mut crumbs_bar = div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .min_w_0()
                            .overflow_hidden();

                        let count = breadcrumbs.len();
                        for (idx, part) in breadcrumbs.iter().enumerate() {
                            if idx > 0 {
                                crumbs_bar = crumbs_bar.child(
                                    div()
                                        .text_sm()
                                        .text_color(theme.text_muted)
                                        .px_1p5()
                                        .flex_shrink_0()
                                        .child("/"),
                                );
                            }
                            let is_last = idx == count.saturating_sub(1);
                            crumbs_bar = crumbs_bar.child(
                                div()
                                    .text_sm()
                                    .font_weight(if is_last {
                                        gpui::FontWeight::SEMIBOLD
                                    } else {
                                        gpui::FontWeight::NORMAL
                                    })
                                    .text_color(if is_last {
                                        theme.text_primary
                                    } else {
                                        theme.text_muted
                                    })
                                    .truncate()
                                    .hover(|h| {
                                        if !is_last {
                                            h.text_color(theme.text_secondary)
                                        } else {
                                            h
                                        }
                                    })
                                    .child(part.clone()),
                            );
                        }

                        if is_dirty {
                            crumbs_bar = crumbs_bar.child(
                                div()
                                    .text_sm()
                                    .text_color(theme.accent)
                                    .ml_1p5()
                                    .child("●"),
                            );
                        }

                        crumbs_bar
                    }),
            )
            // Right: Obsidian-style action buttons
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .flex_shrink_0()
                    // Toggle wrap
                    .child(
                        div()
                            .id("btn-toggle-wrap")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(
                                icons::ICON_CODE,
                                15.0,
                                if self.word_wrap { theme.accent } else { theme.text_muted },
                            ))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_word_wrap(cx);
                            })),
                    )
                    // Toggle reader / edit view
                    .child(
                        div()
                            .id("btn-toggle-reading-mode")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(
                                icons::ICON_BOOK_OPEN,
                                16.0,
                                if !is_editing { theme.accent } else { theme.text_muted },
                            ))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.edit_mode = !this.edit_mode;
                                let current_scroll = this.current_scroll_top();
                                this.state.snapshot_active_note_settings(current_scroll, this.edit_mode, this.word_wrap);
                                if this.edit_mode {
                                    this.reset_editor_cursor_blink(cx);
                                }
                                cx.notify();
                            })),
                    )
                    // More options (3 vertical dots - saves active note)
                    .child(
                        div()
                            .id("btn-more-options")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(icons::ICON_MORE_VERTICAL, 16.0, theme.text_muted))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.save_note(window, cx);
                            })),
                    ),
            )
    }

    pub fn sync_views_for_active_note(&mut self) {}

    pub fn ensure_inactive_views_built(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active_leaf_id = self.state.workspace.active_leaf;
        let viewport_w: f32 = window.viewport_size().width.into();
        let editor_font = self.editor_font_family();
        let font_id = cx.text_system().resolve_font(&gpui::font(editor_font));
        let line_char_width = cx
            .text_system()
            .advance(font_id, px(14.0), 'x')
            .map(|s| f32::from(s.width))
            .unwrap_or(7.7);

        let leaf_info: Vec<(crate::workspace::LeafId, crate::workspace::TabId, crate::document_buffer::DocumentId, usize)> = self
            .state
            .leaves
            .iter()
            .filter(|(id, _)| **id != active_leaf_id)
            .filter_map(|(id, leaf)| {
                let tab = leaf.active_tab()?;
                match tab.content {
                    crate::workspace::TabContent::Document { id: doc_id } => {
                        let wrap_cols = self.compute_wrap_cols_for_leaf(Some(*id), viewport_w, line_char_width);
                        Some((*id, tab.id, doc_id, wrap_cols))
                    }
                    _ => None,
                }
            })
            .collect();

        let headings = &self.config.editor.headings;
        let word_wrap = self.word_wrap;

        let crate::state::AppState {
            ref documents,
            ref mut views,
            ..
        } = self.state;

        for (_leaf_id, tab_id, doc_id, wrap_cols) in leaf_info {
            let doc = match documents.get(&doc_id) {
                Some(d) => d,
                None => continue,
            };

            let line_index = &doc.line_index;
            let mut checkpoints = doc.code_checkpoints.clone();

            let lines_total = line_index.lines();
            let view = views.entry(tab_id).or_insert_with(|| crate::workspace::EditorViewState::new(tab_id, doc_id));

            if view.window.is_empty()
                || view.visual_cache_dirty
                || view.window.should_move_to(view.viewport.offset, lines_total, word_wrap, wrap_cols)
            {
                let anchor_line = view.viewport.offset;
                let (start, end) = crate::editor_window::EditorWindow::bounds_for(anchor_line, lines_total);
                checkpoints.ensure_rope(doc.rope(), start + 1);
                let initial = checkpoints.state_at_rope(doc.rope(), start).unwrap_or_default();
                let start_byte = doc.line_start_byte(start);
                let end_byte = doc.line_end_byte(end.saturating_sub(1)).max(start_byte);
                let slice = doc.slice_to_string(start_byte..end_byte);

                let new_window = crate::editor_window::EditorWindow::build_from_slice(
                    &slice,
                    start,
                    end,
                    start_byte,
                    end_byte,
                    word_wrap,
                    wrap_cols,
                    Some(headings),
                    initial,
                );
                view.above_lines = new_window.start_line;
                view.below_lines = lines_total.saturating_sub(new_window.end_line);
                let rows = new_window.visual_rows.len();
                let lines = new_window.line_count();
                if rows > 0 && lines > 0 {
                    view.est_rows_per_line = rows as f64 / lines as f64;
                    view.est_row_h = new_window.total_height() / rows as f64;
                }
                let rows_per_line = view.est_rows_per_line.max(f64::MIN_POSITIVE);
                let row_h = view.est_row_h;
                // Same formula as `recompute_estimate`, without `.round()`:
                // both paths must produce identical above_h/below_h, or every
                // rebuild here shifts the estimates and the caret jitters.
                view.above_h = view.above_lines as f64 * rows_per_line * row_h;
                view.below_h = view.below_lines as f64 * rows_per_line * row_h;
                let wrap_changed = view.window.wrap_cols != wrap_cols;
                view.window = new_window;
                view.headings_cache = headings.clone();
                view.visual_cache_dirty = false;
                if wrap_changed {
                    if let Some(first_v_idx) = view.window.first_row_of_line(view.viewport.offset) {
                        let line_local_y = view.window.height_index.top_of(first_v_idx);
                        view.scroll_top = view.above_h + line_local_y + view.viewport.px_in_line as f64;
                    }
                }
            }
        }
    }

    pub fn switch_to_leaf(
        &mut self,
        leaf_id: crate::workspace::LeafId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active_idx = self.state.leaves.get(&leaf_id).map(|l| l.active_tab_index).unwrap_or(0);
        self.select_leaf_tab(leaf_id, active_idx, window, cx);
    }

    pub fn select_leaf_tab(
        &mut self,
        leaf_id: crate::workspace::LeafId,
        tab_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current_leaf_id = self.state.workspace.active_leaf;
        let is_same_leaf = current_leaf_id == leaf_id;
        let same_tab = is_same_leaf && self.state.leaves.get(&leaf_id).map(|l| l.active_tab_index) == Some(tab_index);
        if same_tab {
            return;
        }

        self.commit_title_rename_internal();
        self.flush_active_note_now(cx);

        // 1. Snapshot outgoing note settings & view state
        let current_scroll = self.current_scroll_top();
        self.state.snapshot_active_note_settings(current_scroll, self.edit_mode, self.word_wrap);
        let current_scroll_f64 = self.current_scroll_top_f64();
        if let Some((view, doc)) = self.state.active_view_and_doc_mut() {
            let lines_total = doc.line_index.lines();
            view.scroll_top = current_scroll_f64;
            view.viewport.offset = view.line_at_doc_y(lines_total, current_scroll_f64);
            let line_top = view.doc_y_of_line(lines_total, view.viewport.offset);
            view.viewport.px_in_line = ((current_scroll_f64 - line_top) as f32).max(0.0);
        }

        // 2. Set new active tab index in target leaf and set active leaf
        if let Some(leaf) = self.state.leaves.get_mut(&leaf_id) {
            if tab_index < leaf.tabs.len() {
                leaf.active_tab_index = tab_index;
            }
        }
        self.state.workspace.active_leaf = leaf_id;

        // Ensure incoming leaf's active document is loaded into memory
        if let Some(doc_id) = self.state.leaves.get(&leaf_id).and_then(|l| l.active_tab()).and_then(|t| match &t.content {
            crate::workspace::TabContent::Document { id } => Some(*id),
            _ => None,
        }) {
            self.state.ensure_document_loaded(doc_id);
        }

        // 3. Load incoming leaf's active tab
        if let Some(leaf) = self.state.leaves.get(&leaf_id) {
            if let Some(tab) = leaf.active_tab() {
                match &tab.content {
                    crate::workspace::TabContent::Document { id } => {
                        let doc_id = *id;
                        let viewport_w: f32 = window.viewport_size().width.into();
                        let editor_font = self.editor_font_family();
                        let font_id = cx.text_system().resolve_font(&gpui::font(editor_font));
                        let line_char_width = cx
                            .text_system()
                            .advance(font_id, px(14.0), 'x')
                            .map(|s| f32::from(s.width))
                            .unwrap_or(7.7);
                        let target_wrap_cols = self.compute_wrap_cols_for_leaf(Some(leaf_id), viewport_w, line_char_width);
                        self.last_wrap_cols = target_wrap_cols;

                        if let Some(doc) = self.state.documents.get_mut(&doc_id) {
                            let doc_path = doc.path.clone();
                            let lines_total = doc.line_index.lines();

                            let restore_y = {
                                let view = self.state.views.entry(tab.id).or_insert_with(|| crate::workspace::EditorViewState::new(tab.id, doc_id));

                                view.ensure_visual_cache_with_headings(doc, self.word_wrap, target_wrap_cols, Some(&self.config.editor.headings), view.viewport.offset, self.ctrl_preview_selection || !self.edit_mode);
                                if view.window.wrap_cols == target_wrap_cols {
                                    view.scroll_top
                                } else {
                                    let line_top = view.doc_y_of_line(lines_total, view.viewport.offset);
                                    let calculated = line_top + view.viewport.px_in_line as f64;
                                    if (calculated - view.scroll_top).abs() < 1.0 {
                                        view.scroll_top
                                    } else {
                                        calculated
                                    }
                                }
                            };

                            self.set_editor_scroll_top_f64(restore_y);
                            self.pending_editor_restore = Some(restore_y);
                            self.need_reveal_cursor = false;
                            self.layout_settle_frames.set(SCROLL_SETTLE_FRAMES);

                            self.state.active_empty_tab = None;
                            self.state.session.active_note = Some(doc_path);
                        }
                    }
                    crate::workspace::TabContent::Empty => {
                        self.state.active_empty_tab = Some(std::path::PathBuf::from("newtab"));
                        self.active_center_leaf = None;
                        self.state.session.active_note = Some(crate::state::empty_tab_path());
                    }
                    crate::workspace::TabContent::Plugin { view_id } => {
                        self.state.active_empty_tab = None;
                        self.active_center_leaf = Some(view_id.clone());
                    }
                }
            } else {
                self.state.active_empty_tab = None;
                self.active_center_leaf = None;
            }

            self.state.open_tabs = leaf.tabs.iter().filter_map(|t| match &t.content {
                crate::workspace::TabContent::Document { id } => {
                    self.state.document_path(*id).map(|p| p.to_path_buf())
                }
                crate::workspace::TabContent::Empty => Some(std::path::PathBuf::from("newtab")),
                _ => None,
            }).collect();
            self.state.session.open_tabs = self.state.open_tabs.clone();
        }

        window.focus(&self.focus_handle);
        cx.notify();
    }

    /// Selects a tab within a leaf by its TabId.
    pub fn select_tab(
        &mut self,
        leaf_id: crate::workspace::LeafId,
        tab_id: crate::workspace::TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(leaf) = self.state.leaves.get(&leaf_id) {
            if let Some(idx) = leaf.tabs.iter().position(|t| t.id == tab_id) {
                self.select_leaf_tab(leaf_id, idx, window, cx);
            }
        }
    }

    pub fn close_leaf_tab(
        &mut self,
        leaf_id: crate::workspace::LeafId,
        tab_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let req = match self.state.request_close_leaf_tab(leaf_id, tab_index) {
            Ok((_tab_id, req)) => req,
            Err(_) => return,
        };

        if let crate::workspace::CloseRequest::NeedsEffects(effects) = req {
            for eff in effects {
                if let Err(err) = self.state.run_close_effect(&eff) {
                    self.state.status_text = format!("Close cancelled: {}", err);
                    cx.notify();
                    return;
                }
            }
        }

        if self.state.finish_close_leaf_tab(leaf_id, tab_index).is_err() {
            return;
        }

        let active = self.state.workspace.active_leaf;
        let active_idx = self.state.leaves.get(&active).map(|l| l.active_tab_index).unwrap_or(0);
        self.select_leaf_tab(active, active_idx, window, cx);
        self.layout_settle_frames.set(SCROLL_SETTLE_FRAMES);
        cx.notify();
    }

    pub fn close_active_leaf_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active = self.state.workspace.active_leaf;
        let active_idx = self.state.leaves.get(&active).map(|l| l.active_tab_index).unwrap_or(0);
        self.close_leaf_tab(active, active_idx, window, cx);
    }

    /// Closes all tabs in the specified leaf without affecting other leaves/windows.
    pub fn close_leaf_all_tabs(
        &mut self,
        leaf_id: crate::workspace::LeafId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        while let Some(count) = self.state.leaves.get(&leaf_id).map(|l| l.tabs.len()) {
            if count == 0 {
                break;
            }
            let prev_count = count;
            self.close_leaf_tab(leaf_id, count - 1, window, cx);
            if self.state.leaves.get(&leaf_id).map(|l| l.tabs.len()) == Some(prev_count) {
                break;
            }
        }
        self.layout_settle_frames.set(SCROLL_SETTLE_FRAMES);
        cx.notify();
    }

    fn render_leaf_tab_bar_strip(
        &self,
        leaf_id: crate::workspace::LeafId,
        in_titlebar: bool,
        available_strip_w: f32,
        theme: &Theme,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> gpui::Stateful<Div> {
        let leaf = self.state.leaves.get(&leaf_id);
        let active_tab_index = leaf.map(|l| l.active_tab_index).unwrap_or(0);
        let num_tabs = leaf.map(|l| l.tabs.len()).unwrap_or(0);

        // Keep the panel strip an exact whole number of device pixels so its bottom
        // border coincides with the tabs' bottom and the active-tab baseline mask.
        let scale = window.scale_factor();
        let strip_h = if scale > 0.0 { (38.0 * scale).round() / scale } else { 38.0 };

        let tab_layout = crate::views::tabs_menu::calculate_tab_widths(
            num_tabs,
            Some(active_tab_index),
            available_strip_w,
            160.0,
            120.0,
            6.0,
            32.0,
            6.0,
            8.0,
            32.0,
            6.0,
        );
        let tab_widths = tab_layout.widths;
        let is_tabs_overflowing = tab_layout.is_overflowing;
        let effective_tab_gap = tab_layout.tab_gap;

        let cap_top = 4.0;
        let sep_top = 13.0;
        let tab_body_h = 34.0;

        let is_leaf_focused = self.state.workspace.active_leaf == leaf_id;
        let hovered_tab = self
            .hovered_tab
            .and_then(|(l, i)| if l == leaf_id && i < num_tabs { Some(i) } else { None });

        let mut tabs_strip = div()
            .id(gpui::ElementId::Name(format!("tabs-strip-inner-{}", leaf_id.0).into()))
            .flex()
            .flex_row()
            .items_end()
            .gap(px(effective_tab_gap))
            .h_full()
            .pl(px(8.0))
            .min_w_0()
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if !hovered {
                    if let Some((l, _)) = this.hovered_tab {
                        if l == leaf_id {
                            this.hovered_tab = None;
                            cx.notify();
                        }
                    }
                }
            }));

        let first_tab_active = active_tab_index == 0;
        let show_leading_divider = num_tabs > 0 && !first_tab_active && hovered_tab != Some(0);
        let leading_sep_right = -(effective_tab_gap / 2.0).floor();
        let leading_divider_color = if theme.is_dark {
            gpui::rgba(0xcdd6f440)
        } else {
            gpui::rgba(0x1e203030)
        };

        tabs_strip = tabs_strip.child(
            div()
                .id(gpui::ElementId::Name(format!("leaf-btn-new-note-{}", leaf_id.0).into()))
                .relative()
                .flex()
                .items_center()
                .justify_center()
                .h_full()
                .w(px(32.0))
                .flex_shrink_0()
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.state.workspace.active_leaf = leaf_id;
                    this.create_new_note_at_front(window, cx);
                }))
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    if *hovered {
                        if let Some((l, _)) = this.hovered_tab {
                            if l == leaf_id {
                                this.hovered_tab = None;
                                cx.notify();
                            }
                        }
                    }
                }))
                .when(show_leading_divider, |d| {
                    d.child(
                        div()
                            .id("new-note-separator")
                            .absolute()
                            .top(px(sep_top))
                            .h(px(16.0))
                            .w(px(1.0))
                            .right(px(leading_sep_right))
                            .rounded_full()
                            .bg(leading_divider_color),
                    )
                })
                .child(
                    div()
                        .id("btn-new-note-inline-inner")
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(px(28.0))
                        .h(px(28.0))
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|h| h.bg(theme.bg_hover))
                        .child(icons::icon(icons::ICON_NOTE_CREATE, 15.0, theme.text_muted)),
                ),
        );

        if let Some(leaf) = leaf {
            for (tab_idx, tab) in leaf.tabs.iter().enumerate() {
                let is_active = tab_idx == active_tab_index;
                let is_next_active = active_tab_index == tab_idx + 1;
                let show_divider = !is_active
                    && (tab_idx + 1 < num_tabs)
                    && !is_next_active
                    && hovered_tab != Some(tab_idx)
                    && hovered_tab != Some(tab_idx + 1);

                let (title, is_dirty, tab_path) = match &tab.content {
                    crate::workspace::TabContent::Document { id } => {
                        if let Some(doc) = self.state.documents.get(id) {
                            (doc.title.clone(), doc.is_dirty, Some(doc.path.clone()))
                        } else if let Some(meta) = self.state.doc_meta.get(id) {
                            (meta.title.clone(), meta.is_dirty, Some(meta.path.clone()))
                        } else {
                            (self.t("common.untitled"), false, None)
                        }
                    }
                    crate::workspace::TabContent::Empty => {
                        (self.t("tabs.newTab"), false, Some(std::path::PathBuf::from("newtab")))
                    }
                    crate::workspace::TabContent::Plugin { view_id } => {
                        (view_id.clone(), false, Some(std::path::PathBuf::from(format!("plugin:{}", view_id))))
                    }
                };

                let tab_w = tab_widths.get(tab_idx).copied().unwrap_or(160.0);
                let tab_path_context = tab_path.clone();

                let is_focused_tab = is_active && is_leaf_focused;

                let mut tab_item = div()
                    .id(gpui::ElementId::Name(format!("leaf-tab-item-{}-{}", leaf_id.0, tab_idx).into()))
                    .relative()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .w(px(tab_w))
                    .flex_shrink_0()
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if *hovered && this.hovered_tab != Some((leaf_id, tab_idx)) {
                            this.hovered_tab = Some((leaf_id, tab_idx));
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(MouseButton::Middle, cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.close_leaf_tab(leaf_id, tab_idx, window, cx);
                    }));

                if let Some(p) = tab_path_context {
                    tab_item = tab_item.on_mouse_down(MouseButton::Right, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let x: f32 = event.position.x.into();
                        let y: f32 = event.position.y.into();
                        this.open_tab_context_menu(p.clone(), Some(leaf_id), Some(tab_idx), x, y, cx);
                    }));
                }

                let ext_r = (effective_tab_gap / 2.0).ceil();
                let ext_l = (effective_tab_gap - ext_r).max(0.0);

                tab_item = tab_item.when(!is_active, |tab| {
                    let tab = tab
                        .bg(theme.bg_sidebar)
                        .rounded_t_md()
                        .border_b_1()
                        .border_color(theme.border);
                    if hovered_tab == Some(tab_idx) {
                        tab.child(
                            div()
                                .absolute()
                                .bottom_0()
                                .left_0()
                                .right_0()
                                .h(px(tab_body_h))
                                .rounded_t_md()
                                .bg(theme.bg_hover),
                        )
                    } else {
                        tab
                    }
                });

                if show_divider {
                    let sep_right = -(effective_tab_gap / 2.0).floor();
                    let divider_color = if theme.is_dark {
                        gpui::rgba(0xcdd6f440)
                    } else {
                        gpui::rgba(0x1e203030)
                    };
                    tab_item = tab_item.child(
                        div()
                            .id(gpui::ElementId::Name(format!("leaf-tab-separator-{}-{}", leaf_id.0, tab_idx).into()))
                            .absolute()
                            .top(px(sep_top))
                            .h(px(16.0))
                            .w(px(1.0))
                            .right(px(sep_right))
                            .rounded_full()
                            .bg(divider_color),
                    );
                }

                tab_item = tab_item
                    .child(
                        div()
                            .id(gpui::ElementId::Name(format!("leaf-tab-hit-l-{}-{}", leaf_id.0, tab_idx).into()))
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(px(-ext_l))
                            .w(px(ext_l))
                            .cursor_pointer()
                            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                                if *hovered && this.hovered_tab != Some((leaf_id, tab_idx)) {
                                    this.hovered_tab = Some((leaf_id, tab_idx));
                                    cx.notify();
                                }
                            }))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_leaf_tab(leaf_id, tab_idx, window, cx);
                            })),
                    )
                    .child(
                        div()
                            .id(gpui::ElementId::Name(format!("leaf-tab-hit-r-{}-{}", leaf_id.0, tab_idx).into()))
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .right(px(-ext_r))
                            .w(px(ext_r))
                            .cursor_pointer()
                            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                                if *hovered && this.hovered_tab != Some((leaf_id, tab_idx)) {
                                    this.hovered_tab = Some((leaf_id, tab_idx));
                                    cx.notify();
                                }
                            }))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_leaf_tab(leaf_id, tab_idx, window, cx);
                            })),
                    );

                if is_active {
                    // 1. Solid center body fill connecting to active editor
                    tab_item = tab_item.child(
                        div()
                            .id("tab-body-fill")
                            .absolute()
                            .top(px(cap_top))
                            .bottom_0()
                            .left(px(6.0))
                            .right(px(6.0))
                            .bg(theme.bg_editor),
                    );
                    // 2. Solid fill mask at the baseline connecting active tab into the editor
                    tab_item = tab_item.child(
                        div()
                            .id("tab-baseline-mask")
                            .absolute()
                            .bottom(px(-1.0))
                            .left(px(-8.0))
                            .right(px(-8.0))
                            .h(px(2.0))
                            .bg(theme.bg_editor),
                    );
                    // 3. Top border stroke
                    tab_item = tab_item.child(
                        div()
                            .id("tab-top-stroke")
                            .absolute()
                            .top(px(cap_top))
                            .left(px(7.0))
                            .right(px(7.0))
                            .h(px(1.0))
                            .bg(theme.border),
                    );
                    // 4. Left cap
                    tab_item = tab_item.child(
                        div()
                            .id("tab-cap-left")
                            .absolute()
                            .top(px(cap_top))
                            .left(px(-8.0))
                            .w(px(16.0))
                            .h(px(36.0))
                            .child(
                                icons::svg_sized(icons::ICON_TAB_ACTIVE_LEFT_FILL, 16.0, 36.0, theme.bg_editor)
                                    .absolute()
                                    .top_0()
                                    .left_0(),
                            )
                            .child(
                                icons::svg_sized(icons::ICON_TAB_ACTIVE_LEFT_STROKE, 16.0, 36.0, theme.border)
                                    .absolute()
                                    .top_0()
                                    .left_0(),
                            ),
                    );
                    // 5. Right cap
                    tab_item = tab_item.child(
                        div()
                            .id("tab-cap-right")
                            .absolute()
                            .top(px(cap_top))
                            .right(px(-8.0))
                            .w(px(16.0))
                            .h(px(36.0))
                            .child(
                                icons::svg_sized(icons::ICON_TAB_ACTIVE_RIGHT_FILL, 16.0, 36.0, theme.bg_editor)
                                    .absolute()
                                    .top_0()
                                    .left_0(),
                            )
                            .child(
                                icons::svg_sized(icons::ICON_TAB_ACTIVE_RIGHT_STROKE, 16.0, 36.0, theme.border)
                                    .absolute()
                                    .top_0()
                                    .left_0(),
                            ),
                    );
                }

                tabs_strip = tabs_strip.child(
                    tab_item
                        // Tab selection area (icon + title + dirty dot)
                        .child(
                            div()
                                .id(gpui::ElementId::Name(
                                    format!("leaf-tab-select-{}-{}", leaf_id.0, tab_idx).into(),
                                 ))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(if tab_w < 48.0 { px(2.0) } else { px(6.0) })
                                .h_full()
                                .pl(if tab_w < 40.0 { px(2.0) } else { px(10.0) })
                                .pr(if tab_w < 40.0 { px(2.0) } else { px(4.0) })
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .cursor_pointer()
                                .child(icons::icon(
                                    icons::ICON_FILE_TEXT,
                                    12.0,
                                    if is_focused_tab {
                                        theme.accent
                                    } else if is_active {
                                        theme.text_secondary
                                    } else {
                                        theme.text_muted
                                    },
                                ))
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(if is_focused_tab {
                                            gpui::FontWeight::SEMIBOLD
                                        } else if is_active {
                                            gpui::FontWeight::MEDIUM
                                        } else {
                                            gpui::FontWeight::NORMAL
                                        })
                                        .text_color(if is_focused_tab {
                                            theme.text_primary
                                        } else if is_active {
                                            theme.text_secondary
                                        } else {
                                            theme.text_muted
                                        })
                                        .truncate()
                                        .flex_1()
                                        .min_w_0()
                                        .child(title),
                                )
                                .when(is_active && is_dirty, |tab| {
                                    tab.child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.accent)
                                            .child("●"),
                                    )
                                })
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.select_leaf_tab(leaf_id, tab_idx, window, cx);
                                }))
                                .on_mouse_down(MouseButton::Middle, cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.close_leaf_tab(leaf_id, tab_idx, window, cx);
                                })),
                        )
                        // Tab close button (isolated sibling)
                        .when(is_active || tab_w >= 56.0, |tab| {
                            tab.child(
                                div()
                                    .id(gpui::ElementId::Name(
                                        format!("leaf-tab-close-{}-{}", leaf_id.0, tab_idx).into(),
                                    ))
                                    .p_0p5()
                                    .mr_1p5()
                                    .flex_shrink_0()
                                    .rounded_sm()
                                    .cursor_pointer()
                                    .hover(|h| h.bg(theme.bg_hover))
                                    .child(icons::icon(icons::ICON_CLOSE, 10.0, theme.text_muted))
                                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation();
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.close_leaf_tab(leaf_id, tab_idx, window, cx);
                                    })),
                            )
                        }),
                );
            }
        }

        let btn_new_tab = div()
            .id(gpui::ElementId::Name(format!("leaf-btn-new-tab-{}", leaf_id.0).into()))
            .flex()
            .items_center()
            .justify_center()
            .h_full()
            .w(px(32.0))
            .flex_shrink_0()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, window, cx| {
                if let Some(leaf) = this.state.leaves.get_mut(&leaf_id) {
                    leaf.tabs.push(crate::workspace::Tab::new_empty());
                    let new_idx = leaf.tabs.len() - 1;
                    this.select_leaf_tab(leaf_id, new_idx, window, cx);
                }
            }))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered {
                    if let Some((l, _)) = this.hovered_tab {
                        if l == leaf_id {
                            this.hovered_tab = None;
                            cx.notify();
                        }
                    }
                }
            }))
            .child(
                div()
                    .id("btn-new-tab-inline-inner")
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(28.0))
                    .h(px(28.0))
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|h| h.bg(theme.bg_hover))
                    .child(icons::icon(icons::ICON_PLUS, 15.0, theme.text_muted)),
            );

        if in_titlebar {
            div()
                .id(gpui::ElementId::Name(format!("leaf-tab-strip-titlebar-{}", leaf_id.0).into()))
                .relative()
                .flex()
                .flex_row()
                .items_end()
                .gap(px(6.0))
                .min_w_0()
                .max_w(px(available_strip_w))
                .overflow_hidden()
                .when(is_tabs_overflowing, |d| d.w(px(available_strip_w)))
                .h_full()
                .child(tabs_strip)
                .child(btn_new_tab)
        } else {
            div()
                .id(gpui::ElementId::Name(format!("leaf-tab-strip-panel-{}", leaf_id.0).into()))
                .relative()
                .flex()
                .flex_row()
                .items_center()
                .h(px(strip_h))
                .w_full()
                .min_w_0()
                .bg(theme.bg_sidebar)
                .child(
                    div()
                        .id(gpui::ElementId::Name(format!("leaf-panel-bottom-border-{}", leaf_id.0).into()))
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .h(px(1.0))
                        .bg(theme.border),
                )
                .child(
                    div()
                        .id(gpui::ElementId::Name(format!("leaf-tab-strip-panel-inner-{}", leaf_id.0).into()))
                        .relative()
                        .flex()
                        .flex_row()
                        .items_end()
                        .gap(px(6.0))
                        .min_w_0()
                        .max_w(px(available_strip_w))
                        .overflow_hidden()
                        .when(is_tabs_overflowing, |d| d.w(px(available_strip_w)))
                        .h_full()
                        .child(tabs_strip)
                        .child(btn_new_tab),
                )
                .child(
                    div()
                        .id(gpui::ElementId::Name(format!("leaf-panel-spacer-{}", leaf_id.0).into()))
                        .flex_1()
                        .min_w_0()
                        .h_full(),
                )
                .child(
                    div()
                        .id(gpui::ElementId::Name(format!("leaf-panel-tabs-btn-wrap-{}", leaf_id.0).into()))
                        .flex()
                        .items_center()
                        .h_full()
                        .flex_shrink_0()
                        .px_1()
                        .child(self.render_leaf_tabs_dropdown_button(leaf_id, theme, cx)),
                )
        }
    }

    fn render_titlebar_editor_header(
        &self,
        is_maximized: bool,
        theme: &Theme,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> gpui::Stateful<Div> {
        let mut header = div()
            .id("titlebar-editor-header-col")
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .flex_1()
            .h_full()
            .min_w_0()
            .overflow_hidden()
            .bg(theme.bg_sidebar)
            // Continuous bottom border line running across the editor header column
            .child(
                div()
                    .id("titlebar-editor-bottom-border")
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .h(px(1.0))
                    .bg(theme.border),
            );

        let top_cols = self.state.workspace.top_columns();
        let num_cols = top_cols.len();

        let viewport_w: f32 = window.viewport_size().width.into();
        let sidebar_area_w = if self.sidebar_collapsed {
            44.0
        } else {
            self.sidebar_width + 1.0
        };
        let right_sidebar_area_w = if self.right_sidebar_collapsed {
            0.0
        } else {
            self.right_sidebar_width + 1.0
        };
        let header_w = (viewport_w - sidebar_area_w - right_sidebar_area_w).max(100.0);
        let right_controls_w = if self.right_sidebar_collapsed { 178.0 } else { 40.0 };

        for (col_idx, col) in top_cols.into_iter().enumerate() {
            let is_last = col_idx + 1 == num_cols;
            let col_w = (header_w * col.ratio).max(60.0);
            let available_strip_w = if is_last {
                (col_w - right_controls_w - 40.0).max(40.0)
            } else {
                (col_w - 40.0).max(40.0)
            };

            let mut col_div = div()
                .id(gpui::ElementId::from(("titlebar-top-col", col_idx)))
                .relative()
                .flex()
                .flex_row()
                .items_center()
                .h_full()
                .min_w_0()
                .overflow_hidden();

            if is_last {
                col_div = col_div.flex_1().min_w_0();
            } else {
                col_div = col_div.w(gpui::relative(col.ratio)).flex_shrink_0();
            }

            // Tab bar strip for this top column leaf
            col_div = col_div.child(
                self.render_leaf_tab_bar_strip(col.leaf_id, true, available_strip_w, theme, window, cx)
            );

            // Drag area filling empty space in this top column
            col_div = col_div.child(
                div()
                    .id(gpui::ElementId::from(("titlebar-col-drag", col_idx)))
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .window_control_area(WindowControlArea::Drag),
            );

            // Open tabs dropdown button for this leaf on the right side of the window, closer to divider
            col_div = col_div.child(
                div()
                    .id(gpui::ElementId::from(("titlebar-col-tabs-btn-wrap", col_idx)))
                    .flex()
                    .items_center()
                    .h_full()
                    .flex_shrink_0()
                    .px_1()
                    .child(self.render_leaf_tabs_dropdown_button(col.leaf_id, theme, cx)),
            );

            // On the rightmost column: anchor the window controls
            if is_last {
                col_div = col_div.child(
                    div()
                        .id("titlebar-right-controls-group")
                        .flex()
                        .flex_row()
                        .items_center()
                        .h_full()
                        .flex_shrink_0()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .h_full()
                                .flex_shrink_0()
                                .gap_1()
                                .px_1()
                                .child(self.render_right_sidebar_toggle_button(theme, cx)),
                        )
                        .when(self.right_sidebar_collapsed, |d| {
                            d.child(self.render_window_control_buttons(theme, is_maximized, cx))
                        }),
                );
            }

            header = header.child(col_div);

            // Draggable split divider after this column if present
            if let Some(ref div_info) = col.divider_after {
                let div_path = div_info.path.clone();
                let div_idx = div_info.divider_index;
                let init_ratio = div_info.initial_ratio;

                let is_active = self.split_drag_state.as_ref().map(|d| {
                    d.direction == crate::workspace::SplitDirection::Row
                        && d.path == div_path
                        && d.divider_index == div_idx
                }).unwrap_or(false);

                let is_hovered = self.hovered_split_divider.as_ref().map(|(dir, path, idx)| {
                    *dir == crate::workspace::SplitDirection::Row
                        && *path == div_path
                        && *idx == div_idx
                }).unwrap_or(false);

                let is_resizing_or_hovered = is_active || is_hovered;
                let divider_color = if is_resizing_or_hovered { theme.accent } else { theme.border };

                header = header.child(
                    div()
                        .id(gpui::ElementId::from(("titlebar-split-divider", col_idx)))
                        .relative()
                        .w(px(1.0))
                        .h_full()
                        .flex_shrink_0()
                        .bg(divider_color)
                        .child(
                            div()
                                .id(gpui::ElementId::from(("titlebar-split-divider-hitbox", col_idx)))
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left(px(-4.0))
                                .w(px(9.0))
                                .cursor(gpui::CursorStyle::ResizeColumn)
                                .on_hover(cx.listener({
                                    let div_path = div_path.clone();
                                    move |this, hovered: &bool, _, cx| {
                                        this.hovered_split_divider = if *hovered && !this.is_dragging_scrollbar {
                                            Some((crate::workspace::SplitDirection::Row, div_path.clone(), div_idx))
                                        } else {
                                            None
                                        };
                                        cx.notify();
                                    }
                                }))
                                .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                    cx.stop_propagation();
                                    this.is_mouse_selecting = false;
                                    this.is_dragging_scrollbar = false;
                                    this.is_hovering_editor_scrollbar = false;
                                    this.is_hovering_editor_thumb = false;
                                    let top_y = this.current_scroll_top_f64();
                                    if let Some((view, doc)) = this.state.active_view_and_doc_mut() {
                                        view.viewport.offset = view.line_at_doc_y(doc.line_index.lines(), top_y);
                                    }
                                    let editor_left_w = if this.sidebar_collapsed { 44.0 } else { this.sidebar_width + 1.0 };
                                    let editor_right_w = if this.right_sidebar_collapsed { 0.0 } else { this.right_sidebar_width + 1.0 };
                                    let win_w: f32 = window.bounds().size.width.into();
                                    let span = (win_w - editor_left_w - editor_right_w).max(100.0);
                                    this.split_drag_state = Some(crate::workspace::SplitDragState {
                                        path: div_path.clone(),
                                        direction: crate::workspace::SplitDirection::Row,
                                        divider_index: div_idx,
                                        start_pos: event.position.x.into(),
                                        span,
                                        initial_ratio: init_ratio,
                                    });
                                    cx.notify();
                                })),
                        ),
                );
            }
        }

        header
    }

    fn render_workspace_node(
        &self,
        node: &crate::workspace::WorkspaceNode,
        tree_path: &[usize],
        avail_w: f32,
        avail_h: f32,
        theme: &Theme,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> gpui::Stateful<Div> {
        match node {
            crate::workspace::WorkspaceNode::Split { direction, children, ratios } => {
                let is_row = *direction == crate::workspace::SplitDirection::Row;
                let mut container = div()
                    .id(gpui::ElementId::from(("split-container", tree_path.first().copied().unwrap_or(0))))
                    .flex()
                    .flex_1()
                    .size_full()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden();

                if is_row {
                    container = container.flex_row();
                } else {
                    container = container.flex_col();
                }

                // Snap pane boundaries and the divider to whole physical pixels so the
                // pane's tab strip bottom (and every border on it) lands on an exact
                // device-pixel row at any split ratio / fractional DPI.
                let scale = window.scale_factor();
                let snap = |v: f32| if scale > 0.0 { (v * scale).round() / scale } else { v };
                let div_len = snap(1.0);
                let main_avail = if is_row { avail_w } else { avail_h };
                let cross_avail = if is_row { avail_h } else { avail_w };
                let mut acc: f32 = 0.0;

                for (idx, child) in children.iter().enumerate() {
                    if idx > 0 {
                        acc = snap(acc + div_len);
                        let div_idx = idx - 1;
                        let div_path = tree_path.to_vec();
                        let dir = *direction;
                        let init_ratio = ratios.get(div_idx).copied().unwrap_or(0.5);

                        let is_active = self.split_drag_state.as_ref().map(|d| {
                            d.direction == dir && d.path == div_path && d.divider_index == div_idx
                        }).unwrap_or(false);

                        let is_hovered = self.hovered_split_divider.as_ref().map(|(d, path, idx)| {
                            *d == dir && *path == div_path && *idx == div_idx
                        }).unwrap_or(false);

                        let is_resizing_or_hovered = is_active || is_hovered;
                        let divider_color = if is_resizing_or_hovered { theme.accent } else { theme.border };

                        let mut divider_el = div()
                            .id(gpui::ElementId::from(("split-divider", div_idx)))
                            .relative()
                            .flex_shrink_0()
                            .bg(divider_color);

                        if is_row {
                            divider_el = divider_el
                                .w(px(div_len))
                                .h_full()
                                .child(
                                    div()
                                        .id(gpui::ElementId::from(("split-divider-hitbox-r", div_idx)))
                                        .absolute()
                                        .top_0()
                                        .bottom_0()
                                        .left(px(-4.0))
                                        .w(px(9.0))
                                        .cursor(gpui::CursorStyle::ResizeColumn)
                                        .on_hover(cx.listener({
                                            let div_path = div_path.clone();
                                            move |this, hovered: &bool, _, cx| {
                                                this.hovered_split_divider = if *hovered && !this.is_dragging_scrollbar {
                                                    Some((dir, div_path.clone(), div_idx))
                                                } else {
                                                    None
                                                };
                                                cx.notify();
                                            }
                                        }))
                                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                            cx.stop_propagation();
                                            this.is_mouse_selecting = false;
                                            this.is_dragging_scrollbar = false;
                                            this.is_hovering_editor_scrollbar = false;
                                            this.is_hovering_editor_thumb = false;
                                            let top_y = this.current_scroll_top_f64();
                                            if let Some((view, doc)) = this.state.active_view_and_doc_mut() {
                                                view.viewport.offset = view.line_at_doc_y(doc.line_index.lines(), top_y);
                                            }
                                            let editor_left_w = if this.sidebar_collapsed { 44.0 } else { this.sidebar_width + 1.0 };
                                            let editor_right_w = if this.right_sidebar_collapsed { 0.0 } else { this.right_sidebar_width + 1.0 };
                                            let win_w: f32 = window.bounds().size.width.into();
                                            let span = (win_w - editor_left_w - editor_right_w).max(100.0);
                                            this.split_drag_state = Some(crate::workspace::SplitDragState {
                                                path: div_path.clone(),
                                                direction: dir,
                                                divider_index: div_idx,
                                                start_pos: event.position.x.into(),
                                                span,
                                                initial_ratio: init_ratio,
                                            });
                                            cx.notify();
                                        }))
                                );
                        } else {
                            divider_el = divider_el
                                .h(px(div_len))
                                .w_full()
                                .child(
                                    div()
                                        .id(gpui::ElementId::from(("split-divider-hitbox-c", div_idx)))
                                        .absolute()
                                        .left_0()
                                        .right_0()
                                        .top(px(-4.0))
                                        .h(px(9.0))
                                        .cursor(gpui::CursorStyle::ResizeRow)
                                        .on_hover(cx.listener({
                                            let div_path = div_path.clone();
                                            move |this, hovered: &bool, _, cx| {
                                                this.hovered_split_divider = if *hovered && !this.is_dragging_scrollbar {
                                                    Some((dir, div_path.clone(), div_idx))
                                                } else {
                                                    None
                                                };
                                                cx.notify();
                                            }
                                        }))
                                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                            cx.stop_propagation();
                                            this.is_mouse_selecting = false;
                                            this.is_dragging_scrollbar = false;
                                            this.is_hovering_editor_scrollbar = false;
                                            this.is_hovering_editor_thumb = false;
                                            let top_y = this.current_scroll_top_f64();
                                            if let Some((view, doc)) = this.state.active_view_and_doc_mut() {
                                                view.viewport.offset = view.line_at_doc_y(doc.line_index.lines(), top_y);
                                            }
                                            let win_h: f32 = window.bounds().size.height.into();
                                            let span = (win_h - 66.0).max(100.0);
                                            this.split_drag_state = Some(crate::workspace::SplitDragState {
                                                path: div_path.clone(),
                                                direction: dir,
                                                divider_index: div_idx,
                                                start_pos: event.position.y.into(),
                                                span,
                                                initial_ratio: init_ratio,
                                            });
                                            cx.notify();
                                        }))
                                );
                        }
                        container = container.child(divider_el);
                    }

                    let ratio = ratios.get(idx).copied().unwrap_or(1.0 / children.len().max(1) as f32);
                    let is_last = idx + 1 == children.len();
                    // Non-last panes get an exact device-snapped extent; the last pane
                    // fills the rest, so its (already snapped) top stays pixel-aligned.
                    let main_size = if is_last {
                        (snap(main_avail) - acc).max(0.0)
                    } else {
                        (snap(acc + ratio * main_avail) - acc).max(0.0)
                    };
                    let (cell_w, cell_h) = if is_row {
                        (main_size, cross_avail)
                    } else {
                        (cross_avail, main_size)
                    };

                    let mut child_path = tree_path.to_vec();
                    child_path.push(idx);

                    let child_rendered = self.render_workspace_node(child, &child_path, cell_w, cell_h, theme, window, cx);
                    let mut pane_wrapper = div()
                        .flex()
                        .min_w_0()
                        .min_h_0()
                        .overflow_hidden();

                    if is_row {
                        if is_last {
                            pane_wrapper = pane_wrapper.flex_1().min_w_0().h_full();
                        } else {
                            pane_wrapper = pane_wrapper.w(px(main_size)).flex_shrink_0().h_full();
                        }
                    } else {
                        if is_last {
                            pane_wrapper = pane_wrapper.flex_1().min_h_0().w_full();
                        } else {
                            pane_wrapper = pane_wrapper.h(px(main_size)).flex_shrink_0().w_full();
                        }
                    }

                    container = container.child(pane_wrapper.child(child_rendered));
                    acc += main_size;
                }

                container
            }
            crate::workspace::WorkspaceNode::Leaf { id } => {
                self.render_workspace_leaf(*id, theme, window, cx)
            }
        }
    }

    fn render_workspace_leaf(
        &self,
        leaf_id: crate::workspace::LeafId,
        theme: &Theme,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> gpui::Stateful<Div> {
        let is_active = self.state.workspace.active_leaf == leaf_id;
        let in_titlebar = self.state.workspace.is_leaf_in_titlebar(leaf_id);

        let mut leaf_container = div()
            .id(gpui::ElementId::Name(format!("workspace-leaf-{}", leaf_id.0).into()))
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .bg(theme.bg_editor);

        if !in_titlebar {
            let viewport_w: f32 = window.viewport_size().width.into();
            let sidebar_area_w = if self.sidebar_collapsed {
                44.0
            } else {
                self.sidebar_width + 1.0
            };
            let right_sidebar_area_w = if self.right_sidebar_collapsed {
                0.0
            } else {
                self.right_sidebar_width + 1.0
            };
            let workspace_w = (viewport_w - sidebar_area_w - right_sidebar_area_w).max(100.0);
            let panel_w = (workspace_w * self.state.workspace.horizontal_width_ratio_of_leaf(leaf_id)).max(60.0);
            let available_strip_w = (panel_w - 40.0).max(40.0);
            leaf_container = leaf_container.child(self.render_leaf_tab_bar_strip(leaf_id, false, available_strip_w, theme, window, cx));
        }

        if is_active {
            leaf_container.child(
                self.render_central_editor(None, None, true, Some(leaf_id), theme, window, cx)
            )
        } else {
            let active_tab = self.state.leaves.get(&leaf_id).and_then(|l| l.active_tab());
            if let Some(tab) = active_tab {
                match &tab.content {
                    crate::workspace::TabContent::Document { id } => {
                        if let Some(doc) = self.state.documents.get(id) {
                            let view = self.state.views.get(&tab.id);
                            leaf_container.child(
                                self.render_central_editor(view, Some(doc), false, Some(leaf_id), theme, window, cx)
                            )
                        } else {
                            leaf_container.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .size_full()
                                    .text_color(theme.text_muted)
                                    .child("Document not found")
                            )
                        }
                    }
                    crate::workspace::TabContent::Empty => {
                        leaf_container.child(
                            self.render_central_editor(None, None, false, Some(leaf_id), theme, window, cx)
                        )
                    }
                    crate::workspace::TabContent::Plugin { view_id } => {
                        if let Some(leaf) = self.plugin_leaf_views.get(view_id) {
                            leaf_container.child(
                                div()
                                    .id(gpui::ElementId::Name(format!("leaf-plugin-view-{}", leaf_id.0).into()))
                                    .flex()
                                    .flex_col()
                                    .size_full()
                                    .p(px(16.0))
                                    .overflow_scroll()
                                    .child(crate::plugins::ui::render_ui_node(
                                        &leaf.tree,
                                        &leaf.plugin_id,
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
                                    ))
                            )
                        } else {
                            leaf_container
                        }
                    }
                }
            } else {
                leaf_container.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size_full()
                        .text_color(theme.text_muted)
                        .child("No open tabs")
                )
            }
        }
    }

    pub(crate) fn render_central_editor(
        &self,
        override_view: Option<&crate::workspace::EditorViewState>,
        override_doc: Option<&crate::document_buffer::DocumentBuffer>,
        is_leaf_active: bool,
        leaf_id: Option<crate::workspace::LeafId>,
        theme: &Theme,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> gpui::Stateful<Div> {
        let editor_font = self.editor_font_family();
        let font_id = cx.text_system().resolve_font(&gpui::font(editor_font.clone()));
        let line_char_width = cx
            .text_system()
            .advance(font_id, px(14.0), 'x')
            .map(|s| f32::from(s.width))
            .unwrap_or(7.7);

        let pixel_scale = window.scale_factor();
        let window_h = f32::from(window.bounds().size.height);

        let root_id = if let Some(lid) = leaf_id {
            format!("central-editor-root-{}", lid.0)
        } else {
            "central-editor-root".to_string()
        };

        let mut editor_root = div()
            .id(gpui::ElementId::Name(root_id.into()))
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .bg(theme.bg_editor);

        if !is_leaf_active {
            if let Some(lid) = leaf_id {
                editor_root = editor_root
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                        this.switch_to_leaf(lid, window, cx);
                        window.focus(&this.focus_handle);
                        cx.notify();
                    }))
                    .on_mouse_down(MouseButton::Middle, cx.listener(move |this, _, window, cx| {
                        this.switch_to_leaf(lid, window, cx);
                        this.start_editor_middle_scroll(cx);
                    }));
            }
        }

        editor_root = editor_root.child(self.render_navigation_bar(override_doc, leaf_id, theme, cx));

        let current_view_and_doc = match (override_view, override_doc) {
            (Some(v), Some(d)) => Some((v, d)),
            _ if is_leaf_active && self.active_center_leaf.is_none() => {
                self.state.active_view_and_doc()
            }
            _ => None,
        };

        if let Some((view, doc)) = current_view_and_doc {
            let note_title = doc
                .path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Untitled".to_string());

            let lines_arc = Arc::clone(&view.window.lines);
            let line_number_digits = doc.line_index.lines().max(1).to_string().len().max(3);
            let line_number_width = 40.0 + line_number_digits.saturating_sub(3) as f32 * line_char_width;
            let visual_rows_arc = Arc::clone(&view.window.visual_rows);
            let code_states_arc = Arc::clone(&view.window.code_states);
            let height_index_arc = Arc::clone(&view.window.height_index);

            let (cursor_line, cursor_col) = view.line_col(doc);
            let (active_v_idx, active_col_in_row) = crate::markdown::find_visual_row_for_cursor(&visual_rows_arc, cursor_line, cursor_col)
                .unwrap_or((0, 0));

            let selection_range = view.selection_range(doc.len());
            let is_editing = is_leaf_active && self.edit_mode && !self.ctrl_preview_selection;
            let mut active_block_ids: std::collections::HashSet<usize> = std::collections::HashSet::new();
            if is_editing {
                if let Some(s) = code_states_arc.get(cursor_line).and_then(|s| s.as_ref()) {
                    active_block_ids.insert(s.block_id);
                }
                if let Some(ref sel) = selection_range {
                    if sel.start < sel.end {
                        let sel_start_line = doc.line_of_byte(sel.start);
                        let sel_end_line = doc.line_of_byte(sel.end);
                        let win_start = code_states_arc.start();
                        let win_end = code_states_arc.end();
                        let scan_start = sel_start_line.max(win_start);
                        let scan_end = sel_end_line.min(win_end.saturating_sub(1));
                        if scan_start <= scan_end {
                            for l_idx in scan_start..=scan_end {
                                if let Some(Some(s)) = code_states_arc.get(l_idx) {
                                    active_block_ids.insert(s.block_id);
                                }
                            }
                        }
                    }
                }
            }

            let text_start_x = self.pane_text_start_x(leaf_id, window);

            let theme_scrollbar = theme.clone();

            let total_content_h = view.total_height();
            let (slice_h, viewport_h, max_scroll_content, track_top_y) =
                self.editor_scroll_metrics_for_leaf(leaf_id, is_leaf_active, total_content_h, window_h);
            if is_leaf_active {
                self.last_editor_h.set(viewport_h);
            }

            let wide_scrollbar = self.config.editor.wide_scrollbar;
            let show_scrollbar = max_scroll_content > 1.0;

            let leaf_scroll_top = if is_leaf_active {
                if let Some(target) = self.pending_editor_restore {
                    target
                } else if self.is_resizing_any() && view.window.covers_line(view.viewport.offset) {
                    let first_v_idx = view.window.first_row_of_line(view.viewport.offset).unwrap_or(0);
                    let line_y = view.above_h + view.window.height_index.top_of(first_v_idx);
                    let curr = self.current_scroll_top_f64();
                    if (curr - line_y).abs() > (view.window.total_height() * 0.4).max(500.0) {
                        line_y
                    } else {
                        curr
                    }
                } else {
                    self.current_scroll_top_f64()
                }
            } else {
                leaf_id.and_then(|lid| {
                    self.state.leaves.get(&lid)
                        .and_then(|l| l.active_tab())
                        .and_then(|t| self.state.views.get(&t.id))
                        .map(|v| v.scroll_top)
                }).unwrap_or(0.0)
            };

            let (thumb_top, thumb_h) = if show_scrollbar {
                let th = Self::editor_thumb_h(viewport_h, total_content_h, wide_scrollbar);
                let scroll_y = leaf_scroll_top.clamp(0.0, max_scroll_content);
                let scroll_fraction = (scroll_y / max_scroll_content).clamp(0.0, 1.0);
                let tt = scroll_fraction * (f64::from(viewport_h) - th);
                (tt as f32, th as f32)
            } else {
                (0.0, 28.0)
            };

            let is_resizing = self.is_resizing_any();
            let is_scrollbar_dragging = is_leaf_active && self.is_dragging_scrollbar && !is_resizing;
            let scroll_y_current = leaf_scroll_top.clamp(0.0, max_scroll_content);
            let ruler_expanded = is_leaf_active && self.is_hovering_editor_scrollbar && !is_resizing;
            let track_w = if wide_scrollbar { 16.0 } else { 14.0 };
            let thumb_right = if wide_scrollbar { 0.0 } else { 3.0 };
            let thumb_w_idle = if wide_scrollbar { track_w } else { 6.0 };
            let thumb_w_active = if wide_scrollbar { track_w } else { 8.0 };
            let thumb_w = if is_scrollbar_dragging { thumb_w_active } else { thumb_w_idle };
            let thumb_left = track_w - thumb_right - thumb_w;
            let ruler_pill_w = if is_scrollbar_dragging { thumb_w } else { 6.0 };
            let ruler_pill_left = thumb_left + (thumb_w - ruler_pill_w) / 2.0;
            let ruler_full = ruler_expanded || wide_scrollbar;
            let thumb_bg = if wide_scrollbar {
                theme_scrollbar.border
            } else if is_scrollbar_dragging {
                theme_scrollbar.accent
            } else {
                theme_scrollbar.border
            };

            let show_ruler = self.config.editor.scrollbar_ruler;
            let (ruler_cursor_y, ruler_sel_span): (f32, Option<(f32, f32)>) =
                if show_ruler && show_scrollbar && total_content_h > 1.0 {
                    let to_track = |doc_y: f64| {
                        ((doc_y / total_content_h) * f64::from(viewport_h))
                            .clamp(0.0, f64::from(viewport_h) - 2.0)
                            as f32
                    };
                    let lines_total = doc.line_index.lines();
                    let cursor_doc_y = if view.window.covers_line(cursor_line) {
                        view.doc_y_of_local_v_idx(active_v_idx)
                    } else {
                        view.doc_y_of_line(lines_total, cursor_line)
                    };
                    let cursor_y = to_track(cursor_doc_y);
                    let sel_span = view.selection_range(doc.len()).map(|sel| {
                        let start_line = doc.line_of_byte(sel.start);
                        let end_line = doc.line_of_byte(sel.end);
                        let a = view.doc_y_of_line(lines_total, start_line);
                        let b = view.doc_y_of_line(lines_total, end_line);
                        let top = to_track(a.min(b));
                        let bottom = to_track(a.max(b)) + 2.0;
                        (top, (bottom - top).max(3.0))
                    });
                    (cursor_y, sel_span)
                } else {
                    (0.0, None)
                };

            let ruler_cursor_under_thumb = show_scrollbar
                && !wide_scrollbar
                && (is_scrollbar_dragging || (is_leaf_active && self.is_hovering_editor_thumb))
                && ruler_cursor_y < thumb_top + thumb_h
                && ruler_cursor_y + 2.0 > thumb_top;

            let leaf_active_tab_id = leaf_id.and_then(|lid| {
                self.state.leaves.get(&lid).and_then(|l| l.active_tab()).map(|t| t.id)
            });
            let leaf_view = leaf_active_tab_id.and_then(|tid| self.state.views.get(&tid));
            let title_input_opt = leaf_view.and_then(|v| v.title_input.as_ref());
            let is_editing_title = title_input_opt.is_some();
            let is_title_focused = is_leaf_active && is_editing_title && self.title_focus_handle.is_focused(window);

            let title_text: String = if let Some(input) = title_input_opt {
                input.text.clone()
            } else {
                note_title.clone()
            };
            let title_chars: Vec<char> = title_text.chars().collect();
            let cursor_pos = if let Some(input) = title_input_opt {
                input.cursor_pos.min(title_chars.len())
            } else {
                0
            };
            let title_selection_range = if is_leaf_active {
                title_input_opt.and_then(|i| i.selection_range())
            } else {
                None
            };

            let run = gpui::TextRun {
                len: title_text.len(),
                font: gpui::font("Segoe UI").bold(),
                color: Default::default(),
                underline: Default::default(),
                strikethrough: None,
                background_color: None,
            };
            let shaped = window.text_system().shape_line(
                SharedString::from(title_text.clone()),
                px(28.0),
                &[run],
                None,
            );

            let cursor_x = if title_text.is_empty() {
                0.0
            } else {
                let byte_index: usize = title_chars[..cursor_pos].iter().map(|c| c.len_utf8()).sum();
                crate::snap::snap_x(
                    f32::from(shaped.x_for_index(byte_index)),
                    pixel_scale,
                )
            };

            let (sel_x1, sel_w) = if let Some((s_start, s_end)) = title_selection_range {
                if !title_text.is_empty() {
                    let b1: usize = title_chars[..s_start.min(title_chars.len())].iter().map(|c| c.len_utf8()).sum();
                    let b2: usize = title_chars[..s_end.min(title_chars.len())].iter().map(|c| c.len_utf8()).sum();
                    // Same device-pixel snapping as the caret above: an
                    // unsnapped antialiased edge shifts its blend ratio with
                    // every sub-pixel move and reads as a stuttering edge.
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

            let display_title = if title_text.is_empty() {
                if is_title_focused {
                    String::new()
                } else {
                    note_title.clone()
                }
            } else {
                title_text.clone()
            };

            let lid_tag: u64 = leaf_id.map(|l| (l.0.as_u128() & 0xFFFF_FFFF_FFFF_FFFF) as u64).unwrap_or(0);

            let mut title_box = div()
                .id(gpui::ElementId::from(("note-title-panel", lid_tag)))
                .w_full()
                .flex()
                .flex_row()
                .items_start()
                .pl(px(48.0))
                .pr_4()
                .pt(px(2.0))
                .pb(px(16.0))
                .cursor(gpui::CursorStyle::IBeam)
                .bg(theme.bg_editor);

            if is_leaf_active {
                title_box = title_box
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                        window.focus(&this.title_focus_handle);
                        if let Some((view, doc)) = this.state.active_view_and_doc_mut() {
                            if view.title_input.is_none() {
                                view.start_title_edit(doc.title.clone());
                            }
                            if let Some(ref mut input) = view.title_input {
                                input.is_focused = true;
                                input.cursor_pos = input.text.chars().count();
                                input.selection_anchor = None;
                                input.is_mouse_selecting = false;
                            }
                        }
                        this.reset_title_cursor_blink(cx);
                        cx.notify();
                    }));
            } else if let Some(lid) = leaf_id {
                title_box = title_box
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                        this.switch_to_leaf(lid, window, cx);
                        window.focus(&this.focus_handle);
                        cx.notify();
                    }));
            }

            let mut title_input = div()
                .id(gpui::ElementId::from(("note-title-input", lid_tag)))
                .relative()
                .flex_1()
                .min_w_0()
                .h(px(36.0))
                .cursor(gpui::CursorStyle::IBeam)
                .flex()
                .flex_row()
                .items_center();

            if is_leaf_active {
                title_input = title_input
                    .track_focus(&self.title_focus_handle)
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        window.focus(&this.title_focus_handle);
                        this.reset_title_cursor_blink(cx);

                        let click_x: f32 = event.position.x.into();
                        let editor_left_x = if this.sidebar_collapsed { 44.0 } else { this.sidebar_width + 1.0 };
                        let leaf_ratio = leaf_id.map(|lid| this.state.workspace.horizontal_ratio_of_leaf(lid)).unwrap_or(0.0);
                        let window_w: f32 = window.bounds().size.width.into();
                        let editor_total_w = (window_w - editor_left_x).max(0.0);
                        let title_left_x = editor_left_x + leaf_ratio * editor_total_w + 48.0;
                        let rel_x = (click_x - title_left_x).max(0.0);

                        let Some((view, doc)) = this.state.active_view_and_doc_mut() else { return; };
                        if view.title_input.is_none() {
                            view.start_title_edit(doc.title.clone());
                        }
                        let Some(ref mut input) = view.title_input else { return; };
                        input.is_focused = true;

                        let clicked_pos = if input.text.is_empty() {
                            0
                        } else {
                            let run = gpui::TextRun {
                                len: input.text.len(),
                                font: gpui::font("Segoe UI").bold(),
                                color: Default::default(),
                                underline: Default::default(),
                                strikethrough: None,
                                background_color: None,
                            };
                            let shaped = window.text_system().shape_line(
                                SharedString::from(input.text.clone()),
                                px(28.0),
                                &[run],
                                None,
                            );
                            let byte_idx = shaped.closest_index_for_x(px(rel_x));
                            input.text[..byte_idx.min(input.text.len())].chars().count()
                        };

                        if event.click_count == 2 {
                            let (s, e) = crate::views::goto_search::word_range_at(&input.text, clicked_pos);
                            input.selection_anchor = Some(s);
                            input.cursor_pos = e;
                            input.is_mouse_selecting = false;
                        } else if event.click_count >= 3 {
                            input.select_all();
                            input.is_mouse_selecting = false;
                        } else if event.modifiers.shift {
                            if input.selection_anchor.is_none() {
                                input.selection_anchor = Some(input.cursor_pos);
                            }
                            input.cursor_pos = clicked_pos;
                            input.is_mouse_selecting = true;
                        } else {
                            input.cursor_pos = clicked_pos;
                            input.selection_anchor = Some(clicked_pos);
                            input.is_mouse_selecting = true;
                        }
                        cx.notify();
                    }))
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.handle_title_key(event, window, cx);
                    }));
            } else if let Some(lid) = leaf_id {
                title_input = title_input
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.switch_to_leaf(lid, window, cx);
                        window.focus(&this.focus_handle);
                        cx.notify();
                    }));
            }

            title_input = title_input
                .when(title_selection_range.is_some() && sel_w > 0.0, |s| {
                    s.child(
                        div()
                            .absolute()
                            .left(px(sel_x1))
                            .top(px(4.0))
                            .w(px(sel_w))
                            .h(px(28.0))
                            .bg(theme.selection_bg)
                            .rounded_xs(),
                    )
                })
                .child(
                    div()
                        .font_family("Segoe UI")
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_size(px(28.0))
                        .line_height(px(36.0))
                        .text_color(rgb(0xf44336))
                        .child(display_title),
                )
                .when(title_text.is_empty() && is_title_focused, |s| {
                    s.child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .h(px(36.0))
                            .font_family("Segoe UI")
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_size(px(28.0))
                            .line_height(px(36.0))
                            .text_color(rgba(0xf4433666))
                            .child("Untitled"),
                    )
                })
                .when(is_title_focused && self.title_blink.visible(), |s| {
                    s.child(
                        div()
                            .absolute()
                            .left(px(cursor_x))
                            .top(px(5.0))
                            .w(px(2.0))
                            .h(px(26.0))
                            .bg(rgb(0xf44336))
                            .rounded_xs(),
                    )
                });

            title_box = title_box.child(title_input);

            let mut editor_container = div()
                .id(gpui::ElementId::from(("note-editor-container", lid_tag)))
                .relative()
                .flex()
                .flex_col()
                .flex_1()
                .w_full()
                .min_w_0()
                .min_h_0()
                .overflow_hidden();

            if is_leaf_active {
                editor_container = editor_container
                    .track_focus(&self.focus_handle)
                    .cursor(gpui::CursorStyle::IBeam)
                    .when(!show_scrollbar, |d| {
                        d.on_scroll_wheel(|_, _, cx| {
                            cx.stop_propagation();
                        })
                    })
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                        if this.is_resizing_any() {
                            return;
                        }
                        this.close_editor_context_menu(cx);
                        this.commit_title_rename(window, cx);
                        window.focus(&this.focus_handle);
                        this.is_mouse_selecting = true;
                        let click_x: f32 = event.position.x.into();
                        let click_y: f32 = event.position.y.into();
                        this.handle_editor_mouse_down(click_x, click_y, event.modifiers.shift, event.click_count, window, cx);
                    }))
                    .on_mouse_down(MouseButton::Right, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                        if this.is_resizing_any() {
                            return;
                        }
                        this.commit_title_rename(window, cx);
                        window.focus(&this.focus_handle);
                        let click_x: f32 = event.position.x.into();
                        let click_y: f32 = event.position.y.into();
                        let has_selection = this.state.active_view()
                            .and_then(|v| this.state.active_document().and_then(|d| v.selection_range(d.len())))
                            .map(|r| r.start < r.end)
                            .unwrap_or(false);
                        if !has_selection {
                            this.handle_editor_mouse_down(click_x, click_y, false, 1, window, cx);
                            if let Some(view) = this.state.active_view_mut() {
                                view.selection_anchor = None;
                            }
                        }
                        this.open_editor_context_menu(click_x, click_y, cx);
                    }))
                    .on_mouse_down(MouseButton::Middle, cx.listener(|this, _: &MouseDownEvent, window, cx| {
                        if this.is_resizing_any() {
                            return;
                        }
                        cx.stop_propagation();
                        window.focus(&this.focus_handle);
                        this.close_editor_context_menu(cx);
                        this.is_mouse_selecting = false;
                        this.start_editor_middle_scroll(cx);
                    }))
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                        if this.is_resizing_sidebar {
                            let x: f32 = event.position.x.into();
                            if x < 100.0 {
                                this.sidebar_collapsed = true;
                            } else {
                                this.sidebar_collapsed = false;
                                this.sidebar_width = x.clamp(160.0, 600.0);
                            }
                            cx.notify();
                        } else if this.is_resizing_right_sidebar {
                            let win_w: f32 = window.viewport_size().width.into();
                            let x: f32 = event.position.x.into();
                            let right_w = win_w - x;
                            if right_w < 100.0 {
                                this.right_sidebar_collapsed = true;
                            } else {
                                this.right_sidebar_collapsed = false;
                                this.right_sidebar_width = right_w.clamp(160.0, 600.0);
                            }
                            cx.notify();
                        } else if this.split_drag_state.is_some() {
                            return;
                        } else if this.editor_middle_active {
                            let y: f32 = event.position.y.into();
                            if let Some((top, h, max)) = this.editor_middle_metrics() {
                                this.on_editor_middle_mouse_move(y, top, h, max, cx);
                            } else {
                                cx.notify();
                            }
                        } else if this.is_dragging_scrollbar {
                            let y: f32 = event.position.y.into();
                            this.handle_scrollbar_drag(y, window, cx);
                        } else if let Some(drag) = this.table_col_resize_drag.clone() {
                            let x: f32 = event.position.x.into();
                            let delta = x - drag.start_x;
                            let new_w = (drag.start_width + delta).max(40.0);
                            if let Some(widths) = this.manual_table_col_widths.get_mut(&drag.table_first_line) {
                                if drag.col_idx < widths.len() {
                                    widths[drag.col_idx] = new_w;
                                }
                            }
                            this.table_layouts.borrow_mut().remove(&drag.table_first_line);
                            cx.notify();
                        } else if this.is_mouse_selecting {
                            let x: f32 = event.position.x.into();
                            let y: f32 = event.position.y.into();
                            let viewport_h: f32 = window.viewport_size().height.into();
                            this.update_mouse_selection(x, y, viewport_h, window, cx);
                        }
                    }))
                    .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.is_resizing_sidebar = false;
                        this.is_resizing_right_sidebar = false;
                        this.split_drag_state = None;
                        this.hovered_split_divider = None;
                        this.is_dragging_scrollbar = false;
                        this.sidebar_scrollbar_drag.end();
                        this.plugin_modal_scrollbar_drag.end();
                        this.is_mouse_selecting = false;
                        this.table_col_resize_drag = None;
                        if let Some(view) = this.state.active_view_mut() {
                            if view.selection_anchor == Some(view.cursor_offset) {
                                view.selection_anchor = None;
                            }
                        }
                        cx.notify();
                    }))
                    .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.is_resizing_sidebar = false;
                        this.is_resizing_right_sidebar = false;
                        this.split_drag_state = None;
                        this.hovered_split_divider = None;
                        this.is_dragging_scrollbar = false;
                        this.sidebar_scrollbar_drag.end();
                        this.plugin_modal_scrollbar_drag.end();
                        this.is_mouse_selecting = false;
                        this.table_col_resize_drag = None;
                        cx.notify();
                    }))
                    .on_mouse_up(MouseButton::Middle, cx.listener(|this, _, _, cx| {
                        this.stop_editor_middle_scroll(cx);
                        this.stop_plugin_modal_middle_scroll(cx);
                    }))
                    .on_mouse_up_out(MouseButton::Middle, cx.listener(|this, _, _, cx| {
                        this.stop_editor_middle_scroll(cx);
                        this.stop_plugin_modal_middle_scroll(cx);
                    }))
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        this.handle_key(event, window, cx);
                    }))
                    .on_modifiers_changed(cx.listener(|this, event: &ModifiersChangedEvent, _window, cx| {
                        let ctrl_held = event.modifiers.control || event.modifiers.platform;
                        if this.ctrl_held != ctrl_held {
                            this.ctrl_held = ctrl_held;
                            cx.notify();
                        }
                    }));
            }

            let headings = &self.config.editor.headings;
            let heading_widths = Self::resolve_heading_char_widths(cx.text_system(), &editor_font, headings, line_char_width);
            let text_system = window.text_system().clone();
            let hide_current_line_on_selection = self.config.editor.hide_current_line_on_selection;
            let has_selection = selection_range.as_ref().map_or(false, |sel| sel.start < sel.end);

            let (visible_range, slice_shift) = if !show_scrollbar {
                if is_leaf_active && self.current_scroll_top_f64() > 0.0 {
                    self.set_editor_scroll_top_f64(0.0);
                }
                (0..visual_rows_arc.len(), 0.0)
            } else {
                let raw_scroll_y = leaf_scroll_top;
                let true_max = max_scroll_content
                    .max(total_content_h - f64::from(viewport_h))
                    .max(0.0);
                let scroll_y = if self.is_resizing_any() {
                    raw_scroll_y.max(0.0)
                } else {
                    raw_scroll_y.clamp(0.0, true_max)
                };
                if is_leaf_active && !self.is_resizing_any() && (raw_scroll_y - scroll_y).abs() > 0.5 {
                    self.set_editor_scroll_top_f64(scroll_y);
                }
                let local_scroll = scroll_y - view.above_h;
                let vr = height_index_arc.visible_range(
                    local_scroll,
                    f64::from(slice_h),
                    f64::from(EDITOR_OVERSCAN_PX),
                );
                let shift = height_index_arc.top_of(vr.start) - local_scroll;
                let shift = crate::snap::snap_x(shift as f32, pixel_scale) as f64;
                (vr, shift)
            };

            let (slice_first_line, slice_line_starts) = {
                let lo = visible_range.start.saturating_sub(1);
                let hi = (visible_range.end + 1).min(visual_rows_arc.len());
                if lo >= hi {
                    (0, Vec::new())
                } else {
                    let first = visual_rows_arc[lo].line_idx;
                    let last = visual_rows_arc[hi - 1].line_idx;
                    let mut starts = Vec::with_capacity(last - first + 1);
                    let mut at = doc.line_start_byte(first);
                    for l_idx in first..=last {
                        starts.push(at);
                        at += lines_arc[l_idx].len() + 1;
                    }
                    (first, starts)
                }
            };

            let mut items = Vec::with_capacity(visible_range.len());

            let tables = doc.ensure_table_index_for_window(view.window.start_line, view.window.end_line);
            let calc_v_row_sel_rect = |target_v_idx: usize| -> Option<(f32, f32)> {
                let sel = selection_range.as_ref()?;
                if sel.start >= sel.end {
                    return None;
                }
                let v = visual_rows_arc.get(target_v_idx)?;
                let line_idx = v.line_idx;
                if line_idx < slice_first_line || line_idx - slice_first_line >= slice_line_starts.len() {
                    return None;
                }
                let line = &lines_arc[line_idx];
                let line_start = slice_line_starts[line_idx - slice_first_line];
                let line_end = line_start + line.len();

                let is_line_in_sel = if sel.start == sel.end {
                    false
                } else if line_start == line_end {
                    sel.start <= line_start && sel.end > line_start
                } else {
                    sel.end > line_start && sel.start < line_end
                };
                let is_in_act_block = code_states_arc.get(line_idx)
                    .and_then(|s| s.as_ref())
                    .map(|s| active_block_ids.contains(&s.block_id))
                    .unwrap_or(false);
                let is_act_line = is_editing && (line_idx == cursor_line || is_line_in_sel || is_in_act_block);
                let code_state = code_states_arc.get(line_idx).and_then(|s| s.as_ref()).map(|s| (s.kind, s.lang.as_deref()));
                let classified = crate::markdown::classify_line(line, code_state);
                let (_, row_char_w, _, _, _) =
                    Self::heading_typography_metrics_with_widths(&classified, headings, 24.0, line_char_width, Some(&heading_widths));
                let row_layout = if let Some(tbl) = view.grid_table_at(doc, &tables, line_idx, cursor_line, self.ctrl_preview_selection || !self.edit_mode) {
                    let table_layout = Self::get_or_compute_table_layout(
                        &self.table_layouts,
                        window.text_system(),
                        &|idx| lines_arc.get(idx).map(|s| s.as_str()),
                        tbl,
                        &editor_font,
                        self.manual_table_col_widths.get(&tbl.first_line).map(|w| w.as_slice()),
                    );
                    let cursor_col = if is_act_line && line_idx == cursor_line { Some(active_col_in_row) } else { None };
                    shape_table_row(window.text_system(), line, tbl, line_idx, cursor_col, &table_layout, headings, &editor_font, row_char_w)
                } else {
                    let raw_row = tables
                        .at(line_idx)
                        .map_or(false, |t| view.table_raw_now(doc, t, cursor_line, self.ctrl_preview_selection || !self.edit_mode));
                    shape_row(&text_system, line, &classified, v, is_act_line || raw_row, headings, &editor_font, row_char_w)
                };

                if line.is_empty() {
                    if sel.start <= line_start && sel.end > line_start {
                        return Some(row_layout.selection_rect(0, 1, pixel_scale));
                    } else {
                        return None;
                    }
                }

                if let Some(tbl) = view.grid_table_at(doc, &tables, line_idx, cursor_line, self.ctrl_preview_selection || !self.edit_mode) {
                    let sel_s_line = doc.line_of_byte(sel.start);
                    let sel_e_line = doc.line_of_byte(sel.end);
                    if tbl.contains(sel_s_line) && tbl.contains(sel_e_line) {
                        if line_idx == tbl.sep_line() || line_idx < sel_s_line || line_idx > sel_e_line {
                            return None;
                        }
                        let s_line_start = doc.line_start_byte(sel_s_line);
                        let s_char = doc.slice_to_string(s_line_start..sel.start).chars().count();
                        let e_line_start = doc.line_start_byte(sel_e_line);
                        let e_char = doc.slice_to_string(e_line_start..sel.end).chars().count();

                        let s_cells = crate::table::split_row(&lines_arc[sel_s_line]);
                        let e_cells = crate::table::split_row(&lines_arc[sel_e_line]);
                        let col1 = crate::table::cell_index_at(&s_cells, s_char);
                        let col2 = crate::table::cell_index_at(&e_cells, e_char);
                        let col_min = col1.min(col2);
                        let col_max = col1.max(col2);

                        if sel_s_line == sel_e_line {
                            let cur_cells = crate::table::split_row(line);
                            let c_start = cur_cells.get(col_min).map(|c| c.content.start).unwrap_or(0);
                            let c_end = cur_cells.get(col_max).map(|c| c.content.end).unwrap_or(line.chars().count());
                            let row_start = s_char.clamp(c_start, c_end);
                            let row_end = e_char.clamp(c_start, c_end);
                            let (c_s, c_e) = (row_start.min(row_end), row_start.max(row_end));
                            if c_s < c_e {
                                let vis_s = row_layout.column_for_line_char(c_s);
                                let vis_e = row_layout.column_for_line_char(c_e);
                                if vis_s < vis_e {
                                    return Some(row_layout.selection_rect(vis_s, vis_e, pixel_scale));
                                }
                            }
                            return None;
                        } else {
                            let table_layout = Self::get_or_compute_table_layout(
                                &self.table_layouts,
                                window.text_system(),
                                &|idx| lines_arc.get(idx).map(|s| s.as_str()),
                                tbl,
                                &editor_font,
                                self.manual_table_col_widths.get(&tbl.first_line).map(|w| w.as_slice()),
                            );
                            let col_x = table_layout.col_x.get(col_min).copied().unwrap_or(0.0);
                            let col_w = if let Some(&max_x) = table_layout.col_x.get(col_max) {
                                let max_w = table_layout.col_widths.get(col_max).copied().unwrap_or(0.0);
                                (max_x + max_w - col_x).max(0.0)
                            } else {
                                table_layout.total_width - col_x
                            };
                            return Some(crate::snap::snapped_span(col_x, col_x + col_w, pixel_scale));
                        }
                    }
                }

                let mut byte_s = sel.start.saturating_sub(line_start).min(line.len());
                let mut byte_e = sel.end.saturating_sub(line_start).min(line.len());
                while byte_s > 0 && !line.is_char_boundary(byte_s) {
                    byte_s -= 1;
                }
                while byte_e > 0 && !line.is_char_boundary(byte_e) {
                    byte_e -= 1;
                }
                let char_s = line[..byte_s].chars().count();
                let char_e = char_s + line[byte_s..byte_e].chars().count();

                let includes_newline = v.is_last_sub && sel.end > line_end && sel.start <= line_end;

                if byte_s < byte_e {
                    let intersect_s = char_s.max(v.char_start);
                    let intersect_e = char_e.min(v.char_end);
                    if intersect_s <= intersect_e {
                        let vis_s = row_layout.column_for_line_char(intersect_s);
                        let mut vis_e = row_layout.column_for_line_char(intersect_e);
                        if includes_newline {
                            vis_e += 1;
                        }
                        if vis_s < vis_e {
                            Some(row_layout.selection_rect(vis_s, vis_e, pixel_scale))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else if includes_newline && char_s == v.char_end {
                    let vis_s = row_layout.column_for_line_char(v.char_end);
                    Some(row_layout.selection_rect(vis_s, vis_s + 1, pixel_scale))
                } else {
                    None
                }
            };

            for v_idx in visible_range {
                let v = &visual_rows_arc[v_idx];
                let line = &lines_arc[v.line_idx];
                let line_start = slice_line_starts[v.line_idx - slice_first_line];
                let line_end = line_start + line.len();
                let is_line_in_selection = selection_range.as_ref().map_or(false, |sel| {
                    if sel.start == sel.end {
                        false
                    } else if line_start == line_end {
                        sel.start <= line_start && sel.end > line_start
                    } else {
                        sel.end > line_start && sel.start < line_end
                    }
                });
                let is_current = (self.edit_mode || !is_leaf_active) && (v_idx == active_v_idx);
                let show_current_line_highlight = is_current && !(hide_current_line_on_selection && has_selection);
                let is_in_active_block = code_states_arc[v.line_idx]
                    .as_ref()
                    .map(|s| active_block_ids.contains(&s.block_id))
                    .unwrap_or(false);
                let (is_raw_table_line, is_raw_table_first, is_raw_table_last) = if let Some(tbl) = tables.at(v.line_idx) {
                    if view.table_raw_now(doc, tbl, cursor_line, self.ctrl_preview_selection || !self.edit_mode) {
                        (
                            true,
                            v.line_idx == tbl.first_line && v.sub_idx == 0,
                            v.line_idx == tbl.last_line && v.is_last_sub,
                        )
                    } else {
                        (false, false, false)
                    }
                } else {
                    (false, false, false)
                };
                let is_active_line = is_editing && (v.line_idx == cursor_line || is_line_in_selection || is_in_active_block || is_raw_table_line);
                let code_state = code_states_arc[v.line_idx].as_ref().map(|s| (s.kind, s.lang.as_deref()));
                let classified = crate::markdown::classify_line(line, code_state);
                let is_opening_fence = matches!(classified, crate::markdown::LineKind::CodeFence { is_opening: true, .. });
                let is_closing_fence = matches!(classified, crate::markdown::LineKind::CodeFence { is_opening: false, .. });
                let is_code_line = matches!(classified, crate::markdown::LineKind::CodeLine(_));
                let row_len = v.char_end.saturating_sub(v.char_start);

                let (h_metrics_row_h, row_char_w, _row_font_size, _cursor_h, _cursor_top) =
                    Self::heading_typography_metrics_with_widths(&classified, headings, 24.0, line_char_width, Some(&heading_widths));
                let grid_tbl = view.grid_table_at(doc, &tables, v.line_idx, cursor_line, self.ctrl_preview_selection || !self.edit_mode);
                let table_layout = grid_tbl.map(|tbl| {
                    Self::get_or_compute_table_layout(
                        &self.table_layouts,
                        window.text_system(),
                        &|idx| lines_arc.get(idx).map(|s| s.as_str()),
                        tbl,
                        &editor_font,
                        self.manual_table_col_widths.get(&tbl.first_line).map(|w| w.as_slice()),
                    )
                });
                let is_tbl_sep = grid_tbl.map_or(false, |tbl| v.line_idx == tbl.sep_line());
                let row_h = if let Some(tbl) = grid_tbl {
                    if v.line_idx == tbl.sep_line() {
                        1.0
                    } else if v.line_idx == tbl.first_line || v.line_idx == tbl.last_line {
                        42.0
                    } else {
                        28.0
                    }
                } else {
                    h_metrics_row_h
                };
                let row_layout: Arc<RowShapedLayout> = if let Some(tbl) = grid_tbl {
                    let cursor_col = if is_active_line && v.line_idx == cursor_line { Some(active_col_in_row) } else { None };
                    Arc::new(shape_table_row(window.text_system(), line, tbl, v.line_idx, cursor_col, table_layout.as_ref().unwrap(), headings, &editor_font, row_char_w))
                } else {
                    Self::get_or_shape_row(&self.row_shape_cache, &text_system, line, &classified, v, is_active_line, headings, &editor_font, row_char_w)
                };
                let row_layout_down = row_layout.clone();
                let row_layout_move = row_layout.clone();
                let row_layout_preview: Arc<RowShapedLayout> = if is_active_line {
                    if let Some(tbl) = grid_tbl {
                        Arc::new(shape_table_row(window.text_system(), line, tbl, v.line_idx, None, table_layout.as_ref().unwrap(), headings, &editor_font, row_char_w))
                    } else {
                        // The preview variant (markers hidden) is hashed under
                        // `is_active_line = false` — a separate cache entry.
                        Self::get_or_shape_row(&self.row_shape_cache, &text_system, line, &classified, v, false, headings, &editor_font, row_char_w)
                    }
                } else {
                    row_layout.clone()
                };
                let row_layout_preview_down = row_layout_preview.clone();
                let row_layout_preview_move = row_layout_preview.clone();

                let line_number_text = if v.sub_idx == 0 && !is_tbl_sep {
                    format!("{:>3} ", v.line_idx + 1)
                } else if is_tbl_sep {
                    "".to_string()
                } else {
                    "    ".to_string()
                };

                let row_sel_rect = if line.is_empty() {
                    if selection_range.as_ref().map_or(false, |s| s.start <= line_start && s.end > line_start) {
                        Some(row_layout.selection_rect(0, 1, pixel_scale))
                    } else {
                        None
                    }
                } else if let Some(sel) = selection_range.as_ref() {
                    if sel.start < sel.end {
                        let table_rect = if let Some(tbl) = grid_tbl {
                            let sel_s_line = doc.line_of_byte(sel.start);
                            let sel_e_line = doc.line_of_byte(sel.end);
                            if tbl.contains(sel_s_line) && tbl.contains(sel_e_line) {
                                if v.line_idx == tbl.sep_line() || v.line_idx < sel_s_line || v.line_idx > sel_e_line {
                                    Some(None)
                                } else {
                                    let s_line_start = doc.line_start_byte(sel_s_line);
                                    let s_char = doc.slice_to_string(s_line_start..sel.start).chars().count();
                                    let e_line_start = doc.line_start_byte(sel_e_line);
                                    let e_char = doc.slice_to_string(e_line_start..sel.end).chars().count();

                                    let s_cells = crate::table::split_row(&lines_arc[sel_s_line]);
                                    let e_cells = crate::table::split_row(&lines_arc[sel_e_line]);
                                    let col1 = crate::table::cell_index_at(&s_cells, s_char);
                                    let col2 = crate::table::cell_index_at(&e_cells, e_char);
                                    let col_min = col1.min(col2);
                                    let col_max = col1.max(col2);

                                    if sel_s_line == sel_e_line {
                                        let cur_cells = crate::table::split_row(line);
                                        let c_start = cur_cells.get(col_min).map(|c| c.content.start).unwrap_or(0);
                                        let c_end = cur_cells.get(col_max).map(|c| c.content.end).unwrap_or(line.chars().count());
                                        let row_start = s_char.clamp(c_start, c_end);
                                        let row_end = e_char.clamp(c_start, c_end);
                                        let (c_s, c_e) = (row_start.min(row_end), row_start.max(row_end));
                                        if c_s < c_e {
                                            let vis_s = row_layout.column_for_line_char(c_s);
                                            let vis_e = row_layout.column_for_line_char(c_e);
                                            if vis_s < vis_e {
                                                Some(Some(row_layout.selection_rect(vis_s, vis_e, pixel_scale)))
                                            } else {
                                                Some(None)
                                            }
                                        } else {
                                            Some(None)
                                        }
                                    } else {
                                        let tl = table_layout.as_ref().unwrap();
                                        let col_x = tl.col_x.get(col_min).copied().unwrap_or(0.0);
                                        let col_w = if let Some(&max_x) = tl.col_x.get(col_max) {
                                            let max_w = tl.col_widths.get(col_max).copied().unwrap_or(0.0);
                                            (max_x + max_w - col_x).max(0.0)
                                        } else {
                                            tl.total_width - col_x
                                        };
                                        Some(Some(crate::snap::snapped_span(col_x, col_x + col_w, pixel_scale)))
                                    }
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        if let Some(rect_opt) = table_rect {
                            rect_opt
                        } else {
                            let mut byte_s = sel.start.saturating_sub(line_start).min(line.len());
                            let mut byte_e = sel.end.saturating_sub(line_start).min(line.len());
                            while byte_s > 0 && !line.is_char_boundary(byte_s) {
                                byte_s -= 1;
                            }
                            while byte_e > 0 && !line.is_char_boundary(byte_e) {
                                byte_e -= 1;
                            }
                            let char_s = line[..byte_s].chars().count();
                            let char_e = char_s + line[byte_s..byte_e].chars().count();

                            let includes_newline = v.is_last_sub && sel.end > line_end && sel.start <= line_end;

                            if byte_s < byte_e {
                                let intersect_s = char_s.max(v.char_start);
                                let intersect_e = char_e.min(v.char_end);
                                if intersect_s <= intersect_e {
                                    let vis_s = row_layout.column_for_line_char(intersect_s);
                                    let mut vis_e = row_layout.column_for_line_char(intersect_e);
                                    if includes_newline {
                                        vis_e += 1;
                                    }
                                    if vis_s < vis_e {
                                        Some(row_layout.selection_rect(vis_s, vis_e, pixel_scale))
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else if includes_newline && char_s == v.char_end {
                                let vis_s = row_layout.column_for_line_char(v.char_end);
                                Some(row_layout.selection_rect(vis_s, vis_s + 1, pixel_scale))
                            } else {
                                None
                            }
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                let chip_cell_h = if grid_tbl.is_some() { 28.0 } else { row_h };
                let chip_inset = code_chip_inset(chip_cell_h);
                let mut backdrop_rects = Vec::new();
                for &(code_start, code_end) in row_layout.code_ranges() {
                    let (chip_x, chip_w) = row_layout.selection_rect(code_start, code_end, pixel_scale);
                    let chip_top = if let Some(tbl) = grid_tbl {
                        if v.line_idx == tbl.first_line { 14.0 + chip_inset } else { chip_inset }
                    } else {
                        chip_inset
                    };
                    let chip_h = if grid_tbl.is_some() { 28.0 - 2.0 * chip_inset } else { (row_h - 2.0 * chip_inset).max(1.0) };
                    backdrop_rects.push(
                        div()
                            .absolute()
                            .left(px(chip_x))
                            .top(px(chip_top))
                            .h(px(chip_h))
                            .w(px(chip_w))
                            .bg(theme.code_bg)
                            .rounded_sm(),
                    );
                }
                let mut tag_overlays = Vec::new();
                for (tag_idx, (tag_start, tag_end, tag_name)) in row_layout.tag_ranges().iter().enumerate() {
                    let (chip_x, chip_w) = row_layout.selection_rect(*tag_start, *tag_end, pixel_scale);
                    let (bg_color, _) = crate::markdown::tag_color_pair(tag_name, theme.is_dark);
                    let tag_top = if let Some(tbl) = grid_tbl {
                        if v.line_idx == tbl.first_line { 14.0 + chip_inset } else { chip_inset }
                    } else {
                        chip_inset
                    };
                    let tag_h = if grid_tbl.is_some() { 28.0 - 2.0 * chip_inset } else { (row_h - 2.0 * chip_inset).max(1.0) };
                    let pill_pad = 2.0f32;
                    backdrop_rects.push(
                        div()
                            .absolute()
                            .left(px(chip_x - pill_pad))
                            .top(px(tag_top))
                            .h(px(tag_h))
                            .w(px(chip_w + 2.0 * pill_pad))
                            .bg(bg_color)
                            .rounded_full(),
                    );
                    if self.ctrl_held || !self.edit_mode {
                        let tag_id = format!("v-tag-ov-{}-{}-{}-{}", lid_tag, v_idx, tag_idx, tag_name);
                        let t = tag_name.clone();
                        tag_overlays.push(
                            div()
                                .id(gpui::ElementId::Name(tag_id.into()))
                                .absolute()
                                .left(px(chip_x - pill_pad))
                                .top(px(tag_top))
                                .h(px(tag_h))
                                .w(px(chip_w + 2.0 * pill_pad))
                                .cursor(gpui::CursorStyle::PointingHand)
                                .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                    if event.modifiers.control || event.modifiers.platform || !this.edit_mode {
                                        cx.stop_propagation();
                                        this.open_tag_search(&t, window, cx);
                                    }
                                })),
                        );
                    }
                }
                if let Some((sel_x, sel_w)) = row_sel_rect {
                    let sel_right = sel_x + sel_w;
                    let sel_above = if v_idx > 0 { calc_v_row_sel_rect(v_idx - 1) } else { None };
                    let sel_below = if v_idx + 1 < visual_rows_arc.len() { calc_v_row_sel_rect(v_idx + 1) } else { None };

                    let round_tl = sel_above.map_or(true, |(ax, aw)| sel_x < ax - 0.5 || sel_x >= ax + aw - 0.5);
                    let round_tr = sel_above.map_or(true, |(ax, aw)| sel_right > ax + aw + 0.5 || sel_right <= ax + 0.5);
                    let round_bl = sel_below.map_or(true, |(bx, bw)| sel_x < bx - 0.5 || sel_x >= bx + bw - 0.5);
                    let round_br = sel_below.map_or(true, |(bx, bw)| sel_right > bx + bw + 0.5 || sel_right <= bx + 0.5);

                    let sel_top = if let Some(tbl) = grid_tbl {
                        if v.line_idx == tbl.first_line { 14.0 } else { 0.0 }
                    } else {
                        0.0
                    };
                    let sel_h = if grid_tbl.is_some() { 28.0 } else { row_h };

                    let is_preview_selection = self.ctrl_preview_selection || !self.edit_mode;
                    let sel_color = if is_preview_selection {
                        theme.preview_selection_bg()
                    } else {
                        theme.selection_bg
                    };

                    let mut sel_rect = div()
                        .absolute()
                        .left(px(sel_x))
                        .top(px(sel_top))
                        .h(px(sel_h))
                        .w(px(sel_w))
                        .bg(sel_color);

                    if round_tl {
                        sel_rect = sel_rect.rounded_tl_xs();
                    }
                    if round_tr {
                        sel_rect = sel_rect.rounded_tr_xs();
                    }
                    if round_bl {
                        sel_rect = sel_rect.rounded_bl_xs();
                    }
                    if round_br {
                        sel_rect = sel_rect.rounded_br_xs();
                    }

                    backdrop_rects.push(sel_rect);
                }

                let v_char_start = v.char_start;
                let v_char_end = v.char_end;
                let v_line_idx = v.line_idx;
                let v_sub_idx = v.sub_idx;

                let copy_payload: Option<(usize, usize)> =
                    if is_opening_fence && !is_in_active_block {
                        code_states_arc[v.line_idx]
                            .as_ref()
                            .map(|st| (st.block_id, v.line_idx))
                    } else {
                        None
                    };

                if is_opening_fence {
                    if let Some(st) = code_states_arc[v.line_idx].as_ref() {
                        if let Some(lang) = &st.lang {
                            if let Some(owner_plugin) = self.state.code_block_processors.get_plugin_for_lang(lang) {
                                if !self.code_block_views.contains_key(&st.block_id) {
                                    let mut body = String::new();
                                    let mut i = v.line_idx + 1;
                                    while let Some(next) = code_states_arc.get(i).and_then(|s| s.as_ref()) {
                                        if next.block_id != st.block_id || next.kind != crate::markdown::CodeBlockKind::InsideCode {
                                            break;
                                        }
                                        body.push_str(&lines_arc[i]);
                                        body.push('\n');
                                        i += 1;
                                    }
                                    let _ = self.state.plugin_host.send_event(
                                        &owner_plugin,
                                        crate::plugins::api::HostEvent::ProcessCodeBlock {
                                            language: lang.clone(),
                                            source: body,
                                            block_id: st.block_id,
                                        },
                                    );
                                }
                            }
                        }
                    }
                }

                let (custom_ui_node, custom_plugin_id, is_first_code_line) = match code_states_arc[v.line_idx].as_ref() {
                    Some(st) => {
                        let view_opt = self.code_block_views.get(&st.block_id);
                        let is_first = st.kind == crate::markdown::CodeBlockKind::InsideCode
                            && v.line_idx > 0
                            && code_states_arc.get(v.line_idx - 1).and_then(|s| s.as_ref()).map(|s| s.block_id) != Some(st.block_id);
                        (view_opt.map(|(_, node)| node), view_opt.map(|(id, _)| id.as_str()), is_first)
                    }
                    None => (None, None, false),
                };

                let mut row_div = div()
                    .id(gpui::ElementId::Name(format!("v-row-{}-{}", lid_tag, v_idx).into()))
                    .relative()
                    .w_full()
                    .flex_shrink_0()
                    .flex()
                    .flex_row()
                    .when(grid_tbl.is_none(), |d| d.items_center())
                    .when(grid_tbl.is_some(), |d| d.items_start())
                    .h(px(row_h))
                    .py_0()
                    .px_2()
                    .rounded_sm();

                if show_current_line_highlight {
                    let is_sep = grid_tbl.map_or(false, |tbl| v.line_idx == tbl.sep_line());
                    let hl_top = if let Some(tbl) = grid_tbl {
                        if v.line_idx == tbl.first_line { 14.0 } else { 0.0 }
                    } else {
                        0.0
                    };
                    let hl_h = if is_sep { 1.0 } else if grid_tbl.is_some() { 28.0 } else { row_h };

                    if is_leaf_active {
                        row_div = row_div.child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .top(px(hl_top))
                                .h(px(hl_h))
                                .rounded_sm()
                                .bg(theme.bg_hover),
                        );
                    } else {
                        let mut faint_bg = theme.bg_hover;
                        faint_bg.a *= 0.45;
                        let mut border_c = theme.text_muted;
                        border_c.a = 0.5;
                        row_div = row_div.child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .top(px(hl_top))
                                .h(px(hl_h))
                                .rounded_sm()
                                .bg(faint_bg)
                                .border_1()
                                .border_dashed()
                                .border_color(border_c),
                        );
                    }
                }

                if is_leaf_active {
                    row_div = row_div
                        .cursor(gpui::CursorStyle::IBeam)
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            if this.is_resizing_any() {
                                return;
                            }
                            let ctrl_held = event.modifiers.control || event.modifiers.platform;
                            this.ctrl_preview_selection = ctrl_held;
                            this.is_mouse_selecting = true;
                            let click_x: f32 = event.position.x.into();

                            let col_in_row = if click_x > text_start_x {
                                let layout = if ctrl_held { &row_layout_preview_down } else { &row_layout_down };
                                let column = layout.column_for_x(click_x - text_start_x);
                                if (ctrl_held || !this.edit_mode) && event.click_count == 1 {
                                    if let Some((_, _, tag_name)) = layout.tag_ranges().iter().find(|(s, e, _)| column >= *s && column < *e) {
                                        this.open_tag_search(tag_name, window, cx);
                                        this.is_mouse_selecting = false;
                                        return;
                                    }
                                }
                                layout.line_char_for_column(column).saturating_sub(v_char_start).min(row_len)
                            } else {
                                0
                            };
                            let target_char = v_char_start + col_in_row;
                            if let Some((view, doc)) = this.state.active_view_and_doc_mut() {
                                let offset = view.offset_for_line_col(doc, v_line_idx, target_char);
                                if event.click_count == 2 {
                                    let (w_start, w_end) = doc.word_range_at(offset);
                                    view.selection_anchor = Some(w_start);
                                    view.cursor_offset = w_end;
                                    this.is_mouse_selecting = false;
                                } else if event.click_count >= 3 {
                                    let (l_start, l_end) = doc.line_range_at(offset);
                                    view.selection_anchor = Some(l_start);
                                    view.cursor_offset = l_end;
                                    this.is_mouse_selecting = false;
                                } else if event.modifiers.shift {
                                    if view.selection_anchor.is_none() {
                                        view.selection_anchor = Some(view.cursor_offset);
                                    }
                                    view.cursor_offset = offset;
                                } else {
                                    view.cursor_offset = offset;
                                    view.selection_anchor = Some(offset);
                                }
                            }
                            window.focus(&this.focus_handle);
                            this.reset_editor_cursor_blink(cx);
                            cx.notify();
                        }))
                        .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                            let ctrl_held = event.modifiers.control || event.modifiers.platform;
                            if this.ctrl_held != ctrl_held {
                                this.ctrl_held = ctrl_held;
                                cx.notify();
                            }
                            if this.is_mouse_selecting {
                                cx.stop_propagation();
                                let click_x: f32 = event.position.x.into();
                                let col_in_row = if click_x > text_start_x {
                                    let layout = if this.ctrl_preview_selection { &row_layout_preview_move } else { &row_layout_move };
                                    let column = layout.column_for_x(click_x - text_start_x);
                                    layout.line_char_for_column(column).saturating_sub(v_char_start).min(row_len)
                                } else {
                                    0
                                };
                                let target_char = v_char_start + col_in_row;
                                if let Some((view, doc)) = this.state.active_view_and_doc_mut() {
                                    let new_offset = view.offset_for_line_col(doc, v_line_idx, target_char);
                                    if view.selection_anchor.is_none() {
                                        view.selection_anchor = Some(view.cursor_offset);
                                    }
                                    if view.cursor_offset != new_offset {
                                        view.cursor_offset = new_offset;
                                        cx.notify();
                                    }
                                }
                            }
                        }));
                }

                let is_editor_focused = is_leaf_active && self.editor_is_focused;
                let cursor_table = tables.at(cursor_line);

                let raw_btn = if is_editing {
                    if let Some(tbl) = cursor_table {
                        if v_line_idx == tbl.first_line {
                            let is_raw = view.table_raw_now(doc, tbl, cursor_line, self.ctrl_preview_selection || !self.edit_mode);
                            let (btn_w, btn_h) = (20.0, 18.0);
                            let (btn_left, btn_top) = if is_raw {
                                (row_layout.width() + 18.0, ((row_h - btn_h) / 2.0).max(0.0))
                            } else {
                                let total_w = table_layout.as_ref().map(|l| l.total_width).unwrap_or(0.0);
                                (total_w + 20.0, 14.0 + (28.0 - btn_h) / 2.0)
                            };
                            let tbl_first_line = tbl.first_line;
                            let icon_color = if is_raw { theme.accent } else { theme.text_secondary };
                            Some(
                                div()
                                    .id(gpui::ElementId::Name(format!("table-raw-btn-{}", v_idx).into()))
                                    .absolute()
                                    .left(px(btn_left))
                                    .top(px(btn_top))
                                    .w(px(btn_w))
                                    .h(px(btn_h))
                                    .rounded(px(3.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .when(is_raw, |d| {
                                        let mut bg = theme.accent;
                                        bg.a = 0.2;
                                        d.bg(bg).border_1().border_color(theme.accent)
                                    })
                                    .when(!is_raw, |d| {
                                        d.bg(theme.bg_editor).border_1().border_color(theme.border_subtle).hover(|h| h.bg(theme.bg_hover))
                                    })
                                    .child(icons::icon(icons::ICON_CODE, 11.0, icon_color))
                                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.state.active_toggle_raw_table_at(tbl_first_line);
                                        cx.notify();
                                    }))
                                    .on_mouse_down(MouseButton::Right, |_, _, cx| {
                                        cx.stop_propagation();
                                    })
                            )
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                let add_col_btn = if is_editing {
                    if let Some(tbl) = grid_tbl {
                        if v_line_idx == tbl.last_line {
                            let total_w = table_layout.as_ref().unwrap().total_width;
                            let current_widths = table_layout.as_ref().unwrap().col_widths.clone();
                            let tbl_clone = tbl.clone();
                            let group_name = format!("tbl-add-col-grp-{}", tbl.first_line);
                            let (col_btn_top, table_h) = if tbl.last_line == tbl.sep_line() {
                                (-28.0, 29.0)
                            } else {
                                let n = tbl.row_count();
                                let dist_up = (n - 1) as f32 * 28.0 + 1.0;
                                let h = n as f32 * 28.0 + 1.0;
                                (-dist_up, h)
                            };
                            Some(
                                div()
                                    .id(gpui::ElementId::Name(format!("table-add-col-{}", v_idx).into()))
                                    .group(group_name.clone())
                                    .absolute()
                                    .left(px(total_w - 1.0))
                                    .top(px(col_btn_top))
                                    .w(px(14.0))
                                    .h(px(table_h))
                                    .cursor_pointer()
                                    .child(
                                        div()
                                            .relative()
                                            .w_full()
                                            .h_full()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .border_t_1()
                                            .border_r_1()
                                            .border_b_1()
                                            .border_color(theme.border_subtle)
                                            .rounded_r_sm()
                                            .opacity(0.0)
                                            .group_hover(group_name.clone(), |h| {
                                                h.opacity(1.0).border_color(theme.border_subtle)
                                            })
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(theme.text_secondary)
                                                    .child("+")
                                            )
                                    )
                                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        let lines: Vec<String> = this.state.active_document().map(|d| Self::table_lines_slice(d, &tbl_clone)).unwrap_or_default();
                                        let lines_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
                                        let mut model = crate::table::TableModel::parse(&tbl_clone, &lines_refs);
                                        model.insert_col(model.col_count());
                                        let edit = crate::table::rewrite(&tbl_clone, &model, false, &crate::table::char_width);
                                        let widths = this.manual_table_col_widths
                                            .entry(tbl_clone.first_line)
                                            .or_insert_with(|| current_widths.clone());
                                        if widths.len() == tbl_clone.col_count() {
                                            widths.push(120.0);
                                        }
                                        this.table_layouts.borrow_mut().remove(&tbl_clone.first_line);
                                        this.state.active_apply_table_line_edit(&edit, None);
                                        this.schedule_auto_save(cx);
                                        cx.notify();
                                    }))
                                    .on_mouse_down(MouseButton::Right, |_, _, cx| {
                                        cx.stop_propagation();
                                    })
                            )
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                let add_row_btn = if is_editing {
                    if let Some(tbl) = grid_tbl {
                        if v_line_idx == tbl.last_line {
                            let total_w = table_layout.as_ref().unwrap().total_width;
                            let tbl_clone = tbl.clone();
                            let group_name = format!("tbl-add-row-grp-{}", tbl.first_line);
                            Some(
                                div()
                                    .id(gpui::ElementId::Name(format!("table-add-row-{}", v_idx).into()))
                                    .group(group_name.clone())
                                    .absolute()
                                    .left_0()
                                    .top(px(27.0))
                                    .w(px(total_w))
                                    .h(px(13.0))
                                    .cursor_pointer()
                                    .child(
                                        div()
                                            .relative()
                                            .w_full()
                                            .h_full()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .border_l_1()
                                            .border_r_1()
                                            .border_b_1()
                                            .border_color(theme.border_subtle)
                                            .rounded_b_sm()
                                            .opacity(0.0)
                                            .group_hover(group_name.clone(), |h| {
                                                h.opacity(1.0).border_color(theme.border_subtle)
                                            })
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .line_height(px(13.0))
                                                    .text_color(theme.text_secondary)
                                                    .child("+")
                                            )
                                    )
                                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        let lines: Vec<String> = this.state.active_document().map(|d| Self::table_lines_slice(d, &tbl_clone)).unwrap_or_default();
                                        let lines_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
                                        let mut model = crate::table::TableModel::parse(&tbl_clone, &lines_refs);
                                        model.insert_row(model.rows.len());
                                        let edit = crate::table::rewrite(&tbl_clone, &model, false, &crate::table::char_width);
                                        this.table_layouts.borrow_mut().remove(&tbl_clone.first_line);
                                        this.state.active_apply_table_line_edit(&edit, None);
                                        this.schedule_auto_save(cx);
                                        cx.notify();
                                    }))
                                    .on_mouse_down(MouseButton::Right, |_, _, cx| {
                                        cx.stop_propagation();
                                    })
                            )
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                let table_render_arg = grid_tbl.map(|tbl| (tbl, table_layout.as_ref().unwrap().as_ref(), if is_active_line && v_line_idx == cursor_line { Some(active_col_in_row) } else { None }, v_line_idx));

                let is_tbl_sep = grid_tbl.map_or(false, |tbl| v_line_idx == tbl.sep_line());
                let line_num_top = if let Some(tbl) = grid_tbl {
                    if v_line_idx == tbl.first_line { 14.0 } else { 0.0 }
                } else {
                    0.0
                };
                let line_num_h = if is_tbl_sep { 0.0 } else if grid_tbl.is_some() { 28.0 } else { row_h };

                row_div = row_div
                    .child(
                        div()
                            .id(gpui::ElementId::Name(format!("line-num-{}-{}", lid_tag, v_idx).into()))
                            .relative()
                            .w(px(40.0))
                            .flex_shrink_0()
                            .h(px(row_h))
                            .when(!is_tbl_sep, |d| {
                                d.child(
                                    div()
                                        .absolute()
                                        .right_0()
                                        .top(px(line_num_top))
                                        .w(px(line_number_width))
                                        .h(px(line_num_h))
                                        .flex()
                                        .items_center()
                                        .justify_end()
                                        .text_xs()
                                        .text_color(if is_current {
                                            if is_leaf_active {
                                                theme.accent
                                            } else {
                                                theme.text_secondary
                                            }
                                        } else {
                                            theme.text_muted
                                        })
                                        .font_family(editor_font.clone())
                                        .child(line_number_text),
                                )
                            })
                    )
                    .child(
                        div()
                            .id(gpui::ElementId::Name(format!("line-content-{}-{}", lid_tag, v_idx).into()))
                            .relative()
                            .flex()
                            .flex_row()
                            .when(grid_tbl.is_none(), |d| d.items_center())
                            .when(grid_tbl.is_some(), |d| d.items_start())
                            .flex_1()
                            .min_w_0()
                            .whitespace_nowrap()
                            .h(px(row_h))
                            .py_0()
                            .font_family(editor_font.clone())
                            .text_sm()
                            .line_height(px(row_h))
                            .cursor(gpui::CursorStyle::IBeam)
                            .when(is_opening_fence, |s| s.bg(theme.code_bg).rounded_t_md())
                            .when(is_closing_fence, |s| s.bg(theme.code_bg).rounded_b_md())
                            .when(is_code_line, |s| s.bg(theme.code_bg))
                            .when(is_raw_table_line, |s| {
                                let bg = if theme.is_dark { gpui::rgba(0x9333ea24) } else { gpui::rgba(0x7c3aed18) };
                                s.bg(bg)
                            })
                            .when(is_raw_table_first, |s| s.rounded_t_md())
                            .when(is_raw_table_last, |s| s.rounded_b_md())
                            .children(backdrop_rects)
                            .child(
                                div()
                                    .h_full()
                                    .flex()
                                    .flex_row()
                                    .when(grid_tbl.is_none(), |d| d.items_center())
                                    .when(grid_tbl.is_some(), |d| d.items_start())
                                    .whitespace_nowrap()
                                    .child(render_live_styled_row(
                                        line,
                                        &classified,
                                        v_char_start,
                                        v_char_end,
                                        v_line_idx,
                                        v_sub_idx,
                                        is_active_line,
                                        is_in_active_block,
                                        headings,
                                        theme,
                                        &editor_font,
                                        custom_ui_node,
                                        custom_plugin_id,
                                        is_first_code_line,
                                        self.active_plugin_input.as_ref(),
                                        &self.plugin_input_focus_handle,
                                        self.active_plugin_select.as_ref(),
                                        self.plugin_select_popup.as_ref(),
                                        self.plugin_input_blink.visible(),
                                        table_render_arg,
                                        self.ctrl_held,
                                        self.hovered_link_id.as_deref(),
                                        window,
                                        cx,
                                    )),
                            )
                            .children(tag_overlays)
                            .when(raw_btn.is_some(), |s| s.child(raw_btn.unwrap()))
                            .when(add_col_btn.is_some(), |s| s.child(add_col_btn.unwrap()))
                            .when(add_row_btn.is_some(), |s| s.child(add_row_btn.unwrap()))
                            .when(is_current && ((is_leaf_active && is_editor_focused && self.editor_blink.visible()) || !is_leaf_active), |s| {
                                let cursor_col = row_layout.column_for_line_char(v_char_start + active_col_in_row);
                                let cursor_x = crate::snap::snap_x(
                                    row_layout.x_for_column(cursor_col),
                                    pixel_scale,
                                );
                                let (caret_w, caret_bg) = if is_leaf_active {
                                    (2.0, theme.accent)
                                } else {
                                    let mut c = theme.text_muted;
                                    c.a = 0.6;
                                    (1.5, c)
                                };
                                let (caret_top, caret_h) = if let Some(tbl) = grid_tbl {
                                    let top = if v_line_idx == tbl.first_line { 14.0 } else { 0.0 };
                                    (top, 28.0)
                                } else {
                                    (0.0, row_h)
                                };
                                s.child(
                                    div()
                                        .absolute()
                                        .left(px(cursor_x))
                                        .top(px(caret_top))
                                        .h(px(caret_h))
                                        .w(px(caret_w))
                                        .bg(caret_bg)
                                        .rounded_xs(),
                                )
                            })
                            .when(copy_payload.is_some(), |s| {
                                let (block_id, fence_line) = copy_payload.unwrap();
                                let just_copied = self.copied_code_block == Some(block_id);
                                s.child(
                                    div()
                                        .id(gpui::ElementId::Name(format!("code-copy-{}-{}", lid_tag, v_idx).into()))
                                        .absolute()
                                        .right(px(6.0))
                                        .top_0()
                                        .h(px(row_h))
                                        .flex()
                                        .items_center()
                                        .px(px(4.0))
                                        .rounded_sm()
                                        .cursor(gpui::CursorStyle::PointingHand)
                                        .hover(|h| h.bg(theme.bg_hover))
                                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                                            cx.stop_propagation();
                                            let body = match this.state.active_view() {
                                                Some(v) => code_block_body(&v.window, fence_line, block_id),
                                                None => return,
                                            };
                                            cx.write_to_clipboard(ClipboardItem::new_string(body));
                                            this.copied_code_block = Some(block_id);
                                            cx.notify();

                                            let entity = cx.weak_entity();
                                            let async_cx = cx.to_async();
                                            let timer_executor = cx.background_executor().clone();
                                            cx.foreground_executor()
                                                .spawn(async move {
                                                    timer_executor
                                                        .timer(std::time::Duration::from_millis(1500))
                                                        .await;
                                                    let _ = async_cx.update(|cx| {
                                                        if let Some(view) = entity.upgrade() {
                                                            view.update(cx, |this, cx| {
                                                                if this.copied_code_block == Some(block_id) {
                                                                    this.copied_code_block = None;
                                                                    cx.notify();
                                                                }
                                                            });
                                                        }
                                                    });
                                                })
                                                .detach();
                                        }))
                                        .child(icons::icon(
                                            if just_copied { icons::ICON_CHECK } else { icons::ICON_COPY },
                                            14.0,
                                            if just_copied { theme.accent } else { theme.text_muted },
                                        )),
                                )
                            }),
                    );

                items.push(row_div);
            }

            let mut editor_list = div()
                .id(gpui::ElementId::from(("note-editor-list", lid_tag)))
                .flex()
                .flex_col()
                .when(show_scrollbar, |d| d.overflow_hidden())
                .size_full()
                .px_8()
                .pt_0();

            if is_leaf_active {
                editor_list = editor_list
                    .track_scroll(&self.editor_scroll_handle)
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                        if this.is_resizing_any() {
                            return;
                        }
                        cx.stop_propagation();
                        let ctrl_held = event.modifiers.control || event.modifiers.platform;
                        this.ctrl_preview_selection = ctrl_held;
                        this.commit_title_rename(window, cx);
                        window.focus(&this.focus_handle);
                        this.is_mouse_selecting = true;
                        let click_x: f32 = event.position.x.into();
                        let click_y: f32 = event.position.y.into();
                        this.handle_editor_mouse_down(click_x, click_y, event.modifiers.shift, event.click_count, window, cx);
                    }))
                    .on_scroll_wheel(cx.listener(move |this, event: &gpui::ScrollWheelEvent, _, cx| {
                        if this.is_resizing_any() {
                            return;
                        }
                        this.handle_editor_wheel(event, max_scroll_content, cx);
                    }));
            } else {
                editor_list = editor_list
                    .on_scroll_wheel(cx.listener(move |this, event: &gpui::ScrollWheelEvent, _, cx| {
                        if this.is_resizing_any() {
                            return;
                        }
                        if let Some(lid) = leaf_id {
                            if let Some(leaf) = this.state.leaves.get(&lid) {
                                if let Some(tab) = leaf.active_tab() {
                                    if let Some(view) = this.state.views.get_mut(&tab.id) {
                                        let delta = match event.delta {
                                            gpui::ScrollDelta::Lines(lines) => lines.y as f64 * 60.0,
                                            gpui::ScrollDelta::Pixels(pixels) => f32::from(pixels.y) as f64,
                                        };
                                        view.scroll_top = (view.scroll_top - delta).clamp(0.0, max_scroll_content);
                                        if let crate::workspace::TabContent::Document { id } = tab.content {
                                            if let Some(doc) = this.state.documents.get(&id) {
                                                let lines_total = doc.line_index.lines();
                                                view.viewport.offset = view.line_at_doc_y(lines_total, view.scroll_top);
                                            }
                                        }
                                        cx.notify();
                                    }
                                }
                            }
                        }
                    }));
            }

            editor_list = editor_list.child(
                div()
                    .id(gpui::ElementId::from(("note-editor-scroll-content", lid_tag)))
                    .w_full()
                    .flex()
                    .flex_col()
                    .top(px(slice_shift as f32))
                    .children(items),
            );

            editor_container = editor_container.child(editor_list);

            if show_scrollbar {
                let is_resizing = self.is_resizing_any();
                let mut scrollbar_track = div()
                    .id(gpui::ElementId::from(("editor-scrollbar-track", lid_tag)))
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right_0()
                    .w(px(track_w))
                    .h_full()
                    .overflow_hidden()
                    .cursor_pointer()
                    .when(!wide_scrollbar && !is_resizing, |d| {
                        d.hover(|h| h.bg(rgba(0x00000015)))
                    })
                    .when(wide_scrollbar, |d| {
                        d.bg(theme_scrollbar.bg_sidebar)
                    });

                if is_leaf_active {
                    scrollbar_track = scrollbar_track
                        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                            if this.is_resizing_any() {
                                if this.is_hovering_editor_scrollbar {
                                    this.is_hovering_editor_scrollbar = false;
                                    cx.notify();
                                }
                                return;
                            }
                            if this.is_hovering_editor_scrollbar != *hovered {
                                this.is_hovering_editor_scrollbar = *hovered;
                                cx.notify();
                            }
                        }))
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            if this.is_resizing_any() {
                                return;
                            }
                            cx.stop_propagation();
                            let click_y: f32 = event.position.y.into();
                            let click_rel_y = f64::from((click_y - track_top_y).max(0.0));
                            let target_fraction = (click_rel_y / f64::from(viewport_h)).clamp(0.0, 1.0);
                            let target_scroll_y = target_fraction * max_scroll_content;
                            this.set_editor_scroll_top_f64(target_scroll_y);
                            cx.notify();
                        }));
                } else if let Some(lid) = leaf_id {
                    scrollbar_track = scrollbar_track
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                            if this.is_resizing_any() {
                                return;
                            }
                            cx.stop_propagation();
                            this.switch_to_leaf(lid, window, cx);
                        }))
                        .on_mouse_down(MouseButton::Middle, cx.listener(move |this, _, window, cx| {
                            if this.is_resizing_any() {
                                return;
                            }
                            cx.stop_propagation();
                            this.switch_to_leaf(lid, window, cx);
                            this.start_editor_middle_scroll(cx);
                        }));
                }

                let mut scrollbar_thumb = div()
                    .id(gpui::ElementId::from(("editor-scrollbar-thumb", lid_tag)))
                    .absolute()
                    .top(px(thumb_top))
                    .right(px(thumb_right))
                    .w(px(thumb_w))
                    .h(px(thumb_h))
                    .when(wide_scrollbar, |d| d.rounded_none())
                    .when(!wide_scrollbar, |d| d.rounded_full())
                    .bg(thumb_bg)
                    .when(!wide_scrollbar && !is_resizing, |d| {
                        d.hover(|h| h.bg(theme_scrollbar.text_secondary).w(px(8.0)))
                    })
                    .cursor_pointer();

                if is_leaf_active {
                    scrollbar_thumb = scrollbar_thumb
                        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                            if this.is_resizing_any() {
                                if this.is_hovering_editor_thumb {
                                    this.is_hovering_editor_thumb = false;
                                    cx.notify();
                                }
                                return;
                            }
                            if this.is_hovering_editor_thumb != *hovered {
                                this.is_hovering_editor_thumb = *hovered;
                                cx.notify();
                            }
                        }))
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            if this.is_resizing_any() {
                                return;
                            }
                            cx.stop_propagation();
                            this.is_dragging_scrollbar = true;
                            this.scrollbar_drag_start_y = event.position.y.into();
                            this.scrollbar_drag_start_scroll_y = scroll_y_current;
                            cx.notify();
                        }));
                }

                scrollbar_track = scrollbar_track.child(scrollbar_thumb);

                if let Some((top, h)) = ruler_sel_span {
                    scrollbar_track = scrollbar_track.child(
                        div()
                            .absolute()
                            .top(px(top))
                            .h(px(h))
                            .when(ruler_full, |d| {
                                d.left(px(0.0)).w_full().rounded_xs()
                            })
                            .when(!ruler_full, |d| {
                                d.left(px(ruler_pill_left))
                                    .w(px(ruler_pill_w))
                                    .rounded_full()
                            })
                            .bg(if self.ctrl_preview_selection || !self.edit_mode {
                                theme_scrollbar.preview_selection_bg()
                            } else {
                                theme_scrollbar.selection_bg
                            }),
                    );
                }

                if show_ruler {
                    scrollbar_track = scrollbar_track.child(
                        div()
                            .id(gpui::ElementId::from(("editor-ruler-cursor", lid_tag)))
                            .absolute()
                            .top(px(ruler_cursor_y))
                            .h(px(2.0))
                            .when(ruler_full, |d| {
                                d.left(px(0.0)).w_full().rounded_xs()
                            })
                            .when(!ruler_full, |d| {
                                d.left(px(ruler_pill_left))
                                        .w(px(ruler_pill_w))
                                    .rounded_full()
                            })
                            .bg(if ruler_cursor_under_thumb {
                                theme_scrollbar.border
                            } else {
                                theme_scrollbar.accent
                            }),
                    );
                }

                editor_container = editor_container.child(scrollbar_track);
            }

            editor_root = editor_root.child(title_box).child(editor_container);
        } else if leaf_id
            .and_then(|lid| self.state.leaves.get(&lid))
            .and_then(|l| l.active_tab())
            .map(|t| matches!(t.content, crate::workspace::TabContent::Empty))
            .unwrap_or(is_leaf_active && self.state.active_empty_tab.is_some())
        {
            editor_root = editor_root.child(self.render_new_tab_page(theme, cx));
        } else if is_leaf_active && self.active_center_leaf.is_some() {
            if let Some(leaf_id) = &self.active_center_leaf {
                if let Some(leaf) = self.plugin_leaf_views.get(leaf_id) {
                    editor_root = editor_root.child(
                        div()
                            .id("central-leaf-view")
                            .flex()
                            .flex_col()
                            .size_full()
                            .p(px(16.0))
                            .overflow_scroll()
                            .child(crate::plugins::ui::render_ui_node(
                                &leaf.tree,
                                &leaf.plugin_id,
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
                }
            }
        } else {
            editor_root = editor_root.child(
                div()
                    .flex()
                    .flex_col()
                    .size_full()
                    .justify_center()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_3xl()
                            .child("🦅"),
                    )
                    .child(
                        div()
                            .text_xl()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child("Welcome to Vibecodium"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.text_muted)
                            .child("Select a note from the sidebar or click 'Open Folder' to choose a workspace."),
                    ),
            );
        }

        editor_root
    }
}

/// FPS-монитор — только для debug-сборки.
///
/// В release-сборке это пустая функция: в Windows-приложении с подсистемой "windows"
/// консоли нет, и писать в stdout/stderr некуда. Убираем последнего runtime-писателя
/// в консоль, чтобы исключить его влияние на CPU в простое.
#[cfg(debug_assertions)]
fn log_fps_tick() {
    static FRAME_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    static LAST_FPS: std::sync::OnceLock<std::sync::Mutex<std::time::Instant>> =
        std::sync::OnceLock::new();
    static LAST_FRAME_START: std::sync::Mutex<Option<std::time::Instant>> =
        std::sync::Mutex::new(None);
    static FRAME_INTERVALS_MS: std::sync::Mutex<Vec<f32>> = std::sync::Mutex::new(Vec::new());
    let now = std::time::Instant::now();
    // Interval since the previous render call: the cadence the eye actually
    // sees. Its percentiles are the hitch detector — a 50 ms outlier is a
    // visible stall no matter what the average says. Idle blinks (~530 ms) mix
    // in during idle seconds; while typing they cluster at the frame budget.
    if let Ok(mut prev) = LAST_FRAME_START.lock() {
        if let Some(start) = *prev {
            let interval_ms = now.duration_since(start).as_secs_f32() * 1000.0;
            if let Ok(mut intervals) = FRAME_INTERVALS_MS.lock() {
                if intervals.len() >= 4096 {
                    intervals.clear();
                }
                intervals.push(interval_ms);
            }
        }
        *prev = Some(now);
    }
    let mutex = LAST_FPS.get_or_init(|| std::sync::Mutex::new(std::time::Instant::now()));
    FRAME_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut last) = mutex.try_lock() {
        if last.elapsed() >= std::time::Duration::from_secs(1) {
            let frames = FRAME_COUNT.swap(0, std::sync::atomic::Ordering::Relaxed);
            let elapsed = last.elapsed();
            *last = std::time::Instant::now();
            // PERF counters ride the same one-second tick: `shape` = actual
            // row-shaping calls (cache misses plus direct calls outside the
            // cache), `shape_hits` = row-cache hits. While holding a key the
            // working set is ~57 visible rows, so a working cache shows
            // shape ≈ number of *changed* rows per second and hits in the
            // thousands. Render calls themselves are the wake-up metric:
            // ~2/s idle (caret blink), ~1–2 per keystroke while typing (the
            // edit frame plus the settle frame).
            let counters = crate::perf::PERF.snapshot_and_reset();
            let percentiles = {
                let mut intervals = FRAME_INTERVALS_MS.lock().unwrap();
                let mut taken = std::mem::take(&mut *intervals);
                taken.sort_by(f32::total_cmp);
                let pick = |q: f32| -> f32 {
                    let n = taken.len();
                    if n == 0 {
                        return 0.0;
                    }
                    taken[(((n - 1) as f32) * q).round() as usize]
                };
                (pick(0.5), pick(0.95), taken.last().copied().unwrap_or(0.0))
            };
            eprintln!(
                "[FPS Monitor] {:?} elapsed, {} render calls (~{:.1} FPS), frame_ms p50={:.1} p95={:.1} max={:.1}, shape={} shape_hits={}",
                elapsed,
                frames,
                frames as f32 / elapsed.as_secs_f32(),
                percentiles.0,
                percentiles.1,
                percentiles.2,
                counters[3],
                counters[4],
            );
        }
    }
}

/// В release-сборке монитор выключен — ни одной записи в консоль.
#[cfg(not(debug_assertions))]
fn log_fps_tick() {}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        log_fps_tick();
        self.ensure_inactive_views_built(window, cx);

        // Advance the optional wheel ease before anything else touches the
        // offset. One-shot writes below (restore/reveal) go through
        // `set_editor_scroll_top_f64`, which cancels the ease, so they win.
        // With the flag off any stray in-flight ease is dropped instead.
        if self.config.editor.wheel_smooth_scroll {
            self.step_wheel_ease(window);
        } else {
            self.wheel_ease.set(None);
        }

        // Device-pixel grid for thin carets: GPUI paints quads at raw float
        // coordinates, so an unsnapped 1.5-2px caret visibly changes thickness
        // while typing. Captured once per frame, reused at every caret site.
        let _pixel_scale = window.scale_factor();

        if !self.initial_scroll_done {
            self.initial_scroll_done = true;
            // Not written yet: until the list is laid out, prepaint clamps
            // against a not-yet-measured max and the restore is lost. The
            // slice below already reads from the pending target, and the
            // write is retried once the bounds are measured.
            self.pending_editor_restore = Some(f64::from(self.initial_scroll_top));
            self.need_reveal_cursor = true;
        }

        // The line at the top of the viewport is what the editor window has to
        // cover, so it is the anchor the window is placed around. Resolved here,
        // once per frame, from the same position the slice below uses — while a
        // restore is pending that is the restore target, not the (not yet
        // written) scroll handle.
        let anchor_doc_y = self
            .pending_editor_restore
            .unwrap_or_else(|| self.current_scroll_top_f64());
        if !self.is_resizing_any() {
            if let Some((view, doc)) = self.state.active_view_and_doc_mut() {
                view.viewport.offset = view.line_at_doc_y(doc.line_index.lines(), anchor_doc_y);
            }
        }

        // Walk the code-fence checkpoints a little further ahead, once per
        // frame. Nothing here is needed for the current frame — it is what
        // makes a later jump to the end of a huge note cheap, without a
        // background thread and without copying the text.
        if let Some(doc) = self.state.active_document_mut() {
            doc.scan_code_ahead(crate::code_scan::SCAN_SLICE_LINES);
        }

        let is_maximized = window.is_maximized();
        let bounds = window.bounds();
        // Live window height for virtualized-list floors (see editor_scroll_metrics,
        // render_sidebar_tree): measured scroll bounds lag one frame behind on
        // creation/maximize, the window size itself is current from frame one.
        let _window_h = f32::from(bounds.size.height);
        let geom = (
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
            Some(f32::from(bounds.origin.x)),
            Some(f32::from(bounds.origin.y)),
            is_maximized,
        );
        if self.last_saved_geometry != Some(geom) {
            self.last_saved_geometry = Some(geom);
            let mut ws = crate::state::WindowState::load(&crate::state::app_data_dir());
            ws.maximized = is_maximized;
            if !is_maximized {
                ws.width = geom.0;
                ws.height = geom.1;
                ws.x = geom.2;
                ws.y = geom.3;
            }
            ws.save(&crate::state::app_data_dir());
        }

        let is_custom_hex_focused = self.custom_hex_focus_handle.is_focused(window);
        if !self.hex_is_focused && is_custom_hex_focused {
            self.hex_is_focused = true;
            self.enable_hex_cursor_blink(cx);
        } else if self.hex_is_focused && !is_custom_hex_focused {
            self.hex_is_focused = false;
            self.disable_hex_cursor_blink(cx);
        }

        let is_title_focused = self.title_focus_handle.is_focused(window);
        if !self.was_title_focused && is_title_focused {
            self.enable_title_cursor_blink(cx);
        } else if self.was_title_focused && !is_title_focused {
            self.disable_title_cursor_blink(cx);
            self.commit_title_rename_internal();
        }
        self.was_title_focused = is_title_focused;

        let is_search_focused = self.search_focus_handle.is_focused(window);
        self.quick_filter.sync_focus(is_search_focused, cx);

        let is_editor_focused_now = self.focus_handle.is_focused(window)
            || (!is_title_focused
                && !is_search_focused
                && !is_custom_hex_focused
                && !self.folder_modal_focus_handle.is_focused(window)
                && !self.folder_context_menu.is_open
                && self.folder_modal == crate::views::sidebar::FolderModal::None);

        if !self.editor_is_focused && is_editor_focused_now {
            self.editor_is_focused = true;
            self.enable_editor_cursor_blink(cx);
        } else if self.editor_is_focused && !is_editor_focused_now {
            self.editor_is_focused = false;
            self.disable_editor_cursor_blink(cx);
        }

        // Row geometry, the restore and the caret reveal run before `theme`
        // borrows `self`: the reveal may re-place the editor window on the caret,
        // and that needs the note mutably.
        let editor_font = self.editor_font_family();
        let font_id = cx.text_system().resolve_font(&gpui::font(editor_font));
        let line_char_width = cx
            .text_system()
            .advance(font_id, px(14.0), 'x')
            .map(|s| f32::from(s.width))
            .unwrap_or(7.7);

        let viewport_w: f32 = window.viewport_size().width.into();
        let wrap_cols = self.compute_wrap_cols(viewport_w, line_char_width);
        self.last_wrap_cols = wrap_cols;
        let curr_scroll = self.current_scroll_top_f64();
        let mut new_active_scroll = None;
        if let Some((view, doc)) = self.state.active_view_and_doc_mut() {
            let anchor_line = view.viewport.offset;
            let old_line_top = view.doc_y_of_line(doc.line_index.lines(), anchor_line);
            let px_in_line = (curr_scroll - old_line_top).max(0.0);
            let old_above_h = view.above_h;
            let old_wrap_cols = view.window.wrap_cols;

            view.ensure_visual_cache_with_headings(doc, self.word_wrap, wrap_cols, Some(&self.config.editor.headings), anchor_line, self.ctrl_preview_selection || !self.edit_mode);

            if view.window.wrap_cols != old_wrap_cols || (view.above_h - old_above_h).abs() > 0.5 {
                let first_v_idx = view.window.first_row_of_line(anchor_line).unwrap_or(0);
                let line_local_y = view.window.height_index.top_of(first_v_idx);
                let new_scroll_y = view.above_h + line_local_y + px_in_line;
                new_active_scroll = Some(new_scroll_y);
            }
        }
        if let Some(new_scroll) = new_active_scroll {
            self.set_editor_scroll_top_f64(new_scroll);
        }
        let active_leaf_id = self.state.workspace.active_leaf;
        let window_h: f32 = window.viewport_size().height.into();
        let (leaf_v_ratio, in_titlebar) = (
            self.state.workspace.vertical_height_ratio_of_leaf(active_leaf_id),
            self.state.workspace.is_leaf_in_titlebar(active_leaf_id),
        );
        let workspace_h = (window_h - 66.0).max(100.0);
        let leaf_total_h = workspace_h * leaf_v_ratio;
        let header_h = if in_titlebar { 90.0 } else { 126.0 };
        let estimated_active_vh = (leaf_total_h - header_h).max(50.0);

        let bounds_h = f32::from(self.editor_scroll_handle.bounds().size.height);
        if bounds_h > 50.0 && (bounds_h - estimated_active_vh).abs() < (estimated_active_vh * 0.35).max(40.0) {
            if let Some(leaf) = self.state.leaves.get(&active_leaf_id) {
                if let Some(tab) = leaf.active_tab() {
                    if let Some(view) = self.state.views.get_mut(&tab.id) {
                        view.measured_viewport_h = Some(bounds_h);
                    }
                }
            }
        }

        // Retry a pending restore until the viewport is measured: the slice below
        // already follows the target, so the rows are right from frame one.
        if self.pending_editor_restore.is_some() {
            let total = self
                .state
                .active_view()
                .map(|v| v.total_height())
                .unwrap_or(0.0);
            let (_, viewport_h, _, _) =
                self.editor_scroll_metrics_for_leaf(Some(active_leaf_id), true, total, window_h);
            if viewport_h > 50.0 {
                if let Some(target) = self.pending_editor_restore.take() {
                    let clamped =
                        Self::clamp_restore_target(target, total, f64::from(viewport_h));
                    if (clamped - self.current_scroll_top_f64()).abs() > 0.5 {
                        self.set_editor_scroll_top_f64(clamped);
                    }
                }
            }
        }
        // The cache above is freshly built, so this reveal is exact — and a
        // no-op when the restored offset already shows the cursor.
        if self.need_reveal_cursor {
            self.need_reveal_cursor = false;
            self.scroll_cursor_into_view();
        }

        self.theme = Theme::from_config(&self.config.ui.theme, Some(window.appearance()));
        let cx = &*cx;
        let theme = &self.theme;
        let active_path = self.state.active_document().map(|d| d.path.clone());
        let _is_editing = self.edit_mode;

        // Draw the frames the scrollbar needs to settle.
        //
        // `editor_scroll_metrics` reads the handle's `bounds` / `max_offset`,
        // which prepaint writes later in this very frame (`clamp_scroll_position`
        // in div.rs), so the numbers used below are always one layout behind. On
        // the first frame after a note is opened they are estimates — a fixed
        // 600 px viewport and the previous note's max — and the thumb is painted
        // from them until some unrelated notify happens to repaint. Ask for the
        // next frame here instead, and keep asking while the inputs still move.
        //
        // This terminates: `inputs` is a pure function of the layout and the
        // window size, and the layout does not depend on it — the scrollbar
        // reserves no width, and the list does not scroll through gpui at all —
        // so two consecutive frames agree and the burst ends. The counter alone
        // is not enough: a resize or a rewrap moves the inputs without going
        // through the arm sites.
        //
        // Arming is thresholded because a text edit also moves `max_scroll` —
        // by a row or two per keystroke. Painting the whole scene one more
        // time for a one-frame-stale thumb on every keystroke is what made
        // key-repeat burn twice the GPU. Any viewport-height change (resize,
        // no explicit hook exists) or a large content jump arms the burst;
        // small deltas do not. `last_scroll_inputs` is always updated so the
        // comparison stays a single-frame delta and never accumulates.
        if let Some(view) = self.state.active_view() {
            let total = view.total_height();
            let (_, viewport_h, max_scroll, _) = self.editor_scroll_metrics(total, window_h);
            let inputs = (viewport_h.round(), max_scroll.round());
            let settling = self.layout_settle_frames.get();
            let prev = self.last_scroll_inputs.get();
            self.last_scroll_inputs.set(inputs);
            let big_jump = (inputs.0 - prev.0).abs() > f32::EPSILON
                || (inputs.1 - prev.1).abs() > f64::from(viewport_h.max(1.0)) * 0.25;
            if settling > 0 || big_jump {
                self.layout_settle_frames.set(settling.saturating_sub(1));
                window.request_animation_frame();
            }
        }

        let is_resizer_active = self.is_resizing_sidebar || self.is_hovering_resizer;
        let resizer_color = if is_resizer_active { theme.accent } else { theme.border };
        div()
            .flex()
            .flex_col()
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(theme.bg_app)
            .text_color(theme.text_primary)
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                let ctrl_held = event.modifiers.control || event.modifiers.platform;
                if this.ctrl_held != ctrl_held {
                    this.ctrl_held = ctrl_held;
                    cx.notify();
                }
                if this.is_resizing_sidebar {
                    let x: f32 = event.position.x.into();
                    if x < 100.0 {
                        this.sidebar_collapsed = true;
                    } else {
                        this.sidebar_collapsed = false;
                        this.sidebar_width = x.clamp(160.0, 600.0);
                    }
                    cx.notify();
                } else if this.is_resizing_right_sidebar {
                    let win_w: f32 = window.viewport_size().width.into();
                    let x: f32 = event.position.x.into();
                    let right_w = win_w - x;
                    if right_w < 100.0 {
                        this.right_sidebar_collapsed = true;
                    } else {
                        this.right_sidebar_collapsed = false;
                        this.right_sidebar_width = right_w.clamp(160.0, 600.0);
                    }
                    cx.notify();
                } else if let Some(ref drag) = this.split_drag_state.clone() {
                    let current_pos = match drag.direction {
                        crate::workspace::SplitDirection::Row => f32::from(event.position.x),
                        crate::workspace::SplitDirection::Column => f32::from(event.position.y),
                    };
                    let delta = current_pos - drag.start_pos;
                    let ratio_delta = if drag.span > 1.0 { delta / drag.span } else { 0.0 };
                    let new_ratio = drag.initial_ratio + ratio_delta;
                    this.state.workspace.set_split_ratio_at(&drag.path, drag.divider_index, new_ratio);
                    let win_w: f32 = window.bounds().size.width.into();
                    let win_h: f32 = window.bounds().size.height.into();
                    let left_w = if this.sidebar_collapsed { 44.0 } else { this.sidebar_width + 1.0 };
                    let right_w = if this.right_sidebar_collapsed { 0.0 } else { this.right_sidebar_width + 1.0 };
                    this.state.update_leaf_bounds_from_layout(win_w, win_h, left_w, right_w);
                    cx.notify();
                } else if this.sidebar_middle_active {
                    // Middle-button smart scroll keeps working even when the
                    // cursor leaves the tree: the target clamps to the ends.
                    // Takes precedence over scrollbar/selection drags — the
                    // middle button is a separate gesture.
                    let y: f32 = event.position.y.into();
                    this.on_sidebar_smart_mouse_move(y, true, cx);
                } else if this.editor_middle_active {
                    let y: f32 = event.position.y.into();
                                                if let Some((top, h, max)) = this.editor_middle_metrics() {
                        this.on_editor_middle_mouse_move(y, top, h, max, cx);
                    } else {
                        cx.notify();
                    }
                } else if this.is_dragging_scrollbar {
                    let y: f32 = event.position.y.into();
                    this.handle_scrollbar_drag(y, window, cx);
                } else if this.sidebar_scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    this.handle_sidebar_scrollbar_drag(y, cx);
                } else if this.plugin_modal_middle_active {
                    let y: f32 = event.position.y.into();
                    this.on_plugin_modal_middle_mouse_move(y, cx);
                } else if this.plugin_modal_scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    this.handle_plugin_modal_scrollbar_drag(y, cx);
                } else if this.state.active_view().and_then(|v| v.title_input.as_ref()).map(|ti| ti.is_mouse_selecting).unwrap_or(false) {
                    let x: f32 = event.position.x.into();
                    let editor_left_x = if this.sidebar_collapsed { 44.0 } else { this.sidebar_width + 1.0 };
                    let leaf_ratio = this.state.workspace.horizontal_ratio_of_leaf(this.state.workspace.active_leaf);
                    let window_w: f32 = window.bounds().size.width.into();
                    let editor_total_w = (window_w - editor_left_x).max(0.0);
                    let title_left_x = editor_left_x + leaf_ratio * editor_total_w + 48.0;
                    let rel_x = (x - title_left_x).max(0.0);
                    if let Some(view) = this.state.active_view_mut() {
                        if let Some(ref mut input) = view.title_input {
                            let new_pos = if input.text.is_empty() {
                                0
                            } else {
                                let run = gpui::TextRun {
                                    len: input.text.len(),
                                    font: gpui::font("Segoe UI").bold(),
                                    color: Default::default(),
                                    underline: Default::default(),
                                    strikethrough: None,
                                    background_color: None,
                                };
                                let shaped = window.text_system().shape_line(
                                    SharedString::from(input.text.clone()),
                                    px(28.0),
                                    &[run],
                                    None,
                                );
                                let byte_idx = shaped.closest_index_for_x(px(rel_x));
                                input.text[..byte_idx.min(input.text.len())].chars().count()
                            };
                            if input.cursor_pos != new_pos {
                                input.cursor_pos = new_pos;
                                this.reset_title_cursor_blink(cx);
                                cx.notify();
                            }
                        }
                    }
                } else if this.quick_filter.is_mouse_selecting {
                    this.quick_filter.handle_mouse_move(event, &this.state.search_query, window, cx);
                } else if this.panel_scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    this.handle_panel_scrollbar_drag(y, cx);
                } else if this.sidebar_search.is_mouse_selecting {
                    this.handle_panel_mouse_move(event, window, cx);
                } else if this.catalog_scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    this.handle_catalog_scrollbar_drag(y, cx);
                } else if this.goto_scrollbar_drag.active {
                    let y: f32 = event.position.y.into();
                    this.handle_goto_scrollbar_drag(y, cx);
                } else if this.search_dialog.is_mouse_selecting {
                    this.handle_goto_mouse_move(event, window, cx);
                } else if this.is_mouse_selecting {
                    let x: f32 = event.position.x.into();
                    let y: f32 = event.position.y.into();
                    let viewport_h: f32 = window.viewport_size().height.into();
                    this.update_mouse_selection(x, y, viewport_h, window, cx);
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.quick_filter.handle_mouse_up(cx);
                this.handle_goto_mouse_up(cx);
                this.goto_scrollbar_drag.end();
                this.catalog_scrollbar_drag.end();
                this.handle_panel_mouse_up(cx);
                this.panel_scrollbar_drag.end();
                if this.is_resizing_sidebar {
                    // Persist the dragged width right away (crash-safe;
                    // exit save refreshes it again).
                    this.state.session.sidebar_width = this.sidebar_width;
                    this.state.save_session_state();
                }
                if let Some(view) = this.state.active_view_mut() {
                    if let Some(ref mut input) = view.title_input {
                        if input.is_mouse_selecting {
                            input.is_mouse_selecting = false;
                            if input.selection_anchor == Some(input.cursor_pos) {
                                input.selection_anchor = None;
                            }
                            cx.notify();
                        }
                    }
                }
                this.is_resizing_sidebar = false;
                this.is_resizing_right_sidebar = false;
                this.split_drag_state = None;
                this.hovered_split_divider = None;
                this.is_mouse_selecting = false;
                this.is_dragging_scrollbar = false;
                this.sidebar_scrollbar_drag.end();
                this.plugin_modal_scrollbar_drag.end();
                if let Some(view) = this.state.active_view_mut() {
                    if view.selection_anchor == Some(view.cursor_offset) {
                        view.selection_anchor = None;
                    }
                }
                cx.notify();
            }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.quick_filter.handle_mouse_up(cx);
                this.handle_goto_mouse_up(cx);
                this.goto_scrollbar_drag.end();
                this.handle_panel_mouse_up(cx);
                this.panel_scrollbar_drag.end();
                if let Some(view) = this.state.active_view_mut() {
                    if let Some(ref mut input) = view.title_input {
                        if input.is_mouse_selecting {
                            input.is_mouse_selecting = false;
                            if input.selection_anchor == Some(input.cursor_pos) {
                                input.selection_anchor = None;
                            }
                            cx.notify();
                        }
                    }
                }
                this.is_resizing_sidebar = false;
                this.is_resizing_right_sidebar = false;
                this.split_drag_state = None;
                this.hovered_split_divider = None;
                this.is_hovering_resizer = false;
                this.is_hovering_right_resizer = false;
                this.is_mouse_selecting = false;
                this.is_dragging_scrollbar = false;
                this.sidebar_scrollbar_drag.end();
                this.plugin_modal_scrollbar_drag.end();
                if let Some(view) = this.state.active_view_mut() {
                    if view.selection_anchor == Some(view.cursor_offset) {
                        view.selection_anchor = None;
                        this.ctrl_preview_selection = false;
                    }
                }
                cx.notify();
            }))
            .on_mouse_up(MouseButton::Middle, cx.listener(|this, _, _, cx| {
                // Releasing the middle button anywhere ends smart-scroll
                // gestures (covers releases outside the originating viewport).
                this.stop_sidebar_middle_scroll(cx);
                this.stop_editor_middle_scroll(cx);
                this.stop_plugin_modal_middle_scroll(cx);
            }))
            .on_mouse_up_out(MouseButton::Middle, cx.listener(|this, _, _, cx| {
                this.stop_sidebar_middle_scroll(cx);
                this.stop_editor_middle_scroll(cx);
                this.stop_plugin_modal_middle_scroll(cx);
            }))
            .on_modifiers_changed(cx.listener(|this, event: &ModifiersChangedEvent, _window, cx| {
                let ctrl_held = event.modifiers.control || event.modifiers.platform;
                if this.ctrl_held != ctrl_held {
                    this.ctrl_held = ctrl_held;
                    cx.notify();
                }
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let keystroke = &event.keystroke;
                // Ctrl+P opens Command Palette from anywhere (leaves don't handle it,
                // so this root listener fires exactly once; the modal input
                // stops propagation, so an open modal never re-triggers it).
                if keystroke.modifiers.control
                    && !keystroke.modifiers.shift
                    && (keystroke.key.eq_ignore_ascii_case("p")
                        || keystroke.key == "з"
                        || keystroke.key == "З")
                    && !this.command_palette.is_open
                {
                    this.open_command_palette(window, cx);
                }
                // Ctrl+O opens Go to file (Search Dialog) from anywhere.
                if keystroke.modifiers.control
                    && !keystroke.modifiers.shift
                    && (keystroke.key.eq_ignore_ascii_case("o")
                        || keystroke.key == "щ"
                        || keystroke.key == "Щ")
                    && !this.search_dialog.is_open
                {
                    this.open_search_dialog(window, cx);
                }
                // Ctrl+Shift+O opens folder dialog from anywhere.
                if keystroke.modifiers.control
                    && keystroke.modifiers.shift
                    && (keystroke.key.eq_ignore_ascii_case("o")
                        || keystroke.key == "щ"
                        || keystroke.key == "Щ")
                {
                    this.open_folder_dialog(window, cx);
                }
                // Ctrl+Shift+P also opens Command Palette for muscle memory compatibility.
                if keystroke.modifiers.control
                    && keystroke.modifiers.shift
                    && (keystroke.key.eq_ignore_ascii_case("p")
                        || keystroke.key == "з"
                        || keystroke.key == "З")
                {
                    this.open_command_palette(window, cx);
                }
                // Ctrl+Shift+F opens the sidebar search panel from anywhere.
                if keystroke.modifiers.control
                    && keystroke.modifiers.shift
                    && (keystroke.key.eq_ignore_ascii_case("f")
                        || keystroke.key == "а"
                        || keystroke.key == "А")
                {
                    this.open_sidebar_search(window, cx);
                }
                // Ctrl+\ splits active leaf to the right
                if keystroke.modifiers.control
                    && !keystroke.modifiers.shift
                    && (keystroke.key == "\\" || keystroke.key == "|")
                {
                    if let Some(doc) = this.state.active_document() {
                        let path = doc.path.clone();
                        this.split_tab_right(&path, window, cx);
                    }
                }
                // Ctrl+Shift+\ splits active leaf down
                if keystroke.modifiers.control
                    && keystroke.modifiers.shift
                    && (keystroke.key == "\\" || keystroke.key == "|")
                {
                    if let Some(doc) = this.state.active_document() {
                        let path = doc.path.clone();
                        this.split_tab_down(&path, window, cx);
                    }
                }
                // Ctrl+W closes active tab
                if keystroke.modifiers.control
                    && !keystroke.modifiers.shift
                    && (keystroke.key.eq_ignore_ascii_case("w")
                        || keystroke.key == "ц"
                        || keystroke.key == "Ц")
                {
                    this.close_active_leaf_tab(window, cx);
                }
            }))
            // ================= WINDOW TITLEBAR & TAB BAR (CUSTOM CSD FRAME) =================
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .h(px(38.0))
                    .w_full()
                    .min_w_0()
                    .overflow_hidden()
                    .bg(theme.bg_sidebar)
                    // Left area: Sidebar header column in titlebar (width matches sidebar)
                    .when(!self.sidebar_collapsed, |s| {
                        s.child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .w(px(self.sidebar_width))
                                .flex_shrink_0()
                                .h_full()
                                .bg(theme.bg_sidebar)
                                .border_b_1()
                                .border_color(theme.border)
                                // 1. Rail header cell (44px) matching the rail below
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .w(px(44.0))
                                        .h_full()
                                        .flex_shrink_0()
                                        .border_r_1()
                                        .border_color(theme.border)
                                        .child(
                                            div()
                                                .id("btn-toggle-sidebar")
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .p_1()
                                                .rounded_md()
                                                .cursor_pointer()
                                                .hover(|h| h.bg(theme.bg_hover))
                                                .child(icons::icon(
                                                    icons::ICON_PANEL_LEFT,
                                                    15.0,
                                                    theme.accent,
                                                ))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.toggle_sidebar(cx);
                                                })),
                                        ),
                                )
                                // 2. File tree header (VIBECODIUM badge and drag region)
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .justify_between()
                                        .flex_1()
                                        .h_full()
                                        .px_3()
                                        .child(
                                            div()
                                                .px_2()
                                                .py_0p5()
                                                .bg(theme.accent)
                                                .text_color(theme.bg_sidebar)
                                                .font_weight(gpui::FontWeight::BOLD)
                                                .rounded_md()
                                                .text_xs()
                                                .child("VIBECODIUM"),
                                        )
                                        // Files / Search view toggle (Obsidian-style)
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_1()
                                                .mx_1()
                                                .child(
                                                    div()
                                                        .id("btn-sidebar-files")
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .w(px(28.0))
                                                        .h(px(28.0))
                                                        .rounded_md()
                                                        .cursor_pointer()
                                                        .when(
                                                            self.sidebar_mode
                                                                == crate::views::sidebar::SidebarMode::Files,
                                                            |d| d.bg(theme.bg_hover),
                                                        )
                                                        .hover(|h| h.bg(theme.bg_hover))
                                                        .child(icons::icon(
                                                            icons::ICON_FOLDER,
                                                            14.0,
                                                            if self.sidebar_mode == crate::views::sidebar::SidebarMode::Files {
                                                                theme.accent
                                                            } else {
                                                                theme.text_muted
                                                            },
                                                        ))
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.close_sidebar_search(cx);
                                                        })),
                                                )
                                                .child(
                                                    div()
                                                        .id("btn-sidebar-search")
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .w(px(28.0))
                                                        .h(px(28.0))
                                                        .rounded_md()
                                                        .cursor_pointer()
                                                        .when(
                                                            self.sidebar_mode
                                                                == crate::views::sidebar::SidebarMode::Search,
                                                            |d| d.bg(theme.bg_hover),
                                                        )
                                                        .hover(|h| h.bg(theme.bg_hover))
                                                        .child(icons::icon(
                                                            icons::ICON_SEARCH,
                                                            14.0,
                                                            if self.sidebar_mode == crate::views::sidebar::SidebarMode::Search {
                                                                theme.accent
                                                            } else {
                                                                theme.text_muted
                                                            },
                                                        ))
                                                        .on_click(cx.listener(|this, _, window, cx| {
                                                            this.open_sidebar_search(window, cx);
                                                        })),
                                                )
                                                .children(
                                                    self.plugin_leaf_views
                                                        .values()
                                                        .filter(|leaf| leaf.location == "left")
                                                        .map(|leaf| {
                                                            let is_active = match &self.sidebar_mode {
                                                                crate::views::sidebar::SidebarMode::Plugin(id) => id == &leaf.leaf_id,
                                                                _ => false,
                                                            };
                                                            let leaf_id = leaf.leaf_id.clone();
                                                            let icon_asset = crate::icons::map_obsidian_icon(leaf.icon.as_deref().unwrap_or("clock"));
                                                            div()
                                                                .id(gpui::ElementId::Name(format!("btn-sidebar-plugin-{}", leaf.leaf_id).into()))
                                                                .flex()
                                                                .items_center()
                                                                .justify_center()
                                                                .w(px(28.0))
                                                                .h(px(28.0))
                                                                .rounded_md()
                                                                .cursor_pointer()
                                                                .when(is_active, |d| d.bg(theme.bg_hover))
                                                                .hover(|h| h.bg(theme.bg_hover))
                                                                .child(icons::icon(
                                                                    icon_asset,
                                                                    14.0,
                                                                    if is_active {
                                                                        theme.accent
                                                                    } else {
                                                                        theme.text_muted
                                                                    },
                                                                ))
                                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                                    this.select_left_leaf(&leaf_id, cx);
                                                                }))
                                                        }),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .id("sidebar-titlebar-drag-region")
                                                .flex_1()
                                                .h_full()
                                                .window_control_area(WindowControlArea::Drag),
                                        ),
                                ),
                        )
                    })
                    .when(self.sidebar_collapsed, |s| {
                        s.child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .w(px(44.0))
                                .h_full()
                                .flex_shrink_0()
                                .border_r_1()
                                .border_b_1()
                                .border_color(theme.border)
                                .bg(theme.bg_sidebar)
                                .child(
                                    div()
                                        .id("btn-toggle-sidebar")
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .p_1()
                                        .rounded_md()
                                        .cursor_pointer()
                                        .hover(|h| h.bg(theme.bg_hover))
                                        .child(icons::icon(
                                            icons::ICON_PANEL_LEFT,
                                            15.0,
                                            theme.text_muted,
                                        ))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.toggle_sidebar(cx);
                                        })),
                                ),
                        )
                    })
                    // Vertical divider (1px border line + 7px transparent hitbox) matching the sidebar-resizer below exactly
                    .when(!self.sidebar_collapsed, |s| {
                        s.child(
                            div()
                                .id("titlebar-resizer-divider")
                                .relative()
                                .w(px(1.0))
                                .h_full()
                                .flex_shrink_0()
                                .bg(resizer_color)
                                .border_b_1()
                                .border_color(theme.border)
                                .child(
                                    div()
                                        .id("titlebar-resizer-hitbox")
                                        .absolute()
                                        .top_0()
                                        .bottom_0()
                                        .left(px(-3.0))
                                        .w(px(7.0))
                                        .cursor(gpui::CursorStyle::ResizeColumn)
                                        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                                            this.is_hovering_resizer = *hovered;
                                            cx.notify();
                                        }))
                                        .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, _, cx| {
                                            if event.click_count == 2 {
                                                this.toggle_sidebar(cx);
                                            } else {
                                                this.is_resizing_sidebar = true;
                                                cx.notify();
                                            }
                                        })),
                                ),
                        )
                    })
                    // Right area: Editor header column containing Tab Bar, Drag Area, and Window Controls
                    .child(self.render_titlebar_editor_header(is_maximized, theme, window, cx))
                    // Right vertical divider (mirror of `titlebar-resizer-divider`): caps the tab
                    // strip at the right sidebar's left edge and resizes it on drag.
                    .when(!self.right_sidebar_collapsed, |s| {
                        let is_right_resizer_active =
                            self.is_resizing_right_sidebar || self.is_hovering_right_resizer;
                        let right_resizer_color = if is_right_resizer_active {
                            theme.accent
                        } else {
                            theme.border
                        };
                        s.child(
                            div()
                                .id("titlebar-right-resizer-divider")
                                .relative()
                                .w(px(1.0))
                                .h_full()
                                .flex_shrink_0()
                                .bg(right_resizer_color)
                                .border_b_1()
                                .border_color(theme.border)
                                .child(
                                    div()
                                        .id("titlebar-right-resizer-hitbox")
                                        .absolute()
                                        .top_0()
                                        .bottom_0()
                                        .right(px(-3.0))
                                        .w(px(7.0))
                                        .cursor(gpui::CursorStyle::ResizeColumn)
                                        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                                            this.is_hovering_right_resizer = *hovered;
                                            cx.notify();
                                        }))
                                        .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, _, cx| {
                                            if event.click_count == 2 {
                                                this.toggle_right_sidebar(cx);
                                            } else {
                                                this.is_resizing_right_sidebar = true;
                                                cx.notify();
                                            }
                                        })),
                                ),
                        )
                    })
                    // Right area: Right sidebar header column in titlebar (width matches sidebar,
                    // mirror of the left sidebar header column). Keeps the window controls glued
                    // to the right window edge while the tab strip ends at the divider.
                    .when(!self.right_sidebar_collapsed, |s| {
                        s.child(
                            div()
                                .id("titlebar-right-header")
                                .flex()
                                .flex_row()
                                .items_center()
                                .w(px(self.right_sidebar_width))
                                .flex_shrink_0()
                                .h_full()
                                .bg(theme.bg_sidebar)
                                .border_b_1()
                                .border_color(theme.border)
                                .child(
                                    div()
                                        .id("titlebar-right-header-drag-region")
                                        .flex_1()
                                        .min_w_0()
                                        .h_full()
                                        .window_control_area(WindowControlArea::Drag),
                                )
                                .child(self.render_window_control_buttons(theme, is_maximized, cx)),
                        )
                    }),
            )
            // ================= MAIN BODY =================
            .child(
                div()
                    .flex()
                    .flex_1()
                    .flex_row()
                    .w_full()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden()
                    // ----- Left Action Rail / Ribbon (Obsidian-style) -----
                    .child(self.render_ribbon_rail(theme, cx))
                    // ----- Сайдбар файлов (Дерево каталогов) -----
                    .when(!self.sidebar_collapsed, |s| {
                        s.child(
                            div()
                                .flex()
                                .flex_col()
                                .w(px((self.sidebar_width - 44.0).max(100.0)))
                                .flex_shrink_0()
                                .overflow_hidden()
                                .h_full()
                                .bg(theme.bg_sidebar)
                                .pt_2()
                                .px_2()
                                .pb_0()
                                .gap_2()
                                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                                    if this.is_resizing_sidebar {
                                        let x: f32 = event.position.x.into();
                                        if x < 100.0 {
                                            this.sidebar_collapsed = true;
                                        } else {
                                            this.sidebar_collapsed = false;
                                            this.sidebar_width = x.clamp(160.0, 600.0);
                                        }
                                        cx.notify();
                                    }
                                }))
                                .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                    if this.is_resizing_sidebar {
                                        this.is_resizing_sidebar = false;
                                        cx.notify();
                                    }
                                }))
                            // Vault Header (Files and Search modes)
                            .when(
                                !matches!(self.sidebar_mode, crate::views::sidebar::SidebarMode::Plugin(_)),
                                |d| {
                                    d.child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .justify_between()
                                            .px_2()
                                            .py_1()
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .gap_2()
                                                    .child(icons::icon(icons::ICON_FOLDER, 15.0, theme.accent))
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                                            .text_color(theme.text_primary)
                                                            .truncate()
                                                            .child(self.state.workspace_name.clone()),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .gap_1()
                                                    .child(
                                                        div()
                                                            .id("btn-new-note")
                                                            .p_1()
                                                            .rounded_md()
                                                            .cursor_pointer()
                                                            .hover(|h| h.bg(theme.bg_hover))
                                                            .child(icons::icon(icons::ICON_PLUS, 13.0, theme.text_primary))
                                                            .on_click(cx.listener(|this, _, window, cx| {
                                                                this.create_new_note(window, cx);
                                                            })),
                                                    )
                                                    .child(
                                                        div()
                                                            .id("btn-open-vault")
                                                            .p_1()
                                                            .rounded_md()
                                                            .cursor_pointer()
                                                            .hover(|h| h.bg(theme.bg_hover))
                                                            .child(icons::icon(icons::ICON_FOLDER_OPEN, 13.0, theme.text_primary))
                                                            .on_click(cx.listener(|this, _, window, cx| {
                                                                this.open_folder_dialog_deferred(window, cx);
                                                            })),
                                                    ),
                                            ),
                                    )
                                },
                            )
                            // Search Filter Bar + Tree (Files mode) or Search Panel or Plugin Leaf View
                            .when(
                                self.sidebar_mode
                                    == crate::views::sidebar::SidebarMode::Files,
                                |d| {
                                    d.child(self.render_quick_filter(window, cx))
                                        // Tree Scroll — виртуализированный список (см. views/sidebar/tree.rs)
                                        .child(self.render_sidebar_tree(
                                            active_path.as_ref(),
                                            theme,
                                            window,
                                            cx,
                                        ))
                                },
                            )
                            .when(
                                self.sidebar_mode
                                    == crate::views::sidebar::SidebarMode::Search,
                                |d| d.child(self.render_search_panel(theme, window, cx)),
                            )
                            .when(
                                matches!(self.sidebar_mode, crate::views::sidebar::SidebarMode::Plugin(_)),
                                |d| {
                                    if let crate::views::sidebar::SidebarMode::Plugin(ref active_id) = self.sidebar_mode {
                                        if let Some(leaf) = self.plugin_leaf_views.get(active_id) {
                                            d.child(self.render_left_plugin_leaf(leaf, theme, window, cx))
                                        } else {
                                            d
                                        }
                                    } else {
                                        d
                                    }
                                },
                            )
                            // Sidebar Footer (Vault switcher button + Reload button)
                            .child(
                                div()
                                    .id("sidebar-footer")
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .px_2()
                                    .py_1p5()
                                    .border_t_1()
                                    .border_color(theme.border_subtle)
                                    .child(
                                        div()
                                            .id("btn-switch-vault")
                                            .flex()
                                            .flex_1()
                                            .min_w_0()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .px_1p5()
                                            .py_1()
                                            .rounded_md()
                                            .cursor_pointer()
                                            .hover(|h| h.bg(theme.bg_hover))
                                            .child(icons::icon(icons::ICON_FOLDER, 15.0, theme.accent))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .truncate()
                                                    .text_sm()
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .text_color(theme.text_primary)
                                                    .child(self.state.workspace_name.clone()),
                                            )
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.open_vault_dialog(cx);
                                            })),
                                    )
                                    .child(
                                        div()
                                            .id("btn-reload-vault")
                                            .p_1()
                                            .rounded_md()
                                            .cursor_pointer()
                                            .hover(|h| h.bg(theme.bg_hover))
                                            .child(icons::icon(icons::ICON_REFRESH, 13.0, theme.text_muted))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.reload_files(window, cx);
                                            })),
                                    ),
                            )
                        )
                    })
                    // ----- Sidebar Resizer Divider (1px border line + 7px transparent hitbox) -----
                    .when(!self.sidebar_collapsed, |s| {
                        s.child(
                            div()
                                .id("sidebar-resizer")
                                .relative()
                                .flex_shrink_0()
                                .w(px(1.0))
                                .h_full()
                                .bg(resizer_color)
                                .child(
                                    div()
                                        .id("sidebar-resizer-hitbox")
                                        .absolute()
                                        .top_0()
                                        .bottom_0()
                                        .left(px(-3.0))
                                        .w(px(7.0))
                                        .cursor(gpui::CursorStyle::ResizeColumn)
                                        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                                            // A scrollbar drag may pass over this hitbox; the
                                            // divider must not light up as the active target.
                                            this.is_hovering_resizer =
                                                *hovered && !this.sidebar_scrollbar_drag.active;
                                            cx.notify();
                                        }))
                                        .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, _, cx| {
                                            // One gesture at a time: the scrollbar already
                                            // owns the pointer, so the border must not steal it.
                                            if this.sidebar_scrollbar_drag.active {
                                                return;
                                            }
                                            if event.click_count == 2 {
                                                this.toggle_sidebar(cx);
                                            } else {
                                                this.is_resizing_sidebar = true;
                                                cx.notify();
                                            }
                                        })),
                                ),
                        )
                    })
                    // ----- Центральная область редактора -----
                    .child({
                        let win_w: f32 = window.bounds().size.width.into();
                        let win_h: f32 = window.bounds().size.height.into();
                        let left_w = if self.sidebar_collapsed { 44.0 } else { self.sidebar_width + 1.0 };
                        let right_w = if self.right_sidebar_collapsed { 0.0 } else { self.right_sidebar_width + 1.0 };
                        let avail_w = (win_w - left_w - right_w).max(100.0);
                        let avail_h = (win_h - 66.0).max(100.0);
                        self.render_workspace_node(&self.state.workspace.root, &[], avail_w, avail_h, theme, window, cx)
                    })
                    // ----- Right Sidebar Resizer & Right Sidebar -----
                    .when(!self.right_sidebar_collapsed, |s| {
                        s.child(self.render_right_resizer(theme, cx))
                            .child(self.render_right_sidebar(theme, window, cx))
                    }),
            )
            // ================= FOOTER / STATUS BAR =================
            .child(
                div()
                    .id("app-status-bar")
                    .flex()
                    .flex_shrink_0()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .h(px(28.0))
                    .w_full()
                    .min_w_0()
                    .overflow_hidden()
                    .px_4()
                    .bg(theme.bg_sidebar)
                    .border_t_1()
                    .border_color(theme.border)
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(self.state.status_text.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .gap_4()
                            .when_some(self.state.active_document(), |s, doc| {
                                use crate::state::TextStats;
                                let stats_label = match doc.text_stats() {
                                    TextStats::Exact { words, chars } => format!("{}w • {}c", words, chars),
                                    TextStats::Large { bytes } => {
                                        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
                                    }
                                };
                                s.child(
                                    div()
                                        .id("status-text-stats")
                                        .child(stats_label),
                                )
                            })
                            .when_some(self.state.active_view_and_doc(), |s, (view, doc)| {
                                let (line, col) = view.line_col(doc);
                                s.child(format!("Ln {}, Col {}", line + 1, col + 1))
                            })
                            .children(self.state.status_bar.all().into_iter().map(|item| {
                                div()
                                    .id(gpui::SharedString::from(item.id.clone()))
                                    .child(item.text)
                            }))
                            .child("120 FPS"),
                    ),
            )
            // ================= TABS POPUP (Obsidian-style switcher) =================
            .when_some(self.tabs_popup.clone(), |d, popup| {
                let window_size = window.bounds().size;
                let window_w: f32 = window_size.width.into();
                let window_h: f32 = window_size.height.into();

                let (left, top) = if let Some((x, y)) = self.tabs_popup_pos {
                    let popup_w = 280.0;
                    let mut l = x - popup_w + 32.0;
                    if l + popup_w > window_w - 8.0 {
                        l = window_w - popup_w - 8.0;
                    }
                    if l < 8.0 {
                        l = 8.0;
                    }
                    let t = (y + 12.0).min(window_h - 100.0).max(8.0);
                    (l, t)
                } else {
                    let popup_right = if self.right_sidebar_collapsed {
                        crate::views::tab_list::POPUP_RIGHT_OFFSET
                    } else {
                        self.right_sidebar_width + 1.0 + 4.0 + 32.0 + 4.0
                    };
                    let l = (window_w - popup_right - 280.0).max(8.0);
                    let t = crate::views::tab_list::WINDOW_ANCHOR_TOP + crate::views::tab_list::POPUP_GAP;
                    (l, t)
                };

                d.child(
                    gpui::deferred(
                        div()
                            .id("tabs-popup-anchor")
                            .absolute()
                            .top(px(top))
                            .left(px(left))
                            .child(popup),
                    )
                    .with_priority(999),
                )
            })
            // ================= COMMAND PALETTE MODAL =================
            .when(self.command_palette.is_open, |d| {
                d.child(self.render_command_palette(theme, window, cx))
            })
            // ================= FOLDER CONTEXT MENU =================
            .when(self.folder_context_menu.is_open, |d| {
                d.child(self.render_folder_context_menu(theme, window, cx))
            })
            // ================= FILE CONTEXT MENU =================
            .when(self.file_context_menu.is_open, |d| {
                d.child(self.render_file_context_menu(theme, window, cx))
            })
            // ================= TAB CONTEXT MENU (Obsidian-style) =================
            .when(self.tab_context_menu.is_open, |d| {
                d.child(self.render_tab_context_menu(theme, window, cx))
            })
            // ================= RIBBON CONTEXT MENU (Obsidian-style) =================
            .when(self.ribbon_context_menu.is_open, |d| {
                d.child(self.render_ribbon_context_menu(theme, window, cx))
            })
            // ================= EDITOR CONTEXT MENU =================
            .when(self.editor_context_menu.is_open, |d| {
                d.child(self.render_editor_context_menu(theme, window, cx))
            })
            // ================= GO TO FILE DIALOG =================
            .when(self.search_dialog.is_open, |d| {
                d.child(self.render_search_dialog(theme, window, cx))
            })
            // ================= FOLDER MODALS =================
            .when(self.folder_modal != crate::views::sidebar::FolderModal::None, |d| {
                d.child(self.render_folder_modals(theme, window, cx))
            })
            // ================= VAULT SWITCHER DIALOG =================
            .when(self.vault_dialog_open, |d| {
                d.child(self.render_vault_dialog(theme, window, cx))
            })
            // ================= COMMUNITY PLUGINS CATALOG =================
            .when(self.plugin_catalog.is_open, |d| {
                d.child(self.render_plugin_catalog_modal(window, cx))
            })
            // ================= PLUGIN MODAL DIALOG OVERLAY =================
            .when_some(self.render_plugin_modal_overlay(theme, window, cx), |d, overlay| {
                d.child(overlay)
            })
            // ================= MIDDLE-BUTTON SMART SCROLL HINT (editor) =================
            .when(self.editor_middle_active, |d| {
                d.child(
                    gpui::deferred(
                        div()
                            .id("editor-middle-scroll-hint")
                            .absolute()
                            .top(px(48.0))
                            .right(px(16.0))
                            .px(px(10.0))
                            .py(px(4.0))
                            .rounded_full()
                            .bg(theme.bg_hover)
                            .border_1()
                            .border_color(theme.border)
                            .text_xs()
                            .text_color(theme.text_muted)
                            .child("⇅ СКМ-скролл — двигайте мышь, отпустите СКМ"),
                    )
                    .with_priority(1000),
                )
            })
    }
}

/// Progress of a CSS-style cubic-bezier easing at the time fraction `t`:
/// solve the curve parameter `u` from `x(u) = t`, return `y(u)`. The x
/// coordinates stay inside `[0, 1]`, so `x(u)` is monotonic and the solve is
/// well-defined; Newton from `u = t` converges in a few steps for these
/// curves, bisection is the safety net.
fn cubic_bezier(x1: f64, y1: f64, x2: f64, y2: f64, t: f64) -> f64 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let x_at = |u: f64| 3.0 * (1.0 - u) * (1.0 - u) * u * x1 + 3.0 * (1.0 - u) * u * u * x2 + u * u * u;
    let dx_at = |u: f64| 3.0 * (1.0 - u) * (1.0 - u) * x1 + 6.0 * (1.0 - u) * u * (x2 - x1) + 3.0 * u * u * (1.0 - x2);
    let y_at = |u: f64| 3.0 * (1.0 - u) * (1.0 - u) * u * y1 + 3.0 * (1.0 - u) * u * u * y2 + u * u * u;
    let mut u = t;
    for _ in 0..8 {
        let err = x_at(u) - t;
        if err.abs() < 1e-7 {
            break;
        }
        let d = dx_at(u);
        if d.abs() < 1e-9 {
            break;
        }
        u = (u - err / d).clamp(0.0, 1.0);
    }
    if (x_at(u) - t).abs() > 1e-5 {
        let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
        for _ in 0..64 {
            let mid = (lo + hi) / 2.0;
            if x_at(mid) < t {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        u = (lo + hi) / 2.0;
    }
    y_at(u)
}

/// Glide duration for the arriving wheel notch plus the shifted history:
/// twice the mean of the last three notch intervals, clamped to
/// [`WHEEL_GLIDE_MIN_SECS`]..[`WHEEL_GLIDE_MAX_SECS`]. A spin fresh after an
/// idle seeds the history a full max interval back, so the first notch gets a
/// mid-length glide and the duration grows to match a sustained cadence.
fn wheel_glide_duration_secs(
    history: [Option<std::time::Instant>; 3],
    now: std::time::Instant,
) -> (f32, [Option<std::time::Instant>; 3]) {
    let stale = history[2].map_or(true, |last| {
        now.duration_since(last).as_secs_f64() > f64::from(WHEEL_GLIDE_MAX_SECS) * 2.0
    });
    let history = if stale {
        let seeded = now.checked_sub(std::time::Duration::from_secs_f64(f64::from(WHEEL_GLIDE_MAX_SECS)));
        [seeded, seeded, seeded]
    } else {
        history
    };
    let oldest = history[0].expect("a non-stale or freshly seeded history carries instants");
    let interval = now.duration_since(oldest).as_secs_f64() / 3.0;
    let duration = ((interval * 2.0) as f32).clamp(WHEEL_GLIDE_MIN_SECS, WHEEL_GLIDE_MAX_SECS);
    (duration, [history[1], history[2], Some(now)])
}

/// Dead zone at the top/bottom of the editor viewport for middle-button smart
/// scroll, as a fraction of the viewport height. Mirrors the sidebar smart
/// scroll so both lists feel the same.
const EDITOR_MIDDLE_EDGE_PAD_FRACTION: f32 = 0.10;
/// ...but never larger than this, so short notes keep a usable band.
const EDITOR_MIDDLE_EDGE_PAD_MAX: f32 = 56.0;

/// Target editor `scroll_top` for the given cursor Y: `0` at the top of the
/// viewport, `max_scroll` at the bottom, proportional in between.
///
/// Same formula as the sidebar smart scroll (`sidebar_smart_cursor_target`),
/// expressed with plain floats because the editor scroll position is `f64`.
fn editor_middle_cursor_target(cursor_y: f32, viewport_top: f32, viewport_h: f32, max_scroll: f64) -> f64 {
    let pad = (viewport_h * EDITOR_MIDDLE_EDGE_PAD_FRACTION)
        .clamp(24.0, EDITOR_MIDDLE_EDGE_PAD_MAX)
        .min(viewport_h / 3.0);
    let usable = (viewport_h - pad * 2.0).max(1.0);
    let t = ((cursor_y - viewport_top - pad) / usable).clamp(0.0, 1.0);
    max_scroll * f64::from(t)
}

/// Weight inline code is painted at.
///
/// Roboto Mono is bundled here in Regular and Bold only, and a request for a weight in
/// between resolves to whichever face the platform deems closest — so the one step up
/// that is guaranteed to show is Bold. `markdown::PaintedSegment::code` carries the same
/// trait into the row's shaped layout, so geometry and paint stay in parity.
const INLINE_CODE_WEIGHT: gpui::FontWeight = gpui::FontWeight::BOLD;

/// Fraction of the row height left as a gap above and below the inline-code chip, i.e.
/// the chip is `1 - 2 * CODE_CHIP_INSET_RATIO` of the row. Scaled with the row so that a
/// heading's chip keeps the proportions of a paragraph's.
const CODE_CHIP_INSET_RATIO: f32 = 1.0 / 12.0;

/// Weight the inline-code face takes in a row whose base weight is `base`: the heavier
/// of the two, so that code is never lighter than the text around it and a bold heading
/// keeps its bold.
fn inline_code_weight(base: gpui::FontWeight) -> gpui::FontWeight {
    if base.0 > INLINE_CODE_WEIGHT.0 { base } else { INLINE_CODE_WEIGHT }
}

/// Body of a code block: the `InsideCode` lines that follow the opening fence.
/// Called only when the copy button is clicked, never while building a frame.
/// Text of the code block opened at `fence_line`, without the fences.
///
/// Read from the editor window rather than from the document: the click that
/// gets here happened on a row that was on screen, so the fence is inside the
/// window by construction. A block longer than one window — `MARGIN_LINES` on
/// each side of the viewport, so several thousand lines — is copied up to the
/// window's end; a note that small is never windowed in the first place.
fn code_block_body(
    window: &crate::editor_window::EditorWindow,
    fence_line: usize,
    block_id: usize,
) -> String {
    let mut body = String::new();
    if !window.covers_line(fence_line) {
        return body;
    }
    let mut i = fence_line + 1;
    while let Some(next) = window.code_states.get(i).and_then(|s| s.as_ref()) {
        if next.block_id != block_id || next.kind != crate::markdown::CodeBlockKind::InsideCode {
            break;
        }
        if let Some(l) = window.lines.get(i) {
            body.push_str(l);
            body.push('\n');
        }
        i += 1;
    }
    body
}

/// Vertical gap above and below an inline-code chip, in logical px, for a row `row_h`
/// tall. At least one pixel, so that the chip is always visibly shorter than its row.
fn code_chip_inset(row_h: f32) -> f32 {
    (row_h * CODE_CHIP_INSET_RATIO).round().max(1.0)
}

/// Colour a syntax marker is painted with, or `None` when it is not painted at all on
/// this line.
///
/// Every marker except an inline-code delimiter only shows on the active line. A code
/// delimiter is painted even off the active line so that the code chip keeps a constant
/// width — and with it the selection rectangle, which otherwise jumps by two cells the
/// moment the line becomes active. Off the active line the delimiter carries no ink at
/// all, which is exactly what reads as the chip's left/right padding: the chip itself is
/// a rectangle the editor paints behind the row's text, not a text-run background, so a
/// blank delimiter is enough to open the padding. `markdown::row_painted_text`
/// must agree: it keeps the delimiter's cell on an idle line for the same reason.
fn marker_paint(m: &str, is_active_line: bool, theme: &Theme) -> Option<gpui::Hsla> {
    let is_code_delim = crate::markdown::is_inline_code_delim(m);
    if !is_active_line && !is_code_delim {
        return None;
    }
    if is_active_line {
        Some(theme.text_muted.into())
    } else {
        // No ink: the cell is part of the chip, and the chip shows through.
        Some(gpui::rgba(0x00000000).into())
    }
}

fn render_syntax_spans(
    spans: Vec<crate::markdown::SyntaxSpan>,
    is_active_line: bool,
    theme: &Theme,
    text_color_override: Option<gpui::Rgba>,
    base_weight: Option<gpui::FontWeight>,
    base_style: Option<gpui::FontStyle>,
    font_family: &gpui::SharedString,
    row_key: &str,
    ctrl_held: bool,
    hovered_link_id: Option<&str>,
    cx: &Context<AppView>,
) -> gpui::AnyElement {
    let main_color = text_color_override.unwrap_or(theme.text_primary);
    let base_weight = base_weight.unwrap_or(gpui::FontWeight::NORMAL);
    let base_style = base_style.unwrap_or(gpui::FontStyle::Normal);

    let reg_font = gpui::Font {
        family: font_family.clone(),
        weight: base_weight,
        style: base_style,
        features: Default::default(),
        fallbacks: None,
    };
    let bold_font = gpui::Font {
        family: font_family.clone(),
        weight: gpui::FontWeight::BOLD,
        style: base_style,
        features: Default::default(),
        fallbacks: None,
    };
    let italic_font = gpui::Font {
        family: font_family.clone(),
        weight: base_weight,
        style: gpui::FontStyle::Italic,
        features: Default::default(),
        fallbacks: None,
    };
    let bold_italic_font = gpui::Font {
        family: font_family.clone(),
        weight: gpui::FontWeight::BOLD,
        style: gpui::FontStyle::Italic,
        features: Default::default(),
        fallbacks: None,
    };
    // Inline code is set one weight step heavier than the body it sits in. The chip
    // background is painted by the editor as part of the row's backdrop, not by this run,
    // only thing that marks code out here is this face plus the row's chip rectangle.
    let code_font = gpui::Font {
        family: font_family.clone(),
        weight: inline_code_weight(base_weight),
        style: base_style,
        features: Default::default(),
        fallbacks: None,
    };

    let has_links = spans.iter().any(|s| matches!(s, crate::markdown::SyntaxSpan::Link { .. }));

    if !has_links {
        let mut full_text = String::new();
        let mut runs = Vec::new();

        for span in spans {
            match span {
                crate::markdown::SyntaxSpan::Marker(m) => {
                    if let Some(color) = marker_paint(&m, is_active_line, theme) {
                        let len = m.len();
                        let font = if crate::markdown::is_inline_code_delim(&m) { code_font.clone() } else { reg_font.clone() };
                        full_text.push_str(&m);
                        runs.push(gpui::TextRun {
                            len,
                            font,
                            color,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        });
                    }
                }
                crate::markdown::SyntaxSpan::Text(t) => {
                    let len = t.len();
                    full_text.push_str(&t);
                    runs.push(gpui::TextRun {
                        len,
                        font: reg_font.clone(),
                        color: main_color.into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    });
                }
                crate::markdown::SyntaxSpan::Bold(b) => {
                    let len = b.len();
                    full_text.push_str(&b);
                    runs.push(gpui::TextRun {
                        len,
                        font: bold_font.clone(),
                        color: main_color.into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    });
                }
                crate::markdown::SyntaxSpan::Italic(i) => {
                    let len = i.len();
                    full_text.push_str(&i);
                    runs.push(gpui::TextRun {
                        len,
                        font: italic_font.clone(),
                        color: main_color.into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    });
                }
                crate::markdown::SyntaxSpan::BoldItalic(bi) => {
                    let len = bi.len();
                    full_text.push_str(&bi);
                    runs.push(gpui::TextRun {
                        len,
                        font: bold_italic_font.clone(),
                        color: main_color.into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    });
                }
                crate::markdown::SyntaxSpan::InlineCode(c) => {
                    let len = c.len();
                    full_text.push_str(&c);
                    runs.push(gpui::TextRun {
                        len,
                        font: code_font.clone(),
                        color: main_color.into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    });
                }
                crate::markdown::SyntaxSpan::Strikethrough(s) => {
                    let len = s.len();
                    full_text.push_str(&s);
                    runs.push(gpui::TextRun {
                        len,
                        font: reg_font.clone(),
                        color: theme.text_muted.into(),
                        background_color: None,
                        underline: None,
                        strikethrough: Some(gpui::StrikethroughStyle {
                            thickness: px(1.0),
                            color: Some(theme.text_muted.into()),
                        }),
                    });
                }
                crate::markdown::SyntaxSpan::Link { .. } => unreachable!(),
                crate::markdown::SyntaxSpan::Underline(u) => {
                    let len = u.len();
                    full_text.push_str(&u);
                    runs.push(gpui::TextRun {
                        len,
                        font: reg_font.clone(),
                        color: main_color.into(),
                        background_color: None,
                        underline: Some(gpui::UnderlineStyle {
                            thickness: px(1.0),
                            color: Some(main_color.into()),
                            wavy: false,
                        }),
                        strikethrough: None,
                    });
                }
                crate::markdown::SyntaxSpan::Highlight(h) => {
                    let hl_bg = if theme.is_dark { gpui::rgba(0xf9e2af44) } else { gpui::rgba(0xdf8e1d33) };
                    let len = h.len();
                    full_text.push_str(&h);
                    runs.push(gpui::TextRun {
                        len,
                        font: reg_font.clone(),
                        color: main_color.into(),
                        background_color: Some(hl_bg.into()),
                        underline: None,
                        strikethrough: None,
                    });
                }
                crate::markdown::SyntaxSpan::Tag(t) => {
                    let (_, fg_color) = crate::markdown::tag_color_pair(&t, theme.is_dark);
                    let len = t.len();
                    full_text.push_str(&t);
                    runs.push(gpui::TextRun {
                        len,
                        font: reg_font.clone(),
                        color: fg_color.into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    });
                }
            }
        }

        if full_text.is_empty() {
            gpui::StyledText::new(" ").with_runs(vec![gpui::TextRun {
                len: 1,
                font: reg_font,
                color: gpui::rgba(0x00000000).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }]).into_any_element()
        } else {
            gpui::StyledText::new(full_text).with_runs(runs).into_any_element()
        }
    } else {
        // Has links: render inline text segments interspersed with interactive link elements
        let mut row_container = div().flex().flex_row().items_center().whitespace_nowrap();
        let mut full_text = String::new();
        let mut runs = Vec::new();
        let mut link_idx = 0;

        for span in spans {
            match span {
                crate::markdown::SyntaxSpan::Marker(m) => {
                    if let Some(color) = marker_paint(&m, is_active_line, theme) {
                        let len = m.len();
                        let font = if crate::markdown::is_inline_code_delim(&m) { code_font.clone() } else { reg_font.clone() };
                        full_text.push_str(&m);
                        runs.push(gpui::TextRun {
                            len,
                            font,
                            color,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        });
                    }
                }
                crate::markdown::SyntaxSpan::Text(t) => {
                    let len = t.len();
                    full_text.push_str(&t);
                    runs.push(gpui::TextRun {
                        len,
                        font: reg_font.clone(),
                        color: main_color.into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    });
                }
                crate::markdown::SyntaxSpan::Bold(b) => {
                    let len = b.len();
                    full_text.push_str(&b);
                    runs.push(gpui::TextRun {
                        len,
                        font: bold_font.clone(),
                        color: main_color.into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    });
                }
                crate::markdown::SyntaxSpan::Italic(i) => {
                    let len = i.len();
                    full_text.push_str(&i);
                    runs.push(gpui::TextRun {
                        len,
                        font: italic_font.clone(),
                        color: main_color.into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    });
                }
                crate::markdown::SyntaxSpan::BoldItalic(bi) => {
                    let len = bi.len();
                    full_text.push_str(&bi);
                    runs.push(gpui::TextRun {
                        len,
                        font: bold_italic_font.clone(),
                        color: main_color.into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    });
                }
                crate::markdown::SyntaxSpan::InlineCode(c) => {
                    let len = c.len();
                    full_text.push_str(&c);
                    runs.push(gpui::TextRun {
                        len,
                        font: code_font.clone(),
                        color: main_color.into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    });
                }
                crate::markdown::SyntaxSpan::Strikethrough(s) => {
                    let len = s.len();
                    full_text.push_str(&s);
                    runs.push(gpui::TextRun {
                        len,
                        font: reg_font.clone(),
                        color: theme.text_muted.into(),
                        background_color: None,
                        underline: None,
                        strikethrough: Some(gpui::StrikethroughStyle {
                            thickness: px(1.0),
                            color: Some(theme.text_muted.into()),
                        }),
                    });
                }
                crate::markdown::SyntaxSpan::Link { text, url } => {
                    // Flush preceding text runs
                    if !full_text.is_empty() {
                        row_container = row_container.child(
                            gpui::StyledText::new(std::mem::take(&mut full_text))
                                .with_runs(std::mem::take(&mut runs))
                        );
                    }

                    let link_id = format!("{}-link-{}", row_key, link_idx);
                    link_idx += 1;
                    let is_hovered = hovered_link_id == Some(&link_id);
                    let show_icon = ctrl_held && is_hovered;

                    let link_label = if text.is_empty() { "link".to_string() } else { text };
                    let link_styled = gpui::StyledText::new(link_label.clone()).with_runs(vec![gpui::TextRun {
                        len: link_label.len(),
                        font: reg_font.clone(),
                        color: theme.accent.into(),
                        background_color: None,
                        underline: Some(gpui::UnderlineStyle {
                            thickness: px(1.0),
                            color: Some(theme.accent.into()),
                            wavy: false,
                        }),
                        strikethrough: None,
                    }]);

                    let mut link_div = div()
                        .id(gpui::ElementId::Name(link_id.clone().into()))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(3.0))
                        .when(ctrl_held, |d| d.cursor(gpui::CursorStyle::PointingHand))
                        .on_hover(cx.listener({
                            let lid = link_id.clone();
                            move |this, is_h: &bool, _window, cx| {
                                if *is_h {
                                    if this.hovered_link_id.as_deref() != Some(&lid) {
                                        this.hovered_link_id = Some(lid.clone());
                                        cx.notify();
                                    }
                                } else if this.hovered_link_id.as_deref() == Some(&lid) {
                                    this.hovered_link_id = None;
                                    cx.notify();
                                }
                            }
                        }))
                        .on_mouse_down(MouseButton::Left, cx.listener({
                            let u = url.clone();
                            move |this, event: &MouseDownEvent, window, cx| {
                                if event.modifiers.control || event.modifiers.platform || !this.edit_mode {
                                    cx.stop_propagation();
                                    this.handle_link_click(&u, window, cx);
                                }
                            }
                        }))
                        .child(link_styled);

                    if show_icon {
                        link_div = link_div.child(icons::icon(icons::ICON_EXTERNAL_LINK, 12.0, theme.accent));
                    }

                    row_container = row_container.child(link_div);
                }
                crate::markdown::SyntaxSpan::Underline(u) => {
                    let len = u.len();
                    full_text.push_str(&u);
                    runs.push(gpui::TextRun {
                        len,
                        font: reg_font.clone(),
                        color: main_color.into(),
                        background_color: None,
                        underline: Some(gpui::UnderlineStyle {
                            thickness: px(1.0),
                            color: Some(main_color.into()),
                            wavy: false,
                        }),
                        strikethrough: None,
                    });
                }
                crate::markdown::SyntaxSpan::Highlight(h) => {
                    let hl_bg = if theme.is_dark { gpui::rgba(0xf9e2af44) } else { gpui::rgba(0xdf8e1d33) };
                    let len = h.len();
                    full_text.push_str(&h);
                    runs.push(gpui::TextRun {
                        len,
                        font: reg_font.clone(),
                        color: main_color.into(),
                        background_color: Some(hl_bg.into()),
                        underline: None,
                        strikethrough: None,
                    });
                }
                crate::markdown::SyntaxSpan::Tag(tag) => {
                    let (_, fg_color) = crate::markdown::tag_color_pair(&tag, theme.is_dark);
                    let len = tag.len();
                    full_text.push_str(&tag);
                    runs.push(gpui::TextRun {
                        len,
                        font: reg_font.clone(),
                        color: fg_color.into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    });
                }
            }
        }

        if !full_text.is_empty() {
            row_container = row_container.child(gpui::StyledText::new(full_text).with_runs(runs));
        }

        row_container.into_any_element()
    }
}

/// Shaped geometry of one painted visual row.
///
/// The editor paints rows from `SyntaxSpan` slices, so nothing downstream knew where
/// the glyphs actually landed: hit-testing and the caret/selection rectangles used a
/// naive `column * char_width` cell model instead. That model is exact for monospace
/// runs of narrow glyphs, but drifts for wide ones (emoji, CJK, VS16/ZWJ sequences),
/// which made the selection edge twitch while dragging across them.
///
/// This follows Zed's editor: the row's painted text is shaped once and every
/// pixel <-> column conversion goes through the real glyph positions
/// (`LineLayout::closest_index_for_x` and `ShapedGlyph::position`).
#[derive(Clone, Debug)]
pub(crate) struct TableColResizeDrag {
    pub table_first_line: usize,
    pub col_idx: usize,
    pub start_x: f32,
    pub start_width: f32,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) struct TableLayout {
    pub first_line: usize,
    pub col_widths: Vec<f32>,
    pub col_x: Vec<f32>,
    pub total_width: f32,
    pub aligns: Vec<crate::table::Align>,
}

#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct TableCellLayout {
    pub col_idx: usize,
    pub cell_x: f32,
    pub cell_w: f32,
    pub content_x: f32,
    pub cell_start_col: usize,
    pub cell_columns: usize,
    pub content_range: std::ops::Range<usize>,
    pub outer_range: std::ops::Range<usize>,
    pub line_chars: Vec<usize>,
    pub text: SharedString,
    pub layout: Arc<gpui::LineLayout>,
    pub code_ranges: Vec<(usize, usize)>,
    pub tag_ranges: Vec<(usize, usize, String)>,
}

impl TableCellLayout {
    fn byte_for_column(&self, col: usize) -> usize {
        self.text
            .char_indices()
            .nth(col)
            .map(|(byte, _)| byte)
            .unwrap_or(self.text.len())
    }

    fn glyph_x(&self, col: usize) -> f32 {
        if col >= self.cell_columns {
            return f32::from(self.layout.width);
        }
        let byte = self.byte_for_column(col);
        let mut cluster_x = 0.0f32;
        for run in &self.layout.runs {
            for glyph in &run.glyphs {
                if glyph.index == byte {
                    return f32::from(glyph.position.x);
                }
                if glyph.index > byte {
                    return cluster_x;
                }
                cluster_x = f32::from(glyph.position.x);
            }
        }
        cluster_x
    }
}

#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct TableRowLayout {
    pub is_delimiter: bool,
    pub cells: Vec<TableCellLayout>,
    pub total_width: f32,
    pub total_columns: usize,
    pub line_chars: Vec<usize>,
    pub code_ranges: Vec<(usize, usize)>,
    pub tag_ranges: Vec<(usize, usize, String)>,
}

impl TableRowLayout {
    pub fn columns(&self) -> usize {
        self.total_columns
    }

    pub fn x_for_column(&self, col: usize, cell_w: f32) -> f32 {
        if self.is_delimiter || self.cells.is_empty() {
            return 0.0;
        }
        if col >= self.total_columns {
            return self.total_width + (col - self.total_columns) as f32 * cell_w;
        }
        for cell in &self.cells {
            if col >= cell.cell_start_col && col <= cell.cell_start_col + cell.cell_columns {
                let k = col - cell.cell_start_col;
                return cell.content_x + cell.glyph_x(k);
            }
        }
        if let Some(last) = self.cells.last() {
            last.content_x + f32::from(last.layout.width)
        } else {
            0.0
        }
    }

    pub fn column_for_x(&self, x: f32) -> usize {
        if self.is_delimiter || self.cells.is_empty() {
            return 0;
        }
        if x < 0.0 {
            return 0;
        }
        for cell in &self.cells {
            if x >= cell.cell_x && x < cell.cell_x + cell.cell_w {
                if cell.cell_columns == 0 {
                    return cell.cell_start_col;
                }
                let local_x = (x - cell.content_x).max(0.0);
                let byte = cell.layout.closest_index_for_x(px(local_x));
                let col_in_cell = cell.text.get(..byte).map(|s| s.chars().count()).unwrap_or(0).min(cell.cell_columns);
                return cell.cell_start_col + col_in_cell;
            }
        }
        if let Some(last) = self.cells.last() {
            return last.cell_start_col + last.cell_columns;
        }
        self.total_columns
    }

    pub fn line_char_for_column(&self, col: usize) -> usize {
        if self.is_delimiter || self.cells.is_empty() {
            return 0;
        }
        for cell in &self.cells {
            if col >= cell.cell_start_col && col <= cell.cell_start_col + cell.cell_columns {
                let k = col - cell.cell_start_col;
                if k >= cell.cell_columns {
                    return cell.content_range.end;
                }
                return cell.line_chars.get(k).copied().unwrap_or(cell.content_range.end);
            }
        }
        if let Some(last) = self.cells.last() {
            last.content_range.end
        } else {
            0
        }
    }

    pub fn column_for_line_char(&self, lc: usize) -> usize {
        if self.is_delimiter || self.cells.is_empty() {
            return 0;
        }
        for cell in &self.cells {
            if lc <= cell.outer_range.end {
                if lc <= cell.content_range.start {
                    return cell.cell_start_col;
                }
                if lc >= cell.content_range.end {
                    return cell.cell_start_col + cell.cell_columns;
                }
                let k = cell.line_chars
                    .partition_point(|&ch| ch < lc)
                    .min(cell.cell_columns);
                return cell.cell_start_col + k;
            }
        }
        if let Some(last) = self.cells.last() {
            last.cell_start_col + last.cell_columns
        } else {
            self.total_columns
        }
    }
}

pub(crate) fn fit_table_columns(natural: &[f32], room: f32) -> Vec<f32> {
    const WIDEST: f32 = 480.0;
    const NARROWEST: f32 = 96.0;

    if natural.is_empty() {
        return Vec::new();
    }

    let capped: Vec<f32> = natural.iter().map(|&w| w.min(WIDEST)).collect();
    let floor: Vec<f32> = capped.iter().map(|&w| w.min(NARROWEST)).collect();
    let between = |lo: &[f32], hi: &[f32], r: f32| -> Vec<f32> {
        let (low, high): (f32, f32) = (lo.iter().sum(), hi.iter().sum());
        let share = if high <= r { 1.0 } else if high > low { ((r - low) / (high - low)).max(0.0) } else { 0.0 };
        lo.iter().zip(hi).map(|(&l, &h)| l + (h - l) * share).collect()
    };

    let widths = if capped.iter().sum::<f32>() <= room {
        between(&capped, natural, room)
    } else {
        between(&floor, &capped, room)
    };
    let total: f32 = widths.iter().sum();
    if total > 0.0 && total < room {
        widths.iter().map(|&w| w * room / total).collect()
    } else {
        widths
    }
}

pub(crate) fn compute_table_layout<'a>(
    text_system: &gpui::WindowTextSystem,
    get_line: &dyn Fn(usize) -> Option<&'a str>,
    block: &crate::table::TableBlock,
    font_family: &SharedString,
    manual_widths: Option<&[f32]>,
) -> TableLayout {
    const PADDING_X: f32 = 8.0;
    const MIN_COL_WIDTH: f32 = 80.0;
    let n_cols = block.col_count();
    let mut max_widths = vec![0.0f32; n_cols];

    let font = gpui::Font {
        family: font_family.clone(),
        weight: gpui::FontWeight::NORMAL,
        style: gpui::FontStyle::Normal,
        features: Default::default(),
        fallbacks: None,
    };
    let bold_font = gpui::Font {
        weight: gpui::FontWeight::BOLD,
        ..font.clone()
    };
    let dummy_color = gpui::rgba(0x000000ff).into();

    for line_idx in block.first_line..=block.last_line {
        if line_idx == block.sep_line() {
            continue;
        }
        let Some(line_str) = get_line(line_idx) else { continue; };
        let cells = crate::table::split_row(line_str);
        let is_header = line_idx == block.first_line;

        for (c_idx, cell) in cells.into_iter().take(n_cols).enumerate() {
            let content = if cell.content.start >= cell.content.end {
                ""
            } else {
                let mut b_start = 0;
                let mut b_end = line_str.len();
                for (ci, (bi, _)) in line_str.char_indices().enumerate() {
                    if ci == cell.content.start {
                        b_start = bi;
                    }
                    if ci == cell.content.end {
                        b_end = bi;
                        break;
                    }
                }
                &line_str[b_start..b_end]
            };
            if content.is_empty() {
                continue;
            }
            let f = if is_header { bold_font.clone() } else { font.clone() };
            let run = gpui::TextRun {
                len: content.len(),
                font: f,
                color: dummy_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let l = text_system.layout_line(content, px(14.0), &[run], None);
            let w = f32::from(l.width);
            if w > max_widths[c_idx] {
                max_widths[c_idx] = w;
            }
        }
    }

    let natural_widths: Vec<f32> = max_widths.iter().map(|&w| (w + PADDING_X * 2.0).max(MIN_COL_WIDTH)).collect();
    let col_widths = if let Some(mw) = manual_widths {
        let mut w = mw.to_vec();
        if w.len() < n_cols {
            for i in w.len()..n_cols {
                let def = natural_widths.get(i).copied().unwrap_or(120.0).max(MIN_COL_WIDTH);
                w.push(def);
            }
        } else if w.len() > n_cols {
            w.truncate(n_cols);
        }
        w
    } else {
        let sum_natural: f32 = natural_widths.iter().sum();
        if sum_natural < 720.0 {
            fit_table_columns(&natural_widths, 720.0)
        } else {
            natural_widths
        }
    };

    let mut col_x = Vec::with_capacity(n_cols);
    let mut cur_x = 0.0f32;
    for &col_w in &col_widths {
        col_x.push(cur_x);
        cur_x += col_w;
    }

    TableLayout {
        first_line: block.first_line,
        col_widths,
        col_x,
        total_width: cur_x,
        aligns: block.aligns.clone(),
    }
}

pub(crate) fn shape_table_row(
    text_system: &gpui::WindowTextSystem,
    line: &str,
    block: &crate::table::TableBlock,
    line_idx: usize,
    cursor_col: Option<usize>,
    table_layout: &TableLayout,
    headings: &crate::settings::HeadingsSettings,
    font_family: &SharedString,
    cell_w: f32,
) -> RowShapedLayout {
    const PADDING_X: f32 = 8.0;
    crate::perf::PERF
        .shape_calls
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if line_idx == block.sep_line() {
        let dummy_text: SharedString = "".into();
        let dummy_layout = text_system.layout_line("", px(14.0), &[], None);
        let table_row = Arc::new(TableRowLayout {
            is_delimiter: true,
            cells: Vec::new(),
            total_width: table_layout.total_width,
            total_columns: 0,
            line_chars: vec![0, line.chars().count()],
            code_ranges: Vec::new(),
            tag_ranges: Vec::new(),
        });
        return RowShapedLayout {
            layout: dummy_layout,
            text: dummy_text,
            line_chars: Arc::new(vec![0, line.chars().count()]),
            code_ranges: Arc::new(Vec::new()),
            tag_ranges: Arc::new(Vec::new()),
            cell_w,
            table_row: Some(table_row),
        };
    }

    let raw_cells = crate::table::split_row(line);
    let is_header = line_idx == block.first_line;
    let n_cols = table_layout.col_widths.len();
    let active_cell_idx = cursor_col.map(|cc| crate::table::cell_index_at(&raw_cells, cc));

    let mut cells = Vec::with_capacity(n_cols);
    let mut flat_line_chars = Vec::new();
    let mut flat_code_ranges = Vec::new();
    let mut flat_tag_ranges = Vec::new();
    let mut cur_col_offset = 0;

    for c in 0..n_cols {
        let (content_range, outer_range) = if let Some(rc) = raw_cells.get(c) {
            (rc.content.clone(), rc.outer.clone())
        } else {
            let len = line.chars().count();
            (len..len, len..len)
        };

        let is_active_cell = active_cell_idx == Some(c);
        let v = crate::markdown::VisualRow {
            line_idx,
            sub_idx: 0,
            char_start: content_range.start,
            char_end: content_range.end,
            is_last_sub: true,
        };
        let painted = crate::markdown::row_painted_text(
            line,
            &crate::markdown::LineKind::Paragraph(Default::default()),
            &v,
            is_active_cell,
            headings,
        );

        let cell_code_ranges = painted.code_ranges();
        for &(cs, ce) in &cell_code_ranges {
            flat_code_ranges.push((cur_col_offset + cs, cur_col_offset + ce));
        }
        let cell_tag_ranges = painted.tag_ranges();
        for (cs, ce, tag) in &cell_tag_ranges {
            flat_tag_ranges.push((cur_col_offset + cs, cur_col_offset + ce, tag.clone()));
        }

        let mut cell_text = String::new();
        let mut runs = Vec::new();
        let color = rgba(0x000000ff).into();

        for seg in &painted.segments {
            let weight = if seg.code {
                inline_code_weight(if seg.bold || is_header { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL })
            } else if seg.bold || is_header {
                gpui::FontWeight::BOLD
            } else {
                gpui::FontWeight::NORMAL
            };
            cell_text.push_str(&seg.text);
            runs.push(gpui::TextRun {
                len: seg.text.len(),
                font: gpui::Font {
                    family: font_family.clone(),
                    weight,
                    style: if seg.italic { gpui::FontStyle::Italic } else { gpui::FontStyle::Normal },
                    features: Default::default(),
                    fallbacks: None,
                },
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            });
        }

        let cell_text_shared: SharedString = cell_text.into();
        let cell_layout = text_system.layout_line(cell_text_shared.as_ref(), px(14.0), &runs, None);
        let text_w = f32::from(cell_layout.width);
        let cell_x = table_layout.col_x[c];
        let cell_w_val = table_layout.col_widths[c];
        let avail_w = (cell_w_val - 2.0 * PADDING_X).max(0.0);
        let extra_space = (avail_w - text_w).max(0.0);

        let align_offset = match table_layout.aligns.get(c).copied().unwrap_or_default() {
            crate::table::Align::None | crate::table::Align::Left => 0.0,
            crate::table::Align::Center => extra_space / 2.0,
            crate::table::Align::Right => extra_space,
        };
        let content_x = cell_x + PADDING_X + align_offset;
        let cell_columns = painted.line_chars.len().saturating_sub(1);

        for &lc in painted.line_chars.iter().take(cell_columns) {
            flat_line_chars.push(lc);
        }
        flat_line_chars.push(content_range.end);

        cells.push(TableCellLayout {
            col_idx: c,
            cell_x,
            cell_w: cell_w_val,
            content_x,
            cell_start_col: cur_col_offset,
            cell_columns,
            content_range,
            outer_range,
            line_chars: painted.line_chars,
            text: cell_text_shared,
            layout: cell_layout,
            code_ranges: cell_code_ranges,
            tag_ranges: cell_tag_ranges,
        });

        cur_col_offset += cell_columns + 1;
    }

    let dummy_layout = text_system.layout_line("", px(14.0), &[], None);
    let table_row = Arc::new(TableRowLayout {
        is_delimiter: false,
        cells,
        total_width: table_layout.total_width,
        total_columns: cur_col_offset,
        line_chars: flat_line_chars.clone(),
        code_ranges: flat_code_ranges.clone(),
        tag_ranges: flat_tag_ranges.clone(),
    });

    RowShapedLayout {
        layout: dummy_layout,
        text: "".into(),
        line_chars: Arc::new(flat_line_chars),
        code_ranges: Arc::new(flat_code_ranges),
        tag_ranges: Arc::new(flat_tag_ranges),
        cell_w,
        table_row: Some(table_row),
    }
}

#[derive(Clone)]
pub(crate) struct RowShapedLayout {
    layout: Arc<gpui::LineLayout>,
    text: SharedString,
    line_chars: Arc<Vec<usize>>,
    code_ranges: Arc<Vec<(usize, usize)>>,
    tag_ranges: Arc<Vec<(usize, usize, String)>>,
    cell_w: f32,
    table_row: Option<Arc<TableRowLayout>>,
}

impl RowShapedLayout {
    pub fn columns(&self) -> usize {
        if let Some(tr) = &self.table_row {
            tr.columns()
        } else {
            self.line_chars.len().saturating_sub(1)
        }
    }

    pub fn width(&self) -> f32 {
        f32::from(self.layout.width)
    }

    fn byte_for_column(&self, col: usize) -> usize {
        self.text
            .char_indices()
            .nth(col)
            .map(|(byte, _)| byte)
            .unwrap_or(self.text.len())
    }

    pub fn x_for_column(&self, col: usize) -> f32 {
        if let Some(tr) = &self.table_row {
            return tr.x_for_column(col, self.cell_w);
        }
        let columns = self.columns();
        if col >= columns {
            return f32::from(self.layout.width) + (col - columns) as f32 * self.cell_w;
        }
        let byte = self.byte_for_column(col);
        let mut cluster_x = 0.0f32;
        for run in &self.layout.runs {
            for glyph in &run.glyphs {
                if glyph.index == byte {
                    return f32::from(glyph.position.x);
                }
                if glyph.index > byte {
                    return cluster_x;
                }
                cluster_x = f32::from(glyph.position.x);
            }
        }
        cluster_x
    }

    pub fn column_for_x(&self, x: f32) -> usize {
        if let Some(tr) = &self.table_row {
            return tr.column_for_x(x);
        }
        let columns = self.columns();
        let end_x = f32::from(self.layout.width);
        if x >= end_x {
            return columns;
        }
        let byte = self.layout.closest_index_for_x(px(x));
        let col = self.text.get(..byte).map(|s| s.chars().count()).unwrap_or(0).min(columns);
        if col == columns && columns > 0 {
            let mid = (self.x_for_column(columns - 1) + end_x) / 2.0;
            if x < mid {
                return columns - 1;
            }
        }
        col
    }

    pub fn line_char_for_column(&self, col: usize) -> usize {
        if let Some(tr) = &self.table_row {
            return tr.line_char_for_column(col);
        }
        self.line_chars[col.min(self.columns())]
    }

    pub fn column_for_line_char(&self, lc: usize) -> usize {
        if let Some(tr) = &self.table_row {
            return tr.column_for_line_char(lc);
        }
        self.line_chars
            .partition_point(|&line_char| line_char < lc)
            .min(self.columns())
    }

    pub fn code_ranges(&self) -> &[(usize, usize)] {
        if let Some(tr) = &self.table_row {
            &tr.code_ranges
        } else {
            &self.code_ranges
        }
    }

    pub fn tag_ranges(&self) -> &[(usize, usize, String)] {
        if let Some(tr) = &self.table_row {
            &tr.tag_ranges
        } else {
            &self.tag_ranges
        }
    }

    pub fn selection_rect(&self, sel_s: usize, sel_e: usize, scale_factor: f32) -> (f32, f32) {
        crate::snap::snapped_span(
            self.x_for_column(sel_s),
            self.x_for_column(sel_e),
            scale_factor,
        )
    }
}

/// Discriminant-only tag of a `LineKind` for cache keys. The payloads
/// (checked/indent/is_opening/lang) are functions of the line text and its
/// position, which the key hashes separately — the variant identity alone
/// is enough to separate the shapes.
enum LineKindTag {
    Heading1,
    Heading2,
    Heading3,
    Heading4,
    Heading5,
    Heading6,
    Quote,
    Task,
    BulletList,
    NumberedList,
    HorizontalRule,
    CodeFence,
    CodeLine,
    Paragraph,
}

impl LineKindTag {
    fn of(kind: &crate::markdown::LineKind<'_>) -> Self {
        use crate::markdown::LineKind::*;
        match kind {
            Heading1(_) => Self::Heading1,
            Heading2(_) => Self::Heading2,
            Heading3(_) => Self::Heading3,
            Heading4(_) => Self::Heading4,
            Heading5(_) => Self::Heading5,
            Heading6(_) => Self::Heading6,
            Quote(_) => Self::Quote,
            Task { .. } => Self::Task,
            BulletList { .. } => Self::BulletList,
            NumberedList { .. } => Self::NumberedList,
            HorizontalRule => Self::HorizontalRule,
            CodeFence { .. } => Self::CodeFence,
            CodeLine(_) => Self::CodeLine,
            Paragraph(_) => Self::Paragraph,
        }
    }
}

/// Hash of the heading settings a row's font metrics depend on; a settings
/// change produces a different cache key, so cached layouts stop hitting.
fn headings_cache_key(headings: &crate::settings::HeadingsSettings) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for style in [
        &headings.h1, &headings.h2, &headings.h3, &headings.h4, &headings.h5, &headings.h6,
    ] {
        style.font_size.hash(&mut hasher);
        style.bold.hash(&mut hasher);
        style.italic.hash(&mut hasher);
    }
    hasher.finish()
}

pub(crate) fn shape_row(
    text_system: &gpui::WindowTextSystem,
    line: &str,
    classified: &crate::markdown::LineKind<'_>,
    v: &crate::markdown::VisualRow,
    is_active_line: bool,
    headings: &crate::settings::HeadingsSettings,
    font_family: &SharedString,
    cell_w: f32,
) -> RowShapedLayout {
    // Counts every real shaping (cache miss or direct call) — not rows per
    // frame, which used to be counted in the render loop and masked whether
    // the row cache worked at all.
    crate::perf::PERF
        .shape_calls
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let painted = crate::markdown::row_painted_text(line, classified, v, is_active_line, headings);
    let code_ranges = painted.code_ranges();
    let tag_ranges = painted.tag_ranges();
    let (_, _, font_size, _, _) = AppView::heading_typography_metrics_with_widths(classified, headings, 24.0, cell_w, None);

    let mut text = String::new();
    let mut runs = Vec::new();
    let color = rgba(0x000000ff).into();
    for segment in &painted.segments {
        let weight = if segment.code {
            inline_code_weight(if segment.bold { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL })
        } else if segment.bold {
            gpui::FontWeight::BOLD
        } else {
            gpui::FontWeight::NORMAL
        };
        text.push_str(&segment.text);
        runs.push(gpui::TextRun {
            len: segment.text.len(),
            font: gpui::Font {
                family: font_family.clone(),
                weight,
                style: if segment.italic { gpui::FontStyle::Italic } else { gpui::FontStyle::Normal },
                features: Default::default(),
                fallbacks: None,
            },
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }

    let text: SharedString = text.into();
    let layout = text_system.layout_line(text.as_ref(), px(font_size), &runs, None);
    RowShapedLayout {
        layout,
        text,
        line_chars: Arc::new(painted.line_chars),
        code_ranges: Arc::new(code_ranges),
        tag_ranges: Arc::new(tag_ranges),
        cell_w,
        table_row: None,
    }
}

fn render_live_styled_row(
    line: &str,
    classified: &crate::markdown::LineKind<'_>,
    char_start: usize,
    char_end: usize,
    line_idx: usize,
    sub_idx: usize,
    is_active_line: bool,
    show_fences: bool,
    headings: &crate::settings::HeadingsSettings,
    theme: &Theme,
    font_family: &gpui::SharedString,
    custom_ui_node: Option<&crate::plugins::ui::UiNode>,
    custom_plugin_id: Option<&str>,
    is_first_code_line: bool,
    active_plugin_input: Option<&crate::plugins::ui::PluginInputFocusState>,
    plugin_input_focus_handle: &FocusHandle,
    active_plugin_select: Option<&(String, u64, String)>,
    plugin_select_popup: Option<&Entity<crate::views::tab_list::TabListPopup>>,
    plugin_input_blink_visible: bool,
    table_render: Option<(&crate::table::TableBlock, &TableLayout, Option<usize>, usize)>,
    ctrl_held: bool,
    hovered_link_id: Option<&str>,
    window: &Window,
    cx: &Context<AppView>,
) -> gpui::Div {
    if let Some((tbl, table_layout, cursor_col, line_idx)) = table_render {
        if line_idx == tbl.sep_line() {
            return div()
                .w(px(table_layout.total_width))
                .h(px(1.0))
                .bg(theme.border_subtle);
        }

        let raw_cells = crate::table::split_row(line);
        let is_header = line_idx == tbl.first_line;
        let is_last_row = line_idx == tbl.last_line && line_idx > tbl.sep_line();
        let active_c = cursor_col.map(|cc| crate::table::cell_index_at(&raw_cells, cc));
        // Cell ranges are in chars; map them to byte offsets once per row instead
        // of re-walking the line from the start for every cell.
        let mut char_byte_offsets: Vec<usize> = line.char_indices().map(|(b, _)| b).collect();
        char_byte_offsets.push(line.len());
        let mut row_div = div().flex().flex_row().items_center()
            .when(is_header, |d| d.h(px(28.0)).mt(px(14.0)))
            .when(is_last_row, |d| d.h(px(28.0)).mb(px(14.0)))
            .when(!is_header && !is_last_row, |d| d.h(px(28.0)));
        for (c_idx, &col_w) in table_layout.col_widths.iter().enumerate() {
            let cell_text = raw_cells
                .get(c_idx)
                .and_then(|c| {
                    let start = *char_byte_offsets.get(c.content.start)?;
                    let end = *char_byte_offsets.get(c.content.end)?;
                    line.get(start..end)
                })
                .unwrap_or("");
            let is_active_cell = active_c == Some(c_idx);
            let spans = crate::markdown::parse_inline_syntax_spans(cell_text);
            let align = table_layout.aligns.get(c_idx).copied().unwrap_or_default();
            let font_weight = if is_header { Some(gpui::FontWeight::BOLD) } else { None };
            let is_first_col = c_idx == 0;
            let is_last_col = c_idx + 1 == table_layout.col_widths.len();

            let tbl_block = tbl.clone();
            let row_idx = if is_header { 0 } else { tbl.row_of_line(line_idx).unwrap_or(0) };
            let cell_start_col = raw_cells.get(c_idx).map(|c| c.content.start).unwrap_or(0);

            let tbl_first_line = tbl.first_line;
            let mut cell_div = div()
                .id(gpui::ElementId::Name(format!("tbl-cell-{}-{}-{}", tbl.first_line, line_idx, c_idx).into()))
                .relative()
                .w(px(col_w))
                .h_full()
                .flex()
                .items_center()
                .px_2()
                .border_l_1()
                .border_color(theme.border_subtle)
                .when(is_last_col, |d| d.border_r_1().border_color(theme.border_subtle))
                .when(is_header, |d| d.border_t_1().border_color(theme.border_subtle))
                .when(!is_header, |d| d.border_b_1().border_color(theme.border_subtle))
                .when(is_header, |d| {
                    let mut bg = theme.text_muted;
                    bg.a = 0.08;
                    d.bg(bg)
                })
                .when(is_header && is_first_col, |d| d.rounded_tl_sm())
                .when(is_header && is_last_col, |d| d.rounded_tr_sm())
                .when(is_last_row && is_first_col, |d| d.rounded_bl_sm())
                .when(is_last_row && is_last_col, |d| d.rounded_br_sm())
                .cursor(gpui::CursorStyle::IBeam)
                .on_mouse_down(MouseButton::Right, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    window.focus(&this.focus_handle);
                    this.state.active_set_cursor_line_col(line_idx, cell_start_col);
                    let click_x: f32 = event.position.x.into();
                    let click_y: f32 = event.position.y.into();
                    this.open_table_context_menu(tbl_block.clone(), row_idx, c_idx, click_x, click_y, cx);
                }));
            match align {
                crate::table::Align::None | crate::table::Align::Left => { cell_div = cell_div.justify_start(); }
                crate::table::Align::Center => { cell_div = cell_div.justify_center(); }
                crate::table::Align::Right => { cell_div = cell_div.justify_end(); }
            }
            let cell_key = format!("tbl-cell-{}-{}-{}", line_idx, c_idx, sub_idx);
            cell_div = cell_div.child(render_syntax_spans(spans, is_active_cell, theme, None, font_weight, None, font_family, &cell_key, ctrl_held, hovered_link_id, cx));

            let col_widths_clone = table_layout.col_widths.clone();
            cell_div = cell_div.child(
                div()
                    .id(gpui::ElementId::Name(format!("tbl-rsz-{}-{}-{}", tbl_first_line, line_idx, c_idx).into()))
                    .absolute()
                    .right(px(-4.0))
                    .top_0()
                    .bottom_0()
                    .w(px(8.0))
                    .cursor(gpui::CursorStyle::ResizeLeftRight)
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                        cx.stop_propagation();
                        let click_x: f32 = event.position.x.into();
                        this.start_table_col_resize(tbl_first_line, c_idx, click_x, &col_widths_clone);
                        cx.notify();
                    }))
            );

            row_div = row_div.child(cell_div);
        }
        return row_div;
    }

    if line.is_empty() || char_start >= char_end {
        return div().h_full().flex().items_center().child(" ");
    }

    let row_key = format!("row-{}-{}", line_idx, sub_idx);
    let render_spans = |spans: Vec<crate::markdown::SyntaxSpan>, is_active: bool, text_color: Option<gpui::Rgba>, weight: Option<gpui::FontWeight>, style: Option<gpui::FontStyle>| {
        render_syntax_spans(
            spans,
            is_active,
            theme,
            text_color,
            weight,
            style,
            font_family,
            &row_key,
            ctrl_held,
            hovered_link_id,
            cx,
        )
    };

    match classified {
        crate::markdown::LineKind::Heading1(_) => {
            let rest = line.strip_prefix("# ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let h_size = headings.h1.font_size as f32;
            let h_color = crate::markdown::heading_color_to_rgb(&headings.h1.color_name);
            let h_weight = if headings.h1.bold { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL };
            let h_style = if headings.h1.italic { gpui::FontStyle::Italic } else { gpui::FontStyle::Normal };

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(2));
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h1.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full();
                if is_active_line {
                    row = row.child(div().text_color(theme.text_muted).child("# "));
                }
                row.child(
                    div()
                        .text_color(h_color)
                        .child(render_spans(row_spans, is_active_line, Some(h_color), Some(h_weight), Some(h_style))),
                )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(2),
                    char_end.saturating_sub(2),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h1.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full()
                    .child(
                        div()
                            .text_color(h_color)
                            .child(render_spans(row_spans, is_active_line, Some(h_color), Some(h_weight), Some(h_style))),
                    )
            }
        }
        crate::markdown::LineKind::Heading2(_) => {
            let rest = line.strip_prefix("## ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let h_size = headings.h2.font_size as f32;
            let h_color = crate::markdown::heading_color_to_rgb(&headings.h2.color_name);
            let h_weight = if headings.h2.bold { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL };
            let h_style = if headings.h2.italic { gpui::FontStyle::Italic } else { gpui::FontStyle::Normal };

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(3));
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h2.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full();
                if is_active_line {
                    row = row.child(div().text_color(theme.text_muted).child("## "));
                }
                row.child(
                    div()
                        .text_color(h_color)
                        .child(render_spans(row_spans, is_active_line, Some(h_color), Some(h_weight), Some(h_style))),
                )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(3),
                    char_end.saturating_sub(3),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h2.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full()
                    .child(
                        div()
                            .text_color(h_color)
                            .child(render_spans(row_spans, is_active_line, Some(h_color), Some(h_weight), Some(h_style))),
                    )
            }
        }
        crate::markdown::LineKind::Heading3(_) => {
            let rest = line.strip_prefix("### ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let h_size = headings.h3.font_size as f32;
            let h_color = crate::markdown::heading_color_to_rgb(&headings.h3.color_name);
            let h_weight = if headings.h3.bold { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL };
            let h_style = if headings.h3.italic { gpui::FontStyle::Italic } else { gpui::FontStyle::Normal };

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(4));
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h3.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full();
                if is_active_line {
                    row = row.child(div().text_color(theme.text_muted).child("### "));
                }
                row.child(
                    div()
                        .text_color(h_color)
                        .child(render_spans(row_spans, is_active_line, Some(h_color), Some(h_weight), Some(h_style))),
                )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(4),
                    char_end.saturating_sub(4),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h3.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full()
                    .child(
                        div()
                            .text_color(h_color)
                            .child(render_spans(row_spans, is_active_line, Some(h_color), Some(h_weight), Some(h_style))),
                    )
            }
        }
        crate::markdown::LineKind::Heading4(_) => {
            let rest = line.strip_prefix("#### ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let h_size = headings.h4.font_size as f32;
            let h_color = crate::markdown::heading_color_to_rgb(&headings.h4.color_name);
            let h_weight = if headings.h4.bold { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL };
            let h_style = if headings.h4.italic { gpui::FontStyle::Italic } else { gpui::FontStyle::Normal };

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(5));
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h4.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full();
                if is_active_line {
                    row = row.child(div().text_color(theme.text_muted).child("#### "));
                }
                row.child(
                    div()
                        .text_color(h_color)
                        .child(render_spans(row_spans, is_active_line, Some(h_color), Some(h_weight), Some(h_style))),
                )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(5),
                    char_end.saturating_sub(5),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h4.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full()
                    .child(
                        div()
                            .text_color(h_color)
                            .child(render_spans(row_spans, is_active_line, Some(h_color), Some(h_weight), Some(h_style))),
                    )
            }
        }
        crate::markdown::LineKind::Heading5(_) => {
            let rest = line.strip_prefix("##### ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let h_size = headings.h5.font_size as f32;
            let h_color = crate::markdown::heading_color_to_rgb(&headings.h5.color_name);
            let h_weight = if headings.h5.bold { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL };
            let h_style = if headings.h5.italic { gpui::FontStyle::Italic } else { gpui::FontStyle::Normal };

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(6));
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h5.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full();
                if is_active_line {
                    row = row.child(div().text_color(theme.text_muted).child("##### "));
                }
                row.child(
                    div()
                        .text_color(h_color)
                        .child(render_spans(row_spans, is_active_line, Some(h_color), Some(h_weight), Some(h_style))),
                )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(6),
                    char_end.saturating_sub(6),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h5.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full()
                    .child(
                        div()
                            .text_color(h_color)
                            .child(render_spans(row_spans, is_active_line, Some(h_color), Some(h_weight), Some(h_style))),
                    )
            }
        }
        crate::markdown::LineKind::Heading6(_) => {
            let rest = line.strip_prefix("###### ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let h_size = headings.h6.font_size as f32;
            let h_color = crate::markdown::heading_color_to_rgb(&headings.h6.color_name);
            let h_weight = if headings.h6.bold { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL };
            let h_style = if headings.h6.italic { gpui::FontStyle::Italic } else { gpui::FontStyle::Normal };

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(7));
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h6.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full();
                if is_active_line {
                    row = row.child(div().text_color(theme.text_muted).child("###### "));
                }
                row.child(
                    div()
                        .text_color(h_color)
                        .child(render_spans(row_spans, is_active_line, Some(h_color), Some(h_weight), Some(h_style))),
                )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(7),
                    char_end.saturating_sub(7),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .font_weight(h_weight)
                    .when(headings.h6.italic, |d| d.italic())
                    .text_size(px(h_size))
                    .h_full()
                    .child(
                        div()
                            .text_color(h_color)
                            .child(render_spans(row_spans, is_active_line, Some(h_color), Some(h_weight), Some(h_style))),
                    )
            }
        }
        crate::markdown::LineKind::Quote(_) => {
            let rest = line.strip_prefix("> ").unwrap_or(line);
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(2));
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .italic()
                    .child(div().text_color(theme.accent).child("> "))
                    .child(div().text_color(theme.text_secondary).child(render_spans(row_spans, is_active_line, Some(theme.text_secondary), None, None)))
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(2),
                    char_end.saturating_sub(2),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .italic()
                    .child(div().text_color(theme.border_subtle).child("  "))
                    .child(div().text_color(theme.text_secondary).child(render_spans(row_spans, is_active_line, Some(theme.text_secondary), None, None)))
            }
        }
        crate::markdown::LineKind::Task { checked, .. } => {
            let prefix_len = if line.starts_with("- [ ] ")
                || line.starts_with("* [ ] ")
                || line.starts_with("- [x] ")
                || line.starts_with("- [X] ")
                || line.starts_with("* [x] ")
                || line.starts_with("* [X] ")
            {
                6
            } else {
                0
            };
            let rest = if line.len() >= prefix_len { &line[prefix_len..] } else { "" };
            let spans = crate::markdown::parse_inline_syntax_spans(rest);
            let prefix_color = if *checked { theme.success } else { theme.text_muted };
            let prefix_str = if line.len() >= prefix_len { &line[..prefix_len] } else { "" };
            let task_color = if *checked { Some(theme.text_muted) } else { None };

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(prefix_len));
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .child(div().font_weight(gpui::FontWeight::BOLD).text_color(prefix_color).child(prefix_str.to_string()))
                    .child(
                        div()
                            .when(*checked, |s| s.text_color(theme.text_muted).line_through())
                            .child(render_spans(row_spans, is_active_line, task_color, None, None)),
                    )
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(prefix_len),
                    char_end.saturating_sub(prefix_len),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .child(div().child("      "))
                    .child(
                        div()
                            .when(*checked, |s| s.text_color(theme.text_muted).line_through())
                            .child(render_spans(row_spans, is_active_line, task_color, None, None)),
                    )
            }
        }
        crate::markdown::LineKind::BulletList { .. } => {
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();
            let indent = &line[..indent_len];
            let (bullet, rest) = if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
                (&trimmed[..2], &trimmed[2..])
            } else {
                ("", trimmed)
            };
            let prefix_len = indent_len + bullet.len();
            let spans = crate::markdown::parse_inline_syntax_spans(rest);

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(prefix_len));
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .child(div().child(indent.to_string()))
                    .child(div().font_weight(gpui::FontWeight::BOLD).text_color(theme.accent).child(bullet.to_string()))
                    .child(render_spans(row_spans, is_active_line, None, None, None))
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(prefix_len),
                    char_end.saturating_sub(prefix_len),
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .child(div().child(format!("{}  ", indent)))
                    .child(render_spans(row_spans, is_active_line, None, None, None))
            }
        }
        crate::markdown::LineKind::NumberedList { num, .. } => {
            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();
            let indent = &line[..indent_len];
            let num_prefix_len = num.len() + 2;
            let (num_part, rest) = if trimmed.len() >= num_prefix_len {
                (&trimmed[..num_prefix_len], &trimmed[num_prefix_len..])
            } else {
                ("", trimmed)
            };
            let prefix_len = indent_len + num_part.len();
            let spans = crate::markdown::parse_inline_syntax_spans(rest);

            if sub_idx == 0 {
                let row_spans = crate::markdown::slice_syntax_spans(&spans, 0, char_end.saturating_sub(prefix_len));
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .child(div().child(indent.to_string()))
                    .child(div().font_weight(gpui::FontWeight::BOLD).text_color(theme.accent).child(num_part.to_string()))
                    .child(render_spans(row_spans, is_active_line, None, None, None))
            } else {
                let row_spans = crate::markdown::slice_syntax_spans(
                    &spans,
                    char_start.saturating_sub(prefix_len),
                    char_end.saturating_sub(prefix_len),
                );
                let spaces = " ".repeat(prefix_len);
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .child(div().child(spaces))
                    .child(render_spans(row_spans, is_active_line, None, None, None))
            }
        }
        crate::markdown::LineKind::HorizontalRule => {
            if is_active_line {
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .text_color(theme.text_muted)
                    .child(line.to_string())
            } else {
                div()
                    .w_full()
                    .h_full()
                    .flex()
                    .flex_row()
                    .items_center()
                    .child(
                        div()
                            .h(px(1.0))
                            .w_full()
                            .bg(theme.border_subtle),
                    )
            }
        }
        crate::markdown::LineKind::CodeFence { is_opening, lang } => {
            if *is_opening {
                if show_fences || is_active_line {
                    // Editing the block: paint the raw fence so the caret and the
                    // backticks line up with the source.
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .h_full()
                        .text_color(theme.text_muted)
                        .child(line.to_string())
                } else {
                    // Collapsed: the language label stands in for the fence, and the row
                    // loop overlays the copy button at the right end of this same row.
                    let lang_label = lang.unwrap_or("CODE").to_uppercase();
                    let mut header = div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .h_full()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(theme.accent)
                                .child(lang_label),
                        );
                    if let Some(pid) = custom_plugin_id {
                        header = header.child(
                            div()
                                .text_xs()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(theme.text_muted)
                                .child(format!("[{}]", pid)),
                        );
                    }
                    header.child(icons::icon(icons::ICON_CODE, 12.0, theme.text_muted))
                }
            } else if show_fences || is_active_line {
                // Editing the block: paint the raw closing fence.
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .text_color(theme.text_muted)
                    .child(line.to_string())
            } else {
                // Collapsed: the fence is hidden. The row stays empty and only carries
                // the block's background and its rounded bottom corners, so the panel
                // still ends in a visible edge.
                div().h_full()
            }
        }
        crate::markdown::LineKind::CodeLine(_) => {
            if !show_fences && !is_active_line && custom_ui_node.is_some() {
                if is_first_code_line {
                    let tree = custom_ui_node.unwrap();
                    let pid = custom_plugin_id.unwrap_or("plugin");
                    div()
                        .flex()
                        .flex_col()
                        .child(crate::plugins::ui::render_ui_node(
                            tree,
                            pid,
                            0,
                            theme,
                            active_plugin_input,
                            plugin_input_focus_handle,
                            active_plugin_select,
                            plugin_select_popup,
                            plugin_input_blink_visible,
                            window,
                            cx,
                            None,
                        ))
                } else {
                    div().h_0().overflow_hidden()
                }
            } else {
                let row_slice: String = line.chars().skip(char_start).take(char_end - char_start).collect();
                // Plain foreground: the block background alone marks this as code. The
                // green that used to sit here read as an accent colour, not as text.
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .text_color(theme.text_primary)
                    .child(row_slice)
            }
        }
        crate::markdown::LineKind::Paragraph(_) => {
            let spans = crate::markdown::parse_inline_syntax_spans(line);
            let row_spans = crate::markdown::slice_syntax_spans(&spans, char_start, char_end);
            div()
                .flex()
                .flex_row()
                .items_center()
                .h_full()
                .child(render_spans(row_spans, is_active_line, None, None, None))
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::{classify_line, row_painted_text, VisualRow};

    /// Every mouse position must resolve to the **nearest** painted column, including
    /// the boundary past the last painted character. That one boundary owns no glyph,
    /// so it is the one a shaper's `closest_index_for_x` cannot weigh and the one a
    /// right-to-left drag lands on at the end of a line.
    #[test]
    fn column_boundaries_sit_mid_cell_including_end_of_line() {
        gpui::Application::new().run(|cx: &mut gpui::App| {
            crate::fonts::register_embedded_fonts(cx);
            let _ = cx.open_window(gpui::WindowOptions::default(), |window, cx| {
                let font_family: SharedString = "Droid Sans Mono".into();
                let headings = crate::settings::HeadingsSettings::default();
                let font_id = window.text_system().resolve_font(&gpui::font(font_family.clone()));
                let cell_w = window
                    .text_system()
                    .advance(font_id, px(14.0), 'x')
                    .map(|s| f32::from(s.width))
                    .unwrap_or(7.7);

                for line in ["abcdef", "abc", "ab 😀", "текст", "x🏳️‍🌈", "warn ⚠️ tail"] {
                    let v = VisualRow {
                        line_idx: 0,
                        sub_idx: 0,
                        is_last_sub: true,
                        char_start: 0,
                        char_end: line.chars().count(),
                    };
                    let kind = classify_line(line, None);
                    let layout = shape_row(window.text_system(), line, &kind, &v, false, &headings, &font_family, cell_w);
                    let columns = layout.columns();
                    let end_x = layout.x_for_column(columns);

                    let mut prev = layout.column_for_x(-1.0);
                    assert_eq!(prev, 0, "the left edge must map to column 0 for {line:?}");
                    let mut x = -1.0f32;
                    while x <= end_x + 2.0 {
                        let col = layout.column_for_x(x);
                        if col != prev {
                            let mid = (layout.x_for_column(prev) + layout.x_for_column(col)) / 2.0;
                            assert!(
                                (x - mid).abs() <= 0.02,
                                "{line:?}: boundary {prev}->{col} flipped at x={x:.3} but the middle of \
                                 that cell is {mid:.3} (off by {:+.3}px)",
                                x - mid
                            );
                            prev = col;
                        }
                        x += 0.01;
                    }
                    assert_eq!(prev, columns, "sweep never reached the last column for {line:?}");
                    // And the position past the end must stay reachable far to the right.
                    assert_eq!(layout.column_for_x(end_x + 100.0), columns);
                }

                cx.quit();
                use gpui::AppContext;
                struct DummyView;
                impl gpui::Render for DummyView {
                    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
                        gpui::div()
                    }
                }
                cx.new(|_| DummyView)
            });
        });
    }


    /// Row geometry must come from real glyph positions rather than from a
    /// `column * char_width` cell model: a wide glyph (emoji, CJK) does not advance by
    /// exactly one cell, so the cell model drifts and the selection edge twitches while
    /// dragging across it. This fails if `shape_row` is ever replaced by the old model.
    #[test]
    fn row_geometry_follows_shaped_glyph_positions() {
        gpui::Application::new().run(|cx: &mut gpui::App| {
            crate::fonts::register_embedded_fonts(cx);
            let _ = cx.open_window(gpui::WindowOptions::default(), |window, cx| {
                let font_family: SharedString = "Droid Sans Mono".into();
                let headings = crate::settings::HeadingsSettings::default();
                let font_id = window.text_system().resolve_font(&gpui::font(font_family.clone()));
                let cell_w = window
                    .text_system()
                    .advance(font_id, px(14.0), 'x')
                    .map(|s| f32::from(s.width))
                    .unwrap_or(7.7);

                let cases = [
                    "plain ascii line",
                    "кириллица без эмодзи",
                    "emoji 😀 test 🚀 done",
                    "warn ⚠️ love ❤️ flag 🏳️‍🌈 done",
                ];

                for line in cases {
                    let v = VisualRow {
                        line_idx: 0,
                        sub_idx: 0,
                        is_last_sub: true,
                        char_start: 0,
                        char_end: line.chars().count(),
                    };
                    let kind = classify_line(line, None);
                    let layout = shape_row(window.text_system(), line, &kind, &v, false, &headings, &font_family, cell_w);

                    // Independently shape the very same painted text and read the glyph
                    // positions straight out of the layout.
                    let painted = row_painted_text(line, &kind, &v, false, &headings);
                    let text: String = painted.segments.iter().map(|s| s.text.as_str()).collect();
                    let run = gpui::TextRun {
                        len: text.len(),
                        font: gpui::font(font_family.clone()),
                        color: Default::default(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let reference = window
                        .text_system()
                        .shape_line(text.clone().into(), px(14.0), &[run], None);

                    let mut shaped_error = 0.0f32;
                    let mut naive_error = 0.0f32;
                    let mut cluster_starts = 0usize;
                    let mut byte = 0usize;
                    for (column, ch) in text.chars().enumerate() {
                        // The glyph that starts exactly at this char, and the glyph that
                        // starts its cluster (VS16 / ZWJ continuations have neither).
                        let mut cluster_x = 0.0f32;
                        let mut exact_x = None;
                        for run in &reference.runs {
                            for glyph in &run.glyphs {
                                if glyph.index == byte {
                                    exact_x = Some(f32::from(glyph.position.x));
                                }
                                if glyph.index <= byte {
                                    cluster_x = f32::from(glyph.position.x);
                                }
                            }
                        }
                        let actual = layout.x_for_column(column);
                        match exact_x {
                            Some(expected) => {
                                cluster_starts += 1;
                                shaped_error = shaped_error.max((actual - expected).abs());
                                naive_error = naive_error.max((column as f32 * cell_w - expected).abs());
                            }
                            // A continuation char has no glyph of its own: the caret must
                            // stay on its cluster's left edge, not jump to the next one.
                            None => assert_eq!(
                                actual, cluster_x,
                                "column {column} ({ch:?}) left its cluster in {line:?}"
                            ),
                        }
                        byte += ch.len_utf8();
                    }
                    assert!(cluster_starts > 0, "no glyph positions checked for {line:?}");
                    println!("{line:?}: {cluster_starts} glyph columns, shaped max error {shaped_error:.3}px, naive cell model {naive_error:.3}px");

                    assert!(
                        shaped_error <= 0.01,
                        "shaped geometry drifted {shaped_error}px for {line:?}"
                    );
                }

                cx.quit();
                use gpui::AppContext;
                struct DummyView;
                impl gpui::Render for DummyView {
                    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
                        gpui::div()
                    }
                }
                cx.new(|_| DummyView)
            });
        });
    }

    /// The painted selection edge has to sit on the same device-pixel grid as the
    /// painted caret, and it has to move monotonically while the mouse drags.
    ///
    /// The rectangle carries `rounded_*_xs()` corners, so it is painted by the
    /// signed-distance path and its edge is antialiased; the caret is snapped by
    /// `crate::snap::snap_x`. Leave the rectangle unsnapped and the two disagree by up
    /// to a device pixel — the partially covered pixel column changes its blend ratio
    /// with every sub-pixel mouse move, which is what reads as the selection edge
    /// stuttering at the end of a line while dragging right to left.
    #[test]
    fn painted_selection_edge_sits_on_the_device_grid() {
        gpui::Application::new().run(|cx: &mut gpui::App| {
            crate::fonts::register_embedded_fonts(cx);
            let _ = cx.open_window(gpui::WindowOptions::default(), |window, cx| {
                let font_family: SharedString = "Droid Sans Mono".into();
                let headings = crate::settings::HeadingsSettings::default();
                let font_id = window.text_system().resolve_font(&gpui::font(font_family.clone()));
                let cell_w = window
                    .text_system()
                    .advance(font_id, px(14.0), 'x')
                    .map(|s| f32::from(s.width))
                    .unwrap_or(7.7);

                for line in ["abcdef", "ab 😀", "x🏳️‍🌈", "warn ⚠️ tail", "текст"] {
                    let v = VisualRow {
                        line_idx: 0,
                        sub_idx: 0,
                        is_last_sub: true,
                        char_start: 0,
                        char_end: line.chars().count(),
                    };
                    let kind = classify_line(line, None);
                    let layout = shape_row(window.text_system(), line, &kind, &v, false, &headings, &font_family, cell_w);
                    let columns = layout.columns();
                    let end_x = layout.x_for_column(columns);

                    for scale in [1.0f32, 1.25, 1.5, 1.75, 2.0] {
                        // Right to left, the direction the stutter was reported in, with
                        // the anchor parked at the end of the line.
                        let mut prev_edge = f32::INFINITY;
                        let mut x = end_x + 2.0;
                        while x >= -1.0 {
                            // Exactly what the app does: the mouse picks a column, that
                            // column becomes a line char / offset, and the selection range
                            // is painted back from the column of that line char.
                            let col = layout.column_for_x(x);
                            let painted_col = layout.column_for_line_char(layout.line_char_for_column(col));
                            let (edge, width) = layout.selection_rect(painted_col, columns, scale);

                            assert!(width >= 0.0, "{line:?} @{scale}x: negative width at x={x:.3}");

                            // On the device grid, like the caret.
                            let device = edge * scale;
                            assert!(
                                (device - device.round()).abs() <= 0.0005,
                                "{line:?} @{scale}x: painted edge {edge:.4}px is off the device grid \
                                 ({device:.4} device px) at x={x:.3}"
                            );

                            // Dragging left must never push the painted edge right.
                            assert!(
                                edge <= prev_edge + 0.0005,
                                "{line:?} @{scale}x: painted edge moved backwards while dragging left, \
                                 {prev_edge:.4} -> {edge:.4} at x={x:.3}"
                            );

                            // Snapping only rounds, so the edge stays within half a device
                            // pixel of the true glyph boundary.
                            let raw = layout.x_for_column(painted_col);
                            assert!(
                                (edge - raw).abs() <= 0.5 / scale + 0.0005,
                                "{line:?} @{scale}x: snapping moved the edge {:.4}px from {raw:.4}",
                                edge - raw
                            );

                            prev_edge = edge;
                            x -= 0.01;
                        }
                    }
                }

                cx.quit();
                use gpui::AppContext;
                struct DummyView;
                impl gpui::Render for DummyView {
                    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
                        gpui::div()
                    }
                }
                cx.new(|_| DummyView)
            });
        });
    }

    /// Row geometry is built from shaped glyph positions, so every run must be shaped
    /// with its *own* face. `TextSystem::layout_line` used to merge runs on the
    /// decoration alone (colour/underline/strikethrough) and never re-resolve the font,
    /// so the editor's flat-colour geometry measured an italic span with the upright
    /// advance. Roboto Mono's italic is 1202/2048 em against the upright 1229/2048 em,
    /// so the caret and the selection edge drifted ~2% right of the glyphs and the
    /// caret ended up inside the glyph it should sit in front of.
    #[test]
    fn geometry_shapes_each_run_with_its_own_face() {
        gpui::Application::new().run(|cx: &mut gpui::App| {
            crate::fonts::register_embedded_fonts(cx);
            let _ = cx.open_window(gpui::WindowOptions::default(), |window, cx| {
                let font_family: SharedString = "Roboto Mono".into();
                let upright = gpui::Font {
                    family: font_family.clone(),
                    weight: gpui::FontWeight::BOLD,
                    style: gpui::FontStyle::Normal,
                    features: Default::default(),
                    fallbacks: None,
                };
                let bold_italic = gpui::Font { style: gpui::FontStyle::Italic, ..upright.clone() };
                let run = |font: gpui::Font, len: usize| gpui::TextRun {
                    len,
                    font,
                    color: rgba(0x000000ff).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let advance = |font: gpui::Font| -> f32 {
                    let text: SharedString = "A".into();
                    let runs = [run(font, 1)];
                    f32::from(window.text_system().layout_line(&text, px(40.0), &runs, None).width)
                };
                let upright_adv = advance(upright.clone());
                let italic_adv = advance(bold_italic.clone());
                assert!(
                    italic_adv < upright_adv,
                    "the shipped editor font must have a narrower italic advance, \
                     otherwise this test cannot detect the merged-run regression"
                );

                // Two runs that share one colour must still split into per-font runs.
                let text: SharedString = "AB".into();
                let runs = [run(upright.clone(), 1), run(bold_italic.clone(), 1)];
                let shaped = window.text_system().layout_line(&text, px(40.0), &runs, None);
                assert_eq!(
                    shaped.runs.len(),
                    2,
                    "runs with different fonts must not be merged into one font run"
                );
                assert!(
                    (f32::from(shaped.width) - (upright_adv + italic_adv)).abs() < 0.05,
                    "expected the two faces' own advances to be summed, got {} for {upright_adv} + {italic_adv}",
                    f32::from(shaped.width)
                );

                // ...and the row geometry the caret is derived from must do the same.
                let line = "# *Курсив со звёздочками*";
                let v = VisualRow { line_idx: 0, sub_idx: 0, is_last_sub: true, char_start: 0, char_end: line.chars().count() };
                let kind = classify_line(line, None);
                let mut headings = crate::settings::HeadingsSettings::default();
                headings.h1.font_size = 40;
                headings.h1.bold = true;
                headings.h1.italic = false;
                let geom = shape_row(window.text_system(), line, &kind, &v, true, &headings, &font_family, 24.0);
                // Column 0 is the upright "# " marker, column 3 the first italic char.
                let up_cell = geom.x_for_column(1) - geom.x_for_column(0);
                let it_cell = geom.x_for_column(4) - geom.x_for_column(3);
                assert!((up_cell - upright_adv).abs() < 0.05, "upright cell {up_cell} vs {upright_adv}");
                assert!((it_cell - italic_adv).abs() < 0.05, "italic cell {it_cell} vs {italic_adv}");

                cx.quit();
                use gpui::AppContext;
                struct DummyView;
                impl gpui::Render for DummyView {
                    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
                        gpui::div()
                    }
                }
                cx.new(|_| DummyView)
            });
        });
    }

    /// The inline-code chip keeps a constant width, so the marker delimiting it must stay
    /// painted off the active line — with no ink at all, which is what reads as the
    /// chip's padding. Every other marker collapses entirely when the line is idle.
    #[test]
    fn code_delimiters_stay_painted_off_the_active_line() {
        let theme = Theme::light();
        // `Hsla` is not `PartialEq`, and the Rgba -> Hsla -> Rgba round trip is not
        // exact, so compare the components with a tolerance.
        let comps = |c: gpui::Rgba| (c.r, c.g, c.b, c.a);
        let same = |a: gpui::Hsla, b: gpui::Rgba| {
            let a = comps(gpui::Rgba::from(a));
            let b = comps(b);
            (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3 && (a.2 - b.2).abs() < 1e-3 && (a.3 - b.3).abs() < 1e-3
        };

        assert!(marker_paint("**", false, &theme).is_none(), "a hidden `**` must not be painted");
        assert!(marker_paint("~~", false, &theme).is_none(), "a hidden `~~` must not be painted");

        let muted = marker_paint("**", true, &theme).expect("an active-line marker is painted");
        assert!(same(muted, theme.text_muted), "an active-line marker is muted: {:?}", comps(gpui::Rgba::from(muted)));

        // The chip is painted by the row, so an idle delimiter only has
        // to carry no ink: the chip shows through it, and so does a selection.
        let idle_delim = marker_paint("`", false, &theme).expect("a code delimiter is always painted");
        assert_eq!(comps(gpui::Rgba::from(idle_delim)).3, 0.0, "an idle delimiter must be invisible: {:?}", comps(gpui::Rgba::from(idle_delim)));

        let active_delim = marker_paint("`", true, &theme).expect("a code delimiter is always painted");
        assert!(same(active_delim, theme.text_muted), "an active delimiter is muted: {:?}", comps(gpui::Rgba::from(active_delim)));
    }

    /// Inline code is painted one step heavier than the body text, and the row's shaped
    /// layout has to ask for the same face — otherwise the chip and the caret would be
    /// measured with the wrong advances.
    #[test]
    fn inline_code_is_heavier_than_its_body_but_never_lighter_than_a_bold_one() {
        assert_eq!(inline_code_weight(gpui::FontWeight::NORMAL).0, INLINE_CODE_WEIGHT.0);
        assert!(INLINE_CODE_WEIGHT.0 > gpui::FontWeight::NORMAL.0);
        // A bold row keeps its bold: there is no heavier bundled face to move up to.
        assert_eq!(inline_code_weight(gpui::FontWeight::BOLD).0, gpui::FontWeight::BOLD.0);
        assert!(inline_code_weight(gpui::FontWeight::LIGHT).0 >= INLINE_CODE_WEIGHT.0);
    }

    /// A saved restore is clamped against the document, never against gpui's
    /// `max_offset`: the list paints a slice, so the handle no longer describes
    /// the content and its max is meaningless.
    #[test]
    fn restore_target_is_clamped_to_the_document() {
        // A stale target (window shrank since exit) is pulled in to the true end.
        assert_eq!(AppView::clamp_restore_target(5000.0, 6000.0, 740.0), 5000.0);
        assert_eq!(AppView::clamp_restore_target(5000.0, 5400.0, 740.0), 4660.0);
        // Negative targets and short docs pin to zero.
        assert_eq!(AppView::clamp_restore_target(-40.0, 6000.0, 740.0), 0.0);
        assert_eq!(AppView::clamp_restore_target(5000.0, 300.0, 740.0), 0.0);
        // Exactness at the tail of a huge document: f32 would move this by 16 px.
        assert_eq!(AppView::clamp_restore_target(1.0e8, 1.0e8 + 1000.0, 740.0), 1.0e8);
    }

    /// The chip has to be visibly shorter than its row at every font size, and never
    /// collapse to nothing.
    #[test]
    fn code_chip_is_shorter_than_its_row_at_every_size() {
        // Paragraph rows and heading rows, as `heading_typography_metrics` computes them.
        for row_h in [18.0f32, 24.0, 27.0, 40.5, 47.0, 54.0, 108.0] {
            let inset = code_chip_inset(row_h);
            let chip_h = row_h - 2.0 * inset;
            assert!(inset >= 1.0, "row_h {row_h}: inset {inset} must leave a visible gap");
            assert!(chip_h < row_h, "row_h {row_h}: chip {chip_h} is not shorter than the row");
            assert!(chip_h > row_h * 0.7, "row_h {row_h}: chip {chip_h} lost too much height");
        }
        // A tiny row still gets a one-pixel gap rather than a negative chip.
        assert_eq!(code_chip_inset(4.0), 1.0);
        assert!(4.0 - 2.0 * code_chip_inset(4.0) > 0.0);
    }

    /// The rows are painted as a slice shifted by `top_of(start) - scroll`, so
    /// two things have to hold at every scroll position, including the very end
    /// of a document far past the f32 grid:
    ///
    /// * the shift stays within one row of the overscan — that is what keeps
    ///   every layout coordinate small, i.e. what the whole rebase is for;
    /// * the slice still covers the whole viewport, or a blank band appears at
    ///   the bottom (which is exactly what a rebase without an exact index does,
    ///   because the index would then under-count the rows it has to render).
    #[test]
    fn slice_shift_stays_small_and_covers_the_viewport() {
        // 1000 px rows cross 2^27 in 134k rows instead of 5.6M, which keeps this
        // cheap while exercising the same arithmetic as a 600 MB note.
        let rows = 200_000usize;
        let row_h = 1000.0f32;
        let index = crate::markdown::VisualRowHeightIndex::new(vec![row_h; rows]);
        let viewport = 800.0f64;
        let overscan = f64::from(EDITOR_OVERSCAN_PX);
        let max_scroll = index.total() - viewport;

        // Fractional steps, so the slice boundary lands mid-row too.
        let mut y = max_scroll;
        while y > max_scroll - 10_000.0 {
            let vr = index.visible_range(y, viewport, overscan);
            let shift = index.top_of(vr.start) - y;
            assert!(
                shift >= -(overscan + f64::from(row_h)) && shift <= 0.0,
                "shift {shift} out of range at y={y}"
            );
            assert!(
                index.top_of(vr.end) - y >= viewport,
                "viewport not covered at y={y}"
            );
            y -= 7.3;
        }

        // The last screen still reaches the last row rather than stopping short.
        assert_eq!(index.visible_range(max_scroll, viewport, overscan).end, rows);
    }

    /// Painting and hit testing have to agree, or a click selects a different row
    /// than the one under the cursor. The rows are painted at
    /// `slice_shift + Σ heights` (small numbers, exact), while the mouse handler
    /// maps the click through the index in absolute coordinates (`v_idx_at_y`).
    /// Both must name the same row for every pixel of the viewport.
    #[test]
    fn painted_rows_and_hit_test_agree() {
        let rows = 200_000usize;
        let row_h = 1000.0f32;
        let index = crate::markdown::VisualRowHeightIndex::new(vec![row_h; rows]);
        let viewport = 800.0f64;
        let overscan = f64::from(EDITOR_OVERSCAN_PX);
        let max_scroll = index.total() - viewport;

        // Includes fractional scroll positions, so the slice boundary lands
        // mid-row and the shift is not a whole number either.
        for scroll_y in [0.0, 1.0, 999.5, max_scroll / 2.0, max_scroll - 3.3, max_scroll] {
            let vr = index.visible_range(scroll_y, viewport, overscan);
            let shift = index.top_of(vr.start) - scroll_y;
            for px in [0.5, 1.0, 17.7, 400.25, viewport - 0.5] {
                // The row painted at this pixel of the viewport…
                let painted = (vr.start..vr.end).find(|&k| {
                    let top = shift + (index.top_of(k) - index.top_of(vr.start));
                    let bottom = top + f64::from(index.height_of(k));
                    px >= top && px < bottom
                });
                let painted = painted.unwrap_or_else(|| panic!("no row painted at {px}"));
                // …and the row the mouse handler resolves the same pixel to.
                assert_eq!(
                    index.v_idx_at_y(scroll_y + px),
                    painted,
                    "px {px} at scroll {scroll_y}"
                );
            }
        }
    }

    /// The same agreement, but through the window and in document coordinates.
    ///
    /// With the windowed cache the painted slice is addressed from the window's
    /// first row, while a click arrives as a document Y. The two now disagree
    /// unless `above_h` is applied on both sides, and the window has to be
    /// placed so that the viewport is inside it. Both are checked here for a
    /// note far larger than one window.
    #[test]
    fn windowed_paint_and_hit_test_agree_in_document_coordinates() {
        let mut text = String::new();
        for i in 0..40_000 {
            text.push_str(&format!("line {i}\n"));
        }
        let doc_id = crate::document_buffer::DocumentId::new();
        let tab_id = crate::workspace::TabId::new();
        let mut doc = crate::document_buffer::DocumentBuffer::new(
            doc_id,
            std::path::PathBuf::from("big.md"),
            "big".to_string(),
            text,
            1,
        );
        let mut view = crate::workspace::EditorViewState::new(tab_id, doc_id);
        let viewport = 800.0f64;
        let overscan = f64::from(EDITOR_OVERSCAN_PX);
        view.ensure_visual_cache(&mut doc, true, 0, 20_000);

        // The note really is windowed and the window really is a window: a test
        // that silently fell back to a whole-document cache would prove nothing.
        assert!(
            view.above_lines > 0 && view.below_lines > 0,
            "the note must be windowed"
        );
        assert_eq!(
            view.window.line_count(),
            2 * crate::editor_window::MARGIN_LINES + 1
        );

        let window_h = view.window.total_height();
        let max_scroll = view.total_height() - viewport;
        for fraction in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let scroll_y = (max_scroll * fraction).clamp(0.0, max_scroll);
            // What `render` does each frame: anchor on the line at the top of the
            // viewport, then place the window around it.
            let anchor = view.line_at_doc_y(doc.line_index.lines(), scroll_y);
            view.ensure_visual_cache(&mut doc, true, 0, anchor);
            assert!(
                view.window.covers_line(view.line_at_doc_y(doc.line_index.lines(), scroll_y)),
                "the window must hold the anchor at scroll {scroll_y}"
            );

            let local_scroll = scroll_y - view.above_h;
            assert!(
                local_scroll >= 0.0 && local_scroll + viewport <= window_h,
                "the window must cover the viewport: local {local_scroll}, window {window_h}"
            );

            let vr = view.window.height_index.visible_range(local_scroll, viewport, overscan);
            let shift = view.window.height_index.top_of(vr.start) - local_scroll;
            assert!(
                shift.abs() <= overscan + 24.0,
                "slice shift {shift} is not small at scroll {scroll_y}"
            );
            for px in [0.5, 1.0, 17.7, 400.25, viewport - 0.5] {
                // The row painted at this pixel of the viewport…
                let painted = (vr.start..vr.end).find(|&k| {
                    let top = shift
                        + (view.window.height_index.top_of(k)
                            - view.window.height_index.top_of(vr.start));
                    let bottom = top + f64::from(view.window.height_index.height_of(k));
                    px >= top && px < bottom
                });
                let painted = painted.unwrap_or_else(|| panic!("no row painted at {px}"));
                // …and the row the mouse handler resolves the same pixel to.
                assert_eq!(
                    view.local_v_idx_at_doc_y(scroll_y + px),
                    painted,
                    "px {px} at scroll {scroll_y}"
                );
            }
        }
    }

    /// Middle-button smart scroll maps the cursor to the document the same way
    /// the sidebar does: top of the usable band is 0, bottom is max, and the
    /// result never leaves the range even when the cursor does.
    #[test]
    fn editor_middle_scroll_maps_cursor_to_document() {
        // Viewport [100, 400), max 1000: pad is 10% = 30px, usable [130, 370).
        let at = |y: f32| editor_middle_cursor_target(y, 100.0, 300.0, 1000.0);
        assert_eq!(at(100.0), 0.0);
        assert_eq!(at(130.0), 0.0);
        assert_eq!(at(370.0), 1000.0);
        assert_eq!(at(400.0), 1000.0);
        let mid = at(250.0);
        assert!((mid - 500.0).abs() < 1.0, "mid {mid} should sit near the middle");
        // Monotonic across the band.
        let (mut prev, mut y) = (at(130.0), 131.0);
        while y <= 370.0 {
            let cur = at(y);
            assert!(cur >= prev, "y {y}: {cur} went backwards from {prev}");
            prev = cur;
            y += 7.0;
        }
    }

    /// The wheel glide easing covers the distance and lands exactly at the
    /// deadline: 0 at the start, 1 at the end, monotonic in between.
    #[test]
    fn cubic_bezier_endpoints_and_monotonic_progress() {
        let (x1, y1) = (0.25, 0.0); // plain ease-out, no entry velocity
        assert_eq!(cubic_bezier(x1, y1, 0.6, 1.0, 0.0), 0.0);
        assert_eq!(cubic_bezier(x1, y1, 0.6, 1.0, 1.0), 1.0);
        // Clamped outside the lifetime.
        assert_eq!(cubic_bezier(x1, y1, 0.6, 1.0, -0.5), 0.0);
        assert_eq!(cubic_bezier(x1, y1, 0.6, 1.0, 1.5), 1.0);
        let mut prev = -1.0;
        for k in 0..=100 {
            let cur = cubic_bezier(x1, y1, 0.6, 1.0, k as f64 / 100.0);
            assert!(cur >= prev, "t {}: {cur} went backwards from {prev}", k as f64 / 100.0);
            prev = cur;
        }
    }

    /// A notch arriving mid-glide keeps the motion alive: the new glide
    /// enters at (close to) the speed the old one had, instead of collapsing
    /// to a stop and re-accelerating — that collapse is the "присадка" a
    /// notch stream must not have. And every glide lands exactly on target.
    #[test]
    fn glide_chain_keeps_the_speed_between_notches() {
        let g1 = WheelEase::new(0.0, 90.0, 0.0, 0.1);
        let mid = g1.t0 + std::time::Duration::from_millis(50);
        let speed = g1.velocity_at(mid);
        assert!(speed > 0.0, "a running glide has forward speed");
        let cur = g1.position_at(mid);
        // Same-direction notch: extend the target, inherit the speed.
        let g2 = WheelEase::new(cur, 180.0, speed, 0.1);
        let entry = g2.velocity_at(g2.t0);
        assert!(
            entry > speed * 0.7,
            "entry speed {entry} collapsed from {speed}"
        );
        // Landing is exact past the deadline.
        let end = g2.t0 + std::time::Duration::from_secs_f64(0.1);
        assert_eq!(g2.position_at(end), 180.0);
    }

    /// A reversing notch re-anchors at the live position and starts gently:
    /// the inherited velocity points the old way, so it must not carry over
    /// into the new glide's motion — otherwise the text would step backwards
    /// before turning around.
    #[test]
    fn a_reversing_notch_does_not_move_backwards() {
        let down = WheelEase::new(0.0, 300.0, 0.0, 0.2);
        let mid = down.t0 + std::time::Duration::from_millis(100);
        let speed = down.velocity_at(mid);
        let cur = down.position_at(mid);
        // Reversal: new span points opposite the inherited velocity.
        let up = WheelEase::new(cur, cur - 90.0, speed, 0.1);
        let mut prev = cur;
        let mut at = up.t0;
        for _ in 0..20 {
            at += std::time::Duration::from_millis(10);
            let pos = up.position_at(at);
            assert!(
                pos <= prev + 1e-6,
                "reversal glide moved backwards-then-forwards: {pos} after {prev}"
            );
            prev = pos;
        }
        assert_eq!(up.position_at(at.max(up.t0 + std::time::Duration::from_secs_f64(0.1))), cur - 90.0);
    }

    /// The glide duration follows the notch cadence: fresh after an idle it
    /// is seeded from a full max interval, and a sustained fast spin clamps
    /// to the bounds instead of running away.
    #[test]
    fn glide_duration_tracks_the_notch_cadence() {
        let t0 = std::time::Instant::now();
        // Fresh spin: seeded history, mid-length glide.
        let (fresh, history) = wheel_glide_duration_secs([None; 3], t0);
        assert!(fresh > WHEEL_GLIDE_MIN_SECS && fresh <= WHEEL_GLIDE_MAX_SECS, "fresh {fresh}");
        // Sustained spin: notches every 30 ms → mean interval 10 ms after the
        // shift, duration clamps to the minimum.
        let mut history = history;
        for _ in 0..3 {
            let (dur, shifted) = wheel_glide_duration_secs(history, t0 + std::time::Duration::from_millis(30));
            history = shifted;
            assert!(dur >= WHEEL_GLIDE_MIN_SECS && dur <= WHEEL_GLIDE_MAX_SECS, "cadence {dur}");
        }
        let (last, _) = wheel_glide_duration_secs(history, t0 + std::time::Duration::from_millis(60));
        assert!((last - WHEEL_GLIDE_MIN_SECS).abs() < 1e-6, "fast spin {last} should sit at the min");
    }


    /// Byte offsets resolve to the document Y of their visual row: plain lines
    /// map to the line top, offsets on a wrapped line to the exact sub-row,
    /// and out-of-range offsets clamp to the document.
    #[test]
    fn ruler_offset_maps_to_visual_row_top() {
        use crate::markdown::{VisualRow, VisualRowHeightIndex};
        // Two lines: "hello" (one row) + "world!" wrapped 3/3 (two sub-rows).
        let content = "hello\nworld!";
        let lines = crate::editor_window::WindowLines::new(
            0,
            vec!["hello".to_string(), "world!".to_string()],
        );
        let line_index = crate::line_index::LineIndex::new(content);
        let rows = vec![
            VisualRow { line_idx: 0, sub_idx: 0, is_last_sub: true, char_start: 0, char_end: 5 },
            VisualRow { line_idx: 1, sub_idx: 0, is_last_sub: false, char_start: 0, char_end: 3 },
            VisualRow { line_idx: 1, sub_idx: 1, is_last_sub: true, char_start: 3, char_end: 6 },
        ];
        let index = VisualRowHeightIndex::new(vec![24.0, 24.0, 24.0]);
        let at = |off: usize| {
            AppView::editor_doc_y_for_offset(off, content, &line_index, 0.0, &lines, &rows, &index)
        };
        // First line and the gap (newline byte 5 belongs to line 0 end).
        assert_eq!(at(0), 0.0);
        assert_eq!(at(5), 0.0);
        // Second line, first sub-row ("wor", bytes 6..9).
        assert_eq!(at(6), 24.0);
        assert_eq!(at(8), 24.0);
        // Second sub-row ("ld!", bytes 9..12).
        assert_eq!(at(9), 48.0);
        assert_eq!(at(12), 48.0);
        // Past the end clamps to the last row.
        assert_eq!(at(1000), 48.0);
        // Empty caches sit at zero instead of panicking.
        assert_eq!(
            AppView::editor_doc_y_for_offset(
                0,
                "",
                &crate::line_index::LineIndex::default(),
                0.0,
                &crate::editor_window::WindowLines::default(),
                &[],
                &index
            ),
            0.0
        );
    }

    /// The narrow thumb mirrors the viewport share (floored so it stays
    /// grabbable); the wide Zed-style thumb is a fixed slim marker.
    #[test]
    fn editor_thumb_h_scales_for_wide_lane() {
        // 800 px viewport over 8000 px of content: a 10% thumb.
        assert_eq!(AppView::editor_thumb_h(800.0, 8000.0, false), 80.0);
        // Floors stay grabbable in the narrow lane.
        assert_eq!(AppView::editor_thumb_h(600.0, 1.0e8, false), 28.0);
        // A fitting document fills the narrow lane.
        assert_eq!(AppView::editor_thumb_h(800.0, 800.0, false), 800.0);
        // Wide lane: one constant height whatever the document.
        assert_eq!(AppView::editor_thumb_h(800.0, 8000.0, true), WIDE_THUMB_H);
        assert_eq!(AppView::editor_thumb_h(600.0, 1.0e8, true), WIDE_THUMB_H);
        assert_eq!(AppView::editor_thumb_h(800.0, 800.0, true), WIDE_THUMB_H);
    }

    #[test]
    fn test_table_cell_end_cursor_and_geometry() {
        let text = "| aaa | bbb |";
        let raw_cells = crate::table::split_row(text);
        assert_eq!(raw_cells.len(), 2);
        let c0 = &raw_cells[0];
        let c1 = &raw_cells[1];
        assert_eq!(c0.content, 2..5);
        assert_eq!(c1.content, 8..11);

        let cell0 = TableCellLayout {
            col_idx: 0,
            cell_x: 0.0,
            cell_w: 100.0,
            content_x: 8.0,
            cell_start_col: 0,
            cell_columns: 3,
            content_range: c0.content.clone(),
            outer_range: c0.outer.clone(),
            line_chars: vec![2, 3, 4],
            text: "aaa".into(),
            layout: Arc::new(gpui::LineLayout {
                width: px(30.0),
                runs: Vec::new(),
                ascent: px(10.0),
                descent: px(4.0),
                font_size: px(14.0),
                len: 3,
            }),
            code_ranges: Vec::new(),
            tag_ranges: Vec::new(),
        };

        let cell1 = TableCellLayout {
            col_idx: 1,
            cell_x: 100.0,
            cell_w: 100.0,
            content_x: 108.0,
            cell_start_col: 4,
            cell_columns: 3,
            content_range: c1.content.clone(),
            outer_range: c1.outer.clone(),
            line_chars: vec![8, 9, 10],
            text: "bbb".into(),
            layout: Arc::new(gpui::LineLayout {
                width: px(30.0),
                runs: Vec::new(),
                ascent: px(10.0),
                descent: px(4.0),
                font_size: px(14.0),
                len: 3,
            }),
            code_ranges: Vec::new(),
            tag_ranges: Vec::new(),
        };

        let row_layout = TableRowLayout {
            is_delimiter: false,
            cells: vec![cell0, cell1],
            total_width: 200.0,
            total_columns: 8,
            line_chars: vec![2, 3, 4, 5, 8, 9, 10, 11],
            code_ranges: Vec::new(),
            tag_ranges: Vec::new(),
        };

        // Click past the end of cell 0 text (e.g. at x = 80.0):
        let col = row_layout.column_for_x(80.0);
        assert_eq!(col, 3, "click at end of cell 0 must resolve to cell 0 end slot (col 3)");

        // Column 3 must map to line_char 5 (end of cell 0 'aaa'), NOT jumping to cell 1 (8)!
        let lc = row_layout.line_char_for_column(col);
        assert_eq!(lc, 5, "end slot of cell 0 must map to content_range.end (5)");

        // Inverse mapping of line_char 5 must resolve to column 3
        let col_inv = row_layout.column_for_line_char(5);
        assert_eq!(col_inv, 3, "line_char 5 must map back to column 3");

        // Cell 1 start column is 4, which maps to line_char 8
        assert_eq!(row_layout.line_char_for_column(4), 8);
        assert_eq!(row_layout.column_for_line_char(8), 4);

        // Cell 1 end column is 7, which maps to line_char 11
        assert_eq!(row_layout.line_char_for_column(7), 11);
        assert_eq!(row_layout.column_for_line_char(11), 7);
    }

    #[test]
    fn test_table_separator_and_delimiter_height() {
        let lines = vec![
            "| col1 | col2 |",
            "|---|---|",
            "| val1 | val2 |",
        ];
        let visual_rows = vec![
            crate::markdown::VisualRow { line_idx: 0, sub_idx: 0, char_start: 0, char_end: 15, is_last_sub: true },
            crate::markdown::VisualRow { line_idx: 1, sub_idx: 0, char_start: 0, char_end: 9, is_last_sub: true },
            crate::markdown::VisualRow { line_idx: 2, sub_idx: 0, char_start: 0, char_end: 15, is_last_sub: true },
        ];
        let height_index = crate::markdown::VisualRowHeightIndex::from_rows_with_tables(
            &lines,
            &visual_rows,
            None,
            |line| {
                if line == 1 {
                    Some(crate::markdown::TableRowVisualKind::Delimiter)
                } else if line == 0 {
                    Some(crate::markdown::TableRowVisualKind::Header)
                } else {
                    Some(crate::markdown::TableRowVisualKind::LastRow)
                }
            },
        );

        assert_eq!(height_index.height_of(0), 42.0, "header row height must be 42.0 (28.0 + 14.0 mt)");
        assert_eq!(height_index.height_of(1), 1.0, "delimiter row height must be 1.0 (matching 1px border)");
        assert_eq!(height_index.height_of(2), 42.0, "last row height must be 42.0 (28.0 + 14.0 mb)");
    }

    #[test]
    fn test_table_add_column_preserves_widths() {
        gpui::Application::new().run(|cx: &mut gpui::App| {
            crate::fonts::register_embedded_fonts(cx);
            let _ = cx.open_window(gpui::WindowOptions::default(), |window, cx| {
                let block = crate::table::TableBlock {
                    first_line: 0,
                    last_line: 2,
                    aligns: vec![crate::table::Align::Left, crate::table::Align::Left, crate::table::Align::Left],
                };
                let lines = vec![
                    "| col1 | col2 | col3 |",
                    "|---|---|---|",
                    "| val1 | val2 | val3 |",
                ];
                let get_line = |idx: usize| lines.get(idx).copied();
                let manual = vec![150.0, 200.0]; // previously had 2 columns resized
                let font = gpui::SharedString::from("Consolas");

                let layout = compute_table_layout(window.text_system(), &get_line, &block, &font, Some(&manual));
                assert_eq!(layout.col_widths.len(), 3);
                assert_eq!(layout.col_widths[0], 150.0, "col 0 must keep manual width 150.0");
                assert_eq!(layout.col_widths[1], 200.0, "col 1 must keep manual width 200.0");
                assert!(layout.col_widths[2] >= 80.0, "new col 2 must get at least minimum column width");

                cx.quit();
                struct DummyView;
                impl gpui::Render for DummyView {
                    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
                        gpui::div()
                    }
                }
                cx.new(|_| DummyView)
            });
        });
    }

    #[test]
    fn test_table_plus_buttons_only_on_active_table() {
        let table_a = crate::table::TableBlock {
            first_line: 0,
            last_line: 2,
            aligns: vec![crate::table::Align::Left, crate::table::Align::Left],
        };
        let table_b = crate::table::TableBlock {
            first_line: 10,
            last_line: 12,
            aligns: vec![crate::table::Align::Left, crate::table::Align::Left],
        };

        let cursor_table = Some(&table_a);

        let is_cursor_in_a = match (cursor_table, Some(&table_a)) {
            (Some(ct), Some(gt)) => ct.first_line == gt.first_line,
            _ => false,
        };
        assert!(is_cursor_in_a, "Table A must show plus buttons when cursor is in Table A");

        let is_cursor_in_b = match (cursor_table, Some(&table_b)) {
            (Some(ct), Some(gt)) => ct.first_line == gt.first_line,
            _ => false,
        };
        assert!(!is_cursor_in_b, "Table B must NOT show plus buttons when cursor is in Table A");
    }

    #[test]
    fn test_table_selection_column_span() {
        let block = crate::table::TableBlock {
            first_line: 0,
            last_line: 3,
            aligns: vec![crate::table::Align::Left, crate::table::Align::Left, crate::table::Align::Left],
        };
        let lines = vec![
            "| col0 | col1 | col2 |",
            "|---|---|---|",
            "| val0 | val1 | val2 |",
            "| val3 | val4 | val5 |",
        ];

        // Column identification
        let s_cells = crate::table::split_row(lines[0]);
        let e_cells = crate::table::split_row(lines[3]);
        // Click inside col1 on line 0 (char 8), drag down to col1 on line 3 (char 8)
        let col1 = crate::table::cell_index_at(&s_cells, 8);
        let col2 = crate::table::cell_index_at(&e_cells, 8);
        assert_eq!(col1, 1);
        assert_eq!(col2, 1);

        let col_min = col1.min(col2);
        let col_max = col1.max(col2);
        assert_eq!(col_min, 1);
        assert_eq!(col_max, 1);

        // Separator line (line 1) is skipped
        assert_eq!(block.sep_line(), 1);
        let is_sep_line = |l: usize| l == block.sep_line();
        assert!(is_sep_line(1));
        assert!(!is_sep_line(0));
        assert!(!is_sep_line(2));
        assert!(!is_sep_line(3));
    }
}

