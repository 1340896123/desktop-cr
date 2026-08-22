import React, { useCallback, useEffect, useRef, useState } from 'react';
import { makeStyles } from '@fluentui/react-components';
import {
  MicRegular,
  MicOffRegular,
  AddRegular,
  ChevronDownRegular,
  VideoRegular,
  FullScreenMaximizeRegular,
  ClipboardRegular,
  SettingsRegular,
  ImageRegular,
  DesktopRegular,
  KeyboardRegular,
  Wifi3Regular,
} from '@fluentui/react-icons';
import { fontFamily, palette, radius, shadow, spacing, titleBarHeight, zIndex } from '../theme/tokens';
import {
  createLossStats,
  feedLossStats,
  type LossBaseline,
  type LossStats,
} from '../utils/lossStats';
import {
  setFullscreen as requestFullscreen,
  setQuality,
  setResolution,
  syncClipboard,
  onClipboardSynced,
} from '../services/connection';
import { onRemoteFrame, type MonitorInfo } from '../services/capture';
import { getSessionMetrics, requestRemoteMonitors, selectSessionMonitor, onRemoteMonitors, transportLabel, type SessionMetrics } from '../services/session';
import { setAudioMuted, getAudioMuted, onAudioStateChange } from '../services/audio';
import RemoteCanvas from './RemoteCanvas';
import WindowControls from './shared/WindowControls';

