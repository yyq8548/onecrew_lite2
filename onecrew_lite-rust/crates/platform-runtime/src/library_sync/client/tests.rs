use super::*;
use serde_json::json;

#[test]
fn a_healthy_old_library_cannot_claim_sync_compatibility() {
    let mut health: Health = serde_json::from_value(json!({
        "ok": true, "service": "onecrew_lite", "protocol_version": 1,
        "engine": "teable-v2-pglite"
    }))
    .unwrap();
    assert_eq!(
        require_sync_capabilities(&health).unwrap_err(),
        "library_sync_upgrade_required"
    );
    health.capabilities.push("discovery-contacts-v2".into());
    assert!(require_sync_capabilities(&health).is_err());
    health.capabilities.push("recommendation-capture-v1".into());
    assert!(require_sync_capabilities(&health).is_ok());
}

#[tokio::test]
#[ignore = "requires the isolated Library receiver; synthetic no-contact fixture, no provider reads"]
async fn isolated_library_receiver_excludes_scored_sources_without_contacts() {
    assert_eq!(
        std::env::var("ONECREW_TEST_LIBRARY_NO_CONTACT").as_deref(),
        Ok("true")
    );
    let dir = tempfile::tempdir().unwrap();
    let platform = crate::LocalPlatform::open(dir.path()).unwrap();
    crate::discovery_identity_test_support::authenticate(&platform, "rust-library-acceptance");
    let mut publication = identity_publication_fixture();
    let row = &mut publication.material.sources.rows[0];
    row.verdict = "pending".into();
    row.reasons = vec!["contact_unavailable".into()];
    row.contacts.clear();
    row.match_rating = Some(serde_json::from_value(json!({
        "level":"good", "reason":"Synthetic scored source; contact discovery found no usable receipt."
    })).unwrap());
    publication.material.identity.groups[0].members[0].sources[0] = row.clone();
    platform.assert_test_library_publication(&publication).await;
    println!(
        "Synthetic no-contact source: owner score retained, Library rating hidden, idempotent sync PASS"
    );
}

#[tokio::test]
#[ignore = "requires the isolated Library discovery-rust-sync.ts receiver"]
async fn isolated_library_receiver_preserves_score_and_manual_override() {
    use onecrew_creator_runtime::DiscoveryLibraryTableName;
    let base = std::env::var("ONECREW_TEST_LIBRARY_URL").unwrap();
    let url = url::Url::parse(&base).unwrap();
    assert_eq!(url.scheme(), "http");
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    let account = "rust-library-acceptance";
    let client = LibraryClient {
        http: Client::builder()
            .redirect(Policy::none())
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap(),
        base,
        account_id: account.into(),
        control_token: std::env::var("ONECREW_TEST_LIBRARY_TOKEN").unwrap(),
    };
    let dir = tempfile::tempdir().unwrap();
    let platform = crate::LocalPlatform::open(dir.path()).unwrap();
    crate::discovery_identity_test_support::authenticate(&platform, account);
    let mut publication = identity_publication_fixture();
    let web_origin = std::env::var("ONECREW_TEST_WEB_ORIGIN").ok();
    if let Some(origin) = web_origin.as_deref() {
        assert!(matches!(origin, "google" | "yahoo"));
        publication.material.sources.rows[0].hit.platform = origin.into();
        publication.material.sources.rows[0].native_platform = Some("youtube".into());
        publication.material.identity.groups[0].members[0].sources[0] =
            publication.material.sources.rows[0].clone();
    }
    let contacts = vec![onecrew_creator_runtime::DiscoveryContact {
        kind: onecrew_creator_runtime::DiscoveryContactKind::Email,
        original_value: "business@example.test".into(),
        normalized_value: "business@example.test".into(),
        source_url: "https://www.youtube.com/watch?v=video".into(),
        source_excerpt: "Business: business@example.test".into(),
        captured_at: 1,
        native_author_id: "channel".into(),
        attribution: onecrew_creator_runtime::DiscoveryContactAttribution::AuthorPublished,
    }];
    publication.material.sources.rows[0].contacts = contacts.clone();
    publication.material.identity.groups[0].members[0].sources[0].contacts = contacts.clone();
    let rating =
        json!({"level":"good", "reason":"粉丝评论语言符合：英文；博主本人内容语言未核实。"});
    publication.material.sources.rows[0].match_rating =
        Some(serde_json::from_value(rating.clone()).unwrap());
    let name = DiscoveryLibraryTableName {
        name: "语言验收_1005_1".into(),
        previous_auto_name: "OneCrew Sources campaign".into(),
    };
    let first = platform
        .sync_library_snapshot(&client, &publication.material.sources, account, "", &name)
        .await
        .unwrap();
    assert_eq!(first.inserted_rows, 1);
    client
        .publish_identity(&first.table_id, &publication)
        .await
        .unwrap();
    let table = client.campaign_table("campaign").await.unwrap().unwrap();
    let score = table
        .columns
        .iter()
        .find(|column| column.name == "Match evaluation")
        .unwrap()
        .id
        .clone();
    let view_path = format!("/api/v1/dynamic-tables/{}/creators", first.table_id);
    let view: Value = client.request(Method::GET, &view_path, None).await.unwrap();
    assert_eq!(view["state"], "current");
    assert_eq!(view["sources"][0]["row"]["values"][&score], json!(rating));
    assert_eq!(view["sources"][0]["contacts"], json!(contacts));
    if let Some(web_origin) = web_origin {
        let platform_column = table
            .columns
            .iter()
            .find(|column| column.name == "Platform")
            .unwrap();
        assert_eq!(
            view["sources"][0]["row"]["values"][&platform_column.id],
            "youtube"
        );
        assert_eq!(
            view["projection"]["groups"][0]["members"][0]["platform"],
            "youtube"
        );
        assert_eq!(
            publication.material.sources.rows[0].hit.platform,
            web_origin
        );
        let export = client
            .http
            .get(format!(
                "{}/api/v1/dynamic-tables/{}/export",
                client.base, first.table_id
            ))
            .header("X-OneCrew-Account-Id", account)
            .send()
            .await
            .unwrap();
        assert_eq!(export.status(), reqwest::StatusCode::OK);
        assert_eq!(
            export.headers()["content-type"],
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        );
        let bytes = export.bytes().await.unwrap();
        assert!(bytes.len() < 1024 * 1024);
        std::fs::write(std::env::var("ONECREW_TEST_LIBRARY_EXPORT").unwrap(), bytes).unwrap();
    }
    let row_id = view["sources"][0]["row"]["id"].clone();
    let retry = platform
        .sync_library_snapshot(&client, &publication.material.sources, account, "", &name)
        .await
        .unwrap();
    assert_eq!(retry.inserted_rows, 0);
    publication.material.sources.source_revision += 1;
    publication.material.sources.snapshot_hash = "e".repeat(64);
    publication.material.sources.rows[0].match_rating = None;
    publication.material.sources.rows[0].reasons =
        vec!["manual_approved".into(), "language_unverified".into()];
    publication.material.sources.rows[0].evidence_version = "f".repeat(64);
    publication.identity_revision += 1;
    publication.material.identity.snapshot_hash = "1".repeat(64);
    platform
        .sync_library_snapshot(&client, &publication.material.sources, account, "", &name)
        .await
        .unwrap();
    client
        .publish_identity(&first.table_id, &publication)
        .await
        .unwrap();
    let view: Value = client.request(Method::GET, &view_path, None).await.unwrap();
    assert_eq!(view["state"], "current");
    assert_eq!(view["sources"].as_array().unwrap().len(), 1);
    assert_eq!(view["sources"][0]["verdict"], "qualified");
    assert_eq!(
        view["sources"][0]["reasons"],
        json!(["manual_approved", "language_unverified"])
    );
    assert_eq!(view["sources"][0]["row"]["id"], row_id);
    assert!(view["sources"][0]["row"]["values"][&score].is_null());
    println!(
        "Rust production snapshot sync and identity publication -> Library creator-view readback PASS"
    );
}

fn identity_publication_fixture() -> onecrew_creator_runtime::DiscoveryLibraryPublication {
    use onecrew_creator_runtime::*;
    let row = DiscoveryLibraryRow {
        followers: None,
        native_platform: None,
        contacts: vec![],
        hit: DiscoverySearchHit {
            id: "hit".into(),
            campaign_id: "campaign".into(),
            platform: "youtube".into(),
            profile_ref: "profile".into(),
            search_keyword: "shoes".into(),
            source_url: "https://www.youtube.com/watch?v=video".into(),
            title: "Shoes review".into(),
            author_label: "Creator".into(),
            captured_at: 1,
            author_id: Some("channel".into()),
            author_url: Some("https://www.youtube.com/channel/channel".into()),
            author_observed_at: Some(1),
        },
        candidate_id: Some("candidate".into()),
        native_author_id: Some("channel".into()),
        verdict: "qualified".into(),
        reasons: vec![],
        evidence_version: "c".repeat(64),
        match_rating: None,
    };
    DiscoveryLibraryPublication {
        identity_revision: 2,
        material: DiscoveryLibraryMaterial {
            sources: DiscoveryLibrarySnapshot {
                campaign_id: "campaign".into(),
                criteria_revision: 1,
                source_revision: 3,
                snapshot_hash: "a".repeat(64),
                rows: vec![row.clone()],
            },
            identity: DiscoveryIdentityGroupsSnapshot {
                campaign_id: "campaign".into(),
                criteria_revision: 1,
                snapshot_hash: "b".repeat(64),
                groups: vec![DiscoveryIdentityGroup {
                    group_id: "d".repeat(64),
                    has_same_author_review: false,
                    qualified_member_count: 1,
                    decision_ids: vec![],
                    members: vec![DiscoveryIdentityGroupMember {
                        candidate_id: "candidate".into(),
                        platform: "youtube".into(),
                        native_author_id: "channel".into(),
                        display_name: "Creator".into(),
                        profile_url: "https://www.youtube.com/channel/channel".into(),
                        verdict: "qualified".into(),
                        reasons: vec![],
                        sources: vec![row],
                    }],
                }],
                reviews: vec![],
                counts: DiscoveryIdentityGroupCounts {
                    native_accounts: 1,
                    qualified_native_accounts: 1,
                    identity_groups: 1,
                    qualified_identity_groups: 1,
                    qualified_native_only_groups: 1,
                    unbound_source_hits: 0,
                    stale_identity_reviews: 0,
                },
            },
        },
    }
}

#[tokio::test]
async fn identity_publication_reads_back_once_and_reconciles_lost_receipt_without_resending() {
    let publication = identity_publication_fixture();
    let body = super::super::identity::payload("table", &publication).unwrap();
    assert_eq!(body["source_revision"], 3);
    assert_eq!(body["identity_revision"], 2);
    assert_eq!(
        body["groups"][0]["members"][0]["source_hit_ids"],
        json!(["hit"])
    );
    assert!(body["groups"][0].get("qualified_member_count").is_none());
    let receipt = json!({"state":"current","projection":body});
    for post in [Some(receipt.clone()), None] {
        let (client, server) = scripted_server(vec![
            ("POST /_local/discovery/identity ", post),
            (
                "GET /_local/discovery/identity?table_id=table&campaign_id=campaign ",
                Some(receipt.clone()),
            ),
        ])
        .await;
        client
            .publish_identity("table", &publication)
            .await
            .unwrap();
        server.await.unwrap();
    }
    for readback in [
        json!({"state":"sources_changed","projection":null}),
        json!({"state":"current","projection":{}}),
    ] {
        let (client, server) = scripted_server(vec![
            ("POST /_local/discovery/identity ", None),
            (
                "GET /_local/discovery/identity?table_id=table&campaign_id=campaign ",
                Some(readback),
            ),
        ])
        .await;
        assert_eq!(
            client
                .publish_identity("table", &publication)
                .await
                .unwrap_err(),
            "library_readback_incomplete"
        );
        server.await.unwrap();
    }
}

#[test]
fn identity_publication_rejects_unreserved_material_and_wrong_campaign() {
    let mut publication = identity_publication_fixture();
    publication.material.sources.source_revision = 0;
    assert!(super::super::identity::payload("table", &publication).is_err());
    publication.material.sources.source_revision = 3;
    publication.material.identity.campaign_id = "other".into();
    assert!(super::super::identity::payload("table", &publication).is_err());
}

async fn scripted_server(
    steps: Vec<(&'static str, Option<Value>)>,
) -> (LibraryClient, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for (expected, response) in steps {
            let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
                .await
                .unwrap()
                .unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut chunk = [0; 4096];
                let count = socket.read(&mut chunk).await.unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
                assert!(bytes.len() < 1024 * 1024);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                    let length: usize = headers
                        .lines()
                        .find_map(|line| {
                            line.strip_prefix("content-length: ")
                                .map(|n| n.parse().unwrap())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let request = String::from_utf8_lossy(&bytes);
            assert!(request.starts_with(expected), "unexpected request target");
            assert!(
                request
                    .to_lowercase()
                    .contains("x-onecrew-account-id: access-user")
            );
            assert!(request.to_lowercase().contains("authorization: bearer "));
            if let Some(body) = response {
                let body = body.to_string();
                socket.write_all(format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()
                ).as_bytes()).await.unwrap();
            }
        }
    });
    (
        LibraryClient {
            http: Client::builder()
                .no_proxy()
                .redirect(Policy::none())
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap(),
            base: format!("http://{address}"),
            account_id: "access-user".into(),
            control_token: "a".repeat(64),
        },
        server,
    )
}

fn state(rows: Vec<Value>, more: bool) -> Value {
    json!({"table_id":"table","campaign_id":"campaign","bound":true,"rows":rows,"has_more":more})
}

#[tokio::test]
async fn discovery_rejects_replaced_instance_and_wrong_account() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for wrong_instance in [true, false] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let instance = uuid::Uuid::new_v4().to_string();
        let server_instance = if wrong_instance {
            uuid::Uuid::new_v4().to_string()
        } else {
            instance.clone()
        };
        let server = tokio::spawn(async move {
            let mut responses = vec![
                (
                    "/healthz",
                    json!({"ok":true,"service":"onecrew_lite","protocol_version":1,"engine":"teable-v2-pglite","capabilities":["discovery-contacts-v2","recommendation-capture-v1"]}),
                ),
                ("/_local/status", json!({"instance_id":server_instance})),
            ];
            if !wrong_instance {
                responses.push((
                    "/api/v1/access/status",
                    json!({"state":"signed_in","user":{"id":"another-user"}}),
                ));
            }
            for (path, body) in responses {
                let (mut socket, _) =
                    tokio::time::timeout(Duration::from_secs(5), listener.accept())
                        .await
                        .unwrap()
                        .unwrap();
                let mut bytes = [0; 8192];
                let mut received = 0;
                while !bytes[..received].ends_with(b"\r\n\r\n") {
                    let n = socket.read(&mut bytes[received..]).await.unwrap();
                    assert!(n > 0);
                    received += n;
                }
                assert!(
                    String::from_utf8_lossy(&bytes[..received])
                        .starts_with(&format!("GET {path} "))
                );
                let body = body.to_string();
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
        });
        let dir = tempfile::tempdir().unwrap();
        let descriptor = RuntimeDescriptor {
            service: "onecrew_lite".into(),
            protocol_version: 1,
            pid: 123,
            port: address.port(),
            url: format!("http://{address}"),
            instance_id: instance,
            control_token: "a".repeat(64),
            data_dir: dir.path().into(),
        };
        let error = connect_descriptor(descriptor, "access-user")
            .await
            .err()
            .unwrap();
        assert_eq!(
            error,
            if wrong_instance {
                "library_instance_changed"
            } else {
                "library_account_mismatch"
            }
        );
        server.await.unwrap();
    }
}
fn saved_row(hit: &str, row: &str) -> Value {
    json!({"row_id":row,"search_hit_id":hit,"candidate_id":null,"native_author_id":null,
        "verdict":"unverified","reasons":["unbound"],"evidence_version":"a".repeat(64),
        "criteria_revision":1,"source_revision":1})
}

#[tokio::test]
async fn state_paginates_and_rejects_duplicate_or_foreign_rows() {
    let first = "GET /_local/discovery/state?table_id=table&campaign_id=campaign&limit=100 ";
    let next = "GET /_local/discovery/state?table_id=table&campaign_id=campaign&limit=100&after_hit_id=hit-a ";
    let (client, server) = scripted_server(vec![
        (first, Some(state(vec![saved_row("hit-a", "row-a")], true))),
        (next, Some(state(vec![saved_row("hit-b", "row-b")], false))),
    ])
    .await;
    assert_eq!(client.state("table", "campaign").await.unwrap().len(), 2);
    server.await.unwrap();
    for second in [saved_row("hit-a", "row-b"), saved_row("hit-b", "row-a")] {
        let (client, server) = scripted_server(vec![
            (first, Some(state(vec![saved_row("hit-a", "row-a")], true))),
            (next, Some(state(vec![second], false))),
        ])
        .await;
        assert_eq!(
            client.state("table", "campaign").await.unwrap_err(),
            "library_row_identity_changed"
        );
        server.await.unwrap();
    }
    let mut foreign = state(vec![], false);
    foreign["campaign_id"] = json!("another-campaign");
    let (client, server) = scripted_server(vec![(first, Some(foreign))]).await;
    assert_eq!(
        client.state("table", "campaign").await.unwrap_err(),
        "library_invalid_response"
    );
    server.await.unwrap();
}

fn authenticated_platform(path: &Path) -> crate::LocalPlatform {
    use onecrew_local_domain::*;
    let platform = crate::LocalPlatform::open(path).unwrap();
    let metadata = CreatorSyncCredentialMetadata {
        installation_id: uuid::Uuid::new_v4(),
        credential_id: uuid::Uuid::new_v4(),
        access_user_id: "access-user".into(),
        access_device_id: "fixture-device".into(),
        source_instance_id: uuid::Uuid::new_v4(),
        access_pairing_id: "fixture-pairing".into(),
        client_platform: CreatorSyncClientPlatform::Macos,
        app_version: "0.6.16".into(),
        status: CreatorSyncCredentialStatus::Active,
        revision: 1,
        created_at: 100,
        updated_at: 101,
        last_seen_at: Some(101),
        revoked_at: None,
        protocol: CreatorSyncProtocol::ReacherCreatorSync,
        protocol_version: 2,
        capabilities: [CreatorSyncCapability::WriteChunkedQuarantine],
        accepted_bundle: CreatorSyncAcceptedBundle {
            format: "agent-platform.creator-snapshot".into(),
            format_version: 1,
            profile: CreatorSyncBundleProfile::PortablePublic,
            max_snapshot_bytes: 1024,
            max_chunk_bytes: 512,
            max_chunks: 2,
            max_namespace_staged_bytes: 2048,
            max_access_user_staged_bytes: 4096,
        },
    };
    platform
        .vault
        .save_creator_sync_credential(
            &metadata,
            &ServiceCredentialSecret::new(format!("{}_{}", metadata.credential_id, "A".repeat(43))),
        )
        .unwrap();
    platform
}

#[tokio::test]
async fn sync_reconciles_lost_response_and_retry_does_not_write() {
    use onecrew_creator_runtime::{
        DiscoveryLibraryRow, DiscoveryLibrarySnapshot, DiscoverySearchHit,
    };
    let dir = tempfile::tempdir().unwrap();
    let platform = authenticated_platform(dir.path());
    let snapshot = DiscoveryLibrarySnapshot {
        campaign_id: "campaign".into(),
        criteria_revision: 1,
        source_revision: 1,
        snapshot_hash: "b".repeat(64),
        rows: vec![DiscoveryLibraryRow {
            followers: None,
            native_platform: None,
            contacts: vec![],
            hit: DiscoverySearchHit {
                id: "hit-a".into(),
                campaign_id: "campaign".into(),
                platform: "twitter".into(),
                profile_ref: "fixture".into(),
                search_keyword: "AI tools".into(),
                source_url: "https://x.com/fixture/status/1".into(),
                title: "Demo".into(),
                author_label: "Fixture".into(),
                captured_at: 1,
                author_id: None,
                author_url: None,
                author_observed_at: None,
            },
            candidate_id: None,
            native_author_id: None,
            verdict: "unverified".into(),
            reasons: vec!["unbound".into()],
            evidence_version: "a".repeat(64),
            match_rating: None,
        }],
    };
    let table = json!({"table":{"id":"table","columns":super::super::COLUMNS.iter().enumerate().map(|(i,(name,ty))|
        json!({"id":format!("f{i}"),"name":name,"data_type":ty})).collect::<Vec<_>>()}});
    let lookup = "GET /_local/discovery/campaign?campaign_id=campaign ";
    let read = "GET /_local/discovery/state?table_id=table&campaign_id=campaign&limit=100 ";
    let saved = state(vec![saved_row("hit-a", "row-a")], false);
    // The server consumes the full write, then closes before returning its result.
    let (client, server) = scripted_server(vec![
        (lookup, Some(table.clone())),
        (read, Some(state(vec![], false))),
        ("POST /_local/discovery/ingest ", None),
        (read, Some(saved.clone())),
        (read, Some(saved.clone())),
        (lookup, Some(table.clone())),
        (read, Some(saved.clone())),
        (read, Some(saved)),
    ])
    .await;
    assert_eq!(
        platform
            .sync_library_snapshot(
                &client,
                &snapshot,
                "access-user",
                "",
                &onecrew_creator_runtime::DiscoveryLibraryTableName {
                    name: "虚拟货币_1002_1".into(),
                    previous_auto_name: "虚拟货币_1002_1".into()
                }
            )
            .await
            .unwrap()
            .inserted_rows,
        1
    );
    assert_eq!(
        platform
            .sync_library_snapshot(
                &client,
                &snapshot,
                "access-user",
                "",
                &onecrew_creator_runtime::DiscoveryLibraryTableName {
                    name: "虚拟货币_1002_1".into(),
                    previous_auto_name: "虚拟货币_1002_1".into()
                }
            )
            .await
            .unwrap()
            .inserted_rows,
        0
    );
    server.await.unwrap();
    // A missing commit is not silently treated as success or automatically resent.
    let (client, server) = scripted_server(vec![
        (lookup, Some(table)),
        (read, Some(state(vec![], false))),
        ("POST /_local/discovery/ingest ", None),
        (read, Some(state(vec![], false))),
    ])
    .await;
    assert!(
        platform
            .sync_library_snapshot(
                &client,
                &snapshot,
                "access-user",
                "",
                &onecrew_creator_runtime::DiscoveryLibraryTableName {
                    name: "虚拟货币_1002_1".into(),
                    previous_auto_name: "虚拟货币_1002_1".into()
                }
            )
            .await
            .is_err()
    );
    server.await.unwrap();
}
#[test]
fn descriptor_rejects_remote_targets_invalid_identity_and_oversized_files() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("server.json");
    let valid = serde_json::json!({
        "service":"onecrew_lite", "protocol_version":1, "pid":123, "port":8091,
        "url":"http://127.0.0.1:8091", "instance_id":uuid::Uuid::new_v4().to_string(),
        "control_token":"a".repeat(64), "data_dir":dir.path(),
    });
    std::fs::write(&file, serde_json::to_vec(&valid).unwrap()).unwrap();
    assert!(read_descriptor(&file).is_ok());
    for (field, value) in [
        ("url", serde_json::json!("https://example.test")),
        ("url", serde_json::json!("http://127.0.0.1:8092")),
        ("instance_id", serde_json::json!("invalid")),
        ("control_token", serde_json::json!("not-a-token")),
        ("data_dir", serde_json::json!("relative/path")),
        ("pid", serde_json::json!(0)),
        ("protocol_version", serde_json::json!(2)),
    ] {
        let mut invalid = valid.clone();
        invalid[field] = value;
        std::fs::write(&file, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(matches!(read_descriptor(&file),Err(error) if error=="library_runtime_invalid"));
    }
    std::fs::write(&file, vec![b' '; 16385]).unwrap();
    assert!(matches!(read_descriptor(&file),Err(error) if error=="library_runtime_invalid"));
}

#[tokio::test]
async fn bearer_is_sent_only_to_private_local_routes() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for path in ["/healthz", "/_local/discovery/campaign"] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 8192];
            let mut received = 0;
            while !request[..received].ends_with(b"\r\n\r\n") {
                let count = socket.read(&mut request[received..]).await.unwrap();
                assert!(count > 0);
                received += count;
            }
            let text = String::from_utf8_lossy(&request[..received]).to_lowercase();
            assert!(text.contains("x-onecrew-account-id: fixture-user"));
            assert_eq!(
                text.contains("authorization: bearer "),
                path.starts_with("/_local/")
            );
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
                .await
                .unwrap();
        });
        let client = LibraryClient {
            http: Client::builder()
                .no_proxy()
                .redirect(Policy::none())
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap(),
            base: format!("http://{address}"),
            account_id: "fixture-user".into(),
            control_token: "a".repeat(64),
        };
        client
            .request::<Value>(Method::GET, path, None)
            .await
            .unwrap();
        server.await.unwrap();
    }
}
#[tokio::test]
async fn http_failures_distinguish_unavailability_reads_and_writes_without_body_leaks() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for (status, method, expected) in [
        (503, Method::GET, "library_service_unavailable"),
        (500, Method::GET, "library_read_failed"),
        (500, Method::POST, "library_write_failed"),
        (409, Method::GET, "library_account_changed"),
        (401, Method::GET, "library_account_changed"),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 2048];
            let mut received = 0;
            while !request[..received].ends_with(b"\r\n\r\n") {
                let count = socket.read(&mut request[received..]).await.unwrap();
                assert!(count > 0, "complete bounded request headers required");
                received += count;
            }
            let body = r#"{"error":{"code":"access.unavailable","message":"synthetic-secret-must-not-leak"}}"#;
            let reply = format!(
                "HTTP/1.1 {status} Error\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(reply.as_bytes()).await.unwrap();
        });
        let client = LibraryClient {
            http: Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap(),
            base: format!("http://{address}"),
            account_id: "synthetic-user".into(),
            control_token: "a".repeat(64),
        };
        let result = client
            .request::<Value>(method, "/api/v1/dynamic-tables", None)
            .await;
        server.await.unwrap();
        assert_eq!(result.unwrap_err(), expected);
    }
}
