    use super::*;

    #[test]
    fn test_deserialize_community_plugin_entry() {
        let json_str = r#"[
            {
                "id": "obsidian-new-note-np",
                "name": "New Note NP",
                "author": "leruetkins",
                "description": "Adds a Create new note button back to the left ribbon.",
                "repo": "leruetkins/obsidian-new-note-np"
            }
        ]"#;

        let entries: Vec<CatalogPluginEntry> = serde_json::from_str(json_str).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "obsidian-new-note-np");
        assert_eq!(entries[0].name, "New Note NP");
        assert_eq!(entries[0].version, "1.0.0");
        assert_eq!(entries[0].downloads, 0);
    }

    #[test]
    fn test_disk_cache_roundtrip() {
        let unique_name = format!("cache_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
        let temp_file = std::env::temp_dir().join(unique_name).join("community-plugins.json");
        let parent = temp_file.parent().unwrap();
        std::fs::create_dir_all(parent).unwrap();

        let entries = vec![
            CatalogPluginEntry {
                id: "test-plugin".to_string(),
                name: "Test Plugin".to_string(),
                author: "Tester".to_string(),
                description: "Description".to_string(),
                repo: "tester/test-plugin".to_string(),
                branch: None,
                version: "2.0.0".to_string(),
                downloads: 4200,
                updated: Some(1700000000000),
            }
        ];

        let json = serde_json::to_string(&entries).unwrap();
        std::fs::write(&temp_file, &json).unwrap();

        let read_str = std::fs::read_to_string(&temp_file).unwrap();
        let restored: Vec<CatalogPluginEntry> = serde_json::from_str(&read_str).unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].id, "test-plugin");
        assert_eq!(restored[0].downloads, 4200);

        let _ = std::fs::remove_dir_all(parent);
    }
