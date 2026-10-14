//! Exact flattening, safe fields, resource references, and bounds.
use super::call::ProjectionCall;
use onecrew_local_runtime::{
    PresentationField, PresentationItem, PresentationProjection, PresentationSection,
    ResultResourceKind, ResultResourceRef,
};
use serde_json::Value;

/// One `Fields` or `Items` section carries at most this many entries on the
/// wire, which is the result store's own `MAX_SECTION_ITEMS`.
///
/// This is a display capacity, not a safety or resource bound: the values are
/// already in memory and already returned to the caller. So an output with
/// more leaves than a section can hold is shown up to the capacity and says
/// so, rather than taking the whole card down with it.
pub(super) const MAX_PROJECTED_FIELDS: usize = 128;
const MAX_FIELD_BYTES: usize = 8 * 1024;
pub(super) const MAX_PROJECTION_BYTES: usize = 64 * 1024;
pub(super) const MAX_PROJECTION_SECTIONS: usize = 32;
const MAX_PROJECTION_DEPTH: usize = 16;
const MAX_RESOURCE_REFS: usize = 64;
/// A list row names its record in one line and details it in a few more. The
/// row is a summary; the complete record went to the caller with the result.
const MAX_ITEM_TITLE_BYTES: usize = 256;
const MAX_ITEM_DETAIL_BYTES: usize = 1024;
/// A list section stays well inside [`MAX_PROJECTION_BYTES`] so that a long
/// page shortens its own list instead of failing the projection it belongs to.
const MAX_LIST_SECTION_BYTES: usize = 48 * 1024;
/// One record is a record and keeps its complete field detail. Two or more are
/// a list, and a list is read row by row rather than leaf by leaf.
const MIN_LIST_RECORDS: usize = 2;
/// A page's records sit at the output root, under one key, or under one
/// nested key — `items`, `page.items`, or the root array itself.
const MAX_LIST_DEPTH: usize = 2;
/// How far under a record its own name may sit: at the record's own keys, or
/// one level under them — `title`, or `matched.title`.
///
/// A record that keeps what it matched in a sub-object is still naming itself.
/// A name two levels down belongs to something the record merely contains, and
/// every level further multiplies the chance of reading out an unrelated
/// `name` or `id`, so the search stops here.
const MAX_TITLE_DEPTH: usize = 1;

/// Keys a record may be named by, most specific first.
///
/// This is a closed display vocabulary in the same spirit as
/// [`resource_kind`]: it decides only what a row is called, never what the
/// row means. A record that names itself with none of them is numbered.
///
/// [`record_title`] reads the same vocabulary at every depth it searches. No
/// entry here is a path, and none belongs to one provider.
const TITLE_KEYS: &[&str] = &[
    "display_name",
    "name",
    "title",
    "subject",
    "handle",
    "username",
    "screen_name",
    "full_name",
    "text",
    "id",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::operation_registry::v2) enum ProjectionFailure {
    UnsafeField,
    LabelCollision,
    UnnameableField,
    Bounds,
    Encoding,
}

/// What a projection does when a leaf does not fit the card.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FieldBudget {
    /// A confirmation shows exactly what will be submitted, so an argument
    /// set it cannot show completely cannot be shown at all.
    Exact,
    /// An output is evidence of what already ran. Showing the first page of
    /// it and counting the rest keeps the card, and the complete result is
    /// returned to the caller either way.
    Truncating,
}

/// What a projection left out of a card, by reason.
///
/// Both are display bounds rather than losses — the complete result went back
/// with the call — and they are counted apart because they need different
/// words. A card that reports a withheld field as "128 of 240" sends the
/// reader looking for a capacity that is not what stopped it.
#[derive(Clone, Copy, Default)]
pub(super) struct ExactOmissions {
    /// Leaves past [`MAX_PROJECTED_FIELDS`].
    pub(super) truncated: usize,
    /// Keys [`safe_field_key`] does not name, counted with their subtrees.
    pub(super) withheld: usize,
}

pub(super) struct ExactProjection {
    fields: Vec<PresentationField>,
    labels: std::collections::BTreeSet<String>,
    omissions: ExactOmissions,
    budget: FieldBudget,
    max_fields: usize,
}

impl ExactProjection {
    fn new(budget: FieldBudget) -> Self {
        Self::with_field_limit(budget, MAX_PROJECTED_FIELDS)
    }

    fn with_field_limit(budget: FieldBudget, max_fields: usize) -> Self {
        Self {
            fields: Vec::new(),
            labels: std::collections::BTreeSet::new(),
            omissions: ExactOmissions::default(),
            budget,
            max_fields: max_fields.clamp(1, MAX_PROJECTED_FIELDS),
        }
    }

    pub(super) fn into_fields(self) -> Vec<PresentationField> {
        self.fields
    }

    /// The shown fields and what the section left out, by reason.
    pub(super) fn into_parts(self) -> (Vec<PresentationField>, ExactOmissions) {
        (self.fields, self.omissions)
    }
}

/// One page of records projected as rows, with whatever else the output said.
pub(super) struct ListProjection {
    /// The output's remaining leaves — an effective query, a next cursor —
    /// projected exactly, so paging a list does not lose how to continue it.
    pub(super) lead: Vec<PresentationField>,
    pub(super) items: Vec<PresentationItem>,
    /// Records the section could not hold, counted rather than dropped.
    pub(super) omitted: usize,
    /// What the lead and the shown rows left out, by reason: lead leaves past
    /// the section's capacity, and keys withheld by name anywhere in either.
    pub(super) omissions: ExactOmissions,
}

pub(super) fn exact_argument_projection(
    call: &ProjectionCall,
) -> Result<ExactProjection, ProjectionFailure> {
    flatten_exact(&Value::Object(call.arguments.clone()), FieldBudget::Exact)
}

