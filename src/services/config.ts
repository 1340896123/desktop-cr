import { invoke } from '@tauri-apps/api/core';
import { isTauri } from './connection';

/** 对端设备配置（与 Rust 侧 serde 结构一致，camelCase 序列化） */
export interface PeerConfig {
  id: string;
  name: string;
  addr: string;
  platform?: string;
}

/** 应用配置（持久化到 app_config_dir/config.json） */
export interface AppConfig {
  hostEnabled: boolean;
  hostPort: number;
  peers: PeerConfig[];
}

/** 浏览器模式降级时的默认配置 */
const DEFAULT_CONFIG: AppConfig = {
  hostEnabled: false,
  hostPort: 21118,
  peers: [],
};

/** 读取应用配置；浏览器模式返回默认值 */
export async function getAppConfig(): Promise<AppConfig> {
  if (!isTauri()) {
    console.warn('[config] 非 Tauri 环境，返回默认配置');
    return DEFAULT_CONFIG;
  }
  return invoke<AppConfig>('get_app_config');
}

/** 保存应用配置；浏览器模式为 noop */
export async function saveAppConfig(config: AppConfig): Promise<void> {
  if (!isTauri()) {
    console.warn('[config] 非 Tauri 环境，跳过保存配置', config);
    return;
  }
  await invoke('save_app_config', { config });
}

/** 生成唯一对端 id（优先 crypto.randomUUID，兜底 Math.random） */
export function genPeerId(): string {
  return crypto.randomUUID?.() ?? Math.random().toString(36).slice(2);
}
