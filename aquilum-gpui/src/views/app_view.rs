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

mod physics;
mod table_layout;
mod row_shape;
mod paint;
mod render;
mod central_editor;
mod tab_bar;
mod tabs;
mod keys;
mod title;
mod plugins;
mod mouse;
mod table_ops;
mod nav;
mod find;
mod notes;
mod geometry;
mod lifecycle;

pub use self::physics::WheelEase;
pub(crate) use self::table_layout::*;
pub(crate) use self::row_shape::{shape_row, RowShapedLayout};
use self::row_shape::{headings_cache_key, selection_chars_in_line, LineKindTag};
use self::paint::*;
use self::physics::{editor_middle_cursor_target, wheel_glide_duration_secs, WIDE_THUMB_H};
#[cfg(test)]
use self::physics::{cubic_bezier, WHEEL_GLIDE_MAX_SECS, WHEEL_GLIDE_MIN_SECS};

/// Length of the settle burst that repaints the editor until its scroll handle
/// has been measured (see `layout_settle_frames`). Three frames is enough: one
/// drawn from the estimates, one from the freshly measured handle, one that
/// confirms nothing moved.
pub(crate) const SCROLL_SETTLE_FRAMES: u8 = 3;


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
    pub source_mode: bool,
    pub editor_blink: BlinkManager,
    pub editor_is_focused: bool,
    /// Last observed OS window activation state. gpui tracks focus per window and never
    /// clears it when the window is deactivated, so this is the only way to notice that
    /// the app went to the background and stop blinking there.
    pub window_was_active: bool,
    pub is_mouse_selecting: bool,
    pub mouse_drag_started: bool,
    pub mouse_down_pos: Option<(f32, f32)>,
    pub ctrl_preview_selection: bool,
    pub ctrl_held: bool,
    pub hovered_link_id: Option<String>,
    pub hovered_tag: bool,
    /// Image block under the pointer (its line) — the hover toolbar follows it.
    pub hovered_image_line: Option<usize>,
    /// Image block clicked without moving the caret; keeps the toolbar open.
    pub selected_image_line: Option<usize>,
    /// Floating size/radius editor for the selected image.
    pub image_popover: Option<crate::views::image_ui::ImagePopoverState>,
    pub size_drag: Option<crate::views::image_ui::SizeDrag>,
    /// Document the selected image line belongs to, so switching notes does
    /// not leave a selection frame on an unrelated line.
    pub selected_image_doc: Option<crate::document_buffer::DocumentId>,
    pub hovered_image_doc: Option<crate::document_buffer::DocumentId>,
    /// Full-window preview of the original image file.
    pub image_viewer: Option<crate::views::image_ui::ImageViewerState>,
    /// In-place crop/annotation editor for one image block.
    pub image_editor: Option<crate::views::image_editor::ImageEditorState>,
    pub caption_editor: Option<crate::views::image_editor::CaptionEditorState>,
    pub compress_dialog: Option<crate::views::image_compress::CompressDialogState>,
    pub export_dialog: Option<crate::views::export_dialog::ExportDialogState>,
    pub unused_attachments: Option<crate::views::unused_attachments::UnusedAttachmentsState>,
    /// Last painted image rectangle in window coordinates, for the editor
    /// panels that float next to it.
    pub image_editor_rect: Cell<Option<(f32, f32, f32, f32)>>,
    /// Caption numbers for the current document version (figure numbering).
    pub caption_numbers_cache: std::cell::RefCell<
        Option<(
            crate::document_buffer::DocumentId,
            u64,
            u64,
            std::sync::Arc<std::collections::HashMap<usize, u32>>,
        )>,
    >,
    pub sidebar_width: f32,
    pub sidebar_collapsed: bool,
    pub is_resizing_sidebar: bool,
    pub is_hovering_resizer: bool,
    pub selected_sidebar_folder: Option<PathBuf>,
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
    pub show_invisibles: bool,
    pub last_wrap_cols: usize,
    pub is_settings_open: bool,
    pub settings_tab: crate::views::settings_view::SettingsTab,
    pub settings_model: gpui::Entity<crate::settings::SettingsModel>,
    pub settings_window_manager: crate::views::settings_view::SettingsWindowManager,
    pub is_dragging_scrollbar: bool,
    pub scrollbar_drag_start_y: f32,
    pub scrollbar_drag_start_scroll_y: f64,
    pub is_dragging_note_width_slider: bool,
    pub note_width_slider_origin_x: Rc<Cell<f32>>,
    /// Logical width of the window the editor lives in.
    pub window_width_px: f32,
    /// Logical width of the monitor (physical resolution divided by OS scale).
    pub monitor_width_px: f32,
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
    /// Sequence number of the last dialog search request; replies with a
    /// smaller number are stale and dropped (the scan runs on the worker).
    pub(crate) goto_search_seq: u64,
    /// Debounce epoch of the dialog input: every keystroke re-arms the timer,
    /// only the newest arm fires the scan.
    pub(crate) goto_debounce_epoch: usize,
    /// Sequence number of the last sidebar search request (same scheme as
    /// `goto_search_seq`).
    pub(crate) panel_search_seq: u64,
    /// Debounce epoch of the sidebar search input.
    pub(crate) panel_debounce_epoch: usize,
    /// Last index-build percent shown in the status bar (`None` — idle);
    /// change detection for the 150 ms poll loop's notify.
    last_build_progress: Option<u8>,
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
    /// Image line whose inline caption editor should be scrolled into view on
    /// the next frame (the editor sits at the bottom of a possibly tall row).
    pub pending_caption_reveal: Option<usize>,
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
    /// In-editor find/replace bar shown under the note title (Ctrl+F).
    pub find_bar: crate::views::find_bar::FindBarState,
    pub find_query_focus_handle: FocusHandle,
    pub find_replace_focus_handle: FocusHandle,
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
    pub more_options_menu: crate::views::more_options_menu::MoreOptionsMenuState,
    pub ribbon_context_menu: crate::views::ribbon_menu::RibbonContextMenuState,
    pub editor_context_menu: crate::views::editor_context_menu::ContextMenuState,
    pub toolbar_view: crate::views::toolbar_view::ToolbarViewState,
    /// Drag state of the top/bottom editor toolbar icons.
    pub toolbar_drag: Option<crate::views::toolbar_view::ToolbarDragState>,
    /// Context menu of the editor toolbar's empty area.
    pub toolbar_context_menu: crate::views::toolbar_view::ToolbarContextMenuState,
    pub ribbon_dragged_id: Option<String>,
    pub ribbon_drag_start_y: Option<f32>,
    pub ribbon_is_dragging: bool,
    /// Ribbon icon whose slot is highlighted as the drop position.
    pub ribbon_drop_id: Option<String>,
    /// Current pointer position in window coordinates; drives the ribbon drag ghost.
    pub ribbon_drag_pointer_x: f32,
    pub ribbon_drag_pointer_y: f32,
    /// Sidebar content mode (file tree vs Obsidian-style search panel).
    pub sidebar_mode: crate::views::sidebar::SidebarMode,
    pub sidebar_search: crate::views::sidebar::SidebarSearchState,
    pub search_panel_focus_handle: FocusHandle,
    pub panel_scroll_handle: ScrollHandle,
    /// Drag state of the search panel results overlay scrollbar.
    pub panel_scrollbar_drag: crate::views::scrollbar::ScrollbarDrag,
    pub bookmark_drag: Option<crate::views::sidebar::bookmarks::BookmarkDragState>,
    /// Drag state of the file explorer tree (moving files/folders into other folders).
    pub sidebar_tree_drag: Option<crate::views::sidebar::tree::SidebarTreeDragState>,
    /// File just moved by drag-and-drop; its row flashes in the tree for a second.
    pub sidebar_move_flash: Option<PathBuf>,
    /// Increments per flash so the fade animation restarts when the same path is moved again.
    pub sidebar_move_flash_seq: u64,
    pub expanded_bookmark_groups: std::collections::HashSet<String>,
    pub bookmark_context_menu: Option<crate::views::sidebar::bookmarks::BookmarkContextMenuState>,
    pub bookmark_rename: Option<crate::views::sidebar::bookmarks::BookmarkRenameState>,
    pub bookmark_rename_focus_handle: FocusHandle,
    pub bookmark_rename_blink: BlinkManager,
    /// Commits the inline rename when its field loses focus to another control.
    pub bookmark_rename_blur_subscription: Option<Subscription>,
    pub recent_files_sort_order: crate::views::sidebar::recent_files::RecentFilesSortOrder,
    pub recent_files_scroll_handle: ScrollHandle,
    pub recent_files_scrollbar_drag: crate::views::scrollbar::ScrollbarDrag,
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
    /// Compatibility filter dropdown of the in-settings catalog panel
    /// (the shared `TabListPopup`; the standalone catalog window keeps its
    /// own copy on `PluginCatalogView`).
    pub catalog_compat_popup: Option<gpui::Entity<crate::views::tab_list::TabListPopup>>,
    /// Keeps the panel compat dropdown's event subscription alive.
    pub catalog_compat_popup_subscription: Option<Subscription>,
    /// Active interactive text input in plugin UI (plugin_id, view_id, node_id, value, cursor_pos).
    pub active_plugin_input: Option<crate::plugins::ui::PluginInputFocusState>,
    pub plugin_input_focus_handle: FocusHandle,
    pub plugin_input_blink: BlinkManager,
    /// Active interactive dropdown select popup in plugin UI (plugin_id, view_id, node_id, value, cursor_pos).
    pub active_plugin_select: Option<(String, u64, String)>,
}

