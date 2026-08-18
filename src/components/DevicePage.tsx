import React from 'react';
import { makeStyles } from '@fluentui/react-components';
import {
  ArrowRightFilled,
  DocumentArrowUpRegular,
  MoreHorizontalRegular,
  DesktopRegular,
  ArrowRightRegular,
  AddRegular,
} from '@fluentui/react-icons';
import { StatusBadge } from './shared/IconButton';
import { palette, fontFamily, spacing, radius, shadow } from '../theme/tokens';

const useStyles = makeStyles({
  page: {
    flex: 1,
    height: '100%',
    overflowY: 'auto',
    padding: `${spacing.xl}px ${spacing.xxl}px`,
    display: 'flex',
    flexDirection: 'column',
    gap: `${spacing.section}px`,
  },
  header: {
    display: 'flex',
    alignItems: 'center',
    gap: '12px',
    flexWrap: 'wrap',
  },
  title: {
    fontFamily,
    fontSize: '24px',
    fontWeight: 700,
    color: palette.textPrimary,
    letterSpacing: '-0.02em',
    lineHeight: '32px',
    margin: 0,
  },
  heroCard: {
    backgroundColor: palette.backgroundElevated,
    borderRadius: radius.card,
    boxShadow: shadow.card,
    overflow: 'hidden',
    border: `1px solid ${palette.borderLight}`,
  },
  heroPreview: {
    position: 'relative',
    aspectRatio: '16 / 7.4',
    background: `linear-gradient(135deg, #1E5FD1 0%, #5DA8FF 45%, #A7C8F5 75%, #E8F1FF 100%)`,
    overflow: 'hidden',
  },
  heroWallpaper: {
    position: 'absolute',
    inset: 0,
    backgroundImage: `linear-gradient(180deg, rgba(255,255,255,0.08) 1px, transparent 1px),
      linear-gradient(90deg, rgba(255,255,255,0.08) 1px, transparent 1px)`,
    backgroundSize: '28px 28px',
  },
  heroGlow: {
    position: 'absolute',
    right: '-8%',
    top: '-30%',
    width: '60%',
    height: '130%',
    background: 'radial-gradient(closest-side, rgba(255,255,255,0.28), transparent)',
  },
  enterDesktop: {
    position: 'absolute',
    left: '24px',
    bottom: '18px',
    display: 'inline-flex',
    alignItems: 'center',
    gap: '8px',
    padding: '10px 18px',
    borderRadius: radius.control,
    backgroundColor: 'rgba(255, 255, 255, 0.92)',
    color: palette.textPrimary,
    fontFamily,
    fontSize: '14px',
    fontWeight: 600,
    cursor: 'pointer',
    border: 'none',
    boxShadow: shadow.card,
    transition: 'transform 150ms ease, background-color 150ms ease',
    backdropFilter: 'blur(4px)',

    '&:hover': {
      transform: 'translateY(-1px)',
      backgroundColor: '#ffffff',
    },
    '&:active': {
      transform: 'translateY(0)',
    },
  },
  heroActions: {
    display: 'flex',
    alignItems: 'center',
    gap: '4px',
    padding: `${spacing.sm}px ${spacing.lg}px`,
    borderTop: `1px solid ${palette.borderLight}`,
    minHeight: '52px',
  },
  actionBtn: {
    display: 'inline-flex',
    alignItems: 'center',
    gap: '8px',
    padding: '8px 14px',
    borderRadius: radius.control,
    background: 'transparent',
    border: 'none',
    color: palette.textSecondary,
    fontFamily,
    fontSize: '14px',
    cursor: 'pointer',
    transition: 'background-color 150ms ease, color 150ms ease',

    '&:hover': {
      backgroundColor: palette.muted,
      color: palette.textPrimary,
    },
  },
  actionDivider: {
    width: '1px',
    height: '20px',
    backgroundColor: palette.borderLight,
    margin: '0 8px',
  },
  quickSection: {
    display: 'flex',
    flexDirection: 'column',
    gap: `${spacing.sm}px`,
  },
  quickHeader: {
    fontFamily,
    fontSize: '15px',
    fontWeight: 600,
    color: palette.textPrimary,
  },
  quickBody: {
    display: 'flex',
    alignItems: 'center',
    gap: `${spacing.sm}px`,
    border: `1px dashed ${palette.border}`,
    borderRadius: radius.card,
    padding: `${spacing.lg}px ${spacing.lg}px`,
    backgroundColor: 'rgba(255,255,255,0.5)',
    minHeight: '96px',
    flexWrap: 'wrap',
  },
  quickCard: {
    display: 'flex',
    alignItems: 'center',
    gap: '12px',
    padding: `${spacing.sm}px ${spacing.md}px`,
    borderRadius: radius.cardInner,
    backgroundColor: palette.backgroundElevated,
    border: `1px solid ${palette.borderLight}`,
    boxShadow: shadow.card,
    cursor: 'pointer',
    transition: 'box-shadow 150ms ease, transform 150ms ease',

    '&:hover': {
      boxShadow: shadow.popover,
      transform: 'translateY(-1px)',
    },
  },
  quickCardName: {
    fontFamily,
    fontSize: '14px',
    fontWeight: 600,
    color: palette.textPrimary,
    whiteSpace: 'nowrap',
  },
  quickCardMeta: {
    fontFamily,
    fontSize: '12px',
    color: palette.textMuted,
    marginTop: '2px',
  },
  quickAdd: {
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'center',
    gap: '8px',
    cursor: 'pointer',
    border: 'none',
    background: 'transparent',
    padding: '0 8px',
  },
  addCircle: {
    width: '44px',
    height: '44px',
    borderRadius: radius.circle,
    backgroundColor: palette.muted,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    color: palette.textSecondary,
    transition: 'background-color 150ms ease, color 150ms ease',

    '&:hover': {
      backgroundColor: palette.primarySoft,
      color: palette.primary,
    },
  },
  addLabel: {
    fontFamily,
    fontSize: '13px',
    color: palette.textMuted,
  },
});

