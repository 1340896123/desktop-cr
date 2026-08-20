import React from 'react';
import { makeStyles } from '@fluentui/react-components';
import {
  AddRegular,
  ArrowRightFilled,
  DesktopRegular,
  DocumentFolderRegular,
  FolderOpenRegular,
  MoreHorizontalRegular,
} from '@fluentui/react-icons';
import { StatusBadge } from './shared/IconButton';
import { fontFamily, spacing, radius, shadow } from '../theme/tokens';
import { isTauri } from '../services/connection';

const useStyles = makeStyles({
  page: {
    flex: 1,
    height: '100%',
    overflowY: 'auto',
    padding: `${spacing.xl}px ${spacing.xxl}px`,
    display: 'flex',
    flexDirection: 'column',
    gap: `${spacing.section}px`,
    alignItems: 'center',
  },
  inner: {
    width: '100%',
    maxWidth: '896px',
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
    color: '#111827',
    letterSpacing: '-0.02em',
    lineHeight: '32px',
    margin: 0,
  },
  mockBadge: {
    display: 'inline-flex',
    alignItems: 'center',
    padding: '2px 10px',
    borderRadius: radius.pill,
    backgroundColor: '#FFF7E6',
    border: '1px solid rgba(245, 158, 11, 0.35)',
    color: '#B45309',
    fontFamily,
    fontSize: '12px',
    fontWeight: 600,
    whiteSpace: 'nowrap',
  },
  heroCard: {
    backgroundColor: '#ffffff',
    borderRadius: '16px',
    boxShadow: shadow.card,
    overflow: 'hidden',
    border: '1px solid rgba(229, 231, 235, 0.8)',
  },
  heroPreview: {
    position: 'relative',
    height: '264px',
    background: 'linear-gradient(135deg, #1e3c72 0%, #2a5298 100%)',
    overflow: 'hidden',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    cursor: 'pointer',
  },
  wallpaperGraphic: {
    position: 'absolute',
    inset: 0,
    background: 'radial-gradient(circle at 70% 30%, #3b82f6 0%, #1d4ed8 40%, #0f172a 100%)',
    opacity: 0.85,
  },
  waves: {
    position: 'absolute',
    inset: 0,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    color: 'rgba(96, 165, 250, 0.3)',
  },
  enterDesktop: {
    position: 'relative',
    zIndex: 2,
    display: 'inline-flex',
    alignItems: 'center',
    gap: '8px',
    padding: '10px 24px',
    borderRadius: radius.pill,
    backgroundColor: 'rgba(0, 0, 0, 0.2)',
    backdropFilter: 'blur(8px)',
    border: '1px solid rgba(255,255,255,0.2)',
    color: '#ffffff',
    fontFamily,
    fontSize: '16px',
    fontWeight: 600,
    cursor: 'pointer',
    boxShadow: shadow.popover,
    transition: 'background-color 150ms ease, transform 150ms ease',

    '&:hover': {
      backgroundColor: 'rgba(0, 0, 0, 0.4)',
      transform: 'scale(1.05)',
    },
  },
  heroActions: {
    display: 'flex',
    alignItems: 'stretch',
    padding: '12px 0',
    backgroundColor: '#ffffff',
  },
  actionBtn: {
    flex: 1,
    minWidth: 0,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    gap: '8px',
    padding: '6px 8px',
    background: 'transparent',
    border: 'none',
    color: '#374151',
    fontFamily,
    fontSize: '14px',
    fontWeight: 500,
    whiteSpace: 'nowrap',
    cursor: 'pointer',
    transition: 'color 150ms ease',

    '&:hover': {
      color: '#0066ff',
    },
  },
  actionDivider: {
    width: '1px',
    flexShrink: 0,
    backgroundColor: '#F3F4F6',
  },
  quickSection: {
    display: 'flex',
    flexDirection: 'column',
    gap: `${spacing.sm}px`,
  },
  quickHeader: {
    fontFamily,
    fontSize: '12px',
    fontWeight: 600,
    color: '#6B7280',
    letterSpacing: '0.08em',
  },
  quickBody: {
    display: 'flex',
    alignItems: 'center',
    gap: `${spacing.md}px`,
    border: '2px dashed #E5E7EB',
    borderRadius: radius.card,
    padding: `${spacing.xl}px`,
    backgroundColor: 'rgba(249, 250, 251, 0.5)',
    minHeight: '96px',
    flexWrap: 'wrap',
    transition: 'border-color 150ms ease',

    '&:hover': {
      border: '2px dashed #93C5FD',
    },
  },
  quickCard: {
    display: 'flex',
    alignItems: 'center',
    gap: '10px',
    padding: `${spacing.sm}px ${spacing.md}px`,
    borderRadius: radius.cardInner,
    backgroundColor: '#ffffff',
    border: `1px solid ${'#E5E7EB'}`,
    boxShadow: shadow.card,
    cursor: 'pointer',
    transition: 'box-shadow 150ms ease, transform 150ms ease',

    '&:hover': {
      boxShadow: shadow.popover,
      transform: 'translateY(-1px)',
    },
  },
  quickCardIcon: {
    width: '28px',
    height: '28px',
    borderRadius: '6px',
    backgroundColor: '#E8F1FF',
    color: '#0066ff',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    flexShrink: 0,
  },
  quickCardName: {
    fontFamily,
    fontSize: '13px',
    fontWeight: 600,
    color: '#111827',
    whiteSpace: 'nowrap',
  },
  quickCardMeta: {
    fontFamily,
    fontSize: '11px',
    color: '#8A94A6',
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
    flex: 1,
    justifyContent: 'center',
    minHeight: '72px',
  },
  addCircle: {
    width: '40px',
    height: '40px',
    borderRadius: radius.circle,
    backgroundColor: '#E5E7EB',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    color: '#6B7280',
    transition: 'background-color 150ms ease, color 150ms ease',

    '&:hover': {
      backgroundColor: '#0066ff',
      color: '#ffffff',
    },
  },
  addLabel: {
    fontFamily,
    fontSize: '12px',
    color: '#6B7280',
    fontWeight: 500,
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
 * 设备主页：在线胶囊 + 大标题，hero 渐变壁纸卡（内嵌 SVG 波浪 + 玻璃拟态「进入桌面」按钮），
 * 下方「文件传输 / …更多」双栏操作区，快速启动虚线添加框。
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
  const isMock = !isTauri();
  const disabled = connecting || !online;

  return (
    <div className={styles.page}>
      <div className={styles.inner}>
        <div className={styles.header}>
          <StatusBadge online={online} />
          <h1 className={styles.title}>{deviceName}</h1>
          {isMock && (
            <span className={styles.mockBadge} title="浏览器环境,显示模拟数据">模拟模式</span>
          )}
        </div>

        <div className={styles.heroCard}>
          <div className={styles.heroPreview} onClick={onEnterDesktop}>
            <div className={styles.wallpaperGraphic} />
            <div className={styles.waves}>
              <svg width="100%" height="100%" viewBox="0 0 500 300" preserveAspectRatio="none">
                <path d="M0,150 Q125,50 250,150 T500,150 L500,300 L0,300 Z" fill="currentColor" />
                <path d="M0,200 Q150,100 300,200 T500,200 L500,300 L0,300 Z" fill="rgba(59, 130, 246, 0.4)" />
              </svg>
            </div>
            <button
              type="button"
              className={styles.enterDesktop}
              onClick={onEnterDesktop}
              disabled={disabled}
              style={disabled ? { opacity: 0.6, cursor: 'not-allowed' } : undefined}
            >
              <DesktopRegular fontSize={16} />
              {connecting ? '连接中…' : '进入桌面'}
              <ArrowRightFilled fontSize={14} />
            </button>
          </div>
          <div className={styles.heroActions}>
            <button type="button" className={styles.actionBtn} onClick={onFileTransfer}>
              <FolderOpenRegular fontSize={16} style={{ color: '#4B5563' }} />
              文件传输
            </button>
            <span className={styles.actionDivider} />
            <button type="button" className={styles.actionBtn} onClick={onMore}>
              <MoreHorizontalRegular fontSize={16} style={{ color: '#4B5563' }} />
              更多
            </button>
          </div>
        </div>

        <div className={styles.quickSection}>
          <div className={styles.quickHeader}>快速启动</div>
          <div className={styles.quickBody}>
            {quickDevices.map((device) => (
              <div key={device.id} className={styles.quickCard} onClick={() => onSelectQuick?.(device.id)}>
                <span className={styles.quickCardIcon}>
                  <DocumentFolderRegular fontSize={16} />
                </span>
                <div>
                  <div className={styles.quickCardName}>{device.name}</div>
                  <div className={styles.quickCardMeta}>{device.meta}</div>
                </div>
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
    </div>
  );
};

export default DevicePage;