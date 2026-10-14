use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
};

use calamine::{DataRef, Reader, Xlsx};
use rust_xlsxwriter::{
    Color, DataValidation, DataValidationRule, DocProperties, ExcelDateTime, Format, FormatAlign,
    FormatBorder, FormatUnderline, ProtectionOptions, Workbook, Worksheet,
};

use crate::{
    CREATOR_BUNDLE_MAX_ACCOUNTS_PER_CREATOR, CREATOR_BUNDLE_MAX_CREATORS, ContactChannel,
    CreatorBundle, CreatorTransferAccount, CreatorTransferConfidence, CreatorTransferContactRoute,
    CreatorTransferCreator, CreatorTransferLinkState, CreatorTransferPlatform,
    CreatorTransferProfile, CreatorTransferRelationship, CreatorTransferStage,
    CreatorTransferTargetKind, CreatorWorkbookDisplay, CreatorWorkbookExport, LocatorKind,
    SourceKind,
};

use crate::creator_transfer_xlsx_security::{
    XLSX_MAX_COMPRESSED_BYTES, preflight_xlsx, validate_xlsx_tabular_shape,
};

const CREATORS_SHEET: &str = "Creators";
const ACCOUNTS_SHEET: &str = "Accounts";
const CONTACTS_SHEET: &str = "Contacts";
mod recommendation;

const CREATORS_HEADERS_V1: [&str; 15] = [
    "creator_ref",
    "Creator",
    "初次收录",
    "建联方式",
    "X 粉丝",
    "Instagram 粉丝",
    "Telegram 粉丝",
    "YouTube 粉丝",
    "评分",
    "阶段",
    "置顶",
    "最近触达",
    "更新时间",
    "头像 URL",
    "分组",
];
// v2 appends a column, retaining every established workbook column position.
const CREATORS_HEADERS_V2: [&str; 16] = [
    "creator_ref",
    "Creator",
    "初次收录",
    "建联方式",
    "X 粉丝",
    "Instagram 粉丝",
    "Telegram 粉丝",
    "YouTube 粉丝",
    "评分",
    "阶段",
    "置顶",
    "最近触达",
    "更新时间",
    "头像 URL",
    "分组",
    "TikTok 粉丝",
];
const CREATORS_HEADERS_V3: [&str; 19] = [
    "creator_ref",
    "Creator",
    "初次收录",
    "建联方式",
    "X 粉丝",
    "Instagram 粉丝",
    "Telegram 粉丝",
    "YouTube 粉丝",
    "评分",
    "阶段",
    "置顶",
    "最近触达",
    "更新时间",
    "头像 URL",
    "分组",
    "TikTok 粉丝",
    "标签 1",
    "标签 2",
    "标签 3",
];
const CREATORS_HEADERS: [&str; 20] = [
    "creator_ref", "Creator", "初次收录", "建联方式", "X 粉丝", "Instagram 粉丝", "Telegram 粉丝", "YouTube 粉丝",
    "评分", "阶段", "置顶", "最近触达", "更新时间", "头像 URL", "分组", "TikTok 粉丝", "标签 1", "标签 2", "标签 3", "备注",
];
const ACCOUNTS_HEADERS: [&str; 13] = [
    "creator_ref",
    "Creator",
    "platform",
    "target_kind",
    "external_id",
    "handle",
    "profile display name",
    "profile URL",
    "avatar URL",
    "link state",
    "confidence",
    "source kind",
    "capture order",
];
const CONTACTS_HEADERS: [&str; 9] = [
    "creator_ref",
    "Creator",
    "save order",
    "channel",
    "locator kind",
    "display value",
    "normalized value",
    "confidence",
    "source kinds",
];

#[derive(Debug, Clone, PartialEq)]
enum CellValue {
    Empty,
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    DateTime,
}

type SheetRows = BTreeMap<u32, BTreeMap<u32, CellValue>>;

#[derive(Default)]
struct WorkbookCreatorBuilder {
    creator: Option<WorkbookCreatorRow>,
    accounts: Vec<CreatorTransferAccount>,
    contacts: Vec<CreatorTransferContactRoute>,
}

struct WorkbookCreatorRow {
    notes: Option<String>,
    tags: Vec<String>,
    display_name: String,
    score: Option<u8>,
    followers: BTreeMap<CreatorTransferPlatform, u64>,
    stage: CreatorTransferStage,
    pinned: bool,
}

impl CreatorBundle {
    /// Deterministic, human-facing workbook. Canonical JSON remains the sync
    /// and snapshot wire format; no JSON is stored in workbook cells.
    pub fn to_xlsx_bytes(&self) -> Result<Vec<u8>, String> {
        encode_workbook(self, &BTreeMap::new())
    }

    pub fn from_xlsx_bytes(bytes: &[u8]) -> Result<Self, String> {
        decode_workbook(bytes)
    }
}

impl CreatorWorkbookExport {
    pub fn to_xlsx_bytes(&self) -> Result<Vec<u8>, String> {
        encode_workbook(&self.bundle, &self.display_by_record_key)
    }
}

fn encode_workbook(
    bundle: &CreatorBundle,
    display_by_record_key: &BTreeMap<String, CreatorWorkbookDisplay>,
) -> Result<Vec<u8>, String> {
    bundle.to_json_bytes()?;
    let account_rows = bundle.creators.iter().try_fold(0usize, |count, creator| {
        count
            .checked_add(creator.accounts.len())
            .ok_or_else(|| "Creator XLSX account row count cannot be represented".to_string())
    })?;
    let contact_rows = bundle.creators.iter().try_fold(0usize, |count, creator| {
        count
            .checked_add(creator.contact_routes.len())
            .ok_or_else(|| "Creator XLSX contact row count cannot be represented".to_string())
    })?;
    validate_xlsx_tabular_shape(
        [bundle.creators.len(), account_rows, contact_rows],
        [
            CREATORS_HEADERS.len(),
            ACCOUNTS_HEADERS.len(),
            CONTACTS_HEADERS.len(),
        ],
    )?;
    let mut workbook = Workbook::new();
    let created = ExcelDateTime::from_ymd(2000, 1, 1)
        .and_then(|value| value.and_hms(0, 0, 0))
        .map_err(|_| "Creator XLSX fixed creation time is invalid".to_string())?;
    workbook.set_properties(
        &DocProperties::new()
            .set_title("Reacher Creators")
            .set_author("Reacher")
            .set_creation_datetime(&created),
    );

    write_creators_sheet(&mut workbook, bundle, display_by_record_key)?;
    write_accounts_sheet(&mut workbook, bundle)?;
    write_contacts_sheet(&mut workbook, bundle)?;
    let mut output = Cursor::new(Vec::new());
    workbook
        .save_to_writer(&mut output)
        .map_err(|_| "Creator XLSX cannot be encoded".to_string())?;
    let bytes = output.into_inner();
    if bytes.len() > XLSX_MAX_COMPRESSED_BYTES {
        return Err("Creator XLSX exceeds the 32 MiB compressed file limit".into());
    }
    Ok(bytes)
}

fn decode_workbook(bytes: &[u8]) -> Result<CreatorBundle, String> {
    let preflight = preflight_xlsx(bytes)?;
    if preflight.sheet_names == ["推荐"] { return recommendation::decode(bytes, preflight.parts); }
    let required = [CREATORS_SHEET, ACCOUNTS_SHEET, CONTACTS_SHEET];
    if preflight.sheet_names.as_slice() != required {
        return Err(
            "Creator XLSX must contain exactly Creators, Accounts, Contacts in that order".into(),
        );
    }
    // Calamine 0.36.1 dense Range APIs are intentionally not used. The
    // independent ZIP/OOXML preflight runs first, then every sheet is read
    // through its bounded streaming cell reader with formula metadata.
    let mut workbook = Xlsx::new(Cursor::new(bytes))
        .map_err(|_| "Creator XLSX workbook metadata cannot be read".to_string())?;
    let creators = stream_sheet(&mut workbook, CREATORS_SHEET)?;
    let accounts = stream_sheet(&mut workbook, ACCOUNTS_SHEET)?;
    let contacts = stream_sheet(&mut workbook, CONTACTS_SHEET)?;
    parse_workbook(creators, accounts, contacts)
}

