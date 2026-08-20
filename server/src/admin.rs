//! Web 管理后台(dcr-signal 内置 HTTP 服务)。
//!
//! - `POST /api/auth/login` — 账号登录,返回 JWT;
//! - `GET  /api/auth/me` — 校验令牌,返回当前用户名;
//! - `GET/POST/DELETE /api/admin/users[...]` — 用户增删改(全部管理员权限);
//! - `GET  /api/admin/peers` — 信令在线的设备列表;
//! - `GET  /api/admin/stats` — 服务统计;
//! - 其余路径 — 托管 React 管理后台静态文件(`--admin-ui` 目录),
//!   目录缺失时回退到内置的轻量管理页。
//!
//! 管理 API 与桌面客户端共用同一套账号:登录即签发令牌,后续请求带
//! `Authorization: Bearer <token>` 即可。

use std::net::SocketAddr;
use std::path::PathBuf;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tower_http::services::{ServeDir, ServeFile};

use crate::auth::AuthState;
use crate::message::PeerEntry;
use crate::signal::SignalCore;

/// 管理后台共享状态。
#[derive(Clone)]
pub struct AdminState {
    /// 账号认证与用户存储。
    pub auth: AuthState,
    /// 信令核心(读取在线设备列表)。
    pub core: SignalCore,
}

/// API 错误响应(JSON `{"error": "..."}`)。
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn unauthorized(msg: String) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: msg,
        }
    }
fn bad_request(msg: String) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: msg,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

/// 从请求头提取并校验 Bearer 令牌,成功返回用户名。
fn bearer_user(state: &AdminState, headers: &HeaderMap) -> Result<String, ApiError> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| ApiError::unauthorized("缺少认证令牌(Authorization: Bearer <token>)".into()))?;
    state
        .auth
        .validate(token)
        .map_err(|e| ApiError::unauthorized(e))
}

