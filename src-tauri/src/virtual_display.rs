//! 虚拟显示器控制模块(IDD Virtual Display,基于 Amyuni usbmmidd 驱动)。
//!
//! Windows 真实路径:
//! - 定位资源目录 resources/idd_driver(同时兼容 dev 模式 CARGO_MANIFEST_DIR)。
//! - `deviceinstaller64 install usbmmidd.inf usbmmidd` 安装驱动;
//! - 写注册表 HKLM\...\WUDF\Services\usbmmIdd\Parameters\Monitors(目标分辨率放首位);
//! - `deviceinstaller64 enableidd 1/0` 挂载/卸载虚拟屏(每次一个,后进先出,最多 4 个)。
//! - 可选:优先尝试通过 libloading 动态加载 dylib_virtual_display.dll 的
//!   is_device_created/plug_in_monitor/plug_out_monitor,仅当 is_device_created() 为 true
//!   时使用,失败一律回退 usbmmidd 且不报错。
//! 全部真实操作需要管理员权限;输出含"拒绝访问/错误码 5"时返回明确提示。
//! 非 Windows 平台:安装/添加等操作返回明确错误(不做假成功),枚举返回空列表。

use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[cfg(target_os = "windows")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use tauri::Manager;

#[derive(Debug, Clone, Serialize)]
pub struct VirtualMonitor {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub connected: bool,
}

/// 虚拟屏分辨率对应的注册表值名(如 1920x1080 → "1920x1080")。纯函数无平台依赖。
pub(crate) fn monitor_registry_value(width: u32, height: u32) -> String {
    format!("{width}x{height}")
}

/// 安装虚拟显示器驱动。
#[tauri::command]
pub fn install_virtual_display_driver(app: AppHandle) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        let result = install_virtual_display_driver_windows(&app);
        match &result {
            Ok(msg) => crate::operation_log::op_log(
                "virtual_display",
                "install_driver",
                &format!("成功: {msg}"),
            ),
            Err(e) => crate::operation_log::op_log(
                "virtual_display",
                "install_driver",
                &format!("失败: {e}"),
            ),
        }
        result
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = &app;
        log::info!("[virtual_display] 非 Windows 平台不支持虚拟显示器驱动安装");
        crate::operation_log::op_log(
            "virtual_display",
            "install_driver",
            "失败: 非 Windows 平台不支持虚拟显示器驱动安装",
        );
        Err("非 Windows 平台不支持虚拟显示器驱动安装".into())
    }
}

/// 添加一台指定分辨率与刷新率的虚拟显示器,返回其 monitor id。
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
        let driver_dir = locate_driver_dir(&app)?;
        let current = enumerate_virtual_monitors();
        if current.len() >= 4 {
            let err = "最多 4 个虚拟屏".to_string();
            crate::operation_log::op_log("virtual_display", "add_monitor", &format!("失败: {err}"));
            return Err(err);
        }
        // 写入注册表,保证目标分辨率在列表首位(需要管理员权限)
        write_monitor_resolutions(width, height)?;
        // 新 id = 当前最大 virtual id + 1(无则从 0 开始)
        let new_id = current.iter().map(|m| m.id).max().unwrap_or(0) + 1;

        // 优先尝试 dylib 控制 DLL,失败回退 usbmmidd
        if !try_dylib_plug(&driver_dir, true, new_id) {
            let output = run_installer(&driver_dir, &["enableidd", "1"])?;
            if let Some(msg) = check_admin_error(&output) {
                return Err(msg);
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !output.status.success() {
                return Err(format!(
                    "启用虚拟屏失败(退出码 {:?}): {}",
                    output.status.code(),
                    stdout.trim()
                ));
            }
        }

        emit_monitors_changed(&app)?;
        log::info!(
            "[virtual_display] 新增虚拟屏 {width}x{height} @ {fps}fps -> id={new_id}"
        );
        crate::operation_log::op_log(
            "virtual_display",
            "add_monitor",
            &format!("{width}x{height} @ {fps}fps -> id={new_id}"),
        );
        Ok(new_id)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = &app;
        log::info!("[virtual_display] (非 Windows) 不支持添加虚拟显示器");
        crate::operation_log::op_log(
            "virtual_display",
            "add_monitor",
            &format!("{width}x{height} @ {fps}fps 失败: 非 Windows 平台不支持添加虚拟显示器"),
        );
        Err("非 Windows 平台不支持添加虚拟显示器".into())
    }
}

