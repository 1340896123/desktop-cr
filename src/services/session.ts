import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { isTauri } from './connection';
import type { MonitorInfo } from './capture';

/** 会话结构化传输模式 */
export type SessionTransport = 'tcp' | 'udp' | 'relay-udp';

/** 会话实时指标(控制端侧,随 ping/pong 心跳更新) */
export interface SessionMetrics {
  /** 最近一次 ping/pong 往返延迟(毫秒),未测到为 null */
  rttMs: number | null;
  /** 当前连接路径,如 "直连 192.168.1.5" / "中继 ...",未知为 null */
  mode: string | null;
  /** 结构化传输模式(视频数据面实际通道):"tcp" | "udp" | "relay-udp"。
   *  Rust 侧 get_session_metrics 扩展字段;旧后端未提供时为 undefined,浮窗显示 "--"。 */
  transport?: SessionTransport;
}

/** 传输模式中文标签(浮窗展示用) */
export function transportLabel(transport: SessionTransport | null | undefined): string {
  switch (transport) {
    case 'tcp':
      return 'TCP 直连';
    case 'udp':
      return 'UDP 直连';
    case 'relay-udp':
      return 'UDP 中继';
    default:
      return '--';
  }
}

/** 获取当前会话的实时指标 */
export async function getSessionMetrics(): Promise<SessionMetrics | null> {
  if (!isTauri()) {
    console.warn('[session] 非 Tauri 环境，返回演示会话指标(dev 占位)');
    // dev 占位:纯浏览器模式无真实会话,transport 返回 tcp 仅用于浮窗演示
    return { rttMs: null, mode: null, transport: 'tcp' };
  }
  return invoke<SessionMetrics>('get_session_metrics');
}

/** 向被控端请求显示器列表(应答通过 remote-monitors 事件返回) */
export async function requestRemoteMonitors(): Promise<void> {
  if (!isTauri()) {
    console.warn('[session] 非 Tauri 环境，跳过请求远程显示器');
    return;
  }
  await invoke('request_remote_monitors');
}

/** 切换远程会话的目标显示器(下发 Stream.monitor 到被控端,实时切换其抓帧) */
export async function selectSessionMonitor(monitorId: number): Promise<void> {
  if (!isTauri()) {
    console.warn('[session] 非 Tauri 环境，跳过切换远程显示器', monitorId);
    return;
  }
  await invoke('select_session_monitor', { monitorId });
}

/** 订阅远程显示器列表事件(remote-monitors),返回取消订阅函数 */
export async function onRemoteMonitors(
  handler: (monitors: MonitorInfo[]) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) {
    console.warn('[session] 非 Tauri 环境，使用空远程显示器事件源');
    return () => {
      /* noop */
    };
  }
  return listen<{ monitors: MonitorInfo[] }>('remote-monitors', (event) =>
    handler(event.payload.monitors),
  );
}
