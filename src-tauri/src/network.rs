//! 真实 LAN 远程控制协议(TCP,长度前缀 JSON 帧)。
//!
//! 帧格式:每消息 = 4 字节小端长度 + JSON 字节(serde_json)。
//! 消息统一以 `t` 字段区分类型(内部协议,字段 snake_case,不暴露给前端)。
//!
//! - `run_host`:被控端监听,单连接(新连接踢掉旧连接),握手后两路任务:
//!   收消息循环(鼠标/键盘注入、剪贴板写入、ping→pong)+ 发帧循环(复用
//!   `capture::latest_frame()` 的最新 JPEG,base64 后推送 frame)。
//! - `connect_peer`:控制端连接对端,握手后收帧/剪贴板,断线自动清理并广播。
//! 会话管理通过 `static SESSION` 保存对端信息与发送通道。

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

// base64 的 encode/decode 是 Engine trait 方法,需将 trait 引入作用域
use base64::Engine as _;

/// 协议常量(与对端握手校验)。
const APP_NAME: &str = "desktop-cr";
const PROTOCOL_VERSION: u32 = 1;
/// 单帧 JSON 消息上限(16MB,避免畸形长度导致内存暴涨)。
const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
/// 连接超时 / 握手超时。
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);
const HANDSHAKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// 协议消息(以 `t` 字段区分类型;变体名转 kebab-case,如 HelloAck → "hello-ack")。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "kebab-case")]
pub enum Msg {
    /// 握手:发送方标识
    Hello {
        id: String,
        app: String,
        ver: u32,
    },
    /// 握手响应:被控端标识
    HelloAck {
        id: String,
    },
    /// 视频帧(jpeg 为 base64 编码)
    Frame {
        w: u32,
        h: u32,
        seq: u64,
        jpeg: String,
    },
    /// 鼠标事件(x/y 为 0..1 归一化坐标;kind: move|down|up|wheel;button: left|right|middle;delta: 滚轮增量)
    Mouse {
        x: f64,
        y: f64,
        kind: String,
        button: Option<String>,
        delta: f64,
    },
    /// 键盘事件(key: 按键文本;code: DOM KeyboardEvent.code;mods: 修饰键列表)
    Key {
        key: String,
        kind: String,
        code: Option<String>,
        mods: Vec<String>,
    },
    /// 剪贴板文本
    Clipboard {
        text: String,
    },
    /// 流参数调整(控制端 → 被控端,被控端抓帧循环实时应用)
    Stream {
        fps: u32,
        jpeg_quality: u8,
        width: u32,
        height: u32,
    },
    /// 心跳(毫秒时间戳)
    Ping {
        ts: u64,
    },
    /// 心跳响应
    Pong {
        ts: u64,
    },
}

/// 发给对端的消息 = 协议 Msg 的发送子集(序列化时同样以 `t` 字段区分)。
pub type OutMsg = Msg;

/// 推送给控制端前端的远程帧事件负载(camelCase)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteFrameEvent {
    pub width: u32,
    pub height: u32,
    /// 已解码的 JPEG 字节
    pub jpeg: Vec<u8>,
}

/// 活跃会话信息。
struct SessionInner {
    peer_id: String,
    peer_addr: String,
    /// 发给对端的消息通道
    tx: mpsc::Sender<Msg>,
}

static SESSION: Mutex<Option<SessionInner>> = Mutex::new(None);

fn session_guard() -> std::sync::MutexGuard<'static, Option<SessionInner>> {
    SESSION.lock().unwrap_or_else(|e| e.into_inner())
}

/// 本机标识:Windows 取 COMPUTERNAME,否则返回默认值。
pub fn local_id() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "desktop-cr".to_string())
}

/// 当前会话对端 id(无会话时为 None)。
pub fn session_peer() -> Option<String> {
    session_guard().as_ref().map(|s| s.peer_id.clone())
}

/// 通过会话通道向对端发送一条消息;返回 false 表示已断开。
pub async fn session_send(msg: OutMsg) -> bool {
    let tx = session_guard().as_ref().map(|s| s.tx.clone());
    match tx {
        Some(tx) => tx.send(msg).await.is_ok(),
        None => false,
    }
}

/// 关闭当前会话(踢出对端)。
pub fn close_session() {
    *session_guard() = None;
}

