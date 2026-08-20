//! 账号认证与管理核心(dcr-signal Web 管理后台)。
//!
//! - 用户存储:`data-dir/users.json`(argon2 密码哈希,不存明文);
//! - 登录鉴权:JWT(HS256),`data-dir/secret.key` 持久化密钥,重启后令牌仍有效;
//! - 单一管理员角色:所有账号权限相同,首次启动自动创建 admin 账号
//!   (`--admin-pass` 指定,缺省随机生成并打印到日志)。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

/// 令牌有效期(秒)。
const TOKEN_TTL_SECS: u64 = 7 * 24 * 3600;
/// 初始管理员账号名。
pub const DEFAULT_ADMIN: &str = "admin";
/// 密码最小长度。
const MIN_PASSWORD_LEN: usize = 6;

/// 单个账号记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    /// 登录名(唯一,小写规范化存储)。
    pub username: String,
    /// argon2 密码哈希字符串。
    pub password_hash: String,
    /// 创建时间(ISO 8601)。
    pub created_at: String,
    /// 是否被管理员禁用(禁用后无法登录)。
    #[serde(default)]
    pub disabled: bool,
}

/// 用户存储:内存 HashMap + `users.json` 落盘。
#[derive(Debug)]
pub struct UserStore {
    data_dir: PathBuf,
    users: Mutex<HashMap<String, UserRecord>>,
}

impl UserStore {
    /// 加载(或初始化空)用户存储;`data_dir` 不存在时创建。
    pub fn new(data_dir: &Path) -> Self {
        std::fs::create_dir_all(data_dir)
            .unwrap_or_else(|e| log::warn!("[auth] 创建数据目录失败: {e}"));
        let store = Self {
            data_dir: data_dir.to_path_buf(),
            users: Mutex::new(HashMap::new()),
        };
        store.load();
        store
    }

    fn users_file(&self) -> PathBuf {
        self.data_dir.join("users.json")
    }

    fn load(&self) {
        let path = self.users_file();
        match std::fs::read_to_string(&path) {
            Ok(s) => match serde_json::from_str::<Vec<UserRecord>>(&s) {
                Ok(users) => {
                    let mut map = HashMap::new();
                    for u in users {
                        map.insert(u.username.clone(), u);
                    }
                    let count = map.len();
                    *self.users.lock().unwrap_or_else(|e| e.into_inner()) = map;
                    log::info!("[auth] 已加载 {count} 个账号: {}", path.display());
                }
                Err(e) => log::warn!("[auth] 用户文件解析失败,从空列表启动: {e}"),
            },
            Err(_) => log::info!("[auth] 用户文件不存在,首次运行: {}", path.display()),
        }
    }

    fn save(&self) {
        let mut users: Vec<UserRecord> = self
            .users
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect();
        users.sort_by(|a, b| a.username.cmp(&b.username));
        match serde_json::to_string_pretty(&users) {
            Ok(json) => {
                if let Err(e) = std::fs::write(self.users_file(), json) {
                    log::error!("[auth] 写入用户文件失败: {e}");
                }
            }
            Err(e) => log::error!("[auth] 序列化用户列表失败: {e}"),
        }
    }

