//! Human setup pages open in installed Chrome, independently of its extension.
use std::{path::Path, process::Stdio};

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::{ChromeError, detect_chrome};

/// Fixed destinations only; Web cannot supply a URL or browser launch flags.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChromeSetupPage {
    Download,
    Extension,
}

impl ChromeSetupPage {
    fn url(self) -> &'static str {
        match self {
            Self::Download => "https://opencli.info/download/",
            Self::Extension => {
                "https://chromewebstore.google.com/detail/opencli/ildkmabpimmkaediidaifkhjpohdnifk"
            }
        }
    }
}

fn setup_command(executable: &Path, page: ChromeSetupPage) -> Command {
    let mut command = Command::new(executable);
    // Let Chrome own its ordinary profile selection. No user-data-dir, cookie
    // access, or guessed mapping from OpenCLI's opaque browser identity.
    command
        .arg(page.url())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

/// Receipt means launch accepted, not extension installed or profile verified.
pub async fn open_setup_page(page: ChromeSetupPage) -> Result<(), ChromeError> {
    let executable = detect_chrome().ok_or(ChromeError::BrowserNotFound)?;
    let mut child = setup_command(&executable, page).spawn()?;
    // Chrome may stay open until the human closes it. Reap it without blocking
    // the HTTP request or terminating the user's browser when the request ends.
    tokio::spawn(async move {
        if child.wait().await.is_err() {
            log::warn!("Chrome setup process could not be reaped");
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_launch_cannot_accept_arbitrary_urls_or_profile_flags() {
        assert!(serde_json::from_str::<ChromeSetupPage>("\"https://example.com\"").is_err());
        assert!(serde_json::from_str::<ChromeSetupPage>("\"--user-data-dir=/private\"").is_err());
        for page in [ChromeSetupPage::Download, ChromeSetupPage::Extension] {
            let command = setup_command(Path::new("/installed/chrome"), page);
            assert_eq!(command.as_std().get_program(), "/installed/chrome");
            assert_eq!(
                command.as_std().get_args().collect::<Vec<_>>(),
                [page.url()]
            );
        }
    }
}
