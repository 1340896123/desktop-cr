//! FFmpeg 硬件编解码模块(动态加载,运行时探测,仅 Windows)。
//!
//! 通过 `libloading` 在运行时加载 FFmpeg DLL(avcodec/avutil/swscale),避免构建期
//! 链接 FFmpeg(无编译负担,缺 DLL 时优雅回退 JPEG 管线)。编码器/解码器均通过
//! `av_opt_*` 配置,不依赖 AVCodecContext 结构体布局,兼容 FFmpeg 5~9。
//!
//! 能力探测(hwinfo 逻辑):
//!   - `detect_gpus()`:DXGI 枚举适配器(厂商 ID/型号/显存),映射 NVIDIA/Intel/AMD
//!   - `preferred_encoder()`:按 GPU 厂商优先选 h264_nvenc / h264_qsv / h264_amf,
//!     软件回退 libopenh264 / h264;全部不可用返回 None → 调用方回退 JPEG。
//!
//! 编码:H.264 Annex-B(低延迟:无 B 帧 + LOW_DELAY + GOP 2s + 按需强制 IDR),
//!       RGB24 → swscale → YUV420P → avcodec 编码。
//! 解码:H.264 Annex-B → YUV420P → swscale → RGB24(控制器端转 JPEG 供前端显示)。
//!
//! DLL 搜索顺序:`资源目录/ffmpeg` → 可执行文件旁 `ffmpeg/` → `CARGO_MANIFEST_DIR/resources/ffmpeg`
//!   → `FFMPEG_HOME` → 系统 PATH。缺 DLL 时所有接口返回 None/Err,上游走 JPEG 管线。

use std::os::raw::{c_char, c_int, c_void};
use std::sync::OnceLock;

use libloading::{Library, Symbol};

// ---------------------------------------------------------------------------
// FFmpeg 常量(版本间稳定)
// ---------------------------------------------------------------------------

/// AV_PIX_FMT_YUV420P
const AV_PIX_FMT_YUV420P: c_int = 0;
/// AV_PIX_FMT_RGB24
const AV_PIX_FMT_RGB24: c_int = 2;
/// AV_CODEC_ID_H264
const AV_CODEC_ID_H264: c_int = 27;
/// AV_PKT_FLAG_KEY
const AV_PKT_FLAG_KEY: c_int = 0x0001;
/// AV_CODEC_FLAG_LOW_DELAY (1 << 19)
const AV_CODEC_FLAG_LOW_DELAY: i64 = 0x0008_0000;
/// AV_OPT_SEARCH_CHILDREN
const AV_OPT_SEARCH_CHILDREN: c_int = 1;
/// SWS_BILINEAR
const SWS_BILINEAR: c_int = 2;
/// AVERROR(EAGAIN) / AVERROR(EIO=EOF) —— Windows errno 与 POSIX 一致
const AVERROR_EAGAIN: c_int = -11;
const AVERROR_EOF: c_int = -5;

/// FFmpeg 各主版本 DLL 文件名候选(新版本优先)。
const AVCODEC_NAMES: &[&str] = &[
    "avcodec-63", "avcodec-62", "avcodec-61", "avcodec-60", "avcodec-59", "avcodec-58", "avcodec",
];
const AVUTIL_NAMES: &[&str] = &[
    "avutil-61", "avutil-60", "avutil-59", "avutil-58", "avutil-57", "avutil-56", "avutil",
];
const SWSCALE_NAMES: &[&str] = &[
    "swscale-10", "swscale-8", "swscale-7", "swscale-6", "swscale-5", "swscale",
];
const SWRESAMPLE_NAMES: &[&str] = &[
    "swresample-7", "swresample-5", "swresample-4", "swresample-3", "swresample",
];

// ---------------------------------------------------------------------------
// 最小 FFI 结构(仅访问稳定前缀字段)
// ---------------------------------------------------------------------------

/// AVFrame 前导字段(自 FFmpeg 5.0 起布局稳定,后续字段不访问)。
#[repr(C)]
struct AvFrame {
    data: [*mut u8; 8],
    linesize: [c_int; 8],
    extended_data: *mut *mut u8,
    width: c_int,
    height: c_int,
    nb_samples: c_int,
    format: c_int,
    key_frame: c_int,
    pict_type: c_int,
    sample_aspect_ratio: AvRational,
    pts: i64,
    pkt_dts: i64,
    time_base: AvRational,
}

/// AVPacket 前导字段(仅读取 data/size/flags,后续字段不访问)。
#[repr(C)]
struct AvPacket {
    buf: *mut c_void,
    pts: i64,
    dts: i64,
    data: *mut u8,
    size: c_int,
    stream_index: c_int,
    flags: c_int,
}

/// AVChannelLayout(ABI 稳定)。
#[repr(C)]
struct AvChannelLayout {
    order: c_int,
    nb_channels: c_int,
    mask: u64,
    opaque: *mut c_void,
}