#[cfg(test)]
pub(super) fn exact_output_projection(
    result: &Value,
) -> Result<ExactProjection, ProjectionFailure> {
    exact_output_projection_with_limit(result, MAX_PROJECTED_FIELDS)
}

pub(super) fn exact_output_projection_with_limit(
    result: &Value,
    max_fields: usize,
) -> Result<ExactProjection, ProjectionFailure> {
    flatten_exact_with_limit(result, FieldBudget::Truncating, max_fields)
}

/// Every resource reference an output names, independent of how it is shown.
///
/// Collecting references is not display: it reads the few keys
/// [`resource_kind`] recognizes and nothing else. Deriving it from the display
/// projection made a long page's references inherit a display capacity, so a
/// read that returned more leaves than one section holds produced no
/// references and no card at all. This walk cannot fail: an unsafe key's
/// subtree is skipped rather than fatal, and the reference bound stops
/// collection instead of rejecting the output.
pub(super) fn output_resource_refs(result: &Value) -> Vec<ResultResourceRef> {
    let mut resources = Vec::new();
    collect_resources(result, &mut Vec::new(), &mut resources, 0);
    resources
}

/// The output's records as rows, or `None` when it is not one page of records.
///
/// Recognition is structural and never names a key: the shallowest array of
/// two or more objects, at the root or under at most [`MAX_LIST_DEPTH`] object
/// keys, and only when that depth holds exactly one such array. An output that
/// is ambiguous at its shallowest depth keeps the exact field projection.
#[cfg(test)]
pub(super) fn output_list_projection(
    result: &Value,
) -> Result<Option<ListProjection>, ProjectionFailure> {
    output_list_projection_with_limits(
        result,
        MAX_PROJECTED_FIELDS,
        MAX_PROJECTED_FIELDS,
        MAX_ITEM_DETAIL_BYTES,
    )
}

pub(super) fn output_list_projection_with_limits(
    result: &Value,
    max_items: usize,
    max_lead_fields: usize,
    max_detail_bytes: usize,
) -> Result<Option<ListProjection>, ProjectionFailure> {
    let Some(path) = locate_list(result) else {
        return Ok(None);
    };
    let mut node = result;
    for key in &path {
        node = node
            .as_object()
            .and_then(|object| object.get(key))
            .ok_or(ProjectionFailure::Encoding)?;
    }
    let Value::Array(records) = node else {
        return Err(ProjectionFailure::Encoding);
    };
    let mut items = Vec::new();
    let mut used = 0;
    let mut withheld = 0;
    let mut display_closed = false;
    for (index, record) in records.iter().enumerate() {
        let (item, record_withheld) = record_item(record, index, max_detail_bytes)?;
        // 展示容量不能跳过后续记录的标签、编码与深度校验。
        if display_closed || items.len() >= max_items.clamp(1, MAX_PROJECTED_FIELDS) {
            continue;
        }
        let size = item.title.len() + item.description.as_ref().map_or(0, String::len);
        if !items.is_empty() && used + size > MAX_LIST_SECTION_BYTES {
            display_closed = true;
            continue;
        }
        used += size;
        withheld += record_withheld;
        items.push(item);
    }
    let (lead, mut omissions) = lead_fields_with_limit(result, &path, max_lead_fields)?;
    omissions.withheld += withheld;
    Ok(Some(ListProjection {
        lead,
        omitted: records.len() - items.len(),
        omissions,
        items,
    }))
}

#[derive(Clone)]
enum PathPart {
    Key(String),
    Index(usize),
}

fn flatten_exact(value: &Value, budget: FieldBudget) -> Result<ExactProjection, ProjectionFailure> {
    flatten_exact_with_limit(value, budget, MAX_PROJECTED_FIELDS)
}

fn flatten_exact_with_limit(
    value: &Value,
    budget: FieldBudget,
    max_fields: usize,
) -> Result<ExactProjection, ProjectionFailure> {
    let mut projection = ExactProjection::with_field_limit(budget, max_fields);
    flatten_value(value, &mut Vec::new(), &mut projection, 0)?;
    if projection.fields.is_empty() {
        return Err(ProjectionFailure::Bounds);
    }
    Ok(projection)
}

fn flatten_value(
    value: &Value,
    path: &mut Vec<PathPart>,
    projection: &mut ExactProjection,
    depth: usize,
) -> Result<(), ProjectionFailure> {
    if depth > MAX_PROJECTION_DEPTH {
        return Err(ProjectionFailure::Bounds);
    }
    match value {
        Value::Object(object) if object.is_empty() => {
            push_exact_field(path, "{}".to_owned(), projection)
        }
        Value::Object(object) => {
            let mut keys = object.keys().collect::<Vec<_>>();
            keys.sort_unstable();
            for key in keys {
                // A key this card does not name is skipped with its subtree
                // and counted, exactly as the reference walk already skips it.
                // Refusing the whole projection instead withheld nothing more
                // — the key was never going to be shown either way — it only
                // cost the reader every other field standing beside it.
                //
                // A confirmation is the exception, for the same reason it
                // refuses to truncate: it shows precisely what will be
                // submitted, so an argument set it cannot show in full is not
                // shown at all rather than shown with a hole in it.
                if !safe_field_key(key) {
                    if projection.budget == FieldBudget::Exact {
                        return Err(ProjectionFailure::UnsafeField);
                    }
                    projection.omissions.withheld += 1;
                    continue;
                }
                path.push(PathPart::Key(key.clone()));
                flatten_value(&object[key], path, projection, depth + 1)?;
                path.pop();
            }
            Ok(())
        }
        Value::Array(values) if values.is_empty() => {
            push_exact_field(path, "[]".to_owned(), projection)
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                path.push(PathPart::Index(index));
                flatten_value(value, path, projection, depth + 1)?;
                path.pop();
            }
            Ok(())
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
            push_exact_field(path, exact_scalar_value(value)?, projection)
        }
    }
}

