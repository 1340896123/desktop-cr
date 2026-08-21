// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod auth;
mod capture;
#[cfg(target_os = "windows")]
mod ffmpeg_hw;
mod hbb_client;
mod input_injector;
mod network;
mod operation_log;
mod tray;
mod virtual_display;

use tauri::Manager;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    tauri::Builder::default()
        .setup(|app| {
            // 注册配置目录(供 hbb_client 持久化 config.json)
            if let Ok(dir) = app.path().app_config_dir() {
                hbb_client::register_config_dir(dir.clone());
                // 注册日志目录(供 operation_log 按日轮转追加操作日志)
                operation_log::register_log_dir(dir);
            }
            // 被控端模式持久化开启时,应用启动即自动拉起 host(host 任务仅在内存,
            // 重启后不自动恢复会导致本机设备永远显示为 idle/灰色)
            let cfg = hbb_client::load_app_config();
            if cfg.host_enabled {
                let handle = app.handle().clone();
                let port = cfg.host_port;
                tauri::async_runtime::spawn(async move {
                    match hbb_client::start_host(port, handle.clone()).await {
                        Ok(()) => log::info!("[main] 启动时自动开启被控端: 端口 {port}"),
                        Err(e) => log::warn!("[main] 启动时自动开启被控端失败: {e}"),
                    }
                });
            }
            // 系统托盘:常驻图标 + 常用菜单;失败仅记日志,不影响主功能
            tray::setup(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // 账号登录
            auth::login_account,
            auth::register_account,
            auth::check_account_token,
            auth::logout_account,
            auth::get_account,
            auth::fetch_server_policy,
            // 应用配置
            hbb_client::get_app_config,
            hbb_client::save_app_config,
            // RustDesk 客户端封装
            hbb_client::list_devices,
            hbb_client::connect_to_device,
            hbb_client::disconnect_from_device,
            hbb_client::get_connection_state,
            hbb_client::set_stream_quality,
            hbb_client::set_stream_resolution,
            hbb_client::set_fullscreen,
            hbb_client::get_clipboard_text,
            hbb_client::set_clipboard_text,
            hbb_client::sync_clipboard,
            // 会话扩展:远程显示器 / 文件传输 / 实时指标
            hbb_client::request_remote_monitors,
            hbb_client::select_session_monitor,
            hbb_client::send_file,
            hbb_client::get_session_metrics,
            hbb_client::list_directory,
            hbb_client::get_incoming_dir,
            hbb_client::request_remote_dir,
            hbb_client::request_file_pull,
            // 独立文件传输窗口
            hbb_client::open_file_transfer_window,
            hbb_client::get_transfer_device_name,
            // 独立远程会话窗口
            hbb_client::open_remote_session_window,
            hbb_client::get_remote_session_info,
            // 被控端管理
            hbb_client::start_host,
            hbb_client::stop_host,
            hbb_client::is_host_running,
            // 虚拟显示器控制
            virtual_display::install_virtual_display_driver,
            virtual_display::add_virtual_monitor,
            virtual_display::list_virtual_monitors,
            virtual_display::remove_virtual_monitor,
            // 鼠标 / 键盘事件注入
            input_injector::inject_mouse_event,
            input_injector::inject_key_event,
            // 音频静音 / 状态查询
            audio::set_audio_muted,
            audio::get_audio_muted,
            // 屏幕抓取
            capture::list_monitors,
            capture::start_capture,
            capture::stop_capture,
            capture::get_frame,
            // 操作日志
            operation_log::get_operation_logs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
