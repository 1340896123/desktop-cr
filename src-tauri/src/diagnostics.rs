//! 诊断模块:DXGI 回传链路自检(设置页「诊断」tab 一键运行)。
//!
//! 链路与生产远程会话完全一致,全程标准视频编解码(H.264,硬编/硬解优先),
//! **不使用 JPEG**:
//!   真实 DXGI 抓屏(`capture::grab_frame_once`)→ FFmpeg 编码(NVENC/QSV/AMF
//!   优先,软件回退)→ 生产协议帧(`Msg::Frame`,4 字节长度前缀 JSON + base64
//!   Annex-B)→ 本机 TCP 回环 → FFmpeg 解码(D3D11VA 硬解优先,软件回退)
//!   → 各阶段耗时统计,同时经 `dxgi-loop-frame` 事件把编码帧回传前端,
//!   由前端 WebCodecs 解码预览。
//!
//! 真实抓屏失败(无显示器 / 无活动桌面会话)时显式报错,不回退合成帧,
//! 保证「真实本机采集」的诊断结论可信。

use base64::Engine as _;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

use crate::network::{read_msg, write_msg, Msg};

/// 防止并发运行多个诊断(命令按钮置灰之外的服务端兜底)。
static RUNNING: AtomicBool = AtomicBool::new(false);

/// DXGI 回传诊断报告(camelCase 序列化,供前端展示)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DxgiLoopbackReport {
    /// 编解码家族,固定 "h264"(标准视频编解码,本诊断禁止 JPEG)
    pub codec: String,
    /// 实际使用的 FFmpeg 编码器(如 h264_nvenc / h264_qsv / libx264)
    pub encoder: String,
    /// 解码是否走 D3D11VA 硬件路径(false = 软件回退)
    pub decoder_hwaccel: bool,
    /// 采集的显示器序号
    pub monitor_id: u32,
    /// 诊断时长(秒)
    pub seconds: u32,
    /// 目标帧率
    pub target_fps: u32,
    /// 真实抓到的新帧数(DXGI 实际交付;桌面静止时低于发送数属正常)
    pub frames_grabbed: u64,
    /// 经协议发送的帧数
    pub frames_sent: u64,
    /// 本机解码成功的帧数
    pub frames_rendered: u64,
    /// 实时帧率 = 解码帧数 / 总耗时
    pub realtime_fps: f64,
    /// 平均抓屏耗时(毫秒)
    pub avg_capture_ms: f64,
    /// 平均编码耗时(毫秒)
    pub avg_encode_ms: f64,
    /// 平均发送耗时(毫秒)
    pub avg_send_ms: f64,
    /// 平均本机解码耗时(毫秒)
    pub avg_decode_ms: f64,
    /// 平均端到端延迟:发送开始 → 接收解码完成(毫秒,同机时钟)
    pub avg_e2e_latency_ms: f64,
    /// 传输的总字节数(base64 协议字节)
    pub total_transfer_bytes: u64,
    /// 抓屏源分辨率
    pub source_width: u32,
    pub source_height: u32,
    /// 编码输出分辨率
    pub frame_width: u32,
    pub frame_height: u32,
}

/// `dxgi-loop-frame` 事件负载:经回环链路到达接收端的标准编解码帧。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DxgiLoopFrameEvent {
    pub seq: u64,
    pub width: u32,
    pub height: u32,
    /// 是否关键帧(WebCodecs 首块必须为关键帧)
    pub key: bool,
    /// H.264 Annex-B 字节(标准编解码;前端经 WebCodecs 解码后绘制)
    pub data: Vec<u8>,
}

/// 入参收敛:时长 1..=60 秒,帧率 1..=60,目标尺寸 320x180..=1920x1920。纯函数便于测试。
fn clamp_params(
    seconds: u32,
    target_fps: u32,
    target_w: u32,
    target_h: u32,
) -> (u32, u32, u32, u32) {
    (
        seconds.clamp(1, 60),
        target_fps.clamp(1, 60),
        target_w.clamp(320, 1920),
        target_h.clamp(180, 1920),
    )
}

