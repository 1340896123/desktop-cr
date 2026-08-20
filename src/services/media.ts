import { invoke } from '@tauri-apps/api/core';
import { isTauri } from './connection';

/** 音视频全链路测试报告(Rust PipelineReport camelCase 序列化) */
export interface PipelineReport {
  /** 管道类型:"video" | "audio" | "both" */
  kind: string;
  /** 采集/编码的视频帧数 */
  frames: number;
  /** 帧宽度 */
  frameWidth: number;
  /** 帧高度 */
  frameHeight: number;
  /** 采集的音频采样数 */
  audioSamples: number;
  /** 音频采样率(Hz) */
  audioRate: number;
  /** 音频声道数 */
  audioChannels: number;
  /** 输出文件目录 */
  outDir: string;
  /** 总耗时(毫秒) */
  elapsedMs: number;
}

/**
 * 运行音视频全链路测试(采集 → 编码 → 回环传输 → 解码 → 落盘)。
 * 注意:Tauri v2 默认将 Rust 参数名转为 camelCase 键,此处须用 camelCase。
 */
export async function runMediaPipelineTest(
  kind: 'video' | 'audio' | 'both',
  seconds: number,
  outDir: string,
): Promise<PipelineReport | null> {
  if (!isTauri()) {
    console.warn('[media] 非 Tauri 环境，跳过音视频全链路测试', { kind, seconds, outDir });
    return null;
  }
  return invoke<PipelineReport>('run_media_pipeline_test', {
    kind,
    seconds,
    outDir,
  });
}
