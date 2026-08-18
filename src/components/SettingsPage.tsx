import React, { useState } from 'react';
import { makeStyles } from '@fluentui/react-components';
import {
  RocketRegular,
  PowerRegular,
  SleepRegular,
  ArrowSyncRegular,
  FolderOpenRegular,
  DragRegular,
  EditRegular,
  ChevronDownRegular,
} from '@fluentui/react-icons';
import { palette, fontFamily, spacing, radius, shadow } from '../theme/tokens';

const useStyles = makeStyles({
  page: {
    flex: 1,
    height: '100%',
    overflowY: 'auto',
    padding: `${spacing.xl}px ${spacing.xxl}px`,
    display: 'flex',
    flexDirection: 'column',
    gap: `${spacing.md}px`,
    maxWidth: '840px',
  },
  title: {
    fontFamily,
    fontSize: '24px',
    fontWeight: 700,
    color: palette.textPrimary,
    letterSpacing: '-0.02em',
    margin: 0,
    marginBottom: '4px',
  },
  tabs: {
    display: 'flex',
    gap: '24px',
    borderBottom: `1px solid ${palette.borderLight}`,
    marginBottom: '8px',
  },
  tab: {
    position: 'relative',
    padding: '10px 2px',
    border: 'none',
    background: 'transparent',
    fontFamily,
    fontSize: '14px',
    color: palette.textSecondary,
    cursor: 'pointer',
    transition: 'color 150ms ease',

    '&:hover': {
      color: palette.textPrimary,
    },
  },
  tabActive: {
    color: palette.textPrimary,
    fontWeight: 600,
  },
  tabUnderline: {
    position: 'absolute',
    left: 0,
    right: 0,
    bottom: '-1px',
    height: '2px',
    borderRadius: '1px',
    backgroundColor: palette.primary,
  },
  card: {
    backgroundColor: palette.backgroundElevated,
    borderRadius: radius.card,
    boxShadow: shadow.card,
    border: `1px solid ${palette.borderLight}`,
    overflow: 'hidden',
  },
  row: {
    display: 'flex',
    alignItems: 'center',
    gap: '12px',
    padding: '12px 16px',
    minHeight: '48px',
  },
  rowDivider: {
    height: '1px',
    backgroundColor: palette.borderLight,
    margin: '0 16px',
  },
  rowIcon: {
    display: 'flex',
    flexShrink: 0,
    color: palette.textPrimary,
  },
  rowBody: {
    flex: 1,
    minWidth: 0,
  },
  rowTitle: {
    fontFamily,
    fontSize: '14px',
    fontWeight: 600,
    color: palette.textPrimary,
  },
  rowDesc: {
    fontFamily,
    fontSize: '12px',
    color: palette.textMuted,
    marginTop: '2px',
    lineHeight: '18px',
  },
  link: {
    color: palette.primary,
    cursor: 'pointer',
    fontWeight: 500,

    '&:hover': {
      color: palette.primaryHover,
      textDecoration: 'underline',
    },
  },
  pathBox: {
    display: 'flex',
    alignItems: 'center',
    gap: '4px',
    minWidth: 0,
  },
  pathInput: {
    flex: 1,
    minWidth: 0,
    height: '30px',
    padding: '0 10px',
    backgroundColor: palette.background,
    border: `1px solid ${palette.borderLight}`,
    borderRadius: radius.control,
    fontFamily,
    fontSize: '12px',
    color: palette.textSecondary,
    outline: 'none',
    transition: 'border-color 150ms ease',

    '&:focus': {
      border: `1px solid ${palette.primary}`,
    },
  },
  pathIconBtn: {
    display: 'inline-flex',
    alignItems: 'center',
    justifyContent: 'center',
    width: '28px',
    height: '28px',
    border: 'none',
    background: 'transparent',
    borderRadius: radius.control,
    color: palette.textSecondary,
    cursor: 'pointer',
    flexShrink: 0,

    '&:hover': {
      backgroundColor: palette.muted,
      color: palette.textPrimary,
    },
  },
  selectWrap: {
    position: 'relative',
    display: 'flex',
    alignItems: 'center',
    minWidth: '200px',
  },
  select: {
    width: '100%',
    height: '32px',
    padding: '0 30px 0 12px',
    backgroundColor: palette.backgroundElevated,
    border: `1px solid ${palette.border}`,
    borderRadius: radius.control,
    fontFamily,
    fontSize: '13px',
    color: palette.textPrimary,
    appearance: 'none',
    cursor: 'pointer',
    outline: 'none',

    '&:focus': {
      border: `1px solid ${palette.primary}`,
    },
  },
  selectChevron: {
    position: 'absolute',
    right: '8px',
    pointerEvents: 'none',
    color: palette.textMuted,
    display: 'flex',
  },
});

