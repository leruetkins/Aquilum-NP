use std::sync::Mutex;
use std::time::Instant;
use gpui::{
    div, px, prelude::FluentBuilder, Context, InteractiveElement, IntoElement, MouseButton,
    ParentElement, StatefulInteractiveElement, Styled, Window,
};
use crate::icons;
use crate::theme::Theme;
use crate::views::app_view::AppView;

/// Notification severity level matching GPUI Kit's NotificationType.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotificationType {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

impl NotificationType {
    /// SVG icon path for this notification type.
    pub fn icon_path(&self) -> &'static str {
        match self {
            Self::Info => icons::ICON_INFO,
            Self::Success => icons::ICON_CHECK,
            Self::Warning => icons::ICON_TRIANGLE_ALERT,
            Self::Error => icons::ICON_CIRCLE_ALERT,
        }
    }

    /// Primary accent/indicator color for this notification type.
    pub fn icon_color(&self, theme: &Theme) -> gpui::Rgba {
        match self {
            Self::Info => theme.accent,
            Self::Success => theme.success,
            Self::Warning => {
                if theme.is_dark {
                    gpui::rgba(0xf9e2afff)
                } else {
                    gpui::rgba(0xdf8e1dff)
                }
            }
            Self::Error => theme.marked_line_red(),
        }
    }
}

/// Represents an active toast notification item.
#[derive(Clone, Debug)]
pub struct NotificationItem {
    pub id: String,
    pub notif_type: NotificationType,
    pub title: Option<String>,
    pub message: String,
    pub timeout_ms: Option<u64>,
    pub created_at: Instant,
    pub is_hovered: bool,
    pub owner: Option<String>,
}

/// Thread-safe registry holding active notifications.
#[derive(Default)]
pub struct NotificationRegistry {
    items: Mutex<Vec<NotificationItem>>,
}

impl NotificationRegistry {
    /// Create an empty notification registry.
    pub fn new() -> Self {
        Self {
            items: Mutex::new(Vec::new()),
        }
    }

    /// Push a notification or update an existing one if the ID already exists.
    pub fn push(&self, item: NotificationItem) {
        let mut items = self.items.lock().unwrap();
        if let Some(pos) = items.iter().position(|i| i.id == item.id) {
            items[pos] = item;
        } else {
            items.push(item);
        }
    }

    /// Update the text of an existing notification by id.
    pub fn update(&self, id: &str, message: &str) {
        let mut items = self.items.lock().unwrap();
        if let Some(item) = items.iter_mut().find(|i| i.id == id) {
            item.message = message.to_string();
        }
    }

    /// Dismiss a notification by id. Returns true if it was removed.
    pub fn dismiss(&self, id: &str) -> bool {
        let mut items = self.items.lock().unwrap();
        if let Some(pos) = items.iter().position(|i| i.id == id) {
            items.remove(pos);
            true
        } else {
            false
        }
    }

    /// Set hover state for a notification (used to pause auto-dismiss).
    pub fn set_hovered(&self, id: &str, hovered: bool) {
        let mut items = self.items.lock().unwrap();
        if let Some(item) = items.iter_mut().find(|i| i.id == id) {
            item.is_hovered = hovered;
        }
    }

    /// Check if a notification is currently hovered.
    pub fn is_hovered(&self, id: &str) -> bool {
        let items = self.items.lock().unwrap();
        items.iter().find(|i| i.id == id).map(|i| i.is_hovered).unwrap_or(false)
    }

    /// Check if a notification with the given id exists.
    pub fn contains(&self, id: &str) -> bool {
        let items = self.items.lock().unwrap();
        items.iter().any(|i| i.id == id)
    }

    /// Remove all notifications belonging to a specific plugin owner.
    pub fn unregister_all_of(&self, plugin_id: &str) {
        let mut items = self.items.lock().unwrap();
        items.retain(|i| i.owner.as_deref() != Some(plugin_id));
    }

    /// Returns a snapshot of all active notifications in creation order.
    pub fn all(&self) -> Vec<NotificationItem> {
        self.items.lock().unwrap().clone()
    }

    /// Dismiss all notifications.
    pub fn clear(&self) {
        self.items.lock().unwrap().clear();
    }
}

