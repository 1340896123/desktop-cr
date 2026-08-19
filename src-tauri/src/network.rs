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
use std::collections::HashMap;
use std::io::Write;
use std::sync::{LazyLock, Mutex};
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
/// 握手超时。
const HANDSHAKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// serde 默认编码类型(兼容旧版本帧消息)。
fn default_codec() -> String {
    "jpeg".to_string()
}

/// 协议消息(以 `t` 字段区分类型;变体名转 kebab-case,如 HelloAck → "hello-ack")。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// 视频帧(jpeg 为 base64 编码;dur 为被控端编码耗时毫秒,用于性能统计;
/// codec 为 "jpeg" | "h264" | "hevc",h264/hevc 时 jpeg 字段为 Annex-B 字节的 base64)。
    Frame {
        w: u32,
        h: u32,
        seq: u64,
        jpeg: String,
        dur: u32,
        #[serde(default = "default_codec")]
        codec: String,
        #[serde(default)]
        key: bool,
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
    /// 流参数调整(控制端 → 被控端,被控端抓帧循环实时应用;monitor 为可选的目标显示器;
/// codec 为 "jpeg" | "h264",控制端无 FFmpeg 时应下发 "jpeg")。
    Stream {
        fps: u32,
        jpeg_quality: u8,
        width: u32,
        height: u32,
        monitor: Option<u32>,
        #[serde(default = "default_codec")]
        codec: String,
    },
    /// 请求远程显示器列表(控制端 → 被控端)
    Monitors,
    /// 远程显示器列表应答(被控端 → 控制端)
    MonitorsAck {
        monitors: Vec<crate::capture::MonitorInfo>,
    },
    /// 文件传输开始(控制端 → 被控端;id 为本次传输标识)
    FileStart {
        id: u32,
        name: String,
        size: u64,
    },
    /// 文件数据块(data 为 base64 编码的字节块)
    FileData {
        id: u32,
        seq: u64,
        data: String,
    },
    /// 文件传输结束(control 端发送,表示所有块已发完)
    FileEnd {
        id: u32,
        total_chunks: u64,
    },
    /// 文件传输进度应答(被控端 → 控制端)
    FileAck {
        id: u32,
        received: u64,
        total: u64,
    },
    /// 请求对端目录列表(控制端 → 被控端)
    DirList {
        path: String,
    },
    /// 目录列表应答(被控端 → 控制端)
    DirListAck {
        path: String,
        entries: Vec<FileEntry>,
        error: Option<String>,
    },
    /// 请求对端发送指定文件(控制端 → 被控端;id 由控制端分配,对端复用)
    FileRequest {
        id: u32,
        path: String,
    },
    /// 心跳(毫秒时间戳)
    Ping {
        ts: u64,
    },
    /// 心跳响应
    Pong {
        ts: u64,
    },
    /// 音频帧(wav 为 base64 编码;协议预留,当前仅被控端记录日志)
    Audio {
        sample_rate: u16,
        channels: u16,
        seq: u64,
        wav: String,
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
    /// 帧序号(用于丢包统计)
    pub seq: u64,
    /// 被控端编码耗时(毫秒)
    pub dur: u32,
}

/// 会话实时指标(前端性能浮窗)。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetrics {
    /// 最近一次 ping/pong 往返延迟(毫秒)
    pub rtt_ms: Option<u64>,
}

/// 会话实时指标(仅控制端更新)。
static SESSION_METRICS: Mutex<SessionMetrics> = Mutex::new(SessionMetrics { rtt_ms: None });

/// 读取会话实时指标。
pub fn get_session_metrics() -> SessionMetrics {
    SESSION_METRICS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// 被控端当前抓帧显示器(Stream.monitor 切换用)。
static HOST_MONITOR: Mutex<Option<u32>> = Mutex::new(None);

fn host_monitor() -> Option<u32> {
    HOST_MONITOR.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn set_host_monitor(m: Option<u32>) {
    *HOST_MONITOR.lock().unwrap_or_else(|e| e.into_inner()) = m;
}

/// 被控端接收中的文件状态(FileStart → FileData... → FileEnd)。
struct IncomingFile {
    name: String,
    size: u64,
    received: u64,
    writer: Option<std::fs::File>,
}

/// 被控端接收中的文件(id → 状态)。
static INCOMING: LazyLock<Mutex<HashMap<u32, IncomingFile>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 文件传输开始:在被控端接收目录创建文件。
async fn network_file_start(id: u32, name: &str, size: u64) {
    let safe_name = std::path::Path::new(name)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "file.bin".into());
    let dir = crate::hbb_client::incoming_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("[network] 创建接收目录失败: {e}");
        return;
    }
    let path = dir.join(&safe_name);
    match std::fs::File::create(&path) {
        Ok(writer) => {
            if let Ok(mut map) = INCOMING.lock() {
                map.insert(
                    id,
                    IncomingFile {
                        name: safe_name.clone(),
                        size,
                        received: 0,
                        writer: Some(writer),
                    },
                );
            }
            log::info!("[network] 接收文件开始: id={id}, name={safe_name}, size={size}");
            crate::operation_log::op_log("network", "file_start", &format!("id={id} name={safe_name} size={size}"));
        }
        Err(e) => log::warn!("[network] 创建接收文件 {safe_name} 失败: {e}"),
    }
}