// ---------------------------------------------------------------------------
// 认证 API
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct LoginReq {
    username: String,
    password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResp {
    token: String,
    username: String,
}

async fn login(
    State(state): State<AdminState>,
    Json(req): Json<LoginReq>,
) -> Result<Json<LoginResp>, ApiError> {
    let token = state
        .auth
        .login(&req.username, &req.password)
        .map_err(|e| ApiError::unauthorized(e))?;
    Ok(Json(LoginResp {
        token,
        username: req.username.trim().to_lowercase(),
    }))
}

async fn me(
    State(state): State<AdminState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let username = bearer_user(&state, &headers)?;
    Ok(Json(json!({ "username": username })))
}

// ---------------------------------------------------------------------------
// 管理 API(需登录令牌)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateUserReq {
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordReq {
    password: String,
}

async fn list_users(
    State(state): State<AdminState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::auth::UserRecord>>, ApiError> {
    bearer_user(&state, &headers)?;
    Ok(Json(state.auth.store.list_users()))
}

async fn create_user(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Json(req): Json<CreateUserReq>,
) -> Result<impl IntoResponse, ApiError> {
    bearer_user(&state, &headers)?;
    state
        .auth
        .store
        .create_user(&req.username, &req.password)
        .map_err(|e| ApiError::bad_request(e))?;
    Ok((StatusCode::CREATED, Json(json!({ "ok": true }))))
}

async fn delete_user(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Path(username): Path<String>,
) -> Result<Json<Value>, ApiError> {
    bearer_user(&state, &headers)?;
    state
        .auth
        .store
        .delete_user(&username)
        .map_err(|e| ApiError::bad_request(e))?;
    Ok(Json(json!({ "ok": true })))
}

async fn reset_password(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Path(username): Path<String>,
    Json(req): Json<ResetPasswordReq>,
) -> Result<Json<Value>, ApiError> {
    bearer_user(&state, &headers)?;
    state
        .auth
        .store
        .reset_password(&username, &req.password)
        .map_err(|e| ApiError::bad_request(e))?;
    Ok(Json(json!({ "ok": true })))
}

async fn peers(
    State(state): State<AdminState>,
    headers: HeaderMap,
) -> Result<Json<Vec<PeerEntry>>, ApiError> {
    bearer_user(&state, &headers)?;
    Ok(Json(state.core.list_online()))
}

async fn stats(
    State(state): State<AdminState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    bearer_user(&state, &headers)?;
    let peers_online = state.core.list_online().len();
    Ok(Json(json!({
        "users": state.auth.store.count(),
        "peersOnline": peers_online,
    })))
}

// ---------------------------------------------------------------------------
// 静态界面与路由
// ---------------------------------------------------------------------------

/// 构建管理路由:API + 静态界面。
pub fn router(state: AdminState, ui_dir: Option<PathBuf>) -> Router {
    let api = Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/me", get(me))
        .route("/api/admin/users", get(list_users).post(create_user))
.route("/api/admin/users/:username", axum::routing::delete(delete_user))
        .route("/api/admin/users/:username/password", post(reset_password))
        .route("/api/admin/peers", get(peers))
        .route("/api/admin/stats", get(stats))
        .with_state(state);

    match ui_dir.filter(|d| d.is_dir()) {
        // 托管 React 管理后台(构建产物 admin-ui/dist)
        Some(dir) => {
            let index = dir.join("index.html");
            let ui = ServeDir::new(&dir)
                .append_index_html_on_directories(true)
                .fallback(ServeFile::new(index));
            api.fallback_service(ui)
        }
        // 目录缺失时回退到内置轻量管理页
        None => api.fallback(get(embedded_ui)),
    }
}

/// 内置回退管理页:说明如何启用完整后台,并附带可直接使用的极简管理(登录+用户+在线设备)。
async fn embedded_ui() -> Response {
    Response::builder()
        .header(
            axum::http::header::CONTENT_TYPE,
            "text/html; charset=utf-8",
        )
.body(axum::body::Body::from(EMBEDDED_INDEX))
        .unwrap_or_else(|e| {
            log::error!("[admin] 内置页面构造失败: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        })
}

const EMBEDDED_INDEX: &str = r#"<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>dcr-signal 管理后台</title>
<style>
  :root { --bg:#F7F9FB; --card:#fff; --line:#E5E7EB; --text:#111827; --muted:#8A94A6;
          --primary:#2F7EF7; --primary-h:#1E5FD1; --danger:#DC2626; }
  * { box-sizing: border-box; }
  body { margin:0; font-family: "Segoe UI","Microsoft YaHei UI","Microsoft YaHei",system-ui,sans-serif;
         background:var(--bg); color:var(--text); }
  .wrap { max-width:760px; margin:0 auto; padding:40px 20px; }
  h1 { font-size:22px; margin:0 0 4px; }
  .sub { color:var(--muted); font-size:13px; margin-bottom:24px; }
  .card { background:var(--card); border:1px solid var(--line); border-radius:12px;
          padding:20px; margin-bottom:20px; box-shadow:0 1px 2px rgba(16,24,40,.04); }
  .card h2 { font-size:15px; margin:0 0 12px; }
  input { width:100%; padding:9px 12px; margin-bottom:10px; border:1px solid var(--line);
          border-radius:6px; font-size:14px; }
  button { padding:9px 16px; border:none; border-radius:6px; background:var(--primary);
           color:#fff; font-size:14px; cursor:pointer; }
  button:hover { background:var(--primary-h); }
  button.ghost { background:transparent; color:var(--primary); border:1px solid var(--line); }
  button.danger { background:var(--danger); }
  table { width:100%; border-collapse:collapse; font-size:13px; }
  th,td { text-align:left; padding:8px 10px; border-bottom:1px solid var(--line); }
  th { color:var(--muted); font-weight:500; }
  .err { color:var(--danger); font-size:13px; margin:8px 0; }
  .ok { color:#34C759; font-size:13px; margin:8px 0; }
  .hide { display:none; }
  .row { display:flex; gap:8px; align-items:center; }
  .row input { margin-bottom:0; flex:1; }
</style>
</head>
<body>
<div class="wrap">
  <h1>dcr-signal 管理后台</h1>
  <div class="sub">内置轻量管理页(服务端内嵌)。完整界面请构建 React 管理后台:在 server/admin-ui 下执行 <code>npm run build</code>,并以 --admin-ui 指向其 dist 目录。</div>

  <div class="card" id="loginCard">
    <h2>登录</h2>
    <div class="err" id="loginErr"></div>
    <input id="username" placeholder="用户名" autocomplete="username" />
    <input id="password" type="password" placeholder="密码" autocomplete="current-password" />
    <button onclick="doLogin()">登录</button>
  </div>

  <div class="card hide" id="panel">
    <div class="row" style="justify-content:space-between;margin-bottom:12px;">
      <h2 style="margin:0;">管理面板</h2>
      <button class="ghost" onclick="logout()">退出登录</button>
    </div>
    <div class="sub" id="who"></div>
    <h3 style="font-size:14px;">用户管理</h3>
    <table>
      <thead><tr><th>用户名</th><th>创建时间</th><th></th></tr></thead>
      <tbody id="userRows"></tbody>
    </table>
    <div class="row" style="margin-top:10px;">
      <input id="newUser" placeholder="新用户名" />
      <input id="newPass" placeholder="初始密码" type="password" />
      <button onclick="addUser()">添加</button>
    </div>
    <h3 style="font-size:14px;margin-top:20px;">在线设备</h3>
    <table>
      <thead><tr><th>ID</th><th>局域网地址</th><th>外部地址</th></tr></thead>
      <tbody id="peerRows"></tbody>
    </table>
  </div>
</div>
<script>
let token = localStorage.getItem('dcr_admin_token') || '';
async function api(path, opts={}) {
  const headers = {'Content-Type':'application/json'};
  if (token) headers['Authorization'] = 'Bearer ' + token;
  const res = await fetch(path, {...opts, headers});
  const body = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(body.error || ('HTTP ' + res.status));
  return body;
}
function esc(s){ const d=document.createElement('div'); d.textContent=s; return d.innerHTML; }
async function doLogin(){
  const err=document.getElementById('loginErr'); err.textContent='';
  try {
    const r = await api('/api/auth/login', {method:'POST', body: JSON.stringify({
      username: document.getElementById('username').value,
      password: document.getElementById('password').value })});
    token = r.token; localStorage.setItem('dcr_admin_token', token);
    document.getElementById('loginCard').classList.add('hide');
    document.getElementById('panel').classList.remove('hide');
    document.getElementById('who').textContent = '当前账号: ' + r.username;
    await refresh();
  } catch(e){ err.textContent = e.message; }
}
async function refresh(){
  const [users, peers] = await Promise.all([api('/api/admin/users'), api('/api/admin/peers')]);
  const rows = users.map(u => '<tr><td>'+esc(u.username)+'</td><td>'+esc(u.created_at)+'</td>' +
    '<td><button class="danger" onclick="delUser('+JSON.stringify(u.username)+')">删除</button></td></tr>').join('');
  document.getElementById('userRows').innerHTML = rows || '<tr><td colspan="3">暂无用户</td></tr>';
  const prows = peers.map(p => '<tr><td>'+esc(p.id)+'</td><td>'+esc(p.lan)+'</td><td>'+esc(p.external)+'</td></tr>').join('');
  document.getElementById('peerRows').innerHTML = prows || '<tr><td colspan="3">暂无在线设备</td></tr>';
}
async function addUser(){
  try {
    await api('/api/admin/users', {method:'POST', body: JSON.stringify({
      username: document.getElementById('newUser').value,
      password: document.getElementById('newPass').value })});
    document.getElementById('newUser').value=''; document.getElementById('newPass').value='';
    await refresh();
  } catch(e){ alert(e.message); }
}
async function delUser(u){
  if (!confirm('确定删除账号 ' + u + ' ?')) return;
  try { await api('/api/admin/users/' + encodeURIComponent(u), {method:'DELETE'}); await refresh(); }
  catch(e){ alert(e.message); }
}
function logout(){ token=''; localStorage.removeItem('dcr_admin_token'); location.reload(); }
(async function init(){
  if (!token) return;
  try {
    const me = await api('/api/auth/me');
    document.getElementById('loginCard').classList.add('hide');
    document.getElementById('panel').classList.remove('hide');
    document.getElementById('who').textContent = '当前账号: ' + me.username;
    await refresh();
  } catch(_){ localStorage.removeItem('dcr_admin_token'); token=''; }
})();
</script>
</body>
</html>"#;

/// 启动 Web 管理后台服务。`ui_dir` 为 React 管理后台构建产物目录(缺省空)。
/// 返回后持续运行,失败返回错误。
pub async fn serve(state: AdminState, ui_dir: Option<PathBuf>, port: u16) -> Result<(), String> {
    let app = router(state, ui_dir);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("监听管理端口 {port} 失败(被占用?): {e}"))?;
    let local = listener.local_addr().map_err(|e| e.to_string())?;
    log::info!("[admin] Web 管理后台: http://{local}");
    axum::serve(listener, app)
        .await
        .map_err(|e| format!("管理服务退出: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tower::ServiceExt;

    use crate::auth::{AuthState, UserStore};

    fn test_state() -> (AdminState, String) {
        let dir = std::env::temp_dir().join(format!(
            "dcr-admin-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Arc::new(UserStore::new(&dir));
        let pw = store.ensure_bootstrap(Some("bootpass")).unwrap();
        let secret = b"0123456789abcdef0123456789abcdef".to_vec();
        let auth = AuthState::new(store, secret);
        let core = SignalCore::new("relay.example.com:21117");
        (
            AdminState { auth, core },
            pw,
        )
    }

    async fn body_text(resp: Response) -> String {
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .expect("读取响应体失败");
        String::from_utf8_lossy(&bytes).to_string()
    }

    #[tokio::test]
    async fn login_and_admin_flow() {
        let (state, pw) = test_state();
        let app = router(state.clone(), None);

        // 未带令牌访问管理 API → 401
        let resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/admin/users")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        // 错误密码 → 401
        let resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/auth/login")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        json!({"username":"admin","password":"wrong"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        // 正确登录 → 返回令牌
        let resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/auth/login")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        json!({"username":"admin","password":pw}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let login: Value = serde_json::from_str(&body_text(resp).await).unwrap();
        let token = login["token"].as_str().unwrap().to_string();
        assert_eq!(login["username"], "admin");

        // 带令牌:用户列表(含 bootstrap 的 admin)
        let resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/admin/users")
                    .header("authorization", format!("Bearer {token}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let users: Value = serde_json::from_str(&body_text(resp).await).unwrap();
        assert_eq!(users.as_array().unwrap().len(), 1);
        assert_eq!(users[0]["username"], "admin");

        // 创建用户 + 列表 + 删除
        let resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/admin/users")
                    .header("content-type", "application/json")
                    .header("authorization", format!("Bearer {token}"))
                    .body(axum::body::Body::from(
                        json!({"username":"bob","password":"bob123456"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

let resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("DELETE")
                    .uri("/api/admin/users/bob")
                    .header("authorization", format!("Bearer {token}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // 删除最后一个账号被拒
        let resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("DELETE")
                    .uri("/api/admin/users/admin")
                    .header("authorization", format!("Bearer {token}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // stats
        let resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/admin/stats")
                    .header("authorization", format!("Bearer {token}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let stats: Value = serde_json::from_str(&body_text(resp).await).unwrap();
        assert_eq!(stats["users"], 1);
        assert_eq!(stats["peersOnline"], 0);

        // 界面回退页(无 ui_dir)可访问
        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(body_text(resp).await.contains("dcr-signal 管理后台"));
    }
}
