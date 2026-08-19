//! 音视频全链路模块:采集 → 编码 → 传输 → 解码 → 存为本地文件。
//!
//! - 视频链路:真实 DXGI 一次性抓屏(`grab_frame_once`)→ jpeg-encoder 编码为
//!   JPEG → 经 `network.rs` 的真实 TCP framing 回环传输(`loopback_transport_video`)
//!   → `image` crate 解码校验 → 存 `frame_%05d.jpg`。
//! - 音频链路:cpal 真实采集(`capture_audio_seconds`,统一转 i16)→ `hound` 编码
//!   16-bit PCM WAV → 回环传输(`loopback_transport_audio`)→ 解码校验 → 存
//!   `audio_out.wav`。
//!
//! 非 Windows 平台:顶层采集函数(`grab_frame_once` / `capture_audio_seconds`)为
//! 编译占位,直接返回 Err("仅 Windows 支持");其余函数(编码/传输/解码/保存/测试)
//! 跨平台可编译可运行。自动化测试全部使用合成数据,不依赖真实硬件。

use base64::Engine as _;
use crate::network::{read_msg, write_msg, Msg};
use serde::Serialize;
use std::path::Path;

/// 音视频管线的运行报告(字段 camelCase 序列化,供前端展示)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineReport {
    pub kind: String,
    pub frames: u32,
    pub frame_width: u32,
    pub frame_height: u32,
    pub audio_samples: u32,
    pub audio_rate: u16,
    pub audio_channels: u16,
    pub out_dir: String,
    pub elapsed_ms: u64,
}

/// 将 RGB 像素(每像素 3 字节)编码为 JPEG(jpeg-encoder,质量 70)。
///
/// capture.rs 内的 `rgb_to_jpeg` 是私有且带缩放的实现(签名不同),此处按任务
/// 要求在本文件内写一个纯编码的辅助函数。
fn encode_jpeg(rgb: &[u8], w: u32, h: u32) -> Result<Vec<u8>, String> {
    let w = w.max(1);
    let h = h.max(1);
    if w > u16::MAX as u32 || h > u16::MAX as u32 {
        return Err(format!("JPEG 尺寸超出编码上限: {w}x{h}"));
    }
    if rgb.len() != (w as usize) * (h as usize) * 3 {
        return Err(format!(
            "RGB 数据长度不符: 期望 {} 字节,实际 {}",
            (w as usize) * (h as usize) * 3,
            rgb.len()
        ));
    }
    let mut jpeg_buf: Vec<u8> = Vec::new();
    {
        use jpeg_encoder::{ColorType, Encoder};
        let encoder = Encoder::new(&mut jpeg_buf, 70);
        encoder
            .encode(rgb, w as u16, h as u16, ColorType::Rgb)
            .map_err(|e| format!("JPEG 编码失败: {e}"))?;
    }
    Ok(jpeg_buf)
}

/// 视频回环传输:把编码帧经真实 TCP framing(loopback)发到本机服务端再收回来。
///
/// 客户端依次 `write_msg(Msg::Frame)` 发送,发完关闭连接;服务端 `read_msg`
/// 收集收到的帧(读到 EOF 后返回收集结果),用于验证 编码 → 传输 → 解码 全链路。
async fn loopback_transport_video(
    frames: &[(u32, u32, Vec<u8>)],
) -> Result<Vec<(u32, u32, Vec<u8>)>, String> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("绑定回环端口失败: {e}"))?;
    let addr = listener
        .local_addr()
        .map_err(|e| format!("读取回环端口失败: {e}"))?;

    let server = tokio::spawn(async move {
        let (mut stream, _peer) = listener
            .accept()
            .await
            .map_err(|e| format!("接受回环连接失败: {e}"))?;
        let mut collected: Vec<(u32, u32, Vec<u8>)> = Vec::new();
        loop {
            match read_msg(&mut stream).await {
                Ok(Msg::Frame { w, h, seq: _, jpeg }) => {
                    let data = base64::engine::general_purpose::STANDARD
                        .decode(&jpeg)
                        .map_err(|e| format!("视频帧 base64 解码失败: {e}"))?;
                    collected.push((w, h, data));
                }
                // 连接关闭(EOF)即收集完毕
                Err(_) => return Ok(collected),
                _ => {}
            }
        }
    });

    let mut stream = tokio::net::TcpStream::connect(addr)
        .await
        .map_err(|e| format!("连接回环端口失败: {e}"))?;
    for (i, (w, h, jpeg)) in frames.iter().enumerate() {
        write_msg(
            &mut stream,
            &Msg::Frame {
                w: *w,
                h: *h,
                seq: i as u64,
                jpeg: base64::engine::general_purpose::STANDARD.encode(jpeg),
            },
        )
        .await
        .map_err(|e| format!("发送视频帧失败: {e}"))?;
    }
    // 关闭连接,服务端 read_msg 读到 EOF 后返回收集结果
    drop(stream);
    server
        .await
        .map_err(|e| format!("视频传输服务端任务失败: {e}"))?
}

