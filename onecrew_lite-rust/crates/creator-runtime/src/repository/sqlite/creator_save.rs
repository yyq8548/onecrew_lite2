//! Canonical Creator preparation and persistence, with no snapshot or provider reload.

mod execute;
mod persistence;
mod prepare;
mod provenance;

pub(super) use execute::execute_prepared;
pub(super) use prepare::prepare_entries;
pub(super) use provenance::{contact_source_ref, source_metadata};

use sha2::{Digest, Sha256};

fn fingerprint(value: &impl serde::Serialize) -> Result<String, String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).map_err(|error| error.to_string())?)
    ))
}

#[cfg(test)]
mod tests;
