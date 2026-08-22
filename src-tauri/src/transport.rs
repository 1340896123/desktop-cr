//! UDP 传输原语:视频帧分片/重组(自定义二进制分片帧协议)+ UDP 通道。
//!
//! 协议(与 stage1-guide 2.2 节一致,控制面保持 TCP、视频面走 UDP):
//! - 分片头固定 16 字节小端:
//!   magic u32=0x44505255("URPD") + frame_id u32 + frag_idx u16 + frag_cnt u16
//!   + flags u16(bit0=keyframe,bit1=最后分片冗余校验)+ codec u8(0=h264,1=hevc)
//!   + rfu u8(保留 0);
//! - 负载为原始 Annex-B 字节切片,单片(头+负载)≤ 1200 字节
//!   (避开常见 MTU 1500 头部开销,经中继 base64 JSON 封装也安全);
//! - 重组规则:同 frame_id 收齐 frag_cnt 片按 frag_idx 拼接;乱序到达按
//!   frag_idx 放位;超过 timeout_ms(默认 200ms)未收齐整帧丢弃,丢弃后进入
//!   关键帧门控(delta 帧不透传直到下一关键帧,F-1a),同时接收端经 TCP 控制
//!   面回发 `Msg::KeyframeRequest` 请求发送端立即编出 IDR(F-1a 主动恢复)。
//!
//! [`UdpChannel`] 在上述原语之上提供生产通道:直连模式(`UdpDirect`,向对端
//! 地址逐片 `send_to`)与中继模式(`UdpRelay`,分片整体作为 `data` 数据报
//! payload 过 dcr-relay 的 UDP 21119 转发),两种模式接收侧共用同一套
//! parse/重组代码(中继透传裸二进制,与直连同构)。

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// 分片线格式 magic("URPD" 小端)。
pub const SEGMENT_MAGIC: u32 = 0x4450_5255;
/// 分片头固定长度(字节)。
pub const SEGMENT_HEADER_LEN: usize = 16;
/// 单片总长上限(头 16 + 负载),即默认 MTU 预算;负载上限 = 该值 - 头长。
pub const SEGMENT_MTU: usize = 1200;
/// 默认重组超时(毫秒):超时未收齐整帧即丢弃。
pub const REASSEMBLY_TIMEOUT_MS: u64 = 200;
/// UDP 通道保活:udp-keepalive 数据报间隔(毫秒,F-1b 半开检测载体)。
pub const UDP_KEEPALIVE_INTERVAL_MS: u64 = 1000;
/// UDP 通道保活字符串(短 JSON,首字节 '{' 与分片帧天然区分)。
pub const UDP_KEEPALIVE_TEXT: &str = r#"{"t":"udp-keepalive"}"#;
/// 编码类型:0 = H.264。
pub const CODEC_H264: u8 = 0;
/// 编码类型:1 = H.265(HEVC)。
pub const CODEC_HEVC: u8 = 1;
/// flags bit0:关键帧。
pub const FLAG_KEYFRAME: u16 = 0x0001;
/// flags bit1:最后分片(frag_idx == frag_cnt-1 的冗余校验位)。
pub const FLAG_LAST: u16 = 0x0002;

/// UDP 分片(值类型,线格式见 [`encode_segment`])。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UdpSegment {
    /// 帧号(发送侧递增)。
    pub frame_id: u32,
    /// 分片序号(从 0 起)。
    pub frag_idx: u16,
    /// 总分片数。
    pub frag_cnt: u16,
    /// 是否关键帧。
    pub key: bool,
    /// 0=h264,1=hevc。
    pub codec: u8,
    /// 分片负载(原始 Annex-B 字节切片)。
    pub payload: Vec<u8>,
}

/// 重组完成的帧(供后续组包;宽高等由上层按编码器会话补齐)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameData {
    pub frame_id: u32,
    pub key: bool,
    pub codec: u8,
    /// 完整 Annex-B 帧字节(各分片按 frag_idx 拼接)。
    pub data: Vec<u8>,
}

/// 重组统计(进诊断报告/会话浮窗)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReassemblyStats {
    /// 超时/冲突丢弃的整帧数。
    pub dropped_frames: u64,
    /// 重复收到已收分片的次数(视为隐性丢失迹象)。
    pub lost_frags: u64,
    /// 乱序到达的分片数(frag_idx 不等于当前期望值)。
    pub reordered: u64,
    /// 关键帧门控丢弃的 delta 帧数(F-1a:丢帧后等关键帧期间的 delta 不透传)。
    pub gated_frames: u64,
}

/// 线格式编码:16 字节小端头 + 负载。
///
/// 调用方应保证 `seg.payload` 长度使总长 ≤ SEGMENT_MTU([`split_bytes`] 保证)。
pub fn encode_segment(seg: &UdpSegment) -> Vec<u8> {
    let mut out = Vec::with_capacity(SEGMENT_HEADER_LEN + seg.payload.len());
    out.extend_from_slice(&SEGMENT_MAGIC.to_le_bytes());
    out.extend_from_slice(&seg.frame_id.to_le_bytes());
    out.extend_from_slice(&seg.frag_idx.to_le_bytes());
    out.extend_from_slice(&seg.frag_cnt.to_le_bytes());
    let mut flags = 0u16;
    if seg.key {
        flags |= FLAG_KEYFRAME;
    }
    if seg.frag_cnt > 0 && seg.frag_idx + 1 == seg.frag_cnt {
        flags |= FLAG_LAST;
    }
    out.extend_from_slice(&flags.to_le_bytes());
    out.push(seg.codec);
    out.push(0); // rfu
    out.extend_from_slice(&seg.payload);
    out
}

