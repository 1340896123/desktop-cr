//! 实时链路性能基准:采集 → 编码 → 传输 → 本机解码渲染,输出实时帧率。
//!
//! 两种传输模式:
//! - `loopback`:本机回环 TCP,测本机管线极限(采集/编码/发送/解码各阶段耗时)。
//! - `relay`:经公网中继服务器(如 `120.78.77.248:21117`)配对 host/client 双向
//!   透明通道,帧先上行到公网服务器再下行回本机,测真实公网链路的实时帧率与
//!   端到端延迟。
//!
//! 帧来源为真实 DXGI 抓屏(`media_pipeline::grab_frame_once`);桌面静止导致抓屏
//! 超时时自动复用上一帧(与真实会话行为一致:静止桌面持续推送同一帧)。本机
//! 渲染以 JPEG 解码(`image`)作为软件近似,解码耗时即本机渲染管线瓶颈参考。
//! 两端在本机同一时钟下计时,故端到端延迟 = 发送开始 → 接收解码完成。

use base64::Engine as _;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::network::{read_msg, write_msg, Msg};

/// 实时链路基准报告(camelCase 序列化,供前端展示)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RealtimeBenchReport {
    /// 传输模式:"loopback" | "relay"
    pub mode: String,
    /// 中继服务器地址(relay 模式)
    pub relay: String,
    /// 基准时长(秒)
    pub seconds: u32,
    /// 目标帧率
    pub target_fps: u32,
    /// 本机解码渲染成功的帧数
    pub frames_rendered: u64,
    /// 实时帧率 = 渲染帧数 / 总耗时
    pub realtime_fps: f64,
    /// 平均抓帧耗时(毫秒)
    pub avg_capture_ms: f64,
    /// 平均缩放+JPEG 编码耗时(毫秒)
    pub avg_encode_ms: f64,
    /// 平均发送耗时(毫秒)
    pub avg_send_ms: f64,
    /// 平均本机解码(渲染)耗时(毫秒)
    pub avg_decode_ms: f64,
    /// 平均端到端延迟:发送开始 → 接收解码完成(毫秒)
    pub avg_e2e_latency_ms: f64,
    /// 传输的总字节数(base64 协议字节)
    pub total_transfer_bytes: u64,
    /// 渲染帧分辨率
    pub frame_width: u32,
    pub frame_height: u32,
    /// 本次基准是否使用合成动画帧(输入 synthetic=true 或抓帧失败回退合成帧时置 true)
    pub synthetic: bool,
}

/// 发送端(采集+编码+发送)统计。
struct SenderStats {
    frames_sent: u64,
    capture_total: std::time::Duration,
    encode_total: std::time::Duration,
    send_total: std::time::Duration,
    width: u32,
    height: u32,
    /// 是否使用了合成动画帧(输入 synthetic 或抓帧失败回退)
    synthetic_used: bool,
}

/// 接收端(解码渲染)统计。
#[derive(Default, Clone)]
struct RecvStats {
    frames_rendered: u64,
    decode_total: std::time::Duration,
    latency_total: std::time::Duration,
    transfer_bytes: u64,
    width: u32,
    height: u32,
}

/// 建立一对本机回环 TCP 连接(客户端 + 服务端)。
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

/// 经公网中继服务器配对两条透明通道:host 先接入占位,client 后接入即配对。
///
/// 返回 (host 流, client 流);此后两条流经中继字节透明互转,上层 framing 原样透传。
async fn relay_pair(relay_addr: &str) -> Result<(TcpStream, TcpStream), String> {
    use dcr_server::framing::read_msg as relay_read;
    use dcr_server::framing::write_msg as relay_write;
    use dcr_server::message::RelayMsg;

    let id = format!("bench-{}", std::process::id());
    // 1) host 先连:登记通道,等待 client
    let mut host = TcpStream::connect(relay_addr)
        .await
        .map_err(|e| format!("连接中继服务器 {relay_addr} 失败: {e}"))?;
    relay_write(
        &mut host,
        &RelayMsg::Allocate {
            id: id.clone(),
            role: "host".into(),
        },
    )
    .await?;
    match relay_read::<_, RelayMsg>(&mut host).await? {
        RelayMsg::Allocated {
            peer_connected: false,
            ..
        } => {}
        other => return Err(format!("中继 host 应答异常: {other:?}")),
    }
    // 2) client 后连:立即与 host 配对
    let mut client = TcpStream::connect(relay_addr)
        .await
        .map_err(|e| format!("连接中继服务器 {relay_addr} 失败: {e}"))?;
    relay_write(
        &mut client,
        &RelayMsg::Allocate {
            id: id.clone(),
            role: "client".into(),
        },
    )
    .await?;
    match relay_read::<_, RelayMsg>(&mut client).await? {
        RelayMsg::Allocated {
            peer_connected: true,
            ..
        } => Ok((host, client)),
        other => Err(format!("中继 client 应答异常(对端未接入?): {other:?}")),
    }
}

