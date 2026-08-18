//! RustDesk 客户端逻辑封装。
//!
//! 设计目标：复用 RustDesk 的 hbb_common 通信栈（NAT 打洞、hbbs/hbbr 信令、TLS 加密）。
//! 当前阶段为 POC：hbb_common 以 git 依赖形式保留在 Cargo.toml 注释中，
//! 本模块提供跨平台可编译的模拟实现（维护内存中的连接状态）。

use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

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
pub fn connect_to_device(peer_id: String, app: AppHandle) -> Result<ConnectionState, String> {
    {
        let mut inner = state();
        // TODO: 通过 hbb_common 建立 P2P / Relay 连接（阶段一后续 / 阶段二）。
        log::info!("[hbb_client] connect_to_device: {peer_id} (mock)");
        inner.current_peer = Some(peer_id.clone());
    }

    let state = ConnectionState {
        connected: true,
        peer_id: Some(peer_id),
        error: None,
    };
    app.emit("connection-state", &state)
        .map_err(|e| format!("failed to emit connection-state: {e}"))?;
    Ok(state)
}

/// 断开当前连接。
#[tauri::command]
pub fn disconnect_from_device(app: AppHandle) -> Result<(), String> {
    {
        let mut inner = state();
        log::info!("[hbb_client] disconnect_from_device (mock)");
        inner.current_peer = None;
    }

    let state = ConnectionState {
        connected: false,
        peer_id: None,
        error: None,
    };
    app.emit("connection-state", &state)
        .map_err(|e| format!("failed to emit connection-state: {e}"))?;
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

/// 切换全屏（真实 Tauri 窗口操作）。
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
/// Windows 下读取 Unicode 文本；非 Windows 平台返回空串并记录日志。
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
/// Windows 下写入 Unicode 文本；非 Windows 平台仅记录日志。
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

/// 剪贴板双向同步。
///
/// 先读取本地剪贴板文本，再通过 `clipboard-synced` 事件推送给前端
///（POC 中前端可再调用 set_clipboard_text 写回，形成双向演示），返回文本。
#[tauri::command]
pub fn sync_clipboard(app: AppHandle) -> Result<String, String> {
    let text = get_clipboard_text()?;
    app.emit("clipboard-synced", serde_json::json!({ "text": text }))
        .map_err(|e| format!("failed to emit clipboard-synced: {e}"))?;
    Ok(text)
}

#[cfg(target_os = "windows")]
fn clipboard_read_windows() -> Result<String, String> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};

    // 0.58 中 CF_UNICODETEXT 定义于 Win32::System::Ole（CLIPBOARD_FORMAT(13)），
    // 为避免引入庞大的 Ole feature，此处直接使用标准值 13。
    const CF_UNICODETEXT: u32 = 13;

    // 打开系统剪贴板；失败直接返回（无需 CloseClipboard）。
    if unsafe { OpenClipboard(None) }.is_err() {
        return Err("OpenClipboard 失败".into());
    }

    // 内部闭包负责读操作，无论成败外层统一 CloseClipboard。
    let result = (|| {
        // 剪贴板为空或无文本时 GetClipboardData 返回错误/空句柄，按空串处理
        let global = match unsafe { GetClipboardData(CF_UNICODETEXT) } {
            Ok(handle) => handle,
            Err(_) => return Ok(String::new()),
        };
        if global.0.is_null() {
            return Ok(String::new());
        }
        // GetClipboardData 返回 HANDLE，GlobalLock 需要 HGLOBAL，二者同为指针句柄直接转换
        let ptr = unsafe { GlobalLock(HGLOBAL(global.0)) } as *const u16;
        if ptr.is_null() {
            return Err("GlobalLock 失败".into());
        }
        // 扫描 UTF-16 字符串至结尾 null，取出有效长度
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

    // 同 clipboard_read_windows：CF_UNICODETEXT 标准值 13
    const CF_UNICODETEXT: u32 = 13;

    // 打开系统剪贴板；失败直接返回（无需 CloseClipboard）。
    if unsafe { OpenClipboard(None) }.is_err() {
        return Err("OpenClipboard 失败".into());
    }

    // 内部闭包负责写操作，无论成败外层统一 CloseClipboard。
    let result = (|| {
        if unsafe { EmptyClipboard() }.is_err() {
            return Err("EmptyClipboard 失败".into());
        }
        // UTF-16 编码（含结尾 null），分配可移动的全局内存
        let mut units: Vec<u16> = text.encode_utf16().collect();
        units.push(0);
        let hmem = unsafe { GlobalAlloc(GMEM_MOVEABLE, units.len() * 2) }
            .map_err(|e| format!("GlobalAlloc 失败: {e}"))?;
        let ptr = unsafe { GlobalLock(hmem) } as *mut u16;
        if ptr.is_null() {
            // 分配成功但加锁失败：手动释放
            let _ = unsafe { GlobalFree(hmem) };
            return Err("GlobalLock 失败".into());
        }
        unsafe {
            std::ptr::copy_nonoverlapping(units.as_ptr(), ptr, units.len());
        }
        let _ = unsafe { GlobalUnlock(hmem) };
        // 所有权移交给系统，成功后无需 GlobalFree；失败才需要释放。
        // 0.58 中 SetClipboardData 参数为 Param<HANDLE>，HGLOBAL 需转为 HANDLE 传入。
        if unsafe { SetClipboardData(CF_UNICODETEXT, HANDLE(hmem.0)) }.is_err() {
            let _ = unsafe { GlobalFree(hmem) };
            return Err("SetClipboardData 失败".into());
        }
        Ok(())
    })();

    let _ = unsafe { CloseClipboard() };
    result
}
