import React, { useEffect, useState } from 'react';
import { makeStyles } from '@fluentui/react-components';
import {
  RocketRegular,
  PowerRegular,
  SleepRegular,
  ArrowSyncRegular,
  FolderOpenRegular,
  DragRegular,
  ChevronDownRegular,
  DesktopRegular,
  LinkRegular,
  PlayRegular,
  StopRegular,
  PersonRegular,
  AddRegular,
  DeleteRegular,
  GlobeRegular,
  ServerRegular,
  KeyRegular,
  HeadsetRegular,
  FlashRegular,
} from '@fluentui/react-icons';
import { palette, fontFamily, spacing, radius, shadow } from '../theme/tokens';
import { startHost, stopHost, isHostRunning, onHostStateChange, type HostState } from '../services/connection';
import { getAppConfig, saveAppConfig, genPeerId, type AppConfig } from '../services/config';
import { getOperationLogs, type OperationLogEntry } from '../services/logs';
import { getIncomingDir } from '../services/fileTransfer';
import DxgiLoopbackCard from './DxgiLoopbackCard';

const useStyles = makeStyles({
  page: {
    flex: 1,
    height: '100%',
    overflowY: 'auto',
    padding: `${spacing.xl}px ${spacing.xxl}px`,
    display: 'flex',
    flexDirection: 'column',
    gap: `${spacing.md}px`,
    maxWidth: '840px',

    // 窄窗口:收窄页边距,给设置项留出横向空间
    '@media (max-width: 760px)': {
      padding: `${spacing.md}px ${spacing.lg}px`,
    },
    '@media (max-width: 560px)': {
      padding: `${spacing.sm}px ${spacing.md}px`,
    },
  },
  title: {
    fontFamily,
    fontSize: '24px',
    fontWeight: 700,
    color: palette.textPrimary,
    letterSpacing: '-0.02em',
    margin: 0,
    marginBottom: '4px',

    '@media (max-width: 560px)': {
      fontSize: '20px',
    },
  },
  tabs: {
    display: 'flex',
    gap: '24px',
    // 窄窗口:标签自动换行,避免横向溢出(gap 简写会重置 rowGap,故后置声明)
    rowGap: '0px',
    flexWrap: 'wrap',
    borderBottom: `1px solid ${palette.borderLight}`,
    marginBottom: '8px',

    '@media (max-width: 560px)': {
      gap: '16px',
      rowGap: '0px',
    },
  },
  tab: {
    position: 'relative',
    padding: '10px 2px',
    border: 'none',
    background: 'transparent',
    fontFamily,
    fontSize: '14px',
    color: palette.textSecondary,
    cursor: 'pointer',
    transition: 'color 150ms ease',

    '&:hover': {
      color: palette.textPrimary,
    },
  },
  tabActive: {
    color: palette.textPrimary,
    fontWeight: 600,
  },
  tabUnderline: {
    position: 'absolute',
    left: 0,
    right: 0,
    bottom: '-1px',
    height: '2px',
    borderRadius: '1px',
    backgroundColor: palette.primary,
  },
  card: {
    backgroundColor: palette.backgroundElevated,
    borderRadius: radius.card,
    boxShadow: shadow.card,
    border: `1px solid ${palette.borderLight}`,
    overflow: 'hidden',
  },
  row: {
    display: 'flex',
    alignItems: 'center',
    gap: '12px',
    padding: '12px 16px',
    minHeight: '48px',

    // 窄窗口:标题与控件改为上下堆叠,开关/按钮换到下一行右对齐
    '@media (max-width: 640px)': {
      flexWrap: 'wrap',
      alignItems: 'flex-start',
      rowGap: '8px',
    },
  },
  rowDivider: {
    height: '1px',
    backgroundColor: palette.borderLight,
    margin: '0 16px',
  },
  rowIcon: {
    display: 'flex',
    flexShrink: 0,
    color: palette.textPrimary,
  },
  rowBody: {
    flex: 1,
    minWidth: 0,

    // 窄窗口行内换行后,文本占满整行,控件独占下一行
    '@media (max-width: 640px)': {
      flexBasis: '100%',
    },
  },
  rowTitle: {
    fontFamily,
    fontSize: '14px',
    fontWeight: 600,
    color: palette.textPrimary,
  },
  rowDesc: {
    fontFamily,
    fontSize: '12px',
    color: palette.textMuted,
    marginTop: '2px',
    lineHeight: '18px',
  },
  link: {
    color: palette.primary,
    cursor: 'pointer',
    fontWeight: 500,

    '&:hover': {
      color: palette.primaryHover,
      textDecoration: 'underline',
    },
  },
  pathBox: {
    display: 'flex',
    alignItems: 'center',
    gap: '4px',
    minWidth: 0,

    // 窄窗口:路径框占满换行后的整行,长路径靠省略号截断
    '@media (max-width: 640px)': {
      flexBasis: '100%',
    },
  },
  pathInput: {
    flex: 1,
    minWidth: 0,
    height: '30px',
    padding: '0 10px',
    backgroundColor: palette.background,
    border: `1px solid ${palette.borderLight}`,
    borderRadius: radius.control,
    fontFamily,
    fontSize: '12px',
    color: palette.textSecondary,
    outline: 'none',
    transition: 'border-color 150ms ease',

    '&:focus': {
      border: `1px solid ${palette.primary}`,
    },
  },
  pathIconBtn: {
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
    flexShrink: 0,

    '&:hover': {
      backgroundColor: palette.muted,
      color: palette.textPrimary,
    },

    // 窄窗口行内换行后,操作按钮右对齐到下一行
    '@media (max-width: 640px)': {
      marginLeft: 'auto',
    },
  },
  selectWrap: {
    position: 'relative',
    display: 'flex',
    alignItems: 'center',
    minWidth: '200px',

    // 窄窗口:下拉占满换行后的整行,并放宽最小宽度限制
    '@media (max-width: 640px)': {
      minWidth: 0,
      flexBasis: '100%',
    },
  },
  select: {
    width: '100%',
    height: '32px',
    padding: '0 30px 0 12px',
    backgroundColor: palette.backgroundElevated,
    border: `1px solid ${palette.border}`,
    borderRadius: radius.control,
    fontFamily,
    fontSize: '13px',
    color: palette.textPrimary,
    appearance: 'none',
    cursor: 'pointer',
    outline: 'none',

    '&:focus': {
      border: `1px solid ${palette.primary}`,
    },
  },
  selectChevron: {
    position: 'absolute',
    right: '8px',
    pointerEvents: 'none',
    color: palette.textMuted,
    display: 'flex',
  },
  hostToggle: {
    height: '30px',
    padding: '0 14px',
    border: 'none',
    borderRadius: radius.control,
    fontFamily,
    fontSize: '13px',
    fontWeight: 600,
    color: '#fff',
    backgroundColor: palette.primary,
    cursor: 'pointer',
    flexShrink: 0,

    '&:hover': {
      backgroundColor: palette.primaryHover,
    },

    // 窄窗口行内换行后,启停按钮右对齐到下一行
    '@media (max-width: 640px)': {
      marginLeft: 'auto',
    },
  },
  peerFields: {
    display: 'flex',
    gap: '8px',
    marginTop: '8px',
    alignItems: 'center',

    // 窄窗口:名称/地址输入框上下堆叠,添加按钮跟随最后一行
    '@media (max-width: 560px)': {
      flexWrap: 'wrap',
    },
  },
  peerInput: {
    flex: 1,
    minWidth: 0,
    height: '30px',
    padding: '0 10px',
    backgroundColor: palette.background,
    border: `1px solid ${palette.borderLight}`,
    borderRadius: radius.control,
    fontFamily,
    fontSize: '12px',
    color: palette.textSecondary,
    outline: 'none',
    transition: 'border-color 150ms ease',

    '&:focus': {
      border: `1px solid ${palette.primary}`,
    },
  },
  errorText: {
    fontFamily,
    fontSize: '12px',
    color: palette.destructive,
    marginTop: '6px',
  },
  noticeText: {
    fontFamily,
    fontSize: '13px',
    color: palette.primary,
  },
  logHeader: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    padding: '12px 16px',
    borderBottom: `1px solid ${palette.borderLight}`,
  },
  logTitle: {
    fontFamily,
    fontSize: '14px',
    fontWeight: 600,
    color: palette.textPrimary,
  },
  logList: {
    maxHeight: '360px',
    overflowY: 'auto',
    padding: '8px 16px',
    fontFamily: 'monospace',
    fontSize: '12px',
    color: palette.textPrimary,
  },
  logRow: {
    lineHeight: '20px',
    whiteSpace: 'nowrap',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
  },
  logEmpty: {
    padding: '16px',
    fontFamily,
    fontSize: '13px',
    color: palette.textMuted,
  },
  grayBtn: {
    background: '#F3F4F6',
    color: '#374151',
    border: '1px solid #E5E7EB',
    padding: '6px 16px',
    borderRadius: radius.control,
    fontFamily,
    fontSize: '12px',
    fontWeight: 500,
    cursor: 'pointer',
    flexShrink: 0,
    transition: 'background-color 150ms ease',

    '&:hover': {
      backgroundColor: '#E5E7EB',
    },

    // 窄窗口行内换行后,灰按钮右对齐到下一行
    '@media (max-width: 640px)': {
      marginLeft: 'auto',
    },
  },
  macGrid: {
    display: 'grid',
    gridTemplateColumns: '1fr 1fr',
    gap: '8px',
    color: '#4B5563',
    fontSize: '12px',

    // 窄窗口:mac 键位映射表退化为单列
    '@media (max-width: 560px)': {
      gridTemplateColumns: '1fr',
    },
  },
  macRow: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    padding: '8px 10px',
    backgroundColor: '#F9FAFB',
    borderRadius: '8px',
  },
  macRowKey: {
    fontWeight: 600,
    color: '#111827',
  },
});