const useStyles = makeStyles({
  session: {
    height: '100%',
    display: 'flex',
    flexDirection: 'column',
    backgroundColor: palette.background,
    color: palette.textPrimary,
    position: 'relative',
    overflow: 'hidden',
  },
  bar: {
    height: `${titleBarHeight}px`,
    display: 'flex',
    alignItems: 'center',
    gap: `${spacing.md}px`,
    padding: `0 ${spacing.lg}px`,
    backgroundColor: palette.backgroundElevated,
    borderBottom: `1px solid ${palette.borderLight}`,
    userSelect: 'none',
    zIndex: zIndex.titleBar,
    flexShrink: 0,
  },
  barLeft: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    minWidth: 0,
  },
  devicePill: {
    display: 'inline-flex',
    alignItems: 'center',
    gap: '6px',
    padding: '3px 10px',
    borderRadius: radius.control,
    backgroundColor: palette.primarySoft,
    border: `1px solid ${palette.border}`,
    color: palette.textPrimary,
    fontFamily,
    fontSize: '12px',
    fontWeight: 600,
    whiteSpace: 'nowrap',
  },
  devicePillIcon: {
    display: 'flex',
    color: palette.primary,
  },
  superScreen: {
    display: 'inline-flex',
    alignItems: 'center',
    padding: '2px 8px',
    borderRadius: radius.pill,
    backgroundColor: palette.onlineBadgeBg,
    color: palette.onlineBadgeText,
    fontFamily,
    fontSize: '12px',
    fontWeight: 600,
    whiteSpace: 'nowrap',
  },
  signalGroup: {
    display: 'flex',
    alignItems: 'center',
    gap: '6px',
    color: palette.textSecondary,
  },
  signalDot: {
    width: '8px',
    height: '8px',
    borderRadius: '50%',
    backgroundColor: palette.online,
  },
  timer: {
    fontFamily,
    fontSize: '12px',
    color: palette.textSecondary,
    fontVariantNumeric: 'tabular-nums',
  },
  barCenter: {
    display: 'flex',
    alignItems: 'center',
    gap: '4px',
    margin: '0 auto',
  },
  displayTab: {
    display: 'inline-flex',
    alignItems: 'center',
    gap: '6px',
    height: '26px',
    padding: '0 10px',
    backgroundColor: palette.muted,
    border: `1px solid ${palette.borderLight}`,
    color: palette.textPrimary,
    borderRadius: radius.control,
    fontFamily,
    fontSize: '12px',
    fontWeight: 600,
    cursor: 'pointer',
  },
  addBtn: {
    display: 'inline-flex',
    alignItems: 'center',
    justifyContent: 'center',
    width: '26px',
    height: '26px',
    border: 'none',
    background: 'transparent',
    borderRadius: radius.control,
    color: palette.textSecondary,
    cursor: 'pointer',

    '&:hover': {
      backgroundColor: palette.muted,
      color: palette.textPrimary,
    },
  },
  barRight: {
    display: 'flex',
    alignItems: 'center',
    gap: '10px',
    marginLeft: 'auto',
  },
  iconBtn: {
    display: 'inline-flex',
    alignItems: 'center',
    justifyContent: 'center',
    width: '28px',
    height: '28px',
    border: 'none',
    background: 'transparent',
    borderRadius: radius.control,
    color: palette.textSecondary,
    cursor: 'pointer',

    '&:hover': {
      backgroundColor: palette.muted,
      color: palette.textPrimary,
    },
  },
  centerBtn: {
    display: 'inline-flex',
    alignItems: 'center',
    gap: '6px',
    height: '28px',
    padding: '0 10px',
    border: 'none',
    background: 'transparent',
    borderRadius: radius.control,
    color: palette.textSecondary,
    fontFamily,
    fontSize: '12px',
    cursor: 'pointer',

    '&:hover': {
      backgroundColor: palette.muted,
      color: palette.textPrimary,
    },
  },
  disconnectBtn: {
    display: 'inline-flex',
    alignItems: 'center',
    padding: '5px 14px',
    borderRadius: '6px',
    border: 'none',
    backgroundColor: palette.destructive,
    color: palette.textOnPrimary,
    fontFamily,
    fontSize: '12px',
    fontWeight: 500,
    cursor: 'pointer',
    transition: 'background-color 150ms ease',

    '&:hover': {
      backgroundColor: '#B91C1C',
    },
  },
  canvasArea: {
    flex: 1,
    position: 'relative',
    minHeight: 0,
    backgroundColor: '#0B1220',
  },
  perfOverlay: {
    position: 'absolute',
    right: '14px',
    bottom: '14px',
    backgroundColor: 'rgba(255, 255, 255, 0.86)',
    borderRadius: radius.cardInner,
    padding: '10px 14px',
    fontFamily,
    fontSize: '11px',
    lineHeight: '20px',
    color: palette.textSecondary,
    backdropFilter: 'blur(6px)',
    border: `1px solid ${palette.borderLight}`,
    boxShadow: shadow.popover,
    zIndex: 5,
    fontVariantNumeric: 'tabular-nums',
    minWidth: '220px',
  },
  perfTitle: {
    display: 'flex',
    justifyContent: 'space-between',
    color: palette.primary,
    fontWeight: 700,
    borderBottom: `1px solid ${palette.borderLight}`,
    paddingBottom: '4px',
    marginBottom: '4px',
  },
  perfRow: {
    display: 'flex',
    justifyContent: 'space-between',
    gap: '16px',
  },
  perfVal: {
    color: palette.textPrimary,
    fontWeight: 500,
    textAlign: 'right',
  },
  notice: {
    position: 'absolute',
    right: '14px',
    bottom: '184px',
    backgroundColor: 'rgba(17, 24, 39, 0.82)',
    borderRadius: radius.cardInner,
    padding: '8px 14px',
    fontFamily,
    fontSize: '12px',
    lineHeight: '18px',
    color: '#FFFFFF',
    backdropFilter: 'blur(6px)',
    zIndex: 6,
    maxWidth: '320px',
  },
  centerPanel: {
    position: 'absolute',
    top: '48px',
    right: '12px',
    width: '280px',
    backgroundColor: palette.backgroundElevated,
    borderRadius: radius.card,
    border: `1px solid ${palette.borderLight}`,
    boxShadow: shadow.popover,
    padding: '8px',
    zIndex: 20,
  },
  centerTitle: {
    fontFamily,
    fontSize: '13px',
    fontWeight: 600,
    color: palette.textPrimary,
    padding: '6px 10px 8px',
    borderBottom: `1px solid ${palette.borderLight}`,
    marginBottom: '6px',
  },
  centerItem: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '8px 10px',
    borderRadius: '6px',
    fontFamily,
    fontSize: '13px',
    color: palette.textSecondary,
    cursor: 'pointer',

    '&:hover': {
      backgroundColor: palette.primarySoft,
      color: palette.textPrimary,
    },
  },
  centerItemDisabled: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '8px 10px',
    borderRadius: '6px',
    fontFamily,
    fontSize: '13px',
    color: palette.textMuted,
    opacity: 0.6,
    cursor: 'not-allowed',
  },
  centerItemIcon: {
    display: 'flex',
    color: palette.primary,
  },
  displayMenu: {
    position: 'absolute',
    top: '46px',
    left: '50%',
    transform: 'translateX(-50%)',
    backgroundColor: palette.backgroundElevated,
    borderRadius: radius.card,
    border: `1px solid ${palette.borderLight}`,
    boxShadow: shadow.popover,
    padding: '6px',
    zIndex: 20,
    minWidth: '160px',
  },
  displayMenuItem: {
    padding: '7px 12px',
    borderRadius: '6px',
    fontFamily,
    fontSize: '13px',
    color: palette.textSecondary,
    cursor: 'pointer',

    '&:hover': {
      backgroundColor: palette.muted,
    },
  },
  displayMenuItemActive: {
    color: palette.textPrimary,
    fontWeight: 600,
    backgroundColor: palette.primarySoft,
  },
