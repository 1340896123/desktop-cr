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
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager};
use tokio::task::JoinHandle;

/// Tauri 应用标识(与 tauri.conf.json 保持一致,用于兜底配置目录解析)。
const APP_IDENTIFIER: &str = "com.example.winui-remote-desktop";

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
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            host_enabled: false,
            host_port: 21118,
            peers: Vec::new(),
        }
    }
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

/// 流参数配置(被控端抓帧循环实时读取)。
#[derive(Debug, Clone)]
pub(crate) struct StreamConfig {
    pub fps: u32,
    pub jpeg_quality: u8,
    pub target_width: u32,
    pub target_height: u32,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            fps: 15,
            jpeg_quality: 70,
            target_width: 1920,
            target_height: 1080,
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
});

/// 注册配置目录(main.rs setup 中调用)。
pub fn register_config_dir(dir: PathBuf) {
    let _ = CONFIG_DIR.set(dir);
}

/// 当前流参数快照(供抓帧 / 推帧循环读取)。
pub(crate) fn stream_cfg() -> StreamConfig {
    STREAM_CFG.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// 应用被控端收到的流参数(控制端经协议下发,见 network::Msg::Stream)。
pub(crate) fn apply_stream_cfg(fps: u32, jpeg_quality: u8, width: u32, height: u32) {
    let mut cfg = STREAM_CFG.lock().unwrap_or_else(|e| e.into_inner());
    if fps > 0 {
        cfg.fps = fps.clamp(1, 30);
    }
    if jpeg_quality > 0 {
        cfg.jpeg_quality = jpeg_quality.clamp(1, 100);
    }
    if width > 0 {
        cfg.target_width = width.clamp(1, 1920);
    }
    if height > 0 {
        cfg.target_height = height.clamp(1, 1920);
    }
    log::info!(
        "[hbb_client] apply_stream_cfg: {}x{} @ {}fps, jpeg_quality={}",
        cfg.target_width,
        cfg.target_height,
        cfg.fps,
        cfg.jpeg_quality
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

fn load_app_config() -> AppConfig {
    let path = config_file();
    match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
            log::warn!("[hbb_client] 配置文件解析失败,使用默认配置: {e}");
            AppConfig::default()
        }),
        Err(_) => AppConfig::default(),
    }
}

fn save_app_config_inner(config: &AppConfig) -> Result<(), String> {
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
    save_app_config_inner(&config)
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
                name: "本机(被控端)".into(),
                status: status.into(),
                platform: "windows".into(),
            });
        }
        log::info!(
            "[hbb_client] list_devices: 通过 TCP 探测 {} 个对端(其中 {} 在线)",
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
#[tauri::command]
pub async fn connect_to_device(peer_id: String, app: AppHandle) -> Result<ConnectionState, String> {
    // 本机不能连接自己
    if peer_id == "local-host" {
        return Err("本机不能连接自己".to_string());
    }
    // 从配置查找对端地址
    let cfg = load_app_config();
    let peer = cfg
        .peers
        .iter()
        .find(|p| p.id == peer_id)
        .cloned()
        .ok_or_else(|| format!("未找到对端设备: {peer_id}"))?;

    // 真实建立连接;失败时广播断开并返回错误
    if let Err(e) = crate::network::connect_peer(app.clone(), peer_id.clone(), peer.addr.clone()).await
    {
        let state = ConnectionState {
            connected: false,
            peer_id: Some(peer_id.clone()),
            error: Some(e.clone()),
        };
        let _ = app.emit("connection-state", &state);
        return Err(e);
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

/// 设置画面质量(真实生效:写入 STREAM_CFG,被控端抓帧循环实时读取)。
///
/// quality: "low/medium/high" → jpeg 质量 50/70/85,档位默认帧率 10/20/30;
/// fps 为 0 时使用档位默认帧率。
#[tauri::command]
pub async fn set_stream_quality(fps: u32, bitrate: Option<u32>, quality: String) -> Result<(), String> {
    let _ = bitrate; // 协议保留参数(LAN 直连下无需码率控制)
    let (jpeg_quality, default_fps) = match quality.as_str() {
        "low" => (50u8, 10u32),
        "medium" => (70u8, 20u32),
        "high" => (85u8, 30u32),
        _ => (70u8, 15u32),
    };
    let fps = if fps == 0 {
        default_fps
    } else {
        fps.clamp(1, 30)
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
    // 有活跃会话时实时下发到被控端
    if crate::network::session_peer().is_some() {
        let _ = crate::network::session_send(crate::network::Msg::Stream {
            fps,
            jpeg_quality,
            width,
            height,
        })
        .await;
    }
    Ok(())
}

/// 设置流分辨率(写入 STREAM_CFG 并实时下发到被控端)。
#[tauri::command]
pub async fn set_stream_resolution(width: u32, height: u32, fps: u32) -> Result<(), String> {
    let (fps, jpeg_quality) = {
        let cfg = STREAM_CFG.lock().unwrap_or_else(|e| e.into_inner());
        let mut cfg = cfg.clone();
        cfg.target_width = width.clamp(1, 1920);
        cfg.target_height = height.clamp(1, 1920);
        if fps > 0 {
            cfg.fps = fps.clamp(1, 30);
        }
        let (f, q) = (cfg.fps, cfg.jpeg_quality);
        *STREAM_CFG.lock().unwrap_or_else(|e| e.into_inner()) = cfg;
        (f, q)
    };
    log::info!("[hbb_client] set_stream_resolution: {width}x{height} @ {fps}fps");
    // 有活跃会话时实时下发到被控端
    if crate::network::session_peer().is_some() {
        let _ = crate::network::session_send(crate::network::Msg::Stream {
            fps,
            jpeg_quality,
            width: width.clamp(1, 1920),
            height: height.clamp(1, 1920),
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
    window
        .set_fullscreen(fullscreen)
        .map_err(|e| format!("设置全屏失败: {e}"))
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
    Ok(text)
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
            ) {
                log::warn!("[hbb_client] host 抓帧启动失败(继续以无帧模式运行): {e}");
            }
            if let Err(e) = crate::network::serve_host(app.clone(), listener).await {
                log::error!("[hbb_client] host 服务退出: {e}");
            }
            // host 退出(含异常返回)后广播停止
            let _ = app.emit("host-state", serde_json::json!({ "running": false, "port": 0 }));
        });
        *HOST_TASK
            .lock()
            .map_err(|e| format!("failed to lock host task: {e}"))? = Some(handle);
        log::info!("[hbb_client] start_host: 监听 0.0.0.0:{port}");
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        // 非 Windows:仅编译占位(host 仅 Windows 可用)
        log::info!("[hbb_client] start_host (非 Windows,模拟成功)");
        let _ = app.emit("host-state", serde_json::json!({ "running": true, "port": port }));
        Ok(())
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
    // host 停止时一并停止抓帧(该抓帧由 start_host 启动)
    let _ = crate::capture::stop_capture();
    app.emit("host-state", serde_json::json!({ "running": false, "port": 0 }))
        .map_err(|e| format!("failed to emit host-state: {e}"))?;
    log::info!("[hbb_client] stop_host: 已停止");
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
