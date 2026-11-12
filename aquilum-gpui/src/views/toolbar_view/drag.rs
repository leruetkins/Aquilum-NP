use gpui::{div, prelude::*, px, AnyElement, Context, Window};
use crate::icons;
use crate::theme::Theme;
use crate::views::app_view::AppView;

use super::ToolbarDragState;

impl AppView {
    /// Reorders an icon inside a toolbar and persists the bundled toolbar config.
    pub(crate) fn reorder_toolbar_item(
        &mut self,
        toolbar_id: &str,
        from_id: &str,
        to_id: &str,
        cx: &mut Context<Self>,
    ) {
        if from_id == to_id {
            return;
        }
        let Some(spec) = self.state.toolbars.get_toolbar(toolbar_id) else {
            return;
        };
        let mut items = spec.items;
        let Some(from) = items.iter().position(|item| item.id == from_id) else {
            return;
        };
        let Some(to) = items.iter().position(|item| item.id == to_id) else {
            return;
        };

        let item = items.remove(from);
        items.insert(to, item);
        self.state
            .toolbars
            .update_toolbar(toolbar_id, items.clone());

        if toolbar_id == crate::plugins::editor_toolbar::TOOLBAR_PLUGIN_ID {
            let root = self
                .state
                .vault
                .as_ref()
                .map(|vault| vault.root.clone())
                .or_else(|| self.state.workspace_path.clone());
            if let Some(root) = root {
                let _ = crate::plugins::editor_toolbar::save_toolbar_config(&root, &items);
            }
        }
        cx.notify();
    }

    /// Floating preview of the toolbar icon being reordered, drawn at the pointer.
    pub(crate) fn render_toolbar_drag_ghost(&self, theme: &Theme) -> Option<AnyElement> {
        let drag = self.toolbar_drag.as_ref().filter(|drag| drag.is_dragging)?;
        let spec = self.state.toolbars.get_toolbar(&drag.toolbar_id)?;
        let item = spec.items.iter().find(|item| item.id == drag.source_id)?;
        let icon = item.icon()?;
        let title = self.toolbar_item_label(item).unwrap_or_default();
        let icon_asset = icons::resolve_toolbar_icon(icon);

        Some(
            div()
                .absolute()
                .left(px(drag.pointer_x + 14.0))
                .top(px(drag.pointer_y + 10.0))
                .flex()
                .flex_row()
                .items_center()
                .gap_1p5()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(theme.bg_card)
                .border_1()
                .border_color(theme.accent)
                .shadow_lg()
                .opacity(0.92)
                .child(
                    gpui::svg()
                        .path(icon_asset)
                        .w(px(15.0))
                        .h(px(15.0))
                        .text_color(theme.accent)
                        .flex_shrink_0(),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.text_primary)
                        .child(title),
                )
                .into_any_element(),
        )
    }

    /// Arms a toolbar drag from the icon's own mouse-down. Keeping the press on the
    /// icon (instead of hit-testing the strip) means a click always finds its source:
    /// slot positions are window-coordinate estimates and may drift from the flex
    /// layout (overflow, centered rows, panes), which used to drop clicks entirely.
    fn arm_toolbar_drag(
        &mut self,
        toolbar_id: &str,
        item_id: &str,
        x: f32,
        y: f32,
        can_drag: bool,
        cx: &mut Context<Self>,
    ) {
        self.toolbar_view.hovered_tooltip = None;
        self.toolbar_drag = Some(ToolbarDragState {
            toolbar_id: toolbar_id.to_string(),
            source_id: item_id.to_string(),
            start_x: x,
            pointer_x: x,
            pointer_y: y,
            is_dragging: false,
            target_id: None,
            can_drag,
        });
        cx.notify();
    }

    /// Listener for an icon's left press: arms the drag for this exact item.
    pub(super) fn arm_toolbar_drag_listener(
        toolbar_id: String,
        item_id: String,
        can_drag: bool,
    ) -> impl Fn(&mut Self, &gpui::MouseDownEvent, &mut Window, &mut Context<Self>) + 'static {
        move |this, event: &gpui::MouseDownEvent, _, cx| {
            this.arm_toolbar_drag(
                &toolbar_id,
                &item_id,
                event.position.x.into(),
                event.position.y.into(),
                can_drag,
                cx,
            );
        }
    }
}