impl AppView {
    /// Creates and initializes a new AppView instance with loaded configuration.
    /// `vault_override` opens that vault for this run without persisting the
    /// session, so the default vault stays unchanged.
    pub fn new(cx: &mut Context<Self>, vault_override: Option<PathBuf>) -> Self {
        let state = AppState::new_with_vault(vault_override);
        let config = state.settings.get_config();
        let theme = Theme::from_ui(&config.ui, None);
        let ws = crate::state::WindowState::load(&crate::state::app_data_dir());
        let last_saved_geometry = Some((ws.width, ws.height, ws.x, ws.y, ws.maximized));

        let mut edit_mode = true;
        let mut word_wrap = true;
        let show_invisibles = config.editor.show_invisibles;
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
        let recent_files_sort_order = state.session.recent_files_sort_order;
        let settings_model = cx.new(|_cx| {
            crate::settings::SettingsModel::new(state.settings.clone())
        });

        // The find fields must own the same handles the rendered boxes track,
        // otherwise focus and keystrokes go to a handle nobody is bound to.
        let find_query_focus_handle = cx.focus_handle();
        let find_replace_focus_handle = cx.focus_handle();
        // The replace row comes back exactly as it was left when the app
        // closed, so the expander's choice survives a restart.
        let find_replace_open = state.session.find_replace_open;

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
            source_mode: false,
            editor_blink: BlinkManager::new(),
            editor_is_focused: true,
            window_was_active: true,
            is_mouse_selecting: false,
            mouse_drag_started: false,
            mouse_down_pos: None,
            ctrl_preview_selection: false,
            ctrl_held: false,
            hovered_link_id: None,
            hovered_tag: false,
            hovered_image_line: None,
            selected_image_line: None,
            image_popover: None,
            size_drag: None,
            selected_image_doc: None,
            hovered_image_doc: None,
            image_viewer: None,
            image_editor: None,
            caption_editor: None,
            compress_dialog: None,
            export_dialog: None,
            unused_attachments: None,
            image_editor_rect: Cell::new(None),
            caption_numbers_cache: std::cell::RefCell::new(None),
            sidebar_width,
            sidebar_collapsed: false,
            is_resizing_sidebar: false,
            is_hovering_resizer: false,
            selected_sidebar_folder: None,
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
            show_invisibles,
            last_wrap_cols: 80,
            is_settings_open: false,
            settings_tab: crate::views::settings_view::SettingsTab::Ui,
            settings_model,
            settings_window_manager: crate::views::settings_view::SettingsWindowManager::default(),
            is_dragging_scrollbar: false,
            scrollbar_drag_start_y: 0.0,
            scrollbar_drag_start_scroll_y: 0.0,
            is_dragging_note_width_slider: false,
            note_width_slider_origin_x: Rc::new(Cell::new(0.0)),
            window_width_px: 1920.0,
            monitor_width_px: 1920.0,
            is_hovering_editor_scrollbar: false,
            is_hovering_editor_thumb: false,
            sidebar_scrollbar_drag: ScrollbarDrag::default(),
            sidebar_smart_bounds: Rc::new(Cell::new(Bounds::default())),
            sidebar_smart_target_y: Cell::new(None),
            sidebar_smart_last_mouse_y: Cell::new(None),
            goto_search_seq: 0,
            goto_debounce_epoch: 0,
            panel_search_seq: 0,
            panel_debounce_epoch: 0,
            last_build_progress: None,
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
            pending_caption_reveal: None,
            need_reveal_cursor: false,
            layout_settle_frames: Cell::new(SCROLL_SETTLE_FRAMES),
            last_scroll_inputs: Cell::new((0.0, 0.0)),
            is_hovering_close: false,
            title_focus_handle: cx.focus_handle(),
            was_title_focused: false,
            title_blink: BlinkManager::new(),
            find_bar: {
                let mut find_bar = crate::views::find_bar::FindBarState::new(
                    find_query_focus_handle.clone(),
                    find_replace_focus_handle.clone(),
                );
                find_bar.replace_open = find_replace_open;
                find_bar
            },
            find_query_focus_handle,
            find_replace_focus_handle,
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
            more_options_menu: crate::views::more_options_menu::MoreOptionsMenuState::default(),
            ribbon_context_menu: crate::views::ribbon_menu::RibbonContextMenuState::default(),
            editor_context_menu: crate::views::editor_context_menu::ContextMenuState::default(),
            toolbar_view: crate::views::toolbar_view::ToolbarViewState::default(),
            toolbar_drag: None,
            toolbar_context_menu: crate::views::toolbar_view::ToolbarContextMenuState::default(),
            ribbon_dragged_id: None,
            ribbon_drag_start_y: None,
            ribbon_is_dragging: false,
            ribbon_drop_id: None,
            ribbon_drag_pointer_x: 0.0,
            ribbon_drag_pointer_y: 0.0,
            sidebar_mode,
            sidebar_search: crate::views::sidebar::SidebarSearchState::default(),
            search_panel_focus_handle: cx.focus_handle(),
            panel_scroll_handle: ScrollHandle::new(),
            panel_scrollbar_drag: crate::views::scrollbar::ScrollbarDrag::default(),
            bookmark_drag: None,
            sidebar_tree_drag: None,
            sidebar_move_flash: None,
            sidebar_move_flash_seq: 0,
            expanded_bookmark_groups: std::collections::HashSet::new(),
            bookmark_context_menu: None,
            bookmark_rename: None,
            bookmark_rename_focus_handle: cx.focus_handle(),
            bookmark_rename_blink: BlinkManager::new(),
            bookmark_rename_blur_subscription: None,
            recent_files_sort_order,
            recent_files_scroll_handle: ScrollHandle::new(),
            recent_files_scrollbar_drag: crate::views::scrollbar::ScrollbarDrag::default(),
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
            catalog_compat_popup: None,
            catalog_compat_popup_subscription: None,
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
        crate::plugins::editor_toolbar::sync_editor_toolbar_with_vault(&this.state, this.state.vault.as_ref());

        cx.subscribe(&this.settings_model, |this, model, event: &crate::settings::SettingsEvent, cx| {
            match event {
                crate::settings::SettingsEvent::ConfigChanged => {
                    this.config = model.read(cx).config.clone();
                    this.show_invisibles = this.config.editor.show_invisibles;
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
                    let ui = model.read(cx).config.ui.clone();
                    let theme = crate::theme::Theme::from_ui(&ui, None);
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
        // The gesture continues from the widths that are painted right now: when
        // the room shrank, the manual values were already scaled down for display,
        // and starting from the previous (larger) width would make the divider
        // jump on the first move.
        self.manual_table_col_widths
            .insert(table_first_line, current_widths.to_vec());
        let start_width = current_widths
            .get(col_idx)
            .copied()
            .unwrap_or(100.0);
        self.table_col_resize_drag = Some(TableColResizeDrag {
            table_first_line,
            col_idx,
            start_x: click_x,
            start_width,
        });
    }













    /// Paints the in-editor find/replace bar shown between the note title and
    /// the editor body.
    pub(super) fn render_find_bar(&self, leaf_id: Option<crate::workspace::LeafId>, theme: &Theme, window: &mut Window, cx: &Context<Self>) -> Div {
        let pixel_scale = window.scale_factor();
        let font_size = 13.0;
        let font_family = "Segoe UI";
        let query_focused = self.find_bar.query.is_focused(window);
        let replace_focused = self.find_bar.replace.is_focused(window);
        let has_matches = !self.find_bar.matches.is_empty();
        // A single replacement and a whole-document pass cannot both own the
        // document; the pass wins and the button waits for it.
        let replace_one_ready = has_matches && self.find_bar.replace_all.is_none();

        // Matching switches at the right of the query: `Aa` for case and an
        // underlined `ab` for whole words, the same pair the editor's own find
        // popup shows. They are text, not new icons, so they keep the app's
        // icon set unchanged.
        let option_button = |id: &'static str, label: &'static str, active: bool, underline: bool| {
            div()
                .id(id)
                .h(px(20.0))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .px_1()
                .rounded_xs()
                .cursor_pointer()
                .when(active, |d| d.bg(theme.bg_hover))
                .when(!active, |d| d.hover(|h| h.bg(theme.bg_hover)))
                .child(
                    div()
                        .font_family("Segoe UI")
                        .text_xs()
                        .text_color(if active { theme.accent } else { theme.text_muted })
                        .when(underline, |d| d.underline())
                        .child(label),
                )
        };
        let case_toggle = option_button(
            "find-match-case",
            "Aa",
            self.find_bar.case_sensitive,
            false,
        )
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(|this, _, window, cx| this.toggle_find_case(window, cx)));
        let word_toggle = option_button(
            "find-whole-word",
            "ab",
            self.find_bar.whole_word,
            true,
        )
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(|this, _, window, cx| this.toggle_find_whole_word(window, cx)));

        let query_box = div()
            .id("find-query-box")
            .track_focus(&self.find_query_focus_handle)
            .flex()
            .flex_1()
            .min_w(px(180.0))
            .flex_row()
            .items_center()
            .gap_1p5()
            .h(px(28.0))
            .px_2()
            .bg(theme.bg_card)
            .border_1()
            .border_color(if query_focused { theme.accent } else { theme.border })
            .rounded_md()
            .cursor(gpui::CursorStyle::IBeam)
            .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                this.find_bar.replace.blink.disable(cx);
                this.find_bar.query.focus(window);
                this.find_bar.query.handle_mouse_down(event, window, 13.0, "Segoe UI");
                this.find_bar.query.blink.pause_and_show(BlinkTarget::FindQuery, cx);
                cx.notify();
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                if this.find_bar.query.is_mouse_selecting {
                    this.find_bar.query.handle_mouse_move(event, window, 13.0, "Segoe UI");
                    cx.notify();
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.find_bar.query.handle_mouse_up();
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                cx.stop_propagation();
                this.handle_find_key(event, window, cx);
            }))
            .child(icons::icon(icons::ICON_SEARCH, 14.0, theme.text_muted))
            .child(self.find_bar.query.render(
                &self.t("editor.find"),
                theme,
                window,
                pixel_scale,
                font_size,
                font_family,
            ))
            .child(
                div()
                    .flex_shrink_0()
                    .font_family("Segoe UI")
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(self.find_bar.counter()),
            )
            .child(case_toggle)
            .child(word_toggle)
            // Clear button inside the field, after the count, like the other
            // in-field clear buttons of the app.
            .when(!self.find_bar.query.is_empty(), |d| {
                d.child(
                    div()
                        .id("find-query-clear")
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .cursor_pointer()
                        .rounded_sm()
                        .hover(|h| h.bg(theme.bg_hover))
                        .child(icons::icon(icons::ICON_CLOSE, 12.0, theme.text_muted))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(|this, _, window, cx| {
                            // Same as erasing the query by hand: the old ranges
                            // and any pending action belong to the old text.
                            this.find_bar.query.clear();
                            this.find_bar.pending_step = None;
                            this.find_bar.pending_after = None;
                            this.find_bar.pending_replace = false;
                            this.find_bar.cancel_replace_all();
                            this.find_bar.query.focus(window);
                            this.find_bar.query.blink.pause_and_show(BlinkTarget::FindQuery, cx);
                            this.request_find_search(cx);
                        })),
                )
            });

