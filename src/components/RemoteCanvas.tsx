import React, { useCallback, useEffect, useRef, useState } from 'react';
import { makeStyles, tokens } from '@fluentui/react-components';
import { sendKeyEvent, sendMouseEvent, type MouseInputPayload } from '../services/input';
import { onFrame, onRemoteFrame, type CapturedFrame, type RemoteFrame } from '../services/capture';
import { normalizePointer } from '../utils/coords';

const useStyles = makeStyles({
  container: {
    width: '100%',
    height: '100%',
    position: 'relative',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: tokens.colorNeutralBackground2,
    overflow: 'hidden',
  },
  canvas: {
    cursor: 'crosshair',
    maxWidth: '100%',
    maxHeight: '100%',
    touchAction: 'none',
    userSelect: 'none',
  },
  placeholder: {
    color: tokens.colorNeutralForeground3,
    textAlign: 'center',
  },
  errorPanel: {
    color: tokens.colorNeutralForeground1,
    backgroundColor: tokens.colorNeutralBackground2,
    border: `1px solid ${tokens.colorNeutralStroke1}`,
    borderRadius: '8px',
    padding: '18px 24px',
    maxWidth: '420px',
    textAlign: 'center',
    fontSize: '13px',
    lineHeight: '22px',
  },
  overlay: {
    position: 'absolute',
    top: '8px',
    left: '50%',
    transform: 'translateX(-50%)',
    color: tokens.colorNeutralForeground2,
    backgroundColor: tokens.colorNeutralBackground2,
    padding: '4px 12px',
    borderRadius: '4px',
    fontSize: '12px',
  },
  liveBadge: {
    color: '#34C759',
    fontWeight: 600,
  },
  simulatedBadge: {
    color: '#FF9500',
    fontWeight: 600,
  },
});

interface RemoteCanvasProps {
  connected: boolean;
  /** 被控端虚拟屏分辨率 */
  remoteWidth: number;
  remoteHeight: number;
  /** 渲染模式：canvas 兼容模式 / video 高帧率模式 */
  mode?: 'canvas' | 'video';
  /** 画面数据源：local = 本机抓帧预览（capture-frame），remote = 远程画面（remote-frame） */
  streamSource?: 'local' | 'remote';
}

/** 事件负载帧（local 与 remote 契约的公共结构，按 codec 字段分发；seq/dur 仅 remote 源携带） */
type IncomingFrame = {
  width: number;
  height: number;
  key: boolean;
  codec: string;
  data: number[];
  simulated?: boolean;
};
/** 契约自检：事件源回调负载必须可赋给 IncomingFrame（编译期保证两侧类型对齐） */
const incomingFromCaptured = (f: CapturedFrame): IncomingFrame => f;
const incomingFromRemote = (f: RemoteFrame): IncomingFrame => f;
void incomingFromCaptured;
void incomingFromRemote;

/** WebCodecs 配置字符串：h264 → Baseline Profile level 3.0；hevc → Main Profile level 120 */
function webCodecsCodecId(codec: string): string | null {
  switch (codec) {
    case 'h264':
      return 'avc1.42001e';
    case 'hevc':
      return 'hev1.1.6.L120.90';
    default:
      return null;
  }
}

/**
 * 远程画面渲染组件。
 * 监听 pointer / key / wheel 事件，按 README 第 4 节公式做坐标归一化：
 *   X_remote = x * W_remote / W_css
 *   Y_remote = y * H_remote / H_css
 * 帧渲染（WebCodecs 硬解路径）：
 *   - h264/hevc 帧 → VideoDecoder 解码 → VideoFrame 最新帧槽 → requestAnimationFrame 绘制；
 *   - bgra 帧（本机预览）→ 通道交换转 RGBA → putImageData；
 *   - 积压丢弃：解码队列长度 > 1 时丢弃旧 delta 帧只保留最新一帧（D2 延迟控制）。
 * 全程标准视频编解码，禁止 JPEG / Blob / createImageBitmap。
 */
