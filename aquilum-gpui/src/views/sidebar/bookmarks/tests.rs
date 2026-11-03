use std::path::{Path, PathBuf};
    use super::*;

    #[test]
    fn test_obsidian_bookmarks_json_compatibility() {
        let json = r#"{
            "items": [
                { "type": "file", "path": "Дневник.md" },
                { "type": "file", "path": "Минифакью.md" },
                { "type": "folder", "path": "АСТРАЛИНУКС" },
                { "type": "folder", "path": "Продукты" },
                {
                    "type": "group",
                    "title": "Группа без названия",
                    "items": [
                        { "type": "file", "path": "Подключение к стенду.md" }
                    ]
                }
            ]
        }"#;

        let parsed: BookmarksData = serde_json::from_str(json).expect("Must parse Obsidian bookmarks json");
        assert_eq!(parsed.items.len(), 5);
        assert!(parsed.contains_file("Дневник.md"));
        assert!(parsed.contains_file("Минифакью.md"));
        assert!(parsed.contains_file("Подключение к стенду.md"));
        assert!(parsed.contains_folder("АСТРАЛИНУКС"));
        assert!(parsed.contains_folder("Продукты"));
        assert!(!parsed.contains_file("Несуществующая заметка.md"));

        // Round-trip serialize
        let serialized = serde_json::to_string_pretty(&parsed).expect("Must serialize");
        let round_trip: BookmarksData = serde_json::from_str(&serialized).expect("Must re-parse");
        assert_eq!(parsed, round_trip);
    }

    #[test]
    fn test_bookmarks_add_and_remove() {
        let mut data = BookmarksData::default();
        assert!(data.is_empty());

        data.add_file("NoteA.md".into(), None, None);
        assert!(data.contains_file("NoteA.md"));
        assert_eq!(data.items.len(), 1);

        // Avoid duplicate add
        data.add_file("NoteA.md".into(), None, None);
        assert_eq!(data.items.len(), 1);

        data.add_folder("FolderA".into(), None);
        assert!(data.contains_folder("FolderA"));
        assert_eq!(data.items.len(), 2);

        // Group creation
        let group_title = data.create_group("Группа без названия");
        assert_eq!(group_title, "Группа без названия");
        assert_eq!(data.items.len(), 3);

        // Adding into group
        data.add_file("NoteInGroup.md".into(), None, Some("Группа без названия"));
        assert!(data.contains_file("NoteInGroup.md"));

        // Remove file from root
        let removed = data.remove_file("NoteA.md");
        assert!(removed);
        assert!(!data.contains_file("NoteA.md"));

        // Remove file from group
        let removed_group_item = data.remove_file("NoteInGroup.md");
        assert!(removed_group_item);
        assert!(!data.contains_file("NoteInGroup.md"));

        // Remove folder
        let removed_folder = data.remove_folder("FolderA");
        assert!(removed_folder);
        assert!(!data.contains_folder("FolderA"));
    }

    #[test]
    fn test_path_normalization() {
        let mut data = BookmarksData::default();
        data.add_file("sub\\path\\to\\Note.md".into(), None, None);

        assert!(data.contains_file("sub/path/to/Note.md"));
        assert!(data.contains_file("sub\\path\\to\\Note.md"));

        assert!(data.remove_file("sub/path/to/Note.md"));
        assert!(!data.contains_file("sub\\path\\to\\Note.md"));
    }

    #[test]
    fn test_drag_and_drop_reorder_and_move() {
        let mut data = BookmarksData::default();
        data.add_file("Note1.md".into(), None, None);
        data.add_file("Note2.md".into(), None, None);
        data.create_group("GroupA");
        data.create_group("GroupB");

        // Move Note1 from root (index 0) into GroupA (group index 2 in items)
        let item = data.items.remove(0);
        if let BookmarkItem::Group { ref mut items, .. } = data.items[1] {
            items.push(item);
        }
        assert_eq!(data.items.len(), 3);
        if let BookmarkItem::Group { ref items, .. } = data.items[1] {
            assert_eq!(items.len(), 1);
            assert_eq!(
                items[0],
                BookmarkItem::File {
                    path: "Note1.md".into(),
                    title: None,
                    subpath: None
                }
            );
        }

        // Move Note1 from GroupA to GroupB
        let item = if let BookmarkItem::Group { ref mut items, .. } = data.items[1] {
            items.remove(0)
        } else {
            panic!("Expected GroupA");
        };
        if let BookmarkItem::Group { ref mut items, .. } = data.items[2] {
            items.push(item);
        }
        if let BookmarkItem::Group { ref items, .. } = data.items[2] {
            assert_eq!(items.len(), 1);
            assert_eq!(
                items[0],
                BookmarkItem::File {
                    path: "Note1.md".into(),
                    title: None,
                    subpath: None
                }
            );
        }

        // Move Note1 from GroupB back to Root
        let item = if let BookmarkItem::Group { ref mut items, .. } = data.items[2] {
            items.remove(0)
        } else {
            panic!("Expected GroupB");
        };
        data.items.push(item);
        assert_eq!(data.items.len(), 4);
        assert_eq!(
            data.items.last().unwrap(),
            &BookmarkItem::File {
                path: "Note1.md".into(),
                title: None,
                subpath: None
            }
        );
    }

    #[test]
    fn test_rename_group_and_items() {
        let mut data = BookmarksData::default();
        data.create_group("OriginalGroup");
        data.add_file("Note1.md".into(), None, Some("OriginalGroup"));
        data.add_folder("Folder1".into(), None);

        // Rename group
        data.rename_group(0, "RenamedGroup".into());
        if let BookmarkItem::Group { ref title, .. } = data.items[0] {
            assert_eq!(title, "RenamedGroup");
        } else {
            panic!("Expected group");
        }

        // Rename file in group
        data.rename_file_bookmark(Some(0), 0, "CustomNoteTitle".into());
        if let BookmarkItem::Group { ref items, .. } = data.items[0] {
            if let BookmarkItem::File { ref title, .. } = items[0] {
                assert_eq!(title.as_deref(), Some("CustomNoteTitle"));
            } else {
                panic!("Expected file");
            }
        }

        // Rename folder
        data.rename_folder_bookmark(1, "CustomFolderTitle".into());
        if let BookmarkItem::Folder { ref title, .. } = data.items[1] {
            assert_eq!(title.as_deref(), Some("CustomFolderTitle"));
        } else {
            panic!("Expected folder");
        }
    }

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("vibecodium_bm_test_{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn test_collect_files_in_folder_and_bookmark_group() {
        let temp = TempDir::new();
        let root = temp.path();

        let sub_folder = root.join("SubFolder");
        std::fs::create_dir_all(&sub_folder).unwrap();

        let file1 = root.join("Note1.md");
        let file2 = sub_folder.join("Note2.md");
        let ignored_file = root.join(".hidden.md");
        let git_dir = root.join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let git_file = git_dir.join("git_note.md");

        std::fs::write(&file1, "Hello 1").unwrap();
        std::fs::write(&file2, "Hello 2").unwrap();
        std::fs::write(&ignored_file, "Hidden").unwrap();
        std::fs::write(&git_file, "Git").unwrap();

        // Test collect_files_in_folder
        let collected = collect_files_in_folder(root);
        assert_eq!(collected.len(), 2);
        assert!(collected.contains(&file1));
        assert!(collected.contains(&file2));
        assert!(!collected.contains(&ignored_file));
        assert!(!collected.contains(&git_file));

        // Test collect_files_in_bookmark_group
        let group_items = vec![
            BookmarkItem::File {
                path: "Note1.md".to_string(),
                title: None,
                subpath: None,
            },
            BookmarkItem::Folder {
                path: "SubFolder".to_string(),
                title: None,
            },
        ];

        let group_files = collect_files_in_bookmark_group(Some(root), &group_items);
        assert_eq!(group_files.len(), 2);
        assert!(group_files.contains(&file1));
        assert!(group_files.contains(&file2));
    }

    #[test]
    fn test_vault_bookmarks_json_persistence() {
        let temp = TempDir::new();
        let root = temp.path();

        let vault_dir = root.join(".vault");
        std::fs::create_dir_all(&vault_dir).unwrap();
        let bm_path = vault_dir.join("bookmarks.json");

        let mut data = BookmarksData::default();
        data.add_file("TestNote.md".to_string(), Some("My Note".to_string()), None);
        data.add_folder("Work".to_string(), None);

        let json = serde_json::to_string_pretty(&data).unwrap();
        std::fs::write(&bm_path, json).unwrap();

        assert!(bm_path.exists());
        let read_back = std::fs::read_to_string(&bm_path).unwrap();
        let loaded: BookmarksData = serde_json::from_str(&read_back).unwrap();

        assert_eq!(data, loaded);
        assert!(loaded.contains_file("TestNote.md"));
        assert!(loaded.contains_folder("Work"));
    }
