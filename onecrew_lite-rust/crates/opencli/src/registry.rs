use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::OpenCliError;

#[derive(Debug, Clone, Deserialize)]
struct RegisteredCommand {
    command: String,
    site: String,
    name: String,
    access: String,
    browser: bool,
    strategy: Option<String>,
}

pub(super) struct Registry {
    commands: Vec<RegisteredCommand>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OpenCliSiteInventory {
    pub site: String,
    pub read_commands: u32,
    pub write_commands: u32,
    pub browser_reads: u32,
    pub read_names: Vec<String>,
    pub write_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OpenCliInventory {
    pub version: String,
    pub command_count: usize,
    pub sites: Vec<OpenCliSiteInventory>,
}

impl Registry {
    pub(super) fn parse(json: &str) -> Result<Self, OpenCliError> {
        let commands: Vec<RegisteredCommand> =
            serde_json::from_str(json).map_err(|_| OpenCliError::InvalidOutput)?;
        if commands.is_empty()
            || commands.iter().any(|command| {
                command.command != format!("{}/{}", command.site, command.name)
                    || !matches!(command.access.as_str(), "read" | "write")
            })
        {
            return Err(OpenCliError::InvalidOutput);
        }
        Ok(Self { commands })
    }

    pub(super) fn has_browser_read(&self, site: &str, name: &str) -> bool {
        self.commands.iter().any(|command| {
            command.site == site
                && command.name == name
                && command.access == "read"
                && command.browser
        })
    }

    pub(super) fn has_public_read(&self, site: &str, name: &str) -> bool {
        self.commands.iter().any(|command| {
            command.site == site
                && command.name == name
                && command.access == "read"
                && !command.browser
                && command.strategy.as_deref() == Some("public")
        })
    }

    pub(super) fn inventory(&self, version: &str) -> OpenCliInventory {
        let mut sites = BTreeMap::<&str, OpenCliSiteInventory>::new();
        for command in &self.commands {
            let entry = sites
                .entry(&command.site)
                .or_insert_with(|| OpenCliSiteInventory {
                    site: command.site.clone(),
                    read_commands: 0,
                    write_commands: 0,
                    browser_reads: 0,
                    read_names: Vec::new(),
                    write_names: Vec::new(),
                });
            if command.access == "read" {
                entry.read_commands += 1;
                entry.read_names.push(command.name.clone());
                if command.browser {
                    entry.browser_reads += 1;
                }
            } else {
                entry.write_commands += 1;
                entry.write_names.push(command.name.clone());
            }
        }
        for site in sites.values_mut() {
            site.read_names.sort();
            site.write_names.sort();
        }
        OpenCliInventory {
            version: version.to_owned(),
            command_count: self.commands.len(),
            sites: sites.into_values().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_reads_require_explicit_public_strategy_and_no_browser() {
        for (access, browser, strategy, expected) in [
            ("read", false, "public", true),
            ("write", false, "public", false),
            ("read", true, "public", false),
            ("read", false, "cookie", false),
        ] {
            let registry = Registry::parse(
                &serde_json::json!([{
                    "command": "bluesky/search", "site": "bluesky", "name": "search",
                    "access": access, "browser": browser, "strategy": strategy,
                }])
                .to_string(),
            )
            .unwrap();
            assert_eq!(registry.has_public_read("bluesky", "search"), expected);
            assert!(!registry.has_public_read("bluesky", "post"));
        }
    }

    #[test]
    fn write_command_cannot_pass_browser_read_gate() {
        let registry = Registry::parse(
            r#"[{"command":"youtube/search","site":"youtube","name":"search","access":"read","browser":true},{"command":"youtube/like","site":"youtube","name":"like","access":"write","browser":true}]"#,
        )
        .expect("registry");
        assert!(registry.has_browser_read("youtube", "search"));
        assert!(!registry.has_browser_read("youtube", "like"));
        assert_eq!(registry.inventory("1.8.8").sites[0].write_commands, 1);
        assert_eq!(registry.inventory("1.8.8").sites[0].read_names, ["search"]);
    }
}
