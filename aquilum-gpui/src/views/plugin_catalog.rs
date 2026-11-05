use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    div, point, prelude::*, px, rgb, rgba, AnyElement, App, Context, Div, ElementId, FocusHandle,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    ParentElement, Render, ScrollHandle, ScrollWheelEvent, SharedString,
    StatefulInteractiveElement, Styled, WeakEntity, Window, WindowControlArea,
    WindowDecorations, WindowOptions,
};

use crate::blink_manager::BlinkTarget;
use crate::icons;
use crate::markdown::InlineSpan;
use crate::theme::Theme;
use crate::views::app_view::AppView;
use crate::views::goto_search::{highlight_query_terms, highlight_terms, word_range_at};
use crate::views::scrollbar::{
    overlay_scrollbar, ScrollbarDrag, ScrollbarGeometry, ScrollbarStyle,
};
pub use crate::plugins::registry::CatalogPluginEntry;

mod actions;
mod state;
mod view;

pub use state::*;
pub use view::*;

#[cfg(test)]
mod tests;
