import React, { useState } from 'react';
import { makeStyles } from '@fluentui/react-components';
import {
  DesktopRegular,
  CloudRegular,
  HeadsetRegular,
  StarRegular,
  StorageRegular,
  SettingsRegular,
  ChevronDownRegular,
  ChevronRightRegular,
  AddRegular,
  VideoRegular,
} from '@fluentui/react-icons';
import { palette, fontFamily, spacing, sidebarWidth } from '../theme/tokens';

const useStyles = makeStyles({
  sidebar: {
    width: `${sidebarWidth}px`,
    flexShrink: 0,
    height: '100%',
    display: 'flex',
    flexDirection: 'column',
    backgroundColor: palette.sidebar,
    borderRight: `1px solid ${palette.borderLight}`,
    overflowY: 'auto',
    overflowX: 'hidden',
    padding: `${spacing.xs}px 0`,
  },
  group: {
    marginBottom: `${spacing.sm}px`,
  },
  groupHeader: {
    display: 'flex',
    alignItems: 'center',
    gap: '6px',
    padding: '7px 12px 7px 14px',
    fontFamily,
    fontSize: '14px',
    fontWeight: 600,
    color: palette.textPrimary,
    cursor: 'pointer',
    userSelect: 'none',
    transition: 'background-color 150ms ease',

    '&:hover': {
      backgroundColor: palette.sidebarItemHover,
    },
  },
  groupChevron: {
    marginLeft: 'auto',
    display: 'flex',
    color: palette.textMuted,
    flexShrink: 0,
  },
  item: {
    display: 'flex',
    alignItems: 'center',
    gap: '10px',
    padding: '7px 12px 7px 14px',
    fontFamily,
    fontSize: '14px',
    color: palette.textSecondary,
    cursor: 'pointer',
    userSelect: 'none',
    borderLeft: '3px solid transparent',
    transition: 'background-color 150ms ease, color 150ms ease',
    position: 'relative',

    '&:hover': {
      backgroundColor: palette.sidebarItemHover,
      color: palette.textPrimary,
    },
  },
  itemActive: {
    backgroundColor: palette.sidebarItemActive,
    borderLeftColor: palette.primary,
    color: palette.textPrimary,
    fontWeight: 600,
  },
  itemIcon: {
    display: 'flex',
    flexShrink: 0,
    color: palette.primary,
  },
  itemLabel: {
    whiteSpace: 'nowrap',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    flex: 1,
  },
  itemBadge: {
    display: 'flex',
    alignItems: 'center',
    gap: '4px',
    fontSize: '11px',
    color: palette.textMuted,
  },
  spacer: {
    flex: 1,
  },
  footer: {
    borderTop: `1px solid ${palette.borderLight}`,
    paddingTop: `${spacing.xs}px`,
    marginTop: `${spacing.xs}px`,
  },
  addRow: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '7px 12px 7px 14px',
    fontFamily,
    fontSize: '13px',
    color: palette.textMuted,
    cursor: 'pointer',
    userSelect: 'none',

    '&:hover': {
      backgroundColor: palette.sidebarItemHover,
      color: palette.textPrimary,
    },
  },
  addIcon: {
    display: 'flex',
    color: palette.primary,
  },
});

export interface SidebarDevice {
  id: string;
  name: string;
  online: boolean;
}

interface SidebarGroupProps {
  title: string;
  icon: React.ReactNode;
  children: React.ReactNode;
  defaultOpen?: boolean;
  showChevron?: boolean;
}

const SidebarGroup: React.FC<SidebarGroupProps> = ({ title, icon, children, defaultOpen = true, showChevron = true }) => {
  const styles = useStyles();
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className={styles.group}>
      <div
        className={styles.groupHeader}
        onClick={() => setOpen((prev) => !prev)}
        role="button"
        aria-expanded={open}
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') setOpen((prev) => !prev);
        }}
      >
        <span className={styles.itemIcon}>{icon}</span>
        <span>{title}</span>
        {showChevron && (
          <span className={styles.groupChevron}>
            {open ? <ChevronDownRegular fontSize={14} /> : <ChevronRightRegular fontSize={14} />}
          </span>
        )}
      </div>
      {open && children}
    </div>
  );
};

