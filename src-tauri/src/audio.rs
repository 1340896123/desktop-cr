//! 远程会话音频链路:被控端采集 + 控制端播放。
//!
//! - 被控端:常驻音频采集循环(`start_audio_capture`),周期性调用
//!   `media_pipeline::capture_system_audio`(真实 WASAPI 回环,回退 cpal)采集系统
//!   播放的声音,切成小块经 `latest_audio()` 供 `network::host_write_loop` 以
//!   `Msg::Audio` 推送;仅在新块到达(seq 递增)时才发送。
//! - 控制端:收到 `Msg::Audio` 后调用 `play_audio`(cpal 输出流,i16 播放),实现
//!   真实远控语音回传。无输出设备时静默跳过。
//!
//! 非 Windows 平台:全部为编译占位,保证跨平台可编译。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

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

/// 音频采集线程句柄。
static AUDIO_TASK: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);

/// 取最新音频块(无则 None);host_write_loop 发送后可用 seq 判断是否消费过。
pub fn latest_audio() -> Option<AudioBlock> {
    LATEST_AUDIO.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// 启动被控端音频采集(幂等;已运行则返回)。
pub fn start_audio_capture() -> Result<(), String> {
    let mut slot = AUDIO_TASK.lock().unwrap_or_else(|e| e.into_inner());
    if slot.as_ref().map_or(false, |h| !h.is_finished()) {
        return Ok(());
    }
    let handle = std::thread::Builder::new()
        .name("dcr-audio-capture".into())
        .spawn(audio_capture_loop)
        .map_err(|e| format!("创建音频采集线程失败: {e}"))?;
    *slot = Some(handle);
    log::info!("[audio] 被控端音频采集已启动(系统回环,{CHUNK_SECS}s/块)");
    crate::operation_log::op_log("audio", "start", "");
    Ok(())
}

/// 停止被控端音频采集。
pub fn stop_audio_capture() {
    let handle = AUDIO_TASK.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(h) = handle {
        let _ = h.join();
    }
    if let Ok(mut slot) = LATEST_AUDIO.lock() {
        *slot = None;
    }
    log::info!("[audio] 被控端音频采集已停止");
    crate::operation_log::op_log("audio", "stop", "");
}

/// 音频采集线程循环:周期采集系统回环音频,切块发布到 `LATEST_AUDIO`。
fn audio_capture_loop() {
    #[cfg(target_os = "windows")]
    {
        let mut last_log = std::time::Instant::now();
        loop {
            match crate::media_pipeline::capture_system_audio(CHUNK_SECS) {
                Ok((rate, channels, samples)) => {
                    if samples.is_empty() {
                        continue;
                    }
                    let wav = crate::media_pipeline::pcm_to_wav(rate, channels, &samples);
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
                    // 静音/无设备时降频告警,避免刷屏
                    if last_log.elapsed() > Duration::from_secs(5) {
                        log::warn!("[audio] 采集失败(将重试): {e}");
                        last_log = std::time::Instant::now();
                    }
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        log::info!("[audio] (非 Windows) 音频采集线程占位退出");
    }
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
