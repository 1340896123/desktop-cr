import React, { useState } from 'react';
import { makeStyles } from '@fluentui/react-components';
import {
  ArrowLeftRegular,
  ArrowSyncRegular,
  CopyRegular,
  DismissRegular,
  NavigationRegular,
  PersonRegular,
  ServiceBellRegular,
  SquareRegular,
  SubtractRegular,
  WindowAppsRegular,
} from '@fluentui/react-icons';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { IconButton } from './shared/IconButton';
import { fontFamily, titleBarHeight, zIndex } from '../theme/tokens';
import { minimizeWindow, closeWindow } from '../services/window';

const UU_BLUE = '#0066ff';

const useStyles = makeStyles({
  bar: {
    height: `${titleBarHeight}px`,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    background: '#ffffff',
    borderBottom: '1px solid #f3f4f6',
    userSelect: 'none',
    zIndex: zIndex.titleBar,
    position: 'relative',
    flexShrink: 0,
  },
  left: {
    display: 'flex',
    alignItems: 'center',
    gap: '10px',
    paddingLeft: '10px',
    minWidth: 0,
  },
  appGroup: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    paddingLeft: '2px',
  },
  appIcon: {
    width: '20px',
    height: '20px',
    borderRadius: '6px',
    backgroundColor: UU_BLUE,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    flexShrink: 0,
    transform: 'rotate(12deg)',
    boxShadow: '0 1px 2px rgba(0,102,255,0.35)',
  },
  appName: {
    fontFamily,
    fontSize: '14px',
    fontWeight: 700,
    color: '#1F2937',
    letterSpacing: '-0.01em',
    whiteSpace: 'nowrap',
  },
  right: {
    display: 'flex',
    alignItems: 'center',
    height: '100%',
  },
  iconBtns: {
    display: 'flex',
    alignItems: 'center',
    gap: '2px',
    paddingRight: '4px',
    paddingLeft: '4px',
  },
  iconBtnWrap: {
    position: 'relative',
    display: 'inline-flex',
  },
  bellBadge: {
    position: 'absolute',
    top: '2px',
    right: '2px',
    width: '14px',
    height: '14px',
    borderRadius: '50%',
    backgroundColor: '#EF4444',
    color: '#ffffff',
    fontSize: '9px',
    fontWeight: 700,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    lineHeight: 1,
    pointerEvents: 'none',
  },
  noticePopover: {
    position: 'absolute',
    right: '0',
    top: 'calc(100% + 8px)',
    width: '256px',
    backgroundColor: '#ffffff',
    borderRadius: '8px',
    boxShadow: '0 8px 24px rgba(16,24,40,0.12), 0 2px 6px rgba(16,24,40,0.06)',
    border: '1px solid #f3f4f6',
    padding: '12px',
    zIndex: zIndex.modal,
    textAlign: 'left',
  },
  noticeHeader: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    borderBottom: '1px solid #f3f4f6',
    paddingBottom: '8px',
    marginBottom: '8px',
  },
  noticeTitle: {
    fontFamily,
    fontSize: '12px',
    fontWeight: 600,
    color: '#374151',
  },
  noticeReadAll: {
    fontFamily,
    fontSize: '12px',
    color: UU_BLUE,
    cursor: 'pointer',
    background: 'none',
    border: 'none',
    padding: 0,
  },
  noticeItem: {
    padding: '8px',
    borderRadius: '6px',
    fontSize: '12px',
    color: '#4B5563',
  },
  noticeItemBlue: {
    backgroundColor: '#EFF6FF',
  },
  noticeItemGray: {
    backgroundColor: '#F9FAFB',
  },
  noticeItemTitle: {
    fontFamily,
    fontWeight: 500,
    color: '#1F2937',
  },
  noticeItemDesc: {
    fontFamily,
    fontSize: '11px',
    color: '#6B7280',
    marginTop: '2px',
  },
  divider: {
    width: '1px',
    height: '18px',
    backgroundColor: '#E5E7EB',
    margin: '0 2px',
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
    color: '#6B7280',
    cursor: 'pointer',
    fontSize: '13px',
    transition: 'background-color 150ms ease, color 150ms ease',

    '&:hover': {
      backgroundColor: '#F3F4F6',
      color: '#1F2937',
    },
  },
  winBtnClose: {
    '&:hover': {
      backgroundColor: '#E81123',
      color: '#ffffff',
    },
  },
  spin: {
    animationName: {
      from: { transform: 'rotate(0deg)' },
      to: { transform: 'rotate(360deg)' },
    },
    animationDuration: '800ms',
    animationIterationCount: 'infinite',
    animationTimingFunction: 'linear',
    display: 'inline-flex',
  },
});

interface TitleBarProps {
  onBack?: () => void;
  onRefresh?: () => void;
  onShowToast?: (msg: string) => void;
  /** 当前窗口是否最大化（由 App 经 onWindowMaximizedChange 维护） */
  maximized?: boolean;
}

