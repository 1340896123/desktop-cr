//! 信令服务核心(dcr-signal)。
//!
//! 基于 RustDesk hbbs 思路的信令 + NAT 探测服务:
//! - TCP(默认 21116):设备注册 / 心跳保活 / 查找对端 / 在线列表,消息为
//!   长度前缀 JSON(`crate::message::SignalMsg`),连接断开自动注销;
//! - UDP(默认 21115):RFC 5389 标准 STUN Binding(返回 XOR-MAPPED-ADDRESS 反射地址)
//!   并附带一次"不同源端口"的 NAT 探测;同时接受 `{"t":"stun"}` 调试请求。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use tokio::net::{TcpListener, TcpStream, UdpSocket};

use crate::auth;
use crate::config::ConfigStore;
use crate::devices::DeviceStore;
use crate::framing::{read_msg, write_msg};
use crate::message::{PeerEntry, SignalMsg};
use crate::sessions::SessionCore;

use crate::operation_log::op_log;

/// 在线判定超时(超过该时长未心跳视为离线)。
const ONLINE_TIMEOUT: Duration = Duration::from_secs(60);
/// 连接读循环空闲超时(客户端心跳间隔 5s,留足余量)。
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
    /// 注册代次:每次注册递增。连接断开时仅当记录仍持有该代次才注销,
    /// 避免旧连接误删已被新连接接管的同 id 记录。
    pub generation: u64,
}

/// 一次在线设备列表查询的过滤统计。
#[derive(Debug, Clone, Copy)]
pub struct OnlineListStats {
    /// 过期条目清理后的全部在线设备数。
    pub online_total: usize,
    /// 按请求账号过滤后返回的设备数。
    pub returned: usize,
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
    /// 注册代次分配器(每次注册递增)。
    next_gen: Arc<AtomicU64>,
    /// JWT 签名密钥;空 Vec 表示未启用账号认证(直接信任客户端上报的 user)。
    auth_secret: Arc<Vec<u8>>,
}

