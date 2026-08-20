//! RustDesk 客户端逻辑封装(真实实现)。
//!
//! 覆盖:
//! - 配置持久化:app_config_dir/config.json(AppConfig / PeerConfig,camelCase)。
//! - 设备列表:从配置读取 peers,并通过 TCP 探测(1 秒超时)判断在线状态;
//!   host_enabled 时追加本机被控端条目。
//! - 连接管理:connect_to_device → network::connect_peer 真实 TCP 连接;
//!   disconnect_from_device → network::close_session。
//! - 流参数:set_stream_quality / set_stream_resolution 写入 `STREAM_CFG`,
//!   被控端抓帧循环实时读取(前端会话内设置即时生效)。
//! - 被控端:start_host / stop_host / is_host_running。
//! - 剪贴板:读取本机剪贴板并经 network::session_send 实时同步到对端。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager};
use tokio::task::JoinHandle;

// base64 的 encode 是 Engine trait 方法,需将 trait 引入作用域
use base64::Engine as _;

/// Tauri 应用标识(与 tauri.conf.json 保持一致,用于兜底配置目录解析)。
const APP_IDENTIFIER: &str = "com.example.winui-remote-desktop";

/// 流分辨率最大边长(等比缩放上限)。
const MAX_STREAM_EDGE: u32 = 1920;

#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub status: String,
    pub platform: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionState {
    pub connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 应用配置(持久化到 app_config_dir/config.json)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub host_enabled: bool,
    pub host_port: u16,
    pub peers: Vec<PeerConfig>,
    /// 退出后保持被控端运行(仅持久化到配置,系统级自启暂未实现)
    #[serde(default)]
    pub keep_running_on_exit: bool,
    /// 直连失败时允许经中继服务器兜底转发
    #[serde(default = "default_relay_fallback_enabled")]
    pub relay_fallback_enabled: bool,
    /// 信令服务器地址("ip:port",可选;配置后被控端注册/心跳,控制端查找/发现)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal_server: Option<String>,
    /// 中继服务器地址("ip:port",可选;直连失败时经其中继转发)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay_server: Option<String>,
    /// 本机唯一 ID(信令注册用,默认 "dcr-<主机名>")
    #[serde(default = "default_host_id")]
    pub host_id: String,
    /// 账号登录会话(可选;登录后解锁应用,未登录为 None)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<AccountSession>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            host_enabled: false,
            host_port: 21118,
            peers: Vec::new(),
            keep_running_on_exit: false,
            relay_fallback_enabled: true,
            // 默认信令/中继服务器(公网 VPS),被控端注册与跨网段连接兜底
            signal_server: Some("120.78.77.248:21116".into()),
            relay_server: Some("120.78.77.248:21117".into()),
            host_id: default_host_id(),
            account: None,
        }
    }
}

/// 中继兜底默认开启(直连失败时经中继转发为当前默认行为)。
fn default_relay_fallback_enabled() -> bool {
    true
}

/// 默认本机 ID:"dcr-<COMPUTERNAME>"。
pub(crate) fn default_host_id() -> String {
    std::env::var("COMPUTERNAME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|s| format!("dcr-{s}"))
        .unwrap_or_else(|| "dcr-host".into())
}

/// 对端设备配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerConfig {
    pub id: String,
    pub name: String,
    pub addr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
}

/// 账号登录会话(登录 dcr-signal 管理服务后持久化,用于解锁应用)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSession {
    /// 服务地址(如 "http://120.78.77.248:21120")。
    pub server: String,
    /// 登录用户名。
    pub username: String,
    /// JWT 令牌。
    pub token: String,
}

/// 流参数配置(被控端抓帧循环实时读取)。
#[derive(Debug, Clone)]
pub(crate) struct StreamConfig {
    pub fps: u32,
    pub jpeg_quality: u8,
    pub target_width: u32,
    pub target_height: u32,
    /// 编码类型:"jpeg"(默认)或 "h264"(FFmpeg 硬件编码可用时由控制端下发)。
    pub codec: String,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            fps: 15,
            jpeg_quality: 70,
            target_width: 1920,
            target_height: 1080,
            codec: "jpeg".into(),
        }
    }
}

/// 配置目录(由 main.rs setup 注册;未注册时走兜底路径)。
static CONFIG_DIR: OnceLock<PathBuf> = OnceLock::new();

/// 被控端任务句柄。
static HOST_TASK: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

/// 流参数(全局共享)。
static STREAM_CFG: Mutex<StreamConfig> = Mutex::new(StreamConfig {
    fps: 15,
    jpeg_quality: 70,
    target_width: 1920,
    target_height: 1080,
    codec: String::new(),
});

/// 注册配置目录(main.rs setup 中调用)。
pub fn register_config_dir(dir: PathBuf) {
    let _ = CONFIG_DIR.set(dir);
}

/// 控制端流编码选择(默认使用 FFmpeg):本机 FFmpeg 可用(可解码 H.264)时下发 h264,
/// 否则 jpeg。H.265 为可选,默认 h264(兼容性与解码开销更优)。
pub(crate) fn stream_codec_choice() -> String {
    #[cfg(target_os = "windows")]
    {
        if crate::ffmpeg_hw::available() {
            return "h264".into();
        }
    }
    "jpeg".into()
}

/// 未协商时的默认编码:本机 FFmpeg 可用则 h264,否则 jpeg。
fn default_codec_choice() -> String {
    #[cfg(target_os = "windows")]
    {
        if crate::ffmpeg_hw::available() {
            return "h264".into();
        }
    }
    "jpeg".into()
}

/// 当前流参数快照(供抓帧 / 推帧循环读取);codec 为空时解析为默认 FFmpeg 编码。
pub(crate) fn stream_cfg() -> StreamConfig {
    let mut cfg = STREAM_CFG.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if cfg.codec.is_empty() {
        cfg.codec = default_codec_choice();
    }
    cfg
}