/// 音频回环传输:把 WAV 字节经真实 TCP framing 发到本机服务端再收回来。
///
/// 客户端 `write_msg(Msg::Audio)`,发完关闭连接;服务端收集收到的音频 wav 字节。
async fn loopback_transport_audio(wav: &[u8]) -> Result<Vec<u8>, String> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("绑定回环端口失败: {e}"))?;
    let addr = listener
        .local_addr()
        .map_err(|e| format!("读取回环端口失败: {e}"))?;

    let server = tokio::spawn(async move {
        let (mut stream, _peer) = listener
            .accept()
            .await
            .map_err(|e| format!("接受回环连接失败: {e}"))?;
        let mut collected: Vec<u8> = Vec::new();
        loop {
            match read_msg(&mut stream).await {
                Ok(Msg::Audio {
                    sample_rate,
                    channels,
                    seq: _,
                    wav,
                }) => {
                    let data = base64::engine::general_purpose::STANDARD
                        .decode(&wav)
                        .map_err(|e| format!("音频 base64 解码失败: {e}"))?;
                    collected.extend_from_slice(&data);
                    log::info!(
                        "[media_pipeline] 收到音频帧 sample_rate={sample_rate} channels={channels} bytes={}",
                        data.len()
                    );
                }
                // 连接关闭(EOF)即收集完毕
                Err(_) => return Ok(collected),
                _ => {}
            }
        }
    });

    let mut stream = tokio::net::TcpStream::connect(addr)
        .await
        .map_err(|e| format!("连接回环端口失败: {e}"))?;
    write_msg(
        &mut stream,
        &Msg::Audio {
            // 回环自测不关心采样参数,协议字段置 0 即可(服务端仅收集 wav 字节)
            sample_rate: 0,
            channels: 0,
            seq: 0,
            wav: base64::engine::general_purpose::STANDARD.encode(wav),
        },
    )
    .await
    .map_err(|e| format!("发送音频帧失败: {e}"))?;
    // 关闭连接,服务端 read_msg 读到 EOF 后返回收集结果
    drop(stream);
    server
        .await
        .map_err(|e| format!("音频传输服务端任务失败: {e}"))?
}

/// 解码 JPEG 校验:返回解码后的实际宽高。
fn decode_jpeg_verify(jpeg: &[u8]) -> Result<(u32, u32), String> {
    let img = image::load_from_memory(jpeg).map_err(|e| format!("JPEG 解码失败: {e}"))?;
    Ok((img.width(), img.height()))
}

/// 解码 WAV 校验:返回总采样数(每声道帧数 × 声道数)。
fn decode_wav_verify(wav: &[u8]) -> Result<usize, String> {
    let reader =
        hound::WavReader::new(std::io::Cursor::new(wav)).map_err(|e| format!("WAV 解码失败: {e}"))?;
    let channels = reader.spec().channels;
    let frames = reader.duration();
    Ok(frames as usize * channels as usize)
}

/// 内存中的 WAV 写入目标。
///
/// hound 3.x 的 `WavWriter::new` 需要 `W: Write + Seek`,且 `finalize` 会消费写入器、
/// 无 `into_inner`;故用 `Rc<RefCell<Cursor<Vec<u8>>>>` 共享缓冲,写入器被消费后仍可从
/// 外层 `Rc` 取出完整字节。
#[derive(Clone)]
struct MemWavTarget(std::rc::Rc<std::cell::RefCell<std::io::Cursor<Vec<u8>>>>);