    /// 首次启动时创建初始管理员。账号列表为空才创建,返回创建用的密码
    /// (调用方打印到日志;`admin_pass` 缺省则随机生成)。
    pub fn ensure_bootstrap(&self, admin_pass: Option<&str>) -> Option<String> {
        let empty = self
            .users
            .lock()
            .map(|m| m.is_empty())
            .unwrap_or(false);
        if !empty {
            return None;
        }
        let password = admin_pass
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| random_password());
        match self.create_user(DEFAULT_ADMIN, &password) {
            Ok(()) => Some(password),
            Err(e) => {
                log::error!("[auth] 创建初始管理员失败: {e}");
                None
            }
        }
    }

    /// 校验用户名是否合法(字母数字下划线连字符,长度 3..=32)。
    fn validate_username(username: &str) -> Result<String, String> {
        let u = username.trim().to_lowercase();
        let len = u.chars().count();
        if !(3..=32).contains(&len) {
            return Err("用户名长度需为 3..=32".into());
        }
        if !u
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err("用户名仅允许字母、数字、下划线、连字符".into());
        }
        Ok(u)
    }

    fn validate_password(password: &str) -> Result<(), String> {
        if password.chars().count() < MIN_PASSWORD_LEN {
            return Err(format!("密码长度至少 {MIN_PASSWORD_LEN} 位"));
        }
        Ok(())
    }

    /// 创建账号(已存在则报错)。密码自动 argon2 哈希。
    pub fn create_user(&self, username: &str, password: &str) -> Result<(), String> {
        let username = Self::validate_username(username)?;
        Self::validate_password(password)?;
        let mut map = self.users.lock().map_err(|e| e.to_string())?;
        if map.contains_key(&username) {
            return Err(format!("账号已存在: {username}"));
        }
        let record = UserRecord {
            password_hash: hash_password(password)?,
            created_at: now_iso(),
            username: username.clone(),
            disabled: false,
        };
        map.insert(username.clone(), record);
        drop(map);
        self.save();
        log::info!("[auth] 已创建账号: {username}");
        Ok(())
    }

    /// 删除账号;不允许删除最后一个账号(避免失去管理入口)。
    pub fn delete_user(&self, username: &str) -> Result<(), String> {
        let username = username.trim().to_lowercase();
        let mut map = self.users.lock().map_err(|e| e.to_string())?;
        if !map.contains_key(&username) {
            return Err(format!("账号不存在: {username}"));
        }
        if map.len() <= 1 {
            return Err("不能删除最后一个账号(至少保留一个管理员)".into());
        }
        map.remove(&username);
        drop(map);
        self.save();
        log::info!("[auth] 已删除账号: {username}");
        Ok(())
    }

    /// 重置账号密码。
    pub fn reset_password(&self, username: &str, password: &str) -> Result<(), String> {
        let username = username.trim().to_lowercase();
        Self::validate_password(password)?;
        let mut map = self.users.lock().map_err(|e| e.to_string())?;
        let record = map
            .get_mut(&username)
            .ok_or_else(|| format!("账号不存在: {username}"))?;
        record.password_hash = hash_password(password)?;
        drop(map);
        self.save();
        log::info!("[auth] 已重置账号密码: {username}");
        Ok(())
    }

    /// 校验登录:用户名 + 密码匹配,且账号未被禁用。
    pub fn verify_login(&self, username: &str, password: &str) -> bool {
        let username = username.trim().to_lowercase();
        let rec = self
            .users
            .lock()
            .map(|m| m.get(&username).cloned())
            .unwrap_or(None);
        match rec {
            Some(r) => !r.disabled && verify_password(password, &r.password_hash),
            None => false,
        }
    }

    /// 启用/禁用账号(管理后台)。禁用后无法登录。
    pub fn set_disabled(&self, username: &str, disabled: bool) -> Result<(), String> {
        let username = username.trim().to_lowercase();
        let mut map = self.users.lock().unwrap_or_else(|e| e.into_inner());
        let rec = map
            .get_mut(&username)
            .ok_or_else(|| format!("账号不存在: {username}"))?;
        rec.disabled = disabled;
        drop(map);
        self.save();
        log::info!("[auth] 账号 {username} 已{}", if disabled { "禁用" } else { "启用" });
        Ok(())
    }

    /// 账号列表(按用户名排序)。
    pub fn list_users(&self) -> Vec<UserRecord> {
        let mut users: Vec<UserRecord> = self
            .users
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect();
        users.sort_by(|a, b| a.username.cmp(&b.username));
        users
    }

    /// 账号数量。
    pub fn count(&self) -> usize {
        self.users
            .lock()
            .map(|m| m.len())
            .unwrap_or_default()
    }
}

/// 账号认证状态(跨请求共享)。
#[derive(Clone)]
pub struct AuthState {
    /// 用户存储。
    pub store: std::sync::Arc<UserStore>,
    /// JWT 签名密钥。
    pub secret: Vec<u8>,
}