fn write_creators_sheet(
    workbook: &mut Workbook,
    bundle: &CreatorBundle,
    display_by_record_key: &BTreeMap<String, CreatorWorkbookDisplay>,
) -> Result<(), String> {
    let header = header_format();
    let editable = editable_format();
    let editable_number = editable.clone().set_num_format("#,##0");
    let readonly = readonly_format();
    let readonly_wrap = readonly.clone().set_text_wrap();
    let readonly_date = readonly.clone().set_num_format("yyyy-mm-dd hh:mm");
    let readonly_link = readonly
        .clone()
        .set_font_color("4F46E5")
        .set_underline(FormatUnderline::Single);
    let worksheet = workbook.add_worksheet_with_constant_memory();
    worksheet
        .set_name(CREATORS_SHEET)
        .map_err(xlsx_write_error)?;
    worksheet.set_active(true).set_selected(true);
    worksheet.set_row_height(0, 26).map_err(xlsx_write_error)?;
    write_headers(worksheet, &CREATORS_HEADERS, &header)?;
    for (index, creator) in bundle.creators.iter().enumerate() {
        let row = row_number(index)?;
        let creator_ref = creator_ref(index);
        write_text(worksheet, row, 0, &creator_ref, &readonly)?;
        write_text(worksheet, row, 1, &creator.display_name, &editable)?;
        write_optional_text(
            worksheet,
            row,
            2,
            first_captured_display(creator).as_deref(),
            &readonly,
        )?;
        write_optional_text(
            worksheet,
            row,
            3,
            contact_channels_display(creator).as_deref(),
            &readonly_wrap,
        )?;
        for (offset, platform) in [
            CreatorTransferPlatform::X,
            CreatorTransferPlatform::Instagram,
            CreatorTransferPlatform::Telegram,
            CreatorTransferPlatform::YouTube,
        ]
        .into_iter()
        .enumerate()
        {
            write_optional_integer(
                worksheet,
                row,
                4 + offset as u16,
                creator.follower_counts.get(&platform).copied(),
                &editable_number,
            )?;
        }
        write_optional_integer(
            worksheet,
            row,
            8,
            creator.score.map(u64::from),
            &editable_number,
        )?;
        write_text(
            worksheet,
            row,
            9,
            creator.relationship.stage.as_str(),
            &editable,
        )?;
        worksheet
            .write_boolean_with_format(row, 10, creator.pinned, &editable)
            .map_err(xlsx_write_error)?;
        write_optional_integer(
            worksheet,
            row,
            15,
            creator
                .follower_counts
                .get(&CreatorTransferPlatform::TikTok)
                .copied(),
            &editable_number,
        )?;
        for index in 0..3 {
            write_optional_text(
                worksheet,
                row,
                16 + index as u16,
                creator.tags.get(index).map(String::as_str),
                &editable,
            )?;
        }
        write_optional_text(worksheet, row, 19, creator.notes.as_deref(), &editable)?;
        let display = display_by_record_key.get(&creator.record_key);
        write_optional_datetime(
            worksheet,
            row,
            11,
            display.and_then(|value| value.last_touch_at),
            &readonly_date,
        )?;
        write_optional_datetime(
            worksheet,
            row,
            12,
            display.map(|value| value.updated_at),
            &readonly_date,
        )?;
        let avatar = creator_avatar_url(creator);
        write_optional_url(
            worksheet,
            row,
            13,
            avatar.as_deref(),
            &readonly,
            &readonly_link,
        )?;
        write_optional_text(
            worksheet,
            row,
            14,
            display.and_then(|value| value.group_name.as_deref()),
            &readonly,
        )?;
    }
    if !bundle.creators.is_empty() {
        let last_row = u32::try_from(bundle.creators.len())
            .map_err(|_| "Creator XLSX row count cannot be represented".to_string())?;
        let stage_validation = DataValidation::new()
            .allow_list_strings(&["prospect", "engaged", "collaborating", "archived"])
            .map_err(xlsx_write_error)?;
        worksheet
            .add_data_validation(1, 9, last_row, 9, &stage_validation)
            .map_err(xlsx_write_error)?;
        let score_validation =
            DataValidation::new().allow_whole_number(DataValidationRule::Between(0, 100));
        worksheet
            .add_data_validation(1, 8, last_row, 8, &score_validation)
            .map_err(xlsx_write_error)?;
    }
    finish_sheet(worksheet, bundle.creators.len(), CREATORS_HEADERS.len(), 2)?;
    worksheet.set_column_hidden(0).map_err(xlsx_write_error)?;
    for (column, width) in [
        (1, 32.0),
        (2, 28.0),
        (3, 22.0),
        (4, 13.0),
        (5, 16.0),
        (6, 15.0),
        (7, 14.0),
        (8, 9.0),
        (9, 14.0),
        (10, 9.0),
        (11, 20.0),
        (12, 20.0),
        (13, 52.0),
        (14, 22.0),
        (15, 15.0),
        (16, 26.0),
        (17, 26.0),
        (18, 26.0),
    ] {
        worksheet
            .set_column_width(column, width)
            .map_err(xlsx_write_error)?;
    }
    worksheet.protect();
    Ok(())
}

fn write_accounts_sheet(workbook: &mut Workbook, bundle: &CreatorBundle) -> Result<(), String> {
    let header = header_format();
    let body = editable_format();
    let text_id = body.clone().set_num_format("@");
    let readonly = readonly_format();
    let readonly_text = readonly.clone().set_num_format("@");
    let link = body
        .clone()
        .set_font_color("4F46E5")
        .set_underline(FormatUnderline::Single);
    let worksheet = if bundle
        .creators
        .iter()
        .any(|creator| !creator.accounts.is_empty())
    {
        workbook.add_worksheet_with_constant_memory()
    } else {
        workbook.add_worksheet()
    };
    worksheet
        .set_name(ACCOUNTS_SHEET)
        .map_err(xlsx_write_error)?;
    worksheet
        .set_column_format(0, &readonly_text)
        .map_err(xlsx_write_error)?;
    worksheet
        .set_column_format(1, &readonly)
        .map_err(xlsx_write_error)?;
    for column in 2..=12 {
        worksheet
            .set_column_format(column, &body)
            .map_err(xlsx_write_error)?;
    }
    worksheet.set_row_height(0, 26).map_err(xlsx_write_error)?;
    write_headers(worksheet, &ACCOUNTS_HEADERS, &header)?;
    let mut output_row = 1u32;
    for (creator_index, creator) in bundle.creators.iter().enumerate() {
        let reference = creator_ref(creator_index);
        for account in &creator.accounts {
            write_text(worksheet, output_row, 0, &reference, &readonly_text)?;
            write_text(worksheet, output_row, 1, &creator.display_name, &readonly)?;
            write_text(worksheet, output_row, 2, account.platform.as_str(), &body)?;
            write_text(
                worksheet,
                output_row,
                3,
                account.target_kind.as_str(),
                &body,
            )?;
            write_text(worksheet, output_row, 4, &account.external_id, &text_id)?;
            let profile = account.latest_profile.as_ref();
            write_optional_text(
                worksheet,
                output_row,
                5,
                profile.and_then(|value| value.handle.as_deref()),
                &body,
            )?;
            write_optional_text(
                worksheet,
                output_row,
                6,
                profile.and_then(|value| value.display_name.as_deref()),
                &body,
            )?;
            write_optional_url(
                worksheet,
                output_row,
                7,
                profile.and_then(|value| value.profile_url.as_deref()),
                &body,
                &link,
            )?;
            write_optional_url(
                worksheet,
                output_row,
                8,
                profile.and_then(|value| value.avatar_url.as_deref()),
                &body,
                &link,
            )?;
            write_text(worksheet, output_row, 9, account.link_state.as_str(), &body)?;
            write_text(
                worksheet,
                output_row,
                10,
                account.confidence.as_str(),
                &body,
            )?;
            write_text(
                worksheet,
                output_row,
                11,
                account.source_kind.as_str(),
                &body,
            )?;
            worksheet
                .write_number_with_format(output_row, 12, f64::from(account.capture_order), &body)
                .map_err(xlsx_write_error)?;
            output_row += 1;
        }
    }
    finish_sheet(
        worksheet,
        output_row.saturating_sub(1) as usize,
        ACCOUNTS_HEADERS.len(),
        2,
    )?;
    worksheet.set_column_hidden(0).map_err(xlsx_write_error)?;
    for (column, width) in [
        (1, 30.0),
        (2, 12.0),
        (3, 15.0),
        (4, 30.0),
        (5, 28.0),
        (6, 30.0),
        (7, 52.0),
        (8, 52.0),
        (9, 16.0),
        (10, 15.0),
        (11, 28.0),
        (12, 14.0),
    ] {
        worksheet
            .set_column_width(column, width)
            .map_err(xlsx_write_error)?;
    }
    protect_relational_sheet(worksheet);
    Ok(())
}