impl std::io::Write for MemWavTarget {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.borrow_mut().flush()
    }
}

impl std::io::Seek for MemWavTarget {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.0.borrow_mut().seek(pos)
    }
}

/// 将 16-bit PCM 采样编码为 WAV 字节(hound 写内存)。
fn pcm_to_wav(sample_rate: u16, channels: u16, samples: &[i16]) -> Vec<u8> {
    let inner = std::rc::Rc::new(std::cell::RefCell::new(std::io::Cursor::new(Vec::new())));
    let spec = hound::WavSpec {
        channels,
        sample_rate: u32::from(sample_rate),
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut wav = hound::WavWriter::new(MemWavTarget(inner.clone()), spec)
        .expect("内存 WAV 写入器创建失败");
    for s in samples {
        wav.write_sample(*s).expect("WAV 采样写入失败");
    }
    wav.finalize().expect("WAV finalize 失败");
    let out = inner.borrow().get_ref().clone();
    out
}

/// 把编码后的视频帧 / 音频 wav 写入磁盘目录。
///
/// - 视频:逐帧写 `frame_%05d.jpg`。
/// - 音频:写 `audio_out.wav`。
/// 返回写入的文件总数。
fn save_outputs(
    dir: &Path,
    video_jpegs: &[(u32, u32, Vec<u8>)],
    audio_wav: Option<&[u8]>,
) -> Result<usize, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("创建输出目录失败: {e}"))?;
    let mut count = 0usize;
    for (i, (_, _, jpeg)) in video_jpegs.iter().enumerate() {
        std::fs::write(dir.join(format!("frame_{:05}.jpg", i + 1)), jpeg)
            .map_err(|e| format!("写入帧 {i} 失败: {e}"))?;
        count += 1;
    }
    if let Some(wav) = audio_wav {
        std::fs::write(dir.join("audio_out.wav"), wav)
            .map_err(|e| format!("写入音频失败: {e}"))?;
        count += 1;
    }
    Ok(count)
}

/// 编排音视频全链路:按 kind 采集 → 编码 → 回环传输 → 解码校验 → 落盘。
///
/// - kind "video"|"audio"|"both";视频按 `seconds` 秒、每秒 10 帧抓取。
/// - 全程 `Instant` 计时;任一步失败返回 Err 并记录操作日志。
pub fn run_pipeline(kind: &str, seconds: u32, out_dir: &Path) -> Result<PipelineReport, String> {
    let started = std::time::Instant::now();
    crate::operation_log::op_log("media_pipeline", "run", kind);

    let result: Result<PipelineReport, String> = (|| {
        let want_video = kind == "video" || kind == "both";
        let want_audio = kind == "audio" || kind == "both";
        if !want_video && !want_audio {
            return Err(format!("未知的 pipeline kind: {kind}(期望 video/audio/both)"));
        }

        let mut frames: Vec<(u32, u32, Vec<u8>)> = Vec::new();
        let mut audio: Option<Vec<u8>> = None;
        let mut audio_rate: u16 = 0;
        let mut audio_channels: u16 = 0;
        let mut audio_samples: u32 = 0;

        // ---- 视频链路:采集 seconds 秒(每秒 10 帧)→ 编码 → 回环传输 → 解码校验 ----
        if want_video {
            let frame_count = seconds.clamp(1, 30) * 10;
            for i in 0..frame_count {
                if i > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                let (w, h, rgb) = grab_frame_once(0)?;
                let jpeg = encode_jpeg(&rgb, w, h)?;
                frames.push((w, h, jpeg));
            }
            let received = tokio::runtime::Runtime::new()
                .map_err(|e| format!("创建 Tokio 运行时失败: {e}"))?
                .block_on(loopback_transport_video(&frames))?;
            for (w, h, jpeg) in &received {
                let (dw, dh) = decode_jpeg_verify(jpeg)?;
                if dw != *w || dh != *h {
                    return Err(format!("JPEG 解码尺寸不符: 期望 {w}x{h},实际 {dw}x{dh}"));
                }
            }
            frames = received;
        }

        // ---- 音频链路:采集 → WAV 编码 → 回环传输 → 解码校验 ----
        if want_audio {
            let (rate, channels, samples) = capture_audio_seconds(seconds)?;
            let wav = pcm_to_wav(rate, channels, &samples);
            let received = tokio::runtime::Runtime::new()
                .map_err(|e| format!("创建 Tokio 运行时失败: {e}"))?
                .block_on(loopback_transport_audio(&wav))?;
            let n = decode_wav_verify(&received)?;
            if n == 0 {
                return Err("音频解码为空".to_string());
            }
            audio_rate = rate;
            audio_channels = channels;
            audio_samples = samples.len() as u32;
            audio = Some(received);
        }

        // ---- 落盘 ----
        let saved = save_outputs(out_dir, &frames, audio.as_deref())?;
        let (frame_width, frame_height) = frames
            .first()
            .map(|(w, h, _)| (*w, *h))
            .unwrap_or((0, 0));
        let elapsed_ms = started.elapsed().as_millis() as u64;
        log::info!(
            "[media_pipeline] 全链路完成: kind={kind} frames={} saved={saved} elapsed={elapsed_ms}ms",
            frames.len()
        );
        Ok(PipelineReport {
            kind: kind.to_string(),
            frames: frames.len() as u32,
            frame_width,
            frame_height,
            audio_samples,
            audio_rate,
            audio_channels,
            out_dir: out_dir.to_string_lossy().into_owned(),
            elapsed_ms,
        })
    })();

    if let Err(e) = &result {
        crate::operation_log::op_log("media_pipeline", "run", &format!("kind={kind} 失败: {e}"));
    }
    result
}

