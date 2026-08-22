import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { isTauri } from './connection';

/**
 * 一帧本机抓屏画面（capture-frame 事件负载）。
 * Rust 端 DXGI 帧按流配置编码后原样推送：
 *   - codec="h264"/"hevc"：H.264/H.265 Annex-B 编码字节（前端经 WebCodecs 解码）；
 *   - codec="bgra"：BGRA 原始像素字节（前端通道交换转 RGBA 后 putImageData）。
 * 禁止 JPEG。
 */
export interface CapturedFrame {
  monitorId: number;
  width: number;
  height: number;
  /** 是否为关键帧 */
  key: boolean;
  /** 帧编码格式："h264" | "hevc" | "bgra" */
  codec: string;
  /** 编码帧字节（Annex-B）或 BGRA 原始字节 */
  data: number[];
  /** 是否为模拟画面（非 Windows 平台动画帧；真实抓帧缺省/为 false） */
  simulated?: boolean;
}

/** 显示器信息（真实枚举，IDD 虚拟屏以 isVirtual 标记） */
export interface MonitorInfo {
  id: number;
  name: string;
  width: number;
  height: number;
  isPrimary: boolean;
  isVirtual: boolean;
}

/**
 * 远程帧（remote-frame 事件负载）。
 * Rust 控制端 peer_read_loop 收到 Msg::Frame 后原样透传编码帧字节，
 * 前端用 WebCodecs VideoDecoder 解码渲染（不再解码转图像，禁止 JPEG）。
 */
export interface RemoteFrame {
  width: number;
  height: number;
  /** 编码帧字节（H.264/H.265 Annex-B） */
  data: number[];
  /** 帧序号（用于丢包/乱序统计；UDP 模式为 frame_id、TCP 模式为推流 seq，两域独立） */
  seq: number;
  /** 编码耗时（毫秒）；UDP 模式分片头不携带该值 → null（未知，显示 "--"，不造假为 0） */
  dur: number | null;
  /** 是否为关键帧 */
  key: boolean;
  /** 编码格式："h264" | "hevc" */
  codec: string;
  /**
   * 本帧真实传输通道："udp" | "relay-udp" | "tcp"（Rust 侧 emit 时按当帧来源
   * 标注——UDP 重组循环 / TCP 读循环）。丢包统计以帧级标记为准，消除 metrics
   * 2 秒轮询滞后窗口内的跨域错标；字段缺失（旧负载）回退 metrics 轮询值。
   */
  transport?: string;
  /** 是否为模拟画面（协议未携带该字段时保持 undefined，按 false 处理） */
  simulated?: boolean;
}

export interface StartCaptureOptions {
  monitorId: number;
  width: number;
  height: number;
  fps?: number;
}

/** 开始抓取指定显示器的画面流 */
export async function startCapture(options: StartCaptureOptions): Promise<void> {
  if (!isTauri()) {
    console.warn('[capture] 非 Tauri 环境，跳过开始抓帧', options);
    return;
  }
  await invoke('start_capture', {
    monitorId: options.monitorId,
    width: options.width,
    height: options.height,
    fps: options.fps ?? 30,
  });
}

/** 停止抓帧 */
export async function stopCapture(): Promise<void> {
  if (!isTauri()) {
    console.warn('[capture] 非 Tauri 环境，跳过停止抓帧');
    return;
  }
  await invoke('stop_capture');
}

/** 枚举本机所有显示器（真实 EnumDisplayDevicesW；浏览器模式返回空数组） */
export async function listMonitors(): Promise<MonitorInfo[]> {
  if (!isTauri()) {
    console.warn('[capture] 非 Tauri 环境，返回空显示器列表');
    return [];
  }
  return invoke<MonitorInfo[]>('list_monitors');
}

/**
 * 主动拉取最新一帧（Rust 端返回结构：{ width, height, format, data }）。
 * Rust 端在尚无帧时会返回错误，此时降级为 null。
 */
export interface PulledFrame {
  width: number;
  height: number;
  format: string;
  data: number[];
}

export async function getFrame(monitorId: number): Promise<PulledFrame | null> {
  if (!isTauri()) {
    console.warn('[capture] 非 Tauri 环境，返回 null 帧');
    return null;
  }
  try {
    return await invoke<PulledFrame>('get_frame', { monitorId });
  } catch {
    return null;
  }
}

/** 订阅实时抓帧事件（capture-frame，本机预览），返回取消订阅函数 */
export async function onFrame(handler: (frame: CapturedFrame) => void): Promise<UnlistenFn> {
  if (!isTauri()) {
    console.warn('[capture] 非 Tauri 环境，使用空事件源');
    return () => {
      /* noop */
    };
  }
  return listen<CapturedFrame>('capture-frame', (event) => handler(event.payload));
}

/** 订阅远程帧事件（remote-frame，来自被控端），返回取消订阅函数 */
export async function onRemoteFrame(handler: (frame: RemoteFrame) => void): Promise<UnlistenFn> {
  if (!isTauri()) {
    console.warn('[capture] 非 Tauri 环境，使用空远程帧事件源');
    return () => {
      /* noop */
    };
  }
  return listen<RemoteFrame>('remote-frame', (event) => handler(event.payload));
}