/// 仅当会话仍属于指定对端时才关闭(避免误清新会话),返回是否清理。
fn close_session_if(peer_id: &str, peer_addr: &str) -> bool {
    let mut guard = session_guard();
    if let Some(s) = guard.as_ref() {
        if s.peer_id == peer_id && s.peer_addr == peer_addr {
            *guard = None;
            return true;
        }
    }
    false
}

/// 当前毫秒时间戳(用于 ping/pong)。
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// 写一条消息(4 字节小端长度 + JSON)。
async fn write_msg<S: AsyncWrite + Unpin>(stream: &mut S, msg: &Msg) -> Result<(), String> {
    let bytes = serde_json::to_vec(msg).map_err(|e| format!("序列化失败: {e}"))?;
    let len = (bytes.len() as u32).to_le_bytes();
    stream
        .write_all(&len)
        .await
        .map_err(|e| format!("写入长度失败: {e}"))?;
    stream
        .write_all(&bytes)
        .await
        .map_err(|e| format!("写入消息失败: {e}"))?;
    Ok(())
}

/// 读一条消息(4 字节小端长度 + JSON)。
async fn read_msg<R: AsyncRead + Unpin>(stream: &mut R) -> Result<Msg, String> {
    let mut len = [0u8; 4];
    stream
        .read_exact(&mut len)
        .await
        .map_err(|e| format!("读取消息头失败(连接已断开): {e}"))?;
    let n = u32::from_le_bytes(len) as usize;
    if n == 0 || n > MAX_FRAME_BYTES {
        return Err(format!("非法消息长度: {n}"));
    }
    let mut buf = vec![0u8; n];
    stream
        .read_exact(&mut buf)
        .await
        .map_err(|e| format!("读取消息体失败: {e}"))?;
    serde_json::from_slice(&buf).map_err(|e| format!("反序列化失败: {e}"))
}

/// 被控端:监听 0.0.0.0:port,接受单个连接(新连接踢掉旧连接)。
///
/// 公开契约入口(内部实现复用 `serve_host`);start_host 因需要同步报告
/// 端口占用等监听失败,会先预绑定 std 监听器再调用 `serve_host`。
#[allow(dead_code)]
pub async fn run_host(app: AppHandle, port: u16) -> Result<(), String> {
    let listener = TcpListener::bind(("0.0.0.0", port))
        .await
        .map_err(|e| format!("监听 0.0.0.0:{port} 失败(端口被占用?): {e}"))?;
    log::info!("[network] host 监听 0.0.0.0:{port}");
    let _ = app.emit("host-state", serde_json::json!({ "running": true, "port": port }));
    serve_host(app, listener).await
}

/// 在已绑定的监听器上接受连接并服务(供 start_host 预绑定后调用)。
pub(crate) async fn serve_host(app: AppHandle, listener: TcpListener) -> Result<(), String> {
    loop {
        let (stream, addr) = listener
            .accept()
            .await
            .map_err(|e| format!("accept 失败: {e}"))?;
        log::info!("[network] 收到连接: {addr}");
        // 单连接策略:新连接踢掉旧连接(旧会话发送通道关闭后其任务自然退出)
        close_session();
        let app = app.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_host_connection(app.clone(), stream, addr).await {
                log::warn!("[network] 会话结束(错误): {e}");
            }
        });
    }
}

/// 处理单个被控端连接:握手 → 两路任务(收消息 / 推帧)。
async fn handle_host_connection(
    app: AppHandle,
    mut stream: TcpStream,
    addr: std::net::SocketAddr,
) -> Result<(), String> {
    // 1) 握手:等待 hello
    let hello: Msg = tokio::time::timeout(HANDSHAKE_TIMEOUT, read_msg(&mut stream))
        .await
        .map_err(|_| format!("握手超时(来自 {addr})"))??;
    let peer_id = match &hello {
        Msg::Hello { id, app: app_name, ver } => {
            if app_name != APP_NAME || *ver != PROTOCOL_VERSION {
                return Err(format!("协议不匹配: app={app_name}, ver={ver:?}"));
            }
            id.clone()
        }
        other => return Err(format!("首条消息必须是 hello,收到: {other:?}")),
    };

    // 2) 回 hello-ack
    write_msg(&mut stream, &Msg::HelloAck { id: local_id() }).await?;

    // 3) 注册会话
    let (tx, rx) = mpsc::channel::<Msg>(64);
    *session_guard() = Some(SessionInner {
        peer_id: peer_id.clone(),
        peer_addr: addr.to_string(),
        tx,
    });
    let _ = app.emit(
        "connection-state",
        serde_json::json!({ "connected": true, "peerId": peer_id }),
    );

    // 4) 两路任务:收消息 + 推帧
    let (read_half, write_half) = stream.into_split();
    let read_task = tokio::spawn(host_read_loop(app.clone(), read_half));
    let write_task = tokio::spawn(host_write_loop(write_half, rx));

    tokio::select! {
        r = read_task => r.map_err(|e| format!("读任务失败: {e}"))??,
        w = write_task => w.map_err(|e| format!("写任务失败: {e}"))??,
    }

    // 任一路退出即会话结束;仅当会话仍属于自己时才广播断开
    if close_session_if(&peer_id, &addr.to_string()) {
        let _ = app.emit(
            "connection-state",
            serde_json::json!({ "connected": false, "peerId": peer_id }),
        );
    }
    Ok(())
}

