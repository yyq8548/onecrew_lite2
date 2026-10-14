use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read},
    path::{Component, Path},
};

use quick_xml::{Reader, events::Event};
use zip::{CompressionMethod, ZipArchive};

pub(crate) const XLSX_MAX_COMPRESSED_BYTES: usize = crate::CREATOR_BUNDLE_XLSX_MAX_BYTES;
pub(crate) const XLSX_MAX_DECOMPRESSED_BYTES: usize = 256 * 1024 * 1024;
pub(crate) const XLSX_MAX_XML_PART_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const XLSX_MAX_WORKSHEET_PART_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const XLSX_MAX_SHARED_STRINGS_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const XLSX_MAX_ZIP_ENTRIES: usize = 512;
const XLSX_MAX_RATIO: u64 = 100;
const XLSX_MAX_SHEETS: usize = 32;
const XLSX_MAX_ROWS: u32 = 100_000;
const XLSX_MAX_COLUMNS: u32 = 256;
const XLSX_MAX_POPULATED_CELLS: usize = 1_000_000;
const XLSX_MAX_BBOX_AREA: u64 = 1_000_000;
const XLSX_MAX_SHARED_STRINGS: usize = 500_000;
const XLSX_MAX_CELL_TEXT_CHARS: usize = 32_767;
const XLSX_MAX_XML_DEPTH: usize = 64;
const XLSX_MAX_XML_ATTRIBUTES: usize = 64;
const XLSX_MAX_XML_ATTRIBUTE_BYTES: usize = 1024 * 1024;
const XLSX_MAX_XML_TEXT_BYTES: usize = 1024 * 1024;

const CONTENT_TYPES_PART: &str = "[Content_Types].xml";
const ROOT_RELATIONSHIPS_PART: &str = "_rels/.rels";
const CANONICAL_WORKBOOK_PART: &str = "xl/workbook.xml";
const WORKBOOK_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml";

#[derive(Debug)]
pub(crate) struct XlsxPreflight {
    pub(crate) sheet_names: Vec<String>,
    pub(crate) parts: BTreeMap<String, Vec<u8>>,
}

#[derive(Debug)]
struct WorkbookReferences {
    sheet_names: Vec<String>,
    worksheet_parts: BTreeSet<String>,
    shared_strings_part: Option<String>,
}

pub(crate) fn validate_xlsx_tabular_shape(
    data_rows: [usize; 3],
    columns: [usize; 3],
) -> Result<(), String> {
    let mut populated_cells = 0usize;
    for (data_rows, columns) in data_rows.into_iter().zip(columns) {
        let rows = data_rows
            .checked_add(1)
            .ok_or_else(|| "Creator XLSX row count cannot be represented".to_string())?;
        if rows > XLSX_MAX_ROWS as usize {
            return Err("Creator XLSX worksheet has too many rows".into());
        }
        let area = rows
            .checked_mul(columns)
            .ok_or_else(|| "Creator XLSX worksheet area cannot be represented".to_string())?;
        if area > XLSX_MAX_BBOX_AREA as usize {
            return Err("Creator XLSX worksheet populated area is too large".into());
        }
        populated_cells = populated_cells
            .checked_add(area)
            .ok_or_else(|| "Creator XLSX populated cell count cannot be represented".to_string())?;
    }
    if populated_cells > XLSX_MAX_POPULATED_CELLS {
        return Err("Creator XLSX exceeds the populated cell limit".into());
    }
    Ok(())
}

pub(crate) fn preflight_xlsx(bytes: &[u8]) -> Result<XlsxPreflight, String> {
    if bytes.len() > XLSX_MAX_COMPRESSED_BYTES {
        return Err("Creator XLSX exceeds the 32 MiB compressed file limit".into());
    }
    if !bytes.starts_with(b"PK\x03\x04") {
        return Err("Creator import is not an XLSX ZIP package".into());
    }
    if bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0]) {
        return Err("encrypted or legacy Office packages are unsupported".into());
    }

    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| "Creator XLSX ZIP directory is malformed".to_string())?;
    if archive.len() > XLSX_MAX_ZIP_ENTRIES {
        return Err("Creator XLSX contains too many ZIP entries".into());
    }

    let mut names = BTreeSet::new();
    let mut normalized_names = BTreeSet::new();
    let mut parts = BTreeMap::<String, Vec<u8>>::new();
    let mut decompressed_total = 0usize;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| "Creator XLSX contains an unreadable ZIP entry".to_string())?;
        let name = entry.name().to_owned();
        validate_entry_name(&name)?;
        if !names.insert(name.clone()) || !normalized_names.insert(name.to_lowercase()) {
            return Err("Creator XLSX contains duplicate ZIP entry names".into());
        }
        if entry.encrypted() {
            return Err("encrypted Creator XLSX entries are unsupported".into());
        }
        if !matches!(
            entry.compression(),
            CompressionMethod::Stored | CompressionMethod::Deflated
        ) {
            return Err("Creator XLSX uses an unsupported ZIP compression method".into());
        }
        if entry.is_dir()
            || entry.unix_mode().is_some_and(|mode| {
                let file_type = mode & 0o170000;
                file_type != 0 && file_type != 0o100000
            })
        {
            return Err("Creator XLSX ZIP entries must be regular files".into());
        }
        let declared = entry.size();
        let compressed = entry.compressed_size();
        if declared > 0 && (compressed == 0 || declared > compressed.saturating_mul(XLSX_MAX_RATIO))
        {
            return Err("Creator XLSX ZIP entry exceeds the 100:1 expansion limit".into());
        }
        reject_unsupported_part(&name)?;
        let part_limit = part_limit(&name);
        let mut content =
            Vec::with_capacity(usize::try_from(declared.min(part_limit as u64)).unwrap_or(0));
        entry
            .by_ref()
            .take(part_limit as u64 + 1)
            .read_to_end(&mut content)
            .map_err(|_| "Creator XLSX ZIP entry cannot be decompressed".to_string())?;
        if content.len() > part_limit {
            return Err("Creator XLSX contains an oversized OOXML part".into());
        }
        if !content.is_empty()
            && (compressed == 0
                || u64::try_from(content.len()).unwrap_or(u64::MAX)
                    > compressed.saturating_mul(XLSX_MAX_RATIO))
        {
            return Err("Creator XLSX ZIP entry exceeds the 100:1 actual expansion limit".into());
        }
        decompressed_total = decompressed_total
            .checked_add(content.len())
            .ok_or_else(|| "Creator XLSX decompressed size cannot be represented".to_string())?;
        if decompressed_total > XLSX_MAX_DECOMPRESSED_BYTES {
            return Err("Creator XLSX exceeds the 256 MiB decompressed limit".into());
        }
        if name.ends_with(".xml") || name.ends_with(".rels") {
            validate_xml_safety(&content)?;
            parts.insert(name, content);
        }
    }

    for required in [CONTENT_TYPES_PART, ROOT_RELATIONSHIPS_PART] {
        if !names.contains(required) {
            return Err(format!(
                "Creator XLSX is missing required OOXML part {required}"
            ));
        }
    }
    let workbook_part = validate_root_relationships(
        parts
            .get(ROOT_RELATIONSHIPS_PART)
            .expect("required part checked"),
    )?;
    if workbook_part != CANONICAL_WORKBOOK_PART || !names.contains(&workbook_part) {
        return Err("Creator XLSX root must reference exactly xl/workbook.xml".into());
    }
    validate_content_types(
        parts
            .get(CONTENT_TYPES_PART)
            .expect("required part checked"),
        &names,
        &workbook_part,
    )?;
    validate_relationships_are_local(&parts)?;
    let workbook_relationships_part = relationship_part_for(&workbook_part)?;
    let workbook = parts
        .get(&workbook_part)
        .ok_or_else(|| "Creator XLSX canonical workbook part is unreadable".to_string())?;
    let workbook_rels = parts
        .get(&workbook_relationships_part)
        .ok_or_else(|| "Creator XLSX canonical workbook relationships are missing".to_string())?;
    let workbook_directory = package_part_directory(&workbook_part);
    let references =
        validate_workbook_sheets(workbook, workbook_rels, &names, &workbook_directory)?;
    validate_shared_strings(
        references
            .shared_strings_part
            .as_ref()
            .and_then(|part| parts.get(part)),
    )?;
    let mut populated_cells = 0usize;
    for worksheet_part in references.worksheet_parts {
        let content = parts.get(&worksheet_part).ok_or_else(|| {
            "Creator XLSX canonical workbook references an unreadable worksheet".to_string()
        })?;
        let sheet_cells = validate_worksheet(content)
            .map_err(|error| format!("Creator XLSX worksheet {worksheet_part}: {error}"))?;
        populated_cells = populated_cells
            .checked_add(sheet_cells)
            .ok_or_else(|| "Creator XLSX populated cell count cannot be represented".to_string())?;
        if populated_cells > XLSX_MAX_POPULATED_CELLS {
            return Err("Creator XLSX exceeds the populated cell limit".into());
        }
    }
    Ok(XlsxPreflight {
        sheet_names: references.sheet_names,
        parts,
    })
}

