//! Standalone Telegram login test (no desktop app, no MCP).
//!
//! Runs the full login path — request code (with PHONE_MIGRATE handling),
//! sign in, 2FA password if required, then who-am-i — with grammers debug
//! logs via RUST_LOG. Mirrors what the desktop login wizard does.
//!
//! Run:
//!   cargo run -p onecrew-tg-transport --example tg_login_probe -- <api_id> <api_hash> <+phone>

use std::io::{self, Write};
use std::sync::Arc;

use grammers_client::client::{LoginToken, SignInError};
use grammers_client::{Client, InvocationError, SenderPool};
use grammers_session::Session;
use grammers_session::storages::MemorySession;

#[tokio::main]
async fn main() {
    if env_logger::try_init().is_err() {
        eprintln!("logger already initialised");
    }

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: tg_login_probe <api_id> <api_hash> <+phone with country code>");
        std::process::exit(1);
    }
    let api_id: i32 = args[1].parse().expect("api_id must be a number");
    let api_hash = args[2].clone();
    let phone = args[3].clone();

    let session = Arc::new(MemorySession::default());
    let SenderPool {
        runner,
        handle,
        updates: _,
    } = SenderPool::new(Arc::clone(&session), api_id);
    let client = Client::new(handle.clone());
    let pool = tokio::spawn(runner.run());

    println!("== 请求验证码: {phone} (api_id={api_id}) ==");
    let token = match request_code_migrating(&client, &session, &phone, &api_hash).await {
        Ok(token) => {
            println!("✅ 验证码已发送到你的 Telegram App(官方「Telegram」账号消息,不是短信)。");
            token
        }
        Err(error) => {
            println!("❌ 请求失败: {error}");
            println!(
                "   API_ID_INVALID=凭据错;PHONE_NUMBER_INVALID=号码格式错(带 + 和区号);\
                 FLOOD_WAIT_x=太频繁需等 x 秒;PHONE_NUMBER_BANNED=号码被封。"
            );
            shutdown(handle, pool).await;
            return;
        }
    };

    let code = prompt("输入验证码: ");
    if code.is_empty() {
        shutdown(handle, pool).await;
        return;
    }

    let user = match client.sign_in(&token, &code).await {
        Ok(user) => user,
        Err(SignInError::PasswordRequired(password_token)) => {
            let password = prompt("该账号开启了两步验证,输入 2FA 密码: ");
            match client
                .check_password(password_token, password.as_bytes())
                .await
            {
                Ok(user) => user,
                Err(error) => {
                    println!("❌ 2FA 失败: {error}");
                    shutdown(handle, pool).await;
                    return;
                }
            }
        }
        Err(error) => {
            println!("❌ 登录失败: {error}");
            shutdown(handle, pool).await;
            return;
        }
    };

    println!(
        "✅ 登录成功: id={} username={:?} name={}",
        user.id(),
        user.username(),
        user.full_name()
    );

    shutdown(handle, pool).await;
}

fn prompt(message: &str) -> String {
    print!("\n{message}");
    // The prompt is a courtesy; an unflushed prompt still accepts the answer.
    if let Err(error) = io::stdout().flush() {
        eprintln!("prompt flush failed: {error}");
    }
    let mut line = String::new();
    // A failed read leaves `line` empty and the probe reports the empty answer.
    if let Err(error) = io::stdin().read_line(&mut line) {
        eprintln!("prompt read failed: {error}");
    }
    line.trim().to_string()
}

/// Grammers doesn't auto-handle PHONE_MIGRATE; switch the home DC and retry.
async fn request_code_migrating(
    client: &Client,
    session: &MemorySession,
    phone: &str,
    api_hash: &str,
) -> Result<LoginToken, InvocationError> {
    match client.request_login_code(phone, api_hash).await {
        Err(InvocationError::Rpc(rpc)) if rpc.code == 303 && rpc.name.ends_with("_MIGRATE") => {
            match rpc.value {
                Some(dc) => {
                    println!("(号码在 DC {dc},迁移后重试……)");
                    // The retry below reports the real outcome; a session that
                    // could not record the new home DC only makes it fail again.
                    if let Err(error) = session.set_home_dc_id(dc as i32).await {
                        eprintln!("home DC update failed: {error}");
                    }
                    client.request_login_code(phone, api_hash).await
                }
                None => Err(InvocationError::Rpc(rpc)),
            }
        }
        result => result,
    }
}

async fn shutdown(
    handle: grammers_client::sender::SenderPoolFatHandle,
    pool: tokio::task::JoinHandle<()>,
) {
    handle.quit();
    if let Err(error) = pool.await {
        eprintln!("sender pool task ended abnormally: {error}");
    }
}