/// 被控端收消息循环:鼠标/键盘注入、剪贴板写入、ping→pong。
async fn host_read_loop(app: AppHandle, mut read_half: tokio::net::tcp::OwnedReadHalf) -> Result<(), String> {
    loop {
        let msg = read_msg(&mut read_half).await?;
        match msg {
            Msg::Mouse { x, y, kind, button, delta } => {
                // 协议坐标为 0..1 归一化,换算成 0..65535 绝对坐标
                let ex = x.clamp(0.0, 1.0) * 65535.0;
                let ey = y.clamp(0.0, 1.0) * 65535.0;
                let event_type = match kind.as_str() {
                    "down" => "mousedown",
                    "up" => "mouseup",
                    "wheel" => "wheel",
                    _ => "mousemove",
                };
                #[cfg(target_os = "windows")]
                {
                    if let Err(e) = crate::input_injector::inject_mouse_event_windows(
                        ex,
                        ey,
                        event_type,
                        button.as_deref(),
                        delta,
                    ) {
                        log::warn!("[network] 注入鼠标事件失败: {e}");
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    log::info!("[network] (非 Windows) 收到鼠标消息 kind={kind} x={ex:.1} y={ey:.1}");
                }
            }
            Msg::Key { key, kind, code, mods } => {
                let event_type = if kind == "up" { "keyup" } else { "keydown" };
                #[cfg(target_os = "windows")]
                {
                    if let Err(e) = crate::input_injector::inject_key_event_windows(
                        &key,
                        event_type,
                        code.as_deref(),
                        &mods,
                    ) {
                        log::warn!("[network] 注入键盘事件失败: {e}");
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    log::info!("[network] (非 Windows) 收到键盘消息 key={key} kind={kind}");
                }
            }
            Msg::Clipboard { text } => {
                // 写入本机剪贴板并通知前端
                if let Err(e) = crate::hbb_client::set_clipboard_text(text.clone()) {
                    log::warn!("[network] 写入剪贴板失败: {e}");
                }
                let _ = app.emit("clipboard-synced", serde_json::json!({ "text": text }));
            }
            Msg::Stream {
                fps,
                jpeg_quality,
                width,
                height,
            } => {
                // 控制端调整画质/分辨率:实时应用到被控端抓帧配置
                crate::hbb_client::apply_stream_cfg(fps, jpeg_quality, width, height);
            }
            Msg::Ping { ts } => {
                // 心跳:回 pong
                let _ = session_send(Msg::Pong { ts }).await;
            }
            _ => {}
        }
    }
}

/// 被控端推帧循环:按流配置的帧率推送最新 JPEG 帧,同时转发会话消息。
async fn host_write_loop(
    mut write_half: tokio::net::tcp::OwnedWriteHalf,
    mut rx: mpsc::Receiver<Msg>,
) -> Result<(), String> {
    let mut seq: u64 = 0;
    loop {
        let cfg = crate::hbb_client::stream_cfg();
        let wait_ms = (1000u64 / u64::from(cfg.fps.clamp(1, 30))).max(1);
        let outgoing: Option<Msg> = tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(wait_ms)) => {
                // 有最新帧才推送(没有帧则跳过本轮)
                match crate::capture::latest_frame() {
                    Some((w, h, jpeg)) => Some(Msg::Frame {
                        w,
                        h,
                        seq,
                        jpeg: base64::engine::general_purpose::STANDARD.encode(&jpeg),
                    }),
                    None => None,
                }
            }
            m = rx.recv() => m,
        };
        let Some(msg) = outgoing else {
            // 通道关闭 = 会话被替换或主动关闭,结束写循环
            break;
        };
        if let Msg::Frame { seq: s, .. } = &msg {
            seq = s.wrapping_add(1);
        }
        write_msg(&mut write_half, &msg)
            .await
            .map_err(|e| format!("发送消息失败: {e}"))?;
    }
    log::info!("[network] host 写循环结束");
    Ok(())
}

