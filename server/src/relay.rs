//! TURN-like 中继服务核心(dcr-relay)。
//!
//! 基于 RustDesk hbbr 思路的中继服务,在直连(打洞)失败时转发流量:
//! - TCP(默认 21117):对端通过 `allocate {id, role}` 申请通道;host 先连、
//!   client 后连(或反过来),配对成功后两个 TCP 连接做**双向字节透明转发**
//!   (上层 framing 由客户端负责,原样透传);
//! - UDP(默认 21119):`alloc-udp {id}` 登记宿主端点,`data {id, payload}` 把
//!   base64 载荷转发给宿主(对端),宿主也可向对端 id 发数据。payload 约定为
//!   **完整 UDP 分片帧**(客户端 16 字节分片头 + Annex-B 切片)整体 base64,
//!   中继原样解码转发——宿主收到的字节与直连 UDP 完全一致(见 message::RelayUdpMsg)。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine as _;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::watch;

use crate::framing::{read_msg, write_msg};
use crate::message::{RelayMsg, RelayUdpMsg};
use crate::operation_log::op_log;

/// client 等待 host 接入的最长时间。
const HOST_WAIT_TIMEOUT: Duration = Duration::from_secs(30);
/// 连接分配请求超时。
const ALLOCATE_TIMEOUT: Duration = Duration::from_secs(10);
/// UDP 数据报缓冲上限。
const MAX_UDP_DATAGRAM: usize = 64 * 1024;

/// host 侧拆开的读写半段(持有即保持连接存活)。
pub struct HostParts {
    pub read: tokio::net::tcp::OwnedReadHalf,
    pub write: tokio::net::tcp::OwnedWriteHalf,
}

/// 一个中继槽位:存放某 id 的宿主连接与宿主地址,并用 watch 通知等待中的 client。
#[derive(Clone)]
struct Slot {
    inner: Arc<Mutex<Option<HostParts>>>,
    host_addr: Arc<Mutex<String>>,
    ready: watch::Sender<bool>,
}

/// 中继管理器(跨连接共享)。
#[derive(Clone, Default)]
pub struct RelayManager {
    slots: Arc<Mutex<HashMap<String, Slot>>>,
    /// 会话上报目标(dcr-signal 的 UDP 地址);None 表示不上报。
    report_to: Arc<Option<SocketAddr>>,
}

impl RelayManager {
    /// 创建管理器并指定会话上报目标地址(dcr-signal 的 UDP/STUN 端口)。
    pub fn with_report(report_to: Option<SocketAddr>) -> Self {
        Self {
            slots: Arc::new(Mutex::new(HashMap::new())),
            report_to: Arc::new(report_to),
        }
    }

    /// 经 UDP 上报会话事件到 dcr-signal(尽力而为,失败仅记日志)。
    async fn report(&self, payload: String) {
        let Some(target) = self.report_to.as_ref() else {
            return;
        };
        let Ok(sock) = UdpSocket::bind("0.0.0.0:0").await else {
            return;
        };
        if let Err(e) = sock.send_to(payload.as_bytes(), *target).await {
            log::warn!("[relay] 会话上报失败: {e}");
        }
    }

    /// 上报会话开始。
    pub async fn report_session_start(&self, id: &str, host: &str, client: &str) {
        let payload = serde_json::json!({
            "t": "session-start",
            "id": id,
            "host": host,
            "client": client,
        })
        .to_string();
        self.report(payload).await;
    }

    /// 上报会话结束。
    pub async fn report_session_end(&self, id: &str) {
        let payload = serde_json::json!({ "t": "session-end", "id": id }).to_string();
        self.report(payload).await;
    }

    fn slot(&self, id: &str) -> Slot {
        let mut map = self.slots.lock().unwrap_or_else(|e| e.into_inner());
        map.entry(id.to_string())
            .or_insert_with(|| {
                let (tx, _rx) = watch::channel(false);
                Slot {
                    inner: Arc::new(Mutex::new(None)),
                    host_addr: Arc::new(Mutex::new(String::new())),
                    ready: tx,
                }
            })
            .clone()
    }

    /// host 注册:把读写半段与宿主地址放入槽位并通知等待者;替换旧 host(旧连接随之失效)。
    fn host_register(&self, id: &str, addr: &str, parts: HostParts) {
        let slot = self.slot(id);
        {
            let mut inner = slot.inner.lock().unwrap_or_else(|e| e.into_inner());
            *inner = Some(parts);
        }
        *slot.host_addr.lock().unwrap_or_else(|e| e.into_inner()) = addr.to_string();
        let _ = slot.ready.send(true);
    }

