use std::sync::{Mutex, MutexGuard};

use grammers_session::types::{
    ChannelState, DcOption, PeerId, PeerInfo, UpdateState, UpdatesState,
};
use grammers_session::{BoxFuture, Session, SessionData};
use serde::{Deserialize, Serialize};

use crate::error::TgError;

/// Grammers [`Session`] backed by in-memory data that can be snapshotted to and
/// restored from a JSON string, so the encrypted vault stays the source of
/// truth (nothing is written to disk in plaintext).
///
/// Only `home_dc` and `dc_options` (which carry the authorization keys) are
/// persisted; peer cache and update state are rebuilt on reconnect.
#[derive(Default)]
pub struct VaultSession(Mutex<SessionData>);

#[derive(Serialize, Deserialize)]
struct Snapshot {
    home_dc: i32,
    dc_options: Vec<DcOption>,
}

impl VaultSession {
    fn data(&self) -> Result<MutexGuard<'_, SessionData>, TgError> {
        self.0
            .lock()
            .map_err(|_| TgError::Session("session lock poisoned".into()))
    }

    /// Serialize the authorization state to a JSON string for the vault.
    pub fn to_json(&self) -> Result<String, TgError> {
        let data = self.data()?;
        let snapshot = Snapshot {
            home_dc: data.home_dc,
            dc_options: data.dc_options.values().cloned().collect(),
        };
        serde_json::to_string(&snapshot).map_err(|error| TgError::Session(error.to_string()))
    }

    /// Rebuild a session from a JSON string previously produced by [`Self::to_json`].
    pub fn from_json(json: &str) -> Result<Self, TgError> {
        let snapshot: Snapshot =
            serde_json::from_str(json).map_err(|error| TgError::Session(error.to_string()))?;
        let data = SessionData {
            home_dc: snapshot.home_dc,
            dc_options: snapshot
                .dc_options
                .into_iter()
                .map(|option| (option.id, option))
                .collect(),
            ..SessionData::default()
        };
        Ok(Self(Mutex::new(data)))
    }
}

impl Session for VaultSession {
    type Error = TgError;

    fn home_dc_id(&self) -> Result<i32, TgError> {
        Ok(self.data()?.home_dc)
    }

    fn set_home_dc_id(&self, dc_id: i32) -> BoxFuture<'_, Result<(), TgError>> {
        Box::pin(async move {
            self.data()?.home_dc = dc_id;
            Ok(())
        })
    }

    fn dc_option(&self, dc_id: i32) -> Result<Option<DcOption>, TgError> {
        Ok(self.data()?.dc_options.get(&dc_id).cloned())
    }

    fn set_dc_option(&self, dc_option: &DcOption) -> BoxFuture<'_, Result<(), TgError>> {
        let dc_option = dc_option.clone();
        Box::pin(async move {
            self.data()?.dc_options.insert(dc_option.id, dc_option);
            Ok(())
        })
    }

    fn peer(&self, peer: PeerId) -> BoxFuture<'_, Result<Option<PeerInfo>, TgError>> {
        Box::pin(async move { Ok(self.data()?.peer_infos.get(&peer).cloned()) })
    }

    fn cache_peer(&self, peer: &PeerInfo) -> BoxFuture<'_, Result<(), TgError>> {
        let peer = peer.clone();
        Box::pin(async move {
            self.data()?
                .peer_infos
                .entry(peer.id())
                .or_insert_with(|| peer.clone())
                .extend_info(&peer);
            Ok(())
        })
    }

    fn updates_state(&self) -> BoxFuture<'_, Result<UpdatesState, TgError>> {
        Box::pin(async move { Ok(self.data()?.updates_state.clone()) })
    }

    fn set_update_state(&self, update: UpdateState) -> BoxFuture<'_, Result<(), TgError>> {
        Box::pin(async move {
            let mut data = self.data()?;
            match update {
                UpdateState::All(updates_state) => {
                    data.updates_state = updates_state;
                }
                UpdateState::Primary { pts, date, seq } => {
                    data.updates_state.pts = pts;
                    data.updates_state.date = date;
                    data.updates_state.seq = seq;
                }
                UpdateState::Secondary { qts } => {
                    data.updates_state.qts = qts;
                }
                UpdateState::Channel { id, pts } => {
                    data.updates_state.channels.retain(|c| c.id != id);
                    data.updates_state.channels.push(ChannelState { id, pts });
                }
            }
            Ok(())
        })
    }
}