const useSwitchStyles = makeStyles({
  wrap: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    flexShrink: 0,

    // 窄窗口行内换行后,开关右对齐到下一行
    '@media (max-width: 640px)': {
      marginLeft: 'auto',
    },
  },
  track: {
    width: '40px',
    height: '22px',
    borderRadius: '999px',
    position: 'relative',
    cursor: 'pointer',
    border: 'none',
    transition: 'background-color 200ms ease',
    padding: 0,
  },
  thumb: {
    position: 'absolute',
    top: '2px',
    left: '2px',
    width: '18px',
    height: '18px',
    borderRadius: '50%',
    backgroundColor: '#fff',
    boxShadow: '0 1px 2px rgba(0,0,0,0.2)',
    transition: 'transform 200ms ease',
  },
  label: {
    fontFamily,
    fontSize: '12px',
    color: palette.textSecondary,
    width: '14px',
  },
});

interface ToggleSwitchProps {
  on: boolean;
  onChange: (next: boolean) => void;
  /** 禁用后不可交互(未实现功能的开关用) */
  disabled?: boolean;
}

/** 截图风格开关：蓝色轨道(开) / 灰色轨道(关) + 右侧「开/关」文字 */
export const ToggleSwitch: React.FC<ToggleSwitchProps> = ({ on, onChange, disabled }) => {
  const styles = useSwitchStyles();
  return (
    <div className={styles.wrap} style={disabled ? { opacity: 0.45 } : undefined}>
      <button
        type="button"
        className={styles.track}
        style={{
          backgroundColor: on ? palette.primary : '#D5DBE3',
          cursor: disabled ? 'not-allowed' : 'pointer',
        }}
        onClick={() => {
          if (!disabled) onChange(!on);
        }}
        disabled={disabled}
        role="switch"
        aria-checked={on}
        aria-label={on ? '开' : '关'}
      >
        <span
          className={styles.thumb}
          style={{ transform: on ? 'translateX(18px)' : 'translateX(0)' }}
        />
      </button>
      <span className={styles.label}>{on ? '开' : '关'}</span>
    </div>
  );
};