fn push_exact_field(
    path: &[PathPart],
    value: String,
    projection: &mut ExactProjection,
) -> Result<(), ProjectionFailure> {
    // A value too large to show is still refused: shortening it would change
    // what the card claims the owner returned. Only the number of shown
    // leaves gives way, because that is a property of the card, not the data.
    if value.is_empty() || value.len() > MAX_FIELD_BYTES {
        return Err(ProjectionFailure::Bounds);
    }
    let label = field_label(path)?;
    if !projection.labels.insert(label.clone()) {
        return Err(ProjectionFailure::LabelCollision);
    }
    if projection.fields.len() >= projection.max_fields {
        if projection.budget == FieldBudget::Exact {
            return Err(ProjectionFailure::Bounds);
        }
        projection.omissions.truncated += 1;
        return Ok(());
    }
    projection.fields.push(PresentationField { label, value });
    Ok(())
}

fn safe_field_key(key: &str) -> bool {
    let normalized = key
        .bytes()
        .filter(|byte| byte.is_ascii_alphanumeric())
        .map(char::from)
        .flat_map(char::to_lowercase)
        .collect::<String>();
    const FORBIDDEN: &[&str] = &[
        "secret",
        "password",
        "passwd",
        "token",
        "cookie",
        "authorization",
        "apikey",
        "privatekey",
        "clientsecret",
        "rawpayload",
        "rawresponse",
        "rawrequest",
    ];
    normalized != "raw" && !FORBIDDEN.iter().any(|needle| normalized.contains(needle))
}

fn field_label(path: &[PathPart]) -> Result<String, ProjectionFailure> {
    let mut label = String::new();
    for part in path {
        match part {
            PathPart::Key(key) => {
                if !label.is_empty() {
                    label.push_str(" · ");
                }
                label.push_str(&human_key(key)?);
            }
            PathPart::Index(index) => {
                use std::fmt::Write as _;
                write!(&mut label, "[{}]", index + 1).map_err(|_| ProjectionFailure::Encoding)?;
            }
        }
    }
    if label.is_empty() {
        label.push_str("Result");
    }
    (!label.is_empty() && label.len() <= MAX_FIELD_BYTES)
        .then_some(label)
        .ok_or(ProjectionFailure::Bounds)
}

fn human_key(key: &str) -> Result<String, ProjectionFailure> {
    let words = key
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut characters = word.chars();
            let mut output = String::new();
            if let Some(first) = characters.next() {
                output.extend(first.to_uppercase());
            }
            output.extend(characters);
            output
        })
        .collect::<Vec<_>>();
    (!words.is_empty())
        .then(|| words.join(" "))
        .ok_or(ProjectionFailure::UnnameableField)
}

