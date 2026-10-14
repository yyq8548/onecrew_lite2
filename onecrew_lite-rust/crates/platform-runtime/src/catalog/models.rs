use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// A platform identity shared by navigation and typed capability descriptions.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ToolkitId {
    X,
    Instagram,
    Telegram,
    Youtube,
    #[serde(rename = "onecrew")]
    OneCrew,
    Email,
}

impl ToolkitId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Instagram => "instagram",
            Self::Telegram => "telegram",
            Self::Youtube => "youtube",
            Self::OneCrew => "onecrew",
            Self::Email => "email",
        }
    }
}

/// A static declaration identity, not an execution handle or authorization.
///
/// Production identities are declared once alongside their descriptions. The
/// explicit constructor also supports isolated consumer fixtures; catalog
/// construction validates every identity before exposing the directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OperationId(&'static str);

impl OperationId {
    pub const fn from_static(value: &'static str) -> Self {
        Self(value)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// Declared consumers of static capability descriptions.
///
/// Navigation support says nothing about a Web command's existence or the
/// authorization required by an owner. MCP protocol scopes remain in MCP.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogSurface {
    McpBinding,
    WebNavigation,
}

/// Static platform background used by every directory projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolkitSummary {
    pub id: &'static str,
    pub toolkit: ToolkitId,
    pub title: &'static str,
    pub summary: &'static str,
    pub purpose: &'static str,
    pub connection_model: &'static str,
    pub identifier_semantics: &'static [&'static str],
    pub capability_categories: &'static [&'static str],
    pub gotchas: &'static [&'static str],
}

/// Read-only capability identity and navigation text shared with bindings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperationSummary {
    pub id: OperationId,
    pub toolkit_id: ToolkitId,
    pub title: &'static str,
    pub summary: &'static str,
    pub surfaces: &'static [CatalogSurface],
}

impl OperationSummary {
    pub fn supports(&self, surface: CatalogSurface) -> bool {
        self.surfaces.contains(&surface)
    }
}
