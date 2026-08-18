import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { isTauri } from './connection';

/** 虚拟显示器信息（与 Rust 侧 serde 结构一致） */
export interface VirtualMonitor {
  id: number;
  width: number;
  height: number;
  fps: number;
  connected: boolean;
}

/** 安装虚拟显示器驱动（模拟显示器） */
export async function installVirtualDisplayDriver(): Promise<void> {
  if (!isTauri()) {
    console.warn('[vdisplay] 非 Tauri 环境，mock 安装驱动');
    return;
  }
  await invoke('install_virtual_display_driver');
}

/** 新增虚拟显示器，返回新显示器 id */
export async function addVirtualMonitor(width: number, height: number, fps: number): Promise<number> {
  if (!isTauri()) {
    console.warn('[vdisplay] 非 Tauri 环境，mock 新增虚拟屏', { width, height, fps });
    return (Date.now() % 1000) + 1;
  }
  return invoke<number>('add_virtual_monitor', { width, height, fps });
}

/** 获取当前虚拟显示器列表 */
export async function listVirtualMonitors(): Promise<VirtualMonitor[]> {
  if (!isTauri()) {
    console.warn('[vdisplay] 非 Tauri 环境，返回空虚拟屏列表');
    return [];
  }
  return invoke<VirtualMonitor[]>('list_virtual_monitors');
}

/** 移除虚拟显示器 */
export async function removeVirtualMonitor(monitorId: number): Promise<void> {
  if (!isTauri()) {
    console.warn('[vdisplay] 非 Tauri 环境，跳过移除虚拟屏', monitorId);
    return;
  }
  await invoke('remove_virtual_monitor', { monitorId });
}

/** 订阅虚拟屏列表变更事件，返回取消订阅函数 */
export async function onMonitorsChanged(handler: (monitors: VirtualMonitor[]) => void): Promise<UnlistenFn> {
  if (!isTauri()) {
    console.warn('[vdisplay] 非 Tauri 环境，使用空事件源');
    return () => {
      /* noop */
    };
  }
  return listen<VirtualMonitor[]>('virtual-monitors-changed', (event) => handler(event.payload));
}