/// 列出当前已挂载的虚拟显示器(真实枚举 EnumDisplayDevicesW)。
#[tauri::command]
pub fn list_virtual_monitors() -> Result<Vec<VirtualMonitor>, String> {
    #[cfg(target_os = "windows")]
    {
        let monitors = enumerate_virtual_monitors();
        log::info!("[virtual_display] 枚举到 {} 个虚拟屏", monitors.len());
        crate::operation_log::op_log(
            "virtual_display",
            "list_monitors",
            &format!("count={}", monitors.len()),
        );
        Ok(monitors)
    }
    #[cfg(not(target_os = "windows"))]
    {
        log::info!("[virtual_display] (非 Windows) 返回空虚拟屏列表");
        crate::operation_log::op_log(
            "virtual_display",
            "list_monitors",
            "count=0 (非 Windows)",
        );
        Ok(Vec::new())
    }
}

/// 移除指定虚拟显示器(usbmmidd enableidd 0 每次移除一个,后进先出)。
#[tauri::command]
pub fn remove_virtual_monitor(monitor_id: u32, app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let driver_dir = locate_driver_dir(&app)?;
        // 优先尝试 dylib 拔出,失败回退 usbmmidd
        if !try_dylib_plug(&driver_dir, false, monitor_id) {
            let output = run_installer(&driver_dir, &["enableidd", "0"])?;
            if let Some(msg) = check_admin_error(&output) {
                return Err(msg);
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !output.status.success() {
                return Err(format!(
                    "移除虚拟屏失败(退出码 {:?}): {}",
                    output.status.code(),
                    stdout.trim()
                ));
            }
        }
        emit_monitors_changed(&app)?;
        log::info!("[virtual_display] 移除虚拟屏(monitor_id={monitor_id})");
        crate::operation_log::op_log(
            "virtual_display",
            "remove_monitor",
            &format!("id={monitor_id}"),
        );
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = &app;
        log::info!("[virtual_display] (非 Windows) 无虚拟屏可移除,跳过");
        crate::operation_log::op_log(
            "virtual_display",
            "remove_monitor",
            &format!("id={monitor_id} 跳过(非 Windows 平台无虚拟屏)"),
        );
        // 广播空列表,保证前端事件驱动刷新链路在所有平台可用
        emit_monitors_changed(&app)?;
        Ok(())
    }
}

/// 广播虚拟屏列表变更事件(payload 为 VirtualMonitor[] 列表)。
///
/// 所有平台均可调用:Windows 枚举真实虚拟屏后广播;非 Windows 广播空列表。
fn emit_monitors_changed(app: &AppHandle) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let monitors = enumerate_virtual_monitors();
    #[cfg(not(target_os = "windows"))]
    let monitors: Vec<VirtualMonitor> = Vec::new();
    app.emit("virtual-monitors-changed", monitors)
        .map_err(|e| format!("failed to emit virtual-monitors-changed: {e}"))
}

/// 定位 idd_driver 资源目录:优先 app 资源目录,其次 dev 模式源码目录。
#[cfg(target_os = "windows")]
fn locate_driver_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(res) = app.path().resource_dir() {
        candidates.push(res.join("resources").join("idd_driver"));
    }
    // dev 模式:CARGO_MANIFEST_DIR 指向 src-tauri 源码目录
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("idd_driver"),
    );
    for c in candidates {
        if c.join("usbmmIdd.inf").exists() {
            return Ok(c);
        }
    }
    Err("未找到 idd_driver 资源目录(缺少 usbmmIdd.inf),请确认资源已正确打包".into())
}

/// 根据系统位数选择 deviceinstaller 可执行文件名。
#[cfg(target_os = "windows")]
fn installer_exe_name() -> String {
    match std::env::var("PROCESSOR_ARCHITECTURE").as_deref() {
        Ok("AMD64") | Ok("ARM64") | Ok("IA64") => "deviceinstaller64.exe".to_string(),
        _ => "deviceinstaller.exe".to_string(),
    }
}

