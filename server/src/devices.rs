//! 设备档案持久化(dcr-signal Web 管理后台设备管理)。
//!
//! - 持久化于 `data-dir/devices.json`(含离线设备,全部设备可见);
//! - 信令注册/心跳/断开时驱动 online / last_seen / lan / external 等字段;
//! - 管理后台可查看全部设备、启用/禁用、删除;
//! - 设备归属用户(owner)由注册消息携带(登录用户名,未登录为空)。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::operation_log::op_log;

/// 单个设备档案。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRecord {
    /// 设备唯一标识。
    pub id: String,
    /// 归属用户名(未登录为空串)。
    pub owner: String,
    /// 设备名(客户端上报)。
    pub name: String,
    /// 操作系统(客户端上报)。
    pub os: String,
    /// 客户端版本(客户端上报)。
    pub version: String,
    /// 局域网地址("ip:port")。
    pub lan: String,
    /// 外部地址(服务端观察到该设备连接地址)。
    pub external: String,
    /// 首次注册时间(ISO 8601)。
    pub first_seen: String,
    /// 最后心跳时间(ISO 8601)。
    pub last_seen: String,
    /// 当前是否在线。
    pub online: bool,
    /// 是否启用(管理员禁用后拒绝注册)。
    pub enabled: bool,
}

/// 设备存储:内存 HashMap + `devices.json` 落盘。
#[derive(Debug)]
pub struct DeviceStore {
    data_dir: PathBuf,
    devices: Mutex<HashMap<String, DeviceRecord>>,
}

impl DeviceStore {
    /// 加载(或初始化空)设备存储;`data_dir` 不存在时创建。
    pub fn new(data_dir: &Path) -> Self {
        std::fs::create_dir_all(data_dir)
            .unwrap_or_else(|e| log::warn!("[devices] 创建数据目录失败: {e}"));
        let store = Self {
            data_dir: data_dir.to_path_buf(),
            devices: Mutex::new(HashMap::new()),
        };
        store.load();
        store
    }

    fn devices_file(&self) -> PathBuf {
        self.data_dir.join("devices.json")
    }

    fn load(&self) {
        let path = self.devices_file();
        match std::fs::read_to_string(&path) {
            Ok(s) => match serde_json::from_str::<Vec<DeviceRecord>>(&s) {
                Ok(devices) => {
                    let mut map = HashMap::new();
                    for d in devices {
                        map.insert(d.id.clone(), d);
                    }
                    let count = map.len();
                    *self.devices.lock().unwrap_or_else(|e| e.into_inner()) = map;
                    log::info!("[devices] 已加载 {count} 个设备档案: {}", path.display());
                }
                Err(e) => log::warn!("[devices] 设备文件解析失败,从空列表启动: {e}"),
            },
            Err(_) => log::info!("[devices] 设备文件不存在,首次运行: {}", path.display()),
        }
    }

    fn save(&self) {
        let mut devices: Vec<DeviceRecord> = self
            .devices
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect();
        devices.sort_by(|a, b| a.id.cmp(&b.id));
        match serde_json::to_string_pretty(&devices) {
            Ok(json) => {
                if let Err(e) = std::fs::write(self.devices_file(), json) {
                    log::error!("[devices] 写入设备文件失败: {e}");
                }
            }
            Err(e) => log::error!("[devices] 序列化设备列表失败: {e}"),
        }
    }

    /// 注册或更新设备档案(信令注册/心跳调用)。同 id 更新在线信息;
    /// 首次登记记录 first_seen。返回是否为首次注册。
    pub fn touch(
        &self,
        id: &str,
        owner: &str,
        name: &str,
        os: &str,
        version: &str,
        lan: &str,
        external: &str,
        online: bool,
    ) -> bool {
        let now = crate::auth::now_iso();
        let mut map = self.devices.lock().unwrap_or_else(|e| e.into_inner());
        let is_new = !map.contains_key(id);
        let rec = map.entry(id.to_string()).or_insert_with(|| DeviceRecord {
            id: id.to_string(),
            owner: owner.to_string(),
            name: name.to_string(),
            os: os.to_string(),
            version: version.to_string(),
            lan: lan.to_string(),
            external: external.to_string(),
            first_seen: now.clone(),
            last_seen: now.clone(),
            online,
            enabled: true,
        });
        rec.owner = owner.to_string();
        rec.name = name.to_string();
        rec.os = os.to_string();
        rec.version = version.to_string();
        rec.lan = lan.to_string();
        rec.external = external.to_string();
        rec.last_seen = now;
        rec.online = online;
        drop(map);
        self.save();
        if is_new {
            op_log(
                "devices",
                "register",
                &format!("id={id}, owner={owner}, name={name}"),
            );
        }
        is_new
    }

    /// 仅更新在线状态与最后心跳时间(心跳续期 / 断开时调用)。
    pub fn set_online(&self, id: &str, online: bool) {
        let mut map = self.devices.lock().unwrap_or_else(|e| e.into_inner());
        let changed = match map.get_mut(id) {
            Some(rec) => {
                let c = rec.online != online;
                rec.online = online;
                rec.last_seen = crate::auth::now_iso();
                c
            }
            None => false,
        };
        drop(map);
        if changed {
            self.save();
        }
    }

