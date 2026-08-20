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
import { AddRegular, DeleteRegular, DesktopRegular, VideoRegular } from '@fluentui/react-icons';
import {
  addVirtualMonitor,
  installVirtualDisplayDriver,
  listVirtualMonitors,
  onMonitorsChanged,
  removeVirtualMonitor,
  type VirtualMonitor,
} from '../services/virtualDisplay';
import { listMonitors, startCapture, stopCapture, type MonitorInfo } from '../services/capture';
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
  sectionTitle: {
    fontSize: '13px',
    fontWeight: 600,
    color: tokens.colorNeutralForeground2,
    marginTop: '4px',
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
  badge: {
    display: 'inline-block',
    fontSize: '11px',
    lineHeight: '16px',
    padding: '0 6px',
    borderRadius: '4px',
    marginLeft: '6px',
    color: tokens.colorNeutralForegroundOnBrand,
    backgroundColor: tokens.colorBrandBackground,
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
  consolePlaceholder: {
    position: 'absolute',
    inset: 0,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    fontSize: '14px',
    color: tokens.colorNeutralForeground3,
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
}

/**
 * 虚拟屏管理面板：本机真实显示器列表（选中预览抓帧）+
 * 一键增加 1080P / 2K / 4K 虚拟屏，并内嵌本机抓帧预览控制台。
 */
export const VirtualDisplayPanel: React.FC<VirtualDisplayPanelProps> = ({ deviceName }) => {
  const styles = useStyles();
  const [monitors, setMonitors] = useState<VirtualMonitor[]>([]);
  const [realMonitors, setRealMonitors] = useState<MonitorInfo[]>([]);
  const [selectedRealId, setSelectedRealId] = useState<number | null>(null);
  const [driverInstalled, setDriverInstalled] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [isFullscreen, setIsFullscreen] = useState(false);

  const refreshMonitors = useCallback(async () => {
    try {
      setMonitors(await listVirtualMonitors());
    } catch (error) {
      setNotice(`获取虚拟屏列表失败: ${String(error)}`);
    }
  }, []);

  const refreshRealMonitors = useCallback(async () => {
    try {
      setRealMonitors(await listMonitors());
    } catch (error) {
      setNotice(`获取本机显示器列表失败: ${String(error)}`);
    }
  }, []);

  useEffect(() => {
    void refreshMonitors();
    void refreshRealMonitors();
  }, [refreshMonitors, refreshRealMonitors]);

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
      const info = await installVirtualDisplayDriver();
      setDriverInstalled(true);
      setNotice(`驱动安装成功: ${info}`);
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
      await addVirtualMonitor(width, height, fps);
      await refreshMonitors();
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
      await refreshMonitors();
    } catch (error) {
      setNotice(`移除虚拟屏失败: ${String(error)}`);
    }
  };

  const selectedReal =
    realMonitors.find((m) => m.id === selectedRealId) ?? null;

  // 抓帧生命周期：选中的真实显示器变化时启动抓帧，面板卸载时停止；未选中时不做任何事
  useEffect(() => {
    if (!selectedReal) return;
    void startCapture({
      monitorId: selectedReal.id,
      width: selectedReal.width,
      height: selectedReal.height,
      fps: 30,
    });
    return () => {
      void stopCapture();
    };
  }, [selectedReal?.id]);

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

      <div className={styles.sectionTitle}>本机显示器（点击选中用于预览抓帧）</div>
      <div className={styles.list}>
        {realMonitors.map((monitor) => (
          <div
            key={monitor.id}
            className={styles.monitorCard}
            style={{
              cursor: 'pointer',
              borderColor:
                selectedReal?.id === monitor.id
                  ? tokens.colorBrandStroke1
                  : tokens.colorNeutralStroke1,
            }}
            onClick={() => setSelectedRealId(monitor.id)}
          >
            <div>
              <strong>
                <DesktopRegular fontSize={14} /> {monitor.name}
              </strong>
              <span className={styles.monitorMeta}>
                {' '}
                {monitor.width}x{monitor.height}
              </span>
              {monitor.isPrimary && <span className={styles.badge}>主屏</span>}
              {monitor.isVirtual && <span className={styles.badge}>虚拟</span>}
            </div>
          </div>
        ))}
        {realMonitors.length === 0 && (
          <div style={{ color: tokens.colorNeutralForeground3, textAlign: 'center', padding: '12px' }}>
            未检测到本机显示器
          </div>
        )}
      </div>

      <div className={styles.sectionTitle}>虚拟显示器</div>
      <div className={styles.list}>
        {monitors.map((monitor) => (
          <div key={monitor.id} className={styles.monitorCard}>
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
              onClick={() => void removeMonitor(monitor.id)}
              aria-label="移除虚拟屏"
            />
          </div>
        ))}
        {monitors.length === 0 && (
          <div style={{ color: tokens.colorNeutralForeground3, textAlign: 'center', padding: '12px' }}>
            暂无虚拟屏，点击上方按钮添加
          </div>
        )}
      </div>

      <div className={styles.console}>
        <ControlBar
          isFullscreen={isFullscreen}
          onToggleFullscreen={() => setIsFullscreen((prev) => !prev)}
          onOpenSettings={() => setNotice('设置面板（虚拟显示器设置暂未提供）')}
        />
        {selectedReal ? (
          <RemoteCanvas
            connected
            remoteWidth={selectedReal.width}
            remoteHeight={selectedReal.height}
            mode="canvas"
            streamSource="local"
          />
        ) : (
          <div className={styles.consolePlaceholder}>请选择显示器预览</div>
        )}
      </div>
    </div>
  );
};

export default VirtualDisplayPanel;
