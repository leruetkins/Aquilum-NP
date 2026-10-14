use std::time::Duration;
use gpui::Context;
use crate::views::app_view::AppView;

/// Manages cursor blinking lifecycle identically to Zed's `BlinkManager`:
/// - When enabled (focused): blinks continuously at regular intervals.
/// - When disabled (blurred): instantly hides the cursor (`visible = false`) and kills background timers.
/// - When typing/moving (`pause_and_show`): forces the cursor visible and delays the next blink cycle.
/// - When suspended (`suspend_visible`): the OS window is inactive. The timer is killed but the
///   cursor stays drawn solidly, so the app stops re-rendering while keeping the caret on screen.
#[derive(Clone, Debug)]
pub struct BlinkManager {
    interval: Duration,
    epoch: usize,
    blinking_paused: bool,
    visible: bool,
    enabled: bool,
    /// Set while the OS window is inactive. Distinct from `disable()`: the cursor is still
    /// drawn, only the timer is dead. Kept so a repeated `suspend_visible` is a no-op.
    suspended: bool,
}

impl Default for BlinkManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BlinkManager {
    pub const DEFAULT_INTERVAL: Duration = Duration::from_millis(530);

    pub fn new() -> Self {
        Self {
            interval: Self::DEFAULT_INTERVAL,
            epoch: 0,
            blinking_paused: false,
            visible: false,
            enabled: false,
            suspended: false,
        }
    }

    /// Whether the cursor should currently be drawn.
    #[inline]
    pub fn visible(&self) -> bool {
        self.enabled && self.visible
    }

    /// Whether the cursor is currently enabled (has focus).
    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Enables blinking (called when gaining focus). Shows cursor immediately and starts loop.
    pub fn enable(&mut self, target: BlinkTarget, cx: &mut Context<AppView>) {
        self.suspended = false;

        if self.enabled {
            self.pause_and_show(target, cx);
            return;
        }

        self.enabled = true;
        self.visible = true;
        self.blinking_paused = false;
        self.epoch = self.epoch.wrapping_add(1);
        let epoch = self.epoch;

        self.spawn_blink_cycle(epoch, target, cx);
        cx.notify();
    }

    /// Disables blinking (called when losing focus).
    /// Instantly hides cursor (`visible = false`) and invalidates any running background timer.
    pub fn disable(&mut self, cx: &mut Context<AppView>) {
        self.suspended = false;

        if !self.enabled && !self.visible {
            return;
        }

        self.enabled = false;
        self.visible = false;
        self.blinking_paused = false;
        self.epoch = self.epoch.wrapping_add(1); // Invalidates running timers immediately
        cx.notify();
    }

    /// Suspends blinking while the OS window is inactive, leaving the cursor drawn solidly
    /// visible: the timer dies, the cursor stays on screen.
    ///
    /// This is deliberately *not* `disable()`. `disable()` is for losing in-app focus and
    /// hides the cursor. Here the caret should remain visible — the only thing we refuse to
    /// do is re-render the whole view twice a second for a caret that cannot be interacted
    /// with (gpui's per-window focus never notices the window was deactivated, so without
    /// this the blink loop runs forever in the background).
    ///
    /// Idempotent by design: the caller invokes it from `Render::render`, so a repeated call
    /// must not `notify()` again or the view would re-render in a loop.
    pub fn suspend_visible(&mut self, cx: &mut Context<AppView>) {
        if self.suspended {
            return;
        }

        // Nothing was blinking — do not resurrect a cursor that is meant to be hidden.
        if !self.enabled && !self.visible {
            return;
        }

        self.suspended = true;
        self.enabled = true;
        self.visible = true;
        self.blinking_paused = true;
        // Kills any running timer immediately, including a pending resume scheduled by
        // `pause_and_show`, since those all check `epoch` before acting.
        self.epoch = self.epoch.wrapping_add(1);
        cx.notify();
    }

    /// Temporarily pauses blinking and forces the cursor to be visible (called on keypress / click).
    /// Keeps cursor solidly visible for 530ms before resuming regular blinking.
    pub fn pause_and_show(&mut self, target: BlinkTarget, cx: &mut Context<AppView>) {
        self.suspended = false;
        self.enabled = true;
        self.visible = true;
        self.blinking_paused = true;
        self.epoch = self.epoch.wrapping_add(1);
        let epoch = self.epoch;

        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let timer_executor = cx.background_executor().clone();
        let interval = self.interval;

        cx.foreground_executor().spawn(async move {
            timer_executor.timer(interval).await;
            entity.update(&mut async_cx, |this, cx| {
                let bm = target.get_mut(this);
                if bm.epoch == epoch && bm.enabled {
                    bm.blinking_paused = false;
                    bm.spawn_blink_cycle(epoch, target, cx);
                }
            }).ok();
        }).detach();

        cx.notify();
    }

    fn spawn_blink_cycle(&mut self, epoch: usize, target: BlinkTarget, cx: &mut Context<AppView>) {
        let entity = cx.weak_entity();
        let mut async_cx = cx.to_async();
        let timer_executor = cx.background_executor().clone();
        let interval = self.interval;

        cx.foreground_executor().spawn(async move {
            loop {
                timer_executor.timer(interval).await;
                let should_continue = entity.update(&mut async_cx, |this, cx| {
                    let bm = target.get_mut(this);
                    if bm.epoch != epoch || !bm.enabled || bm.blinking_paused {
                        false
                    } else {
                        bm.visible = !bm.visible;
                        cx.notify();
                        true
                    }
                }).unwrap_or(false);

                if !should_continue {
                    break;
                }
            }
        }).detach();
    }
}

/// Identifies which blink manager in `AppView` is being animated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlinkTarget {
    Editor,
    Title,
    Hex,
    QuickFilter,
    GotoSearch,
    SidebarSearch,
    CommandPalette,
    PluginCatalog,
    PluginInput,
}

impl BlinkTarget {
    fn get_mut<'a>(&self, view: &'a mut AppView) -> &'a mut BlinkManager {
        match self {
            BlinkTarget::Editor => &mut view.editor_blink,
            BlinkTarget::Title => &mut view.title_blink,
            BlinkTarget::Hex => &mut view.hex_blink,
            BlinkTarget::QuickFilter => &mut view.quick_filter.blink,
            BlinkTarget::GotoSearch => &mut view.goto_blink,
            BlinkTarget::SidebarSearch => &mut view.sidebar_search.blink,
            BlinkTarget::CommandPalette => &mut view.command_palette_blink,
            BlinkTarget::PluginCatalog => &mut view.catalog_blink,
            BlinkTarget::PluginInput => &mut view.plugin_input_blink,
        }
    }
}
