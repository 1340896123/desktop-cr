//! 屏幕抓取模块(DXGI 桌面复制 + 纯 Rust JPEG 编码)。
//!
//! Windows 真实路径:
//!   CreateDXGIFactory1 → EnumAdapters1(0) → EnumOutputs(monitor_id)
//!   → D3D11CreateDevice → IDXGIOutput::DuplicateOutput 桌面复制
//!   → 每帧 AcquireNextFrame 拿到 ID3D11Texture2D → 拷贝到 CPU 可读 staging 纹理
//!   → Map/Unmap 读出 BGRA → 转 RGB → 按目标尺寸等比缩放(双线性,不放大)
//!   → jpeg-encoder 编码为 JPEG → 经 `capture-frame` 事件推送,并缓存最新帧供 `get_frame`/`latest_frame` 拉取。
//! 目标尺寸/帧率/画质实时读取 `crate::hbb_client::stream_cfg()`(set_stream_* 命令即时生效)。
//! 非 Windows 平台保留程序化动画帧(仅编译占位,保证跨平台可编译),并以
//! `simulated: true` 标记,前端据此与真实抓帧区分。

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use tokio::task::JoinHandle;

/// 显示器信息(供多屏选择 / IDD 虚拟屏识别)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    pub id: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
    pub is_virtual: bool,
}

/// `get_frame` 返回的帧结构:真实抓帧后为 JPEG 编码数据。
#[derive(Debug, Clone, Serialize)]
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    pub format: String,
    /// JPEG 编码数据(真实实现)或 RGBA 原始像素(非 Windows 模拟)
    pub data: Vec<u8>,
}

/// 推送给前端的抓帧事件负载(字段以 camelCase 序列化)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedFrameEvent {
    pub monitor_id: u32,
    pub width: u32,
    pub height: u32,
    /// JPEG 编码数据
    pub jpeg: Vec<u8>,
    /// 是否为模拟画面(非 Windows 平台动画帧;真实 DXGI 抓帧为 false)
    pub simulated: bool,
}

/// 最新帧快照:未开始抓帧时为 None。
static LATEST_FRAME: Mutex<Option<CapturedFrame>> = Mutex::new(None);

/// 最新 FFmpeg 视频帧(H.264/H.265 Annex-B,宽, 高, 字节, 是否关键帧)。仅供远端会话推帧。
static LATEST_VIDEO: Mutex<Option<(u32, u32, Vec<u8>, bool)>> = Mutex::new(None);

/// 最近一帧 JPEG 编码耗时(毫秒,供远程性能统计)。
static LATEST_ENCODE_DUR: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// 最近一帧 FFmpeg 视频编码耗时(毫秒,供远程性能统计)。
static LATEST_VIDEO_DUR: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// 当前抓帧循环任务句柄:用于 stop_capture 取消循环。
static CAPTURE_TASK: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

/// 枚举系统中所有显示器(真实 EnumDisplayDevicesW / EnumDisplaySettingsW)。
///
/// `is_virtual` 判断:DeviceString 含 "usbmmidd" / "USB Mobile Monitor" 即视为 IDD 虚拟屏。
#[tauri::command]
pub fn list_monitors(_app: AppHandle) -> Result<Vec<MonitorInfo>, String> {
    #[cfg(target_os = "windows")]
    {
        list_monitors_windows()
    }
    #[cfg(not(target_os = "windows"))]
    {
        log::info!("[capture] list_monitors (非 Windows,返回空列表)");
        crate::operation_log::op_log("capture", "list_monitors", "count=0 (非 Windows 模拟)");
        Ok(Vec::new())
    }
}

