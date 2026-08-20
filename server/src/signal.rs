//! 信令服务核心(dcr-signal)。
//!
//! 基于 RustDesk hbbs 思路的信令 + NAT 探测服务:
//! - TCP(默认 21116):设备注册 / 心跳保活 / 查找对端 / 在线列表,消息为
//!   长度前缀 JSON(`crate::message::SignalMsg`),连接断开自动注销;
//! - UDP(默认 21115):RFC 5389 标准 STUN Binding(返回 XOR-MAPPED-ADDRESS 反射地址)
//!   并附带一次"不同源端口"的 NAT 探测;同时接受 `{"t":"stun"}` 调试请求。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use tokio::net::{TcpListener, TcpStream, UdpSocket};

use crate::config::ConfigStore;
use crate::devices::DeviceStore;
use crate::framing::{read_msg, write_msg};
use crate::message::{PeerEntry, SignalMsg};
use crate::sessions::SessionCore;

/// 在线判定超时(超过该时长未心跳视为离线)。
const ONLINE_TIMEOUT: Duration = Duration::from_secs(60);
/// 连接读循环空闲超时(客户端心跳间隔 20s,留足余量)。
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// 一个已注册对端的记录。
#[derive(Clone)]
pub struct PeerRecord {
    /// 局域网地址(客户端上报,"ip:port")。
    pub lan: String,
    /// 外部地址(服务端观察到该对端的连接地址)。
    pub external: String,
    /// 最近心跳时间。
    pub last_seen: Instant,
}

/// 信令服务核心状态(跨连接共享)。
#[derive(Clone)]
pub struct SignalCore {
    peers: Arc<Mutex<HashMap<String, PeerRecord>>>,
    relay_hint: Arc<String>,
    /// 设备档案(持久化)。
    devices: Arc<DeviceStore>,
    /// 实时会话(中继上报)。
    sessions: Arc<SessionCore>,
    /// 服务策略配置(共享)。
    cfg: Arc<RwLock<ConfigStore>>,
}

impl SignalCore {
    /// 创建核心,`relay_hint` 为可选中继服务器地址("host:port",空串表示无)。
    pub fn new(relay_hint: &str) -> Self {
        let cfg = Arc::new(RwLock::new(ConfigStore::new(
            &std::env::temp_dir().join("dcr-signal-config-default"),
            crate::config::ServerConfig {
                relay_hint: relay_hint.to_string(),
                ..Default::default()
            },
        )));
        Self {
            peers: Arc::new(Mutex::new(HashMap::new())),
            relay_hint: Arc::new(relay_hint.to_string()),
            devices: Arc::new(DeviceStore::new(&std::env::temp_dir().join("dcr-signal-devices-default"))),
            sessions: Arc::new(SessionCore::new()),
            cfg,
        }
    }

    /// 创建核心并注入共享存储(由入口程序装配;`cfg` 已持久化到数据目录)。
    pub fn with_stores(
        relay_hint: &str,
        devices: Arc<DeviceStore>,
        sessions: Arc<SessionCore>,
        cfg: Arc<RwLock<ConfigStore>>,
    ) -> Self {
        Self {
            peers: Arc::new(Mutex::new(HashMap::new())),
            relay_hint: Arc::new(relay_hint.to_string()),
            devices,
            sessions,
            cfg,
        }
    }

    /// 共享配置存储。
    pub fn config(&self) -> Arc<RwLock<ConfigStore>> {
        self.cfg.clone()
    }

    /// 共享设备档案。
    pub fn device_store(&self) -> Arc<DeviceStore> {
        self.devices.clone()
    }

    /// 共享会话核心。
    pub fn session_core(&self) -> Arc<SessionCore> {
        self.sessions.clone()
    }

    /// 服务端注册策略校验:返回 Ok 表示允许,Err 携带拒绝原因。
    /// - 维护模式拒绝新注册;
    /// - 客户端版本低于下限拒绝;
    /// - 单用户设备数超上限拒绝(该设备尚未登记时);
    /// - 设备被管理员禁用拒绝。
    pub fn check_register_policy(&self, user: &str, version: &str, id: &str) -> Result<(), String> {
        let cfg = self.cfg.read().unwrap_or_else(|e| e.into_inner()).get();
        if cfg.maintenance_mode {
            return Err("服务器维护中,请稍后再试".into());
        }
        if !version.is_empty() && !cfg.min_client_version.is_empty()
            && version_cmp(version, &cfg.min_client_version) < 0
        {
            return Err(format!(
                "客户端版本过低(当前 {version},最低 {})，请升级客户端",
                cfg.min_client_version
            ));
        }
        if !self.devices.is_enabled(id) {
            return Err("设备已被管理员禁用".into());
        }
        if !user.is_empty() && cfg.max_devices_per_user > 0 {
            let owned = self.devices.count_by_owner(user);
            if owned >= cfg.max_devices_per_user {
                return Err(format!(
                    "设备数已达上限({}台),请先在后台删除不再使用的设备",
                    cfg.max_devices_per_user
                ));
            }
        }
        Ok(())
    }

