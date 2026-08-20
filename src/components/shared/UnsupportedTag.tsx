import React from 'react';

interface UnsupportedTagProps {
  /** 标签文案，默认「不支持」 */
  label?: string;
  /**
   * 变种：
   * - unsupported：橙黄警示，用于「当前版本不支持 / 暂未开放」的功能
   * - demo：灰阶中性，用于「本地演示 / 非真实数据」的展示性内容
   */
  variant?: 'unsupported' | 'demo';
  style?: React.CSSProperties;
}

const base: React.CSSProperties = {
  display: 'inline-block',
  marginLeft: 6,
  padding: '1px 7px',
  fontSize: 11,
  fontWeight: 600,
  lineHeight: '16px',
  borderRadius: 4,
  whiteSpace: 'nowrap',
  fontFamily: '-apple-system, "Segoe UI", "Microsoft YaHei", system-ui, sans-serif',
  verticalAlign: 'middle',
};

/**
 * 统一的「不支持 / 暂未开放 / 演示」标识，用于明确标注生产 Tauri 中
 * 尚未实现或仅本地演示、无真实后端支撑的 UI 段落，避免用户误判为可用功能。
 */
export const UnsupportedTag: React.FC<UnsupportedTagProps> = ({
  label = '不支持',
  variant = 'unsupported',
  style,
}) => {
  const merged: React.CSSProperties =
    variant === 'demo'
      ? { ...base, backgroundColor: '#F3F4F6', color: '#6B7280', border: '1px solid #E5E7EB', ...style }
      : { ...base, backgroundColor: '#FEF3C7', color: '#92400E', border: '1px solid #F59E0B', ...style };
  return <span style={merged}>{label}</span>;
};

export default UnsupportedTag;