/// AVCodecParameters(ABI 稳定,严格对齐 avcodec.h/codec_par.h 布局)。
/// 用于向 AVCodecContext 写入宽高/像素格式/码率/帧率(FFmpeg 8+ 已移除对应 AVOption)。
#[repr(C)]
struct AvCodecParameters {
    codec_type: c_int,
    codec_id: c_int,
    codec_tag: u32,
    extradata: *mut u8,
    extradata_size: c_int,
    coded_side_data: *mut c_void,
    nb_coded_side_data: c_int,
    format: c_int,
    bit_rate: i64,
    bits_per_coded_sample: c_int,
    bits_per_raw_sample: c_int,
    profile: c_int,
    level: c_int,
    width: c_int,
    height: c_int,
    sample_aspect_ratio: AvRational,
    framerate: AvRational,
    field_order: c_int,
    color_range: c_int,
    color_primaries: c_int,
    color_trc: c_int,
    color_space: c_int,
    chroma_location: c_int,
    video_delay: c_int,
    ch_layout: AvChannelLayout,
    sample_rate: c_int,
    block_align: c_int,
    frame_size: c_int,
    initial_padding: c_int,
    trailing_padding: c_int,
    seek_preroll: c_int,
    alpha_mode: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct AvRational {
    num: c_int,
    den: c_int,
}

// ---------------------------------------------------------------------------
// 符号类型
// ---------------------------------------------------------------------------

type FindEncByName = unsafe extern "C" fn(*const c_char) -> *const c_void;
type FindDecoder = unsafe extern "C" fn(c_int) -> *const c_void;
type AllocCtx3 = unsafe extern "C" fn(*const c_void) -> *mut c_void;
type Open2 = unsafe extern "C" fn(*mut c_void, *const c_void, *mut *mut c_void) -> c_int;
type SendFrame = unsafe extern "C" fn(*mut c_void, *const AvFrame) -> c_int;
type ReceivePacket = unsafe extern "C" fn(*mut c_void, *mut AvPacket) -> c_int;
type SendPacket = unsafe extern "C" fn(*mut c_void, *const AvPacket) -> c_int;
type ReceiveFrame = unsafe extern "C" fn(*mut c_void, *mut AvFrame) -> c_int;
type FreeCtx = unsafe extern "C" fn(*mut *mut c_void);
type FrameAlloc = unsafe extern "C" fn() -> *mut AvFrame;
type FrameFree = unsafe extern "C" fn(*mut *mut AvFrame);
type FrameGetBuffer = unsafe extern "C" fn(*mut AvFrame, c_int) -> c_int;
type FrameUnref = unsafe extern "C" fn(*mut AvFrame);
type PacketAlloc = unsafe extern "C" fn() -> *mut AvPacket;
type PacketFree = unsafe extern "C" fn(*mut *mut AvPacket);
type PacketUnref = unsafe extern "C" fn(*mut AvPacket);
type OptSetInt = unsafe extern "C" fn(*mut c_void, *const c_char, i64, c_int) -> c_int;
type OptSetQ = unsafe extern "C" fn(*mut c_void, *const c_char, AvRational, c_int) -> c_int;
type OptSet = unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char, c_int) -> c_int;
type StrError = unsafe extern "C" fn(c_int, *mut c_char, usize) -> c_int;
type ParametersAlloc = unsafe extern "C" fn() -> *mut AvCodecParameters;
type ParametersFree = unsafe extern "C" fn(*mut *mut AvCodecParameters);
type ParametersToContext = unsafe extern "C" fn(*mut c_void, *const AvCodecParameters) -> c_int;
type SwsGetCtx =
    unsafe extern "C" fn(c_int, c_int, c_int, c_int, c_int, c_int, c_int, *mut c_void, *mut c_void, *const f64) -> *mut c_void;
type SwsScale = unsafe extern "C" fn(
    *mut c_void,
    *const *const u8,
    *const c_int,
    c_int,
    c_int,
    *const *mut u8,
    *const c_int,
) -> c_int;
type SwsFree = unsafe extern "C" fn(*mut c_void);

// ---------------------------------------------------------------------------
// 加载与符号解析
// ---------------------------------------------------------------------------

/// 持有的 FFmpeg 库(经 Box::leak 提为 'static,进程生命周期内不释放)。
/// 按依赖顺序加载(avutil → swresample → swscale → avcodec),使依赖先驻留进程,
/// 后续加载 avcodec 时其 import 表可按模块名命中已加载的 avutil/swresample。
struct Libs {
    avcodec: &'static Library,
    avutil: &'static Library,
    swscale: &'static Library,
    /// 仅用于维持 avcodec 依赖驻留(本模块不直接使用)。
    _swresample: &'static Library,
}

static LIBS: OnceLock<Option<&'static Libs>> = OnceLock::new();

/// 解析出的全部符号(构建一次,后续零查找开销)。
struct Fns {
    avcodec_find_encoder_by_name: Symbol<'static, FindEncByName>,
    avcodec_find_decoder: Symbol<'static, FindDecoder>,
    avcodec_alloc_context3: Symbol<'static, AllocCtx3>,
    avcodec_open2: Symbol<'static, Open2>,
    avcodec_send_frame: Symbol<'static, SendFrame>,
    avcodec_receive_packet: Symbol<'static, ReceivePacket>,
    avcodec_send_packet: Symbol<'static, SendPacket>,
    avcodec_receive_frame: Symbol<'static, ReceiveFrame>,
    avcodec_free_context: Symbol<'static, FreeCtx>,
    av_frame_alloc: Symbol<'static, FrameAlloc>,
    av_frame_free: Symbol<'static, FrameFree>,
    av_frame_get_buffer: Symbol<'static, FrameGetBuffer>,
    av_frame_unref: Symbol<'static, FrameUnref>,
    av_packet_alloc: Symbol<'static, PacketAlloc>,
    av_packet_free: Symbol<'static, PacketFree>,
    av_packet_unref: Symbol<'static, PacketUnref>,
    av_opt_set_int: Symbol<'static, OptSetInt>,
    av_opt_set_q: Symbol<'static, OptSetQ>,
    av_opt_set: Symbol<'static, OptSet>,
    av_strerror: Symbol<'static, StrError>,
    avcodec_parameters_alloc: Symbol<'static, ParametersAlloc>,
    avcodec_parameters_free: Symbol<'static, ParametersFree>,
    avcodec_parameters_to_context: Symbol<'static, ParametersToContext>,
    sws_get_context: Symbol<'static, SwsGetCtx>,
    sws_scale: Symbol<'static, SwsScale>,
    sws_free_context: Symbol<'static, SwsFree>,
}

static FNS: OnceLock<Option<&'static Fns>> = OnceLock::new();

/// 返回已加载的符号集合;DLL 缺失/加载失败时返回 None。
fn fns() -> Option<&'static Fns> {
    FNS.get_or_init(build_fns).as_deref()
}

