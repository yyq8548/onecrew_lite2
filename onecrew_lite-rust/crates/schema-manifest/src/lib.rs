//! Deterministic schema manifests for Rust-owned public adapter contracts.
//!
//! This crate canonicalizes generated JSON Schema, constrains integer ranges
//! for JavaScript consumers, and computes stable contract identities. It does
//! not own adapter behavior or generated TypeScript artifacts.

use std::collections::{BTreeMap, BTreeSet};

use schemars::{JsonSchema, generate::SchemaSettings};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use thiserror::Error;

const MAX_INLINE_DEPTH: usize = 64;
const JS_MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaDirection {
    Serialize,
    Deserialize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaRoot {
    pub name: String,
    pub direction: SchemaDirection,
    pub id: String,
    pub revision: String,
    pub schema: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaManifest {
    pub schema_version: u32,
    pub target: String,
    pub roots: Vec<SchemaRoot>,
    pub manifest_revision: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalSchema {
    document: Value,
    revision: String,
}

impl CanonicalSchema {
    pub fn document(&self) -> &Value {
        &self.document
    }

    pub fn revision(&self) -> &str {
        &self.revision
    }
}

pub fn canonical_schema<T: JsonSchema>(
    direction: SchemaDirection,
) -> Result<CanonicalSchema, SchemaManifestError> {
    canonical_schema_with_options::<T>(direction, None, None, false)
}

pub fn contract_schema_root<T: JsonSchema>(
    name: &str,
    direction: SchemaDirection,
) -> Result<SchemaRoot, SchemaManifestError> {
    let id = format!("https://reacher.local/contracts/{name}.schema.json");
    let canonical = canonical_schema_with_options::<T>(direction, Some(&id), Some(name), true)?;
    Ok(SchemaRoot {
        name: name.to_owned(),
        direction,
        id,
        revision: canonical.revision,
        schema: canonical.document,
    })
}

impl SchemaManifest {
    pub fn new(
        target: impl Into<String>,
        mut roots: Vec<SchemaRoot>,
    ) -> Result<Self, SchemaManifestError> {
        roots.sort_by(|left, right| left.name.cmp(&right.name));
        let mut names = BTreeSet::new();
        if roots.iter().any(|root| !names.insert(root.name.clone())) {
            return Err(SchemaManifestError::DuplicateRoot);
        }
        let target = target.into();
        let unsigned = serde_json::json!({
            "schema_version": 1,
            "target": target,
            "roots": roots,
        });
        let encoded = serde_json::to_vec(&unsigned).map_err(|_| SchemaManifestError::Generation)?;
        let manifest_revision = sha256_revision(&encoded);
        let target = unsigned["target"]
            .as_str()
            .ok_or(SchemaManifestError::Generation)?
            .to_owned();
        let roots = serde_json::from_value(unsigned["roots"].clone())
            .map_err(|_| SchemaManifestError::Generation)?;
        Ok(Self {
            schema_version: 1,
            target,
            roots,
            manifest_revision,
        })
    }
}

fn canonical_schema_with_options<T: JsonSchema>(
    direction: SchemaDirection,
    id: Option<&str>,
    title: Option<&str>,
    javascript_safe_integers: bool,
) -> Result<CanonicalSchema, SchemaManifestError> {
    let settings = match direction {
        SchemaDirection::Serialize => SchemaSettings::draft2020_12().for_serialize(),
        SchemaDirection::Deserialize => SchemaSettings::draft2020_12().for_deserialize(),
    };
    let schema = settings.into_generator().into_root_schema_for::<T>();
    let mut document = serde_json::to_value(schema).map_err(|_| SchemaManifestError::Generation)?;
    inline_schema(&mut document)?;
    if contains_ref_or_defs(&document) {
        return Err(SchemaManifestError::UnresolvedReference);
    }
    let object = document
        .as_object_mut()
        .ok_or(SchemaManifestError::RootIsNotObject)?;
    if let Some(id) = id {
        object.insert("$id".into(), Value::String(id.to_owned()));
    }
    if let Some(title) = title {
        object.insert("title".into(), Value::String(title.to_owned()));
    }
    if javascript_safe_integers {
        constrain_javascript_integers(&mut document);
    }
    canonicalize(&mut document);
    let encoded = serde_json::to_vec(&document).map_err(|_| SchemaManifestError::Generation)?;
    Ok(CanonicalSchema {
        document,
        revision: sha256_revision(&encoded),
    })
}

fn constrain_javascript_integers(value: &mut Value) {
    match value {
        Value::Object(object) => {
            if object.get("format") == Some(&Value::String("uint64".into())) {
                object.insert("maximum".into(), Value::from(JS_MAX_SAFE_INTEGER));
            }
            if object.get("format") == Some(&Value::String("int64".into())) {
                object.insert("minimum".into(), Value::from(-(JS_MAX_SAFE_INTEGER as i64)));
                object.insert("maximum".into(), Value::from(JS_MAX_SAFE_INTEGER));
            }
            for child in object.values_mut() {
                constrain_javascript_integers(child);
            }
        }
        Value::Array(values) => values.iter_mut().for_each(constrain_javascript_integers),
        _ => {}
    }
}

fn inline_schema(schema: &mut Value) -> Result<(), SchemaManifestError> {
    let definitions = schema
        .as_object_mut()
        .and_then(|object| object.remove("$defs"))
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    resolve_refs(schema, &definitions, 0)
}

fn resolve_refs(
    value: &mut Value,
    definitions: &Map<String, Value>,
    depth: usize,
) -> Result<(), SchemaManifestError> {
    if depth > MAX_INLINE_DEPTH {
        return Err(SchemaManifestError::ReferenceDepthExceeded);
    }
    match value {
        Value::Object(object) => {
            let reference = object.remove("$ref");
            for child in object.values_mut() {
                resolve_refs(child, definitions, depth + 1)?;
            }
            if let Some(reference) = reference {
                let key = reference
                    .as_str()
                    .and_then(|reference| reference.strip_prefix("#/$defs/"))
                    .ok_or(SchemaManifestError::UnsupportedReference)?;
                let mut replacement = definitions
                    .get(key)
                    .cloned()
                    .ok_or(SchemaManifestError::UnknownReference)?;
                resolve_refs(&mut replacement, definitions, depth + 1)?;
                // Keywords beside `$ref` (a field's description or default)
                // apply alongside the definition and win over its annotations.
                let Value::Object(replacement) = replacement else {
                    return Err(SchemaManifestError::UnsupportedReference);
                };
                for (keyword, member) in replacement {
                    object.entry(keyword).or_insert(member);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                resolve_refs(item, definitions, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn contains_ref_or_defs(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            object.contains_key("$ref")
                || object.contains_key("$defs")
                || object.values().any(contains_ref_or_defs)
        }
        Value::Array(items) => items.iter().any(contains_ref_or_defs),
        _ => false,
    }
}

fn canonicalize(value: &mut Value) {
    match value {
        Value::Object(object) => {
            let mut sorted = BTreeMap::new();
            for (key, mut child) in std::mem::take(object) {
                canonicalize(&mut child);
                sorted.insert(key, child);
            }
            object.extend(sorted);
        }
        Value::Array(items) => items.iter_mut().for_each(canonicalize),
        _ => {}
    }
}

fn sha256_revision(encoded: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(encoded))
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum SchemaManifestError {
    #[error("schema generation failed")]
    Generation,
    #[error("schema contains an unsupported reference")]
    UnsupportedReference,
    #[error("schema references an unknown definition")]
    UnknownReference,
    #[error("schema reference depth exceeded")]
    ReferenceDepthExceeded,
    #[error("schema contains an unresolved reference")]
    UnresolvedReference,
    #[error("schema root is not an object")]
    RootIsNotObject,
    #[error("schema manifest contains a duplicate root")]
    DuplicateRoot,
}

#[cfg(test)]
mod tests {
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(JsonSchema, Serialize, Deserialize)]
    struct Example {
        revision: u64,
        note: Option<String>,
    }

    #[test]
    fn contract_hash_covers_id_and_javascript_number_policy() {
        let root = contract_schema_root::<Example>("Example", SchemaDirection::Serialize)
            .expect("contract root");
        assert_eq!(root.schema["$id"], root.id);
        let revision = &root.schema["properties"]["revision"];
        assert_eq!(revision["maximum"], JS_MAX_SAFE_INTEGER);
        assert!(root.revision.starts_with("sha256:"));
    }

    /// Pick a result ordering.
    #[derive(JsonSchema, Serialize, Deserialize, Default)]
    #[serde(rename_all = "snake_case")]
    enum Mode {
        #[default]
        Top,
        Latest,
    }

    #[derive(JsonSchema, Serialize, Deserialize)]
    struct Referencing {
        /// Ordering for this search.
        #[serde(default)]
        mode: Mode,
        plain: Mode,
    }

    #[test]
    fn inlined_references_keep_sibling_keywords_over_the_definition() {
        let schema = canonical_schema::<Referencing>(SchemaDirection::Deserialize)
            .expect("canonical schema");
        let properties = &schema.document()["properties"];
        assert_eq!(
            properties["mode"]["description"],
            "Ordering for this search."
        );
        assert_eq!(properties["mode"]["default"], "top");
        assert_eq!(
            properties["mode"]["enum"],
            serde_json::json!(["top", "latest"])
        );
        assert_eq!(
            properties["plain"]["description"],
            "Pick a result ordering."
        );
        assert!(properties["plain"].get("default").is_none());
        assert!(!contains_ref_or_defs(schema.document()));
    }

    #[test]
    fn manifest_order_is_stable_and_duplicates_fail_closed() {
        let first =
            contract_schema_root::<Example>("B", SchemaDirection::Serialize).expect("first root");
        let second = contract_schema_root::<Example>("A", SchemaDirection::Deserialize)
            .expect("second root");
        let manifest = SchemaManifest::new("test", vec![first.clone(), second]).expect("manifest");
        assert_eq!(manifest.roots[0].name, "A");
        assert_eq!(manifest.roots[1].name, "B");
        assert_eq!(
            SchemaManifest::new("test", vec![first.clone(), first]),
            Err(SchemaManifestError::DuplicateRoot)
        );
    }
}
