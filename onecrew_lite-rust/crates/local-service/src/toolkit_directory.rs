use onecrew_local_runtime::{CapabilityCatalog, CatalogSurface, ToolkitId};
use serde::Serialize;

use crate::ServiceState;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ToolkitOperationSummary {
    pub id: String,
    pub title: String,
    pub summary: String,
}

/// The existing HTTP navigation projection. No MCP schema, binding or
/// callable authority is copied into this response.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ToolkitDirectory {
    pub id: String,
    pub toolkit: ToolkitId,
    pub title: String,
    pub summary: String,
    pub purpose: String,
    pub connection_model: String,
    pub identifier_semantics: Vec<String>,
    pub capability_categories: Vec<String>,
    pub gotchas: Vec<String>,
    pub operations: Vec<ToolkitOperationSummary>,
}

pub(super) async fn toolkit_directory(
    state: &ServiceState,
) -> Result<Vec<ToolkitDirectory>, String> {
    let catalog = state.catalog().await?;
    Ok(project_directory(&catalog))
}

pub(crate) fn project_directory(catalog: &CapabilityCatalog) -> Vec<ToolkitDirectory> {
    catalog
        .toolkits()
        .filter_map(|toolkit| {
            let operations = catalog
                .operations()
                .filter(|operation| {
                    operation.toolkit_id == toolkit.toolkit
                        && operation.supports(CatalogSurface::WebNavigation)
                        && !onecrew_local_runtime::is_retired_operation(operation.id.as_str())
                })
                .map(|operation| ToolkitOperationSummary {
                    id: operation.id.as_str().to_owned(),
                    title: operation.title.to_owned(),
                    summary: operation.summary.to_owned(),
                })
                .collect::<Vec<_>>();
            if operations.is_empty() {
                return None;
            }
            Some(ToolkitDirectory {
                id: toolkit.id.to_owned(),
                toolkit: toolkit.toolkit,
                title: toolkit.title.to_owned(),
                summary: toolkit.summary.to_owned(),
                purpose: toolkit.purpose.to_owned(),
                connection_model: toolkit.connection_model.to_owned(),
                identifier_semantics: toolkit
                    .identifier_semantics
                    .iter()
                    .map(|v| (*v).to_owned())
                    .collect(),
                capability_categories: toolkit
                    .capability_categories
                    .iter()
                    .map(|v| (*v).to_owned())
                    .collect(),
                gotchas: toolkit.gotchas.iter().map(|v| (*v).to_owned()).collect(),
                operations,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use onecrew_local_runtime::{OperationId, OperationSummary, ToolkitSummary};

    fn mask_search_plan_guidance(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Array(values) => serde_json::Value::Array(
                values.into_iter().map(mask_search_plan_guidance).collect(),
            ),
            serde_json::Value::Object(fields) => {
                let is_search_plan = fields.get("id").and_then(serde_json::Value::as_str)
                    == Some(OperationId::ONECREW_SEARCH_PLAN_PREPARE.as_str());
                serde_json::Value::Object(
                    fields
                        .into_iter()
                        .map(|(key, value)| {
                            if is_search_plan && key == "summary" {
                                (
                                    key,
                                    serde_json::Value::String(
                                        "<current search-plan guidance>".to_owned(),
                                    ),
                                )
                            } else {
                                (key, mask_search_plan_guidance(value))
                            }
                        })
                        .collect(),
                )
            }
            value => value,
        }
    }

    fn search_plan_summary(value: &serde_json::Value) -> Option<&str> {
        match value {
            serde_json::Value::Array(values) => values.iter().find_map(search_plan_summary),
            serde_json::Value::Object(fields) => {
                if fields.get("id").and_then(serde_json::Value::as_str)
                    == Some(OperationId::ONECREW_SEARCH_PLAN_PREPARE.as_str())
                {
                    fields.get("summary").and_then(serde_json::Value::as_str)
                } else {
                    fields.values().find_map(search_plan_summary)
                }
            }
            _ => None,
        }
    }

    #[test]
    fn real_http_directory_matches_the_frozen_navigation_contract() {
        let catalog = CapabilityCatalog::build().unwrap();
        let value = serde_json::to_value(project_directory(&catalog)).unwrap();
        let mut historical = value.clone();
        let operations = historical
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|toolkit| toolkit["id"] == "toolkit.onecrew")
            .unwrap()["operations"]
            .as_array_mut()
            .unwrap();
        assert_eq!(
            operations
                .iter()
                .filter(|operation| operation["id"] == "onecrew.discovery.draft.prepare")
                .count(),
            1
        );
        operations.retain(|operation| operation["id"] != "onecrew.discovery.draft.prepare");
        // Assert the current target adjustment before comparing the historical export.
        let target = operations
            .iter_mut()
            .find(|operation| operation["id"] == "onecrew.discovery.target.increase")
            .unwrap();
        assert_eq!(
            *target,
            serde_json::json!({"id":"onecrew.discovery.target.increase","title":"Adjust discovery Campaign target","summary":"Record an authenticated, idempotent decision to raise or lower the approved Campaign target while preserving frozen criteria, sources and read budget."})
        );
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../../platform-runtime/src/catalog/fixtures/directory-full.json"
        ))
        .unwrap();
        let current_summary = catalog
            .operation(OperationId::ONECREW_SEARCH_PLAN_PREPARE.as_str())
            .unwrap()
            .summary;
        assert_eq!(search_plan_summary(&value), Some(current_summary));
        assert_eq!(
            mask_search_plan_guidance(historical),
            mask_search_plan_guidance(expected)
        );
        for toolkit in value.as_array().unwrap() {
            assert_eq!(toolkit.as_object().unwrap().len(), 10);
            for operation in toolkit["operations"].as_array().unwrap() {
                assert_eq!(operation.as_object().unwrap().len(), 3);
                for key in ["id", "title", "summary"] {
                    assert!(operation[key].is_string());
                }
            }
        }
    }

    #[test]
    fn navigation_support_does_not_require_or_expose_a_mcp_binding() {
        static TOOLKITS: [ToolkitSummary; 1] = [ToolkitSummary {
            id: "toolkit.x",
            toolkit: ToolkitId::X,
            title: "X",
            summary: "Fixture toolkit",
            purpose: "Fixture navigation",
            connection_model: "Fixture connection",
            identifier_semantics: &[],
            capability_categories: &[],
            gotchas: &[],
        }];
        static OPERATIONS: [OperationSummary; 2] = [
            OperationSummary {
                id: OperationId::from_static("x.navigation"),
                toolkit_id: ToolkitId::X,
                title: "Navigation",
                summary: "Navigation only",
                surfaces: &[CatalogSurface::WebNavigation],
            },
            OperationSummary {
                id: OperationId::from_static("x.mcp_only"),
                toolkit_id: ToolkitId::X,
                title: "MCP",
                summary: "MCP only",
                surfaces: &[CatalogSurface::McpBinding],
            },
        ];
        let catalog = CapabilityCatalog::from_static(&TOOLKITS, &OPERATIONS).unwrap();
        let value = serde_json::to_value(project_directory(&catalog)).unwrap();
        assert_eq!(value[0]["operations"].as_array().unwrap().len(), 1);
        assert_eq!(value[0]["operations"][0]["id"], "x.navigation");
        assert!(!serde_json::to_string(&value).unwrap().contains("mcp_only"));
    }
}
