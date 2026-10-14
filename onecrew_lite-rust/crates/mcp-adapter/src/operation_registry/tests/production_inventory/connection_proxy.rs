use super::*;

/// The proxy object of the three connection start inputs is a frozen public
/// contract: its inlined schema, the transient `/proxy/password` secret path,
/// and which values pass schema validation and the registered typed decode.
#[test]
fn connection_start_inputs_keep_the_frozen_proxy_contract() {
    let directory = tempfile::tempdir().unwrap();
    let platform =
        std::sync::Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let registry = crate::operation_registry::build_registry(
        production_owners(platform),
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .unwrap();
    let frozen_proxy_schema = serde_json::json!({
        "description": "Residential (家宽) proxy a profile routes all of its outbound platform\ntraffic through. Persisted (with credentials) inside the encrypted profile\nsecret; `None` there means a direct connection (this machine's IP).",
        "properties": {
            "host": {"type": "string"},
            "kind": {"enum": ["http", "https", "socks5"], "type": "string"},
            "password": {"type": ["string", "null"]},
            "port": {"format": "uint16", "maximum": 65535, "minimum": 0, "type": "integer"},
            "username": {"type": ["string", "null"]}
        },
        "required": ["kind", "host", "port"],
        "type": "object"
    });
    let accepted = [
        serde_json::json!({"kind": "http", "host": "proxy.example", "port": 8080, "username": "proxy-user", "password": "proxy-password"}),
        serde_json::json!({"kind": "socks5", "host": "socks.example", "port": 1080, "username": null, "password": null}),
        serde_json::json!({"kind": "https", "host": "secure.example", "port": 443}),
        serde_json::json!({"kind": "http", "host": "proxy.example", "port": 3128, "password": "lonely-password"}),
        serde_json::json!({"kind": "socks5", "host": "", "port": 0}),
        serde_json::json!({"kind": "socks5", "host": "socks.example", "port": 65535, "label": "ignored"}),
    ];
    let rejected = [
        serde_json::json!({"kind": "HTTP", "host": "proxy.example", "port": 8080}),
        serde_json::json!({"kind": "socks4", "host": "proxy.example", "port": 1080}),
        serde_json::json!({"host": "proxy.example", "port": 8080}),
        serde_json::json!({"kind": "http", "port": 8080}),
        serde_json::json!({"kind": "http", "host": "proxy.example"}),
        serde_json::json!({"kind": "http", "host": "proxy.example", "port": 65536}),
        serde_json::json!({"kind": "http", "host": "proxy.example", "port": -1}),
        serde_json::json!({"kind": "http", "host": "proxy.example", "port": 8080.5}),
        serde_json::json!({"kind": "http", "host": "proxy.example", "port": "8080"}),
        serde_json::json!({"kind": "http", "host": "proxy.example", "port": 8080, "username": "proxy-user", "password": 42}),
        serde_json::json!("http://proxy-user:proxy-password@proxy.example:8080"),
        serde_json::Value::Null,
    ];
    let inputs = [
        (
            "x.connection.browser.start",
            serde_json::json!({"platform": "x", "profile_name": "operator", "x_chat_preference": "disabled", "idempotency_key": "proxy-contract"}),
        ),
        (
            "instagram.connection.browser.start",
            serde_json::json!({"platform": "instagram", "profile_name": "operator", "x_chat_preference": "disabled", "idempotency_key": "proxy-contract"}),
        ),
        (
            "telegram.connection.login.start",
            serde_json::json!({"api_id": 12345, "api_hash": "api-hash", "idempotency_key": "proxy-contract"}),
        ),
    ];
    for (id, base) in inputs {
        let operation = registry_operation(&registry, id);
        let document = operation.input.document();
        assert_eq!(document["properties"]["proxy"], frozen_proxy_schema, "{id}");
        assert!(
            document["required"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("proxy")),
            "{id}"
        );
        assert!(
            operation
                .sensitive_input_paths
                .iter()
                .any(|path| path == "/proxy/password"),
            "{id}"
        );
        assert!(
            operation
                .input
                .validates_instance_pointer("/proxy/password"),
            "{id}"
        );
        for (proxy, expected) in accepted
            .iter()
            .map(|proxy| (proxy, true))
            .chain(rejected.iter().map(|proxy| (proxy, false)))
        {
            let mut arguments = base.clone();
            arguments["proxy"] = proxy.clone();
            let schema_accepts = operation.input.validate(&arguments);
            let decodes = if id == "telegram.connection.login.start" {
                serde_json::from_value::<onecrew_local_runtime::TelegramAccountConnectionStartInput>(
                    arguments,
                )
                .is_ok()
            } else {
                serde_json::from_value::<onecrew_local_runtime::BrowserAccountConnectionStartInput>(
                    arguments,
                )
                .is_ok()
            };
            assert_eq!(
                (schema_accepts, decodes),
                (expected, expected),
                "{id}: {proxy}"
            );
        }
    }
}