interface QuickDevice {
  id: string;
  name: string;
  meta: string;
  online: boolean;
}

interface DevicePageProps {
  deviceName: string;
  online: boolean;
  connecting?: boolean;
  quickDevices?: QuickDevice[];
  onEnterDesktop?: () => void;
  onFileTransfer?: () => void;
  onMore?: () => void;
  onAddDevice?: () => void;
  onSelectQuick?: (id: string) => void;
}

/**
 * 截图主内容区：设备标题 + 在线状态胶囊，主卡片（壁纸预览 + 进入桌面 + 文件传输/更多），
 * 快速启动设备卡片 + 圆形"+"添加按钮。
 */
export const DevicePage: React.FC<DevicePageProps> = ({
  deviceName,
  online,
  connecting = false,
  quickDevices = [],
  onEnterDesktop,
  onFileTransfer,
  onMore,
  onAddDevice,
  onSelectQuick,
}) => {
  const styles = useStyles();

  return (
    <div className={styles.page}>
      <div className={styles.header}>
        <h1 className={styles.title}>{deviceName}</h1>
        <StatusBadge online={online} />
      </div>

      <div className={styles.heroCard}>
        <div className={styles.heroPreview}>
          <div className={styles.heroWallpaper} />
          <div className={styles.heroGlow} />
          <button
            type="button"
            className={styles.enterDesktop}
            onClick={onEnterDesktop}
            disabled={connecting || !online}
            style={connecting || !online ? { opacity: 0.6, cursor: 'not-allowed' } : undefined}
          >
            <DesktopRegular fontSize={16} />
            {connecting ? '连接中…' : '进入桌面'}
            <ArrowRightFilled fontSize={14} />
          </button>
        </div>
        <div className={styles.heroActions}>
          <button type="button" className={styles.actionBtn} onClick={onFileTransfer}>
            <DocumentArrowUpRegular fontSize={16} />
            文件传输
          </button>
          <span className={styles.actionDivider} />
          <button type="button" className={styles.actionBtn} onClick={onMore}>
            <MoreHorizontalRegular fontSize={16} />
            更多
          </button>
        </div>
      </div>

      <div className={styles.quickSection}>
        <div className={styles.quickHeader}>快速启动</div>
        <div className={styles.quickBody}>
          {quickDevices.map((device) => (
            <div key={device.id} className={styles.quickCard} onClick={() => onSelectQuick?.(device.id)}>
              <span
                style={{
                  width: 8,
                  height: 8,
                  borderRadius: '50%',
                  backgroundColor: device.online ? palette.online : palette.offline,
                  flexShrink: 0,
                }}
              />
              <div>
                <div className={styles.quickCardName}>{device.name}</div>
                <div className={styles.quickCardMeta}>{device.meta}</div>
              </div>
              <ArrowRightRegular fontSize={14} style={{ color: palette.textMuted }} />
            </div>
          ))}
          <button type="button" className={styles.quickAdd} onClick={onAddDevice}>
            <span className={styles.addCircle}>
              <AddRegular fontSize={20} />
            </span>
            <span className={styles.addLabel}>添加</span>
          </button>
        </div>
      </div>
    </div>
  );
};

export default DevicePage;