impl AuthState {
    /// 创建认证状态;`secret` 由调用方从 `secret.key` 加载或生成。
    pub fn new(store: std::sync::Arc<UserStore>, secret: Vec<u8>) -> Self {
        Self { store, secret }
    }

    /// 校验登录并签发 JWT,成功返回令牌。
    pub fn login(&self, username: &str, password: &str) -> Result<String, String> {
        if !self.store.verify_login(username, password) {
            return Err("用户名或密码错误".into());
        }
        let username = username.trim().to_lowercase();
        let token = issue_token(&self.secret, &username)
            .map_err(|e| format!("令牌签发失败: {e}"))?;
        log::info!("[auth] 登录成功: {username}");
        Ok(token)
    }

    /// 校验令牌,成功返回用户名。
    pub fn validate(&self, token: &str) -> Result<String, String> {
        verify_token(&self.secret, token)
    }
}

/// argon2 哈希密码。
fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| format!("密码哈希失败: {e}"))
}

/// 校验明文密码与哈希是否匹配。
fn verify_password(password: &str, hash: &str) -> bool {
    let parsed = match PasswordHash::new(hash) {
        Ok(p) => p,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// JWT 载荷。
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    /// 用户名。
    sub: String,
    /// 角色(当前统一 admin)。
    role: String,
    /// 过期时间(Unix 秒)。
    exp: u64,
}

/// 签发 JWT(HS256)。
fn issue_token(secret: &[u8], username: &str) -> Result<String, String> {
    let exp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs()
        + TOKEN_TTL_SECS;
    let claims = Claims {
        sub: username.to_string(),
        role: "admin".into(),
        exp,
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(secret),
    )
    .map_err(|e| format!("令牌签发失败: {e}"))
}

/// 校验 JWT,返回用户名。
fn verify_token(secret: &[u8], token: &str) -> Result<String, String> {
    let mut validation = jsonwebtoken::Validation::default();
    // jsonwebtoken 9 默认不校验 exp(且默认 leeway 60s),收紧为严格校验
    validation.validate_exp = true;
    validation.leeway = 0;
    let data = jsonwebtoken::decode::<Claims>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(secret),
        &validation,
    )
    .map_err(|e| format!("令牌校验失败: {e}"))?;
    Ok(data.claims.sub)
}

/// 当前时间 ISO 8601(UTC)。
pub(crate) fn now_iso() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    // 简单 UTC 格式化(YYYY-MM-DDTHH:MM:SSZ,分钟级精度足够)
    let days = secs / 86400;
    let rem = secs % 86400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // 1970-01-01 是星期四;用 days 推年月日
    let (y, mo, d) = civil_from_days(days as i64);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

/// 天数(自 1970-01-01)转公历年月日(Howard Hinnant 算法)。
pub(crate) fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 随机生成初始管理员密码(16 位字母数字)。
fn random_password() -> String {
    use rand::RngCore;
    const CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnpqrstuvwxyz23456789";
    let mut rng = OsRng;
    (0..16)
        .map(|_| CHARS[(rng.next_u32() as usize) % CHARS.len()] as char)
        .collect()
}

