import React, { useCallback, useEffect, useRef, useState } from 'react';
import { makeStyles } from '@fluentui/react-components';
import {
  ShieldRegular,
  MicRegular,
  MicOffRegular,
  AddRegular,
  ChevronDownRegular,
  VideoRegular,
  FullScreenMaximizeRegular,
  ClipboardRegular,
  SettingsRegular,
  ImageRegular,
  ArrowLeftRegular,
  DesktopRegular,
  KeyboardRegular,
} from '@fluentui/react-icons';
import { palette, fontFamily, radius, zIndex } from '../theme/tokens';
import RemoteCanvas from './RemoteCanvas';

const useStyles = makeStyles({
  session: {
    height: '100%',
    display: 'flex',
    flexDirection: 'column',
    backgroundColor: '#1C2733',
    position: 'relative',
    overflow: 'hidden',
  },
  bar: {
    height: '40px',
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '0 8px',
    backgroundColor: 'rgba(38, 47, 58, 0.95)',
    borderBottom: '1px solid rgba(255,255,255,0.08)',
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
  backBtn: {
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
    },
  },
  appIcon: {
    width: '18px',
    height: '18px',
    borderRadius: '5px',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    flexShrink: 0,
  },
  deviceName: {
    fontFamily,
    fontSize: '14px',
    fontWeight: 600,
    color: '#F2F5F8',
    whiteSpace: 'nowrap',
  },
  shield: {
    display: 'inline-flex',
    color: '#6BB7FF',
    flexShrink: 0,
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
  signal: {
    width: '8px',
    height: '8px',
    borderRadius: '50%',
    backgroundColor: '#34C759',
  },
  timer: {
    fontFamily,
    fontSize: '12px',
    color: '#C9D2DD',
    fontVariantNumeric: 'tabular-nums',
    marginLeft: '4px',
  },
  barRight: {
    display: 'flex',
    alignItems: 'center',
    gap: '4px',
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
  canvasArea: {
    flex: 1,
    position: 'relative',
    minHeight: 0,
  },
  perfOverlay: {
    position: 'absolute',
    right: '14px',
    bottom: '14px',
    backgroundColor: 'rgba(15, 23, 32, 0.78)',
    borderRadius: '8px',
    padding: '10px 14px',
    fontFamily,
    fontSize: '12px',
    lineHeight: '20px',
    color: '#D6DEE6',
    backdropFilter: 'blur(4px)',
    zIndex: 5,
    fontVariantNumeric: 'tabular-nums',
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

interface RemoteSessionViewProps {
  deviceName: string;
  connected: boolean;
  onExit?: () => void;
}

/**
 * 截图「远程控制窗口」界面：深色会话栏（设备名/盾牌/显示屏标签/计时/麦克风/控制中心）+
 * 全屏远程画面 + 右下角性能状态浮窗。
 */
export const RemoteSessionView: React.FC<RemoteSessionViewProps> = ({ deviceName, connected, onExit }) => {
  const styles = useStyles();
  const [elapsed, setElapsed] = useState(0);
  const [micMuted, setMicMuted] = useState(false);
  const [centerOpen, setCenterOpen] = useState(false);
  const [displayMenuOpen, setDisplayMenuOpen] = useState(false);
  const [displayId, setDisplayId] = useState('1');
  const [fullscreen, setFullscreen] = useState(false);
  const timerRef = useRef<number | null>(null);

  useEffect(() => {
    timerRef.current = window.setInterval(() => setElapsed((prev) => prev + 1), 1000);
    return () => {
      if (timerRef.current) window.clearInterval(timerRef.current);
    };
  }, []);

  const selected = DISPLAYS.find((d) => d.id === displayId) ?? DISPLAYS[0];

  const handleCenterAction = useCallback((fn: () => void) => {
    fn();
    setCenterOpen(false);
  }, []);

  return (
    <div className={styles.session}>
      <div className={styles.bar}>
        <div className={styles.barLeft}>
          {onExit && (
            <button type="button" className={styles.backBtn} onClick={onExit} aria-label="退出远程会话">
              <ArrowLeftRegular fontSize={16} />
            </button>
          )}
          <span className={styles.appIcon}>
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" aria-hidden>
              <rect x="4" y="4" width="16" height="16" rx="4" fill={palette.primary} />
              <path d="M9 12h6M12 9v6" stroke="#fff" strokeWidth="2" strokeLinecap="round" />
            </svg>
          </span>
          <span className={styles.deviceName}>{deviceName}</span>
          <span className={styles.shield}>
            <ShieldRegular fontSize={16} />
          </span>
        </div>

        <div className={styles.barCenter}>
          <div className={styles.displayTab} onClick={() => setDisplayMenuOpen((prev) => !prev)}>
            <DesktopRegular fontSize={14} />
            {selected.label}
            <ChevronDownRegular fontSize={12} />
          </div>
          <button type="button" className={styles.addBtn} aria-label="添加显示屏">
            <AddRegular fontSize={16} />
          </button>
          <span className={styles.signal} title="连接正常" />
          <span className={styles.timer}>{formatElapsed(elapsed)}</span>
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
        </div>
      </div>

      <div className={styles.canvasArea}>
        <RemoteCanvas connected={connected} remoteWidth={selected.width} remoteHeight={selected.height} mode="canvas" />

        <div className={styles.perfOverlay}>
          <div className={styles.perfRow}>
            <span>UDP relay</span>
            <span className={styles.perfVal}>{formatElapsed(elapsed)}</span>
          </div>
          <div className={styles.perfRow}>
            <span>帧率</span>
            <span className={styles.perfVal}>28 fps</span>
          </div>
          <div className={styles.perfRow}>
            <span>码率</span>
            <span className={styles.perfVal}>3.0 Mbps</span>
          </div>
          <div className={styles.perfRow}>
            <span>网络延迟</span>
            <span className={styles.perfVal}>10 ms</span>
          </div>
          <div className={styles.perfRow}>
            <span>帧耗时</span>
            <span className={styles.perfVal}>15 ms frm.</span>
          </div>
          <div className={styles.perfRow}>
            <span>丢包</span>
            <span className={styles.perfVal}>0.08 loss</span>
          </div>
          <div className={styles.perfRow}>
            <span>分辨率</span>
            <span className={styles.perfVal}>4K</span>
          </div>
        </div>
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
          <div className={styles.centerItem} onClick={() => handleCenterAction(() => setFullscreen((prev) => !prev))}>
            <span className={styles.centerItemIcon}>
              <FullScreenMaximizeRegular fontSize={16} />
            </span>
            全屏切换 {fullscreen ? '(开)' : '(关)'}
          </div>
          <div className={styles.centerItem} onClick={() => handleCenterAction(() => {})}>
            <span className={styles.centerItemIcon}>
              <VideoRegular fontSize={16} />
            </span>
            画质：高清（60fps）
          </div>
          <div className={styles.centerItem} onClick={() => handleCenterAction(() => {})}>
            <span className={styles.centerItemIcon}>
              <ImageRegular fontSize={16} />
            </span>
            分辨率：3840 x 2160
          </div>
          <div className={styles.centerItem} onClick={() => handleCenterAction(() => {})}>
            <span className={styles.centerItemIcon}>
              <ClipboardRegular fontSize={16} />
            </span>
            剪贴板同步
          </div>
          <div className={styles.centerItem} onClick={() => handleCenterAction(() => {})}>
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