    /// 注册对端(同 id 重复注册视为更新地址,last one wins)。
    pub fn register(&self, id: &str, lan: &str, external: &str) {
        if let Ok(mut map) = self.peers.lock() {
            map.insert(
                id.to_string(),
                PeerRecord {
                    lan: lan.to_string(),
                    external: external.to_string(),
                    last_seen: Instant::now(),
                },
            );
        }
    }

    /// 心跳续期;未注册的 id 返回 Err。
    pub fn heartbeat(&self, id: &str) -> Result<(), String> {
        let mut map = self.peers.lock().map_err(|e| e.to_string())?;
        match map.get_mut(id) {
            Some(rec) => {
                rec.last_seen = Instant::now();
                Ok(())
            }
            None => Err(format!("未注册的 id: {id}")),
        }
    }

    /// 注销对端。
    pub fn unregister(&self, id: &str) {
        if let Ok(mut map) = self.peers.lock() {
            map.remove(id);
        }
    }

    /// 查找对端在线信息,返回 `(lan, external, relay_hint)`;离线/未知返回 None。
    pub fn lookup(&self, id: &str) -> Option<(String, String, String)> {
        let mut map = self.peers.lock().ok()?;
        let rec = map.get(id)?;
        if rec.last_seen.elapsed() > ONLINE_TIMEOUT {
            map.remove(id);
            return None;
        }
        Some((
            rec.lan.clone(),
            rec.external.clone(),
            self.relay_hint.as_str().to_string(),
        ))
    }

    /// 在线对端列表(自动剔除超时条目)。
    pub fn list_online(&self) -> Vec<PeerEntry> {
        let mut map = self.peers.lock().unwrap_or_else(|e| e.into_inner());
        map.retain(|_, rec| rec.last_seen.elapsed() <= ONLINE_TIMEOUT);
        map.iter()
            .map(|(id, rec)| PeerEntry {
                id: id.clone(),
                lan: rec.lan.clone(),
                external: rec.external.clone(),
            })
            .collect()
    }

    /// 移除超时条目(定时调用)。
    pub fn prune(&self) {
        if let Ok(mut map) = self.peers.lock() {
            map.retain(|_, rec| rec.last_seen.elapsed() <= ONLINE_TIMEOUT);
        }
    }
}

/// 语义化版本比较:a < b 返回负,相等返回 0,a > b 返回正。
/// 按点号分段比较数字段;非数字段按字符串比较;段数不足视为 0。
fn version_cmp(a: &str, b: &str) -> i32 {
    let pa: Vec<&str> = a.split('.').collect();
    let pb: Vec<&str> = b.split('.').collect();
    let n = pa.len().max(pb.len());
    for i in 0..n {
        let sa = pa.get(i).copied().unwrap_or("0");
        let sb = pb.get(i).copied().unwrap_or("0");
        match (sa.parse::<i64>(), sb.parse::<i64>()) {
            (Ok(na), Ok(nb)) => {
                if na != nb {
                    return (na - nb).signum() as i32;
                }
            }
            _ => {
                let ord = sa.cmp(sb);
                if ord != std::cmp::Ordering::Equal {
                    return match ord {
                        std::cmp::Ordering::Less => -1,
                        std::cmp::Ordering::Greater => 1,
                        std::cmp::Ordering::Equal => 0,
                    };
                }
            }
        }
    }
    0
}