/// 按最大边长等比缩放(结果四舍五入、最小为 1),保持宽高比;未超限时保持原值。
fn scale_to_limit(width: u32, height: u32, max_edge: u32) -> (u32, u32) {
    let max_dim = width.max(height);
    if max_dim <= max_edge || max_edge == 0 {
        return (width, height);
    }
    let scale = f64::from(max_edge) / f64::from(max_dim);
    (
        ((f64::from(width) * scale).round() as u32).max(1),
        ((f64::from(height) * scale).round() as u32).max(1),
    )
}

/// 应用被控端收到的流参数(控制端经协议下发,见 network::Msg::Stream)。
pub(crate) fn apply_stream_cfg(fps: u32, jpeg_quality: u8, width: u32, height: u32, codec: String) {
    let mut cfg = STREAM_CFG.lock().unwrap_or_else(|e| e.into_inner());
    if fps > 0 {
        cfg.fps = fps.clamp(1, 60);
    }
    if jpeg_quality > 0 {
        cfg.jpeg_quality = jpeg_quality.clamp(1, 100);
    }
    if width > 0 && height > 0 {
        // 宽高同时提供时等比缩放,避免破坏宽高比
        let (w, h) = scale_to_limit(width, height, MAX_STREAM_EDGE);
        cfg.target_width = w;
        cfg.target_height = h;
    } else {
        if width > 0 {
            cfg.target_width = width.clamp(1, MAX_STREAM_EDGE);
        }
        if height > 0 {
            cfg.target_height = height.clamp(1, MAX_STREAM_EDGE);
        }
    }
    // codec 为空 → 默认 FFmpeg(h264 可用时);仅接受 jpeg/h264/hevc
    if codec.is_empty() {
        cfg.codec = default_codec_choice();
    } else if matches!(codec.as_str(), "jpeg" | "h264" | "hevc") {
        cfg.codec = codec;
    } else {
        cfg.codec = "jpeg".into();
    }
    log::info!(
        "[hbb_client] apply_stream_cfg: {}x{} @ {}fps, jpeg_quality={}, codec={}",
        cfg.target_width,
        cfg.target_height,
        cfg.fps,
        cfg.jpeg_quality,
        cfg.codec
    );
    crate::operation_log::op_log(
        "hbb_client",
        "apply_stream_cfg",
        &format!(
            "{}x{} @ {}fps jpeg_quality={} codec={}",
            cfg.target_width, cfg.target_height, cfg.fps, cfg.jpeg_quality, cfg.codec
        ),
    );
}

fn config_file() -> PathBuf {
    let dir = CONFIG_DIR
        .get()
        .cloned()
        .unwrap_or_else(default_config_dir);
    dir.join("config.json")
}

/// 兜底配置目录:未注册时按 Tauri 规则解析 %APPDATA%/<identifier>(Windows)。
fn default_config_dir() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    base.join(APP_IDENTIFIER)
}

pub(crate) fn load_app_config() -> AppConfig {
    let path = config_file();
    match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
            log::warn!("[hbb_client] 配置文件解析失败,使用默认配置: {e}");
            AppConfig::default()
        }),
        Err(_) => AppConfig::default(),
    }
}

pub(crate) fn save_app_config_inner(config: &AppConfig) -> Result<(), String> {
    let path = config_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败: {e}"))?;
    }
    let json = serde_json::to_string_pretty(config).map_err(|e| format!("序列化配置失败: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("写入配置文件失败: {e}"))?;
    log::info!("[hbb_client] 配置已保存到 {}", path.display());
    Ok(())
}

/// 读取应用配置(文件不存在时返回默认配置)。
#[tauri::command]
pub fn get_app_config() -> AppConfig {
    load_app_config()
}

/// 保存应用配置(真实写文件,目录不存在先创建)。
#[tauri::command]
pub fn save_app_config(config: AppConfig) -> Result<(), String> {
    let result = save_app_config_inner(&config);
    crate::operation_log::op_log(
        "hbb_client",
        "save_app_config",
        &format!("host_port={} peers={}", config.host_port, config.peers.len()),
    );
    result
}

/// 发现/检索设备列表(真实:读取配置 peers 并通过 TCP 探测在线状态)。
///
/// 通过 TCP 探测:对每个 peer 用 tokio TcpStream::connect(1 秒超时),
/// 连通 = online,否则 offline;host_enabled 时追加本机被控端条目。
#[tauri::command]
pub async fn list_devices() -> Vec<DeviceInfo> {
    #[cfg(target_os = "windows")]
    {
        let cfg = load_app_config();
        let mut devices = Vec::new();

        // 并发 TCP 探测
        let probes: Vec<JoinHandle<bool>> = cfg
            .peers
            .iter()
            .map(|p| {
                let addr = p.addr.clone();
                tokio::spawn(async move { tcp_probe(&addr).await })
            })
            .collect();
        let mut online = Vec::with_capacity(probes.len());
        for h in probes {
            online.push(h.await.unwrap_or(false));
        }

        for (i, peer) in cfg.peers.iter().enumerate() {
            let status = if online.get(i).copied().unwrap_or(false) {
                "online"
            } else {
                "offline"
            };
            devices.push(DeviceInfo {
                id: peer.id.clone(),
                name: peer.name.clone(),
                status: status.into(),
                platform: peer.platform.clone().unwrap_or_else(|| "unknown".into()),
            });
        }

        // 被控端运行中时追加本机条目
        if cfg.host_enabled {
            let status = if is_host_running() { "online" } else { "idle" };
            devices.push(DeviceInfo {
                id: "local-host".into(),
                name: crate::network::local_id(),
                status: status.into(),
                platform: "windows".into(),
            });
        }

        // 信令服务器发现的在线设备(不在本地配置的按 ID 展示,可经信令/中继连接)
        if let Some(sig) = cfg.signal_server.clone() {
            match crate::network::signal_list(&sig).await {
                Ok(peers) => {
                    // 本机自身已在 local-host 条目展示,需从信令列表剔除(避免重复出现两台设备码)
                    let local_id = if cfg.host_id.trim().is_empty() {
                        default_host_id()
                    } else {
                        cfg.host_id.clone()
                    };
                    // 已登录账号时仅展示本账号设备,避免混入其他账号的在线设备
                    let my_user = cfg.account.as_ref().map(|a| a.username.clone());
                    for p in peers {
                        if p.id == local_id {
                            continue;
                        }
                        if let Some(u) = &my_user {
                            if p.owner != *u {
                                continue;
                            }
                        }
                        if !devices.iter().any(|d| d.id == p.id) {
                            devices.push(DeviceInfo {
                                id: p.id.clone(),
                                name: if p.name.is_empty() {
                                    p.id.clone()
                                } else {
                                    p.name.clone()
                                },
                                status: "online".into(),
                                platform: "signal".into(),
                            });
                        }
                    }
                }
                Err(e) => log::warn!("[hbb_client] 信令设备列表获取失败: {e}"),
            }
        }

        log::info!(
            "[hbb_client] list_devices: 共 {} 个对端(其中 {} 在线)",
            devices.len(),
            devices.iter().filter(|d| d.status == "online").count()
        );
        devices
    }
    #[cfg(not(target_os = "windows"))]
    {
        // 非 Windows:仅编译占位,返回空列表(真实探测/被控端仅 Windows 可用)
        log::info!("[hbb_client] list_devices (非 Windows,返回空列表)");
        Vec::new()
    }
}

/// TCP 连通性探测(1 秒超时)。
#[cfg(target_os = "windows")]
async fn tcp_probe(addr: &str) -> bool {
    matches!(
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            tokio::net::TcpStream::connect(addr)
        )
        .await,
        Ok(Ok(_))
    )
}

