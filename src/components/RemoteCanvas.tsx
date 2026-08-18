import React, { useCallback, useEffect, useRef, useState } from 'react';
import { makeStyles, tokens } from '@fluentui/react-components';
import { sendKeyEvent, sendMouseEvent, type MouseInputPayload } from '../services/input';

const useStyles = makeStyles({
  container: {
    width: '100%',
    height: '100%',
    position: 'relative',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: tokens.colorNeutralBackground2,
    overflow: 'hidden',
  },
  canvas: {
    cursor: 'crosshair',
    maxWidth: '100%',
    maxHeight: '100%',
    touchAction: 'none',
    userSelect: 'none',
  },
  placeholder: {
    color: tokens.colorNeutralForeground3,
    textAlign: 'center',
  },
  overlay: {
    position: 'absolute',
    top: '8px',
    left: '50%',
    transform: 'translateX(-50%)',
    color: tokens.colorNeutralForeground2,
    backgroundColor: tokens.colorNeutralBackground2,
    padding: '4px 12px',
    borderRadius: '4px',
    fontSize: '12px',
  },
});

interface RemoteCanvasProps {
  connected: boolean;
  /** 被控端虚拟屏分辨率 */
  remoteWidth: number;
  remoteHeight: number;
  /** 渲染模式：canvas 兼容模式 / video 高帧率模式 */
  mode?: 'canvas' | 'video';
}

/**
 * 远程画面渲染组件。
 * 监听 pointer / key / wheel 事件，按 README 第 4 节公式做坐标归一化：
 *   X_remote = x * W_remote / W_css
 *   Y_remote = y * H_remote / H_css
 */
export const RemoteCanvas: React.FC<RemoteCanvasProps> = ({
  connected,
  remoteWidth,
  remoteHeight,
  mode = 'canvas',
}) => {
  const styles = useStyles();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [frameSize, setFrameSize] = useState({ width: remoteWidth, height: remoteHeight });

  const normalize = useCallback(
    (clientX: number, clientY: number) => {
      const canvas = canvasRef.current;
      if (!canvas) return null;
      const rect = canvas.getBoundingClientRect();
      const xCss = clientX - rect.left;
      const yCss = clientY - rect.top;
      const xRemote = Math.min(Math.max((xCss * remoteWidth) / rect.width, 0), remoteWidth);
      const yRemote = Math.min(Math.max((yCss * remoteHeight) / rect.height, 0), remoteHeight);
      return { x: xRemote, y: yRemote };
    },
    [remoteWidth, remoteHeight],
  );

  const buildMousePayload = useCallback(
    (
      event: React.PointerEvent,
      eventType: MouseInputPayload['event_type'],
    ): MouseInputPayload | null => {
      const pos = normalize(event.clientX, event.clientY);
      if (!pos) return null;
      const buttonMap: Record<number, MouseInputPayload['button']> = {
        0: 'left',
        1: 'middle',
        2: 'right',
      };
      const payload: MouseInputPayload = { event_type: eventType, ...pos };
      if (eventType !== 'mousemove' && eventType !== 'wheel') {
        payload.button = buttonMap[event.button] ?? 'left';
      }
      return payload;
    },
    [normalize],
  );

  const handlePointerDown = useCallback(
    (event: React.PointerEvent) => {
      const payload = buildMousePayload(event, 'mousedown');
      if (payload) void sendMouseEvent(payload);
    },
    [buildMousePayload],
  );

  const handlePointerUp = useCallback(
    (event: React.PointerEvent) => {
      const payload = buildMousePayload(event, 'mouseup');
      if (payload) void sendMouseEvent(payload);
    },
    [buildMousePayload],
  );

  const handlePointerMove = useCallback(
    (event: React.PointerEvent) => {
      const payload = buildMousePayload(event, 'mousemove');
      if (payload) void sendMouseEvent(payload);
    },
    [buildMousePayload],
  );

  const handleWheel = useCallback(
    (event: React.WheelEvent) => {
      const pos = normalize(event.clientX, event.clientY);
      if (!pos) return;
      void sendMouseEvent({
        event_type: 'wheel',
        ...pos,
        delta_y: event.deltaY,
      });
    },
    [normalize],
  );

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent) => {
      event.preventDefault();
      void sendKeyEvent({
        key: event.key,
        event_type: 'keydown',
        code: event.code,
        modifiers: ['ctrlKey', 'shiftKey', 'altKey', 'metaKey'].filter((m) => event[m as keyof React.KeyboardEvent]),
      });
    },
    [],
  );

  const handleKeyUp = useCallback(
    (event: React.KeyboardEvent) => {
      event.preventDefault();
      void sendKeyEvent({
        key: event.key,
        event_type: 'keyup',
        code: event.code,
        modifiers: [],
      });
    },
    [],
  );

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || mode !== 'canvas') return;
    canvas.tabIndex = 0;
    canvas.focus();
  }, [mode]);

  useEffect(() => {
    setFrameSize({ width: remoteWidth, height: remoteHeight });
  }, [remoteWidth, remoteHeight]);

  // 兼容模式下绘制一个模拟帧，用于演示坐标区域
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || mode !== 'canvas') return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.fillStyle = '#1f2937';
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.strokeStyle = '#4b5563';
    ctx.lineWidth = 1;
    for (let x = 0; x <= canvas.width; x += 64) {
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, canvas.height);
      ctx.stroke();
    }
    for (let y = 0; y <= canvas.height; y += 64) {
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(canvas.width, y);
      ctx.stroke();
    }
    ctx.fillStyle = '#9ca3af';
    ctx.font = '14px system-ui';
    ctx.textAlign = 'center';
    ctx.fillText(`Remote Desktop ${remoteWidth}x${remoteHeight} (Canvas Mode)`, canvas.width / 2, canvas.height / 2);
  }, [mode, frameSize, remoteWidth, remoteHeight]);

  if (!connected) {
    return (
      <div className={styles.container}>
        <div className={styles.placeholder}>
          <p>尚未连接到远程设备</p>
          <p>请在「设备」页选择一个设备开始连接</p>
        </div>
      </div>
    );
  }

  if (mode === 'video') {
    return (
      <div className={styles.container}>
        <div className={styles.overlay}>Video 渲染模式（WebRTC 高帧率）</div>
        <video
          style={{ maxWidth: '100%', maxHeight: '100%' }}
          controls={false}
          muted
          playsInline
          onPointerDown={handlePointerDown}
          onPointerUp={handlePointerUp}
          onPointerMove={handlePointerMove}
          onWheel={handleWheel}
          onKeyDown={handleKeyDown}
          onKeyUp={handleKeyUp}
        />
      </div>
    );
  }

  return (
    <div className={styles.container}>
      <div className={styles.overlay}>
        {frameSize.width}x{frameSize.height} · 已连接
      </div>
      <canvas
        ref={canvasRef}
        className={styles.canvas}
        width={frameSize.width}
        height={frameSize.height}
        onPointerDown={handlePointerDown}
        onPointerUp={handlePointerUp}
        onPointerMove={handlePointerMove}
        onWheel={handleWheel}
        onKeyDown={handleKeyDown}
        onKeyUp={handleKeyUp}
      />
    </div>
  );
};

export default RemoteCanvas;