/// 线格式解码:校验长度、magic、frag 序号与最后分片冗余位,负载字节原样取回。
pub fn parse_segment(bytes: &[u8]) -> Result<UdpSegment, String> {
    if bytes.len() < SEGMENT_HEADER_LEN {
        return Err(format!(
            "分片过短: {} 字节(至少需要 {SEGMENT_HEADER_LEN})",
            bytes.len()
        ));
    }
    let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if magic != SEGMENT_MAGIC {
        return Err(format!("magic 不匹配(0x{magic:08x})"));
    }
    let frame_id = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    let frag_idx = u16::from_le_bytes([bytes[8], bytes[9]]);
    let frag_cnt = u16::from_le_bytes([bytes[10], bytes[11]]);
    let flags = u16::from_le_bytes([bytes[12], bytes[13]]);
    let codec = bytes[14];
    let rfu = bytes[15];
    if rfu != 0 {
        return Err(format!("rfu 非零(0x{rfu:02x}),线格式版本不兼容"));
    }
    if frag_cnt == 0 {
        return Err("frag_cnt 为 0".into());
    }
    if frag_idx >= frag_cnt {
        return Err(format!("frag_idx {frag_idx} 越界(frag_cnt={frag_cnt})"));
    }
    // 最后分片冗余校验位必须与序号一致(线格式自检)。
    let last_flag = flags & FLAG_LAST != 0;
    if last_flag != (frag_idx + 1 == frag_cnt) {
        return Err(format!(
            "最后分片冗余位不一致(flags=0x{flags:04x}, frag_idx={frag_idx}, frag_cnt={frag_cnt})"
        ));
    }
    let key = flags & FLAG_KEYFRAME != 0;
    if codec != CODEC_H264 && codec != CODEC_HEVC {
        return Err(format!("未知 codec: {codec}"));
    }
    Ok(UdpSegment {
        frame_id,
        frag_idx,
        frag_cnt,
        key,
        codec,
        payload: bytes[SEGMENT_HEADER_LEN..].to_vec(),
    })
}

/// 纯字节分片原语:把一帧的原始字节切成 UDP 分片(独立可测,不依赖编码器)。
///
/// `mtu` 为单片总长(头+负载)上限,自动收敛到 ≥ 头长 + 1;每片负载
/// `mtu - SEGMENT_HEADER_LEN` 字节,最后一片取剩余。
pub fn split_bytes(
    frame_id: u32,
    key: bool,
    codec: u8,
    data: &[u8],
    mtu: usize,
) -> Vec<UdpSegment> {
    let payload_cap = mtu.saturating_sub(SEGMENT_HEADER_LEN).max(1);
    let frag_cnt = data.len().div_ceil(payload_cap).max(1) as u16;
    let mut out = Vec::with_capacity(frag_cnt as usize);
    for (idx, chunk) in data.chunks(payload_cap).enumerate() {
        out.push(UdpSegment {
            frame_id,
            frag_idx: idx as u16,
            frag_cnt,
            key,
            codec,
            payload: chunk.to_vec(),
        });
    }
    out
}

/// 把编码器输出分包为 UDP 分片(M2 契约 `EncodedPacket` 的薄封装)。
///
/// 帧号取 `pkt.seq` 低 32 位(线格式 frame_id 为 u32);宽高不入分片头,
/// 由上层会话按编码器配置补齐。`codec_name` 为会话编码名("h264"/"hevc"),
/// 非法值按 H.264 处理。
pub fn split_packet(
    pkt: &crate::ffmpeg_hw::EncodedPacket,
    codec_name: &str,
    mtu: usize,
) -> Vec<UdpSegment> {
    split_bytes(
        pkt.seq as u32,
        pkt.key,
        if codec_name.eq_ignore_ascii_case("hevc") {
            CODEC_HEVC
        } else {
            CODEC_H264
        },
        &pkt.data,
        mtu,
    )
}

/// 分片线格式 codec 值 → 协议编码名("h264"/"hevc");非法值按 h264。
pub fn codec_name_from_u8(codec: u8) -> &'static str {
    if codec == CODEC_HEVC {
        "hevc"
    } else {
        "h264"
    }
}

/// 单帧重组槽:记录收到的分片位图与数据。
struct FrameSlot {
    /// `parts[i] = Some(bytes)` 表示第 i 片已到。
    parts: Vec<Option<Vec<u8>>>,
    key: bool,
    codec: u8,
    received: u16,
    first_seen: Instant,
    /// 本帧是否已检测到乱序(用于 stats(reordered) 只计一次)。
    reorder_seen: bool,
}

/// 分片重组器:乱序放位 + 超时丢帧 + 关键帧门控(F-1a)。
///
/// 用法:接收循环把 [`parse_segment`] 成功的 [`UdpSegment`] 逐片 `push`,
/// 返回 `Some(FrameData)` 表示该帧收齐(按 frag_idx 拼接的完整 Annex-B 字节)。
///
/// 关键帧门控:丢帧(超时/分片数冲突)后进入"等关键帧"状态,delta 帧收齐也不
/// 透传(直接丢弃,计数进 `gated_frames`),直到下一个关键帧到达才恢复输出。
/// 依据:H.264/H.265 delta 帧参考前帧,前置帧缺失时解码必然花屏/失败,
/// 原样透传只会把损坏推给解码器——等 IDR 才是正确恢复点(门控不吞关键帧:
/// key 帧始终透传并解除门控)。
pub struct FragmentReassembler {
    timeout: Duration,
    slots: HashMap<u32, FrameSlot>,
    stats: ReassemblyStats,
    /// 丢帧后是否处于"等关键帧"门控状态。
    awaiting_key: bool,
}

