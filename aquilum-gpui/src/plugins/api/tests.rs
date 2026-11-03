    use super::*;

    #[test]
    fn test_api_call_serialization_roundtrip() {
        let call = ApiCall {
            plugin_id: "test-plugin".into(),
            request_id: 101,
            request: ApiRequest::VaultRead {
                path: "Notes/Todo.md".into(),
            },
        };

        let json = serde_json::to_string(&call).unwrap();
        assert!(json.contains("\"kind\":\"vault_read\""));
        assert!(json.contains("\"request_id\":101"));

        let deserialized: ApiCall = serde_json::from_str(&json).unwrap();
        assert_eq!(call, deserialized);
    }

    #[test]
    fn test_host_event_response_roundtrip() {
        let event = HostEvent::response_ok(
            101,
            serde_json::json!({
                "content": "# Hello World"
            }),
        );

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"kind\":\"response\""));
        assert!(json.contains("\"ok\":true"));

        let deserialized: HostEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_file_open_event_none_path() {
        let event = HostEvent::FileOpen { path: None };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: HostEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }
