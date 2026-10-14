use super::*;

#[test]
fn archived_creator_cannot_prepare_email() {
    let (_directory, runtime, gateway) = runtime(TestGateway::default());
    let (creator_id, contact_route_id) = creator_email_route(&runtime);
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    runtime.archive_creator(&creator_id).unwrap();

    let error = runtime
        .prepare_email(PrepareEmailInput {
            creator_id,
            contact_route_id,
            mailbox_ref_id: mailbox.mailbox_ref_id,
            reply_to_email_id: None,
        })
        .unwrap_err();
    assert!(error.contains("archived Creator"), "{error}");
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn template_render_uses_confirmed_handles_and_rejects_html_variables() {
    let (_directory, runtime, _gateway) = runtime(TestGateway::default());
    let (creator_id, _) = creator_email_route(&runtime);
    let mut rejected_account = None;
    for (platform, external_id, handle, link_state) in [
        (Platform::X, "creator-x", "@creator_x", LinkState::Confirmed),
        (
            Platform::Instagram,
            "creator-ig",
            "@creator_ig",
            LinkState::Rejected,
        ),
    ] {
        let account = runtime
            .repository
            .upsert_platform_account(UpsertPlatformAccountInput {
                platform,
                target_kind: TargetKind::User,
                external_id: external_id.into(),
            })
            .unwrap();
        runtime
            .repository
            .link_creator_account(&creator_id, &account.id, link_state, Confidence::Exact)
            .unwrap();
        if link_state == LinkState::Rejected {
            rejected_account = Some(account.id.clone());
        }
        let source = runtime
            .repository
            .create_observation_source(ObservationSourceInput {
                kind: SourceKind::InitialPlatformAccount,
                platform: Some(platform),
                source_url: None,
                external_ref: Some(external_id.into()),
                account_ref_id: None,
                metadata: serde_json::json!({}),
            })
            .unwrap();
        runtime
            .repository
            .append_platform_account_observation(PlatformAccountObservationInput {
                platform_account_id: account.id,
                source_id: source.id,
                handle: Some(handle.into()),
                display_name: None,
                profile_url: None,
                public_data: serde_json::json!({}),
            })
            .unwrap();
    }

    let mut template_changes = runtime.subscribe_changes();
    let template = runtime
        .save_email_template(SaveEmailTemplateInput {
            id: None,
            name: "Ambiguous handle".into(),
            subject: "Hello {{name}} {{handle}}".into(),
            text: "Offer for {{handle}}: {{offer}}".into(),
            html: Some("<p>Static safe HTML</p>".into()),
            expected_revision: None,
        })
        .unwrap();
    assert_eq!(
        template_changes.try_recv().unwrap().scopes,
        [CreatorChangeScope::EmailTemplates]
    );
    assert_eq!(template.revision, 1);
    assert_eq!(
        template.variables,
        vec![
            "handle".to_string(),
            "name".to_string(),
            "offer".to_string()
        ]
    );

    let rendered = runtime
        .render_email_template(RenderEmailTemplateInput {
            template_id: template.id.clone(),
            creator_id: Some(creator_id.clone()),
            variables: std::collections::BTreeMap::from([("offer".into(), "<b>A&B</b>".into())]),
        })
        .unwrap();
    assert!(rendered.unresolved.is_empty());
    assert!(rendered.subject.ends_with("@creator_x"));
    assert_eq!(rendered.html.as_deref(), Some("<p>Static safe HTML</p>"));

    runtime
        .repository
        .link_creator_account(
            &creator_id,
            &rejected_account.unwrap(),
            LinkState::Confirmed,
            Confidence::Exact,
        )
        .unwrap();
    let ambiguous = runtime
        .render_email_template(RenderEmailTemplateInput {
            template_id: template.id.clone(),
            creator_id: Some(creator_id.clone()),
            variables: std::collections::BTreeMap::from([("offer".into(), "Offer".into())]),
        })
        .unwrap();
    assert_eq!(ambiguous.unresolved, vec!["handle"]);
    assert!(ambiguous.subject.contains("{{handle}}"));

    let explicit = runtime
        .render_email_template(RenderEmailTemplateInput {
            template_id: template.id.clone(),
            creator_id: Some(creator_id),
            variables: std::collections::BTreeMap::from([
                ("handle".into(), "@chosen".into()),
                ("offer".into(), "Offer".into()),
            ]),
        })
        .unwrap();
    assert!(explicit.unresolved.is_empty());
    assert!(explicit.subject.ends_with("@chosen"));

    let unsafe_html = runtime
        .save_email_template(SaveEmailTemplateInput {
            id: None,
            name: "Unsafe HTML substitution".into(),
            subject: "Hello".into(),
            text: "Body".into(),
            html: Some("<a href=\"{{url}}\">Open</a>".into()),
            expected_revision: None,
        })
        .unwrap_err();
    assert!(unsafe_html.contains("not allowed in html"), "{unsafe_html}");

    let updated = runtime
        .save_email_template(SaveEmailTemplateInput {
            id: Some(template.id.clone()),
            name: template.name,
            subject: template.subject,
            text: template.text,
            html: template.html,
            expected_revision: Some(template.revision),
        })
        .unwrap();
    assert_eq!(updated.revision, 2);
    let stale = runtime
        .save_email_template(SaveEmailTemplateInput {
            id: Some(template.id),
            name: "Stale".into(),
            subject: "Stale".into(),
            text: "Stale".into(),
            html: None,
            expected_revision: Some(1),
        })
        .unwrap_err();
    assert!(stale.contains("revision conflict"), "{stale}");
}