fn write_contacts_sheet(workbook: &mut Workbook, bundle: &CreatorBundle) -> Result<(), String> {
    let header = header_format();
    let body = editable_format();
    let readonly = readonly_format();
    let readonly_text = readonly.clone().set_num_format("@");
    let wrap = body.clone().set_text_wrap();
    let worksheet = if bundle
        .creators
        .iter()
        .any(|creator| !creator.contact_routes.is_empty())
    {
        workbook.add_worksheet_with_constant_memory()
    } else {
        workbook.add_worksheet()
    };
    worksheet
        .set_name(CONTACTS_SHEET)
        .map_err(xlsx_write_error)?;
    worksheet
        .set_column_format(0, &readonly_text)
        .map_err(xlsx_write_error)?;
    worksheet
        .set_column_format(1, &readonly)
        .map_err(xlsx_write_error)?;
    for column in 2..=8 {
        worksheet
            .set_column_format(column, &body)
            .map_err(xlsx_write_error)?;
    }
    worksheet.set_row_height(0, 26).map_err(xlsx_write_error)?;
    write_headers(worksheet, &CONTACTS_HEADERS, &header)?;
    let mut output_row = 1u32;
    for (creator_index, creator) in bundle.creators.iter().enumerate() {
        let reference = creator_ref(creator_index);
        for route in &creator.contact_routes {
            write_text(worksheet, output_row, 0, &reference, &readonly_text)?;
            write_text(worksheet, output_row, 1, &creator.display_name, &readonly)?;
            worksheet
                .write_number_with_format(output_row, 2, f64::from(route.save_order), &body)
                .map_err(xlsx_write_error)?;
            write_text(worksheet, output_row, 3, route.channel.as_str(), &body)?;
            write_text(worksheet, output_row, 4, route.locator_kind.as_str(), &body)?;
            write_text(worksheet, output_row, 5, &route.display_value, &body)?;
            write_text(worksheet, output_row, 6, &route.normalized_value, &body)?;
            write_text(worksheet, output_row, 7, route.confidence.as_str(), &body)?;
            let sources = route
                .source_kinds
                .iter()
                .map(|source| source.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            write_optional_text(
                worksheet,
                output_row,
                8,
                (!sources.is_empty()).then_some(sources.as_str()),
                &wrap,
            )?;
            output_row += 1;
        }
    }
    finish_sheet(
        worksheet,
        output_row.saturating_sub(1) as usize,
        CONTACTS_HEADERS.len(),
        2,
    )?;
    worksheet.set_column_hidden(0).map_err(xlsx_write_error)?;
    for (column, width) in [
        (1, 30.0),
        (2, 12.0),
        (3, 18.0),
        (4, 18.0),
        (5, 44.0),
        (6, 44.0),
        (7, 13.0),
        (8, 34.0),
    ] {
        worksheet
            .set_column_width(column, width)
            .map_err(xlsx_write_error)?;
    }
    protect_relational_sheet(worksheet);
    Ok(())
}

fn protect_relational_sheet(worksheet: &mut Worksheet) {
    worksheet.protect_with_options(&ProtectionOptions {
        insert_rows: true,
        delete_rows: true,
        sort: true,
        use_autofilter: true,
        ..ProtectionOptions::default()
    });
}

fn finish_sheet(
    worksheet: &mut Worksheet,
    data_rows: usize,
    columns: usize,
    frozen_columns: u16,
) -> Result<(), String> {
    let last_row =
        u32::try_from(data_rows).map_err(|_| "Creator XLSX row count cannot be represented")?;
    let last_col = u16::try_from(columns.saturating_sub(1))
        .map_err(|_| "Creator XLSX column count cannot be represented")?;
    worksheet
        .autofilter(0, 0, last_row, last_col)
        .map_err(xlsx_write_error)?;
    worksheet
        .set_freeze_panes(1, frozen_columns)
        .map_err(xlsx_write_error)?;
    Ok(())
}

fn write_headers<const N: usize>(
    worksheet: &mut Worksheet,
    headers: &[&str; N],
    format: &Format,
) -> Result<(), String> {
    for (column, header) in headers.iter().enumerate() {
        worksheet
            .write_string_with_format(0, column as u16, *header, format)
            .map_err(xlsx_write_error)?;
    }
    Ok(())
}

fn header_format() -> Format {
    Format::new()
        .set_bold()
        .set_font_color(Color::White)
        .set_background_color("1F2937")
        .set_border_bottom(FormatBorder::Thin)
        .set_align(FormatAlign::VerticalCenter)
}

fn editable_format() -> Format {
    Format::new()
        .set_unlocked()
        .set_align(FormatAlign::VerticalCenter)
}

fn readonly_format() -> Format {
    Format::new()
        .set_background_color("F3F4F6")
        .set_font_color("6B7280")
        .set_align(FormatAlign::VerticalCenter)
}

fn write_text(
    worksheet: &mut Worksheet,
    row: u32,
    column: u16,
    value: &str,
    format: &Format,
) -> Result<(), String> {
    worksheet
        .write_string_with_format(row, column, value, format)
        .map_err(xlsx_write_error)?;
    Ok(())
}

fn write_optional_text(
    worksheet: &mut Worksheet,
    row: u32,
    column: u16,
    value: Option<&str>,
    format: &Format,
) -> Result<(), String> {
    match value {
        Some(value) => write_text(worksheet, row, column, value, format),
        None => {
            worksheet
                .write_blank(row, column, format)
                .map_err(xlsx_write_error)?;
            Ok(())
        }
    }
}

fn write_optional_integer(
    worksheet: &mut Worksheet,
    row: u32,
    column: u16,
    value: Option<u64>,
    format: &Format,
) -> Result<(), String> {
    match value {
        Some(value) => {
            worksheet
                .write_number_with_format(row, column, value as f64, format)
                .map_err(xlsx_write_error)?;
            Ok(())
        }
        None => {
            worksheet
                .write_blank(row, column, format)
                .map_err(xlsx_write_error)?;
            Ok(())
        }
    }
}

fn write_optional_datetime(
    worksheet: &mut Worksheet,
    row: u32,
    column: u16,
    value: Option<u64>,
    format: &Format,
) -> Result<(), String> {
    match value {
        Some(value) => {
            let timestamp = i64::try_from(value)
                .map_err(|_| "Creator XLSX timestamp cannot be represented".to_string())?;
            let datetime = ExcelDateTime::from_timestamp(timestamp).map_err(|_| {
                "Creator XLSX timestamp is outside the Excel date range".to_string()
            })?;
            worksheet
                .write_datetime_with_format(row, column, &datetime, format)
                .map_err(xlsx_write_error)?;
            Ok(())
        }
        None => {
            worksheet
                .write_blank(row, column, format)
                .map_err(xlsx_write_error)?;
            Ok(())
        }
    }
}

fn write_optional_url(
    worksheet: &mut Worksheet,
    row: u32,
    column: u16,
    value: Option<&str>,
    fallback_format: &Format,
    link_format: &Format,
) -> Result<(), String> {
    let Some(value) = value else {
        worksheet
            .write_blank(row, column, fallback_format)
            .map_err(xlsx_write_error)?;
        return Ok(());
    };
    if valid_web_url(value) {
        worksheet
            .write_url_with_options(row, column, value, value, "", Some(link_format))
            .map_err(xlsx_write_error)?;
    } else {
        write_text(worksheet, row, column, value, fallback_format)?;
    }
    Ok(())
}

fn valid_web_url(value: &str) -> bool {
    url::Url::parse(value)
        .ok()
        .is_some_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
}

fn first_captured_display(creator: &CreatorTransferCreator) -> Option<String> {
    creator
        .accounts
        .iter()
        .filter(|account| account.link_state != CreatorTransferLinkState::Rejected)
        .min_by_key(|account| account.capture_order)
        .map(|account| {
            let identity = account
                .latest_profile
                .as_ref()
                .and_then(|profile| profile.handle.as_deref())
                .filter(|handle| !handle.trim().is_empty())
                .map(|handle| format!("@{}", handle.trim().trim_start_matches('@')))
                .unwrap_or_else(|| account.external_id.clone());
            format!("{} · {identity}", account.platform.as_str())
        })
}

fn contact_channels_display(creator: &CreatorTransferCreator) -> Option<String> {
    let mut seen = BTreeSet::new();
    let channels = creator
        .contact_routes
        .iter()
        .filter_map(|route| seen.insert(route.channel).then_some(route.channel.as_str()))
        .collect::<Vec<_>>()
        .join("\n");
    (!channels.is_empty()).then_some(channels)
}

fn creator_avatar_url(creator: &CreatorTransferCreator) -> Option<String> {
    creator
        .accounts
        .iter()
        .filter(|account| account.link_state != CreatorTransferLinkState::Rejected)
        .filter_map(|account| {
            account
                .latest_profile
                .as_ref()
                .and_then(|profile| profile.avatar_url.clone())
                .map(|avatar| (account.capture_order, avatar))
        })
        .min_by_key(|(capture_order, _)| *capture_order)
        .map(|(_, avatar)| avatar)
}

fn creator_ref(index: usize) -> String {
    format!("creator_{index:06}")
}

fn row_number(index: usize) -> Result<u32, String> {
    u32::try_from(index)
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| "Creator XLSX row cannot be represented".to_string())
}