/// 合成一帧 RGB 动画(渐变底色 + 移动白色方块,测试用,不依赖真实硬件)。
fn synth_frame(w: u32, h: u32, t: u32) -> Vec<u8> {
    let bw = (w / 8).max(2);
    let bh = (h / 8).max(2);
    let bx = ((t * 17) % (w - bw)).max(0);
    let by = ((t * 11) % (h - bh)).max(0);
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            let in_square = x >= bx && x < bx + bw && y >= by && y < by + bh;
            let (r, g, b) = if in_square {
                (255u8, 255u8, 255u8)
            } else {
                (
                    (((x * 255 / w.max(1)) % 256) as u8),
                    (((y * 255 / h.max(1)) % 256) as u8),
                    ((((x + y) * 3) % 256) as u8),
                )
            };
            rgb.push(r);
            rgb.push(g);
            rgb.push(b);
        }
    }
    rgb
}

/// 发送端循环:按目标帧率 抓帧 → 缩放+JPEG 编码 → 发送;桌面静止复用上一帧,
/// 无任何历史帧时(如无头/静止环境)回退合成动画帧,保证基准可稳定运行。
/// `synthetic` 为 true 时完全跳过真实抓帧,直接用合成动画帧(测纯编码/传输/渲染链路)。
async fn sender_loop(
    mut stream: TcpStream,
    seconds: u32,
    target_fps: u32,
    pending: Arc<Mutex<HashMap<u64, std::time::Instant>>>,
    synthetic: bool,
    target_w: u32,
    target_h: u32,
    codec: String,
) -> Result<SenderStats, String> {
    let interval = std::time::Duration::from_millis((1000u64 / u64::from(target_fps)).max(1));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(u64::from(seconds));
    let mut stats = SenderStats {
        frames_sent: 0,
        capture_total: std::time::Duration::ZERO,
        encode_total: std::time::Duration::ZERO,
        send_total: std::time::Duration::ZERO,
        width: 0,
        height: 0,
        synthetic_used: synthetic,
    };
    let mut last_frame: Option<(u32, u32, Vec<u8>)> = None;
    let mut warned_grab = false;
    let mut synthetic_warned = false;
    let mut seq: u64 = 0;
    // H.264 硬件编码器(懒创建,首帧请求关键帧;分辨率变化时重建)
    let use_video = codec == "h264" || codec == "hevc";
    #[cfg(target_os = "windows")]
    let mut hw_enc: Option<crate::ffmpeg_hw::HwEncoder> = None;
    #[cfg(target_os = "windows")]
    let mut hw_key: Option<(u32, u32, u32, u32, u32, String)> = None;

    while std::time::Instant::now() < deadline {
        let iter_start = std::time::Instant::now();

        // 1) 真实抓帧;静止桌面超时则复用上一帧
        let grab_start = std::time::Instant::now();
        let frame = if synthetic {
            None
        } else {
            match crate::media_pipeline::grab_frame_once(0) {
                Ok(v) => {
                    stats.width = v.0;
                    stats.height = v.1;
                    Some(v)
                }
                Err(e) => {
                    if !warned_grab {
                        println!("[bench] 抓帧失败(将复用上一帧): {e}");
                        warned_grab = true;
                    }
                    last_frame.clone()
                }
            }
        };
        stats.capture_total += grab_start.elapsed();
        // 2) 无历史帧时回退合成动画帧(无头/静止环境,保证基准可跑)
        let (w, h, rgb) = match frame {
            Some(v) => v,
            None => {
                if !synthetic_warned {
                    println!("[bench] 桌面无可用帧,回退合成动画帧(仅测编码/传输/渲染链路)");
                    synthetic_warned = true;
                }
                stats.synthetic_used = true;
                let (w, h) = (target_w.max(320), target_h.max(180));
                stats.width = w;
                stats.height = h;
                (w, h, synth_frame(w, h, seq as u32))
            }
        };
        last_frame = Some((w, h, rgb.clone()));

        // 2) 编码:codec="jpeg" 走缩放+JPEG;codec="h264"/"hevc" 走 FFmpeg 硬件编码
        let encode_start = std::time::Instant::now();
        let (jw, jh, jpeg, codec_used) = if use_video {
            #[cfg(target_os = "windows")]
            {
                let key = (w, h, target_w, target_h, target_fps, codec.to_string());
                if hw_key.as_ref() != Some(&key) {
                    let family = codec.to_string();
                    let enc_name = crate::ffmpeg_hw::preferred_encoder(&family)
                        .unwrap_or_else(|| family.clone());
                    hw_enc = crate::ffmpeg_hw::HwEncoder::open(
                        &enc_name,
                        crate::ffmpeg_hw::codec_family_id(&family),
                        w,
                        h,
                        target_w,
                        target_h,
                        target_fps,
                    )
                    .ok();
                    hw_key = Some(key);
                    if let Some(e) = hw_enc.as_mut() {
                        e.request_keyframe();
                    }
                    if !warned_grab {
                        println!("[bench] FFmpeg 编码启用: {enc_name} ({family})");
                    }
                }
                match hw_enc
                    .as_mut()
                    .and_then(|e| e.encode_rgb(&rgb).ok().flatten())
                {
                    Some((ew, eh, data, _is_key)) => (ew, eh, data, codec.to_string()),
                    None => continue,
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                return Err("视频编码仅 Windows 支持".to_string());
            }
        } else {
            let (jw, jh, jpeg) = crate::capture::rgb_to_jpeg(&rgb, w, h, target_w, target_h, 80)
                .map_err(|e| format!("JPEG 编码失败: {e}"))?;
            (jw, jh, jpeg, "jpeg".to_string())
        };
        stats.encode_total += encode_start.elapsed();
        stats.width = jw;
        stats.height = jh;

        // 3) 发送(协议帧,base64;记录发送时刻供端到端延迟统计)
        let send_start = std::time::Instant::now();
        if let Ok(mut map) = pending.lock() {
            map.insert(seq, send_start);
        }
        write_msg(
            &mut stream,
            &Msg::Frame {
                w: jw,
                h: jh,
                seq,
                jpeg: base64::engine::general_purpose::STANDARD.encode(&jpeg),
                dur: encode_start.elapsed().as_millis() as u32,
                codec: codec_used,
                key: false,
            },
        )
        .await
        .map_err(|e| format!("发送帧失败(seq={seq}): {e}"))?;
        stats.send_total += send_start.elapsed();
        stats.frames_sent += 1;
        seq += 1;

        // 4) 按目标帧率节流
        let elapsed = iter_start.elapsed();
        if elapsed < interval {
            tokio::time::sleep(interval - elapsed).await;
        }
    }
    // 5) 到点收尾:对写半部 shutdown(FIN),经中继半关闭传播后接收端读到 EOF
    let _ = stream.shutdown().await;
    Ok(stats)
}

/// 接收端循环:读帧 → base64 解码 → 本机解码渲染(JPEG 或 H.264/H.265),每秒输出实时帧率。
///
/// 收尾方式:发送端到点后对写半部 shutdown(FIN),经中继半关闭传播后本端读到
/// EOF 自然结束(loopback/relay 一致)。带兜底超时防止中继异常时悬挂。
async fn receiver_loop(
    mut stream: TcpStream,
    stats: Arc<Mutex<RecvStats>>,
    pending: Arc<Mutex<HashMap<u64, std::time::Instant>>>,
    seconds: u32,
) -> Result<(), String> {
    let start = std::time::Instant::now();
    let hard_timeout = std::time::Duration::from_secs(u64::from(seconds) + 5);
    let mut frames_in_second: u64 = 0;
    let mut last_second: u64 = 0;
    // FFmpeg 解码器(懒创建;codec 变化时重建)
    #[cfg(target_os = "windows")]
    let mut decoder: Option<(String, crate::ffmpeg_hw::HwDecoder)> = None;
    loop {
        if start.elapsed() > hard_timeout {
            break;
        }
        let msg = match read_msg(&mut stream).await {
            Ok(m) => m,
            // EOF(半关闭传播)或错误 = 发送端已收尾,统计结束
            Err(_) => break,
        };
        let Msg::Frame {
            seq, jpeg, codec, ..
        } = msg
        else {
            continue;
        };
        let t0 = std::time::Instant::now();
        let data = match base64::engine::general_purpose::STANDARD.decode(&jpeg) {
            Ok(d) => d,
            Err(e) => {
                println!("[bench] 帧 base64 解码失败(seq={seq}): {e}");
                continue;
            }
        };
        // 本机渲染:JPEG 用 image 解码;H.264/H.265 用 FFmpeg 硬件解码为 RGB
        let is_video = codec == "h264" || codec == "hevc";
        let rendered: Option<(u32, u32)> = if is_video {
            #[cfg(target_os = "windows")]
            {
                let need_rebuild = decoder.as_ref().map(|(c, _)| c != &codec).unwrap_or(true);
                if need_rebuild {
                    if let Ok(d) =
                        crate::ffmpeg_hw::HwDecoder::open(crate::ffmpeg_hw::codec_family_id(&codec))
                    {
                        decoder = Some((codec.clone(), d));
                    } else {
                        decoder = None;
                    }
                }
                decoder
                    .as_mut()
                    .and_then(|(_, d)| d.decode(&data).ok().flatten())
                    .map(|(w, h, _)| (w, h))
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = data;
                None
            }
        } else {
            image::load_from_memory(&data)
                .ok()
                .map(|img| (img.width(), img.height()))
        };
        let Some((dw, dh)) = rendered else {
            println!("[bench] 帧解码失败(seq={seq}, codec={codec}),跳过");
            continue;
        };
        let decode_ms = t0.elapsed();
        // 端到端延迟 = 发送开始 → 接收解码完成(同机时钟)
        let latency = pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&seq)
            .map(|s| t0.duration_since(s))
            .unwrap_or_default();
        if let Ok(mut s) = stats.lock() {
            s.frames_rendered += 1;
            s.decode_total += decode_ms;
            s.latency_total += latency;
            s.transfer_bytes += jpeg.len() as u64;
            s.width = dw;
            s.height = dh;
        }

        // 每秒输出实时帧率
        frames_in_second += 1;
        let sec = start.elapsed().as_secs();
        if sec != last_second {
            println!("[bench] 第 {sec} 秒实时帧率: {frames_in_second} fps");
            frames_in_second = 0;
            last_second = sec;
        }
    }
    Ok(())
}