/// 一次性真实 DXGI 抓屏,返回 (宽, 高, RGB 字节)。
///
/// 流程:CreateDXGIFactory1 → EnumAdapters1(0) → EnumOutputs(monitor_id) →
/// D3D11CreateDevice → IDXGIOutput1::DuplicateOutput → AcquireNextFrame(0)
/// (DXGI_ERROR_WAIT_TIMEOUT 重试最多 3 次)→ 拷贝到 staging → Map 读 BGRA →
/// bgra_to_rgb → ReleaseFrame;COM 资源随作用域结束自动释放。
#[cfg(target_os = "windows")]
pub fn grab_frame_once(monitor_id: u32) -> Result<(u32, u32, Vec<u8>), String> {
    use windows::core::Interface;
    use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
    use windows::Win32::Graphics::Direct3D11::{
        D3D11CreateDevice, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_FLAG, D3D11_MAP_READ,
        D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
        ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
    };
    use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO, IDXGIFactory1,
        IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource,
    };

    // 1) DXGI 工厂 → 适配器(0) → 指定输出(显示器)
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }
        .map_err(|e| format!("CreateDXGIFactory1 失败: {e}"))?;
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
    let result: Result<(u32, u32, Vec<u8>), String> = (|| {
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
        staging_desc.SampleDesc = DXGI_SAMPLE_DESC { Count: 1, Quality: 0 };
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

        // 7) BGRA → RGB
        let rgb = crate::capture::bgra_to_rgb(&bgra, src_w, src_h);
        Ok((src_w, src_h, rgb))
    })();

    // 无论成败都释放桌面复制帧
    let _ = unsafe { dup.ReleaseFrame() };
    result
}

/// 非 Windows:编译占位,真实抓屏仅 Windows 支持。
#[cfg(not(target_os = "windows"))]
pub fn grab_frame_once(monitor_id: u32) -> Result<(u32, u32, Vec<u8>), String> {
    let _ = monitor_id;
    Err("仅 Windows 支持".to_string())
}