/// 在驱动目录下运行 deviceinstaller 工具。
#[cfg(target_os = "windows")]
fn run_installer(driver_dir: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    let exe = driver_dir.join(installer_exe_name());
    if !exe.exists() {
        return Err(format!("缺少驱动工具: {}", exe.display()));
    }
    std::process::Command::new(&exe)
        .args(args)
        .current_dir(driver_dir)
        .output()
        .map_err(|e| format!("运行 {} 失败: {e}", exe.display()))
}

/// 解析工具输出,识别"非管理员权限"错误(拒绝访问 / 错误码 5)。
#[cfg(target_os = "windows")]
fn check_admin_error(output: &std::process::Output) -> Option<String> {
    let stdout = String::from_utf8_lossy(&output.stdout).to_lowercase();
    let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
    let combined = format!("{stdout}\n{stderr}");
    if combined.contains("access denied")
        || combined.contains("access is denied")
        || combined.contains("error 5")
        || output.status.code() == Some(5)
    {
        return Some("需要以管理员身份运行".to_string());
    }
    None
}

/// Windows 真实驱动安装:deviceinstaller64 install usbmmidd.inf usbmmidd。
#[cfg(target_os = "windows")]
fn install_virtual_display_driver_windows(app: &AppHandle) -> Result<String, String> {
    let driver_dir = locate_driver_dir(app)?;
    let output = run_installer(&driver_dir, &["install", "usbmmidd.inf", "usbmmidd"])?;
    if let Some(msg) = check_admin_error(&output) {
        return Err(msg);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // usbmmidd 成功输出通常含 "success";退出码 0 且无 "failed" 亦视为成功
    let ok = output.status.success()
        && (stdout.contains("success") || (!stdout.contains("failed") && !stderr.contains("failed")));
    if ok {
        log::info!("[virtual_display] 驱动安装成功: {}", stdout.trim());
        Ok("虚拟显示器驱动安装成功(usbmmidd)".into())
    } else {
        Err(format!(
            "虚拟显示器驱动安装失败(退出码 {:?}): {}\n{}",
            output.status.code(),
            stdout.trim(),
            stderr.trim()
        ))
    }
}

/// 写注册表 HKLM\...\usbmmIdd\Parameters\Monitors,目标分辨率放 "0" 首位。
#[cfg(target_os = "windows")]
fn write_monitor_resolutions(width: u32, height: u32) -> Result<(), String> {
    use windows::Win32::Foundation::ERROR_ACCESS_DENIED;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_WRITE,
        REG_OPTION_NON_VOLATILE, REG_SZ,
    };

    const SUBKEY: &str =
        "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\WUDF\\Services\\usbmmIdd\\Parameters\\Monitors";

    // 打开/创建注册表键
    let subkey_wide: Vec<u16> = SUBKEY.encode_utf16().chain(std::iter::once(0)).collect();
    let mut key: HKEY = HKEY(std::ptr::null_mut());
    let ret = unsafe {
        RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            windows::core::PCWSTR::from_raw(subkey_wide.as_ptr()),
            0,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut key,
            None,
        )
    };
    if ret.0 != 0 {
        if ret.0 == ERROR_ACCESS_DENIED.0 {
            return Err("写入注册表失败: 需要以管理员身份运行".into());
        }
        return Err(format!("打开注册表键失败(错误码 {}): 需要管理员权限", ret.0));
    }

    // 目标分辨率放首位(值名 "0"),其余用 usbmmidd 默认分辨率(共 10 项)
    let defaults = [
        "1024x768",
        "1360x768",
        "1440x900",
        "1600x900",
        "1600x1200",
        "1920x1080",
        "1920x1200",
        "2560x1440",
        "3840x2160",
    ];
    let target = monitor_registry_value(width, height);
    let values: Vec<(String, String)> = std::iter::once(target.clone())
        .chain(defaults.iter().map(|s| s.to_string()).filter(|s| *s != target))
        .take(10)
        .enumerate()
        .map(|(i, v)| (i.to_string(), v))
        .collect();

    let mut failed: Option<String> = None;
    for (name, value) in &values {
        let bytes: Vec<u8> = value
            .encode_utf16()
            .chain(std::iter::once(0))
            .flat_map(|u| u.to_le_bytes())
            .collect();
        let name_wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let ret = unsafe {
            RegSetValueExW(
                key,
                windows::core::PCWSTR::from_raw(name_wide.as_ptr()),
                0,
                REG_SZ,
                Some(&bytes),
            )
        };
        if ret.0 != 0 {
            failed = Some(format!("写入注册表值 {name} 失败(错误码 {})", ret.0));
            break;
        }
    }
    unsafe {
        let _ = RegCloseKey(key);
    }
    if let Some(e) = failed {
        return Err(format!("{e}(可能需要以管理员身份运行)"));
    }
    Ok(())
}

