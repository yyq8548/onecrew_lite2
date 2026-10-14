use std::sync::Arc;

use jsonschema::{Draft, Validator};
use onecrew_schema_manifest::{SchemaDirection, canonical_schema};
use schemars::JsonSchema;
use serde_json::Value;

pub(crate) struct CompiledSchema {
    document: Value,
    validator: Arc<Validator>,
    revision: String,
}

impl CompiledSchema {
    pub(crate) fn for_input<T: JsonSchema>() -> Result<Self, SchemaBuildError> {
        Self::build::<T>(false)
    }

    pub(crate) fn for_output<T: JsonSchema>() -> Result<Self, SchemaBuildError> {
        Self::build::<T>(true)
    }

    fn build<T: JsonSchema>(serialize: bool) -> Result<Self, SchemaBuildError> {
        let direction = if serialize {
            SchemaDirection::Serialize
        } else {
            SchemaDirection::Deserialize
        };
        let canonical = canonical_schema::<T>(direction).map_err(SchemaBuildError::from)?;
        let document = canonical.document().clone();
        let validator = jsonschema::options()
            .with_draft(Draft::Draft202012)
            .build(&document)
            .map_err(|_| SchemaBuildError::InvalidSchema)?;
        let revision = canonical.revision().to_owned();
        Ok(Self {
            document,
            validator: Arc::new(validator),
            revision,
        })
    }

    pub(crate) fn validate(&self, value: &Value) -> bool {
        self.validator.is_valid(value)
    }

    pub(crate) fn document(&self) -> &Value {
        &self.document
    }

    pub(crate) fn revision(&self) -> &str {
        &self.revision
    }

    pub(crate) fn validates_instance_pointer(&self, pointer: &str) -> bool {
        self.instance_subschemas(pointer).is_some()
    }

    #[cfg(test)]
    pub(crate) fn accepts_pointer_value_from(
        &self,
        input_pointer: &str,
        source: &CompiledSchema,
        output_pointer: &str,
    ) -> bool {
        let Some(targets) = self.instance_subschemas(input_pointer) else {
            return false;
        };
        let Some(sources) = source.instance_subschemas(output_pointer) else {
            return false;
        };
        sources.iter().all(|source| {
            targets
                .iter()
                .any(|target| schemas_are_assignable(source, target))
        })
    }

    fn instance_subschemas(&self, pointer: &str) -> Option<Vec<&Value>> {
        let tokens = decode_pointer(pointer)?;
        let schemas = resolve_instance_path(&self.document, &tokens);
        (!schemas.is_empty()).then_some(schemas)
    }
}

fn decode_pointer(pointer: &str) -> Option<Vec<String>> {
    if pointer.is_empty() || !pointer.starts_with('/') {
        return None;
    }
    pointer[1..]
        .split('/')
        .map(|token| {
            let mut decoded = String::new();
            let mut chars = token.chars();
            while let Some(character) = chars.next() {
                if character != '~' {
                    decoded.push(character);
                    continue;
                }
                match chars.next()? {
                    '0' => decoded.push('~'),
                    '1' => decoded.push('/'),
                    _ => return None,
                }
            }
            Some(decoded)
        })
        .collect()
}

fn resolve_instance_path<'a>(schema: &'a Value, tokens: &[String]) -> Vec<&'a Value> {
    if tokens.is_empty() {
        return vec![schema];
    }
    let mut matches = Vec::new();
    for keyword in ["oneOf", "anyOf", "allOf"] {
        if let Some(branches) = schema.get(keyword).and_then(Value::as_array) {
            for branch in branches {
                matches.extend(resolve_instance_path(branch, tokens));
            }
        }
    }
    if let Some(property) = schema
        .get("properties")
        .and_then(Value::as_object)
        .and_then(|properties| properties.get(&tokens[0]))
    {
        matches.extend(resolve_instance_path(property, &tokens[1..]));
    }
    if tokens[0] == "*"
        && let Some(items) = schema.get("items")
    {
        matches.extend(resolve_instance_path(items, &tokens[1..]));
    }
    matches
}

#[cfg(test)]
fn schemas_are_assignable(source: &Value, target: &Value) -> bool {
    validation_shape(source.clone()) == validation_shape(target.clone())
}

#[cfg(test)]
fn validation_shape(mut schema: Value) -> Value {
    match &mut schema {
        Value::Object(object) => {
            for annotation in [
                "$schema",
                "title",
                "description",
                "default",
                "examples",
                "deprecated",
                "readOnly",
                "writeOnly",
            ] {
                object.remove(annotation);
            }
            for child in object.values_mut() {
                *child = validation_shape(std::mem::take(child));
            }
        }
        Value::Array(items) => {
            for item in items {
                *item = validation_shape(std::mem::take(item));
            }
        }
        _ => {}
    }
    schema
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SchemaBuildError {
    Generation,
    UnsupportedReference,
    UnknownReference,
    ReferenceDepthExceeded,
    UnresolvedReference,
    RootIsNotObject,
    InvalidSchema,
}

impl From<onecrew_schema_manifest::SchemaManifestError> for SchemaBuildError {
    fn from(error: onecrew_schema_manifest::SchemaManifestError) -> Self {
        use onecrew_schema_manifest::SchemaManifestError;
        match error {
            SchemaManifestError::Generation => Self::Generation,
            SchemaManifestError::UnsupportedReference => Self::UnsupportedReference,
            SchemaManifestError::UnknownReference => Self::UnknownReference,
            SchemaManifestError::ReferenceDepthExceeded => Self::ReferenceDepthExceeded,
            SchemaManifestError::UnresolvedReference => Self::UnresolvedReference,
            SchemaManifestError::RootIsNotObject => Self::RootIsNotObject,
            SchemaManifestError::DuplicateRoot => Self::Generation,
        }
    }
}
