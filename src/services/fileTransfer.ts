import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { isTauri } from './connection';

/** 目录项(与后端 FileEntry 的 camelCase 字段对应)。 */
export interface FileEntry {
  name: string;
  isDir: boolean;
  modifiedMs: number | null;
  size: number;
  ext: string;
}

/** 文件传输进度事件负载(direction 区分本机发送/远端接收)。 */
export interface FileProgress {
  id: number;
  received: number;
  total: number;
  name?: string;
  direction: 'send' | 'recv';
}

/** 远端目录列表应答事件负载。 */
export interface RemoteDirectory {
  path: string;
  entries: FileEntry[];
  error: string | null;
}

const mockDir: FileEntry[] = [
  { name: 'Documents', isDir: true, modifiedMs: Date.now(), size: 0, ext: '' },
  { name: 'Downloads', isDir: true, modifiedMs: Date.now(), size: 0, ext: '' },
  { name: 'Desktop', isDir: true, modifiedMs: Date.now(), size: 0, ext: '' },
  { name: '报告.docx', isDir: false, modifiedMs: Date.now() - 86400000, size: 482345, ext: 'docx' },
  { name: '照片.png', isDir: false, modifiedMs: Date.now() - 3600000, size: 2048576, ext: 'png' },
];

/** 列出本机目录内容。 */
export async function listDirectory(path: string): Promise<FileEntry[]> {
  if (!isTauri()) {
    console.warn('[fileTransfer] 非 Tauri 环境，返回 mock 目录', path);
    return mockDir;
  }
  return invoke<FileEntry[]>('list_directory', { path });
}

/** 本机接收目录(对端推送的文件落盘于此)。 */
export async function getIncomingDir(): Promise<string> {
  if (!isTauri()) {
    return 'C:\\incoming(mock)';
  }
  return invoke<string>('get_incoming_dir');
}

/** 发送本地文件到对端(返回后端分配的传输 id,用于匹配进度)。 */
export async function sendFile(path: string): Promise<number> {
  if (!isTauri()) {
    console.warn('[fileTransfer] 非 Tauri 环境，模拟发送', path);
    return Math.floor(Math.random() * 1000) + 1;
  }
  return invoke<number>('send_file', { path });
}

/** 请求对端目录列表(应答经 remote-directory 事件返回)。 */
export async function requestRemoteDir(path: string): Promise<void> {
  if (!isTauri()) {
    console.warn('[fileTransfer] 非 Tauri 环境，跳过远端目录请求', path);
    return;
  }
  await invoke('request_remote_dir', { path });
}

/** 请求对端发送指定文件(id 由本机分配,对端按此 id 回传,进度可对账)。 */
export async function requestFilePull(id: number, path: string): Promise<void> {
  if (!isTauri()) {
    console.warn('[fileTransfer] 非 Tauri 环境，模拟拉取', path);
    return;
  }
  await invoke('request_file_pull', { id, path });
}

/** 订阅文件传输进度事件,返回取消订阅函数。 */
export async function onFileProgress(
  handler: (progress: FileProgress) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) {
    console.warn('[fileTransfer] 非 Tauri 环境，使用空事件源');
    return () => {
      /* noop */
    };
  }
  return listen<FileProgress>('file-progress', (event) => handler(event.payload));
}

/** 订阅远端目录列表事件,返回取消订阅函数。 */
export async function onRemoteDirectory(
  handler: (directory: RemoteDirectory) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) {
    console.warn('[fileTransfer] 非 Tauri 环境，使用 mock 目录事件源');
    return () => {
      /* noop */
    };
  }
  return listen<RemoteDirectory>('remote-directory', (event) => handler(event.payload));
}

/** 读取独立文件传输窗口的对端设备名(主窗口打开窗口时写入;浏览器模式返回 null)。 */
export async function getTransferDeviceName(): Promise<string | null> {
  if (!isTauri()) {
    console.warn('[fileTransfer] 非 Tauri 环境，无对端设备名');
    return null;
  }
  const name = await invoke<string | null>('get_transfer_device_name');
  return name || null;
}