fn build_fns() -> Option<&'static Fns> {
    let libs = libs()?;
    unsafe {
        Some(Box::leak(Box::new(Fns {
            avcodec_find_encoder_by_name: libs.avcodec.get(b"avcodec_find_encoder_by_name").ok()?,
            avcodec_find_decoder: libs.avcodec.get(b"avcodec_find_decoder").ok()?,
            avcodec_alloc_context3: libs.avcodec.get(b"avcodec_alloc_context3").ok()?,
            avcodec_open2: libs.avcodec.get(b"avcodec_open2").ok()?,
            avcodec_send_frame: libs.avcodec.get(b"avcodec_send_frame").ok()?,
            avcodec_receive_packet: libs.avcodec.get(b"avcodec_receive_packet").ok()?,
            avcodec_send_packet: libs.avcodec.get(b"avcodec_send_packet").ok()?,
            avcodec_receive_frame: libs.avcodec.get(b"avcodec_receive_frame").ok()?,
            avcodec_free_context: libs.avcodec.get(b"avcodec_free_context").ok()?,
            av_frame_alloc: libs.avutil.get(b"av_frame_alloc").ok()?,
            av_frame_free: libs.avutil.get(b"av_frame_free").ok()?,
            av_frame_get_buffer: libs.avutil.get(b"av_frame_get_buffer").ok()?,
            av_frame_unref: libs.avutil.get(b"av_frame_unref").ok()?,
            av_packet_alloc: libs.avcodec.get(b"av_packet_alloc").ok()?,
            av_packet_free: libs.avcodec.get(b"av_packet_free").ok()?,
            av_packet_unref: libs.avcodec.get(b"av_packet_unref").ok()?,
            av_opt_set_int: libs.avutil.get(b"av_opt_set_int").ok()?,
            av_opt_set_q: libs.avutil.get(b"av_opt_set_q").ok()?,
            av_opt_set: libs.avutil.get(b"av_opt_set").ok()?,
            av_strerror: libs.avutil.get(b"av_strerror").ok()?,
            avcodec_parameters_alloc: libs.avcodec.get(b"avcodec_parameters_alloc").ok()?,
            avcodec_parameters_free: libs.avcodec.get(b"avcodec_parameters_free").ok()?,
            avcodec_parameters_to_context: libs.avcodec.get(b"avcodec_parameters_to_context").ok()?,
            sws_get_context: libs.swscale.get(b"sws_getContext").ok()?,
            sws_scale: libs.swscale.get(b"sws_scale").ok()?,
            sws_free_context: libs.swscale.get(b"sws_freeContext").ok()?,
        })))
    }
}

fn libs() -> Option<&'static Libs> {
    LIBS.get_or_init(load_libs).as_deref()
}

fn load_libs() -> Option<&'static Libs> {
    for dir in candidate_dirs() {
        if let Some(l) = try_load_in_dir(&dir) {
            return Some(Box::leak(Box::new(l)) as &'static Libs);
        }
    }
    try_load_system()
        .map(|l| Box::leak(Box::new(l)) as &'static Libs)
}

/// 搜索路径:资源目录 → 可执行文件旁 → CARGO_MANIFEST_DIR/resources → FFMPEG_HOME。
fn candidate_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(p) = exe.parent() {
            dirs.push(p.join("ffmpeg"));
            dirs.push(p.join("resources").join("ffmpeg"));
            if let Some(pp) = p.parent() {
                dirs.push(pp.join("resources").join("ffmpeg"));
            }
            dirs.push(p.to_path_buf());
        }
    }
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        dirs.push(std::path::PathBuf::from(manifest).join("resources").join("ffmpeg"));
    }
    if let Ok(home) = std::env::var("FFMPEG_HOME") {
        let home = std::path::PathBuf::from(home);
        dirs.push(home.join("bin"));
        dirs.push(home);
    }
    let mut seen = std::collections::HashSet::new();
    dirs.retain(|d| seen.insert(d.clone()));
    dirs
}

fn try_load_in_dir(dir: &std::path::Path) -> Option<Libs> {
    // 依赖顺序:先 avutil,再 swresample / swscale,最后 avcodec
    let avutil = first_library(dir, AVUTIL_NAMES)?;
    let swresample = first_library(dir, SWRESAMPLE_NAMES)?;
    let swscale = first_library(dir, SWSCALE_NAMES)?;
    let avcodec = first_library(dir, AVCODEC_NAMES)?;
    Some(Libs {
        avcodec: Box::leak(Box::new(avcodec)),
        avutil: Box::leak(Box::new(avutil)),
        swscale: Box::leak(Box::new(swscale)),
        _swresample: Box::leak(Box::new(swresample)),
    })
}

fn first_library(dir: &std::path::Path, names: &[&str]) -> Option<Library> {
    for n in names {
        let p = dir.join(format!("{n}.dll"));
        if p.exists() {
            if let Ok(l) = unsafe { Library::new(&p) } {
                return Some(l);
            }
        }
    }
    None
}

/// 系统 PATH 兜底(按文件名解析,依赖 DLL 搜索顺序)。
fn try_load_system() -> Option<Libs> {
    for u in AVUTIL_NAMES {
        let Ok(avutil) = (unsafe { Library::new(u) }) else {
            continue;
        };
        let Ok(swresample) = (unsafe { Library::new("swresample") }) else {
            continue;
        };
        for s in SWSCALE_NAMES {
            let Ok(swscale) = (unsafe { Library::new(s) }) else {
                continue;
            };
            for c in AVCODEC_NAMES {
                if let Ok(avcodec) = unsafe { Library::new(c) } {
                    return Some(Libs {
                        avcodec: Box::leak(Box::new(avcodec)),
                        avutil: Box::leak(Box::new(avutil)),
                        swscale: Box::leak(Box::new(swscale)),
                        _swresample: Box::leak(Box::new(swresample)),
                    });
                }
            }
        }
    }
    None
}

/// FFmpeg DLL 是否可用。
pub fn available() -> bool {
    fns().is_some()
}

// ---------------------------------------------------------------------------
// 硬件探测(hwinfo)
// ---------------------------------------------------------------------------

/// GPU 信息(DXGI 适配器描述)。
#[derive(Debug, Clone)]
pub struct GpuInfo {
    pub vendor_id: u32,
    pub vendor: String,
    pub name: String,
    pub vram_mb: u64,
}

/// 枚举本机 GPU(真实 DXGI 适配器,含基本显示驱动)。
pub fn detect_gpus() -> Vec<GpuInfo> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};

    let mut out = Vec::new();
    let Ok(factory) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else {
        return out;
    };
    let mut i: u32 = 0;
    loop {
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(i) }) else {
            break;
        };
        if let Ok(desc) = unsafe { adapter.GetDesc1() } {
            let end = desc.Description.iter().position(|&c| c == 0).unwrap_or(desc.Description.len());
            let name = String::from_utf16_lossy(&desc.Description[..end]);
            out.push(GpuInfo {
                vendor_id: desc.VendorId,
                vendor: vendor_name(desc.VendorId).to_string(),
                name,
                vram_mb: (desc.DedicatedVideoMemory / (1024 * 1024)) as u64,
            });
        }
        i += 1;
    }
    out
}