fn exact_scalar_value(value: &Value) -> Result<String, ProjectionFailure> {
    let value = match value {
        Value::Null => "null".to_owned(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        // An empty value would fail the non-empty check below, so it is shown
        // as `""`. Otherwise only characters a card cannot display force the
        // whole value into JSON. Line breaks and tabs are how a written body
        // is shaped and the card renders them as such, so escaping those
        // turned every multi-line message into one quoted run of `\n`.
        Value::String(value)
            if value.is_empty()
                || value.chars().any(|character| {
                    character.is_control() && !matches!(character, '\n' | '\t')
                }) =>
        {
            serde_json::to_string(value).map_err(|_| ProjectionFailure::Encoding)?
        }
        Value::String(value) => value.clone(),
        Value::Array(_) | Value::Object(_) => return Err(ProjectionFailure::Encoding),
    };
    (!value.is_empty() && value.len() <= MAX_FIELD_BYTES)
        .then_some(value)
        .ok_or(ProjectionFailure::Bounds)
}

fn collect_resources(
    value: &Value,
    path: &mut Vec<PathPart>,
    resources: &mut Vec<ResultResourceRef>,
    depth: usize,
) {
    if depth > MAX_PROJECTION_DEPTH || resources.len() >= MAX_RESOURCE_REFS {
        return;
    }
    match value {
        Value::Object(object) => {
            let mut keys = object.keys().collect::<Vec<_>>();
            keys.sort_unstable();
            for key in keys {
                // A key a field projection refuses to name is not walked for
                // references either, so nothing under it can become one.
                if !safe_field_key(key) {
                    continue;
                }
                path.push(PathPart::Key(key.clone()));
                collect_resources(&object[key], path, resources, depth + 1);
                path.pop();
            }
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                path.push(PathPart::Index(index));
                collect_resources(value, path, resources, depth + 1);
                path.pop();
            }
        }
        Value::String(text) => push_resource(path, text, resources),
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn push_resource(path: &[PathPart], value: &str, resources: &mut Vec<ResultResourceRef>) {
    let Some(PathPart::Key(key)) = path.last() else {
        return;
    };
    let Some(kind) = resource_kind(key) else {
        return;
    };
    if !valid_resource_id(value) {
        return;
    }
    if resources
        .iter()
        .any(|resource| resource.kind == kind && resource.id == value)
    {
        return;
    }
    if resources.len() >= MAX_RESOURCE_REFS {
        return;
    }
    resources.push(ResultResourceRef {
        kind,
        id: value.to_owned(),
        label: field_label(path).ok(),
    });
}

fn resource_kind(key: &str) -> Option<ResultResourceKind> {
    Some(match key {
        "creator_id" | "candidate_id" => ResultResourceKind::Creator,
        "conversation_id" | "thread_id" => ResultResourceKind::Conversation,
        "connection_id" | "operator_account_id" | "credential_id" | "mailbox_ref_id" => {
            ResultResourceKind::Connection
        }
        "external_id" | "channel_id" | "post_id" | "message_id" | "community_id" | "video_id"
        | "discovery_id" => ResultResourceKind::ProviderObject,
        _ => return None,
    })
}

fn valid_resource_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
}

fn locate_list(value: &Value) -> Option<Vec<String>> {
    if let Value::Array(values) = value
        && is_record_list(values)
    {
        return Some(Vec::new());
    }
    let object = value.as_object()?;
    let mut keys = object.keys().collect::<Vec<_>>();
    keys.sort_unstable();
    let mut direct = Vec::new();
    for key in &keys {
        if safe_field_key(key)
            && matches!(&object[key.as_str()], Value::Array(values) if is_record_list(values))
        {
            direct.push((*key).clone());
        }
    }
    // The shallowest depth decides. A page that names its records under one
    // key is not overruled by a facet list one level further in.
    if !direct.is_empty() {
        return (direct.len() == 1).then(|| vec![direct.remove(0)]);
    }
    let mut nested = Vec::new();
    for key in keys {
        if !safe_field_key(key) {
            continue;
        }
        let Some(child) = object[key].as_object() else {
            continue;
        };
        let mut child_keys = child.keys().collect::<Vec<_>>();
        child_keys.sort_unstable();
        for inner in child_keys {
            if safe_field_key(inner)
                && matches!(&child[inner.as_str()], Value::Array(values) if is_record_list(values))
            {
                nested.push(vec![key.clone(), inner.clone()]);
            }
        }
    }
    if nested.len() == 1 {
        return Some(nested.remove(0));
    }
    None
}

const _: () = assert!(
    MAX_LIST_DEPTH == 2,
    "locate_list walks the root and one nested key"
);

fn is_record_list(values: &[Value]) -> bool {
    values.len() >= MIN_LIST_RECORDS && values.iter().all(Value::is_object)
}

/// One record as a row, with the number of its keys withheld by name.
fn record_item(
    record: &Value,
    index: usize,
    max_detail_bytes: usize,
) -> Result<(PresentationItem, usize), ProjectionFailure> {
    let object = record.as_object().ok_or(ProjectionFailure::Encoding)?;
    let named = record_title(object);
    let title = match &named {
        // A row's name occupies one line, so its runs of whitespace close up.
        // Only the name is reshaped this way; every detail below keeps the
        // value the owner returned.
        Some((_, value)) => one_line(value, MAX_ITEM_TITLE_BYTES),
        None => format!("[{}]", index + 1),
    };
    let mut detail = ExactProjection::new(FieldBudget::Truncating);
    flatten_value(record, &mut Vec::new(), &mut detail, 0)?;
    // The name leaves the detail under the very label the flattening gave it,
    // at whatever depth it was found, so no row prints its own title twice.
    let skipped = named.map(|(path, _)| field_label(&path)).transpose()?;
    let description = record_detail(&detail.fields, skipped.as_deref(), max_detail_bytes);
    Ok((
        PresentationItem { title, description },
        detail.omissions.withheld,
    ))
}

/// Where a record names itself: the path to that name, and its value.
///
/// The record's own keys answer first, in [`TITLE_KEYS`] order, so a record
/// that names itself never takes its name out of something it contains. Only
/// when none of them names it does the search go one level in, bounded by
/// [`MAX_TITLE_DEPTH`].
///
/// One level in, the first [`TITLE_KEYS`] word any sub-object uses must be
/// used by exactly one of them. A hit carrying both a `matched.title` and a
/// `channel.title` is numbered instead: nothing in the shape says which of the
/// two the row is, and naming it after whichever key sorted first would be a
/// coincidence, not a reading. Numbering is what such a row did before this
/// looked any deeper, so the ambiguous case is no worse than it was.
///
/// Requiring that single match to be unique is also what keeps the result
/// independent of the order an object is iterated in.
fn record_title(object: &serde_json::Map<String, Value>) -> Option<(Vec<PathPart>, &str)> {
    for key in TITLE_KEYS {
        if let Some(value) = named_value(object, key) {
            return Some((vec![PathPart::Key((*key).to_owned())], value));
        }
    }
    // A key this card may not name is not walked for a name either, exactly as
    // the field and reference walks skip its subtree.
    let nested = object
        .iter()
        .filter(|(key, _)| safe_field_key(key))
        .filter_map(|(key, value)| Some((key, value.as_object()?)))
        .collect::<Vec<_>>();
    for key in TITLE_KEYS {
        let mut named = nested
            .iter()
            .filter_map(|&(parent, child)| Some((parent, named_value(child, key)?)));
        let Some((parent, value)) = named.next() else {
            continue;
        };
        if named.next().is_some() {
            return None;
        }
        return Some((
            vec![
                PathPart::Key(parent.clone()),
                PathPart::Key((*key).to_owned()),
            ],
            value,
        ));
    }
    None
}

const _: () = assert!(
    MAX_TITLE_DEPTH == 1,
    "record_title reads the record's own keys and one level under them"
);

/// The name a record carries under `key`, when that is a key a card may show
/// and the record filled it in with text.
fn named_value<'a>(object: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a str> {
    if !safe_field_key(key) {
        return None;
    }
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

/// The record's remaining leaves, one `Label: value` line each, bounded.
fn record_detail(
    fields: &[PresentationField],
    skipped: Option<&str>,
    max_detail_bytes: usize,
) -> Option<String> {
    let max_detail_bytes = max_detail_bytes.clamp(1, MAX_ITEM_DETAIL_BYTES);
    let mut detail = String::new();
    for field in fields {
        if skipped == Some(field.label.as_str()) {
            continue;
        }
        let line = format!("{}: {}", field.label, field.value);
        if detail.len() + line.len() + 1 > max_detail_bytes {
            detail.push('…');
            break;
        }
        if !detail.is_empty() {
            detail.push('\n');
        }
        detail.push_str(&line);
    }
    (!detail.is_empty()).then_some(detail)
}

fn one_line(value: &str, limit: usize) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.len() <= limit {
        return collapsed;
    }
    let mut end = limit;
    while !collapsed.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &collapsed[..end])
}

