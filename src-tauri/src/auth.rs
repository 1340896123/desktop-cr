//! 账号登录模块(桌面客户端登录 dcr-signal 管理服务)。
//!
//! - 登录:调用 `POST {server}/api/auth/login` 换取 JWT,持久化到本地配置
//!   (AppConfig.account),应用启动时校验令牌后解锁主界面;
//! - 退出:清除本地会话;
//! - 所有命令均基于 reqwest 异步 HTTP(不占用 Tauri 主线程)。

use std::sync::OnceLock;

use crate::hbb_client::{load_app_config, save_app_config_inner, AccountSession};

/// 复用全局 HTTP 客户端。
fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

/// 规范化服务地址:无协议前缀自动补 http://,去尾部斜杠。
fn normalize_server(server: &str) -> String {
    let mut s = server.trim().to_string();
    if !s.starts_with("http://") && !s.starts_with("https://") {
        s = format!("http://{s}");
    }
    while s.ends_with('/') {
        s.pop();
    }
    s
}

/// 登录 dcr-signal 账号服务;成功后持久化会话并返回。
#[tauri::command]
pub async fn login_account(
    server: String,
    username: String,
    password: String,
) -> Result<AccountSession, String> {
    let server = normalize_server(&server);
    let resp = http()
        .post(format!("{server}/api/auth/login"))
        .json(&serde_json::json!({ "username": username, "password": password }))
        .send()
        .await
        .map_err(|e| format!("无法连接服务器 {server}: {e}"))?;
    let status = resp.status();
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("服务器响应解析失败: {e}"))?;
    if !status.is_success() {
        let msg = body
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("登录失败,请检查用户名或密码");
        return Err(msg.to_string());
    }
    let token = body
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    if token.is_empty() {
        return Err("登录响应缺少令牌".into());
    }
    let login_name = body
        .get("username")
        .and_then(|v| v.as_str())
        .unwrap_or(&username)
        .to_string();
    let session = AccountSession {
        server,
        username: login_name,
        token,
    };
    save_session(&session)?;
    crate::operation_log::op_log(
        "auth",
        "login_account",
        &format!("server={} user={}", session.server, session.username),
    );
    Ok(session)
}

/// 校验当前令牌是否有效(应用启动时调用;令牌过期则提示重新登录)。
#[tauri::command]
pub async fn check_account_token(session: AccountSession) -> Result<String, String> {
    let server = normalize_server(&session.server);
    let resp = http()
        .get(format!("{server}/api/auth/me"))
        .bearer_auth(&session.token)
        .send()
        .await
        .map_err(|e| format!("无法连接服务器 {server}: {e}"))?;
    if !resp.status().is_success() {
        return Err("登录已过期或无效,请重新登录".into());
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("服务器响应解析失败: {e}"))?;
    Ok(body
        .get("username")
        .and_then(|v| v.as_str())
        .unwrap_or(&session.username)
        .to_string())
}

/// 退出登录:清除本地会话。
#[tauri::command]
pub fn logout_account() -> Result<(), String> {
    let mut cfg = load_app_config();
    cfg.account = None;
    save_app_config_inner(&cfg)?;
    crate::operation_log::op_log("auth", "logout_account", "");
    Ok(())
}

/// 读取当前登录会话(未登录返回 None)。
#[tauri::command]
pub fn get_account() -> Option<AccountSession> {
    load_app_config().account
}

/// 持久化会话到本地配置。
fn save_session(session: &AccountSession) -> Result<(), String> {
    let mut cfg = load_app_config();
    cfg.account = Some(session.clone());
    save_app_config_inner(&cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_url_normalization() {
        assert_eq!(
            normalize_server("120.78.77.248:21120"),
            "http://120.78.77.248:21120"
        );
        assert_eq!(
            normalize_server("  http://example.com:21120/  "),
            "http://example.com:21120"
        );
        assert_eq!(
            normalize_server("https://svc.example.com"),
            "https://svc.example.com"
        );
        assert_eq!(normalize_server("localhost:21120///"), "http://localhost:21120");
    }
}