/// 开始对指定显示器抓帧(真实 DXGI 桌面复制)。
///
/// width/height/fps 参数保留契约(前端可直接调用);真实抓帧时目标尺寸/帧率/画质
/// 以 `crate::hbb_client::stream_cfg()` 为准,从而支持会话内 set_stream_* 实时调整。
/// 必须为 async:内部 `tokio::spawn` 需要 Tokio 运行时上下文(Tauri 异步命令提供)。
#[tauri::command]
pub async fn start_capture(
    monitor_id: u32,
    width: u32,
    height: u32,
    fps: u32,
    app: AppHandle,
) -> Result<(), String> {
    // 幂等:先停止旧循环再启动新循环
    stop_capture_inner();

    #[cfg(target_os = "windows")]
    {
        // 参数仅作契约保留,真实参数读取流配置(stream_cfg)
        let _ = (width, height, fps);
        let handle = tokio::spawn(async move {
            if let Err(e) = dxgi_capture_loop(app.clone(), monitor_id).await {
                log::error!("[capture] DXGI 抓屏循环退出: {e}");
            }
        });
        *CAPTURE_TASK
            .lock()
            .map_err(|e| format!("failed to lock capture task: {e}"))? = Some(handle);
        log::info!("[capture] DXGI 抓屏启动: monitor {monitor_id}");
        crate::operation_log::op_log(
            "capture",
            "start_capture",
            &format!("monitor={monitor_id} {width}x{height} @ {fps}fps"),
        );
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        let handle = tokio::spawn(async move {
            mock_capture_loop(app.clone(), monitor_id, width, height, fps).await
        });
        *CAPTURE_TASK
            .lock()
            .map_err(|e| format!("failed to lock capture task: {e}"))? = Some(handle);
        log::info!(
            "[capture] 模拟抓屏启动(非 Windows): monitor {monitor_id}, {width}x{height} @ {fps}fps"
        );
        crate::operation_log::op_log(
            "capture",
            "start_capture",
            &format!("monitor={monitor_id} {width}x{height} @ {fps}fps (非 Windows 模拟)"),
        );
        Ok(())
    }
}

/// 停止抓帧(取消循环任务并释放资源),幂等。
#[tauri::command]
pub fn stop_capture() -> Result<(), String> {
    stop_capture_inner();
    log::info!("[capture] 停止抓屏循环");
    crate::operation_log::op_log("capture", "stop_capture", "");
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
    if let Ok(mut slot) = LATEST_VIDEO.lock() {
        *slot = None;
    }
}

/// 取回最新一帧(真实实现 format 为 "jpeg")。
#[tauri::command]
pub fn get_frame(monitor_id: u32) -> Result<CapturedFrame, String> {
    let slot = LATEST_FRAME
        .lock()
        .map_err(|e| format!("failed to lock latest frame: {e}"))?;
    match slot.as_ref() {
        Some(frame) => {
            log::info!(
                "[capture] 返回最新帧 (monitor {monitor_id})：{}x{} format={}",
                frame.width,
                frame.height,
                frame.format
            );
            Ok(frame.clone())
        }
        None => Err(format!("capture not started (monitor {monitor_id})")),
    }
}

/// 供 host / 网络层拉取最新一帧:返回 (width, height, jpeg 字节);无帧时返回 None。
pub fn latest_frame() -> Option<(u32, u32, Vec<u8>)> {
    let slot = LATEST_FRAME.lock().ok()?;
    let frame = slot.as_ref()?;
    Some((frame.width, frame.height, frame.data.clone()))
}

/// 供 host / 网络层拉取最新 FFmpeg 视频帧(H.264/H.265):返回 (width, height, Annex-B 字节, 是否关键帧)。
pub fn latest_video() -> Option<(u32, u32, Vec<u8>, bool)> {
    LATEST_VIDEO.lock().ok()?.clone()
}

/// 最近一帧 JPEG 编码耗时(毫秒)。
pub fn latest_frame_dur_ms() -> u32 {
    LATEST_ENCODE_DUR.load(std::sync::atomic::Ordering::Relaxed)
}

/// 最近一帧 FFmpeg 视频编码耗时(毫秒)。
pub fn latest_video_dur_ms() -> u32 {
    LATEST_VIDEO_DUR.load(std::sync::atomic::Ordering::Relaxed)
}

/// 按最大尺寸等比缩放(只缩小不放大),返回缩放后的 (宽, 高)。纯函数无平台依赖。
pub(crate) fn scale_dimensions(src_w: u32, src_h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    let src_w = src_w.max(1);
    let src_h = src_h.max(1);
    let max_w = max_w.max(1);
    let max_h = max_h.max(1);
    // 同时受限于两个方向的最大值,且不超过源分辨率(不放大)
    let scale = (max_w as f64 / src_w as f64)
        .min(max_h as f64 / src_h as f64)
        .min(1.0);
    let w = ((src_w as f64 * scale).round() as u32).max(1);
    let h = ((src_h as f64 * scale).round() as u32).max(1);
    (w, h)
}