/// 真实枚举当前 usbmmidd 虚拟显示器。
#[cfg(target_os = "windows")]
fn enumerate_virtual_monitors() -> Vec<VirtualMonitor> {
    use windows::core::PCWSTR;
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayDevicesW, EnumDisplaySettingsW, DEVMODEW, DISPLAY_DEVICEW, ENUM_CURRENT_SETTINGS,
    };

    fn wstr(a: &[u16]) -> String {
        let end = a.iter().position(|&c| c == 0).unwrap_or(a.len());
        String::from_utf16_lossy(&a[..end])
    }

    let mut monitors = Vec::new();
    let mut i: u32 = 0;
    loop {
        let mut dd = DISPLAY_DEVICEW::default();
        if !unsafe { EnumDisplayDevicesW(None, i, &mut dd, 0) }.as_bool() {
            break;
        }
        let lower = wstr(&dd.DeviceString).to_lowercase();
        if lower.contains("usbmmidd") || lower.contains("usb mobile monitor") {
            let mut dm = DEVMODEW::default();
            let (mut width, mut height) = (0u32, 0u32);
            if unsafe {
                EnumDisplaySettingsW(
                    PCWSTR::from_raw(dd.DeviceName.as_ptr()),
                    ENUM_CURRENT_SETTINGS,
                    &mut dm,
                )
            }
            .as_bool()
            {
                width = dm.dmPelsWidth;
                height = dm.dmPelsHeight;
            }
            monitors.push(VirtualMonitor {
                id: i,
                width,
                height,
                fps: 60,
                connected: true,
            });
        }
        i += 1;
    }
    monitors
}

/// 尝试通过 dylib_virtual_display.dll 动态挂载/卸载虚拟屏。
///
/// 仅当 is_device_created() 为 true 时使用该 DLL;任何失败/缺失均返回 false,
/// 交由 usbmmidd 回退处理(不报错)。
#[cfg(target_os = "windows")]
fn try_dylib_plug(driver_dir: &Path, plug_in: bool, id: u32) -> bool {
    let dll = driver_dir.join("dylib_virtual_display.dll");
    if !dll.exists() {
        return false;
    }
    let handled = (|| -> Option<bool> {
        let lib = unsafe { libloading::Library::new(&dll) }.ok()?;
        let created: libloading::Symbol<unsafe extern "C" fn() -> i32> =
            unsafe { lib.get(b"is_device_created").ok()? };
        // 未通过 DLL 创建设备则不用它
        if unsafe { created() } == 0 {
            return Some(false);
        }
        let name: &[u8] = if plug_in {
            b"plug_in_monitor"
        } else {
            b"plug_out_monitor"
        };
        let f: libloading::Symbol<unsafe extern "C" fn(u32) -> i32> =
            unsafe { lib.get(name).ok()? };
        let rc = unsafe { f(id) };
        Some(rc == 0)
    })();
    match handled {
        Some(true) => {
            log::info!(
                "[virtual_display] dylib 控制 DLL 处理成功(plug_in={plug_in}, id={id})"
            );
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_registry_value_formats() {
        assert_eq!(monitor_registry_value(1920, 1080), "1920x1080");
        assert_eq!(monitor_registry_value(2560, 1440), "2560x1440");
    }
}

