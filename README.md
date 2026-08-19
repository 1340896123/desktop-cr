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
- [8. 已交付能力清单](#8-已交付能力清单)
- [9. 信令 / STUN / TURN 服务](#9-信令--stun--turn-服务)

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

> 注记(项目收尾后现状):RustDesk 的 `hbb_common`/`scrap` 是 git submodule 形态的重依赖链(多个 rustdesk-org fork 依赖、sodiumoxide C 编译、bindgen 等),在 Windows 上构建脆弱且需要 libclang;`hbb_common` 作为 git 依赖无法拉取 submodule 内容。当前网络层改为**自研直连 TCP 协议**(`network.rs`,长度前缀 JSON 帧 + base64 JPEG),真实实现了 LAN 远程控制的全部能力;抓屏直接用 windows crate 的 DXGI API(与 scrap 底层同一技术),已隔离在 `capture.rs`,未来可平滑替换为 RustDesk 生态。

实际依赖(`src-tauri/Cargo.toml`):

```toml
[dependencies]
tauri = { version = "2", features = [] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
log = "0.4"
env_logger = "0.11"

# 轻量纯 Rust 依赖:协议 base64 / JPEG 编码 / IDD dylib 动态加载
base64 = "0.22"
jpeg-encoder = "0.6"
libloading = "0.8"

[target.'cfg(windows)'.dependencies]
windows = { version = "0.58", features = [
  "Win32_Foundation",
  "Win32_UI_Input_KeyboardAndMouse",
  "Win32_Graphics_Dxgi", "Win32_Graphics_Direct3D", "Win32_Graphics_Direct3D11",
  "Win32_Graphics_Gdi", "Win32_System_LibraryLoader", "Win32_System_Registry",
  "Win32_System_DataExchange", "Win32_System_Memory", "Win32_System_SystemServices",
  "Win32_Security",
] }
```

### 2.2 虚拟屏幕 (Virtual Display) 驱动集成

基于微软 IDD(Indirect Display Driver)框架,本架构通过 Tauri Command 控制虚拟显示器增删。已落地真实实现:

驱动资源(`src-tauri/resources/idd_driver/`,随安装包打包,来源:RustDesk 官方 Windows 发行包 + Amyuni usbmmidd 官方工具包):

- `usbmmIdd.inf` / `usbmmidd.cat`(Amyuni 微软签名 IDD 驱动)
- `x64/usbmmIdd.dll`、`Win32/usbmmIdd.dll`(驱动二进制)
- `deviceinstaller64.exe` / `deviceinstaller.exe`(官方驱动安装器)
- `dylib_virtual_display.dll`(RustDesk IDD 控制 DLL,`libloading` 加载,失败静默回退 usbmmidd)

控制逻辑(`src-tauri/src/virtual_display.rs`,Windows 真实路径):

1. **安装驱动**:`deviceinstaller64 install usbmmIdd.inf usbmmidd`(需管理员权限,失败返回真实错误信息)。
2. **添加虚拟屏**:写入注册表 `HKLM\...\WUDF\Services\usbmmIdd\Parameters\Monitors` 的分辨率列表(目标分辨率放首位),再执行 `deviceinstaller64 enableidd 1`(每次新增一个,最多 4 个),返回新显示器 id 并广播 `virtual-monitors-changed`。
3. **枚举/移除**:`EnumDisplayDevicesW` 真实枚举(按 DeviceString 识别 usbmmidd/Indirect 虚拟屏);`deviceinstaller64 enableidd 0` 移除。
4. **可选增强**:`libloading` 动态加载 `dylib_virtual_display.dll`,当 `is_device_created()` 为真时经 `plug_in_monitor`/`plug_out_monitor` 操作 RustDesk IDD 驱动。

> 刷新率周期 T_f = 1000 / fps (ms) 由驱动侧消费;安装驱动/增删虚拟屏均需要 UAC 管理员权限,非管理员时返回明确提示。

## 3. 画面传输与渲染管道

画面传输采用 **真实 DXGI 抓屏 + JPEG 编码 + LAN TCP / Tauri IPC 事件** 双通道:

```
+------------------+         +-------------------+         +---------------------+
| 远程主机采集端   |         | Rust 编码层        |         | 前端渲染层 (Canvas)  |
| (DXGI Output     | ------> | (JPEG 编码,       | ------> | createImageBitmap +  |
|  Duplication)    |         |  base64/TCP 帧)   |         | drawImage)           |
+------------------+         +-------------------+         +---------------------+
```

### 3.1 视频传输两条路线

**远程模式 (已实现 - LAN TCP 帧流)：**

- 被控端 `capture.rs` 用 IDXGIOutputDuplication 抓帧 → jpeg-encoder 编码 → `network.rs` 以 4 字节长度前缀 JSON 帧(`frame` 消息,base64 JPEG)推送到控制端
- 控制端 `remote-frame` 事件携带 JPEG 字节 → 前端 `<canvas>` `createImageBitmap` + `drawImage` 渲染
- 流参数(画质/分辨率/帧率)经 `stream` 消息实时下发被控端并即时生效

**本机预览模式 (已实现 - IPC 事件)：**

- 同一套 DXGI 抓帧循环,`capture-frame` 事件推送 JPEG 到前端 `<canvas>` 预览

> WebRTC `<video>` 高帧率路线(H.264/AV1 硬件编码)依赖 RustDesk 官方信令服务器基础设施,为远期蓝图;当前 JPEG-over-TCP 已满足 LAN 场景。

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
│   ├── components/                 # Fluent UI 业务组件
│   │   ├── ControlBar.tsx          # 顶部悬浮工具栏
│   │   ├── RemoteCanvas.tsx        # 远程画面 Canvas(JPEG 渲染,local/remote 双源)
│   │   ├── RemoteSessionView.tsx   # 远程会话窗口(控制中心/性能浮窗)
│   │   ├── VirtualDisplayPanel.tsx # 虚拟屏管理 + 本机显示器预览
│   │   ├── DevicePage.tsx / SettingsPage.tsx / FileTransferPage.tsx / Sidebar.tsx ...
│   ├── services/                   # Tauri IPC 封装层 (Invoke / Listen,isTauri 守卫)
│   │   ├── connection.ts           # 连接/会话/剪贴板/全屏/被控端管理
│   │   ├── capture.ts              # 抓帧 + 真实显示器枚举 + remote-frame 订阅
│   │   ├── virtualDisplay.ts       # IDD 虚拟屏增删/驱动安装
│   │   ├── config.ts               # 应用配置持久化(设备列表/端口)
│   │   └── input.ts                # 鼠标/键盘事件注入
│   ├── App.tsx                     # 应用入口与视图路由
│   └── main.tsx
├── src-tauri/                      # Tauri Rust 后端工程
│   ├── resources/
│   │   └── idd_driver/             # usbmmidd 签名驱动 + deviceinstaller + RustDesk dylib
│   ├── src/
│   │   ├── capture.rs              # 真实 DXGI 抓屏 + JPEG 编码 + 显示器枚举
│   │   ├── network.rs              # 真实 LAN TCP 协议(host/peer 会话)
│   │   ├── hbb_client.rs           # 会话管理/配置持久化/被控端/剪贴板/流参数
│   │   ├── virtual_display.rs      # 真实 IDD 虚拟屏(usbmmidd + dylib)
│   │   ├── input_injector.rs       # Win32 SendInput 鼠标键盘注入
│   │   └── main.rs                 # Tauri 注册与 Commands 绑定
│   ├── Cargo.toml                  # Rust 依赖声明
│   └── tauri.conf.json             # Tauri 配置文件
└── package.json
```

## 7. 开发里程碑路线图

- [x] **阶段一：环境搭建与 POC 验证**
      集成 Tauri v2 + Fluent UI v9 工程骨架,完成「设备 / 远程会话 / 虚拟屏 / 文件传输 / 设置」全套 UI 与 IPC 服务层。RustDesk `hbb_common` 导入因依赖链过重(重 git 依赖 + C 编译)暂缓,网络层改为自研直连 TCP 协议(见 §2.1 注记)。
- [x] **阶段二：虚拟屏驱动集成**
      真实 IDD 驱动接入:`resources/idd_driver/` 内置 usbmmidd 签名驱动 + 官方 `deviceinstaller64` + RustDesk `dylib_virtual_display.dll`;驱动安装 / 注册表分辨率写入 / `enableidd 1|0` 增删虚拟屏 / 真实显示器枚举均已在 Windows 落地。前端「虚拟屏管理」面板支持一键增加 1080P/2K/4K。
- [x] **阶段三：画面抓取与传输**
      真实 DXGI 抓屏(IDXGIOutputDuplication + D3D11)逐帧 JPEG 编码;本地预览经 `capture-frame` 事件推送到 `<canvas>`;远程画面经自研 TCP 协议(`network.rs`)以 base64 JPEG 帧流传输,前端 `remote-frame` 事件 + `createImageBitmap` 渲染。WebRTC `<video>` 高帧率路线留待对接官方 HBBS/HBBR 时启用。
- [x] **阶段四：控制注入与细节优化**
      Windows 真实 SendInput 注入(鼠标绝对坐标 + 键盘 code→VK 全映射,含修饰键/扩展键);远程会话内鼠标/键盘/滚轮事件经协议实时注入被控端;控制中心全屏/画质/分辨率/剪贴板同步真实生效;流参数经 `stream` 消息实时下发被控端。

## 8. 已交付能力清单

- **本机预览**:选择任意显示器,实时 DXGI 抓帧 + JPEG 预览。
- **LAN 远程控制**:一台机器以「被控端」运行(设置页配置端口并启动),另一台通过「设备列表」添加 `ip:port` 后一键进入会话——真实画面流 + 真实鼠标键盘注入 + 剪贴板双向同步 + 画质/分辨率实时调节。
- **虚拟显示器**:管理员权限下安装 usbmmidd IDD 驱动,一键添加 1080P/2K/4K 虚拟屏(最多 4 个),支持枚举/移除。
- **配置持久化**:对端设备列表、被控端口、被控自启开关持久化到 `%APPDATA%/com.example.winui-remote-desktop/config.json`。

> 说明:HBBS/HBBR 信令服务器、NAT 打洞与 WebRTC 视频轨道依赖 RustDesk 官方服务器基础设施与重依赖链,属外部依赖项;当前直连 TCP 已实现完整远程桌面功能,该层替换不影响前端与注入/抓帧模块。

## 9. 信令 / STUN / TURN 服务

基于 RustDesk hbbs/hbbr 思路,本项目自研了三件套服务器(`server/` crate,Windows 可直接运行):

| exe | 角色 | 端口 | 能力 |
| --- | --- | --- | --- |
| `dcr-signal.exe` | 信令 + STUN | TCP 21116 / UDP 21115 | 设备注册/心跳/查找/在线列表(连接断开自动注销);RFC 5389 二进制 STUN Binding(XOR-MAPPED-ADDRESS)+ 不同源端口 NAT 探测;`--relay-hint` 下发中继地址 |
| `dcr-relay.exe` | TURN-like 中继 | TCP 21117 / UDP 21119 | TCP `allocate {id,role}` 配对后双向字节透明转发;UDP `alloc-udp`/`data` 数据报转发 |

**构建**(Windows):
```powershell
cd server
cargo build --release   # 产出 server\target\release\dcr-signal.exe、dcr-relay.exe
```

**部署**(需公网 IP 的 VPS 或可端口转发的主机,云安全组放行 21115-21119):
```bash
# 信令 + STUN(relay-hint 告诉控制端中继地址)
./dcr-signal --bind 0.0.0.0 --port 21116 --udp-port 21115 --relay-hint <中继IP:21117>
# 中继
./dcr-relay --bind 0.0.0.0 --port 21117 --udp-port 21119
```

**客户端接入**:设置页「网络 → 服务器与 ID」配置 信令服务器 / 中继服务器 / 本机 ID 后:

- **被控端**启动时向信令注册本机 ID 与局域网地址并 20s 心跳(配置即生效);
- **控制端**设备列表自动合并信令发现的在线设备;连接回退链为 `配置 LAN 直连 → 信令外部地址 → 中继兜底`,直连/打洞失败时经中继透明转发(上层 framing 原样透传)。

协议细节(长度前缀 JSON 帧):信令 `register/heartbeat/unregister/lookup/list`;中继 `allocate/allocated`。消息类型与 framing 由客户端经 `dcr-server = { path = "../server" }` 共享,与 `network.rs` 保持一致。