/// 连接到指定 peer(真实网络连接)。
///
/// 连接路径回退链(见 `network::open_transport`):
/// 配置的 LAN 直连 → 信令服务器返回的外部地址 → 中继服务器兜底。
/// 若该 peer 不在本地配置(信令发现/仅凭 ID),则从信令服务器查询其地址。
#[tauri::command]
pub async fn connect_to_device(peer_id: String, app: AppHandle) -> Result<ConnectionState, String> {
    // 本机不能连接自己
    if peer_id == "local-host" {
        let err = "本机不能连接自己".to_string();
        crate::operation_log::op_log("hbb_client", "connect_to_device", &format!("失败: {err}"));
        return Err(err);
    }
    let cfg = load_app_config();
    let peer = cfg.peers.iter().find(|p| p.id == peer_id).cloned();

    // 信令服务器查询(可选):拿外部地址与中继提示;信令发现的设备借此获取地址
    let mut direct = peer.as_ref().map(|p| p.addr.clone());
    let mut external: Option<String> = None;
    let mut relay = cfg.relay_server.clone();
    if let Some(sig) = cfg.signal_server.clone() {
        match crate::network::signal_lookup(&sig, &peer_id).await {
            Ok(Some((lan, ext, hint))) => {
                log::info!("[hbb_client] 信令查到 {peer_id}: lan={lan}, external={ext}");
                if direct.is_none() && !lan.is_empty() {
                    direct = Some(lan);
                }
                if !ext.is_empty() {
                    external = Some(ext);
                }
                if relay.is_none() && !hint.is_empty() {
                    relay = Some(hint);
                }
            }
            Ok(None) => log::info!("[hbb_client] 信令显示 {peer_id} 离线"),
            Err(e) => log::warn!("[hbb_client] 信令查询失败: {e}"),
        }
    }
    let direct = direct.ok_or_else(|| {
        let err = format!("未找到对端设备(且信令不可用): {peer_id}");
        crate::operation_log::op_log("hbb_client", "connect_to_device", &format!("失败: {err}"));
        err
    })?;

    // 真实建立连接(直连 → 外部 → 中继);失败时广播断开并返回错误
    match crate::network::connect_peer(app.clone(), peer_id.clone(), direct, external, relay).await {
        Ok(via) => {
            crate::operation_log::op_log(
                "hbb_client",
                "connect_to_device",
                &format!("成功: peer={peer_id}, via={via}"),
            );
        }
        Err(e) => {
            let state = ConnectionState {
                connected: false,
                peer_id: Some(peer_id.clone()),
                error: Some(e.clone()),
            };
            let _ = app.emit("connection-state", &state);
            crate::operation_log::op_log("hbb_client", "connect_to_device", &format!("失败: {e}"));
            return Err(e);
        }
    }

    let state = ConnectionState {
        connected: true,
        peer_id: Some(peer_id.clone()),
        error: None,
    };
    app.emit("connection-state", &state)
        .map_err(|e| format!("failed to emit connection-state: {e}"))?;
    Ok(state)
}

/// 断开当前连接。
#[tauri::command]
pub fn disconnect_from_device(app: AppHandle) -> Result<(), String> {
    let peer = crate::network::session_peer().unwrap_or_default();
    crate::operation_log::op_log("hbb_client", "disconnect_from_device", &format!("peer={peer}"));
    crate::network::close_session();
    let state = ConnectionState {
        connected: false,
        peer_id: None,
        error: None,
    };
    app.emit("connection-state", &state)
        .map_err(|e| format!("failed to emit connection-state: {e}"))?;
    Ok(())
}

