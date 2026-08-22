//! 诊断模块:DXGI 回传链路自检(设置页「诊断」tab 一键运行,支持 TCP / UDP 两种传输模式)。
//!
//! 链路与生产远程会话一致,全程标准视频编解码(H.264,硬编/硬解优先),
//! **不使用 JPEG**:
//!   真实 DXGI 抓屏(`capture::grab_frame_once`)→ FFmpeg 编码(NVENC/QSV/AMF
//!   优先,软件回退)→ 传输回环:
//!   - TCP 模式:生产协议帧(`Msg::Frame`,4 字节长度前缀 JSON + base64 Annex-B)
//!     → 本机 TCP 回环;
//!   - UDP 模式:生产数据面(`transport::split_packet` 分片 ≤1200B → `UdpChannel`
//!     直连本机回环 → `FragmentReassembler` 重组);
//!   → FFmpeg 解码(D3D11VA 硬解优先,软件回退)→ 各阶段耗时统计,同时经
//!   `dxgi-loop-frame` 事件把编码帧回传前端,由前端 WebCodecs 解码预览
//!   (事件负载含 `codec` 字段,前端按 h264/hevc 分发)。
//!
//! 真实抓屏失败(无显示器 / 无活动桌面会话)时显式报错,不回退合成帧,
//! 保证「真实本机采集」的诊断结论可信。
//!
//! F2 端到端延迟口径:发送开始时刻 → 接收端解码完成时刻(同机时钟),
//! 如实输出实测值;超过参考基线(硬编硬解 ≤80ms / 软编软解 ≤150ms)时
//! 附带各阶段分解定位(抓屏/编码/发送/解码耗时)。

use base64::Engine as _;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream, UdpSocket};

use crate::network::{read_msg, write_msg, Msg};

/// 防止并发运行多个诊断(命令按钮置灰之外的服务端兜底)。
static RUNNING: AtomicBool = AtomicBool::new(false);

/// 诊断传输模式(TCP 生产协议回环 / UDP 生产数据面回环)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopbackTransport {
    Tcp,
    Udp,
}

impl LoopbackTransport {
    /// 从前端参数解析("tcp"|"udp",其余值回退 TCP)。
    fn from_opt(s: Option<&str>) -> Self {
        if s.as_deref() == Some("udp") {
            Self::Udp
        } else {
            Self::Tcp
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Udp => "udp",
        }
    }
}

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
    /// 传输模式:"tcp" | "udp"(F1 新增,前端据此分组展示)
    pub transport: String,
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
    /// 传输的总字节数(TCP 为 base64 协议字节,UDP 为分片线格式字节)
    pub total_transfer_bytes: u64,
    /// 抓屏源分辨率
    pub source_width: u32,
    pub source_height: u32,
    /// 编码输出分辨率
    pub frame_width: u32,
    pub frame_height: u32,
    /// UDP 模式:发送的整帧分片总数(TCP 模式为 0)
    pub udp_fragments: u64,
    /// UDP 模式:丢失/重复分片计数(回环应为 0;TCP 模式为 0)
    pub udp_lost_fragments: u64,
    /// UDP 模式:乱序到达分片计数(回环一般为 0;TCP 模式为 0)
    pub udp_reordered_fragments: u64,
    /// UDP 模式:因超时未收齐而丢弃的整帧数(回环应为 0;TCP 模式为 0)
    pub udp_dropped_frames: u64,
    /// UDP 模式:平均单帧重组耗时(毫秒,首片到达 → 收齐拼接;TCP 模式为 0)
    pub avg_reassembly_ms: f64,
    /// F2:端到端延迟评估结论(如"达标"或超基线时的阶段分解定位)
    pub e2e_assessment: String,
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
    /// 编码格式:"h264" | "hevc"(前端按此分发 WebCodecs 配置)
    pub codec: String,
    /// H.264/H.265 Annex-B 字节(标准编解码;前端经 WebCodecs 解码后绘制)
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
    /// UDP 模式:发送的分片总数。
    udp_fragments: u64,
}

/// 接收端(协议读取 + FFmpeg 解码)统计。
#[derive(Default, Clone)]
struct RecvStats {
    frames_rendered: u64,
    decode_total: Duration,
    latency_total: Duration,
    transfer_bytes: u64,
    decoder_hwaccel: bool,
    /// UDP 重组统计(丢帧/缺片/乱序)。
    udp_lost_fragments: u64,
    udp_reordered_fragments: u64,
    udp_dropped_frames: u64,
    /// 重组总耗时(首片到达 → 收齐拼接),供 avg_reassembly_ms。
    reassembly_total: Duration,
}

