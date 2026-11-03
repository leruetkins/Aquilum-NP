    use super::*;
    use crate::images::{Annotation, AnnotationKind, ImageEdit, Point};

    fn canvas(w: u32, h: u32) -> image::RgbaImage {
        image::RgbaImage::from_pixel(w, h, image::Rgba([40, 40, 220, 255]))
    }

    #[test]
    fn a_filled_rectangle_changes_its_interior_pixels() {
        let mut bytes = canvas(40, 40).into_raw();
        let annotation = Annotation {
            kind: AnnotationKind::Rect,
            points: vec![Point::new(5.0, 5.0), Point::new(30.0, 30.0)],
            color: "#ffffff".to_string(),
            width: 3.0,
            filled: true,
            text: None,
            font_size: 16.0,
        };
        draw_annotations(&mut bytes, 40, 40, &[annotation], (0.0, 0.0));
        let image = image::RgbaImage::from_raw(40, 40, bytes).unwrap();
        assert_eq!(image.get_pixel(15, 15)[0], 255);
        assert_eq!(image.get_pixel(1, 1)[0], 40);
    }

    #[test]
    fn rounded_mask_clears_the_corner_alpha() {
        let mut image = canvas(100, 60);
        apply_round_mask(&mut image, 20.0);
        assert_eq!(image.get_pixel(0, 0)[3], 0);
        assert_eq!(image.get_pixel(50, 30)[3], 255);
    }

    #[test]
    fn text_rasterization_produces_cyrillic_pixels() {
        let (w, h, rgba) = rasterize_text("Привет", 20.0, [0, 0, 0, 255], 200.0).unwrap();
        assert!(w > 10 && h > 10);
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        assert!(rgba.chunks_exact(4).any(|p| p[3] > 0));
    }

    #[test]
    fn baking_applies_crop_and_annotations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.png");
        canvas(80, 40).save(&path).unwrap();
        let edit = ImageEdit {
            crop: Some(crate::images::CropRect { x: 10.0, y: 5.0, w: 40.0, h: 20.0 }),
            annotations: vec![Annotation {
                kind: AnnotationKind::Line,
                points: vec![Point::new(10.0, 10.0), Point::new(40.0, 30.0)],
                ..Default::default()
            }],
            ..Default::default()
        };
        let baked = bake_rgba(
            &path,
            &edit,
            &BakeOptions { apply_crop: true, ..Default::default() },
        )
        .unwrap();
        assert_eq!(baked.dimensions(), (40, 20));
    }