/// 控制端:连接对端(8 秒超时)→ 握手 → 启动收发循环。
pub async fn connect_peer(app: AppHandle, id: String, addr: String) -> Result<(), String> {
    // 1) 连接(超时)
    let mut stream = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(&addr))
        .await
        .map_err(|_| format!("连接 {addr} 超时(8秒)"))?
        .map_err(|e| format!("连接 {addr} 失败: {e}"))?;

    // 2) 握手:发 hello,收 hello-ack
    write_msg(
        &mut stream,
        &Msg::Hello {
            id: local_id(),
            app: APP_NAME.into(),
            ver: PROTOCOL_VERSION,
        },
    )
    .await?;
    let ack: Msg = tokio::time::timeout(HANDSHAKE_TIMEOUT, read_msg(&mut stream))
        .await
        .map_err(|_| "握手超时(未收到 hello-ack)".to_string())??;
    match ack {
        Msg::HelloAck { id: host_id } => {
            log::info!("[network] 握手成功,对端: {host_id} ({addr})");
        }
        other => return Err(format!("握手响应异常: {other:?}")),
    }

    // 3) 注册会话
    let (tx, rx) = mpsc::channel::<Msg>(64);
    *session_guard() = Some(SessionInner {
        peer_id: id.clone(),
        peer_addr: addr.clone(),
        tx,
    });

    // 4) 收消息循环(帧/剪贴板)+ 写通道循环(转发 session_send 的消息)
    let (read_half, write_half) = stream.into_split();
    let read_app = app.clone();
    let read_id = id.clone();
    let read_addr = addr.clone();
    tokio::spawn(async move {
        peer_read_loop(read_app, read_half, read_id, read_addr).await;
    });
    tokio::spawn(async move {
        if let Err(e) = peer_write_loop(write_half, rx).await {
            log::warn!("[network] 控制端写循环结束: {e}");
        }
    });

    Ok(())
}

/// 控制端收消息循环:远程帧推送、剪贴板同步、心跳记录;断线清理会话并广播。
async fn peer_read_loop(
    app: AppHandle,
    mut read_half: tokio::net::tcp::OwnedReadHalf,
    peer_id: String,
    peer_addr: String,
) {
    let result: Result<(), String> = async {
        loop {
            let msg = read_msg(&mut read_half).await?;
            match msg {
                Msg::Frame { w, h, seq: _, jpeg } => {
                    // base64 解码后推送远程帧
                    match base64::engine::general_purpose::STANDARD.decode(&jpeg) {
                        Ok(data) => {
                            let _ = app.emit(
                                "remote-frame",
                                RemoteFrameEvent {
                                    width: w,
                                    height: h,
                                    jpeg: data,
                                },
                            );
                        }
                        Err(e) => log::warn!("[network] 帧数据 base64 解码失败: {e}"),
                    }
                }
                Msg::Clipboard { text } => {
                    // 对端剪贴板同步
                    let _ = app.emit("clipboard-synced", serde_json::json!({ "text": text }));
                }
                Msg::Pong { ts } => {
                    // 记录心跳延迟(仅调试)
                    let now = now_ms();
                    log::debug!("[network] pong 延迟: {} ms", now.saturating_sub(ts));
                }
                _ => {}
            }
        }
    }
    .await;

    // 断线清理:仅当会话仍属于自己时广播断开
    if close_session_if(&peer_id, &peer_addr) {
        let _ = app.emit(
            "connection-state",
            serde_json::json!({
                "connected": false,
                "peerId": peer_id,
                "error": result.err().unwrap_or_else(|| "连接已断开".to_string()),
            }),
        );
    }
}

/// 控制端写通道循环:仅转发 session_send 写入的消息。
async fn peer_write_loop(
    mut write_half: tokio::net::tcp::OwnedWriteHalf,
    mut rx: mpsc::Receiver<Msg>,
) -> Result<(), String> {
    while let Some(msg) = rx.recv().await {
        write_msg(&mut write_half, &msg)
            .await
            .map_err(|e| format!("发送消息失败: {e}"))?;
    }
    Ok(())
}
