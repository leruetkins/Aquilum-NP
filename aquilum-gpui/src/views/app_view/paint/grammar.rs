/// Thickness of a school-grammar line in device pixels.
///
/// The native wavy underline covers three logical pixels, so a one-device-pixel
/// rule reads as a hairline next to it. Two device pixels match the weight of the
/// wave and the list indent guides without turning a solid line into a bar.
pub(crate) const GRAMMAR_LINE_DEVICE_WIDTH: f32 = 2.0;

/// How far below the native underline the solid, dotted and dash-dot lines sit,
/// in device pixels. A rule on the native underline is cramped against the
/// glyphs and does not line up with the dash-dot pattern, so solid moves down
/// with the marked lines. The drop is clamped so a dot never crosses the bottom
/// of the row, where the canvas would cut it.
pub(crate) const GRAMMAR_MARKED_LINE_DROP: f32 = 4.0;

/// Segments of the line under one grammar member, as
/// `(x, y, width, height, corner radius)` in row logical px.
///
/// Wavy lines are native text underlines. GPUI has no double, dotted or dash-dot
/// underline, and a native solid underline cannot be moved down to the level of
/// those patterns, so the rest are painted by the row backdrop. `y` matches the
/// native underline position in `vendor/gpui/src/text_system/line.rs`. Dots are
/// twice as thick as the line, centred on it and rounded to circles, so a dash-dot
/// line keeps its points on the same axis as the dashes.
pub(crate) fn grammar_line_segments(
    x: f32,
    width: f32,
    style: crate::markdown::GrammarLine,
    ascent: f32,
    descent: f32,
    line_height: f32,
    scale: f32,
) -> Vec<(f32, f32, f32, f32, f32)> {
    let mut segments = Vec::new();
    let y = (line_height - ascent - descent) / 2.0 + ascent + descent * 0.618;
    let device = |n: f32| crate::snap::device_thickness(n, scale);
    // A mark is only two or four device pixels tall, so a fractional device
    // offset flattens the edge it starts on. Snapping to the device grid keeps
    // a dot round and its top and bottom alike.
    let snap = |v: f32| {
        if scale > 0.0 {
            (v * scale).round() / scale
        } else {
            v
        }
    };
    let stroke = device(GRAMMAR_LINE_DEVICE_WIDTH);
    let dot = stroke * 2.0;
    let marked_y = snap(
        (y + device(GRAMMAR_MARKED_LINE_DROP))
            .min(line_height - stroke * 1.5 - device(1.0))
            .max(y),
    );
    let dot_y = marked_y - (dot - stroke) / 2.0;
    let push = |segments: &mut Vec<(f32, f32, f32, f32, f32)>,
                    left: f32,
                    w: f32,
                    top: f32,
                    h: f32,
                    radius: f32| {
        if w > 0.0 {
            segments.push((left, top, w, h, radius));
        }
    };
    match style {
        crate::markdown::GrammarLine::None | crate::markdown::GrammarLine::Wavy => {}
        crate::markdown::GrammarLine::Solid => {
            push(&mut segments, x, width, marked_y, stroke, 0.0);
        }
        crate::markdown::GrammarLine::Double => {
            push(&mut segments, x, width, y, stroke, 0.0);
            push(&mut segments, x, width, y + stroke + device(2.0), stroke, 0.0);
        }
        crate::markdown::GrammarLine::Dotted => {
            // A dot every ten device pixels: four of ink, six of paper. Only
            // whole dots are drawn and the run is centred in the member, so the
            // line neither ends in a speck nor hugs the left edge.
            let step = device(10.0);
            let mut count = 0usize;
            while count as f32 * step + dot <= width {
                count += 1;
            }
            if count > 0 {
                let total = (count as f32 - 1.0) * step + dot;
                let shift = (width - total) / 2.0;
                for i in 0..count {
                    push(
                        &mut segments,
                        snap(x + shift + i as f32 * step),
                        dot,
                        dot_y,
                        dot,
                        dot / 2.0,
                    );
                }
            }
        }
        crate::markdown::GrammarLine::DashDot => {
            let dash = device(16.0);
            let gap = device(10.0);
            // Collect whole marks first: a truncated dash reads as a mistake.
            let mut marks: Vec<(f32, f32, bool)> = Vec::new();
            let mut off = 0.0;
            loop {
                if off + dash > width {
                    break;
                }
                marks.push((off, dash, false));
                off += dash + gap;
                if off + dot > width {
                    break;
                }
                marks.push((off, dot, true));
                off += dot + gap;
            }
            if marks.is_empty() {
                // Too narrow for a dash: keep the member underlined.
                if width > 0.0 {
                    push(&mut segments, snap(x), width, marked_y, stroke, 0.0);
                }
            } else {
                let total = marks.last().map_or(0.0, |(o, w, _)| o + w);
                let shift = (width - total) / 2.0;
                for (o, w, is_dot) in marks {
                    if is_dot {
                        push(&mut segments, snap(x + shift + o), w, dot_y, dot, dot / 2.0);
                    } else {
                        push(&mut segments, snap(x + shift + o), w, marked_y, stroke, 0.0);
                    }
                }
            }
        }
    }
    segments
}
