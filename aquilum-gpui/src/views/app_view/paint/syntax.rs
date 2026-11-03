use super::super::*;
use super::grammar::GRAMMAR_LINE_DEVICE_WIDTH;

/// Weight inline code is painted at.
///
/// Roboto Mono is bundled here in Regular and Bold only, and a request for a weight in
/// between resolves to whichever face the platform deems closest — so the one step up
/// that is guaranteed to show is Bold. `markdown::PaintedSegment::code` carries the same
/// trait into the row's shaped layout, so geometry and paint stay in parity.
pub(crate) const INLINE_CODE_WEIGHT: gpui::FontWeight = gpui::FontWeight::BOLD;

/// Fraction of the row height left as a gap above and below the inline-code chip, i.e.
/// the chip is `1 - 2 * CODE_CHIP_INSET_RATIO` of the row. Scaled with the row so that a
/// heading's chip keeps the proportions of a paragraph's.
pub(crate) const CODE_CHIP_INSET_RATIO: f32 = 1.0 / 12.0;

/// Weight the inline-code face takes in a row whose base weight is `base`: the heavier
/// of the two, so that code is never lighter than the text around it and a bold heading
/// keeps its bold.
pub(crate) fn inline_code_weight(base: gpui::FontWeight) -> gpui::FontWeight {
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
pub(crate) fn code_block_body(
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
pub(crate) fn code_chip_inset(row_h: f32) -> f32 {
    (row_h * CODE_CHIP_INSET_RATIO).round().max(1.0)
}

/// Colour a syntax marker is painted with, or `None` when it is not painted at all.
///
/// `visible` is the reveal decision for this marker (`markdown::InlineReveal`): only the
/// inline element under the caret/selection (or the whole line after Alt+click) shows its
/// markup. A hidden inline-code delimiter is still painted — with no ink — so that the
/// code chip keeps a constant width, and with it the selection rectangle. Off the active
/// line the delimiter's blank cell is exactly what reads as the chip's left/right padding:
/// the chip itself is a rectangle the editor paints behind the row's text, not a text-run
/// background, so a blank delimiter is enough to open the padding.
/// `markdown::row_painted_text` must agree: it keeps the delimiter's cell for the same
/// reason.
pub(crate) fn marker_paint(m: &str, visible: bool, theme: &Theme) -> Option<gpui::Hsla> {
    if visible {
        Some(theme.text_muted.into())
    } else if crate::markdown::is_inline_code_delim(m) {
        // No ink: the cell is part of the chip, and the chip shows through.
        Some(gpui::rgba(0x00000000).into())
    } else {
        None
    }
}

pub(crate) fn render_syntax_spans(
    spans: Vec<crate::markdown::VisibleSpan>,
    theme: &Theme,
    text_color_override: Option<gpui::Rgba>,
    base_weight: Option<gpui::FontWeight>,
    base_style: Option<gpui::FontStyle>,
    font_family: &gpui::SharedString,
    row_key: &str,
    ctrl_held: bool,
    hovered_link_id: Option<&str>,
    scale: f32,
    paint_native_solid: bool,
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

    let has_links = spans
        .iter()
        .any(|s| matches!(s.span, crate::markdown::SyntaxSpan::Link { .. }));

    // A grammar member's wavy line is a native text underline; solid and the
    // other line styles are painted by the row backdrop (`grammar_line_segments`),
    // which reads the same style from `RowShapedLayout`. Table cells have no row
    // backdrop, so their solid line stays native. The member paints one run per
    // inline span it carries, so emphasis and code inside it keep their face while
    // the line under them stays continuous.
    let push_grammar = |kind: crate::markdown::GrammarKind,
                        line_color: Option<crate::markdown::GrammarColor>,
                        text_color: Option<crate::markdown::GrammarColor>,
                        content: &[crate::markdown::VisibleSpan],
                        full_text: &mut String,
                        runs: &mut Vec<gpui::TextRun>| {
        let style = crate::markdown::GrammarSpanStyle {
            line: kind.line(),
            line_color,
            text_color,
        };
        let text_rgba = style.text_rgba(main_color);
        let line_rgba = style.line_rgba(main_color);
        let underline = match style.line {
            crate::markdown::GrammarLine::Solid if paint_native_solid => Some(gpui::UnderlineStyle {
                thickness: px(crate::snap::device_thickness(GRAMMAR_LINE_DEVICE_WIDTH, scale)),
                color: Some(line_rgba.into()),
                wavy: false,
            }),
            crate::markdown::GrammarLine::Wavy => Some(gpui::UnderlineStyle {
                thickness: px(1.0),
                color: Some(line_rgba.into()),
                wavy: true,
            }),
            _ => None,
        };
        for visible in content {
            let (text, font, color, strikethrough) = match &visible.span {
                crate::markdown::SyntaxSpan::Marker(m) => {
                    // The escape backslash never paints, revealed or not.
                    if m == "\\" {
                        continue;
                    }
                    let Some(color) = marker_paint(m, visible.marker_visible, theme) else {
                        continue;
                    };
                    let font = if crate::markdown::is_inline_code_delim(m) {
                        code_font.clone()
                    } else {
                        reg_font.clone()
                    };
                    (m.as_str(), font, color, None)
                }
                crate::markdown::SyntaxSpan::Text(t) => (t.as_str(), reg_font.clone(), text_rgba.into(), None),
                crate::markdown::SyntaxSpan::Bold(t) => {
                    (t.as_str(), bold_font.clone(), text_rgba.into(), None)
                }
                crate::markdown::SyntaxSpan::Italic(t) => {
                    (t.as_str(), italic_font.clone(), text_rgba.into(), None)
                }
                crate::markdown::SyntaxSpan::BoldItalic(t) => {
                    (t.as_str(), bold_italic_font.clone(), text_rgba.into(), None)
                }
                crate::markdown::SyntaxSpan::InlineCode(t) => {
                    (t.as_str(), code_font.clone(), text_rgba.into(), None)
                }
                crate::markdown::SyntaxSpan::Strikethrough(t) => (
                    t.as_str(),
                    reg_font.clone(),
                    text_rgba.into(),
                    Some(gpui::StrikethroughStyle {
                        thickness: px(1.0),
                        color: Some(text_rgba.into()),
                    }),
                ),
                other => (
                    crate::markdown::span_text(other),
                    reg_font.clone(),
                    text_rgba.into(),
                    None,
                ),
            };
            full_text.push_str(text);
            runs.push(gpui::TextRun {
                len: text.len(),
                font,
                color,
                background_color: None,
                underline,
                strikethrough,
            });
        }
    };

    if !has_links {
        let mut full_text = String::new();
        let mut runs = Vec::new();

        for visible in spans {
            let marker_visible = visible.marker_visible;
            match visible.span {
                crate::markdown::SyntaxSpan::Marker(m) => {
                    if let Some(color) = marker_paint(&m, marker_visible, theme) {
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
                crate::markdown::SyntaxSpan::Grammar { kind, line_color, text_color, content, .. } => {
                    push_grammar(kind, line_color, text_color, &content, &mut full_text, &mut runs);
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

        for visible in spans {
            let marker_visible = visible.marker_visible;
            match visible.span {
                crate::markdown::SyntaxSpan::Marker(m) => {
                    if let Some(color) = marker_paint(&m, marker_visible, theme) {
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
                crate::markdown::SyntaxSpan::Grammar { kind, line_color, text_color, content, .. } => {
                    push_grammar(kind, line_color, text_color, &content, &mut full_text, &mut runs);
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