/// 发送端(真实抓屏 + H.264 编码 + 协议发送)统计。
struct SenderStats {
    frames_grabbed: u64,
    frames_sent: u64,
    capture_total: Duration,
    encode_total: Duration,
    send_total: Duration,
    encoder: String,
    source_w: u32,
    source_h: u32,
    enc_w: u32,
    enc_h: u32,
}

/// 接收端(协议读取 + FFmpeg 解码)统计。
#[derive(Default, Clone)]
struct RecvStats {
    frames_rendered: u64,
    decode_total: Duration,
    latency_total: Duration,
    transfer_bytes: u64,
    decoder_hwaccel: bool,
}

/// 建立一对本机回环 TCP 连接((发送端流, 接收端流)),走真实 TCP 栈与生产 framing。
async fn loopback_pair() -> Result<(TcpStream, TcpStream), String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("绑定回环端口失败: {e}"))?;
    let addr = listener
        .local_addr()
        .map_err(|e| format!("读取回环端口失败: {e}"))?;
    let client = TcpStream::connect(addr)
        .await
        .map_err(|e| format!("回环连接失败: {e}"))?;
    let (server, _) = listener
        .accept()
        .await
        .map_err(|e| format!("回环接受失败: {e}"))?;
    Ok((client, server))
}

/// 发送端循环:按目标帧率 真实抓屏 → H.264 编码 → 生产协议帧发送。
///
/// - 桌面静止导致单次抓屏超时时复用上一帧(与生产会话行为一致);
/// - 尚无任何历史帧且持续抓不到(无显示器/无活动桌面)超过 3 秒宽限期 → 显式报错;
/// - 编码器懒创建,首帧请求关键帧(WebCodecs 预览依赖首块为关键帧)。
#[cfg(target_os = "windows")]
async fn sender_loop(
    mut stream: TcpStream,
    monitor_id: u32,
    seconds: u32,
    target_fps: u32,
    target_w: u32,
    target_h: u32,
    pending: Arc<Mutex<HashMap<u64, Instant>>>,
) -> Result<SenderStats, String> {
    let interval = Duration::from_millis((1000u64 / u64::from(target_fps)).max(1));
    let start = Instant::now();
    let deadline = start + Duration::from_secs(u64::from(seconds));
    // 首帧宽限期:真实采集必须成功,超时仍未取到任何帧则判定环境不可采集
    let first_frame_grace = Duration::from_secs(3);
    let mut stats = SenderStats {
        frames_grabbed: 0,
        frames_sent: 0,
        capture_total: Duration::ZERO,
        encode_total: Duration::ZERO,
        send_total: Duration::ZERO,
        encoder: String::new(),
        source_w: 0,
        source_h: 0,
        enc_w: 0,
        enc_h: 0,
    };
    let mut last_frame: Option<(u32, u32, Vec<u8>)> = None;
    let mut seq: u64 = 0;
    // H.264 编码器(懒创建;源分辨率变化时重建)
    let mut enc: Option<crate::ffmpeg_hw::HwEncoder> = None;
    let mut enc_key: Option<(u32, u32, u32, u32, u32)> = None;

    while Instant::now() < deadline {
        let iter_start = Instant::now();

        // 1) 真实 DXGI 抓屏;失败时复用上一帧,无历史帧则在宽限期内重试
        let grab_start = Instant::now();
        match crate::capture::grab_frame_once(monitor_id) {
            Ok((w, h, rgb)) => {
                stats.frames_grabbed += 1;
                stats.source_w = w;
                stats.source_h = h;
                last_frame = Some((w, h, rgb));
            }
            Err(e) => {
                if last_frame.is_none() {
                    if iter_start.duration_since(start) < first_frame_grace {
                        tokio::time::sleep(Duration::from_millis(30)).await;
                        continue;
                    }
                    return Err(format!("真实抓屏失败(未取得任何首帧): {e}"));
                }
                log::debug!("[diagnostics] 单次抓屏超时,复用上一帧: {e}");
            }
        }
        stats.capture_total += grab_start.elapsed();
        let Some((w, h, rgb)) = last_frame.clone() else {
            continue;
        };

        // 2) H.264 编码(标准视频编解码;本链路禁止 JPEG)
        let key = (w, h, target_w, target_h, target_fps);
        if enc_key.as_ref() != Some(&key) {
            let enc_name = crate::ffmpeg_hw::preferred_encoder("h264")
                .ok_or_else(|| "无可用 H.264 编码器(FFmpeg DLL 缺失或初始化失败)".to_string())?;
            let mut opened = crate::ffmpeg_hw::HwEncoder::open(
                &enc_name,
                crate::ffmpeg_hw::codec_family_id("h264"),
                w,
                h,
                target_w,
                target_h,
                target_fps,
            )?;
            opened.request_keyframe();
            stats.encoder = enc_name;
            enc = Some(opened);
            enc_key = Some(key);
            log::info!("[diagnostics] H.264 编码器启用: {}", stats.encoder);
        }
        let encode_start = Instant::now();
        let Some(enc) = enc.as_mut() else {
            continue;
        };
        let Some((ew, eh, data, is_key)) = enc.encode_rgb(&rgb)
            .map_err(|e| format!("H.264 编码失败(seq={seq}): {e}"))?
        else {
            // 编码器缓冲中暂无输出(如 B 帧延迟),跳过本次发送
            continue;
        };
        stats.encode_total += encode_start.elapsed();
        stats.enc_w = ew;
        stats.enc_h = eh;

        // 3) 生产协议帧发送(base64 Annex-B;记录发送时刻供端到端延迟统计)
        let send_start = Instant::now();
        if let Ok(mut map) = pending.lock() {
            map.insert(seq, send_start);
        }
        write_msg(
            &mut stream,
            &Msg::Frame {
                w: ew,
                h: eh,
                seq,
                jpeg: base64::engine::general_purpose::STANDARD.encode(&data),
                dur: encode_start.elapsed().as_millis() as u32,
                codec: "h264".to_string(),
                key: is_key,
            },
        )
        .await
        .map_err(|e| format!("发送帧失败(seq={seq}): {e}"))?;
        stats.send_total += send_start.elapsed();
        stats.frames_sent += 1;
        seq += 1;

        // 4) 按目标帧率节流;节流被跳过时也强制让出执行权,避免接收端被饿死
        let elapsed = iter_start.elapsed();
        if elapsed < interval {
            tokio::time::sleep(interval - elapsed).await;
        } else {
            tokio::task::yield_now().await;
        }
    }
    // 收尾:对写半部 shutdown(FIN),接收端读到 EOF 自然结束
    let _ = stream.shutdown().await;
    Ok(stats)
}

