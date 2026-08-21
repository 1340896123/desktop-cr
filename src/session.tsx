import React, { useCallback, useEffect, useRef, useState } from 'react';
import ReactDOM from 'react-dom/client';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen } from '@tauri-apps/api/event';
import { RemoteSessionView } from './components/RemoteSessionView';
import {
  connectToDevice,
  disconnectFromDevice,
  getConnectionState,
  onConnectionStateChange,
  type ConnectionState,
} from './services/connection';
import { invoke } from '@tauri-apps/api/core';
import { fontFamily } from './theme/tokens';
import './styles/global.css';

interface RemoteSessionInfo {
  peerId: string;
  deviceName: string;
}

/** 读取主窗口写入的目标设备信息(独立会话窗口必为 Tauri 环境) */
async function getRemoteSessionInfo(): Promise<RemoteSessionInfo | null> {
  return invoke<RemoteSessionInfo | null>('get_remote_session_info');
}

const boxStyle: React.CSSProperties = {
  height: '100vh',
  display: 'flex',
  flexDirection: 'column',
  alignItems: 'center',
  justifyContent: 'center',
  gap: '14px',
  backgroundColor: '#0f172a',
  color: '#F8FAFC',
  fontFamily,
};

const btnStyle: React.CSSProperties = {
  padding: '6px 18px',
  borderRadius: '6px',
  border: 'none',
  backgroundColor: '#2563EB',
  color: '#ffffff',
  fontFamily,
  fontSize: '13px',
  cursor: 'pointer',
};

const ghostBtnStyle: React.CSSProperties = {
  ...btnStyle,
  backgroundColor: 'transparent',
  border: '1px solid rgba(148, 163, 184, 0.5)',
  color: '#CBD5E1',
};

/**
 * 独立远程会话窗口入口:读取主窗口写入的目标设备,自行发起连接并整窗承载
 * 会话视图;断开会话即关窗(关窗事件由 Rust 侧兜底清理会话)。
 */
const SessionApp: React.FC = () => {
  const [info, setInfo] = useState<RemoteSessionInfo | null>(null);
  const [state, setState] = useState<ConnectionState>({ connected: false });
  const [error, setError] = useState<string | null>(null);
  const [wasConnected, setWasConnected] = useState(false);
  const wasConnectedRef = useRef(false);

  // 连接(重试共用):失败展示错误与重试按钮
  const connect = useCallback(async (target: RemoteSessionInfo) => {
    setError(null);
    setWasConnected(false);
    wasConnectedRef.current = false;
    try {
      const next = await connectToDevice(target.peerId);
      setState(next);
      if (next.connected) {
        wasConnectedRef.current = true;
        setWasConnected(true);
      }
    } catch (e) {
      console.error('[session] 连接失败', e);
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let unlistenTarget: (() => void) | undefined;
    void (async () => {
      const target = await getRemoteSessionInfo();
      if (!target) {
        setError('未指定要连接的设备');
        return;
      }
      if (disposed) return;
      setInfo(target);
      // 已有活跃会话(如窗口重开)则直接沿用,否则发起新连接
      const existing = await getConnectionState();
      if (disposed) return;
      if (existing.connected && existing.peerId === target.peerId) {
        setState(existing);
        wasConnectedRef.current = true;
        setWasConnected(true);
        return;
      }
      await connect(target);
    })();

    // 意外掉线(对端关闭/网络中断):状态事件推来 connected=false 时切换到已断开视图
    void onConnectionStateChange((next) => {
      setState(next);
      if (!next.connected && wasConnectedRef.current) {
        setWasConnected(true);
        if (next.error) setError(next.error);
      }
    }).then((fn) => {
      if (disposed) {
        fn();
      } else {
        unlisten = fn;
      }
    });

    // 主窗口对另一设备再次进入桌面:切换目标重连(旧会话由新连接替换)
    void listen<{ peerId: string; deviceName: string }>(
      'remote-session-target',
      (event) => {
        const target = event.payload;
        setInfo(target);
        setState({ connected: false });
        void connect(target);
      },
    ).then((fn) => {
      if (disposed) {
        fn();
      } else {
        unlistenTarget = fn;
      }
    });

    return () => {
      disposed = true;
      unlisten?.();
      unlistenTarget?.();
    };
  }, [connect]);

  const connected = state.connected && info != null && state.peerId === info.peerId;

  // 断开连接:结束会话并关闭本窗口(清理由 Rust 侧关窗事件兜底)
  const handleExit = useCallback(async () => {
    await disconnectFromDevice().catch(() => undefined);
    await getCurrentWindow().close();
  }, []);

  if (!connected && error && wasConnected) {
    return (
      <div style={boxStyle}>
        <div style={{ fontSize: '18px', fontWeight: 600 }}>连接已断开</div>
        <div style={{ fontSize: '13px', color: '#FCA5A5', maxWidth: '480px', textAlign: 'center', lineHeight: '22px' }}>
          {error}
        </div>
        {info && (
          <button type="button" style={btnStyle} onClick={() => void connect(info)}>
            重新连接
          </button>
        )}
        <button type="button" style={ghostBtnStyle} onClick={() => void getCurrentWindow().close()}>
          关闭窗口
        </button>
      </div>
    );
  }

  if (!connected && error) {
    return (
      <div style={boxStyle}>
        <div style={{ fontSize: '18px', fontWeight: 600 }}>连接失败</div>
        <div style={{ fontSize: '13px', color: '#FCA5A5', maxWidth: '480px', textAlign: 'center', lineHeight: '22px' }}>
          {error}
        </div>
        {info && (
          <button type="button" style={btnStyle} onClick={() => void connect(info)}>
            重试连接
          </button>
        )}
        <button type="button" style={ghostBtnStyle} onClick={() => void getCurrentWindow().close()}>
          关闭窗口
        </button>
      </div>
    );
  }

  if (!info || !connected) {
    return (
      <div style={boxStyle}>
        <div style={{ fontSize: '15px' }}>正在连接 {info?.deviceName ?? '远程设备'}…</div>
      </div>
    );
  }

  return (
    <RemoteSessionView
      deviceName={info.deviceName}
      connected
      onExit={() => void handleExit()}
    />
  );
};

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <SessionApp />
  </React.StrictMode>,
);
