import { invoke } from '@tauri-apps/api/core';
import { isTauri } from './connection';

/** 操作日志条目（与 Rust 侧 serde 结构一致） */
export interface OperationLogEntry {
  time: string;
  module: string;
  action: string;
  detail: string;
}

/** 获取操作日志（最新在前，limit 为最大条数，默认 100）；浏览器模式返回空数组 */
export async function getOperationLogs(limit?: number): Promise<OperationLogEntry[]> {
  if (!isTauri()) {
    console.warn('[logs] 非 Tauri 环境，返回空操作日志');
    return [];
  }
  return invoke<OperationLogEntry[]>('get_operation_logs', { limit: limit ?? 100 });
}