fn validate_entry_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.contains('\0')
        || name.contains('\\')
        || name.contains("//")
        || name.starts_with('/')
        || name.ends_with('/')
    {
        return Err("Creator XLSX contains an unsafe ZIP entry name".into());
    }
    let path = Path::new(name);
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::CurDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err("Creator XLSX contains a path-traversal ZIP entry".into());
    }
    Ok(())
}

fn reject_unsupported_part(name: &str) -> Result<(), String> {
    let lower = name.to_lowercase();
    let forbidden = [
        "vbaproject",
        "externallinks/",
        "embeddings/",
        "activex/",
        "customui/",
        "connections.xml",
        "querytables/",
        "pivottables/",
        "pivotcache/",
        "charts/",
        "media/",
        "macrosheets/",
        "intlmacrosheets/",
    ];
    if forbidden.iter().any(|needle| lower.contains(needle)) {
        return Err("Creator XLSX contains unsupported active or external content".into());
    }
    if lower.ends_with(".bin") || lower.ends_with(".xlsb") || lower.ends_with(".xlsm") {
        return Err("Creator XLSX contains unsupported binary or macro content".into());
    }
    Ok(())
}

fn part_limit(name: &str) -> usize {
    if name.starts_with("xl/worksheets/") {
        XLSX_MAX_WORKSHEET_PART_BYTES
    } else if name == "xl/sharedStrings.xml" {
        XLSX_MAX_SHARED_STRINGS_BYTES
    } else {
        XLSX_MAX_XML_PART_BYTES
    }
}

fn validate_xml_safety(bytes: &[u8]) -> Result<(), String> {
    let xml = std::str::from_utf8(bytes)
        .map_err(|_| "Creator XLSX OOXML must use UTF-8 or ASCII".to_string())?;
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        return Err("Creator XLSX OOXML must use UTF-8 or ASCII".into());
    }
    let prefix = xml
        .chars()
        .take(256)
        .collect::<String>()
        .to_ascii_lowercase();
    if prefix.contains("encoding=\"utf-16") || prefix.contains("encoding='utf-16") {
        return Err("Creator XLSX OOXML must use UTF-8 or ASCII".into());
    }
    let mut reader = Reader::from_reader(bytes);
    let mut depth = 0usize;
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                depth += 1;
                if depth > XLSX_MAX_XML_DEPTH {
                    return Err("Creator XLSX XML nesting is too deep".into());
                }
                validate_xml_attributes(&event)?;
            }
            Ok(Event::Empty(event)) => {
                validate_xml_attributes(&event)?;
            }
            Ok(Event::End(_)) => depth = depth.saturating_sub(1),
            Ok(Event::Decl(declaration)) => {
                if let Some(encoding) = declaration.encoding() {
                    let encoding = encoding
                        .map_err(|_| "Creator XLSX XML encoding declaration is malformed")?;
                    let encoding = String::from_utf8_lossy(&encoding).to_ascii_lowercase();
                    if !matches!(encoding.as_str(), "utf-8" | "us-ascii" | "ascii") {
                        return Err("Creator XLSX OOXML must use UTF-8 or ASCII".into());
                    }
                }
            }
            Ok(Event::Text(text)) => {
                if text.len() > XLSX_MAX_XML_TEXT_BYTES {
                    return Err("Creator XLSX XML text node is too large".into());
                }
            }
            Ok(Event::CData(text)) => {
                if text.len() > XLSX_MAX_XML_TEXT_BYTES {
                    return Err("Creator XLSX XML text node is too large".into());
                }
            }
            Ok(Event::DocType(_)) => return Err("Creator XLSX DTD content is unsupported".into()),
            Ok(Event::PI(_)) => {
                return Err("Creator XLSX processing instructions are unsupported".into());
            }
            Ok(Event::Eof) => break,
            Err(_) => return Err("Creator XLSX contains malformed OOXML".into()),
            _ => {}
        }
    }
    Ok(())
}

fn validate_xml_attributes(event: &quick_xml::events::BytesStart<'_>) -> Result<(), String> {
    let mut count = 0usize;
    for attribute in event.attributes() {
        let attribute = attribute.map_err(|_| "Creator XLSX XML attribute is malformed")?;
        count += 1;
        if count > XLSX_MAX_XML_ATTRIBUTES {
            return Err("Creator XLSX XML element has too many attributes".into());
        }
        if attribute.value.len() > XLSX_MAX_XML_ATTRIBUTE_BYTES {
            return Err("Creator XLSX XML attribute value is too large".into());
        }
    }
    Ok(())
}