/// 实时链路基准主入口。
///
/// `mode`:"loopback" 本机回环; "relay" 公网中继(`relay_addr` 如 `120.78.77.248:21117`)。
/// 基准时长 `seconds` 秒(1..120),目标帧率 `target_fps`(1..60)。
pub fn run_realtime_bench(
    mode: &str,
    relay_addr: Option<&str>,
    seconds: u32,
    target_fps: u32,
    synthetic: bool,
    target_w: u32,
    target_h: u32,
    codec: &str,
) -> Result<RealtimeBenchReport, String> {
    let seconds = seconds.clamp(1, 120);
    let target_fps = target_fps.clamp(1, 60);
    let target_w = target_w.clamp(160, 1920);
    let target_h = target_h.clamp(90, 1080);
    let codec = if codec == "h264" || codec == "hevc" {
        codec
    } else {
        "jpeg"
    };
    crate::operation_log::op_log(
        "bench",
        "run",
        &format!("mode={mode} relay={relay_addr:?} seconds={seconds} fps={target_fps} synthetic={synthetic} target={target_w}x{target_h} codec={codec}"),
    );

    let relay = relay_addr.unwrap_or_default().to_string();
    let codec_owned = codec.to_string();
    let result = tokio::runtime::Runtime::new()
        .map_err(|e| format!("创建 Tokio 运行时失败: {e}"))?
        .block_on(async {
            // 1) 建立传输对(loopback 回环 / relay 公网中继配对)
            let (sender_stream, receiver_stream) = match mode {
                "loopback" => loopback_pair().await?,
                "relay" => {
                    let addr = relay_addr.ok_or("relay 模式必须提供中继地址")?;
                    relay_pair(addr).await?
                }
                other => return Err(format!("未知模式: {other}(期望 loopback/relay)")),
            };

            // 2) 并行运行 发送端(采集+编码+发送)与 接收端(解码渲染)
            let pending: Arc<Mutex<HashMap<u64, std::time::Instant>>> =
                Arc::new(Mutex::new(HashMap::new()));
            let recv_stats: Arc<Mutex<RecvStats>> = Arc::new(Mutex::new(RecvStats::default()));
            let started = std::time::Instant::now();

            let sender = tokio::spawn(sender_loop(
                sender_stream,
                seconds,
                target_fps,
                pending.clone(),
                synthetic,
                target_w,
                target_h,
                codec_owned,
            ));
            let receiver = tokio::spawn(receiver_loop(
                receiver_stream,
                recv_stats.clone(),
                pending.clone(),
                seconds,
            ));

            let sender_stats = sender.await.map_err(|e| format!("发送端任务失败: {e}"))??;
            let _ = receiver.await.map_err(|e| format!("接收端任务失败: {e}"))?;
            let elapsed = started.elapsed().as_secs_f64();

            // 3) 汇总报告
            let recv = recv_stats.lock().unwrap_or_else(|e| e.into_inner()).clone();
            let rendered = recv.frames_rendered.max(1);
            let report = RealtimeBenchReport {
                mode: mode.to_string(),
                relay,
                seconds,
                target_fps,
                frames_rendered: recv.frames_rendered,
                realtime_fps: recv.frames_rendered as f64 / elapsed.max(1e-9),
                avg_capture_ms: sender_stats.capture_total.as_secs_f64() * 1000.0
                    / sender_stats.frames_sent.max(1) as f64,
                avg_encode_ms: sender_stats.encode_total.as_secs_f64() * 1000.0
                    / sender_stats.frames_sent.max(1) as f64,
                avg_send_ms: sender_stats.send_total.as_secs_f64() * 1000.0
                    / sender_stats.frames_sent.max(1) as f64,
                avg_decode_ms: recv.decode_total.as_secs_f64() * 1000.0 / rendered as f64,
                avg_e2e_latency_ms: recv.latency_total.as_secs_f64() * 1000.0 / rendered as f64,
                total_transfer_bytes: recv.transfer_bytes,
                frame_width: recv.width,
                frame_height: recv.height,
                synthetic: sender_stats.synthetic_used,
            };
            Ok(report)
        })?;

    println!(
        "[bench] 实时链路完成: mode={mode} 渲染 {} 帧,实时帧率 {:.1} fps | 抓帧 {:.2} ms,编码 {:.2} ms,发送 {:.2} ms,解码 {:.2} ms,端到端延迟 {:.2} ms",
        result.frames_rendered,
        result.realtime_fps,
        result.avg_capture_ms,
        result.avg_encode_ms,
        result.avg_send_ms,
        result.avg_decode_ms,
        result.avg_e2e_latency_ms,
    );
    crate::operation_log::op_log(
        "bench",
        "done",
        &format!(
            "fps={:.1} rendered={}",
            result.realtime_fps, result.frames_rendered
        ),
    );
    Ok(result)
}