/// 从 `data-dir/secret.key` 加载或生成 JWT 密钥(32 字节随机)。
pub fn load_or_create_secret(data_dir: &Path) -> Vec<u8> {
    std::fs::create_dir_all(data_dir).unwrap_or_else(|e| log::warn!("[auth] 创建数据目录失败: {e}"));
    let path = data_dir.join("secret.key");
    if let Ok(bytes) = std::fs::read(&path) {
        if bytes.len() >= 16 {
            return bytes;
        }
    }
    let mut secret = vec![0u8; 32];
    use rand::RngCore;
    OsRng.fill_bytes(&mut secret);
    if let Err(e) = std::fs::write(&path, &secret) {
        log::error!("[auth] 写入密钥文件失败: {e}");
    }
    log::info!("[auth] 已生成 JWT 密钥: {}", path.display());
    secret
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dcr-auth-test-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn hash_and_verify_roundtrip() {
        let hash = hash_password("secret123").unwrap();
        assert!(verify_password("secret123", &hash));
        assert!(!verify_password("wrong", &hash));
        assert!(!verify_password("", &hash));
    }

    #[test]
    fn hash_is_salted_and_not_plaintext() {
        let a = hash_password("same-pass").unwrap();
        let b = hash_password("same-pass").unwrap();
        assert_ne!(a, b, "相同密码两次哈希应不同(随机盐)");
        assert!(!a.contains("same-pass"));
    }

    #[test]
    fn token_roundtrip_and_expiry() {
        let secret = b"0123456789abcdef0123456789abcdef";
        let token = issue_token(secret, "alice").unwrap();
        assert_eq!(verify_token(secret, &token).unwrap(), "alice");
        // 篡改令牌应校验失败
        let mut bad = token.clone();
        bad.pop();
        bad.push('x');
        assert!(verify_token(secret, &bad).is_err());
    }

    #[test]
    fn token_expired_rejected() {
        let secret = b"0123456789abcdef0123456789abcdef";
        let expired = Claims {
            sub: "alice".into(),
            role: "admin".into(),
            exp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - 10,
        };
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &expired,
            &jsonwebtoken::EncodingKey::from_secret(secret),
        )
        .unwrap();
        assert!(verify_token(secret, &token).is_err(), "过期令牌应被拒绝");
    }

    #[test]
    fn user_store_crud_and_persistence() {
        let dir = tmp_dir("crud");
        let store = UserStore::new(&dir);
        assert_eq!(store.count(), 0);
        assert!(store.create_user("Alice", "pass123").is_ok());
        // 大小写规范化
        assert!(store.verify_login("alice", "pass123"));
        assert!(store.verify_login("ALICE", "pass123"));
        assert!(!store.verify_login("alice", "bad"));
        // 重复创建失败
        assert!(store.create_user("alice", "other123").is_err());
        // 非法用户名
        assert!(store.create_user("bad name!", "pass123").is_err());
        // 密码过短
        assert!(store.create_user("bob", "123").is_err());

        // 持久化:新建存储重新加载
        let store2 = UserStore::new(&dir);
        assert_eq!(store2.count(), 1);
        assert!(store2.verify_login("alice", "pass123"));

        // 删除最后一个账号应被拒绝
        assert!(store2.delete_user("alice").is_err());
        store2.create_user("bob", "pass456").unwrap();
        assert!(store2.delete_user("alice").is_ok());
        assert_eq!(store2.count(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn bootstrap_creates_admin_once() {
        let dir = tmp_dir("bootstrap");
        let store = UserStore::new(&dir);
        let pw = store.ensure_bootstrap(None).expect("首次应创建 admin");
        assert_eq!(pw.len(), 16);
        assert!(store.verify_login("admin", &pw));
        // 二次调用不再创建
        assert!(store.ensure_bootstrap(Some("ignored")).is_none());
        // 指定密码生效
        let dir2 = tmp_dir("bootstrap2");
        let store2 = UserStore::new(&dir2);
        store2.ensure_bootstrap(Some("custom123")).unwrap();
        assert!(store2.verify_login("admin", "custom123"));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&dir2).ok();
    }

    #[test]
    fn secret_persists() {
        let dir = tmp_dir("secret");
        let s1 = load_or_create_secret(&dir);
        let s2 = load_or_create_secret(&dir);
        assert_eq!(s1, s2, "密钥应持久化,重启后一致");
        assert!(s1.len() >= 16);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn civil_date_roundtrip() {
        // 1970-01-01
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        // 2026-01-01 = 20454 天(1970..2025 共 56 年,其中 14 个闰年)
        assert_eq!(civil_from_days(20454), (2026, 1, 1));
        // 2000-02-29(闰年)= 11016 天
        assert_eq!(civil_from_days(11016), (2000, 2, 29));
    }
}