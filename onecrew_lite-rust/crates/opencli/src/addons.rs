//! Explicit setup of bundled read-only add-ons. Missing-only installation never
//! replaces existing files. Explicit repair preserves old owned folders outside
//! the active registry before replacing them; unrelated add-ons stay untouched.
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::{OpenCli, OpenCliError};

struct Addon {
    id: &'static str,
    // Publish the command entry last; an interrupted core write is not loadable.
    files: &'static [(&'static str, &'static [u8])],
}

const ADDONS: &[Addon] = &[
    Addon {
        id: "onecrew-youtube",
        files: &[
            (
                "language.mjs",
                include_bytes!("../../../integrations/opencli/onecrew-youtube/language.mjs"),
            ),
            (
                "language.js",
                include_bytes!("../../../integrations/opencli/onecrew-youtube/language.js"),
            ),
            (
                "evidence.mjs",
                include_bytes!("../../../integrations/opencli/onecrew-youtube/evidence.mjs"),
            ),
            (
                "channel.mjs",
                include_bytes!("../../../integrations/opencli/onecrew-youtube/channel.mjs"),
            ),
            (
                "channel-contact.js",
                include_bytes!("../../../integrations/opencli/onecrew-youtube/channel-contact.js"),
            ),
            (
                "video-evidence.js",
                include_bytes!("../../../integrations/opencli/onecrew-youtube/video-evidence.js"),
            ),
        ],
    },
    Addon {
        id: "onecrew-tiktok",
        files: &[
            (
                "search.mjs",
                include_bytes!("../../../integrations/opencli/onecrew-tiktok/search.mjs"),
            ),
            (
                "search.js",
                include_bytes!("../../../integrations/opencli/onecrew-tiktok/search.js"),
            ),
        ],
    },
    Addon {
        id: "onecrew-bilibili",
        files: &[
            (
                "search.mjs",
                include_bytes!("../../../integrations/opencli/onecrew-bilibili/search.mjs"),
            ),
            (
                "search.js",
                include_bytes!("../../../integrations/opencli/onecrew-bilibili/search.js"),
            ),
        ],
    },
    Addon {
        id: "onecrew-tiktok-profile",
        files: &[
            (
                "profile.mjs",
                include_bytes!("../../../integrations/opencli/onecrew-tiktok-profile/profile.mjs"),
            ),
            (
                "read.js",
                include_bytes!("../../../integrations/opencli/onecrew-tiktok-profile/read.js"),
            ),
        ],
    },
    Addon {
        id: "onecrew-bilibili-profile",
        files: &[
            (
                "profile.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-bilibili-profile/profile.mjs"
                ),
            ),
            (
                "read.js",
                include_bytes!("../../../integrations/opencli/onecrew-bilibili-profile/read.js"),
            ),
        ],
    },
    Addon {
        id: "onecrew-bilibili-video",
        files: &[
            (
                "language-page.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-bilibili-video/language-page.mjs"
                ),
            ),
            (
                "language.mjs",
                include_bytes!("../../../integrations/opencli/onecrew-bilibili-video/language.mjs"),
            ),
            (
                "language.js",
                include_bytes!("../../../integrations/opencli/onecrew-bilibili-video/language.js"),
            ),
            (
                "video.mjs",
                include_bytes!("../../../integrations/opencli/onecrew-bilibili-video/video.mjs"),
            ),
            (
                "read.js",
                include_bytes!("../../../integrations/opencli/onecrew-bilibili-video/read.js"),
            ),
        ],
    },
    Addon {
        id: "onecrew-twitter-identity",
        files: &[
            (
                "profile.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-twitter-identity/profile.mjs"
                ),
            ),
            (
                "read.js",
                include_bytes!("../../../integrations/opencli/onecrew-twitter-identity/read.js"),
            ),
        ],
    },
    Addon {
        id: "onecrew-twitter-post-identity",
        files: &[
            (
                "post.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-twitter-post-identity/post.mjs"
                ),
            ),
            (
                "read.js",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-twitter-post-identity/read.js"
                ),
            ),
        ],
    },
    Addon {
        id: "onecrew-twitter-post-content",
        files: &[
            (
                "language.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-twitter-post-content/language.mjs"
                ),
            ),
            (
                "content.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-twitter-post-content/content.mjs"
                ),
            ),
            (
                "read.js",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-twitter-post-content/read.js"
                ),
            ),
        ],
    },
    Addon {
        id: "onecrew-site-auth",
        files: &[
            (
                "probe.mjs",
                include_bytes!("../../../integrations/opencli/onecrew-site-auth/probe.mjs"),
            ),
            (
                "whoami.js",
                include_bytes!("../../../integrations/opencli/onecrew-site-auth/whoami.js"),
            ),
        ],
    },
    Addon {
        id: "onecrew-substack",
        files: &[
            (
                "search.mjs",
                include_bytes!("../../../integrations/opencli/onecrew-substack/search.mjs"),
            ),
            (
                "search.js",
                include_bytes!("../../../integrations/opencli/onecrew-substack/search.js"),
            ),
        ],
    },
    Addon {
        id: "onecrew-source-search",
        files: &[
            (
                "search.mjs",
                include_bytes!("../../../integrations/opencli/onecrew-source-search/search.mjs"),
            ),
            (
                "search.js",
                include_bytes!("../../../integrations/opencli/onecrew-source-search/search.js"),
            ),
        ],
    },
    Addon {
        id: "onecrew-browser-search",
        files: &[
            (
                "douyin-evidence.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/douyin-evidence.mjs"
                ),
            ),
            (
                "douyin-comments.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/douyin-comments.mjs"
                ),
            ),
            (
                "douyin-profile.js",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/douyin-profile.js"
                ),
            ),
            (
                "linkedin-search.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/linkedin-search.mjs"
                ),
            ),
            (
                "linkedin-evidence.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/linkedin-evidence.mjs"
                ),
            ),
            (
                "linkedin-comments.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/linkedin-comments.mjs"
                ),
            ),
            (
                "linkedin-profile.js",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/linkedin-profile.js"
                ),
            ),
            (
                "pinterest-evidence.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/pinterest-evidence.mjs"
                ),
            ),
            (
                "pinterest-profile.js",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/pinterest-profile.js"
                ),
            ),
            (
                "weibo-read.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/weibo-read.mjs"
                ),
            ),
            (
                "weibo-read.js",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/weibo-read.js"
                ),
            ),
            (
                "weibo-post.js",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/weibo-post.js"
                ),
            ),
            (
                "instagram-comments.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/instagram-comments.mjs"
                ),
            ),
            (
                "instagram-comments.js",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/instagram-comments.js"
                ),
            ),
            (
                "facebook-comments.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/facebook-comments.mjs"
                ),
            ),
            (
                "facebook-reel-comments.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/facebook-reel-comments.mjs"
                ),
            ),
            (
                "facebook-recent.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/facebook-recent.mjs"
                ),
            ),
            (
                "facebook-comments.js",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/facebook-comments.js"
                ),
            ),
            (
                "facebook-profile.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/facebook-profile.mjs"
                ),
            ),
            (
                "facebook-identity.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/facebook-identity.mjs"
                ),
            ),
            (
                "facebook-native-profile.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/facebook-native-profile.mjs"
                ),
            ),
            (
                "facebook-native-original.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/facebook-native-original.mjs"
                ),
            ),
            (
                "facebook-profile.js",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/facebook-profile.js"
                ),
            ),
            (
                "instagram-profile.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/instagram-profile.mjs"
                ),
            ),
            (
                "instagram-profile.js",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/instagram-profile.js"
                ),
            ),
            (
                "tiktok-comments.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/tiktok-comments.mjs"
                ),
            ),
            (
                "tiktok-comments.js",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/tiktok-comments.js"
                ),
            ),
            (
                "tiktok-language-recent.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/tiktok-language-recent.mjs"
                ),
            ),
            (
                "tiktok-language-recent.js",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/tiktok-language-recent.js"
                ),
            ),
            (
                "reddit-comments.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/reddit-comments.mjs"
                ),
            ),
            (
                "reddit-profile.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/reddit-profile.mjs"
                ),
            ),
            (
                "reddit-profile.js",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/reddit-profile.js"
                ),
            ),
            (
                "reddit-recent.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/reddit-recent.mjs"
                ),
            ),
            (
                "reddit-recent.js",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/reddit-recent.js"
                ),
            ),
            (
                "reddit-search.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/reddit-search.mjs"
                ),
            ),
            (
                "reddit-search.js",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/reddit-search.js"
                ),
            ),
            (
                "instagram-search.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/instagram-search.mjs"
                ),
            ),
            (
                "instagram-search.js",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/instagram-search.js"
                ),
            ),
            (
                "instagram-recent.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/instagram-recent.mjs"
                ),
            ),
            (
                "instagram-recent.js",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/instagram-recent.js"
                ),
            ),
            (
                "comments.mjs",
                include_bytes!("../../../integrations/opencli/onecrew-browser-search/comments.mjs"),
            ),
            (
                "comments.js",
                include_bytes!("../../../integrations/opencli/onecrew-browser-search/comments.js"),
            ),
            (
                "original-language.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/original-language.mjs"
                ),
            ),
            (
                "owned-page.mjs",
                include_bytes!(
                    "../../../integrations/opencli/onecrew-browser-search/owned-page.mjs"
                ),
            ),
            (
                "search.js",
                include_bytes!("../../../integrations/opencli/onecrew-browser-search/search.js"),
            ),
        ],
    },
    Addon {
        id: "onecrew-public-search",
        files: &[
            (
                "read.mjs",
                include_bytes!("../../../integrations/opencli/onecrew-public-search/read.mjs"),
            ),
            (
                "search.js",
                include_bytes!("../../../integrations/opencli/onecrew-public-search/search.js"),
            ),
        ],
    },
    Addon {
        id: "onecrew-bluesky",
        files: &[
            (
                "evidence.mjs",
                include_bytes!("../../../integrations/opencli/onecrew-bluesky/evidence.mjs"),
            ),
            (
                "read.js",
                include_bytes!("../../../integrations/opencli/onecrew-bluesky/read.js"),
            ),
        ],
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenCliAddonState {
    Missing,
    Ready,
    Conflict,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenCliAddonStatus {
    pub id: &'static str,
    pub state: OpenCliAddonState,
}

impl OpenCli {
    /// Inspect local files only. No installation, browser activity or provider read.
    pub async fn addon_status(&self) -> Vec<OpenCliAddonStatus> {
        tokio::task::spawn_blocking(|| match user_home() {
            Some(home) => inspect(&home),
            None => unavailable(),
        })
        .await
        .unwrap_or_else(|_| unavailable())
    }

    /// A caller must explicitly approve this local setup effect. Only missing
    /// bundled directories may be created; conflicts leave every directory alone.
    pub async fn install_bundled_addons(
        &self,
        approved: bool,
    ) -> Result<Vec<OpenCliAddonStatus>, OpenCliError> {
        if !approved {
            return Err(OpenCliError::InvalidInput("approved"));
        }
        if self.version != "1.8.8" {
            return Err(OpenCliError::VersionChanged);
        }
        tokio::task::spawn_blocking(|| match user_home() {
            Some(home) => install(&home),
            None => unavailable(),
        })
        .await
        .map_err(|_| OpenCliError::Unavailable)
    }

    /// Explicit repair of the fixed OneCrew add-on inventory. Preserve old
    /// folders outside `clis`, including unknown files, rather than deleting them.
    pub async fn repair_bundled_addons(
        &self,
        approved: bool,
    ) -> Result<Vec<OpenCliAddonStatus>, OpenCliError> {
        if !approved {
            return Err(OpenCliError::InvalidInput("approved"));
        }
        if self.version != crate::VERIFIED_VERSION {
            return Err(OpenCliError::VersionChanged);
        }
        tokio::task::spawn_blocking(|| {
            let home = user_home().ok_or(OpenCliError::Unavailable)?;
            let result = repair(&home).map_err(|_| OpenCliError::Unavailable)?;
            if result
                .iter()
                .any(|item| item.state != OpenCliAddonState::Ready)
            {
                return Err(OpenCliError::ReadCommandUnavailable);
            }
            Ok(result)
        })
        .await
        .map_err(|_| OpenCliError::Unavailable)?
    }
}

pub(super) async fn require_ready(id: &'static str) -> Result<(), OpenCliError> {
    let ready = tokio::task::spawn_blocking(move || {
        user_home().is_some_and(|home| {
            inspect(&home)
                .iter()
                .any(|item| item.id == id && item.state == OpenCliAddonState::Ready)
        })
    })
    .await
    .unwrap_or(false);
    if ready {
        Ok(())
    } else {
        Err(OpenCliError::ReadCommandUnavailable)
    }
}

fn user_home() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os(if cfg!(windows) {
        "USERPROFILE"
    } else {
        "HOME"
    })?);
    home.is_absolute().then_some(home)
}