fn xlsx_write_error(_: rust_xlsxwriter::XlsxError) -> String {
    "Creator XLSX cannot be encoded".into()
}

fn stream_sheet(workbook: &mut Xlsx<Cursor<&[u8]>>, name: &str) -> Result<SheetRows, String> {
    let mut reader = workbook
        .worksheet_cells_reader(name)
        .map_err(|_| format!("Creator XLSX sheet {name} cannot be streamed"))?;
    let mut rows = SheetRows::new();
    let mut cell_count = 0usize;
    while let Some(cell) = reader
        .next_cell_with_formula_metadata()
        .map_err(|_| format!("Creator XLSX sheet {name} contains an unreadable cell"))?
    {
        if cell.formula.is_some() {
            return Err(format!("Creator XLSX sheet {name} contains a formula"));
        }
        cell_count += 1;
        if cell_count > 1_000_000 {
            return Err("Creator XLSX exceeds the populated cell limit".into());
        }
        let value = owned_cell(cell.value)?;
        if !matches!(value, CellValue::Empty) {
            rows.entry(cell.pos.0)
                .or_default()
                .insert(cell.pos.1, value);
        }
    }
    Ok(rows)
}

fn owned_cell(value: DataRef<'_>) -> Result<CellValue, String> {
    match value {
        DataRef::Empty => Ok(CellValue::Empty),
        DataRef::String(value) => Ok(CellValue::String(value)),
        DataRef::SharedString(value) => Ok(CellValue::String(value.to_owned())),
        DataRef::Int(value) => Ok(CellValue::Int(value)),
        DataRef::Float(value) if value.is_finite() => Ok(CellValue::Float(value)),
        DataRef::Bool(value) => Ok(CellValue::Bool(value)),
        DataRef::DateTime(_) | DataRef::DateTimeIso(_) => Ok(CellValue::DateTime),
        DataRef::DurationIso(_) => Err("Creator XLSX duration cells are unsupported".into()),
        DataRef::Error(_) => Err("Creator XLSX error cells are unsupported".into()),
        DataRef::Float(_) => Err("Creator XLSX non-finite numbers are unsupported".into()),
    }
}

