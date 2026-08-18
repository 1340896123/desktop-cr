import React, { useCallback, useEffect, useState } from 'react';
import {
  Button,
  makeStyles,
  MessageBar,
  MessageBarBody,
  Spinner,
  tokens,
  Tooltip,
} from '@fluentui/react-components';
import { AddRegular, DeleteRegular, VideoRegular } from '@fluentui/react-icons';
import {
  addVirtualMonitor,
  installVirtualDisplayDriver,
  listVirtualMonitors,
  onMonitorsChanged,
  removeVirtualMonitor,
  type VirtualMonitor,
} from '../services/virtualDisplay';
import { startCapture, stopCapture } from '../services/capture';
import RemoteCanvas from './RemoteCanvas';
import ControlBar from './ControlBar';

const useStyles = makeStyles({
  panel: {
    height: '100%',
    display: 'flex',
    flexDirection: 'column',
    gap: '12px',
    padding: '16px',
    backgroundColor: tokens.colorNeutralBackground1,
    overflowY: 'auto',
  },
  toolbar: {
    display: 'flex',
    flexWrap: 'wrap',
    gap: '8px',
    alignItems: 'center',
  },
  list: {
    display: 'flex',
    flexDirection: 'column',
    gap: '8px',
  },
  monitorCard: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    padding: '8px 12px',
    border: `1px solid ${tokens.colorNeutralStroke1}`,
    borderRadius: '6px',
    backgroundColor: tokens.colorNeutralBackground1,
  },
  monitorMeta: {
    fontSize: '13px',
    color: tokens.colorNeutralForeground2,
  },
  console: {
    flex: 1,
    minHeight: '420px',
    position: 'relative',
    border: `1px solid ${tokens.colorNeutralStroke1}`,
    borderRadius: '8px',
    overflow: 'hidden',
    backgroundColor: tokens.colorNeutralBackground2,
  },
});

const PRESETS: Array<{ label: string; width: number; height: number; fps: number }> = [
  { label: '1080P', width: 1920, height: 1080, fps: 60 },
  { label: '2K', width: 2560, height: 1440, fps: 60 },
  { label: '4K', width: 3840, height: 2160, fps: 60 },
];

interface VirtualDisplayPanelProps {
  /** 当前连接的设备名称（展示在面板标题区） */
  deviceName?: string;
  /** 远程设备是否已连接 */
  connected?: boolean;
}

/**
 * 虚拟屏管理面板：一键增加 1080P / 2K / 4K 虚拟屏，并内嵌远程控制台。
 */
export const VirtualDisplayPanel: React.FC<VirtualDisplayPanelProps> = ({ deviceName, connected = false }) => {
  const styles = useStyles();
  const [monitors, setMonitors] = useState<VirtualMonitor[]>([]);
  const [driverInstalled, setDriverInstalled] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [selectedId, setSelectedId] = useState<number | null>(null);

  const refreshMonitors = useCallback(async () => {
    try {
      setMonitors(await listVirtualMonitors());
    } catch (error) {
      setNotice(`获取虚拟屏列表失败: ${String(error)}`);
    }
  }, []);

  useEffect(() => {
    void refreshMonitors();
  }, [refreshMonitors]);

  // 订阅虚拟屏列表变更事件，自动刷新列表
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void onMonitorsChanged((list) => setMonitors(list)).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  const installDriver = async () => {
    setBusy(true);
    setNotice(null);
    try {
      await installVirtualDisplayDriver();
      setDriverInstalled(true);
    } catch (error) {
      setNotice(`驱动安装失败: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  };

  const addMonitor = async (width: number, height: number, fps: number) => {
    setBusy(true);
    setNotice(null);
    try {
      const id = await addVirtualMonitor(width, height, fps);
      setMonitors((prev) => [...prev, { id, width, height, fps, connected: true }]);
      setSelectedId(id);
    } catch (error) {
      setNotice(`添加虚拟屏失败: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  };

  const removeMonitor = async (id: number) => {
    setNotice(null);
    try {
      await removeVirtualMonitor(id);
      setMonitors((prev) => prev.filter((m) => m.id !== id));
      if (selectedId === id) setSelectedId(null);
    } catch (error) {
      setNotice(`移除虚拟屏失败: ${String(error)}`);
    }
  };

  const selected = monitors.find((m) => m.id === selectedId) ?? monitors[0] ?? null;

  // 抓帧生命周期：选中虚拟屏存在时启动抓帧（分辨率由 Rust 端 clamp），面板卸载时停止
  useEffect(() => {
    if (!selected) return;
    void startCapture({
      monitorId: selected.id,
      width: selected.width,
      height: selected.height,
      fps: selected.fps,
    });
    return () => {
      void stopCapture();
    };
  }, [selected?.id]);

  return (
    <div className={styles.panel}>
      <div className={styles.toolbar}>
        {deviceName && (
          <span style={{ fontWeight: 600, fontSize: '15px' }}>{deviceName} · 远程会话</span>
        )}
        <Button
          appearance="primary"
          icon={<VideoRegular />}
          onClick={() => void installDriver()}
          disabled={busy || driverInstalled}
        >
          {driverInstalled ? '驱动已安装' : '安装虚拟显示器驱动'}
        </Button>
        {PRESETS.map((preset) => (
          <Tooltip key={preset.label} content={`${preset.width}x${preset.height} @ ${preset.fps}Hz`} relationship="label">
            <Button
              icon={<AddRegular />}
              onClick={() => void addMonitor(preset.width, preset.height, preset.fps)}
              disabled={busy}
            >
              增加 {preset.label} 虚拟屏
            </Button>
          </Tooltip>
        ))}
        {busy && <Spinner size="tiny" label="处理中" />}
      </div>

      {notice && (
        <MessageBar intent="warning">
          <MessageBarBody>{notice}</MessageBarBody>
        </MessageBar>
      )}

      <div className={styles.list}>
        {monitors.map((monitor) => (
          <div
            key={monitor.id}
            className={styles.monitorCard}
            style={{
              cursor: 'pointer',
              borderColor: selected?.id === monitor.id ? tokens.colorBrandStroke1 : tokens.colorNeutralStroke1,
            }}
            onClick={() => setSelectedId(monitor.id)}
          >
            <div>
              <strong>
                虚拟屏 #{monitor.id} · {monitor.width}x{monitor.height}
              </strong>
              <div className={styles.monitorMeta}>
                {monitor.fps}Hz · {monitor.connected ? '已连接' : '未连接'}
              </div>
            </div>
            <Button
              icon={<DeleteRegular />}
              appearance="subtle"
              onClick={(e) => {
                e.stopPropagation();
                void removeMonitor(monitor.id);
              }}
              aria-label="移除虚拟屏"
            />
          </div>
        ))}
        {monitors.length === 0 && (
          <div style={{ color: tokens.colorNeutralForeground3, textAlign: 'center', padding: '24px' }}>
            暂无虚拟屏，点击上方按钮添加
          </div>
        )}
      </div>

      <div className={styles.console}>
        <ControlBar
          isFullscreen={isFullscreen}
          onToggleFullscreen={() => setIsFullscreen((prev) => !prev)}
          onOpenSettings={() => setNotice('设置面板（POC 阶段占位）')}
        />
        <RemoteCanvas
          connected={connected || (selected?.connected ?? false)}
          remoteWidth={selected?.width ?? 1920}
          remoteHeight={selected?.height ?? 1080}
          mode="canvas"
        />
      </div>
    </div>
  );
};

export default VirtualDisplayPanel;
