import React, { useCallback, useEffect, useRef, useState } from 'react';
import { makeStyles, tokens } from '@fluentui/react-components';
import { DesktopRegular } from '@fluentui/react-icons';
import {
  onDxgiLoopFrame,
  runDxgiLoopback,
  type DxgiLoopbackReport,
} from '../services/diagnostics';
import { listMonitors, type MonitorInfo } from '../services/capture';
import { palette, radius, shadow } from '../theme/tokens';

const useStyles = makeStyles({
  card: {
    backgroundColor: palette.backgroundElevated,
    borderRadius: radius.card,
    boxShadow: shadow.card,
    border: `1px solid ${palette.borderLight}`,
    overflow: 'hidden',
  },
  row: { display: 'flex', alignItems: 'center', gap: '12px', padding: '14px 18px' },
  rowDivider: { height: '1px', backgroundColor: palette.borderLight },
  rowBody: { flex: 1, display: 'flex', flexDirection: 'column', gap: '4px', minWidth: 0 },
  rowTitle: { fontSize: '14px', fontWeight: 600, color: palette.textPrimary },
  rowDesc: { fontSize: '12px', color: palette.textSecondary },
  paramRow: {
    display: 'flex',
    flexWrap: 'wrap',
    alignItems: 'center',
    gap: '8px',
    marginBottom: '8px',
  },
  paramLabel: { fontSize: '12px', color: palette.textSecondary },
  paramInput: {
    height: '30px',
    padding: '0 10px',
    border: `1px solid ${palette.border}`,
    borderRadius: '6px',
    fontSize: '13px',
    color: palette.textPrimary,
    backgroundColor: palette.backgroundElevated,
    outline: 'none',
    '&:focus': { border: `1px solid ${palette.primary}` },
  },
  select: {
    height: '30px',
    padding: '0 8px',
    border: `1px solid ${palette.border}`,
    borderRadius: '6px',
    fontSize: '13px',
    color: palette.textPrimary,
    backgroundColor: palette.backgroundElevated,
    outline: 'none',
  },
  monitorSelect: { flex: '1', minWidth: '180px' },
  grayBtn: {
    height: '32px',
    padding: '0 16px',
    border: `1px solid ${palette.border}`,
    borderRadius: '6px',
    fontSize: '13px',
    fontWeight: 600,
    color: palette.textPrimary,
    backgroundColor: palette.muted,
    cursor: 'pointer',
    ':disabled': { opacity: 0.55, cursor: 'not-allowed' },
  },
  errorText: { fontSize: '12px', color: '#D93025', marginBottom: '8px' },
  benchResult: {
    margin: '0 18px 16px',
    padding: '12px 14px',
    backgroundColor: palette.muted,
    borderRadius: '6px',
    fontSize: '12px',
    lineHeight: 1.8,
    color: palette.textPrimary,
    whiteSpace: 'pre-wrap',
    fontFamily: 'inherit',
  },
  preview: {
    position: 'relative',
    margin: '0 18px 16px',
    border: `1px solid ${palette.borderLight}`,
    borderRadius: '8px',
    overflow: 'hidden',
    backgroundColor: '#101418',
    minHeight: '180px',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
  },
  canvas: { maxWidth: '100%', maxHeight: '360px', display: 'block' },
  placeholder: { color: tokens.colorNeutralForeground3, fontSize: '13px', padding: '24px' },
  liveBadge: { color: '#34C759', fontWeight: 600 },
});

/**
 * 诊断卡片:DXGI 回传自检(真实本机采集)。
 * 真实 DXGI 抓屏 → H.264 编码(NVENC/QSV/AMF 优先)→ 生产协议帧 → 本机 TCP 回环
 * → FFmpeg 解码(D3D11VA 硬解优先)。全程标准视频编解码,禁止 JPEG;
 * 回环到达的 H.264 帧经 `dxgi-loop-frame` 事件回传,前端用 WebCodecs 解码预览。
 */
