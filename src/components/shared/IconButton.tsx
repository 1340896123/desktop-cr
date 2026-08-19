import React from 'react';
import { makeStyles } from '@fluentui/react-components';
import { palette, radius, zIndex } from '../../theme/tokens';

const useStyles = makeStyles({
  button: {
    display: 'inline-flex',
    alignItems: 'center',
    justifyContent: 'center',
    minWidth: '32px',
    height: '32px',
    padding: '0 6px',
    border: 'none',
    borderRadius: radius.control,
    background: 'transparent',
    color: palette.textSecondary,
    cursor: 'pointer',
    transition: 'background-color 150ms ease, color 150ms ease',

    '&:hover': {
      backgroundColor: 'rgba(138, 148, 166, 0.15)',
      color: palette.textPrimary,
    },
    '&:active': {
      backgroundColor: 'rgba(138, 148, 166, 0.25)',
    },
    '&:focus-visible': {
      outline: `2px solid ${palette.primary}`,
      outlineOffset: '-2px',
    },
  },
  disabled: {
    opacity: 0.5,
    cursor: 'not-allowed',

    '&:hover': {
      backgroundColor: 'transparent',
    },
  },
});

interface IconButtonProps {
  label: string;
  children: React.ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  style?: React.CSSProperties;
  /** 位于窗口拖拽区域内时设为 true,阻止按钮触发窗口拖动 */
  dragExclude?: boolean;
}

/** 截图风格的纯图标按钮：细线性单色图标，hover 出现浅灰圆角背景 */
export const IconButton: React.FC<IconButtonProps> = ({
  label,
  children,
  onClick,
  disabled,
  style,
  dragExclude,
}) => {
  const styles = useStyles();
  return (
    <button
      type="button"
      className={disabled ? `${styles.button} ${styles.disabled}` : styles.button}
      onClick={onClick}
      disabled={disabled}
      aria-label={label}
      title={label}
      style={style}
      data-tauri-drag-region={dragExclude ? 'false' : undefined}
    >
      {children}
    </button>
  );
};

const badgeStyles = makeStyles({
  badge: {
    display: 'inline-flex',
    alignItems: 'center',
    gap: '6px',
    padding: '3px 10px',
    borderRadius: radius.pill,
    backgroundColor: palette.onlineBadgeBg,
    color: palette.onlineBadgeText,
    fontSize: '12px',
    fontWeight: 600,
    lineHeight: '16px',
  },
  dot: {
    width: '6px',
    height: '6px',
    borderRadius: radius.circle,
  },
  offline: {
    backgroundColor: palette.textMuted,
  },
});

interface StatusBadgeProps {
  online: boolean;
  label?: string;
}

/** 截图风格状态胶囊：黑底 + 绿点/灰点 + 白字 */
export const StatusBadge: React.FC<StatusBadgeProps> = ({ online, label }) => {
  const styles = badgeStyles();
  return (
    <span className={styles.badge}>
      <span
        className={`${styles.dot} ${online ? '' : styles.offline}`}
        style={{ backgroundColor: online ? palette.online : palette.offline }}
      />
      {label ?? (online ? '在线' : '离线')}
    </span>
  );
};

export { zIndex };
