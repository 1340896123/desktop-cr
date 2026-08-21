import React, { useCallback, useEffect, useRef, useState } from 'react';
import { makeStyles } from '@fluentui/react-components';
import { TitleBar } from './components/TitleBar';
import { Sidebar, type SidebarDevice } from './components/Sidebar';
import { DevicePage } from './components/DevicePage';
import RemoteSessionView from './components/RemoteSessionView';
import VirtualDisplayPanel from './components/VirtualDisplayPanel';
import FileTransferPage from './components/FileTransferPage';
import SettingsPage from './components/SettingsPage';
import RemoteAssistPage from './components/RemoteAssistPage';
import LoginPage from './components/LoginPage';
import { Toast } from './components/shared/Toast';
import { UnsupportedTag } from './components/shared/UnsupportedTag';
import {
  connectToDevice,
  disconnectFromDevice,
  getDevices,
  getConnectionState,
  onConnectionStateChange,
  type ConnectionState,
  type DeviceInfo,
} from './services/connection';
import {
  getAccount,
  checkAccountToken,
  logoutAccount,
  onAuthExpired,
  type AccountSession,
} from './services/auth';
import { palette, fontFamily, spacing } from './theme/tokens';
import { onWindowMaximizedChange, openFileTransferWindow } from './services/window';

const useStyles = makeStyles({
  root: {
    display: 'flex',
    flexDirection: 'column',
    height: '100vh',
    width: '100vw',
    overflow: 'hidden',
    backgroundColor: '#F4F6F9',
  },
  body: {
    flex: 1,
    display: 'flex',
    overflow: 'hidden',
  },
  content: {
    flex: 1,
    overflow: 'hidden',
    position: 'relative',
    backgroundColor: '#F4F6F9',
  },
  placeholderPage: {
    height: '100%',
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'center',
    justifyContent: 'center',
    gap: `${spacing.sm}px`,
    fontFamily,
    color: palette.textMuted,
    fontSize: '14px',
  },
  placeholderTitle: {
    fontSize: '20px',
    fontWeight: 600,
    color: palette.textPrimary,
  },
});

type View = 'home' | 'session' | 'transfer' | 'settings' | 'cloud' | 'vdisplay' | 'assist';

