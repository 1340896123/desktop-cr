import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { isTauri } from './connection';

/**
 * 一帧本机抓屏画面（JPEG 字节数组，Rust 端将 DXGI 帧编码为 JPEG 后推送）。
 */
export interface CapturedFrame {
  monitorId: number;
  width: number;
  height: number;
  jpeg: number[];
  /** 是否为模拟画面（非 Windows 平台动画帧；真实抓帧缺省/为 false） */
  simulated?: boolean;
}

/** 显示器信息（真实枚举，IDD 虚拟屏以 isVirtual 标记） */
export interface MonitorInfo {
  id: number;
  name: string;
  width: number;
  height: number;
  isPrimary: boolean;
  isVirtual: boolean;
}

/** 远程帧（来自被控端，经 LAN 协议解码后的 JPEG 字节） */
export interface RemoteFrame {
  width: number;
  height: number;
  jpeg: number[];
  /** 帧序号（用于丢包/乱序统计） */
  seq: number;
  /** 编码耗时（毫秒） */
  dur: number;
  /** 是否为模拟画面（协议未携带该字段时保持 undefined，按 false 处理） */
  simulated?: boolean;
}

export interface StartCaptureOptions {
  monitorId: number;
  width: number;
  height: number;
  fps?: number;
}

/** 开始抓取指定显示器的画面流 */
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

/** 枚举本机所有显示器（真实 EnumDisplayDevicesW；浏览器模式返回空数组） */
export async function listMonitors(): Promise<MonitorInfo[]> {
  if (!isTauri()) {
    console.warn('[capture] 非 Tauri 环境，返回空显示器列表');
    return [];
  }
  return invoke<MonitorInfo[]>('list_monitors');
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

/** 订阅实时抓帧事件（capture-frame，本机预览），返回取消订阅函数 */
export async function onFrame(handler: (frame: CapturedFrame) => void): Promise<UnlistenFn> {
  if (!isTauri()) {
    console.warn('[capture] 非 Tauri 环境，使用空事件源');
    return () => {
      /* noop */
    };
  }
  return listen<CapturedFrame>('capture-frame', (event) => handler(event.payload));
}

/** 订阅远程帧事件（remote-frame，来自被控端），返回取消订阅函数 */
export async function onRemoteFrame(handler: (frame: RemoteFrame) => void): Promise<UnlistenFn> {
  if (!isTauri()) {
    console.warn('[capture] 非 Tauri 环境，使用空远程帧事件源');
    return () => {
      /* noop */
    };
  }
  return listen<RemoteFrame>('remote-frame', (event) => handler(event.payload));
}
