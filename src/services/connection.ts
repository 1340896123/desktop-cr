import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

/**
 * 判断当前是否运行在 Tauri 环境中。
 * 纯浏览器环境下 window.__TAURI_INTERNALS__ 不存在，需做降级处理。
 */
export const isTauri = (): boolean =>
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export interface DeviceInfo {
  id: string;
  name: string;
  status: string;
  platform?: string;
}

export interface ConnectionState {
  connected: boolean;
  peerId?: string;
  error?: string;
}

export interface QualityOptions {
  fps: number;
  bitrate?: number;
  quality: 'low' | 'medium' | 'high';
}

export interface ResolutionOptions {
  width: number;
  height: number;
  fps: number;
}

export interface ClipboardData {
  text: string;
  format?: string;
}

const mockDevices: DeviceInfo[] = [
  { id: 'mock-01', name: 'Desktop-Office (Mock)', status: 'online', platform: 'windows' },
  { id: 'mock-02', name: 'NAS-Server (Mock)', status: 'offline', platform: 'linux' },
];

const mockState: ConnectionState = { connected: false, peerId: 'mock-01' };

/** 连接到远程设备 */
export async function connectToDevice(peerId: string): Promise<ConnectionState> {
  if (!isTauri()) {
    console.warn('[connection] 非 Tauri 环境，返回 mock 连接结果', peerId);
    return { connected: true, peerId };
  }
  return invoke<ConnectionState>('connect_to_device', { peerId });
}

/** 断开当前连接 */
export async function disconnectFromDevice(): Promise<void> {
  if (!isTauri()) {
    console.warn('[connection] 非 Tauri 环境，跳过断开连接');
    return;
  }
  await invoke('disconnect_from_device');
}

/** 获取已发现的设备列表 */
export async function getDevices(): Promise<DeviceInfo[]> {
  if (!isTauri()) {
    console.warn('[connection] 非 Tauri 环境，返回 mock 设备列表');
    return mockDevices;
  }
  return invoke<DeviceInfo[]>('list_devices');
}

/** 获取当前连接状态 */
export async function getConnectionState(): Promise<ConnectionState> {
  if (!isTauri()) {
    return mockState;
  }
  return invoke<ConnectionState>('get_connection_state');
}

/** 设置画面质量参数 */
export async function setQuality(options: QualityOptions): Promise<void> {
  if (!isTauri()) {
    console.warn('[connection] 非 Tauri 环境，跳过设置画质', options);
    return;
  }
  await invoke('set_stream_quality', { fps: options.fps, bitrate: options.bitrate, quality: options.quality });
}

/** 设置远程分辨率 */
export async function setResolution(options: ResolutionOptions): Promise<void> {
  if (!isTauri()) {
    console.warn('[connection] 非 Tauri 环境，跳过设置分辨率', options);
    return;
  }
  await invoke('set_stream_resolution', { width: options.width, height: options.height, fps: options.fps });
}

/** 切换全屏 */
export async function setFullscreen(fullscreen: boolean): Promise<void> {
  if (!isTauri()) {
    console.warn('[connection] 非 Tauri 环境，跳过全屏切换', fullscreen);
    return;
  }
  await invoke('set_fullscreen', { fullscreen });
}

/** 剪贴板双向同步 */
export async function syncClipboard(): Promise<void> {
  if (!isTauri()) {
    console.warn('[connection] 非 Tauri 环境，跳过剪贴板同步');
    return;
  }
  await invoke('sync_clipboard');
}

/** 订阅连接状态变更事件，返回取消订阅函数 */
export async function onConnectionStateChange(
  handler: (state: ConnectionState) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) {
    console.warn('[connection] 非 Tauri 环境，使用 mock 事件源');
    return () => {
      /* noop */
    };
  }
  return listen<ConnectionState>('connection-state', (event) => handler(event.payload));
}