/// 建立一对本机回环 TCP 连接((发送端流, 接收端流)),走真实 TCP 栈与生产 framing。
async fn tcp_loopback_pair() -> Result<(TcpStream, TcpStream), String> {
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

/// 建立一对本机回环 UDP socket((发送端, 接收端绑定的地址)),走真实 UDP 栈与生产分片协议。
async fn udp_loopback_pair() -> Result<(UdpSocket, std::net::SocketAddr), String> {
    // 返回 (接收端 socket, 接收端地址):发送侧经 UdpChannel 自行绑定随机端口。
    let recv_sock = UdpSocket::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("绑定 UDP 回环端口失败: {e}"))?;
    let recv_addr = recv_sock
        .local_addr()
        .map_err(|e| format!("读取 UDP 回环端口失败: {e}"))?;
    Ok((recv_sock, recv_addr))
}

/// 帧收尾通道:发送端结束(且 UDP 模式补发结束哨兵)后通知接收循环退出。
/// 哨兵 = frame_id u32::MAX 的 1 片分片(有效线格式但不会被误认为数据帧,
/// 接收端以 frame_id == u32::MAX::-1 判定;哨兵本身 frame_id 取 MAX,
/// 接收侧收到即退出,不进重组器)。
const UDP_EOF_SENTINEL: u32 = u32::MAX;

/// 构造 UDP 结束哨兵数据报(16 字节头 + 0 负载的"最后一片")。
fn udp_eof_datagram() -> Vec<u8> {
    crate::transport::encode_segment(&crate::transport::UdpSegment {
        frame_id: UDP_EOF_SENTINEL,
        frag_idx: 0,
        frag_cnt: 1,
        key: true,
        codec: crate::transport::CODEC_H264,
        payload: Vec::new(),
    })
}

/// 发送端循环:按目标帧率 真实抓屏 → H.264 编码 → 生产协议发送(TCP)或生产分片发送(UDP)。
///
/// - 桌面静止导致单次抓屏超时时复用上一帧(与生产会话行为一致);
/// - 尚无任何历史帧且持续抓不到(无显示器/无活动桌面)超过 3 秒宽限期 → 显式报错;
/// - 编码器懒创建,首帧请求关键帧(WebCodecs 预览依赖首块为关键帧)。
///
/// 帧号 seq 从 0 起,与 UDP 分片 frame_id 同一来源(生产 `host_write_loop`
/// 以 `EncodedPacket.seq` 为分片帧号,此处对齐口径)。
#[cfg(target_os = "windows")]
async fn sender_loop(
    transport: LoopbackTransport,
    mut stream: Option<TcpStream>,
    udp: Option<crate::transport::UdpChannel>,
    udp_peer: Option<std::net::SocketAddr>,
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
        udp_fragments: 0,
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
                0, // 诊断链路无画质档位输入:0 = 编码器内置启发式码率
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
        let encode_dur = encode_start.elapsed();
        stats.encode_total += encode_dur;
        stats.enc_w = ew;
        stats.enc_h = eh;

        // 3) 传输发送(记录发送时刻供端到端延迟统计)
        let send_start = Instant::now();
        if let Ok(mut map) = pending.lock() {
            map.insert(seq, send_start);
        }
        match transport {
            LoopbackTransport::Tcp => {
                let Some(stream) = stream.as_mut() else {
                    return Err("TCP 模式缺少回环流".to_string());
                };
                write_msg(
                    stream,
                    &Msg::Frame {
                        w: ew,
                        h: eh,
                        seq,
                        data: base64::engine::general_purpose::STANDARD.encode(&data),
                        dur: encode_dur.as_millis() as u32,
                        codec: "h264".to_string(),
                        key: is_key,
                    },
                )
                .await
                .map_err(|e| format!("发送帧失败(seq={seq}): {e}"))?;
            }
            LoopbackTransport::Udp => {
                // 生产数据面同构:EncodedPacket → split_packet → UdpChannel 直连发送
                let chan = udp.as_ref().ok_or("UDP 模式缺少通道")?;
                let _ = udp_peer;
                let pkt = crate::ffmpeg_hw::EncodedPacket {
                    width: ew,
                    height: eh,
                    data,
                    key: is_key,
                    seq,
                };
                let segs =
                    crate::transport::split_packet(&pkt, "h264", crate::transport::SEGMENT_MTU);
                chan.send_packet(&segs)
                    .await
                    .map_err(|e| format!("UDP 发送帧失败(seq={seq}): {e}"))?;
                stats.udp_fragments += segs.len() as u64;
            }
        }
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
    // 收尾:TCP 对写半部 shutdown(FIN),接收端读到 EOF 自然结束;
    // UDP 发结束哨兵(接收循环收到即退出)。
    if let Some(mut stream) = stream {
        let _ = stream.shutdown().await;
    }
    if transport == LoopbackTransport::Udp {
        if let Some(chan) = udp {
            // 哨兵连发 3 次(UDP 不保证送达,回环下足够可靠;丢失则接收端兜底超时退出)
            for _ in 0..3 {
                let _ = chan.send_raw(&udp_eof_datagram()).await;
            }
        }
    }
    Ok(stats)
}