/// 文件数据块:写入文件并回传进度应答。
async fn network_file_data(id: u32, seq: u64, data: &str) {
    let bytes = match base64::engine::general_purpose::STANDARD.decode(data) {
        Ok(b) => b,
        Err(e) => {
            log::warn!("[network] 文件数据块 base64 解码失败: {e}");
            return;
        }
    };
    let received;
    let total;
    {
        let mut map = INCOMING.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(f) = map.get_mut(&id) {
            if let Some(writer) = f.writer.as_mut() {
                if let Err(e) = writer.write_all(&bytes) {
                    log::warn!("[network] 写入文件数据失败: {e}");
                    f.writer = None;
                } else {
                    f.received += bytes.len() as u64;
                }
            }
            received = f.received;
            total = f.size;
        } else {
            log::debug!("[network] 收到未知文件 id={id} 的数据块(seq={seq}),忽略");
            return;
        }
    }
    // 释放锁后再回传进度(每块一次,供前端进度条)
    let _ = session_send(Msg::FileAck {
        id,
        received,
        total,
    })
    .await;
}

/// 文件传输结束:关闭文件、记录日志。
async fn network_file_end(id: u32, total_chunks: u64) {
    let (received, size, name, completed) = {
        let mut map = INCOMING.lock().unwrap_or_else(|e| e.into_inner());
        match map.remove(&id) {
            Some(mut f) => {
                let completed = f.received == f.size;
                f.writer = None;
                (f.received, f.size, f.name.clone(), completed)
            }
            None => return,
        }
    };
    // 释放锁后再回传最终进度
    let _ = session_send(Msg::FileAck {
        id,
        received,
        total: size,
    })
    .await;
    log::info!(
        "[network] 接收文件完成: id={id}, name={name}, received={received}/{size} bytes, chunks={total_chunks}, ok={completed}"
    );
    crate::operation_log::op_log(
        "network",
        "file_end",
        &format!("id={id} name={name} ok={completed}"),
    );
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
pub(crate) async fn write_msg<S: AsyncWrite + Unpin>(
    stream: &mut S,
    msg: &Msg,
) -> Result<(), String> {
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
pub(crate) async fn read_msg<R: AsyncRead + Unpin>(stream: &mut R) -> Result<Msg, String> {
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
        crate::operation_log::op_log("network", "host_accept", &format!("addr={addr}"));
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

    let sess_err: Result<(), String> = tokio::select! {
        r = read_task => r.map_err(|e| format!("读任务失败: {e}"))?,
        w = write_task => w.map_err(|e| format!("写任务失败: {e}"))?,
    };

    // 任一路退出即会话结束;仅当会话仍属于自己时才广播断开
    if close_session_if(&peer_id, &addr.to_string()) {
        let reason = sess_err
            .as_ref()
            .err()
            .cloned()
            .unwrap_or_else(|| "会话结束(正常)".to_string());
        crate::operation_log::op_log(
            "network",
            "host_session_end",
            &format!("peer={peer_id} addr={addr} reason={reason}"),
        );
        let _ = app.emit(
            "connection-state",
            serde_json::json!({ "connected": false, "peerId": peer_id }),
        );
    }
    sess_err?;
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
                monitor,
                codec,
            } => {
                // 控制端调整画质/分辨率:实时应用到被控端抓帧配置
                crate::hbb_client::apply_stream_cfg(fps, jpeg_quality, width, height, codec);
                // 目标显示器变化时重启抓帧循环
                if let Some(target) = monitor {
                    let current = host_monitor();
                    if current != Some(target) {
                        log::info!("[network] 切换被控端抓帧显示器: {:?} → {target}", current);
                        let _ = crate::capture::stop_capture();
                        let cfg = crate::hbb_client::stream_cfg();
                        if let Err(e) = crate::capture::start_capture(
                            target,
                            cfg.target_width,
                            cfg.target_height,
                            cfg.fps,
                            app.clone(),
                        )
                        .await
                        {
                            log::warn!("[network] 切换抓帧显示器失败: {e}");
                        }
                        set_host_monitor(Some(target));
                    }
                }
            }
            Msg::Monitors => {
                // 应答远程显示器列表
                #[cfg(target_os = "windows")]
                {
                    match crate::capture::list_monitors(app.clone()) {
                        Ok(monitors) => {
                            let _ = session_send(Msg::MonitorsAck { monitors }).await;
                        }
                        Err(e) => log::warn!("[network] 枚举显示器失败: {e}"),
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let _ = session_send(Msg::MonitorsAck { monitors: vec![] }).await;
                }
            }
            Msg::FileStart { id, name, size } => {
                network_file_start(id, &name, size).await;
            }
            Msg::FileData { id, seq, data } => {
                network_file_data(id, seq, &data).await;
            }
            Msg::FileEnd { id, total_chunks } => {
                network_file_end(id, total_chunks).await;
            }
            Msg::Ping { ts } => {
                // 心跳:回 pong
                let _ = session_send(Msg::Pong { ts }).await;
            }
            Msg::Audio {
                sample_rate,
                channels,
                seq,
                ..
            } => {
                // 音频帧(协议预留):仅记录日志,暂不做播放
                log::info!(
                    "[network] 收到音频帧 sample_rate={sample_rate} channels={channels} seq={seq}"
                );
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
                // 有最新帧才推送(没有帧则跳过本轮);codec 依流配置选 FFmpeg 视频或 JPEG
                let use_video = cfg.codec == "h264" || cfg.codec == "hevc";
                let frame = if use_video {
                    crate::capture::latest_video()
                        .map(|(w, h, data, key)| (w, h, data, cfg.codec.clone(), key))
                        .or_else(|| {
                            crate::capture::latest_frame()
                                .map(|(w, h, j)| (w, h, j, "jpeg".to_string(), false))
                        })
                } else {
                    crate::capture::latest_frame()
                        .map(|(w, h, j)| (w, h, j, "jpeg".to_string(), false))
                };
                match frame {
                    Some((w, h, data, codec, key)) => Some(Msg::Frame {
                        w,
                        h,
                        seq,
                        jpeg: base64::engine::general_purpose::STANDARD.encode(&data),
                        dur: if use_video {
                            crate::capture::latest_video_dur_ms()
                        } else {
                            crate::capture::latest_frame_dur_ms()
                        },
                        codec,
                        key,
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
///
/// 连接路径回退链:先直连配置的 `addr`(LAN)→ 信令服务器返回的外部地址
/// (`external`)→ 中继服务器(`relay`)兜底;返回路径描述供日志/前端展示。
pub async fn connect_peer(
    app: AppHandle,
    id: String,
    addr: String,
    external: Option<String>,
    relay: Option<String>,
) -> Result<String, String> {
    // 1) 打开传输通道(直连 → 外部 → 中继)
    let (mut stream, via) =
        open_transport(Some(&addr), external.as_deref(), relay.as_deref(), &id).await?;
    log::info!("[network] 传输路径: {via}");

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
            log::info!("[network] 握手成功,对端: {host_id} ({via})");
            crate::operation_log::op_log(
                "network",
                "connect",
                &format!("peer={host_id} via={via}"),
            );
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

    // 3.5) 连接即下发当前流参数(codec 偏好等),使被控端默认走 FFmpeg 编码
    {
        let cfg = crate::hbb_client::stream_cfg();
        let _ = session_send(Msg::Stream {
            fps: cfg.fps,
            jpeg_quality: cfg.jpeg_quality,
            width: cfg.target_width,
            height: cfg.target_height,
            monitor: None,
            codec: crate::hbb_client::stream_codec_choice(),
        })
        .await;
    }

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

    Ok(via)
}

/// 获取本机局域网 IPv4(通过 UDP socket 路由探测,不实际发包)。
pub fn local_ipv4() -> Option<std::net::Ipv4Addr> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:53").ok()?;
    match sock.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(v4) => Some(v4),
        _ => None,
    }
}

/// 连接信令服务器并发起一次请求,读取应答后断开。
async fn signal_query<T>(addr: &str, send: dcr_server::message::SignalMsg) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
    let mut stream = tokio::time::timeout(
        std::time::Duration::from_secs(8),
        TcpStream::connect(addr),
    )
    .await
    .map_err(|_| format!("连接信令服务器 {addr} 超时"))?
    .map_err(|e| format!("连接信令服务器 {addr} 失败: {e}"))?;
    dcr_server::framing::write_msg(&mut stream, &send).await?;
    dcr_server::framing::read_msg(&mut stream).await
}

/// 查询对端在信令服务器上的信息,返回 (lan, external, relay_hint);离线返回 None。
pub async fn signal_lookup(
    signal_addr: &str,
    id: &str,
) -> Result<Option<(String, String, String)>, String> {
    use dcr_server::message::SignalMsg;
    let ack: SignalMsg = signal_query(
        signal_addr,
        SignalMsg::Lookup {
            id: id.to_string(),
        },
    )
    .await?;
    match ack {
        SignalMsg::LookupAck {
            online,
            lan,
            external,
            relay_hint,
        } => {
            if online {
                Ok(Some((lan, external, relay_hint)))
            } else {
                Ok(None)
            }
        }
        _ => Err("信令服务器应答异常".into()),
    }
}

/// 查询信令服务器在线设备列表。
pub async fn signal_list(signal_addr: &str) -> Result<Vec<dcr_server::message::PeerEntry>, String> {
    use dcr_server::message::SignalMsg;
    let ack: SignalMsg = signal_query(signal_addr, SignalMsg::List).await?;
    match ack {
        SignalMsg::ListAck { peers } => Ok(peers),
        _ => Err("信令服务器应答异常".into()),
    }
}

/// 经中继服务器连接对端(role=client),返回已建立的连接。
async fn open_relay_stream(relay_addr: &str, peer_id: &str) -> Result<TcpStream, String> {
    use dcr_server::message::RelayMsg;
    let mut stream = tokio::time::timeout(
        std::time::Duration::from_secs(8),
        TcpStream::connect(relay_addr),
    )
    .await
    .map_err(|_| format!("连接中继服务器 {relay_addr} 超时"))?
    .map_err(|e| format!("连接中继服务器 {relay_addr} 失败: {e}"))?;
    dcr_server::framing::write_msg(
        &mut stream,
        &RelayMsg::Allocate {
            id: peer_id.to_string(),
            role: "client".into(),
        },
    )
    .await?;
    let ack: RelayMsg = dcr_server::framing::read_msg(&mut stream).await?;
    match ack {
        RelayMsg::Allocated { peer_connected, .. } => {
            if peer_connected {
                Ok(stream)
            } else {
                Err(format!("对端 {peer_id} 未接入中继"))
            }
        }
        other => Err(format!("中继应答异常: {other:?}")),
    }
}

/// 打开到对端的传输通道:直连(配置 LAN)→ 外部地址(信令返回)→ 中继兜底。
///
/// 返回 (已建立的 TcpStream, 路径描述)。
pub(crate) async fn open_transport(
    direct: Option<&str>,
    external: Option<&str>,
    relay: Option<&str>,
    peer_id: &str,
) -> Result<(TcpStream, String), String> {
    // 1) 直连配置地址(LAN)
    if let Some(addr) = direct {
        match tokio::time::timeout(
            std::time::Duration::from_secs(3),
            TcpStream::connect(addr),
        )
        .await
        {
            Ok(Ok(s)) => return Ok((s, format!("直连 {addr}"))),
            Ok(Err(e)) => log::info!("[network] 直连 {addr} 失败: {e}"),
            Err(_) => log::info!("[network] 直连 {addr} 超时(3秒)"),
        }
    }
    // 2) 外部地址(信令服务器返回的反射地址)
    if let Some(addr) = external {
        if direct.map(|d| d != addr).unwrap_or(true) {
            match tokio::time::timeout(
                std::time::Duration::from_secs(3),
                TcpStream::connect(addr),
            )
            .await
            {
                Ok(Ok(s)) => return Ok((s, format!("直连外部 {addr}"))),
                Ok(Err(e)) => log::info!("[network] 直连外部 {addr} 失败: {e}"),
                Err(_) => log::info!("[network] 直连外部 {addr} 超时(3秒)"),
            }
        }
    }
    // 3) 中继兜底
    if let Some(relay) = relay {
        match open_relay_stream(relay, peer_id).await {
            Ok(s) => return Ok((s, format!("中继 {relay}"))),
            Err(e) => log::info!("[network] 中继连接失败: {e}"),
        }
    }
    Err("所有连接路径均失败(直连/外部/中继)".into())
}

/// 向信令服务器注册本机并持续心跳(host 侧,连接断开自动重连)。
pub(crate) async fn signal_register_loop(
    signal_addr: Option<String>,
    host_id: String,
    lan: String,
) {
    let Some(signal_addr) = signal_addr else {
        return;
    };
    log::info!("[network] 信令注册循环启动: id={host_id}, lan={lan}, server={signal_addr}");
    loop {
        // 注册
        match signal_query(
            &signal_addr,
            dcr_server::message::SignalMsg::Register {
                id: host_id.clone(),
                lan: lan.clone(),
            },
        )
        .await
        {
            Ok(dcr_server::message::SignalMsg::RegisterAck { ok, msg }) => {
                log::info!("[network] 信令注册完成: ok={ok}, {msg}");
            }
            Ok(_) => log::warn!("[network] 信令注册应答异常"),
            Err(e) => {
                log::warn!("[network] 信令注册失败: {e}");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
        }
        // 心跳(20 秒间隔;失败则重连注册)
        let mut heartbeat_ok = true;
        while heartbeat_ok {
            tokio::time::sleep(std::time::Duration::from_secs(20)).await;
            match signal_query(
                &signal_addr,
                dcr_server::message::SignalMsg::Heartbeat {
                    id: host_id.clone(),
                },
            )
            .await
            {
                Ok(dcr_server::message::SignalMsg::RegisterAck { ok, .. }) => {
                    if !ok {
                        log::warn!("[network] 信令心跳被拒,重新注册");
                        heartbeat_ok = false;
                    }
                }
                Ok(_) => log::warn!("[network] 信令心跳应答异常"),
                Err(e) => {
                    log::warn!("[network] 信令心跳失败: {e},重新注册");
                    heartbeat_ok = false;
                }
            }
        }
        // 等待后重新注册
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}

/// 控制端收消息循环:远程帧推送、剪贴板同步、心跳记录;断线清理会话并广播。
async fn peer_read_loop(
    app: AppHandle,
    mut read_half: tokio::net::tcp::OwnedReadHalf,
    peer_id: String,
    peer_addr: String,
) {
    // FFmpeg 解码器(懒创建,会话内复用;codec 变化时重建)
    let mut decoder: Option<(String, crate::ffmpeg_hw::HwDecoder)> = None;
    let result: Result<(), String> = async {
        loop {
            let msg = read_msg(&mut read_half).await?;
            match msg {
                Msg::Frame {
                    w,
                    h,
                    seq,
                    jpeg,
                    dur,
                    codec,
                    key: _,
                } => {
                    // base64 解码后推送远程帧(携带序号与编码耗时供性能统计)
                    match base64::engine::general_purpose::STANDARD.decode(&jpeg) {
                        Ok(data) => {
                            if codec == "h264" || codec == "hevc" {
                                // FFmpeg 解码(H.264/H.265)→ 缩放 → JPEG,前端渲染方式不变
                                match decode_video_to_jpeg(&mut decoder, &codec, &data, 1280, 720, 80)
                                {
                                    Some((dw, dh, jpeg_bytes)) => {
                                        let _ = app.emit(
                                            "remote-frame",
                                            RemoteFrameEvent {
                                                width: dw,
                                                height: dh,
                                                jpeg: jpeg_bytes,
                                                seq,
                                                dur,
                                            },
                                        );
                                    }
                                    None => log::warn!(
                                        "[network] {codec} 帧解码失败(seq={seq}),跳过"
                                    ),
                                }
                            } else {
                                let _ = app.emit(
                                    "remote-frame",
                                    RemoteFrameEvent {
                                        width: w,
                                        height: h,
                                        jpeg: data,
                                        seq,
                                        dur,
                                    },
                                );
                            }
                        }
                        Err(e) => log::warn!("[network] 帧数据 base64 解码失败: {e}"),
                    }
                }
                Msg::Clipboard { text } => {
                    // 对端剪贴板同步
                    let _ = app.emit("clipboard-synced", serde_json::json!({ "text": text }));
                }
                Msg::MonitorsAck { monitors } => {
                    // 远程显示器列表
                    let _ = app.emit("remote-monitors", serde_json::json!({ "monitors": monitors }));
                }
                Msg::FileAck { id, received, total } => {
                    // 文件传输进度
                    let _ = app.emit(
                        "file-progress",
                        serde_json::json!({ "id": id, "received": received, "total": total }),
                    );
                }
                Msg::FileStart { id, name, size } => {
                    // 被控端 → 控制端反向文件传输:复用接收状态机(落盘 + 回 FileAck)
                    network_file_start(id, &name, size).await;
                }
                Msg::FileData { id, seq, data } => {
                    network_file_data(id, seq, &data).await;
                }
                Msg::FileEnd { id, total_chunks } => {
                    network_file_end(id, total_chunks).await;
                }
                Msg::Pong { ts } => {
                    // 记录 ping/pong 往返延迟(实时指标)
                    let now = now_ms();
                    let rtt = now.saturating_sub(ts);
                    *SESSION_METRICS
                        .lock()
                        .unwrap_or_else(|e| e.into_inner()) = SessionMetrics {
                        rtt_ms: Some(rtt),
                    };
                    log::debug!("[network] pong 延迟: {rtt} ms");
                }
                _ => {}
            }
        }
    }
    .await;

    // 断线清理:仅当会话仍属于自己时广播断开
    if close_session_if(&peer_id, &peer_addr) {
        let reason = result.err().unwrap_or_else(|| "连接已断开".to_string());
        crate::operation_log::op_log(
            "network",
            "disconnect",
            &format!("peer={peer_id} addr={peer_addr} reason={reason}"),
        );
        let _ = app.emit(
            "connection-state",
            serde_json::json!({
                "connected": false,
                "peerId": peer_id,
                "error": reason,
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

/// 将 H.264/H.265 Annex-B 帧解码为 JPEG(控制器端,供前端渲染)。
///
/// 内部复用会话级解码器(codec 变化时按 `codec_family_id` 重建);解码 → RGB24 →
/// 等比缩放到 (max_w, max_h) → JPEG。
fn decode_video_to_jpeg(
    decoder: &mut Option<(String, crate::ffmpeg_hw::HwDecoder)>,
    codec: &str,
    data: &[u8],
    max_w: u32,
    max_h: u32,
    quality: u8,
) -> Option<(u32, u32, Vec<u8>)> {
    let need_rebuild = decoder.as_ref().map(|(c, _)| c != codec).unwrap_or(true);
    if need_rebuild {
        if let Ok(d) = crate::ffmpeg_hw::HwDecoder::open(crate::ffmpeg_hw::codec_family_id(codec)) {
            *decoder = Some((codec.to_string(), d));
        }
    }
    let (_, d) = decoder.as_mut()?;
    let (dw, dh, rgb) = d.decode(data).ok()??;
    let (jw, jh) = crate::capture::scale_dimensions(dw, dh, max_w, max_h);
    let (_, _, jpeg) = crate::capture::rgb_to_jpeg(&rgb, dw, dh, jw, jh, quality).ok()?;
    Some((jw, jh, jpeg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msg_tag_serialization() {
        // 序列化后应含内部标签 `t`
        let hello_ack = Msg::HelloAck { id: "peer-1".into() };
        let s = serde_json::to_string(&hello_ack).unwrap();
        assert!(s.contains("\"t\":\"hello-ack\""));

        let frame = Msg::Frame {
            w: 640,
            h: 360,
            seq: 3,
            jpeg: "AQID".into(),
            dur: 2,
            codec: "jpeg".into(),
            key: false,
        };
        let s = serde_json::to_string(&frame).unwrap();
        assert!(s.contains("\"t\":\"frame\""));
        let s = serde_json::to_string(&frame).unwrap();
        assert!(s.contains("\"t\":\"frame\""));

        let audio = Msg::Audio {
            sample_rate: 48000,
            channels: 2,
            seq: 0,
            wav: "AA==".into(),
        };
        let s = serde_json::to_string(&audio).unwrap();
        assert!(s.contains("\"t\":\"audio\""));
    }

    #[test]
    fn msg_roundtrip() {
        let variants = vec![
            Msg::Hello {
                id: "client-1".into(),
                app: "desktop-cr".into(),
                ver: 1,
            },
            Msg::HelloAck { id: "host-1".into() },
            Msg::Frame {
                w: 1280,
                h: 720,
                seq: 42,
                jpeg: "aGVsbG8=".into(),
                dur: 8,
                codec: "h264".into(),
                key: true,
            },
            Msg::Mouse {
                x: 0.5,
                y: 0.25,
                kind: "move".into(),
                button: None,
                delta: 0.0,
            },
            Msg::Key {
                key: "a".into(),
                kind: "down".into(),
                code: Some("KeyA".into()),
                mods: vec!["Control".into()],
            },
            Msg::Clipboard { text: "你好".into() },
            Msg::Stream {
                fps: 30,
                jpeg_quality: 85,
                width: 1920,
                height: 1080,
                monitor: Some(1),
                codec: "h264".into(),
            },
            Msg::Ping { ts: 111 },
            Msg::Pong { ts: 222 },
            Msg::Audio {
                sample_rate: 44100,
                channels: 1,
                seq: 7,
                wav: "d2F2".into(),
            },
        ];
        for v in variants {
            let bytes = serde_json::to_vec(&v).unwrap();
            let back: Msg = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(v, back);
        }
    }

    #[tokio::test]
    async fn framing_roundtrip_tcp() {
        // 监听本地随机端口,一端 write_msg 一端 read_msg,验证 framing 往返一致
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut client = TcpStream::connect(addr).await.unwrap();
        let (mut server, _) = listener.accept().await.unwrap();

        let frame = Msg::Frame {
            w: 960,
            h: 540,
            seq: 9,
            jpeg: "AQIDBAUG".into(),
            dur: 3,
            codec: "jpeg".into(),
            key: false,
        };
        write_msg(&mut server, &frame).await.unwrap();
        let got = read_msg(&mut client).await.unwrap();
        assert_eq!(got, frame);
    }

    // ------------------------------------------------------------------
    // 文件传输:单元测试 + 全双工并发双向 + 最大速率基准
    // ------------------------------------------------------------------

    /// 文件传输测试串行锁:接收状态机依赖全局 SESSION/INCOMING/临时目录,
    /// 并行执行会互相覆盖,串行执行保证隔离。
    static FILE_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    /// 测试辅助:注册唯一临时配置目录(OnceLock 首次生效,后续测试共用),
    /// 并返回实际生效的接收目录(首个注册者的目录,与接收状态机落盘路径一致)。
    fn test_file_env(tag: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("desktop-cr-ft-{tag}-{stamp}"));
        // 忽略失败:首个测试注册成功即可,其余测试共用同一目录
        let _ = crate::hbb_client::register_config_dir(dir.clone());
        crate::hbb_client::incoming_dir()
    }

    /// 测试辅助:构造假会话,使接收状态机回传的 FileAck 进入可读通道。
    fn fake_session(tx: mpsc::Sender<Msg>) {
        *session_guard() = Some(SessionInner {
            peer_id: "test-peer".into(),
            peer_addr: "127.0.0.1:0".into(),
            tx,
        });
    }

    /// 测试辅助:生成带模式的测试文件(每文件唯一偏移,便于字节级比对)。
    fn make_src_file(path: &std::path::Path, base: u8, size: usize) {
        let bytes: Vec<u8> = (0..size).map(|i| base.wrapping_add((i % 251) as u8)).collect();
        std::fs::write(path, bytes).unwrap();
    }

    /// 测试辅助:发送端核心循环 —— FileStart → 64KB 块 FileData → FileEnd(与 send_file_task 同块尺寸/编码)。
    async fn test_send_file(
        writer: std::sync::Arc<tokio::sync::Mutex<tokio::net::tcp::OwnedWriteHalf>>,
        path: std::path::PathBuf,
        id: u32,
    ) {
        use tokio::io::AsyncReadExt;
        let size = std::fs::metadata(&path).unwrap().len();
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "file.bin".into());
        write_msg(
            &mut *writer.lock().await,
            &Msg::FileStart { id, name, size },
        )
        .await
        .unwrap();
        let mut file = tokio::fs::File::open(&path).await.unwrap();
        let mut buf = vec![0u8; 64 * 1024];
        let mut seq = 0u64;
        loop {
            let n = file.read(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            write_msg(
                &mut *writer.lock().await,
                &Msg::FileData {
                    id,
                    seq,
                    data: base64::engine::general_purpose::STANDARD.encode(&buf[..n]),
                },
            )
            .await
            .unwrap();
            seq += 1;
        }
        write_msg(
            &mut *writer.lock().await,
            &Msg::FileEnd { id, total_chunks: seq },
        )
        .await
        .unwrap();
    }

    /// 测试辅助:接收端循环 —— 分发 FileStart/FileData/FileEnd 到真实接收状态机,直到对端关闭连接。
    async fn test_recv_loop(mut read_half: tokio::net::tcp::OwnedReadHalf) {
        loop {
            match read_msg(&mut read_half).await {
                Ok(Msg::FileStart { id, name, size }) => network_file_start(id, &name, size).await,
                Ok(Msg::FileData { id, seq, data }) => network_file_data(id, seq, &data).await,
                Ok(Msg::FileEnd { id, total_chunks }) => network_file_end(id, total_chunks).await,
                Ok(_) => {}
                Err(_) => break,
            }
        }
    }

    #[tokio::test]
    async fn file_transfer_receive_state_machine() {
        // 直接驱动接收状态机:FileStart → 3×FileData → FileEnd
        // 验证:每块回 FileAck、结束回最终进度、文件按块拼接落盘且字节一致。
        let _ft = FILE_TEST_LOCK.lock().await;
        let _log = crate::operation_log::test_lock::LOG_WRITE_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let incoming = test_file_env("sm");
        let (tx, mut rx) = mpsc::channel::<Msg>(64);
        fake_session(tx);

        let id: u32 = 41_001;
        let name = "state-machine.bin";
        let payload: Vec<u8> = (0..256u32).map(|i| (i % 251) as u8).collect();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&payload);
        let total = (payload.len() * 3) as u64;

        network_file_start(id, name, total).await;
        for seq in 0..3u64 {
            network_file_data(id, seq, &b64).await;
        }
        network_file_end(id, 3).await;

        // 3 块各一次 FileAck + 结束一次最终进度
        let mut acks: Vec<Msg> = Vec::new();
        while let Ok(m) = rx.try_recv() {
            acks.push(m);
        }
        assert_eq!(acks.len(), 4, "应收到 4 条 FileAck,实际: {acks:?}");
        for a in &acks {
            match a {
                Msg::FileAck { id: aid, received, total: t } => {
                    assert_eq!(*aid, id);
                    assert_eq!(total, *t);
                    assert!(*received <= total);
                }
                _ => panic!("应只收到 FileAck,实际: {a:?}"),
            }
        }
        // 结束应答应报完整进度
        match acks.last() {
            Some(Msg::FileAck { received, total: t, .. }) => {
                assert_eq!(*received, total);
                assert_eq!(*t, total);
            }
            _ => unreachable!(),
        }

        // 落盘字节 = 3 × payload 拼接
        let disk = std::fs::read(incoming.join(name)).unwrap();
        let mut expect = Vec::with_capacity(total as usize);
        for _ in 0..3 {
            expect.extend_from_slice(&payload);
        }
        assert_eq!(disk, expect, "落盘内容应与发送内容逐字节一致");

        std::fs::remove_file(incoming.join(name)).ok();
        close_session();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn file_transfer_duplex_concurrent_loopback() {
        // 真实 TCP 全双工:双向同时各传 2 个并发文件(共 4 个),验证字节一致。
        let _ft = FILE_TEST_LOCK.lock().await;
        let _log = crate::operation_log::test_lock::LOG_WRITE_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let incoming = test_file_env("duplex");
        let (tx, _ack_drain) = mpsc::channel::<Msg>(256);
        fake_session(tx);

        // 生成 4 个源文件(各 512KB,唯一模式)
        let src_dir = std::env::temp_dir().join(format!(
            "desktop-cr-ft-src-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&src_dir).unwrap();
        let size = 512 * 1024;
        let a1 = src_dir.join("a1.bin");
        let a2 = src_dir.join("a2.bin");
        let b1 = src_dir.join("b1.bin");
        let b2 = src_dir.join("b2.bin");
        make_src_file(&a1, 1, size);
        make_src_file(&a2, 2, size);
        make_src_file(&b1, 3, size);
        make_src_file(&b2, 4, size);

        // A=客户端(发送 a1/a2,接收 B 的 b1/b2);B=服务端(发送 b1/b2,接收 A 的 a1/a2)
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let client = TcpStream::connect(addr).await.unwrap();
        let (server, _) = listener.accept().await.unwrap();
        let (c_read, c_write) = client.into_split();
        let (s_read, s_write) = server.into_split();

        let c_write = std::sync::Arc::new(tokio::sync::Mutex::new(c_write));
        let s_write = std::sync::Arc::new(tokio::sync::Mutex::new(s_write));

        let result = tokio::time::timeout(std::time::Duration::from_secs(30), async {
            let side_a = async {
                let recv = tokio::spawn(test_recv_loop(c_read));
                let s1 = tokio::spawn(test_send_file(c_write.clone(), a1.clone(), 51_001));
                let s2 = tokio::spawn(test_send_file(c_write.clone(), a2.clone(), 51_002));
                s1.await.unwrap();
                s2.await.unwrap();
                drop(c_write); // 关闭本侧写半部 → 对端 recv EOF
                recv.await.unwrap();
            };
            let side_b = async {
                let recv = tokio::spawn(test_recv_loop(s_read));
                let s1 = tokio::spawn(test_send_file(s_write.clone(), b1.clone(), 52_001));
                let s2 = tokio::spawn(test_send_file(s_write.clone(), b2.clone(), 52_002));
                s1.await.unwrap();
                s2.await.unwrap();
                drop(s_write); // 关闭本侧写半部 → 对端 recv EOF
                recv.await.unwrap();
            };
            tokio::join!(side_a, side_b);
        })
        .await;

        assert!(result.is_ok(), "全双工双向传输超时(30 秒)");

        // 校验 4 个接收文件与源逐字节一致
        for (src, recv_name) in [
            (&a1, "a1.bin"),
            (&a2, "a2.bin"),
            (&b1, "b1.bin"),
            (&b2, "b2.bin"),
        ] {
            let disk = std::fs::read(incoming.join(recv_name)).unwrap();
            let orig = std::fs::read(src).unwrap();
            assert_eq!(disk, orig, "接收文件 {recv_name} 与源内容不一致");
        }

        for f in ["a1.bin", "a2.bin", "b1.bin", "b2.bin"] {
            std::fs::remove_file(incoming.join(f)).ok();
        }
        std::fs::remove_dir_all(&src_dir).ok();
        close_session();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn file_transfer_max_rate_loopback() {
        // 最大速率基准:双向同时各传一个文件(各 16MB),测量聚合吞吐。
        let _ft = FILE_TEST_LOCK.lock().await;
        let _log = crate::operation_log::test_lock::LOG_WRITE_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let incoming = test_file_env("rate");
        let (tx, _ack_drain) = mpsc::channel::<Msg>(1024);
        fake_session(tx);

        let src_dir = std::env::temp_dir().join(format!(
            "desktop-cr-ft-rate-src-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&src_dir).unwrap();
        let size = 16 * 1024 * 1024;
        let up = src_dir.join("up.bin");
        let down = src_dir.join("down.bin");
        make_src_file(&up, 7, size);
        make_src_file(&down, 9, size);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let client = TcpStream::connect(addr).await.unwrap();
        let (server, _) = listener.accept().await.unwrap();
        let (c_read, c_write) = client.into_split();
        let (s_read, s_write) = server.into_split();

        let c_write = std::sync::Arc::new(tokio::sync::Mutex::new(c_write));
        let s_write = std::sync::Arc::new(tokio::sync::Mutex::new(s_write));

        let started = std::time::Instant::now();
        let result = tokio::time::timeout(std::time::Duration::from_secs(60), async {
            let side_a = async {
                let recv = tokio::spawn(test_recv_loop(c_read));
                let s = tokio::spawn(test_send_file(c_write.clone(), up.clone(), 61_001));
                s.await.unwrap();
                drop(c_write); // 关闭本侧写半部 → 对端 recv EOF
                recv.await.unwrap();
            };
            let side_b = async {
                let recv = tokio::spawn(test_recv_loop(s_read));
                let s = tokio::spawn(test_send_file(s_write.clone(), down.clone(), 62_001));
                s.await.unwrap();
                drop(s_write); // 关闭本侧写半部 → 对端 recv EOF
                recv.await.unwrap();
            };
            tokio::join!(side_a, side_b);
        })
        .await;
        assert!(result.is_ok(), "速率基准超时(60 秒)");
        let elapsed = started.elapsed().as_secs_f64();

        // 双向同时传输 → 每条方向都传了 size 字节,聚合为 2×size
        let total_bytes = (size * 2) as f64;
        let mbps_total = total_bytes / elapsed / 1024.0 / 1024.0;
        let mbps_each = size as f64 / elapsed / 1024.0 / 1024.0;
        println!(
            "[文件传输速率基准] 双向同时: 总耗时 {elapsed:.3}s, 单方向 {mbps_each:.1} MB/s, 聚合 {mbps_total:.1} MB/s (回环 TCP + base64/JSON framing)"
        );
        // 回环下限保守断言,防止极端环境导致基准退化为不可用
        assert!(mbps_total > 1.0, "聚合吞吐异常偏低: {mbps_total:.2} MB/s");

        for f in ["up.bin", "down.bin"] {
            let disk = std::fs::read(incoming.join(f)).unwrap();
            let orig = std::fs::read(src_dir.join(f)).unwrap();
            assert_eq!(disk, orig, "接收文件 {f} 与源内容不一致");
            std::fs::remove_file(incoming.join(f)).ok();
        }
        std::fs::remove_dir_all(&src_dir).ok();
        close_session();
    }
}