displayMenuEmpty: {
    padding: '7px 12px',
    borderRadius: '6px',
    fontFamily,
    fontSize: '13px',
    color: palette.textMuted,
    opacity: 0.7,
    cursor: 'default',
    whiteSpace: 'nowrap',
  },
});

/** 控制中心画质选项（fps 上限 Rust 侧已放宽到 60） */
const QUALITY_OPTIONS: Array<{ key: 'low' | 'medium' | 'high'; label: string; fps: number }> = [
  { key: 'low', label: '流畅（30fps）', fps: 30 },
  { key: 'medium', label: '均衡（60fps）', fps: 60 },
  { key: 'high', label: '高清（60fps）', fps: 60 },
];

/** 控制中心分辨率选项 */
const RESOLUTION_OPTIONS: Array<{ key: '1920x1080' | '2560x1440' | '3840x2160'; label: string; width: number; height: number }> = [
  { key: '1920x1080', label: '1920 x 1080', width: 1920, height: 1080 },
  { key: '2560x1440', label: '2560 x 1440', width: 2560, height: 1440 },
  { key: '3840x2160', label: '3840 x 2160', width: 3840, height: 2160 },
];

function formatElapsed(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  return [h, m, s].map((v) => String(v).padStart(2, '0')).join(':');
}

function resolutionLabel(display: MonitorInfo | null): string {
  if (!display) return '--';
  if (display.height >= 2160) return '4K';
  if (display.height >= 1440) return '2K';
  return '1080P';
}

interface RemoteSessionViewProps {
  deviceName: string;
  connected: boolean;
  onExit?: () => void;
  onOpenVirtualDisplays?: () => void;
  /** 独立窗口模式:顶栏变为无边框窗口标题栏(拖拽区 + 自绘最小化/最大化/关闭) */
  standalone?: boolean;
}

/**
 * 远程会话窗口：深色顶部栏（设备名胶囊 + 超级屏 + 信号 + 计时 / 显示屏下拉 /
 * 麦克风 + 控制中心 + 断开连接）+ 全屏远程画面 + 右下角性能浮窗。
 * standalone 模式下顶栏兼任无边框窗口标题栏（可拖拽移动窗口）。
 */