type TabKey = '常规' | '安全' | '键盘' | '网络' | '账号' | '日志' | '诊断';

/** 校验对端地址格式 host:port（IPv4 / hostname，端口 1..65535） */
function isValidPeerAddr(addr: string): boolean {
  const match = addr.match(
    /^([0-9]{1,3}(?:\.[0-9]{1,3}){3}|[a-zA-Z0-9](?:[a-zA-Z0-9-]*[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]*[a-zA-Z0-9])?)*):([0-9]{1,5})$/,
  );
  if (!match) return false;
  const port = Number(match[2]);
  return port >= 1 && port <= 65535;
}

/** 格式化日志时间：ISO 转「YYYY-MM-DD HH:mm:ss.mmm」，截断到毫秒 */
function formatLogTime(time: string): string {
  const s = time.replace('T', ' ').replace('Z', '');
  const dotIndex = s.indexOf('.');
  return dotIndex >= 0 ? s.slice(0, dotIndex + 4) : s;
}

/**
 * 设置界面：顶部「常规/安全/键盘/网络/账号/日志」标签页 + 卡片式设置项。
 * 「网络」tab 为真实功能（被控端模式 + 对端设备列表，读写持久化配置），
 * 「账号」tab 展示当前登录账号与退出登录，「日志」tab 展示操作日志，其余 tab 保持静态展示。
 */
