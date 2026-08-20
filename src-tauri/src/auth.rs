//! 账号登录模块(桌面客户端登录 dcr-signal 管理服务)。
//!
//! - 登录:调用 `POST {server}/api/auth/login` 换取 JWT,持久化到本地配置
//!   (AppConfig.account),应用启动时校验令牌后解锁主界面;
//! - 注册:调用 `POST {server}/api/auth/register` 自助注册,成功后自动签发令牌(注册即登录);
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
    finish_auth(server, body, username)
}

/// 注册 dcr-signal 账号服务;成功后自动签发令牌并持久化会话(注册即登录)。
#[tauri::command]
pub async fn register_account(
    server: String,
    username: String,
    password: String,
) -> Result<AccountSession, String> {
    let server = normalize_server(&server);
    let resp = http()
        .post(format!("{server}/api/auth/register"))
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
            .unwrap_or("注册失败,请稍后重试");
        return Err(msg.to_string());
    }
    finish_auth(server, body, username)
}

/// 解析登录/注册成功响应:提取令牌与用户名,持久化会话并写操作日志。
fn finish_auth(
    server: String,
    body: serde_json::Value,
    fallback_username: String,
) -> Result<AccountSession, String> {
    let token = body
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    if token.is_empty() {
        return Err("服务器响应缺少令牌".into());
    }
    let login_name = body
        .get("username")
        .and_then(|v| v.as_str())
        .unwrap_or(&fallback_username)
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

/// 服务端策略配置(从 dcr-signal 管理 API 拉取,需登录令牌)。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerPolicy {
    /// 公告(服务端维护消息,客户端展示)。
    pub announcement: String,
    /// 客户端最低版本(低于该版本服务端拒绝注册)。
    pub min_client_version: String,
    /// 维护模式(开启后新设备无法注册)。
    pub maintenance_mode: bool,
}

/// 拉取服务端策略配置(公告/客户端版本下限/维护模式)。
/// 未登录或服务不可用返回默认空值(不阻塞客户端功能)。
#[tauri::command]
pub async fn fetch_server_policy(session: AccountSession) -> ServerPolicy {
    let empty = ServerPolicy {
        announcement: String::new(),
        min_client_version: String::new(),
        maintenance_mode: false,
    };
    let server = normalize_server(&session.server);
    let resp = match http()
        .get(format!("{server}/api/admin/config"))
        .bearer_auth(&session.token)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log::warn!("[auth] 拉取服务端策略失败: {e}");
            return empty;
        }
    };
    if !resp.status().is_success() {
        log::warn!("[auth] 拉取服务端策略失败: HTTP {}", resp.status());
        return empty;
    }
    match resp.json::<serde_json::Value>().await {
        Ok(v) => ServerPolicy {
            announcement: v
                .get("announcement")
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .to_string(),
            min_client_version: v
                .get("minClientVersion")
                .or_else(|| v.get("min_client_version"))
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .to_string(),
            maintenance_mode: v
                .get("maintenanceMode")
                .or_else(|| v.get("maintenance_mode"))
                .and_then(|x| x.as_bool())
                .unwrap_or(false),
        },
        Err(e) => {
            log::warn!("[auth] 服务端策略解析失败: {e}");
            empty
        }
    }
}

/// 持久化会话到本地配置。信令/中继地址由用户配置独立管理,避免登录
/// 管理服务时覆盖自定义端口或独立部署地址。
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