/// 处理单个信令 TCP 连接:循环读消息、按类型处理,断开时注销该连接登记的 id。
pub async fn handle_signal_conn(core: Arc<SignalCore>, mut stream: TcpStream) {
    let addr = stream.peer_addr().map(|a| a.to_string()).unwrap_or_default();
    let mut conn_id: Option<String> = None;
    loop {
        let msg: SignalMsg = match tokio::time::timeout(IDLE_TIMEOUT, read_msg(&mut stream)).await {
            Ok(Ok(m)) => m,
            Ok(Err(e)) => {
                log::debug!("[signal] 连接读消息结束({addr}): {e}");
                break;
            }
            Err(_) => {
                log::debug!("[signal] 连接空闲超时({addr}),断开");
                break;
            }
        };
        match msg {
            SignalMsg::Register {
                id,
                lan,
                name,
                os,
                version,
                user,
            } => {
                // 策略校验:维护模式 / 版本下限 / 设备禁用 / 设备数上限
                if let Err(e) = core.check_register_policy(&user, &version, &id) {
                    log::warn!("[signal] 注册被拒: id={id}, 原因={e}");
                    let _ = write_msg(
                        &mut stream,
                        &SignalMsg::RegisterAck { ok: false, msg: e },
                    )
                    .await;
                    break;
                }
                let duplicated = core
                    .peers
                    .lock()
                    .map(|m| m.contains_key(&id))
                    .unwrap_or(false);
                core.register(&id, &lan, &addr);
                core.devices
                    .touch(&id, &user, &name, &os, &version, &lan, &addr, true);
                conn_id = Some(id.clone());
                log::info!("[signal] 注册: id={id}, lan={lan}, external={addr}, user={user}, os={os}, v={version}");
                let _ = write_msg(
                    &mut stream,
                    &SignalMsg::RegisterAck {
                        ok: true,
                        msg: if duplicated {
                            "id 已存在,已更新地址".into()
                        } else {
                            "ok".into()
                        },
                    },
                )
                .await;
            }
            SignalMsg::Heartbeat { id } => {
                match core.heartbeat(&id) {
                    Ok(()) => {
                        core.devices.set_online(&id, true);
                        let _ = write_msg(
                            &mut stream,
                            &SignalMsg::RegisterAck {
                                ok: true,
                                msg: "ok".into(),
                            },
                        )
                        .await;
                    }
                    Err(e) => {
                        let _ = write_msg(
                            &mut stream,
                            &SignalMsg::RegisterAck {
                                ok: false,
                                msg: e,
                            },
                        )
                        .await;
                    }
                }
            }
            SignalMsg::Lookup { id } => {
                let found = core.lookup(&id);
                let (online, lan, external, relay_hint) = match found {
                    Some((lan, external, relay_hint)) => (true, lan, external, relay_hint),
                    None => (false, String::new(), String::new(), String::new()),
                };
                let _ = write_msg(
                    &mut stream,
                    &SignalMsg::LookupAck {
                        online,
                        lan,
                        external,
                        relay_hint,
                    },
                )
                .await;
            }
            SignalMsg::List => {
                let peers = core.list_online();
                log::info!("[signal] list: 在线 {} 个", peers.len());
                let _ = write_msg(&mut stream, &SignalMsg::ListAck { peers }).await;
            }
            SignalMsg::Unregister { id } => {
                core.unregister(&id);
                if conn_id.as_deref() == Some(id.as_str()) {
                    conn_id = None;
                }
                log::info!("[signal] 注销: id={id}");
                let _ = write_msg(
                    &mut stream,
                    &SignalMsg::RegisterAck {
                        ok: true,
                        msg: "ok".into(),
                    },
                )
                .await;
            }
            _ => {}
        }
    }
    // 连接断开,注销该连接登记的对端
    if let Some(id) = conn_id.take() {
        core.unregister(&id);
        core.devices.set_online(&id, false);
        log::info!("[signal] 连接断开({addr}),注销 id={id}");
    }
}

/// 处理一个 UDP 数据报(STUN Binding / JSON 调试请求 / 中继会话事件)。
pub async fn handle_stun_packet(
    sock: &UdpSocket,
    probe_sock: &UdpSocket,
    sessions: Option<Arc<SessionCore>>,
    max_concurrent: usize,
    buf: Vec<u8>,
    src: SocketAddr,
) {
    // 标准 RFC 5389 Binding Request:头 20 字节,type=0x0001
    if buf.len() >= 20 && buf[0] == 0x00 && buf[1] == 0x01 {
        if let Ok(txn) = crate::stun::parse_binding_request(&buf) {
            match crate::stun::build_binding_response(&txn, src) {
                Ok(resp) => {
                    if let Err(e) = sock.send_to(&resp, src).await {
                        log::warn!("[signal] STUN 响应发送失败: {e}");
                        return;
                    }
                    // 从不同源端口向同一地址发探测包(best-effort NAT 类型判断)
                    let _ = probe_sock.send_to(b"P", src).await;
                }
                Err(e) => log::warn!("[signal] STUN 响应构造失败: {e}"),
            }
            return;
        }
    }
    // 调试/会话事件用的 JSON 请求
    if buf.starts_with(b"{") {
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&buf) {
            let t = v.get("t").and_then(|t| t.as_str());
            match t {
                Some("stun") => {
                    let mapped = format!("{{\"t\":\"binding\",\"mapped\":\"{src}\"}}");
                    let _ = sock.send_to(mapped.as_bytes(), src).await;
                }
                // 中继上报会话开始/结束(仅本地回环信任来源;UDP 无法鉴权,
                // 仅影响监控展示,不影响任何权限数据)
                Some("session-start") => {
                    let id = v.get("id").and_then(|x| x.as_str()).unwrap_or_default();
                    let host = v.get("host").and_then(|x| x.as_str()).unwrap_or_default();
                    let client = v.get("client").and_then(|x| x.as_str()).unwrap_or_default();
                    if let Some(sc) = &sessions {
                        let _ = sc.start(id, host, client, max_concurrent);
                    }
                }
                Some("session-end") => {
                    let id = v.get("id").and_then(|x| x.as_str()).unwrap_or_default();
                    if let Some(sc) = &sessions {
                        sc.end(id);
                    }
                }
                _ => {}
            }
        }
    }
}