/// 获取当前连接状态(读取真实会话状态)。
#[tauri::command]
pub fn get_connection_state() -> ConnectionState {
    match crate::network::session_peer() {
        Some(peer) => ConnectionState {
            connected: true,
            peer_id: Some(peer),
            error: None,
        },
        None => ConnectionState {
            connected: false,
            peer_id: None,
            error: None,
        },
    }
}

/// 画质档位 → (jpeg 质量, 默认帧率):low→(50,10)、medium→(70,20)、high→(85,30)、未知→(70,15)。
pub(crate) fn quality_params(quality: &str) -> (u8, u32) {
    match quality {
        "low" => (50u8, 10u32),
        "medium" => (70u8, 20u32),
        "high" => (85u8, 30u32),
        _ => (70u8, 15u32),
    }
}

/// 设置画面质量(真实生效:写入 STREAM_CFG,被控端抓帧循环实时读取)。
///
/// quality: "low/medium/high" → jpeg 质量 50/70/85,档位默认帧率 10/20/30;
/// fps 为 0 时使用档位默认帧率。
#[tauri::command]
pub async fn set_stream_quality(fps: u32, bitrate: Option<u32>, quality: String) -> Result<(), String> {
    let _ = bitrate; // 协议保留参数(LAN 直连下无需码率控制)
    let (jpeg_quality, default_fps) = quality_params(&quality);
    let fps = if fps == 0 {
        default_fps
    } else {
        fps.clamp(1, 60)
    };
    let (width, height) = {
        let cfg = STREAM_CFG.lock().unwrap_or_else(|e| e.into_inner());
        let mut cfg = cfg.clone();
        cfg.fps = fps;
        cfg.jpeg_quality = jpeg_quality;
        let (w, h) = (cfg.target_width, cfg.target_height);
        *STREAM_CFG.lock().unwrap_or_else(|e| e.into_inner()) = cfg;
        (w, h)
    };
    log::info!(
        "[hbb_client] set_stream_quality: fps={fps}, quality={quality}, jpeg_quality={jpeg_quality}"
    );
    crate::operation_log::op_log(
        "hbb_client",
        "set_stream_quality",
        &format!("fps={fps} quality={quality} jpeg_quality={jpeg_quality}"),
    );
    // 有活跃会话时实时下发到被控端
    if crate::network::session_peer().is_some() {
        let _ = crate::network::session_send(crate::network::Msg::Stream {
            fps,
            jpeg_quality,
            width,
            height,
            monitor: None,
            codec: stream_codec_choice(),
        })
        .await;
    }
    Ok(())
}

/// 设置流分辨率(写入 STREAM_CFG 并实时下发到被控端)。
#[tauri::command]
pub async fn set_stream_resolution(width: u32, height: u32, fps: u32) -> Result<(), String> {
    let (fps, jpeg_quality, scaled_w, scaled_h) = {
        let cfg = STREAM_CFG.lock().unwrap_or_else(|e| e.into_inner());
        let mut cfg = cfg.clone();
        let (w, h) = scale_to_limit(width, height, MAX_STREAM_EDGE);
        cfg.target_width = w;
        cfg.target_height = h;
        if fps > 0 {
            cfg.fps = fps.clamp(1, 60);
        }
        let (f, q) = (cfg.fps, cfg.jpeg_quality);
        *STREAM_CFG.lock().unwrap_or_else(|e| e.into_inner()) = cfg;
        (f, q, w, h)
    };
    log::info!("[hbb_client] set_stream_resolution: {scaled_w}x{scaled_h} @ {fps}fps");
    crate::operation_log::op_log(
        "hbb_client",
        "set_stream_resolution",
        &format!("{scaled_w}x{scaled_h} @ {fps}fps"),
    );
    // 有活跃会话时实时下发到被控端
    if crate::network::session_peer().is_some() {
        let _ = crate::network::session_send(crate::network::Msg::Stream {
            fps,
            jpeg_quality,
            width: scaled_w,
            height: scaled_h,
            monitor: None,
            codec: stream_codec_choice(),
        })
        .await;
    }
    Ok(())
}

/// 切换全屏(真实 Tauri 窗口操作,保持原实现)。
#[tauri::command]
pub fn set_fullscreen(fullscreen: bool, app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "找不到主窗口 (main)".to_string())?;
    let result = window
        .set_fullscreen(fullscreen)
        .map_err(|e| format!("设置全屏失败: {e}"));
    crate::operation_log::op_log(
        "hbb_client",
        "set_fullscreen",
        &format!("fullscreen={fullscreen}"),
    );
    result
}

/// 独立文件传输窗口的对端设备名(主窗口打开窗口时写入,窗口页面读取)。
static TRANSFER_DEVICE_NAME: Mutex<Option<String>> = Mutex::new(None);

/// 打开独立文件传输窗口(单例:已存在时聚焦到前台)。
///
/// 对端设备名写入静态变量,由窗口页面经 `get_transfer_device_name` 读取,
/// 用于远端面板标题展示(避免跨窗口 URL 传参的转义问题)。
///
/// 注意:必须为 async 命令 —— Windows 上 `WebviewWindowBuilder::build()`
/// 在同步 command 中会死锁(Webview2 已知问题,见 tauri WebviewWindowBuilder 文档)。
#[tauri::command]
pub async fn open_file_transfer_window(
    app: AppHandle,
    device_name: Option<String>,
) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("file-transfer") {
        let _ = win.set_focus();
        return Ok(());
    }
    *TRANSFER_DEVICE_NAME
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = device_name;
    tauri::WebviewWindowBuilder::new(
        &app,
        "file-transfer",
        tauri::WebviewUrl::App("transfer.html".into()),
    )
    .title("文件传输")
    .inner_size(1100.0, 720.0)
    .min_inner_size(640.0, 480.0)
    .resizable(true)
    .build()
    .map(|_| ())
    .map_err(|e| format!("打开文件传输窗口失败: {e}"))
}

