# Design System — desktopcr 风格统一设计系统

从目标截图提取的「浅色 Windows 企业远程桌面客户端」设计语言。所有组件基于 `src/theme/tokens.ts`（TypeScript tokens）与 `src/styles/global.css`（CSS 变量）实现，避免组件内散落硬编码色值。

## 设计定位

- 企业工具型、Windows 桌面原生感、克制清晰、功能优先
- 现代 SaaS 风格，轻描边、低阴影、大留白、统一圆角
- 品牌主色为高饱和蓝色，选中态使用淡蓝背景 + 左侧蓝色细条

## 布局骨架（三段式）

```
+----------------------------------+---------------------------+
| TitleBar（40px，白底细描边）      |   返回 | [icon] desktopcr | 刷新/复制/设置/头像/菜单 | _ | x  |
+----------------------------------+---------------------------+
| Sidebar（232px，浅灰蓝）          |  Main Content（近白）     |
|   我的设备（设备列表+选中高亮）    |   [设备标题] [在线胶囊]   |
|   云设备  → 市场                  |   ┌ 主卡片               │
|   远程协助→ 开始协助/收藏设备      |   │ 壁纸预览 + 进入桌面→  │
|   （弹性留白）                    |   └ 文件传输 | 更多        │
|   设置（底部固定）                |   快速启动：[设备卡]+ [add]│
+----------------------------------+---------------------------+
```

## 颜色 Token

| Token | 值 | 用途 |
| --- | --- | --- |
| `--rd-bg` | `#F7F9FB` | 主内容背景（近白冷灰） |
| `--rd-bg-elevated` | `#FFFFFF` | 卡片/浮层背景 |
| `--rd-bg-sidebar` | `#EEF3F7` | 侧边栏背景（浅灰蓝） |
| `--rd-bg-sidebar-item-active` | `#E8F1FF` | 选中项背景 |
| `--rd-primary` | `#2F7EF7` | 品牌主蓝 |
| `--rd-primary-hover` | `#1E5FD1` | 主蓝 hover |
| `--rd-primary-active` | `#5DA8FF` | 主蓝 active/辅助蓝 |
| `--rd-text-primary` | `#111827` | 主文字（深灰黑） |
| `--rd-text-secondary` | `#4B5563` | 次级文字 |
| `--rd-text-muted` | `#8A94A6` | 弱辅助文字 |
| `--rd-online` | `#34C759` | 在线状态点 |
| `--rd-offline` | `#9CA3AF` | 离线状态点 |
| `--rd-badge-bg` | `#171717` | 状态胶囊黑底 |
| `--rd-badge-text` | `#FFFFFF` | 状态胶囊白字 |
| `--rd-border` | `#D9E1E8` | 卡片边框 |
| `--rd-border-light` | `#E5E7EB` | 分割线/轻描边 |

## 字体与字号

- 字体族：`Segoe UI Variable / Segoe UI / Microsoft YaHei / system-ui`
- 页面主标题 24px / 700，应用名 14px / 600，侧边栏条目 14px / 400（选中 600）
- 卡片操作按钮 14px / 400，状态胶囊 12px / 600

## 间距与圆角

- 间距体系：4/8 制（4/8/12/16/20/24/32/28），页面左右留白 24–32px
- 主卡片圆角 12px，内嵌小卡片 8px，按钮 6px，状态胶囊 999px
- 卡片阴影：`0 1px 2px rgba(16,24,40,.04), 0 1px 3px rgba(16,24,40,.06)`

## 组件规范

### 顶部栏 TitleBar
- 高 40px，白底 92% + 底部 1px 浅描边
- 左：返回按钮 + 蓝色圆角方块应用图标 + 应用名
- 右：图标按钮组（刷新/复制/设置）+ 圆形头像 + 更多，竖线分隔窗口控制（最小化/关闭）
- 窗口关闭按钮 hover 为红底 `#E81123` 白字

### 侧边栏 Sidebar
- 宽 232px，浅灰蓝背景，右侧 1px 浅描边
- 分组可折叠（我的设备/云设备/远程协助），组头 14px/600 + chevron
- 条目：图标（主蓝）+ 文字 + 右侧状态点；选中项淡蓝背景 + 3px 左侧蓝条 + 加粗
- 底部设置项固定，通过弹性空白与上方分隔

### 主内容 DevicePage
- 设备标题 24px/700 + 黑底白字状态胶囊（绿点=在线，灰点=离线）
- 主卡片：顶部 16:7.4 蓝紫渐变壁纸预览 + 白色半透明「进入桌面 →」按钮，底部操作栏（文件传输 | 更多）
- 快速启动：虚线框容器 + 白底快捷设备卡（状态点+名称+元信息+右箭头）+ 居中圆形「+」添加按钮

### 通用 IconButton / StatusBadge
- 图标按钮：32px，透明底，hover 浅灰圆角背景，focus-visible 2px 主蓝描边
- 状态胶囊：黑底 + 6px 圆点 + 白字，圆角 999px

## 暗色模式

截图展示的是亮色模式。若扩展暗色模式，遵循「去饱和/提亮色调变体而非反转」原则，对比度独立验证（正文 ≥4.5:1，次级 ≥3:1）。

## 实现文件

- `src/theme/tokens.ts` — TypeScript 设计 token（palette/spacing/radius/fontSize/shadow）
- `src/styles/global.css` — CSS 变量与基础重置
- `src/components/shared/IconButton.tsx` — IconButton + StatusBadge
- `src/components/TitleBar.tsx` — 顶部栏
- `src/components/Sidebar.tsx` — 侧边栏
- `src/components/DevicePage.tsx` — 主内容区
- `src/App.tsx` — 三段式布局组装