fn unavailable() -> Vec<OpenCliAddonStatus> {
    ADDONS
        .iter()
        .map(|addon| OpenCliAddonStatus {
            id: addon.id,
            state: OpenCliAddonState::Unavailable,
        })
        .collect()
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0 // Includes directory junctions.
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

// Check every existing component without following known symlinks/reparse points.
// This is conflict detection, not isolation from a hostile local administrator.
fn directory_state(path: &Path) -> Result<(), OpenCliAddonState> {
    for ancestor in path.ancestors().collect::<Vec<_>>().into_iter().rev() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) if metadata.is_dir() && !is_link(&metadata) => (),
            Ok(_) => return Err(OpenCliAddonState::Conflict),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(OpenCliAddonState::Missing);
            }
            Err(_) => return Err(OpenCliAddonState::Unavailable),
        }
    }
    Ok(())
}

fn inspect_one(root: &Path, addon: &Addon) -> OpenCliAddonState {
    let directory = root.join(addon.id);
    if let Err(state) = directory_state(&directory) {
        return state;
    }
    let checked = (|| -> io::Result<bool> {
        // Unexpected files can also be executable add-ons; exact directory only.
        let entries = fs::read_dir(&directory)?
            .take(addon.files.len() + 1)
            .collect::<io::Result<Vec<_>>>()?;
        if entries.len() != addon.files.len() {
            return Ok(false);
        }
        for (name, expected) in addon.files {
            let path = directory.join(name);
            let metadata = fs::symlink_metadata(&path)?;
            if !metadata.is_file() || is_link(&metadata) || metadata.len() != expected.len() as u64
            {
                return Ok(false);
            }
            let mut bytes = Vec::new();
            fs::File::open(path)?
                .take(expected.len() as u64 + 1)
                .read_to_end(&mut bytes)?;
            if bytes != *expected {
                return Ok(false);
            }
        }
        Ok(true)
    })();
    match checked {
        Ok(true) => OpenCliAddonState::Ready,
        Ok(false) => OpenCliAddonState::Conflict,
        Err(error) if error.kind() == io::ErrorKind::NotFound => OpenCliAddonState::Conflict,
        Err(_) => OpenCliAddonState::Unavailable,
    }
}

