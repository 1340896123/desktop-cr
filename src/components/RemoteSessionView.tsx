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
import { setFullscreen as requestFullscreen, setQuality, setResolution, syncClipboard } from '../services/connection';
import { onRemoteFrame } from '../services/capture';
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

interface DisplayOption {
  id: string;
  label: string;
  width: number;
  height: number;
  fps: number;
}

const DISPLAYS: DisplayOption[] = [
  { id: '1', label: '显示屏 1', width: 3840, height: 2160, fps: 60 },
  { id: '2', label: '显示屏 2', width: 1920, height: 1080, fps: 60 },
  { id: '3', label: '显示屏 3', width: 2560, height: 1440, fps: 60 },
];

function formatElapsed(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  return [h, m, s].map((v) => String(v).padStart(2, '0')).join(':');
}

function resolutionLabel(display: DisplayOption): string {
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
  const [displayId, setDisplayId] = useState('1');
  const [fullscreen, setFullscreen] = useState(false);
  const [fps, setFps] = useState(0);
  const [notice, setNotice] = useState<string | null>(null);
  const timerRef = useRef<number | null>(null);
  const frameCountRef = useRef(0);

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

  const selected = DISPLAYS.find((d) => d.id === displayId) ?? DISPLAYS[0];

  // 统计实时帧率：基于 remote-frame 事件计数（与 RemoteCanvas 内部的帧订阅互不冲突）。
  useEffect(() => {
    if (!connected) return;
    frameCountRef.current = 0;
    let unlisten: (() => void) | undefined;
    void onRemoteFrame(() => {
      frameCountRef.current += 1;
    }).then((fn) => {
      unlisten = fn;
    });
    const interval = window.setInterval(() => {
      setFps(frameCountRef.current);
      frameCountRef.current = 0;
    }, 1000);
    return () => {
      unlisten?.();
      window.clearInterval(interval);
    };
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
            {selected.label}
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
            onClick={() => setMicMuted((prev) => !prev)}
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
            <span>连接模式: UDP P2P</span>
            <span>{resolutionLabel(selected)}</span>
          </div>
          <div className={styles.perfRow}>
            <span>帧率:</span>
            <span className={styles.perfVal}>{fps > 0 ? `${fps} fps` : '-- fps'}</span>
          </div>
          <div className={styles.perfRow}>
            <span>码率:</span>
            <span className={styles.perfVal}>2.7 Mbps</span>
          </div>
          <div className={styles.perfRow}>
            <span>延迟:</span>
            <span className={styles.perfVal}>2 ms</span>
          </div>
          <div className={styles.perfRow}>
            <span>丢包率:</span>
            <span className={styles.perfVal}>0.0% loss</span>
          </div>
        </div>

        {notice && <div className={styles.notice}>{notice}</div>}
      </div>

      {displayMenuOpen && (
        <div className={styles.displayMenu}>
          {DISPLAYS.map((display) => (
            <div
              key={display.id}
              className={display.id === displayId ? `${styles.displayMenuItem} ${styles.displayMenuItemActive}` : styles.displayMenuItem}
              onClick={() => {
                setDisplayId(display.id);
                setDisplayMenuOpen(false);
              }}
            >
              {display.label} · {display.width}x{display.height}
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
          <div className={styles.centerItem} onClick={() => handleCenterAction(() => void setQuality({ fps: 60, quality: 'high' }))}>
            <span className={styles.centerItemIcon}>
              <VideoRegular fontSize={16} />
            </span>
            画质：高清（60fps）
          </div>
          <div className={styles.centerItem} onClick={() => handleCenterAction(() => void setResolution({ width: 3840, height: 2160, fps: 60 }))}>
            <span className={styles.centerItemIcon}>
              <ImageRegular fontSize={16} />
            </span>
            分辨率：3840 x 2160
          </div>
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