const useSwitchStyles = makeStyles({
  wrap: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    flexShrink: 0,
  },
  track: {
    width: '40px',
    height: '22px',
    borderRadius: '999px',
    position: 'relative',
    cursor: 'pointer',
    border: 'none',
    transition: 'background-color 200ms ease',
    padding: 0,
  },
  thumb: {
    position: 'absolute',
    top: '2px',
    left: '2px',
    width: '18px',
    height: '18px',
    borderRadius: '50%',
    backgroundColor: '#fff',
    boxShadow: '0 1px 2px rgba(0,0,0,0.2)',
    transition: 'transform 200ms ease',
  },
  label: {
    fontFamily,
    fontSize: '12px',
    color: palette.textSecondary,
    width: '14px',
  },
});

interface ToggleSwitchProps {
  on: boolean;
  onChange: (next: boolean) => void;
}

/** 截图风格开关：蓝色轨道(开) / 灰色轨道(关) + 右侧「开/关」文字 */
export const ToggleSwitch: React.FC<ToggleSwitchProps> = ({ on, onChange }) => {
  const styles = useSwitchStyles();
  return (
    <div className={styles.wrap}>
      <button
        type="button"
        className={styles.track}
        style={{ backgroundColor: on ? palette.primary : '#D5DBE3' }}
        onClick={() => onChange(!on)}
        role="switch"
        aria-checked={on}
        aria-label={on ? '开' : '关'}
      >
        <span
          className={styles.thumb}
          style={{ transform: on ? 'translateX(18px)' : 'translateX(0)' }}
        />
      </button>
      <span className={styles.label}>{on ? '开' : '关'}</span>
    </div>
  );
};

const settings: Record<string, { title: string; desc?: string; icon: React.ReactNode }[]> = {
  安全: [
    { title: '连接前锁定屏幕', icon: <ShieldRegularIcon /> },
    { title: '每次连接需要验证码', icon: <PowerRegular fontSize={16} /> },
  ],
  键盘: [
    { title: '发送 Ctrl 组合键到远端', icon: <RocketRegular fontSize={16} /> },
    { title: '将 Win 键发送到远端', icon: <PowerRegular fontSize={16} /> },
  ],
  网络: [
    { title: '使用中继服务器（UDP Relay）', icon: <ArrowSyncRegular fontSize={16} /> },
    { title: '优先使用 TCP 直连', icon: <DragRegular fontSize={16} /> },
  ],
};

function ShieldRegularIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
      <path d="M12 3l7 3v6c0 4.4-3 7.5-7 9-4-1.5-7-4.6-7-9V6l7-3z" />
    </svg>
  );
}

type TabKey = '常规' | '安全' | '键盘' | '网络';

/**
 * 截图「设置」界面：左侧为应用导航（App 侧边栏），右侧为设置内容区，
 * 顶部「常规/安全/键盘/网络」标签页 + 卡片式设置项（开关/路径/下拉）。
 */