/// 接收端循环(TCP):读协议帧 → base64 解码 → FFmpeg 解码(D3D11VA 硬解优先)→ 统计,
/// 并把到达接收端的编码帧经 `dxgi-loop-frame` 事件回传前端做 WebCodecs 预览。
async fn receiver_loop_tcp(
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
            data,
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
        let frame = base64::engine::general_purpose::STANDARD
            .decode(&data)
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
                    codec: codec.clone(),
                    data: frame.clone(),
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
                .and_then(|d| d.decode(&frame).ok().flatten());
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
                s.transfer_bytes += data.len() as u64;
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (&seq, &w, &h, &key, &codec, &stats, &pending);
        }
    }
    Ok(())
}

/// 接收端循环(UDP):真实 socket 收包 → parse_segment → FragmentReassembler 重组
/// → FFmpeg 解码 → 统计;编码帧同样经 `dxgi-loop-frame` 回传前端预览。
///
/// 结束条件:收到结束哨兵数据报(frame_id == UDP_EOF_SENTINEL),或兜底超时。
/// 单帧重组耗时 = 收齐回調时刻 - 首片到达时刻(经 FragmentReassembler 不暴露
/// 首片时刻,此处按收齐时刻与"该帧首片到达"的外部记录差值近似;实现上在
/// 收到每帧第 0 片时记录时刻)。
async fn receiver_loop_udp(
    sock: Arc<UdpSocket>,
    stats: Arc<Mutex<RecvStats>>,
    pending: Arc<Mutex<HashMap<u64, Instant>>>,
    seconds: u32,
    app: Option<AppHandle>,
) -> Result<(), String> {
    let start = Instant::now();
    let hard_timeout = Duration::from_secs(u64::from(seconds) + 5);
    #[cfg(target_os = "windows")]
    let mut decoder: Option<crate::ffmpeg_hw::HwDecoder> = None;
    let mut re = crate::transport::FragmentReassembler::new(
        crate::transport::REASSEMBLY_TIMEOUT_MS,
    );
    // 每帧首片到达时刻(重组耗时口径:首片到达 → 收齐拼接)
    let mut first_frag_at: HashMap<u32, Instant> = HashMap::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        if start.elapsed() > hard_timeout {
            break;
        }
        let (n, _src) = match tokio::time::timeout(
            Duration::from_millis(500),
            sock.recv_from(&mut buf),
        )
        .await
        {
            Ok(Ok(x)) => x,
            Ok(Err(e)) => return Err(format!("UDP 回环接收失败: {e}")),
            Err(_) => continue, // 500ms 无包,回到超时判定
        };
        let packet = &buf[..n];
        let seg = match crate::transport::parse_segment(packet) {
            Ok(s) => s,
            Err(_) => continue, // 非分片包(哨兵之外的杂包)忽略
        };
        // 结束哨兵
        if seg.frame_id == UDP_EOF_SENTINEL {
            break;
        }
        if seg.frag_idx == 0 {
            first_frag_at.entry(seg.frame_id).or_insert_with(Instant::now);
        }
        let line_len = crate::transport::SEGMENT_HEADER_LEN + seg.payload.len();
        let Some(frame) = re.push(seg) else {
            if let Ok(mut s) = stats.lock() {
                s.transfer_bytes += line_len as u64;
            }
            continue;
        };
        let reassembly_dur = first_frag_at
            .remove(&frame.frame_id)
            .map(|t0| t0.elapsed())
            .unwrap_or_default();
        let codec = crate::transport::codec_name_from_u8(frame.codec).to_string();
        // 回传前端预览(与 TCP 模式同构,含 codec 字段)
        if let Some(app) = &app {
            let _ = app.emit(
                "dxgi-loop-frame",
                DxgiLoopFrameFrameEventForUdp::into_event(
                    frame.frame_id as u64,
                    frame.key,
                    codec.clone(),
                    &frame.data,
                ),
            );
        }
        // 本机渲染近似:FFmpeg 解码(硬解优先)
        #[cfg(target_os = "windows")]
        {
            let t0 = Instant::now();
            if decoder.is_none() {
                let d =
                    crate::ffmpeg_hw::HwDecoder::open(crate::ffmpeg_hw::codec_family_id(&codec))?;
                let hwaccel = d.using_hwaccel();
                log::info!("[diagnostics] H.264 解码器就绪(UDP,hwaccel={hwaccel})");
                decoder = Some(d);
                if let Ok(mut s) = stats.lock() {
                    s.decoder_hwaccel = hwaccel;
                }
            }
            let rendered = decoder
                .as_mut()
                .and_then(|d| d.decode(&frame.data).ok().flatten());
            let decode_ms = t0.elapsed();
            let Some((_dw, _dh, _rgb)) = rendered else {
                log::warn!(
                    "[diagnostics] 帧解码无输出(frame_id={}),跳过",
                    frame.frame_id
                );
                if let Ok(mut s) = stats.lock() {
                    s.transfer_bytes += line_len as u64;
                }
                continue;
            };
            // UDP 模式宽高来自编码器会话(分片头不携带),端到端以 seq=frame_id 对账
            let latency = pending
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&(frame.frame_id as u64))
                .and_then(|sent_at| t0.checked_duration_since(sent_at))
                .unwrap_or_default();
            if let Ok(mut s) = stats.lock() {
                s.frames_rendered += 1;
                s.decode_total += decode_ms;
                s.latency_total += latency;
                s.reassembly_total += reassembly_dur;
                s.transfer_bytes += line_len as u64;
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (&pending, &codec);
        }
    }
    // 快照重组统计(丢帧/缺片/乱序)进报告
    let st = re.stats();
    if let Ok(mut s) = stats.lock() {
        s.udp_lost_fragments = st.lost_frags;
        s.udp_reordered_fragments = st.reordered;
        s.udp_dropped_frames = st.dropped_frames;
    }
    Ok(())
}

