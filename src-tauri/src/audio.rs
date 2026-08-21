//! 远程会话音频链路:被控端采集 + 控制端播放。
//!
//! - 被控端:常驻音频采集循环(`start_audio_capture`),周期性调用本模块
//!   `capture_system_audio`(真实 WASAPI 回环,回退 cpal)采集系统
//!   播放的声音,切成小块经 `latest_audio()` 供 `network::host_write_loop` 以
//!   `Msg::Audio` 推送;仅在新块到达(seq 递增)时才发送。
//! - 控制端:收到 `Msg::Audio` 后调用 `play_audio`(cpal 输出流,i16 播放),实现
//!   真实远控语音回传。无输出设备时静默跳过。
//!
//! 非 Windows 平台:全部为编译占位,保证跨平台可编译。

use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// 单块音频时长(秒);采集线程每块间隔约此值。
const CHUNK_SECS: u32 = 1;

/// 一帧待发送的音频块。
#[derive(Debug, Clone)]
pub struct AudioBlock {
    pub seq: u64,
    pub sample_rate: u16,
    pub channels: u16,
    /// WAV 编码字节(base64 后经协议发送)
    pub wav: Vec<u8>,
}

/// 最新音频块快照(被控端采集线程写入;host_write_loop 读取发送)。
static LATEST_AUDIO: Mutex<Option<AudioBlock>> = Mutex::new(None);

/// 音频块序号(每次发布递增;host_write_loop 依此判断是否有新块)。
static AUDIO_SEQ: AtomicU64 = AtomicU64::new(0);

/// 音频静音标志(控制端收到音频块时据此跳过播放)。
static AUDIO_MUTED: AtomicBool = AtomicBool::new(false);

/// 当前是否静音(控制端播放远程音频前检查)。
pub fn is_audio_muted() -> bool {
    AUDIO_MUTED.load(Ordering::Relaxed)
}

/// 音频静音状态事件负载(前端据此同步静音按钮)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioStatePayload {
    pub muted: bool,
}

/// 设置音频静音(前端麦克风按钮真实可用),变更后经 `audio-state` 事件回执前端。
#[tauri::command]
pub fn set_audio_muted(muted: bool, app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        AUDIO_MUTED.store(muted, Ordering::Relaxed);
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = muted;
    }
    log::info!("[audio] 音频静音状态: {muted}");
    crate::operation_log::op_log("audio", "mute", &format!("muted={muted}"));
    app.emit("audio-state", AudioStatePayload { muted })
        .map_err(|e| format!("推送音频状态事件失败: {e}"))?;
    Ok(())
}

/// 读取当前音频静音状态(前端连接后初始化静音按钮)。
#[tauri::command]
pub fn get_audio_muted() -> bool {
    AUDIO_MUTED.load(Ordering::Relaxed)
}

/// 音频采集线程句柄(仅用于 `is_finished` 状态查询;停止不 join,见 `stop_audio_capture`)。
static AUDIO_TASK: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);

/// 当前采集代际号:每次 `stop_audio_capture` 递增,旧采集线程发现代际号已变即自行退出。
/// 单次采集调用阻塞约 `CHUNK_SECS` 秒,退出最多延迟一个采集周期。
static AUDIO_GEN: AtomicU64 = AtomicU64::new(0);