fn parse_workbook(
    mut creators_rows: SheetRows,
    mut accounts_rows: SheetRows,
    mut contacts_rows: SheetRows,
) -> Result<CreatorBundle, String> {
    let header_row = creators_rows.remove(&0);
    let is_v4 = header_row.as_ref().is_some_and(|row| row.values().any(|cell| matches!(cell, CellValue::String(value) if value == "备注")));
    let is_v3 = header_row.as_ref().is_some_and(|row| {
        row.values()
            .any(|cell| matches!(cell, CellValue::String(value) if value == "标签 1"))
    });
    let is_v2 = header_row.as_ref().is_some_and(|row| {
        row.values()
            .any(|cell| matches!(cell, CellValue::String(value) if value == "TikTok 粉丝"))
    });
    let creators_header = if is_v4 {
        parse_headers(CREATORS_SHEET, header_row, &CREATORS_HEADERS)?
    } else if is_v3 {
        parse_headers(CREATORS_SHEET, header_row, &CREATORS_HEADERS_V3)?
    } else if is_v2 {
        parse_headers(CREATORS_SHEET, header_row, &CREATORS_HEADERS_V2)?
    } else {
        parse_headers(CREATORS_SHEET, header_row, &CREATORS_HEADERS_V1)?
    };
    let accounts_header =
        parse_headers(ACCOUNTS_SHEET, accounts_rows.remove(&0), &ACCOUNTS_HEADERS)?;
    let contacts_header =
        parse_headers(CONTACTS_SHEET, contacts_rows.remove(&0), &CONTACTS_HEADERS)?;

    let mut builders = BTreeMap::<String, WorkbookCreatorBuilder>::new();
    for (row_number, row) in creators_rows {
        reject_columns_after_header(CREATORS_SHEET, row_number, &row, &creators_header)?;
        let reference = required_string(
            CREATORS_SHEET,
            row_number,
            &row,
            &creators_header,
            "creator_ref",
        )?;
        validate_creator_ref(&reference)?;
        if builders.contains_key(&reference) {
            return Err("Creator XLSX duplicates a creator_ref".into());
        }
        let display_name = required_string(
            CREATORS_SHEET,
            row_number,
            &row,
            &creators_header,
            "Creator",
        )?;
        let score = optional_u64(CREATORS_SHEET, row_number, &row, &creators_header, "评分")?
            .map(|value| {
                u8::try_from(value).map_err(|_| "Creator XLSX score must be 0-100".to_string())
            })
            .transpose()?;
        if score.is_some_and(|value| value > 100) {
            return Err("Creator XLSX score must be 0-100".into());
        }
        let mut followers = BTreeMap::new();
        for (header, platform) in [
            ("X 粉丝", CreatorTransferPlatform::X),
            ("Instagram 粉丝", CreatorTransferPlatform::Instagram),
            ("Telegram 粉丝", CreatorTransferPlatform::Telegram),
            ("YouTube 粉丝", CreatorTransferPlatform::YouTube),
        ] {
            if let Some(value) =
                optional_u64(CREATORS_SHEET, row_number, &row, &creators_header, header)?
            {
                followers.insert(platform, value);
            }
        }
        if is_v2
            && let Some(count) = optional_u64(
                CREATORS_SHEET,
                row_number,
                &row,
                &creators_header,
                "TikTok 粉丝",
            )?
        {
            followers.insert(CreatorTransferPlatform::TikTok, count);
        }
        let stage = parse_stage(&required_string(
            CREATORS_SHEET,
            row_number,
            &row,
            &creators_header,
            "阶段",
        )?)?;
        let pinned = required_bool(CREATORS_SHEET, row_number, &row, &creators_header, "置顶")?;
        validate_readonly_creator_cells(row_number, &row, &creators_header)?;
        builders.insert(
            reference,
            WorkbookCreatorBuilder {
                creator: Some(WorkbookCreatorRow {
                    notes: if is_v4 { optional_string(CREATORS_SHEET, row_number, &row, &creators_header, "备注")? } else { None },
                    tags: if is_v3 {
                        crate::creators::normalize_creator_tags(
                            ["标签 1", "标签 2", "标签 3"]
                                .into_iter()
                                .map(|header| {
                                    optional_string(
                                        CREATORS_SHEET,
                                        row_number,
                                        &row,
                                        &creators_header,
                                        header,
                                    )
                                })
                                .collect::<Result<Vec<_>, _>>()?
                                .into_iter()
                                .flatten()
                                .collect(),
                        )
                        .map_err(|error| error.to_string())?
                    } else {
                        Vec::new()
                    },
                    display_name,
                    score,
                    followers,
                    stage,
                    pinned,
                }),
                accounts: Vec::new(),
                contacts: Vec::new(),
            },
        );
    }
    if builders.len() > CREATOR_BUNDLE_MAX_CREATORS {
        return Err(format!(
            "Creator XLSX exceeds the maximum of {CREATOR_BUNDLE_MAX_CREATORS} Creators"
        ));
    }

    let mut account_orders = BTreeSet::new();
    for (row_number, row) in accounts_rows {
        reject_columns_after_header(ACCOUNTS_SHEET, row_number, &row, &accounts_header)?;
        let reference = required_string(
            ACCOUNTS_SHEET,
            row_number,
            &row,
            &accounts_header,
            "creator_ref",
        )?;
        let builder = builders
            .get_mut(&reference)
            .ok_or_else(|| "Creator XLSX Accounts row has an orphan creator_ref".to_string())?;
        let _creator_name = required_string(
            ACCOUNTS_SHEET,
            row_number,
            &row,
            &accounts_header,
            "Creator",
        )?;
        let capture_order = required_u32(
            ACCOUNTS_SHEET,
            row_number,
            &row,
            &accounts_header,
            "capture order",
        )?;
        if !account_orders.insert((reference.clone(), capture_order)) {
            return Err("Creator XLSX duplicates an account capture order".into());
        }
        if builder.accounts.len() >= CREATOR_BUNDLE_MAX_ACCOUNTS_PER_CREATOR {
            return Err(format!(
                "Creator XLSX Creator exceeds the {CREATOR_BUNDLE_MAX_ACCOUNTS_PER_CREATOR} account limit"
            ));
        }
        let handle = optional_string(ACCOUNTS_SHEET, row_number, &row, &accounts_header, "handle")?;
        let profile_name = optional_string(
            ACCOUNTS_SHEET,
            row_number,
            &row,
            &accounts_header,
            "profile display name",
        )?;
        let profile_url = optional_string(
            ACCOUNTS_SHEET,
            row_number,
            &row,
            &accounts_header,
            "profile URL",
        )?;
        let avatar_url = optional_string(
            ACCOUNTS_SHEET,
            row_number,
            &row,
            &accounts_header,
            "avatar URL",
        )?;
        let latest_profile = (handle.is_some()
            || profile_name.is_some()
            || profile_url.is_some()
            || avatar_url.is_some())
        .then_some(CreatorTransferProfile {
            handle,
            display_name: profile_name,
            profile_url,
            avatar_url,
        });
        builder.accounts.push(CreatorTransferAccount {
            capture_order,
            platform: parse_platform(&required_string(
                ACCOUNTS_SHEET,
                row_number,
                &row,
                &accounts_header,
                "platform",
            )?)?,
            target_kind: parse_target_kind(&required_string(
                ACCOUNTS_SHEET,
                row_number,
                &row,
                &accounts_header,
                "target_kind",
            )?)?,
            external_id: required_string(
                ACCOUNTS_SHEET,
                row_number,
                &row,
                &accounts_header,
                "external_id",
            )?,
            link_state: parse_link_state(&required_string(
                ACCOUNTS_SHEET,
                row_number,
                &row,
                &accounts_header,
                "link state",
            )?)?,
            confidence: parse_confidence(&required_string(
                ACCOUNTS_SHEET,
                row_number,
                &row,
                &accounts_header,
                "confidence",
            )?)?,
            source_kind: SourceKind::parse(&required_string(
                ACCOUNTS_SHEET,
                row_number,
                &row,
                &accounts_header,
                "source kind",
            )?)?,
            latest_profile,
        });
    }

    let mut contact_orders = BTreeSet::new();
    for (row_number, row) in contacts_rows {
        reject_columns_after_header(CONTACTS_SHEET, row_number, &row, &contacts_header)?;
        let reference = required_string(
            CONTACTS_SHEET,
            row_number,
            &row,
            &contacts_header,
            "creator_ref",
        )?;
        let builder = builders
            .get_mut(&reference)
            .ok_or_else(|| "Creator XLSX Contacts row has an orphan creator_ref".to_string())?;
        let _creator_name = required_string(
            CONTACTS_SHEET,
            row_number,
            &row,
            &contacts_header,
            "Creator",
        )?;
        let save_order = required_u32(
            CONTACTS_SHEET,
            row_number,
            &row,
            &contacts_header,
            "save order",
        )?;
        if !contact_orders.insert((reference.clone(), save_order)) {
            return Err("Creator XLSX duplicates a contact save order".into());
        }
        let source_kinds = optional_string(
            CONTACTS_SHEET,
            row_number,
            &row,
            &contacts_header,
            "source kinds",
        )?
        .map(|value| parse_source_kinds(&value))
        .transpose()?
        .unwrap_or_default();
        builder.contacts.push(CreatorTransferContactRoute {
            save_order,
            channel: ContactChannel::parse(&required_string(
                CONTACTS_SHEET,
                row_number,
                &row,
                &contacts_header,
                "channel",
            )?)?,
            locator_kind: LocatorKind::parse(&required_string(
                CONTACTS_SHEET,
                row_number,
                &row,
                &contacts_header,
                "locator kind",
            )?)?,
            display_value: required_string(
                CONTACTS_SHEET,
                row_number,
                &row,
                &contacts_header,
                "display value",
            )?,
            normalized_value: required_string(
                CONTACTS_SHEET,
                row_number,
                &row,
                &contacts_header,
                "normalized value",
            )?,
            confidence: parse_confidence(&required_string(
                CONTACTS_SHEET,
                row_number,
                &row,
                &contacts_header,
                "confidence",
            )?)?,
            source_kinds,
        });
    }

    let creators = builders
        .into_values()
        .map(|mut builder| {
            let creator = builder
                .creator
                .take()
                .ok_or_else(|| "Creator XLSX child row is missing its Creator row".to_string())?;
            Ok(CreatorTransferCreator {
                notes: creator.notes,
                semantic_sha256: String::new(),
                record_key: String::new(),
                display_name: creator.display_name,
                tags: creator.tags,
                score: creator.score,
                follower_counts: creator.followers,
                pinned: creator.pinned,
                relationship: CreatorTransferRelationship {
                    stage: creator.stage,
                },
                accounts: builder.accounts,
                contact_routes: builder.contacts,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    if !is_v2 {
        crate::creator_transfer::reject_v1_tiktok(&creators)?;
    }
    crate::creator_transfer::build_creator_bundle(creators)
}

fn parse_headers<const N: usize>(
    sheet: &str,
    row: Option<BTreeMap<u32, CellValue>>,
    expected: &[&str; N],
) -> Result<BTreeMap<String, u32>, String> {
    let row = row.ok_or_else(|| format!("Creator XLSX sheet {sheet} is missing its header"))?;
    let expected_set = expected.iter().copied().collect::<BTreeSet<_>>();
    let mut result = BTreeMap::new();
    for (column, value) in row {
        let CellValue::String(header) = value else {
            return Err(format!(
                "Creator XLSX sheet {sheet} header cells must be text"
            ));
        };
        if !expected_set.contains(header.as_str()) {
            return Err(format!(
                "Creator XLSX sheet {sheet} contains unknown header {header}"
            ));
        }
        if result.insert(header, column).is_some() {
            return Err(format!("Creator XLSX sheet {sheet} duplicates a header"));
        }
    }
    for header in expected {
        if !result.contains_key(*header) {
            return Err(format!(
                "Creator XLSX sheet {sheet} is missing required header {header}"
            ));
        }
    }
    Ok(result)
}

fn reject_columns_after_header(
    sheet: &str,
    row_number: u32,
    row: &BTreeMap<u32, CellValue>,
    headers: &BTreeMap<String, u32>,
) -> Result<(), String> {
    let valid = headers.values().copied().collect::<BTreeSet<_>>();
    if row.keys().any(|column| !valid.contains(column)) {
        return Err(format!(
            "Creator XLSX sheet {sheet} row {} contains data outside known columns",
            row_number + 1
        ));
    }
    Ok(())
}

fn cell<'a>(
    row: &'a BTreeMap<u32, CellValue>,
    headers: &BTreeMap<String, u32>,
    header: &str,
) -> Option<&'a CellValue> {
    headers.get(header).and_then(|column| row.get(column))
}

fn required_string(
    sheet: &str,
    row_number: u32,
    row: &BTreeMap<u32, CellValue>,
    headers: &BTreeMap<String, u32>,
    header: &str,
) -> Result<String, String> {
    optional_string(sheet, row_number, row, headers, header)?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!(
                "Creator XLSX sheet {sheet} row {} requires text in {header}",
                row_number + 1
            )
        })
}