/**
 * 顶部栏：左侧返回 + 蓝色旋转方块图标 + 应用名「网易UU远程」；
 * 右侧刷新 / 独立窗口 / 通知铃铛（红底徽标 + 消息通知弹层）/ 用户 / 汉堡 /
 * 细分隔线 / 最小化 / 最大化还原 / 关闭（hover 变红）。
 */
export const TitleBar: React.FC<TitleBarProps> = ({ onBack, onRefresh, onShowToast, maximized = false }) => {
  const styles = useStyles();
  const [refreshing, setRefreshing] = useState(false);
  const [noticeOpen, setNoticeOpen] = useState(false);
  const [unread, setUnread] = useState(2);

  const handleRefresh = () => {
    if (!onRefresh) return;
    setRefreshing(true);
    onRefresh();
    window.setTimeout(() => {
      setRefreshing(false);
      onShowToast?.('设备及列表已刷新');
    }, 800);
  };

  const handleToggleMaximize = () => {
    if (typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)) {
      onShowToast?.(maximized ? '退出全屏' : '切换全屏模式');
      return;
    }
    void getCurrentWindow().toggleMaximize();
  };

  const handleClose = () => void closeWindow();

  return (
    <div className={styles.bar} data-tauri-drag-region="deep">
      <div className={styles.left}>
        {onBack && (
          <IconButton label="返回" onClick={onBack} dragExclude>
            <ArrowLeftRegular fontSize={18} />
          </IconButton>
        )}
        <div className={styles.appGroup}>
          <span className={styles.appIcon}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" aria-hidden>
              <rect x="3" y="3" width="14" height="14" rx="3" fill="#ffffff" opacity="0.85" />
              <rect x="8" y="8" width="13" height="13" rx="3" fill="#ffffff" />
            </svg>
          </span>
          <span className={styles.appName}>网易UU远程</span>
        </div>
      </div>

      <div className={styles.right}>
        <div className={styles.iconBtns}>
          <IconButton label="刷新" onClick={handleRefresh} dragExclude>
            <span className={refreshing ? styles.spin : undefined}>
              <ArrowSyncRegular fontSize={16} />
            </span>
          </IconButton>
          <IconButton
            label="独立窗口"
            onClick={() => onShowToast?.('弹出独立窗口')}
            dragExclude
          >
            <WindowAppsRegular fontSize={16} />
          </IconButton>

          <span className={styles.iconBtnWrap}>
            <IconButton label="消息通知" onClick={() => setNoticeOpen((prev) => !prev)} dragExclude>
              <ServiceBellRegular fontSize={16} />
            </IconButton>
            {unread > 0 && <span className={styles.bellBadge}>{unread}</span>}
            {noticeOpen && (
              <div className={styles.noticePopover}>
                <div className={styles.noticeHeader}>
                  <span className={styles.noticeTitle}>消息通知</span>
                  <button
                    type="button"
                    className={styles.noticeReadAll}
                    onClick={() => {
                      setUnread(0);
                      setNoticeOpen(false);
                    }}
                  >
                    全部已读
                  </button>
                </div>
                <div className={styles.noticeItemBlue}>
                  <div className={styles.noticeItemTitle}>设备在线通知</div>
                  <div className={styles.noticeItemDesc}>AAAAA 设备已上线</div>
                </div>
                <div className={styles.noticeItemGray}>
                  <div className={styles.noticeItemTitle}>安全提示</div>
                  <div className={styles.noticeItemDesc}>已开启临时验证码防护</div>
                </div>
              </div>
            )}
          </span>

          <IconButton label="用户" onClick={() => onShowToast?.('用户个人中心')} dragExclude>
            <PersonRegular fontSize={16} />
          </IconButton>
          <IconButton label="菜单" onClick={() => onShowToast?.('菜单功能开发中')} dragExclude>
            <NavigationRegular fontSize={16} />
          </IconButton>
          <span className={styles.divider} />
        </div>

        <div className={styles.winControls}>
          <button
            type="button"
            className={styles.winBtn}
            onClick={() => void minimizeWindow()}
            aria-label="最小化"
            data-tauri-drag-region="false"
          >
            <SubtractRegular fontSize={14} />
          </button>
          <button
            type="button"
            className={styles.winBtn}
            onClick={handleToggleMaximize}
            aria-label={maximized ? '还原窗口' : '最大化'}
            data-tauri-drag-region="false"
          >
            {maximized ? <CopyRegular fontSize={14} /> : <SquareRegular fontSize={13} />}
          </button>
          <button
            type="button"
            className={`${styles.winBtn} ${styles.winBtnClose}`}
            onClick={handleClose}
            aria-label="关闭"
            data-tauri-drag-region="false"
          >
            <DismissRegular fontSize={14} />
          </button>
        </div>
      </div>
    </div>
  );
};

export default TitleBar;