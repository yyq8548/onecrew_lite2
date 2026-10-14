use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ContactChannel, CreatorChangeScope, DeleteEmailTemplateInput, EmailTemplate,
    EmailTemplateGetInput, RenderEmailTemplateInput, RenderedEmailTemplate, SaveEmailTemplateInput,
};

impl crate::CreatorRuntime {
    /// Run this use case under bounded read admission before blocking dispatch.
    pub async fn list_email_templates_async(
        &self,
        input: crate::ListEmailTemplatesInput,
    ) -> Result<crate::Page<crate::EmailTemplateSummary>, crate::CreatorRepositoryCallError<String>>
    {
        let owner = self.clone();
        self.repository_read_segment(move || owner.list_email_templates(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::list_email_templates_async`] to own admission and dispatch.
    pub fn list_email_templates(
        &self,
        input: crate::ListEmailTemplatesInput,
    ) -> Result<crate::Page<crate::EmailTemplateSummary>, String> {
        self.repository.list_email_templates(
            crate::MAIL_PAGE_LIMITS.effective(input.limit),
            input.cursor.as_deref(),
        )
    }

    /// Run this use case under bounded read admission before blocking dispatch.
    pub async fn get_email_template_async(
        &self,
        input: EmailTemplateGetInput,
    ) -> Result<EmailTemplate, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_read_segment(move || owner.get_email_template(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::get_email_template_async`] to own admission and dispatch.
    pub fn get_email_template(
        &self,
        input: EmailTemplateGetInput,
    ) -> Result<EmailTemplate, String> {
        self.repository.get_email_template(&input.template_id)
    }

    /// Run this use case under bounded write admission before blocking dispatch.
    pub async fn save_email_template_async(
        &self,
        input: SaveEmailTemplateInput,
    ) -> Result<EmailTemplate, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.save_email_template(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::save_email_template_async`] to own admission and dispatch.
    pub fn save_email_template(
        &self,
        input: SaveEmailTemplateInput,
    ) -> Result<EmailTemplate, String> {
        validate_template_body(&input.subject, &input.text, input.html.as_deref())?;
        if let Some(html) = input.html.as_ref()
            && !parse_template_keys(html)?.is_empty()
        {
            return Err(
                "email template variables are not allowed in html; use static html or subject/text variables"
                    .into(),
            );
        }
        let variables = collect_template_variables([&input.subject, &input.text])?;
        let template = self
            .repository
            .save_email_template(input, variables.into_iter().collect())?;
        self.publish_changes([CreatorChangeScope::EmailTemplates]);
        Ok(template)
    }

    /// Run this use case under bounded write admission before blocking dispatch.
    pub async fn delete_email_template_async(
        &self,
        input: DeleteEmailTemplateInput,
    ) -> Result<bool, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.delete_email_template(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::delete_email_template_async`] to own admission and dispatch.
    pub fn delete_email_template(&self, input: DeleteEmailTemplateInput) -> Result<bool, String> {
        let deleted = self.repository.delete_email_template(input)?;
        if deleted {
            self.publish_changes([CreatorChangeScope::EmailTemplates]);
        }
        Ok(deleted)
    }

    /// Read immutable inputs before entering the bounded rendering segment.
    pub async fn render_email_template_async(
        &self,
        input: RenderEmailTemplateInput,
    ) -> Result<RenderedEmailTemplate, crate::CreatorRepositoryCallError<String>> {
        let input = self
            .repository_cpu_segment(move || {
                for key in input.variables.keys() {
                    validate_template_key(key)?;
                }
                Ok(input)
            })
            .await?;
        let repository = std::sync::Arc::clone(&self.repository);
        let template_id = input.template_id.clone();
        let template = self
            .repository_read_segment(move || repository.get_email_template(&template_id))
            .await?;
        let record = if let Some(creator_id) = input.creator_id.clone() {
            Some(self.get_creator_async(creator_id).await?)
        } else {
            None
        };
        self.repository_cpu_segment(move || {
            let mut values = email_template_builtins_for_record(record.as_ref());
            values.extend(input.variables);
            render_email_template_values(template, values)
        })
        .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::render_email_template_async`] to own admission and dispatch.
    pub fn render_email_template(
        &self,
        input: RenderEmailTemplateInput,
    ) -> Result<RenderedEmailTemplate, String> {
        for key in input.variables.keys() {
            validate_template_key(key)?;
        }
        let template = self.repository.get_email_template(&input.template_id)?;
        let mut values = self.email_template_builtins(input.creator_id.as_ref())?;
        values.extend(input.variables);
        render_email_template_values(template, values)
    }

    fn email_template_builtins(
        &self,
        creator_id: Option<&crate::CreatorId>,
    ) -> Result<BTreeMap<String, String>, String> {
        let Some(creator_id) = creator_id else {
            return Ok(BTreeMap::new());
        };
        let record = self.repository.get_creator(creator_id)?;
        Ok(email_template_builtins_for_record(Some(&record)))
    }
}

fn render_email_template_values(
    template: EmailTemplate,
    values: BTreeMap<String, String>,
) -> Result<RenderedEmailTemplate, String> {
    let mut unresolved = BTreeSet::new();
    let subject = render_template_value(&template.subject, &values, &mut unresolved)?;
    let text = render_template_value(&template.text, &values, &mut unresolved)?;
    Ok(RenderedEmailTemplate {
        template_id: template.id,
        revision: template.revision,
        subject,
        text,
        html: template.html,
        unresolved: unresolved.into_iter().collect(),
    })
}

pub(crate) fn email_template_builtins_for_record(
    record: Option<&crate::CreatorRecord>,
) -> BTreeMap<String, String> {
    let Some(record) = record else {
        return BTreeMap::new();
    };
    let confirmed_accounts = record
        .account_links
        .iter()
        .filter(|link| link.state == crate::LinkState::Confirmed)
        .map(|link| link.platform_account_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut latest_handles = BTreeMap::new();
    for observation in &record.account_observations {
        if confirmed_accounts.contains(observation.platform_account_id.as_str())
            && let Some(handle) = observation.handle.as_deref().map(str::trim)
            && !handle.is_empty()
        {
            latest_handles
                .entry(observation.platform_account_id.as_str().to_owned())
                .or_insert_with(|| handle.to_owned());
        }
    }
    let handles = latest_handles.into_values().collect::<BTreeSet<_>>();
    let emails = record
        .contact_routes
        .iter()
        .filter(|route| route.locator.channel == ContactChannel::Email)
        .map(|route| route.locator.normalized_value.clone())
        .collect::<BTreeSet<_>>();
    email_template_builtins_from_values(&record.creator.display_name, &handles, &emails)
}

pub(crate) fn email_template_builtins_from_values(
    display_name: &str,
    handles: &BTreeSet<String>,
    emails: &BTreeSet<String>,
) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    values.insert("name".into(), display_name.to_owned());
    if let Some(first_name) = display_name.split_whitespace().next()
        && !first_name.is_empty()
    {
        values.insert("first_name".into(), first_name.to_owned());
    }
    if handles.len() == 1 {
        values.insert("handle".into(), handles.iter().next().unwrap().clone());
    }
    if emails.len() == 1 {
        values.insert("email".into(), emails.iter().next().unwrap().clone());
    }
    values
}

fn validate_template_body(subject: &str, text: &str, html: Option<&str>) -> Result<(), String> {
    if subject.trim().is_empty() {
        return Err("email template subject is required".into());
    }
    if text.trim().is_empty() && html.map(str::trim).unwrap_or_default().is_empty() {
        return Err("email template text or html body is required".into());
    }
    Ok(())
}

fn collect_template_variables<'a>(
    templates: impl IntoIterator<Item = &'a String>,
) -> Result<BTreeSet<String>, String> {
    let mut variables = BTreeSet::new();
    for template in templates {
        variables.extend(parse_template_keys(template)?);
    }
    Ok(variables)
}

fn parse_template_keys(template: &str) -> Result<Vec<String>, String> {
    let mut keys = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find("{{") {
        if rest[..open].contains("}}") {
            return Err("email template contains an unmatched closing delimiter".into());
        }
        let after_open = &rest[open + 2..];
        let close = after_open
            .find("}}")
            .ok_or_else(|| "email template contains an unmatched opening delimiter".to_string())?;
        let key = &after_open[..close];
        validate_template_key(key)?;
        keys.push(key.to_owned());
        rest = &after_open[close + 2..];
    }
    if rest.contains("}}") {
        return Err("email template contains an unmatched closing delimiter".into());
    }
    Ok(keys)
}

pub(crate) fn validate_template_key(key: &str) -> Result<(), String> {
    let mut characters = key.chars();
    let valid_first = characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_');
    let valid_rest = characters
        .all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '.');
    if valid_first && valid_rest {
        Ok(())
    } else {
        Err(format!("invalid email template variable: {key}"))
    }
}

pub(crate) fn render_template_value(
    template: &str,
    values: &BTreeMap<String, String>,
    unresolved: &mut BTreeSet<String>,
) -> Result<String, String> {
    parse_template_keys(template)?;
    let mut rendered = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find("{{") {
        rendered.push_str(&rest[..open]);
        let after_open = &rest[open + 2..];
        let close = after_open
            .find("}}")
            .expect("template grammar was validated");
        let key = &after_open[..close];
        match values.get(key) {
            Some(value) => rendered.push_str(value),
            None => {
                unresolved.insert(key.to_owned());
                rendered.push_str("{{");
                rendered.push_str(key);
                rendered.push_str("}}");
            }
        }
        rest = &after_open[close + 2..];
    }
    rendered.push_str(rest);
    Ok(rendered)
}