fn optional_string(
    sheet: &str,
    row_number: u32,
    row: &BTreeMap<u32, CellValue>,
    headers: &BTreeMap<String, u32>,
    header: &str,
) -> Result<Option<String>, String> {
    match cell(row, headers, header) {
        None | Some(CellValue::Empty) => Ok(None),
        Some(CellValue::String(value)) => Ok(Some(value.clone())),
        _ => Err(format!(
            "Creator XLSX sheet {sheet} row {} requires a text cell in {header}",
            row_number + 1
        )),
    }
}

fn optional_u64(
    sheet: &str,
    row_number: u32,
    row: &BTreeMap<u32, CellValue>,
    headers: &BTreeMap<String, u32>,
    header: &str,
) -> Result<Option<u64>, String> {
    let value = match cell(row, headers, header) {
        None | Some(CellValue::Empty) => return Ok(None),
        Some(CellValue::Int(value)) if *value >= 0 => u64::try_from(*value).ok(),
        Some(CellValue::Float(value))
            if *value >= 0.0 && value.fract() == 0.0 && *value <= u64::MAX as f64 =>
        {
            Some(*value as u64)
        }
        _ => None,
    };
    value.map(Some).ok_or_else(|| {
        format!(
            "Creator XLSX sheet {sheet} row {} requires a non-negative integer in {header}",
            row_number + 1
        )
    })
}

fn required_u32(
    sheet: &str,
    row_number: u32,
    row: &BTreeMap<u32, CellValue>,
    headers: &BTreeMap<String, u32>,
    header: &str,
) -> Result<u32, String> {
    let value = optional_u64(sheet, row_number, row, headers, header)?.ok_or_else(|| {
        format!(
            "Creator XLSX sheet {sheet} row {} requires {header}",
            row_number + 1
        )
    })?;
    u32::try_from(value).map_err(|_| {
        format!(
            "Creator XLSX sheet {sheet} row {} {header} overflows u32",
            row_number + 1
        )
    })
}

fn required_bool(
    sheet: &str,
    row_number: u32,
    row: &BTreeMap<u32, CellValue>,
    headers: &BTreeMap<String, u32>,
    header: &str,
) -> Result<bool, String> {
    match cell(row, headers, header) {
        Some(CellValue::Bool(value)) => Ok(*value),
        _ => Err(format!(
            "Creator XLSX sheet {sheet} row {} requires a boolean in {header}",
            row_number + 1
        )),
    }
}

fn validate_readonly_creator_cells(
    row_number: u32,
    row: &BTreeMap<u32, CellValue>,
    headers: &BTreeMap<String, u32>,
) -> Result<(), String> {
    for header in ["初次收录", "建联方式", "头像 URL", "分组"] {
        optional_string(CREATORS_SHEET, row_number, row, headers, header)?;
    }
    for header in ["最近触达", "更新时间"] {
        if !matches!(
            cell(row, headers, header),
            None | Some(CellValue::Empty | CellValue::DateTime)
        ) {
            return Err(format!(
                "Creator XLSX sheet Creators row {} requires a date cell in {header}",
                row_number + 1
            ));
        }
    }
    Ok(())
}

fn validate_creator_ref(value: &str) -> Result<(), String> {
    if value.len() > 64
        || value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err("Creator XLSX creator_ref is invalid".into());
    }
    Ok(())
}

fn parse_source_kinds(value: &str) -> Result<Vec<SourceKind>, String> {
    if value.contains('\r') {
        return Err("Creator XLSX source kinds must use one LF-separated value per line".into());
    }
    value
        .split('\n')
        .map(|line| {
            if line.is_empty() || line.trim() != line {
                Err("Creator XLSX source kinds contain a blank or padded line".into())
            } else {
                SourceKind::parse(line)
            }
        })
        .collect()
}

fn parse_platform(value: &str) -> Result<CreatorTransferPlatform, String> {
    match value {
        "x" => Ok(CreatorTransferPlatform::X),
        "instagram" => Ok(CreatorTransferPlatform::Instagram),
        "telegram" => Ok(CreatorTransferPlatform::Telegram),
        "youtube" => Ok(CreatorTransferPlatform::YouTube),
        "tiktok" => Ok(CreatorTransferPlatform::TikTok),
        _ => Err(format!("unknown CreatorTransferPlatform: {value}")),
    }
}

fn parse_stage(value: &str) -> Result<CreatorTransferStage, String> {
    match value {
        "prospect" => Ok(CreatorTransferStage::Prospect),
        "engaged" => Ok(CreatorTransferStage::Engaged),
        "collaborating" => Ok(CreatorTransferStage::Collaborating),
        "archived" => Ok(CreatorTransferStage::Archived),
        _ => Err(format!("unknown CreatorTransferStage: {value}")),
    }
}

fn parse_target_kind(value: &str) -> Result<CreatorTransferTargetKind, String> {
    match value {
        "user" => Ok(CreatorTransferTargetKind::User),
        "bot" => Ok(CreatorTransferTargetKind::Bot),
        "channel" => Ok(CreatorTransferTargetKind::Channel),
        "supergroup" => Ok(CreatorTransferTargetKind::Supergroup),
        "group" => Ok(CreatorTransferTargetKind::Group),
        "creator" => Ok(CreatorTransferTargetKind::Creator),
        "business" => Ok(CreatorTransferTargetKind::Business),
        _ => Err(format!("unknown CreatorTransferTargetKind: {value}")),
    }
}

fn parse_link_state(value: &str) -> Result<CreatorTransferLinkState, String> {
    match value {
        "claimed" => Ok(CreatorTransferLinkState::Claimed),
        "confirmed" => Ok(CreatorTransferLinkState::Confirmed),
        "rejected" => Ok(CreatorTransferLinkState::Rejected),
        _ => Err(format!("unknown CreatorTransferLinkState: {value}")),
    }
}

fn parse_confidence(value: &str) -> Result<CreatorTransferConfidence, String> {
    match value {
        "exact" => Ok(CreatorTransferConfidence::Exact),
        "asserted" => Ok(CreatorTransferConfidence::Asserted),
        "possible" => Ok(CreatorTransferConfidence::Possible),
        _ => Err(format!("unknown CreatorTransferConfidence: {value}")),
    }
}

#[cfg(test)]
pub(crate) fn rewrite_xlsx_part_for_test(
    bytes: &[u8],
    part_name: &str,
    rewrite: impl FnOnce(String) -> String,
) -> Vec<u8> {
    use std::io::{Read, Write};

    use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

    let mut input = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut output = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let mut rewrite = Some(rewrite);
    for index in 0..input.len() {
        let mut entry = input.by_index(index).unwrap();
        let name = entry.name().to_string();
        let mut content = Vec::new();
        entry.read_to_end(&mut content).unwrap();
        if name == part_name {
            let source = String::from_utf8(content).unwrap();
            content = rewrite.take().unwrap()(source).into_bytes();
        }
        output.start_file(name, options).unwrap();
        output.write_all(&content).unwrap();
    }
    assert!(rewrite.is_none(), "test XLSX part was not found");
    output.finish().unwrap().into_inner()
}

