//! 系统托盘:常驻图标 + 常用菜单(显示主窗口 / 允许他人协助 / 文件传输 / 退出)。
//!
//! - 图标复用应用默认窗口图标(`default_window_icon`,已解码 RGBA,无需运行时 PNG 解码);
//! - 「允许他人协助」与设置页同一接线:写 `host_enabled` 配置 + 真实启停被控端,
//!   并监听 `host-state` 事件同步勾选态(设置页/托盘/启动自动拉起任一处切换,两边一致);
//! - 「文件传输」复用独立单例窗口(`open_file_transfer_window`,已存在则聚焦);
//! - 关闭主窗口不退出:拦截 `CloseRequested` 隐藏到托盘(后台被控端继续运行),
//!   真正退出仅经托盘菜单 `app.exit`(不触发 CloseRequested,不会被拦截)。

use std::sync::{LazyLock, Mutex};

use tauri::menu::{CheckMenuItem, MenuBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Listener, Manager, WindowEvent, Wry};

/// 托盘 ID(用于 `tray_by_id` 查找,当前仅一处托盘,常量即可)。
const TRAY_ID: &str = "main";

/// 菜单项 ID:点击后由 `on_menu_event` 分发。
const MENU_SHOW: &str = "show";
const MENU_HOST: &str = "toggle-host";
const MENU_TRANSFER: &str = "file-transfer";
const MENU_QUIT: &str = "quit";

/// 「允许他人协助」菜单项:托盘菜单无法按 id 反查子项,静态持有以便状态联动刷新。
/// `CheckMenuItem` 内部为 Arc + 主线程操作,Send/Sync 由 tauri 保证。
static HOST_ITEM: LazyLock<Mutex<Option<CheckMenuItem<Wry>>>> = LazyLock::new(|| Mutex::new(None));

/// 显示主窗口(已显示则置前;最小化状态先还原)。
fn show_main_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        if win.is_minimized().unwrap_or(false) {
            let _ = win.unminimize();
        }
        let _ = win.show();
        let _ = win.set_focus();
    }
}

/// 更新「允许他人协助」勾选态与托盘 tooltip(host 运行状态变化时调用)。
fn refresh_host_ui(app: &AppHandle, running: bool) {
    let text = if running {
        "允许他人协助(运行中)"
    } else {
        "允许他人协助"
    };
    let tooltip = if running {
        "WinUI Remote Desktop - 被控端运行中"
    } else {
        "WinUI Remote Desktop"
    };
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(tooltip));
    }
    if let Ok(slot) = HOST_ITEM.lock() {
        if let Some(item) = slot.as_ref() {
            let _ = item.set_text(text);
            let _ = item.set_checked(running);
        }
    }
}

/// 「允许他人协助」切换逻辑(与设置页 toggleAllowAssist 同一接线):
/// 持久化 `host_enabled` 并真实启停被控端;启停结果经 `host-state` 事件回传刷新菜单。
fn toggle_host(app: &AppHandle) {
    let mut cfg = crate::hbb_client::load_app_config();
    cfg.host_enabled = !cfg.host_enabled;
    let next_enabled = cfg.host_enabled;
    let port = cfg.host_port;
    if let Err(e) = crate::hbb_client::save_app_config(cfg) {
        log::warn!("[tray] 保存配置失败: {e}");
        return;
    }
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = if next_enabled {
            crate::hbb_client::start_host(port, handle.clone()).await
        } else {
            crate::hbb_client::stop_host(handle.clone()).await
        };
        if let Err(e) = result {
            log::warn!("[tray] 切换被控端失败: {e}");
        }
    });
}

/// 创建托盘图标与菜单。失败仅记录日志不中断启动(托盘缺失不影响主功能)。
pub fn setup(app: &AppHandle) {
    let host_item = match CheckMenuItem::with_id(
        app,
        MENU_HOST,
        "允许他人协助",
        true,
        crate::hbb_client::is_host_running(),
        None::<&str>,
    ) {
        Ok(item) => item,
        Err(e) => {
            log::warn!("[tray] 创建被控端菜单项失败: {e}");
            return;
        }
    };
    *HOST_ITEM.lock().unwrap_or_else(|e| e.into_inner()) = Some(host_item.clone());

    let menu = MenuBuilder::new(app)
        .text(MENU_SHOW, "显示主窗口")
        .item(&host_item)
        .text(MENU_TRANSFER, "文件传输")
        .separator()
        .text(MENU_QUIT, "退出")
        .build();

    let menu = match menu {
        Ok(m) => m,
        Err(e) => {
            log::warn!("[tray] 创建托盘菜单失败: {e}");
            *HOST_ITEM.lock().unwrap_or_else(|e| e.into_inner()) = None;
            return;
        }
    };

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("WinUI Remote Desktop")
        .menu(&menu)
        // Windows 习惯:左键单击显示主窗口,右键弹出菜单
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.0.as_str() {
            MENU_SHOW => show_main_window(app),
            MENU_HOST => toggle_host(app),
            MENU_TRANSFER => {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = crate::hbb_client::open_file_transfer_window(handle, None).await
                    {
                        log::warn!("[tray] 打开文件传输窗口失败: {e}");
                    }
                });
            }
            MENU_QUIT => {
                // 退出前落盘:控制端视角该进程消失表现为连接被重置(os error 10054)
                crate::operation_log::op_log(
                    "tray",
                    "app_exit",
                    "经托盘菜单退出进程,所有会话连接随进程关闭(对端将收到连接断开/重置)",
                );
                app.exit(0)
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    if let Err(e) = builder.build(app) {
        log::warn!("[tray] 创建托盘图标失败: {e}");
        *HOST_ITEM.lock().unwrap_or_else(|e| e.into_inner()) = None;
        return;
    }

    // 被控端状态联动:设置页/托盘/启动自动拉起任一处启停 host,托盘勾选态与 tooltip 同步刷新
    let listener = app.clone();
    app.listen("host-state", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let running = payload
                .get("running")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            refresh_host_ui(&listener, running);
        }
    });

    // 关闭主窗口 = 隐藏到托盘(保留后台被控端),真正退出走托盘菜单
    if let Some(win) = app.get_webview_window("main") {
        let hidden = win.clone();
        win.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = hidden.hide();
            }
        });
    }

    // 托盘就绪后按当前 host 状态初始化一次 tooltip/文案
    refresh_host_ui(app, crate::hbb_client::is_host_running());
    crate::operation_log::op_log("tray", "setup", "");
}
