import { describe, expect, it } from 'vitest';
import { genPeerId } from './config';

describe('genPeerId', () => {
  it('返回非空字符串', () => {
    const id = genPeerId();
    expect(typeof id).toBe('string');
    expect(id.length).toBeGreaterThan(0);
  });

  it('两次调用返回不同值', () => {
    expect(genPeerId()).not.toBe(genPeerId());
  });
});