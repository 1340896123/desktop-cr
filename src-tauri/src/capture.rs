//! 屏幕抓取模块（DXGI / Scrap）。
//!
//! Windows 平台通过 RustDesk scrap 复用 DXGI 高性能抓屏；
//! 非 Windows 平台返回 NotSupported 或模拟帧，保证工程可编译。

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    pub format: String,
    /// RGBA 原始像素数据（POC 阶段通常为空）
    pub data: Vec<u8>,
}

static CAPTURE_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 开始对指定显示器抓帧。
#[tauri::command]
pub fn start_capture(monitor_id: u32) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        start_capture_windows(monitor_id)
    }
    #[cfg(not(target_os = "windows"))]
    {
        log::info!("[capture] 非 Windows 平台：模拟开始抓取 monitor {monitor_id}");
        CAPTURE_ACTIVE.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn start_capture_windows(monitor_id: u32) -> Result<(), String> {
    // TODO: 复用 scrap / DXGI 建立抓帧循环（阶段三实现）。
    log::info!("[capture] Windows: start capture on monitor {monitor_id}");
    CAPTURE_ACTIVE.store(true, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

/// 停止抓帧。
#[tauri::command]
pub fn stop_capture() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        stop_capture_windows()
    }
    #[cfg(not(target_os = "windows"))]
    {
        log::info!("[capture] 非 Windows 平台：模拟停止抓帧");
        CAPTURE_ACTIVE.store(false, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn stop_capture_windows() -> Result<(), String> {
    // TODO: 停止抓帧循环并释放 DXGI 资源（阶段三实现）。
    CAPTURE_ACTIVE.store(false, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

/// 取回最新一帧。
#[tauri::command]
pub fn get_frame(monitor_id: u32) -> Result<CapturedFrame, String> {
    #[cfg(target_os = "windows")]
    {
        get_frame_windows(monitor_id)
    }
    #[cfg(not(target_os = "windows"))]
    {
        if !CAPTURE_ACTIVE.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("capture not started".into());
        }
        log::info!("[capture] 非 Windows 平台：返回模拟空白帧 (monitor {monitor_id})");
        Ok(CapturedFrame {
            width: 1920,
            height: 1080,
            format: "rgba8".into(),
            data: Vec::new(),
        })
    }
}

#[cfg(target_os = "windows")]
fn get_frame_windows(monitor_id: u32) -> Result<CapturedFrame, String> {
    // TODO: 从抓帧循环的最新帧缓冲区取帧（阶段三实现）。
    log::info!("[capture] Windows: get frame on monitor {monitor_id}");
    Ok(CapturedFrame {
        width: 1920,
        height: 1080,
        format: "rgba8".into(),
        data: Vec::new(),
    })
}
