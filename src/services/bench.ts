import { invoke } from '@tauri-apps/api/core';
import { isTauri } from './connection';

/** 实时链路基准报告(Rust RealtimeBenchReport camelCase 序列化) */
export interface RealtimeBenchReport {
  /** 传输模式:"loopback" | "relay" */
  mode: string;
  /** 中继服务器地址(relay 模式) */
  relay: string;
  /** 基准时长(秒) */
  seconds: number;
  /** 目标帧率 */
  targetFps: number;
  /** 本机解码渲染成功的帧数 */
  framesRendered: number;
  /** 实时帧率 = 渲染帧数 / 总耗时 */
  realtimeFps: number;
  /** 平均抓帧耗时(毫秒) */
  avgCaptureMs: number;
  /** 平均缩放+编码耗时(毫秒) */
  avgEncodeMs: number;
  /** 平均发送耗时(毫秒) */
  avgSendMs: number;
  /** 平均本机解码(渲染)耗时(毫秒) */
  avgDecodeMs: number;
  /** 平均端到端延迟:发送开始 → 接收解码完成(毫秒) */
  avgE2eLatencyMs: number;
  /** 传输的总字节数(base64 协议字节) */
  totalTransferBytes: number;
  /** 渲染帧分辨率 */
  frameWidth: number;
  frameHeight: number;
}

/** 实时链路基准参数 */
export interface RealtimeBenchParams {
  /** 传输模式:本机回环或经公网中继 */
  mode: 'loopback' | 'relay';
  /** 中继服务器地址(relay 模式必填) */
  relayAddr?: string;
  /** 基准时长(秒) */
  seconds: number;
  /** 目标帧率 */
  targetFps: number;
  /** 是否跳过真实抓屏,使用合成动画帧 */
  synthetic: boolean;
  /** 目标分辨率宽度 */
  targetW: number;
  /** 目标分辨率高度 */
  targetH: number;
  /** 编码格式 */
  codec: 'jpeg' | 'h264' | 'hevc';
}

/**
 * 运行实时链路性能基准(真实 DXGI 采集 → 编码 → 协议帧发送 → 本机解码渲染)。
 * 注意:Tauri v2 默认将 Rust 参数名转为 camelCase 键,此处须用 camelCase。
 */
export async function runRealtimeBench(
  params: RealtimeBenchParams,
): Promise<RealtimeBenchReport | null> {
  if (!isTauri()) {
    console.warn('[bench] 非 Tauri 环境，跳过实时链路基准', params);
    return null;
  }
  return invoke<RealtimeBenchReport>('run_realtime_bench_command', {
    mode: params.mode,
    relayAddr: params.relayAddr,
    seconds: params.seconds,
    targetFps: params.targetFps,
    synthetic: params.synthetic,
    targetW: params.targetW,
    targetH: params.targetH,
    codec: params.codec,
  });
}
