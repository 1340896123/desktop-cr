import { describe, expect, it } from 'vitest';
import { normalizePointer } from './coords';

describe('normalizePointer', () => {
  const rect = { left: 100, top: 50, width: 800, height: 600 };
  const remoteWidth = 1920;
  const remoteHeight = 1080;

  it('中心点映射到远程坐标中值', () => {
    const result = normalizePointer(100 + 400, 50 + 300, rect, remoteWidth, remoteHeight);
    expect(result.x).toBeCloseTo(960, 5);
    expect(result.y).toBeCloseTo(540, 5);
  });

  it('越界坐标 clamp 到 0 与远端边界', () => {
    expect(normalizePointer(10, 10, rect, remoteWidth, remoteHeight)).toEqual({ x: 0, y: 0 });
    const right = normalizePointer(100 + 2000, 50 + 2000, rect, remoteWidth, remoteHeight);
    expect(right.x).toBe(remoteWidth);
    expect(right.y).toBe(remoteHeight);
  });

  it('0 尺寸 rect 返回原点，避免除零', () => {
    const zero = { left: 0, top: 0, width: 0, height: 0 };
    expect(normalizePointer(50, 60, zero, remoteWidth, remoteHeight)).toEqual({ x: 0, y: 0 });
  });
});