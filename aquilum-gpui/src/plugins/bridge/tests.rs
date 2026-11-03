    use super::*;
    use crate::plugins::manifest::PluginPermissions;
    use futures::channel::mpsc::unbounded;

    #[test]
    fn test_plugin_permissions_enforcement() {
        let host = PluginHostState::new();
        let (tx, _rx) = unbounded();

        let manifest = PluginManifest {
            id: "readonly-plugin".into(),
            name: "ReadOnly".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions {
                vault: VaultAccess::Read,
                network: vec!["api.github.com".into()],
            },
        };

        host.register_plugin(manifest, tx);

        // Read is allowed
        assert!(host
            .check_permission(
                "readonly-plugin",
                &ApiRequest::VaultRead {
                    path: "note.md".into()
                }
            )
            .is_ok());

        // Write is rejected
        assert!(host
            .check_permission(
                "readonly-plugin",
                &ApiRequest::VaultModify {
                    path: "note.md".into(),
                    data: "new".into(),
                }
            )
            .is_err());

        // Allowed network domain passes
        assert!(host
            .check_permission(
                "readonly-plugin",
                &ApiRequest::Http {
                    url: "https://api.github.com/repos".into(),
                    method: "GET".into(),
                    body: None,
                }
            )
            .is_ok());

        // OpenFile requires vault:read (readonly passes, none fails)
        assert!(host
            .check_permission(
                "readonly-plugin",
                &ApiRequest::OpenFile {
                    path: "note.md".into(),
                }
            )
            .is_ok());

        // SSRF: localhost and private IPs are blocked even if whitelist contains wildcards
        assert!(validate_network_url("http://localhost:8080/admin", &["*".into()]).is_err());
        assert!(validate_network_url("http://127.0.0.1:3000/", &["*".into()]).is_err());
        assert!(validate_network_url("http://192.168.1.1/router", &["*".into()]).is_err());
        assert!(validate_network_url("http://10.0.0.5/", &["*".into()]).is_err());

        // Subdomain trickery: evil.com/api.github.com is blocked
        assert!(validate_network_url("https://evil.com/api.github.com", &["api.github.com".into()]).is_err());
    }

    #[test]
    fn test_unregistered_plugin_rejected() {
        let host = PluginHostState::new();
        assert!(host
            .check_permission(
                "nonexistent",
                &ApiRequest::VaultRead {
                    path: "note.md".into()
                }
            )
            .is_err());
    }

    #[test]
    fn test_set_view_tree_permission_allowed() {
        let host = PluginHostState::new();
        let (tx, _rx) = unbounded();
        let manifest = PluginManifest {
            id: "ui-plugin".into(),
            name: "UI Plugin".into(),
            version: "1.0.0".into(),
            min_app_version: None,
            description: "".into(),
            author: "".into(),
            permissions: PluginPermissions::default(),
        };
        host.register_plugin(manifest, tx);

        let req = ApiRequest::SetViewTree {
            view_id: 1,
            tree: crate::plugins::ui::UiNode::Separator,
        };
        assert!(host.check_permission("ui-plugin", &req).is_ok());
    }