    /// 设置设备启用/禁用(管理后台)。
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        let mut map = self.devices.lock().unwrap_or_else(|e| e.into_inner());
        let rec = map
            .get_mut(id)
            .ok_or_else(|| format!("设备不存在: {id}"))?;
        rec.enabled = enabled;
        drop(map);
        self.save();
        log::info!("[devices] 设备 {id} 已{}", if enabled { "启用" } else { "禁用" });
        op_log(
            "devices",
            "set_enabled",
            &format!("id={id} {}", if enabled { "enabled" } else { "disabled" }),
        );
        Ok(())
    }

    /// 查询设备是否启用(不存在视为启用,注册时按需调用)。
    pub fn is_enabled(&self, id: &str) -> bool {
        self.devices
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .map(|d| d.enabled)
            .unwrap_or(true)
    }

    /// 删除设备档案(管理后台)。返回是否删除成功。
    pub fn delete(&self, id: &str) -> Result<(), String> {
        let mut map = self.devices.lock().unwrap_or_else(|e| e.into_inner());
        if map.remove(id).is_none() {
            return Err(format!("设备不存在: {id}"));
        }
        drop(map);
        self.save();
        log::info!("[devices] 已删除设备: {id}");
        op_log("devices", "delete", &format!("id={id}"));
        Ok(())
    }

    /// 查询单个设备档案(不存在返回 None)。
    pub fn get(&self, id: &str) -> Option<DeviceRecord> {
        self.devices
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .cloned()
    }

    /// 全部设备档案(含离线,按 id 排序)。
    pub fn list(&self) -> Vec<DeviceRecord> {
        let mut devices: Vec<DeviceRecord> = self
            .devices
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect();
        devices.sort_by(|a, b| a.id.cmp(&b.id));
        devices
    }

    /// 某用户当前已登记设备数(用于设备数上限校验)。
    pub fn count_by_owner(&self, owner: &str) -> usize {
        self.devices
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .filter(|d| d.owner == owner)
            .count()
    }

    /// 某用户已登记设备数(排除指定 id,用于重连时的上限校验:已登记设备不占新名额)。
    pub fn count_by_owner_excluding(&self, owner: &str, exclude_id: &str) -> usize {
        self.devices
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .filter(|d| d.owner == owner && d.id != exclude_id)
            .count()
    }

    /// 设备总数(含离线)。
    pub fn count(&self) -> usize {
        self.devices
            .lock()
            .map(|m| m.len())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dcr-devices-test-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn register_touch_and_persistence() {
        let dir = tmp_dir("crud");
        let store = DeviceStore::new(&dir);
        let is_new = store.touch("pc-a", "alice", "办公室PC", "Windows 11", "0.1.0", "192.168.1.5:21118", "203.0.113.9:21118", true);
        assert!(is_new, "首次应登记");
        assert_eq!(store.count(), 1);
        let rec = store.list().remove(0);
        assert_eq!(rec.owner, "alice");
        assert_eq!(rec.name, "办公室PC");
        assert!(rec.online);

        // 二次 touch 不算新设备,更新在线信息
        let is_new2 = store.touch("pc-a", "alice", "办公室PC", "Windows 11", "0.1.0", "192.168.1.5:21118", "203.0.113.9:21118", true);
        assert!(!is_new2);

        // 重载持久化
        let store2 = DeviceStore::new(&dir);
        assert_eq!(store2.count(), 1);
        assert_eq!(store2.list()[0].id, "pc-a");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn enable_disable_and_delete() {
        let dir = tmp_dir("en");
        let store = DeviceStore::new(&dir);
        store.touch("pc-a", "bob", "PC", "Windows", "0.1.0", "l", "e", true);
        assert!(store.is_enabled("pc-a"));
        store.set_enabled("pc-a", false).unwrap();
        assert!(!store.is_enabled("pc-a"));
        assert!(store.set_enabled("nope", false).is_err());
        store.delete("pc-a").unwrap();
        assert_eq!(store.count(), 0);
        assert!(store.delete("pc-a").is_err(), "二次删除应失败");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn online_flag_and_owner_count() {
        let dir = tmp_dir("owner");
        let store = DeviceStore::new(&dir);
        store.touch("a", "alice", "A", "Windows", "0.1.0", "l", "e", true);
        store.touch("b", "alice", "B", "Windows", "0.1.0", "l", "e", true);
        store.touch("c", "bob", "C", "Windows", "0.1.0", "l", "e", true);
        assert_eq!(store.count_by_owner("alice"), 2);
        assert_eq!(store.count_by_owner("bob"), 1);
        store.set_online("a", false);
        let list = store.list();
        assert!(!list.iter().find(|d| d.id == "a").unwrap().online);
        assert!(list.iter().find(|d| d.id == "b").unwrap().online);
        std::fs::remove_dir_all(&dir).ok();
    }
}
