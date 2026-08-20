import React from 'react';
import { makeStyles } from '@fluentui/react-components';
import { CheckmarkCircleFilled } from '@fluentui/react-icons';
import { fontFamily } from '../../theme/tokens';

const useStyles = makeStyles({
  toast: {
    position: 'fixed',
    top: '56px',
    left: '50%',
    transform: 'translateX(-50%)',
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '8px 16px',
    backgroundColor: 'rgba(15, 23, 32, 0.92)',
    color: '#ffffff',
    borderRadius: '8px',
    fontFamily,
    fontSize: '12px',
    fontWeight: 500,
    boxShadow: '0 8px 24px rgba(0,0,0,0.35)',
    border: '1px solid rgba(255,255,255,0.08)',
    zIndex: 200,
    pointerEvents: 'none',
    maxWidth: '70vw',
  },
  icon: {
    display: 'flex',
    color: '#34C759',
    flexShrink: 0,
  },
});

interface ToastProps {
  message: string | null;
}

/**
 * 应用级 Toast：顶部中央深色圆角条 + 绿色对勾图标，由 App 统一驱动，2s 自动消失。
 */
export const Toast: React.FC<ToastProps> = ({ message }) => {
  const styles = useStyles();
  if (!message) return null;
  return (
    <div className={styles.toast} role="status">
      <span className={styles.icon}>
        <CheckmarkCircleFilled fontSize={16} />
      </span>
      <span>{message}</span>
    </div>
  );
};

export default Toast;