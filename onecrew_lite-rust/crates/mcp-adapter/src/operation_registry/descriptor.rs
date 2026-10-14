use serde_json::Value;

use super::schema::CompiledSchema;

mod catalog;
mod connections;
mod entries;
mod filters;
mod identity;
mod pointer;
mod safety;

#[cfg(test)]
pub(crate) use connections::MAX_CONNECTIONS_PER_TOOLKIT;
pub(super) use connections::deserialize_toolkit_filters;
pub(crate) use connections::{
    ConnectionListInput, ConnectionListResult, ConnectionRef, ConnectionRequirement,
    MailboxToolkitId, OperatorToolkitId, ServiceToolkitId,
};
pub(crate) use entries::{
    EntryKind, EntryMetadata, OperationEntry, OperationRegistration, OperationViewKind,
    PlaybookDifficulty, PlaybookEntry, PlaybookStep, RegistryEntry, ToolkitGuideEntry,
};
pub(crate) use identity::{
    ApprovalClass, EffectClass, EntryId, SecretClass, SurfaceId, SurfaceSet,
};
pub(crate) use pointer::InstancePointer;
pub(crate) use safety::{
    AuditMode, ConfirmationMode, ConfirmationSafety, IdempotencyMode, IdempotencySafety,
    OperationSafety, OperationSafetySummary, ReadbackMode, ReadbackSafety,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DescriptorError {
    InvalidId,
    InvalidInstancePointer,
    InvalidBoundedString,
    SearchTextNotEnglish,
    SearchTextHasNoMeaningfulTerms,
    EmptySurfaces,
}

/// The model view of a compiled schema. It removes only keywords that carry
/// nothing usable for the model: `$schema`, the Rust type-name root `title`,
/// integer `format`s, bounds that restate that integer type's range, and
/// `default: null`. Validation and `schema_revision` use the canonical
/// document.
pub(crate) fn schema_json(schema: &CompiledSchema) -> Value {
    let mut view = schema.document().clone();
    if let Value::Object(root) = &mut view {
        root.remove("$schema");
        root.remove("title");
    }
    project_model_view(&mut view);
    view
}

fn project_model_view(schema: &mut Value) {
    let Value::Object(object) = schema else {
        return;
    };
    if object.get("default") == Some(&Value::Null) {
        object.remove("default");
    }
    if let Some((minimum, maximum)) = object
        .get("format")
        .and_then(Value::as_str)
        .and_then(integer_format_range)
    {
        object.remove("format");
        // Unsigned minimum 0 stays: it states that the value is not negative.
        if minimum != 0 && object.get("minimum").and_then(Value::as_i64) == Some(minimum) {
            object.remove("minimum");
        }
        if object.get("maximum").and_then(Value::as_u64) == Some(maximum) {
            object.remove("maximum");
        }
    }
    for (keyword, child) in object.iter_mut() {
        match (keyword.as_str(), child) {
            // Instance values are data, not schemas.
            ("const" | "default" | "enum" | "examples", _) => {}
            // Maps from names to schemas.
            ("properties" | "patternProperties" | "dependentSchemas", Value::Object(schemas)) => {
                schemas.values_mut().for_each(project_model_view);
            }
            (_, Value::Array(schemas)) => schemas.iter_mut().for_each(project_model_view),
            (_, child) => project_model_view(child),
        }
    }
}

/// The inclusive range of a schemars integer `format`.
fn integer_format_range(format: &str) -> Option<(i64, u64)> {
    Some(match format {
        "uint8" => (0, u8::MAX.into()),
        "uint16" => (0, u16::MAX.into()),
        "uint32" => (0, u32::MAX.into()),
        "uint64" | "uint" => (0, u64::MAX),
        "int8" => (i8::MIN.into(), i8::MAX.unsigned_abs().into()),
        "int16" => (i16::MIN.into(), i16::MAX.unsigned_abs().into()),
        "int32" => (i32::MIN.into(), i32::MAX.unsigned_abs().into()),
        "int64" | "int" => (i64::MIN, i64::MAX.unsigned_abs()),
        _ => return None,
    })
}

pub(super) use catalog::deserialize_effect_filters;
pub(crate) use catalog::{
    CatalogEntrySummary, CatalogMatchRole, DomainId, PlaybookSummary, SearchLimit, SearchText,
};