/// 接收端循环:读协议帧 → base64 解码 → FFmpeg 解码(D3D11VA 硬解优先)→ 统计,
/// 并把到达接收端的编码帧经 `dxgi-loop-frame` 事件回传前端做 WebCodecs 预览。
async fn receiver_loop(
    mut stream: TcpStream,
    stats: Arc<Mutex<RecvStats>>,
    pending: Arc<Mutex<HashMap<u64, Instant>>>,
    seconds: u32,
    app: Option<AppHandle>,
) -> Result<(), String> {
    let start = Instant::now();
    // 兜底超时:发送端 FIN 后应自然 EOF,此处仅防异常悬挂
    let hard_timeout = Duration::from_secs(u64::from(seconds) + 5);
    #[cfg(target_os = "windows")]
    let mut decoder: Option<crate::ffmpeg_hw::HwDecoder> = None;
    loop {
        if start.elapsed() > hard_timeout {
            break;
        }
        let msg = match read_msg(&mut stream).await {
            Ok(m) => m,
            // EOF(半关闭传播)或错误 = 发送端已收尾
            Err(_) => break,
        };
        let Msg::Frame {
            seq,
            w,
            h,
            jpeg,
            codec,
            key,
            ..
        } = msg
        else {
            continue;
        };
        // 本诊断只接受标准视频编解码帧(H.264/H.265);出现其他类型视为链路异常
        if codec != "h264" && codec != "hevc" {
            return Err(format!("收到非标准编解码帧(codec={codec}),本诊断禁止 JPEG"));
        }
        let data = base64::engine::general_purpose::STANDARD
            .decode(&jpeg)
            .map_err(|e| format!("帧 base64 解码失败(seq={seq}): {e}"))?;

        // 回传前端预览(即链路实际传输内容,前端经 WebCodecs 解码绘制)
        if let Some(app) = &app {
            let _ = app.emit(
                "dxgi-loop-frame",
                DxgiLoopFrameEvent {
                    seq,
                    width: w,
                    height: h,
                    key,
                    data: data.clone(),
                },
            );
        }

        // 本机渲染近似:FFmpeg 解码(D3D11VA 硬解优先,自动软件回退)
        #[cfg(target_os = "windows")]
        {
            let t0 = Instant::now();
            if decoder.is_none() {
                let d =
                    crate::ffmpeg_hw::HwDecoder::open(crate::ffmpeg_hw::codec_family_id(&codec))?;
                let hwaccel = d.using_hwaccel();
                log::info!("[diagnostics] H.264 解码器就绪(hwaccel={hwaccel})");
                decoder = Some(d);
                if let Ok(mut s) = stats.lock() {
                    s.decoder_hwaccel = hwaccel;
                }
            }
            let rendered = decoder
                .as_mut()
                .and_then(|d| d.decode(&data).ok().flatten());
            let decode_ms = t0.elapsed();
            let Some((_dw, _dh, _rgb)) = rendered else {
                log::warn!("[diagnostics] 帧解码无输出(seq={seq}),跳过");
                continue;
            };
            let latency = pending
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&seq)
                .and_then(|sent_at| t0.checked_duration_since(sent_at))
                .unwrap_or_default();
            if let Ok(mut s) = stats.lock() {
                s.frames_rendered += 1;
                s.decode_total += decode_ms;
                s.latency_total += latency;
                s.transfer_bytes += jpeg.len() as u64;
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (&seq, &w, &h, &key, &stats, &pending);
        }
    }
    Ok(())
}