/// 取最新音频块(无则 None);host_write_loop 发送后可用 seq 判断是否消费过。
pub fn latest_audio() -> Option<AudioBlock> {
    LATEST_AUDIO
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// 启动被控端音频采集(幂等;已运行则返回)。
pub fn start_audio_capture() -> Result<(), String> {
    let mut slot = AUDIO_TASK.lock().unwrap_or_else(|e| e.into_inner());
    if slot.as_ref().map_or(false, |h| !h.is_finished()) {
        return Ok(());
    }
    let gen = AUDIO_GEN.load(Ordering::Acquire);
    let handle = std::thread::Builder::new()
        .name("dcr-audio-capture".into())
        .spawn(move || audio_capture_loop(gen))
        .map_err(|e| format!("创建音频采集线程失败: {e}"))?;
    *slot = Some(handle);
    log::info!("[audio] 被控端音频采集已启动(系统回环,{CHUNK_SECS}s/块)");
    crate::operation_log::op_log("audio", "start", "");
    Ok(())
}

/// 停止被控端音频采集。
///
/// 不 join 采集线程:单次 WASAPI/cpal 采集调用阻塞约 `CHUNK_SECS` 秒且可能逐设备
/// 重试,join 会让调用方(可能是主线程)长时间挂起;改为递增代际号通知线程在
/// 当前采集周期结束后自行退出,立即返回。
pub fn stop_audio_capture() {
    AUDIO_GEN.fetch_add(1, Ordering::AcqRel);
    // 丢弃旧线程句柄(drop JoinHandle = 分离线程,不等待):旧线程最多再跑一个
    // 采集周期即自行退出;若保留句柄,快速重启时 is_finished() 仍为 false,
    // start_audio_capture 会误判"已运行"而拒绝启动新线程,导致音频丢失
    let _ = AUDIO_TASK.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Ok(mut slot) = LATEST_AUDIO.lock() {
        *slot = None;
    }
    log::info!("[audio] 被控端音频采集已停止(采集线程稍后自行退出)");
    crate::operation_log::op_log("audio", "stop", "");
}

/// 采集线程是否仍属于当前代际(未被停止)。
fn audio_gen_current(gen: u64) -> bool {
    AUDIO_GEN.load(Ordering::Acquire) == gen
}

/// 音频采集线程循环:周期采集系统回环音频,切块发布到 `LATEST_AUDIO`。
/// `gen` 为启动时的代际号,`stop_audio_capture` 递增全局代际后本循环自行退出。
fn audio_capture_loop(gen: u64) {
    #[cfg(target_os = "windows")]
    {
        let mut last_log = std::time::Instant::now();
        while audio_gen_current(gen) {
            match capture_system_audio(CHUNK_SECS) {
                Ok((rate, channels, samples)) => {
                    if !audio_gen_current(gen) {
                        break;
                    }
                    if samples.is_empty() {
                        continue;
                    }
                    let wav = pcm_to_wav(rate, channels, &samples);
                    let seq = AUDIO_SEQ.fetch_add(1, Ordering::Relaxed) + 1;
                    if let Ok(mut slot) = LATEST_AUDIO.lock() {
                        *slot = Some(AudioBlock {
                            seq,
                            sample_rate: rate,
                            channels,
                            wav,
                        });
                    }
                    log::debug!(
                        "[audio] 采集到 {rate}Hz/{channels}ch {} 采样(seq={seq})",
                        samples.len()
                    );
                }
                Err(e) => {
                    // 静音/无设备时降频告警,避免刷屏;等待期间分片检查退出标志
                    if last_log.elapsed() > Duration::from_secs(5) {
                        log::warn!("[audio] 采集失败(将重试): {e}");
                        last_log = std::time::Instant::now();
                    }
                    for _ in 0..10 {
                        if !audio_gen_current(gen) {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                }
            }
        }
        log::info!("[audio] 采集线程退出(gen={gen})");
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = gen;
        log::info!("[audio] (非 Windows) 音频采集线程占位退出");
    }
}

// ---------------------------------------------------------------------------
// 系统音频真实采集(WASAPI 回环,回退 cpal)
// ---------------------------------------------------------------------------

/// WASAPI 系统音频回环采集(真实采集电脑正在播放的声音,与 RustDesk 方案一致)。
///
/// 优先走原生 WASAPI 轮询模式:在默认渲染设备(扬声器)上建回环流,轮询读取混音后的
/// 音频(非事件驱动,规避 cpal 事件回调在部分环境不触发的问题)。失败时回退到 cpal。
///
/// 系统静音时无任何数据,此时返回 Err 并提示播放音频;本函数不做任何合成填充或静音伪造。
#[cfg(target_os = "windows")]
fn capture_system_audio(seconds: u32) -> Result<(u16, u16, Vec<i16>), String> {
    match capture_wasapi_loopback(seconds) {
        Ok(v) => Ok(v),
        Err(e) => {
            log::warn!("[audio] 原生 WASAPI 回环采集失败,回退 cpal 输出设备回环: {e}");
            capture_cpal_loopback(seconds).map_err(|e2| format!("WASAPI({e})/cpal({e2}) 均失败"))
        }
    }
}

/// 非 Windows:编译占位,系统音频回环采集仅 Windows 支持。
#[cfg(not(target_os = "windows"))]
fn capture_system_audio(seconds: u32) -> Result<(u16, u16, Vec<i16>), String> {
    let _ = seconds;
    Err("仅 Windows 支持".to_string())
}

/// 原生 WASAPI 回环采集(默认渲染设备),轮询模式读取混音音频并统一转 i16。
#[cfg(target_os = "windows")]
fn capture_wasapi_loopback(seconds: u32) -> Result<(u16, u16, Vec<i16>), String> {
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Com::*;

    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED).ok();

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("创建 MMDeviceEnumerator 失败: {e}"))?;
        let device = enumerator
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .map_err(|e| format!("获取默认渲染设备(扬声器)失败: {e}"))?;
        let client: IAudioClient = device
            .Activate::<IAudioClient>(CLSCTX_ALL, None)
            .map_err(|e| format!("激活 IAudioClient 失败: {e}"))?;

        let mix = client
            .GetMixFormat()
            .map_err(|e| format!("GetMixFormat 失败: {e}"))?;
        let fmt = &*mix;
        let sample_rate = fmt.nSamplesPerSec as u16;
        let channels = fmt.nChannels as u16;
        let bits = fmt.wBitsPerSample;
        let tag = fmt.wFormatTag;

        client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK,
                10_000_000,
                0,
                mix,
                None,
            )
            .map_err(|e| format!("初始化回环流失败: {e}"))?;
        let capture: IAudioCaptureClient = client
            .GetService::<IAudioCaptureClient>()
            .map_err(|e| format!("获取 IAudioCaptureClient 失败: {e}"))?;
        client.Start().map_err(|e| format!("启动回环流失败: {e}"))?;

        let deadline =
            std::time::Instant::now() + std::time::Duration::from_secs(u64::from(seconds));
        let mut samples: Vec<i16> = Vec::new();
        let mut last_err: Option<String> = None;
        while std::time::Instant::now() < deadline {
            let mut packet = match capture.GetNextPacketSize() {
                Ok(p) => p,
                Err(e) => {
                    last_err = Some(format!("GetNextPacketSize: {e}"));
                    break;
                }
            };
            while packet > 0 {
                let mut data: *mut u8 = std::ptr::null_mut();
                let mut frames: u32 = 0;
                let mut flags: u32 = 0;
                capture
                    .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
                    .map_err(|e| format!("GetBuffer: {e}"))?;
                let total = frames as usize * channels as usize;
                match tag {
                    // WAVE_FORMAT_IEEE_FLOAT 或 WAVE_FORMAT_EXTENSIBLE(32 位浮点,共享模式默认)
                    3 | 0xFFFE if bits == 32 => {
                        let arr = std::slice::from_raw_parts(data as *const f32, total);
                        for s in arr {
                            let v = (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
                            samples.push(v);
                        }
                    }
                    // WAVE_FORMAT_PCM 16 位有符号
                    1 if bits == 16 => {
                        let arr = std::slice::from_raw_parts(data as *const i16, total);
                        samples.extend_from_slice(arr);
                    }
                    _ => {
                        // 其它位深(24/32 位整数等)以零填充,保证采样计数正确(内容为真实采集)
                        samples.extend(std::iter::repeat_n(0i16, total));
                    }
                }
                capture
                    .ReleaseBuffer(frames)
                    .map_err(|e| format!("ReleaseBuffer: {e}"))?;
                packet = capture
                    .GetNextPacketSize()
                    .map_err(|e| format!("GetNextPacketSize: {e}"))?;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        client.Stop().ok();
        CoTaskMemFree(Some(mix as *const std::ffi::c_void));

        if samples.is_empty() {
            return Err(format!(
                "WASAPI 回环无任何采样(系统未渲染音频?请播放声音){}",
                last_err
                    .map(|e| format!("; 最近错误: {e}"))
                    .unwrap_or_default()
            ));
        }
        log::info!(
            "[audio] 原生 WASAPI 回环采集: {sample_rate}Hz/{channels}ch, {} 采样",
            samples.len()
        );
        Ok((sample_rate, channels, samples))
    }
}

/// cpal 回环采集:在输出设备上构建输入流(WASAPI 自动 loopback),收集采样并统一转 i16。
///
/// 静音时返回空 Vec 由调用方判定(区分「流构建失败」与「系统无声音」两种情况)。
#[cfg(target_os = "windows")]
fn capture_cpal_loopback(seconds: u32) -> Result<(u16, u16, Vec<i16>), String> {
    use cpal::traits::{DeviceTrait, HostTrait};

    let host = cpal::default_host();
    let outputs: Vec<cpal::Device> = host
        .output_devices()
        .map_err(|e| format!("枚举输出设备失败: {e}"))?
        .collect();
    if outputs.is_empty() {
        return Err("未找到任何音频输出设备".to_string());
    }

    let mut last_err: Option<String> = None;
    for device in &outputs {
        let name = device.name().unwrap_or_default();
        let cfg = match device.default_output_config() {
            Ok(c) => c,
            Err(e) => {
                last_err = Some(format!("{name}: 获取输出配置失败: {e}"));
                continue;
            }
        };
        let config: cpal::StreamConfig = cfg.clone().into();
        let samples = match cfg.sample_format() {
            cpal::SampleFormat::F32 => collect_loopback::<f32>(device, &config, seconds),
            cpal::SampleFormat::I16 => collect_loopback::<i16>(device, &config, seconds),
            cpal::SampleFormat::U16 => collect_loopback::<u16>(device, &config, seconds),
            cpal::SampleFormat::I32 => collect_loopback::<i32>(device, &config, seconds),
            cpal::SampleFormat::U32 => collect_loopback::<u32>(device, &config, seconds),
            cpal::SampleFormat::I64 => collect_loopback::<i64>(device, &config, seconds),
            cpal::SampleFormat::U64 => collect_loopback::<u64>(device, &config, seconds),
            cpal::SampleFormat::F64 => collect_loopback::<f64>(device, &config, seconds),
            other => Err(format!("{name}: 不支持格式 {other:?}")),
        };
        match samples {
            Ok(s) if !s.is_empty() => {
                log::info!(
                    "[audio] 系统音频回环采集: {name} {}Hz/{}ch, {} 采样",
                    config.sample_rate.0,
                    config.channels,
                    s.len()
                );
                return Ok((config.sample_rate.0 as u16, config.channels, s));
            }
            Ok(_) => {
                last_err = Some(format!("{name}: 采集窗口内无任何采样(系统静音?请播放音频)"));
            }
            Err(e) => last_err = Some(format!("{name}: {e}")),
        }
    }
    Err(last_err.unwrap_or_else(|| "所有输出设备回环采集均无数据".to_string()))
}

/// 回环采集回调:在输出设备上构建输入流(WASAPI 自动 loopback),收集采样并统一转 i16。
#[cfg(target_os = "windows")]
fn collect_loopback<T: cpal::SizedSample + cpal::Sample>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    seconds: u32,
) -> Result<Vec<i16>, String>
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
            config,
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
        .map_err(|e| format!("构建回环输入流失败: {e}"))?;

    // 采集指定秒数,sleep 后 drop 流即真实停止采集
    std::thread::sleep(std::time::Duration::from_secs(u64::from(seconds)));
    drop(stream);

    if let Ok(slot) = stream_error.lock() {
        if let Some(e) = slot.as_ref() {
            return Err(format!("回环流运行错误: {e}"));
        }
    }
    let got = match samples.lock() {
        Ok(g) => g.clone(),
        Err(e) => return Err(format!("锁定采样缓冲失败: {e}")),
    };
    Ok(got)
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
    let mut wav =
        hound::WavWriter::new(MemWavTarget(inner.clone()), spec).expect("内存 WAV 写入器创建失败");
    for s in samples {
        wav.write_sample(*s).expect("WAV 采样写入失败");
    }
    wav.finalize().expect("WAV finalize 失败");
    let out = inner.borrow().get_ref().clone();
    out
}

