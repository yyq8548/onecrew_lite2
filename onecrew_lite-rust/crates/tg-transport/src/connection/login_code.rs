//! Phone/email login-code flow: requesting, describing, and submitting a
//! Telegram login code without the phone-code hash ever leaving Rust.

use grammers_client::InvocationError;
use grammers_session::Session;
use grammers_tl_types as tl;
use serde::Serialize;

use super::Connection;
use crate::error::TgError;

/// Telegram replies `PHONE_MIGRATE_x` / `USER_MIGRATE_x` (303) when the account
/// lives on another datacenter. Grammers only auto-handles FILE_MIGRATE, so we
/// detect it here and return the target DC id.
fn migrate_dc(error: &InvocationError) -> Option<i32> {
    match error {
        InvocationError::Rpc(rpc) if rpc.code == 303 && rpc.name.ends_with("_MIGRATE") => {
            rpc.value.map(|dc| dc as i32)
        }
        _ => None,
    }
}

/// Outcome of submitting a login code.
pub enum SignInStep {
    /// Sign-in completed; the connection is now authorized.
    Complete,
    /// The account has 2FA enabled; call [`Connection::check_password`] next.
    PasswordRequired,
}

/// Product-facing description of the route Telegram selected for this login
/// code. This is deliberately independent from grammers' generated TL types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoginCodeMethod {
    TelegramApp,
    Sms,
    VoiceCall,
    FlashCall,
    MissedCall,
    Email,
    EmailSetupRequired,
    Fragment,
    FirebaseSms,
    SmsWord,
    SmsPhrase,
}

/// Non-secret delivery metadata returned by `auth.sendCode`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoginCodeDelivery {
    pub method: LoginCodeMethod,
    pub code_length: Option<i32>,
    pub destination_hint: Option<String>,
    pub timeout_seconds: Option<i32>,
    pub next_method: Option<LoginCodeMethod>,
    pub action_url: Option<String>,
    pub can_submit_code: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoginCodeVerification {
    Phone,
    Email,
}

/// Opaque state required by `auth.signIn`. The phone-code hash never leaves the
/// Rust login state or reaches the desktop webview.
pub struct LoginCodeToken {
    phone: String,
    phone_code_hash: String,
    verification: LoginCodeVerification,
}

pub struct LoginCodeRequest {
    pub token: LoginCodeToken,
    pub delivery: LoginCodeDelivery,
}

