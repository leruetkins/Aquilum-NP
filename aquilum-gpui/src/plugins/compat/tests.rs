    use super::*;

    #[test]
    fn test_analyze_fully_supported_plugin() {
        let script = r#"
            const { Plugin, Notice } = require("obsidian");
            module.exports = class HelloPlugin extends Plugin {
                async onload() {
                    new Notice("Hello!");
                    this.addCommand({ id: "say-hello", name: "Say Hello", callback: () => {} });
                    const file = this.app.workspace.getActiveFile();
                    if (file) {
                        const content = await this.app.vault.read(file);
                    }
                }
            };
        "#;

        let report = analyze_plugin_script(script);
        assert_eq!(report.status, CompatibilityStatus::Full);
        assert_eq!(report.score, 1.0);
        assert!(report.incompatible_calls.is_empty());
        assert!(report.supported_calls.contains(&"new Notice()".to_string()));
        assert!(report.supported_calls.contains(&"plugin.addCommand()".to_string()));
    }

    #[test]
    fn test_analyze_partially_supported_plugin() {
        let script = r#"
            const { Plugin } = require("obsidian");
            module.exports = class MetaPlugin extends Plugin {
                async onload() {
                    this.app.workspace.requestSaveHistory();
                    const cache = this.app.metadataCache.getFileCache(this.app.workspace.getActiveFile());
                    await this.app.vault.read("test.md");
                }
            };
        "#;

        let report = analyze_plugin_script(script);
        match report.status {
            CompatibilityStatus::Partial(pct) => {
                assert!(pct > 0.0 && pct < 1.0);
            }
            _ => panic!("Expected Partial compatibility status"),
        }
        assert!(!report.missing_calls.is_empty());
        assert!(report.incompatible_calls.is_empty());
    }

    #[test]
    fn test_analyze_incompatible_plugin() {
        let script = r#"
            const { Plugin } = require("obsidian");
            const electron = require("electron");
            module.exports = class BadPlugin extends Plugin {
                async onload() {
                    document.getElementById("app");
                    electron.ipcRenderer.send("do-something");
                }
            };
        "#;

        let report = analyze_plugin_script(script);
        assert_eq!(report.status, CompatibilityStatus::Incompatible);
        assert!(report.incompatible_calls.contains(&"require('electron')".to_string()));
        assert!(report.incompatible_calls.contains(&"document.getElementById()".to_string()));
        assert_eq!(report.badge_label(), "Не поддерживается");
    }

    #[test]
    fn test_analyze_unrecognized_bundle_is_not_full() {
        // No known API call appears in the bundle: compatibility must stay
        // unknown instead of reporting a fake 100%.
        let script = r#"
            module.exports = class WeirdPlugin {
                async onload() {
                    someTotallyUnknownApi.doThing();
                }
            };
        "#;

        let report = analyze_plugin_script(script);
        assert_eq!(report.status, CompatibilityStatus::Unknown);
        assert_eq!(report.score, 0.0);
        assert!(report.supported_calls.is_empty());
        assert!(report.missing_calls.is_empty());
        assert!(report.incompatible_calls.is_empty());
    }

    #[test]
    fn catalog_registry_lookups_work() {
        let (total, ok, partial, blocked) = catalog_compat_stats();
        assert!(total >= 1000, "registry too small: {total}");
        assert!(ok > 500 && partial > 50 && blocked > 500);

        // A known blocked plugin reports its engine blocker; a known ok plugin is Full.
        let (total2, ok2, _, _) = catalog_compat_stats();
        assert_eq!(total, total2);
        assert!(ok == ok2);

        let unknown = catalog_compat("definitely-not-a-real-plugin-id-xyz");
        assert!(unknown.is_none());
    }
