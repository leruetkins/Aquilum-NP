use serde::{Deserialize, Serialize};
use super::edit::{CropRect, ImageEdit};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationKind {
    #[default]
    Arrow,
    Line,
    Rect,
    Ellipse,
    Highlight,
    Pencil,
    Text,
    Label,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Annotation {
    pub kind: AnnotationKind,
    /// Coordinates are original image pixels. Rect/ellipse/line/arrow use the
    /// first and the last point; pencil/highlight keep every sample.
    pub points: Vec<Point>,
    /// `#rrggbb` or `#rrggbbaa`.
    pub color: String,
    /// Stroke width in original pixels.
    pub width: f32,
    pub filled: bool,
    pub text: Option<String>,
    pub font_size: f32,
}

impl Default for Annotation {
    fn default() -> Self {
        Self {
            kind: AnnotationKind::Arrow,
            points: Vec::new(),
            color: "#e5484d".to_string(),
            width: 3.0,
            filled: false,
            text: None,
            font_size: 18.0,
        }
    }
}

impl Annotation {
    pub fn with_points(kind: AnnotationKind, points: Vec<Point>) -> Self {
        Self {
            kind,
            points,
            ..Self::default()
        }
    }

    pub fn is_textual(&self) -> bool {
        matches!(self.kind, AnnotationKind::Text | AnnotationKind::Label)
    }

    pub fn bounds(&self) -> Option<RectF> {
        if self.points.is_empty() {
            return None;
        }
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for p in &self.points {
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }
        let pad = if self.is_textual() {
            self.font_size
        } else {
            self.width.max(2.0)
        };
        Some(RectF {
            x: min_x - pad,
            y: min_y - pad,
            w: (max_x - min_x) + pad * 2.0,
            h: (max_y - min_y) + pad * 2.0,
        })
    }

    pub fn hit(&self, point: Point, tolerance: f32) -> bool {
        let Some(bounds) = self.bounds() else {
            return false;
        };
        bounds.inflate(tolerance).contains(point)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RectF {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl RectF {
    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x <= self.right() && p.y >= self.y && p.y <= self.bottom()
    }

    pub fn inflate(&self, by: f32) -> Self {
        Self {
            x: self.x - by,
            y: self.y - by,
            w: self.w + by * 2.0,
            h: self.h + by * 2.0,
        }
    }

    pub fn intersects(&self, other: &RectF) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }
}

/// A stroke or fill in image coordinates. GPUI and tiny-skia both consume this
/// same list, so the editor and the exported file cannot disagree.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub points: Vec<Point>,
    pub closed: bool,
    pub filled: bool,
}

pub fn parse_hex_color(value: &str) -> [u8; 4] {
    let hex = value.trim().trim_start_matches('#');
    let parse = |s: &str| u8::from_str_radix(s, 16).unwrap_or(0);
    match hex.len() {
        3 => {
            let r = parse(&hex[0..1]);
            let g = parse(&hex[1..2]);
            let b = parse(&hex[2..3]);
            [r * 17, g * 17, b * 17, 255]
        }
        6 => [parse(&hex[0..2]), parse(&hex[2..4]), parse(&hex[4..6]), 255],
        8 => [
            parse(&hex[0..2]),
            parse(&hex[2..4]),
            parse(&hex[4..6]),
            parse(&hex[6..8]),
        ],
        _ => [229, 72, 77, 255],
    }
}

pub fn color_to_hex(rgba: [u8; 4]) -> String {
    if rgba[3] == 255 {
        format!("#{:02x}{:02x}{:02x}", rgba[0], rgba[1], rgba[2])
    } else {
        format!("#{:02x}{:02x}{:02x}{:02x}", rgba[0], rgba[1], rgba[2], rgba[3])
    }
}

fn ellipse_points(rect: RectF) -> Vec<Point> {
    const SEGMENTS: usize = 48;
    let cx = rect.x + rect.w / 2.0;
    let cy = rect.y + rect.h / 2.0;
    let rx = rect.w.abs() / 2.0;
    let ry = rect.h.abs() / 2.0;
    (0..SEGMENTS)
        .map(|i| {
            let angle = i as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
            Point::new(cx + rx * angle.cos(), cy + ry * angle.sin())
        })
        .collect()
}

fn arrow_head(shaft_start: Point, tip: Point, width: f32) -> (Point, Vec<Point>) {
    let dx = tip.x - shaft_start.x;
    let dy = tip.y - shaft_start.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.5 {
        return (tip, Vec::new());
    }
    let ux = dx / len;
    let uy = dy / len;
    let head = (width * 3.6).clamp(8.0, 44.0);
    let base = Point::new(tip.x - ux * head, tip.y - uy * head);
    let nx = -uy;
    let ny = ux;
    let half = head * 0.45;
    let left = Point::new(base.x + nx * half, base.y + ny * half);
    let right = Point::new(base.x - nx * half, base.y - ny * half);
    (base, vec![tip, left, right])
}

/// Strokes and fills for one annotation. Text and labels are drawn separately
/// from `text`/`font_size`.
pub fn annotation_geometry(annotation: &Annotation) -> Vec<Shape> {
    let first = annotation.points.first().copied();
    let last = annotation.points.last().copied();
    match annotation.kind {
        AnnotationKind::Line => match (first, last) {
            (Some(a), Some(b)) => vec![Shape {
                points: vec![a, b],
                closed: false,
                filled: false,
            }],
            _ => Vec::new(),
        },
        AnnotationKind::Arrow => match (first, last) {
            (Some(a), Some(b)) => {
                let (base, head) = arrow_head(a, b, annotation.width);
                let mut shapes = vec![Shape {
                    points: vec![a, base],
                    closed: false,
                    filled: false,
                }];
                if head.len() == 3 {
                    shapes.push(Shape {
                        points: head,
                        closed: true,
                        filled: true,
                    });
                }
                shapes
            }
            _ => Vec::new(),
        },
        AnnotationKind::Rect => match (first, last) {
            (Some(a), Some(b)) => {
                let rect = RectF {
                    x: a.x.min(b.x),
                    y: a.y.min(b.y),
                    w: (b.x - a.x).abs(),
                    h: (b.y - a.y).abs(),
                };
                vec![Shape {
                    points: vec![
                        Point::new(rect.x, rect.y),
                        Point::new(rect.right(), rect.y),
                        Point::new(rect.right(), rect.bottom()),
                        Point::new(rect.x, rect.bottom()),
                    ],
                    closed: true,
                    filled: annotation.filled,
                }]
            }
            _ => Vec::new(),
        },
        AnnotationKind::Ellipse => match (first, last) {
            (Some(a), Some(b)) => {
                let rect = RectF {
                    x: a.x.min(b.x),
                    y: a.y.min(b.y),
                    w: (b.x - a.x).abs(),
                    h: (b.y - a.y).abs(),
                };
                vec![Shape {
                    points: ellipse_points(rect),
                    closed: true,
                    filled: annotation.filled,
                }]
            }
            _ => Vec::new(),
        },
        AnnotationKind::Highlight | AnnotationKind::Pencil => {
            if annotation.points.len() < 2 {
                Vec::new()
            } else {
                vec![Shape {
                    points: annotation.points.clone(),
                    closed: false,
                    filled: false,
                }]
            }
        }
        AnnotationKind::Label => {
            let p = first.unwrap_or(Point::new(0.0, 0.0));
            let r = annotation.font_size.max(8.0);
            let rect = RectF {
                x: p.x - r,
                y: p.y - r,
                w: r * 2.0,
                h: r * 2.0,
            };
            vec![Shape {
                points: ellipse_points(rect),
                closed: true,
                filled: true,
            }]
        }
        AnnotationKind::Text => Vec::new(),
    }
}

/// Shifts every annotation after a destructive crop and drops annotations that
/// fell completely outside the frame.
pub fn shift_annotations_after_crop(annotations: &mut Vec<Annotation>, crop: CropRect) {
    let frame = RectF {
        x: 0.0,
        y: 0.0,
        w: crop.w,
        h: crop.h,
    };
    for annotation in annotations.iter_mut() {
        for p in &mut annotation.points {
            p.x -= crop.x;
            p.y -= crop.y;
        }
    }
    annotations.retain(|annotation| annotation.bounds().is_some_and(|b| b.intersects(&frame)));
}

pub fn scale_annotations(annotations: &mut [Annotation], factor: f32) {
    if !(factor.is_finite() && factor > 0.0) {
        return;
    }
    for annotation in annotations {
        for p in &mut annotation.points {
            p.x *= factor;
            p.y *= factor;
        }
        annotation.width *= factor;
        annotation.font_size *= factor;
    }
}

/// Crop actually applied for display, or `None` when the stored rectangle is
/// the whole picture (so a stale full-size crop cannot change the layout).
pub fn visible_crop(edit: &ImageEdit, natural: (u32, u32)) -> Option<CropRect> {
    let crop = edit.crop.as_ref()?.clamp_to(natural);
    let full = (crop.w >= natural.0 as f32 - 0.5) && (crop.h >= natural.1 as f32 - 0.5);
    if full && crop.x <= 0.5 && crop.y <= 0.5 {
        None
    } else {
        Some(crop)
    }
}

/// Height of a caption block (above or below the picture), including the gap.
pub fn caption_height(edit: &ImageEdit, display_width: f32) -> f32 {
    let Some(caption) = edit.caption.as_ref() else {
        return 0.0;
    };
    if caption.text.trim().is_empty() {
        return 0.0;
    }
    let char_w = 6.2_f32;
    let per_line = (display_width.max(80.0) / char_w).floor().max(1.0);
    let lines = (caption.text.chars().count() as f32 / per_line).ceil().max(1.0);
    lines * 18.0 + 6.0
}