fn inspect(home: &Path) -> Vec<OpenCliAddonStatus> {
    let root = home.join(".opencli/clis");
    ADDONS
        .iter()
        .map(|addon| OpenCliAddonStatus {
            id: addon.id,
            state: inspect_one(&root, addon),
        })
        .collect()
}

fn create_directory(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    #[cfg(not(unix))]
    let _ = &mut builder;
    builder.create(path)
}

fn install(home: &Path) -> Vec<OpenCliAddonStatus> {
    let before = inspect(home);
    if directory_state(home) == Err(OpenCliAddonState::Missing) {
        return unavailable();
    }
    if before.iter().any(|item| {
        matches!(
            item.state,
            OpenCliAddonState::Conflict | OpenCliAddonState::Unavailable
        )
    }) {
        return before;
    }
    // Create only two fixed descendants of the existing home, never its ancestors.
    for parent in [home.join(".opencli"), home.join(".opencli/clis")] {
        match directory_state(&parent) {
            Ok(()) => (),
            Err(OpenCliAddonState::Missing) => {
                if create_directory(&parent).is_err() && directory_state(&parent).is_err() {
                    return unavailable();
                }
            }
            Err(_) => return unavailable(),
        }
    }
    let root = home.join(".opencli/clis");
    for addon in ADDONS {
        if inspect_one(&root, addon) != OpenCliAddonState::Missing {
            continue;
        }
        let directory = root.join(addon.id);
        // Exclusive directory creation owns this attempt. No fallback to opening
        // another installer's directory, no destructive cleanup after failure.
        if let Err(error) = create_directory(&directory) {
            let mut result = inspect(home);
            if error.kind() != io::ErrorKind::AlreadyExists {
                result
                    .iter_mut()
                    .find(|item| item.id == addon.id)
                    .unwrap()
                    .state = OpenCliAddonState::Unavailable;
            }
            return result;
        }
        let written = (|| -> io::Result<()> {
            for (name, bytes) in addon.files {
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(directory.join(name))?;
                file.write_all(bytes)?;
                file.sync_all()?;
            }
            Ok(())
        })();
        if written.is_err() {
            break;
        }
    }
    inspect(home)
}