/// 读取独立文件传输窗口的对端设备名(未设置时返回 None)。
#[tauri::command]
pub fn get_transfer_device_name() -> Option<String> {
    TRANSFER_DEVICE_NAME
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// 读取系统剪贴板文本。
///
/// Windows 下读取 Unicode 文本;非 Windows 平台返回空串并记录日志。
#[tauri::command]
pub fn get_clipboard_text() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        clipboard_read_windows()
    }
    #[cfg(not(target_os = "windows"))]
    {
        log::info!("[hbb_client] get_clipboard_text (非 Windows,返回空串)");
        Ok(String::new())
    }
}

/// 写入系统剪贴板文本。
///
/// Windows 下写入 Unicode 文本;非 Windows 平台仅记录日志。
#[tauri::command]
pub fn set_clipboard_text(text: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        clipboard_write_windows(&text)
    }
    #[cfg(not(target_os = "windows"))]
    {
        log::info!("[hbb_client] set_clipboard_text (非 Windows,忽略)");
        Ok(())
    }
}

/// 剪贴板双向同步:读取本机剪贴板,若有活跃会话则通过 network::session_send
/// 实时发送到对端({"t":"clipboard", text}),再广播 `clipboard-synced` 事件并返回文本。
#[tauri::command]
pub async fn sync_clipboard(app: AppHandle) -> Result<String, String> {
    let text = get_clipboard_text()?;
    // 有活跃会话时真实同步到对端
    if crate::network::session_peer().is_some() {
        let sent = crate::network::session_send(crate::network::OutMsg::Clipboard {
            text: text.clone(),
        })
        .await;
        if !sent {
            log::warn!("[hbb_client] 剪贴板同步失败: 会话已断开");
        }
    }
    app.emit("clipboard-synced", serde_json::json!({ "text": text }))
        .map_err(|e| format!("failed to emit clipboard-synced: {e}"))?;
    crate::operation_log::op_log(
        "hbb_client",
        "sync_clipboard",
        &format!("len={}", text.chars().count()),
    );
    Ok(text)
}

/// 被控端文件接收目录(app_config_dir/incoming)。
pub(crate) fn incoming_dir() -> PathBuf {
    let dir = CONFIG_DIR.get().cloned().unwrap_or_else(default_config_dir);
    dir.join("incoming")
}

/// 请求远程显示器列表(经会话发送 Monitors,应答通过 `remote-monitors` 事件返回)。
#[tauri::command]
pub async fn request_remote_monitors() -> Result<(), String> {
    if crate::network::session_peer().is_none() {
        return Err("无活跃会话".into());
    }
    let sent = crate::network::session_send(crate::network::Msg::Monitors).await;
    if !sent {
        return Err("会话已断开".into());
    }
    Ok(())
}

/// 切换远程会话的目标显示器(下发 Stream.monitor 到被控端,实时切换其抓帧)。
#[tauri::command]
pub async fn select_session_monitor(monitor_id: u32) -> Result<(), String> {
    if crate::network::session_peer().is_none() {
        return Err("无活跃会话".into());
    }
    let cfg = stream_cfg();
    let sent = crate::network::session_send(crate::network::Msg::Stream {
        fps: cfg.fps,
        jpeg_quality: cfg.jpeg_quality,
        width: cfg.target_width,
        height: cfg.target_height,
        monitor: Some(monitor_id),
        codec: stream_codec_choice(),
    })
    .await;
    if !sent {
        return Err("会话已断开".into());
    }
    crate::operation_log::op_log(
        "hbb_client",
        "select_session_monitor",
        &format!("monitor={monitor_id}"),
    );
    Ok(())
}

/// 发送本地文件到对端(经会话文件传输协议;进度通过 `file-progress` 事件上报)。
#[tauri::command]
pub async fn send_file(path: String, app: AppHandle) -> Result<u32, String> {
    if crate::network::session_peer().is_none() {
        return Err("无活跃会话".into());
    }
    static FILE_ID: AtomicU32 = AtomicU32::new(1);
    let id = FILE_ID.fetch_add(1, Ordering::SeqCst);
    if !send_file_with_id(id, path.clone(), app) {
        return Err("读取文件信息失败(路径不存在或不是文件)".into());
    }
    Ok(id)
}

/// 以指定 id 发送文件(被控端响应控制端 FileRequest 时复用其分配的 id,保证两侧任务可对账)。
/// 校验会话与文件存在性;返回是否成功发起。
pub(crate) fn send_file_with_id(id: u32, path: String, app: AppHandle) -> bool {
    if crate::network::session_peer().is_none() {
        return false;
    }
    let meta = match std::fs::metadata(&path) {
        Ok(m) => m,
        Err(e) => {
            log::warn!("[hbb_client] 读取文件信息失败(id={id}): {e}");
            return false;
        }
    };
    if !meta.is_file() {
        return false;
    }
    let size = meta.len();
    let name = std::path::Path::new(&path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "file.bin".into());
    tokio::spawn(async move {
        send_file_task(id, path, name, size, app).await;
    });
    true
}