impl AppView {
    /// Push a notification toast with severity type and optional duration.
    pub fn push_notification(
        &mut self,
        notif_type: NotificationType,
        title: Option<String>,
        message: impl Into<String>,
        timeout_ms: Option<u64>,
        cx: &mut Context<Self>,
    ) -> String {
        static NOTIF_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = format!("toast-{}", NOTIF_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst));
        let message_str = message.into();
        let item = NotificationItem {
            id: id.clone(),
            notif_type,
            title,
            message: message_str,
            timeout_ms,
            created_at: Instant::now(),
            is_hovered: false,
            owner: None,
        };
        self.state.notifications.push(item);
        self.spawn_notification_auto_dismiss(id.clone(), timeout_ms, cx);
        cx.notify();
        id
    }

    /// Push a plugin Notice into the notification toast overlay.
    pub fn push_plugin_notice(
        &mut self,
        id: Option<String>,
        title: Option<String>,
        message: impl Into<String>,
        timeout_ms: Option<u64>,
        owner: Option<String>,
        cx: &mut Context<Self>,
    ) -> String {
        static NOTIF_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let final_id = id.unwrap_or_else(|| {
            format!("notice-{}", NOTIF_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
        });
        let message_str = message.into();
        let item = NotificationItem {
            id: final_id.clone(),
            notif_type: NotificationType::Info,
            title,
            message: message_str,
            timeout_ms,
            created_at: Instant::now(),
            is_hovered: false,
            owner,
        };
        self.state.notifications.push(item);
        self.spawn_notification_auto_dismiss(final_id.clone(), timeout_ms, cx);
        cx.notify();
        final_id
    }

    /// Dismiss a notification by ID.
    pub fn dismiss_notification(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.state.notifications.dismiss(id) {
            cx.notify();
        }
    }

    /// Spawns a background timer to automatically dismiss a notification after its timeout.
    pub fn spawn_notification_auto_dismiss(
        &self,
        id: String,
        timeout_ms: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        let Some(ms) = timeout_ms else { return; };
        if ms == 0 { return; }

        let wait_dur = std::time::Duration::from_millis(ms);
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let background = cx.background_executor().clone();

        cx.foreground_executor().spawn(async move {
            background.timer(wait_dur).await;
            let _ = entity.update(&mut async_cx, |this, cx| {
                if !this.state.notifications.is_hovered(&id) {
                    this.dismiss_notification(&id, cx);
                } else {
                    // Re-check after 1.5s if the user was hovering when the timer elapsed
                    this.spawn_notification_auto_dismiss(id, Some(1500), cx);
                }
            });
        }).detach();
    }

    /// Renders the floating notification toast overlay container.
    pub fn render_notifications(
        &self,
        theme: &Theme,
        _window: &Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let notifs = self.state.notifications.all();
        let border_color = theme.border;
        let bg_color = theme.bg_card;
        let text_primary = theme.text_primary;
        let text_secondary = theme.text_secondary;
        let text_muted = theme.text_muted;
        let bg_hover = theme.bg_hover;

        gpui::deferred(
            div()
                .id("notification-overlay-container")
                .absolute()
                .top(px(40.0))
                .right(px(16.0))
                .w(px(340.0))
                .max_w(px(380.0))
                .flex()
                .flex_col()
                .gap_2()
                .children(notifs.into_iter().map(|notif| {
                    let id = notif.id.clone();
                    let id_hover = id.clone();
                    let id_close = id.clone();
                    let icon_path = notif.notif_type.icon_path();
                    let icon_color = notif.notif_type.icon_color(theme);

                    div()
                        .id(gpui::SharedString::from(format!("toast-{}", id)))
                        .occlude()
                        .w_full()
                        .bg(bg_color)
                        .border_1()
                        .border_color(border_color)
                        .rounded(px(8.0))
                        .shadow_lg()
                        .p(px(12.0))
                        .flex()
                        .flex_row()
                        .items_start()
                        .gap_3()
                        .cursor(gpui::CursorStyle::Arrow)
                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_mouse_down(MouseButton::Right, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_mouse_down(MouseButton::Middle, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_mouse_up(MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_mouse_up(MouseButton::Right, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_mouse_up(MouseButton::Middle, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_click(|_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_scroll_wheel(|_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_hover(cx.listener(move |this, hovered: &bool, _, _| {
                            this.state.notifications.set_hovered(&id_hover, *hovered);
                        }))
                        // Severity Icon
                        .child(icons::icon(icon_path, 18.0, icon_color))
                        // Content column (optional title + message)
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_w_0()
                                .gap(px(2.0))
                                .when_some(notif.title, |s, title| {
                                    s.child(
                                        div()
                                            .text_sm()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(text_primary)
                                            .child(title),
                                    )
                                })
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(text_secondary)
                                        .line_height(gpui::relative(1.4))
                                        .child(notif.message),
                                ),
                        )
                        // Close (dismiss) button
                        .child(
                            div()
                                .id(gpui::SharedString::from(format!("toast-close-{}", id)))
                                .occlude()
                                .cursor_pointer()
                                .p(px(2.0))
                                .rounded(px(4.0))
                                .hover(move |h| h.bg(bg_hover))
                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation();
                                })
                                .on_mouse_down(MouseButton::Right, |_, _, cx| {
                                    cx.stop_propagation();
                                })
                                .on_mouse_down(MouseButton::Middle, |_, _, cx| {
                                    cx.stop_propagation();
                                })
                                .on_mouse_up(MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation();
                                })
                                .on_mouse_up(MouseButton::Right, |_, _, cx| {
                                    cx.stop_propagation();
                                })
                                .on_mouse_up(MouseButton::Middle, |_, _, cx| {
                                    cx.stop_propagation();
                                })
                                .on_scroll_wheel(|_, _, cx| {
                                    cx.stop_propagation();
                                })
                                .child(icons::icon(icons::ICON_CLOSE, 14.0, text_muted))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.dismiss_notification(&id_close, cx);
                                })),
                        )
                }))
        )
        .with_priority(1100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_registry_lifecycle() {
        let registry = NotificationRegistry::new();
        assert!(registry.all().is_empty());

        let item1 = NotificationItem {
            id: "notice-1".into(),
            notif_type: NotificationType::Info,
            title: Some("Plugin".into()),
            message: "Hello".into(),
            timeout_ms: Some(5000),
            created_at: Instant::now(),
            is_hovered: false,
            owner: Some("test-plugin".into()),
        };
        registry.push(item1);
        assert_eq!(registry.all().len(), 1);
        assert_eq!(registry.all()[0].message, "Hello");

        // Update message
        registry.update("notice-1", "Updated message");
        assert_eq!(registry.all()[0].message, "Updated message");

        // Hover
        registry.set_hovered("notice-1", true);
        assert!(registry.is_hovered("notice-1"));

        // Unregister plugin
        registry.unregister_all_of("test-plugin");
        assert!(registry.all().is_empty());
    }
}