const DxgiLoopbackCard: React.FC = () => {
  const styles = useStyles();
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [monitorId, setMonitorId] = useState<number>(0);
  const [seconds, setSeconds] = useState('5');
  const [targetFps, setTargetFps] = useState('30');
  const [targetW, setTargetW] = useState('1280');
  const [targetH, setTargetH] = useState('720');
  const [running, setRunning] = useState(false);
  const [report, setReport] = useState<DxgiLoopbackReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [previewLive, setPreviewLive] = useState(false);
  const [decoderError, setDecoderError] = useState<string | null>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const runningRef = useRef(false);

  useEffect(() => {
    void listMonitors().then((list) => {
      setMonitors(list);
      const primary = list.find((m) => m.isPrimary) ?? list[0];
      if (primary) setMonitorId(primary.id);
    });
  }, []);

  // WebCodecs H.264 解码器(懒创建;诊断进行中持续解码回传帧)
  useEffect(() => {
    if (!running) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let decoder: VideoDecoder | null = null;
    const frameQueue: Array<{ key: boolean; timestamp: number; data: Uint8Array }> = [];
    let nextTimestamp = 0;

    const ensureDecoder = (width: number, height: number): boolean => {
      if (decoder) return true;
      if (typeof VideoDecoder === 'undefined') {
        setDecoderError('当前 WebView 不支持 WebCodecs,无法预览(诊断结论不受影响)');
        return false;
      }
      const canvas = canvasRef.current;
      const ctx = canvas?.getContext('2d');
      if (!canvas || !ctx) return false;
      try {
        decoder = new VideoDecoder({
          output: (frame: VideoFrame) => {
            if (disposed || !canvasRef.current) {
              frame.close();
              return;
            }
            const c = canvasRef.current;
            if (c.width !== frame.displayWidth || c.height !== frame.displayHeight) {
              c.width = frame.displayWidth;
              c.height = frame.displayHeight;
            }
            const c2d = c.getContext('2d');
            c2d?.drawImage(frame, 0, 0);
            frame.close();
          },
          error: (e: DOMException) => {
            setDecoderError(`WebCodecs 解码错误: ${e.message}`);
          },
        });
        decoder.configure({
          codec: 'avc1.42001f',
          codedWidth: width,
          codedHeight: height,
          optimizeForLatency: true,
        });
        setPreviewLive(true);
        return true;
      } catch (e) {
        setDecoderError(`WebCodecs 初始化失败: ${String(e)}`);
        return false;
      }
    };

    const pump = () => {
      if (!decoder || decoder.decodeQueueSize > 4) return;
      const chunk = frameQueue.shift();
      if (!chunk) return;
      try {
        decoder.decode(
          new EncodedVideoChunk({
            type: chunk.key ? 'key' : 'delta',
            timestamp: chunk.timestamp,
            data: chunk.data,
          }),
        );
      } catch (e) {
        setDecoderError(`WebCodecs 解码入队失败: ${String(e)}`);
      }
    };
    let gotKeyframe = false;

    void onDxgiLoopFrame((frame) => {
      if (disposed) return;
      if (!ensureDecoder(frame.width, frame.height)) return;
      // 首块必须为关键帧(WebCodecs 的 delta 帧不能独立解码)
      if (!gotKeyframe && !frame.key) return;
      if (frame.key) gotKeyframe = true;
      frameQueue.push({
        key: frame.key,
        timestamp: nextTimestamp,
        data: new Uint8Array(frame.data),
      });
      nextTimestamp += 1;
      pump();
    }).then((fn) => {
      if (!disposed) unlisten = fn;
    });

    return () => {
      disposed = true;
      unlisten?.();
      decoder?.close();
    };
  }, [running]);

  const run = useCallback(async () => {
    if (runningRef.current) return;
    runningRef.current = true;
    setRunning(true);
    setError(null);
    setReport(null);
    setPreviewLive(false);
    setDecoderError(null);
    try {
      const result = await runDxgiLoopback({
        monitorId,
        seconds: Number(seconds) || 5,
        targetFps: Number(targetFps) || 30,
        targetWidth: Number(targetW) || 1280,
        targetHeight: Number(targetH) || 720,
      });
      if (result) setReport(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setRunning(false);
      runningRef.current = false;
    }
  }, [monitorId, seconds, targetFps, targetW, targetH]);

  const monitorOptions =
    monitors.length > 0
      ? monitors
      : [{ id: 0, name: '显示器 0', width: 0, height: 0, isPrimary: true, isVirtual: false }];

  return (
    <div className={styles.card}>
      <div className={styles.row}>
        <DesktopRegular fontSize={16} />
        <div className={styles.rowBody}>
          <div className={styles.rowTitle}>DXGI 回传 · 真实本机采集</div>
          <div className={styles.rowDesc}>
            真实 DXGI 抓屏 → H.264 编码(硬编优先)→ 协议帧 → 本机 TCP 回环 → 硬解优先解码;
            全程标准视频编解码,禁止 JPEG
          </div>
        </div>
      </div>
      <div className={styles.rowDivider} />
      <div className={styles.row}>
        <div className={styles.rowBody}>
          <div className={styles.paramRow}>
            <span className={styles.paramLabel}>显示器</span>
            <select
              className={`${styles.select} ${styles.monitorSelect}`}
              value={monitorId}
              onChange={(e) => setMonitorId(Number(e.target.value))}
            >
              {monitorOptions.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.name}
                  {m.width > 0 ? ` (${m.width}x${m.height})` : ''}
                  {m.isPrimary ? ' · 主屏' : ''}
                  {m.isVirtual ? ' · 虚拟' : ''}
                </option>
              ))}
            </select>
            <span className={styles.paramLabel}>秒数</span>
            <input
              className={styles.paramInput}
              style={{ width: 64 }}
              value={seconds}
              onChange={(e) => setSeconds(e.target.value)}
            />
            <span className={styles.paramLabel}>目标帧率</span>
            <input
              className={styles.paramInput}
              style={{ width: 64 }}
              value={targetFps}
              onChange={(e) => setTargetFps(e.target.value)}
            />
          </div>
          <div className={styles.paramRow}>
            <span className={styles.paramLabel}>分辨率</span>
            <input
              className={styles.paramInput}
              style={{ width: 76 }}
              value={targetW}
              onChange={(e) => setTargetW(e.target.value)}
            />
            <span className={styles.paramLabel}>x</span>
            <input
              className={styles.paramInput}
              style={{ width: 76 }}
              value={targetH}
              onChange={(e) => setTargetH(e.target.value)}
            />
          </div>
          {error && <div className={styles.errorText}>{error}</div>}
          <button type="button" className={styles.grayBtn} disabled={running} onClick={() => void run()}>
            {running ? '运行中…' : '运行 DXGI 回传诊断'}
          </button>
        </div>
      </div>

      <div className={styles.preview}>
        <canvas ref={canvasRef} className={styles.canvas} width={960} height={540} />
        {!previewLive && (
          <div className={styles.placeholder}>
            {decoderError ?? (running ? '等待回环帧…' : '运行诊断后在此预览本机采集画面(H.264 解码)')}
          </div>
        )}
      </div>

      {report && (
        <pre className={styles.benchResult}>{`编解码: ${report.codec} | 编码器: ${report.encoder} | 解码: ${
          report.decoderHwaccel ? 'D3D11VA 硬解' : '软件解码'
        }
显示器: #${report.monitorId} | 时长: ${report.seconds}s | 目标帧率: ${report.targetFps}
采集源: ${report.sourceWidth}x${report.sourceHeight} → 编码输出: ${report.frameWidth}x${report.frameHeight}
真实抓帧: ${report.framesGrabbed} | 发送: ${report.framesSent} | 解码: ${report.framesRendered} | 实时帧率: ${report.realtimeFps.toFixed(1)} fps
平均耗时 — 抓屏 ${report.avgCaptureMs.toFixed(2)} ms | 编码 ${report.avgEncodeMs.toFixed(2)} ms | 发送 ${report.avgSendMs.toFixed(2)} ms | 解码 ${report.avgDecodeMs.toFixed(2)} ms
端到端延迟: ${report.avgE2eLatencyMs.toFixed(2)} ms | 传输量: ${(report.totalTransferBytes / 1024 / 1024).toFixed(2)} MB`}</pre>
      )}
    </div>
  );
};

export default DxgiLoopbackCard;