/// 后台发送文件:FileStart → FileData(64KB 块)→ FileEnd,逐块上报本地进度。
async fn send_file_task(id: u32, path: String, name: String, size: u64, app: AppHandle) {
    use tokio::io::AsyncReadExt;
    if !crate::network::session_send(crate::network::Msg::FileStart {
        id,
        name: name.clone(),
        size,
    })
    .await
    {
        log::warn!("[hbb_client] 文件发送失败: 会话已断开(id={id})");
        return;
    }
    let mut file = match tokio::fs::File::open(&path).await {
        Ok(f) => f,
        Err(e) => {
            log::error!("[hbb_client] 打开文件失败: {e}");
            return;
        }
    };
    const CHUNK: usize = 64 * 1024;
    let mut buf = vec![0u8; CHUNK];
    let mut seq: u64 = 0;
    let mut sent_bytes: u64 = 0;
    loop {
        let n = match file.read(&mut buf).await {
            Ok(n) => n,
            Err(e) => {
                log::error!("[hbb_client] 读取文件失败: {e}");
                return;
            }
        };
        if n == 0 {
            break;
        }
        let chunk = &buf[..n];
        if !crate::network::session_send(crate::network::Msg::FileData {
            id,
            seq,
            data: base64::engine::general_purpose::STANDARD.encode(chunk),
        })
        .await
        {
            log::warn!("[hbb_client] 文件发送中断: 会话已断开(id={id})");
            return;
        }
        sent_bytes += n as u64;
        seq += 1;
        let _ = app.emit(
            "file-progress",
            serde_json::json!({
                "id": id,
                "received": sent_bytes,
                "total": size,
                "name": name,
                "direction": "send",
            }),
        );
    }
    if !crate::network::session_send(crate::network::Msg::FileEnd { id, total_chunks: seq }).await {
        log::warn!("[hbb_client] 文件结束通知失败: 会话已断开(id={id})");
        return;
    }
    log::info!("[hbb_client] 文件发送完成: id={id}, name={name}, size={size}, chunks={seq}");
    crate::operation_log::op_log(
        "hbb_client",
        "send_file",
        &format!("id={id} name={name} size={size}"),
    );
}

/// 列出本机目录内容(文件传输页「我的电脑」面板真实浏览)。
#[tauri::command]
pub fn list_directory(path: String) -> Result<Vec<crate::network::FileEntry>, String> {
    crate::network::list_dir(&path)
}

/// 本机接收目录(对端推送的文件落盘于此,文件传输页「接收」展示用)。
#[tauri::command]
pub fn get_incoming_dir() -> String {
    incoming_dir().to_string_lossy().to_string()
}

/// 请求对端目录列表(控制端 → 被控端,应答经 `remote-directory` 事件返回)。
#[tauri::command]
pub async fn request_remote_dir(path: String) -> Result<(), String> {
    if crate::network::session_peer().is_none() {
        return Err("无活跃会话".into());
    }
    let sent = crate::network::session_send(crate::network::Msg::DirList { path }).await;
    if !sent {
        return Err("会话已断开".into());
    }
    Ok(())
}

/// 请求对端发送指定文件(控制端 → 被控端;id 由控制端分配,对端按此 id 回传)。
#[tauri::command]
pub async fn request_file_pull(id: u32, path: String) -> Result<(), String> {
    if crate::network::session_peer().is_none() {
        return Err("无活跃会话".into());
    }
    let sent = crate::network::session_send(crate::network::Msg::FileRequest { id, path }).await;
    if !sent {
        return Err("会话已断开".into());
    }
    Ok(())
}

/// 会话实时指标(RTT 等,供性能浮窗)。
#[tauri::command]
pub fn get_session_metrics() -> crate::network::SessionMetrics {
    crate::network::get_session_metrics()
}

/// 启动被控端(真实监听 0.0.0.0:port,并自动启动抓帧)。
///
/// 已运行则幂等返回 Ok;端口占用等监听失败返回 Err。
/// 必须为 async:Tauri 异步命令运行在其 Tokio 运行时上,`tokio::spawn` 才有运行时上下文。
#[tauri::command]
pub async fn start_host(port: u16, app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        if is_host_running() {
            log::info!("[hbb_client] host 已在运行,忽略 start_host");
            return Ok(());
        }
        // 同步预绑定,即时报告端口占用等错误
        let std_listener = std::net::TcpListener::bind(("0.0.0.0", port))
            .map_err(|e| format!("监听 0.0.0.0:{port} 失败(端口被占用?): {e}"))?;
        let cfg = stream_cfg();

        // 信令注册信息:服务器地址 / 本机 ID / 局域网地址(供广域网被发现)
        let app_cfg = load_app_config();
        let signal_addr = app_cfg.signal_server.clone();
        let host_id = if app_cfg.host_id.trim().is_empty() {
            default_host_id()
        } else {
            app_cfg.host_id.clone()
        };
        let lan_ip = crate::network::local_ipv4()
            .map(|i| i.to_string())
            .unwrap_or_else(|| "127.0.0.1".into());
        let lan = format!("{lan_ip}:{port}");
        // 设备信息(供管理后台设备档案与注册策略):归属用户取登录账号,未登录为空;
        // 设备名取主机名,系统取 OS 环境变量,版本取编译期 Cargo 版本
        let device_user = app_cfg
            .account
            .as_ref()
            .map(|a| a.username.clone())
            .unwrap_or_default();
        let device_name = std::env::var("COMPUTERNAME")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| default_host_id());
        let device_os = std::env::var("OS")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".into());
        let device_version = env!("CARGO_PKG_VERSION").to_string();

        let handle = tokio::spawn(async move {
            let listener = match tokio::net::TcpListener::from_std(std_listener) {
                Ok(l) => l,
                Err(e) => {
                    log::error!("[hbb_client] 转换监听器失败: {e}");
                    return;
                }
            };
            // 被控端默认抓取 monitor 0(主显示器);扩展多屏支持时可改为选择器
            if let Err(e) = crate::capture::start_capture(
                0,
                cfg.target_width,
                cfg.target_height,
                cfg.fps,
                app.clone(),
            )
            .await
            {
                log::warn!("[hbb_client] host 抓帧启动失败(继续以无帧模式运行): {e}");
            }
            // 音频链路:启动被控端音频采集(系统回环);失败不阻塞 host
            if let Err(e) = crate::audio::start_audio_capture() {
                log::warn!("[hbb_client] host 音频采集启动失败(继续无音频): {e}");
            }
            // 配置了信令服务器则后台注册本机并心跳(随 host 停止而取消)
            let reg_task = if signal_addr.is_some() {
                Some(tokio::spawn(crate::network::signal_register_loop(
                    signal_addr,
                    host_id,
                    lan,
                    device_user,
                    device_name,
                    device_os,
                    device_version,
                )))
            } else {
                None
            };
            if let Err(e) = crate::network::serve_host(app.clone(), listener).await {
                log::error!("[hbb_client] host 服务退出: {e}");
            }
            if let Some(t) = reg_task {
                t.abort();
            }
            // host 退出(含异常返回)后广播停止
            let _ = app.emit("host-state", serde_json::json!({ "running": false, "port": 0 }));
        });
        *HOST_TASK
            .lock()
            .map_err(|e| format!("failed to lock host task: {e}"))? = Some(handle);
        log::info!("[hbb_client] start_host: 监听 0.0.0.0:{port}");
        crate::operation_log::op_log("hbb_client", "start_host", &format!("port={port}"));
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        // 被控端(host)依赖 Windows 专属能力(DXGI 抓屏 / Win32 输入注入 / IDD 虚拟屏),
        // 非 Windows 平台不支持。直接返回错误,避免伪造"运行中"假象(与虚拟屏返回 Err 的
        // 处理方式保持一致)。
        let msg = format!("被控端(host)仅 Windows 平台支持,当前平台不可用(port={port})");
        log::warn!("[hbb_client] start_host 失败: {msg}");
        crate::operation_log::op_log(
            "hbb_client",
            "start_host",
            &format!("失败: 非 Windows 平台不支持 (port={port})"),
        );
        // 广播"未运行"以保持状态一致(host 实际并未启动,避免使用伪造的 running:true)
        let _ = app.emit("host-state", serde_json::json!({ "running": false, "port": 0 }));
        Err(msg)
    }
}