export const RemoteSessionView: React.FC<RemoteSessionViewProps> = ({
  deviceName,
  connected,
  onExit,
  onOpenVirtualDisplays,
  standalone = false,
}) => {
  const styles = useStyles();
  const [elapsed, setElapsed] = useState(0);
  const [micMuted, setMicMuted] = useState(false);
  const [centerOpen, setCenterOpen] = useState(false);
  const [displayMenuOpen, setDisplayMenuOpen] = useState(false);
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [displayId, setDisplayId] = useState(1);
  const [fullscreen, setFullscreen] = useState(false);
  const [fps, setFps] = useState(0);
  const [bitrate, setBitrate] = useState('0.0');
  const [lossPct, setLossPct] = useState(0);
  /** 平均编码耗时（毫秒）；UDP 模式无 dur → null 显示 "--"（F-4：禁止造假为 0） */
  const [avgEncodeMs, setAvgEncodeMs] = useState<number | null>(null);
  const [metrics, setMetrics] = useState<SessionMetrics | null>(null);
  const [qualityChoice, setQualityChoice] = useState<'low' | 'medium' | 'high'>('high');
  const [resolutionChoice, setResolutionChoice] = useState<'1920x1080' | '2560x1440' | '3840x2160'>('1920x1080');
  const [notice, setNotice] = useState<string | null>(null);
  const timerRef = useRef<number | null>(null);
  const frameCountRef = useRef(0);
  const bitsRef = useRef(0);
  // 丢包统计（F-4 纯函数）：seq 连续性口径——UDP 域为 frame_id、TCP 域为推流 seq，
  // 传输模式切换时基线重置豁免（resets），否则产生虚假丢包尖峰
  const lostRef = useRef<LossStats>(createLossStats());
  const lastFrameRef = useRef<LossBaseline | null>(null);
  // 编码耗时统计（F-4）：仅累计 dur 非空的帧（UDP 模式 dur=null 不计入，不造假）
  const durSumRef = useRef(0);
  const durCountRef = useRef(0);
  // 会话指标镜像（帧回调内读取最新 transport，避免 effect 依赖重建订阅）
  const metricsRef = useRef<SessionMetrics | null>(null);
  useEffect(() => {
    metricsRef.current = metrics;
  }, [metrics]);

  useEffect(() => {
    timerRef.current = window.setInterval(() => setElapsed((prev) => prev + 1), 1000);
    return () => {
      if (timerRef.current) window.clearInterval(timerRef.current);
    };
  }, []);

  // 剪贴板同步通知：3 秒后自动清除
  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => setNotice(null), 3000);
    return () => window.clearTimeout(timer);
  }, [notice]);

  const selected = monitors.find((m) => m.id === displayId) ?? monitors[0] ?? null;

  // 远程显示器列表：连接建立时重置为空，仅请求远程真实列表并订阅（不再用本机显示器兜底）
  useEffect(() => {
    if (!connected) return;
    setMonitors([]);
    setDisplayId(1);
    let unlisten: (() => void) | undefined;
    void requestRemoteMonitors();
    void onRemoteMonitors((ms) => {
      if (ms.length > 0) setMonitors(ms);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [connected]);

  // 音频静音状态：连接后读取当前状态并订阅事件回执，前后端双源同步静音按钮
  useEffect(() => {
    if (!connected) return;
    let unlisten: (() => void) | undefined;
    void getAudioMuted().then((muted) => setMicMuted(muted));
    void onAudioStateChange((muted) => setMicMuted(muted)).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [connected]);

  // 统计实时帧率/码率/丢包率：基于 remote-frame 事件计数（与 RemoteCanvas 内部的帧订阅互不冲突）。
  useEffect(() => {
    if (!connected) return;
    frameCountRef.current = 0;
    bitsRef.current = 0;
    lostRef.current = createLossStats();
    lastFrameRef.current = null;
    durSumRef.current = 0;
    durCountRef.current = 0;
    setLossPct(0);
    setAvgEncodeMs(null);
    let unlisten: (() => void) | undefined;
    void onRemoteFrame((frame) => {
      frameCountRef.current += 1;
      // 码率累计：编码帧字节（H.264/H.265 Annex-B）* 8 换算为 bit
      bitsRef.current += frame.data.length * 8;
      // 编码耗时累计（仅真实值；UDP 模式 dur=null 不计入，F-4 禁止造假）
      if (frame.dur != null) {
        durSumRef.current += frame.dur;
        durCountRef.current += 1;
      }
      // 丢包统计（F-4 纯函数 + R2-B 帧级标记）：优先用帧自带 transport（当帧
      // 真实来源，消除 metrics 2 秒轮询滞后窗口的跨域错标）；字段缺失（旧负载）
      // 回退 metrics 轮询值；两者皆无按 tcp
      const transport = frame.transport ?? metricsRef.current?.transport ?? 'tcp';
      lastFrameRef.current = feedLossStats(
        lostRef.current,
        frame.seq,
        transport,
        lastFrameRef.current,
      );
    }).then((fn) => {
      unlisten = fn;
    });
    const interval = window.setInterval(() => {
      setFps(frameCountRef.current);
      setBitrate((bitsRef.current / 1e6).toFixed(1));
      frameCountRef.current = 0;
      bitsRef.current = 0;
      const total = lostRef.current.lost + lostRef.current.received;
      setLossPct(total > 0 ? (lostRef.current.lost / total) * 100 : 0);
      // 平均编码耗时：无真实样本时保持 null（浮窗显示 "--"，禁止显示假 0）
      setAvgEncodeMs(durCountRef.current > 0 ? durSumRef.current / durCountRef.current : null);
      durSumRef.current = 0;
      durCountRef.current = 0;
    }, 1000);
    return () => {
      unlisten?.();
      window.clearInterval(interval);
    };
  }, [connected]);

  // 连接模式与延迟：每 2 秒轮询真实会话指标（rttMs 由控制端主动 Ping 心跳更新）
  useEffect(() => {
    if (!connected) return;
    let alive = true;
    const poll = async () => {
      const m = await getSessionMetrics();
      if (alive && m) setMetrics(m);
    };
    void poll();
    const interval = window.setInterval(() => void poll(), 2000);
    return () => {
      alive = false;
      window.clearInterval(interval);
    };
  }, [connected]);

  // 订阅对端剪贴板同步事件（对端复制内容自动通知）
  useEffect(() => {
    if (!connected) return;
    let unlisten: (() => void) | undefined;
    void onClipboardSynced((text) => {
      setNotice(`对端剪贴板已同步:${text.slice(0, 20)}…`);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [connected]);

  const handleCenterAction = useCallback((fn: () => void) => {
    fn();
    setCenterOpen(false);
  }, []);

  return (
    <div className={styles.session}>
      <div className={styles.bar} data-tauri-drag-region={standalone ? 'deep' : undefined}>
        <div className={styles.barLeft}>
          <span className={styles.devicePill} data-tauri-drag-region={standalone ? 'false' : undefined}>
            <span className={styles.devicePillIcon}>
              <DesktopRegular fontSize={11} />
            </span>
            {deviceName}
          </span>
          <span className={styles.superScreen}>超级屏</span>
          <span className={styles.signalGroup}>
            <span className={styles.signalDot} />
            <Wifi3Regular fontSize={12} />
            <span className={styles.timer}>{formatElapsed(elapsed)}</span>
          </span>
        </div>

        <div className={styles.barCenter} data-tauri-drag-region={standalone ? 'false' : undefined}>
          <div
            className={styles.displayTab}
            onClick={() => {
              if (selected) setDisplayMenuOpen((prev) => !prev);
            }}
          >
            <DesktopRegular fontSize={14} />
            {selected ? selected.name || `显示屏 ${monitors.indexOf(selected) + 1}` : '正在获取远程显示器…'}
            <ChevronDownRegular fontSize={12} />
          </div>
          {onOpenVirtualDisplays && (
            <button
              type="button"
              className={styles.addBtn}
              aria-label="添加显示屏"
              onClick={onOpenVirtualDisplays}
            >
              <AddRegular fontSize={16} />
            </button>
          )}
          {!onOpenVirtualDisplays && standalone && <span style={{ width: '26px' }} />}
        </div>

        <div className={styles.barRight}>
          <button
            type="button"
            className={styles.iconBtn}
            data-tauri-drag-region={standalone ? 'false' : undefined}
            onClick={() => {
              // 控制端静音远程回传的系统声音（真实调用 set_audio_muted 命令）
              const next = !micMuted;
              setMicMuted(next);
              void setAudioMuted(next);
            }}
            aria-label={micMuted ? '取消静音' : '静音'}
          >
            {micMuted ? <MicOffRegular fontSize={16} /> : <MicRegular fontSize={16} />}
          </button>
          <button
            type="button"
            className={styles.centerBtn}
            data-tauri-drag-region={standalone ? 'false' : undefined}
            onClick={() => setCenterOpen((prev) => !prev)}
          >
            <SettingsRegular fontSize={15} />
            控制中心
          </button>
          <button
            type="button"
            className={styles.disconnectBtn}
            data-tauri-drag-region={standalone ? 'false' : undefined}
            onClick={onExit}
          >
            断开连接
          </button>
          {standalone && (
            <span data-tauri-drag-region="false" style={{ display: 'inline-flex', height: '100%', marginLeft: '4px' }}>
              <WindowControls />
            </span>
          )}
        </div>
      </div>

      <div className={styles.canvasArea}>
        <RemoteCanvas connected={connected} remoteWidth={selected?.width ?? 1280} remoteHeight={selected?.height ?? 720} mode="canvas" streamSource="remote" />

        <div className={styles.perfOverlay}>
          <div className={styles.perfTitle}>
            <span>连接模式: {metrics?.mode ?? '--'}</span>
            <span>{resolutionLabel(selected)}</span>
          </div>
          <div className={styles.perfRow}>
            <span>传输模式:</span>
            <span className={styles.perfVal}>{transportLabel(metrics?.transport)}</span>
          </div>
          <div className={styles.perfRow}>
            <span>帧率:</span>
            <span className={styles.perfVal}>{fps > 0 ? `${fps} fps` : '-- fps'}</span>
          </div>
          <div className={styles.perfRow}>
            <span>码率:</span>
            <span className={styles.perfVal}>{bitrate} Mbps</span>
          </div>
          <div className={styles.perfRow}>
            <span>延迟:</span>
            <span className={styles.perfVal}>{metrics?.rttMs != null ? `${metrics.rttMs} ms` : '-- ms'}</span>
          </div>
          <div className={styles.perfRow}>
            <span>编码耗时:</span>
            <span className={styles.perfVal}>{avgEncodeMs != null ? `${avgEncodeMs.toFixed(1)} ms` : '-- ms'}</span>
          </div>
          <div className={styles.perfRow}>
            <span>丢包率:</span>
            <span className={styles.perfVal}>{lossPct.toFixed(1)}% loss</span>
          </div>
        </div>

        {notice && <div className={styles.notice}>{notice}</div>}
      </div>

      {displayMenuOpen && (
        <div className={styles.displayMenu}>
          {monitors.length === 0 ? (
            <div className={styles.displayMenuEmpty}>暂无远程显示器信息</div>
          ) : (
            monitors.map((monitor) => (
              <div
                key={monitor.id}
                className={monitor.id === displayId ? `${styles.displayMenuItem} ${styles.displayMenuItemActive}` : styles.displayMenuItem}
                onClick={() => {
                  // 切换目标显示器：下发 select_session_monitor 到被控端实时切换抓帧
                  setDisplayId(monitor.id);
                  void selectSessionMonitor(monitor.id);
                  setDisplayMenuOpen(false);
                }}
              >
                {monitor.name || `显示屏 ${monitors.indexOf(monitor) + 1}`} · {monitor.width}x{monitor.height}
              </div>
            ))
          )}
        </div>
      )}

      {centerOpen && (
        <div className={styles.centerPanel}>
          <div className={styles.centerTitle}>控制中心</div>
          <div
            className={styles.centerItem}
            onClick={() =>
              handleCenterAction(() => {
                void requestFullscreen(!fullscreen);
                setFullscreen((prev) => !prev);
              })
            }
          >
            <span className={styles.centerItemIcon}>
              <FullScreenMaximizeRegular fontSize={16} />
            </span>
            全屏切换 {fullscreen ? '(开)' : '(关)'}
          </div>
          {QUALITY_OPTIONS.map((opt) => (
            <div
              key={opt.key}
              className={styles.centerItem}
              onClick={() =>
                handleCenterAction(() => {
                  setQualityChoice(opt.key);
                  void setQuality({ fps: opt.fps, quality: opt.key });
                })
              }
            >
              <span className={styles.centerItemIcon}>
                <VideoRegular fontSize={16} />
              </span>
              画质：{opt.label}
              {qualityChoice === opt.key ? '（当前）' : ''}
            </div>
          ))}
          {RESOLUTION_OPTIONS.map((opt) => (
            <div
              key={opt.key}
              className={styles.centerItem}
              onClick={() =>
                handleCenterAction(() => {
                  setResolutionChoice(opt.key);
                  void setResolution({ width: opt.width, height: opt.height, fps: 60 });
                })
              }
            >
              <span className={styles.centerItemIcon}>
                <ImageRegular fontSize={16} />
              </span>
              分辨率：{opt.label}
              {resolutionChoice === opt.key ? '（当前）' : ''}
            </div>
          ))}
          <div
            className={styles.centerItem}
            onClick={() =>
              handleCenterAction(() => {
                void syncClipboard().then((text) => {
                  if (text) setNotice(`剪贴板已同步:${text.slice(0, 20)}…`);
                });
              })
            }
          >
            <span className={styles.centerItemIcon}>
              <ClipboardRegular fontSize={16} />
            </span>
            剪贴板同步
          </div>
          <div
            className={styles.centerItemDisabled}
            onClick={() => handleCenterAction(() => setNotice('键盘输入设置暂未实现'))}
          >
            <span className={styles.centerItemIcon}>
              <KeyboardRegular fontSize={16} />
            </span>
            键盘输入设置
          </div>
        </div>
      )}
    </div>
  );
};

export default RemoteSessionView;