impl SignalCore {
    /// 创建核心,`relay_hint` 为可选中继服务器地址("host:port",空串表示无)。
    /// 不启用账号认证(测试/自托管开放模式用)。
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
            devices: Arc::new(DeviceStore::new(
                &std::env::temp_dir().join("dcr-signal-devices-default"),
            )),
            sessions: Arc::new(SessionCore::new()),
            cfg,
            next_gen: Arc::new(AtomicU64::new(1)),
            auth_secret: Arc::new(Vec::new()),
        }
    }

    /// 创建核心并注入共享存储(由入口程序装配;`cfg` 已持久化到数据目录)。
    /// `auth_secret` 为 JWT 签名密钥(为空表示不启用账号认证)。
    pub fn with_stores(
        relay_hint: &str,
        devices: Arc<DeviceStore>,
        sessions: Arc<SessionCore>,
        cfg: Arc<RwLock<ConfigStore>>,
        auth_secret: Vec<u8>,
    ) -> Self {
        Self {
            peers: Arc::new(Mutex::new(HashMap::new())),
            relay_hint: Arc::new(relay_hint.to_string()),
            devices,
            sessions,
            cfg,
            next_gen: Arc::new(AtomicU64::new(1)),
            auth_secret: Arc::new(auth_secret),
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
    /// - 单用户设备数超上限拒绝(该设备尚未登记时,已登记设备重连不占新名额);
    /// - 设备被管理员禁用拒绝。
    pub fn check_register_policy(&self, user: &str, version: &str, id: &str) -> Result<(), String> {
        let cfg = self.cfg.read().unwrap_or_else(|e| e.into_inner()).get();
        if cfg.maintenance_mode {
            return Err("服务器维护中,请稍后再试".into());
        }
        if !version.is_empty()
            && !cfg.min_client_version.is_empty()
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
            // 已归属当前用户的设备重连不占用新名额(避免达到上限后重连被误拒)
            let owned = self.devices.count_by_owner_excluding(user, id);
            if owned >= cfg.max_devices_per_user {
                return Err(format!(
                    "设备数已达上限({}台),请先在后台删除不再使用的设备",
                    cfg.max_devices_per_user
                ));
            }
        }
        Ok(())
    }

    /// 是否启用账号认证(持有 JWT 密钥)。
    pub fn auth_enabled(&self) -> bool {
        !self.auth_secret.is_empty()
    }

    /// 校验登录令牌:有效返回令牌中的用户名,否则返回 None。
    /// 未启用认证或令牌为空时也返回 None(交由调用方按开放模式处理)。
    pub fn authenticate(&self, token: &str) -> Option<String> {
        if self.auth_secret.is_empty() || token.is_empty() {
            return None;
        }
        auth::verify_token(&self.auth_secret, token).ok()
    }

    /// 查询对端地址是否被允许:`token` 为请求方登录令牌。启用认证时仅允许
    /// 查询本账号设备与未归属设备(防止知道设备 ID 即查他账号地址);
    /// 未启用认证(开放模式)一律允许。
    pub fn lookup_allowed(&self, id: &str, token: &str) -> bool {
        if !self.auth_enabled() {
            return true;
        }
        let me = if token.is_empty() {
            None
        } else {
            self.authenticate(token)
        };
        match self.devices.get(id) {
            Some(dev) => dev.owner.is_empty() || me.as_deref() == Some(dev.owner.as_str()),
            // 未建档设备:仅登录用户可查(从未注册过的 id 本就查不到地址,不泄露)
            None => me.is_some(),
        }
    }

    /// 注册对端(同 id 重复注册视为更新地址,last one wins),返回本次注册代次。
    pub fn register(&self, id: &str, lan: &str, external: &str) -> u64 {
        let mut map = self.peers.lock().unwrap_or_else(|e| e.into_inner());
        let generation = self.next_gen.fetch_add(1, Ordering::Relaxed);
        map.insert(
            id.to_string(),
            PeerRecord {
                lan: lan.to_string(),
                external: external.to_string(),
                last_seen: Instant::now(),
                generation,
            },
        );
        generation
    }

    /// 该 id 的在线记录是否仍持有指定代次(本连接是否仍持有该记录所有权)。
    pub fn owns_generation(&self, id: &str, generation: u64) -> bool {
        self.peers
            .lock()
            .map(|m| {
                m.get(id)
                    .map(|r| r.generation == generation)
                    .unwrap_or(false)
            })
            .unwrap_or(false)
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

    /// 仅当记录仍持有指定代次时原子续期。
    pub fn heartbeat_if_owner(&self, id: &str, generation: u64) -> Result<(), String> {
        let mut map = self.peers.lock().map_err(|e| e.to_string())?;
        match map.get_mut(id) {
            Some(rec) if rec.generation == generation => {
                rec.last_seen = Instant::now();
                Ok(())
            }
            Some(_) => Err(format!("id 已被其他连接接管: {id}")),
            None => Err(format!("未注册的 id: {id}")),
        }
    }

    /// 注销对端。
    pub fn unregister(&self, id: &str) {
        if let Ok(mut map) = self.peers.lock() {
            map.remove(id);
        }
    }

    /// 仅当记录仍持有指定代次时原子注销,返回是否实际移除。
    pub fn unregister_if_owner(&self, id: &str, generation: u64) -> bool {
        let Ok(mut map) = self.peers.lock() else {
            return false;
        };
        if matches!(map.get(id), Some(rec) if rec.generation == generation) {
            map.remove(id);
            true
        } else {
            false
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
        self.list_online_with_stats().0
    }

    /// 在线对端列表及统计(管理与诊断用途)。
    pub fn list_online_with_stats(&self) -> (Vec<PeerEntry>, OnlineListStats) {
        self.list_online_filtered(|_| true)
    }

    /// 在线对端列表(按归属账号过滤):`user` 非空时仅返回该账号设备,
    /// 空串(未登录)时仅返回未归属设备,避免跨账号地址泄露。
    pub fn list_online_for(&self, user: &str) -> Vec<PeerEntry> {
        self.list_online_for_with_stats(user).0
    }

    /// 按归属账号过滤在线设备并返回统计,用于排查同账号发现问题。
    pub fn list_online_for_with_stats(&self, user: &str) -> (Vec<PeerEntry>, OnlineListStats) {
        self.list_online_filtered(|owner| owner == user)
    }

    /// 在线对端列表(通用过滤):先剔除超时条目,再按 `keep` 过滤归属账号。
    fn list_online_filtered(
        &self,
        keep: impl Fn(&str) -> bool,
    ) -> (Vec<PeerEntry>, OnlineListStats) {
        let mut map = self.peers.lock().unwrap_or_else(|e| e.into_inner());
        map.retain(|_, rec| rec.last_seen.elapsed() <= ONLINE_TIMEOUT);
        let online_total = map.len();
        let peers: Vec<PeerEntry> = map
            .iter()
            .filter_map(|(id, rec)| {
                // 名称/归属取自设备档案(注册时上报);档案缺失时以 id 兜底
                let dev = self.devices.get(id);
                let owner = dev.as_ref().map(|d| d.owner.clone()).unwrap_or_default();
                if !keep(&owner) {
                    return None;
                }
                Some(PeerEntry {
                    id: id.clone(),
                    name: dev
                        .as_ref()
                        .map(|d| d.name.clone())
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| id.clone()),
                    owner,
                    lan: rec.lan.clone(),
                    external: rec.external.clone(),
                })
            })
            .collect();
        let stats = OnlineListStats {
            online_total,
            returned: peers.len(),
        };
        (peers, stats)
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
    let addr = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_default();
    // 本连接登记的对端(id, 注册代次);代次用于断开时判定记录所有权
    let mut conn: Option<(String, u64)> = None;
    let disconnect_reason = loop {
        let msg: SignalMsg = match tokio::time::timeout(IDLE_TIMEOUT, read_msg(&mut stream)).await {
            Ok(Ok(m)) => m,
            Ok(Err(e)) => {
                log::debug!("[signal] 连接读消息结束({addr}): {e}");
                break format!("读取消息失败: {e}");
            }
            Err(_) => {
                log::debug!("[signal] 连接空闲超时({addr}),断开");
                break format!("连接空闲超时({}秒)", IDLE_TIMEOUT.as_secs());
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
                token,
            } => {
                // 认证开启时以令牌解析出的用户名为准(忽略客户端自报的 user);
                // 令牌无效直接拒绝。开放模式没有 JWT 密钥,此时信任 user 字段,
                // 即使客户端仍携带登录令牌也不能被错误降级为匿名设备。
                let (effective_user, auth_source) = if !core.auth_enabled() {
                    (user.clone(), "开放模式")
                } else if !token.is_empty() {
                    match core.authenticate(&token) {
                        Some(u) => (u, "令牌有效"),
                        None => {
                            let msg = "登录令牌无效或已过期,请重新登录".to_string();
                            log::warn!("[signal] 注册被拒(令牌无效): id={id}");
                            op_log(
                                "signal",
                                "register_rejected",
                                &format!("id={id}, reason=token_invalid"),
                            );
                            let _ = write_msg(
                                &mut stream,
                                &SignalMsg::RegisterAck {
                                    ok: false,
                                    msg,
                                    auth_error: true,
                                },
                            )
                            .await;
                            break "注册被拒: 登录令牌无效".to_string();
                        }
                    }
                } else {
                    // 服务端启用了账号认证:无令牌一律视为未登录,不得冒用他人账号
                    (String::new(), "未携带令牌")
                };
                let previous_owner = core
                    .devices
                    .get(&id)
                    .map(|dev| dev.owner)
                    .unwrap_or_default();
                // 归属冲突检查仅在启用认证时执行:此时 effective_user 来自受
                // 校验的令牌,可防止他账号强占设备。开放模式(无认证)信任客户端
                // 上报的 user,允许同设备归属随登录账号切换(否则换号后设备被锁死)。
                if core.auth_enabled() {
                    if !previous_owner.is_empty() && previous_owner != effective_user {
                        let msg =
                            format!("设备已归属账号 {}(请使用该账号登录后注册)", previous_owner);
                        log::warn!("[signal] 注册被拒(归属冲突): id={id}, 归属={previous_owner}, 请求={effective_user}");
                        op_log("signal", "register_rejected", &format!("id={id}, reason=owner_conflict, owner={previous_owner}, requested_owner={effective_user}"));
                        let _ = write_msg(
                            &mut stream,
                            &SignalMsg::RegisterAck {
                                ok: false,
                                msg,
                                auth_error: false,
                            },
                        )
                        .await;
                        break "注册被拒: 设备归属冲突".to_string();
                    }
                }
                // 策略校验:维护模式 / 版本下限 / 设备禁用 / 设备数上限
                if let Err(e) = core.check_register_policy(&effective_user, &version, &id) {
                    log::warn!("[signal] 注册被拒: id={id}, 原因={e}");
                    op_log(
                        "signal",
                        "register_rejected",
                        &format!("id={id}, reason={e}"),
                    );
                    let reason = e.clone();
                    let _ = write_msg(
                        &mut stream,
                        &SignalMsg::RegisterAck {
                            ok: false,
                            msg: e,
                            auth_error: false,
                        },
                    )
                    .await;
                    break format!("注册被拒: {reason}");
                }
                let duplicated = core
                    .peers
                    .lock()
                    .map(|m| m.contains_key(&id))
                    .unwrap_or(false);
                let generation = core.register(&id, &lan, &addr);
                core.devices.touch(
                    &id,
                    &effective_user,
                    &name,
                    &os,
                    &version,
                    &lan,
                    &addr,
                    true,
                );
                conn = Some((id.clone(), generation));
                log::info!(
                    "[signal] 注册: id={id}, lan={lan}, external={addr}, user={effective_user}, previous_owner={previous_owner}, auth={auth_source}, os={os}, v={version}, duplicate={duplicated}, generation={generation}"
                );
                op_log(
                    "signal",
                    "register_accepted",
                    &format!(
                        "id={id}, owner={effective_user}, previous_owner={previous_owner}, auth={auth_source}, duplicate={duplicated}, generation={generation}, lan={lan}, external={addr}"
                    ),
                );
                let _ = write_msg(
                    &mut stream,
                    &SignalMsg::RegisterAck {
                        ok: true,
                        msg: if duplicated {
                            "id 已存在,已更新地址".into()
                        } else {
                            "ok".into()
                        },
                        auth_error: false,
                    },
                )
                .await;
            }
            SignalMsg::Heartbeat { id } => {
                // 所有权校验:仅允许本连接注册(且仍持有代次)的记录续期,
                // 防止任意连接刷新其他设备的在线时间造成虚假在线。
                let ack = match conn.as_ref() {
                    Some((cid, generation)) if cid == &id => {
                        match core.heartbeat_if_owner(&id, *generation) {
                            Ok(()) => {
                                core.devices.set_online(&id, true);
                                SignalMsg::RegisterAck {
                                    ok: true,
                                    msg: "ok".into(),
                                    auth_error: false,
                                }
                            }
                            Err(e) => SignalMsg::RegisterAck {
                                ok: false,
                                msg: e,
                                auth_error: false,
                            },
                        }
                    }
                    _ => {
                        log::warn!("[signal] 心跳被拒(非本连接注册): id={id}");
                        op_log(
                            "signal",
                            "heartbeat_rejected",
                            &format!("id={id}, reason=未注册或已被其他连接接管, source={addr}"),
                        );
                        SignalMsg::RegisterAck {
                            ok: false,
                            msg: "未注册或已被其他连接接管".into(),
                            auth_error: false,
                        }
                    }
                };
                let _ = write_msg(&mut stream, &ack).await;
            }
            SignalMsg::Lookup { id, token } => {
                // 鉴权(启用认证时):仅返回本账号设备或未归属设备的地址;
                // 令牌无效/未登录按匿名处理,杜绝跨账号地址泄露。
                let found = if core.lookup_allowed(&id, &token) {
                    core.lookup(&id)
                } else {
                    None
                };
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
            SignalMsg::List { user, token } => {
                // 认证:令牌有效时以令牌用户名为准;令牌缺失/无效一律按未登录处理
                // (仅返回未归属设备),杜绝伪造账号名查询他人设备。
                // 令牌无效且服务端启用了认证时置 auth_error,客户端据此提示重新登录
                // (否则令牌过期后设备列表静默为空,用户无从知晓需重新登录)。
                let requested_user = user.clone();
                let mut auth_error = false;
                let (effective_user, auth_source) = if !core.auth_enabled() {
                    // 自托管开放模式没有 JWT 密钥,兼容旧客户端及带有本地
                    // 登录令牌的客户端,按 user 字段进行设备归属过滤。
                    (user, "开放模式")
                } else if !token.is_empty() {
                    match core.authenticate(&token) {
                        Some(u) => (u, "令牌有效"),
                        None => {
                            log::warn!("[signal] list 令牌无效,按未登录处理: {addr}");
                            auth_error = core.auth_enabled();
                            (String::new(), "令牌无效")
                        }
                    }
                } else if core.auth_enabled() {
                    (String::new(), "未携带令牌")
                } else {
                    (user, "开放模式")
                };
                let (peers, stats) = core.list_online_for_with_stats(&effective_user);
                let peer_ids = peers
                    .iter()
                    .map(|peer| format!("{}@{}", peer.id, peer.owner))
                    .collect::<Vec<_>>()
                    .join("|");
                log::info!(
                    "[signal] 设备发现: source={addr}, requested_user={requested_user}, effective_user={effective_user}, auth={auth_source}, online_total={}, returned={}, peers=[{peer_ids}]",
                    stats.online_total,
                    stats.returned,
                );
                op_log(
                    "signal",
                    "discovery",
                    &format!(
                        "source={addr}, requested_user={requested_user}, effective_user={effective_user}, auth={auth_source}, online_total={}, returned={}, peers=[{peer_ids}]",
                        stats.online_total,
                        stats.returned,
                    ),
                );
                if auth_error {
                    op_log(
                        "signal",
                        "discovery_auth_failed",
                        &format!(
                            "source={addr}, requested_user={requested_user}, online_total={}",
                            stats.online_total
                        ),
                    );
                }
                let _ = write_msg(&mut stream, &SignalMsg::ListAck { peers, auth_error }).await;
            }
            SignalMsg::Unregister { id } => {
                // 仅注销本连接登记且仍持有代次(所有权)的记录
                let removed = match conn.as_ref() {
                    Some((cid, generation)) if cid == &id => {
                        core.unregister_if_owner(&id, *generation)
                    }
                    _ => false,
                };
                if removed {
                    conn = None;
                }
                log::info!(
                    "[signal] 注销: id={id},{}",
                    if removed {
                        "已注销"
                    } else {
                        "非本连接持有,忽略"
                    }
                );
                if removed {
                    op_log(
                        "signal",
                        "unregister",
                        &format!("id={id}, source={addr}, reason=客户端请求"),
                    );
                }
                let _ = write_msg(
                    &mut stream,
                    &SignalMsg::RegisterAck {
                        ok: true,
                        msg: "ok".into(),
                        auth_error: false,
                    },
                )
                .await;
            }
            _ => {}
        }
    };
    // 连接断开:仅当记录仍由本连接持有(代次一致)时才注销,
    // 避免旧连接断开误删已被新连接接管的同 id 记录(重连竞态)。
    if let Some((id, generation)) = conn.take() {
        if core.unregister_if_owner(&id, generation) {
            core.devices.set_online(&id, false);
            log::info!("[signal] 连接断开({addr}),注销 id={id}, reason={disconnect_reason}");
            op_log(
                "signal",
                "unregister_disconnect",
                &format!(
                    "id={id}, source={addr}, generation={generation}, reason={disconnect_reason}"
                ),
            );
        } else {
            log::info!("[signal] 连接断开({addr}),id={id} 已被新连接接管,跳过注销");
        }
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
            prune_core.sessions.prune(Duration::from_secs(idle));
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

    #[test]
    fn list_online_filters_by_owner() {
        let core = SignalCore::new("");
        core.register("a", "10.0.0.1:1", "1.1.1.1:1");
        core.register("b", "10.0.0.2:1", "2.2.2.2:1");
        core.register("c", "10.0.0.3:1", "3.3.3.3:1");
        core.devices.touch(
            "a",
            "alice",
            "A",
            "Windows",
            "0.1.0",
            "10.0.0.1:1",
            "1.1.1.1:1",
            true,
        );
        core.devices.touch(
            "b",
            "bob",
            "B",
            "Windows",
            "0.1.0",
            "10.0.0.2:1",
            "2.2.2.2:1",
            true,
        );
        // c 未建档 → owner 为空(未归属设备)

        // 登录用户只见自己账号设备
        let alice = core.list_online_for("alice");
        assert_eq!(alice.len(), 1);
        assert_eq!(alice[0].id, "a");
        let bob = core.list_online_for("bob");
        assert_eq!(bob.len(), 1);
        assert_eq!(bob[0].id, "b");
        // 未登录只可见未归属设备
        let anon = core.list_online_for("");
        assert_eq!(anon.len(), 1);
        assert_eq!(anon[0].id, "c");
        // 管理后台视角:全部设备
        assert_eq!(core.list_online().len(), 3);
    }

    #[test]
    fn register_generation_ownership() {
        let core = SignalCore::new("");
        let g1 = core.register("x", "10.0.0.1:1", "1.1.1.1:1");
        assert!(core.owns_generation("x", g1));
        assert!(!core.owns_generation("x", g1 + 999), "未知代次不持有");
        // 同 id 再次注册:代次递增,旧代次失去所有权(旧连接断开不再误删)
        let g2 = core.register("x", "10.0.0.2:2", "1.1.1.1:2");
        assert_ne!(g1, g2, "每次注册代次应递增");
        assert!(core.owns_generation("x", g2));
        assert!(!core.owns_generation("x", g1), "旧代次应失去所有权");
    }

    #[test]
    fn generation_owned_mutations_are_atomic() {
        let core = SignalCore::new("");
        let old_generation = core.register("x", "10.0.0.1:1", "1.1.1.1:1");
        let new_generation = core.register("x", "10.0.0.2:2", "1.1.1.1:2");

        assert!(core.heartbeat_if_owner("x", old_generation).is_err());
        assert_eq!(core.lookup("x").unwrap().0, "10.0.0.2:2");
        assert!(!core.unregister_if_owner("x", old_generation));
        assert!(core.lookup("x").is_some());
        assert!(core.heartbeat_if_owner("x", new_generation).is_ok());
        assert!(core.unregister_if_owner("x", new_generation));
        assert!(core.lookup("x").is_none());
    }

    #[test]
    fn device_limit_excludes_current_device() {
        // 隔离存储,避免与其它测试共享默认目录造成数据串扰
        let dir =
            std::env::temp_dir().join(format!("dcr-signal-limit-test-{}", std::process::id()));
        let cfg = Arc::new(RwLock::new(ConfigStore::new(
            &dir,
            crate::config::ServerConfig {
                max_devices_per_user: 1,
                ..Default::default()
            },
        )));
        let core = SignalCore::with_stores(
            "",
            Arc::new(DeviceStore::new(&dir)),
            Arc::new(crate::sessions::SessionCore::new()),
            cfg,
            Vec::new(),
        );

        // 首个设备:允许
        assert!(core.check_register_policy("alice", "0.1.0", "pc-a").is_ok());
        core.devices
            .touch("pc-a", "alice", "A", "Windows", "0.1.0", "l", "e", true);

        // 新设备 pc-b:已达上限,拒绝
        assert!(core
            .check_register_policy("alice", "0.1.0", "pc-b")
            .is_err());
        // 已登记设备 pc-a 重连:不占新名额,允许
        assert!(core.check_register_policy("alice", "0.1.0", "pc-a").is_ok());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn auth_token_resolves_username() {
        use crate::auth::AuthState;
        let secret = b"test-secret-0123456789abcdef012345".to_vec();
        let dir = std::env::temp_dir().join(format!("dcr-signal-auth-test-{}", std::process::id()));
        let store = std::sync::Arc::new(crate::auth::UserStore::new(&dir));
        let auth = AuthState::new(store, secret.clone());
        auth.store.create_user("alice", "pass123").unwrap();
        let token = auth.login("alice", "pass123").unwrap();

        let core = SignalCore::new("");
        // 未启用认证(空密钥):无法校验,返回 None
        assert!(core.authenticate(&token).is_none());
        let core2 = SignalCore::with_stores(
            "",
            std::sync::Arc::new(DeviceStore::new(&std::env::temp_dir())),
            std::sync::Arc::new(crate::sessions::SessionCore::new()),
            std::sync::Arc::new(RwLock::new(ConfigStore::new(
                &std::env::temp_dir(),
                crate::config::ServerConfig::default(),
            ))),
            secret,
        );
        assert_eq!(core2.authenticate(&token).as_deref(), Some("alice"));
        assert!(core2.authenticate("forged").is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lookup_allowed_respects_account_ownership() {
        use crate::auth::AuthState;
        use crate::auth::UserStore;
        let secret = b"test-secret-lookup-0123456789abc".to_vec();
        let dir =
            std::env::temp_dir().join(format!("dcr-signal-lookup-test-{}", std::process::id()));
        let auth = AuthState::new(std::sync::Arc::new(UserStore::new(&dir)), secret.clone());
        auth.store.create_user("alice", "pass123").unwrap();
        auth.store.create_user("bob", "pass123").unwrap();
        let alice_token = auth.login("alice", "pass123").unwrap();
        let bob_token = auth.login("bob", "pass123").unwrap();

        let core = SignalCore::with_stores(
            "",
            std::sync::Arc::new(DeviceStore::new(&dir)),
            std::sync::Arc::new(crate::sessions::SessionCore::new()),
            std::sync::Arc::new(RwLock::new(ConfigStore::new(
                &dir,
                crate::config::ServerConfig::default(),
            ))),
            secret,
        );
        // pc-a 归属 alice;pc-free 未归属
        core.devices
            .touch("pc-a", "alice", "A", "Windows", "0.1.0", "l", "e", true);
        core.devices
            .touch("pc-free", "", "F", "Windows", "0.1.0", "l", "e", true);

        // 归属账号本人可查;其他账号/匿名不可查他人设备
        assert!(core.lookup_allowed("pc-a", &alice_token));
        assert!(
            !core.lookup_allowed("pc-a", &bob_token),
            "他账号不可查 alice 的设备"
        );
        assert!(!core.lookup_allowed("pc-a", ""), "匿名不可查已归属设备");
        assert!(!core.lookup_allowed("pc-a", "forged"), "无效令牌按匿名处理");
        // 未归属设备:登录用户与匿名均可查(与列表语义一致)
        assert!(core.lookup_allowed("pc-free", &bob_token));
        assert!(core.lookup_allowed("pc-free", ""));
        std::fs::remove_dir_all(&dir).ok();
    }
}
