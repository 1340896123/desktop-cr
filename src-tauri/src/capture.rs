//! 屏幕抓取模块(DXGI 桌面复制 + H.264/H.265 标准视频编码)。
//!
//! Windows 真实路径:
//!   CreateDXGIFactory1 → EnumAdapters1(0) → EnumOutputs(monitor_id)
//!   → D3D11CreateDevice → IDXGIOutput::DuplicateOutput 桌面复制
//!   → 每帧 AcquireNextFrame 拿到 ID3D11Texture2D → 拷贝到 CPU 可读 staging 纹理
//!   → Map/Unmap 读出 BGRA(每帧 CPU 拷贝第 1 次)→ 以 BGRA 直送 FFmpeg 编码器
//!   (sws 内转换 YUV420P,无中间 Vec;编码器不可用时 BGRA 原始字节直推预览)
//!   → H.264/H.265 Annex-B 存入 `LATEST_VIDEO`,经 `capture-frame` 事件推送
//!   (负载 data 为编码帧字节或 BGRA 原始字节,**全程禁止 JPEG**)。
//! 目标尺寸/帧率实时读取 `crate::hbb_client::stream_cfg()`(set_stream_* 命令即时生效)。
//! 桌面静止时 AcquireNextFrame 超时直接跳过本轮(不编码不推帧,A4),
//! 每 5 秒输出一次帧间隔/空转统计。
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

/// 原始帧像素格式(契约 4.2)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameFormat {
    /// BGRA 每像素 4 字节(DXGI 桌面复制原生格式)
    Bgra8,
    /// RGB 每像素 3 字节
    Rgb24,
    /// NV12 半平面(Y 平面 + UV 交织平面,共 w*h*3/2 字节)
    Nv12,
}

impl FrameFormat {
    /// 每帧紧凑数据字节数(无 pitch)。
    pub fn bytes_per_frame(&self, w: u32, h: u32) -> usize {
        match self {
            FrameFormat::Bgra8 => w as usize * h as usize * 4,
            FrameFormat::Rgb24 => w as usize * h as usize * 3,
            FrameFormat::Nv12 => w as usize * h as usize * 3 / 2,
        }
    }
}

/// 采集原始帧(契约 4.2):data 为紧凑行数据(无 pitch)。
#[derive(Debug, Clone)]
pub struct RawFrame {
    pub width: u32,
    pub height: u32,
    pub format: FrameFormat,
    pub data: Vec<u8>,
}

/// `get_frame` 返回的帧结构:真实抓帧后为编码帧(H.264/H.265)或 BGRA 原始像素。
#[derive(Debug, Clone, Serialize)]
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    pub format: String,
    /// H.264/H.265 编码数据或 BGRA 原始像素(禁止 JPEG)
    pub data: Vec<u8>,
}

/// 推送给前端的抓帧事件负载(契约 4.2,字段以 camelCase 序列化):
/// data 为 H.264/H.265 编码帧字节(前端 WebCodecs 解码)或 BGRA 原始字节,
/// **不得是 JPEG**。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedFrameEvent {
    pub monitor_id: u32,
    pub width: u32,
    pub height: u32,
    /// 是否关键帧(h264/hevc);BGRA 直推时为 true(可整帧直绘)
    pub key: bool,
    /// "h264" | "hevc" | "bgra"
    pub codec: String,
    /// H.264/H.265 Annex-B 字节或 BGRA 原始字节(禁止 JPEG)
    pub data: Vec<u8>,
    /// 是否为模拟画面(非 Windows 平台动画帧;真实 DXGI 抓帧为 false)
    pub simulated: bool,
}

/// 最新帧快照(本机预览源,format 为 "h264"/"hevc"/"bgra"):未开始抓帧时为 None。
static LATEST_FRAME: Mutex<Option<CapturedFrame>> = Mutex::new(None);

/// 最新 FFmpeg 视频编码帧(Annex-B,契约 4.2 `EncodedPacket`)。仅供远端会话推帧。
static LATEST_VIDEO: Mutex<Option<crate::ffmpeg_hw::EncodedPacket>> = Mutex::new(None);

/// 最近一帧预览帧准备耗时(毫秒,供远程性能统计)。
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
                crate::operation_log::op_log(
                    "capture",
                    "capture_loop_dead",
                    &format!("monitor={monitor_id}, reason={e}(抓屏循环已退出,不再产出视频帧)"),
                );
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

/// 取回最新一帧(format 为 "h264"/"hevc"/"bgra";前端无组件调用,契约保留)。
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

