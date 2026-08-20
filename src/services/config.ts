import { invoke } from '@tauri-apps/api/core';
import { isTauri } from './connection';

/** 对端设备配置（与 Rust 侧 serde 结构一致，camelCase 序列化） */
export interface PeerConfig {
  id: string;
  name: string;
  addr: string;
  platform?: string;
}

/** 账号登录会话（登录 dcr-signal 服务后持久化） */
export interface AccountSession {
  server: string;
  username: string;
  token: string;
}

/** 应用配置（持久化到 app_config_dir/config.json） */
export interface AppConfig {
  hostEnabled: boolean;
  hostPort: number;
  peers: PeerConfig[];
  /** 退出后保持被控端运行(仅持久化,系统级自启暂未实现) */
  keepRunningOnExit: boolean;
  /** 直连失败时允许经中继服务器兜底转发 */
  relayFallbackEnabled: boolean;
  /** 信令服务器地址 "ip:port"，配置后被控端向其注册并心跳 */
  signalServer?: string;
  /** 中继服务器地址 "ip:port"，直连失败时经其中继转发 */
  relayServer?: string;
  /** 本机唯一 ID，信令注册用，默认 "dcr-<主机名>" */
  hostId: string;
  /** 账号登录会话，登录后解锁应用 */
  account?: AccountSession;
}

/** 浏览器模式降级时的默认配置 */
const DEFAULT_CONFIG: AppConfig = {
  hostEnabled: false,
  hostPort: 21118,
  peers: [],
  keepRunningOnExit: false,
  relayFallbackEnabled: true,
  signalServer: '120.78.77.248:21116',
  relayServer: '120.78.77.248:21117',
  hostId: 'dcr-browser',
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
