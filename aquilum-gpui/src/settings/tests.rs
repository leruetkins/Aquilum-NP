    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_settings_registry_ids_unique() {
        let mut ids = HashSet::new();
        for s in SETTINGS {
            assert!(ids.insert(s.id), "Duplicate setting id: {}", s.id);
            assert_eq!(s.url(), format!("{}://settings/{}", APP_URL_SCHEME, s.id));
        }
    }

    #[test]
    fn test_get_setting_by_id_and_alias() {
        let s = get_setting_by_id("ui.theme").expect("ui.theme must exist");
        assert_eq!(s.id, "ui.theme");

        let s_alias = get_setting_by_id("appearance.theme").expect("appearance.theme alias must resolve");
        assert_eq!(s_alias.id, "ui.theme");

        assert!(get_setting_by_id("non.existent.id").is_none());
    }

    #[test]
    fn test_get_and_set_config_value() {
        let mut config = AppConfig::default();
        let val = get_config_value(&config, "ui.theme").expect("ui.theme value");
        assert_eq!(val, serde_json::json!("system"));

        assert!(!is_setting_modified(&config, "ui.theme"));

        set_config_value(&mut config, "ui.theme", serde_json::json!("dark")).expect("set ui.theme");
        assert_eq!(config.ui.theme, "dark");
        assert!(is_setting_modified(&config, "ui.theme"));

        set_config_value(&mut config, "editor.wordWrap", serde_json::json!(false)).expect("set editor.wordWrap");
        assert!(!config.editor.word_wrap);
        assert!(is_setting_modified(&config, "editor.wordWrap"));
    }

    #[test]
    fn test_ui_theme_slots_default_and_serde() {
        let default_ui = UiSettings::default();
        assert_eq!(default_ui.theme, "system");
        assert_eq!(default_ui.light_theme, "light");
        assert_eq!(default_ui.dark_theme, "dark");

        let parsed: UiSettings =
            serde_json::from_str(r#"{"theme":"dark","darkTheme":"obsidian-dark"}"#).unwrap();
        assert_eq!(parsed.dark_theme, "obsidian-dark");
        assert_eq!(parsed.light_theme, "light");

        let serialized = serde_json::to_string(&parsed).unwrap();
        assert!(serialized.contains(r#""lightTheme":"light""#));
    }

    #[test]
    fn test_theme_slot_settings_are_registered() {
        assert_eq!(get_setting_by_id("ui.lightTheme").map(|s| s.id), Some("ui.lightTheme"));
        assert_eq!(get_setting_by_id("ui.darkTheme").map(|s| s.id), Some("ui.darkTheme"));
    }

    #[test]
    fn test_search_settings() {
        // Direct ID and URL match
        let res_id = search_settings("editor.wordWrap", "ru");
        assert_eq!(res_id.len(), 1);
        assert_eq!(res_id[0].id, "editor.wordWrap");

        let res_url = search_settings(&format!("{}://settings/editor.wordWrap", APP_URL_SCHEME), "en");
        assert_eq!(res_url.len(), 1);
        assert_eq!(res_url[0].id, "editor.wordWrap");

        // Russian keyword search
        let res_ru = search_settings("перенос", "ru");
        assert!(res_ru.iter().any(|s| s.id == "editor.wordWrap"));

        // English keyword search
        let res_en = search_settings("wrap", "en");
        assert!(res_en.iter().any(|s| s.id == "editor.wordWrap"));

        // Multi-keyword search
        let res_font = search_settings("шрифт размер", "ru");
        assert!(res_font.iter().any(|s| s.id == "editor.fontSizeBase"));

        // A Russian query still matches while the interface is in another language
        let res_ru_in_de = search_settings("перенос", "de");
        assert!(res_ru_in_de.iter().any(|s| s.id == "editor.wordWrap"));
    }

    #[test]
    fn test_exact_reveal_cursor_default_and_serde() {
        let default_settings = EditorSettings::default();
        assert!(default_settings.exact_reveal_cursor);

        let json_empty = "{}";
        let parsed: EditorSettings = serde_json::from_str(json_empty).unwrap();
        assert!(parsed.exact_reveal_cursor);

        let json_false = r#"{"exactRevealCursor": false}"#;
        let parsed_false: EditorSettings = serde_json::from_str(json_false).unwrap();
        assert!(!parsed_false.exact_reveal_cursor);
    }

    #[test]
    fn test_editor_width_pct_default_and_serde() {
        let default_settings = EditorSettings::default();
        assert_eq!(default_settings.editor_width_pct, 100);

        // Deserializing empty JSON should default editor_width_pct to 100
        let json_empty = "{}";
        let parsed: EditorSettings = serde_json::from_str(json_empty).unwrap();
        assert_eq!(parsed.editor_width_pct, 100);

        // Deserializing with custom percentage (e.g. 45 from slider)
        let json_custom = r#"{"editorWidthPct": 45}"#;
        let parsed_custom: EditorSettings = serde_json::from_str(json_custom).unwrap();
        assert_eq!(parsed_custom.editor_width_pct, 45);

        // Serialization includes editorWidthPct
        let serialized = serde_json::to_string(&parsed_custom).unwrap();
        assert!(serialized.contains(r#""editorWidthPct":45"#));
    }

    #[test]
    fn test_note_align_default_and_serde() {
        let default_settings = EditorSettings::default();
        assert_eq!(default_settings.note_align, NoteAlignment::Center);

        // Deserializing empty JSON should default note_align to Center
        let json_empty = "{}";
        let parsed: EditorSettings = serde_json::from_str(json_empty).unwrap();
        assert_eq!(parsed.note_align, NoteAlignment::Center);

        // Deserializing with custom alignment "left"
        let json_left = r#"{"noteAlign": "left"}"#;
        let parsed_left: EditorSettings = serde_json::from_str(json_left).unwrap();
        assert_eq!(parsed_left.note_align, NoteAlignment::Left);

        // Deserializing with custom alignment "center"
        let json_center = r#"{"noteAlign": "center"}"#;
        let parsed_center: EditorSettings = serde_json::from_str(json_center).unwrap();
        assert_eq!(parsed_center.note_align, NoteAlignment::Center);

        // Serialization includes noteAlign
        let serialized = serde_json::to_string(&parsed_left).unwrap();
        assert!(serialized.contains(r#""noteAlign":"left""#));
    }

    #[test]
    fn test_title_align_default_and_serde() {
        let default_settings = EditorSettings::default();
        assert_eq!(default_settings.title_align, TitleAlignment::WindowLeft);

        // Deserializing empty JSON should default title_align to WindowLeft
        let json_empty = "{}";
        let parsed: EditorSettings = serde_json::from_str(json_empty).unwrap();
        assert_eq!(parsed.title_align, TitleAlignment::WindowLeft);

        // Deserializing legacy "center" and "left" aliases
        let json_legacy_center = r#"{"titleAlign": "center"}"#;
        let parsed_legacy_center: EditorSettings = serde_json::from_str(json_legacy_center).unwrap();
        assert_eq!(parsed_legacy_center.title_align, TitleAlignment::WindowCenter);

        let json_legacy_left = r#"{"titleAlign": "left"}"#;
        let parsed_legacy_left: EditorSettings = serde_json::from_str(json_legacy_left).unwrap();
        assert_eq!(parsed_legacy_left.title_align, TitleAlignment::WindowLeft);

        // Deserializing editor_left and editor_center
        let json_editor_left = r#"{"titleAlign": "editor_left"}"#;
        let parsed_editor_left: EditorSettings = serde_json::from_str(json_editor_left).unwrap();
        assert_eq!(parsed_editor_left.title_align, TitleAlignment::EditorLeft);

        let json_editor_center = r#"{"titleAlign": "editor_center"}"#;
        let parsed_editor_center: EditorSettings = serde_json::from_str(json_editor_center).unwrap();
        assert_eq!(parsed_editor_center.title_align, TitleAlignment::EditorCenter);

        // Serialization produces snake_case
        let serialized = serde_json::to_string(&parsed_editor_center).unwrap();
        assert!(serialized.contains(r#""titleAlign":"editor_center""#));
    }
