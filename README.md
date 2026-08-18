# WinUI Remote Desktop (Tauri + Fluent UI React v9 + RustDesk)

本文档旨在指导如何利用 **Tauri v2 + React (Fluent UI React v9)** 作为前端界面框架，并深度复用 **RustDesk 开源生态**（通信协议、中继服务、网络打洞、控制注入及 IDD 虚拟显示器驱动），构建一套具备现代 **Windows 11 (WinUI 3)** 视觉风格的轻量级、高性能远程桌面客户端。

## 目录

- [1. 整体架构与设计理念](#1-整体架构与设计理念)
- [2. 核心模块与 RustDesk 生态复用方案](#2-核心模块与-rustdesk-生态复用方案)
- [3. 画面传输与渲染管道](#3-画面传输与渲染管道)
- [4. 控制事件管线](#4-控制事件管线)
- [5. UI 架构设计](#5-ui-架构设计)
- [6. 项目工程目录结构](#6-项目工程目录结构)
- [7. 开发里程碑路线图](#7-开发里程碑路线图)

## 1. 整体架构与设计理念

系统采用 **分层解耦 + 事件驱动** 的设计。前端充当纯粹的展示与交互控制层，Rust 侧充当核心通信引擎与底层系统驱动接口层。

### 1.1 架构图解

```
+-----------------------------------------------------------------------------------+
|                            前端 UI 层 (React + Fluent UI v9)                       |
|  +----------------------+ +----------------------+ +---------------------------+  |
|  |   设备列表与设置 UI   | |  控制栏 / 悬浮工具栏 | | Video Canvas / RTC Player |  |
|  +----------------------+ +----------------------+ +---------------------------+  |
+-----------------------------------------|-----------------------------------------+
                                          | Tauri IPC Bridge (Commands / Events)
+-----------------------------------------v-----------------------------------------+
|                            Tauri Backend Core (Rust)                              |
|  +------------------+ +-------------------+ +-------------------+ +------------+  |
|  |  Signaling Client| |   Video Pipeline  | |   Input Service   | | IDD Driver |  |
|  |   (hbb_common)   | |  (Scrap / DXGI)   | |   (Win32 / Enigo) | | Controller |  |
|  +--------|---------+ +---------|---------+ +---------|---------+ +-----|------+  |
+-----------|---------------------|-------------------|-----------------|-----------+
            | P2P / Relay         | Stream            | Events          | Win32 API
+-----------v---------------------v-------------------v-----------------v-----------+
|                              网络与系统基础基础设施                                |
|  +----------------------+ +----------------------+ +---------------------------+  |
|  | RustDesk HBBS / HBBR | |  Remote Target HW/OS | | RustDesk Virtual Display  |  |
|  +----------------------+ +----------------------+ +---------------------------+  |
+-----------------------------------------------------------------------------------+
```

### 1.2 技术栈选型

| 模块 | 技术选型 | 选用理由 / 复用策略 |
| --- | --- | --- |
| GUI 框架 | Tauri v2 + React 18 | 内存占用低，打包体积小（< 15MB），原生支持 Web Workers / WebRTC |
| UI 组件库 | Fluent UI React v9 (@fluentui/react-components) | 微软官方 Fluent Design / WinUI 3 风格组件库，体验高度原生 |
| 网络与打洞 | RustDesk hbb_common | 深度复用 RustDesk 的 NAT 打洞、hbbs/hbbr 信令协议与 TLS 加密 |
| 屏幕采集 | RustDesk scrap / DXGI | 复用 RustDesk 的 Win32 / DXGI 高性能屏幕抓取代码 |
| 虚拟屏支持 | RustDesk IDD Driver (rustdesk-idd-driver) | 微软 Windows Indirect Display Driver，支持多分辨率/多刷新率动态挂载 |
| 控制注入 | RustDesk enigo / Win32 SendInput | 模拟键盘、鼠标、剪贴板数据同步 |

## 2. 核心模块与 RustDesk 生态复用方案

### 2.1 Cargo 依赖

在 `src-tauri/Cargo.toml` 中，直接引入 RustDesk 的核心子模块，无需重写通信与加密逻辑：

```toml
[package]
name = "winui-remote-desktop"
version = "0.1.0"
edition = "2021"

[dependencies]
tauri = { version = "2.0.0-rc", features = ["protocol-asset"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1.35", features = ["full"] }

# 复用 RustDesk 核心依赖 (定位到 git 仓库的 libs 目录或本地 submodule)
hbb_common = { git = "https://github.com/rustdesk/rustdesk", subdirectory = "libs/hbb_common" }
scrap = { git = "https://github.com/rustdesk/rustdesk", subdirectory = "libs/scrap" }

# Windows 底层支持
windows = { version = "0.52", features = ["Win32_Foundation", "Win32_UI_Input_KeyboardAndMouse"] }
```

### 2.2 虚拟屏幕 (Virtual Display) 驱动集成

RustDesk 基于微软 IDD 框架实现了动态虚拟屏。本架构通过 Tauri Command 对其进行控制。

驱动准备：将编译好的 `rustdesk-idd-driver.dll` 与 `cert.cer` 放入安装包资源目录 `src-tauri/resources/idd_driver/`。

Rust 控制逻辑 (`src-tauri/src/virtual_display.rs`)：

```rust
use std::process::Command;

#[tauri::command]
pub fn install_virtual_display_driver() -> Result<String, String> {
    // 调用 nefcon 或 devcon 挂载驱动
    let output = Command::new("idd_driver/setup.exe")
        .arg("install")
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok("Virtual display driver installed successfully".into())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

#[tauri::command]
pub fn add_virtual_monitor(width: u32, height: u32, fps: u32) -> Result<u32, String> {
    // 通过 RustDesk 的 IDD API 动态添加虚拟显示器
    // 刷新率 T_f = 1000 / fps (ms)
    log::info!("Adding virtual display: {}x{} @ {}Hz", width, height, fps);
    // 返回屏幕索引或 ID
    Ok(1)
}
```

## 3. 画面传输与渲染管道

画面传输采用 **低延迟 P2P/WebRTC/WebSocket 直连** 方案：

```
+------------------+         +-------------------+         +---------------------+
| 远程主机采集端   |         | Rust 转换与编码层 |         | 前端渲染层 (Canvas) |
| (DXGI / Scrap)   | ------> | (H.264 / VP9 /    | ------> | WebRTC DataChannel  |
| / 虚拟显示器     |         | Shared Memory)    |         | or Web Workers      |
+------------------+         +-------------------+         +---------------------+
```

### 3.1 视频传输两条路线

**高帧率模式 (推荐 - WebRTC Video Track / DataChannel)：**

- Rust 侧复用 RustDesk 通信栈打通 P2P 通道
- 将 DXGI/IDD 抓取的帧通过 H.264/AV1 硬件编码，送入 WebRTC Video Track
- 前端使用 `<video>` 标签渲染，支持硬件加速解码，延迟 ≤ 30ms

**兼容模式 (Shared Memory / ImageData)：**

- 在同机模式或高安全限制环境下，Rust 将 RGB/YUV 原始帧写入 Tauri 共享内存/二进制 Channel
- 前端使用 Web Workers 在 `<canvas>` 上绘制

## 4. 控制事件管线

为了确保操控精准度（包括高 DPI 缩放、多屏坐标映射及组合键拦截）：

1. **前端捕获**：前端 Canvas 监听 `onPointerMove`, `onKeyDown`, `onWheel` 事件。
2. **坐标归一化**：假设 Canvas 显示分辨率为 `(W_css, H_css)`，被控端实际虚拟屏分辨率为 `(W_remote, H_remote)`，点击坐标为 `(x, y)`，转换公式为：

   ```
   X_remote = x * W_remote / W_css
   Y_remote = y * H_remote / H_css
   ```

3. **Rust 注入**：Rust 收到规范化的 JSON Payload，调用 Win32 `SendInput` API 进行系统级事件模拟。

```ts
// TS 端事件结构体
interface MouseInputPayload {
  event_type: 'mousemove' | 'mousedown' | 'mouseup' | 'wheel';
  x: number; // 归一化后的 X 坐标
  y: number; // 归一化后的 Y 坐标
  button?: 'left' | 'right' | 'middle';
  delta_y?: number;
}
```

## 5. UI 架构设计

UI 遵循 Windows 11 WinUI 3 设计语言，主要包括以下组件布局：

```
+-------------------------------------------------------------------------+
| [≡] RemoteDesktop WinUI                       [-][□][✕]                 |
+---------------+---------------------------------------------------------+
|  侧边导航     |  主控制区                                                |
|  [🏠] 设备    |  +---------------------------------------------------+  |
|  [🖥️] 虚拟屏  |  | 远程控制台 (Remote Session Window)                 |  |
|  [⚙️] 设置    |  | +-----------------------------------------------+ |  |
|               |  | |                                               | |  |
|               |  | |            [ Canvas / Video Stream ]          | |  |
|               |  | |                                               | |  |
|               |  | +-----------------------------------------------+ |  |
|               |  +---------------------------------------------------+  |
|               |  | 顶部悬浮工具栏 (Toolbar: 画质/分辨率/虚拟屏配置)  |  |
+---------------+---------------------------------------------------------+
```

### 5.1 前端核心 Provider 配置 (`src/App.tsx`)

```tsx
import React, { useState } from 'react';
import {
  FluentProvider,
  webDarkTheme,
  webLightTheme,
  Button,
  TabList,
  Tab
} from '@fluentui/react-components';
import { Desktop28Regular, Settings28Regular, Add28Regular } from '@fluentui/react-icons';

export const App: React.FC = () => {
  const [isDarkMode, setIsDarkMode] = useState(true);

  return (
    <FluentProvider theme={isDarkMode ? webDarkTheme : webLightTheme}>
      <div style={{ display: 'flex', height: '100vh', width: '100vw' }}>
        {/* WinUI 侧边导航栏 */}
        <nav style={{ width: '64px', borderRight: '1px solid #333', padding: '8px' }}>
          <TabList vertical defaultSelectedValue="devices">
            <Tab value="devices" icon={<Desktop28Regular />} />
            <Tab value="virtual_display" icon={<Add28Regular />} />
            <Tab value="settings" icon={<Settings28Regular />} />
          </TabList>
        </nav>

        {/* 主内容区域 */}
        <main style={{ flex: 1, position: 'relative', overflow: 'hidden' }}>
          {/* Canvas Viewport 渲染区域 */}
        </main>
      </div>
    </FluentProvider>
  );
};
```

## 6. 项目工程目录结构

```
my-tauri-remote-desktop/
├── docs/
│   └── technical_architecture.md   # 本架构文档
├── src/                            # React 前端工程
│   ├── assets/                     # 静态资源
│   ├── components/                 # Fluent UI 业务组件
│   │   ├── ControlBar.tsx          # 顶部悬浮工具栏
│   │   ├── RemoteCanvas.tsx        # 远程画面 Canvas / Video 封装
│   │   └── VirtualDisplayPanel.tsx # 虚拟屏管理面板
│   ├── services/                   # Tauri IPC 封装层 (Invoke / Listen)
│   │   ├── connection.ts
│   │   └── input.ts
│   ├── App.tsx                     # 应用入口与 FluentProvider 根组件
│   └── main.tsx
├── src-tauri/                      # Tauri Rust 后端工程
│   ├── resources/                  # 包含 idd_driver 驱动文件
│   │   └── idd_driver/
│   ├── src/
│   │   ├── hbb_client.rs           # RustDesk hbb_common 逻辑封装
│   │   ├── virtual_display.rs      # IDD 驱动挂载与控制逻辑
│   │   ├── input_injector.rs       # Win32 SendInput 鼠标键盘注入
│   │   ├── capture.rs              # 屏幕抓取逻辑 (Scrap/DXGI)
│   │   └── main.rs                 # Tauri 注册与 Commands 绑定
│   ├── Cargo.toml                  # Rust 依赖声明
│   └── tauri.conf.json             # Tauri 配置文件
└── package.json
```

## 7. 开发里程碑路线图

- [ ] **阶段一：环境搭建与 POC 验证**
      集成 Tauri v2 + Fluent UI v9。将 RustDesk `hbb_common` 导入后端，验证底层 ID/中继服务器注册。
- [ ] **阶段二：虚拟屏驱动集成**
      封装 Rust 控制 Windows IDD 驱动的接口。在前端通过 UI 实现"一键增加 1080P/2K/4K 虚拟屏"。
- [ ] **阶段三：画面抓取与传输**
      实现 DXGI/Scrap 对指定虚拟屏的帧捕获。建立 WebRTC / Shared Memory 通道将画质传输到前端 `<video>` / `<canvas>`。
- [ ] **阶段四：控制注入与细节优化**
      精确映射按键与鼠标相对/绝对坐标。Fluent UI 悬浮控制栏（全屏切换、分辨率调节、剪贴板同步等）功能完备。
