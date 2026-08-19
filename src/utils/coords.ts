/** 画布元素矩形信息（getBoundingClientRect 的结构子集） */
export interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}

/**
 * 将客户端坐标归一化为远程画面坐标（纯函数，可单元测试）。
 * 公式：X_remote = clamp((clientX - left) * remoteWidth / width, 0, remoteWidth)。
 * rect 尺寸非法（<=0）时返回原点，避免除零。
 */
export function normalizePointer(
  clientX: number,
  clientY: number,
  rect: Rect,
  remoteWidth: number,
  remoteHeight: number,
): { x: number; y: number } {
  if (rect.width <= 0 || rect.height <= 0) {
    return { x: 0, y: 0 };
  }
  const xCss = clientX - rect.left;
  const yCss = clientY - rect.top;
  const x = Math.min(Math.max((xCss * remoteWidth) / rect.width, 0), remoteWidth);
  const y = Math.min(Math.max((yCss * remoteHeight) / rect.height, 0), remoteHeight);
  return { x, y };
}