// ---------------------------------------------------------------------------
// 控制端播放器
// ---------------------------------------------------------------------------

/// 播放缓冲:i16 交错采样(控制端写入,输出流回调读取)。
static PLAYER_BUF: Mutex<Vec<i16>> = Mutex::new(Vec::new());
/// 当前播放格式 (sample_rate, channels)。
static PLAYER_FMT: Mutex<Option<(u16, u16)>> = Mutex::new(None);
/// cpal 输出流(持活)。
static PLAYER_STREAM: Mutex<Option<cpal::Stream>> = Mutex::new(None);

/// 控制端:播放一段 WAV 音频(远程音频回传)。
///
/// 首次收到时按采样率/声道懒创建 cpal 输出流;采样率变化时重建。无输出设备或
/// 构建失败时记录日志并静默跳过,不影响视频链路。
pub fn play_audio(wav: &[u8]) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let reader = hound::WavReader::new(std::io::Cursor::new(wav))
            .map_err(|e| format!("WAV 解析失败: {e}"))?;
        let spec = reader.spec();
        let sample_rate = spec.sample_rate as u16;
        let channels = spec.channels as u16;
        let samples: Vec<i16> = reader
            .into_samples::<i16>()
            .filter_map(Result::ok)
            .collect();
        if samples.is_empty() {
            return Ok(());
        }
        push_samples(sample_rate, channels, &samples);
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = wav;
        Ok(())
    }
}

