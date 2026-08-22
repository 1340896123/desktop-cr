import { describe, expect, it } from 'vitest';
import { createLossStats, feedLossStats, type LossBaseline } from './lossStats';

/**
 * F-4 验证:UDP(frame_id 域)↔ TCP(独立 seq 域)回退切换时,
 * 丢包统计基线重置——seq 域跳变不产生虚假丢包尖峰;
 * 域内真实跳号仍按连续性计为丢包(语义保留)。
 */
describe('feedLossStats 丢包统计基线重置', () => {
  it('同域连续帧:零丢包', () => {
    const stats = createLossStats();
    let last: LossBaseline | null = null;
    for (let seq = 0; seq < 10; seq++) {
      last = feedLossStats(stats, seq, 'udp', last);
    }
    expect(stats.lost).toBe(0);
    expect(stats.received).toBe(10);
    expect(stats.resets).toBe(0);
  });

  it('UDP→TCP 回退切换:frame_id 域(97..99)切到 seq 域(0..2)无虚假丢包', () => {
    const stats = createLossStats();
    let last: LossBaseline | null = null;
    // UDP 域:97, 98, 99
    for (const seq of [97, 98, 99]) {
      last = feedLossStats(stats, seq, 'udp', last);
    }
    // 回退切换:TCP 域从 0 重新计数(99 → 0 为域跳变,不是丢包)
    for (const seq of [0, 1, 2]) {
      last = feedLossStats(stats, seq, 'tcp', last);
    }
    expect(stats.lost).toBe(0);
    expect(stats.received).toBe(6);
    expect(stats.resets).toBe(1);
  });

  it('TCP→UDP(反向切换,如重连后重建 UDP 通道):同样豁免', () => {
    const stats = createLossStats();
    let last: LossBaseline | null = null;
    for (const seq of [0, 1, 2]) {
      last = feedLossStats(stats, seq, 'tcp', last);
    }
    // TCP seq 2 → UDP frame_id 500(编码器帧号已增长):域切换豁免
    for (const seq of [500, 501]) {
      last = feedLossStats(stats, seq, 'udp', last);
    }
    expect(stats.lost).toBe(0);
    expect(stats.resets).toBe(1);
  });

  it('域内真实跳号仍计为丢包(UDP 丢帧语义保留)', () => {
    const stats = createLossStats();
    let last: LossBaseline | null = null;
    last = feedLossStats(stats, 10, 'udp', last);
    // UDP 丢帧:10 → 14(丢 3 帧)
    last = feedLossStats(stats, 14, 'udp', last);
    expect(stats.lost).toBe(3);
    expect(stats.resets).toBe(0);
  });

  it('同域 seq 回退(会话重置/编码器重建):重置基准不计丢包', () => {
    const stats = createLossStats();
    let last: LossBaseline | null = null;
    last = feedLossStats(stats, 50, 'tcp', last);
    // 编码器重建后 seq 回到 0
    last = feedLossStats(stats, 0, 'tcp', last);
    last = feedLossStats(stats, 1, 'tcp', last);
    expect(stats.lost).toBe(0);
    expect(stats.received).toBe(3);
  });

  it('R2-B:帧级 transport 优先——回退瞬间 TCP 帧带帧级 tcp 标记,不产生虚假丢包尖峰', () => {
    // 复现 R2-B 场景:metrics 镜像仍为 udp(2 秒轮询滞后窗口内),但 TCP 帧
    // 自带帧级 transport="tcp"——按帧级标记喂入,切换即基线重置。
    const stats = createLossStats();
    let last: LossBaseline | null = null;
    // UDP 域末期 frame_id 较小(编码器刚重建,如 seq=5)
    for (const seq of [5, 6, 7]) {
      last = feedLossStats(stats, seq, 'udp', last);
    }
    // 回退后 TCP 帧自带帧级 tcp 标记(seq 已增长到 97;若被错标 udp 域会判
    // lost=89 虚假尖峰——判别器 N8 复现值)
    const metricsTransport = 'udp'; // 轮询滞后:metrics 仍显示 udp
    for (const seq of [97, 98]) {
      // 与 RemoteSessionView 同口径:frame.transport ?? metrics ?? 'tcp'
      const frameTransport = 'tcp';
      const transport = frameTransport ?? metricsTransport;
      last = feedLossStats(stats, seq, transport, last);
    }
    expect(stats.lost).toBe(0);
    expect(stats.resets).toBe(1);
  });

  it('R2-B 补充:帧级标记缺失(旧负载)回退 metrics 轮询值,行为与 F-4 主修复一致', () => {
    const stats = createLossStats();
    let last: LossBaseline | null = null;
    // 旧负载无帧级 transport:回退 metrics 值 udp(域内连续帧零丢包)
    for (const seq of [5, 6, 7]) {
      const metricsTransport = 'udp';
      const frameTransport: string | undefined = undefined;
      const transport = frameTransport ?? metricsTransport;
      last = feedLossStats(stats, seq, transport, last);
    }
    expect(stats.resets).toBe(0);
    expect(stats.lost).toBe(0);
  });
});
