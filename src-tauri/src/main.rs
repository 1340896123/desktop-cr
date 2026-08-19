// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod hbb_client;
mod input_injector;
mod media_pipeline;
mod network;
mod operation_log;
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
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
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
            // 屏幕抓取
            capture::list_monitors,
            capture::start_capture,
            capture::stop_capture,
            capture::get_frame,
            // 音视频全链路测试
            media_pipeline::run_media_pipeline_test,
            // 操作日志
            operation_log::get_operation_logs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
