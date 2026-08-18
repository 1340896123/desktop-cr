import React, { useCallback, useEffect, useState } from 'react';
import { makeStyles } from '@fluentui/react-components';
import { TitleBar } from './components/TitleBar';
import { Sidebar, type SidebarDevice } from './components/Sidebar';
import { DevicePage } from './components/DevicePage';
import RemoteSessionView from './components/RemoteSessionView';
import FileTransferPage from './components/FileTransferPage';
import SettingsPage from './components/SettingsPage';
import {
  getDevices,
  getConnectionState,
  onConnectionStateChange,
  type ConnectionState,
  type DeviceInfo,
} from './services/connection';
import { palette, fontFamily, spacing } from './theme/tokens';

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

type View = 'home' | 'session' | 'transfer' | 'settings' | 'cloud';

export const App: React.FC = () => {
  const styles = useStyles();
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [state, setState] = useState<ConnectionState>({ connected: false });
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [view, setView] = useState<View>('home');

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
    meta: device.status === 'online' ? '在线 · 可连接' : '离线',
    online: device.status === 'online',
  }));

  const selectDevice = (id: string) => {
    setSelectedId(id);
    setView('home');
  };

  return (
    <div className={styles.root}>
      <TitleBar
        onBack={view === 'session' || view === 'transfer' ? () => setView('home') : undefined}
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
        />

        <main className={styles.content}>
          {view === 'session' && selected && (
            <RemoteSessionView
              key={selected.id}
              deviceName={selected.name}
              connected={state.connected && state.peerId === selected.id}
              onExit={() => setView('home')}
            />
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
              quickDevices={quickDevices}
              onEnterDesktop={() => setView('session')}
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
