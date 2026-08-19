import React, { useCallback, useEffect, useState } from 'react';
import { makeStyles } from '@fluentui/react-components';
import { TitleBar } from './components/TitleBar';
import { Sidebar, type SidebarDevice } from './components/Sidebar';
import { DevicePage } from './components/DevicePage';
import RemoteSessionView from './components/RemoteSessionView';
import VirtualDisplayPanel from './components/VirtualDisplayPanel';
import FileTransferPage from './components/FileTransferPage';
import SettingsPage from './components/SettingsPage';
import {
  connectToDevice,
  disconnectFromDevice,
  getDevices,
  getConnectionState,
  onConnectionStateChange,
  type ConnectionState,
  type DeviceInfo,
} from './services/connection';
import { palette, fontFamily, spacing } from './theme/tokens';
import { onWindowMaximizedChange } from './services/window';

const useStyles = makeStyles({
  root: {
    display: 'flex',
    flexDirection: 'column',
    height: '100vh',
    width: '100vw',
    overflow: 'hidden',
    backgroundColor: palette.background,
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

type View = 'home' | 'session' | 'transfer' | 'settings' | 'cloud' | 'vdisplay';

export const App: React.FC = () => {
  const styles = useStyles();
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [state, setState] = useState<ConnectionState>({ connected: false });
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [view, setView] = useState<View>('home');
  const [connecting, setConnecting] = useState(false);
  const [maximized, setMaximized] = useState(false);

  const load = useCallback(async () => {
    setDevices(await getDevices());
    setState(await getConnectionState());
  }, []);

  useEffect(() => {
    void load();
    let unlisten: (() => void) | undefined;
    void onConnectionStateChange((next) => setState(next)).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [load]);

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

  return (
    <div
      className={styles.root}
      style={{
        borderRadius: maximized ? 0 : 8,
        border: maximized ? 'none' : `1px solid ${palette.borderLight}`,
      }}
    >
      <TitleBar
        onBack={
          view === 'session'
            ? handleExitSession
            : view === 'vdisplay' || view === 'transfer'
              ? () => setView('home')
              : undefined
        }
        onRefresh={() => void load()}
        onSettings={() => setView('settings')}
      />

      <div className={styles.body}>
        <Sidebar
          devices={sidebarDevices}
          selectedDeviceId={selected?.id ?? null}
          onSelectDevice={selectDevice}
          onSelectCloud={() => setView('cloud')}
          onSelectAssist={() => setView('cloud')}
          onSelectFavorites={() => setView('cloud')}
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
            <FileTransferPage />
          )}

          {view === 'settings' && (
            <SettingsPage />
          )}

          {view === 'home' && selected && (
            <DevicePage
              deviceName={selected.name}
              online={selected.status === 'online'}
              connecting={connecting}
              quickDevices={quickDevices}
              onEnterDesktop={() => void handleEnterDesktop()}
              onFileTransfer={() => setView('transfer')}
              onMore={() => setView('transfer')}
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
              <div className={styles.placeholderTitle}>云设备市场</div>
              <div>通过市场发现云端设备，或在设置中配置 HBBS / HBBR 服务器（后续阶段）</div>
              <div style={{ color: palette.textMuted }}>
                在线设备 {onlineCount} / {devices.length}
              </div>
            </div>
          )}
        </main>
      </div>
    </div>
  );
};

export default App;
