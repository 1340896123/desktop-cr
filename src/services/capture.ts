import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { isTauri } from './connection';

/**
 * 一帧远程桌面画面（RGBA 原始像素，尺寸由 Rust 端 clamp 到 ≤ 480x270）。
 */
export interface CapturedFrame {
  monitorId: number;
  width: number;
  height: number;
  rgba: number[];
}

export interface StartCaptureOptions {
  monitorId: number;
  width: number;
  height: number;
  fps?: number;
}

/** 开始抓取指定虚拟屏的画面流 */
export async function startCapture(options: StartCaptureOptions): Promise<void> {
  if (!isTauri()) {
    console.warn('[capture] 非 Tauri 环境，跳过开始抓帧', options);
    return;
  }
  await invoke('start_capture', {
    monitorId: options.monitorId,
    width: options.width,
    height: options.height,
    fps: options.fps ?? 30,
  });
}

/** 停止抓帧 */
export async function stopCapture(): Promise<void> {
  if (!isTauri()) {
    console.warn('[capture] 非 Tauri 环境，跳过停止抓帧');
    return;
  }
  await invoke('stop_capture');
}

/**
 * 主动拉取最新一帧（Rust 端返回结构：{ width, height, format, data }）。
 * Rust 端在尚无帧时会返回错误，此时降级为 null。
 */
export interface PulledFrame {
  width: number;
  height: number;
  format: string;
  data: number[];
}

export async function getFrame(monitorId: number): Promise<PulledFrame | null> {
  if (!isTauri()) {
    console.warn('[capture] 非 Tauri 环境，返回 null 帧');
    return null;
  }
  try {
    return await invoke<PulledFrame>('get_frame', { monitorId });
  } catch {
    return null;
  }
}

/** 订阅实时抓帧事件，返回取消订阅函数 */
export async function onFrame(handler: (frame: CapturedFrame) => void): Promise<UnlistenFn> {
  if (!isTauri()) {
    console.warn('[capture] 非 Tauri 环境，使用空事件源');
    return () => {
      /* noop */
    };
  }
  return listen<CapturedFrame>('capture-frame', (event) => handler(event.payload));
}