fn validate_content_types(
    bytes: &[u8],
    names: &BTreeSet<String>,
    workbook_part: &str,
) -> Result<(), String> {
    let mut reader = Reader::from_reader(bytes);
    let mut override_parts = BTreeSet::new();
    let mut default_extensions = BTreeSet::new();
    let mut workbook_overrides = 0usize;
    loop {
        match reader.read_event() {
            Ok(Event::Empty(event)) | Ok(Event::Start(event))
                if event.local_name().as_ref() == b"Override" =>
            {
                let attrs = attributes(&event)?;
                let part_name = attrs.get("PartName").ok_or_else(|| {
                    "Creator XLSX content-type override is missing PartName".to_string()
                })?;
                let content_type = attrs.get("ContentType").ok_or_else(|| {
                    "Creator XLSX content-type override is missing ContentType".to_string()
                })?;
                reject_forbidden_content_type(content_type)?;
                let normalized_part = normalize_content_type_part(part_name)?;
                if !override_parts.insert(normalized_part.clone()) {
                    return Err("Creator XLSX duplicates a content-type override".into());
                }
                if !names.contains(&normalized_part) {
                    return Err("Creator XLSX content types reference a missing part".into());
                }
                if content_type.eq_ignore_ascii_case(WORKBOOK_CONTENT_TYPE) {
                    workbook_overrides += 1;
                    if normalized_part != workbook_part {
                        return Err(
                            "Creator XLSX workbook content type targets an alternate root".into(),
                        );
                    }
                } else if content_type
                    .to_ascii_lowercase()
                    .contains("spreadsheetml.sheet.main")
                {
                    return Err("Creator XLSX has an unsupported workbook content type".into());
                }
            }
            Ok(Event::Empty(event)) | Ok(Event::Start(event))
                if event.local_name().as_ref() == b"Default" =>
            {
                let attrs = attributes(&event)?;
                let extension = attrs.get("Extension").ok_or_else(|| {
                    "Creator XLSX default content type is missing Extension".to_string()
                })?;
                let content_type = attrs.get("ContentType").ok_or_else(|| {
                    "Creator XLSX default content type is missing ContentType".to_string()
                })?;
                if extension.is_empty()
                    || !extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
                    || !default_extensions.insert(extension.to_ascii_lowercase())
                {
                    return Err("Creator XLSX default content type extension is invalid".into());
                }
                // Spreadsheet writers may declare unused default MIME types.
                // Only an actual part with this extension can activate that type;
                // explicit overrides and all actual binary/active parts stay gated.
                if names.iter().any(|name| name.rsplit_once('.')
                    .is_some_and(|(_, suffix)| suffix.eq_ignore_ascii_case(extension))) {
                    reject_forbidden_content_type(content_type)?;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return Err("Creator XLSX content types are malformed".into()),
            _ => {}
        }
    }
    if workbook_overrides != 1 {
        return Err("Creator XLSX must declare exactly one canonical workbook content type".into());
    }
    Ok(())
}

fn reject_forbidden_content_type(content_type: &str) -> Result<(), String> {
    let lower = content_type.to_ascii_lowercase();
    let forbidden = [
        "macroenabled",
        "vnd.ms-office.vbaproject",
        "vnd.ms-office.activex",
        "officedocument.oleobject",
        "officedocument.package",
        "spreadsheetml.externallink",
        "application/vnd.ms-excel.controlproperties+xml",
    ];
    if forbidden.iter().any(|needle| lower.contains(needle)) {
        return Err("Creator XLSX contains a forbidden active content type".into());
    }
    Ok(())
}

fn validate_root_relationships(bytes: &[u8]) -> Result<String, String> {
    let mut reader = Reader::from_reader(bytes);
    let mut relationship_ids = BTreeSet::new();
    let mut office_document = None;
    loop {
        match reader.read_event() {
            Ok(Event::Empty(event)) | Ok(Event::Start(event))
                if event.local_name().as_ref() == b"Relationship" =>
            {
                let attrs = attributes(&event)?;
                let id = attrs
                    .get("Id")
                    .ok_or_else(|| "Creator XLSX root relationship is missing Id".to_string())?;
                if !relationship_ids.insert(id.clone()) {
                    return Err("Creator XLSX duplicates a root relationship Id".into());
                }
                let kind = attrs
                    .get("Type")
                    .ok_or_else(|| "Creator XLSX root relationship is missing Type".to_string())?;
                let target = attrs.get("Target").ok_or_else(|| {
                    "Creator XLSX root relationship is missing Target".to_string()
                })?;
                if is_office_document_relationship(kind) {
                    if attrs
                        .get("TargetMode")
                        .is_some_and(|mode| mode.eq_ignore_ascii_case("external"))
                    {
                        return Err("Creator XLSX root workbook cannot be external".into());
                    }
                    let normalized = normalize_relationship_target("", target)?;
                    if normalized != CANONICAL_WORKBOOK_PART {
                        return Err(
                            "Creator XLSX root must reference exactly xl/workbook.xml".into()
                        );
                    }
                    if office_document.replace(normalized).is_some() {
                        return Err(
                            "Creator XLSX must contain exactly one officeDocument root".into()
                        );
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return Err("Creator XLSX root relationships are malformed".into()),
            _ => {}
        }
    }
    office_document
        .ok_or_else(|| "Creator XLSX root relationship does not reference a workbook".to_string())
}

fn is_office_document_relationship(kind: &str) -> bool {
    matches!(
        kind,
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument"
            | "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument"
    )
}

fn validate_relationships_are_local(parts: &BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    for (name, bytes) in parts {
        if name.ends_with(".rels") {
            let mut reader = Reader::from_reader(bytes.as_slice());
            let mut relationship_ids = BTreeSet::new();
            loop {
                match reader.read_event() {
                    Ok(Event::Empty(event)) | Ok(Event::Start(event))
                        if event.local_name().as_ref() == b"Relationship" =>
                    {
                        let attrs = attributes(&event)?;
                        let id = attrs
                            .get("Id")
                            .ok_or_else(|| "Creator XLSX relationship is missing Id".to_string())?;
                        if !relationship_ids.insert(id.clone()) {
                            return Err("Creator XLSX duplicates a relationship Id".into());
                        }
                        let kind = attrs.get("Type").ok_or_else(|| {
                            "Creator XLSX relationship is missing Type".to_string()
                        })?;
                        let target = attrs.get("Target").ok_or_else(|| {
                            "Creator XLSX relationship is missing Target".to_string()
                        })?;
                        reject_forbidden_relationship_type(kind)?;
                        if attrs
                            .get("TargetMode")
                            .is_some_and(|mode| mode.eq_ignore_ascii_case("external"))
                        {
                            let safe_hyperlink = kind.ends_with("/hyperlink")
                                && url::Url::parse(target).ok().is_some_and(|url| {
                                    matches!(url.scheme(), "http" | "https")
                                        && url.host_str().is_some()
                                });
                            if !safe_hyperlink {
                                return Err(
                                    "Creator XLSX external relationships are unsupported".into()
                                );
                            }
                        }
                    }
                    Ok(Event::Eof) => break,
                    Err(_) => return Err("Creator XLSX relationships are malformed".into()),
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn reject_forbidden_relationship_type(kind: &str) -> Result<(), String> {
    let lower = kind.to_ascii_lowercase();
    let forbidden_suffixes = [
        "/oleobject",
        "/package",
        "/control",
        "/activexcontrol",
        "/activexcontrolbinary",
        "/ctrlprop",
        "/vbaproject",
        "/externallink",
        "/externallinkpath",
        "/attachedtemplate",
    ];
    if forbidden_suffixes
        .iter()
        .any(|suffix| lower.ends_with(suffix))
        || lower.contains("/customui")
    {
        return Err("Creator XLSX contains a forbidden active relationship type".into());
    }
    Ok(())
}

fn normalize_content_type_part(part_name: &str) -> Result<String, String> {
    let target = part_name
        .strip_prefix('/')
        .ok_or_else(|| "Creator XLSX content-type PartName must be absolute".to_string())?;
    normalize_relationship_target("", target)
}

fn normalize_relationship_target(base_directory: &str, target: &str) -> Result<String, String> {
    if target.is_empty()
        || target.contains('\\')
        || target.contains('?')
        || target.contains('#')
        || target.contains('%')
    {
        return Err("Creator XLSX relationship target is unsafe".into());
    }
    let absolute = target.starts_with('/');
    let target = target.trim_start_matches('/');
    let mut segments = Vec::new();
    if !absolute && !base_directory.is_empty() {
        segments.extend(base_directory.split('/'));
    }
    for segment in target.split('/') {
        if segment.is_empty() || matches!(segment, "." | "..") {
            return Err("Creator XLSX relationship target is unsafe".into());
        }
        if !segment.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'[' | b']')
        }) {
            return Err("Creator XLSX relationship target is unsafe".into());
        }
        segments.push(segment);
    }
    Ok(segments.join("/"))
}

fn relationship_part_for(source_part: &str) -> Result<String, String> {
    let (directory, file_name) = source_part
        .rsplit_once('/')
        .ok_or_else(|| "Creator XLSX workbook part is not canonical".to_string())?;
    Ok(format!("{directory}/_rels/{file_name}.rels"))
}

fn package_part_directory(part: &str) -> String {
    part.rsplit_once('/')
        .map(|(directory, _)| directory.to_string())
        .unwrap_or_default()
}

fn validate_workbook_sheets(
    workbook: &[u8],
    workbook_rels: &[u8],
    names: &BTreeSet<String>,
    workbook_directory: &str,
) -> Result<WorkbookReferences, String> {
    let mut relationship_targets = BTreeMap::new();
    let mut shared_strings_target = None;
    let mut reader = Reader::from_reader(workbook_rels);
    loop {
        match reader.read_event() {
            Ok(Event::Empty(event)) | Ok(Event::Start(event))
                if event.local_name().as_ref() == b"Relationship" =>
            {
                let attributes = attributes(&event)?;
                if let (Some(id), Some(target), Some(kind)) = (
                    attributes.get("Id"),
                    attributes.get("Target"),
                    attributes.get("Type"),
                ) && kind.ends_with("/worksheet")
                {
                    if relationship_targets
                        .insert(id.clone(), target.clone())
                        .is_some()
                    {
                        return Err("Creator XLSX duplicates a worksheet relationship Id".into());
                    }
                } else if let (Some(target), Some(kind)) =
                    (attributes.get("Target"), attributes.get("Type"))
                    && kind.ends_with("/sharedStrings")
                {
                    let part = normalize_relationship_target(workbook_directory, target)?;
                    if !names.contains(&part) {
                        return Err(
                            "Creator XLSX sharedStrings relationship target is missing".into()
                        );
                    }
                    if shared_strings_target.replace(part).is_some() {
                        return Err("Creator XLSX duplicates its sharedStrings relationship".into());
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return Err("Creator XLSX workbook relationships are malformed".into()),
            _ => {}
        }
    }
    let mut sheets = Vec::new();
    let mut worksheet_parts = BTreeSet::new();
    let mut filter_database: Option<(usize, String)> = None;
    let mut reader = Reader::from_reader(workbook);
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) if event.local_name().as_ref() == b"definedName" => {
                let attrs = attributes(&event)?;
                if attrs.get("name").map(String::as_str) != Some("_xlnm._FilterDatabase")
                    || !matches!(attrs.get("hidden").map(String::as_str), Some("1" | "true"))
                {
                    return Err("Creator XLSX defined-name formulas are unsupported".into());
                }
                let local_sheet_id = attrs
                    .get("localSheetId")
                    .ok_or_else(|| {
                        "Creator XLSX filter definition is missing its sheet".to_string()
                    })?
                    .parse::<usize>()
                    .map_err(|_| "Creator XLSX filter definition sheet is invalid".to_string())?;
                if filter_database
                    .replace((local_sheet_id, String::new()))
                    .is_some()
                {
                    return Err("Creator XLSX defined names cannot be nested".into());
                }
            }
            Ok(Event::Empty(event)) if event.local_name().as_ref() == b"definedName" => {
                return Err("Creator XLSX defined-name formulas are unsupported".into());
            }
            Ok(Event::Text(text)) if filter_database.is_some() => {
                let text = std::str::from_utf8(text.as_ref())
                    .map_err(|_| "Creator XLSX filter definition is malformed".to_string())?;
                filter_database
                    .as_mut()
                    .ok_or_else(|| "Creator XLSX filter definition is malformed".to_string())?
                    .1
                    .push_str(text);
            }
            Ok(Event::End(event)) if event.local_name().as_ref() == b"definedName" => {
                let (local_sheet_id, reference) = filter_database
                    .take()
                    .ok_or_else(|| "Creator XLSX filter definition is malformed".to_string())?;
                validate_filter_database_reference(local_sheet_id, &reference)?;
            }
            Ok(Event::Empty(event)) | Ok(Event::Start(event))
                if event.local_name().as_ref() == b"sheet" =>
            {
                let attributes = attributes(&event)?;
                let name = attributes
                    .get("name")
                    .ok_or_else(|| "Creator XLSX sheet is missing its name".to_string())?
                    .clone();
                let relationship_id = attributes
                    .get("r:id")
                    .or_else(|| attributes.get("id"))
                    .ok_or_else(|| "Creator XLSX sheet is missing its relationship".to_string())?;
                let target = relationship_targets
                    .get(relationship_id)
                    .ok_or_else(|| "Creator XLSX sheet relationship is unresolved".to_string())?;
                let part = normalize_relationship_target(workbook_directory, target)?;
                if !names.contains(&part) {
                    return Err("Creator XLSX references a missing worksheet part".into());
                }
                if !worksheet_parts.insert(part) {
                    return Err("Creator XLSX references one worksheet more than once".into());
                }
                sheets.push(name);
            }
            Ok(Event::Eof) => break,
            Err(_) => return Err("Creator XLSX workbook metadata is malformed".into()),
            _ => {}
        }
    }
    if filter_database.is_some() {
        return Err("Creator XLSX filter definition is malformed".into());
    }
    if sheets.is_empty() || sheets.len() > XLSX_MAX_SHEETS {
        return Err("Creator XLSX has an unsupported sheet count".into());
    }
    let distinct = sheets
        .iter()
        .map(|name| name.to_lowercase())
        .collect::<BTreeSet<_>>();
    if distinct.len() != sheets.len() {
        return Err("Creator XLSX contains duplicate sheet names".into());
    }
    Ok(WorkbookReferences {
        sheet_names: sheets,
        worksheet_parts,
        shared_strings_part: shared_strings_target,
    })
}

fn validate_filter_database_reference(
    local_sheet_id: usize,
    reference: &str,
) -> Result<(), String> {
    let sheet = ["Creators", "Accounts", "Contacts"]
        .get(local_sheet_id)
        .ok_or_else(|| "Creator XLSX filter definition sheet is invalid".to_string())?;
    let suffix = reference
        .strip_prefix(&format!("{sheet}!$A$1:$"))
        .ok_or_else(|| "Creator XLSX filter definition range is invalid".to_string())?;
    let (column, row) = suffix
        .split_once('$')
        .ok_or_else(|| "Creator XLSX filter definition range is invalid".to_string())?;
    if column.is_empty()
        || column.len() > 3
        || !column.bytes().all(|byte| byte.is_ascii_uppercase())
        || parse_cell_reference(&format!("{column}{row}"))
            .and_then(|(row, col)| enforce_dimensions(row, col))
            .is_err()
    {
        return Err("Creator XLSX filter definition range is invalid".into());
    }
    Ok(())
}

fn validate_shared_strings(bytes: Option<&Vec<u8>>) -> Result<(), String> {
    let Some(bytes) = bytes else { return Ok(()) };
    let mut reader = Reader::from_reader(bytes.as_slice());
    let mut string_count = 0usize;
    let mut in_string = false;
    let mut chars = 0usize;
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) | Ok(Event::Empty(event))
                if event.local_name().as_ref() == b"sst" =>
            {
                let attrs = attributes(&event)?;
                if let Some(value) = attrs.get("uniqueCount") {
                    let declared = value.parse::<usize>().map_err(|_| {
                        "Creator XLSX sharedStrings uniqueCount is invalid".to_string()
                    })?;
                    if declared > XLSX_MAX_SHARED_STRINGS {
                        return Err("Creator XLSX has too many shared strings".into());
                    }
                }
            }
            Ok(Event::Start(event)) if event.local_name().as_ref() == b"si" => {
                string_count += 1;
                if string_count > XLSX_MAX_SHARED_STRINGS {
                    return Err("Creator XLSX has too many shared strings".into());
                }
                in_string = true;
                chars = 0;
            }
            Ok(Event::Empty(event)) if event.local_name().as_ref() == b"si" => {
                string_count += 1;
                if string_count > XLSX_MAX_SHARED_STRINGS {
                    return Err("Creator XLSX has too many shared strings".into());
                }
            }
            Ok(Event::End(event)) if event.local_name().as_ref() == b"si" => in_string = false,
            Ok(Event::Text(text)) if in_string => {
                chars =
                    chars.saturating_add(String::from_utf8_lossy(text.as_ref()).chars().count());
                if chars > XLSX_MAX_CELL_TEXT_CHARS {
                    return Err("Creator XLSX contains an oversized shared string".into());
                }
            }
            Ok(Event::CData(text)) if in_string => {
                chars =
                    chars.saturating_add(String::from_utf8_lossy(text.as_ref()).chars().count());
                if chars > XLSX_MAX_CELL_TEXT_CHARS {
                    return Err("Creator XLSX contains an oversized shared string".into());
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return Err("Creator XLSX sharedStrings are malformed".into()),
            _ => {}
        }
    }
    Ok(())
}

fn validate_worksheet(bytes: &[u8]) -> Result<usize, String> {
    let mut reader = Reader::from_reader(bytes);
    let mut cells = 0usize;
    let mut max_row = 0u32;
    let mut max_col = 0u32;
    let mut rows = 0usize;
    let mut row_references = BTreeSet::new();
    let mut cell_references = BTreeSet::new();
    let mut in_cell = false;
    let mut cell_chars = 0usize;
    loop {
        match reader.read_event() {
            Ok(Event::Empty(event)) | Ok(Event::Start(event))
                if event.local_name().as_ref() == b"dimension" =>
            {
                if let Some(reference) = attributes(&event)?.get("ref") {
                    let tail = reference.rsplit(':').next().unwrap_or(reference);
                    let (row, col) = parse_cell_reference(tail)?;
                    enforce_dimensions(row, col)?;
                    if u64::from(row).saturating_mul(u64::from(col)) > XLSX_MAX_BBOX_AREA {
                        return Err("Creator XLSX worksheet declared area is too large".into());
                    }
                }
            }
            Ok(Event::Empty(event)) | Ok(Event::Start(event))
                if event.local_name().as_ref() == b"row" =>
            {
                rows += 1;
                if rows > XLSX_MAX_ROWS as usize {
                    return Err("Creator XLSX worksheet has too many rows".into());
                }
                let row = attributes(&event)?
                    .get("r")
                    .ok_or_else(|| {
                        "Creator XLSX worksheet row is missing its reference".to_string()
                    })?
                    .parse::<u32>()
                    .map_err(|_| "Creator XLSX worksheet row reference is invalid".to_string())?;
                if row == 0 || row > XLSX_MAX_ROWS {
                    return Err(
                        "Creator XLSX worksheet dimensions exceed the supported limit".into(),
                    );
                }
                if !row_references.insert(row) {
                    return Err("Creator XLSX worksheet duplicates a row reference".into());
                }
            }
            Ok(Event::Start(event)) if event.local_name().as_ref() == b"c" => {
                let attrs = attributes(&event)?;
                let reference = attrs
                    .get("r")
                    .ok_or_else(|| "Creator XLSX cell is missing its reference".to_string())?;
                let (row, col) = parse_cell_reference(reference)?;
                enforce_dimensions(row, col)?;
                if !cell_references.insert((row, col)) {
                    return Err("Creator XLSX worksheet duplicates a cell reference".into());
                }
                max_row = max_row.max(row);
                max_col = max_col.max(col);
                in_cell = true;
                cell_chars = 0;
                cells += 1;
                if cells > XLSX_MAX_POPULATED_CELLS {
                    return Err("Creator XLSX worksheet has too many populated cells".into());
                }
            }
            Ok(Event::Empty(event)) if event.local_name().as_ref() == b"c" => {
                let attrs = attributes(&event)?;
                let reference = attrs
                    .get("r")
                    .ok_or_else(|| "Creator XLSX cell is missing its reference".to_string())?;
                let (row, col) = parse_cell_reference(reference)?;
                enforce_dimensions(row, col)?;
                if !cell_references.insert((row, col)) {
                    return Err("Creator XLSX worksheet duplicates a cell reference".into());
                }
                max_row = max_row.max(row);
                max_col = max_col.max(col);
                in_cell = false;
                cell_chars = 0;
                cells += 1;
                if cells > XLSX_MAX_POPULATED_CELLS {
                    return Err("Creator XLSX worksheet has too many populated cells".into());
                }
            }
            Ok(Event::Empty(event)) | Ok(Event::Start(event))
                if event.local_name().as_ref() == b"f" =>
            {
                return Err("Creator XLSX formulas are unsupported".into());
            }
            Ok(Event::End(event)) if event.local_name().as_ref() == b"c" => in_cell = false,
            Ok(Event::Text(text)) if in_cell => {
                cell_chars = cell_chars
                    .saturating_add(String::from_utf8_lossy(text.as_ref()).chars().count());
                if cell_chars > XLSX_MAX_CELL_TEXT_CHARS {
                    return Err("Creator XLSX contains an oversized cell string".into());
                }
            }
            Ok(Event::CData(text)) if in_cell => {
                cell_chars = cell_chars
                    .saturating_add(String::from_utf8_lossy(text.as_ref()).chars().count());
                if cell_chars > XLSX_MAX_CELL_TEXT_CHARS {
                    return Err("Creator XLSX contains an oversized cell string".into());
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return Err("Creator XLSX worksheet XML is malformed".into()),
            _ => {}
        }
    }
    if u64::from(max_row).saturating_mul(u64::from(max_col)) > XLSX_MAX_BBOX_AREA {
        return Err("Creator XLSX worksheet populated area is too large".into());
    }
    Ok(cells)
}

fn enforce_dimensions(row: u32, col: u32) -> Result<(), String> {
    if row == 0 || row > XLSX_MAX_ROWS || col == 0 || col > XLSX_MAX_COLUMNS {
        return Err("Creator XLSX worksheet dimensions exceed the supported limit".into());
    }
    Ok(())
}

fn parse_cell_reference(reference: &str) -> Result<(u32, u32), String> {
    let reference = reference.trim_matches('$');
    let split = reference
        .find(|character: char| character.is_ascii_digit())
        .ok_or_else(|| "Creator XLSX cell reference is invalid".to_string())?;
    let (letters, digits) = reference.split_at(split);
    if letters.is_empty()
        || digits.is_empty()
        || !letters.bytes().all(|byte| byte.is_ascii_alphabetic())
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("Creator XLSX cell reference is invalid".into());
    }
    let mut col = 0u32;
    for byte in letters.bytes() {
        col = col
            .checked_mul(26)
            .and_then(|value| value.checked_add(u32::from(byte.to_ascii_uppercase() - b'A' + 1)))
            .ok_or_else(|| "Creator XLSX cell column cannot be represented".to_string())?;
    }
    let row = digits
        .parse::<u32>()
        .map_err(|_| "Creator XLSX cell row cannot be represented".to_string())?;
    Ok((row, col))
}

fn attributes(
    event: &quick_xml::events::BytesStart<'_>,
) -> Result<BTreeMap<String, String>, String> {
    let mut result = BTreeMap::new();
    for attribute in event.attributes() {
        let attribute = attribute.map_err(|_| "Creator XLSX XML attribute is malformed")?;
        if attribute.value.len() > XLSX_MAX_XML_ATTRIBUTE_BYTES {
            return Err("Creator XLSX XML attribute value is too large".into());
        }
        let key = String::from_utf8_lossy(attribute.key.as_ref()).into_owned();
        let value = std::str::from_utf8(attribute.value.as_ref())
            .map_err(|_| "Creator XLSX XML attribute value is malformed")?
            .to_owned();
        if result.insert(key, value).is_some() {
            return Err("Creator XLSX XML element duplicates an attribute".into());
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read, Write};

    use zip::{ZipWriter, write::SimpleFileOptions};

    use super::*;

    #[test]
    fn unused_binary_default_does_not_make_an_xml_workbook_active() {
        let golden = include_bytes!("../tests/fixtures/creator_workbook_golden.xlsx");
        let types = zip_text(golden, CONTENT_TYPES_PART).replace("</Types>",
            "<Default Extension=\"bin\" ContentType=\"application/vnd.ms-excel.sheet.binary.macroEnabled.main\"/></Types>");
        let package = rewrite_package(golden,
            BTreeMap::from([(CONTENT_TYPES_PART, types.clone().into_bytes())]), Vec::new());
        assert!(preflight_xlsx(&package).is_ok());
        let active = rewrite_package(golden,
            BTreeMap::from([(CONTENT_TYPES_PART, types.into_bytes())]),
            vec![("xl/payload.bin", vec![1, 2, 3])]);
        assert!(preflight_xlsx(&active).is_err());
    }

    #[test]
    fn zip_gate_rejects_path_traversal_and_actual_high_ratio_entries() {
        let traversal = one_entry_zip("../outside.xml", b"<x/>", CompressionMethod::Stored);
        assert!(
            preflight_xlsx(&traversal)
                .unwrap_err()
                .contains("path-traversal")
        );

        let high_ratio = one_entry_zip(
            "[Content_Types].xml",
            &vec![b'a'; 2 * 1024 * 1024],
            CompressionMethod::Deflated,
        );
        assert!(
            preflight_xlsx(&high_ratio)
                .unwrap_err()
                .contains("expansion limit")
        );
    }

    #[test]
    fn xml_gate_rejects_extreme_dimensions_formulas_and_shared_string_counts() {
        assert!(
            validate_worksheet(
                br#"<worksheet><dimension ref="A1:XFD1048576"/><sheetData/></worksheet>"#
            )
            .unwrap_err()
            .contains("dimensions")
        );
        assert!(validate_worksheet(br#"<worksheet><sheetData><row r="1"><c r="A1"><f>1+1</f></c></row></sheetData></worksheet>"#)
            .unwrap_err()
            .contains("formulas"));
        assert!(
            validate_shared_strings(Some(&br#"<sst uniqueCount="500001"></sst>"#.to_vec()))
                .unwrap_err()
                .contains("too many shared strings")
        );
        let empty_shared_strings =
            format!("<sst>{}</sst>", "<si/>".repeat(XLSX_MAX_SHARED_STRINGS + 1));
        assert!(
            validate_shared_strings(Some(&empty_shared_strings.into_bytes()))
                .unwrap_err()
                .contains("too many shared strings")
        );
        let repeated_rows = format!(
            "<worksheet><sheetData>{}</sheetData></worksheet>",
            (1..=XLSX_MAX_ROWS + 1)
                .map(|row| format!("<row r=\"{row}\"/>"))
                .collect::<String>()
        );
        assert!(
            validate_worksheet(repeated_rows.as_bytes())
                .unwrap_err()
                .contains("too many rows")
        );
        assert!(
            validate_worksheet(
                br#"<worksheet><sheetData><row r="2"/><row r="2"/></sheetData></worksheet>"#
            )
            .unwrap_err()
            .contains("duplicates a row reference")
        );
        assert!(
            validate_worksheet(
                br#"<worksheet><sheetData><row r="2"><c r="A2"/><c r="A2"/></row></sheetData></worksheet>"#
            )
            .unwrap_err()
            .contains("duplicates a cell reference")
        );
        let oversized = format!(
            "<worksheet><sheetData><row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>{}</t></is></c></row></sheetData></worksheet>",
            "a".repeat(XLSX_MAX_CELL_TEXT_CHARS + 1)
        );
        assert!(
            validate_worksheet(oversized.as_bytes())
                .unwrap_err()
                .contains("oversized cell string")
        );
    }

    #[test]
    fn xml_gate_rejects_non_utf8_and_oversized_attributes() {
        assert!(
            validate_xml_safety(b"<x>\xff</x>")
                .unwrap_err()
                .contains("UTF-8")
        );
        let oversized_attribute = format!(
            "<x value=\"{}\"/>",
            "a".repeat(XLSX_MAX_XML_ATTRIBUTE_BYTES + 1)
        );
        assert!(
            validate_xml_safety(oversized_attribute.as_bytes())
                .unwrap_err()
                .contains("attribute value is too large")
        );
        assert!(
            validate_workbook_sheets(
                b"<workbook><definedNames><definedName name=\"unsafe\">1+1</definedName></definedNames></workbook>",
                b"<Relationships/>",
                &BTreeSet::new(),
                "xl",
            )
            .unwrap_err()
            .contains("defined-name formulas")
        );
    }

    #[test]
    fn export_shape_gate_matches_the_import_row_area_and_cell_caps() {
        assert!(validate_xlsx_tabular_shape([10_000, 10_000, 10_000], [15, 13, 9]).is_ok());
        assert!(
            validate_xlsx_tabular_shape([XLSX_MAX_ROWS as usize, 0, 0], [15, 13, 9])
                .unwrap_err()
                .contains("too many rows")
        );
        assert!(
            validate_xlsx_tabular_shape([10_000, 80_000, 10_000], [15, 13, 9])
                .unwrap_err()
                .contains("populated area")
        );
    }

    #[test]
    fn package_gate_rejects_macro_and_active_parts_before_parser_construction() {
        for name in [
            "xl/vbaProject.bin",
            "xl/activeX/activeX1.xml",
            "xl/externalLinks/externalLink1.xml",
        ] {
            let bytes = one_entry_zip(name, b"payload", CompressionMethod::Stored);
            assert!(preflight_xlsx(&bytes).unwrap_err().contains("unsupported"));
        }
    }

    #[test]
    fn canonical_root_rejects_an_alternate_workbook_before_calamine_can_follow_it() {
        let root = br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="alternate/workbook.xml"/>
</Relationships>"#;
        let workbook = br#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"/>"#;
        let hostile_strings = br#"<?xml version="1.0" encoding="UTF-8"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" uniqueCount="999999"/>"#;
        let package = rewrite_package(
            include_bytes!("../tests/fixtures/creator_workbook_golden.xlsx"),
            BTreeMap::from([(ROOT_RELATIONSHIPS_PART, root.to_vec())]),
            vec![
                ("alternate/workbook.xml", workbook.to_vec()),
                ("alternate/sharedStrings.xml", hostile_strings.to_vec()),
            ],
        );
        assert!(
            preflight_xlsx(&package)
                .unwrap_err()
                .contains("exactly xl/workbook.xml")
        );

        let duplicate_root = String::from_utf8(root.to_vec())
            .unwrap()
            .replace(
                "Target=\"alternate/workbook.xml\"/>",
                "Target=\"xl/workbook.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>",
            );
        let package = rewrite_package(
            include_bytes!("../tests/fixtures/creator_workbook_golden.xlsx"),
            BTreeMap::from([(ROOT_RELATIONSHIPS_PART, duplicate_root.into_bytes())]),
            Vec::new(),
        );
        assert!(
            preflight_xlsx(&package)
                .unwrap_err()
                .contains("exactly one officeDocument root")
        );

        let golden = include_bytes!("../tests/fixtures/creator_workbook_golden.xlsx");
        let workbook_rels = zip_text(golden, "xl/_rels/workbook.xml.rels").replace(
            "</Relationships>",
            "<Relationship Id=\"rIdShared\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings\" Target=\"alternate/sharedStrings.xml\"/></Relationships>",
        );
        let content_types = zip_text(golden, CONTENT_TYPES_PART).replace(
            "</Types>",
            "<Override PartName=\"/xl/alternate/sharedStrings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml\"/></Types>",
        );
        let package = rewrite_package(
            golden,
            BTreeMap::from([
                ("xl/_rels/workbook.xml.rels", workbook_rels.into_bytes()),
                (CONTENT_TYPES_PART, content_types.into_bytes()),
            ]),
            vec![("xl/alternate/sharedStrings.xml", hostile_strings.to_vec())],
        );
        assert!(
            preflight_xlsx(&package)
                .unwrap_err()
                .contains("too many shared strings")
        );
    }

    #[test]
    fn renamed_payload_is_rejected_by_content_and_relationship_types() {
        let golden = include_bytes!("../tests/fixtures/creator_workbook_golden.xlsx");
        let content_types = zip_text(golden, CONTENT_TYPES_PART);
        let active_content_types = content_types.replace(
            "</Types>",
            "<Override PartName=\"/xl/custom/payload.dat\" ContentType=\"application/vnd.ms-office.activeX+xml\"/></Types>",
        );
        let package = rewrite_package(
            golden,
            BTreeMap::from([(CONTENT_TYPES_PART, active_content_types.into_bytes())]),
            vec![("xl/custom/payload.dat", b"renamed ActiveX".to_vec())],
        );
        assert!(
            preflight_xlsx(&package)
                .unwrap_err()
                .contains("forbidden active content type")
        );

        let safe_content_types = content_types.replace(
            "</Types>",
            "<Override PartName=\"/xl/custom/payload.dat\" ContentType=\"application/octet-stream\"/></Types>",
        );
        let workbook_rels = zip_text(golden, "xl/_rels/workbook.xml.rels").replace(
            "</Relationships>",
            "<Relationship Id=\"rIdActive\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/activeXControl\" Target=\"custom/payload.dat\"/></Relationships>",
        );
        let package = rewrite_package(
            golden,
            BTreeMap::from([
                (CONTENT_TYPES_PART, safe_content_types.into_bytes()),
                ("xl/_rels/workbook.xml.rels", workbook_rels.into_bytes()),
            ]),
            vec![("xl/custom/payload.dat", b"renamed ActiveX".to_vec())],
        );
        assert!(
            preflight_xlsx(&package)
                .unwrap_err()
                .contains("forbidden active relationship type")
        );

        let control_content_types = content_types.replace(
            "</Types>",
            "<Override PartName=\"/xl/custom/control.dat\" ContentType=\"application/octet-stream\"/></Types>",
        );
        let control_relationships = zip_text(golden, "xl/_rels/workbook.xml.rels").replace(
            "</Relationships>",
            "<Relationship Id=\"rIdCtrl\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/CtrlProp\" Target=\"custom/control.dat\"/></Relationships>",
        );
        let package = rewrite_package(
            golden,
            BTreeMap::from([
                (CONTENT_TYPES_PART, control_content_types.into_bytes()),
                (
                    "xl/_rels/workbook.xml.rels",
                    control_relationships.into_bytes(),
                ),
            ]),
            vec![(
                "xl/custom/control.dat",
                b"renamed control properties".to_vec(),
            )],
        );
        assert!(
            preflight_xlsx(&package)
                .unwrap_err()
                .contains("forbidden active relationship type")
        );

        let exact_control_content_types = content_types.replace(
            "</Types>",
            "<Override PartName=\"/xl/custom/control.dat\" ContentType=\"application/vnd.ms-excel.controlproperties+xml\"/></Types>",
        );
        let package = rewrite_package(
            golden,
            BTreeMap::from([(CONTENT_TYPES_PART, exact_control_content_types.into_bytes())]),
            vec![(
                "xl/custom/control.dat",
                b"renamed control properties".to_vec(),
            )],
        );
        assert!(
            preflight_xlsx(&package)
                .unwrap_err()
                .contains("forbidden active content type")
        );
    }

    #[test]
    fn filter_database_accepts_xml_schema_true_without_relaxing_defined_names() {
        let golden = include_bytes!("../tests/fixtures/creator_workbook_golden.xlsx");
        let workbook =
            zip_text(golden, CANONICAL_WORKBOOK_PART).replace("hidden=\"1\"", "hidden=\"true\"");
        assert!(workbook.contains("hidden=\"true\""));
        let package = rewrite_package(
            golden,
            BTreeMap::from([(CANONICAL_WORKBOOK_PART, workbook.into_bytes())]),
            Vec::new(),
        );
        assert!(preflight_xlsx(&package).is_ok());
        assert!(crate::CreatorBundle::from_xlsx_bytes(&package).is_ok());
    }

    #[test]
    fn complete_package_rejects_duplicate_header_data_rows_and_cells() {
        let golden = include_bytes!("../tests/fixtures/creator_workbook_golden.xlsx");
        let sheet = zip_text(golden, "xl/worksheets/sheet1.xml");
        let duplicate_header = sheet.replacen("<sheetData>", "<sheetData><row r=\"1\"/>", 1);
        let package = rewrite_package(
            golden,
            BTreeMap::from([("xl/worksheets/sheet1.xml", duplicate_header.into_bytes())]),
            Vec::new(),
        );
        assert!(
            preflight_xlsx(&package)
                .unwrap_err()
                .contains("duplicates a row reference")
        );

        let duplicate_data = sheet.replacen("</sheetData>", "<row r=\"2\"/></sheetData>", 1);
        let package = rewrite_package(
            golden,
            BTreeMap::from([("xl/worksheets/sheet1.xml", duplicate_data.into_bytes())]),
            Vec::new(),
        );
        assert!(
            preflight_xlsx(&package)
                .unwrap_err()
                .contains("duplicates a row reference")
        );

        let duplicate_cell =
            sheet.replacen("</row></sheetData>", "<c r=\"B2\"/></row></sheetData>", 1);
        let package = rewrite_package(
            golden,
            BTreeMap::from([("xl/worksheets/sheet1.xml", duplicate_cell.into_bytes())]),
            Vec::new(),
        );
        assert!(
            preflight_xlsx(&package)
                .unwrap_err()
                .contains("duplicates a cell reference")
        );
    }

    fn one_entry_zip(name: &str, content: &[u8], method: CompressionMethod) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut archive = ZipWriter::new(cursor);
        archive
            .start_file(
                name,
                SimpleFileOptions::default().compression_method(method),
            )
            .unwrap();
        archive.write_all(content).unwrap();
        archive.finish().unwrap().into_inner()
    }

    fn zip_text(bytes: &[u8], name: &str) -> String {
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut content = String::new();
        archive
            .by_name(name)
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        content
    }

    fn rewrite_package(
        original: &[u8],
        mut replacements: BTreeMap<&str, Vec<u8>>,
        additions: Vec<(&str, Vec<u8>)>,
    ) -> Vec<u8> {
        let mut input = ZipArchive::new(Cursor::new(original)).unwrap();
        let mut output = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for index in 0..input.len() {
            let mut entry = input.by_index(index).unwrap();
            let name = entry.name().to_string();
            let mut content = Vec::new();
            entry.read_to_end(&mut content).unwrap();
            if let Some(replacement) = replacements.remove(name.as_str()) {
                content = replacement;
            }
            output.start_file(name, options).unwrap();
            output.write_all(&content).unwrap();
        }
        assert!(replacements.is_empty());
        for (name, content) in additions {
            output.start_file(name, options).unwrap();
            output.write_all(&content).unwrap();
        }
        output.finish().unwrap().into_inner()
    }
}