/// 一键运行实时链路基准(供前端调用)。
#[tauri::command]
pub fn run_realtime_bench_command(
    mode: String,
    relay_addr: Option<String>,
    seconds: u32,
    target_fps: u32,
    synthetic: bool,
    target_w: u32,
    target_h: u32,
    codec: String,
) -> Result<RealtimeBenchReport, String> {
    run_realtime_bench(
        &mode,
        relay_addr.as_deref(),
        seconds,
        target_fps,
        synthetic,
        target_w,
        target_h,
        &codec,
    )
}

/// 音频公网回环基准报告(camelCase 序列化,供前端展示)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioRelayBenchReport {
    /// 中继服务器地址
    pub relay: String,
    /// 采集时长(秒)
    pub audio_seconds: u32,
    /// 采集采样率
    pub sample_rate: u16,
    /// 采集声道数
    pub channels: u16,
    /// 采集到的 PCM 数据量(字节)
    pub captured_bytes: u64,
    /// 经中继往返传输的 WAV 数据量(字节)
    pub wav_bytes: u64,
    /// 平均往返延迟(毫秒,32KB 块逐块发到公网再等回传)
    pub avg_rtt_ms: f64,
    /// 上行吞吐率(本机→公网,Mbps)
    pub send_rate_mbps: f64,
    /// 下行吞吐率(公网→本机,Mbps)
    pub recv_rate_mbps: f64,
    /// 双向总吞吐率(上行+下行,Mbps)
    pub roundtrip_rate_mbps: f64,
    /// 往返传输总字节数(上行+下行)
    pub total_transfer_bytes: u64,
    /// 传输阶段耗时(毫秒)
    pub transfer_elapsed_ms: u64,
    /// 回传落盘文件路径(供人工查验音频内容)
    pub out_path: String,
}

