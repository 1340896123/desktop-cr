//! RustDesk 客户端逻辑封装。
//!
//! 设计目标：复用 RustDesk 的 hbb_common 通信栈（NAT 打洞、hbbs/hbbr 信令、TLS 加密）。
//! 当前阶段为 POC：hbb_common 以 git 依赖形式保留在 Cargo.toml 注释中，
//! 本模块提供跨平台可编译的模拟实现（维护内存中的连接状态）。

use serde::Serialize;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub status: String,
    pub platform: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConnectionState {
    pub connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

struct ClientInner {
    current_peer: Option<String>,
}

static STATE: Mutex<ClientInner> = Mutex::new(ClientInner { current_peer: None });

fn state() -> std::sync::MutexGuard<'static, ClientInner> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// 发现/检索设备列表。
///
/// POC 阶段返回内置示例设备；阶段一后续可接入 hbbs 服务器注册与发现。
#[tauri::command]
pub fn list_devices() -> Vec<DeviceInfo> {
    log::info!("[hbb_client] list_devices (mock)");
    vec![
        DeviceInfo {
            id: "desktop-office".into(),
            name: "Desktop-Office".into(),
            status: "online".into(),
            platform: "windows".into(),
        },
        DeviceInfo {
            id: "nas-server".into(),
            name: "NAS-Server".into(),
            status: "offline".into(),
            platform: "linux".into(),
        },
    ]
}

/// 连接到指定 peer。
#[tauri::command]
pub fn connect_to_device(peer_id: String) -> Result<ConnectionState, String> {
    let mut inner = state();
    // TODO: 通过 hbb_common 建立 P2P / Relay 连接（阶段一后续 / 阶段二）。
    log::info!("[hbb_client] connect_to_device: {peer_id} (mock)");
    inner.current_peer = Some(peer_id.clone());
    Ok(ConnectionState {
        connected: true,
        peer_id: Some(peer_id),
        error: None,
    })
}

/// 断开当前连接。
#[tauri::command]
pub fn disconnect_from_device() -> Result<(), String> {
    let mut inner = state();
    log::info!("[hbb_client] disconnect_from_device (mock)");
    inner.current_peer = None;
    Ok(())
}

/// 获取当前连接状态。
#[tauri::command]
pub fn get_connection_state() -> ConnectionState {
    let inner = state();
    match &inner.current_peer {
        Some(peer) => ConnectionState {
            connected: true,
            peer_id: Some(peer.clone()),
            error: None,
        },
        None => ConnectionState {
            connected: false,
            peer_id: None,
            error: None,
        },
    }
}

/// 设置画面质量（码率 / 帧率 / 画质档位）。
#[tauri::command]
pub fn set_stream_quality(fps: u32, bitrate: Option<u32>, quality: String) -> Result<(), String> {
    log::info!(
        "[hbb_client] set_stream_quality: fps={fps}, bitrate={bitrate:?}, quality={quality} (mock)"
    );
    Ok(())
}

/// 设置流分辨率。
#[tauri::command]
pub fn set_stream_resolution(width: u32, height: u32, fps: u32) -> Result<(), String> {
    log::info!(
        "[hbb_client] set_stream_resolution: {width}x{height} @ {fps}Hz (mock)"
    );
    Ok(())
}

/// 切换全屏。
#[tauri::command]
pub fn set_fullscreen(fullscreen: bool) -> Result<(), String> {
    log::info!("[hbb_client] set_fullscreen: {fullscreen} (mock)");
    Ok(())
}

/// 剪贴板双向同步。
#[tauri::command]
pub fn sync_clipboard() -> Result<(), String> {
    log::info!("[hbb_client] sync_clipboard (mock)");
    Ok(())
}
