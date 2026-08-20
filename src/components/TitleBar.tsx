import React, { useState } from 'react';
import { makeStyles } from '@fluentui/react-components';
import {
  ArrowSyncRegular,
  NavigationRegular,
  PersonRegular,
  ServiceBellRegular,
  WindowAppsRegular,
} from '@fluentui/react-icons';
import { IconButton } from './shared/IconButton';
import { WindowControls } from './shared/WindowControls';
import { UnsupportedTag } from './shared/UnsupportedTag';
import { fontFamily, palette, titleBarHeight, zIndex } from '../theme/tokens';

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
  userPopover: {
    position: 'absolute',
    right: '0',
    top: 'calc(100% + 8px)',
    width: '264px',
    backgroundColor: '#ffffff',
    borderRadius: '8px',
    boxShadow: '0 8px 24px rgba(16,24,40,0.12), 0 2px 6px rgba(16,24,40,0.06)',
    border: '1px solid #f3f4f6',
    padding: '12px',
    zIndex: zIndex.modal,
    textAlign: 'left',
  },
  userHeader: {
    display: 'flex',
    alignItems: 'center',
    gap: '10px',
    borderBottom: '1px solid #f3f4f6',
    paddingBottom: '10px',
    marginBottom: '10px',
  },
  userAvatar: {
    width: '34px',
    height: '34px',
    borderRadius: '50%',
    backgroundColor: UU_BLUE,
    color: '#ffffff',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    fontFamily,
    fontSize: '15px',
    fontWeight: 700,
    flexShrink: 0,
  },
  userName: {
    fontFamily,
    fontSize: '14px',
    fontWeight: 600,
    color: '#1F2937',
    whiteSpace: 'nowrap',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
  },
  userServer: {
    fontFamily,
    fontSize: '11px',
    color: '#6B7280',
    marginTop: '2px',
    whiteSpace: 'nowrap',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
  },
  userInfoRow: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '6px 0',
    fontFamily,
    fontSize: '12px',
    color: '#4B5563',
  },
  userInfoLabel: {
    color: '#9CA3AF',
    flexShrink: 0,
  },
  userInfoValue: {
    color: '#1F2937',
    whiteSpace: 'nowrap',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
  },
  logoutBtn: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    gap: '6px',
    width: '100%',
    marginTop: '10px',
    padding: '8px 0',
    borderRadius: '6px',
    backgroundColor: palette.destructive,
    color: '#ffffff',
    fontFamily,
    fontSize: '13px',
    fontWeight: 600,
    border: 'none',
    cursor: 'pointer',
    transition: 'opacity 150ms ease',
    '&:hover': {
      opacity: 0.9,
    },
  },
  divider: {
    width: '1px',
    height: '18px',
    backgroundColor: '#E5E7EB',
    margin: '0 2px',
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
  onRefresh?: () => void;
  onShowToast?: (msg: string) => void;
  /** 当前登录账号信息(用于用户中心展示) */
  account?: { username: string; server: string; token: string } | null;
  /** 退出登录回调 */
  onLogout?: () => void;
}

/**
 * 顶部栏：左侧返回 + 蓝色旋转方块图标 + 应用名「网易UU远程」；
 * 右侧刷新 / 独立窗口 / 通知铃铛（红底徽标 + 消息通知弹层）/ 用户（账号信息 + 登出浮层）/ 汉堡 /
 * 细分隔线 / 窗口控制组（最小化 / 最大化还原 / 关闭，hover 变红）。
 */
export const TitleBar: React.FC<TitleBarProps> = ({ onRefresh, onShowToast, account, onLogout }) => {
  const styles = useStyles();
  const [refreshing, setRefreshing] = useState(false);
  const [noticeOpen, setNoticeOpen] = useState(false);
  const [userOpen, setUserOpen] = useState(false);
  const [unread, setUnread] = useState(2);

  const handleLogout = () => {
    setUserOpen(false);
    onLogout?.();
  };

  const handleRefresh = () => {
    if (!onRefresh) return;
    setRefreshing(true);
    onRefresh();
    window.setTimeout(() => {
      setRefreshing(false);
      onShowToast?.('设备及列表已刷新');
    }, 800);
  };

  return (
    <div className={styles.bar} data-tauri-drag-region="deep">
      <div className={styles.left}>
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
            onClick={() => onShowToast?.('「独立窗口」功能暂未开放')}
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
                  <span className={styles.noticeTitle}>
                    消息通知
                    <UnsupportedTag label="演示" variant="demo" />
                  </span>
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
                  <div className={styles.noticeItemDesc}>AAAAA 设备已上线（演示数据）</div>
                </div>
                <div className={styles.noticeItemGray}>
                  <div className={styles.noticeItemTitle}>安全提示</div>
                  <div className={styles.noticeItemDesc}>已开启临时验证码防护（演示数据）</div>
                </div>
                <div className={styles.noticeItemGray} style={{ fontSize: '11px', color: '#9CA3AF' }}>
                  以上为界面演示通知，非真实消息
                </div>
              </div>
            )}
          </span>

          <span className={styles.iconBtnWrap}>
            <IconButton label="用户" onClick={() => setUserOpen((prev) => !prev)} dragExclude>
              <PersonRegular fontSize={16} />
            </IconButton>
            {userOpen && (
              <div className={styles.userPopover}>
                <div className={styles.userHeader}>
                  <span className={styles.userAvatar}>
                    {(account?.username ?? '?').charAt(0).toUpperCase()}
                  </span>
                  <div style={{ minWidth: 0 }}>
                    <div className={styles.userName}>{account?.username ?? '未登录'}</div>
                    <div className={styles.userServer}>{account?.server ?? '暂无服务地址'}</div>
                  </div>
                </div>
                <div className={styles.userInfoRow}>
                  <span className={styles.userInfoLabel}>账号状态</span>
                  <span className={styles.userInfoValue} style={{ color: palette.online }}>
                    {account ? '已登录' : '未登录'}
                  </span>
                </div>
                <div className={styles.userInfoRow}>
                  <span className={styles.userInfoLabel}>令牌</span>
                  <span className={styles.userInfoValue}>
                    {account?.token
                      ? `${account.token.slice(0, 8)}…${account.token.slice(-4)}`
                      : '—'}
                  </span>
                </div>
                <button type="button" className={styles.logoutBtn} onClick={handleLogout}>
                  <span aria-hidden>↪</span>
                  退出登录
                </button>
              </div>
            )}
          </span>
          <IconButton label="菜单" onClick={() => onShowToast?.('菜单功能暂未开放')} dragExclude>
            <NavigationRegular fontSize={16} />
          </IconButton>
          <span className={styles.divider} />
        </div>

        <WindowControls onToggleMaximize={() => onShowToast?.('切换全屏模式')} />
      </div>
    </div>
  );
};

export default TitleBar;