fn repair(home: &Path) -> io::Result<Vec<OpenCliAddonStatus>> {
    let before = inspect(home);
    if before
        .iter()
        .all(|item| item.state == OpenCliAddonState::Ready)
    {
        return Ok(before);
    }
    if before.iter().all(|item| {
        matches!(
            item.state,
            OpenCliAddonState::Missing | OpenCliAddonState::Ready
        )
    }) {
        return Ok(install(home));
    }
    let root = home.join(".opencli/clis");
    directory_state(&root).map_err(|_| io::Error::other("unsafe add-on root"))?;
    // Serialize explicit repairs across processes; the worker holds the lease
    // through backup, publication and rollback even if its caller is cancelled.
    let lease = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(home.join(".opencli/onecrew-repair.lock"))?;
    fs2::FileExt::try_lock_exclusive(&lease)?;
    let backup_root = home.join(".opencli/onecrew-backups");
    match directory_state(&backup_root) {
        Ok(()) => (),
        Err(OpenCliAddonState::Missing) => create_directory(&backup_root)?,
        Err(_) => return Err(io::Error::other("unsafe backup root")),
    }
    let backup = tempfile::Builder::new()
        .prefix("addons-")
        .tempdir_in(&backup_root)?
        .keep();
    let mut moved = Vec::new();
    let result = (|| -> io::Result<Vec<OpenCliAddonStatus>> {
        for addon in ADDONS {
            if inspect_one(&root, addon) == OpenCliAddonState::Ready {
                continue;
            }
            let source = root.join(addon.id);
            match fs::symlink_metadata(&source) {
                Ok(metadata) if metadata.is_dir() && !is_link(&metadata) => {
                    fs::rename(&source, backup.join(addon.id))?;
                    moved.push(addon.id);
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                _ => return Err(io::Error::other("unsafe add-on directory")),
            }
        }
        let after = install(home);
        if after
            .iter()
            .any(|item| item.state != OpenCliAddonState::Ready)
        {
            return Err(io::Error::other("add-on repair readback failed"));
        }
        Ok(after)
    })();
    if result.is_err() {
        for id in moved.into_iter().rev() {
            let current = root.join(id);
            if current.exists() {
                let addon = ADDONS.iter().find(|addon| addon.id == id).unwrap();
                // Roll back only candidate files this installer could have
                // written. Preserve the backup if another writer added data.
                for entry in fs::read_dir(&current)? {
                    let entry = entry?;
                    let metadata = fs::symlink_metadata(entry.path())?;
                    let expected = addon
                        .files
                        .iter()
                        .find(|(name, _)| entry.file_name() == *name);
                    if !metadata.is_file()
                        || is_link(&metadata)
                        || expected.is_none_or(|(_, bytes)| {
                            if metadata.len() != bytes.len() as u64 {
                                return true;
                            }
                            let mut actual = Vec::new();
                            fs::File::open(entry.path())
                                .and_then(|file| {
                                    file.take(bytes.len() as u64 + 1).read_to_end(&mut actual)
                                })
                                .is_err()
                                || actual.as_slice() != *bytes
                        })
                    {
                        return Err(io::Error::other(
                            "add-on backup retained after concurrent change",
                        ));
                    }
                }
                fs::remove_dir_all(&current)?;
            }
            fs::rename(backup.join(id), current)?;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_repair_replaces_old_owned_addons_and_preserves_other_files() {
        let home = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        assert!(
            install(home.path())
                .iter()
                .all(|item| item.state == OpenCliAddonState::Ready)
        );
        let root = home.path().join(".opencli/clis");
        let old = root.join(ADDONS[0].id).join(ADDONS[0].files[0].0);
        fs::write(&old, b"old onecrew addon").unwrap();
        fs::write(
            root.join(ADDONS[0].id).join("custom-note.txt"),
            b"keep this note",
        )
        .unwrap();
        fs::create_dir(root.join("user-addon")).unwrap();
        fs::write(root.join("user-addon/custom.js"), b"keep unrelated addon").unwrap();
        fs::write(home.path().join(".opencli/profile.json"), b"keep profiles").unwrap();
        let result = repair(home.path()).unwrap();
        assert!(
            result
                .iter()
                .all(|item| item.state == OpenCliAddonState::Ready)
        );
        assert_eq!(
            fs::read(root.join("user-addon/custom.js")).unwrap(),
            b"keep unrelated addon"
        );
        assert_eq!(
            fs::read(home.path().join(".opencli/profile.json")).unwrap(),
            b"keep profiles"
        );
        let backup = fs::read_dir(home.path().join(".opencli/onecrew-backups"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(
            fs::read(backup.join(ADDONS[0].id).join(ADDONS[0].files[0].0)).unwrap(),
            b"old onecrew addon"
        );
        assert_eq!(
            fs::read(backup.join(ADDONS[0].id).join("custom-note.txt")).unwrap(),
            b"keep this note"
        );
    }

    #[cfg(unix)]
    #[test]
    fn unsafe_legacy_addon_restores_already_moved_folders_without_following_links() {
        let home = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        install(home.path());
        let root = home.path().join(".opencli/clis");
        let first = root.join(ADDONS[0].id).join(ADDONS[0].files[0].0);
        fs::write(&first, b"old addon").unwrap();
        fs::remove_dir_all(root.join(ADDONS[1].id)).unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        fs::write(elsewhere.path().join("keep.txt"), b"keep external data").unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), root.join(ADDONS[1].id)).unwrap();
        assert!(repair(home.path()).is_err());
        assert_eq!(fs::read(first).unwrap(), b"old addon");
        assert_eq!(
            fs::read(elsewhere.path().join("keep.txt")).unwrap(),
            b"keep external data"
        );
    }

    #[tokio::test]
    async fn approval_and_version_are_checked_before_any_filesystem_effect() {
        let cli = OpenCli {
            executable: PathBuf::from("must-not-run"), script: None, version: "changed".into(),
            registry: crate::registry::Registry::parse(r#"[{"command":"test/read","site":"test","name":"read","access":"read","browser":false,"strategy":"PUBLIC"}]"#).unwrap(),
        };
        assert_eq!(
            cli.install_bundled_addons(false).await,
            Err(OpenCliError::InvalidInput("approved"))
        );
        assert_eq!(
            cli.install_bundled_addons(true).await,
            Err(OpenCliError::VersionChanged)
        );
    }

    #[test]
    fn status_is_read_only_and_install_is_exact_and_idempotent() {
        let home = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        assert!(
            inspect(home.path())
                .iter()
                .all(|s| s.state == OpenCliAddonState::Missing)
        );
        assert!(!home.path().join(".opencli").exists());
        let result = install(home.path());
        assert!(result.iter().all(|s| s.state == OpenCliAddonState::Ready));
        assert_eq!(install(home.path()), result);
        for addon in ADDONS {
            for (name, bytes) in addon.files {
                assert_eq!(
                    fs::read(home.path().join(".opencli/clis").join(addon.id).join(name)).unwrap(),
                    *bytes
                );
            }
        }
    }

    #[tokio::test]
    #[ignore = "requires installed OpenCLI and all exact bundled add-ons; verifies no-op installation only"]
    async fn installed_addon_setup_is_an_idempotent_local_operation() {
        let cli = OpenCli::inspect_system().await.unwrap();
        let before = cli.addon_status().await;
        assert!(
            before.iter().all(|s| s.state == OpenCliAddonState::Ready),
            "precondition: exact development add-ons already installed"
        );
        let root = user_home().unwrap().join(".opencli/clis");
        let modified = || {
            ADDONS
                .iter()
                .flat_map(|addon| {
                    addon.files.iter().map(|(name, _)| {
                        fs::metadata(root.join(addon.id).join(name))
                            .unwrap()
                            .modified()
                            .unwrap()
                    })
                })
                .collect::<Vec<_>>()
        };
        let timestamps = modified();
        assert_eq!(cli.install_bundled_addons(true).await.unwrap(), before);
        assert_eq!(modified(), timestamps);
        println!(
            "bundled adapters verified; no-op installation preserved every modification time; no platform read"
        );
    }

    #[tokio::test]
    #[ignore = "explicit local setup effect; requires OPENCLI_TEST_INSTALL_ADDONS=true"]
    async fn explicitly_approved_bundled_addon_installation() {
        assert_eq!(
            std::env::var("OPENCLI_TEST_INSTALL_ADDONS").as_deref(),
            Ok("true")
        );
        let cli = OpenCli::inspect_system().await.unwrap();
        let result = cli.install_bundled_addons(true).await.unwrap();
        assert_eq!(result.len(), ADDONS.len());
        assert!(
            result
                .iter()
                .all(|item| item.state == OpenCliAddonState::Ready)
        );
        println!("bundled add-ons ready={}; no platform read", result.len());
    }

    #[test]
    fn conflicts_and_partial_installs_never_overwrite_or_install_other_addons() {
        for contents in [None, Some(b"user code".as_slice())] {
            let home = tempfile::tempdir().unwrap();
            let directory = home.path().join(".opencli/clis/onecrew-youtube");
            fs::create_dir_all(&directory).unwrap();
            if let Some(bytes) = contents {
                fs::write(directory.join("video-evidence.js"), bytes).unwrap();
            }
            let result = install(home.path());
            assert_eq!(result[0].state, OpenCliAddonState::Conflict);
            assert!(!home.path().join(".opencli/clis/onecrew-tiktok").exists());
            assert!(!home.path().join(".opencli/clis/onecrew-bilibili").exists());
            if let Some(bytes) = contents {
                assert_eq!(
                    fs::read(directory.join("video-evidence.js")).unwrap(),
                    bytes
                );
            }
        }
    }

    #[test]
    fn changed_or_extra_files_are_conflicts_not_ready() {
        let home = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        install(home.path());
        let directory = home.path().join(".opencli/clis/onecrew-tiktok");
        fs::write(directory.join("extra.js"), "unrelated").unwrap();
        assert_eq!(install(home.path())[1].state, OpenCliAddonState::Conflict);
        assert_eq!(
            fs::read_to_string(directory.join("extra.js")).unwrap(),
            "unrelated"
        );
        fs::write(directory.join("search.mjs"), "changed").unwrap();
        assert_eq!(install(home.path())[1].state, OpenCliAddonState::Conflict);
        assert_eq!(
            fs::read_to_string(directory.join("search.mjs")).unwrap(),
            "changed"
        );
    }

    #[test]
    fn non_directory_parent_is_preserved() {
        let home = tempfile::tempdir().unwrap();
        fs::write(home.path().join(".opencli"), "existing file").unwrap();
        assert!(
            install(home.path())
                .iter()
                .all(|s| s.state == OpenCliAddonState::Conflict)
        );
        assert_eq!(
            fs::read_to_string(home.path().join(".opencli")).unwrap(),
            "existing file"
        );
    }

    #[test]
    fn nonexistent_home_is_not_created_or_reported_installable() {
        let parent = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let missing = parent.path().join("missing-home");
        assert!(
            install(&missing)
                .iter()
                .all(|s| s.state == OpenCliAddonState::Unavailable)
        );
        assert!(!missing.exists());
    }

    #[cfg(windows)]
    #[test]
    fn junction_parent_is_rejected_without_writing_outside_home() {
        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let link = home.path().join(".opencli");
        // Test-only directory junction; no administrator rights or policy change.
        let result = std::process::Command::new("cmd.exe")
            .args(["/D", "/C", "mklink", "/J"])
            .arg(&link)
            .arg(outside.path())
            .output()
            .unwrap();
        assert!(result.status.success(), "test junction creation failed");
        let states = install(home.path());
        let untouched = fs::read_dir(outside.path()).unwrap().count() == 0;
        fs::remove_dir(&link).unwrap(); // Remove the junction only, never its target.
        assert!(
            states
                .iter()
                .all(|s| s.state == OpenCliAddonState::Conflict)
        );
        assert!(untouched);
    }

    #[cfg(unix)]
    #[test]
    fn linked_parent_is_rejected_without_writing_outside_home() {
        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), home.path().join(".opencli")).unwrap();
        assert!(
            install(home.path())
                .iter()
                .all(|s| s.state == OpenCliAddonState::Conflict)
        );
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    }
}
