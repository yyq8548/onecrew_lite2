//! Adapter for the Library's public five-column recommendation workbook.
use super::*;
use quick_xml::{Reader as XmlReader, events::Event};

fn attrs(e: &quick_xml::events::BytesStart<'_>) -> Result<BTreeMap<String, String>, String> {
    e.attributes()
        .map(|a| {
            let a = a.map_err(|e| e.to_string())?;
            Ok((
                String::from_utf8(a.key.as_ref().to_vec()).map_err(|e| e.to_string())?,
                quick_xml::escape::unescape(
                    std::str::from_utf8(&a.value).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?
                .into_owned(),
            ))
        })
        .collect()
}

fn comments(xml: &[u8]) -> Result<BTreeMap<String, String>, String> {
    let mut reader = XmlReader::from_reader(xml);
    let mut result = BTreeMap::new();
    let mut current = None;
    let mut in_text = false;
    loop {
        match reader.read_event().map_err(|e| e.to_string())? {
            Event::Start(e) if e.local_name().as_ref() == b"comment" => {
                current = attrs(&e)?.remove("ref");
            }
            Event::Start(e) if e.local_name().as_ref() == b"t" => in_text = true,
            Event::Text(e) if in_text => {
                if let Some(cell) = &current {
                    result
                        .entry(cell.clone())
                        .or_insert_with(String::new)
                        .push_str(
                            &quick_xml::escape::unescape(&e.decode().map_err(|e| e.to_string())?)
                                .map_err(|e| e.to_string())?,
                        );
                }
            }
            Event::GeneralRef(e) if in_text => {
                if let Some(cell) = &current {
                    let escaped = format!("&{};", e.decode().map_err(|e| e.to_string())?);
                    result
                        .entry(cell.clone())
                        .or_insert_with(String::new)
                        .push_str(
                            &quick_xml::escape::unescape(&escaped).map_err(|e| e.to_string())?,
                        );
                }
            }
            Event::End(e) if e.local_name().as_ref() == b"t" => in_text = false,
            Event::End(e) if e.local_name().as_ref() == b"comment" => current = None,
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(result)
}

fn elements(xml: &[u8], name: &[u8]) -> Result<Vec<BTreeMap<String, String>>, String> {
    let mut reader = XmlReader::from_reader(xml);
    let mut result = Vec::new();
    loop {
        match reader.read_event().map_err(|e| e.to_string())? {
            Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == name => {
                result.push(attrs(&e)?)
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(result)
}

fn route(
    value: &str,
    channel: ContactChannel,
    display: &str,
    order: u32,
) -> Result<CreatorTransferContactRoute, String> {
    let normalized = crate::contact_routes::normalize_discovered_contact_route(
        &crate::DiscoveredContactRoute {
            channel,
            kind: LocatorKind::Handle,
            value: value.into(),
            source: SourceKind::CreatorImport,
            source_url: None,
        },
    )?;
    Ok(CreatorTransferContactRoute {
        save_order: order,
        channel,
        locator_kind: normalized.kind,
        normalized_value: normalized.normalized_value,
        display_value: display.into(),
        confidence: CreatorTransferConfidence::Asserted,
        source_kinds: vec![SourceKind::CreatorImport],
    })
}

pub(super) fn decode(
    bytes: &[u8],
    parts: BTreeMap<String, Vec<u8>>,
) -> Result<CreatorBundle, String> {
    let mut workbook =
        Xlsx::new(Cursor::new(bytes)).map_err(|_| "Library XLSX metadata cannot be read")?;
    let mut rows = stream_sheet(&mut workbook, "推荐")?;
    let headers = parse_headers(
        "推荐",
        rows.remove(&0),
        &["名字", "类别", "平台", "评分", "联系方式"],
    )?;
    let notes = comments(
        parts
            .get("xl/comments1.xml")
            .map(Vec::as_slice)
            .unwrap_or_default(),
    )?;
    let mut targets = BTreeMap::new();
    for mut rel in elements(
        parts
            .get("xl/worksheets/_rels/sheet1.xml.rels")
            .map(Vec::as_slice)
            .unwrap_or_default(),
        b"Relationship",
    )? {
        if rel.get("Type").is_some_and(|v| v.ends_with("/hyperlink")) {
            if let (Some(id), Some(target)) = (rel.remove("Id"), rel.remove("Target")) {
                targets.insert(id, target);
            }
        }
    }
    let mut links = BTreeMap::new();
    for mut link in elements(
        parts
            .get("xl/worksheets/sheet1.xml")
            .ok_or("Library XLSX worksheet part missing")?,
        b"hyperlink",
    )? {
        if let (Some(cell), Some(id)) = (link.remove("ref"), link.remove("r:id")) {
            links.insert(
                cell,
                targets
                    .get(&id)
                    .ok_or("Library XLSX hyperlink target missing")?
                    .clone(),
            );
        }
    }
    let mut creators = Vec::new();
    for (number, row) in rows {
        reject_columns_after_header("推荐", number, &row, &headers)?;
        let text = |key| optional_string("推荐", number, &row, &headers, key);
        let name = required_string("推荐", number, &row, &headers, "名字")?;
        let platform = text("平台")?.unwrap_or_default();
        let category = text("类别")?.unwrap_or_default();
        let score = text("评分")?.unwrap_or_default();
        let contacts = text("联系方式")?.unwrap_or_default();
        let cell = format!("A{}", number + 1);
        let profile = links
            .get(&cell)
            .ok_or_else(|| format!("推荐第 {} 行缺少主页链接，不能仅按名字创建身份", number + 1))?;
        let parsed_url = url::Url::parse(profile).map_err(|_| "Library profile URL invalid")?;
        if !matches!(parsed_url.scheme(), "https" | "http")
            || parsed_url.host_str().is_none()
            || !parsed_url.username().is_empty()
            || parsed_url.password().is_some()
        {
            return Err("Library profile URL invalid".into());
        }
        let mut routes = vec![route(profile, ContactChannel::Website, profile, 0)?];
        for line in contacts.lines().map(str::trim).filter(|s| !s.is_empty()) {
            let wechat = line
                .strip_prefix("微信：")
                .or_else(|| line.strip_prefix("微信:"))
                .or_else(|| line.strip_prefix("WeChat: "));
            let next = if let Some(value) = wechat {
                let handle = value.split(['（', '(', ' ']).next().unwrap_or_default();
                route(handle, ContactChannel::Wechat, handle, routes.len() as u32)?
            } else if let Some(start) = line.find("https://").or_else(|| line.find("http://")) {
                let value = &line[start..];
                route(value, ContactChannel::Website, value, routes.len() as u32)?
            } else if line.contains('@') {
                route(line, ContactChannel::Email, line, routes.len() as u32)?
            } else {
                return Err(format!(
                    "推荐第 {} 行包含无法识别的联系方式，请核对：{line}",
                    number + 1
                ));
            };
            if !routes
                .iter()
                .any(|r| r.channel == next.channel && r.normalized_value == next.normalized_value)
            {
                routes.push(next);
            }
        }
        let mut accounts = Vec::new();
        if let Some(identity) = notes
            .get(&cell)
            .and_then(|s| s.lines().find_map(|l| l.strip_prefix("Identity: ")))
        {
            let id: serde_json::Value =
                serde_json::from_str(identity).map_err(|_| "Library identity comment invalid")?;
            let kind = id
                .get("platform")
                .and_then(|v| v.as_str())
                .ok_or("Library identity platform missing")?;
            let external_id = id
                .get("platform_id")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .ok_or("Library identity ID missing")?;
            // Browser handles are not the scoped TikTok provider IDs used by CRM.
            // Preserve those profiles as URL contacts and retain their source identity in notes.
            let mapped = match kind {
                "x" | "twitter" => Some(CreatorTransferPlatform::X),
                "tiktok" if external_id.starts_with("tto:") => {
                    Some(CreatorTransferPlatform::TikTok)
                }
                "youtube" => Some(CreatorTransferPlatform::YouTube),
                "instagram" => Some(CreatorTransferPlatform::Instagram),
                "telegram" => Some(CreatorTransferPlatform::Telegram),
                _ => None,
            };
            if let Some(mapped) = mapped {
                accounts.push(CreatorTransferAccount {
                    capture_order: 0,
                    platform: mapped,
                    target_kind: if mapped == CreatorTransferPlatform::YouTube {
                        CreatorTransferTargetKind::Channel
                    } else {
                        CreatorTransferTargetKind::User
                    },
                    external_id: external_id.into(),
                    link_state: CreatorTransferLinkState::Claimed,
                    confidence: CreatorTransferConfidence::Asserted,
                    source_kind: SourceKind::CreatorImport,
                    latest_profile: Some(CreatorTransferProfile {
                        handle: None,
                        display_name: Some(name.clone()),
                        profile_url: Some(profile.clone()),
                        avatar_url: None,
                    }),
                });
            }
        }
        let reason = notes
            .get(&format!("D{}", number + 1))
            .cloned()
            .unwrap_or_default();
        let original = format!(
            "Library 推荐导入\n名字：{name}\n平台：{platform}\n类别：{category}\n评分：{score}\n评分说明：{reason}\n联系方式：{contacts}\n{}",
            notes.get(&cell).cloned().unwrap_or_default()
        );
        creators.push(CreatorTransferCreator {
            semantic_sha256: String::new(),
            record_key: String::new(),
            display_name: name,
            tags: vec![],
            notes: Some(original),
            score: None,
            follower_counts: BTreeMap::new(),
            pinned: false,
            relationship: CreatorTransferRelationship {
                stage: CreatorTransferStage::Prospect,
            },
            accounts,
            contact_routes: routes,
        });
    }
    let mut creators = merge_profile_rows(creators);
    // Capture order is global across accounts, not per person. Keep the import
    // and subsequent database export identical so the receipt deduplicates it.
    for (order, account) in creators
        .iter_mut()
        .flat_map(|c| &mut c.accounts)
        .enumerate()
    {
        account.capture_order = order as u32;
    }
    crate::creator_transfer::build_creator_bundle(creators)
}

// A row explicitly links another row's primary profile: retain one person and all
// evidence. Names and shared email/WeChat addresses alone never merge identities.
fn merge_profile_rows(creators: Vec<CreatorTransferCreator>) -> Vec<CreatorTransferCreator> {
    let mut parents: Vec<usize> = (0..creators.len()).collect();
    fn root(parents: &[usize], mut i: usize) -> usize {
        while parents[i] != i {
            i = parents[i];
        }
        i
    }
    let mut primary = BTreeMap::new();
    for (i, c) in creators.iter().enumerate() {
        if let Some(previous) = primary.insert(c.contact_routes[0].normalized_value.clone(), i) {
            let a = root(&parents, previous);
            let b = root(&parents, i);
            parents[b] = a;
        }
    }
    for (i, c) in creators.iter().enumerate() {
        for route in c
            .contact_routes
            .iter()
            .filter(|r| r.channel == ContactChannel::Website)
        {
            if let Some(j) = primary.get(&route.normalized_value) {
                let a = root(&parents, i);
                let b = root(&parents, *j);
                parents[b] = a;
            }
        }
    }
    let mut merged: BTreeMap<usize, CreatorTransferCreator> = BTreeMap::new();
    for (i, mut c) in creators.into_iter().enumerate() {
        match merged.entry(root(&parents, i)) {
            std::collections::btree_map::Entry::Vacant(e) => {
                e.insert(c);
            }
            std::collections::btree_map::Entry::Occupied(mut e) => {
                let target = e.get_mut();
                if let Some(notes) = c.notes.take() {
                    target
                        .notes
                        .get_or_insert_with(String::new)
                        .push_str(&format!("\n\n{notes}"));
                }
                for mut account in c.accounts {
                    if !target.accounts.iter().any(|a| {
                        a.platform == account.platform && a.external_id == account.external_id
                    }) {
                        account.capture_order = target.accounts.len() as u32;
                        target.accounts.push(account);
                    }
                }
                for mut route in c.contact_routes {
                    if !target.contact_routes.iter().any(|r| {
                        r.channel == route.channel && r.normalized_value == route.normalized_value
                    }) {
                        route.save_order = target.contact_routes.len() as u32;
                        target.contact_routes.push(route);
                    }
                }
            }
        }
    }
    merged.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn library_export_preserves_wechat_email_and_source_notes() {
        let mut workbook = Workbook::new();
        let sheet = workbook.add_worksheet();
        sheet.set_name("推荐").unwrap();
        for (i, value) in ["名字", "类别", "平台", "评分", "联系方式"]
            .iter()
            .enumerate()
        {
            sheet.write_string(0, i as u16, *value).unwrap();
        }
        for row in [1, 2] {
            sheet
                .write_url_with_text(row, 0, "https://space.bilibili.com/12345", "Example")
                .unwrap();
            sheet.write_string(row, 2, "bilibili").unwrap();
            sheet.write_string(row, 3, "良好").unwrap();
            sheet
                .write_string(row, 4, "微信：Example_WX（备注合作）\nMAIL@example.com")
                .unwrap();
            sheet
                .insert_note(
                    row,
                    0,
                    &rust_xlsxwriter::Note::new(
                        "Identity: {\"platform\":\"bilibili\",\"platform_id\":\"12345\"}",
                    ),
                )
                .unwrap();
            sheet
                .insert_note(
                    row,
                    3,
                    &rust_xlsxwriter::Note::new("Original score reason & evidence"),
                )
                .unwrap();
        }
        let bytes = workbook.save_to_buffer().unwrap();
        let result = CreatorBundle::from_xlsx_bytes(&bytes).unwrap();
        assert_eq!(result.creator_count, 1);
        let creator = &result.creators[0];
        assert_eq!(creator.contact_routes.len(), 3);
        assert!(
            creator
                .contact_routes
                .iter()
                .any(|r| r.channel == ContactChannel::Wechat && r.normalized_value == "example_wx")
        );
        assert!(creator.contact_routes.iter().any(
            |r| r.channel == ContactChannel::Email && r.normalized_value == "mail@example.com"
        ));
        assert!(
            creator
                .notes
                .as_ref()
                .unwrap()
                .contains("Original score reason & evidence")
        );
        assert_eq!(
            CreatorBundle::from_xlsx_bytes(&result.to_xlsx_bytes().unwrap()).unwrap(),
            result
        );
    }

    #[test]
    fn shared_email_alone_does_not_merge_people() {
        let make = |url: &str| CreatorTransferCreator {
            semantic_sha256: String::new(),
            record_key: String::new(),
            display_name: "Same display name".into(),
            notes: None,
            tags: vec![],
            score: None,
            follower_counts: BTreeMap::new(),
            pinned: false,
            relationship: CreatorTransferRelationship {
                stage: CreatorTransferStage::Prospect,
            },
            accounts: vec![],
            contact_routes: vec![
                route(url, ContactChannel::Website, url, 0).unwrap(),
                route(
                    "agency@example.com",
                    ContactChannel::Email,
                    "agency@example.com",
                    1,
                )
                .unwrap(),
            ],
        };
        let creators = merge_profile_rows(vec![
            make("https://example.com/a"),
            make("https://example.com/b"),
        ]);
        assert_eq!(creators.len(), 2);
        assert!(crate::creator_transfer::build_creator_bundle(creators).is_err());
    }
    #[tokio::test]
    async fn public_source_notes_survive_without_account_or_contact_observations() {
        let directory = tempfile::tempdir().unwrap();
        let runtime =
            crate::CreatorRuntime::open(directory.path(), crate::CreatorGateways::default())
                .unwrap();
        let bundle = crate::creator_transfer::build_creator_bundle(vec![CreatorTransferCreator {
            semantic_sha256: String::new(),
            record_key: String::new(),
            display_name: "Source only".into(),
            notes: Some("Original public score explanation".into()),
            tags: vec![],
            score: None,
            follower_counts: BTreeMap::new(),
            pinned: false,
            relationship: CreatorTransferRelationship {
                stage: CreatorTransferStage::Prospect,
            },
            accounts: vec![],
            contact_routes: vec![],
        }])
        .unwrap();
        let prepared = runtime
            .prepare_creator_import_xlsx_bytes_async(bundle.to_xlsx_bytes().unwrap())
            .await
            .unwrap();
        let crate::PrepareCreatorImportResult::ConfirmationRequired { operation_id, .. } = prepared
        else {
            panic!("preview missing");
        };
        runtime
            .confirm_creator_import_async(crate::ConfirmCreatorImportInput { operation_id })
            .await
            .unwrap();
        assert_eq!(runtime.export_creator_workbook().unwrap().bundle, bundle);
    }

    #[tokio::test]
    #[ignore = "uses an explicitly supplied local workbook, never committed as a fixture"]
    async fn local_recommendation_import_roundtrip() {
        let path = std::env::var("ONECREW_XLSX_FIXTURE").expect("set ONECREW_XLSX_FIXTURE");
        let bytes = std::fs::read(path).unwrap();
        let bundle = CreatorBundle::from_xlsx_bytes(&bytes).unwrap();
        println!(
            "creators={} contacts={} wechat={}",
            bundle.creator_count,
            bundle
                .creators
                .iter()
                .map(|c| c.contact_routes.len())
                .sum::<usize>(),
            bundle
                .creators
                .iter()
                .flat_map(|c| &c.contact_routes)
                .filter(|r| r.channel == ContactChannel::Wechat)
                .count()
        );
        let roundtrip = CreatorBundle::from_xlsx_bytes(&bundle.to_xlsx_bytes().unwrap()).unwrap();
        assert_eq!(bundle, roundtrip);
        let directory = tempfile::tempdir().unwrap();
        let runtime =
            crate::CreatorRuntime::open(directory.path(), crate::CreatorGateways::default())
                .unwrap();
        let prepared = runtime
            .prepare_creator_import_xlsx_bytes_async(bytes.clone())
            .await
            .unwrap();
        let crate::PrepareCreatorImportResult::ConfirmationRequired { operation_id, .. } = prepared
        else {
            panic!("expected preview: {prepared:?}");
        };
        let result = runtime
            .confirm_creator_import_async(crate::ConfirmCreatorImportInput { operation_id })
            .await
            .unwrap();
        assert!(matches!(
            result,
            crate::ConfirmCreatorImportResult::Imported { .. }
        ));
        assert!(matches!(
            runtime
                .prepare_creator_import_xlsx_bytes_async(bytes)
                .await
                .unwrap(),
            crate::PrepareCreatorImportResult::AlreadyImported { .. }
        ));
        assert_eq!(
            runtime
                .list_creators_async(crate::ListCreatorsInput {
                    limit: Some(100),
                    ..Default::default()
                })
                .await
                .unwrap()
                .items
                .len(),
            bundle.creator_count as usize
        );
        let exported = runtime.export_creator_workbook().unwrap();
        assert_eq!(
            exported
                .bundle
                .creators
                .iter()
                .filter(|c| c.notes.is_some())
                .count(),
            bundle.creator_count as usize
        );
        let reimport = runtime
            .prepare_creator_import_xlsx_bytes_async(exported.to_xlsx_bytes().unwrap())
            .await
            .unwrap();
        assert!(
            matches!(
                reimport,
                crate::PrepareCreatorImportResult::AlreadyImported { .. }
            ),
            "roundtrip unexpectedly changes identity: {reimport:?}"
        );
    }
}
