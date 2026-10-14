//! QR login protocol and its display-only rendering: the raw Telegram login
//! token never crosses the transport boundary.

use std::time::Duration;

use base64::{Engine as _, engine::general_purpose};
use grammers_session::Session;
use grammers_session::updates::UpdatesLike;
use grammers_tl_types as tl;
use qrcode::{QrCode, render::unicode, types::Color};
use serde::Serialize;
use tokio::time::timeout;

use super::Connection;
use crate::error::TgError;

/// Display-only QR login challenge. No raw Telegram login URI or token field
/// crosses the transport boundary; callers receive PNG/terminal renderings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QrLoginChallenge {
    pub image_data_url: String,
    #[serde(skip_serializing)]
    pub terminal_qr: String,
    pub expires_at: i32,
}

/// Result of exporting or refreshing a Telegram QR login token.
pub enum QrLoginExport {
    Pending(QrLoginChallenge),
    /// The signed-in mobile client approved the QR, but the account still
    /// requires its two-step-verification password on this new session.
    PasswordRequired,
    Complete,
}

impl Connection {
    /// Export a short-lived QR login token using Telegram's official MTProto
    /// flow. `auth.loginTokenMigrateTo` is resolved inside the transport, and a
    /// successful authorization is committed to the same in-memory session used
    /// by phone login.
    pub async fn export_qr_login(&self) -> Result<QrLoginExport, TgError> {
        let mut response = match self
            .client
            .invoke(&tl::functions::auth::ExportLoginToken {
                api_id: self.api_id,
                api_hash: self.api_hash.clone(),
                except_ids: Vec::new(),
            })
            .await
        {
            Ok(response) => response,
            Err(error) if error.is("SESSION_PASSWORD_NEEDED") => {
                return Ok(QrLoginExport::PasswordRequired);
            }
            Err(error) => return Err(TgError::Network(error.to_string())),
        };
        let mut migrations = 0_u8;

        loop {
            match response {
                tl::enums::auth::LoginToken::Token(token) => {
                    return Ok(QrLoginExport::Pending(render_qr_challenge(
                        token.token,
                        token.expires,
                    )?));
                }
                tl::enums::auth::LoginToken::MigrateTo(migrate) => {
                    migrations += 1;
                    if migrations > 3 {
                        return Err(TgError::Auth(
                            "Telegram QR login exceeded the datacenter migration limit".into(),
                        ));
                    }
                    let old_dc = self
                        .session
                        .home_dc_id()
                        .map_err(|error| TgError::Session(error.to_string()))?;
                    // Telegram Desktop switches the session's main DC before
                    // importing the QR token. This also ensures a following
                    // account.getPassword/auth.checkPassword pair uses the DC
                    // that accepted the login token.
                    self.session
                        .set_home_dc_id(migrate.dc_id)
                        .await
                        .map_err(|error| TgError::Session(error.to_string()))?;
                    let imported = self
                        .client
                        .invoke_in_dc(
                            migrate.dc_id,
                            &tl::functions::auth::ImportLoginToken {
                                token: migrate.token,
                            },
                        )
                        .await;
                    self.handle.disconnect_from_dc(old_dc);
                    response = match imported {
                        Ok(response) => response,
                        Err(error) if error.is("SESSION_PASSWORD_NEEDED") => {
                            return Ok(QrLoginExport::PasswordRequired);
                        }
                        Err(error) => return Err(TgError::Network(error.to_string())),
                    };
                }
                tl::enums::auth::LoginToken::Success(success) => match success.authorization {
                    tl::enums::auth::Authorization::Authorization(auth) => {
                        self.complete_login(auth).await?;
                        return Ok(QrLoginExport::Complete);
                    }
                    tl::enums::auth::Authorization::SignUpRequired(_) => {
                        return Err(TgError::Auth(
                            "sign up with an official Telegram client is required".into(),
                        ));
                    }
                },
            }
        }
    }

    /// Wait briefly for Telegram's `updateLoginToken` signal. This is not a
    /// network polling loop: the SenderPool pushes the passive update into this
    /// receiver after the signed-in mobile client approves the QR code.
    pub async fn wait_for_qr_login_update(&self, max_wait: Duration) -> Result<bool, TgError> {
        let mut updates = self.updates.lock().await;
        match timeout(max_wait, async {
            loop {
                match updates.recv().await {
                    Some(update) if contains_login_token_update(&update) => return Ok(true),
                    Some(_) => continue,
                    None => {
                        return Err(TgError::Network(
                            "Telegram update stream closed during QR login".into(),
                        ));
                    }
                }
            }
        })
        .await
        {
            Ok(result) => result,
            Err(_) => Ok(false),
        }
    }
}

