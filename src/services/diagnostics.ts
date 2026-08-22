import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { isTauri } from './connection';

/** 诊断链路传输模式 */
export type LoopbackTransport = 'tcp' | 'udp';

/**
 * DXGI 回传诊断报告(Rust DxgiLoopbackReport camelCase 序列化)。
 * 链路全程 H.264/H.265 标准编解码(硬编/硬解优先),禁止 JPEG。
 * transport 组字段为 UDP 模式扩展(向后兼容可选字段,旧后端/TCP 模式缺省)。
 */
export interface DxgiLoopbackReport {
  /** 编解码家族:"h264" | "hevc" */
  codec: string;
  /** 实际使用的 FFmpeg 编码器(如 h264_nvenc / h264_qsv / libx264) */
  encoder: string;
  /** 解码是否走 D3D11VA 硬件路径 */
  decoderHwaccel: boolean;
  monitorId: number;
  seconds: number;
  targetFps: number;
  /** 真实抓到的新帧数(桌面静止时低于发送数属正常) */
  framesGrabbed: number;
  framesSent: number;
  framesRendered: number;
  realtimeFps: number;
  avgCaptureMs: number;
  avgEncodeMs: number;
  avgSendMs: number;
  avgDecodeMs: number;
  avgE2eLatencyMs: number;
  totalTransferBytes: number;
  sourceWidth: number;
  sourceHeight: number;
  frameWidth: number;
  frameHeight: number;
  /** 传输模式:"tcp" | "udp"(UDP 回环 / UDP 中继) */
  transport?: LoopbackTransport;
  /** UDP 模式:整帧分片总数 */
  udpFragments?: number;
  /** UDP 模式:丢失分片计数(回环应为 0) */
  udpLostFragments?: number;
  /** UDP 模式:乱序到达分片计数 */
  udpReorderedFragments?: number;
  /** UDP 模式:因超时未收齐而丢弃的整帧数 */
  udpDroppedFrames?: number;
  /** UDP 模式:平均单帧重组耗时(毫秒) */
  avgReassemblyMs?: number;
}

/** dxgi-loop-frame 事件负载:回环链路到达的编码帧 + 元数据 */
export interface DxgiLoopFrame {
  seq: number;
  width: number;
  height: number;
  key: boolean;
  /** 编码格式:"h264"(现有) | "hevc"(按 codec 字段处理,兼容 h264) */
  codec: string;
  /** H.264/H.265 Annex-B 字节(前端经 WebCodecs 解码绘制) */
  data: number[];
}

/**
 * 运行 DXGI 回传诊断(真实本机采集 → H.264 编码 → 协议帧 → TCP/UDP 回环 → 解码)。
 * 注意:Tauri v2 默认将 Rust 参数名转为 camelCase 键,此处须用 camelCase。
 * transport 可选,缺省 "tcp";UDP 模式报告含丢包/乱片/分片数/重组耗时统计。
 */
export async function runDxgiLoopback(options: {
  monitorId: number;
  seconds: number;
  targetFps: number;
  targetWidth: number;
  targetHeight: number;
  transport?: LoopbackTransport;
}): Promise<DxgiLoopbackReport | null> {
  if (!isTauri()) {
    console.warn('[diagnostics] 非 Tauri 环境，跳过 DXGI 回传诊断', options);
    return null;
  }
  return invoke<DxgiLoopbackReport>('run_dxgi_loopback', {
    monitorId: options.monitorId,
    seconds: options.seconds,
    targetFps: options.targetFps,
    targetWidth: options.targetWidth,
    targetHeight: options.targetHeight,
    transport: options.transport ?? 'tcp',
  });
}

/** 订阅回环帧事件(dxgi-loop-frame,编码帧 Annex-B),返回取消订阅函数 */
export async function onDxgiLoopFrame(
  handler: (frame: DxgiLoopFrame) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) {
    console.warn('[diagnostics] 非 Tauri 环境，使用空事件源');
    return () => {
      /* noop */
    };
  }
  return listen<DxgiLoopFrame>('dxgi-loop-frame', (event) => handler(event.payload));
}