export const RemoteCanvas: React.FC<RemoteCanvasProps> = ({
  connected,
  remoteWidth,
  remoteHeight,
  mode = 'canvas',
  streamSource = 'local',
}) => {
  const styles = useStyles();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [frameSize, setFrameSize] = useState({ width: remoteWidth, height: remoteHeight });
  const [live, setLive] = useState(false);
  const [simulated, setSimulated] = useState(false);
  const [decodeError, setDecodeError] = useState<string | null>(null);
  const [renderFps, setRenderFps] = useState(0);

  // WebCodecs 不可用（D4）：显示明确中文错误提示，不白屏不崩溃
  const webCodecsAvailable = typeof window !== 'undefined' && 'VideoDecoder' in window;

  // —— 渲染内部状态（ref，避免重订阅事件源） ——
  const latestFrameRef = useRef<VideoFrame | null>(null); // 最新帧槽（D2）
  const decoderRef = useRef<VideoDecoder | null>(null);
  const decoderCodecRef = useRef<string | null>(null); // 已 configure 的 codec 家族
  const gotKeyframeRef = useRef(false);
  const pendingChunkRef = useRef<EncodedVideoChunk | null>(null); // 待解码的最新一帧（≤1 积压）
  const timestampRef = useRef(0);
  const decodedCountRef = useRef(0); // 解码输出计数（渲染帧率滑动窗口）
  const decodedTimestampsRef = useRef<number[]>([]); // 最近 1s 输出时间戳
  const rafRef = useRef<number | null>(null);
  const statsTimerRef = useRef<number | null>(null);

  const normalize = useCallback(
    (clientX: number, clientY: number) => {
      const canvas = canvasRef.current;
      if (!canvas) return null;
      const rect = canvas.getBoundingClientRect();
      return normalizePointer(clientX, clientY, rect, remoteWidth, remoteHeight);
    },
    [remoteWidth, remoteHeight],
  );

  const buildMousePayload = useCallback(
    (
      event: React.PointerEvent,
      eventType: MouseInputPayload['event_type'],
    ): MouseInputPayload | null => {
      const pos = normalize(event.clientX, event.clientY);
      if (!pos) return null;
      const buttonMap: Record<number, MouseInputPayload['button']> = {
        0: 'left',
        1: 'middle',
        2: 'right',
      };
      const payload: MouseInputPayload = { event_type: eventType, ...pos };
      if (eventType !== 'mousemove' && eventType !== 'wheel') {
        payload.button = buttonMap[event.button] ?? 'left';
      }
      return payload;
    },
    [normalize],
  );

  const handlePointerDown = useCallback(
    (event: React.PointerEvent) => {
      const payload = buildMousePayload(event, 'mousedown');
      if (payload) void sendMouseEvent(payload);
    },
    [buildMousePayload],
  );

  const handlePointerUp = useCallback(
    (event: React.PointerEvent) => {
      const payload = buildMousePayload(event, 'mouseup');
      if (payload) void sendMouseEvent(payload);
    },
    [buildMousePayload],
  );

  const handlePointerMove = useCallback(
    (event: React.PointerEvent) => {
      const payload = buildMousePayload(event, 'mousemove');
      if (payload) void sendMouseEvent(payload);
    },
    [buildMousePayload],
  );

  const handleWheel = useCallback(
    (event: React.WheelEvent) => {
      const pos = normalize(event.clientX, event.clientY);
      if (!pos) return;
      void sendMouseEvent({
        event_type: 'wheel',
        ...pos,
        delta_y: event.deltaY,
      });
    },
    [normalize],
  );

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent) => {
      event.preventDefault();
      void sendKeyEvent({
        key: event.key,
        event_type: 'keydown',
        code: event.code,
        modifiers: ['ctrlKey', 'shiftKey', 'altKey', 'metaKey'].filter((m) => event[m as keyof React.KeyboardEvent]),
      });
    },
    [],
  );

  const handleKeyUp = useCallback(
    (event: React.KeyboardEvent) => {
      event.preventDefault();
      void sendKeyEvent({
        key: event.key,
        event_type: 'keyup',
        code: event.code,
        modifiers: [],
      });
    },
    [],
  );

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || mode !== 'canvas') return;
    canvas.tabIndex = 0;
    canvas.focus();
  }, [mode]);

  useEffect(() => {
    setFrameSize({ width: remoteWidth, height: remoteHeight });
  }, [remoteWidth, remoteHeight]);

  /** 确保（或复用）指定 codec 家族的 VideoDecoder；失败返回 null 并记录错误 */
  const ensureDecoder = useCallback(
    (codec: string, width: number, height: number): VideoDecoder | null => {
      const existing = decoderRef.current;
      if (existing && decoderCodecRef.current === codec) return existing;
      if (existing) {
        // codec 家族切换（h264 ↔ hevc）：重建解码器
        try {
          existing.close();
        } catch {
          /* 已关闭则忽略 */
        }
        decoderRef.current = null;
        decoderCodecRef.current = null;
        gotKeyframeRef.current = false;
      }
      const codecId = webCodecsCodecId(codec);
      if (!codecId) return null;
      try {
        const decoder = new VideoDecoder({
          output: (frame: VideoFrame) => {
            // 最新帧槽：新帧到达即关闭旧帧（VideoFrame 须显式释放，避免 GPU 内存泄漏）
            const prev = latestFrameRef.current;
            latestFrameRef.current = frame;
            prev?.close();
            const now = performance.now();
            decodedTimestampsRef.current.push(now);
            decodedCountRef.current += 1;
          },
          error: (e: DOMException) => {
            setDecodeError(`WebCodecs 解码错误: ${e.message}`);
          },
        });
        decoder.configure({
          codec: codecId,
          codedWidth: width,
          codedHeight: height,
          optimizeForLatency: true,
        });
        decoderRef.current = decoder;
        decoderCodecRef.current = codec;
        gotKeyframeRef.current = false;
        setDecodeError(null);
        return decoder;
      } catch (e) {
        setDecodeError(`WebCodecs 初始化失败: ${String(e)}`);
        return null;
      }
    },
    [],
  );

  /** 将积压的最新一帧送入解码器（每 tick 至多 1 帧，队列空闲时回调泵） */
  const pumpDecode = useCallback(() => {
    const decoder = decoderRef.current;
    const chunk = pendingChunkRef.current;
    if (!decoder || !chunk || decoder.state !== 'configured') return;
    if (decoder.decodeQueueSize > 1) return; // 积压 > 1：暂缓入队，等待输出排空
    pendingChunkRef.current = null;
    try {
      decoder.decode(chunk);
    } catch (e) {
      setDecodeError(`WebCodecs 解码入队失败: ${String(e)}`);
    }
  }, []);

  // 订阅帧事件（本机 capture-frame 或远程 remote-frame）→ WebCodecs 解码 / BGRA 直绘。
  // 解码 output 维护最新帧槽，绘制统一走 requestAnimationFrame 循环（D1/D2）。
  useEffect(() => {
    if (!connected || mode !== 'canvas') return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    // 重置会话内状态
    gotKeyframeRef.current = false;
    pendingChunkRef.current = null;
    setLive(false);
    setSimulated(false);
    setDecodeError(null);
    if (!webCodecsAvailable) return; // D4：渲染层显示错误提示，不订阅不崩溃

    const handleVideoFrame = (frame: IncomingFrame) => {
      setLive(true);
      setSimulated(Boolean(frame.simulated));
      setFrameSize((prev) =>
        prev.width === frame.width && prev.height === frame.height
          ? prev
          : { width: frame.width, height: frame.height },
      );
      const decoder = ensureDecoder(frame.codec, frame.width, frame.height);
      if (!decoder) return;
      // 首块必须为关键帧（WebCodecs 的 delta 帧不能独立解码）
      if (!gotKeyframeRef.current) {
        if (!frame.key) return;
        gotKeyframeRef.current = true;
      }
      const chunk = new EncodedVideoChunk({
        type: frame.key ? 'key' : 'delta',
        timestamp: timestampRef.current,
        data: new Uint8Array(frame.data),
      });
      timestampRef.current += 1;
      // 积压丢弃：pending 槽只保留最新一帧（覆盖旧 delta，D2 延迟控制）
      pendingChunkRef.current = chunk;
      pumpDecode();
    };

    /** BGRA 原始帧直绘：通道交换 BGRA → RGBA 后 putImageData */
    const handleBgraFrame = (frame: IncomingFrame) => {
      const canvas = canvasRef.current;
      const ctx = canvas?.getContext('2d');
      if (!canvas || !ctx) return;
      if (canvas.width !== frame.width || canvas.height !== frame.height) {
        canvas.width = frame.width;
        canvas.height = frame.height;
      }
      const bgra = frame.data;
      const rgba = new Uint8ClampedArray(bgra.length);
      for (let i = 0; i + 3 < bgra.length; i += 4) {
        rgba[i] = bgra[i + 2]; // R ← B
        rgba[i + 1] = bgra[i + 1]; // G
        rgba[i + 2] = bgra[i]; // B ← R
        rgba[i + 3] = bgra[i + 3]; // A
      }
      try {
        ctx.putImageData(new ImageData(rgba, frame.width, frame.height), 0, 0);
        setLive(true);
        setSimulated(Boolean(frame.simulated));
        setFrameSize((prev) =>
          prev.width === frame.width && prev.height === frame.height
            ? prev
            : { width: frame.width, height: frame.height },
        );
      } catch (e) {
        setDecodeError(`BGRA 帧绘制失败: ${String(e)}`);
      }
    };

    const handleFrame = (frame: IncomingFrame) => {
      if (disposed) return;
      if (frame.codec === 'bgra') {
        handleBgraFrame(frame);
        return;
      }
      handleVideoFrame(frame);
    };

    const subPromise =
      streamSource === 'remote'
        ? onRemoteFrame((frame) => handleFrame(frame))
        : onFrame((frame) => handleFrame(frame));
    void subPromise.then((fn) => {
      if (!disposed) unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [connected, mode, streamSource, webCodecsAvailable, ensureDecoder, pumpDecode]);

  // requestAnimationFrame 绘制循环：每帧把最新帧槽绘制到画布（适配画布尺寸）
  useEffect(() => {
    if (!connected || mode !== 'canvas' || !webCodecsAvailable) return;
    const draw = () => {
      const frame = latestFrameRef.current;
      if (frame) {
        const canvas = canvasRef.current;
        const ctx = canvas?.getContext('2d');
        if (canvas && ctx) {
          if (canvas.width !== frame.displayWidth || canvas.height !== frame.displayHeight) {
            canvas.width = frame.displayWidth;
            canvas.height = frame.displayHeight;
            setFrameSize((prev) =>
              prev.width === frame.displayWidth && prev.height === frame.displayHeight
                ? prev
                : { width: frame.displayWidth, height: frame.displayHeight },
            );
          }
          ctx.drawImage(frame, 0, 0, canvas.width, canvas.height);
        }
      }
      rafRef.current = window.requestAnimationFrame(draw);
    };
    rafRef.current = window.requestAnimationFrame(draw);
    return () => {
      if (rafRef.current != null) window.cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
    };
  }, [connected, mode, webCodecsAvailable]);

  // 解码输出空闲回调：排空积压的 pending 帧 + 每秒滑动窗口统计渲染帧率
  useEffect(() => {
    if (!connected || mode !== 'canvas' || !webCodecsAvailable) return;
    const tick = () => {
      pumpDecode();
      const now = performance.now();
      const window1s = decodedTimestampsRef.current.filter((t) => now - t <= 1000);
      decodedTimestampsRef.current = window1s;
      setRenderFps(window1s.length);
    };
    statsTimerRef.current = window.setInterval(tick, 250);
    return () => {
      if (statsTimerRef.current != null) window.clearInterval(statsTimerRef.current);
      statsTimerRef.current = null;
    };
  }, [connected, mode, webCodecsAvailable, pumpDecode]);

  // 卸载/断开时释放解码器与最新帧槽
  useEffect(() => {
    return () => {
      try {
        decoderRef.current?.close();
      } catch {
        /* 已关闭则忽略 */
      }
      decoderRef.current = null;
      decoderCodecRef.current = null;
      latestFrameRef.current?.close();
      latestFrameRef.current = null;
      pendingChunkRef.current = null;
    };
  }, []);

  // 帧尺寸变化时 canvas 的 width/height 会被重置并清空，下一 rAF 会以最新帧重绘；
  // 无编码帧（BGRA 路径）时清空为占位背景
  useEffect(() => {
    if (latestFrameRef.current) return; // rAF 循环会重绘
    const canvas = canvasRef.current;
    if (!canvas || mode !== 'canvas') return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.clearRect(0, 0, canvas.width, canvas.height);
  }, [mode, frameSize]);

  // 兼容模式下绘制一个模拟帧，用于演示坐标区域；收到第一帧后停止绘制网格
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || mode !== 'canvas' || live) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.fillStyle = '#1f2937';
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.strokeStyle = '#4b5563';
    ctx.lineWidth = 1;
    for (let x = 0; x <= canvas.width; x += 64) {
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, canvas.height);
      ctx.stroke();
    }
    for (let y = 0; y <= canvas.height; y += 64) {
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(canvas.width, y);
      ctx.stroke();
    }
    ctx.fillStyle = '#9ca3af';
    ctx.font = '14px system-ui';
    ctx.textAlign = 'center';
    ctx.fillText(`Remote Desktop ${remoteWidth}x${remoteHeight} (Canvas Mode)`, canvas.width / 2, canvas.height / 2);
  }, [mode, frameSize, remoteWidth, remoteHeight, live]);

  if (!connected) {
    return (
      <div className={styles.container}>
        <div className={styles.placeholder}>
          <p>尚未连接到远程设备</p>
          <p>请在「设备」页选择一个设备开始连接</p>
        </div>
      </div>
    );
  }

  if (mode === 'video') {
    return (
      <div className={styles.container}>
        <div className={styles.overlay}>Video 渲染模式（WebRTC 高帧率）</div>
        <video
          style={{ maxWidth: '100%', maxHeight: '100%' }}
          controls={false}
          muted
          playsInline
          onPointerDown={handlePointerDown}
          onPointerUp={handlePointerUp}
          onPointerMove={handlePointerMove}
          onWheel={handleWheel}
          onKeyDown={handleKeyDown}
          onKeyUp={handleKeyUp}
        />
      </div>
    );
  }

  // D4：WebCodecs 不可用时给出明确中文错误提示（画面区替换为说明面板，不白屏不崩溃）
  if (!webCodecsAvailable) {
    return (
      <div className={styles.container}>
        <div className={styles.errorPanel}>
          <p style={{ fontWeight: 600, marginBottom: '6px' }}>当前 WebView 不支持 WebCodecs，无法硬解渲染</p>
          <p>请使用 WebView2（Edge 内核）或升级浏览器版本后重试；输入事件仍可正常上报。</p>
        </div>
      </div>
    );
  }

  const overlayLabel =
    streamSource === 'remote'
      ? `远程画面 · Live · ${renderFps > 0 ? `${renderFps} fps` : ''}`
      : simulated
        ? '本机预览 · 模拟画面（非真实抓屏）'
        : '本机预览 · Live';
  const overlayBadgeClass = simulated ? styles.simulatedBadge : styles.liveBadge;

  return (
    <div className={styles.container}>
      <div className={styles.overlay}>
        {frameSize.width}x{frameSize.height} · 已连接
        {live && <span className={overlayBadgeClass}> · {overlayLabel}</span>}
        {decodeError && <span> · 解码异常</span>}
      </div>
      <canvas
        ref={canvasRef}
        className={styles.canvas}
        width={frameSize.width}
        height={frameSize.height}
        onPointerDown={handlePointerDown}
        onPointerUp={handlePointerUp}
        onPointerMove={handlePointerMove}
        onWheel={handleWheel}
        onKeyDown={handleKeyDown}
        onKeyUp={handleKeyUp}
      />
    </div>
  );
};

export default RemoteCanvas;
