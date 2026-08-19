import React from 'react';
import { makeStyles } from '@fluentui/react-components';
import {
  ArrowLeftRegular,
  ArrowSyncRegular,
  CopyRegular,
  DismissRegular,
  SubtractRegular,
  SettingsRegular,
  MoreHorizontalRegular,
} from '@fluentui/react-icons';
import { IconButton } from './shared/IconButton';
import { palette, fontFamily, radius, titleBarHeight, zIndex } from '../theme/tokens';
import { minimizeWindow, closeWindow } from '../services/window';

const useStyles = makeStyles({
  bar: {
    height: `${titleBarHeight}px`,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    background: 'rgba(255, 255, 255, 0.92)',
    borderBottom: `1px solid ${palette.borderLight}`,
    userSelect: 'none',
    zIndex: zIndex.titleBar,
    position: 'relative',
  },
  left: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    paddingLeft: '8px',
    minWidth: 0,
  },
  appGroup: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    paddingLeft: '4px',
  },
  appIcon: {
    width: '18px',
    height: '18px',
    borderRadius: '5px',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    flexShrink: 0,
  },
  appName: {
    fontFamily,
    fontSize: '14px',
    fontWeight: 600,
    color: palette.textPrimary,
    whiteSpace: 'nowrap',
  },
  right: {
    display: 'flex',
    alignItems: 'center',
    gap: '2px',
    height: '100%',
  },
  iconBtns: {
    display: 'flex',
    alignItems: 'center',
    gap: '2px',
    paddingRight: '8px',
  },
  divider: {
    width: '1px',
    height: '20px',
    backgroundColor: palette.borderLight,
    margin: '0 4px',
  },
  avatar: {
    width: '26px',
    height: '26px',
    borderRadius: radius.circle,
    background: `linear-gradient(135deg, ${palette.primary}, ${palette.primaryActive})`,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    color: '#fff',
    fontSize: '12px',
    fontWeight: 600,
    cursor: 'pointer',
    flexShrink: 0,
    marginLeft: '6px',
  },
  winControls: {
    display: 'flex',
    alignItems: 'center',
    height: '100%',
  },
  winBtn: {
    width: '44px',
    height: '100%',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    border: 'none',
    background: 'transparent',
    color: palette.textSecondary,
    cursor: 'pointer',
    fontSize: '13px',
    transition: 'background-color 150ms ease, color 150ms ease',
  },
  winBtnClose: {
    '&:hover': {
      backgroundColor: '#E81123',
      color: '#ffffff',
    },
  },
});

interface TitleBarProps {
  onBack?: () => void;
  onRefresh?: () => void;
  onSettings?: () => void;
  onMinimize?: () => void;
  onClose?: () => void;
}

/**
 * 截图顶部栏：左侧返回 + 蓝色应用图标 + 应用名；右侧刷新/复制/通知/头像/菜单与窗口控制。
 */
export const TitleBar: React.FC<TitleBarProps> = ({
  onBack,
  onRefresh,
  onSettings,
  onMinimize,
  onClose,
}) => {
  const styles = useStyles();

  const handleMinimize = () => void (onMinimize ? onMinimize() : minimizeWindow());
  const handleClose = () => void (onClose ? onClose() : closeWindow());

  return (
    <div className={styles.bar} data-tauri-drag-region="">
      <div className={styles.left}>
        {onBack && (
          <IconButton label="返回" onClick={onBack} dragExclude>
            <ArrowLeftRegular fontSize={18} />
          </IconButton>
        )}
        <div className={styles.appGroup}>
          <span className={styles.appIcon}>
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" aria-hidden>
              <rect x="4" y="4" width="16" height="16" rx="4" fill={palette.primary} />
              <path d="M9 12h6M12 9v6" stroke="#fff" strokeWidth="2" strokeLinecap="round" />
            </svg>
          </span>
          <span className={styles.appName}>desktopcr</span>
        </div>
      </div>

      <div className={styles.right}>
        <div className={styles.iconBtns}>
          {onRefresh && (
            <IconButton label="刷新" onClick={onRefresh} dragExclude>
              <ArrowSyncRegular fontSize={16} />
            </IconButton>
          )}
          <IconButton label="复制窗口信息" onClick={() => {}} dragExclude>
            <CopyRegular fontSize={16} />
          </IconButton>
          <IconButton label="设置" onClick={onSettings} dragExclude>
            <SettingsRegular fontSize={16} />
          </IconButton>
          <span className={styles.avatar} title="账户" data-tauri-drag-region="no">
            U
          </span>
          <IconButton label="更多" onClick={() => {}} dragExclude>
            <MoreHorizontalRegular fontSize={16} />
          </IconButton>
        </div>

        <div className={styles.divider} />

        <div className={styles.winControls}>
          <button
            type="button"
            className={styles.winBtn}
            onClick={handleMinimize}
            aria-label="最小化"
            data-tauri-drag-region="no"
          >
            <SubtractRegular fontSize={14} />
          </button>
          <button
            type="button"
            className={`${styles.winBtn} ${styles.winBtnClose}`}
            onClick={handleClose}
            aria-label="关闭"
            data-tauri-drag-region="no"
          >
            <DismissRegular fontSize={14} />
          </button>
        </div>
      </div>
    </div>
  );
};

export default TitleBar;
