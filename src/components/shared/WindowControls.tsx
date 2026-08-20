import React, { useEffect, useState } from 'react';
import { makeStyles } from '@fluentui/react-components';
import {
  CopyRegular,
  DismissRegular,
  SquareRegular,
  SubtractRegular,
} from '@fluentui/react-icons';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { closeWindow, minimizeWindow, onWindowMaximizedChange } from '../../services/window';

const useStyles = makeStyles({
  controls: {
    display: 'flex',
    alignItems: 'center',
    height: '100%',
  },
  btn: {
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
  close: {
    '&:hover': {
      backgroundColor: '#E81123',
      color: '#ffffff',
    },
  },
});

interface WindowControlsProps {
  /** 浏览器模式点击最大化/还原时的回退行为 */
  onToggleMaximize?: () => void;
}

/**
 * 无边框窗口的自绘窗口控制按钮组：最小化 / 最大化还原 / 关闭（hover 变红）。
 * 内部自行订阅窗口最大化状态；Tauri 环境真实调用窗口 API，浏览器模式为 noop。
 */
export const WindowControls: React.FC<WindowControlsProps> = ({ onToggleMaximize }) => {
  const styles = useStyles();
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void onWindowMaximizedChange(setMaximized).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  const handleToggleMaximize = () => {
    if (typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)) {
      onToggleMaximize?.();
      return;
    }
    void getCurrentWindow().toggleMaximize();
  };

  return (
    <div className={styles.controls}>
      <button
        type="button"
        className={styles.btn}
        onClick={() => void minimizeWindow()}
        aria-label="最小化"
        title="最小化"
        data-tauri-drag-region="false"
      >
        <SubtractRegular fontSize={14} />
      </button>
      <button
        type="button"
        className={styles.btn}
        onClick={handleToggleMaximize}
        aria-label={maximized ? '还原窗口' : '最大化'}
        title={maximized ? '还原窗口' : '最大化'}
        data-tauri-drag-region="false"
      >
        {maximized ? <CopyRegular fontSize={14} /> : <SquareRegular fontSize={13} />}
      </button>
      <button
        type="button"
        className={`${styles.btn} ${styles.close}`}
        onClick={() => void closeWindow()}
        aria-label="关闭"
        title="关闭"
        data-tauri-drag-region="false"
      >
        <DismissRegular fontSize={14} />
      </button>
    </div>
  );
};

export default WindowControls;