#[cfg(test)]
pub(crate) fn mutate_readonly_creator_cells_for_test(bytes: &[u8]) -> Vec<u8> {
    rewrite_xlsx_part_for_test(bytes, "xl/worksheets/sheet1.xml", |mut sheet| {
        sheet = sheet.replace("本地分组", "ignored local group");
        sheet = sheet.replace("Restore Group", "ignored restored group");
        sheet = sheet.replace(
            "https://example.com/avatar.png",
            "https://ignored.example/avatar.png",
        );
        for reference in ["L2", "M2"] {
            remove_xml_cell(&mut sheet, reference);
        }
        sheet
    })
}

#[cfg(test)]
fn remove_xml_cell(sheet: &mut String, reference: &str) {
    let marker = format!("<c r=\"{reference}\"");
    let start = sheet.find(&marker).unwrap();
    let relative_end = sheet[start..].find("</c>").unwrap();
    let end = start + relative_end + "</c>".len();
    sheet.replace_range(start..end, "");
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use sha2::Digest;

    use super::*;

    fn fixture() -> CreatorBundle {
        crate::creator_transfer::build_creator_bundle(vec![CreatorTransferCreator {
            notes: None,
            tags: Vec::new(),
            semantic_sha256: String::new(),
            record_key: String::new(),
            display_name: "示例, Creator 🚀".into(),
            score: Some(91),
            follower_counts: BTreeMap::from([(CreatorTransferPlatform::X, 53_000)]),
            pinned: true,
            relationship: CreatorTransferRelationship {
                stage: CreatorTransferStage::Engaged,
            },
            accounts: vec![CreatorTransferAccount {
                capture_order: 7,
                platform: CreatorTransferPlatform::X,
                target_kind: CreatorTransferTargetKind::User,
                external_id: "00012345678901234567890".into(),
                link_state: CreatorTransferLinkState::Confirmed,
                confidence: CreatorTransferConfidence::Exact,
                source_kind: SourceKind::InitialPlatformAccount,
                latest_profile: Some(CreatorTransferProfile {
                    handle: Some("=not-a-formula".into()),
                    display_name: Some("名称\n第二行".into()),
                    profile_url: Some("https://example.com/user".into()),
                    avatar_url: Some("https://example.com/avatar.png".into()),
                }),
            }],
            contact_routes: vec![CreatorTransferContactRoute {
                save_order: 3,
                channel: ContactChannel::Email,
                locator_kind: LocatorKind::EmailAddress,
                normalized_value: "hello@example.com".into(),
                display_value: "+safe@example.com".into(),
                confidence: CreatorTransferConfidence::Asserted,
                source_kinds: vec![SourceKind::ProfileBio, SourceKind::ProfileBio],
            }],
        }])
        .unwrap()
    }

    fn fixture_workbook() -> CreatorWorkbookExport {
        let bundle = fixture();
        let record_key = bundle.creators[0].record_key.clone();
        CreatorWorkbookExport {
            bundle,
            display_by_record_key: BTreeMap::from([(
                record_key,
                CreatorWorkbookDisplay {
                    last_touch_at: Some(1_700_000_000),
                    updated_at: 1_700_000_100,
                    group_name: Some("本地分组".into()),
                },
            )]),
        }
    }

    #[test]
    fn tiktok_v2_workbook_import_and_v3_round_trip_preserve_followers_and_identity() {
        let mut workbook = fixture_workbook();
        let creator = &mut workbook.bundle.creators[0];
        creator.accounts[0].platform = CreatorTransferPlatform::TikTok;
        creator.accounts[0].external_id = "tto:123456".into();
        creator
            .follower_counts
            .insert(CreatorTransferPlatform::TikTok, 98765);
        workbook.bundle =
            crate::creator_transfer::build_creator_bundle(workbook.bundle.creators).unwrap();
        let encoded = workbook.to_xlsx_bytes().unwrap();
        assert_eq!(
            CreatorBundle::from_xlsx_bytes(&encoded).unwrap(),
            workbook.bundle
        );
        let legacy =
            rewrite_xlsx_part_for_test(&encoded, "xl/worksheets/sheet1.xml", |mut sheet| {
                // Reconstruct the released sixteen-column sheet without tag cells.
                for reference in ["Q1", "R1", "S1", "T1", "Q2", "R2", "S2"] {
                    let start = sheet.find(&format!("<c r=\"{reference}\"")).unwrap();
                    let open_end = start + sheet[start..].find('>').unwrap();
                    let end = if sheet.as_bytes()[open_end - 1] == b'/' {
                        open_end + 1
                    } else {
                        start + sheet[start..].find("</c>").unwrap() + 4
                    };
                    sheet.replace_range(start..end, "");
                }
                sheet.replace("A1:T2", "A1:P2")
            });
        assert_eq!(
            CreatorBundle::from_xlsx_bytes(&legacy).unwrap(),
            workbook.bundle
        );
    }

    #[test]
    fn deterministic_workbook_stream_round_trip_preserves_canonical_bundle() {
        let workbook = fixture_workbook();
        let bundle = workbook.bundle.clone();
        let golden_bundle = CreatorBundle::from_import_json_bytes(include_bytes!(
            "../tests/fixtures/creator_workbook_golden.json"
        ))
        .unwrap();
        assert_eq!(bundle, golden_bundle);
        let first = workbook.to_xlsx_bytes().unwrap();
        let second = workbook.to_xlsx_bytes().unwrap();
        assert_eq!(
            format!(
                "{:x}",
                sha2::Sha256::digest(include_bytes!(
                    "../tests/fixtures/creator_workbook_golden.xlsx"
                ))
            ),
            "11e1e4066a67d945663ee5a93ad5cb50fe6145fe902ae637cd70e37a99dc833f"
        );
        assert_eq!(
            CreatorBundle::from_xlsx_bytes(include_bytes!(
                "../tests/fixtures/creator_workbook_golden.xlsx"
            ))
            .unwrap(),
            golden_bundle
        );
        assert_eq!(first, second);
        assert_eq!(
            CreatorBundle::from_xlsx_bytes(&first).unwrap(),
            golden_bundle
        );
        let accounts = zip_part(&first, "xl/worksheets/sheet2.xml");
        assert!(accounts.contains("=not-a-formula"));
        assert!(!accounts.contains("<f>"));
        let contacts = zip_part(&first, "xl/worksheets/sheet3.xml");
        assert!(contacts.contains("+safe@example.com"));
    }

    #[test]
    fn local_annotations_change_artifact_bytes_but_never_the_imported_bundle_or_hash() {
        let bundle = fixture();
        let record_key = bundle.creators[0].record_key.clone();
        let first = CreatorWorkbookExport {
            bundle: bundle.clone(),
            display_by_record_key: BTreeMap::from([(
                record_key.clone(),
                CreatorWorkbookDisplay {
                    last_touch_at: Some(1_700_000_000),
                    updated_at: 1_700_000_100,
                    group_name: Some("本地分组".into()),
                },
            )]),
        }
        .to_xlsx_bytes()
        .unwrap();
        let annotated_sheet = zip_part(&first, "xl/worksheets/sheet1.xml");
        assert!(annotated_sheet.contains("r=\"L2\""));
        assert!(annotated_sheet.contains("r=\"M2\""));
        assert!(annotated_sheet.contains("本地分组"));
        assert!(zip_part(&first, "xl/styles.xml").contains("yyyy-mm-dd hh:mm"));
        let edited = mutate_readonly_creator_cells_for_test(&first);
        assert_ne!(first, edited);
        assert_eq!(CreatorBundle::from_xlsx_bytes(&first).unwrap(), bundle);
        assert_eq!(CreatorBundle::from_xlsx_bytes(&edited).unwrap(), bundle);
        assert_eq!(
            CreatorBundle::from_xlsx_bytes(&edited)
                .unwrap()
                .content_sha256,
            bundle.content_sha256
        );
    }

    #[test]
    fn editing_only_the_main_creator_name_keeps_existing_child_rows_valid() {
        let source = fixture_workbook().to_xlsx_bytes().unwrap();
        let edited = rewrite_xlsx_part_for_test(&source, "xl/worksheets/sheet1.xml", |sheet| {
            sheet.replace("示例, Creator 🚀", "只改主表名称")
        });
        let bundle = CreatorBundle::from_xlsx_bytes(&edited).unwrap();
        assert_eq!(bundle.creators[0].display_name, "只改主表名称");
        assert_eq!(bundle.creators[0].accounts.len(), 1);
        assert_eq!(bundle.creators[0].contact_routes.len(), 1);
    }

    #[test]
    fn workbook_contains_only_three_relational_sheets_and_no_formula_cells() {
        let bytes = fixture().to_xlsx_bytes().unwrap();
        let preflight = preflight_xlsx(&bytes).unwrap();
        assert_eq!(
            preflight.sheet_names,
            [CREATORS_SHEET, ACCOUNTS_SHEET, CONTACTS_SHEET]
        );
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains("row_kind"));
        assert!(!text.contains("<f>"));
    }

    #[test]
    fn parser_source_uses_only_calamine_streaming_cell_api() {
        let source = include_str!("creator_transfer_xlsx.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(production.contains("worksheet_cells_reader"));
        assert!(production.contains("next_cell_with_formula_metadata"));
        for forbidden in [
            "worksheet_range(",
            "worksheet_range_ref(",
            "worksheets(",
            "worksheet_formula(",
            "table_by_name(",
        ] {
            assert!(
                !production.contains(forbidden),
                "dense Calamine API found: {forbidden}"
            );
        }
    }

    #[test]
    fn exported_workbook_freezes_filters_hides_refs_and_protects_readonly_cells() {
        let bytes = fixture().to_xlsx_bytes().unwrap();
        let creators = zip_part(&bytes, "xl/worksheets/sheet1.xml");
        assert!(creators.contains("<pane xSplit=\"2\" ySplit=\"1\""));
        assert!(creators.contains("state=\"frozen\""));
        assert!(creators.contains("<autoFilter ref=\"A1:T2\""));
        let legacy = zip_part(
            include_bytes!("../tests/fixtures/creator_workbook_golden.xlsx"),
            "xl/worksheets/sheet1.xml",
        );
        assert!(legacy.contains("<autoFilter ref=\"A1:O2\""));
        assert!(creators.contains("<sheetProtection"));
        assert!(creators.contains("hidden=\"1\""));
        assert!(creators.contains("tabSelected=\"1\""));
        assert!(creators.contains("<dataValidations"));
        let style = |reference: &str| {
            creators
                .split(&format!("r=\"{reference}\""))
                .nth(1)
                .unwrap()
                .split("s=\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .to_owned()
        };
        for reference in ["Q2", "R2", "S2"] {
            assert_eq!(
                style(reference),
                style("B2"),
                "blank tag cells must remain editable"
            );
        }
        let accounts = zip_part(&bytes, "xl/worksheets/sheet2.xml");
        assert!(accounts.contains("00012345678901234567890"));
        assert!(accounts.contains("t=\"inlineStr\""));
        assert!(accounts.contains("<sheetProtection"));
        let contacts = zip_part(&bytes, "xl/worksheets/sheet3.xml");
        assert!(contacts.contains("<sheetProtection"));
    }

    #[test]
    fn reordered_headers_are_accepted_but_unknown_duplicate_and_formula_cells_are_rejected() {
        let mut reversed = CREATORS_HEADERS;
        reversed.reverse();
        let reordered = custom_workbook(&reversed, CreatorCellOverride::None, false);
        let bundle = CreatorBundle::from_xlsx_bytes(&reordered).unwrap();
        assert_eq!(bundle.creator_count, 1);
        assert_eq!(bundle.creators[0].display_name, "Header order");

        let mut unknown = CREATORS_HEADERS;
        unknown[2] = "unexpected";
        assert!(
            CreatorBundle::from_xlsx_bytes(&custom_workbook(
                &unknown,
                CreatorCellOverride::None,
                false,
            ))
            .unwrap_err()
            .contains("unknown header")
        );

        let mut duplicate = CREATORS_HEADERS;
        duplicate[2] = "Creator";
        assert!(
            CreatorBundle::from_xlsx_bytes(&custom_workbook(
                &duplicate,
                CreatorCellOverride::None,
                false,
            ))
            .unwrap_err()
            .contains("duplicates a header")
        );

        assert!(
            CreatorBundle::from_xlsx_bytes(&custom_workbook(
                &CREATORS_HEADERS,
                CreatorCellOverride::FormulaName,
                false,
            ))
            .unwrap_err()
            .contains("formulas are unsupported")
        );

        for (override_kind, expected) in [
            (CreatorCellOverride::MinusName, "-literal"),
            (CreatorCellOverride::AtName, "@literal"),
        ] {
            let bundle = CreatorBundle::from_xlsx_bytes(&custom_workbook(
                &CREATORS_HEADERS,
                override_kind,
                false,
            ))
            .unwrap();
            assert_eq!(bundle.creators[0].display_name, expected);
        }
    }

    #[test]
    fn wrong_creator_cell_type_and_orphan_account_are_rejected() {
        assert!(
            CreatorBundle::from_xlsx_bytes(&custom_workbook(
                &CREATORS_HEADERS,
                CreatorCellOverride::NumericName,
                false,
            ))
            .unwrap_err()
            .contains("requires a text cell")
        );
        assert!(
            CreatorBundle::from_xlsx_bytes(&custom_workbook(
                &CREATORS_HEADERS,
                CreatorCellOverride::None,
                true,
            ))
            .unwrap_err()
            .contains("orphan creator_ref")
        );
    }

    #[derive(Clone, Copy)]
    enum CreatorCellOverride {
        None,
        FormulaName,
        NumericName,
        MinusName,
        AtName,
    }

    fn custom_workbook(
        creators_headers: &[&str],
        creator_override: CreatorCellOverride,
        orphan_account: bool,
    ) -> Vec<u8> {
        let mut workbook = Workbook::new();
        let creators = workbook.add_worksheet();
        creators.set_name(CREATORS_SHEET).unwrap();
        for (column, header) in creators_headers.iter().enumerate() {
            creators.write_string(0, column as u16, *header).unwrap();
            match *header {
                "creator_ref" => {
                    creators
                        .write_string(1, column as u16, "creator_000000")
                        .unwrap();
                }
                "Creator" => match creator_override {
                    CreatorCellOverride::None => {
                        creators
                            .write_string(1, column as u16, "Header order")
                            .unwrap();
                    }
                    CreatorCellOverride::FormulaName => {
                        creators.write_formula(1, column as u16, "=1+1").unwrap();
                    }
                    CreatorCellOverride::NumericName => {
                        creators.write_number(1, column as u16, 7).unwrap();
                    }
                    CreatorCellOverride::MinusName => {
                        creators.write_string(1, column as u16, "-literal").unwrap();
                    }
                    CreatorCellOverride::AtName => {
                        creators.write_string(1, column as u16, "@literal").unwrap();
                    }
                },
                "阶段" => {
                    creators.write_string(1, column as u16, "prospect").unwrap();
                }
                "置顶" => {
                    creators.write_boolean(1, column as u16, false).unwrap();
                }
                _ => {}
            }
        }
        let accounts = workbook.add_worksheet();
        accounts.set_name(ACCOUNTS_SHEET).unwrap();
        for (column, header) in ACCOUNTS_HEADERS.iter().enumerate() {
            accounts.write_string(0, column as u16, *header).unwrap();
        }
        if orphan_account {
            for (column, value) in [
                "creator_missing",
                "Missing",
                "x",
                "user",
                "01234567890123456789",
                "",
                "",
                "",
                "",
                "confirmed",
                "exact",
                "initial_platform_account",
            ]
            .into_iter()
            .enumerate()
            {
                if !value.is_empty() {
                    accounts.write_string(1, column as u16, value).unwrap();
                }
            }
            accounts.write_number(1, 12, 0).unwrap();
        }
        let contacts = workbook.add_worksheet();
        contacts.set_name(CONTACTS_SHEET).unwrap();
        for (column, header) in CONTACTS_HEADERS.iter().enumerate() {
            contacts.write_string(0, column as u16, *header).unwrap();
        }
        workbook.save_to_buffer().unwrap()
    }

    fn zip_part(bytes: &[u8], name: &str) -> String {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut part = String::new();
        archive
            .by_name(name)
            .unwrap()
            .read_to_string(&mut part)
            .unwrap();
        part
    }
}
