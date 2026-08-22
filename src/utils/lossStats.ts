/**
 * 丢包统计纯函数(F-4):按 seq 连续性累计丢包/接收。
 *
 * 背景:UDP 模式 seq = frame_id(编码器帧号)、TCP 模式 seq = 推流循环独立计数,
 * 两域数值互不相关;UDP↔TCP 回退切换时 seq 跳变若直接按连续性统计会产生
 * 虚假丢包尖峰。切换时重置基线豁免(真实丢帧仍按域内连续性统计,语义保留)。
 * 抽出为纯函数便于单元测试(组件内经 ref 持有状态逐帧喂入)。
 */

/** 丢包统计状态 */
export interface LossStats {
  /** 域内跳号累计的丢帧数 */
  lost: number;
  /** 收到的帧数 */
  received: number;
  /** 基线重置次数(域切换豁免;单测断言用) */
  resets: number;
}

export function createLossStats(): LossStats {
  return { lost: 0, received: 0, resets: 0 };
}

/** 最近一帧的统计基线(seq + 传输模式) */
export interface LossBaseline {
  seq: number;
  transport: string;
}

/** 喂入一帧(seq + 当前传输模式),更新丢包统计并返回新的基线。 */
export function feedLossStats(
  stats: LossStats,
  seq: number,
  transport: string,
  last: LossBaseline | null,
): LossBaseline {
  // 传输模式切换 → 基线重置:seq 域跳变豁免(F-4)
  if (last && last.transport !== transport) {
    stats.resets += 1;
    stats.received += 1;
    return { seq, transport };
  }
  if (last) {
    if (seq > last.seq + 1) {
      stats.lost += seq - last.seq - 1;
    } else if (seq < last.seq) {
      // 同域 seq 回退(会话重置/编码器重建):重置基准,本帧不计丢包
      stats.received += 1;
      return { seq, transport };
    }
  }
  stats.received += 1;
  return { seq, transport };
}