fn render_qr_challenge(token: Vec<u8>, expires_at: i32) -> Result<QrLoginChallenge, TgError> {
    let encoded = general_purpose::URL_SAFE_NO_PAD.encode(token);
    let login_url = format!("tg://login?token={encoded}");
    let qr = QrCode::new(login_url.as_bytes())
        .map_err(|error| TgError::Session(format!("could not render Telegram QR code: {error}")))?;
    let png = render_qr_png(&qr)?;
    let terminal_qr = qr.render::<unicode::Dense1x2>().quiet_zone(true).build();
    Ok(QrLoginChallenge {
        image_data_url: format!(
            "data:image/png;base64,{}",
            general_purpose::STANDARD.encode(png)
        ),
        terminal_qr,
        expires_at,
    })
}

fn render_qr_png(qr: &QrCode) -> Result<Vec<u8>, TgError> {
    const QUIET_ZONE_MODULES: usize = 4;
    const MIN_IMAGE_DIMENSION: usize = 288;

    let qr_modules = qr.width();
    let total_modules = qr_modules + QUIET_ZONE_MODULES * 2;
    let module_scale = MIN_IMAGE_DIMENSION.div_ceil(total_modules).max(1);
    let dimension = total_modules * module_scale;
    let mut pixels = vec![u8::MAX; dimension * dimension];

    for module_y in 0..qr_modules {
        for module_x in 0..qr_modules {
            if qr[(module_x, module_y)] != Color::Dark {
                continue;
            }
            let pixel_x = (module_x + QUIET_ZONE_MODULES) * module_scale;
            let pixel_y = (module_y + QUIET_ZONE_MODULES) * module_scale;
            for row in pixel_y..pixel_y + module_scale {
                pixels[row * dimension + pixel_x..row * dimension + pixel_x + module_scale].fill(0);
            }
        }
    }

    let dimension = u32::try_from(dimension)
        .map_err(|_| TgError::Session("Telegram QR image is too large".into()))?;
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, dimension, dimension);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|error| {
            TgError::Session(format!("could not encode Telegram QR PNG: {error}"))
        })?;
        writer.write_image_data(&pixels).map_err(|error| {
            TgError::Session(format!("could not encode Telegram QR PNG: {error}"))
        })?;
    }
    Ok(bytes)
}

fn contains_login_token_update(update: &UpdatesLike) -> bool {
    let UpdatesLike::Updates(updates) = update else {
        return false;
    };
    match updates {
        tl::enums::Updates::UpdateShort(value) => {
            matches!(&value.update, tl::enums::Update::LoginToken)
        }
        tl::enums::Updates::Combined(value) => value
            .updates
            .iter()
            .any(|update| matches!(update, tl::enums::Update::LoginToken)),
        tl::enums::Updates::Updates(value) => value
            .updates
            .iter()
            .any(|update| matches!(update, tl::enums::Update::LoginToken)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_qr_login_for_gui_and_terminal_clients() {
        let challenge = render_qr_challenge(vec![0, 1, 2, 253, 254, 255], 1_800_000_000)
            .expect("QR challenge should render");

        let encoded = challenge
            .image_data_url
            .strip_prefix("data:image/png;base64,")
            .expect("GUI challenge must be a PNG data URL");
        let png = general_purpose::STANDARD
            .decode(encoded)
            .expect("PNG data URL must be valid base64");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));

        let lines = challenge.terminal_qr.lines().collect::<Vec<_>>();
        assert!(
            lines.len() > 10,
            "terminal QR should have scan-sized height"
        );
        let width = lines[0].chars().count();
        assert!(width > 20, "terminal QR should have scan-sized width");
        assert!(lines.iter().all(|line| line.chars().count() == width));
        assert!(lines[0].chars().all(|character| character == ' '));
        assert!(
            lines[lines.len() - 1]
                .chars()
                .all(|character| character == ' ')
        );
        assert!(challenge.terminal_qr.contains('█'));
        assert_eq!(challenge.expires_at, 1_800_000_000);
    }
}
