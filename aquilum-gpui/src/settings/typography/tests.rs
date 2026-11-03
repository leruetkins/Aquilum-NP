use super::*;
use crate::settings::EditorSettings;

fn editor_with(font_size: u32, line_height: f32) -> EditorSettings {
    EditorSettings {
        line_height,
        font: crate::settings::FontSettings {
            font_size_base: font_size,
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn row_height_follows_font_size_and_line_height() {
    let cases = [
        (12, 1.8, 20.0),
        (14, 1.8, 24.0),
        (16, 1.8, 28.0),
        (18, 1.8, 32.0),
        (14, 1.2, 16.0),
        (14, 2.5, 36.0),
    ];
    for (font_size, line_height, expected) in cases {
        let typography = EditorTypography::from_editor(&editor_with(font_size, line_height));
        assert_eq!(
            typography.row_height, expected,
            "font {font_size}px at line height {line_height} must give a {expected}px row"
        );
        assert_eq!(
            typography.row_height % 4.0,
            0.0,
            "row height must stay on the device grid"
        );
    }
}

#[test]
fn default_typography_uses_the_editor_defaults() {
    let typography = EditorTypography::default();
    assert_eq!(typography.base_font_size, 16.0);
    assert_eq!(typography.row_height, 28.0);
    assert_eq!(typography.table_line_height, 24.0);
}

#[test]
fn headings_keep_their_hierarchy_when_the_base_grows() {
    let typography = EditorTypography::from_editor(&editor_with(18, 1.8));
    let sizes = [
        typography.headings.h1.font_size,
        typography.headings.h2.font_size,
        typography.headings.h3.font_size,
        typography.headings.h4.font_size,
        typography.headings.h5.font_size,
        typography.headings.h6.font_size,
    ];
    assert_eq!(sizes, [28, 24, 22, 19, 18, 17]);
    assert!(sizes.windows(2).all(|pair| pair[0] >= pair[1]));
    assert!(sizes[0] > 18, "h1 must stay above the body text");
    assert!(sizes[4] >= 18, "h5 must not fall below the body text");
}

#[test]
fn heading_colours_and_faces_survive_scaling() {
    let typography = EditorTypography::from_editor(&editor_with(18, 1.8));
    let base = crate::settings::HeadingsSettings::default();
    assert_eq!(typography.headings.h1.color_name, base.h1.color_name);
    assert_eq!(typography.headings.h1.bold, base.h1.bold);
    assert_eq!(typography.headings.h1.italic, base.h1.italic);
}

#[test]
fn table_line_height_scales_with_the_font() {
    let cases = [(14, 20.0), (16, 24.0), (18, 24.0), (28, 40.0)];
    for (font_size, expected) in cases {
        let typography = EditorTypography::from_editor(&editor_with(font_size, 1.8));
        assert_eq!(typography.table_line_height, expected);
    }
}
