/// Duration bounds of the optional wheel glide, seconds. A notch gets
/// `2 × mean interval of the last three notches`, clamped to this band: a
/// sustained spin shortens the glide so the next notch keeps the motion
/// alive, an isolated notch keeps a mid-length one. Off by default
/// (`editor.wheelSmoothScroll`): with the flag off every notch applies
/// instantly and the text stops dead.
pub(super) const WHEEL_GLIDE_MIN_SECS: f32 = 0.05;
pub(super) const WHEEL_GLIDE_MAX_SECS: f32 = 0.20;

/// Fixed thumb height (px) of the Zed-style wide scrollbar lane: a position
/// marker that never resizes with the document.
pub(super) const WIDE_THUMB_H: f64 = 25.0;

/// One in-flight wheel glide. `start` is the live position when the notch
/// arrived, `target` where it lands; `curve` is the P1 control point of a
/// cubic-bezier easing (P2 fixed at `(0.6, 1.0)`) whose initial slope carries
/// the velocity of the glide the notch interrupted — a notch stream reads as
/// one continuous motion instead of a series of restarts. Stepped in `render`
/// against a hard deadline; past it the position lands exactly on `target`.
#[derive(Clone, Copy)]
pub struct WheelEase {
    pub(super) start: f64,
    pub(super) target: f64,
    pub(super) t0: std::time::Instant,
    pub(super) duration_secs: f32,
    pub(super) curve: (f64, f64),
}

impl WheelEase {
    /// Builds a glide from `from` to `to` entering with `velocity` (px/s).
    /// The curve's initial tangent encodes that velocity: slope =
    /// velocity·duration/span, then both coordinates are divided by
    /// `(1 + slope²)^½` so a large velocity only tilts the tangent, never
    /// breaks the time domain. An opposing velocity (reversal) or a zero
    /// span degrades to the plain ease-out.
    pub(super) fn new(from: f64, to: f64, velocity: f64, duration_secs: f32) -> Self {
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

    pub(super) fn position_at(&self, at: std::time::Instant) -> f64 {
        let t = (at.duration_since(self.t0).as_secs_f32() / self.duration_secs).clamp(0.0, 1.0);
        self.start + (self.target - self.start) * cubic_bezier(self.curve.0, self.curve.1, 0.6, 1.0, f64::from(t))
    }

    /// Current speed (px/s), numerically — what an arriving notch inherits.
    /// At the very start the backward sample is clamped to `t0`, so the
    /// interval shrinks with the position difference instead of measuring a
    /// half-span average against a full interval (which reads as half speed).
    pub(super) fn velocity_at(&self, at: std::time::Instant) -> f64 {
        let h = std::time::Duration::from_millis(1);
        let before = at.max(self.t0);
        let after = at + h;
        let dt = after.duration_since(before).as_secs_f64();
        (self.position_at(after) - self.position_at(before)) / dt
    }
}

/// Progress of a CSS-style cubic-bezier easing at the time fraction `t`:
/// solve the curve parameter `u` from `x(u) = t`, return `y(u)`. The x
/// coordinates stay inside `[0, 1]`, so `x(u)` is monotonic and the solve is
/// well-defined; Newton from `u = t` converges in a few steps for these
/// curves, bisection is the safety net.
pub(super) fn cubic_bezier(x1: f64, y1: f64, x2: f64, y2: f64, t: f64) -> f64 {
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
pub(super) fn wheel_glide_duration_secs(
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
pub(super) fn editor_middle_cursor_target(cursor_y: f32, viewport_top: f32, viewport_h: f32, max_scroll: f64) -> f64 {
    let pad = (viewport_h * EDITOR_MIDDLE_EDGE_PAD_FRACTION)
        .clamp(24.0, EDITOR_MIDDLE_EDGE_PAD_MAX)
        .min(viewport_h / 3.0);
    let usable = (viewport_h - pad * 2.0).max(1.0);
    let t = ((cursor_y - viewport_top - pad) / usable).clamp(0.0, 1.0);
    max_scroll * f64::from(t)
}