/// 诊断主链路:`app` 为 Some 时向前端回传 `dxgi-loop-frame` 事件(None 供测试驱动)。
#[cfg(target_os = "windows")]
async fn run_chain(
    app: Option<AppHandle>,
    monitor_id: u32,
    seconds: u32,
    target_fps: u32,
    target_w: u32,
    target_h: u32,
) -> Result<DxgiLoopbackReport, String> {
    if !crate::ffmpeg_hw::available() {
        return Err("FFmpeg DLL 未加载,标准编解码(H.264)不可用".to_string());
    }
    let (seconds, target_fps, target_w, target_h) =
        clamp_params(seconds, target_fps, target_w, target_h);

    let (send_stream, recv_stream) = loopback_pair().await?;
    let pending: Arc<Mutex<HashMap<u64, Instant>>> = Arc::new(Mutex::new(HashMap::new()));
    let recv_stats = Arc::new(Mutex::new(RecvStats::default()));

    let wall_start = Instant::now();
    let sender = tokio::spawn(sender_loop(
        send_stream,
        monitor_id,
        seconds,
        target_fps,
        target_w,
        target_h,
        pending.clone(),
    ));
    let receiver = tokio::spawn(receiver_loop(
        recv_stream,
        recv_stats.clone(),
        pending,
        seconds,
        app,
    ));

    // 先等发送端(先暴露采集/编码侧错误);其流随即关闭,接收端自然 EOF 收尾
    let send = sender
        .await
        .map_err(|e| format!("发送任务异常: {e}"))??;
    receiver
        .await
        .map_err(|e| format!("接收任务异常: {e}"))??;
    let recv = recv_stats.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let wall_secs = wall_start.elapsed().as_secs_f64();
    let rendered = recv.frames_rendered.max(1) as f64;
    let sent = send.frames_sent.max(1) as f64;

    Ok(DxgiLoopbackReport {
        codec: "h264".to_string(),
        encoder: send.encoder,
        decoder_hwaccel: recv.decoder_hwaccel,
        monitor_id,
        seconds,
        target_fps,
        frames_grabbed: send.frames_grabbed,
        frames_sent: send.frames_sent,
        frames_rendered: recv.frames_rendered,
        realtime_fps: recv.frames_rendered as f64 / wall_secs.max(f64::MIN_POSITIVE),
        avg_capture_ms: send.capture_total.as_secs_f64() * 1000.0 / sent,
        avg_encode_ms: send.encode_total.as_secs_f64() * 1000.0 / sent,
        avg_send_ms: send.send_total.as_secs_f64() * 1000.0 / sent,
        avg_decode_ms: recv.decode_total.as_secs_f64() * 1000.0 / rendered,
        avg_e2e_latency_ms: recv.latency_total.as_secs_f64() * 1000.0 / rendered,
        total_transfer_bytes: recv.transfer_bytes,
        source_width: send.source_w,
        source_height: send.source_h,
        frame_width: send.enc_w,
        frame_height: send.enc_h,
    })
}

