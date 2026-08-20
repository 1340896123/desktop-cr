import React, { useState } from 'react';
import { makeStyles } from '@fluentui/react-components';
import {
  ChevronDownRegular,
  ChevronRightRegular,
  CloudRegular,
  DesktopRegular,
  GridRegular,
  HeadsetRegular,
  SettingsRegular,
  StarRegular,
} from '@fluentui/react-icons';
import { fontFamily } from '../theme/tokens';

const UU_BLUE = '#0066ff';

const useStyles = makeStyles({
  sidebar: {
    width: '240px',
    flexShrink: 0,
    height: '100%',
    display: 'flex',
    flexDirection: 'column',
    backgroundColor: '#f7f9fc',
    borderRight: '1px solid rgba(229, 231, 235, 0.8)',
    overflow: 'hidden',
    fontFamily,
    fontSize: '12px',
    userSelect: 'none',
  },
  nav: {
    flex: 1,
    overflowY: 'auto',
    overflowX: 'hidden',
    padding: '8px',
    display: 'flex',
    flexDirection: 'column',
    gap: '4px',
  },
  group: {
    display: 'flex',
    flexDirection: 'column',
    gap: '2px',
  },
  groupHeader: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    padding: '8px 12px',
    borderRadius: '8px',
    cursor: 'pointer',
    fontWeight: 600,
    color: '#374151',
    transition: 'background-color 150ms ease',

    '&:hover': {
      backgroundColor: 'rgba(229, 231, 235, 0.6)',
    },
  },
  groupTitle: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
  },
  groupIcon: {
    display: 'flex',
    color: '#6B7280',
  },
  chevron: {
    display: 'flex',
    color: '#9CA3AF',
    fontSize: '10px',
  },
  subList: {
    display: 'flex',
    flexDirection: 'column',
    gap: '2px',
    paddingLeft: '8px',
  },
  item: {
    display: 'flex',
    alignItems: 'center',
    gap: '10px',
    padding: '6px 10px',
    paddingLeft: '12px',
    borderRadius: '8px',
    cursor: 'pointer',
    color: '#4B5563',
    position: 'relative',
    transition: 'background-color 150ms ease',

    '&:hover': {
      backgroundColor: 'rgba(229, 231, 235, 0.4)',
    },
  },
  itemActive: {
    backgroundColor: 'rgba(229, 231, 235, 0.8)',
    color: '#111827',
    fontWeight: 600,
  },
  activeBar: {
    position: 'absolute',
    left: 0,
    top: '6px',
    bottom: '6px',
    width: '4px',
    backgroundColor: UU_BLUE,
    borderTopRightRadius: '4px',
    borderBottomRightRadius: '4px',
  },
  statusDot: {
    width: '8px',
    height: '8px',
    borderRadius: '50%',
    flexShrink: 0,
  },
  gridSquare: {
    width: '16px',
    height: '16px',
    borderRadius: '4px',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    color: '#ffffff',
    flexShrink: 0,
  },
  blueSquare: {
    width: '16px',
    height: '16px',
    borderRadius: '4px',
    backgroundColor: UU_BLUE,
    color: '#ffffff',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    flexShrink: 0,
  },
  itemLabel: {
    whiteSpace: 'nowrap',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
  },
  footer: {
    padding: '8px',
    borderTop: '1px solid rgba(229, 231, 235, 0.7)',
  },
});

export interface SidebarDevice {
  id: string;
  name: string;
  online: boolean;
}

interface SidebarProps {
  devices: SidebarDevice[];
  selectedDeviceId?: string | null;
  onSelectDevice?: (id: string) => void;
  onSelectCloud?: () => void;
  onSelectAssist?: () => void;
  onSelectFavorites?: () => void;
  onSelectSettings?: () => void;
  onSelectVirtualDisplays?: () => void;
  /** 当前激活的视图标识，用于高亮「设置」与「远程协助」子项 */
  activeView?: string;
}

/**
 * 左侧边栏：我的设备 / 云设备 / 远程协助（可折叠分组），底部固定「设置」。
 * 设备子项 = 状态点 + 方块网格图标 + 名称，选中项灰色背景 + 左侧蓝色竖条。
 */
