    use super::*;

    #[test]
    fn test_manifest_validation() {
        let manifest = PluginManifest {
            id: "my-sample-plugin".into(),
            name: "My Sample Plugin".into(),
            version: "1.0.0".into(),
            min_app_version: Some("0.1.0".into()),
            description: "A great plugin".into(),
            author: "Author".into(),
            permissions: PluginPermissions {
                vault: VaultAccess::Write,
                network: vec!["api.github.com".into()],
            },
        };

        assert!(manifest.validate().is_ok());
        assert!(manifest.is_compatible_with_host("0.1.0"));
        assert!(manifest.is_compatible_with_host("1.0.0"));
        assert!(!manifest.is_compatible_with_host("0.0.9"));
    }

    #[test]
    fn test_manifest_invalid_id() {
        let mut manifest = PluginManifest {
            id: "invalid id with spaces!".into(),
            name: "Plugin".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };

        assert!(manifest.validate().is_err());
        manifest.id = "valid-id_123".into();
        assert!(manifest.validate().is_ok());

        // Reserved Windows names must fail
        manifest.id = "con".into();
        assert!(manifest.validate().is_err());
        manifest.id = "aux".into();
        assert!(manifest.validate().is_err());
        manifest.id = "com1".into();
        assert!(manifest.validate().is_err());

        // Leading dash must fail
        manifest.id = "-invalid-start".into();
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_plugin_grants_store_and_sha256() {
        let bundle_bytes = b"console.log('Hello Obsidian Plugin');";
        let hash = compute_sha256(bundle_bytes);
        assert_eq!(hash.len(), 64);

        let mut store = PluginGrantsStore::default();
        let plugin_id = "sample-plugin";

        assert!(!store.is_trusted(plugin_id, &hash));

        store.add_grant(
            plugin_id,
            &hash,
            PluginPermissions {
                vault: VaultAccess::Read,
                network: vec!["api.example.com".into()],
            },
        );

        assert!(store.is_trusted(plugin_id, &hash));
        assert!(!store.is_trusted(plugin_id, "other_hash"));
        assert!(!store.is_trusted("other_plugin", &hash));

        let grant = store.get_grant(plugin_id, &hash).unwrap();
        assert_eq!(grant.granted_permissions.vault, VaultAccess::Read);
        assert_eq!(grant.granted_permissions.network, vec!["api.example.com".to_string()]);
    }