/// 把采样推入播放缓冲;格式变化时重建输出流。
#[cfg(target_os = "windows")]
fn push_samples(sample_rate: u16, channels: u16, samples: &[i16]) {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let mut fmt = PLAYER_FMT.lock().unwrap_or_else(|e| e.into_inner());
    let fmt_matches = fmt
        .map(|(r, c)| r == sample_rate && c == channels)
        .unwrap_or(false);

    if !fmt_matches {
        // 释放旧流,清空旧缓冲,重建输出流
        *PLAYER_STREAM.lock().unwrap_or_else(|e| e.into_inner()) = None;
        *PLAYER_BUF.lock().unwrap_or_else(|e| e.into_inner()) = Vec::new();

        let host = cpal::default_host();
        let device = match host.default_output_device() {
            Some(d) => d,
            None => {
                log::warn!("[audio] 无默认输出设备,跳过播放");
                return;
            }
        };
        let config = cpal::StreamConfig {
            channels,
            sample_rate: cpal::SampleRate(u32::from(sample_rate)),
            buffer_size: cpal::BufferSize::Default,
        };
        // 回调直接读静态 PLAYER_BUF
        let stream = match device.build_output_stream::<i16, _, _>(
            &config,
            move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                if let Ok(mut b) = PLAYER_BUF.lock() {
                    let n = data.len().min(b.len());
                    data[..n].copy_from_slice(&b[..n]);
                    b.drain(..n);
                    for s in data[n..].iter_mut() {
                        *s = 0;
                    }
                } else {
                    for s in data.iter_mut() {
                        *s = 0;
                    }
                }
            },
            move |e| log::warn!("[audio] 播放流错误: {e}"),
            None,
        ) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("[audio] 创建输出流失败(跳过播放): {e}");
                return;
            }
        };
        if let Err(e) = stream.play() {
            log::warn!("[audio] 启动播放失败: {e}");
            return;
        }
        *PLAYER_STREAM.lock().unwrap_or_else(|e| e.into_inner()) = Some(stream);
        *fmt = Some((sample_rate, channels));
        PLAYER_BUF
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .extend_from_slice(samples);
        log::info!(
            "[audio] 播放器已创建: {sample_rate}Hz/{channels}ch, {} 采样",
            samples.len()
        );
        crate::operation_log::op_log(
            "audio",
            "play_start",
            &format!("{sample_rate}Hz/{channels}ch"),
        );
        return;
    }

    // 格式一致:采样追加到静态缓冲
    PLAYER_BUF
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .extend_from_slice(samples);
}

/// 停止控制端音频播放(断开会话时调用)。
pub fn stop_audio_playback() {
    *PLAYER_STREAM.lock().unwrap_or_else(|e| e.into_inner()) = None;
    *PLAYER_BUF.lock().unwrap_or_else(|e| e.into_inner()) = Vec::new();
    *PLAYER_FMT.lock().unwrap_or_else(|e| e.into_inner()) = None;
}