/// 启动完整信令服务(TCP accept + UDP STUN + 定时清理)。
///
/// `listener` 与 `udp_socket` 应已绑定;`core` 为共享信令核心(由调用方创建,
/// 可同时供 Web 管理后台读取在线设备列表)。
pub async fn serve(
    listener: TcpListener,
    udp_socket: UdpSocket,
    core: Arc<SignalCore>,
) -> Result<(), String> {
    let bind_addr = udp_socket.local_addr().map_err(|e| e.to_string())?;
    log::info!("[signal] STUN/UDP 服务地址: {bind_addr}");

    // UDP(STUN)循环
    let probe_sock = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| format!("绑定探测 socket 失败: {e}"))?;
    let udp_sessions = core.sessions.clone();
    let udp_cfg = core.cfg.clone();
    tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        loop {
            let (n, src) = match udp_socket.recv_from(&mut buf).await {
                Ok(v) => v,
                Err(e) => {
                    log::error!("[signal] UDP recv 失败: {e}");
                    continue;
                }
            };
            let max_concurrent = udp_cfg
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .get()
                .max_concurrent_sessions;
            handle_stun_packet(
                &udp_socket,
                &probe_sock,
                Some(udp_sessions.clone()),
                max_concurrent,
                buf[..n].to_vec(),
                src,
            )
            .await;
        }
    });

    // 定时清理超时条目与空闲会话
    let prune_core = core.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(10));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            prune_core.prune();
            let idle = prune_core
                .cfg
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .get()
                .session_idle_timeout_secs;
            prune_core
                .sessions
                .prune(Duration::from_secs(idle));
        }
    });

    // TCP accept 循环
    loop {
        let (stream, addr) = listener
            .accept()
            .await
            .map_err(|e| format!("accept 失败: {e}"))?;
        log::info!("[signal] 新连接: {addr}");
        let core = core.clone();
        tokio::spawn(async move {
            handle_signal_conn(core, stream).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_lookup_heartbeat_cycle() {
        let core = SignalCore::new("relay.example.com:21117");
        assert!(core.lookup("pc-a").is_none(), "未注册时应查不到");
        core.register("pc-a", "192.168.1.5:21118", "203.0.113.9:21118");
        let (lan, external, hint) = core.lookup("pc-a").unwrap();
        assert_eq!(lan, "192.168.1.5:21118");
        assert_eq!(external, "203.0.113.9:21118");
        assert_eq!(hint, "relay.example.com:21117");
        assert!(core.heartbeat("pc-a").is_ok());
        assert!(core.heartbeat("nobody").is_err());
        core.unregister("pc-a");
        assert!(core.lookup("pc-a").is_none(), "注销后应查不到");
    }

    #[test]
    fn list_online_returns_registered() {
        let core = SignalCore::new("");
        core.register("a", "10.0.0.1:1", "1.1.1.1:1");
        core.register("b", "10.0.0.2:1", "2.2.2.2:1");
        let peers = core.list_online();
        assert_eq!(peers.len(), 2);
        assert!(peers.iter().any(|p| p.id == "a"));
        assert!(peers.iter().any(|p| p.id == "b"));
    }

    #[test]
    fn duplicate_register_replaces_addr() {
        let core = SignalCore::new("");
        core.register("x", "10.0.0.1:1", "1.1.1.1:1");
        core.register("x", "10.0.0.2:2", "1.1.1.1:2");
        let (lan, external, _) = core.lookup("x").unwrap();
        assert_eq!(lan, "10.0.0.2:2");
        assert_eq!(external, "1.1.1.1:2");
    }
}