export const Sidebar: React.FC<SidebarProps> = ({
  devices,
  selectedDeviceId,
  onSelectDevice,
  onSelectCloud,
  onSelectAssist,
  onSelectFavorites,
  onSelectSettings,
  activeView,
}) => {
  const styles = useStyles();
  const [expanded, setExpanded] = useState({ myDevices: true, cloudDevices: false, remoteAssist: true });
  const [assistSub, setAssistSub] = useState<'start' | 'favorites'>('start');

  const toggle = (key: keyof typeof expanded) => setExpanded((prev) => ({ ...prev, [key]: !prev[key] }));

  return (
    <nav className={styles.sidebar} aria-label="主导航">
      <div className={styles.nav}>
        <div className={styles.group}>
          <div className={styles.groupHeader} onClick={() => toggle('myDevices')} role="button" aria-expanded={expanded.myDevices}>
            <span className={styles.groupTitle}>
              <span className={styles.groupIcon}>
                <DesktopRegular fontSize={16} />
              </span>
              <span>我的设备</span>
            </span>
            <span className={styles.chevron}>
              {expanded.myDevices ? <ChevronDownRegular fontSize={12} /> : <ChevronRightRegular fontSize={12} />}
            </span>
          </div>
          {expanded.myDevices && (
            <div className={styles.subList}>
              {devices.map((device) => {
                const active = device.id === selectedDeviceId && activeView !== 'settings';
                return (
                  <div
                    key={device.id}
                    className={active ? `${styles.item} ${styles.itemActive}` : styles.item}
                    onClick={() => onSelectDevice?.(device.id)}
                    role="button"
                    tabIndex={0}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' || e.key === ' ') onSelectDevice?.(device.id);
                    }}
                  >
                    {active && <span className={styles.activeBar} />}
                    <span
                      className={styles.statusDot}
                      style={{ backgroundColor: device.online ? '#10B981' : '#D1D5DB' }}
                    />
                    <span
                      className={styles.gridSquare}
                      style={{ backgroundColor: device.online ? '#3B82F6' : '#9CA3AF' }}
                    >
                      <GridRegular fontSize={9} />
                    </span>
                    <span className={styles.itemLabel}>{device.name}</span>
                  </div>
                );
              })}
              {devices.length === 0 && (
                <div className={styles.item}>
                  <span className={styles.itemLabel} style={{ color: '#9CA3AF' }}>暂无设备</span>
                </div>
              )}
            </div>
          )}
        </div>

        <div className={styles.group}>
          <div className={styles.groupHeader} onClick={() => toggle('cloudDevices')} role="button" aria-expanded={expanded.cloudDevices}>
            <span className={styles.groupTitle}>
              <span className={styles.groupIcon}>
                <CloudRegular fontSize={16} />
              </span>
              <span>云设备</span>
            </span>
            <span className={styles.chevron}>
              {expanded.cloudDevices ? <ChevronDownRegular fontSize={12} /> : <ChevronRightRegular fontSize={12} />}
            </span>
          </div>
          {expanded.cloudDevices && (
            <div className={styles.subList}>
              <div className={styles.item} onClick={onSelectCloud} role="button" tabIndex={0}>
                <span className={styles.blueSquare}>
                  <CloudRegular fontSize={9} />
                </span>
                <span className={styles.itemLabel}>市场</span>
              </div>
            </div>
          )}
        </div>

        <div className={styles.group}>
          <div className={styles.groupHeader} onClick={() => toggle('remoteAssist')} role="button" aria-expanded={expanded.remoteAssist}>
            <span className={styles.groupTitle}>
              <span className={styles.groupIcon}>
                <HeadsetRegular fontSize={16} />
              </span>
              <span>远程协助</span>
            </span>
            <span className={styles.chevron}>
              {expanded.remoteAssist ? <ChevronDownRegular fontSize={12} /> : <ChevronRightRegular fontSize={12} />}
            </span>
          </div>
          {expanded.remoteAssist && (
            <div className={styles.subList}>
              <div
                className={assistSub === 'start' && activeView === 'assist' ? `${styles.item} ${styles.itemActive}` : styles.item}
                onClick={() => {
                  setAssistSub('start');
                  onSelectAssist?.();
                }}
                role="button"
                tabIndex={0}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    setAssistSub('start');
                    onSelectAssist?.();
                  }
                }}
              >
                {assistSub === 'start' && activeView === 'assist' && <span className={styles.activeBar} />}
                <span className={styles.blueSquare}>
                  <HeadsetRegular fontSize={9} />
                </span>
                <span className={styles.itemLabel}>开始协助</span>
              </div>
              <div
                className={assistSub === 'favorites' && activeView === 'assist' ? `${styles.item} ${styles.itemActive}` : styles.item}
                onClick={() => {
                  setAssistSub('favorites');
                  onSelectFavorites?.();
                }}
                role="button"
                tabIndex={0}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    setAssistSub('favorites');
                    onSelectFavorites?.();
                  }
                }}
              >
                {assistSub === 'favorites' && activeView === 'assist' && <span className={styles.activeBar} />}
                <span className={styles.blueSquare}>
                  <StarRegular fontSize={9} />
                </span>
                <span className={styles.itemLabel}>收藏设备</span>
              </div>
            </div>
          )}
        </div>
      </div>

      <div className={styles.footer}>
        <div
          className={activeView === 'settings' ? `${styles.item} ${styles.itemActive}` : styles.item}
          onClick={onSelectSettings}
          role="button"
          tabIndex={0}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') onSelectSettings?.();
          }}
        >
          {activeView === 'settings' && <span className={styles.activeBar} />}
          <span className={styles.groupIcon}>
            <SettingsRegular fontSize={16} />
          </span>
          <span className={styles.itemLabel}>设置</span>
        </div>
      </div>
    </nav>
  );
};

export default Sidebar;