/// 厂商 ID → 名称。
pub fn vendor_name(vendor_id: u32) -> &'static str {
    match vendor_id {
        0x10DE => "NVIDIA",
        0x1002 | 0x1022 => "AMD",
        0x8086 => "Intel",
        0x1414 => "Microsoft(Basic)",
        _ => "Unknown",
    }
}

/// 探测指定名称的 H.264 编码器是否可用。
pub fn encoder_available(name: &str) -> bool {
    let Some(f) = fns() else { return false };
    let Ok(name_c) = std::ffi::CString::new(name) else {
        return false;
    };
    unsafe { (f.avcodec_find_encoder_by_name)(name_c.as_ptr()) }.is_null() == false
}

/// 所有候选编码器及其可用性。
pub fn detect_encoders() -> Vec<(&'static str, bool)> {
    [
        "h264_nvenc",
        "h264_qsv",
        "h264_amf",
        "libopenh264",
        "h264",
    ]
    .iter()
    .map(|n| (*n, encoder_available(n)))
    .collect()
}

/// 按 GPU 厂商优先选择编码器(nvenc/qsv/amf → 软件回退)。
pub fn preferred_encoder() -> Option<String> {
    let gpus = detect_gpus();
    let has_nvidia = gpus.iter().any(|g| g.vendor_id == 0x10DE);
    let has_intel = gpus.iter().any(|g| g.vendor_id == 0x8086);
    let has_amd = gpus.iter().any(|g| matches!(g.vendor_id, 0x1002 | 0x1022));

    let mut order: Vec<&'static str> = Vec::new();
    if has_nvidia {
        order.push("h264_nvenc");
    }
    if has_intel {
        order.push("h264_qsv");
    }
    if has_amd {
        order.push("h264_amf");
    }
    if order.is_empty() {
        // 无厂商信息时按通用顺序探测
        order.extend(["h264_nvenc", "h264_qsv", "h264_amf"]);
    }
    order.push("libopenh264");
    order.push("h264");

    for name in order {
        if encoder_available(name) {
            return Some(name.to_string());
        }
    }
    None
}

/// 汇总报告(供日志 / 操作日志展示)。
pub fn capability_report() -> String {
    let mut lines = Vec::new();
    lines.push(format!("ffmpeg_dll={}", if available() { "loaded" } else { "missing" }));
    let gpus = detect_gpus();
    if gpus.is_empty() {
        lines.push("gpu=无".to_string());
    } else {
        for g in &gpus {
            lines.push(format!("gpu={} ({}, {}MB)", g.name, g.vendor, g.vram_mb));
        }
    }
    let encs: Vec<String> = detect_encoders()
        .iter()
        .filter(|(_, ok)| *ok)
        .map(|(n, _)| n.to_string())
        .collect();
    lines.push(format!("encoders=[{}]", encs.join(", ")));
    lines.push(format!("preferred={:?}", preferred_encoder()));
    lines.join("; ")
}

// ---------------------------------------------------------------------------
// H.264 编码器
// ---------------------------------------------------------------------------

/// FFmpeg H.264 编码器(RGB24 输入,Annex-B 输出)。单线程使用。
pub struct HwEncoder {
    f: &'static Fns,
    ctx: *mut c_void,
    sws: *mut c_void,
    yuv: *mut AvFrame,
    rgb_in: *mut AvFrame,
    pkt: *mut AvPacket,
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
    frame_count: i64,
    pending_idr: bool,
}

// 原始指针仅在该实例所在任务线程内访问,跨 await 移动需 Send。
unsafe impl Send for HwEncoder {}

