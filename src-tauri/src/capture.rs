//! 屏幕抓取模块（DXGI / Scrap）。
//!
//! 真实 DXGI 抓屏为未来阶段工作（TODO），当前所有平台共享一套模拟实现：
//! tokio 异步循环程序化生成 RGBA 动画帧，通过 `capture-frame` 事件推送给前端，
//! 并保存最新帧供 `get_frame` 拉取。

use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use tokio::task::JoinHandle;

/// 模拟帧最大尺寸（控制 IPC 载荷大小）。
const MAX_FRAME_WIDTH: u32 = 480;
const MAX_FRAME_HEIGHT: u32 = 270;

/// 模拟抓帧循环允许的帧率范围。
const MIN_FPS: u32 = 1;
const MAX_FPS: u32 = 30;

#[derive(Debug, Clone, Serialize)]
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    pub format: String,
    /// RGBA 原始像素数据
    pub data: Vec<u8>,
}

/// 推送给前端的抓帧事件负载。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedFrameEvent {
    pub monitor_id: u32,
    pub width: u32,
    pub height: u32,
    /// RGBA 原始像素
    pub rgba: Vec<u8>,
}

/// 最新帧快照（monitor_id 维度）：未开始抓帧时为 None。
static LATEST_FRAME: Mutex<Option<CapturedFrame>> = Mutex::new(None);

/// 当前抓帧循环任务句柄：用于 stop_capture 取消循环。
static CAPTURE_TASK: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

/// 程序化生成一帧 RGBA 动画：
/// - 随时间变化的渐变底色
/// - 左右往返的移动亮带
/// - 静态棋盘格叠加
/// - 斜向往返移动的白色"光标"方块
fn generate_mock_frame(t: u32, width: u32, height: u32) -> Vec<u8> {
    // 取模限制动画周期，避免长时间运行后 t 溢出（debug 下 panic）
    let t = t % 60_000;
    let w = width.max(1);
    let h = height.max(1);
    let mut data = vec![0u8; (w * h * 4) as usize];

    let cell: u32 = 16;
    // 移动亮带：在宽度方向上往返
    let band_w = (w / 8).max(1);
    let band_x = ((t % (w + band_w)) as u32).min(w - 1);
    let band_end = (band_x + band_w).min(w);
    // 光标方块：斜向往返
    let cw = (w / 32).clamp(4, 16);
    let ch = (h / 32).clamp(4, 16);
    let cx = (t * 5 % (w + cw)) as u32;
    let cx = if cx < w { cx } else { w + cw - cx - 1 };
    let cy = (t * 7 % (h + ch)) as u32;
    let cy = if cy < h { cy } else { h + ch - cy - 1 };

    for y in 0..h {
        for x in 0..w {
            let idx = ((y * w + x) * 4) as usize;
            // 渐变底色随时间滚动
            let r = ((x * 255 / w) + t) % 256;
            let g = ((y * 255 / h) + t * 2) % 256;
            let b = ((x / 4 + y / 4) + t * 3) % 256;
            let mut pixel = [r as u8, g as u8, b as u8, 255];

            // 棋盘格叠加
            if ((x / cell) + (y / cell)) % 2 == 1 {
                pixel[0] = pixel[0].saturating_add(30);
                pixel[1] = pixel[1].saturating_add(30);
                pixel[2] = pixel[2].saturating_add(30);
            }
            // 移动亮带
            if x >= band_x && x < band_end {
                pixel[0] = 255;
                pixel[1] = pixel[1].saturating_add(40);
                pixel[2] = pixel[2].saturating_add(40);
            }
            // 光标方块（白色）
            if x >= cx && x < cx + cw && y >= cy && y < cy + ch {
                pixel = [255, 255, 255, 255];
            }

            data[idx..idx + 4].copy_from_slice(&pixel);
        }
    }
    data
}

/// 开始对指定显示器抓帧（模拟循环）。
///
/// width/height/fps 仅作为"请求尺寸/帧率"参考，实际生成帧使用 clamp 后尺寸。
#[tauri::command]
pub fn start_capture(
    monitor_id: u32,
    width: u32,
    height: u32,
    fps: u32,
    app: AppHandle,
) -> Result<(), String> {
    // TODO: 阶段三——替换为真实的 DXGI / scrap 抓帧循环。
    let w = width.clamp(1, MAX_FRAME_WIDTH);
    let h = height.clamp(1, MAX_FRAME_HEIGHT);
    let fps = fps.clamp(MIN_FPS, MAX_FPS);
    let interval_ms = (1000 / fps) as u64;

    // 幂等：先停止旧循环再启动新循环
    stop_capture_inner();

    let handle = tokio::spawn(async move {
        let mut frame_idx: u32 = 0;
        let mut timer = tokio::time::interval(std::time::Duration::from_millis(interval_ms));
        loop {
            timer.tick().await;

            let rgba = generate_mock_frame(frame_idx, w, h);
            let payload = CapturedFrameEvent {
                monitor_id,
                width: w,
                height: h,
                rgba: rgba.clone(),
            };
            let _ = app.emit("capture-frame", &payload);

            let snapshot = CapturedFrame {
                width: w,
                height: h,
                format: "rgba8".into(),
                data: rgba,
            };
            if let Ok(mut slot) = LATEST_FRAME.lock() {
                *slot = Some(snapshot);
            }

            frame_idx = frame_idx.wrapping_add(1);
        }
    });

    *CAPTURE_TASK
        .lock()
        .map_err(|e| format!("failed to lock capture task: {e}"))? = Some(handle);

    log::info!(
        "[capture] 模拟抓帧循环启动：monitor {monitor_id}, {}x{} @ {fps}fps (间隔 {interval_ms}ms)",
        w,
        h
    );
    Ok(())
}

/// 停止抓帧（取消循环任务并释放资源），幂等。
#[tauri::command]
pub fn stop_capture() -> Result<(), String> {
    stop_capture_inner();
    log::info!("[capture] 停止模拟抓帧循环");
    Ok(())
}

fn stop_capture_inner() {
    if let Ok(mut slot) = CAPTURE_TASK.lock() {
        if let Some(handle) = slot.take() {
            handle.abort();
        }
    }
    if let Ok(mut slot) = LATEST_FRAME.lock() {
        *slot = None;
    }
}

/// 取回最新一帧。
#[tauri::command]
pub fn get_frame(monitor_id: u32) -> Result<CapturedFrame, String> {
    let slot = LATEST_FRAME
        .lock()
        .map_err(|e| format!("failed to lock latest frame: {e}"))?;
    match slot.as_ref() {
        Some(frame) => {
            log::info!("[capture] 返回最新帧 (monitor {monitor_id})：{}x{}", frame.width, frame.height);
            Ok(frame.clone())
        }
        None => Err(format!("capture not started (monitor {monitor_id})")),
    }
}
