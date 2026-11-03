use std::io::Cursor;
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbaImage::new(width, height)
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        bytes
    }

    #[test]
    fn names_are_timestamped_and_unique() {
        let first = new_name("png");
        let second = new_name("png");
        assert!(first.ends_with(".png"));
        assert!(first.len() > 20);
        assert_ne!(first, second);
    }

    #[test]
    fn a_wiki_reference_parses_with_and_without_size() {
        let plain = parse_image_reference("![[2026-10-31-1515-k3x9.png]]").unwrap();
        assert_eq!(plain.target, "2026-10-31-1515-k3x9.png");
        assert_eq!((plain.width, plain.height), (None, None));
        let sized = parse_image_reference("  ![[a.png|300x200]]  ").unwrap();
        assert_eq!((sized.width, sized.height), (Some(300), Some(200)));
        assert_eq!(sized.markup(), "![[a.png|300x200]]");
        let width = parse_image_reference("![[a.png|300]]").unwrap();
        assert_eq!(width.markup(), "![[a.png|300]]");
    }

    #[test]
    fn text_and_remote_urls_are_not_attachments() {
        assert!(parse_image_reference("text ![[a.png]]").is_none());
        assert!(parse_image_reference("![alt](https://example.com/a.png)").is_none());
        assert!(parse_image_reference("![alt](attachments/a.png)").is_some());
    }

    #[test]
    fn display_size_keeps_the_ratio_and_fits_the_column() {
        let image = parse_image_reference("![[a.png]]").unwrap();
        let (w, h) = image.display_size((2000, 1000), 600.0);
        assert_eq!((w, h), (600.0, 300.0));
        let sized = image.with_size(Some(300), None);
        let (w, h) = sized.display_size((2000, 1000), 600.0);
        assert_eq!((w, h), (300.0, 150.0));
        let both = image.with_size(Some(300), Some(400));
        assert_eq!(both.display_size((2000, 1000), 600.0), (300.0, 400.0));
    }

    #[test]
    fn clipboard_raster_is_stored_as_png() {
        let dir = tempfile::tempdir().unwrap();
        let note = dir.path().join("Note.md");
        std::fs::write(&note, "").unwrap();
        let imported = import_images(
            &note,
            "attachments",
            ImageImportSource::Clipboard { bytes: png(4, 3), format: gpui::ImageFormat::Png },
        );
        let image = imported.into_iter().next().unwrap().unwrap();
        assert_eq!((image.width, image.height), (4, 3));
        assert!(image.path.is_file());
        assert!(image.path.parent().unwrap().ends_with("attachments"));
    }

    #[test]
    fn a_file_is_copied_byte_for_byte_and_resolved_beside_the_note() {
        let dir = tempfile::tempdir().unwrap();
        let note = dir.path().join("sub").join("Note.md");
        std::fs::create_dir_all(note.parent().unwrap()).unwrap();
        std::fs::write(&note, "").unwrap();
        let source = dir.path().join("source.png");
        std::fs::write(&source, png(2, 2)).unwrap();
        let imported = import_images(&note, "attachments", ImageImportSource::Files(vec![source]))
            .into_iter()
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(std::fs::read(&imported.path).unwrap(), png(2, 2));
        let resolved = resolve_attachment(Some(dir.path()), &note, "attachments", &imported.name).unwrap();
        assert_eq!(resolved, imported.path);
    }

    #[test]
    fn a_damaged_file_is_reported_and_other_files_still_import() {
        let dir = tempfile::tempdir().unwrap();
        let note = dir.path().join("Note.md");
        std::fs::write(&note, "").unwrap();
        let bad = dir.path().join("bad.png");
        std::fs::write(&bad, b"not an image").unwrap();
        let good_source = dir.path().join("good.png");
        std::fs::write(&good_source, png(1, 1)).unwrap();
        let results = import_images(
            &note,
            "attachments",
            ImageImportSource::Files(vec![bad, good_source]),
        );
        assert!(results[0].is_err());
        assert!(results[1].is_ok());
    }

    #[test]
    fn edit_sidecar_is_dropped_when_it_becomes_empty() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("a.png");
        std::fs::write(&image, png(1, 1)).unwrap();
        set_corner_radius(&image, Some(8.0)).unwrap();
        assert!(edit_path(&image).is_file());
        assert_eq!(load_edit(&image).corner_radius, Some(8.0));
        set_corner_radius(&image, None).unwrap();
        assert!(!edit_path(&image).is_file());
    }

    #[test]
    fn a_dib_is_wrapped_into_a_decodable_bmp() {
        let width = 2u32;
        let height = 2u32;
        let stride = ((width * 3).div_ceil(4) * 4) as usize;
        let mut dib = vec![0u8; 40 + stride * height as usize];
        dib[0..4].copy_from_slice(&40u32.to_le_bytes());
        dib[4..8].copy_from_slice(&width.to_le_bytes());
        dib[8..12].copy_from_slice(&height.to_le_bytes());
        dib[12..14].copy_from_slice(&1u16.to_le_bytes());
        dib[14..16].copy_from_slice(&24u16.to_le_bytes());
        let bmp = dib_to_bmp(&dib).unwrap();
        assert_eq!(&bmp[0..2], b"BM");
        assert_eq!(u32::from_le_bytes(bmp[2..6].try_into().unwrap()) as usize, bmp.len());
        assert_eq!(u32::from_le_bytes(bmp[10..14].try_into().unwrap()), 14 + 40);
        let decoded = image::load_from_memory_with_format(&bmp, image::ImageFormat::Bmp).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (2, 2));
    }

    #[test]
    fn radius_is_a_share_of_the_shorter_side() {
        let edit = ImageEdit { corner_radius: Some(10.0), ..Default::default() };
        assert_eq!(radius_px(&edit, 400.0, 200.0), 20.0);
    }

    #[test]
    fn copying_between_notes_keeps_pixels_and_sidecar_under_a_new_name() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let from = root.join("a").join("NoteA.md");
        let to = root.join("b").join("NoteB.md");
        std::fs::create_dir_all(from.parent().unwrap()).unwrap();
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        let source_dir = attachments_dir(&from, "Files");
        std::fs::create_dir_all(&source_dir).unwrap();
        let source = source_dir.join("pic.png");
        std::fs::write(&source, b"pixels").unwrap();
        std::fs::write(edit_path(&source), b"{\"version\":1}").unwrap();

        let copied = copy_attachment_to(Some(root), &from, &to, "Files", "pic.png").unwrap();
        assert_ne!(copied, "pic.png");
        let target = attachments_dir(&to, "Files").join(&copied);
        assert_eq!(std::fs::read(&target).unwrap(), b"pixels");
        assert!(edit_path(&target).is_file());
        assert!(source.is_file());
    }

    #[test]
    fn saving_without_a_reference_moves_the_pair_to_the_trash() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let note = root.join("Test").join("test1.md");
        std::fs::create_dir_all(note.parent().unwrap()).unwrap();
        std::fs::write(&note, "").unwrap();
        let source_dir = attachments_dir(&note, "Files");
        std::fs::create_dir_all(&source_dir).unwrap();
        let source = source_dir.join("pic.png");
        std::fs::write(&source, b"pixels").unwrap();
        std::fs::write(edit_path(&source), b"{\"version\":1}").unwrap();

        let mut previous = std::collections::HashSet::new();
        previous.insert("pic.png".to_string());
        reconcile_saved_references(
            Some(root),
            &note,
            "Files",
            &previous,
            &std::collections::HashSet::new(),
            true,
        );

        assert!(!source.exists());
        let trashed = trash_dir(root, "Files").join("pic.png");
        assert_eq!(std::fs::read(&trashed).unwrap(), b"pixels");
        assert!(edit_path(&trashed).is_file());
    }

    #[test]
    fn saving_without_a_reference_keeps_the_pair_when_auto_trash_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let note = root.join("Test").join("test2.md");
        std::fs::create_dir_all(note.parent().unwrap()).unwrap();
        std::fs::write(&note, "").unwrap();
        let source_dir = attachments_dir(&note, "Files");
        std::fs::create_dir_all(&source_dir).unwrap();
        let source = source_dir.join("pic2.png");
        std::fs::write(&source, b"pixels").unwrap();

        let mut previous = std::collections::HashSet::new();
        previous.insert("pic2.png".to_string());
        reconcile_saved_references(
            Some(root),
            &note,
            "Files",
            &previous,
            &std::collections::HashSet::new(),
            false,
        );

        assert!(source.exists(), "Attachment should not be trashed when auto_trash is disabled");
    }

    #[test]
    fn moving_between_notes_preserves_the_name_and_the_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let from = root.join("a").join("NoteA.md");
        let to = root.join("b").join("NoteB.md");
        std::fs::create_dir_all(from.parent().unwrap()).unwrap();
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::write(&from, "").unwrap();
        std::fs::write(&to, "").unwrap();
        let source_dir = attachments_dir(&from, "Files");
        std::fs::create_dir_all(&source_dir).unwrap();
        let source = source_dir.join("pic.png");
        std::fs::write(&source, b"pixels").unwrap();
        std::fs::write(edit_path(&source), b"{\"version\":1}").unwrap();

        let moved = move_attachment(Some(root), &from, &to, "Files", "pic.png").unwrap();
        assert_eq!(moved, "pic.png");
        let target = attachments_dir(&to, "Files").join("pic.png");
        assert_eq!(std::fs::read(&target).unwrap(), b"pixels");
        assert!(edit_path(&target).is_file());
        assert!(!source.exists());
    }

    #[test]
    fn referenced_elsewhere_streams_files_and_ignores_case() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let note1 = root.join("Note1.md");
        let note2 = root.join("Note2.md");
        let huge = root.join("Huge.md");

        std::fs::write(&note1, "# Title\nSome text\n![[Target.PNG]]\nMore text").unwrap();
        std::fs::write(&note2, "# Other\n![[other.png]]").unwrap();

        // Simulate a large file (> 256KB) filled with filler lines to ensure
        // buffer boundary traversal works properly and does not read whole files.
        let mut filler = String::with_capacity(300 * 1024);
        for i in 0..10_000 {
            filler.push_str("ordinary line of text without any image references\n");
            if i == 5000 {
                filler.push_str("![[shared.png]]\n");
            }
        }
        std::fs::write(&huge, &filler).unwrap();

        // 1. Found in Note1 (case-insensitive)
        assert!(referenced_elsewhere(root, "target.png", &[]));
        assert!(referenced_elsewhere(root, "Target.PNG", &[]));

        // 2. Excluded note is skipped
        assert!(!referenced_elsewhere(root, "target.png", &[&note1]));

        // 3. Found deep inside a large file across chunks
        assert!(referenced_elsewhere(root, "shared.png", &[]));

        // 4. Non-existent reference is not found
        assert!(!referenced_elsewhere(root, "nonexistent.png", &[]));
    }

    #[test]
    fn attachment_ref_index_updates_and_tracks_references_in_memory() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let note_a = root.join("A.md");
        let note_b = root.join("B.md");

        std::fs::write(&note_a, "![[photo.png]]").unwrap();
        std::fs::write(&note_b, "no attachments").unwrap();

        // 1. Initial indexing
        assert!(referenced_elsewhere(root, "photo.png", &[]));
        assert!(!referenced_elsewhere(root, "photo.png", &[&note_a]));

        // 2. Note B adds reference to photo.png in memory
        let mut b_refs = std::collections::HashSet::new();
        b_refs.insert("photo.png".to_string());
        update_note_references(&note_b, &b_refs);

        // Now photo.png is referenced by Note B too
        assert!(referenced_elsewhere(root, "photo.png", &[&note_a]));

        // 3. Note A removes its reference
        update_note_references(&note_a, &std::collections::HashSet::new());
        assert!(!referenced_elsewhere(root, "photo.png", &[&note_b]));
        assert!(referenced_elsewhere(root, "photo.png", &[&note_a]));

        // 4. Note B deleted
        remove_note_from_ref_index(&note_b);
        assert!(!referenced_elsewhere(root, "photo.png", &[]));
    }