/// 供 host / 网络层拉取最新 FFmpeg 视频编码帧(契约 4.2:`EncodedPacket`)。
/// (生产写循环经 `latest_video_testable` 调用,测试注入源回落到本函数)
#[cfg_attr(test, allow(dead_code))]
pub fn latest_video() -> Option<crate::ffmpeg_hw::EncodedPacket> {
    LATEST_VIDEO.lock().ok()?.clone()
}

/// host 写循环取帧入口:生产 = latest_video()(真实采集产物);测试 =
/// 注入源优先(生产会话级回环测试,真实编码帧、无 DXGI 依赖)。取帧语义与
/// latest_video 相同(取最新一帧;注入源按序弹出等价采集逐帧更新)。
#[doc(hidden)]
pub fn latest_video_testable() -> Option<crate::ffmpeg_hw::EncodedPacket> {
    #[cfg(test)]
    {
        crate::capture::test_frame_source::pop_latest_video()
    }
    #[cfg(not(test))]
    {
        latest_video()
    }
}

/// 最近一帧 FFmpeg 视频编码耗时(毫秒)。
pub fn latest_video_dur_ms() -> u32 {
    LATEST_VIDEO_DUR.load(std::sync::atomic::Ordering::Relaxed)
}

/// 请求下一帧编码输出为关键帧(F-1a:控制端 UDP 丢帧后经 TCP 控制面发
/// KeyframeRequest,被控端写循环调用本函数)。
///
/// R2-A 修复语义:本标志触发**编码器重建**(采集循环在安全点——上一帧已发完、
/// 本帧编码前——按同参数重建 HwEncoder,新实例首帧自然输出 IDR),对
/// NVENC/QSV/AMF/libx264 等全部编码器通用;不依赖 `forced_idr` 私有选项
/// (该选项仅在 libx264 家族存在,在 h264_nvenc 上返回 AVERROR_OPTION_NOT_FOUND
/// 被静默吞掉——Round 2 失效点 R2-A 的根因)。
pub fn request_video_keyframe() {
    VIDEO_KEYFRAME_REQUESTED.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// 测试注入帧源(C5 生产会话级回环测试用,#[cfg(test)] 编译期隔离,不进生产):
/// 设置后 `latest_video()` 返回注入的真实编码帧序列(经真实 HwEncoder 编码),
/// 使 host_write_loop 在无 DXGI 桌面复制依赖下推送真实 H.264 码流;
/// 未设置时 latest_video 维持采集循环的真实产物(生产路径不变)。
#[cfg(test)]
pub(crate) mod test_frame_source {
    use super::LATEST_VIDEO;
    use std::sync::Mutex;

    /// 注入帧序列(消费侧按序弹出;空 = 无帧可推)。
    static INJECTED: Mutex<Vec<crate::ffmpeg_hw::EncodedPacket>> = Mutex::new(Vec::new());

    /// 注入一批编码帧(测试用;真实 HwEncoder 产物,非合成字节)。
    /// (消费在 network.rs 生产会话级回环测试;allow 消除仅测试构建下的误警)
    #[allow(dead_code)]
    pub(crate) fn set_frames(frames: Vec<crate::ffmpeg_hw::EncodedPacket>) {
        *INJECTED
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = frames;
    }

    /// 弹出下一帧:优先注入源;注入源耗尽后回落 LATEST_VIDEO(真实采集)。
    /// host_write_loop 的取帧语义为"取最新一帧",注入源按序逐帧弹出等价于
    /// 采集循环按帧更新 LATEST_VIDEO。
    pub(crate) fn pop_latest_video() -> Option<crate::ffmpeg_hw::EncodedPacket> {
        let mut inj = INJECTED.lock().unwrap_or_else(|e| e.into_inner());
        if !inj.is_empty() {
            return Some(inj.remove(0));
        }
        drop(inj);
        LATEST_VIDEO.lock().ok()?.clone()
    }

    /// R2-A 关键帧请求在测试路径的等价物:把注入源**下一帧**标记为关键帧
    /// (生产路径 = 采集循环安全点重建编码器,新实例首帧自然 IDR——见
    /// network.rs 写循环消费 KEYFRAME_REQUESTED 分支)。仅修改首帧 key
    /// 标记,不动真实编码码流字节。
    #[cfg(test)]
    pub(crate) fn request_next_keyframe() {
        let mut inj = INJECTED
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(first) = inj.first_mut() {
            first.key = true;
        }
    }
}

/// F-1a:待生效的关键帧请求(采集循环与推帧循环经原子量传递)。
static VIDEO_KEYFRAME_REQUESTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

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
///
/// 历史用途:JPEG 编码前置转换(已移除,全链路禁止 JPEG);现仅测试与诊断兜底使用。
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

/// 一次性真实 DXGI 抓屏,返回 BGRA 原始帧(契约 4.2,诊断与测试入口)。
///
/// 与 `dxgi_capture_loop` 同管线,但独立创建桌面复制并只抓一帧即释放:
/// CreateDXGIFactory1 → EnumAdapters1(0) → EnumOutputs(monitor_id) →
/// D3D11CreateDevice → IDXGIOutput1::DuplicateOutput → AcquireNextFrame(0)
/// (DXGI_ERROR_WAIT_TIMEOUT 重试最多 3 次)→ 拷贝 staging → Map 读 BGRA(唯一一次
/// CPU 拷贝)→ ReleaseFrame;COM 资源随作用域结束自动释放。
/// 复制建立后的首次 AcquireNextFrame 会交付当前桌面内容,因此静止桌面也能取到首帧。
#[cfg(target_os = "windows")]
pub fn grab_raw_frame(monitor_id: u32) -> Result<RawFrame, String> {
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
        .map_err(|e| format!("DuplicateOutput 失败(桌面捕获不可用): {e}"))?;

    // 4) 抓取一帧:WAIT_TIMEOUT 重试最多 3 次,仍超时返回 Err
    let result: Result<RawFrame, String> = (|| {
        let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource: Option<IDXGIResource> = None;
        let mut acquired = false;
        for _attempt in 0..3 {
            match unsafe { dup.AcquireNextFrame(0, &mut frame_info, &mut resource) } {
                Ok(()) => {
                    acquired = true;
                    break;
                }
                Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => continue,
                Err(e) => return Err(format!("AcquireNextFrame 失败: {e}")),
            }
        }
        if !acquired {
            return Err("AcquireNextFrame 连续超时(3 次): 桌面无新帧".to_string());
        }
        let resource = resource.ok_or("AcquireNextFrame 未返回资源")?;
        let tex: ID3D11Texture2D = resource
            .cast()
            .map_err(|e| format!("桌面资源转换 ID3D11Texture2D 失败: {e}"))?;
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
        unsafe { device.CreateTexture2D(&staging_desc, None, Some(&mut staging)) }
            .map_err(|e| format!("创建 staging 纹理失败: {e}"))?;
        let staging = staging.ok_or("创建 staging 纹理未返回纹理")?;
        unsafe { ctx.CopyResource(&staging, &tex) };
        drop(tex);

        // 6) Map 读出像素(注意 RowPitch 可能大于 width*4)
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe { ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)) }
            .map_err(|e| format!("Map 失败: {e}"))?;
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

        Ok(RawFrame {
            width: src_w,
            height: src_h,
            format: FrameFormat::Bgra8,
            data: bgra,
        })
    })();

    // 无论成败都释放桌面复制帧
    let _ = unsafe { dup.ReleaseFrame() };
    result
}

