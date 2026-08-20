import { invoke } from '@tauri-apps/api/core';
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
