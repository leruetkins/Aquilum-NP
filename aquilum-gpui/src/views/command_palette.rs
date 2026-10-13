use gpui::{AppContext, Context, SharedString, Window};
use crate::icons;
use crate::views::app_view::AppView;
use crate::views::tab_list::{TabEntry, TabListEvent, TabListPopup};

impl AppView {
    /// Открывает палитру команд (Ctrl+Shift+P)
    pub fn open_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.command_palette_popup.is_some() {
            self.close_command_palette(cx);
            return;
        }

        // Закрываем другие всплывающие окна
        self.close_tabs_popup(cx);

        let commands = self.state.commands.all();
        let entries: Vec<TabEntry> = commands
            .iter()
            .enumerate()
            .map(|(i, cmd)| TabEntry {
                id: i,
                title: SharedString::from(cmd.name.clone()),
                subtitle: cmd.hotkey.as_ref().map(|h| SharedString::from(h.clone())),
                icon: icons::ICON_TERMINAL,
                font_family: None,
            })
            .collect();

        let theme = self.theme.clone();
        let popup = cx.new(|cx| {
            TabListPopup::new(entries, 0, theme, None, None, None, cx)
        });

        // Фокусируем попап для мгновенной клавиатурной навигации
        let focus_h = popup.read(cx).focus_handle().clone();
        window.focus(&focus_h);

        let subscription = cx.subscribe_in(
            &popup,
            window,
            |this, _popup, event: &TabListEvent, window, cx| {
                match event {
                    TabListEvent::Activate(id) => {
                        let commands = this.state.commands.all();
                        if let Some(cmd) = commands.get(*id) {
                            let cmd_id = cmd.id.clone();
                            this.close_command_palette(cx);
                            this.execute_command(&cmd_id, window, cx);
                        } else {
                            this.close_command_palette(cx);
                        }
                    }
                    TabListEvent::Dismiss => {
                        this.close_command_palette(cx);
                    }
                    _ => {}
                }
            },
        );

        self.command_palette_popup = Some(popup);
        self.command_palette_subscription = Some(subscription);
        cx.notify();
    }

    /// Закрывает палитру команд
    pub fn close_command_palette(&mut self, cx: &mut Context<Self>) {
        self.command_palette_popup = None;
        self.command_palette_subscription = None;
        cx.notify();
    }

    /// Диспетчер исполнения команд
    pub fn execute_command(&mut self, cmd_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        // Сначала пробуем вызвать зарегистрированный callback
        let executed = self.state.commands.execute(cmd_id);

        // Затем выполняем встроенное действие хоста, если это встроенная команда
        match cmd_id {
            "editor:save" => {
                self.state.save_active_note();
                cx.notify();
            }
            "workspace:new-tab" => {
                self.open_empty_tab(window, cx);
            }
            "workspace:close-tab" => {
                if let Some(ref note) = self.state.active_note {
                    let path = note.path.clone();
                    self.close_tab(&path, cx);
                }
            }
            "view:toggle-left-sidebar" => {
                self.toggle_sidebar(cx);
            }
            "view:toggle-right-sidebar" => {
                self.toggle_right_sidebar(cx);
            }
            "app:open-settings" => {
                self.toggle_settings(cx);
            }
            "editor:toggle-word-wrap" => {
                self.toggle_word_wrap(cx);
            }
            _ => {
                if !executed {
                    self.state.status_text = format!("Unknown command: {}", cmd_id);
                    cx.notify();
                }
            }
        }
    }
}