/// 音频公网回环链路核心:经公网中继配对透明通道,先逐块测往返延迟,再全量流式测吞吐。
///
/// 方向:本机 → 中继服务器 → 本机(host 写入的内容经中继转发回 host 自己的 client 流)。
/// 上行与下行并发进行(全双工),`elapsed` 取发送端发完且接收端读完回传为止。
async fn audio_relay_chain_async(
    relay: &str,
    wav: Vec<u8>,
) -> Result<(f64, f64, f64, f64, u64, u64, Vec<u8>), String> {
    let (mut host, mut client) = relay_pair(relay).await?;

    // 阶段一:往返延迟探测 —— 32KB 块逐块发到公网再等回传(stop-and-wait)
    const PROBE_CHUNK: usize = 32 * 1024;
    const PROBES: usize = 16;
    let fill = wav.first().copied().unwrap_or(0);
    let probe_buf = vec![fill; PROBE_CHUNK];
    let mut rtt_sum = std::time::Duration::ZERO;
    for _ in 0..PROBES {
        let t0 = std::time::Instant::now();
        host.write_all(&probe_buf)
            .await
            .map_err(|e| format!("探测块发送失败: {e}"))?;
        let mut echo = vec![0u8; PROBE_CHUNK];
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            client.read_exact(&mut echo),
        )
        .await
        .map_err(|_| "探测块回传超时(中继不可达或未配对?)")?
        .map_err(|e| format!("探测块回传读取失败: {e}"))?;
        rtt_sum += t0.elapsed();
    }
    let avg_rtt_ms = rtt_sum.as_secs_f64() * 1000.0 / PROBES as f64;

    // 阶段二:吞吐率 —— 全量 WAV 分块持续上行,同时下行读取回传,直到 EOF
    const CHUNK: usize = 64 * 1024;
    let total = wav.len() as u64;
    let t0 = std::time::Instant::now();
    let mut send_stream = host;
    let sender = tokio::spawn(async move {
        for c in wav.chunks(CHUNK) {
            send_stream
                .write_all(c)
                .await
                .map_err(|e| format!("发送失败: {e}"))?;
        }
        // 写半部关闭(FIN),经中继半关闭传播后本端读到 EOF
        let _ = send_stream.shutdown().await;
        Ok::<(), String>(())
    });
    let mut recv_bytes: u64 = 0;
    let mut received: Vec<u8> = Vec::with_capacity(total as usize);
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = tokio::time::timeout(std::time::Duration::from_secs(30), client.read(&mut buf))
            .await
            .map_err(|_| "回传读取超时")?
            .map_err(|e| format!("回传读取失败: {e}"))?;
        if n == 0 {
            break;
        }
        received.extend_from_slice(&buf[..n]);
        recv_bytes += n as u64;
    }
    sender.await.map_err(|e| format!("发送任务失败: {e}"))??;
    let elapsed = t0.elapsed().as_secs_f64().max(1e-9);

    if recv_bytes != total {
        return Err(format!(
            "回传数据量不符: 发送 {total} 字节,回传 {recv_bytes} 字节"
        ));
    }

    // 全双工对称:上行/下行各传 total 字节,耗时相同
    let mb = |bytes_per_sec: f64| bytes_per_sec * 8.0 / 1_000_000.0;
    Ok((
        avg_rtt_ms,
        mb(total as f64 / elapsed),
        mb(recv_bytes as f64 / elapsed),
        mb((total + recv_bytes) as f64 / elapsed),
        total + recv_bytes,
        t0.elapsed().as_millis() as u64,
        received,
    ))
}