/// UDP 预览事件的轻量构造辅助(与 TCP 模式负载同构;宽高分片头不携带,
/// 置 0 由前端按解码器实际输出尺寸自适应,与生产 remote-frame 口径一致)。
struct DxgiLoopFrameFrameEventForUdp;
impl DxgiLoopFrameFrameEventForUdp {
    fn into_event(seq: u64, key: bool, codec: String, data: &[u8]) -> DxgiLoopFrameEvent {
        DxgiLoopFrameEvent {
            seq,
            width: 0,
            height: 0,
            key,
            codec,
            data: data.to_vec(),
        }
    }
}

/// F2:端到端延迟评估(硬编硬解基线 80ms / 软编软解 150ms;UDP 与 TCP 差距 ≤10%)。
///
/// 如实输出结论:达标或超基线时给出阶段分解(抓屏/编码/发送/解码)。
fn assess_e2e_latency(
    report: &DxgiLoopbackReport,
) -> String {
    let hardware_enc = crate::ffmpeg_hw::encoder_category(&report.encoder).contains("硬件");
    let hardware_dec = report.decoder_hwaccel;
    let baseline = if hardware_enc && hardware_dec {
        80.0
    } else {
        150.0
    };
    let e2e = report.avg_e2e_latency_ms;
    let headroom = e2e <= baseline;
    let stage_breakdown = format!(
        "阶段分解: 抓屏 {:.1}ms + 编码 {:.1}ms + 发送 {:.1}ms + 解码 {:.1}ms(端到端实测 {:.1}ms, 基线 {:.0}ms, {}+{})",
        report.avg_capture_ms,
        report.avg_encode_ms,
        report.avg_send_ms,
        report.avg_decode_ms,
        e2e,
        baseline,
        if hardware_enc { "硬编" } else { "软编" },
        if hardware_dec { "硬解" } else { "软解" },
    );
    if headroom {
        format!("达标({}+{} 端到端 {:.1}ms ≤ 基线 {:.0}ms)", if hardware_enc { "硬编" } else { "软编" }, if hardware_dec { "硬解" } else { "软解" }, e2e, baseline)
    } else {
        format!("超基线({}+{} 端到端 {:.1}ms > 基线 {:.0}ms);{}", if hardware_enc { "硬编" } else { "软编" }, if hardware_dec { "硬解" } else { "软解" }, e2e, baseline, stage_breakdown)
    }
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
    transport: LoopbackTransport,
) -> Result<DxgiLoopbackReport, String> {
    if !crate::ffmpeg_hw::available() {
        return Err("FFmpeg DLL 未加载,标准编解码(H.264)不可用".to_string());
    }
    let (seconds, target_fps, target_w, target_h) =
        clamp_params(seconds, target_fps, target_w, target_h);

    let pending: Arc<Mutex<HashMap<u64, Instant>>> = Arc::new(Mutex::new(HashMap::new()));
    let recv_stats = Arc::new(Mutex::new(RecvStats::default()));
    let wall_start = Instant::now();

    let mut report = match transport {
        LoopbackTransport::Tcp => {
            let (send_stream, recv_stream) = tcp_loopback_pair().await?;
            let sender = tokio::spawn(sender_loop(
                transport,
                Some(send_stream),
                None,
                None,
                monitor_id,
                seconds,
                target_fps,
                target_w,
                target_h,
                pending.clone(),
            ));
            let receiver = tokio::spawn(receiver_loop_tcp(
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
            build_report(monitor_id, seconds, target_fps, transport, send, recv_stats, wall_start)
        }
        LoopbackTransport::Udp => {
            // UDP 回环:接收端先绑定,发送端经 UdpChannel(生产数据面同构)发分片
            let (recv_sock, recv_addr) = udp_loopback_pair().await?;
            let recv_handle = Arc::new(recv_sock);
            let receiver = tokio::spawn(receiver_loop_udp(
                recv_handle.clone(),
                recv_stats.clone(),
                pending.clone(),
                seconds,
                app,
            ));
            let chan = crate::transport::UdpChannel::direct(recv_addr).await?;
            let sender = tokio::spawn(sender_loop(
                transport,
                None,
                Some(chan),
                Some(recv_addr),
                monitor_id,
                seconds,
                target_fps,
                target_w,
                target_h,
                pending,
            ));
            let send = sender
                .await
                .map_err(|e| format!("发送任务异常: {e}"))??;
            receiver
                .await
                .map_err(|e| format!("接收任务异常: {e}"))??;
            build_report(monitor_id, seconds, target_fps, transport, send, recv_stats, wall_start)
        }
    };
    report.e2e_assessment = assess_e2e_latency(&report);
    Ok(report)
}

/// 汇总两侧统计为报告(F2:如实输出端到端实测值)。
#[cfg(target_os = "windows")]
fn build_report(
    monitor_id: u32,
    seconds: u32,
    target_fps: u32,
    transport: LoopbackTransport,
    send: SenderStats,
    recv_stats: Arc<Mutex<RecvStats>>,
    wall_start: Instant,
) -> DxgiLoopbackReport {
    let recv = recv_stats
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let wall_secs = wall_start.elapsed().as_secs_f64();
    let rendered = recv.frames_rendered.max(1) as f64;
    let sent = send.frames_sent.max(1) as f64;
    DxgiLoopbackReport {
        codec: "h264".to_string(),
        encoder: send.encoder,
        decoder_hwaccel: recv.decoder_hwaccel,
        monitor_id,
        seconds,
        target_fps,
        transport: transport.as_str().to_string(),
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
        udp_fragments: send.udp_fragments,
        udp_lost_fragments: recv.udp_lost_fragments,
        udp_reordered_fragments: recv.udp_reordered_fragments,
        udp_dropped_frames: recv.udp_dropped_frames,
        avg_reassembly_ms: recv.reassembly_total.as_secs_f64() * 1000.0 / rendered,
        e2e_assessment: String::new(), // 由 assess_e2e_latency 填充
    }
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
    transport: LoopbackTransport,
) -> Result<DxgiLoopbackReport, String> {
    let _ = (monitor_id, seconds, target_fps, target_w, target_h, transport);
    Err("DXGI 回传诊断仅 Windows 支持".to_string())
}

/// FFmpeg 编解码能力报告(B3/L12):硬编/软编、硬解/软解实际路径,
/// 与本机 GPU 匹配(如 NVIDIA 机器优先 NVENC)。前端诊断页可展示。
#[tauri::command]
pub fn get_ffmpeg_capability() -> String {
    crate::ffmpeg_hw::capability_report()
}

/// 设置页「诊断」命令:DXGI 回传自检(真实本机采集,H.264 标准编解码,禁止 JPEG)。
///
/// `transport`:"tcp" | "udp"(缺省 tcp;UDP 模式走生产分片数据面回环)。
#[tauri::command]
pub async fn run_dxgi_loopback(
    app: AppHandle,
    monitor_id: u32,
    seconds: u32,
    target_fps: u32,
    target_width: u32,
    target_height: u32,
    transport: Option<String>,
) -> Result<DxgiLoopbackReport, String> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err("已有诊断在运行中,请稍后再试".to_string());
    }
    let mode = LoopbackTransport::from_opt(transport.as_deref());
    let result = run_chain(
        Some(app),
        monitor_id,
        seconds,
        target_fps,
        target_width,
        target_height,
        mode,
    )
    .await;
    RUNNING.store(false, Ordering::SeqCst);
    match &result {
        Ok(report) => crate::operation_log::op_log(
            "diagnostics",
            "run_dxgi_loopback",
            &format!(
                "monitor={monitor_id} transport={} {}s@{}fps 抓帧{} 发送{} 解码{} {:.1}fps e2e={:.1}ms",
                report.transport,
                report.seconds,
                report.target_fps,
                report.frames_grabbed,
                report.frames_sent,
                report.frames_rendered,
                report.realtime_fps,
                report.avg_e2e_latency_ms
            ),
        ),
        Err(e) => crate::operation_log::op_log(
            "diagnostics",
            "run_dxgi_loopback",
            &format!("monitor={monitor_id} transport={} 失败: {e}", mode.as_str()),
        ),
    }
    // F1/C6:UDP 模式统计随报告落盘(operations-*.log)
    if let Ok(report) = &result {
        if report.transport == "udp" {
            crate::operation_log::op_log(
                "diagnostics",
                "run_dxgi_loopback_udp_stats",
                &format!(
                    "transport=udp 分片{} 丢片{} 乱片{} 丢帧{} 平均重组{:.2}ms",
                    report.udp_fragments,
                    report.udp_lost_fragments,
                    report.udp_reordered_fragments,
                    report.udp_dropped_frames,
                    report.avg_reassembly_ms
                ),
            );
        }
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

    #[test]
    fn transport_option_parses() {
        assert_eq!(LoopbackTransport::from_opt(None), LoopbackTransport::Tcp);
        assert_eq!(
            LoopbackTransport::from_opt(Some("tcp")),
            LoopbackTransport::Tcp
        );
        assert_eq!(
            LoopbackTransport::from_opt(Some("udp")),
            LoopbackTransport::Udp
        );
        // 非法值回退 TCP
        assert_eq!(
            LoopbackTransport::from_opt(Some("pigeon")),
            LoopbackTransport::Tcp
        );
    }

    /// UDP 结束哨兵:有效线格式、frame_id 为哨兵值、可被 parse_segment 还原。
    #[test]
    fn udp_eof_sentinel_roundtrip() {
        let gram = udp_eof_datagram();
        let seg = crate::transport::parse_segment(&gram).expect("哨兵应为有效线格式");
        assert_eq!(seg.frame_id, UDP_EOF_SENTINEL);
        assert_eq!(seg.frag_cnt, 1);
        assert!(seg.payload.is_empty());
    }

    /// F2 评估纯函数:硬编硬解达标 / 软编软解超基线给出阶段分解。
    #[test]
    fn e2e_assessment_reports_baseline() {
        let mut report = DxgiLoopbackReport {
            codec: "h264".into(),
            encoder: "h264_nvenc".into(),
            decoder_hwaccel: true,
            monitor_id: 0,
            seconds: 3,
            target_fps: 30,
            transport: "tcp".into(),
            frames_grabbed: 90,
            frames_sent: 90,
            frames_rendered: 90,
            realtime_fps: 30.0,
            avg_capture_ms: 3.0,
            avg_encode_ms: 3.0,
            avg_send_ms: 1.0,
            avg_decode_ms: 2.0,
            avg_e2e_latency_ms: 40.0,
            total_transfer_bytes: 1_000_000,
            source_width: 2560,
            source_height: 1440,
            frame_width: 1280,
            frame_height: 720,
            udp_fragments: 0,
            udp_lost_fragments: 0,
            udp_reordered_fragments: 0,
            udp_dropped_frames: 0,
            avg_reassembly_ms: 0.0,
            e2e_assessment: String::new(),
        };
        let a = assess_e2e_latency(&report);
        assert!(a.contains("达标"), "硬编硬解 40ms 应达标: {a}");

        // 软编软解超 150ms 基线:应含阶段分解
        report.encoder = "libx264".into();
        report.decoder_hwaccel = false;
        report.avg_e2e_latency_ms = 180.0;
        let b = assess_e2e_latency(&report);
        assert!(b.contains("超基线"), "软编软解 180ms 应超基线: {b}");
        assert!(b.contains("阶段分解"), "超基线应给出阶段分解: {b}");
    }

    /// F3④:端到端 UDP 真实链路(transport 模式参数化):DXGI 采集 → H.264 编码
    /// → UDP 分片回环 → 重组 → 解码(需活动桌面,默认忽略)。
    ///
    /// 运行:`cargo test -- --ignored dxgi_loopback_udp_real_chain --nocapture`
    #[cfg(target_os = "windows")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore]
    async fn dxgi_loopback_udp_real_chain() {
        // F-3:DXGI 临界区互斥(与采集基准共用一把进程级锁,并行跑 ignored
        // 全集时 DuplicateOutput 争抢被串行化,不再需要 --test-threads=1)
        let _dxgi = crate::capture::tests::TEST_DXGI_MUTEX
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let report = run_chain(None, 0, 3, 30, 1280, 720, LoopbackTransport::Udp)
            .await
            .expect("DXGI UDP 回传链路应跑通");
        println!(
            "[diag-udp] 编码器={} 硬解={} 抓帧 {} → 发送 {} → 解码 {} | 实时 {:.1} fps | 抓屏 {:.2}ms 编码 {:.2}ms 发送 {:.2}ms 解码 {:.2}ms 端到端 {:.2}ms | 分片 {} 丢片 {} 乱片 {} 丢帧 {} 重组 {:.2}ms | {}",
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
            report.udp_fragments,
            report.udp_lost_fragments,
            report.udp_reordered_fragments,
            report.udp_dropped_frames,
            report.avg_reassembly_ms,
            report.e2e_assessment,
        );
        assert!(report.frames_grabbed > 0, "未抓到任何真实帧");
        assert!(report.frames_sent > 0, "未发送任何帧");
        assert!(report.frames_rendered > 0, "未解码任何帧");
        assert_eq!(report.transport, "udp");
        assert!(report.udp_fragments > 0, "UDP 模式应有分片统计");
        assert_eq!(report.udp_lost_fragments, 0, "回环不允许丢片");
        assert_eq!(report.udp_dropped_frames, 0, "回环不允许丢帧");
    }

    /// 本机真实验证:DXGI 采集 → H.264 编码 → TCP 回环 → 解码全链路(需活动桌面,默认忽略)。
    ///
    /// 多线程 flavor:发送端的同步 DXGI 抓屏会阻塞工作线程,须与接收端并行调度。
    /// 运行:`cargo test -- --ignored dxgi_loopback_real_chain --nocapture`
    #[cfg(target_os = "windows")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore]
    async fn dxgi_loopback_real_chain() {
        // F-3:DXGI 临界区互斥(同 dxgi_loopback_udp_real_chain)
        let _dxgi = crate::capture::tests::TEST_DXGI_MUTEX
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let report = run_chain(None, 0, 3, 30, 1280, 720, LoopbackTransport::Tcp)
            .await
            .expect("DXGI 回传链路应跑通");
        println!(
            "[diag-tcp] 编码器={} 硬解={} 抓帧 {} → 发送 {} → 解码 {} | 实时 {:.1} fps | 抓屏 {:.2}ms 编码 {:.2}ms 发送 {:.2}ms 解码 {:.2}ms 端到端 {:.2}ms | {}",
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
            report.e2e_assessment,
        );
        assert!(report.frames_grabbed > 0, "未抓到任何真实帧");
        assert!(report.frames_sent > 0, "未发送任何帧");
        assert!(report.frames_rendered > 0, "未解码任何帧");
        assert_eq!(report.transport, "tcp");
    }
}