    /// client 取走宿主连接;未就绪返回 None。返回宿主连接与其地址。
    fn take_host(&self, id: &str) -> Option<(HostParts, String)> {
        let slot = self.slot(id);
        let parts = slot.inner.lock().unwrap_or_else(|e| e.into_inner()).take();
        parts.map(|p| {
            let addr = slot
                .host_addr
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            (p, addr)
        })
    }

    /// client 侧等待宿主接入(最多 `HOST_WAIT_TIMEOUT`),返回取到的宿主连接与宿主地址。
    async fn wait_host(&self, id: &str) -> Option<(HostParts, String)> {
        let slot = self.slot(id);
        let mut rx = slot.ready.subscribe();
        let deadline = tokio::time::Instant::now() + HOST_WAIT_TIMEOUT;
        loop {
            if let Some(parts) = slot.inner.lock().ok().and_then(|mut g| g.take()) {
                let addr = slot
                    .host_addr
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                return Some((parts, addr));
            }
            if *rx.borrow() {
                continue;
            }
            match tokio::time::timeout_at(deadline, rx.changed()).await {
                Ok(Ok(_)) => continue,
                _ => return None,
            }
        }
    }
}

/// 把两段连接做双向字节透明转发;任一端 EOF 时对另一端的写半部发 FIN(半关闭
/// 传播),使对端读到 EOF 正常收尾,而不是等对方强制断开。
async fn pipe(
    host: HostParts,
    client_write: tokio::net::tcp::OwnedWriteHalf,
    client_read: tokio::net::tcp::OwnedReadHalf,
) {
    let (mut hr, mut hw) = (host.read, host.write);
    // host.read → client.write 与 client.read → host.write 两条单向拷贝并行执行;
    // 任一侧读 EOF 即对另一侧写半部 shutdown(半关闭传播),避免全连接强关
    let a = async {
        let mut cw = client_write;
        let _ = tokio::io::copy(&mut hr, &mut cw).await;
        let _ = cw.shutdown().await;
    };
    let b = async {
        let mut cr = client_read;
        let _ = tokio::io::copy(&mut cr, &mut hw).await;
        let _ = hw.shutdown().await;
    };
    tokio::join!(a, b);
}

/// 处理单个中继 TCP 连接:首条消息必须是 `allocate`。
pub async fn handle_relay_conn(manager: RelayManager, mut stream: TcpStream, addr: SocketAddr) {
    let msg: RelayMsg = match tokio::time::timeout(ALLOCATE_TIMEOUT, read_msg(&mut stream)).await {
        Ok(Ok(m)) => m,
        Ok(Err(e)) => {
            log::warn!("[relay] 首条消息读取失败({addr}): {e}");
            return;
        }
        Err(_) => {
            log::warn!("[relay] 连接({addr})在 {ALLOCATE_TIMEOUT:?} 内未发送 allocate,关闭");
            return;
        }
    };
    let (id, role) = match msg {
        RelayMsg::Allocate { id, role } => (id, role),
        _ => {
            log::warn!("[relay] 首条消息不是 allocate({addr})");
            return;
        }
    };
    log::info!("[relay] allocate: id={id}, role={role}, from={addr}");
    op_log(
        "relay",
        "allocate",
        &format!("id={id}, role={role}, from={addr}"),
    );

    if role == "host" {
        // 先回 ack,再把读写半段存入管理器(连接保持存活)
        let _ = write_msg(
            &mut stream,
            &RelayMsg::Allocated {
                id: id.clone(),
                peer_connected: false,
            },
        )
        .await;
        let host_addr = addr.to_string();
        let (read, write) = stream.into_split();
        manager.host_register(&id, &host_addr, HostParts { read, write });
        log::info!("[relay] host {id} 已登记,等待 client 接入");
        return;
    }

    // client:取宿主连接或等待
    let (host_parts, host_addr) = match manager.take_host(&id) {
        Some(p) => p,
        None => match manager.wait_host(&id).await {
            Some(p) => p,
            None => {
                let _ = write_msg(
                    &mut stream,
                    &RelayMsg::Allocated {
                        id: id.clone(),
                        peer_connected: false,
                    },
                )
                .await;
                log::info!("[relay] client {id} 等待 host 超时,关闭");
                return;
            }
        },
    };
    let _ = write_msg(
        &mut stream,
        &RelayMsg::Allocated {
            id: id.clone(),
            peer_connected: true,
        },
    )
    .await;
    let (cr, cw) = stream.into_split();
    let client_addr = addr.to_string();
    log::info!("[relay] 配对成功: id={id}");
    op_log(
        "relay",
        "paired",
        &format!("id={id}, host={host_addr}, client={client_addr}"),
    );
    // 上报会话开始给信令(监控用)
    manager
        .report_session_start(&id, &host_addr, &client_addr)
        .await;
    pipe(host_parts, cw, cr).await;
    // 会话结束
    manager.report_session_end(&id).await;
    log::info!("[relay] 会话结束: id={id}");
    op_log("relay", "session_end", &format!("id={id}"));
}