/// 停止被控端(取消任务、停止抓帧并广播)。
#[tauri::command]
pub fn stop_host(app: AppHandle) -> Result<(), String> {
    if let Ok(mut slot) = HOST_TASK.lock() {
        if let Some(handle) = slot.take() {
            handle.abort();
        }
    }
    // host 停止时一并停止抓帧与音频采集(由 start_host 启动)
    let _ = crate::capture::stop_capture();
    crate::audio::stop_audio_capture();
    app.emit("host-state", serde_json::json!({ "running": false, "port": 0 }))
        .map_err(|e| format!("failed to emit host-state: {e}"))?;
    log::info!("[hbb_client] stop_host: 已停止");
    crate::operation_log::op_log("hbb_client", "stop_host", "");
    Ok(())
}

/// 被控端是否运行中。
#[tauri::command]
pub fn is_host_running() -> bool {
    let slot = HOST_TASK.lock().unwrap_or_else(|e| e.into_inner());
    matches!(&*slot, Some(h) if !h.is_finished())
}

#[cfg(target_os = "windows")]
fn clipboard_read_windows() -> Result<String, String> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};

    // 0.58 中 CF_UNICODETEXT 定义于 Win32::System::Ole(CLIPBOARD_FORMAT(13)),
    // 为避免引入庞大的 Ole feature,此处直接使用标准值 13。
    const CF_UNICODETEXT: u32 = 13;

    // 打开系统剪贴板;失败直接返回(无需 CloseClipboard)。
    if unsafe { OpenClipboard(None) }.is_err() {
        return Err("OpenClipboard 失败".into());
    }

    // 内部闭包负责读操作,无论成败外层统一 CloseClipboard。
    let result = (|| {
        // 剪贴板为空或无文本时 GetClipboardData 返回错误/空句柄,按空串处理
        let global = match unsafe { GetClipboardData(CF_UNICODETEXT) } {
            Ok(handle) => handle,
            Err(_) => return Ok(String::new()),
        };
        if global.0.is_null() {
            return Ok(String::new());
        }
        // GetClipboardData 返回 HANDLE,GlobalLock 需要 HGLOBAL,二者同为指针句柄直接转换
        let ptr = unsafe { GlobalLock(HGLOBAL(global.0)) } as *const u16;
        if ptr.is_null() {
            return Err("GlobalLock 失败".into());
        }
        // 扫描 UTF-16 字符串至结尾 null,取出有效长度
        let mut len = 0usize;
        while unsafe { *ptr.add(len) } != 0 {
            len += 1;
        }
        let text = unsafe { String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len)) };
        let _ = unsafe { GlobalUnlock(HGLOBAL(global.0)) };
        Ok(text)
    })();

    let _ = unsafe { CloseClipboard() };
    result
}