        let nav_button = |id: &'static str, icon: &'static str, active: bool| {
            div()
                .id(id)
                .w(px(24.0))
                .h(px(24.0))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded_sm()
                .cursor_pointer()
                .when(!active, |d| d.opacity(0.4))
                .when(active, |d| d.hover(|h| h.bg(theme.bg_hover)))
                .child(icons::icon(icon, 14.0, theme.text_secondary))
        };

        let prev = nav_button("find-prev", icons::ICON_ARROW_LEFT, has_matches)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| this.find_step(-1, cx)));
        let next = nav_button("find-next", icons::ICON_ARROW_RIGHT, has_matches)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| this.find_step(1, cx)));
        let toggle = nav_button(
            "find-toggle-replace",
            if self.find_bar.replace_open {
                icons::ICON_CHEVRON_DOWN
            } else {
                icons::ICON_CHEVRON_RIGHT
            },
            true,
        )
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(|this, _, _, cx| {
            this.find_bar.replace_open = !this.find_bar.replace_open;
            // Kept in the session as it changes, so any save on the way out
            // (or before it) records the state the user last chose.
            this.state.session.find_replace_open = this.find_bar.replace_open;
            cx.notify();
        }));
        let close = nav_button("find-close", icons::ICON_CLOSE, true)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, window, cx| this.close_find_bar(window, cx)));

        let window_w = f32::from(window.bounds().size.width);
        let left = self.find_bar_left_offset_for_leaf(leaf_id, window_w);

        let mut root = div()
            .w_full()
            .flex()
            .flex_col()
            .gap_1p5()
            // Bound to the note center while keeping at least 16px padding from pane edges.
            .pl(px(left))
            .pr(px(16.0))
            .pb(px(12.0))
            .bg(theme.bg_editor);

        // The expander sits left of the fields, so the query and replace
        // fields start on the same x. Both fields live in one column and both
        // groups of buttons in another, which keeps the two fields exactly the
        // same length whatever the buttons next to them measure.
        let mut fields = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_1p5()
            .child(
                div()
                    .w_full()
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .child(query_box),
            );
        let mut actions = div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            // The row gap only exists while both rows are drawn; the collapsed
            // measuring row has no height, so no gap may separate it.
            .when(self.find_bar.replace_open, |d| d.gap_1p5());

        // The replace buttons keep their place even while the second row is
        // collapsed: the actions column is as wide as this row whatever the
        // panel shows, so opening the replace field never shortens either
        // field. The buttons follow the fields instead of being pushed to the
        // right edge.
        let action_label = |text: String| {
            div()
                .font_family("Segoe UI")
                .text_xs()
                .text_color(theme.text_primary)
                .child(text)
        };
        let action_button = |id: &'static str, text: String| {
            div()
                .id(id)
                .h(px(24.0))
                .flex_shrink_0()
                .flex()
                .items_center()
                .px_2()
                .bg(theme.bg_card)
                .border_1()
                .border_color(theme.border)
                .rounded_sm()
                .child(action_label(text))
        };

        // "Mark all" sits right after the prev/next arrows: it belongs to the
        // query row, not to the replace row. Its own state is visible (accent
        // border while lines are marked), and a second press clears the marks.
        let marks_active = self
            .state
            .active_document()
            .map(|doc| self.find_bar.marks_valid_for(doc.id, doc.version))
            .unwrap_or(false);
        let mark_all_ready = has_matches || marks_active;
        let mut mark_all = action_button("find-mark-all", self.t("editor.mark_all"));
        if marks_active {
            mark_all = mark_all.bg(theme.bg_hover).border_color(theme.accent);
        }
        if mark_all_ready {
            mark_all = mark_all
                .cursor_pointer()
                .hover(|h| h.bg(theme.bg_hover))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(|this, _, _, cx| this.find_mark_all(cx)));
        } else {
            mark_all = mark_all.opacity(0.45).cursor_not_allowed();
        }

        actions = actions.child(
            div()
                .h(px(28.0))
                .flex()
                .flex_row()
                .items_center()
                .gap_1p5()
                .child(prev)
                .child(next)
                .child(mark_all)
                .child(close),
        );
        // Nothing found means nothing to replace: the whole-document pass
        // waits for a non-empty list instead of rewriting a document the
        // search just found no matches in.
        let replace_all_busy = self.find_bar.replace_all.is_some();
        let replace_all_ready = has_matches && !replace_all_busy;
        let replace_live = self.find_bar.replace_open;
        let mut replace_one = action_button("find-replace-one", self.t("editor.replace"));
        let mut replace_all = action_button("find-replace-all", self.t("editor.replace_all"));
        if replace_live {
            replace_one = replace_one
                .cursor_pointer()
                .when(!replace_one_ready, |d| d.opacity(0.45).cursor_not_allowed())
                .when(replace_one_ready, |d| d.hover(|h| h.bg(theme.bg_hover)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(|this, _, _, cx| this.find_replace_current(cx)));
            replace_all = replace_all
                .cursor_pointer()
                // A running pass is not dimmed: the replace field beside it
                // is showing its progress.
                .when(!replace_all_ready && !replace_all_busy, |d| {
                    d.opacity(0.45).cursor_not_allowed()
                })
                .when(replace_all_busy, |d| d.cursor_not_allowed())
                .when(replace_all_ready, |d| d.hover(|h| h.bg(theme.bg_hover)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(|this, _, _, cx| this.find_replace_all(cx)));
        }

        let mut replace_actions = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1p5()
            .child(replace_one)
            .child(replace_all);
        if replace_live {
            replace_actions = replace_actions.h(px(28.0));
        } else {
            // Collapsed: this row only measures the buttons so the actions
            // column keeps the width it has when the field is open. It takes
            // no height, has no listeners and no hitboxes, so nothing can be
            // clicked where nothing is drawn.
            replace_actions = replace_actions.h(px(0.0)).invisible();
        }
        actions = actions.child(replace_actions);

        if self.find_bar.replace_open {
            let replace_box = div()
                .id("find-replace-box")
                .track_focus(&self.find_replace_focus_handle)
                .flex()
                .flex_1()
                .min_w(px(180.0))
                .flex_row()
                .items_center()
                .gap_1p5()
                .h(px(28.0))
                .px_2()
                .bg(theme.bg_card)
                .border_1()
                .border_color(if replace_focused { theme.accent } else { theme.border })
                .rounded_md()
                .cursor(gpui::CursorStyle::IBeam)
                .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.find_bar.query.blink.disable(cx);
                    this.find_bar.replace.focus(window);
                    this.find_bar.replace.handle_mouse_down(event, window, 13.0, "Segoe UI");
                    this.find_bar.replace.blink.pause_and_show(BlinkTarget::FindReplace, cx);
                    cx.notify();
                }))
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                    if this.find_bar.replace.is_mouse_selecting {
                        this.find_bar.replace.handle_mouse_move(event, window, 13.0, "Segoe UI");
                        cx.notify();
                    }
                }))
                .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.find_bar.replace.handle_mouse_up();
                    cx.notify();
                }))
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.handle_find_key(event, window, cx);
                }))
                .child(icons::icon(icons::ICON_REFRESH, 13.0, theme.text_muted))
                .child(self.find_bar.replace.render(
                    &self.t("editor.replace_placeholder"),
                    theme,
                    window,
                    pixel_scale,
                    font_size,
                    font_family,
                ))
                // Progress of the whole-document pass lives at the right edge
                // of the field it rewrites, leaving the buttons to their names.
                .when_some(self.find_bar.replace_all_percent(), |d, percent| {
                    d.child(
                        div()
                            .flex_shrink_0()
                            .font_family("Segoe UI")
                            .text_xs()
                            .text_color(theme.text_secondary)
                            .child(percent),
                    )
                })
                .when(!self.find_bar.replace.is_empty(), |d| {
                    d.child(
                        div()
                            .id("find-replace-clear")
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .cursor_pointer()
                            .rounded_sm()
                            .hover(|h| h.bg(theme.bg_hover))
                            .child(icons::icon(icons::ICON_CLOSE, 12.0, theme.text_muted))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.find_bar.replace.clear();
                                this.find_bar.replace.focus(window);
                                this.find_bar.replace.blink.pause_and_show(BlinkTarget::FindReplace, cx);
                                cx.notify();
                            })),
                    )
                });

            fields = fields.child(
                div()
                    .w_full()
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .child(replace_box),
            );
        }

        root = root.child(
            div()
                .w_full()
                .max_w(px(620.0))
                // Centred over the note content instead of the window.
                .flex()
                .flex_row()
                .items_start()
                .gap_1p5()
                .child(
                    div()
                        .h(px(28.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .child(toggle),
                )
                .child(fields)
                .child(actions),
        );

        root
    }
}

/// FPS monitor — only for debug builds.
///
/// In release builds this is an empty function: in a Windows app with the "windows" subsystem
/// there is no console, and there is nowhere to write to stdout/stderr. We remove the last runtime writer
/// to the console, to eliminate its effect on idle CPU.
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

/// In release builds the monitor is disabled — not a single console write.
#[cfg(not(debug_assertions))]
fn log_fps_tick() {}











#[cfg(test)]
mod tests;
