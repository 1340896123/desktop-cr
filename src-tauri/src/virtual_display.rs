//! 虚拟显示器控制模块（IDD Virtual Display）。
//!
//! Windows 平台通过 RustDesk IDD 驱动动态挂载/卸载虚拟显示器（未来阶段实现）；
//! 当前所有平台共享一份内存注册表（MONITORS），保证 add / list / remove 状态一致。

use serde::Serialize;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

/// 虚拟显示器注册表：所有平台共享，保证 add/list/remove 状态一致。
static MONITORS: Mutex<Vec<VirtualMonitor>> = Mutex::new(Vec::new());

/// 虚拟显示器自增 id 分配器。
static NEXT_ID: AtomicU32 = AtomicU32::new(1);

/// 虚拟屏列表变更事件名（前端通过 listen 订阅）。
const MONITORS_CHANGED_EVENT: &str = "virtual-monitors-changed";

#[derive(Debug, Clone, Serialize)]
pub struct VirtualMonitor {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub connected: bool,
}

/// 取当前注册表快照并广播给前端。
fn emit_monitors_changed(app: &AppHandle) -> Result<(), String> {
    let snapshot = MONITORS
        .lock()
        .map_err(|e| format!("failed to lock monitor registry: {e}"))?
        .clone();
    app.emit(MONITORS_CHANGED_EVENT, snapshot)
        .map_err(|e| format!("failed to emit {MONITORS_CHANGED_EVENT}: {e}"))
}

/// 安装虚拟显示器驱动（nefcon / devcon）。
#[tauri::command]
pub fn install_virtual_display_driver(app: AppHandle) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        install_virtual_display_driver_windows(&app)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = &app;
        log::info!("[virtual_display] 非 Windows 平台：模拟驱动安装成功");
        Ok("Virtual display driver installed (simulated on non-Windows platform)".into())
    }
}

#[cfg(target_os = "windows")]
fn install_virtual_display_driver_windows(app: &AppHandle) -> Result<String, String> {
    // 通过 nefcon / devcon 安装 resources/idd_driver 下的驱动。
    // 实际路径由 Tauri 资源解析得到（app.path().resource_dir()）。
    let driver_dir = app
        .path()
        .resource_dir()
        .map_err(|e| format!("failed to resolve resource dir: {e}"))?
        .join("resources")
        .join("idd_driver");

    let nefcon = driver_dir.join("nefcon.exe");
    let devcon = driver_dir.join("devcon.exe");

    let tool = if nefcon.exists() {
        Some((nefcon.as_path(), "nefcon"))
    } else if devcon.exists() {
        Some((devcon.as_path(), "devcon"))
    } else {
        // 驱动安装工具尚未随资源打包（resources/idd_driver 为占位目录），返回模拟成功。
        log::warn!(
            "[virtual_display] 未找到 {}/nefcon.exe 或 devcon.exe，模拟安装成功",
            driver_dir.display()
        );
        return Ok(
            "Virtual display driver installed (simulated: installer binary not bundled)".into(),
        );
    };

    let (tool_path, tool_name) = tool.unwrap();
    let output = std::process::Command::new(tool_path)
        .args(["install", "rustdesk_idd_driver.inf", "idd_driver"])
        .output()
        .map_err(|e| format!("failed to spawn {tool_name}: {e}"))?;

    if output.status.success() {
        Ok("Virtual display driver installed successfully".into())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// 添加一台指定分辨率与刷新率的虚拟显示器，返回其 monitor id。
///
/// 刷新率周期 T_f = 1000 / fps (ms)。fps 会被 clamp 到 1..=144。
#[tauri::command]
pub fn add_virtual_monitor(
    width: u32,
    height: u32,
    fps: u32,
    app: AppHandle,
) -> Result<u32, String> {
    let width = width.max(1);
    let height = height.max(1);
    let fps = fps.clamp(1, 144);

    #[cfg(target_os = "windows")]
    {
        // TODO: 阶段二——通过 RustDesk IDD API 动态创建真实虚拟显示器，
        // 创建成功后仍需插入下方注册表，保证 list 返回一致。
        log::info!("[virtual_display] Windows: add {}x{} @ {}Hz", width, height, fps);
    }

    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let monitor = VirtualMonitor {
        id,
        width,
        height,
        fps,
        connected: true,
    };
    MONITORS
        .lock()
        .map_err(|e| format!("failed to lock monitor registry: {e}"))?
        .push(monitor.clone());

    emit_monitors_changed(&app)?;
    log::info!(
        "[virtual_display] 注册表添加虚拟屏 {}x{} @ {}Hz -> id={id}",
        width,
        height,
        fps
    );
    Ok(id)
}

/// 列出当前已挂载的虚拟显示器（直接读注册表）。
#[tauri::command]
pub fn list_virtual_monitors() -> Result<Vec<VirtualMonitor>, String> {
    let monitors = MONITORS
        .lock()
        .map_err(|e| format!("failed to lock monitor registry: {e}"))?
        .clone();
    log::info!("[virtual_display] 注册表返回 {} 个虚拟屏", monitors.len());
    Ok(monitors)
}

/// 移除指定虚拟显示器。
#[tauri::command]
pub fn remove_virtual_monitor(monitor_id: u32, app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // TODO: 阶段二——通过 RustDesk IDD API 卸载真实虚拟显示器。
        log::info!("[virtual_display] Windows: remove monitor {monitor_id}");
    }

    let mut registry = MONITORS
        .lock()
        .map_err(|e| format!("failed to lock monitor registry: {e}"))?;
    let before = registry.len();
    registry.retain(|m| m.id != monitor_id);
    let removed = registry.len() < before;
    drop(registry);

    if removed {
        log::info!("[virtual_display] 从注册表移除虚拟屏 id={monitor_id}");
        emit_monitors_changed(&app)?;
    } else {
        log::warn!("[virtual_display] 未找到要移除的虚拟屏 id={monitor_id}");
    }
    Ok(())
}