/// UDP 中继消息类型(统一定义于 [`crate::message::RelayUdpMsg`],此处重导出便于使用)。
pub use crate::message::RelayUdpMsg as UdpRelayMsg;

/// 把一个 UDP 分片帧(裸二进制字节)封装为 `data` 数据报(供客户端/测试使用):
/// `{"t":"data","id":...,"payload":"<base64>"}`。中继解码后原样转发,宿主收到
/// 的字节与本函数输入完全一致——与直连 UDP 路径同构。
pub fn encode_udp_data_datagram(id: &str, frame_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let payload = base64::engine::general_purpose::STANDARD
        .encode(frame_bytes)
        .to_string();
    serde_json::to_vec(&RelayUdpMsg::Data {
        id: id.to_string(),
        payload,
    })
    .map_err(|e| format!("data 数据报序列化失败: {e}"))
}

/// 处理一个 UDP 数据报:登记宿主端点 / 转发载荷。
pub async fn handle_udp_packet(
    endpoints: &Arc<Mutex<HashMap<String, SocketAddr>>>,
    sock: &UdpSocket,
    buf: Vec<u8>,
    src: SocketAddr,
) {
    let msg: RelayUdpMsg = match serde_json::from_slice(&buf) {
        Ok(m) => m,
        Err(_) => return,
    };
    match msg {
        RelayUdpMsg::AllocUdp { id } => {
            endpoints
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(id.clone(), src);
            log::info!("[relay-udp] 宿主 {id} 登记于 {src}");
            op_log("relay", "udp_alloc", &format!("id={id}, src={src}"));
            let _ = sock
                .send_to(format!("{{\"t\":\"allocated\"}}").as_bytes(), src)
                .await;
        }
        RelayUdpMsg::Data { id, payload } => {
            let target = {
                let map = endpoints.lock().unwrap_or_else(|e| e.into_inner());
                map.get(&id).cloned()
            };
            let Some(target) = target else {
                log::debug!("[relay-udp] 未知宿主 {id},丢弃");
                return;
            };
            match base64::engine::general_purpose::STANDARD.decode(&payload) {
                Ok(bytes) => {
                    let _ = sock.send_to(&bytes, target).await;
                }
                Err(e) => log::warn!("[relay-udp] base64 解码失败: {e}"),
            }
        }
    }
}

/// 启动完整中继服务(TCP 字节转发 + UDP 数据报转发)。
/// `report_to` 为 dcr-signal 的 UDP 地址(会话监控上报目标),None 表示不上报。
pub async fn serve_tcp(listener: TcpListener, report_to: Option<SocketAddr>) -> Result<(), String> {
    let manager = RelayManager::with_report(report_to);
    loop {
        let (stream, addr) = listener
            .accept()
            .await
            .map_err(|e| format!("accept 失败: {e}"))?;
        log::info!("[relay] 新连接: {addr}");
        let manager = manager.clone();
        tokio::spawn(async move {
            handle_relay_conn(manager, stream, addr).await;
        });
    }
}

/// 启动 UDP 中继循环。
pub async fn serve_udp(socket: UdpSocket) -> Result<(), String> {
    let endpoints: Arc<Mutex<HashMap<String, SocketAddr>>> = Arc::new(Mutex::new(HashMap::new()));
    let socket = Arc::new(socket);
    let mut buf = vec![0u8; MAX_UDP_DATAGRAM];
    loop {
        let (n, src) = match socket.recv_from(&mut buf).await {
            Ok(v) => v,
            Err(e) => {
                log::error!("[relay-udp] recv 失败: {e}");
                continue;
            }
        };
        let endpoints = endpoints.clone();
        let sock = socket.clone();
        let data = buf[..n].to_vec();
        tokio::spawn(async move {
            handle_udp_packet(&endpoints, &sock, data, src).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_register_take() {
        let m = RelayManager::default();
        // 用真实 TCP 拆分出读写半段
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let a = TcpStream::connect(listener.local_addr().unwrap())
                .await
                .unwrap();
            let (s, _) = listener.accept().await.unwrap();
            let (read, write) = s.into_split();
            m.host_register("x", "10.0.0.1:21118", HostParts { read, write });
            // 确认 take 可取回(含宿主地址)
            let (parts, addr) = m.take_host("x").unwrap();
            assert_eq!(addr, "10.0.0.1:21118");
            drop(parts);
            assert!(m.take_host("x").is_none(), "第二次应取不到");
            drop(a);
        });
    }
}