impl FragmentReassembler {
    /// 按超时毫秒创建;生产默认 [`REASSEMBLY_TIMEOUT_MS`]。
    pub fn new(timeout_ms: u64) -> Self {
        Self {
            timeout: Duration::from_millis(timeout_ms.max(1)),
            slots: HashMap::new(),
            stats: ReassemblyStats::default(),
            awaiting_key: false,
        }
    }

    /// 清理超时未收齐的帧(丢弃并计数);整帧丢弃即进入关键帧门控(F-1a)。
    fn prune(&mut self) {
        let expired: Vec<u32> = self
            .slots
            .iter()
            .filter(|(_, s)| s.first_seen.elapsed() >= self.timeout)
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            // lost_frags 按缺失分片数累计:整帧丢弃时缺失片 = 总片数 - 已收片数。
            if let Some(slot) = self.slots.remove(&id) {
                self.stats.lost_frags += (slot.parts.len() as u16 - slot.received) as u64;
                self.stats.dropped_frames += 1;
                // 丢弃的可能是关键帧也可能是 delta——保守进入门控:
                // 若丢弃的是关键帧,后续到达的关键帧会立即解除门控,代价仅为
                // 丢帧到下一关键帧之间的 delta(本就无法正确解码)。
                self.awaiting_key = true;
            }
        }
    }

    /// 投喂一个分片;返回 `Some(FrameData)` 表示对应帧已收齐并按序拼接完成。
    ///
    /// 乱序分片按 frag_idx 放位;重复分片计数进 `lost_frags` 并忽略;
    /// 同 frame_id 分片数不一致(线格式冲突)时丢弃旧槽从头收;
    /// 超时整帧丢弃,等下一关键帧。
    pub fn push(&mut self, seg: UdpSegment) -> Option<FrameData> {
        self.prune();
        let entry = self.slots.entry(seg.frame_id).or_insert_with(|| FrameSlot {
            parts: vec![None; seg.frag_cnt as usize],
            key: seg.key,
            codec: seg.codec,
            received: 0,
            first_seen: Instant::now(),
            reorder_seen: false,
        });
        if entry.parts.len() != seg.frag_cnt as usize {
            // 同帧号分片数冲突:丢旧帧重收(罕见,防御线格式错乱)。
            self.stats.dropped_frames += 1;
            self.awaiting_key = true; // 冲突即链路错乱,保守等关键帧(F-1a)
            let slot = self.slots.get_mut(&seg.frame_id)?;
            *slot = FrameSlot {
                parts: vec![None; seg.frag_cnt as usize],
                key: seg.key,
                codec: seg.codec,
                received: 0,
                first_seen: Instant::now(),
                reorder_seen: false,
            };
        }
        let slot = self.slots.get_mut(&seg.frame_id)?;
        // 乱序检测:到达的片不是"当前最小缺失片"即记一次(每帧只记一次,
        // 否则中间缺片时后续每片都会重复计数)。
        let expect = slot
            .parts
            .iter()
            .position(|p| p.is_none())
            .unwrap_or(slot.parts.len()) as u16;
        if seg.frag_idx != expect && !slot.reorder_seen {
            self.stats.reordered += 1;
            slot.reorder_seen = true;
        }
        if slot.parts[seg.frag_idx as usize].is_some() {
            self.stats.lost_frags += 1; // 重复分片按丢失迹象计
            return None;
        }
        slot.parts[seg.frag_idx as usize] = Some(seg.payload);
        slot.received += 1;
        if slot.received < seg.frag_cnt {
            return None;
        }
        // 收齐:按序拼接并移除槽位。
        let slot = self.slots.remove(&seg.frame_id)?;
        // 关键帧门控(F-1a):丢帧后 delta 帧不透传(丢弃计数),等关键帧恢复。
        if self.awaiting_key && !slot.key {
            self.stats.gated_frames += 1;
            return None;
        }
        self.awaiting_key = false; // 关键帧到达(或未门控),恢复输出
        let mut data = Vec::with_capacity(
            slot.parts
                .iter()
                .map(|p| p.as_ref().map(|v| v.len()).unwrap_or(0))
                .sum(),
        );
        for part in slot.parts {
            data.extend_from_slice(&part?);
        }
        Some(FrameData {
            frame_id: seg.frame_id,
            key: slot.key,
            codec: slot.codec,
            data,
        })
    }

    /// 当前统计(累计值,含超时丢帧/重复分片/乱序帧/门控丢弃)。
    pub fn stats(&self) -> ReassemblyStats {
        self.stats
    }

    /// 是否处于关键帧门控状态(丢帧后等待关键帧;诊断/测试用)。
    #[cfg_attr(not(test), allow(dead_code))] // 生产由门控内部消费;查询口供测试/诊断
    pub fn awaiting_keyframe(&self) -> bool {
        self.awaiting_key
    }
}

// ---------------------------------------------------------------------------
// UdpChannel:生产 UDP 通道(直连 / 中继两模式,接收侧共用分片重组)
// ---------------------------------------------------------------------------

/// UDP 通道模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UdpMode {
    /// 直连:向对端地址逐片 `send_to`(LAN 地址或 STUN 反射地址)。
    UdpDirect,
    /// 中继:分片帧整体作为 `data` 数据报 payload 过 dcr-relay UDP 21119 转发。
    UdpRelay,
}

impl UdpMode {
    /// 指标用短名(get_session_metrics.transport 的值)。
    #[allow(dead_code)] // network.rs 以字面量 "udp"/"relay-udp" 传递;保留供诊断报告/M5 复用
    pub fn as_str(&self) -> &'static str {
        match self {
            UdpMode::UdpDirect => "udp",
            UdpMode::UdpRelay => "relay-udp",
        }
    }
}