/// The output with its record list removed, projected exactly, and what that
/// projection left out.
fn lead_fields_with_limit(
    result: &Value,
    path: &[String],
    max_fields: usize,
) -> Result<(Vec<PresentationField>, ExactOmissions), ProjectionFailure> {
    if path.is_empty() {
        return Ok((Vec::new(), ExactOmissions::default()));
    }
    let remainder = without_list(result, path);
    let Value::Object(object) = &remainder else {
        return Ok((Vec::new(), ExactOmissions::default()));
    };
    if object.is_empty() {
        return Ok((Vec::new(), ExactOmissions::default()));
    }
    let mut projection = ExactProjection::with_field_limit(FieldBudget::Truncating, max_fields);
    flatten_value(&remainder, &mut Vec::new(), &mut projection, 0)?;
    Ok(projection.into_parts())
}

fn without_list(value: &Value, path: &[String]) -> Value {
    let Some((head, rest)) = path.split_first() else {
        return Value::Null;
    };
    let Some(object) = value.as_object() else {
        return Value::Null;
    };
    let mut pruned = serde_json::Map::new();
    for (key, value) in object {
        if key != head {
            pruned.insert(key.clone(), value.clone());
            continue;
        }
        if rest.is_empty() {
            continue;
        }
        let nested = without_list(value, rest);
        if nested.as_object().is_some_and(serde_json::Map::is_empty) {
            continue;
        }
        pruned.insert(key.clone(), nested);
    }
    Value::Object(pruned)
}

pub(super) fn ensure_projection_bounds(
    projection: &PresentationProjection,
) -> Result<(), ProjectionFailure> {
    if projection.sections.len() > MAX_PROJECTION_SECTIONS
        || projection.title.is_empty()
        || projection.title.len() > MAX_FIELD_BYTES
        || projection
            .summary
            .as_ref()
            .is_some_and(|summary| summary.is_empty() || summary.len() > MAX_FIELD_BYTES)
    {
        return Err(ProjectionFailure::Bounds);
    }
    for section in &projection.sections {
        match section {
            PresentationSection::CreatorSaveRows { rows } => {
                if rows.is_empty()
                    || rows.len() > onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS
                {
                    return Err(ProjectionFailure::Bounds);
                }
            }
            PresentationSection::Fields { heading, fields } => {
                if fields.is_empty()
                    || fields.len() > MAX_PROJECTED_FIELDS
                    || heading.as_ref().is_some_and(|heading| {
                        heading.is_empty() || heading.len() > MAX_FIELD_BYTES
                    })
                    || fields.iter().any(|field| {
                        field.label.is_empty()
                            || field.label.len() > MAX_FIELD_BYTES
                            || field.value.is_empty()
                            || field.value.len() > MAX_FIELD_BYTES
                    })
                {
                    return Err(ProjectionFailure::Bounds);
                }
            }
            PresentationSection::Items { heading, items } => {
                if items.is_empty()
                    || items.len() > MAX_PROJECTED_FIELDS
                    || heading.as_ref().is_some_and(|heading| {
                        heading.is_empty() || heading.len() > MAX_FIELD_BYTES
                    })
                    // The store refuses an empty title or an empty
                    // description that is present; catching it here keeps the
                    // reason with the projection that produced it.
                    || items.iter().any(|item| {
                        item.title.is_empty()
                            || item.title.len() > MAX_FIELD_BYTES
                            || item.description.as_ref().is_some_and(|description| {
                                description.is_empty() || description.len() > MAX_FIELD_BYTES
                            })
                    })
                {
                    return Err(ProjectionFailure::Bounds);
                }
            }
            PresentationSection::Notice { heading, body, .. } => {
                if body.is_empty()
                    || body.len() > MAX_FIELD_BYTES
                    || heading.as_ref().is_some_and(|heading| {
                        heading.is_empty() || heading.len() > MAX_FIELD_BYTES
                    })
                {
                    return Err(ProjectionFailure::Bounds);
                }
            }
        }
    }
    let bytes = serde_json::to_vec(projection).map_err(|_| ProjectionFailure::Encoding)?;
    (bytes.len() <= MAX_PROJECTION_BYTES)
        .then_some(())
        .ok_or(ProjectionFailure::Bounds)
}

#[cfg(test)]
mod tests {
    #[test]
    fn truncated_fields_still_reject_colliding_and_unnameable_labels() {
        let fields = (0..128)
            .map(|index| (format!("a{index:03}"), serde_json::json!(index)))
            .collect::<serde_json::Map<_, _>>();
        let collision = Value::Object(
            fields
                .clone()
                .into_iter()
                .chain([
                    ("next_cursor".into(), serde_json::json!("one")),
                    ("next-cursor".into(), serde_json::json!("two")),
                ])
                .collect(),
        );
        assert!(matches!(
            exact_output_projection_with_limit(&collision, 2),
            Err(ProjectionFailure::LabelCollision)
        ));
        let unnameable = Value::Object(
            fields
                .into_iter()
                .chain([("z_field".into(), serde_json::json!({"___": "unsafe label"}))])
                .collect(),
        );
        assert!(matches!(
            exact_output_projection_with_limit(&unnameable, 2),
            Err(ProjectionFailure::UnnameableField)
        ));
    }

