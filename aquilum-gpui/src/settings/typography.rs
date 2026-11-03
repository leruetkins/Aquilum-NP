use super::{EditorSettings, HeadingStyle, HeadingsSettings};

/// Body font size the row-height and table constants were designed against.
///
/// The editor used to paint 14 px text in 24 px rows; every height in the
/// layout was tuned for that pair. Sizes configured elsewhere (headings, table
/// line height) are expressed in pixels at this reference size, so they are
/// scaled by `base_font_size / REFERENCE_FONT_SIZE`.
pub const REFERENCE_FONT_SIZE: f32 = 14.0;

/// Editor metrics derived from [`EditorSettings`].
///
/// Row geometry is built without access to the text system, so every height it
/// needs must come from numbers the settings already carry. `base_font_size` is
/// the size body text is painted and shaped at; `row_height` and
/// `table_line_height` are the heights those sizes imply; `headings` holds the
/// heading sizes already scaled to the same base.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorTypography {
    pub base_font_size: f32,
    pub row_height: f32,
    pub table_line_height: f32,
    pub headings: HeadingsSettings,
}

impl EditorTypography {
    pub fn from_editor(editor: &EditorSettings) -> Self {
        let base_font_size = (editor.font.font_size_base as f32).clamp(12.0, 32.0);
        let line_height = editor.line_height.clamp(1.2, 2.5);
        Self {
            base_font_size,
            row_height: device_grid_row(base_font_size * line_height),
            table_line_height: device_grid_row(base_font_size * 20.0 / REFERENCE_FONT_SIZE),
            headings: scaled_headings(&editor.headings, base_font_size),
        }
    }

    /// Metrics every layout constant was tuned for: 14 px text in 24 px rows.
    ///
    /// Used where no configured typography is supplied (window builders without
    /// a view, numeric tests), so those paths keep the reference geometry.
    pub fn reference() -> Self {
        Self {
            base_font_size: REFERENCE_FONT_SIZE,
            row_height: 24.0,
            table_line_height: 20.0,
            headings: HeadingsSettings::default(),
        }
    }
}

impl Default for EditorTypography {
    fn default() -> Self {
        Self::from_editor(&EditorSettings::default())
    }
}

/// Rows sit on multiples of four logical pixels: at 125/150/175/200/225/250%
/// that is a whole number of device pixels, so row backgrounds, highlights and
/// selection rectangles never seam.
fn device_grid_row(height: f32) -> f32 {
    ((height / 4.0).round() * 4.0).max(4.0)
}

fn scaled_headings(headings: &HeadingsSettings, base_font_size: f32) -> HeadingsSettings {
    let scale = base_font_size / REFERENCE_FONT_SIZE;
    HeadingsSettings {
        h1: scaled_heading(&headings.h1, scale),
        h2: scaled_heading(&headings.h2, scale),
        h3: scaled_heading(&headings.h3, scale),
        h4: scaled_heading(&headings.h4, scale),
        h5: scaled_heading(&headings.h5, scale),
        h6: scaled_heading(&headings.h6, scale),
    }
}

fn scaled_heading(style: &HeadingStyle, scale: f32) -> HeadingStyle {
    let mut scaled = style.clone();
    scaled.font_size = ((style.font_size as f32) * scale).round().max(1.0) as u32;
    scaled
}

#[cfg(test)]
mod tests;
