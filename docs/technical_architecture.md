# WinUI Remote Desktop 技术架构文档

> 本文档由 README.md 提炼整理，作为项目技术架构基线。原始需求见 `/workspace/README.md`。

## 1. 项目定位

基于 **Tauri v2 + React 18 (Fluent UI React v9)** 构建的远程桌面客户端，复用 **RustDesk 开源生态**（通信协议、中继服务、网络打洞、控制注入及 IDD 虚拟显示器驱动），具备现代 Windows 11 (WinUI 3) 视觉风格。

## 2. 整体架构

系统采用 **分层解耦 + 事件驱动** 设计：前端为纯粹的展示与交互控制层，Rust 侧为核心通信引擎与底层系统驱动接口层，中间通过 Tauri IPC Bridge (Commands / Events) 通信。

```
+-----------------------------------------+
|  前端 UI 层 (React + Fluent UI v9)      |
|  设备列表 / 控制栏 / Video Canvas       |
+--------------------|--------------------+
                     | Tauri IPC (Commands/Events)
+--------------------v--------------------+
|  Tauri Backend Core (Rust)              |
|  Signaling(hbb_common) / Video(Scrap)   |
|  Input(Enigo/SendInput) / IDD Driver    |
+--------------------|--------------------+
                     | P2P / Relay / Stream / Win32 API
+--------------------v--------------------+
|  网络与系统基础设施 (HBBS/HBBR/远程主机) |
+-----------------------------------------+
```

## 3. 技术栈选型

| 模块 | 技术选型 | 复用策略 |
| --- | --- | --- |
| GUI 框架 | Tauri v2 + React 18 | 内存低、体积小（<15MB）、支持 Web Workers/WebRTC |
| UI 组件库 | Fluent UI React v9 | 微软官方 Fluent Design / WinUI 3 风格 |
| 网络与打洞 | RustDesk hbb_common | NAT 打洞、hbbs/hbbr 信令协议与 TLS 加密 |
| 屏幕采集 | RustDesk scrap / DXGI | Win32 / DXGI 高性能抓屏 |
| 虚拟屏支持 | RustDesk IDD Driver | 多分辨率/多刷新率动态挂载 |
| 控制注入 | RustDesk enigo / Win32 SendInput | 键盘、鼠标、剪贴板数据同步 |

## 4. 画面传输与渲染管道

- **高帧率模式（推荐）**：Rust 复用 RustDesk 通信栈打通 P2P，将 DXGI/IDD 帧经 H.264/AV1 硬件编码送入 WebRTC Video Track，前端 `<video>` 渲染，延迟 ≤ 30ms。
- **兼容模式**：Rust 将 RGB/YUV 原始帧写入共享内存 / 二进制 Channel，前端用 Web Workers 在 `<canvas>` 上绘制。

## 5. 控制事件管线

前端 Canvas 监听 `onPointerMove` / `onKeyDown` / `onWheel`，按坐标归一化公式换算后发送给 Rust：

```
X_remote = x * W_remote / W_css
Y_remote = y * H_remote / H_css
```

Rust 收到 JSON Payload 后调用 Win32 `SendInput` 模拟系统级事件。TS 侧事件结构体：

```ts
interface MouseInputPayload {
  event_type: 'mousemove' | 'mousedown' | 'mouseup' | 'wheel';
  x: number;
  y: number;
  button?: 'left' | 'right' | 'middle';
  delta_y?: number;
}
```

## 6. 工程目录结构

```
/workspace/
├── docs/
│   └── technical_architecture.md      # 本架构文档
├── src/                               # React 前端工程 (Vite + TS)
│   ├── assets/                        # 静态资源
│   ├── components/
│   │   ├── ControlBar.tsx             # 顶部悬浮工具栏
│   │   ├── RemoteCanvas.tsx           # 远程画面 Canvas/Video 封装
│   │   └── VirtualDisplayPanel.tsx    # 虚拟屏管理面板
│   ├── services/
│   │   ├── connection.ts              # Tauri IPC invoke/listen 封装
│   │   └── input.ts                   # 控制事件发送封装
│   ├── App.tsx                        # 入口 + FluentProvider + 侧边导航
│   ├── main.tsx
│   ├── vite-env.d.ts
│   └── index.html
├── src-tauri/                         # Tauri Rust 后端工程
│   ├── resources/idd_driver/          # IDD 虚拟显示器驱动资源
│   ├── src/
│   │   ├── hbb_client.rs              # RustDesk 逻辑封装（跨平台占位）
│   │   ├── virtual_display.rs         # IDD 虚拟屏控制（cfg 平台分离）
│   │   ├── input_injector.rs          # 鼠标键盘注入（cfg 平台分离）
│   │   ├── capture.rs                 # 屏幕抓取（cfg 平台分离）
│   │   └── main.rs                    # Tauri 注册与 Commands 绑定
│   ├── Cargo.toml
│   ├── build.rs
│   └── tauri.conf.json
├── package.json
├── vite.config.ts
├── tsconfig.json
└── .gitignore
```

## 7. 跨平台策略

| 模块 | Windows | Linux（当前开发环境） |
| --- | --- | --- |
| virtual_display | devcon/nefcon 挂载 IDD 驱动 | 模拟成功，返回自增 monitor id |
| input_injector | Win32 SendInput（`windows` crate） | 仅记录日志，返回成功 |
| capture | scrap / DXGI 抓帧 | 返回模拟帧或 NotSupported |
| hbb_client | hbb_common 真实信令 | 内存 mock 状态 |

Windows 专属代码均通过 `#[cfg(target_os = "windows")]` 隔离；RustDesk 的 `hbb_common` / `scrap` 以注释形式保留在 `Cargo.toml`（避免 Linux 构建失败），待 Windows 打包时启用。

## 8. 开发里程碑

- [x] **阶段一：环境搭建与 POC 验证** — Tauri v2 + Fluent UI v9 工程骨架，Rust 跨平台可编译，前端可独立预览。
- [ ] **阶段二：虚拟屏驱动集成** — 封装 Rust 控制 Windows IDD 驱动接口，前端一键增加 1080P/2K/4K 虚拟屏。
- [ ] **阶段三：画面抓取与传输** — DXGI/Scrap 帧捕获，WebRTC / Shared Memory 通道传输到前端。
- [ ] **阶段四：控制注入与细节优化** — 按键与鼠标精确映射，全屏/分辨率/剪贴板等功能完备。
