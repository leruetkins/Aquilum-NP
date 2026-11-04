    use super::*;
    use crate::markdown::{classify_line, row_painted_text, VisualRow};

    /// Reference metrics (14 px text, 24 px rows) carrying the given headings.
    fn test_typography(headings: &crate::settings::HeadingsSettings) -> crate::settings::EditorTypography {
        let editor = crate::settings::EditorSettings {
            line_height: 24.0 / crate::settings::REFERENCE_FONT_SIZE,
            headings: headings.clone(),
            font: crate::settings::FontSettings {
                font_size_base: crate::settings::REFERENCE_FONT_SIZE as u32,
                ..Default::default()
            },
            ..Default::default()
        };
        crate::settings::EditorTypography::from_editor(&editor)
    }

    /// Body metrics follow the configured size; headings keep the scaled sizes.
    #[test]
    fn row_metrics_use_the_configured_font_size() {
        let editor = crate::settings::EditorSettings {
            line_height: 1.8,
            font: crate::settings::FontSettings {
                font_size_base: 18,
                ..Default::default()
            },
            ..Default::default()
        };
        let typography = crate::settings::EditorTypography::from_editor(&editor);

        let body = classify_line("plain text", None);
        let (row_h, char_w, font_size, _, _) =
            AppView::heading_typography_metrics_with_widths(&body, &typography, 8.0, None);
        assert_eq!(row_h, 32.0);
        assert_eq!(font_size, 18.0);
        assert_eq!(char_w, 8.0, "body rows keep the measured cell width");

        let heading = classify_line("# heading", None);
        let (heading_row_h, _, heading_size, _, _) =
            AppView::heading_typography_metrics_with_widths(&heading, &typography, 8.0, None);
        assert_eq!(heading_size, typography.headings.h1.font_size as f32);
        assert!(heading_row_h >= row_h);
    }

    /// Dotted and dash-dot marks are separate shapes a few device pixels tall, so
    /// a mark that lands between device pixels loses half of its edge. Every mark
    /// must sit on the device grid, stay inside its member and inside the row,
    /// and be a whole dash or a whole dot.
    #[test]
    fn grammar_marked_lines_stay_whole_and_inside_the_row() {
        let scale = 1.75;
        let line_height = 24.0;
        let x = 3.3;
        let width = 41.9;
        let stroke = crate::snap::device_thickness(GRAMMAR_LINE_DEVICE_WIDTH, scale);
        let dot = stroke * 2.0;
        let dash = crate::snap::device_thickness(16.0, scale);
        for (ascent, descent) in [(14.67f32, 3.79f32), (12.99, 3.42)] {
            for style in [
                crate::markdown::GrammarLine::Dotted,
                crate::markdown::GrammarLine::DashDot,
            ] {
                let segments =
                    grammar_line_segments(x, width, style, ascent, descent, line_height, scale);
                assert!(!segments.is_empty());
                let mut first_left = f32::INFINITY;
                let mut last_right = f32::NEG_INFINITY;
                for (left, top, w, h, radius) in segments {
                    for v in [left, top] {
                        let device = v * scale;
                        assert!(
                            (device - device.round()).abs() < 0.01,
                            "{style:?} mark is off the device grid at {device}"
                        );
                    }
                    assert!(w == dot || w == dash, "{style:?} drew a partial mark of {w}");
                    if w == dot {
                        assert!(radius > 0.0, "a dot must be round");
                    }
                    assert!(left >= x - 0.5 && left + w <= x + width + 0.5);
                    assert!(top >= 0.0 && top + h <= line_height + 0.001);
                    first_left = first_left.min(left);
                    last_right = last_right.max(left + w);
                }
                let left_margin = first_left - x;
                let right_margin = x + width - last_right;
                assert!(
                    (left_margin - right_margin).abs() <= 1.0 / scale + 0.001,
                    "{style:?} pattern is not centred: {left_margin} vs {right_margin}"
                );
            }
        }
    }

    /// The solid grammar line shares the level of the dash-dot dashes; a native
    /// underline sits higher and would break the row of styles.
    #[test]
    fn grammar_solid_line_sits_at_the_dash_dot_level() {
        let scale = 1.75;
        let line_height = 24.0;
        let x = 3.3;
        let width = 41.9;
        for (ascent, descent) in [(14.67f32, 3.79f32), (12.99, 3.42)] {
            let solid = grammar_line_segments(
                x,
                width,
                crate::markdown::GrammarLine::Solid,
                ascent,
                descent,
                line_height,
                scale,
            );
            assert_eq!(solid.len(), 1);
            let (left, top, w, h, radius) = solid[0];
            assert!((left - x).abs() < 0.001 && (w - width).abs() < 0.001);
            assert_eq!(radius, 0.0);
            assert!(top >= 0.0 && top + h <= line_height);
            let dash_dot = grammar_line_segments(
                x,
                width,
                crate::markdown::GrammarLine::DashDot,
                ascent,
                descent,
                line_height,
                scale,
            );
            assert!(
                dash_dot
                    .iter()
                    .any(|&(_, d_top, _, d_h, _)| (d_top - top).abs() < 0.001
                        && (d_h - h).abs() < 0.001),
                "the solid line is not at the dash level"
            );
        }
    }

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
                    let layout = shape_row(window.text_system(), line, &kind, &v, false, &crate::markdown::InlineReveal::None, &crate::settings::EditorTypography::reference(), &font_family, cell_w);
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
                    let layout = shape_row(window.text_system(), line, &kind, &v, false, &crate::markdown::InlineReveal::None, &crate::settings::EditorTypography::reference(), &font_family, cell_w);

                    // Independently shape the very same painted text and read the glyph
                    // positions straight out of the layout.
                    let painted = row_painted_text(line, &kind, &v, false, &crate::markdown::InlineReveal::None, &headings);
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

    /// A list marker is painted by its own box, which rounds the text width up to
    /// whole pixels, while the caret geometry comes from the shaped line, which
    /// does not: at 14px `"1. "` measures 25.2041 but its box gets 26. The
    /// renderer therefore gives the indent and marker boxes explicit cell widths;
    /// this pins the other half of that contract — the shaped prefix sits on whole
    /// cells, so `chars * cell_w` is exactly where the caret expects it.
    #[test]
    fn list_marker_prefix_sits_on_whole_cells() {
        gpui::Application::new().run(|cx: &mut gpui::App| {
            crate::fonts::register_embedded_fonts(cx);
            let _ = cx.open_window(gpui::WindowOptions::default(), |window, cx| {
                let font_family: SharedString = "Droid Sans Mono".into();
                let font_id = window.text_system().resolve_font(&gpui::font(font_family.clone()));
                let cell_w = window
                    .text_system()
                    .advance(font_id, px(14.0), 'x')
                    .map(|s| f32::from(s.width))
                    .unwrap_or(7.7);

                for line in [
                    "1. one", "  2. two", "    10. ten", "- one", "  - two", "    - three",
                ] {
                    let v = VisualRow {
                        line_idx: 0,
                        sub_idx: 0,
                        is_last_sub: true,
                        char_start: 0,
                        char_end: line.chars().count(),
                    };
                    let kind = classify_line(line, None);
                    let prefix_cols = crate::markdown::continuation_prefix_cols(line, &kind);
                    assert!(prefix_cols > 0, "{line:?} is not a list row");
                    let layout = shape_row(
                        window.text_system(),
                        line,
                        &kind,
                        &v,
                        false,
                        &crate::markdown::InlineReveal::None,
                        &crate::settings::EditorTypography::reference(),
                        &font_family,
                        cell_w,
                    );
                    let expected = prefix_cols as f32 * cell_w;
                    let actual = layout.x_for_column(prefix_cols);
                    assert!(
                        (actual - expected).abs() < 0.01,
                        "{line:?}: the prefix ends at {actual:.4} but its {prefix_cols} cells end at {expected:.4}"
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
                    let layout = shape_row(window.text_system(), line, &kind, &v, false, &crate::markdown::InlineReveal::None, &crate::settings::EditorTypography::reference(), &font_family, cell_w);
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
                let typography = test_typography(&headings);
                let geom = shape_row(window.text_system(), line, &kind, &v, true, &crate::markdown::InlineReveal::All, &typography, &font_family, 24.0);
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
    /// to a stop and re-accelerating — that collapse is the "hitch" a
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

    /// The line-number gutter grows with the digit count of the document: a
    /// short note keeps the narrow baseline, a millions-long note gets room
    /// for every digit, and the width never oscillates between the two.
    #[test]
    fn line_number_gutter_width_tracks_digit_count() {
        let gutter = AppView::line_number_gutter_width;
        let near = |got: f32, want: f32| {
            assert!((got - want).abs() < 0.01, "gutter {got} != {want}");
        };
        near(gutter(1), 40.0);
        near(gutter(20), 40.0);
        near(gutter(999), 40.0);
        near(gutter(1000), 47.7);
        near(gutter(7_769_829), 70.8);
        near(gutter(10_000_000), 78.5);
        assert!(gutter(20) < gutter(999_999));
        assert!(gutter(999_999) < gutter(10_000_000));
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
            cell_start_col: 0,
            cell_columns: 3,
            content_range: c0.content.clone(),
            outer_range: c0.outer.clone(),
            lines: vec![TableCellLine {
                text: "aaa".into(),
                layout: Arc::new(gpui::LineLayout {
                    width: px(30.0),
                    runs: Vec::new(),
                    ascent: px(10.0),
                    descent: px(4.0),
                    font_size: px(14.0),
                    len: 3,
                }),
                columns: 3,
                line_chars: vec![2, 3, 4],
                raw_start: 2,
                raw_end: 5,
                content_x: 8.0,
            }],
            code_ranges: Vec::new(),
            tag_ranges: Vec::new(),
        };

        let cell1 = TableCellLayout {
            col_idx: 1,
            cell_x: 100.0,
            cell_w: 100.0,
            cell_start_col: 4,
            cell_columns: 3,
            content_range: c1.content.clone(),
            outer_range: c1.outer.clone(),
            lines: vec![TableCellLine {
                text: "bbb".into(),
                layout: Arc::new(gpui::LineLayout {
                    width: px(30.0),
                    runs: Vec::new(),
                    ascent: px(10.0),
                    descent: px(4.0),
                    font_size: px(14.0),
                    len: 3,
                }),
                columns: 3,
                line_chars: vec![8, 9, 10],
                raw_start: 8,
                raw_end: 11,
                content_x: 108.0,
            }],
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
            line_h: 20.0,
        };

        // Click past the end of cell 0 text (e.g. at x = 80.0):
        let col = row_layout.column_for_xy(80.0, 0.0);
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
            false,
            |line, _| {
                if line == 1 {
                    Some(crate::markdown::TableRowVisualKind::Delimiter)
                } else if line == 0 {
                    Some(crate::markdown::TableRowVisualKind::Header { lines: 1 })
                } else {
                    Some(crate::markdown::TableRowVisualKind::LastRow { lines: 1 })
                }
            },
            |_, _| None,
        );

        assert_eq!(height_index.height_of(0), 42.0, "header row height must be 42.0 (28.0 + 14.0 mt)");
        assert_eq!(height_index.height_of(1), 1.0, "delimiter row height must be 1.0 (matching 1px border)");
        assert_eq!(height_index.height_of(2), 42.0, "last row height must be 42.0 (28.0 + 14.0 mb)");
    }

    /// A wrapped table cell must paint its lines at the height the row
    /// geometry claims, and a click on any of them must resolve to a char of
    /// that very line. This is the contract the height index and the renderer
    /// share: same column budgets, same wrap.
    #[test]
    fn wrapped_table_cell_maps_clicks_to_the_painted_line() {
        gpui::Application::new().run(|cx: &mut gpui::App| {
            crate::fonts::register_embedded_fonts(cx);
            let _ = cx.open_window(gpui::WindowOptions::default(), |window, cx| {
                let text = "| a1 | alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu |";
                let block = crate::table::TableBlock {
                    first_line: 0,
                    last_line: 2,
                    aligns: vec![crate::table::Align::Left, crate::table::Align::Left],
                };
                let lines = vec![
                    "| id | a long description column |",
                    "|---|---|",
                    text,
                ];
                let get_line = |idx: usize| lines.get(idx).copied();
                let font = gpui::SharedString::from("Consolas");
                // A narrow long column forces the text to wrap.
                let manual = vec![60.0, 200.0];
                let layout =
                    compute_table_layout(window.text_system(), &get_line, &block, &font, Some(&manual), 7.7, 720.0, 14.0, 20.0);
                let col_chars = layout.col_chars.clone();
                assert!(col_chars[1] >= 4 && col_chars[1] <= 26, "budget {:?}", col_chars);

                let headings = crate::settings::HeadingsSettings::default();
                let cells = crate::table::split_row(text);
                let ranges: Vec<std::ops::Range<usize>> =
                    cells.iter().map(|c| c.content.clone()).collect();
                let n = crate::markdown::table_row_wrap_lines(
                    text,
                    &ranges,
                    &col_chars,
                    None,
                    &crate::markdown::InlineReveal::None,
                    &headings,
                );
                assert!(n >= 3, "the long cell must wrap into several lines, got {n}");
                // Row height is exactly the wrapped height, edges included.
                let kind = crate::markdown::TableRowVisualKind::LastRow { lines: n };
                assert_eq!(
                    kind.height(20.0),
                    n as f32 * crate::markdown::TABLE_LINE_H + 2.0 * crate::markdown::TABLE_PAD_Y
                        + crate::markdown::TABLE_EDGE_MARGIN
                );

                let row = shape_table_row(
                    window.text_system(),
                    text,
                    &block,
                    2,
                    None,
                    &layout,
                    &headings,
                    &font,
                    7.7,
                );

                // The sub-lines of the long cell, in raw document chars.
                let wrap = crate::markdown::table_cell_wrap_lines(
                    text,
                    ranges[1].clone(),
                    false,
                    &crate::markdown::InlineReveal::None,
                    &headings,
                    col_chars[1],
                );
                assert_eq!(wrap.len(), n);

                // A vertical probe inside the long cell: one y per painted line.
                for (k, line) in wrap.iter().enumerate() {
                    let y = crate::markdown::TABLE_PAD_Y
                        + k as f32 * crate::markdown::TABLE_LINE_H
                        + crate::markdown::TABLE_LINE_H / 2.0;
                    let col = row.column_for_xy(150.0, y);
                    let lc = row.line_char_for_column(col);
                    assert!(
                        lc >= line.raw_start && lc <= line.raw_end,
                        "click on painted line {k} resolved to char {lc}, outside {:?}",
                        line
                    );
                    // The caret is drawn on that very line.
                    assert_eq!(
                        row.top_for_column(col),
                        crate::markdown::TABLE_PAD_Y
                            + k as f32 * crate::markdown::TABLE_LINE_H
                    );
                }

                // One selection rectangle per painted line of the wrapped cell.
                let sel_start = row.column_for_line_char(ranges[1].start + 1);
                let sel_end = row.column_for_line_char(ranges[1].end - 1);
                let rects = row.span_rects(sel_start, sel_end, 1.0);
                assert_eq!(rects.len(), n, "selection must follow every painted line");

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

                let layout = compute_table_layout(window.text_system(), &get_line, &block, &font, Some(&manual), 7.7, 720.0, 14.0, 20.0);
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

    /// Inline code inside a quote or a callout header has to put its chip exactly on
    /// the glyphs the row paints, and the painted columns have to land on those
    /// glyphs too. Quoted prose is measured with the italic faces while the marker
    /// region stays upright; italic advances are not the upright ones, so a run
    /// shaped without the row's style drifts the chip and the selection a cell away.
    #[test]
    fn inline_code_chip_in_a_quote_lands_on_the_rendered_glyphs() {
        gpui::Application::new().run(|cx: &mut gpui::App| {
            crate::fonts::register_embedded_fonts(cx);
            let _ = cx.open_window(gpui::WindowOptions::default(), |window, cx| {
                let font_family: SharedString = "Roboto Mono".into();
                let font_id = window.text_system().resolve_font(&gpui::font(font_family.clone()));
                let cell_w = window
                    .text_system()
                    .advance(font_id, px(14.0), 'x')
                    .map(|s| f32::from(s.width))
                    .unwrap_or(7.7);
                let shape = |text: &str, weight: gpui::FontWeight, style: gpui::FontStyle| {
                    let run = gpui::TextRun {
                        len: text.len(),
                        font: gpui::Font {
                            family: font_family.clone(),
                            weight,
                            style,
                            features: Default::default(),
                            fallbacks: None,
                        },
                        color: Default::default(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    window.text_system().layout_line(text, px(14.0), &[run], None)
                };

                fn push_run(
                    text: &mut String,
                    runs: &mut Vec<gpui::TextRun>,
                    family: &gpui::SharedString,
                    s: &str,
                    weight: gpui::FontWeight,
                    style: gpui::FontStyle,
                ) {
                    if s.is_empty() {
                        return;
                    }
                    text.push_str(s);
                    runs.push(gpui::TextRun {
                        len: s.len(),
                        font: gpui::Font {
                            family: family.clone(),
                            weight,
                            style,
                            features: Default::default(),
                            fallbacks: None,
                        },
                        color: Default::default(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    });
                }

                for line in [
                    "> Информационная выноска с **форматированием** и `кодом`.",
                    "> [!info] Заголовок выноски с `кодом`",
                    "> >[!tip] Познавательная заметка с `кодом`",
                    "> > вложенная цитата с **жирным** и `кодом`.",
                ] {
                    for active in [false, true] {
                        let total = line.chars().count();
                        let kind = classify_line(line, None);
                        let reveal = if active {
                            crate::markdown::InlineReveal::All
                        } else {
                            crate::markdown::InlineReveal::None
                        };
                        let v = VisualRow {
                            line_idx: 0,
                            sub_idx: 0,
                            is_last_sub: true,
                            char_start: 0,
                            char_end: total,
                        };
                        let layout = shape_row(
                            window.text_system(),
                            line,
                            &kind,
                            &v,
                            active,
                            &reveal,
                            &crate::settings::EditorTypography::reference(),
                            &font_family,
                            cell_w,
                        );
                        let callout = crate::markdown::callout_prefix(line);
                        let quote = crate::markdown::quote_prefix(line).expect("a quote line");
                        let prefix = &line[..quote.bytes];
                        let prefix_chars = prefix.chars().count();

                        // The marker region is the renderer's own upright block:
                        // the quote prefix, then the icon — and the type name only
                        // when the callout has no title.
                        let prefix_w = if let Some(c) = callout {
                            if active {
                                f32::from(shape(&line[..c.bytes], gpui::FontWeight::NORMAL, gpui::FontStyle::Normal).width)
                            } else {
                                let spaces = f32::from(shape(&" ".repeat(prefix_chars), gpui::FontWeight::NORMAL, gpui::FontStyle::Normal).width);
                                let label = if c.title.is_empty() {
                                    f32::from(shape(
                                        &crate::markdown::callout_label_padded(&c, prefix_chars + crate::markdown::CALLOUT_ICON_CELLS),
                                        gpui::FontWeight::BOLD,
                                        gpui::FontStyle::Normal,
                                    ).width)
                                } else {
                                    0.0
                                };
                                spaces + cell_w * crate::markdown::CALLOUT_ICON_CELLS as f32 + label
                            }
                        } else if active {
                            f32::from(shape(prefix, gpui::FontWeight::NORMAL, gpui::FontStyle::Normal).width)
                        } else {
                            f32::from(shape(&" ".repeat(prefix_chars), gpui::FontWeight::NORMAL, gpui::FontStyle::Normal).width)
                        };

                        // A callout header is a bold upright label; quoted prose is
                        // upright too. Both are passed to `render_syntax_spans` as its
                        // base, and the painted row measures the same faces.
                        let (base_weight, base_style, content, content_offset, marker_chars) = match callout {
                            Some(c) => (gpui::FontWeight::BOLD, gpui::FontStyle::Normal, c.title, c.chars, c.chars),
                            None => (gpui::FontWeight::NORMAL, gpui::FontStyle::Normal, &line[quote.bytes..], prefix_chars, prefix_chars),
                        };
                        let spans = crate::markdown::parse_inline_syntax_spans(content);
                        let visible = crate::markdown::slice_syntax_spans_revealed(
                            &spans,
                            0,
                            total - marker_chars,
                            content_offset,
                            &reveal,
                        );

                        let mut text = String::new();
                        let mut runs = Vec::new();
                        let mut code_start = None;
                        let mut code_end = None;
                        let mut delimiters = 0usize;
                        for span in &visible {
                            match &span.span {
                                crate::markdown::SyntaxSpan::Marker(m) => {
                                    let delim = crate::markdown::is_inline_code_delim(m);
                                    if !span.marker_visible && !delim {
                                        continue;
                                    }
                                    if delim && delimiters == 0 {
                                        code_start = Some(text.len());
                                    }
                                    let (weight, style) = if delim {
                                        (inline_code_weight(base_weight), base_style)
                                    } else {
                                        (base_weight, base_style)
                                    };
                                    push_run(&mut text, &mut runs, &font_family, m, weight, style);
                                    if delim {
                                        delimiters += 1;
                                        if delimiters == 2 {
                                            code_end = Some(text.len());
                                        }
                                    }
                                }
                                crate::markdown::SyntaxSpan::Text(t) => push_run(&mut text, &mut runs, &font_family, t, base_weight, base_style),
                                crate::markdown::SyntaxSpan::Bold(t) => push_run(&mut text, &mut runs, &font_family, t, gpui::FontWeight::BOLD, base_style),
                                crate::markdown::SyntaxSpan::Italic(t) => push_run(&mut text, &mut runs, &font_family, t, base_weight, gpui::FontStyle::Italic),
                                crate::markdown::SyntaxSpan::BoldItalic(t) => push_run(&mut text, &mut runs, &font_family, t, gpui::FontWeight::BOLD, gpui::FontStyle::Italic),
                                crate::markdown::SyntaxSpan::InlineCode(t) => push_run(&mut text, &mut runs, &font_family, t, inline_code_weight(base_weight), base_style),
                                _ => {}
                            }
                        }
                        let shaped = window.text_system().layout_line(text.as_str(), px(14.0), &runs, None);
                        let code_start = code_start.expect("no inline code in the case");
                        let code_end = code_end.expect("no closing inline-code delimiter");
                        let ranges = layout.code_ranges();
                        assert_eq!(ranges.len(), 1, "{line:?}: expected one chip, got {ranges:?}");
                        let (cs, ce) = ranges[0];

                        let rendered_start = prefix_w + f32::from(shaped.x_for_index(code_start));
                        let rendered_end = prefix_w + f32::from(shaped.x_for_index(code_end));
                        let painted_start = layout.x_for_column(cs);
                        let painted_end = layout.x_for_column(ce);
                        assert!(
                            (rendered_start - painted_start).abs() < 0.02,
                            "{line:?} (active={active}): chip starts at {painted_start:.3} but the glyphs start at {rendered_start:.3} ({:+.3}px)",
                            rendered_start - painted_start
                        );
                        assert!(
                            (rendered_end - painted_end).abs() < 0.02,
                            "{line:?} (active={active}): chip ends at {painted_end:.3} but the glyphs end at {rendered_end:.3} ({:+.3}px)",
                            rendered_end - painted_end
                        );
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

    #[test]
    fn test_note_width_scaling_and_centering() {
        let mut config = crate::settings::AppConfig::default();
        config.editor.editor_width_pct = 100;

        let max_w = 1200.0f32;
        let content_w = (crate::settings::note_width_px(config.editor.editor_width_pct, 1920.0, 1700.0) as f32).min(max_w);
        assert_eq!(content_w, 1200.0);
        let margin_left = ((max_w - content_w) / 2.0).round();
        assert_eq!(margin_left, 0.0);

        // 45 maps to 1040 px on a 1920 px logical display.
        config.editor.editor_width_pct = 45;
        let content_w_45 = (crate::settings::note_width_px(config.editor.editor_width_pct, 1920.0, 1700.0) as f32).min(max_w);
        assert_eq!(content_w_45, 1040.0);
        let margin_left_45 = ((max_w - content_w_45) / 2.0).round();
        assert_eq!(margin_left_45, 80.0);
        assert_eq!(margin_left_45 + content_w_45 + margin_left_45, max_w);

        // Alignment: Center splits extra space equally
        let extra_space = max_w - content_w_45;
        let (margin_left_center, margin_right_center) = match crate::settings::NoteAlignment::Center {
            crate::settings::NoteAlignment::Left => (0.0, extra_space),
            crate::settings::NoteAlignment::Center => (extra_space / 2.0, extra_space / 2.0),
        };
        assert_eq!(margin_left_center, 80.0);
        assert_eq!(margin_right_center, 80.0);
        assert_eq!(margin_left_center + content_w_45 + margin_right_center, max_w);

        // Alignment: Left puts zero margin on left and full extra space on right
        let (margin_left_left, margin_right_left) = match crate::settings::NoteAlignment::Left {
            crate::settings::NoteAlignment::Left => (0.0, extra_space),
            crate::settings::NoteAlignment::Center => (extra_space / 2.0, extra_space / 2.0),
        };
        assert_eq!(margin_left_left, 0.0);
        assert_eq!(margin_right_left, 160.0);
        assert_eq!(margin_left_left + content_w_45 + margin_right_left, max_w);

        // Title alignment:
        // WindowLeft: starts at 48.0 px regardless of the slider position
        let title_w = 200.0f32;
        let title_win_left = match crate::settings::TitleAlignment::WindowLeft {
            crate::settings::TitleAlignment::WindowLeft => 48.0f32,
            crate::settings::TitleAlignment::WindowCenter => ((max_w - title_w) / 2.0).max(48.0).round(),
            crate::settings::TitleAlignment::EditorLeft => 48.0 + margin_left_45,
            crate::settings::TitleAlignment::EditorCenter => 48.0 + margin_left_45 + ((content_w_45 - title_w) / 2.0).round(),
        };
        assert_eq!(title_win_left, 48.0);

        // WindowCenter: centered across the entire pane (1200 - 200) / 2 = 500
        let title_win_center = match crate::settings::TitleAlignment::WindowCenter {
            crate::settings::TitleAlignment::WindowLeft => 48.0f32,
            crate::settings::TitleAlignment::WindowCenter => ((max_w - title_w) / 2.0).max(48.0).round(),
            crate::settings::TitleAlignment::EditorLeft => 48.0 + margin_left_45,
            crate::settings::TitleAlignment::EditorCenter => 48.0 + margin_left_45 + ((content_w_45 - title_w) / 2.0).round(),
        };
        assert_eq!(title_win_center, 500.0);

        // EditorLeft: aligned to the left edge of the editor note column (48 + 80 = 128)
        let title_ed_left = match crate::settings::TitleAlignment::EditorLeft {
            crate::settings::TitleAlignment::WindowLeft => 48.0f32,
            crate::settings::TitleAlignment::WindowCenter => ((max_w - title_w) / 2.0).max(48.0).round(),
            crate::settings::TitleAlignment::EditorLeft => 48.0 + margin_left_45,
            crate::settings::TitleAlignment::EditorCenter => 48.0 + margin_left_45 + ((content_w_45 - title_w) / 2.0).round(),
        };
        assert_eq!(title_ed_left, 128.0);

        // EditorCenter: centered across the editor note column 48 + 80 + (1040 - 200) / 2 = 548
        let title_ed_center = match crate::settings::TitleAlignment::EditorCenter {
            crate::settings::TitleAlignment::WindowLeft => 48.0f32,
            crate::settings::TitleAlignment::WindowCenter => ((max_w - title_w) / 2.0).max(48.0).round(),
            crate::settings::TitleAlignment::EditorLeft => 48.0 + margin_left_45,
            crate::settings::TitleAlignment::EditorCenter => 48.0 + margin_left_45 + ((content_w_45 - title_w) / 2.0).round(),
        };
        assert_eq!(title_ed_center, 548.0);

        // Find bar alignment:
        // Bar max width is 620.0
        let bar_w = 620.0f32;
        // In full width mode (or Center alignment), extra_space splits equally:
        let note_center_center = (max_w + margin_left_center - margin_right_center) / 2.0;
        let find_bar_left_center = (note_center_center - bar_w / 2.0).clamp(16.0, max_w - 16.0 - bar_w).round();
        // Since margin_left == margin_right, note_center is 600.0, so find bar is at (1200 - 620) / 2 = 290.0
        assert_eq!(find_bar_left_center, 290.0);

        // In Left alignment, margin_left is 0.0, margin_right is 160.0
        let note_center_left = (max_w + margin_left_left - margin_right_left) / 2.0;
        let find_bar_left_left = (note_center_left - bar_w / 2.0).clamp(16.0, max_w - 16.0 - bar_w).round();
        // 520 - 310 = 210.0
        assert_eq!(find_bar_left_left, 210.0);
    }

    /// Shrinking the note width re-wraps every line, so the old pixel scroll
    /// no longer shows the same rows. The scroll must follow the caret row:
    /// its screen Y is what has to survive the reflow.
    #[test]
    fn width_change_pins_the_caret_row_on_screen() {
        let text = (0..400)
            .map(|i| format!("line {i}: {}\n", "words ".repeat(40)))
            .collect::<String>();
        let doc_id = crate::document_buffer::DocumentId::new();
        let mut doc = crate::document_buffer::DocumentBuffer::new(
            doc_id,
            std::path::PathBuf::from("wrap.md"),
            "wrap".to_string(),
            text,
            1,
        );
        let mut view =
            crate::workspace::EditorViewState::new(crate::workspace::TabId::new(), doc_id);
        let cursor_line = 200;
        let cursor_col = 7;
        view.cursor_offset = view.offset_for_line_col(&doc, cursor_line, cursor_col);
        view.rebuild_visual_cache_with_headings(&mut doc, true, 80, None, cursor_line, false);

        let wide_y = AppView::row_doc_y(&view, cursor_line, Some(cursor_col)).unwrap();
        let viewport_h = 800.0;
        // The caret sits in the middle of the viewport.
        let wide_scroll = wide_y - viewport_h / 2.0;
        let screen_y = wide_y - wide_scroll;

        view.visual_cache_dirty = true;
        view.ensure_visual_cache_with_headings(&mut doc, true, 30, None, cursor_line, false);
        let narrow_y = AppView::row_doc_y(&view, cursor_line, Some(cursor_col)).unwrap();
        assert!(
            narrow_y > wide_y + 500.0,
            "the narrower note must add rows above the caret ({wide_y} -> {narrow_y})"
        );

        let pinned =
            AppView::pinned_scroll(wide_scroll, wide_y, narrow_y, view.total_height(), viewport_h);
        assert!((narrow_y - pinned - screen_y).abs() < 1e-6);
    }

    #[test]
    fn test_whitespace_dot_ranges() {
        assert_eq!(whitespace_dot_ranges(""), Vec::<std::ops::Range<usize>>::new());
        assert_eq!(whitespace_dot_ranges("a"), Vec::<std::ops::Range<usize>>::new());
        assert_eq!(whitespace_dot_ranges("a b"), Vec::<std::ops::Range<usize>>::new());
        assert_eq!(whitespace_dot_ranges("a  b"), vec![1..3]);
        assert_eq!(whitespace_dot_ranges("   "), vec![0..3]);
        assert_eq!(whitespace_dot_ranges("  hello   world  "), vec![0..2, 7..10, 15..17]);
        assert_eq!(whitespace_dot_ranges("    fn main() {"), vec![0..4]);
        assert_eq!(whitespace_dot_ranges("привет  мир"), vec![6..8]);
        // Leading spaces: even 1 space at the start of a line is visible
        assert_eq!(whitespace_dot_ranges(" a"), vec![0..1]);
        assert_eq!(whitespace_dot_ranges(" привет"), vec![0..1]);
        // Trailing spaces: even 1 space at the end of a line is visible
        assert_eq!(whitespace_dot_ranges("a "), vec![1..2]);
        assert_eq!(whitespace_dot_ranges("a  b "), vec![1..3, 4..5]);
        assert_eq!(whitespace_dot_ranges(" "), vec![0..1]);
        assert_eq!(whitespace_dot_ranges("hello world "), vec![11..12]);
        // Lines with CRLF / trailing \r: trailing spaces must still show dots
        assert_eq!(whitespace_dot_ranges("a \r"), vec![1..2]);
        assert_eq!(whitespace_dot_ranges("a  b \r"), vec![1..3, 4..5]);
        // With show_all = true (show invisibles mode), all spaces are marked:
        assert_eq!(whitespace_dot_ranges_with_mode("a b", true), vec![1..2]);
        assert_eq!(whitespace_dot_ranges_with_mode("   ", true), vec![0..1, 1..2, 2..3]);
        assert_eq!(whitespace_dot_ranges_with_mode("hello world \r", true), vec![5..6, 11..12]);
    }

    #[test]
    fn table_cells_show_whitespace_dots() {
        gpui::Application::new().run(|cx: &mut gpui::App| {
            crate::fonts::register_embedded_fonts(cx);
            let _ = cx.open_window(gpui::WindowOptions::default(), |window, cx| {
                let line = "| a  b | c |";
                let block = crate::table::TableBlock {
                    first_line: 1,
                    last_line: 3,
                    aligns: vec![crate::table::Align::Left, crate::table::Align::Left],
                };
                let lines = vec![
                    "| h1 | h2 |",
                    "|---|---|",
                    line,
                ];
                // `TableBlock` addresses document lines 1..=3.
                let get_line = |idx: usize| idx.checked_sub(1).and_then(|i| lines.get(i)).copied();
                let font = gpui::SharedString::from("Consolas");
                let layout = compute_table_layout(
                    window.text_system(),
                    &get_line,
                    &block,
                    &font,
                    None,
                    7.7,
                    720.0,
                    14.0,
                    20.0,
                );
                let headings = crate::settings::HeadingsSettings::default();
                let row = shape_table_row(
                    window.text_system(),
                    line,
                    &block,
                    3,
                    None,
                    &layout,
                    &headings,
                    &font,
                    7.7,
                );

                let quads = whitespace_dot_quads(line, &row, 42.0, 1.0, &[], None, false, 0.0);
                assert_eq!(quads.len(), 2, "both spaces of the double space must get a dot");
                let cell_x = layout.col_x[0];
                let cell_w = layout.col_widths[0];
                let expected_center_y =
                    crate::markdown::TABLE_PAD_Y + layout.line_h / 2.0;
                for (x, y, w, h, _) in quads {
                    assert!(
                        x >= cell_x && x + w <= cell_x + cell_w,
                        "dot {x}..{} must sit inside the first cell {cell_x}..{}",
                        x + w,
                        cell_x + cell_w
                    );
                    assert!(
                        (y + h / 2.0 - expected_center_y).abs() < 0.51,
                        "dot must sit on the cell's first painted line"
                    );
                }

                // "Show invisibles" marks every space; the spaces trimmed out of
                // a cell are not painted, so the count stays the same here.
                let all = whitespace_dot_quads(line, &row, 42.0, 1.0, &[], None, true, 0.0);
                assert_eq!(all.len(), 2);

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
    fn test_mouse_drag_threshold() {
        let is_drag = |dx: f32, dy: f32| dx * dx + dy * dy >= 16.0;
        assert!(!is_drag(0.0, 0.0));
        assert!(!is_drag(2.0, 2.0));
        assert!(!is_drag(3.5, 0.0));
        assert!(is_drag(4.0, 0.0));
        assert!(is_drag(0.0, 4.0));
        assert!(is_drag(3.0, 3.0));
    }

    #[test]
    fn test_collapsed_tag_reveal_column_stability() {
        let line = "Check out [[TargetName|alias]] for more";
        let classified = crate::markdown::classify_line(line, None);
        let headings = crate::settings::HeadingsSettings::default();
        let v = crate::markdown::VisualRow {
            line_idx: 0,
            sub_idx: 0,
            is_last_sub: true,
            char_start: 0,
            char_end: line.chars().count(),
        };

        // Collapsed painted row displays "Check out alias for more"
        let collapsed_painted = crate::markdown::row_painted_text(
            line,
            &classified,
            &v,
            true,
            &crate::markdown::InlineReveal::None,
            &headings,
        );
        let click_col = 12;
        let collapsed_char = crate::markdown::line_char_for_painted_boundary(&collapsed_painted.line_chars, click_col);
        assert!(collapsed_char > 20);

        // When placing the caret at that character, the tag reveals
        let reveal = crate::markdown::inline_reveal(
            line,
            &classified,
            Some(collapsed_char),
            None,
            false,
        );
        assert!(!matches!(reveal, crate::markdown::InlineReveal::None));

        // Re-evaluating the click against the revealed row keeps the caret at the same column
        let revealed_painted = crate::markdown::row_painted_text(
            line,
            &classified,
            &v,
            true,
            &reveal,
            &headings,
        );
        let rev_char = crate::markdown::line_char_for_painted_boundary(&revealed_painted.line_chars, click_col);
        assert_eq!(rev_char, click_col);
    }

    #[test]
    fn test_collapsed_heading_reveal_column_stability() {
        let line = "###### H6";
        let classified = crate::markdown::classify_line(line, None);
        let headings = crate::settings::HeadingsSettings::default();
        let v = crate::markdown::VisualRow {
            line_idx: 0,
            sub_idx: 0,
            is_last_sub: true,
            char_start: 0,
            char_end: line.chars().count(),
        };

        // Inactive painted row hides "###### " and displays only "H6"
        let inactive_painted = crate::markdown::row_painted_text(
            line,
            &classified,
            &v,
            false,
            &crate::markdown::InlineReveal::None,
            &headings,
        );
        let click_col = 0;
        let inactive_char = crate::markdown::line_char_for_painted_boundary(&inactive_painted.line_chars, click_col);
        // In the raw string, 'H' is at index 7 (after "###### ")
        assert_eq!(inactive_char, 7);

        // When the heading activates, it is reshaped with is_active_line = true
        let active_painted = crate::markdown::row_painted_text(
            line,
            &classified,
            &v,
            true,
            &crate::markdown::InlineReveal::None,
            &headings,
        );
        let active_char = crate::markdown::line_char_for_painted_boundary(&active_painted.line_chars, click_col);
        // In the active row, column 0 maps to char 0 (the first '#'), staying at column 0!
        assert_eq!(active_char, 0);
    }