/// 按偏好选择输入设备的采集配置:优先 i16/f32、采样率接近 16000/44100、单/双声道;
/// 否则回退到默认配置。返回 (StreamConfig, SampleFormat)。
#[cfg(target_os = "windows")]
fn pick_input_config(
    device: &cpal::Device,
) -> Result<(cpal::StreamConfig, cpal::SampleFormat), String> {
    use cpal::traits::DeviceTrait;

    if let Ok(configs) = device.supported_input_configs() {
        let mut best: Option<(i64, cpal::SupportedStreamConfig)> = None;
        for range in configs {
            let score = config_score(
                range.sample_format(),
                range.min_sample_rate().0,
                range.channels(),
            );
            let cfg = if range.min_sample_rate().0 <= 16000 && range.max_sample_rate().0 >= 16000 {
                range.try_with_sample_rate(cpal::SampleRate(16000))
            } else if range.min_sample_rate().0 <= 44100 && range.max_sample_rate().0 >= 44100 {
                range.try_with_sample_rate(cpal::SampleRate(44100))
            } else {
                Some(range.with_max_sample_rate())
            };
            if let Some(cfg) = cfg {
                if best.as_ref().map_or(true, |(s, _)| score > *s) {
                    best = Some((score, cfg));
                }
            }
        }
        if let Some((_, cfg)) = best {
            let fmt = cfg.sample_format();
            let config: cpal::StreamConfig = cfg.into();
            return Ok((config, fmt));
        }
    }
    let default = device
        .default_input_config()
        .map_err(|e| format!("获取默认输入配置失败: {e}"))?;
    let fmt = default.sample_format();
    let config: cpal::StreamConfig = default.into();
    Ok((config, fmt))
}

/// 为支持的输入配置打分(分数越高越优先)。
#[cfg(target_os = "windows")]
fn config_score(fmt: cpal::SampleFormat, sample_rate: u32, channels: u16) -> i64 {
    let mut score = 0i64;
    score += match fmt {
        cpal::SampleFormat::I16 | cpal::SampleFormat::F32 => 4,
        cpal::SampleFormat::U16 => 2,
        _ => 0,
    };
    let diff = (i64::from(sample_rate) - 16000)
        .abs()
        .min((i64::from(sample_rate) - 44100).abs());
    score += match diff {
        d if d < 200 => 4,
        d if d < 2000 => 2,
        _ => 0,
    };
    score += match channels {
        1..=2 => 2,
        _ => 0,
    };
    score
}

/// 真实音频采集:收集指定秒数(上限 10 秒)的输入采样,统一转为 i16。
///
/// 返回 (sample_rate, channels, samples)。sleep 结束后 drop 流即真实停止采集。
#[cfg(target_os = "windows")]
fn capture_audio_seconds(seconds: u32) -> Result<(u16, u16, Vec<i16>), String> {
    use cpal::traits::HostTrait;
    use cpal::SampleFormat;

    let host = cpal::default_host();
    let device = host.default_input_device().ok_or("未找到音频输入设备")?;
    let (config, sample_format) = pick_input_config(&device)?;
    let seconds = seconds.clamp(1, 10);
    log::info!(
        "[media_pipeline] 音频采集开始: {}s, {:?}, {}ch",
        seconds,
        sample_format,
        config.channels
    );
    match sample_format {
        SampleFormat::F32 => collect_audio::<f32>(&device, config, seconds),
        SampleFormat::I16 => collect_audio::<i16>(&device, config, seconds),
        SampleFormat::U16 => collect_audio::<u16>(&device, config, seconds),
        SampleFormat::I32 => collect_audio::<i32>(&device, config, seconds),
        SampleFormat::U32 => collect_audio::<u32>(&device, config, seconds),
        SampleFormat::I64 => collect_audio::<i64>(&device, config, seconds),
        SampleFormat::U64 => collect_audio::<u64>(&device, config, seconds),
        SampleFormat::F64 => collect_audio::<f64>(&device, config, seconds),
        _ => Err(format!("不支持的采样格式: {sample_format:?}")),
    }
}

/// 非 Windows:编译占位,真实音频采集仅 Windows 支持。
#[cfg(not(target_os = "windows"))]
fn capture_audio_seconds(seconds: u32) -> Result<(u16, u16, Vec<i16>), String> {
    let _ = seconds;
    Err("仅 Windows 支持".to_string())
}