/// UDP 数据报接收上限(单片 ≤1200B,中继 `data` JSON 封装后 ≤2KB,留足余量)。
const RECV_BUF_LEN: usize = 8 * 1024;

/// UDP 视频通道:发送侧把分片(直连逐片 / 中继整体封装)发往对端;接收侧
/// [`recv_loop`] 收包 → parse → 重组,收齐一帧回调一次(负载与 TCP 模式同构)。
///
/// 建立流程由 network.rs 协商(UdpInit/UdpInitAck + udp-hello 互发确认),
/// 本结构只负责"已确认连通后"的数据面收发;发送失败由调用方回退 TCP。
///
/// 生产路径(network.rs)经 `from_socket`/`relay_with_socket` 复用协商期端口
/// 构造;`direct`/`relay` 便捷构造器与查询方法供诊断链路与独立使用者,
/// M5 接线(诊断 UDP 模式)后自然消警。
#[allow(dead_code)]
#[allow(dead_code)]
pub struct UdpChannel {
    sock: std::sync::Arc<tokio::net::UdpSocket>,
    mode: UdpMode,
    /// 发送目标:直连 = 对端 socket 地址;中继 = (relay 地址, 对端 id)。
    direct_peer: Option<std::net::SocketAddr>,
    relay_addr: Option<std::net::SocketAddr>,
    relay_peer_id: String,
    /// 重组统计快照(诊断报告用)。
    stats: std::sync::Arc<std::sync::Mutex<ReassemblyStats>>,
}