export const SettingsPage: React.FC<{ onLogout?: () => void }> = ({ onLogout }) => {
  const styles = useStyles();
  const [tab, setTab] = useState<TabKey>('常规');

  const [config, setConfig] = useState<AppConfig | null>(null);
  const [hostState, setHostState] = useState<HostState>({ running: false, port: 0 });
  const [hostError, setHostError] = useState<string | null>(null);
  // 「允许他人协助」切换防抖:启停是异步过程,连续快速切换会让停止/启动交错
  // (端口占用、状态事件乱序),进行中时忽略新的切换
  const hostTogglingRef = React.useRef(false);
  const [portInput, setPortInput] = useState('21118');
  const [signalServerInput, setSignalServerInput] = useState('');
  const [relayServerInput, setRelayServerInput] = useState('');
  const [hostIdInput, setHostIdInput] = useState('');
  const [newPeerName, setNewPeerName] = useState('');
  const [newPeerAddr, setNewPeerAddr] = useState('');
  const [peerError, setPeerError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [logs, setLogs] = useState<OperationLogEntry[]>([]);
  const [logsError, setLogsError] = useState<string | null>(null);
  const [incomingDir, setIncomingDir] = useState('');

  // 加载操作日志（最新在前）
  const loadLogs = async () => {
    setLogsError(null);
    try {
      const entries = await getOperationLogs(100);
      setLogs(entries);
    } catch (error) {
      setLogsError(`加载操作日志失败: ${String(error)}`);
    }
  };

  // 进入页面时加载配置 + 被控端状态，并订阅 host-state 事件
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void (async () => {
      const cfg = await getAppConfig();
      setConfig(cfg);
      setPortInput(String(cfg.hostPort));
      setSignalServerInput(cfg.signalServer ?? '');
      setRelayServerInput(cfg.relayServer ?? '');
      setHostIdInput(cfg.hostId ?? '');
    })();
    // 真实接收目录(文件传输落盘位置),供「存储路径」展示
    void getIncomingDir().then((dir) => {
      setIncomingDir(dir);
    });
    void isHostRunning().then((running) => setHostState((prev) => ({ ...prev, running })));
    void onHostStateChange((state) => setHostState(state)).then((fn) => {
      unlisten = fn;
    });
    void loadLogs();
    return () => unlisten?.();
  }, []);

  // 配置变更后防抖持久化
  useEffect(() => {
    if (!config) return;
    const timer = window.setTimeout(() => {
      void saveAppConfig(config);
    }, 400);
    return () => window.clearTimeout(timer);
  }, [config]);

  // 操作提示自动清除
  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => setNotice(null), 2500);
    return () => window.clearTimeout(timer);
  }, [notice]);

  // 直连失败时允许中继兜底：翻转 relayFallbackEnabled 并经防抖持久化
  const toggleRelayFallback = () => {
    setConfig((prev) => (prev ? { ...prev, relayFallbackEnabled: !prev.relayFallbackEnabled } : prev));
  };

  // 退出后保持被控端运行：翻转 keepRunningOnExit 并经防抖持久化
  const toggleKeepRunningOnExit = () => {
    setConfig((prev) => (prev ? { ...prev, keepRunningOnExit: !prev.keepRunningOnExit } : prev));
  };

  // 允许他人远程协助：持久化 hostEnabled 并真实启停被控端（与远程协助页同一接线）
  const toggleAllowAssist = async () => {
    if (!config || hostTogglingRef.current) return;
    hostTogglingRef.current = true;
    const next: AppConfig = { ...config, hostEnabled: !config.hostEnabled };
    setConfig(next);
    setHostError(null);
    try {
      await saveAppConfig(next);
      if (next.hostEnabled) {
        await startHost(config.hostPort);
        setNotice(`已开启远程协助，端口 ${config.hostPort}`);
      } else {
        await stopHost();
        setNotice('已关闭远程协助');
      }
    } catch (error) {
      setHostError(String(error));
      setNotice(`操作失败: ${String(error)}`);
    } finally {
      hostTogglingRef.current = false;
    }
  };

  const toggleHost = async () => {
    setHostError(null);
    if (hostState.running) {
      try {
        await stopHost();
      } catch (error) {
        setHostError(String(error));
      }
      return;
    }
    const port = Number(portInput);
    if (!Number.isInteger(port) || port < 1 || port > 65535) {
      setHostError('端口需在 1..65535 之间');
      return;
    }
    try {
      await startHost(port);
    } catch (error) {
      setHostError(`启动被控端失败: ${String(error)}`);
    }
  };

  const saveServerConfig = async () => {
    const sig = signalServerInput.trim();
    const relay = relayServerInput.trim();
    const id = hostIdInput.trim();
    const next: AppConfig = config
      ? {
          ...config,
          signalServer: sig ? sig : undefined,
          relayServer: relay ? relay : undefined,
          hostId: id,
        }
      : {
          hostEnabled: true,
          hostPort: Number(portInput) || 21118,
          peers: [],
          keepRunningOnExit: false,
          relayFallbackEnabled: true,
          signalServer: sig ? sig : undefined,
          relayServer: relay ? relay : undefined,
          hostId: id || 'dcr-browser',
        };
    setConfig(next);
    try {
      await saveAppConfig(next);
      setNotice('服务器与 ID 已保存');
    } catch (error) {
      setNotice(`保存失败: ${String(error)}`);
    }
  };

  const addPeer = () => {
    const name = newPeerName.trim();
    const addr = newPeerAddr.trim();
    if (!name) {
      setPeerError('请输入设备名称');
      return;
    }
    if (!isValidPeerAddr(addr)) {
      setPeerError('地址格式应为 host:port（如 192.168.1.10:21118）');
      return;
    }
    setConfig((prev) =>
      prev ? { ...prev, peers: [...prev.peers, { id: genPeerId(), name, addr }] } : prev,
    );
    setNewPeerName('');
    setNewPeerAddr('');
    setPeerError(null);
    setNotice('设备已添加并保存');
  };

  const deletePeer = (id: string) => {
    setConfig((prev) =>
      prev ? { ...prev, peers: prev.peers.filter((p) => p.id !== id) } : prev,
    );
    setNotice('设备已删除并保存');
  };

  const hostPort = config?.hostPort ?? 21118;

  return (
    <div className={styles.page}>
      <h1 className={styles.title}>设置</h1>

      <div className={styles.tabs}>
        {(['常规', '安全', '键盘', '网络', '账号', '日志', '诊断'] as TabKey[]).map((t) => (
          <button
            key={t}
            type="button"
            className={t === tab ? `${styles.tab} ${styles.tabActive}` : styles.tab}
            onClick={() => setTab(t)}
          >
            {t}
            {t === tab && <span className={styles.tabUnderline} />}
          </button>
        ))}
      </div>

      {tab === '常规' && (
        <>
          <div className={styles.card}>
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <RocketRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>开机自动启动</div>
                <div className={styles.rowDesc}>（暂未实现）</div>
              </div>
              <ToggleSwitch on={false} onChange={() => {}} disabled />
            </div>
            <div className={styles.rowDivider} />
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <PowerRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>允许通过远程开机启动</div>
                <div className={styles.rowDesc}>
                  协助配置本设备进行远程开机 <span className={styles.link}>开始协助</span>
                  （暂未实现）
                </div>
              </div>
              <ToggleSwitch on={false} onChange={() => {}} disabled />
            </div>
            <div className={styles.rowDivider} />
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <SleepRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>防止电脑休眠</div>
                <div className={styles.rowDesc}>休眠将导致电脑无法远程控制（暂未实现）</div>
              </div>
              <ToggleSwitch on={false} onChange={() => {}} disabled />
            </div>
          </div>

          <div className={styles.card}>
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <ArrowSyncRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>自动更新</div>
                <div className={styles.rowDesc}>
                  开启后会在电脑闲时自动更新，避免打扰（暂未实现）
                </div>
              </div>
              <ToggleSwitch on={false} onChange={() => {}} disabled />
            </div>
            <div className={styles.rowDivider} />
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <DragRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>被控时拖拽文件显示发送浮窗</div>
                <div className={styles.rowDesc}>开启后，可在被控时向主控端发送文件（暂未实现）</div>
              </div>
              <ToggleSwitch on={false} onChange={() => {}} disabled />
            </div>
          </div>

          <div className={styles.card}>
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <FolderOpenRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>移动设备传输文件存储路径</div>
              </div>
              <div className={styles.pathBox}>
                <input className={styles.pathInput} value={incomingDir} readOnly />
                <button
                  type="button"
                  className={styles.pathIconBtn}
                  aria-label="打开目录"
                  onClick={() => setNotice('打开目录功能暂未实现')}
                >
                  <FolderOpenRegular fontSize={14} />
                </button>
              </div>
            </div>
            <div className={styles.rowDivider} />
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <RocketRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>关闭窗口时</div>
              </div>
              <div className={styles.selectWrap}>
                <select className={styles.select} defaultValue="隐藏至菜单栏">
                  <option>隐藏至菜单栏</option>
                  <option>退出程序</option>
                </select>
                <span className={styles.selectChevron}>
                  <ChevronDownRegular fontSize={12} />
                </span>
              </div>
            </div>
          </div>
        </>
      )}

      {tab === '安全' && (
        <>
          <div className={styles.card}>
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <PersonRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>允许同账号控制本设备</div>
                <div className={styles.rowDesc}>（暂未实现）</div>
              </div>
              <ToggleSwitch on={false} onChange={() => {}} disabled />
            </div>
            <div className={styles.rowDivider} />
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <HeadsetRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>允许他人远程协助</div>
              </div>
              <ToggleSwitch on={config?.hostEnabled ?? false} onChange={() => void toggleAllowAssist()} />
            </div>
            <div className={styles.rowDivider} />
            <div className={styles.row}>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>远程协助连接本设备的方式</div>
                <div className={styles.rowDesc}>通过本机【设备 ID】和【设备验证码】即可发起远程协助</div>
              </div>
              <div className={styles.selectWrap}>
                <select className={styles.select} defaultValue="验证码连接">
                  <option>验证码连接</option>
                </select>
                <span className={styles.selectChevron}>
                  <ChevronDownRegular fontSize={12} />
                </span>
              </div>
            </div>
          </div>
        </>
      )}

      {tab === '键盘' && (
        <>
          <div className={styles.card}>
            <div className={styles.row}>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>仅控制端响应的快捷键</div>
                <div className={styles.rowDesc}>远控时按下以下快捷键，仅在本机响应，不会传到被控端。</div>
              </div>
              <button type="button" className={styles.grayBtn}>
                添加
              </button>
            </div>
          </div>
          <div className={styles.card}>
            <div className={styles.row}>
              <span className={styles.rowBody}>
                <span className={styles.rowTitle}>远控 macOS 按键映射</span>
              </span>
              <button type="button" className={styles.grayBtn}>
                还原默认按键
              </button>
            </div>
            <div className={styles.rowDivider} />
            <div className={styles.row}>
              <div className={styles.macGrid} style={{ width: '100%' }}>
                <div className={styles.macRow}>
                  <span>Win 键</span>
                  <span className={styles.macRowKey}>⌘ Command</span>
                </div>
                <div className={styles.macRow}>
                  <span>Alt 键</span>
                  <span className={styles.macRowKey}>⌥ Option</span>
                </div>
              </div>
            </div>
          </div>
        </>
      )}

      {tab === '网络' && (
        <>
          <div className={styles.card}>
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <FlashRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>Direct P2P 直连加速</div>
                <div className={styles.rowDesc}>开启后直连失败时允许经中继服务器兜底转发</div>
              </div>
              <ToggleSwitch
                on={config?.relayFallbackEnabled ?? true}
                onChange={() => toggleRelayFallback()}
              />
            </div>
          </div>

          <div className={styles.card}>
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <GlobeRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>服务器与 ID</div>
                <div className={styles.rowDesc}>配置信令/中继服务器与本机 ID，保存后被控端可被广域网发现</div>
              </div>
            </div>
            <div className={styles.rowDivider} />
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <GlobeRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>信令服务器</div>
                <div className={styles.rowDesc}>
                  配置后本机被控端会注册并可被广域网发现，控制端连接时会查询对端地址并回退到中继
                </div>
              </div>
              <input
                className={styles.pathInput}
                style={{ width: 220, maxWidth: "100%" }}
                placeholder="signal.example.com:21116"
                value={signalServerInput}
                onChange={(e) => setSignalServerInput(e.target.value)}
              />
            </div>
            <div className={styles.rowDivider} />
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <ServerRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>中继服务器</div>
                <div className={styles.rowDesc}>直连失败时经其中继转发流量（打洞失败兜底）</div>
              </div>
              <input
                className={styles.pathInput}
                style={{ width: 220, maxWidth: "100%" }}
                placeholder="relay.example.com:21117"
                value={relayServerInput}
                onChange={(e) => setRelayServerInput(e.target.value)}
              />
            </div>
            <div className={styles.rowDivider} />
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <KeyRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>本机 ID</div>
                <div className={styles.rowDesc}>信令注册用唯一标识，被控端与发现列表显示</div>
              </div>
              <input
                className={styles.pathInput}
                style={{ width: 220, maxWidth: "100%" }}
                placeholder="dcr-主机名"
                value={hostIdInput}
                onChange={(e) => setHostIdInput(e.target.value)}
              />
            </div>
            <div className={styles.rowDivider} />
            <div className={styles.row}>
              <div className={styles.rowBody} />
              <button type="button" className={styles.hostToggle} onClick={() => void saveServerConfig()}>
                保存
              </button>
            </div>
          </div>

          <div className={styles.card}>
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <DesktopRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>被控端模式（远程控制本机）</div>
                <div className={styles.rowDesc}>监听指定端口，允许其他设备远程控制本机</div>
              </div>
            </div>
            <div className={styles.rowDivider} />
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <LinkRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>端口</div>
                <div className={styles.rowDesc}>范围 1..65535，默认 21118</div>
              </div>
              <input
                type="number"
                className={styles.pathInput}
                style={{ width: 120, maxWidth: "100%" }}
                value={portInput}
                min={1}
                max={65535}
                onChange={(e) => {
                  const raw = e.target.value;
                  setPortInput(raw);
                  const v = Number(raw);
                  if (Number.isInteger(v) && v >= 1 && v <= 65535) {
                    setConfig((prev) => (prev ? { ...prev, hostPort: v } : prev));
                  }
                }}
              />
            </div>
            <div className={styles.rowDivider} />
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                {hostState.running ? <StopRegular fontSize={16} /> : <PlayRegular fontSize={16} />}
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>
                  {hostState.running
                    ? `运行中 · 端口 ${hostState.port || hostPort}`
                    : '未运行'}
                </div>
              </div>
              <button
                type="button"
                className={styles.hostToggle}
                style={hostState.running ? { backgroundColor: palette.destructive } : undefined}
                onClick={() => void toggleHost()}
              >
                {hostState.running ? '停止被控端' : '启动被控端'}
              </button>
            </div>
            {hostError && <div className={styles.errorText}>{hostError}</div>}
            <div className={styles.rowDivider} />
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <RocketRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>退出后保持被控端运行</div>
                <div className={styles.rowDesc}>当前仅持久化到配置，不实现系统级自启</div>
              </div>
              <ToggleSwitch
                on={config?.keepRunningOnExit ?? false}
                onChange={() => toggleKeepRunningOnExit()}
              />
            </div>
          </div>

          <div className={styles.card}>
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <PersonRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>设备列表（远程桌面对端）</div>
                <div className={styles.rowDesc}>配置可远程控制的对端设备，保存后生效</div>
              </div>
            </div>
            <div className={styles.rowDivider} />
            {(config?.peers ?? []).map((peer) => (
              <React.Fragment key={peer.id}>
                <div className={styles.row}>
                  <div className={styles.rowBody}>
                    <div className={styles.rowTitle}>{peer.name}</div>
                    <div className={styles.rowDesc}>{peer.addr}</div>
                  </div>
                  <button
                    type="button"
                    className={styles.pathIconBtn}
                    onClick={() => deletePeer(peer.id)}
                    aria-label="删除设备"
                  >
                    <DeleteRegular fontSize={14} />
                  </button>
                </div>
                <div className={styles.rowDivider} />
              </React.Fragment>
            ))}
            {(config?.peers ?? []).length === 0 && (
              <div className={styles.row}>
                <div className={styles.rowDesc}>暂无对端设备，请在下方向添加</div>
              </div>
            )}
            <div className={styles.rowDivider} />
            <div className={styles.row}>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>添加设备</div>
                <div className={styles.peerFields}>
                  <input
                    className={styles.peerInput}
                    placeholder="名称"
                    value={newPeerName}
                    onChange={(e) => setNewPeerName(e.target.value)}
                  />
                  <input
                    className={styles.peerInput}
                    placeholder="host:port"
                    value={newPeerAddr}
                    onChange={(e) => setNewPeerAddr(e.target.value)}
                  />
                  <button type="button" className={styles.pathIconBtn} onClick={addPeer} aria-label="添加设备">
                    <AddRegular fontSize={14} />
                  </button>
                </div>
                {peerError && <div className={styles.errorText}>{peerError}</div>}
              </div>
            </div>
          </div>
        </>
      )}

      {tab === '账号' && (
        <div className={styles.card}>
          <div className={styles.row}>
            <span className={styles.rowIcon}>
              <PersonRegular fontSize={16} />
            </span>
            <div className={styles.rowBody}>
              <div className={styles.rowTitle}>当前登录账号</div>
              <div className={styles.rowDesc}>
                {config?.account
                  ? `${config.account.username} · ${config.account.server}`
                  : '未登录（登录后解锁远程控制功能）'}
              </div>
            </div>
            {config?.account && (
              <button
                type="button"
                className={styles.hostToggle}
                style={{ backgroundColor: palette.destructive }}
                onClick={() => onLogout?.()}
              >
                退出登录
              </button>
            )}
          </div>
        </div>
      )}

      {tab === '日志' && (
        <div className={styles.card}>
          <div className={styles.logHeader}>
            <div className={styles.logTitle}>操作日志</div>
            <button
              type="button"
              className={styles.pathIconBtn}
              onClick={() => void loadLogs()}
              aria-label="刷新日志"
            >
              <ArrowSyncRegular fontSize={14} />
            </button>
          </div>
          {logsError && <div className={styles.errorText}>{logsError}</div>}
          {logs.length === 0 && !logsError && <div className={styles.logEmpty}>暂无操作日志</div>}
          {logs.length > 0 && (
            <div className={styles.logList}>
              {logs.map((entry, idx) => (
                <div key={`${entry.time}-${idx}`} className={styles.logRow}>
                  {formatLogTime(entry.time)} [{entry.module}] {entry.action} {entry.detail}
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {tab === '诊断' && <DxgiLoopbackCard />}

      {notice && <div className={styles.noticeText}>{notice}</div>}
    </div>
  );
};

export default SettingsPage;