    #[test]
    fn omitted_list_records_still_enforce_depth_and_label_safety() {
        let nested = (0..MAX_PROJECTION_DEPTH + 1).fold(
            serde_json::json!("leaf"),
            |value, _| serde_json::json!({"nested": value}),
        );
        for (tail, expected) in [
            (nested, ProjectionFailure::Bounds),
            (
                serde_json::json!({"next_cursor":"one", "next-cursor":"two"}),
                ProjectionFailure::LabelCollision,
            ),
            (
                serde_json::json!({"___":"invalid"}),
                ProjectionFailure::UnnameableField,
            ),
        ] {
            let records = (0..16)
                .map(|index| serde_json::json!({"title":format!("Item {index}")}))
                .chain([tail])
                .collect::<Vec<_>>();
            let output = serde_json::json!({"items": records});
            assert!(
                matches!(output_list_projection_with_limits(&output, 16, 16, 1024), Err(error) if error == expected)
            );
        }
    }

    use super::*;

    #[test]
    fn a_written_body_keeps_its_line_breaks_while_undisplayable_values_stay_escaped() {
        // A message is shaped by its line breaks and the card renders them, so
        // projecting them as one quoted `\n` run hid the body it was showing.
        let body = "Hi River,\n\nBest regards,\nGoAI";
        assert_eq!(
            exact_scalar_value(&Value::String(body.to_owned())).unwrap(),
            body
        );
        assert_eq!(
            exact_scalar_value(&Value::String("one\tcolumn".to_owned())).unwrap(),
            "one\tcolumn"
        );
        // An empty value would fail the non-empty bound, so it stays visible.
        assert_eq!(
            exact_scalar_value(&Value::String(String::new())).unwrap(),
            "\"\""
        );
        // Everything a card cannot display is still shown as its JSON literal:
        // a terminal escape, a NUL, and a lone carriage return all overwrite or
        // reinterpret what surrounds them.
        for undisplayable in ["a\u{1b}[31mred", "a\u{0}b", "a\rb"] {
            let projected = exact_scalar_value(&Value::String(undisplayable.to_owned())).unwrap();
            assert!(projected.starts_with('"'), "{projected}");
            assert_ne!(projected, undisplayable);
        }
    }

    /// A page is recognized by its shape, never by a key this adapter knows.
    #[test]
    fn a_page_of_records_is_located_at_the_shallowest_unambiguous_depth() {
        let record = serde_json::json!({"id": "one"});
        let two = serde_json::json!([record, record]);
        // The root array, one key, and one nested key are all pages.
        assert_eq!(locate_list(&two), Some(Vec::new()));
        assert_eq!(
            locate_list(&serde_json::json!({"items": two, "next_cursor": "c"})),
            Some(vec!["items".to_owned()])
        );
        assert_eq!(
            locate_list(&serde_json::json!({"effective_query": "q", "page": {"items": two}})),
            Some(vec!["page".to_owned(), "items".to_owned()])
        );
        // A shallower list wins over deeper facet lists instead of tying.
        assert_eq!(
            locate_list(
                &serde_json::json!({"items": two, "facets": {"stages": two, "platforms": two}})
            ),
            Some(vec!["items".to_owned()])
        );
        // One record is a record; two sibling lists are ambiguous. Both keep
        // the exact field projection rather than guess.
        assert_eq!(locate_list(&serde_json::json!({"hits": [record]})), None);
        assert_eq!(locate_list(&serde_json::json!({"a": two, "b": two})), None);
        assert_eq!(locate_list(&serde_json::json!({"value": "plain"})), None);
    }