#[cfg(target_os = "windows")]
fn clipboard_write_windows(text: &str) -> Result<(), String> {
    use windows::Win32::Foundation::{GlobalFree, HANDLE};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

    // 同 clipboard_read_windows:CF_UNICODETEXT 标准值 13
    const CF_UNICODETEXT: u32 = 13;

    // 打开系统剪贴板;失败直接返回(无需 CloseClipboard)。
    if unsafe { OpenClipboard(None) }.is_err() {
        return Err("OpenClipboard 失败".into());
    }

    // 内部闭包负责写操作,无论成败外层统一 CloseClipboard。
    let result = (|| {
        if unsafe { EmptyClipboard() }.is_err() {
            return Err("EmptyClipboard 失败".into());
        }
        // UTF-16 编码(含结尾 null),分配可移动的全局内存
        let mut units: Vec<u16> = text.encode_utf16().collect();
        units.push(0);
        let hmem = unsafe { GlobalAlloc(GMEM_MOVEABLE, units.len() * 2) }
            .map_err(|e| format!("GlobalAlloc 失败: {e}"))?;
        let ptr = unsafe { GlobalLock(hmem) } as *mut u16;
        if ptr.is_null() {
            // 分配成功但加锁失败:手动释放
            let _ = unsafe { GlobalFree(hmem) };
            return Err("GlobalLock 失败".into());
        }
        unsafe {
            std::ptr::copy_nonoverlapping(units.as_ptr(), ptr, units.len());
        }
        let _ = unsafe { GlobalUnlock(hmem) };
        // 所有权移交给系统,成功后无需 GlobalFree;失败才需要释放。
        // 0.58 中 SetClipboardData 参数为 Param<HANDLE>,HGLOBAL 需转为 HANDLE 传入。
        if unsafe { SetClipboardData(CF_UNICODETEXT, HANDLE(hmem.0)) }.is_err() {
            let _ = unsafe { GlobalFree(hmem) };
            return Err("SetClipboardData 失败".into());
        }
        Ok(())
    })();

    let _ = unsafe { CloseClipboard() };
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_params_mapping() {
        assert_eq!(quality_params("low"), (50u8, 10u32));
        assert_eq!(quality_params("medium"), (70u8, 20u32));
        assert_eq!(quality_params("high"), (85u8, 30u32));
        assert_eq!(quality_params("ultra"), (70u8, 15u32));
    }

    #[test]
    fn apply_stream_cfg_clamps() {
        // 持锁避免与 operation_log 测试并发写同一日志文件
        let _guard = crate::operation_log::test_lock::LOG_WRITE_LOCK.lock().unwrap();
        // 越界输入: fps>60 截到 60、quality>100 截到 100、超大分辨率等比缩到 1920 边
        apply_stream_cfg(99, 200, 4000, 3000, String::new());
        let cfg = stream_cfg();
        assert_eq!(cfg.fps, 60);
        assert_eq!(cfg.jpeg_quality, 100);
        assert_eq!(cfg.target_width, 1920);
        assert_eq!(cfg.target_height, 1440);

        // width/height/fps 为 0 时保持原值不变
        apply_stream_cfg(0, 0, 0, 0, String::new());
        let cfg = stream_cfg();
        assert_eq!(cfg.fps, 60);
        assert_eq!(cfg.jpeg_quality, 100);
        assert_eq!(cfg.target_width, 1920);
        assert_eq!(cfg.target_height, 1440);

        // codec 仅接受 h264/jpeg(空视为 jpeg,非法值归一为 jpeg)
        apply_stream_cfg(0, 0, 0, 0, "h264".into());
        assert_eq!(stream_cfg().codec, "h264");
        apply_stream_cfg(0, 0, 0, 0, "vp8".into());
        assert_eq!(stream_cfg().codec, "jpeg");
    }

    #[test]
    fn app_config_camel_case_json() {
        let cfg = AppConfig {
            host_enabled: true,
            host_port: 21118,
            peers: vec![PeerConfig {
                id: "peer-1".into(),
                name: "本机".into(),
                addr: "192.168.1.5:21118".into(),
                platform: Some("windows".into()),
            }],
            keep_running_on_exit: true,
            relay_fallback_enabled: false,
            signal_server: Some("signal.example.com:21116".into()),
            relay_server: Some("relay.example.com:21117".into()),
            host_id: "dcr-test-pc".into(),
            account: Some(AccountSession {
                server: "http://signal.example.com:21120".into(),
                username: "alice".into(),
                token: "jwt-token".into(),
            }),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        // camelCase 字段名
        assert!(json.contains("\"hostEnabled\""));
        assert!(json.contains("\"hostPort\""));
        assert!(json.contains("\"peers\""));
        assert!(json.contains("\"keepRunningOnExit\""));
        assert!(json.contains("\"relayFallbackEnabled\""));
        assert!(json.contains("\"addr\""));
        assert!(json.contains("\"signalServer\""));
        assert!(json.contains("\"relayServer\""));
        assert!(json.contains("\"hostId\""));
        assert!(json.contains("\"account\""));
        assert!(json.contains("\"username\""));
        assert!(json.contains("\"jwt-token\""));

        // serde roundtrip 后内容一致
        let back: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.host_enabled, cfg.host_enabled);
        assert_eq!(back.host_port, cfg.host_port);
        assert_eq!(back.peers.len(), 1);
        assert_eq!(back.keep_running_on_exit, cfg.keep_running_on_exit);
        assert_eq!(back.relay_fallback_enabled, cfg.relay_fallback_enabled);
        assert_eq!(back.peers[0].addr, "192.168.1.5:21118");
        assert_eq!(back.peers[0].platform.as_deref(), Some("windows"));
        assert_eq!(back.signal_server.as_deref(), Some("signal.example.com:21116"));
        assert_eq!(back.relay_server.as_deref(), Some("relay.example.com:21117"));
        assert_eq!(back.host_id, "dcr-test-pc");
        assert_eq!(back.account.as_ref().unwrap().username, "alice");
        assert_eq!(back.account.as_ref().unwrap().token, "jwt-token");

        // 旧配置(缺新字段)反序列化应回退到默认值
        let old = r#"{"hostEnabled":true,"hostPort":21118,"peers":[]}"#;
        let back: AppConfig = serde_json::from_str(old).unwrap();
        assert!(back.signal_server.is_none());
        assert!(back.relay_server.is_none());
        assert!(back.account.is_none(), "旧配置无 account 字段,应回退为 None");
        assert!(!back.host_id.is_empty(), "host_id 应有默认值");
        assert!(!back.keep_running_on_exit, "旧配置无 keepRunningOnExit,应回退 false");
        assert!(back.relay_fallback_enabled, "旧配置无 relayFallbackEnabled,应回退 true");
    }
}

