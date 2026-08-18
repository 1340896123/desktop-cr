//! 虚拟显示器控制模块（IDD Virtual Display）。
//!
//! Windows 平台通过 RustDesk IDD 驱动动态挂载/卸载虚拟显示器；
//! 非 Windows 平台（Linux 开发环境）提供可编译的模拟实现。

use serde::Serialize;
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug, Clone, Serialize)]
pub struct VirtualMonitor {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub connected: bool,
}

/// 安装虚拟显示器驱动（nefcon / devcon）。
#[tauri::command]
pub fn install_virtual_display_driver() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        install_virtual_display_driver_windows()
    }
    #[cfg(not(target_os = "windows"))]
    {
        log::info!("[virtual_display] 非 Windows 平台：模拟驱动安装成功");
        Ok("Virtual display driver installed (simulated on non-Windows platform)".into())
    }
}

#[cfg(target_os = "windows")]
fn install_virtual_display_driver_windows() -> Result<String, String> {
    // 通过 nefcon / devcon 安装 resources/idd_driver 下的驱动。
    // 实际路径由 Tauri 资源解析得到，这里以 devcon 为例。
    let resource_dir = "resources/idd_driver";
    let output = std::process::Command::new("devcon")
        .current_dir(resource_dir)
        .args(["install", "rustdesk_idd_driver.inf", "idd_driver"])
        .output()
        .map_err(|e| format!("failed to spawn devcon: {e}"))?;

    if output.status.success() {
        Ok("Virtual display driver installed successfully".into())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// 添加一台指定分辨率与刷新率的虚拟显示器，返回其 monitor id。
///
/// 刷新率周期 T_f = 1000 / fps (ms)，由 RustDesk IDD API 消费。
#[tauri::command]
pub fn add_virtual_monitor(width: u32, height: u32, fps: u32) -> Result<u32, String> {
    #[cfg(target_os = "windows")]
    {
        add_virtual_monitor_windows(width, height, fps)
    }
    #[cfg(not(target_os = "windows"))]
    {
        static NEXT_ID: AtomicU32 = AtomicU32::new(1);
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        log::info!(
            "[virtual_display] 非 Windows 平台：模拟添加虚拟屏 {}x{} @ {}Hz -> id={id}",
            width,
            height,
            fps
        );
        Ok(id)
    }
}

#[cfg(target_os = "windows")]
fn add_virtual_monitor_windows(width: u32, height: u32, fps: u32) -> Result<u32, String> {
    // TODO: 通过 RustDesk IDD API 动态添加虚拟显示器（阶段二实现）。
    log::info!("[virtual_display] Windows: add {}x{} @ {}Hz", width, height, fps);
    Ok(1)
}

/// 列出当前已挂载的虚拟显示器。
#[tauri::command]
pub fn list_virtual_monitors() -> Result<Vec<VirtualMonitor>, String> {
    #[cfg(target_os = "windows")]
    {
        list_virtual_monitors_windows()
    }
    #[cfg(not(target_os = "windows"))]
    {
        log::info!("[virtual_display] 非 Windows 平台：返回空虚拟屏列表");
        Ok(Vec::new())
    }
}

#[cfg(target_os = "windows")]
fn list_virtual_monitors_windows() -> Result<Vec<VirtualMonitor>, String> {
    // TODO: 枚举 IDD 虚拟显示器（阶段二实现）。
    Ok(Vec::new())
}

/// 移除指定虚拟显示器。
#[tauri::command]
pub fn remove_virtual_monitor(monitor_id: u32) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        remove_virtual_monitor_windows(monitor_id)
    }
    #[cfg(not(target_os = "windows"))]
    {
        log::info!("[virtual_display] 非 Windows 平台：模拟移除虚拟屏 id={monitor_id}");
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn remove_virtual_monitor_windows(monitor_id: u32) -> Result<(), String> {
    // TODO: 通过 RustDesk IDD API 卸载虚拟显示器（阶段二实现）。
    log::info!("[virtual_display] Windows: remove monitor {monitor_id}");
    Ok(())
}