/// 将 BGRA 像素(每像素 4 字节,丢弃 alpha)转换为 RGB(每像素 3 字节)。纯函数无平台依赖。
pub(crate) fn bgra_to_rgb(data: &[u8], w: u32, h: u32) -> Vec<u8> {
    let n = (w as usize) * (h as usize);
    let mut out = Vec::with_capacity(n * 3);
    for px in data.chunks_exact(4).take(n) {
        out.push(px[2]);
        out.push(px[1]);
        out.push(px[0]);
    }
    out
}

/// 将 RGB 数据(每像素 3 字节)按目标尺寸等比缩放(双线性,不放大)并编码为 JPEG。
///
/// 返回 (jpeg 宽, jpeg 高, jpeg 字节)。目标尺寸 clamp ≤ 1920,且不超过源分辨率。
pub(crate) fn rgb_to_jpeg(
    rgb: &[u8],
    src_w: u32,
    src_h: u32,
    target_w: u32,
    target_h: u32,
    quality: u8,
) -> Result<(u32, u32, Vec<u8>), String> {
    let src_w = src_w.max(1);
    let src_h = src_h.max(1);
    let (jw, jh) = scale_dimensions(src_w, src_h, target_w.min(1920), target_h.min(1920));
    let scale = jw as f64 / src_w as f64;

    let mut out = Vec::with_capacity((jw * jh * 3) as usize);
    if jw == src_w && jh == src_h {
        out.extend_from_slice(rgb);
    } else {
        // 双线性缩放:遍历目标像素,取源 4 邻域按权重混合
        for y in 0..jh {
            let sy = (y as f64 + 0.5) / scale - 0.5;
            let sy0 = sy.floor().max(0.0) as u32;
            let sy1 = (sy0 + 1).min(src_h - 1);
            let fy = (sy - sy0 as f64) as f32;
            for x in 0..jw {
                let sx = (x as f64 + 0.5) / scale - 0.5;
                let sx0 = sx.floor().max(0.0) as u32;
                let sx1 = (sx0 + 1).min(src_w - 1);
                let fx = (sx - sx0 as f64) as f32;
                let p00 = ((sy0 * src_w + sx0) * 3) as usize;
                let p01 = ((sy0 * src_w + sx1) * 3) as usize;
                let p10 = ((sy1 * src_w + sx0) * 3) as usize;
                let p11 = ((sy1 * src_w + sx1) * 3) as usize;
                for c in 0..3 {
                    let v = rgb[p00 + c] as f32 * (1.0 - fx) * (1.0 - fy)
                        + rgb[p01 + c] as f32 * fx * (1.0 - fy)
                        + rgb[p10 + c] as f32 * (1.0 - fx) * fy
                        + rgb[p11 + c] as f32 * fx * fy;
                    out.push(v.round().clamp(0.0, 255.0) as u8);
                }
            }
        }
    }

    // JPEG 编码(jpeg-encoder 0.6:Encoder::new(writer, quality) + encode)
    let mut jpeg_buf: Vec<u8> = Vec::new();
    {
        use jpeg_encoder::{ColorType, Encoder};
        let encoder = Encoder::new(&mut jpeg_buf, quality);
        encoder
            .encode(&out, jw as u16, jh as u16, ColorType::Rgb)
            .map_err(|e| format!("JPEG 编码失败: {e}"))?;
    }
    Ok((jw, jh, jpeg_buf))
}

#[cfg(target_os = "windows")]
fn list_monitors_windows() -> Result<Vec<MonitorInfo>, String> {
    use windows::core::PCWSTR;
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayDevicesW, EnumDisplaySettingsW, DEVMODEW, DISPLAY_DEVICEW,
        DISPLAY_DEVICE_PRIMARY_DEVICE, ENUM_CURRENT_SETTINGS,
    };

    /// 将 [u16] 宽字符数组(以 0 结尾)转换为 String。
    fn wstr(a: &[u16]) -> String {
        let end = a.iter().position(|&c| c == 0).unwrap_or(a.len());
        String::from_utf16_lossy(&a[..end])
    }

    let mut monitors = Vec::new();
    let mut i: u32 = 0;
    loop {
        let mut dd = DISPLAY_DEVICEW::default();
        let ok = unsafe { EnumDisplayDevicesW(None, i, &mut dd, 0) }.as_bool();
        if !ok {
            break;
        }
        let name = wstr(&dd.DeviceName);
        let device_string = wstr(&dd.DeviceString);
        let is_primary = dd.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE != 0;
        // IDD 虚拟显示器特征:usbmmidd 驱动设备名为 "USB Mobile Monitor",或设备字符串含 "usbmmidd"
        let is_virtual = {
            let lower = device_string.to_lowercase();
            lower.contains("usbmmidd") || lower.contains("usb mobile monitor")
        };

        // 取当前分辨率
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

        monitors.push(MonitorInfo {
            id: i,
            name,
            width,
            height,
            is_primary,
            is_virtual,
        });
        i += 1;
    }
    log::info!("[capture] list_monitors 枚举到 {} 台显示器", monitors.len());
    crate::operation_log::op_log(
        "capture",
        "list_monitors",
        &format!("count={}", monitors.len()),
    );
    Ok(monitors)
}