/// 非 Windows:编译占位,真实抓屏仅 Windows 支持(显式报错,不伪造成功)。
#[cfg(not(target_os = "windows"))]
pub fn grab_raw_frame(_monitor_id: u32) -> Result<RawFrame, String> {
    Err("仅 Windows 支持".to_string())
}

/// 一次性真实 DXGI 抓屏,返回 (宽, 高, RGB 字节)。
///
/// **旧接口薄封装**(diagnostics.rs 既有调用兼容):内部经 `grab_raw_frame`
/// 取 BGRA 后一次 `bgra_to_rgb` 转换(诊断链路独立管线,不计入持久循环拷贝预算)。
/// 新代码应直接使用 `grab_raw_frame`。
#[cfg(target_os = "windows")]
pub fn grab_frame_once(monitor_id: u32) -> Result<(u32, u32, Vec<u8>), String> {
    let raw = grab_raw_frame(monitor_id)?;
    let rgb = bgra_to_rgb(&raw.data, raw.width, raw.height);
    Ok((raw.width, raw.height, rgb))
}

/// 非 Windows:编译占位,真实抓屏仅 Windows 支持。
#[cfg(not(target_os = "windows"))]
pub fn grab_frame_once(_monitor_id: u32) -> Result<(u32, u32, Vec<u8>), String> {
    Err("仅 Windows 支持".to_string())
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

    // FFmpeg 视频编码器(懒创建,分辨率/帧率/码率档位/编码器变化时重建)
    let mut hw_enc: Option<crate::ffmpeg_hw::HwEncoder> = None;
    let mut hw_key: Option<(u32, u32, u32, u32, u32, u32, String)> = None;
    let mut hw_logged = false;

    // A4 帧间隔/空转统计:桌面静止时 AcquireNextFrame 超时直接跳过(不编码不推帧),
    // 每 5 秒输出一次统计,证明空转期无编码输出、CPU 占用下降。
    let stat_start = std::time::Instant::now();
    let mut stat_frames = 0u64;
    let mut stat_timeouts = 0u64;
    let mut last_frame_at: Option<std::time::Instant> = None;
    let mut frame_gap_total_ms = 0u64;
    let mut frame_gap_max_ms = 0u64;

    let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
    loop {
        // 实时读取流配置(前端 set_stream_quality/set_stream_resolution 即时生效)
        let cfg = crate::hbb_client::stream_cfg();
        let interval_ms = (1000u64 / u64::from(cfg.fps.clamp(1, 30))).max(1);
        tokio::time::sleep(std::time::Duration::from_millis(interval_ms)).await;

        // 4) 获取下一帧;DXGI_ERROR_WAIT_TIMEOUT 表示桌面暂无新帧 → 跳过本轮
        //    (A4:不编码、不推帧、不产生任何输出,纯空转等待)
        let mut resource: Option<IDXGIResource> = None;
        if let Err(e) = unsafe { dup.AcquireNextFrame(0, &mut frame_info, &mut resource) } {
            if e.code() == DXGI_ERROR_WAIT_TIMEOUT {
                stat_timeouts += 1;
                if stat_start.elapsed() >= std::time::Duration::from_secs(5) {
                    log::info!(
                        "[capture] 帧间隔统计({:.1}s): 新帧 {stat_frames}, 空转等待 {stat_timeouts} 次, 帧间平均 {gap_avg:.0}ms / 最大 {frame_gap_max_ms}ms(静止桌面不编码)",
                        stat_start.elapsed().as_secs_f64(),
                        gap_avg = if stat_frames > 1 {
                            frame_gap_total_ms as f64 / (stat_frames - 1) as f64
                        } else {
                            0.0
                        }
                    );
                    stat_frames = 0;
                    stat_timeouts = 0;
                    frame_gap_total_ms = 0;
                    frame_gap_max_ms = 0;
                }
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

        // A4 帧间隔累计(仅真实新帧)
        if let Some(at) = last_frame_at {
            let gap = at.elapsed().as_millis() as u64;
            frame_gap_total_ms += gap;
            frame_gap_max_ms = frame_gap_max_ms.max(gap);
        }
        last_frame_at = Some(std::time::Instant::now());
        stat_frames += 1;

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

        // 6) Map 读出像素(注意 RowPitch 可能大于 width*4)—— 每帧 CPU 拷贝第 1 次
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

        // BGRA 原始帧(第 2 次 CPU 拷贝发生在编码器 sws 内部转换,无中间 Vec)
        let raw = RawFrame {
            width: src_w,
            height: src_h,
            format: FrameFormat::Bgra8,
            data: bgra,
        };

        // 7) FFmpeg 视频编码路径(H.264/H.265):流配置 codec∈{h264,hevc} 且 FFmpeg
        //    DLL 可用时启用;BGRA 直入编码器(sws 内转 YUV420P)
        let video_active =
            (cfg.codec == "h264" || cfg.codec == "hevc") && crate::ffmpeg_hw::available();
        let mut video_packet: Option<crate::ffmpeg_hw::EncodedPacket> = None;
        if video_active {
            // 码率档位(F-2):STREAM_CFG.bitrate_kbps 随档位变化(1500/4000/8000),
            // 计入编码器重建 key → 档位切换时编码器按新码率重建,码率真实生效
            let mut key = (
                src_w,
                src_h,
                cfg.target_width,
                cfg.target_height,
                cfg.fps,
                cfg.bitrate_kbps,
                cfg.codec.clone(),
            );
            // R2-A:丢帧反馈的关键帧请求 → 强制编码器重建(安全点:上一帧已发完,
            // 本帧编码前;写循环单线程顺序消费,无并发访问)。重建后新编码器首帧
            // 自然输出 IDR,恢复不等 GOP 周期(g=fps*2,30fps 下最长 2 秒)——
            // 代价一次编码器重建(实测约几十 ms,远优于 2 秒)。不采用
            // forced_idr 私有选项(在 nvenc/qsv/amf 上不存在,Round 2 已实测失效)。
            if VIDEO_KEYFRAME_REQUESTED.swap(false, std::sync::atomic::Ordering::Relaxed) {
                key.0 = u32::MAX; // key 必不相等 → 强制重建
                log::info!("[capture] 应用关键帧请求(丢帧恢复),重建编码器强制 IDR");
            }
            if hw_key.as_ref() != Some(&key) {
                // 分辨率/帧率/码率档位/编码器变化(或关键帧请求)→ 重建编码器
                // (新实例首帧即 IDR)
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
                    cfg.bitrate_kbps,
                )
                .ok();
                hw_key = Some(key);
                if let Some(e) = hw_enc.as_mut() {
                    e.request_keyframe();
                }
                if !hw_logged {
                    log::info!(
                        "[capture] FFmpeg 视频编码启用: {} @ {}x{};{}",
                        hw_enc
                            .as_ref()
                            .map(|e| e.params_summary())
                            .unwrap_or_default(),
                        hw_enc.as_ref().map(|e| e.dims().0).unwrap_or(0),
                        hw_enc.as_ref().map(|e| e.dims().1).unwrap_or(0),
                        crate::ffmpeg_hw::capability_report()
                    );
                    hw_logged = true;
                }
            }
            if let Some(enc) = hw_enc.as_mut() {
                // F-1a/R2-A:关键帧请求已在上方重建分支消费(重建首帧即 IDR),
                // 此处不再走 forced_idr(该私有选项在 nvenc/qsv/amf 不存在)
                let enc_start = std::time::Instant::now();
                match enc.encode_frame(&raw) {
                    Ok(pkt) => {
                        LATEST_VIDEO_DUR.store(
                            enc_start.elapsed().as_millis() as u32,
                            std::sync::atomic::Ordering::Relaxed,
                        );
                        if let Some(p) = pkt {
                            if let Ok(mut slot) = LATEST_VIDEO.lock() {
                                *slot = Some(p.clone());
                            }
                            video_packet = Some(p);
                        }
                    }
                    Err(e) => log::warn!("[capture] FFmpeg 编码失败: {e}"),
                }
            }
        } else {
            hw_enc = None;
            hw_key = None;
            hw_logged = false;
        }

        // 8) 本机预览帧(A3):优先复用 LATEST_VIDEO 编码帧(codec="h264"/"hevc",
        //    预览与远端同源);FFmpeg 不可用时回退 BGRA 原始字节(codec="bgra",
        //    前端 putImageData 直绘)。**禁止 JPEG**。
        //    video_active=false 且 FFmpeg 不可用 → 仅推小尺寸 BGRA(480x270 等比),
        //    控制本机预览带宽;video_active=true 时全量复用编码帧,不再单独编码。
        let emit_start = std::time::Instant::now();
        let (ew, eh, ekey, ecodec, edata) = if let Some(pkt) = video_packet.as_ref() {
            (pkt.width, pkt.height, pkt.key, cfg.codec.clone(), pkt.data.clone())
        } else if video_active {
            // 编码器本帧无输出(如刚重建):不推预览,等下一帧
            continue;
        } else {
            // FFmpeg 不可用:缩放 BGRA(等比小图,纯 CPU 双线性)
            let (tw, th) = scale_dimensions(src_w, src_h, 480, 270);
            let scaled = scale_bgra(&raw.data, src_w, src_h, tw, th);
            (tw, th, true, "bgra".to_string(), scaled)
        };
        LATEST_ENCODE_DUR.store(
            emit_start.elapsed().as_millis() as u32,
            std::sync::atomic::Ordering::Relaxed,
        );

        // 9) 推送事件并缓存最新帧(负载契约 4.2:camelCase,含 key/codec/data)
        let payload = CapturedFrameEvent {
            monitor_id,
            width: ew,
            height: eh,
            key: ekey,
            codec: ecodec.clone(),
            data: edata.clone(),
            simulated: false,
        };
        let _ = app.emit("capture-frame", &payload);

        let snapshot = CapturedFrame {
            width: ew,
            height: eh,
            format: ecodec,
            data: edata,
        };
        if let Ok(mut slot) = LATEST_FRAME.lock() {
            *slot = Some(snapshot);
        }
    }
}

/// 纯 CPU 双线性缩放 BGRA(仅本机预览 FFmpeg 不可用兜底路径使用)。输出紧凑行数据。
pub(crate) fn scale_bgra(data: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    let (src_w, src_h) = (src_w.max(1), src_h.max(1));
    let (dst_w, dst_h) = (dst_w.max(1), dst_h.max(1));
    if dst_w == src_w && dst_h == src_h {
        return data[..FrameFormat::Bgra8.bytes_per_frame(src_w, src_h)]
            .to_vec();
    }
    let sx_ratio = src_w as f64 / dst_w as f64;
    let sy_ratio = src_h as f64 / dst_h as f64;
    let mut out = vec![0u8; FrameFormat::Bgra8.bytes_per_frame(dst_w, dst_h)];
    for y in 0..dst_h {
        let sy = (y as f64 + 0.5) * sy_ratio - 0.5;
        let sy0 = sy.floor().max(0.0) as u32;
        let sy1 = (sy0 + 1).min(src_h - 1);
        let fy = (sy - sy0 as f64) as f32;
        for x in 0..dst_w {
            let sx = (x as f64 + 0.5) * sx_ratio - 0.5;
            let sx0 = sx.floor().max(0.0) as u32;
            let sx1 = (sx0 + 1).min(src_w - 1);
            let fx = (sx - sx0 as f64) as f32;
            let p00 = ((sy0 * src_w + sx0) * 4) as usize;
            let p01 = ((sy0 * src_w + sx1) * 4) as usize;
            let p10 = ((sy1 * src_w + sx0) * 4) as usize;
            let p11 = ((sy1 * src_w + sx1) * 4) as usize;
            let dst = ((y * dst_w + x) * 4) as usize;
            for c in 0..4 {
                let v = data[p00 + c] as f32 * (1.0 - fx) * (1.0 - fy)
                    + data[p01 + c] as f32 * fx * (1.0 - fy)
                    + data[p10 + c] as f32 * (1.0 - fx) * fy
                    + data[p11 + c] as f32 * fx * fy;
                out[dst + c] = v.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

/// 非 Windows 平台:程序化生成一帧 BGRA 动画(仅编译占位)。
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

/// 非 Windows 平台:模拟抓帧循环(仅编译占位)。BGRA 原始字节直推(codec="bgra",
/// 前端 putImageData 直绘),禁止 JPEG。
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

        let bgra = generate_mock_frame(frame_idx, w, h);
        let payload = CapturedFrameEvent {
            monitor_id,
            width: w,
            height: h,
            key: true,
            codec: "bgra".to_string(),
            data: bgra.clone(),
            simulated: true,
        };
        let _ = app.emit("capture-frame", &payload);

        let snapshot = CapturedFrame {
            width: w,
            height: h,
            format: "bgra".into(),
            data: bgra,
        };
        if let Ok(mut slot) = LATEST_FRAME.lock() {
            *slot = Some(snapshot);
        }

        frame_idx = frame_idx.wrapping_add(1);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// F-3:DXGI 桌面复制(DuplicateOutput)全进程唯一,并行测试会争抢导致
    /// "未抓到任何真实帧"(判别器实测并行连跑 2/2 全失败)。所有创建
    /// duplication 的 ignored 真实硬件测试(dxgi_max_fps_benchmark 与
    /// diagnostics 的 dxgi_loopback_* 系列)统一经此锁串行化临界区——
    /// `cargo test -- --ignored` 默认并行时,跨测试的锁竞争天然串行执行,
    /// 无需 --test-threads=1(std 同步锁,测试为同步/异步任务内使用均安全)。
    pub(crate) static TEST_DXGI_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// 生成一帧合成测试像素数据(跨模块测试工具,不进生产链路):
    /// 渐变 + 帧间变化图案,按 `format` 输出紧凑行数据。
    pub(crate) fn synthetic_frame(w: u32, h: u32, format: FrameFormat) -> RawFrame {
        let n = (w as usize) * (h as usize);
        let mut data = match format {
            FrameFormat::Bgra8 | FrameFormat::Rgb24 => {
                Vec::with_capacity(format.bytes_per_frame(w, h))
            }
            FrameFormat::Nv12 => Vec::with_capacity(format.bytes_per_frame(w, h)),
        };
        match format {
            FrameFormat::Bgra8 => {
                for y in 0..h {
                    for x in 0..w {
                        data.extend_from_slice(&[
                            (x % 256) as u8,
                            (y % 256) as u8,
                            ((x ^ y) % 256) as u8,
                            255,
                        ]);
                    }
                }
            }
            FrameFormat::Rgb24 => {
                for y in 0..h {
                    for x in 0..w {
                        data.extend_from_slice(&[
                            (x % 256) as u8,
                            (y % 256) as u8,
                            ((x ^ y) % 256) as u8,
                        ]);
                    }
                }
            }
            FrameFormat::Nv12 => {
                // Y 平面:亮度渐变(w*h 字节)
                for y in 0..h {
                    for x in 0..w {
                        data.push(((x + y) % 256) as u8);
                    }
                }
                // UV 交织平面:n/2 个 (U,V) 对 = n 字节;合计 w*h + w*h/2 = w*h*3/2
                for _i in 0..n / 4 {
                    data.push(0x80);
                    data.push(0x80);
                }
                // n/2 像素对在奇数宽度下可能少 1 字节,不足处补齐 3/2 长度
                while data.len() < format.bytes_per_frame(w, h) {
                    data.push(0x80);
                }
            }
        }
        RawFrame {
            width: w,
            height: h,
            format,
            data,
        }
    }

    #[test]
    fn frame_format_bytes() {
        // 紧凑字节数:w*h*bpp(NV12 = 1.5 字节/像素)
        assert_eq!(FrameFormat::Bgra8.bytes_per_frame(100, 50), 20_000);
        assert_eq!(FrameFormat::Rgb24.bytes_per_frame(100, 50), 15_000);
        assert_eq!(FrameFormat::Nv12.bytes_per_frame(100, 50), 7_500);
    }

    #[test]
    fn synthetic_frame_lengths() {
        for fmt in [FrameFormat::Bgra8, FrameFormat::Rgb24, FrameFormat::Nv12] {
            let f = synthetic_frame(64, 32, fmt);
            assert_eq!(f.data.len(), fmt.bytes_per_frame(64, 32));
            assert_eq!((f.width, f.height), (64, 32));
        }
    }

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

    #[test]
    fn scale_bgra_identity_and_shrink() {
        // 原尺寸直通
        let src = synthetic_frame(8, 4, FrameFormat::Bgra8);
        let out = scale_bgra(&src.data, 8, 4, 8, 4);
        assert_eq!(out, src.data);
        // 缩小一半:长度匹配且 alpha 保留
        let out = scale_bgra(&src.data, 8, 4, 4, 2);
        assert_eq!(out.len(), FrameFormat::Bgra8.bytes_per_frame(4, 2));
        for px in out.chunks_exact(4) {
            assert_eq!(px[3], 255, "缩放后 alpha 应保持不透明");
        }
    }

    /// A1/A2:本机 DXGI 真实采集(RawFrame)吞吐基准(需要显示器/GPU,默认忽略)。
    ///
    /// 与 `dxgi_capture_loop` 同管线(持久 duplication:WAIT_TIMEOUT 直接跳过,不做
    /// 同步重试),测量纯采集耗时(AcquireNextFrame → CopyResource → Map 读出):
    /// - 平均抓屏耗时(A1:1080p 目标 ≤ 10ms;更高分辨率如实输出并折算说明)
    /// - 每帧平均 CPU 拷贝字节数(A2:Map 读出 BGRA 1 次 = w*h*4 字节,
    ///   编码器 sws 内转换为第 2 次,无中间 Vec)
    /// - 帧间隔统计(A4:静止桌面空转等待次数,空转期不产生任何帧输出)
    ///
    /// 运行:`cargo test --release -- --ignored dxgi_max_fps_benchmark --nocapture`
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

        // F-3:DXGI 临界区互斥——并行跑 ignored 全集时与其他 duplication 测试串行
        let _dxgi = TEST_DXGI_MUTEX
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        let monitors = list_monitors_windows().expect("枚举显示器失败");
        assert!(!monitors.is_empty(), "本机未检测到显示器");
        let monitor = &monitors[0];
        println!(
            "[bench] 目标显示器 #{}: {} {}x{}",
            monitor.id, monitor.name, monitor.width, monitor.height
        );

        // 建立持久 duplication(与 dxgi_capture_loop 同生命周期模型)
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

        // staging 纹理按首帧尺寸创建一次复用(与持久循环一致;分辨率变化时重建)
        let mut staging: Option<ID3D11Texture2D> = None;
        let mut frame_w = 0u32;
        let mut frame_h = 0u32;

        let duration = std::time::Duration::from_secs(3);
        let start = std::time::Instant::now();
        let deadline = start + duration;
        let mut frames = 0u32;
        let mut wait_timeouts = 0u64;
        let mut grab_total = std::time::Duration::ZERO;
        let mut copy_bytes_total = 0u64;
        let mut gap_max_ms = 0u64;
        let mut last_frame_at: Option<std::time::Instant> = None;
        let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();

        while std::time::Instant::now() < deadline {
            // 1) 取帧;WAIT_TIMEOUT = 桌面暂无新帧 → 空转计数,不计时(A4 行为)
            let mut resource: Option<IDXGIResource> = None;
            let grab_start = std::time::Instant::now();
            match unsafe { dup.AcquireNextFrame(0, &mut frame_info, &mut resource) } {
                Ok(()) => {}
                Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => {
                    wait_timeouts += 1;
                    continue;
                }
                Err(e) => panic!("AcquireNextFrame 失败: {e}"),
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
            frame_w = src_w;
            frame_h = src_h;

            // 2) staging 复用(尺寸变化重建)
            let need_rebuild = match staging.as_ref() {
                Some(s) => {
                    let mut d = D3D11_TEXTURE2D_DESC::default();
                    unsafe { s.GetDesc(&mut d) };
                    d.Width != src_w || d.Height != src_h
                }
                None => true,
            };
            if need_rebuild {
                let mut sd = desc;
                sd.Usage = D3D11_USAGE_STAGING;
                sd.BindFlags = 0;
                sd.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                sd.MiscFlags = 0;
                sd.MipLevels = 1;
                sd.ArraySize = 1;
                sd.SampleDesc = DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                };
                let mut st: Option<ID3D11Texture2D> = None;
                unsafe { device.CreateTexture2D(&sd, None, Some(&mut st)) }
                    .expect("创建 staging 纹理失败");
                staging = st;
            }
            let st = staging.as_ref().unwrap();
            unsafe { ctx.CopyResource(st, &tex) };
            drop(tex);

            // 3) Map 读出(A2:每帧 CPU 拷贝第 1 次)
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            if let Err(e) = unsafe { ctx.Map(st, 0, D3D11_MAP_READ, 0, Some(&mut mapped)) } {
                let _ = unsafe { dup.ReleaseFrame() };
                panic!("Map 失败: {e}");
            }
            let row_pitch = mapped.RowPitch as usize;
            let mut bgra = vec![0u8; (src_w as usize) * (src_h as usize) * 4];
            for y in 0..src_h as usize {
                // SAFETY: 指针指向已 Map 的 staging 数据,行偏移在分配范围内
                let src_row = unsafe { (mapped.pData as *const u8).add(y * row_pitch) };
                let dst_row =
                    &mut bgra[y * (src_w as usize) * 4..(y + 1) * (src_w as usize) * 4];
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        src_row,
                        dst_row.as_mut_ptr(),
                        (src_w as usize) * 4,
                    );
                }
            }
            unsafe { ctx.Unmap(st, 0) };
            let _ = unsafe { dup.ReleaseFrame() };

            // 4) 计时与统计(真实消费:校验紧凑长度契约)
            grab_total += grab_start.elapsed();
            copy_bytes_total += bgra.len() as u64;
            frames += 1;
            assert_eq!(
                bgra.len(),
                FrameFormat::Bgra8.bytes_per_frame(src_w, src_h)
            );
            if let Some(at) = last_frame_at {
                gap_max_ms = gap_max_ms.max(at.elapsed().as_millis() as u64);
            }
            last_frame_at = Some(std::time::Instant::now());
        }
        drop(staging);
        drop(dup);

        assert!(frames > 0, "3 秒内未抓到任何帧");
        let elapsed = start.elapsed().as_secs_f64();
        let real_fps = frames as f64 / elapsed;
        let avg_grab_ms = grab_total.as_secs_f64() * 1000.0 / frames as f64;
        let avg_copy_bytes = copy_bytes_total / frames as u64;
        let pixels = (frame_w as f64) * (frame_h as f64);
        // A1 预算按 1080p 像素量折算(4K 桌面允许同比例放宽)
        let budget_ms = 10.0 * (pixels / (1920.0 * 1080.0)).max(1.0);
        println!(
            "[bench] 3 秒交付 {frames} 帧(空转等待 {wait_timeouts} 次)→ 真实帧率 {real_fps:.1} fps | 纯采集(取帧+staging+Map)平均 {avg_grab_ms:.2} ms(A1 预算 {frame_w}x{frame_h} ≤ {budget_ms:.1}ms,{}) | 每帧 CPU 拷贝 {avg_copy_bytes} 字节(A2:Map 读出 1 次,sws 转换为第 2 次,共 ≤ 2 次) | 帧间隔最大 {gap_max_ms} ms(空转期无输出,A4)",
            if avg_grab_ms <= budget_ms { "达标" } else { "超出预算,需 GPU/驱动解释" }
        );
        assert!(
            avg_grab_ms <= budget_ms,
            "A1: {frame_w}x{frame_h} 纯采集 {avg_grab_ms:.2} ms 超出折算预算 {budget_ms:.1} ms"
        );
    }
}
