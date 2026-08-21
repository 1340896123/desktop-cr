import { getCurrentWindow } from '@tauri-apps/api/window';
import { invoke } from '@tauri-apps/api/core';

const isTauri = (): boolean =>
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

/** 最小化当前窗口;浏览器模式为 noop */
export async function minimizeWindow(): Promise<void> {
  if (!isTauri()) {
    console.warn('[window] 浏览器模式,跳过最小化');
    return;
  }
  await getCurrentWindow().minimize();
}

/** 关闭当前窗口;浏览器模式为 noop */
export async function closeWindow(): Promise<void> {
  if (!isTauri()) {
    console.warn('[window] 浏览器模式,跳过关闭');
    return;
  }
  await getCurrentWindow().close();
}

/** 查询窗口是否处于最大化状态 */
export async function isWindowMaximized(): Promise<boolean> {
  if (!isTauri()) return false;
  return getCurrentWindow().isMaximized();
}

/**
 * 监听窗口最大化状态变化(含尺寸变化),首次立即回调一次。
 * 返回取消订阅函数;浏览器模式直接返回 noop。
 */
export async function onWindowMaximizedChange(
  cb: (maximized: boolean) => void,
): Promise<() => void> {
  if (!isTauri()) return () => {};
  const win = getCurrentWindow();
  const refresh = async () => cb(await win.isMaximized());
  await refresh();
  const unlisten = await win.onResized(refresh);
  return unlisten;
}

/**
 * 打开独立文件传输窗口(单例:已存在时聚焦)。返回是否成功;
 * 浏览器模式返回 false,由调用方回退到页内视图。
 */
export async function openFileTransferWindow(deviceName?: string): Promise<boolean> {
  if (!isTauri()) {
    console.warn('[window] 浏览器模式,无法打开独立窗口');
    return false;
  }
  await invoke('open_file_transfer_window', { deviceName: deviceName ?? null });
  return true;
}

/**
 * 打开独立远程会话窗口(单例:已存在时切换目标并聚焦),连接由该窗口自行发起。
 * 返回是否成功;浏览器模式返回 false,由调用方回退到页内会话视图。
 */
export async function openRemoteSessionWindow(peerId: string, deviceName: string): Promise<boolean> {
  if (!isTauri()) {
    console.warn('[window] 浏览器模式,无法打开独立窗口');
    return false;
  }
  await invoke('open_remote_session_window', { peerId, deviceName });
  return true;
}