/// 真实 DXGI 桌面复制抓屏循环(随任务结束释放全部 DXGI 资源)。
#[cfg(target_os = "windows")]
async fn dxgi_capture_loop(app: AppHandle, monitor_id: u32) -> Result<(), String> {
    use windows::core::Interface;
    use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
    use windows::Win32::Graphics::Direct3D11::{
        D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
        D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_FLAG, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ,
        D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    };
    use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource,
        DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO,
    };

    // 1) DXGI 工厂 → 适配器(0) → 指定输出(显示器)
    let factory: IDXGIFactory1 =
        unsafe { CreateDXGIFactory1() }.map_err(|e| format!("CreateDXGIFactory1 失败: {e}"))?;
    let adapter = unsafe { factory.EnumAdapters1(0) }
        .map_err(|e| format!("EnumAdapters1(0) 失败(可能无 GPU): {e}"))?;
    let output = unsafe { adapter.EnumOutputs(monitor_id) }
        .map_err(|e| format!("EnumOutputs({monitor_id}) 失败(显示器不存在或不可捕获): {e}"))?;

    // 2) 基于该适配器创建 D3D11 设备与立即上下文
    let mut device: Option<ID3D11Device> = None;
    let mut ctx: Option<ID3D11DeviceContext> = None;
    unsafe {
        D3D11CreateDevice(
            &adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            None,
            D3D11_CREATE_DEVICE_FLAG(0),
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut ctx),
        )
    }
    .map_err(|e| format!("D3D11CreateDevice 失败: {e}"))?;
    let device = device.ok_or("D3D11CreateDevice 未返回设备")?;
    let ctx = ctx.ok_or("D3D11CreateDevice 未返回设备上下文")?;

    // 3) 桌面复制输出(DuplicateOutput 定义于 IDXGIOutput1)
    let output1: IDXGIOutput1 = output
        .cast()
        .map_err(|e| format!("IDXGIOutput → IDXGIOutput1 转换失败: {e}"))?;
    let dup: IDXGIOutputDuplication = unsafe { output1.DuplicateOutput(&device) }
        .map_err(|e| format!("DuplicateOutput 失败(桌面捕获不可用, UNAVAILABLE): {e}"))?;

    log::info!("[capture] DXGI 抓屏就绪: monitor {monitor_id}");

    // FFmpeg 视频编码器(懒创建,分辨率/帧率/编码器变化时重建)
    let mut hw_enc: Option<crate::ffmpeg_hw::HwEncoder> = None;
    let mut hw_key: Option<(u32, u32, u32, u32, u32, String)> = None;
    let mut hw_logged = false;

    let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
    loop {
        // 实时读取流配置(前端 set_stream_quality/set_stream_resolution 即时生效)
        let cfg = crate::hbb_client::stream_cfg();
        let interval_ms = (1000u64 / u64::from(cfg.fps.clamp(1, 30))).max(1);
        tokio::time::sleep(std::time::Duration::from_millis(interval_ms)).await;

        // 4) 获取下一帧;DXGI_ERROR_WAIT_TIMEOUT 表示暂无新帧,继续等待
        let mut resource: Option<IDXGIResource> = None;
        if let Err(e) = unsafe { dup.AcquireNextFrame(0, &mut frame_info, &mut resource) } {
            if e.code() == DXGI_ERROR_WAIT_TIMEOUT {
                continue;
            }
            log::error!("[capture] AcquireNextFrame 失败: {e}");
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            continue;
        }
        let Some(resource) = resource else {
            let _ = unsafe { dup.ReleaseFrame() };
            continue;
        };
        let tex: ID3D11Texture2D = match resource.cast() {
            Ok(t) => t,
            Err(e) => {
                log::warn!("[capture] 桌面资源转换 ID3D11Texture2D 失败: {e}");
                let _ = unsafe { dup.ReleaseFrame() };
                continue;
            }
        };
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { tex.GetDesc(&mut desc) };
        let src_w = desc.Width;
        let src_h = desc.Height;

        // 5) 拷贝到 CPU 可读的 staging 纹理
        let mut staging_desc = desc;
        staging_desc.Usage = D3D11_USAGE_STAGING;
        staging_desc.BindFlags = 0;
        staging_desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        staging_desc.MiscFlags = 0;
        staging_desc.MipLevels = 1;
        staging_desc.ArraySize = 1;
        staging_desc.SampleDesc = DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        };
        let mut staging: Option<ID3D11Texture2D> = None;
        if let Err(e) = unsafe { device.CreateTexture2D(&staging_desc, None, Some(&mut staging)) } {
            log::error!("[capture] 创建 staging 纹理失败: {e}");
            let _ = unsafe { dup.ReleaseFrame() };
            continue;
        }
        let Some(staging) = staging else {
            let _ = unsafe { dup.ReleaseFrame() };
            continue;
        };
        unsafe { ctx.CopyResource(&staging, &tex) };
        drop(tex);

        // 6) Map 读出像素(注意 RowPitch 可能大于 width*4)
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        if let Err(e) = unsafe { ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)) } {
            log::error!("[capture] Map 失败: {e}");
            let _ = unsafe { dup.ReleaseFrame() };
            continue;
        }
        let row_pitch = mapped.RowPitch as usize;
        let mut bgra = vec![0u8; (src_w as usize) * (src_h as usize) * 4];
        for y in 0..src_h as usize {
            // SAFETY: 指针指向已 Map 的 staging 纹理数据,行偏移不超过分配范围
            let src_row = unsafe { (mapped.pData as *const u8).add(y * row_pitch) };
            let dst_row = &mut bgra[y * (src_w as usize) * 4..(y + 1) * (src_w as usize) * 4];
            unsafe {
                std::ptr::copy_nonoverlapping(src_row, dst_row.as_mut_ptr(), (src_w as usize) * 4);
            }
        }
        unsafe { ctx.Unmap(&staging, 0) };
        drop(staging);
        let _ = unsafe { dup.ReleaseFrame() };

        // 7) BGRA → RGB
        let rgb = bgra_to_rgb(&bgra, src_w, src_h);

        // 7.5) FFmpeg 视频编码路径(H.264/H.265):流配置 codec∈{h264,hevc} 且 FFmpeg DLL 可用时启用
        let video_active =
            (cfg.codec == "h264" || cfg.codec == "hevc") && crate::ffmpeg_hw::available();
        if video_active {
            let key = (
                src_w,
                src_h,
                cfg.target_width,
                cfg.target_height,
                cfg.fps,
                cfg.codec.clone(),
            );
            if hw_key.as_ref() != Some(&key) {
                // 分辨率/帧率/编码器变化 → 重建编码器(首帧请求关键帧)
                let family = cfg.codec.clone();
                let enc_name =
                    crate::ffmpeg_hw::preferred_encoder(&family).unwrap_or_else(|| family.clone());
                hw_enc = crate::ffmpeg_hw::HwEncoder::open(
                    &enc_name,
                    crate::ffmpeg_hw::codec_family_id(&family),
                    src_w,
                    src_h,
                    cfg.target_width,
                    cfg.target_height,
                    cfg.fps,
                )
                .ok();
                hw_key = Some(key);
                if let Some(e) = hw_enc.as_mut() {
                    e.request_keyframe();
                }
                if !hw_logged {
                    log::info!(
                        "[capture] FFmpeg 视频编码启用: {} ({family}) @ {}x{};{}",
                        enc_name,
                        hw_enc.as_ref().map(|e| e.dims().0).unwrap_or(0),
                        hw_enc.as_ref().map(|e| e.dims().1).unwrap_or(0),
                        crate::ffmpeg_hw::capability_report()
                    );
                    hw_logged = true;
                }
            }
            if let Some(enc) = hw_enc.as_mut() {
                let enc_start = std::time::Instant::now();
                match enc.encode_rgb(&rgb) {
                    Ok(Some((ew, eh, data, is_key))) => {
                        LATEST_VIDEO_DUR.store(
                            enc_start.elapsed().as_millis() as u32,
                            std::sync::atomic::Ordering::Relaxed,
                        );
                        if let Ok(mut slot) = LATEST_VIDEO.lock() {
                            *slot = Some((ew, eh, data, is_key));
                        }
                    }
                    Ok(None) => {}
                    Err(e) => log::warn!("[capture] FFmpeg 编码失败: {e}"),
                }
            }
        } else {
            hw_enc = None;
            hw_key = None;
            hw_logged = false;
        }

        // 8) 缩放 + JPEG 编码(目标尺寸/画质来自流配置),记录编码耗时。
        //    FFmpeg 视频编码激活时仅生成小尺寸预览图(供本地 capture-frame 面板),避免全尺寸 CPU 编码。
        let encode_start = std::time::Instant::now();
        let (jw, jh, jpeg) = if video_active {
            match rgb_to_jpeg(&rgb, src_w, src_h, 480, 270, 60) {
                Ok(v) => v,
                Err(e) => {
                    log::error!("[capture] 预览帧编码失败: {e}");
                    continue;
                }
            }
        } else {
            match rgb_to_jpeg(
                &rgb,
                src_w,
                src_h,
                cfg.target_width,
                cfg.target_height,
                cfg.jpeg_quality,
            ) {
                Ok(v) => v,
                Err(e) => {
                    log::error!("[capture] 编码帧失败: {e}");
                    continue;
                }
            }
        };
        LATEST_ENCODE_DUR.store(
            encode_start.elapsed().as_millis() as u32,
            std::sync::atomic::Ordering::Relaxed,
        );

        // 9) 推送事件并缓存最新帧
        let payload = CapturedFrameEvent {
            monitor_id,
            width: jw,
            height: jh,
            jpeg: jpeg.clone(),
            simulated: false,
        };
        let _ = app.emit("capture-frame", &payload);

        let snapshot = CapturedFrame {
            width: jw,
            height: jh,
            format: "jpeg".into(),
            data: jpeg,
        };
        if let Ok(mut slot) = LATEST_FRAME.lock() {
            *slot = Some(snapshot);
        }
    }
}

