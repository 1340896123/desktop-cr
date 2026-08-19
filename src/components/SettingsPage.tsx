import React, { useEffect, useState } from 'react';
import { makeStyles } from '@fluentui/react-components';
import {
  RocketRegular,
  PowerRegular,
  SleepRegular,
  ArrowSyncRegular,
  FolderOpenRegular,
  DragRegular,
  EditRegular,
  ChevronDownRegular,
  DesktopRegular,
  LinkRegular,
  PlayRegular,
  StopRegular,
  PersonRegular,
  AddRegular,
  DeleteRegular,
} from '@fluentui/react-icons';
import { palette, fontFamily, spacing, radius, shadow } from '../theme/tokens';
import { startHost, stopHost, isHostRunning, onHostStateChange, type HostState } from '../services/connection';
import { getAppConfig, saveAppConfig, genPeerId, type AppConfig } from '../services/config';
import { getOperationLogs, type OperationLogEntry } from '../services/logs';

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
  },
  title: {
    fontFamily,
    fontSize: '24px',
    fontWeight: 700,
    color: palette.textPrimary,
    letterSpacing: '-0.02em',
    margin: 0,
    marginBottom: '4px',
  },
  tabs: {
    display: 'flex',
    gap: '24px',
    borderBottom: `1px solid ${palette.borderLight}`,
    marginBottom: '8px',
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
  },
  selectWrap: {
    position: 'relative',
    display: 'flex',
    alignItems: 'center',
    minWidth: '200px',
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
  },
  peerFields: {
    display: 'flex',
    gap: '8px',
    marginTop: '8px',
    alignItems: 'center',
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
});

const useSwitchStyles = makeStyles({
  wrap: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    flexShrink: 0,
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
}

/** 截图风格开关：蓝色轨道(开) / 灰色轨道(关) + 右侧「开/关」文字 */
export const ToggleSwitch: React.FC<ToggleSwitchProps> = ({ on, onChange }) => {
  const styles = useSwitchStyles();
  return (
    <div className={styles.wrap}>
      <button
        type="button"
        className={styles.track}
        style={{ backgroundColor: on ? palette.primary : '#D5DBE3' }}
        onClick={() => onChange(!on)}
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

const settings: Record<string, { title: string; desc?: string; icon: React.ReactNode }[]> = {
  安全: [
    { title: '连接前锁定屏幕', icon: <ShieldRegularIcon /> },
    { title: '每次连接需要验证码', icon: <PowerRegular fontSize={16} /> },
  ],
  键盘: [
    { title: '发送 Ctrl 组合键到远端', icon: <RocketRegular fontSize={16} /> },
    { title: '将 Win 键发送到远端', icon: <PowerRegular fontSize={16} /> },
  ],
};

function ShieldRegularIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
      <path d="M12 3l7 3v6c0 4.4-3 7.5-7 9-4-1.5-7-4.6-7-9V6l7-3z" />
    </svg>
  );
}

type TabKey = '常规' | '安全' | '键盘' | '网络' | '日志';

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
 * 设置界面：顶部「常规/安全/键盘/网络/日志」标签页 + 卡片式设置项。
 * 「网络」tab 为真实功能（被控端模式 + 对端设备列表，读写持久化配置），
 * 「日志」tab 展示操作日志，其余 tab 保持静态展示。
 */
export const SettingsPage: React.FC = () => {
  const styles = useStyles();
  const [tab, setTab] = useState<TabKey>('常规');
  const [toggles, setToggles] = useState<Record<string, boolean>>({
    autostart: true,
    wakeup: false,
    sleep: true,
    autoupdate: false,
    dragfloat: true,
    lock: true,
    password: false,
    ctrl: true,
    win: false,
    relay: true,
    tcp: false,
  });

  const [config, setConfig] = useState<AppConfig | null>(null);
  const [hostState, setHostState] = useState<HostState>({ running: false, port: 0 });
  const [hostError, setHostError] = useState<string | null>(null);
  const [portInput, setPortInput] = useState('21118');
  const [newPeerName, setNewPeerName] = useState('');
  const [newPeerAddr, setNewPeerAddr] = useState('');
  const [peerError, setPeerError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [logs, setLogs] = useState<OperationLogEntry[]>([]);
  const [logsError, setLogsError] = useState<string | null>(null);

  const toggle = (key: string) => setToggles((prev) => ({ ...prev, [key]: !prev[key] }));

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
    })();
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

  const toggleHostEnabled = () => {
    setConfig((prev) => (prev ? { ...prev, hostEnabled: !prev.hostEnabled } : prev));
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
        {(['常规', '安全', '键盘', '网络', '日志'] as TabKey[]).map((t) => (
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
              </div>
              <ToggleSwitch on={toggles.autostart} onChange={() => toggle('autostart')} />
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
                </div>
              </div>
              <ToggleSwitch on={toggles.wakeup} onChange={() => toggle('wakeup')} />
            </div>
            <div className={styles.rowDivider} />
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <SleepRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>防止电脑休眠</div>
                <div className={styles.rowDesc}>休眠将导致电脑无法远程控制（强烈推荐开启）</div>
              </div>
              <ToggleSwitch on={toggles.sleep} onChange={() => toggle('sleep')} />
            </div>
          </div>

          <div className={styles.card}>
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <ArrowSyncRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>自动更新</div>
                <div className={styles.rowDesc}>开启后会在电脑闲时自动更新，避免打扰（电脑休眠时无法更新）</div>
              </div>
              <ToggleSwitch on={toggles.autoupdate} onChange={() => toggle('autoupdate')} />
            </div>
            <div className={styles.rowDivider} />
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <DragRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>被控时拖拽文件显示发送浮窗</div>
                <div className={styles.rowDesc}>开启后，可在被控时向主控端发送文件</div>
              </div>
              <ToggleSwitch on={toggles.dragfloat} onChange={() => toggle('dragfloat')} />
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
                <input
                  className={styles.pathInput}
                  defaultValue={'C:\\Program Files\\Netease\\GameViewer\\Download'}
                  readOnly
                />
                <button type="button" className={styles.pathIconBtn} aria-label="编辑路径">
                  <EditRegular fontSize={14} />
                </button>
                <button type="button" className={styles.pathIconBtn} aria-label="打开目录">
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

      {(tab === '安全' || tab === '键盘') && (
        <div className={styles.card}>
          {(settings[tab] ?? []).map((row, idx) => (
            <React.Fragment key={row.title}>
              {idx > 0 && <div className={styles.rowDivider} />}
              <div className={styles.row}>
                <span className={styles.rowIcon}>{row.icon}</span>
                <div className={styles.rowBody}>
                  <div className={styles.rowTitle}>{row.title}</div>
                  {row.desc && <div className={styles.rowDesc}>{row.desc}</div>}
                </div>
                <ToggleSwitch on={toggles[row.title] ?? false} onChange={() => toggle(row.title)} />
              </div>
            </React.Fragment>
          ))}
        </div>
      )}

      {tab === '网络' && (
        <>
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
                style={{ width: 120 }}
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
              <ToggleSwitch on={config?.hostEnabled ?? false} onChange={() => toggleHostEnabled()} />
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

          {notice && <div className={styles.noticeText}>{notice}</div>}
        </>
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
    </div>
  );
};

export default SettingsPage;
