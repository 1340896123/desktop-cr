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
import { fontFamily, radius, zIndex } from '../theme/tokens';
import {
  setFullscreen as requestFullscreen,
  setQuality,
  setResolution,
  syncClipboard,
  onClipboardSynced,
} from '../services/connection';
import { onRemoteFrame, listMonitors, type MonitorInfo } from '../services/capture';
import { getSessionMetrics, requestRemoteMonitors, selectSessionMonitor, onRemoteMonitors, type SessionMetrics } from '../services/session';
import { setAudioMuted } from '../services/audio';
import RemoteCanvas from './RemoteCanvas';

const useStyles = makeStyles({
  session: {
    height: '100%',
    display: 'flex',
    flexDirection: 'column',
    backgroundColor: '#0f172a',
    position: 'relative',
    overflow: 'hidden',
  },
  bar: {
    height: '40px',
    display: 'flex',
    alignItems: 'center',
    gap: '12px',
    padding: '0 14px',
    backgroundColor: 'rgba(30, 41, 59, 0.95)',
    borderBottom: '1px solid rgba(51, 65, 85, 0.5)',
    userSelect: 'none',
    zIndex: zIndex.titleBar,
    flexShrink: 0,
    backdropFilter: 'blur(4px)',
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
    borderRadius: '6px',
    backgroundColor: '#334155',
    color: '#F8FAFC',
    fontFamily,
    fontSize: '12px',
    fontWeight: 700,
    whiteSpace: 'nowrap',
  },
  devicePillIcon: {
    display: 'flex',
    color: '#60A5FA',
  },
  superScreen: {
    display: 'inline-flex',
    alignItems: 'center',
    padding: '2px 8px',
    borderRadius: '4px',
    backgroundColor: 'rgba(37, 99, 235, 0.8)',
    color: '#ffffff',
    fontFamily,
    fontSize: '10px',
    whiteSpace: 'nowrap',
  },
  signalGroup: {
    display: 'flex',
    alignItems: 'center',
    gap: '6px',
    color: '#34D399',
  },
  signalDot: {
    width: '8px',
    height: '8px',
    borderRadius: '50%',
    backgroundColor: '#34D399',
  },
  timer: {
    fontFamily,
    fontSize: '12px',
    color: '#CBD5E1',
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
    backgroundColor: '#F2F5F8',
    color: '#1C2733',
    borderRadius: '6px',
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
    color: '#C9D2DD',
    cursor: 'pointer',

    '&:hover': {
      backgroundColor: 'rgba(255,255,255,0.1)',
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
    color: '#C9D2DD',
    cursor: 'pointer',

    '&:hover': {
      backgroundColor: 'rgba(255,255,255,0.1)',
      color: '#F2F5F8',
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
    color: '#C9D2DD',
    fontFamily,
    fontSize: '12px',
    cursor: 'pointer',

    '&:hover': {
      backgroundColor: 'rgba(255,255,255,0.1)',
      color: '#F2F5F8',
    },
  },
  disconnectBtn: {
    display: 'inline-flex',
    alignItems: 'center',
    padding: '5px 14px',
    borderRadius: '6px',
    border: 'none',
    backgroundColor: '#DC2626',
    color: '#ffffff',
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
  },
  perfOverlay: {
    position: 'absolute',
    right: '14px',
    bottom: '14px',
    backgroundColor: 'rgba(15, 23, 32, 0.9)',
    borderRadius: '8px',
    padding: '10px 14px',
    fontFamily,
    fontSize: '11px',
    lineHeight: '20px',
    color: '#CBD5E1',
    backdropFilter: 'blur(4px)',
    border: '1px solid rgba(51, 65, 85, 0.8)',
    zIndex: 5,
    fontVariantNumeric: 'tabular-nums',
    minWidth: '220px',
  },
  perfTitle: {
    display: 'flex',
    justifyContent: 'space-between',
    color: '#34D399',
    fontWeight: 700,
    borderBottom: '1px solid rgba(30, 41, 59, 0.9)',
    paddingBottom: '4px',
    marginBottom: '4px',
  },
  perfRow: {
    display: 'flex',
    justifyContent: 'space-between',
    gap: '16px',
  },
  perfVal: {
    color: '#FFFFFF',
    fontWeight: 500,
    textAlign: 'right',
  },
  notice: {
    position: 'absolute',
    right: '14px',
    bottom: '184px',
    backgroundColor: 'rgba(15, 23, 32, 0.78)',
    borderRadius: '8px',
    padding: '8px 14px',
    fontFamily,
    fontSize: '12px',
    lineHeight: '18px',
    color: '#FFFFFF',
    backdropFilter: 'blur(4px)',
    zIndex: 6,
    maxWidth: '320px',
  },
  centerPanel: {
    position: 'absolute',
    top: '48px',
    right: '12px',
    width: '280px',
    backgroundColor: '#232B36',
    borderRadius: '10px',
    border: '1px solid rgba(255,255,255,0.1)',
    boxShadow: '0 8px 24px rgba(0,0,0,0.35)',
    padding: '8px',
    zIndex: 20,
  },
  centerTitle: {
    fontFamily,
    fontSize: '13px',
    fontWeight: 600,
    color: '#F2F5F8',
    padding: '6px 10px 8px',
    borderBottom: '1px solid rgba(255,255,255,0.08)',
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
    color: '#C9D2DD',
    cursor: 'pointer',

    '&:hover': {
      backgroundColor: 'rgba(255,255,255,0.08)',
      color: '#F2F5F8',
    },
  },
  centerItemIcon: {
    display: 'flex',
    color: '#6BB7FF',
  },
  displayMenu: {
    position: 'absolute',
    top: '46px',
    left: '50%',
    transform: 'translateX(-50%)',
    backgroundColor: '#232B36',
    borderRadius: '10px',
    border: '1px solid rgba(255,255,255,0.1)',
    boxShadow: '0 8px 24px rgba(0,0,0,0.35)',
    padding: '6px',
    zIndex: 20,
    minWidth: '160px',
  },
  displayMenuItem: {
    padding: '7px 12px',
    borderRadius: '6px',
    fontFamily,
    fontSize: '13px',
    color: '#C9D2DD',
    cursor: 'pointer',

    '&:hover': {
      backgroundColor: 'rgba(255,255,255,0.08)',
    },
  },
  displayMenuItemActive: {
    color: '#FFFFFF',
    fontWeight: 600,
    backgroundColor: 'rgba(107, 183, 255, 0.15)',
  },
});

/** 远程显示器列表为空时的降级默认屏（浏览器 / 非 Windows 下界面不崩） */
const DEFAULT_MONITORS: MonitorInfo[] = [
  { id: 1, name: '默认显示屏', width: 1920, height: 1080, isPrimary: true, isVirtual: false },
];

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

function resolutionLabel(display: MonitorInfo): string {
  if (display.height >= 2160) return '4K';
  if (display.height >= 1440) return '2K';
  return '1080P';
}

interface RemoteSessionViewProps {
  deviceName: string;
  connected: boolean;
  onExit?: () => void;
  onOpenVirtualDisplays?: () => void;
}

/**
 * 远程会话窗口：深色顶部栏（设备名胶囊 + 超级屏 + 信号 + 计时 / 显示屏下拉 /
 * 麦克风 + 控制中心 + 断开连接）+ 全屏远程画面 + 右下角性能浮窗。
 */
export const RemoteSessionView: React.FC<RemoteSessionViewProps> = ({ deviceName, connected, onExit, onOpenVirtualDisplays }) => {
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
  const [metrics, setMetrics] = useState<SessionMetrics | null>(null);
  const [qualityChoice, setQualityChoice] = useState<'low' | 'medium' | 'high'>('high');
  const [resolutionChoice, setResolutionChoice] = useState<'1920x1080' | '2560x1440' | '3840x2160'>('1920x1080');
  const [notice, setNotice] = useState<string | null>(null);
  const timerRef = useRef<number | null>(null);
  const frameCountRef = useRef(0);
  const bitsRef = useRef(0);
  const lastSeqRef = useRef(-1);
  const lostRef = useRef(0);
  const receivedRef = useRef(0);

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

  const effectiveMonitors = monitors.length > 0 ? monitors : DEFAULT_MONITORS;
  const selected = effectiveMonitors.find((m) => m.id === displayId) ?? effectiveMonitors[0];

  // 远程显示器列表：连接后先用本机显示器兜底展示，再请求远程真实列表并订阅
  useEffect(() => {
    if (!connected) return;
    let unlisten: (() => void) | undefined;
    void listMonitors().then((ms) => {
      if (ms.length > 0) setMonitors(ms);
    });
    void requestRemoteMonitors();
    void onRemoteMonitors((ms) => {
      if (ms.length > 0) setMonitors(ms);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [connected]);

  // 统计实时帧率/码率/丢包率：基于 remote-frame 事件计数（与 RemoteCanvas 内部的帧订阅互不冲突）。
  useEffect(() => {
    if (!connected) return;
    frameCountRef.current = 0;
    bitsRef.current = 0;
    lastSeqRef.current = -1;
    lostRef.current = 0;
    receivedRef.current = 0;
    setLossPct(0);
    let unlisten: (() => void) | undefined;
    void onRemoteFrame((frame) => {
      frameCountRef.current += 1;
      // 码率累计：JPEG 字节数 * 8 换算为 bit
      bitsRef.current += frame.jpeg.length * 8;
      // 丢包统计：按 seq 连续性计数（seq 回绕时重置基准）
      if (lastSeqRef.current >= 0) {
        if (frame.seq > lastSeqRef.current + 1) {
          lostRef.current += frame.seq - lastSeqRef.current - 1;
        } else if (frame.seq < lastSeqRef.current) {
          lastSeqRef.current = frame.seq;
          return;
        }
      }
      lastSeqRef.current = frame.seq;
      receivedRef.current += 1;
    }).then((fn) => {
      unlisten = fn;
    });
    const interval = window.setInterval(() => {
      setFps(frameCountRef.current);
      setBitrate((bitsRef.current / 1e6).toFixed(1));
      frameCountRef.current = 0;
      bitsRef.current = 0;
      const total = lostRef.current + receivedRef.current;
      setLossPct(total > 0 ? (lostRef.current / total) * 100 : 0);
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
      <div className={styles.bar}>
        <div className={styles.barLeft}>
          <span className={styles.devicePill}>
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

        <div className={styles.barCenter}>
          <div className={styles.displayTab} onClick={() => setDisplayMenuOpen((prev) => !prev)}>
            <DesktopRegular fontSize={14} />
            {selected.name || `显示屏 ${effectiveMonitors.indexOf(selected) + 1}`}
            <ChevronDownRegular fontSize={12} />
          </div>
          <button
            type="button"
            className={styles.addBtn}
            aria-label="添加显示屏"
            onClick={onOpenVirtualDisplays}
          >
            <AddRegular fontSize={16} />
          </button>
        </div>

        <div className={styles.barRight}>
          <button
            type="button"
            className={styles.iconBtn}
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
          <button type="button" className={styles.centerBtn} onClick={() => setCenterOpen((prev) => !prev)}>
            <SettingsRegular fontSize={15} />
            控制中心
          </button>
          <button type="button" className={styles.disconnectBtn} onClick={onExit}>
            断开连接
          </button>
        </div>
      </div>

      <div className={styles.canvasArea}>
        <RemoteCanvas connected={connected} remoteWidth={selected.width} remoteHeight={selected.height} mode="canvas" streamSource="remote" />

        <div className={styles.perfOverlay}>
          <div className={styles.perfTitle}>
            <span>连接模式: {metrics?.mode ?? '--'}</span>
            <span>{resolutionLabel(selected)}</span>
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
            <span>丢包率:</span>
            <span className={styles.perfVal}>{lossPct.toFixed(1)}% loss</span>
          </div>
        </div>

        {notice && <div className={styles.notice}>{notice}</div>}
      </div>

      {displayMenuOpen && (
        <div className={styles.displayMenu}>
          {effectiveMonitors.map((monitor) => (
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
              {monitor.name || `显示屏 ${effectiveMonitors.indexOf(monitor) + 1}`} · {monitor.width}x{monitor.height}
            </div>
          ))}
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
            className={styles.centerItem}
            onClick={() =>
              handleCenterAction(() => {
                console.info('[session] 键盘输入设置：POC 阶段占位');
              })
            }
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