use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use uuid::Uuid;

use crate::{
    DeleteEmailTemplateInput, EmailTemplate, EmailTemplateId, SaveEmailTemplateInput,
    repository::EmailTemplateRepository,
};

use super::{
    super::{SqliteRepository, json, now},
    revision::{conversion_error, current_revision, ensure_revision_change, revision_conflict},
};

impl EmailTemplateRepository for SqliteRepository {
    fn list_email_templates(
        &self,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<crate::Page<crate::EmailTemplateSummary>, String> {
        let context = "email_template_list";
        let position = cursor
            .map(|cursor| super::super::decode_keyset_single(cursor, context))
            .transpose()?;
        let mut connection = self.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        let mut statement = transaction
            .prepare(
                "SELECT id FROM email_templates
                 WHERE (?1 IS NULL OR updated_at < ?1
                   OR (updated_at = ?1 AND id < ?2))
                 ORDER BY updated_at DESC, id DESC
                 LIMIT ?3",
            )
            .map_err(|error| error.to_string())?;
        let ids = statement
            .query_map(
                rusqlite::params![
                    position.as_ref().map(|(updated_at, _)| updated_at),
                    position.as_ref().map(|(_, id)| id.as_str()),
                    u64::from(limit) + 1
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        drop(statement);
        let summaries = ids
            .into_iter()
            .map(|id| {
                load_template(&transaction, &EmailTemplateId::new(id)).map(|template| {
                    crate::EmailTemplateSummary {
                        id: template.id,
                        name: template.name,
                        subject: template.subject,
                        variables: template.variables,
                        revision: template.revision,
                        created_at: template.created_at,
                        updated_at: template.updated_at,
                    }
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let page = super::super::keyset_page(summaries, limit as usize, context, |summary| {
            (vec![summary.updated_at], summary.id.as_str())
        });
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(page)
    }

    fn get_email_template(&self, template_id: &EmailTemplateId) -> Result<EmailTemplate, String> {
        let connection = self.read_connection()?;
        load_template(&connection, template_id)
    }

    fn save_email_template(
        &self,
        input: SaveEmailTemplateInput,
        variables: Vec<String>,
    ) -> Result<EmailTemplate, String> {
        let name = input.name.trim();
        if name.is_empty() {
            return Err("email template name is required".into());
        }
        let timestamp = now();
        let connection = self.lock()?;
        let id = match input.id {
            None => {
                if input.expected_revision.is_some() {
                    return Err("new email template must not include expected_revision".into());
                }
                let id = EmailTemplateId::new(Uuid::new_v4().to_string());
                connection
                    .execute(
                        "INSERT INTO email_templates
                           (id, name, subject_template, text_template, html_template,
                            variables_json, revision, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?7)",
                        params![
                            id.as_str(),
                            name,
                            input.subject,
                            input.text,
                            input.html,
                            json(&variables)?,
                            timestamp
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                id
            }
            Some(id) => {
                let expected_revision = input.expected_revision.ok_or_else(|| {
                    "existing email template requires expected_revision".to_string()
                })?;
                let changed = connection
                    .execute(
                        "UPDATE email_templates
                         SET name = ?3, subject_template = ?4, text_template = ?5,
                             html_template = ?6, variables_json = ?7,
                             revision = revision + 1, updated_at = ?8
                         WHERE id = ?1 AND revision = ?2",
                        params![
                            id.as_str(),
                            expected_revision,
                            name,
                            input.subject,
                            input.text,
                            input.html,
                            json(&variables)?,
                            timestamp
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                ensure_revision_change(
                    &connection,
                    "email_templates",
                    id.as_str(),
                    expected_revision,
                    changed,
                    "email template",
                )?;
                id
            }
        };
        load_template(&connection, &id)
    }

    fn delete_email_template(&self, input: DeleteEmailTemplateInput) -> Result<bool, String> {
        let connection = self.lock()?;
        let changed = connection
            .execute(
                "DELETE FROM email_templates WHERE id = ?1 AND revision = ?2",
                params![input.template_id.as_str(), input.expected_revision],
            )
            .map_err(|error| error.to_string())?;
        if changed == 1 {
            return Ok(true);
        }
        let revision =
            current_revision(&connection, "email_templates", input.template_id.as_str())?;
        match revision {
            None => Ok(false),
            Some(actual) => Err(revision_conflict(
                "email template",
                input.expected_revision,
                actual,
            )),
        }
    }
}

fn load_template(connection: &Connection, id: &EmailTemplateId) -> Result<EmailTemplate, String> {
    connection
        .query_row(
            "SELECT name, subject_template, text_template, html_template, variables_json,
                    revision, created_at, updated_at
             FROM email_templates WHERE id = ?1",
            [id.as_str()],
            |row| {
                let variables_json = row.get::<_, String>(4)?;
                Ok(EmailTemplate {
                    id: id.clone(),
                    name: row.get(0)?,
                    subject: row.get(1)?,
                    text: row.get(2)?,
                    html: row.get(3)?,
                    variables: serde_json::from_str(&variables_json)
                        .map_err(|error| conversion_error(error.to_string()))?,
                    revision: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "email template not found".into())
}
