import { invoke } from '@tauri-apps/api/core';
import { isTauri } from './connection';

/**
 * 鼠标事件负载（与 Rust 侧 serde 结构一致）。
 * 坐标经过归一化处理：X_remote = x * W_remote / W_css
 */
export interface MouseInputPayload {
  event_type: 'mousemove' | 'mousedown' | 'mouseup' | 'wheel';
  /** 归一化后的 X 坐标（0 ~ W_remote） */
  x: number;
  /** 归一化后的 Y 坐标（0 ~ H_remote） */
  y: number;
  button?: 'left' | 'right' | 'middle';
  delta_y?: number;
}

/** 键盘事件负载 */
export interface KeyInputPayload {
  key: string;
  event_type: 'keydown' | 'keyup';
  /** DOM KeyboardEvent.code，如 KeyA、Space */
  code?: string;
  /** 同时按下的修饰键 */
  modifiers?: string[];
}

/** 发送鼠标事件到被控端 */
export async function sendMouseEvent(payload: MouseInputPayload): Promise<void> {
  if (!isTauri()) {
    console.warn('[input] 非 Tauri 环境，忽略鼠标事件', payload);
    return;
  }
  await invoke('inject_mouse_event', {
    x: payload.x,
    y: payload.y,
    eventType: payload.event_type,
    button: payload.button ?? null,
    deltaY: payload.delta_y ?? 0,
  });
}

/** 发送键盘事件到被控端 */
export async function sendKeyEvent(payload: KeyInputPayload): Promise<void> {
  if (!isTauri()) {
    console.warn('[input] 非 Tauri 环境，忽略键盘事件', payload);
    return;
  }
  await invoke('inject_key_event', {
    key: payload.key,
    eventType: payload.event_type,
    code: payload.code ?? null,
    modifiers: payload.modifiers ?? [],
  });
}