export const SettingsPage: React.FC = () => {
  const styles = useStyles();
  const [tab, setTab] = useState<TabKey>('常规');
  const [toggles, setToggles] = useState<Record<string, boolean>>({
    autostart: true,
    wakeup: false,
    sleep: true,
    autoupdate: false,
    dragfloat: true,
    lock: true,
    password: false,
    ctrl: true,
    win: false,
    relay: true,
    tcp: false,
  });

  const toggle = (key: string) => setToggles((prev) => ({ ...prev, [key]: !prev[key] }));

  return (
    <div className={styles.page}>
      <h1 className={styles.title}>设置</h1>

      <div className={styles.tabs}>
        {(['常规', '安全', '键盘', '网络'] as TabKey[]).map((t) => (
          <button
            key={t}
            type="button"
            className={t === tab ? `${styles.tab} ${styles.tabActive}` : styles.tab}
            onClick={() => setTab(t)}
          >
            {t}
            {t === tab && <span className={styles.tabUnderline} />}
          </button>
        ))}
      </div>

      {tab === '常规' && (
        <>
          <div className={styles.card}>
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <RocketRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>开机自动启动</div>
              </div>
              <ToggleSwitch on={toggles.autostart} onChange={() => toggle('autostart')} />
            </div>
            <div className={styles.rowDivider} />
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <PowerRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>允许通过远程开机启动</div>
                <div className={styles.rowDesc}>
                  协助配置本设备进行远程开机 <span className={styles.link}>开始协助</span>
                </div>
              </div>
              <ToggleSwitch on={toggles.wakeup} onChange={() => toggle('wakeup')} />
            </div>
            <div className={styles.rowDivider} />
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <SleepRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>防止电脑休眠</div>
                <div className={styles.rowDesc}>休眠将导致电脑无法远程控制（强烈推荐开启）</div>
              </div>
              <ToggleSwitch on={toggles.sleep} onChange={() => toggle('sleep')} />
            </div>
          </div>

          <div className={styles.card}>
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <ArrowSyncRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>自动更新</div>
                <div className={styles.rowDesc}>开启后会在电脑闲时自动更新，避免打扰（电脑休眠时无法更新）</div>
              </div>
              <ToggleSwitch on={toggles.autoupdate} onChange={() => toggle('autoupdate')} />
            </div>
            <div className={styles.rowDivider} />
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <DragRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>被控时拖拽文件显示发送浮窗</div>
                <div className={styles.rowDesc}>开启后，可在被控时向主控端发送文件</div>
              </div>
              <ToggleSwitch on={toggles.dragfloat} onChange={() => toggle('dragfloat')} />
            </div>
          </div>

          <div className={styles.card}>
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <FolderOpenRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>移动设备传输文件存储路径</div>
              </div>
              <div className={styles.pathBox}>
                <input
                  className={styles.pathInput}
                  defaultValue={'C:\\Program Files\\Netease\\GameViewer\\Download'}
                  readOnly
                />
                <button type="button" className={styles.pathIconBtn} aria-label="编辑路径">
                  <EditRegular fontSize={14} />
                </button>
                <button type="button" className={styles.pathIconBtn} aria-label="打开目录">
                  <FolderOpenRegular fontSize={14} />
                </button>
              </div>
            </div>
            <div className={styles.rowDivider} />
            <div className={styles.row}>
              <span className={styles.rowIcon}>
                <RocketRegular fontSize={16} />
              </span>
              <div className={styles.rowBody}>
                <div className={styles.rowTitle}>关闭窗口时</div>
              </div>
              <div className={styles.selectWrap}>
                <select className={styles.select} defaultValue="隐藏至菜单栏">
                  <option>隐藏至菜单栏</option>
                  <option>退出程序</option>
                </select>
                <span className={styles.selectChevron}>
                  <ChevronDownRegular fontSize={12} />
                </span>
              </div>
            </div>
          </div>
        </>
      )}

      {(tab === '安全' || tab === '键盘' || tab === '网络') && (
        <div className={styles.card}>
          {(settings[tab] ?? []).map((row, idx) => (
            <React.Fragment key={row.title}>
              {idx > 0 && <div className={styles.rowDivider} />}
              <div className={styles.row}>
                <span className={styles.rowIcon}>{row.icon}</span>
                <div className={styles.rowBody}>
                  <div className={styles.rowTitle}>{row.title}</div>
                  {row.desc && <div className={styles.rowDesc}>{row.desc}</div>}
                </div>
                <ToggleSwitch on={toggles[row.title] ?? false} onChange={() => toggle(row.title)} />
              </div>
            </React.Fragment>
          ))}
        </div>
      )}
    </div>
  );
};

export default SettingsPage;