interface SidebarItemProps {
  icon?: React.ReactNode;
  label: string;
  active?: boolean;
  right?: React.ReactNode;
  onClick?: () => void;
}

export const SidebarItem: React.FC<SidebarItemProps> = ({ icon, label, active, right, onClick }) => {
  const styles = useStyles();
  return (
    <div
      className={active ? `${styles.item} ${styles.itemActive}` : styles.item}
      onClick={onClick}
      role="button"
      tabIndex={0}
      aria-current={active}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') onClick?.();
      }}
    >
      {icon && <span className={styles.itemIcon}>{icon}</span>}
      <span className={styles.itemLabel}>{label}</span>
      {right && <span className={styles.itemBadge}>{right}</span>}
    </div>
  );
};

interface SidebarProps {
  devices: SidebarDevice[];
  selectedDeviceId?: string | null;
  onSelectDevice?: (id: string) => void;
  onSelectCloud?: () => void;
  onSelectAssist?: () => void;
  onSelectFavorites?: () => void;
  onSelectSettings?: () => void;
  onSelectVirtualDisplays?: () => void;
}

/**
 * 截图左侧边栏：我的设备 / 云设备 / 远程协助 分组，底部设置。
 * 选中设备：淡蓝背景 + 左侧蓝色细条。
 */
export const Sidebar: React.FC<SidebarProps> = ({
  devices,
  selectedDeviceId,
  onSelectDevice,
  onSelectCloud,
  onSelectAssist,
  onSelectFavorites,
  onSelectSettings,
  onSelectVirtualDisplays,
}) => {
  const styles = useStyles();
  const onlineCount = devices.filter((d) => d.online).length;

  return (
    <nav className={styles.sidebar} aria-label="主导航">
      <SidebarGroup title="我的设备" icon={<DesktopRegular fontSize={16} />} defaultOpen>
        {devices.map((device) => (
          <SidebarItem
            key={device.id}
            icon={<DesktopRegular fontSize={15} />}
            label={device.name}
            active={device.id === selectedDeviceId}
            right={
              <span>
                <span
                  style={{
                    display: 'inline-block',
                    width: 6,
                    height: 6,
                    borderRadius: '50%',
                    backgroundColor: device.online ? palette.online : palette.offline,
                  }}
                />
                {onlineCount > 0 ? ` ${onlineCount} 在线` : ''}
              </span>
            }
            onClick={() => onSelectDevice?.(device.id)}
          />
        ))}
        <SidebarItem
          icon={<VideoRegular fontSize={15} />}
          label="虚拟屏管理"
          onClick={onSelectVirtualDisplays}
        />
        <div className={styles.addRow} role="button" tabIndex={0} onClick={onSelectCloud}>
          <span className={styles.addIcon}>
            <AddRegular fontSize={15} />
          </span>
          <span>添加设备</span>
        </div>
      </SidebarGroup>

      <SidebarGroup title="云设备" icon={<CloudRegular fontSize={16} />} defaultOpen>
        <SidebarItem
          icon={<StorageRegular fontSize={15} />}
          label="市场"
          onClick={onSelectCloud}
        />
      </SidebarGroup>

      <SidebarGroup title="远程协助" icon={<HeadsetRegular fontSize={16} />} defaultOpen>
        <SidebarItem
          icon={<HeadsetRegular fontSize={15} />}
          label="开始协助"
          onClick={onSelectAssist}
        />
        <SidebarItem
          icon={<StarRegular fontSize={15} />}
          label="收藏设备"
          onClick={onSelectFavorites}
        />
      </SidebarGroup>

      <div className={styles.spacer} />

      <div className={styles.footer}>
        <SidebarItem
          icon={<SettingsRegular fontSize={16} />}
          label="设置"
          onClick={onSelectSettings}
        />
      </div>
    </nav>
  );
};

export default Sidebar;