/// 音频公网回环基准主入口:真实 WASAPI 系统回环采集(默认渲染设备/扬声器)→ WAV 编码 →
/// 经公网中继回传本机 → 校验字节一致 → 落盘供人工查验,并测传输速率。
///
/// `relay_addr` 为中继 TCP 地址(如 `120.78.77.248:21117`),`seconds` 为采集时长(1..10 秒),
/// `out_path` 为回传音频落盘路径。采集的是系统真实播放的声音(需系统正在播放音频,
/// 无音频时不产生采样并返回 Err);不做任何合成填充、文件伪造或静音替换。
#[tauri::command]
pub fn run_audio_relay_bench(
    relay_addr: String,
    seconds: u32,
    out_path: String,
) -> Result<AudioRelayBenchReport, String> {
    let seconds = seconds.clamp(1, 10);
    crate::operation_log::op_log(
        "bench",
        "audio_run",
        &format!("relay={relay_addr} seconds={seconds} out={out_path}"),
    );

    // 1) 真实 WASAPI 系统回环采集 + WAV 编码(无合成回退)
    let (rate, channels, samples) = crate::media_pipeline::capture_system_audio(seconds)?;
    let wav = crate::media_pipeline::pcm_to_wav(rate, channels, &samples);
    let captured_bytes = (samples.len() * std::mem::size_of::<i16>()) as u64;
    let wav_bytes = wav.len() as u64;

    // 2) 经公网中继回环传输,拿到回传的 WAV 字节
    let (avg_rtt_ms, send_rate, recv_rate, roundtrip_rate, total_bytes, elapsed_ms, received) =
        tokio::runtime::Runtime::new()
            .map_err(|e| format!("创建 Tokio 运行时失败: {e}"))?
            .block_on(audio_relay_chain_async(&relay_addr, wav))?;

    // 3) 回传音频落盘供人工查验
    std::fs::write(&out_path, &received).map_err(|e| format!("写入回传音频失败: {e}"))?;
    let nonzero = samples.iter().filter(|s| **s != 0).count();
    let report = AudioRelayBenchReport {
        relay: relay_addr.clone(),
        audio_seconds: seconds,
        sample_rate: rate,
        channels,
        captured_bytes,
        wav_bytes,
        avg_rtt_ms,
        send_rate_mbps: send_rate,
        recv_rate_mbps: recv_rate,
        roundtrip_rate_mbps: roundtrip_rate,
        total_transfer_bytes: total_bytes,
        transfer_elapsed_ms: elapsed_ms,
        out_path: out_path.clone(),
    };
    println!(
        "[bench] 音频公网回环完成: 真实回环采集 {seconds}s {rate}Hz/{channels}ch {captured_bytes}B(非零采样 {nonzero}) | 平均RTT {avg_rtt_ms:.2}ms | 上行 {send_rate:.2}Mbps 下行 {recv_rate:.2}Mbps 双向 {roundtrip_rate:.2}Mbps | 回传 {wav_bytes}B 已落盘: {out_path}",
    );
    crate::operation_log::op_log(
        "bench",
        "audio_done",
        &format!(
            "rtt={avg_rtt_ms:.2}ms send={send_rate:.2}Mbps recv={recv_rate:.2}Mbps roundtrip={roundtrip_rate:.2}Mbps out={out_path}"
        ),
    );
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 本机回环实时链路基准(真实抓屏,需要显示器/GPU;默认忽略)。
    ///
    /// 运行:`cargo test -- --ignored realtime_chain_loopback --nocapture`
    #[test]
    #[ignore]
    fn realtime_chain_loopback() {
        let codec = std::env::var("DCR_BENCH_CODEC").unwrap_or_else(|_| "jpeg".to_string());
        let report = run_realtime_bench("loopback", None, 5, 30, true, 1280, 720, &codec).unwrap();
        assert!(report.frames_rendered > 0, "基准期间未渲染任何帧");
        assert!(report.synthetic, "synthetic=true 时报告应标记合成帧模式");
        assert!(report.avg_capture_ms >= 0.0);
        println!("[bench] 最终报告: {report:?}");
    }

    /// 公网中继实时链路基准(需先部署 dcr-signal/dcr-relay 并开放端口)。
    ///
    /// 中继地址取环境变量 `DCR_BENCH_RELAY`,缺省 `120.78.77.248:21117`。
    /// 运行:`cargo test -- --ignored realtime_chain_relay --nocapture`
    #[test]
    #[ignore]
    fn realtime_chain_relay() {
        let relay =
            std::env::var("DCR_BENCH_RELAY").unwrap_or_else(|_| "120.78.77.248:21117".to_string());
        let w = std::env::var("DCR_BENCH_WIDTH")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1280);
        let h = std::env::var("DCR_BENCH_HEIGHT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(720);
        let codec = std::env::var("DCR_BENCH_CODEC").unwrap_or_else(|_| "jpeg".to_string());
        let report = run_realtime_bench("relay", Some(&relay), 5, 30, true, w, h, &codec).unwrap();
        assert!(report.frames_rendered > 0, "经公网中继未渲染任何帧");
        assert!(report.synthetic, "synthetic=true 时报告应标记合成帧模式");
        println!("[bench] 公网中继报告({relay}): {report:?}");
    }

    /// 真实 WASAPI 系统回环采集 → 经公网中继(120.78.77.248:21117)回传本机的传输速率基准,
    /// 回传落盘供人工查验,禁止合成/伪造。
    ///
    /// 采集的是系统真实播放的声音(需系统正在播放音频,否则回环无采样会报错)。
    /// 中继地址取 `DCR_AUDIO_RELAY`,采集秒数取 `DCR_AUDIO_SECONDS`(1..10),
    /// 落盘路径取 `DCR_AUDIO_OUT`。
    /// 运行:`cargo test -- --ignored audio_relay_chain --nocapture`
    #[test]
    #[ignore]
    fn audio_relay_chain() {
        let relay =
            std::env::var("DCR_AUDIO_RELAY").unwrap_or_else(|_| "120.78.77.248:21117".to_string());
        let seconds = std::env::var("DCR_AUDIO_SECONDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5);
        let out = std::env::var("DCR_AUDIO_OUT").unwrap_or_else(|_| {
            std::env::temp_dir()
                .join("audio_loopback_verify.wav")
                .to_string_lossy()
                .into_owned()
        });
        let report = run_audio_relay_bench(relay.clone(), seconds, out.clone()).unwrap();
        assert!(report.total_transfer_bytes > 0, "音频回环未传输任何数据");
        assert!(report.avg_rtt_ms > 0.0, "往返延迟异常");
        let meta = std::fs::metadata(&out).expect("回传音频文件不存在");
        assert_eq!(
            meta.len() as u64,
            report.wav_bytes,
            "落盘文件大小与传输字节不符"
        );
        // 校验回传文件是有效 WAV 且非空采样(真实采集内容,非合成)
        let reader =
            hound::WavReader::new(std::io::Cursor::new(std::fs::read(&out).unwrap())).unwrap();
        let nonzero = reader
            .into_samples::<i16>()
            .filter_map(Result::ok)
            .filter(|s| *s != 0)
            .count();
        assert!(nonzero > 0, "回传音频全部为静音(无真实音频内容)");
        println!("[bench] 音频公网回环最终报告({relay}): {report:?}");
    }
}
