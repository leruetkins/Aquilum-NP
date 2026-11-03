    use super::*;

    #[test]
    fn test_code_block_registry_lifecycle() {
        let reg = CodeBlockProcessorRegistry::new();
        assert!(!reg.has_processor("dataview"));

        reg.register("dataview", "plugin-dataview");
        assert!(reg.has_processor("dataview"));
        assert!(reg.has_processor("DataView")); // Case-insensitive
        assert_eq!(reg.get_plugin_for_lang("dataview"), Some("plugin-dataview".to_string()));

        reg.register("tasks", "plugin-tasks");
        assert_eq!(reg.all_languages().len(), 2);

        reg.unregister_all_of("plugin-dataview");
        assert!(!reg.has_processor("dataview"));
        assert!(reg.has_processor("tasks"));

        reg.clear();
        assert_eq!(reg.all_languages().len(), 0);
    }