/// 非 Windows 平台:程序化生成一帧 RGBA 动画(仅编译占位)。
#[cfg(not(target_os = "windows"))]
fn generate_mock_frame(t: u32, width: u32, height: u32) -> Vec<u8> {
    // 取模限制动画周期,避免长时间运行后 t 溢出(debug 下 panic)
    let t = t % 60_000;
    let w = width.max(1);
    let h = height.max(1);
    let mut data = vec![0u8; (w * h * 4) as usize];

    let cell: u32 = 16;
    // 移动亮带:在宽度方向上往返
    let band_w = (w / 8).max(1);
    let band_x = ((t % (w + band_w)) as u32).min(w - 1);
    let band_end = (band_x + band_w).min(w);
    // 光标方块:斜向往返
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
            // 光标方块(白色)
            if x >= cx && x < cx + cw && y >= cy && y < cy + ch {
                pixel = [255, 255, 255, 255];
            }

            data[idx..idx + 4].copy_from_slice(&pixel);
        }
    }
    data
}

/// 非 Windows 平台:模拟抓帧循环(仅编译占位)。
#[cfg(not(target_os = "windows"))]
async fn mock_capture_loop(app: AppHandle, monitor_id: u32, width: u32, height: u32, fps: u32) {
    let w = width.clamp(1, 480);
    let h = height.clamp(1, 270);
    let fps = fps.clamp(1, 30);
    let interval_ms = (1000 / fps) as u64;
    let mut frame_idx: u32 = 0;
    let mut timer = tokio::time::interval(std::time::Duration::from_millis(interval_ms));
    loop {
        timer.tick().await;

        let rgba = generate_mock_frame(frame_idx, w, h);
        // RGBA → RGB
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for px in rgba.chunks_exact(4) {
            rgb.push(px[0]);
            rgb.push(px[1]);
            rgb.push(px[2]);
        }
        let jpeg = match rgb_to_jpeg(&rgb, w, h, w, h, 70) {
            Ok((_, _, j)) => j,
            Err(e) => {
                log::warn!("[capture] 模拟帧 JPEG 编码失败: {e}");
                frame_idx = frame_idx.wrapping_add(1);
                continue;
            }
        };
        let payload = CapturedFrameEvent {
            monitor_id,
            width: w,
            height: h,
            jpeg: jpeg.clone(),
            simulated: true,
        };
        let _ = app.emit("capture-frame", &payload);

        let snapshot = CapturedFrame {
            width: w,
            height: h,
            format: "jpeg".into(),
            data: jpeg,
        };
        if let Ok(mut slot) = LATEST_FRAME.lock() {
            *slot = Some(snapshot);
        }

        frame_idx = frame_idx.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_dimensions_shrinks_and_never_enlarges() {
        // 等比缩小:1920x1080 → 960x540
        assert_eq!(scale_dimensions(1920, 1080, 960, 540), (960, 540));
        // 源比目标小:不放大
        assert_eq!(scale_dimensions(640, 360, 1920, 1080), (640, 360));
        assert_eq!(scale_dimensions(240, 135, 1920, 1080), (240, 135));
    }

    #[test]
    fn bgra_to_rgb_order_and_length() {
        // 2x2 = 4 像素 BGRA 输入(B,G,R,A)
        let data: Vec<u8> = vec![
            0x11, 0x22, 0x33, 0xFF, 0x44, 0x55, 0x66, 0xFF, 0x77, 0x88, 0x99, 0xFF, 0xAA, 0xBB,
            0xCC, 0xFF,
        ];
        let out = bgra_to_rgb(&data, 2, 2);
        // 长度 = w*h*3
        assert_eq!(out.len(), 12);
        // 每像素 BGR → RGB 顺序(丢弃 alpha)
        assert_eq!(
            out,
            vec![0x33, 0x22, 0x11, 0x66, 0x55, 0x44, 0x99, 0x88, 0x77, 0xCC, 0xBB, 0xAA,]
        );
    }

    /// 本机 DXGI 真实抓帧 + 缩放 + JPEG 编码吞吐基准(需要显示器/GPU,默认忽略)。
    ///
    /// 无节流(不按 fps 睡眠)连续抓帧 3 秒,统计:
    /// - 真实帧率:DXGI 实际交付帧数 / 时长(受桌面内容变化频率限制)
    /// - 管道理论最大帧率:单帧(采集 + 缩放 + 编码)平均耗时的倒数
    ///
    /// 运行:`cargo test -- --ignored dxgi_max_fps_benchmark --nocapture`
    #[cfg(target_os = "windows")]
    #[test]
    #[ignore]
    fn dxgi_max_fps_benchmark() {
        use windows::core::Interface;
        use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
        use windows::Win32::Graphics::Direct3D11::{
            D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
            D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_FLAG, D3D11_MAPPED_SUBRESOURCE,
            D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
        };
        use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;
        use windows::Win32::Graphics::Dxgi::{
            CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource,
            DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO,
        };

        let monitors = list_monitors_windows().expect("枚举显示器失败");
        assert!(!monitors.is_empty(), "本机未检测到显示器");
        let monitor = &monitors[0];
        println!(
            "[bench] 目标显示器 #{}: {} {}x{}",
            monitor.id, monitor.name, monitor.width, monitor.height
        );

        let factory: IDXGIFactory1 =
            unsafe { CreateDXGIFactory1() }.expect("CreateDXGIFactory1 失败");
        let adapter =
            unsafe { factory.EnumAdapters1(0) }.expect("EnumAdapters1(0) 失败(可能无 GPU)");
        let output = unsafe { adapter.EnumOutputs(monitor.id) }
            .expect("EnumOutputs 失败: 显示器不存在或不可捕获");
        let mut device: Option<ID3D11Device> = None;
        let mut ctx: Option<ID3D11DeviceContext> = None;
        unsafe {
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                None,
                D3D11_CREATE_DEVICE_FLAG(0),
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut ctx),
            )
        }
        .expect("D3D11CreateDevice 失败");
        let device = device.expect("D3D11CreateDevice 未返回设备");
        let ctx = ctx.expect("D3D11CreateDevice 未返回设备上下文");
        let output1: IDXGIOutput1 = output.cast().expect("IDXGIOutput → IDXGIOutput1 转换失败");
        let dup: IDXGIOutputDuplication = unsafe { output1.DuplicateOutput(&device) }
            .expect("DuplicateOutput 失败(桌面捕获不可用)");

        let duration = std::time::Duration::from_secs(3);
        let start = std::time::Instant::now();
        let deadline = start + duration;
        let mut frames = 0u32;
        let mut wait_timeouts = 0u64;
        let mut proc_total = std::time::Duration::ZERO;
        let mut encode_total = std::time::Duration::ZERO;
        let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();

        while std::time::Instant::now() < deadline {
            // 1) 取帧;WAIT_TIMEOUT 表示暂无新帧(桌面静止),继续空转
            let mut resource: Option<IDXGIResource> = None;
            if let Err(e) = unsafe { dup.AcquireNextFrame(0, &mut frame_info, &mut resource) } {
                if e.code() == DXGI_ERROR_WAIT_TIMEOUT {
                    wait_timeouts += 1;
                    continue;
                }
                let _ = unsafe { dup.ReleaseFrame() };
                panic!("AcquireNextFrame 失败: {e}");
            }
            let Some(resource) = resource else {
                let _ = unsafe { dup.ReleaseFrame() };
                continue;
            };
            let tex: ID3D11Texture2D = resource
                .cast()
                .unwrap_or_else(|e| panic!("桌面资源转换 ID3D11Texture2D 失败: {e}"));

            let mut desc = D3D11_TEXTURE2D_DESC::default();
            unsafe { tex.GetDesc(&mut desc) };
            let (src_w, src_h) = (desc.Width, desc.Height);

            // 2) 拷贝到 CPU 可读 staging 纹理(与 dxgi_capture_loop 同管线)
            let mut staging_desc = desc;
            staging_desc.Usage = D3D11_USAGE_STAGING;
            staging_desc.BindFlags = 0;
            staging_desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            staging_desc.MiscFlags = 0;
            staging_desc.MipLevels = 1;
            staging_desc.ArraySize = 1;
            staging_desc.SampleDesc = DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            };
            let mut staging: Option<ID3D11Texture2D> = None;
            if let Err(e) =
                unsafe { device.CreateTexture2D(&staging_desc, None, Some(&mut staging)) }
            {
                let _ = unsafe { dup.ReleaseFrame() };
                panic!("创建 staging 纹理失败: {e}");
            }
            let staging = staging.expect("CreateTexture2D 未返回纹理");
            unsafe { ctx.CopyResource(&staging, &tex) };
            drop(tex);

            // 3) Map 读出像素(注意 RowPitch 可能大于 width*4)
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            if let Err(e) = unsafe { ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)) } {
                let _ = unsafe { dup.ReleaseFrame() };
                panic!("Map 失败: {e}");
            }
            let row_pitch = mapped.RowPitch as usize;
            let mut bgra = vec![0u8; (src_w as usize) * (src_h as usize) * 4];
            for y in 0..src_h as usize {
                let src_row = unsafe { (mapped.pData as *const u8).add(y * row_pitch) };
                let dst_row = &mut bgra[y * (src_w as usize) * 4..(y + 1) * (src_w as usize) * 4];
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        src_row,
                        dst_row.as_mut_ptr(),
                        (src_w as usize) * 4,
                    );
                }
            }
            unsafe { ctx.Unmap(&staging, 0) };
            drop(staging);
            let _ = unsafe { dup.ReleaseFrame() };

            // 4) BGRA→RGB → 缩放 + JPEG 编码,记录耗时
            let proc_start = std::time::Instant::now();
            let rgb = bgra_to_rgb(&bgra, src_w, src_h);
            let encode_start = std::time::Instant::now();
            let _ = rgb_to_jpeg(&rgb, src_w, src_h, 1920, 1920, 85).expect("JPEG 编码失败");
            encode_total += encode_start.elapsed();
            proc_total += proc_start.elapsed();
            frames += 1;
        }
        drop(dup);

        let elapsed = start.elapsed().as_secs_f64();
        let real_fps = frames as f64 / elapsed;
        let avg_proc_ms = proc_total.as_secs_f64() * 1000.0 / frames.max(1) as f64;
        let pipeline_max_fps = if avg_proc_ms > 0.0 {
            1000.0 / avg_proc_ms
        } else {
            f64::INFINITY
        };
        let avg_encode_ms = encode_total.as_secs_f64() * 1000.0 / frames.max(1) as f64;
        println!(
            "[bench] 3 秒交付 {frames} 帧(空等 {wait_timeouts} 次)→ 真实帧率 {real_fps:.1} fps | 单帧管道 {avg_proc_ms:.2} ms(编码 {avg_encode_ms:.2} ms)→ 管道理论最大 {pipeline_max_fps:.1} fps"
        );
        assert!(frames > 0, "3 秒内未抓到任何帧");
    }
}
