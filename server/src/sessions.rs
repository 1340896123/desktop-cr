//! 实时会话监控(dcr-signal Web 管理后台会话监控)。
//!
//! - 只保留**活跃会话**(内存,不持久化历史——本产品不审计);
//! - 会话事件由 dcr-relay 经 UDP 上报(`{"t":"session-start"|"session-end"}`),
//!   信令侧 `handle_stun_packet` 接收并写入本模块;
//! - 提供空闲超时清理与全局并发上限检查。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::operation_log::op_log;

/// 单个活跃会话。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    /// 中继 peer id(会话标识)。
    pub id: String,
    /// host 侧地址。
    pub host: String,
    /// client 侧地址。
    pub client: String,
    /// 链路类型(当前均为 "relay")。
    pub via: String,
    /// 开始时间(ISO 8601)。
    pub started_at: String,
}

/// 会话核心:内存活跃会话表 + 各自最近活动时间。
#[derive(Debug, Default)]
pub struct SessionCore {
    sessions: Mutex<HashMap<String, SessionRecord>>,
    activity: Mutex<HashMap<String, Instant>>,
}

impl SessionCore {
    /// 新建空会话核心。
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录会话开始;同 id 已存在则替换(视为旧会话被新会话覆盖)。
    /// `max_concurrent` 非 0 且当前会话数已达上限时返回 Err(不写入)。
    pub fn start(&self, id: &str, host: &str, client: &str, max_concurrent: usize) -> Result<(), String> {
        {
            let map = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
            if max_concurrent > 0 && map.len() >= max_concurrent && !map.contains_key(id) {
                return Err(format!("全局并发会话已达上限({max_concurrent})"));
            }
        }
        let rec = SessionRecord {
            id: id.to_string(),
            host: host.to_string(),
            client: client.to_string(),
            via: "relay".into(),
            started_at: crate::auth::now_iso(),
        };
        self.sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.to_string(), rec);
        self.activity
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.to_string(), Instant::now());
        log::info!("[sessions] 会话开始: id={id}, host={host}, client={client}");
        op_log(
            "sessions",
            "start",
            &format!("id={id}, host={host}, client={client}"),
        );
        Ok(())
    }

    /// 记录会话结束。
    pub fn end(&self, id: &str) {
        let removed = self
            .sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id)
            .is_some();
        self.activity
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
        if removed {
            log::info!("[sessions] 会话结束: id={id}");
            op_log("sessions", "end", &format!("id={id}"));
        }
    }

    /// 活跃会话列表(按 id 排序)。
    pub fn list(&self) -> Vec<SessionRecord> {
        let mut list: Vec<SessionRecord> = self
            .sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }

    /// 当前活跃会话数。
    pub fn count(&self) -> usize {
        self.sessions
            .lock()
            .map(|m| m.len())
            .unwrap_or_default()
    }

    /// 清理超过 `max_idle` 未活动的会话(定时调用)。
    pub fn prune(&self, max_idle: Duration) {
        let now = Instant::now();
        let stale: Vec<String> = self
            .activity
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|(_, at)| now.duration_since(**at) > max_idle)
            .map(|(id, _)| id.clone())
            .collect();
        for id in stale {
            self.end(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_end_list_cycle() {
        let core = SessionCore::new();
        core.start("peer-1", "10.0.0.1:21118", "10.0.0.2:21117", 0).unwrap();
        assert_eq!(core.count(), 1);
        let rec = core.list().remove(0);
        assert_eq!(rec.id, "peer-1");
        assert_eq!(rec.via, "relay");
        core.end("peer-1");
        assert_eq!(core.count(), 0);
        assert!(core.list().is_empty());
    }

    #[test]
    fn concurrent_limit_rejects() {
        let core = SessionCore::new();
        core.start("a", "h1", "c1", 1).unwrap();
        // 上限 1,新 id 被拒
        assert!(core.start("b", "h2", "c2", 1).is_err());
        assert_eq!(core.count(), 1);
        // 同 id 覆盖不计数上限
        assert!(core.start("a", "h1", "c9", 1).is_ok());
        assert_eq!(core.count(), 1);
    }

    #[test]
    fn prune_removes_idle() {
        let core = SessionCore::new();
        core.start("a", "h1", "c1", 0).unwrap();
        core.start("b", "h2", "c2", 0).unwrap();
        // 0 空闲超时:立即清理
        core.prune(Duration::from_secs(0));
        assert_eq!(core.count(), 0);
    }
}