    /// A search page whose channel metadata sits beside its hits instead of
    /// inside each of them.
    ///
    /// The metadata is a map, so the page still holds exactly one array of
    /// records and its hits stay the located list. Moving that metadata out
    /// of the hits must also not change whether the page reaches a card at
    /// all, which is what holding both shapes side by side fixes in place.
    #[test]
    fn a_page_whose_channels_sit_beside_its_hits_projects_exactly_as_it_did_inside_them() {
        // One full YouTube page: 50 video hits across 30 channels, each
        // channel description at the measured 298-character mean.
        let description = "d".repeat(298);
        let channel = |index: usize| {
            serde_json::json!({
                "title": format!("Channel {index}"),
                "thumbnail_url": "https://i.ytimg.com/channel.jpg",
                "description": description,
                "custom_url": format!("@channel{index}"),
                "country": "US",
                "subscriber_count": 1_000,
                "view_count": 2_000,
                "video_count": 30,
            })
        };
        let hit = |index: usize| {
            serde_json::json!({
                "channel_id": format!("UC-channel-{}", index % 30),
                "channel_url":
                    format!("https://www.youtube.com/channel/UC-channel-{}", index % 30),
                "matched": {
                    "title": format!("Upload {index}"),
                    "description": "What this upload covers",
                    "video_id": format!("video-{index}"),
                    "published_at": "2026-09-01T00:00:00Z",
                },
            })
        };
        let items = (0..50).map(hit).collect::<Vec<_>>();
        let channels = (0..30)
            .map(|index| (format!("UC-channel-{index}"), channel(index)))
            .collect::<serde_json::Map<_, _>>();
        // The same page as it was built before: every hit carrying its own
        // copy of the channel that matched it.
        let copied = (0..50)
            .map(|index| {
                let mut record = hit(index);
                record["channel"] = channel(index % 30);
                record
            })
            .collect::<Vec<_>>();

        for (label, page, lead_left_out) in [
            (
                "channels beside the hits",
                serde_json::json!({
                    "items": items,
                    "channels": channels,
                    "next_page_token": "provider+/cursor==",
                }),
                // Thirty channels of eight leaves each, against one section's
                // capacity of 128: the lead shows 128 and counts the rest.
                112,
            ),
            (
                "channels copied into each hit",
                serde_json::json!({"items": copied, "next_page_token": "provider+/cursor=="}),
                // Every channel rides inside its own hit, so the lead holds
                // nothing but the withheld key and has nothing to shorten.
                0,
            ),
        ] {
            // A map is not a second array, so the hits stay the one page of
            // records at the shallowest depth either way.
            assert_eq!(
                locate_list(&page),
                Some(vec!["items".to_owned()]),
                "{label}"
            );
            // `next_page_token` normalizes to `nextpagetoken`, which contains
            // a forbidden name, so that one key is not shown. Withholding a
            // key is not a reason to withhold the page standing beside it:
            // every hit still reaches the card, and the card counts the key.
            let list = output_list_projection(&page)
                .expect("a withheld key does not fail the page")
                .expect("a page of hits");
            assert_eq!(list.items.len(), 50, "{label}");
            assert_eq!(list.omitted, 0, "{label}");
            assert_eq!(list.omissions.withheld, 1, "{label}");
            // The lead's own capacity loss is counted on the same card, so a
            // page does not drop 112 channel leaves without saying so.
            assert_eq!(list.omissions.truncated, lead_left_out, "{label}");
            // Withheld means withheld: the value never reaches the card.
            let shown =
                serde_json::to_string(&serde_json::json!({"lead": list.lead, "items": list.items}))
                    .unwrap();
            assert!(!shown.contains("provider+/cursor=="), "{label}");
            println!(
                "{label}: {} rows, {} lead fields, {} lead leaves left out, {} B of sections",
                list.items.len(),
                list.lead.len(),
                list.omissions.truncated,
                shown.len()
            );
            assert!(
                shown.len() <= MAX_PROJECTION_BYTES,
                "{label}: {} B",
                shown.len()
            );
        }
    }

    /// The three display findings have separate internal causes. Only the
    /// unsafe-key case is a rule about what a card may display, and skipping
    /// that one must not quietly turn the other two into gaps nobody is told
    /// about.
    #[test]
    fn an_unshowable_key_is_skipped_and_counted_while_ambiguous_labels_stay_fatal() {
        // The key and its subtree are left out, the rest of the output is
        // still shown, and the card is handed a count to say so with.
        let output = serde_json::json!({
            "next_page_token": "provider+/cursor==",
            "total": 3,
            "auth": {"token": "secret-value", "scope": "read"},
        });
        let (fields, omissions) = exact_output_projection(&output).unwrap().into_parts();
        let labels = fields
            .iter()
            .map(|field| field.label.as_str())
            .collect::<Vec<_>>();
        assert_eq!(labels, ["Auth · Scope", "Total"]);
        assert_eq!(omissions.withheld, 2);
        assert_eq!(omissions.truncated, 0);
        let shown = serde_json::to_string(&fields).unwrap();
        assert!(!shown.contains("provider+/cursor==") && !shown.contains("secret-value"));

        // Two paths that make one label, and a key with no label at all, are
        // display defects rather than safety rules. Both stay fatal: a field
        // that disappeared with no count is precisely what the skip above is
        // counted to avoid, and neither of these can be counted honestly.
        assert!(matches!(
            exact_output_projection(&serde_json::json!({
                "next_cursor": "a",
                "next-cursor": "b"
            })),
            Err(ProjectionFailure::LabelCollision)
        ));
        assert!(matches!(
            exact_output_projection(&serde_json::json!({"___": "value"})),
            Err(ProjectionFailure::UnnameableField)
        ));

        // A confirmation shows exactly what will be submitted, so it still
        // refuses rather than showing an argument set with a hole in it.
        let arguments = serde_json::json!({"api_key": "k", "to": "someone"});
        assert!(matches!(
            flatten_exact(&arguments, FieldBudget::Exact),
            Err(ProjectionFailure::UnsafeField)
        ));
        assert!(flatten_exact(&arguments, FieldBudget::Truncating).is_ok());
    }