impl Connection {
    /// Request a login code and retain Telegram's real delivery metadata. The
    /// high-level grammers helper discards this data, so the strongly-typed raw
    /// request is used while keeping the rest of the grammers connection.
    pub async fn request_login_code(&self, phone: &str) -> Result<LoginCodeRequest, TgError> {
        let request = tl::functions::auth::SendCode {
            phone_number: phone.to_string(),
            api_id: self.api_id,
            api_hash: self.api_hash.clone(),
            settings: tl::types::CodeSettings {
                allow_flashcall: false,
                current_number: false,
                allow_app_hash: false,
                allow_missed_call: false,
                allow_firebase: false,
                logout_tokens: None,
                token: None,
                app_sandbox: None,
                unknown_number: false,
            }
            .into(),
        };

        let response = match self.client.invoke(&request).await {
            Ok(response) => response,
            Err(error) => match migrate_dc(&error) {
                Some(dc) => {
                    let old_dc = self
                        .session
                        .home_dc_id()
                        .map_err(|error| TgError::Session(error.to_string()))?;
                    self.handle.disconnect_from_dc(old_dc);
                    self.session
                        .set_home_dc_id(dc)
                        .await
                        .map_err(|error| TgError::Session(error.to_string()))?;
                    self.client
                        .invoke(&request)
                        .await
                        .map_err(|error| TgError::Network(error.to_string()))?
                }
                None => return Err(TgError::Network(error.to_string())),
            },
        };

        match response {
            tl::enums::auth::SentCode::Code(code) => {
                let (delivery, verification) =
                    login_code_delivery(code.r#type, code.next_type, code.timeout);
                if !delivery.can_submit_code {
                    return Err(TgError::Auth(
                        "Telegram requires email setup in an official client before this login can continue"
                            .into(),
                    ));
                }
                Ok(LoginCodeRequest {
                    token: LoginCodeToken {
                        phone: phone.to_string(),
                        phone_code_hash: code.phone_code_hash,
                        verification,
                    },
                    delivery,
                })
            }
            tl::enums::auth::SentCode::Success(_) => Err(TgError::Auth(
                "Telegram returned an already-authorized login instead of a code".into(),
            )),
            tl::enums::auth::SentCode::PaymentRequired(_) => Err(TgError::Auth(
                "Telegram requires paid authorization in an official client".into(),
            )),
        }
    }

    pub async fn sign_in(&self, token: &LoginCodeToken, code: &str) -> Result<SignInStep, TgError> {
        let (phone_code, email_verification) = match token.verification {
            LoginCodeVerification::Phone => (Some(code.to_string()), None),
            LoginCodeVerification::Email => (
                None,
                Some(
                    tl::types::EmailVerificationCode {
                        code: code.to_string(),
                    }
                    .into(),
                ),
            ),
        };
        match self
            .client
            .invoke(&tl::functions::auth::SignIn {
                phone_number: token.phone.clone(),
                phone_code_hash: token.phone_code_hash.clone(),
                phone_code,
                email_verification,
            })
            .await
        {
            Ok(tl::enums::auth::Authorization::Authorization(auth)) => {
                self.complete_login(auth).await?;
                Ok(SignInStep::Complete)
            }
            Ok(tl::enums::auth::Authorization::SignUpRequired(_)) => Err(TgError::Auth(
                "sign up with an official Telegram client is required".into(),
            )),
            Err(error) if error.is("SESSION_PASSWORD_NEEDED") => Ok(SignInStep::PasswordRequired),
            Err(error) if error.is("PHONE_CODE_*") || error.is("EMAIL_VERIFY_*") => {
                Err(TgError::Auth("invalid or expired login code".into()))
            }
            Err(error) => Err(TgError::Auth(error.to_string())),
        }
    }
}

fn login_code_delivery(
    type_: tl::enums::auth::SentCodeType,
    next_type: Option<tl::enums::auth::CodeType>,
    timeout_seconds: Option<i32>,
) -> (LoginCodeDelivery, LoginCodeVerification) {
    use tl::enums::auth::SentCodeType;

    let (method, code_length, destination_hint, action_url, verification, can_submit_code) =
        match type_ {
            SentCodeType::App(value) => (
                LoginCodeMethod::TelegramApp,
                Some(value.length),
                Some("Telegram service chat 777000 on another signed-in device".into()),
                None,
                LoginCodeVerification::Phone,
                true,
            ),
            SentCodeType::Sms(value) => (
                LoginCodeMethod::Sms,
                Some(value.length),
                None,
                None,
                LoginCodeVerification::Phone,
                true,
            ),
            SentCodeType::Call(value) => (
                LoginCodeMethod::VoiceCall,
                Some(value.length),
                None,
                None,
                LoginCodeVerification::Phone,
                true,
            ),
            SentCodeType::FlashCall(value) => (
                LoginCodeMethod::FlashCall,
                None,
                Some(value.pattern),
                None,
                LoginCodeVerification::Phone,
                true,
            ),
            SentCodeType::MissedCall(value) => (
                LoginCodeMethod::MissedCall,
                Some(value.length),
                Some(value.prefix),
                None,
                LoginCodeVerification::Phone,
                true,
            ),
            SentCodeType::EmailCode(value) => (
                LoginCodeMethod::Email,
                Some(value.length),
                Some(value.email_pattern),
                None,
                LoginCodeVerification::Email,
                true,
            ),
            SentCodeType::SetUpEmailRequired(_) => (
                LoginCodeMethod::EmailSetupRequired,
                None,
                None,
                None,
                LoginCodeVerification::Email,
                false,
            ),
            SentCodeType::FragmentSms(value) => (
                LoginCodeMethod::Fragment,
                Some(value.length),
                None,
                Some(value.url),
                LoginCodeVerification::Phone,
                true,
            ),
            SentCodeType::FirebaseSms(value) => (
                LoginCodeMethod::FirebaseSms,
                Some(value.length),
                None,
                None,
                LoginCodeVerification::Phone,
                true,
            ),
            SentCodeType::SmsWord(value) => (
                LoginCodeMethod::SmsWord,
                None,
                value.beginning,
                None,
                LoginCodeVerification::Phone,
                true,
            ),
            SentCodeType::SmsPhrase(value) => (
                LoginCodeMethod::SmsPhrase,
                None,
                value.beginning,
                None,
                LoginCodeVerification::Phone,
                true,
            ),
        };
    (
        LoginCodeDelivery {
            method,
            code_length,
            destination_hint,
            timeout_seconds,
            next_method: next_type.map(login_code_method),
            action_url,
            can_submit_code,
        },
        verification,
    )
}

fn login_code_method(type_: tl::enums::auth::CodeType) -> LoginCodeMethod {
    match type_ {
        tl::enums::auth::CodeType::Sms => LoginCodeMethod::Sms,
        tl::enums::auth::CodeType::Call => LoginCodeMethod::VoiceCall,
        tl::enums::auth::CodeType::FlashCall => LoginCodeMethod::FlashCall,
        tl::enums::auth::CodeType::MissedCall => LoginCodeMethod::MissedCall,
        tl::enums::auth::CodeType::FragmentSms => LoginCodeMethod::Fragment,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_app_delivery_without_claiming_sms_delivery() {
        let (delivery, verification) = login_code_delivery(
            tl::types::auth::SentCodeTypeApp { length: 5 }.into(),
            Some(tl::types::auth::CodeTypeSms {}.into()),
            Some(60),
        );

        assert_eq!(delivery.method, LoginCodeMethod::TelegramApp);
        assert_eq!(delivery.code_length, Some(5));
        assert_eq!(delivery.next_method, Some(LoginCodeMethod::Sms));
        assert_eq!(delivery.timeout_seconds, Some(60));
        assert_eq!(verification, LoginCodeVerification::Phone);
    }

    #[test]
    fn maps_email_code_to_email_verification() {
        let (delivery, verification) = login_code_delivery(
            tl::types::auth::SentCodeTypeEmailCode {
                apple_signin_allowed: false,
                google_signin_allowed: false,
                email_pattern: "a***@example.com".into(),
                length: 6,
                reset_available_period: None,
                reset_pending_date: None,
            }
            .into(),
            None,
            None,
        );

        assert_eq!(delivery.method, LoginCodeMethod::Email);
        assert_eq!(
            delivery.destination_hint.as_deref(),
            Some("a***@example.com")
        );
        assert_eq!(verification, LoginCodeVerification::Email);
    }
}
