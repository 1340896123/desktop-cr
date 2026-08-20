import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { isTauri } from './connection';

/**
 * 远程会话中被控端回传的系统声音,控制端可一键静音。
 * 在控制端本地生效:静音后丢弃收到的音频块,不再播放。
 */
export async function setAudioMuted(muted: boolean): Promise<void> {
  if (!isTauri()) {
    console.warn('[audio] 非 Tauri 环境，跳过静音设置', muted);
    return;
  }
  await invoke('set_audio_muted', { muted });
}

/** 读取当前音频静音状态（前端连接后初始化静音按钮） */
export async function getAudioMuted(): Promise<boolean> {
  if (!isTauri()) {
    console.warn('[audio] 非 Tauri 环境，返回未静音');
    return false;
  }
  return invoke<boolean>('get_audio_muted');
}

/** 订阅音频静音状态事件（audio-state，前端据回执同步静音按钮），返回取消订阅函数 */
export async function onAudioStateChange(
  handler: (muted: boolean) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) {
    console.warn('[audio] 非 Tauri 环境，使用空音频状态事件源');
    return () => {
      /* noop */
    };
  }
  return listen<{ muted: boolean }>('audio-state', (event) => handler(event.payload.muted));
}