    /// A row is named by what the record says it is, not by its position.
    ///
    /// A search hit keeps the video it matched in a sub-object, so reading
    /// only the record's own keys named every row of a YouTube search page
    /// `[1]`…`[50]` — a numbered list of the one thing that has to be
    /// scannable. The name is read one level in, and only when that is
    /// unambiguous.
    #[test]
    fn a_record_is_named_one_level_in_only_when_that_name_is_unambiguous() {
        let hit = |index: usize, title: &str| {
            serde_json::json!({
                "channel_id": format!("UC-channel-{index}"),
                "channel_url": format!("https://www.youtube.com/channel/UC-channel-{index}"),
                "matched": {
                    "title": title,
                    "description": "What this upload covers",
                    "video_id": format!("video-{index}"),
                    "published_at": "2026-09-01T00:00:00Z",
                },
            })
        };
        let page = serde_json::json!({
            "items": [
                hit(0, "Rust in 100 Seconds"),
                hit(1, "Why the borrow checker is easier than you think"),
                hit(2, "Building a CLI with   clap\n  and ratatui"),
            ],
            "channels": {},
            "next_page_token": "provider+/cursor==",
        });
        let list = output_list_projection(&page)
            .expect("a page of hits projects")
            .expect("a page of hits");
        let rows = list
            .items
            .iter()
            .map(|item| item.title.as_str())
            .collect::<Vec<_>>();
        println!("rows: {rows:#?}");
        assert_eq!(
            rows,
            [
                "Rust in 100 Seconds",
                "Why the borrow checker is easier than you think",
                // A row's name is one line, so the value's own wrapping closes
                // up — and only the name is reshaped that way.
                "Building a CLI with clap and ratatui",
            ]
        );
        // The name leaves the detail under the label the flattening gave it,
        // and everything else the record said stays.
        let first = list.items[0]
            .description
            .as_deref()
            .expect("a row details itself");
        assert!(!first.contains("Matched · Title"), "{first}");
        assert!(first.contains("Matched · Video Id: video-0"), "{first}");
        assert!(first.contains("Matched · Description:"), "{first}");

        for (label, record, expected) in [
            // A record that names itself is never renamed from inside itself.
            (
                "its own keys answer first",
                serde_json::json!({"title": "Its own name", "matched": {"title": "Nested"}}),
                "Its own name",
            ),
            // Two sub-objects offering the same word leave nothing to decide
            // between them, so the row is numbered exactly as it was before.
            (
                "two sub-objects disagree",
                serde_json::json!({
                    "matched": {"title": "The video"},
                    "channel": {"title": "The channel"},
                }),
                "[5]",
            ),
            // A key a card may not name cannot name a row either.
            (
                "the name sits under a key that may not be shown",
                serde_json::json!({"raw_response": {"title": "From the wire"}}),
                "[5]",
            ),
            // Two levels down is something the record contains, not the
            // record: the bound holds rather than hunting for any `title`.
            (
                "the name sits past the depth bound",
                serde_json::json!({"page": {"matched": {"title": "Too far"}}}),
                "[5]",
            ),
        ] {
            let (item, _) =
                record_item(&record, 4, MAX_ITEM_DETAIL_BYTES).expect("a record projects as a row");
            assert_eq!(item.title, expected, "{label}");
        }
    }

    /// References are evidence, not display: a page far past one section's
    /// capacity still names every resource it carries.
    #[test]
    fn resource_references_outlive_the_display_capacity() {
        let records = (0..60)
            .map(|index| {
                serde_json::json!({
                    "post_id": format!("post-{index}"),
                    "text": "x",
                    "language": "en",
                })
            })
            .collect::<Vec<_>>();
        let page = serde_json::json!({"page": {"items": records}});
        assert!(
            exact_output_projection(&page)
                .unwrap()
                .into_parts()
                .1
                .truncated
                > 0,
            "the fixture must exceed one section's capacity"
        );
        let refs = output_resource_refs(&page);
        assert_eq!(refs.len(), 60);
        assert!(refs.iter().all(|reference| {
            reference.kind == ResultResourceKind::ProviderObject
                && reference.id.starts_with("post-")
        }));
    }

    /// Reproduce the pre-batch-budget failure with the same native page shape:
    /// eight calls, fifty content records per call. Each page fits the old
    /// per-section limits, but the unallocated concatenation crosses the
    /// generic projection byte bound. This pins the observed trigger rather
    /// than attributing the old failed card to an unverified Host problem.
    #[test]
    fn eight_fifty_unallocated_native_pages_hit_the_projection_byte_bound() {
        let page = |call_index: usize| {
            let records = (0..50)
                .map(|record_index| {
                    serde_json::json!({
                        "post_id": format!("post-{call_index}-{record_index}"),
                        "text": format!(
                            "Post {record_index} contains a bounded native body {}",
                            "x".repeat(96)
                        ),
                        "language": "en",
                        "author": {
                            "candidate_id": format!("candidate-{call_index}-{record_index}"),
                            "display_name": format!("Creator {call_index}-{record_index}"),
                            "handle": format!("creator_{call_index}_{record_index}"),
                        },
                        "media": [
                            {"width": 947, "height": 1183},
                            {"width": 320, "height": 400},
                        ],
                    })
                })
                .collect::<Vec<_>>();
            serde_json::json!({
                "effective_query": format!("query-{call_index}"),
                "page": {
                    "items": records,
                    "next_cursor": format!("cursor-{call_index}"),
                },
            })
        };
        let mut sections = vec![PresentationSection::Items {
            heading: Some("Operations".to_owned()),
            items: (0..8)
                .map(|index| PresentationItem {
                    title: format!("Native read {index}"),
                    description: Some("Completed.".to_owned()),
                })
                .collect(),
        }];
        let mut total_records = 0;
        for call_index in 0..8 {
            let list = output_list_projection(&page(call_index))
                .expect("native page shape is encodable")
                .expect("native page has one unambiguous records list");
            total_records += list.items.len() + list.omitted;
            sections.push(PresentationSection::Fields {
                heading: Some(format!("Native read {call_index}")),
                fields: list.lead,
            });
            sections.push(PresentationSection::Items {
                heading: Some(format!("Native read {call_index}")),
                items: list.items,
            });
        }
        let projection = PresentationProjection {
            kind: onecrew_local_runtime::ResultPresentationKind::new("batch_result").unwrap(),
            title: "Execution result".to_owned(),
            summary: Some("Batch execution completed.".to_owned()),
            sections,
        };
        let bytes = serde_json::to_vec(&projection).unwrap();
        println!(
            "unallocated native pages: calls=8 records={total_records} sections={} bytes={}",
            projection.sections.len(),
            bytes.len()
        );
        assert_eq!(total_records, 8 * 50);
        assert!(projection.sections.len() <= MAX_PROJECTION_SECTIONS);
        assert!(bytes.len() > MAX_PROJECTION_BYTES);
        assert_eq!(
            ensure_projection_bounds(&projection),
            Err(ProjectionFailure::Bounds)
        );
    }
}
