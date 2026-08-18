/**
 * 统一设计系统（Design Tokens）
 * 从目标截图提取的「desktopcr」浅色企业工具风格：
 *   - 主背景近白略带冷灰，侧边栏浅灰蓝形成轻微分区
 *   - 品牌主色为高饱和蓝色，选中态使用淡蓝背景 + 左侧蓝色细条
 *   - 文字层级：深灰黑 / 次级灰 / 弱辅助灰
 *   - 轻描边、低阴影、大留白、统一圆角
 */

export const palette = {
  // 背景层级
  background: '#F7F9FB',
  backgroundElevated: '#FFFFFF',
  sidebar: '#EEF3F7',
  sidebarItemHover: '#E3ECF5',
  sidebarItemActive: '#E8F1FF',
  muted: '#F1F3F5',

  // 品牌主色
  primary: '#2F7EF7',
  primaryHover: '#1E5FD1',
  primaryActive: '#5DA8FF',
  primarySoft: '#E8F1FF',

  // 文字
  textPrimary: '#111827',
  textSecondary: '#4B5563',
  textMuted: '#8A94A6',
  textOnPrimary: '#FFFFFF',

  // 状态
  online: '#34C759',
  offline: '#9CA3AF',
  onlineBadgeBg: '#171717',
  onlineBadgeText: '#FFFFFF',

  // 边框与分割线
  border: '#D9E1E8',
  borderLight: '#E5E7EB',

  // 功能色
  destructive: '#DC2626',
  warning: '#D97706',
} as const;

export const spacing = {
  xxs: 4,
  xs: 8,
  sm: 12,
  md: 16,
  lg: 20,
  xl: 24,
  xxl: 32,
  section: 28,
} as const;

export const radius = {
  card: '12px',
  cardInner: '8px',
  control: '6px',
  pill: '999px',
  circle: '50%',
} as const;

export const fontSize = {
  xs: 12,
  sm: 13,
  md: 14,
  lg: 15,
  xl: 16,
  title: 24,
  appTitle: 15,
} as const;

export const fontFamily =
  `-apple-system, "Segoe UI Variable", "Segoe UI", "Microsoft YaHei UI", "Microsoft YaHei", ` +
  `system-ui, sans-serif`;

export const shadow = {
  card: '0 1px 2px rgba(16, 24, 40, 0.04), 0 1px 3px rgba(16, 24, 40, 0.06)',
  popover: '0 4px 12px rgba(16, 24, 40, 0.08), 0 2px 4px rgba(16, 24, 40, 0.04)',
} as const;

export const breakpoints = {
  sidebar: 800,
} as const;

/** 侧边栏宽度（与截图一致） */
export const sidebarWidth = 232;

/** 顶部栏高度 */
export const titleBarHeight = 40;

export const zIndex = {
  titleBar: 40,
  controlBar: 10,
  modal: 100,
} as const;