export const App: React.FC = () => {
  const styles = useStyles();
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [state, setState] = useState<ConnectionState>({ connected: false });
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [view, setView] = useState<View>('home');
  const [connecting, setConnecting] = useState(false);
  const [maximized, setMaximized] = useState(false);
  const [account, setAccount] = useState<AccountSession | null>(null);
  const [authChecked, setAuthChecked] = useState(false);
  const [toastMsg, setToastMsg] = useState<string | null>(null);
  const toastTimerRef = useRef<number | null>(null);
  const loadRequestRef = useRef(0);

  const showToast = useCallback((msg: string) => {
    setToastMsg(msg);
    if (toastTimerRef.current) window.clearTimeout(toastTimerRef.current);
    toastTimerRef.current = window.setTimeout(() => setToastMsg(null), 2000);
  }, []);

  useEffect(() => {
    return () => {
      if (toastTimerRef.current) window.clearTimeout(toastTimerRef.current);
    };
  }, []);

  const load = useCallback(async () => {
    const requestId = ++loadRequestRef.current;
    const nextDevices = await getDevices();
    if (requestId !== loadRequestRef.current) return;
    setDevices(nextDevices);
    const nextState = await getConnectionState();
    if (requestId !== loadRequestRef.current) return;
    setState(nextState);
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let authExpired = false;

    const handleAuthExpired = async () => {
      authExpired = true;
      loadRequestRef.current += 1;
      setAuthChecked(true);
      await logoutAccount().catch(() => undefined);
      if (disposed) return;
      setState({ connected: false });
      setDevices([]);
      setView('home');
      setAccount(null);
      showToast('登录已过期,请重新登录');
    };

    void (async () => {
      try {
        const remove = await onAuthExpired(handleAuthExpired);
        if (disposed) {
          remove();
          return;
        }
        unlisten = remove;
      } catch (error) {
        console.error('[app] 注册登录过期监听失败', error);
      }

      const existing = await getAccount();
      if (disposed || authExpired) return;
      if (existing) {
        try {
          await checkAccountToken(existing);
          if (disposed || authExpired) return;
          setAccount(existing);
        } catch {
          await logoutAccount().catch(() => undefined);
          if (disposed || authExpired) return;
          setAccount(null);
        }
      } else {
        setAccount(null);
      }
      if (!disposed && !authExpired) setAuthChecked(true);
    })();

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [showToast]);

  useEffect(() => {
    if (!authChecked || !account) return;
    void load();
    // 信令注册/心跳会在登录状态变化后异步更新设备归属;定时刷新保证
    // 两台同账号客户端无需手动点击刷新即可看到最新在线状态。
    const timer = window.setInterval(() => {
      void load();
    }, 5000);
    return () => window.clearInterval(timer);
  }, [account, authChecked, load]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void onConnectionStateChange((next) => setState(next)).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void onWindowMaximizedChange(setMaximized).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  const sidebarDevices: SidebarDevice[] = devices.map((device) => ({
    id: device.id,
    name: device.name,
    online: device.status === 'online',
  }));

  const selected =
    devices.find((device) => device.id === selectedId) ?? devices[0] ?? null;

  const onlineCount = devices.filter((device) => device.status === 'online').length;

  const quickDevices = devices.slice(0, 2).map((device) => ({
    id: device.id,
    name: device.name,
    meta:
      device.status === 'online'
        ? '在线 · 可连接'
        : device.status === 'idle'
          ? '等待连接'
          : '离线',
    online: device.status === 'online',
  }));

  const selectDevice = (id: string) => {
    setSelectedId(id);
    setView('home');
  };

  // 进入远程桌面：先发起连接，成功后再切到会话视图
  const handleEnterDesktop = async () => {
    if (!selected) return;
    setConnecting(true);
    try {
      const next = await connectToDevice(selected.id);
      setState(next);
      setView('session');
    } catch (error) {
      console.error('[app] 连接失败', error);
      showToast(`连接失败: ${String(error)}`);
    } finally {
      setConnecting(false);
    }
  };

  // 打开独立文件传输窗口(Tauri 模式);浏览器模式回退到页内视图
  const handleOpenTransfer = async () => {
    const opened = await openFileTransferWindow(selected?.name);
    if (!opened) setView('transfer');
  };

  // 远程协助页：匹配到对端设备后连接；若不在设备列表则补充进列表以便进入会话视图
  const handleConnectPeer = async (peerId: string, name: string) => {
    setDevices((prev) =>
      prev.some((d) => d.id === peerId) ? prev : [...prev, { id: peerId, name, status: 'online' }],
    );
    setSelectedId(peerId);
    setConnecting(true);
    try {
      const next = await connectToDevice(peerId);
      setState(next);
      setView('session');
      showToast(`已连接设备 ${name}`);
    } catch (error) {
      console.error('[app] 远程协助连接失败', error);
      showToast(`连接失败: ${String(error)}`);
    } finally {
      setConnecting(false);
    }
  };

  // 退出远程会话：浏览器模式 disconnect 是 noop，本地状态需显式重置
  const handleExitSession = async () => {
    void disconnectFromDevice();
    setState({ connected: false });
    setView('home');
  };

  // 退出账号登录：清除本地会话并回到登录页
  const handleLogout = async () => {
    try {
      await logoutAccount();
    } finally {
      loadRequestRef.current += 1;
      setDevices([]);
      setState({ connected: false });
      setView('home');
      setAccount(null);
    }
  };

  // 未完成登录校验前不渲染内容；校验后未登录则进入登录页
  if (!authChecked) {
    return <div className={styles.root} />;
  }

  if (authChecked && !account) {
    return (
      <>
        <LoginPage
          onLogin={(s) => {
            setAccount(s);
          }}
        />
        <Toast message={toastMsg} />
      </>
    );
  }

  // 账号登录门禁：校验未完成显示加载态,未登录显示登录页
  if (!authChecked) {
    return (
      <div className={styles.root}>
        <div className={styles.placeholderPage}>
          <div className={styles.placeholderTitle}>正在验证登录状态…</div>
        </div>
      </div>
    );
  }
  if (!account) {
    return <LoginPage onLogin={(acc) => setAccount(acc)} />;
  }

  return (
    <div
      className={styles.root}
      style={{
        borderRadius: maximized ? 0 : 8,
        border: maximized ? 'none' : `1px solid ${palette.borderLight}`,
      }}
    >
      <TitleBar
        onRefresh={() => void load()}
        onShowToast={showToast}
        account={account}
        onLogout={() => void handleLogout()}
      />

      <div className={styles.body}>
        <Sidebar
          devices={sidebarDevices}
          selectedDeviceId={selected?.id ?? null}
          activeView={view}
          onSelectDevice={selectDevice}
          onSelectCloud={() => setView('cloud')}
          onSelectAssist={() => setView('assist')}
          onSelectFavorites={() => setView('assist')}
          onSelectSettings={() => setView('settings')}
          onSelectVirtualDisplays={() => setView('vdisplay')}
        />

        <main className={styles.content}>
          {view === 'session' && selected && (
            <RemoteSessionView
              key={selected.id}
              deviceName={selected.name}
              connected={state.connected && state.peerId === selected.id}
              onExit={handleExitSession}
              onOpenVirtualDisplays={() => setView('vdisplay')}
            />
          )}

          {view === 'vdisplay' && selected && (
            <VirtualDisplayPanel deviceName={selected.name} />
          )}

          {view === 'transfer' && (
            <FileTransferPage deviceName={selected?.name} />
          )}

          {view === 'settings' && (
            <SettingsPage onLogout={() => void handleLogout()} />
          )}

          {view === 'assist' && (
            <RemoteAssistPage onConnectDevice={handleConnectPeer} onShowToast={showToast} />
          )}

          {view === 'home' && selected && (
            <DevicePage
              deviceName={selected.name}
              online={selected.status === 'online'}
              connecting={connecting}
              quickDevices={quickDevices}
              onEnterDesktop={() => void handleEnterDesktop()}
              onFileTransfer={() => void handleOpenTransfer()}
              onMore={() => void handleOpenTransfer()}
              onAddDevice={() => setView('cloud')}
              onSelectQuick={(id) => selectDevice(id)}
            />
          )}

          {view === 'home' && !selected && (
            <div className={styles.placeholderPage}>
              <div className={styles.placeholderTitle}>暂无设备</div>
              <div>请先添加或选择一台设备</div>
            </div>
          )}

          {view === 'cloud' && (
            <div className={styles.placeholderPage}>
              <div className={styles.placeholderTitle}>
                云设备市场
                <UnsupportedTag label="暂未开放" />
              </div>
              <div>通过市场发现云端设备，或在设置中配置信令 / 中继服务器（后续阶段）</div>
              <div style={{ color: palette.textMuted }}>
                在线设备 {onlineCount} / {devices.length}
              </div>
            </div>
          )}
        </main>
      </div>

      <Toast message={toastMsg} />
    </div>
  );
};

export default App;
