use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::models::{CreatorId, EmailTemplateId};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmailTemplate {
    pub id: EmailTemplateId,
    pub name: String,
    pub subject: String,
    pub text: String,
    pub html: Option<String>,
    pub variables: Vec<String>,
    pub revision: u64,
    pub created_at: u64,
    pub updated_at: u64,
}

/// List-row projection of one template: identity and declared variables
/// without the template bodies.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmailTemplateSummary {
    pub id: EmailTemplateId,
    pub name: String,
    pub subject: String,
    pub variables: Vec<String>,
    pub revision: u64,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListEmailTemplatesInput {
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmailTemplateGetInput {
    pub template_id: EmailTemplateId,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SaveEmailTemplateInput {
    pub id: Option<EmailTemplateId>,
    pub name: String,
    pub subject: String,
    pub text: String,
    pub html: Option<String>,
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteEmailTemplateInput {
    pub template_id: EmailTemplateId,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderEmailTemplateInput {
    pub template_id: EmailTemplateId,
    pub creator_id: Option<CreatorId>,
    #[serde(default)]
    pub variables: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RenderedEmailTemplate {
    pub template_id: EmailTemplateId,
    pub revision: u64,
    pub subject: String,
    pub text: String,
    pub html: Option<String>,
    pub unresolved: Vec<String>,
}