/// 非 Windows:编译占位(真实 DXGI 采集与 FFmpeg 链路仅 Windows 支持)。
#[cfg(not(target_os = "windows"))]
async fn run_chain(
    _app: Option<AppHandle>,
    monitor_id: u32,
    seconds: u32,
    target_fps: u32,
    target_w: u32,
    target_h: u32,
) -> Result<DxgiLoopbackReport, String> {
    let _ = (monitor_id, seconds, target_fps, target_w, target_h);
    Err("DXGI 回传诊断仅 Windows 支持".to_string())
}

/// 设置页「诊断」命令:DXGI 回传自检(真实本机采集,H.264 标准编解码,禁止 JPEG)。
#[tauri::command]
pub async fn run_dxgi_loopback(
    app: AppHandle,
    monitor_id: u32,
    seconds: u32,
    target_fps: u32,
    target_width: u32,
    target_height: u32,
) -> Result<DxgiLoopbackReport, String> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err("已有诊断在运行中,请稍后再试".to_string());
    }
    let result = run_chain(
        Some(app),
        monitor_id,
        seconds,
        target_fps,
        target_width,
        target_height,
    )
    .await;
    RUNNING.store(false, Ordering::SeqCst);
    match &result {
        Ok(report) => crate::operation_log::op_log(
            "diagnostics",
            "run_dxgi_loopback",
            &format!(
                "monitor={monitor_id} {}s@{}fps 抓帧{} 发送{} 解码{} {:.1}fps",
                report.seconds,
                report.target_fps,
                report.frames_grabbed,
                report.frames_sent,
                report.frames_rendered,
                report.realtime_fps
            ),
        ),
        Err(e) => crate::operation_log::op_log(
            "diagnostics",
            "run_dxgi_loopback",
            &format!("monitor={monitor_id} 失败: {e}"),
        ),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_params_bounds_inputs() {
        // 下限收敛
        assert_eq!(clamp_params(0, 0, 0, 0), (1, 1, 320, 180));
        // 上限收敛
        assert_eq!(clamp_params(3600, 240, 7680, 4320), (60, 60, 1920, 1920));
        // 合法值原样保留
        assert_eq!(clamp_params(5, 30, 1280, 720), (5, 30, 1280, 720));
    }

    /// 本机真实验证:DXGI 采集 → H.264 编码 → TCP 回环 → 解码全链路(需活动桌面,默认忽略)。
    ///
    /// 多线程 flavor:发送端的同步 DXGI 抓屏会阻塞工作线程,须与接收端并行调度。
    /// 运行:`cargo test -- --ignored dxgi_loopback_real_chain --nocapture`
    #[cfg(target_os = "windows")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore]
    async fn dxgi_loopback_real_chain() {
        let report = run_chain(None, 0, 3, 30, 1280, 720)
            .await
            .expect("DXGI 回传链路应跑通");
        println!(
            "[diag] 编码器={} 硬解={} 抓帧 {} → 发送 {} → 解码 {} | 实时 {:.1} fps | 抓屏 {:.2}ms 编码 {:.2}ms 发送 {:.2}ms 解码 {:.2}ms 端到端 {:.2}ms",
            report.encoder,
            report.decoder_hwaccel,
            report.frames_grabbed,
            report.frames_sent,
            report.frames_rendered,
            report.realtime_fps,
            report.avg_capture_ms,
            report.avg_encode_ms,
            report.avg_send_ms,
            report.avg_decode_ms,
            report.avg_e2e_latency_ms,
        );
        assert!(report.frames_grabbed > 0, "未抓到任何真实帧");
        assert!(report.frames_sent > 0, "未发送任何帧");
        assert!(report.frames_rendered > 0, "未解码任何帧");
    }
}