/// 便捷构造器与查询方法(direct/relay/mode/socket/socket_handle/stats)供
/// 诊断链路与独立使用者,M5 接线(诊断 UDP 模式)后自然消警。
#[allow(dead_code)]
impl UdpChannel {
    /// 创建通道并绑定本机随机 UDP 端口(直连模式,对端地址为协商结果)。
    pub async fn direct(peer: std::net::SocketAddr) -> Result<Self, String> {
        let sock = tokio::net::UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|e| format!("绑定 UDP 端口失败: {e}"))?;
        Ok(Self {
            sock: std::sync::Arc::new(sock),
            mode: UdpMode::UdpDirect,
            direct_peer: Some(peer),
            relay_addr: None,
            relay_peer_id: String::new(),
            stats: std::sync::Arc::new(std::sync::Mutex::new(ReassemblyStats::default())),
        })
    }

    /// 创建中继模式通道:向 `relay_udp_addr` 发 `data` 数据报(payload = 完整
    /// 分片帧 base64),转发目标为已 `alloc-udp` 登记的 `peer_id` 宿主。
    /// 绑定后需先经 [`alloc_relay`] 完成登记,对端才能收到本端发出的帧。
    pub async fn relay(
        relay_udp_addr: std::net::SocketAddr,
        peer_id: &str,
    ) -> Result<Self, String> {
        let local = if relay_udp_addr.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        };
        let sock = tokio::net::UdpSocket::bind(local)
            .await
            .map_err(|e| format!("绑定 UDP 端口失败: {e}"))?;
        Ok(Self {
            sock: std::sync::Arc::new(sock),
            mode: UdpMode::UdpRelay,
            direct_peer: None,
            relay_addr: Some(relay_udp_addr),
            relay_peer_id: peer_id.to_string(),
            stats: std::sync::Arc::new(std::sync::Mutex::new(ReassemblyStats::default())),
        })
    }

    /// 从已有 socket 包一层(测试/诊断用;mode 与发送目标由参数给定)。
    pub fn from_socket(
        sock: std::sync::Arc<tokio::net::UdpSocket>,
        mode: UdpMode,
        peer: Option<std::net::SocketAddr>,
    ) -> Self {
        Self {
            sock,
            mode,
            direct_peer: if mode == UdpMode::UdpDirect { peer } else { None },
            relay_addr: if mode == UdpMode::UdpRelay { peer } else { None },
            relay_peer_id: String::new(),
            stats: std::sync::Arc::new(std::sync::Mutex::new(ReassemblyStats::default())),
        }
    }

    /// 中继模式:复用已有 socket(如协商期已 alloc-udp 登记的端口),
    /// 发送目标为中继地址 + 转发目标 id。与 [`UdpChannel::relay`] 的区别
    /// 仅是不重新绑定端口(登记映射保持在同一 socket 上)。
    pub fn relay_with_socket(
        sock: std::sync::Arc<tokio::net::UdpSocket>,
        relay_addr: std::net::SocketAddr,
        peer_id: &str,
    ) -> Self {
        Self {
            sock,
            mode: UdpMode::UdpRelay,
            direct_peer: None,
            relay_addr: Some(relay_addr),
            relay_peer_id: peer_id.to_string(),
            stats: std::sync::Arc::new(std::sync::Mutex::new(ReassemblyStats::default())),
        }
    }

    /// 当前模式。
    pub fn mode(&self) -> UdpMode {
        self.mode
    }

    /// 本机绑定地址(协商时经 UdpInit 下发 listen_port 用)。
    pub fn local_addr(&self) -> Result<std::net::SocketAddr, String> {
        self.sock.local_addr().map_err(|e| e.to_string())
    }

    /// 底层 socket 引用(协商期发 udp-hello / 探测复用同一端口)。
    pub fn socket(&self) -> &tokio::net::UdpSocket {
        &self.sock
    }

    /// 底层 socket 共享句柄(协商期与通道复用同一端口)。
    pub fn socket_handle(&self) -> std::sync::Arc<tokio::net::UdpSocket> {
        self.sock.clone()
    }

    /// 向对端发送一段原始字节(直连裸发;中继封装为 data 数据报)。
    /// 用于协商期 udp-hello 与数据面分片共用同一出口。
    pub async fn send_raw(&self, bytes: &[u8]) -> Result<(), String> {
        match self.mode {
            UdpMode::UdpDirect => {
                let peer = self
                    .direct_peer
                    .ok_or("直连模式未设置对端地址")?;
                self.sock
                    .send_to(bytes, peer)
                    .await
                    .map_err(|e| format!("UDP 直连发送失败: {e}"))?;
                Ok(())
            }
            UdpMode::UdpRelay => {
                let relay = self
                    .relay_addr
                    .ok_or("中继模式未设置中继地址")?;
                if self.relay_peer_id.is_empty() {
                    // 未指定转发目标(如协商期探测):直接发裸字节给中继
                    self.sock
                        .send_to(bytes, relay)
                        .await
                        .map_err(|e| format!("UDP 中继发送失败: {e}"))?;
                    return Ok(());
                }
                let gram = crate::network::encode_relay_udp_data(&self.relay_peer_id, bytes)?;
                self.sock
                    .send_to(&gram, relay)
                    .await
                    .map_err(|e| format!("UDP 中继发送失败: {e}"))?;
                Ok(())
            }
        }
    }

    /// 发送一个已分片的视频帧(逐片 send;中继模式逐片封装 data 数据报)。
    /// 任一片失败即返回 Err(调用方回退 TCP,不中断会话)。
    pub async fn send_packet(&self, segs: &[UdpSegment]) -> Result<(), String> {
        for seg in segs {
            let bytes = encode_segment(seg);
            self.send_raw(&bytes).await?;
        }
        Ok(())
    }

    /// 接收循环:收包 → parse_segment(中继模式下收到的是中继转发的裸二进制
    /// 分片,与直连同构)→ 重组;每收齐一帧回调 `on_frame(frame_id, key, codec, data)`。
    ///
    /// 非分片包(如对端 udp-hello JSON、中继 allocated 应答)交给 `on_control`
    /// (可选),由协商层处理;包解析失败仅记日志继续,不终止循环。
    /// 返回值:Err 仅在 socket 错误时给出(调用方回退 TCP)。
    pub async fn recv_loop<F, C>(
        &self,
        mut on_frame: F,
        mut on_control: C,
    ) -> Result<(), String>
    where
        F: FnMut(u32, bool, u8, Vec<u8>),
        C: FnMut(&[u8]),
    {
        let mut re = FragmentReassembler::new(REASSEMBLY_TIMEOUT_MS);
        let mut buf = vec![0u8; RECV_BUF_LEN];
        loop {
            let (n, src) = self
                .sock
                .recv_from(&mut buf)
                .await
                .map_err(|e| format!("UDP 接收失败: {e}"))?;
            let packet = &buf[..n];
            match parse_segment(packet) {
                Ok(seg) => {
                    if let Some(frame) = re.push(seg) {
                        on_frame(frame.frame_id, frame.key, frame.codec, frame.data);
                    }
                }
                Err(_) => {
                    // 非分片帧:控制包(udp-hello / allocated / 探测)交协商层
                    on_control(packet);
                    let _ = src;
                }
            }
            // 快照统计供诊断
            if let Ok(mut s) = self.stats.lock() {
                *s = re.stats();
            }
        }
    }

    /// 重组统计快照(丢帧/缺片/乱序,诊断报告与会话浮窗用)。
    pub fn stats(&self) -> ReassemblyStats {
        self.stats
            .lock()
            .map(|s| *s)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 简单 LCG 伪随机数(无外部依赖,可复现)。
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0 >> 16
        }
        /// [min, max] 闭区间均匀取值。
        fn range(&mut self, min: u64, max: u64) -> u64 {
            min + self.next() % (max - min + 1)
        }
        /// 用 LCG 输出填充字节(确定性内容)。
        fn fill(&mut self, len: usize) -> Vec<u8> {
            (0..len).map(|_| (self.next() & 0xff) as u8).collect()
        }
    }

    /// ① 头编解码往返:各种规模/flags/codec 下 encode→parse 逐字段一致,
    /// 且线格式自校验(错误 magic、短包、rfu 非零、冗余位不一致)均拒绝。
    #[test]
    fn segment_codec_roundtrip() {
        let cases = vec![
            (1u32, 0u16, 1u16, true, CODEC_H264, 0usize),
            (2, 0, 3, false, CODEC_HEVC, 100),
            (u32::MAX, 54, 55, true, CODEC_H264, 1184), // mtu=1200 的最后一片满载
            (42, 7, 8, false, CODEC_HEVC, 1),
        ];
        for (frame_id, frag_idx, frag_cnt, key, codec, plen) in cases {
            let seg = UdpSegment {
                frame_id,
                frag_idx,
                frag_cnt,
                key,
                codec,
                payload: (0..plen).map(|i| (i % 251) as u8).collect(),
            };
            let bytes = encode_segment(&seg);
            assert!(bytes.len() <= SEGMENT_MTU || plen + SEGMENT_HEADER_LEN > SEGMENT_MTU);
            assert_eq!(bytes.len(), SEGMENT_HEADER_LEN + plen);
            let back = parse_segment(&bytes).unwrap_or_else(|e| panic!("解析失败: {e}"));
            assert_eq!(back, seg);
        }

        // 非法输入拒绝。
        let ok = encode_segment(&UdpSegment {
            frame_id: 1,
            frag_idx: 0,
            frag_cnt: 2,
            key: false,
            codec: CODEC_H264,
            payload: vec![1, 2, 3],
        });
        assert!(
            parse_segment(&ok[..SEGMENT_HEADER_LEN - 1]).is_err(),
            "短于头的包应拒绝"
        );
        let mut bad = ok.clone();
        bad[0] ^= 0xFF; // 破坏 magic
        assert!(parse_segment(&bad).is_err(), "magic 错应拒绝");
        let mut bad = ok.clone();
        bad[15] = 1; // rfu 非零
        assert!(parse_segment(&bad).is_err(), "rfu 非零应拒绝");
        // flags 为小端 u16(bytes[12]=低字节):非最后片置 FLAG_LAST → 冗余位不一致
        let mut bad = ok.clone();
        bad[12] = 0x02; // frag_idx=0、frag_cnt=2,不是最后片
        assert!(parse_segment(&bad).is_err(), "非最后片置 FLAG_LAST 应拒绝");
        // 反向:最后片清掉 FLAG_LAST → 同样不一致
        let mut bad2 = encode_segment(&UdpSegment {
            frame_id: 1,
            frag_idx: 1,
            frag_cnt: 2,
            key: false,
            codec: CODEC_H264,
            payload: vec![4, 5],
        });
        bad2[12] = 0x00; // 最后片清掉 FLAG_LAST
        assert!(parse_segment(&bad2).is_err(), "最后片缺 FLAG_LAST 应拒绝");
        let mut bad = ok.clone();
        bad[14] = 9; // 未知 codec
        assert!(parse_segment(&bad).is_err(), "未知 codec 应拒绝");
        // frag_idx 越界。
        let mut bad = ok.clone();
        bad[8..10].copy_from_slice(&5u16.to_le_bytes());
        assert!(parse_segment(&bad).is_err(), "frag_idx 越界应拒绝");
    }

    /// ② C3 核心:500 个随机大小(0.5KB~64KB)帧分片 → 部分乱序投喂 →
    /// 重组逐字节相等、零丢失(回环即本测试)。
    #[test]
    fn reassemble_500_frames_reordered() {
        let mut rng = Lcg(0x5EED_2026_0821);
        let mut re = FragmentReassembler::new(REASSEMBLY_TIMEOUT_MS);
        let t0 = std::time::Instant::now();
        let mut total_segs = 0usize;
        let mut total_bytes = 0usize;
        for frame_id in 1u32..=500 {
            let len = rng.range(512, 64 * 1024) as usize;
            let data = rng.fill(len);
            let key = frame_id % 30 == 1; // 约 30 帧一个关键帧
            let codec = if frame_id % 3 == 0 { CODEC_HEVC } else { CODEC_H264 };
            let segs = split_bytes(frame_id, key, codec, &data, SEGMENT_MTU);
            assert!(
                segs.iter().all(|s| encode_segment(s).len() <= SEGMENT_MTU),
                "单片不得超 MTU"
            );
            // 拼接回去必须等于原始数据(分片完备性)。
            let joined: Vec<u8> = segs.iter().flat_map(|s| s.payload.iter().copied()).collect();
            assert_eq!(joined, data, "分片拼接应与原帧一致");
            total_segs += segs.len();
            total_bytes += len;
            // 线格式编码后解析,再乱序投喂(>2 片的帧把前 2 片与最后 1 片打乱)。
            let mut parsed: Vec<UdpSegment> = segs.iter().map(|s| parse_segment(&encode_segment(s)).unwrap()).collect();
            if parsed.len() > 2 {
                let last = parsed.len() - 1;
                parsed.swap(0, last); // 首片与最后片互换
                let third = 2.min(last);
                parsed.swap(1, third); // 第二片再错位
            }
            let got = parsed
                .into_iter()
                .find_map(|s| re.push(s))
                .expect("所有分片投喂完应收齐一帧");
            assert_eq!(got.frame_id, frame_id);
            assert_eq!(got.key, key);
            assert_eq!(got.codec, codec);
            assert_eq!(got.data, data, "重组帧应与原始字节逐字节相等");
        }
        let stats = re.stats();
        assert_eq!(stats.dropped_frames, 0, "回环不允许丢帧");
        assert_eq!(stats.lost_frags, 0, "回环不允许重复/缺片");
        assert!(stats.reordered > 0, "乱序路径应被真实触发");
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        println!(
            "500 帧分片重组: 总分片 {total_segs}, 总字节 {total_bytes}, 总耗时 {ms:.1}ms, 平均单帧 {:.3}ms",
            ms / 500.0
        );
    }

    /// ③ 超时丢帧路径:丢一半分片,超时(压缩到 20ms)后应整帧丢弃并计数;
    /// 随后投喂新帧分片仍能正常收齐(等下一关键帧语义)。
    #[test]
    fn reassemble_timeout_drops_partial_frame() {
        let mut re = FragmentReassembler::new(20);
        // 帧 1:只投 0/4 片。
        let segs = split_bytes(1, true, CODEC_H264, &vec![7u8; 4096], SEGMENT_MTU);
        assert_eq!(segs.len(), 4);
        assert!(re.push(segs[0].clone()).is_none());
        // 等 25ms > 20ms 超时。
        std::thread::sleep(Duration::from_millis(25));
        // 帧 2:完整投喂,触发 prune 清理帧 1 并正常收齐帧 2。
        let segs2 = split_bytes(2, true, CODEC_H264, &vec![9u8; 3000], SEGMENT_MTU);
        let mut got = None;
        for s in &segs2 {
            if let Some(f) = re.push(s.clone()) {
                got = Some(f);
            }
        }
        let got = got.expect("帧 2 应完整收齐");
        assert_eq!(got.data, vec![9u8; 3000]);
        let stats = re.stats();
        assert_eq!(stats.dropped_frames, 1, "超时帧 1 应被丢弃");
        assert_eq!(stats.lost_frags, 3, "帧 1 缺 3 片应计入 lost_frags");
    }

    /// 重复分片:同片二次投喂被忽略并计入 lost_frags,帧仍可收齐。
    #[test]
    fn duplicate_fragment_counted() {
        let mut re = FragmentReassembler::new(REASSEMBLY_TIMEOUT_MS);
        let segs = split_bytes(7, false, CODEC_H264, &vec![5u8; 2000], SEGMENT_MTU);
        assert_eq!(segs.len(), 2);
        assert!(re.push(segs[0].clone()).is_none());
        assert!(re.push(segs[0].clone()).is_none(), "重复片应被忽略");
        let got = re.push(segs[1].clone()).expect("仍应收齐");
        assert_eq!(got.data, vec![5u8; 2000]);
        assert_eq!(re.stats().lost_frags, 1);
        assert_eq!(re.stats().dropped_frames, 0);
    }

    /// F-1a 关键帧门控:丢帧超时后,完整到达的 delta 帧不透传(计数 gated),
    /// 直到下一个关键帧到达才恢复输出——WebCodecs 不会收到无法解码的悬空 delta。
    #[test]
    fn keyframe_gating_after_timeout_drop() {
        let mut re = FragmentReassembler::new(20);
        // 帧 1(关键帧)只投 1/2 片 → 超时丢弃 → 进入门控。
        let segs = split_bytes(1, true, CODEC_H264, &vec![3u8; 1800], SEGMENT_MTU);
        assert!(re.push(segs[0].clone()).is_none());
        std::thread::sleep(Duration::from_millis(25));
        // 帧 2(delta)完整到达:门控期间不透传。
        let segs2 = split_bytes(2, false, CODEC_H264, &vec![4u8; 1800], SEGMENT_MTU);
        let mut got_delta = None;
        for s in &segs2 {
            if let Some(f) = re.push(s.clone()) {
                got_delta = Some(f);
            }
        }
        assert!(got_delta.is_none(), "丢帧后 delta 帧不应透传");
        assert!(re.awaiting_keyframe(), "应处于等关键帧状态");
        // 帧 3(delta)同样被门控。
        let segs3 = split_bytes(3, false, CODEC_H264, &vec![6u8; 1500], SEGMENT_MTU);
        let mut got3 = None;
        for s in &segs3 {
            if let Some(f) = re.push(s.clone()) {
                got3 = Some(f);
            }
        }
        assert!(got3.is_none(), "门控期间后续 delta 仍不应透传");
        // 帧 4(关键帧)完整到达:透传并解除门控。
        let segs4 = split_bytes(4, true, CODEC_H264, &vec![7u8; 1500], SEGMENT_MTU);
        let mut got4 = None;
        for s in &segs4 {
            if let Some(f) = re.push(s.clone()) {
                got4 = Some(f);
            }
        }
        let key_frame = got4.expect("关键帧应透传并解除门控");
        assert_eq!(key_frame.data, vec![7u8; 1500]);
        assert!(!re.awaiting_keyframe(), "关键帧后应解除门控");
        // 帧 5(delta)恢复正常透传。
        let segs5 = split_bytes(5, false, CODEC_H264, &vec![8u8; 1500], SEGMENT_MTU);
        let mut got5 = None;
        for s in &segs5 {
            if let Some(f) = re.push(s.clone()) {
                got5 = Some(f);
            }
        }
        assert!(got5.is_some(), "解除门控后 delta 恢复透传");
        let stats = re.stats();
        assert_eq!(stats.dropped_frames, 1);
        assert_eq!(stats.gated_frames, 2, "两帧被门控的 delta 应计数");
    }

    /// F-1a 补充:无丢帧时门控不影响 delta 透传(正常流零门控)。
    #[test]
    fn keyframe_gating_inactive_without_loss() {
        let mut re = FragmentReassembler::new(REASSEMBLY_TIMEOUT_MS);
        // key → delta → delta 全部正常透传。
        for (id, key) in [(1u32, true), (2, false), (3, false)] {
            let segs = split_bytes(id, key, CODEC_H264, &vec![id as u8; 1600], SEGMENT_MTU);
            let mut got = None;
            for s in &segs {
                if let Some(f) = re.push(s.clone()) {
                    got = Some(f);
                }
            }
            assert!(got.is_some(), "无丢帧时帧 {id} 应正常透传");
        }
        assert_eq!(re.stats().gated_frames, 0);
        assert!(!re.awaiting_keyframe());
    }

    // ------------------------------------------------------------------
    // UdpChannel:直连/中继回环(真实 UdpSocket,C3/C4 的 crate 内验证)
    // ------------------------------------------------------------------

    /// 简单 LCG(与上方同实现,异步测试内复用)。
    fn lcg_fill(seed: u64, len: usize) -> Vec<u8> {
        let mut s = seed;
        (0..len)
            .map(|_| {
                s = s
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                (s >> 16) as u8
            })
            .collect()
    }

    /// ② UdpChannel 直连回环:两个真实 UDP 端口,A→B 发 20 个分片化帧,
    /// B 经 recv_loop 重组,逐字节相等、零丢帧;控制包(udp-hello JSON)同通道送达。
    #[tokio::test]
    async fn udp_channel_direct_loopback() {
        let bind_b = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr_b = bind_b.local_addr().unwrap();
        let chan_a = UdpChannel::direct(addr_b).await.unwrap();

        // B 侧接收循环(分片 → 帧;控制包 → 记录)
        let (tx_frame, mut rx_frame) = tokio::sync::mpsc::unbounded_channel::<(u32, bool, u8, Vec<u8>)>();
        let (tx_ctrl, mut rx_ctrl) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let recv_task = tokio::spawn(async move {
            let chan = UdpChannel::from_socket(std::sync::Arc::new(bind_b), UdpMode::UdpDirect, None);
            chan.recv_loop(
                |id, key, codec, data| {
                    let _ = tx_frame.send((id, key, codec, data));
                },
                |pkt| {
                    let _ = tx_ctrl.send(pkt.to_vec());
                },
            )
            .await
        });

        // 先发一个 udp-hello 控制包(协商期同通道)
        chan_a
            .send_raw(br#"{"t":"udp-hello","token":"tok-1"}"#)
            .await
            .unwrap();

        // 发 20 个随机大小帧(0.5KB~24KB);帧间 2ms 间隔模拟生产 fps 节流
        // (host_write_loop 按帧率推送;零间隔突发约 120KB 会瞬间塞满接收侧
        // OS 缓冲——那是 UDP 语义本身,非分片协议缺陷)
        let mut sent: HashMap<u32, Vec<u8>> = HashMap::new();
        for frame_id in 1u32..=20 {
            let len = 512 + (frame_id as usize * 1173) % (24 * 1024);
            let data = lcg_fill(u64::from(frame_id), len);
            let segs = split_bytes(frame_id, frame_id % 5 == 1, CODEC_H264, &data, SEGMENT_MTU);
            chan_a.send_packet(&segs).await.unwrap();
            sent.insert(frame_id, data);
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }

        // 校验控制包
        let ctrl = tokio::time::timeout(std::time::Duration::from_secs(3), rx_ctrl.recv())
            .await
            .expect("控制包超时")
            .unwrap();
        assert!(ctrl.starts_with(b"{\"t\":\"udp-hello\""));

        // 校验 20 帧全部重组一致
        let mut got: HashMap<u32, (bool, u8, Vec<u8>)> = HashMap::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while got.len() < 20 && std::time::Instant::now() < deadline {
            match tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                rx_frame.recv(),
            )
            .await
            {
                Ok(Some((id, key, codec, data))) => {
                    got.insert(id, (key, codec, data));
                }
                _ => break,
            }
        }
        assert_eq!(got.len(), 20, "20 帧应全部收齐,实际 {}", got.len());
        for (id, data) in &sent {
            let (key, codec, recv) = &got[id];
            assert_eq!(recv, data, "帧 {id} 重组字节应一致");
            assert_eq!(*codec, CODEC_H264);
            assert_eq!(*key, id % 5 == 1);
        }
        recv_task.abort();
    }

    /// ③ UdpChannel 中继模式回环:起真实 relay::serve_udp(dcr-server 库),
    /// host 侧 alloc-udp 登记后,client 侧 UdpChannel(UdpRelay)发送分片帧,
    /// host 侧 recv_loop 收到中继透传的裸二进制分片并重组一致。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn udp_channel_relay_loopback() {
        // 起真实中继 UDP 服务
        let relay_sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let relay_addr = relay_sock.local_addr().unwrap();
        let relay_task = tokio::spawn(async move {
            let _ = dcr_server::relay::serve_udp(relay_sock).await;
        });

        // host 侧:绑定 UDP 端口并 alloc-udp 登记(id=relay-host-1)
        let host_bind = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let host_addr = host_bind.local_addr().unwrap();
        let alloc = serde_json::json!({ "t": "alloc-udp", "id": "relay-host-1" });
        host_bind
            .send_to(alloc.to_string().as_bytes(), relay_addr)
            .await
            .unwrap();
        let mut ack_buf = [0u8; 128];
        let (n, _) = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            host_bind.recv_from(&mut ack_buf),
        )
        .await
        .expect("allocated 应答超时")
        .unwrap();
        assert_eq!(&ack_buf[..n], b"{\"t\":\"allocated\"}", "登记应答 allocated");
        let _ = host_addr;

        // host 侧接收循环(收到的是中继透传的裸二进制分片,与直连同构)
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(u32, bool, u8, Vec<u8>)>();
        let host_task = tokio::spawn(async move {
            let chan = UdpChannel::from_socket(std::sync::Arc::new(host_bind), UdpMode::UdpDirect, None);
            chan.recv_loop(
                |id, key, codec, data| {
                    let _ = tx.send((id, key, codec, data));
                },
                |_| {},
            )
            .await
        });

        // client 侧:中继模式通道,目标 id=relay-host-1
        let chan = UdpChannel::relay(relay_addr, "relay-host-1").await.unwrap();
        assert_eq!(chan.mode(), UdpMode::UdpRelay);
        assert_eq!(chan.mode().as_str(), "relay-udp");

        // 发 5 个分片化帧(含 24KB 大帧,>20 片)
        let mut sent: HashMap<u32, Vec<u8>> = HashMap::new();
        for frame_id in 1u32..=5 {
            let len = 800 * frame_id as usize + 1024 * 12;
            let data = lcg_fill(0xBEEF + u64::from(frame_id), len);
            let segs = split_bytes(frame_id, frame_id == 1, CODEC_HEVC, &data, SEGMENT_MTU);
            assert!(segs.len() > 1, "测试帧应有多分片");
            chan.send_packet(&segs).await.unwrap();
            sent.insert(frame_id, data);
        }

        let mut got: HashMap<u32, (bool, u8, Vec<u8>)> = HashMap::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while got.len() < 5 && std::time::Instant::now() < deadline {
            match tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                rx.recv(),
            )
            .await
            {
                Ok(Some((id, key, codec, data))) => {
                    got.insert(id, (key, codec, data));
                }
                _ => break,
            }
        }
        assert_eq!(got.len(), 5, "5 帧应全部经中继转发并重组,实际 {}", got.len());
        for (id, data) in &sent {
            let (key, codec, recv) = &got[id];
            assert_eq!(recv, data, "帧 {id} 经中继重组字节应一致");
            assert_eq!(*codec, CODEC_HEVC);
            assert_eq!(*key, *id == 1);
        }
        host_task.abort();
        relay_task.abort();
    }
}