impl HwEncoder {
    /// 打开 H.264 编码器。`codec_name` 来自 `preferred_encoder()` 探测结果。
    ///
    /// 输入为 RGB24,内部经 swscale 等比缩放到 (dst_w, dst_h)(不放大)。
    pub fn open(
        codec_name: &str,
        src_w: u32,
        src_h: u32,
        dst_w: u32,
        dst_h: u32,
        fps: u32,
    ) -> Result<Self, String> {
        let f = fns().ok_or("FFmpeg DLL 未加载")?;
        let src_w = src_w.max(2);
        let src_h = src_h.max(2);
        let (dst_w, dst_h) = crate::capture::scale_dimensions(src_w, src_h, dst_w, dst_h);
        let fps = fps.clamp(1, 144);
        let bitrate = bitrate_for(dst_w, dst_h, fps);

        unsafe {
            let codec_c = std::ffi::CString::new(codec_name).map_err(|_| "编码器名含 NUL".to_string())?;
            let codec = (f.avcodec_find_encoder_by_name)(codec_c.as_ptr());
            if codec.is_null() {
                return Err(format!("编码器不可用: {codec_name}"));
            }
            let mut ctx = (f.avcodec_alloc_context3)(codec);
            if ctx.is_null() {
                return Err("avcodec_alloc_context3 失败".to_string());
            }

            // 宽高/像素格式/码率:经 AVCodecParameters(ABI 稳定,FFmpeg 8+ 已移除对应 AVOption)
            let mut par = (f.avcodec_parameters_alloc)();
            if par.is_null() {
                return Err("avcodec_parameters_alloc 失败".to_string());
            }
            (*par).codec_type = 0; // AVMEDIA_TYPE_VIDEO
            (*par).codec_id = AV_CODEC_ID_H264;
            (*par).format = AV_PIX_FMT_YUV420P;
            (*par).width = dst_w as c_int;
            (*par).height = dst_h as c_int;
            (*par).bit_rate = bitrate as i64;
            (*par).sample_aspect_ratio = AvRational { num: 1, den: 1 };
            (*par).framerate = AvRational { num: fps as c_int, den: 1 };
            let rc = (f.avcodec_parameters_to_context)(ctx, par);
            (f.avcodec_parameters_free)(&mut par);
            if rc < 0 {
                return Err(av_err(&f, rc, "avcodec_parameters_to_context 失败"));
            }

            // 其余核心参数经 av_opt(g=GOP 秒数*2、bf=0 无 B 帧、LOW_DELAY、单线程、时基)
            let opt = |name: &str| std::ffi::CString::new(name).unwrap();
            (f.av_opt_set_int)(ctx, opt("g").as_ptr(), (fps * 2) as i64, 0);
            (f.av_opt_set_int)(ctx, opt("bf").as_ptr(), 0, 0);
            (f.av_opt_set_int)(ctx, opt("threads").as_ptr(), 1, 0);
            (f.av_opt_set_int)(ctx, opt("flags").as_ptr(), AV_CODEC_FLAG_LOW_DELAY, 0);
            // 封顶 VBR:maxrate/bufsize 限制码率过冲(核心 option,FFmpeg 5~9 均可用)
            let maxrate = (bitrate as f64 * 1.5) as i64;
            (f.av_opt_set_int)(ctx, opt("maxrate").as_ptr(), maxrate, 0);
            (f.av_opt_set_int)(ctx, opt("bufsize").as_ptr(), maxrate, 0);
            (f.av_opt_set_q)(
                ctx,
                opt("time_base").as_ptr(),
                AvRational { num: 1, den: fps as c_int },
                0,
            );

            // 厂商私有参数(尽力而为,失败不影响)
            apply_private_opts(&f, ctx, codec_name);

            let rc = (f.avcodec_open2)(ctx, codec, std::ptr::null_mut());
            if rc < 0 {
                let msg = av_err(&f, rc, "avcodec_open2 失败");
                (f.avcodec_free_context)(&mut ctx);
                return Err(msg);
            }

            // swscale:RGB24 → YUV420P(含缩放)
            let sws = (f.sws_get_context)(
                src_w as c_int,
                src_h as c_int,
                AV_PIX_FMT_RGB24,
                dst_w as c_int,
                dst_h as c_int,
                AV_PIX_FMT_YUV420P,
                SWS_BILINEAR,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            if sws.is_null() {
                (f.avcodec_free_context)(&mut ctx);
                return Err("sws_getContext 失败".to_string());
            }

            // YUV 输出帧(缓冲区由 av_frame_get_buffer 分配)
            let mut yuv = (f.av_frame_alloc)();
            if yuv.is_null() {
                (f.sws_free_context)(sws);
                (f.avcodec_free_context)(&mut ctx);
                return Err("av_frame_alloc 失败".to_string());
            }
            (*yuv).format = AV_PIX_FMT_YUV420P;
            (*yuv).width = dst_w as c_int;
            (*yuv).height = dst_h as c_int;
            if (f.av_frame_get_buffer)(yuv, 32) < 0 {
                (f.av_frame_free)(&mut yuv);
                (f.sws_free_context)(sws);
                (f.avcodec_free_context)(&mut ctx);
                return Err("av_frame_get_buffer 失败".to_string());
            }

            // RGB 输入帧(每次编码时指向调用方缓冲区)
            let mut rgb_in = (f.av_frame_alloc)();
            if rgb_in.is_null() {
                (f.av_frame_free)(&mut yuv);
                (f.sws_free_context)(sws);
                (f.avcodec_free_context)(&mut ctx);
                return Err("av_frame_alloc 失败".to_string());
            }
            (*rgb_in).format = AV_PIX_FMT_RGB24;
            (*rgb_in).width = src_w as c_int;
            (*rgb_in).height = src_h as c_int;

            let pkt = (f.av_packet_alloc)();
            if pkt.is_null() {
                (f.av_frame_free)(&mut yuv);
                (f.av_frame_free)(&mut rgb_in);
                (f.sws_free_context)(sws);
                (f.avcodec_free_context)(&mut ctx);
                return Err("av_packet_alloc 失败".to_string());
            }

            Ok(Self {
                f,
                ctx,
                sws,
                yuv,
                rgb_in,
                pkt,
                src_w,
                src_h,
                dst_w,
                dst_h,
                frame_count: 0,
                pending_idr: false,
            })
        }
    }

    /// 编码尺寸(等比缩放后的目标)。
    pub fn dims(&self) -> (u32, u32) {
        (self.dst_w, self.dst_h)
    }

    /// 请求下一帧输出为关键帧(IDR)。仅在编码器支持 forced_idr 时生效。
    pub fn request_keyframe(&mut self) {
        self.pending_idr = true;
    }

    /// 编码一帧 RGB24(长度须为 src_w*src_h*3),返回 (宽, 高, Annex-B 字节, 是否关键帧)。
    /// 编码器未输出包时返回 Ok(None)(通常不会发生,低延迟模式下每帧一包)。
    pub fn encode_rgb(&mut self, rgb: &[u8]) -> Result<Option<(u32, u32, Vec<u8>, bool)>, String> {
        if rgb.len() < (self.src_w as usize * self.src_h as usize * 3) {
            return Err(format!(
                "RGB 输入长度不足: {} < {}",
                rgb.len(),
                self.src_w * self.src_h * 3
            ));
        }
        let f = self.f;
        unsafe {
            // 绑定输入缓冲
            (*self.rgb_in).data[0] = rgb.as_ptr() as *mut u8;
            (*self.rgb_in).linesize[0] = (self.src_w * 3) as c_int;
            (*self.rgb_in).data[1] = std::ptr::null_mut();
            (*self.rgb_in).data[2] = std::ptr::null_mut();

            // 强制 IDR
            if self.pending_idr {
                let name = std::ffi::CString::new("forced_idr").unwrap();
                let _ = (f.av_opt_set_int)(self.ctx, name.as_ptr(), 1, AV_OPT_SEARCH_CHILDREN);
            }

            // RGB → YUV420P(swscale 内部缩放)
            let src: [*const u8; 4] = [rgb.as_ptr(), std::ptr::null(), std::ptr::null(), std::ptr::null()];
            let src_stride: [c_int; 4] = [(self.src_w * 3) as c_int, 0, 0, 0];
            let dst = (*self.yuv).data.as_ptr();
            let dst_stride = (*self.yuv).linesize.as_ptr();
            let rc = (f.sws_scale)(
                self.sws,
                src.as_ptr(),
                src_stride.as_ptr(),
                0,
                self.src_h as c_int,
                dst,
                dst_stride,
            );
            if rc != self.dst_h as c_int {
                return Err(format!("sws_scale 失败: {rc}"));
            }

            // 发送
            (*self.yuv).pts = self.frame_count;
            self.frame_count += 1;
            let rc = (f.avcodec_send_frame)(self.ctx, self.yuv);
            if self.pending_idr {
                let name = std::ffi::CString::new("forced_idr").unwrap();
                let _ = (f.av_opt_set_int)(self.ctx, name.as_ptr(), 0, AV_OPT_SEARCH_CHILDREN);
                self.pending_idr = false;
            }
            if rc < 0 && rc != AVERROR_EAGAIN {
                return Err(av_err(&f, rc, "avcodec_send_frame 失败"));
            }

            // 收集输出包
            let mut out: Vec<u8> = Vec::new();
            let mut key = false;
            loop {
                let rc = (f.avcodec_receive_packet)(self.ctx, self.pkt);
                if rc == 0 {
                    let size = (*self.pkt).size as usize;
                    if size > 0 {
                        let data = std::slice::from_raw_parts((*self.pkt).data, size);
                        out.extend_from_slice(data);
                        if (*self.pkt).flags & AV_PKT_FLAG_KEY != 0 {
                            key = true;
                        }
                    }
                    (f.av_packet_unref)(self.pkt);
                } else if rc == AVERROR_EAGAIN || rc == AVERROR_EOF {
                    break;
                } else {
                    (f.av_packet_unref)(self.pkt);
                    return Err(av_err(&f, rc, "avcodec_receive_packet 失败"));
                }
            }
            if out.is_empty() {
                Ok(None)
            } else {
                Ok(Some((self.dst_w, self.dst_h, out, key)))
            }
        }
    }
}

impl Drop for HwEncoder {
    fn drop(&mut self) {
        let f = self.f;
        unsafe {
            (f.av_packet_free)(&mut self.pkt);
            (f.av_frame_free)(&mut self.yuv);
            (f.av_frame_free)(&mut self.rgb_in);
            (f.sws_free_context)(self.sws);
            (f.avcodec_free_context)(&mut self.ctx);
        }
    }
}

/// 编码码率估算(像素·帧率启发式,clamp 500kbps ~ 8Mbps)。
fn bitrate_for(w: u32, h: u32, fps: u32) -> u64 {
    let bits = (w as u64 * h as u64 * fps as u64) / 20;
    bits.clamp(500_000, 8_000_000)
}

/// 应用厂商私有参数(尽力而为;失败静默忽略)。
fn apply_private_opts(f: &Fns, ctx: *mut c_void, codec_name: &str) {
    let set = |name: &str, val: &str| {
        let n = std::ffi::CString::new(name).unwrap();
        let v = std::ffi::CString::new(val).unwrap();
        unsafe { (f.av_opt_set)(ctx, n.as_ptr(), v.as_ptr(), AV_OPT_SEARCH_CHILDREN) }
    };
    match codec_name {
        "h264_nvenc" => {
            // 低延迟:zero-latency 预设 p4 / llhq 二选一
            let _ = set("preset", "p4");
            let _ = set("tune", "ull");
            let _ = set("zerolatency", "1");
            let _ = set("rc", "vbr");
        }
        "h264_qsv" => {
            let _ = set("preset", "veryfast");
            let _ = set("low_power", "0");
        }
        "h264_amf" => {
            let _ = set("usage", "lowlatency");
            let _ = set("quality", "speed");
            let _ = set("rate_control", "cbr");
        }
        "libopenh264" => {
            let _ = set("complexity", "low");
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// H.264 解码器
// ---------------------------------------------------------------------------

/// FFmpeg H.264 解码器(Annex-B 输入,RGB24 输出)。单线程使用。
pub struct HwDecoder {
    f: &'static Fns,
    ctx: *mut c_void,
    pkt: *mut AvPacket,
    frame: *mut AvFrame,
    sws: *mut c_void,
    sws_w: u32,
    sws_h: u32,
    sws_fmt: c_int,
}

unsafe impl Send for HwDecoder {}

impl HwDecoder {
    /// 打开 H.264 软件解码器(native h264,FFmpeg 全版本内置)。
    pub fn open() -> Result<Self, String> {
        let f = fns().ok_or("FFmpeg DLL 未加载")?;
        unsafe {
            let codec = (f.avcodec_find_decoder)(AV_CODEC_ID_H264);
            if codec.is_null() {
                return Err("未找到 h264 解码器".to_string());
            }
            let mut ctx = (f.avcodec_alloc_context3)(codec);
            if ctx.is_null() {
                return Err("avcodec_alloc_context3 失败".to_string());
            }
            if (f.avcodec_open2)(ctx, codec, std::ptr::null_mut()) < 0 {
                (f.avcodec_free_context)(&mut ctx);
                return Err("avcodec_open2 失败".to_string());
            }
            let mut pkt = (f.av_packet_alloc)();
            let mut frame = (f.av_frame_alloc)();
            if pkt.is_null() || frame.is_null() {
                if !pkt.is_null() {
                    (f.av_packet_free)(&mut pkt);
                }
                if !frame.is_null() {
                    (f.av_frame_free)(&mut frame);
                }
                (f.avcodec_free_context)(&mut ctx);
                return Err("av_packet/frame alloc 失败".to_string());
            }
            Ok(Self {
                f,
                ctx,
                pkt,
                frame,
                sws: std::ptr::null_mut(),
                sws_w: 0,
                sws_h: 0,
                sws_fmt: -1,
            })
        }
    }

    /// 解码一帧 H.264 Annex-B,返回 (宽, 高, RGB24)。数据不足/非关键帧无输出时返回 Ok(None)。
    pub fn decode(&mut self, data: &[u8]) -> Result<Option<(u32, u32, Vec<u8>)>, String> {
        if data.is_empty() {
            return Ok(None);
        }
        let f = self.f;
        unsafe {
            (*self.pkt).data = data.as_ptr() as *mut u8;
            (*self.pkt).size = data.len() as c_int;
            (*self.pkt).pts = 0;

            let rc = (f.avcodec_send_packet)(self.ctx, self.pkt);
            (f.av_packet_unref)(self.pkt);
            if rc < 0 && rc != AVERROR_EAGAIN {
                return Err(av_err(&f, rc, "avcodec_send_packet 失败"));
            }

            let mut out: Option<(u32, u32, Vec<u8>)> = None;
            loop {
                let rc = (f.avcodec_receive_frame)(self.ctx, self.frame);
                if rc == 0 {
                    let w = (*self.frame).width as u32;
                    let h = (*self.frame).height as u32;
                    if w == 0 || h == 0 {
                        (f.av_frame_unref)(self.frame);
                        continue;
                    }
                    if self.sws.is_null() || self.sws_w != w || self.sws_h != h || self.sws_fmt != (*self.frame).format {
                        if !self.sws.is_null() {
                            (f.sws_free_context)(self.sws);
                        }
                        self.sws = (f.sws_get_context)(
                            w as c_int,
                            h as c_int,
                            (*self.frame).format,
                            w as c_int,
                            h as c_int,
                            AV_PIX_FMT_RGB24,
                            SWS_BILINEAR,
                            std::ptr::null_mut(),
                            std::ptr::null_mut(),
                            std::ptr::null(),
                        );
                        self.sws_w = w;
                        self.sws_h = h;
                        self.sws_fmt = (*self.frame).format;
                        if self.sws.is_null() {
                            (f.av_frame_unref)(self.frame);
                            return Err("sws_getContext 失败".to_string());
                        }
                    }
                    let mut rgb = vec![0u8; (w * h * 3) as usize];
                    let src = (*self.frame).data.as_ptr() as *const *const u8;
                    let src_stride = (*self.frame).linesize.as_ptr();
                    let dst: [*mut u8; 1] = [rgb.as_mut_ptr()];
                    let dst_stride: [c_int; 1] = [(w * 3) as c_int];
                    (f.sws_scale)(
                        self.sws,
                        src,
                        src_stride,
                        0,
                        h as c_int,
                        dst.as_ptr(),
                        dst_stride.as_ptr(),
                    );
                    out = Some((w, h, rgb));
                    (f.av_frame_unref)(self.frame);
                    break;
                } else if rc == AVERROR_EAGAIN || rc == AVERROR_EOF {
                    break;
                } else {
                    let e = av_err(&f, rc, "avcodec_receive_frame 失败");
                    (f.av_frame_unref)(self.frame);
                    return Err(e);
                }
            }
            Ok(out)
        }
    }
}

impl Drop for HwDecoder {
    fn drop(&mut self) {
        let f = self.f;
        unsafe {
            if !self.sws.is_null() {
                (f.sws_free_context)(self.sws);
            }
            (f.av_frame_free)(&mut self.frame);
            (f.av_packet_free)(&mut self.pkt);
            (f.avcodec_free_context)(&mut self.ctx);
        }
    }
}

// ---------------------------------------------------------------------------
// 工具
// ---------------------------------------------------------------------------

/// 格式化为 FFmpeg 错误文本(带前缀)。
fn av_err(f: &Fns, rc: c_int, prefix: &str) -> String {
    let mut buf = [0 as c_char; 128];
    unsafe {
        (f.av_strerror)(rc, buf.as_mut_ptr(), buf.len());
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let bytes: Vec<u8> = buf[..end].iter().map(|&c| c as u8).collect();
    let msg = String::from_utf8_lossy(&bytes);
    format!("{prefix}(rc={rc}): {msg}")
}

/// 判断数据是否以 H.264 Annex-B 起始码开头(00 00 01 或 00 00 00 01)。纯函数(测试工具)。
#[allow(dead_code)]
pub fn has_annexb_prefix(data: &[u8]) -> bool {
    data.len() >= 3
        && data[0] == 0
        && data[1] == 0
        && (data[2] == 1 || (data.len() >= 4 && data[2] == 0 && data[3] == 1))
}

/// 统计 Annex-B NAL 单元数量(扫描 00 00 01 / 00 00 00 01 起始码)。纯函数(测试工具)。
#[allow(dead_code)]
pub fn count_nalus(data: &[u8]) -> usize {
    let mut n = 0usize;
    let mut i = 0usize;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            n += 1;
            i += 3;
            continue;
        }
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 0 && i + 3 < data.len() && data[i + 3] == 1 {
            n += 1;
            i += 4;
            continue;
        }
        i += 1;
    }
    n
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annexb_prefix_detection() {
        assert!(has_annexb_prefix(&[0, 0, 0, 1, 0x67]));
        assert!(has_annexb_prefix(&[0, 0, 1, 0x68]));
        assert!(!has_annexb_prefix(&[0, 0, 0]));
        assert!(!has_annexb_prefix(&[1, 0, 0, 1]));
        assert!(!has_annexb_prefix(&[]));
    }

    #[test]
    fn nalu_counting() {
        // 3 字节 + 4 字节起始码混排
        let data = [0, 0, 0, 1, 0x67, 0, 0, 1, 0x68, 0, 0, 0, 1, 0x65, 0, 0, 0, 0];
        assert_eq!(count_nalus(&data), 3);
        assert_eq!(count_nalus(&[]), 0);
        // 内部 00 00 00 00 不应误判(最后一个 0 非 1)
        assert_eq!(count_nalus(&[0, 0, 0, 0, 0, 1]), 1);
    }

    /// 本机 FFmpeg 能力探测(需 DLL,默认忽略)。
    #[test]
    #[ignore]
    fn ffmpeg_capability_report() {
        println!("[ffmpeg] {}", crate::ffmpeg_hw::capability_report());
        let encs = detect_encoders();
        for (n, ok) in &encs {
            println!("[ffmpeg] 编码器 {n}: {}", if *ok { "可用" } else { "不可用" });
        }
        println!("[ffmpeg] 首选: {:?}", preferred_encoder());
        assert!(available());
    }

    /// 探针:检查 AVCodecContext 上可用的核心 option(诊断用,默认忽略)。
    #[test]
    #[ignore]
    fn ffmpeg_probe_options() {
        let f = fns().unwrap();
        unsafe {
            let c = std::ffi::CString::new("h264_nvenc").unwrap();
            let codec = (f.avcodec_find_encoder_by_name)(c.as_ptr());
            let mut ctx = (f.avcodec_alloc_context3)(codec);
            assert!(!ctx.is_null());
            for name in [
                "width", "height", "pix_fmt", "gop_size", "max_b_frames", "threads", "flags",
                "bit_rate", "time_base", "frame_rate", "preset",
                "bf", "gop", "g", "forced_idr", "zerolatency", "tune", "rc", "rc-lookahead",
                "maxrate", "bufsize", "cq", "cq-level", "qp",
            ] {
                let n = std::ffi::CString::new(name).unwrap();
                let rc = (f.av_opt_set_int)(ctx, n.as_ptr(), 1, 0);
                println!("[probe] int {name}: rc={rc}");
                let rc2 = (f.av_opt_set_int)(ctx, n.as_ptr(), 1, AV_OPT_SEARCH_CHILDREN);
                println!("[probe] int-children {name}: rc={rc2}");
            }
            let tb = std::ffi::CString::new("time_base").unwrap();
            let rc = (f.av_opt_set_q)(
                ctx,
                tb.as_ptr(),
                AvRational { num: 1, den: 30 },
                0,
            );
            println!("[probe] q time_base: rc={rc}");
            let p = std::ffi::CString::new("preset").unwrap();
            let v = std::ffi::CString::new("p4").unwrap();
            let rc = (f.av_opt_set)(ctx, p.as_ptr(), v.as_ptr(), AV_OPT_SEARCH_CHILDREN);
            println!("[probe] str preset: rc={rc}");
            // 参数结构隔离测试
            let mut par = (f.avcodec_parameters_alloc)();
            println!("[probe] par 分配: {}", !par.is_null());
            (*par).codec_type = 0;
            (*par).codec_id = 27;
            (*par).format = 0;
            (*par).width = 320;
            (*par).height = 180;
            (*par).bit_rate = 3_000_000;
            let rc = (f.avcodec_parameters_to_context)(ctx, par);
            println!("[probe] parameters_to_context: rc={rc}");
            (f.avcodec_parameters_free)(&mut par);
            // 读取是否生效(经 av_opt_get_int 校验 width/height 偏移)
            (f.avcodec_free_context)(&mut ctx);
        }
    }

    /// 1080p H.264 硬件编码吞吐基准:连续编码 60 帧,报告帧率与单帧耗时(需 DLL,默认忽略)。
    #[test]
    #[ignore]
    fn ffmpeg_h264_benchmark() {
        let codec = preferred_encoder().expect("无可用 H.264 编码器");
        let (w, h) = (1920u32, 1080u32);
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for i in 0..(w * h) {
            let x = i % w;
            let y = i / w;
            rgb.extend_from_slice(&[
                (x % 256) as u8,
                (y % 256) as u8,
                ((x ^ y) % 256) as u8,
            ]);
        }
        let mut enc = HwEncoder::open(&codec, w, h, 1920, 1080, 60).expect("打开编码器失败");
        enc.request_keyframe();

        let frames = 60u32;
        let start = std::time::Instant::now();
        let mut total_bytes = 0usize;
        let mut key_count = 0u32;
        for _ in 0..frames {
            if let Some((_, _, data, key)) = enc.encode_rgb(&rgb).expect("编码失败") {
                total_bytes += data.len();
                if key {
                    key_count += 1;
                }
            }
        }
        let elapsed = start.elapsed().as_secs_f64();
        let fps = frames as f64 / elapsed;
        let avg_ms = elapsed * 1000.0 / frames as f64;
        println!(
            "[bench-h264] {codec}: {frames} 帧 @ {w}x{h} 用时 {elapsed:.2}s → {fps:.1} fps,单帧 {avg_ms:.2} ms,输出 {total_bytes} B(≈{:.1} kbps),关键帧 {key_count} 个",
            total_bytes as f64 * 8.0 * fps / 1000.0
        );
        assert!(fps > 10.0, "硬件编码帧率异常: {fps:.1}");
    }

    /// 真实编解码往返:合成 RGB → 硬件/软件 H.264 编码 → 解码 → 对比尺寸与内容(需 DLL,默认忽略)。
    #[test]
    #[ignore]
    fn ffmpeg_h264_roundtrip() {
        let codec = preferred_encoder().expect("无可用 H.264 编码器");
        let (w, h) = (320u32, 180u32);
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                // 渐变 + 色块,便于内容校验
                let (r, g, b) = (
                    ((x * 255 / w) as u8),
                    ((y * 255 / h) as u8),
                    (x.wrapping_mul(31) ^ y.wrapping_mul(17)) as u8,
                );
                rgb.extend_from_slice(&[r, g, b]);
            }
        }

        let mut enc = HwEncoder::open(&codec, w, h, w, h, 30).expect("打开编码器失败");
        enc.request_keyframe();
        let mut all: Vec<u8> = Vec::new();
        let mut got_key = false;
        // 编 5 帧,保证拿到 IDR + 至少一帧内容
        for _ in 0..5 {
            match enc.encode_rgb(&rgb).expect("编码失败") {
                Some((_, _, data, key)) => {
                    all.extend_from_slice(&data);
                    got_key |= key;
                }
                None => {}
            }
        }
        assert!(!all.is_empty(), "编码无输出");
        assert!(has_annexb_prefix(&all), "输出应含 Annex-B 起始码");
        assert!(count_nalus(&all) >= 2, "应包含 SPS/PPS + 帧数据 NAL");
        assert!(got_key, "应输出关键帧");

        let mut dec = HwDecoder::open().expect("打开解码器失败");
        let out = dec.decode(&all).expect("解码失败").expect("解码无输出");
        let (dw, dh, rgb_out) = out;
        assert_eq!((dw, dh), (w, h), "解码尺寸应一致");

        // 内容校验:整帧平均色差应远小于 128(编码/解码损失远小于信号)
        let n = (dw * dh * 3) as usize;
        let mut sum = 0u64;
        for i in (0..n).step_by(97) {
            let d = (rgb[i] as i32 - rgb_out[i] as i32).abs() as u64;
            sum += d;
        }
        let samples = ((n + 96) / 97) as u64;
        let avg = sum / samples.max(1);
        assert!(avg < 48, "解码内容偏差过大: {avg}");
    }
}