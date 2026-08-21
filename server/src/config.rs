//! 服务策略配置(dcr-signal Web 管理后台可读写)。
//!
//! - 持久化于 `data-dir/config.json`;首次启动用命令行初始值创建,之后以文件为准;
//! - 管理后台 `GET/PUT /api/admin/config` 读取/更新,更新后即时写入文件并生效;
//! - 供信令注册/密码校验/会话上限等策略判断共享读取。

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use crate::operation_log::op_log;

/// 服务策略配置(全部字段可被管理后台修改)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// 自助注册开关。
    pub open_register: bool,
    /// 密码最小长度(下限 6)。
    pub min_password_len: usize,
    /// 单用户设备数上限(0=不限)。
    pub max_devices_per_user: usize,
    /// 全局中继并发会话上限(0=不限)。
    pub max_concurrent_sessions: usize,
    /// 会话空闲超时秒数(后台清理用)。
    pub session_idle_timeout_secs: u64,
    /// 维护模式(开启后拒绝新设备注册)。
    pub maintenance_mode: bool,
    /// 公告(客户端拉取展示)。
    pub announcement: String,
    /// 客户端最低版本(低于此版本拒绝注册)。
    pub min_client_version: String,
    /// 中继地址("host:port",可空)。
    pub relay_hint: String,
    /// 中继管理地址("host:port",可空,供后台展示)。
    pub relay_admin: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            open_register: true,
            min_password_len: 6,
            max_devices_per_user: 0,
            max_concurrent_sessions: 0,
            session_idle_timeout_secs: 300,
            maintenance_mode: false,
            announcement: String::new(),
            min_client_version: "0.1.0".into(),
            relay_hint: String::new(),
            relay_admin: String::new(),
        }
    }
}

/// 配置文件访问器:内存共享 + `config.json` 落盘。
#[derive(Debug, Clone)]
pub struct ConfigStore {
    path: PathBuf,
    inner: Arc<RwLock<ServerConfig>>,
}

impl ConfigStore {
    /// 加载(或初始化)配置。`cli_defaults` 为命令行初始值,仅当配置文件
    /// 不存在时作为默认写入;之后以文件内容为准。
    pub fn new(data_dir: &Path, cli_defaults: ServerConfig) -> Self {
        std::fs::create_dir_all(data_dir)
            .unwrap_or_else(|e| log::warn!("[config] 创建数据目录失败: {e}"));
        let path = data_dir.join("config.json");
        let cfg = match std::fs::read_to_string(&path) {
            Ok(s) => match serde_json::from_str::<ServerConfig>(&s) {
                Ok(c) => {
                    log::info!("[config] 已加载配置: {}", path.display());
                    c
                }
                Err(e) => {
                    log::warn!("[config] 配置文件解析失败,使用初始默认: {e}");
                    cli_defaults
                }
            },
            Err(_) => {
                log::info!("[config] 配置文件不存在,写入初始默认: {}", path.display());
                cli_defaults
            }
        };
        let store = Self {
            path,
            inner: Arc::new(RwLock::new(cfg)),
        };
        store.save();
        store
    }

    /// 保存当前配置到磁盘。
    pub fn save(&self) {
        let cfg = self.inner.read().unwrap_or_else(|e| e.into_inner());
        match serde_json::to_string_pretty(&*cfg) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&self.path, json) {
                    log::error!("[config] 写入配置文件失败: {e}");
                }
            }
            Err(e) => log::error!("[config] 序列化配置失败: {e}"),
        }
    }

    /// 读取当前配置(克隆)。
    pub fn get(&self) -> ServerConfig {
        self.inner.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// 用新配置整体替换并落盘(管理后台 PUT 使用)。
    pub fn update(&self, cfg: ServerConfig) {
        let summary = format!(
            "open_register={}, maintenance={}, min_password_len={}, max_devices_per_user={}, max_concurrent_sessions={}, min_client_version={}",
            cfg.open_register,
            cfg.maintenance_mode,
            cfg.min_password_len,
            cfg.max_devices_per_user,
            cfg.max_concurrent_sessions,
            cfg.min_client_version,
        );
        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
        *guard = cfg;
        drop(guard);
        self.save();
        op_log("config", "update", &summary);
    }

    /// 读取单项:维护模式。
    pub fn is_maintenance(&self) -> bool {
        self.get().maintenance_mode
    }

    /// 读取单项:密码最小长度。
    pub fn min_password_len(&self) -> usize {
        self.get().min_password_len
    }

    /// 读取单项:中继地址。
    pub fn relay_hint(&self) -> String {
        self.get().relay_hint
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("dcr-config-test-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn load_or_create_writes_defaults() {
        let dir = tmp_dir("create");
        let store = ConfigStore::new(&dir, ServerConfig::default());
        assert!(store.get().open_register);
        // 配置文件应已落盘
        let path = dir.join("config.json");
        assert!(path.exists(), "配置应落盘");
        let back: ServerConfig =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(back.open_register);
        assert_eq!(back.min_client_version, "0.1.0");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn update_persists_and_reloads() {
        let dir = tmp_dir("update");
        let store = ConfigStore::new(&dir, ServerConfig::default());
        let mut cfg = store.get();
        cfg.maintenance_mode = true;
        cfg.min_password_len = 8;
        cfg.announcement = "维护公告".into();
        store.update(cfg);
        // 重载确认持久化
        let store2 = ConfigStore::new(&dir, ServerConfig::default());
        assert!(store2.is_maintenance());
        assert_eq!(store2.min_password_len(), 8);
        assert_eq!(store2.get().announcement, "维护公告");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cli_defaults_used_only_first_time() {
        let dir = tmp_dir("cli");
        let mut cli = ServerConfig::default();
        cli.open_register = false;
        cli.relay_hint = "relay.example.com:21117".into();
        let store = ConfigStore::new(&dir, cli);
        assert!(!store.get().open_register, "首次应使用 CLI 默认");
        assert_eq!(store.get().relay_hint, "relay.example.com:21117");
        // 二次创建(文件已存在)不应覆盖文件内容
        let store2 = ConfigStore::new(&dir, ServerConfig::default());
        assert!(!store2.get().open_register, "应以文件为准");
        std::fs::remove_dir_all(&dir).ok();
    }
}