/// 泛型采集回调:构建输入流,把回调里的 `&[T]` 统一 `to_sample::<i16>()` 收集。
#[cfg(target_os = "windows")]
fn collect_audio<T: cpal::SizedSample + cpal::Sample>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    seconds: u32,
) -> Result<(u16, u16, Vec<i16>), String>
where
    i16: cpal::FromSample<T>,
{
    use cpal::traits::DeviceTrait;
    use std::sync::{Arc, Mutex};

    let samples: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
    let stream_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let buf = Arc::clone(&samples);
    let err_slot = Arc::clone(&stream_error);

    let stream = device
        .build_input_stream::<T, _, _>(
            &config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                if let Ok(mut dst) = buf.lock() {
                    for s in data {
                        dst.push(s.to_sample::<i16>());
                    }
                }
            },
            move |e| {
                if let Ok(mut slot) = err_slot.lock() {
                    *slot = Some(format!("{e}"));
                }
            },
            None,
        )
        .map_err(|e| format!("构建音频输入流失败: {e}"))?;

    // 采集指定秒数,sleep 后 drop 流即真实停止采集
    std::thread::sleep(std::time::Duration::from_secs(u64::from(seconds)));
    drop(stream);

    if let Ok(slot) = stream_error.lock() {
        if let Some(e) = slot.as_ref() {
            return Err(format!("音频流运行错误: {e}"));
        }
    }
    let got = samples
        .lock()
        .map_err(|e| format!("锁定采样缓冲失败: {e}"))?
        .clone();
    if got.is_empty() {
        return Err("未采集到任何音频采样".to_string());
    }
    Ok((config.sample_rate.0 as u16, config.channels, got))
}

/// 一键运行音视频全链路测试(采集 → 编码 → 回环传输 → 解码 → 落盘)。
#[tauri::command]
pub fn run_media_pipeline_test(
    kind: String,
    seconds: u32,
    out_dir: String,
) -> Result<PipelineReport, String> {
    run_pipeline(&kind, seconds, std::path::Path::new(&out_dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 合成一帧 RGB 渐变 + 移动白色方块(测试用,不依赖真实硬件)。
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

    /// 生成唯一的临时输出目录(避免与其他测试/真实运行冲突)。
    fn unique_temp_dir(name: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("media-pipeline-{name}-{stamp}"))
    }

    #[tokio::test]
    async fn video_chain_test() {
        let mut frames: Vec<(u32, u32, Vec<u8>)> = Vec::new();
        for t in 0..3 {
            let rgb = synth_frame(240, 135, t);
            let jpeg = encode_jpeg(&rgb, 240, 135).unwrap();
            frames.push((240, 135, jpeg));
        }

        // 回环传输:发送 3 帧,服务端应原样收到 3 帧
        let received = loopback_transport_video(&frames).await.unwrap();
        assert_eq!(received.len(), 3);

        // 解码校验:每帧均为 240x135 JPEG
        for (w, h, jpeg) in &received {
            assert_eq!((*w, *h), (240, 135));
            assert_eq!(decode_jpeg_verify(jpeg).unwrap(), (240, 135));
        }

        // 落盘校验:3 个 jpg 文件存在且非空
        let dir = unique_temp_dir("video");
        let saved = save_outputs(&dir, &received, None).unwrap();
        assert_eq!(saved, 3);
        for i in 1..=3 {
            let p = dir.join(format!("frame_{i:05}.jpg"));
            assert!(p.exists(), "缺少文件: {}", p.display());
            assert!(std::fs::metadata(&p).unwrap().len() > 0);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn audio_chain_test() {
        // 合成 2 秒 16000Hz 单声道 440Hz 正弦波(i16,共 32000 采样)
        let rate = 16000u16;
        let channels = 1u16;
        let mut samples = Vec::with_capacity(32000);
        for i in 0..32000i32 {
            let v =
                ((f64::from(i)) * 2.0 * std::f64::consts::PI * 440.0 / f64::from(rate)).sin();
            samples.push((v * 0.5 * f64::from(i16::MAX)) as i16);
        }

        // 编码 → 回环传输 → 解码校验(采样数 = 32000)
        let wav = pcm_to_wav(rate, channels, &samples);
        let received = loopback_transport_audio(&wav).await.unwrap();
        assert_eq!(decode_wav_verify(&received).unwrap(), 32000);

        // 落盘校验:audio_out.wav 存在且非空
        let dir = unique_temp_dir("audio");
        let saved = save_outputs(&dir, &[], Some(&received)).unwrap();
        assert_eq!(saved, 1);
        let p = dir.join("audio_out.wav");
        assert!(p.exists());
        assert!(std::fs::metadata(&p).unwrap().len() > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
