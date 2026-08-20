# 桌面远程控制客户端(Desktop-CR)数据链路完整性审查报告

**审查环境**: `F:\desktop-cr` (Windows / win32)
**审查范围**: 前端 `src/*.tsx` + `src/services/*.ts` ↔ Rust 后端 `src-tauri/src/*.rs` ↔ 独立服务 `server/`
**审查方法**: 从用户 UI 界面出发，逐层下钻至 service → Rust 命令 → 真实系统 API / 网络协议 / 文件持久化。

---

## 核心结论

**项目"真实化收尾"的声明总体属实**——Windows 平台上的抓屏、协议、注入、虚拟屏、持久化、认证均为真实实现；伪造/模拟仅存在于两类受控场景：**(a) 非 Tauri 浏览器 `npm run dev` 的 dev 回退假数据**；**(b) 非 Windows 平台的编译占位**。此外有少量 UI 入口（云市场、键盘设置、常规开关）是**设计性悬空/未实现**，需明确指出。

---

## 一、UI 界面清单与数据来源映射表

### 1.1 登录与账号
| UI 界面 | 数据展示点 | Service 函数 | Rust 命令 | 真实/模拟判定 |
|---|---|---|---|---|
| `LoginPage.tsx` | 账号/密码/服务器(默认 `120.78.77.248:21120`) | `auth.ts: loginAccount / registerAccount` | `auth::login_account` / `auth::register_account` | **真实 A/B**: `auth.rs:32-87` 真实 `POST {server}/api/auth/login\|register`，`server/src/admin.rs:413-414` 真实实现 |
| `LoginPage.tsx` + `App.tsx:97-113` | 启动时令牌校验 | `auth.ts: checkAccountToken` | `auth::check_account_token` | **真实 B**: `auth.rs:124-144` 真实 `GET /api/auth/me` |
| `LoginPage.tsx` | 本地会话读取 | `auth.ts: getAccount` | `auth::get_account` | **真实 C**: 从 `config.json` 读取 (`hbb_client.rs:158-160`) |
| `RemoteAssistPage.tsx:252-259,389` | "验证码"(8 位随机串) | 无(本地 `generateCode()`) | 无 | **伪造 D**: `Math.random()` 生成，UI 自注"本地演示生成" |

### 1.2 主页 / 设备
| UI 界面 | 数据展示点 | Service 函数 | Rust 命令 | 真实/模拟判定 |
|---|---|---|---|---|
| `App.tsx:137-158` | 设备列表、在线数 | `connection.ts: getDevices` | `hbb_client::list_devices` | **Windows 真实 A/B**; 非 Tauri 回退写死 `mockDevices` (`connection.ts:72-78`) |
| `DevicePage.tsx:286-296` | "模拟模式"徽标 | `isTauri()` | — | 仅浏览器模式显示，真实标注 |
| `App.tsx:328-336` | **云设备市场**(在线设备 X/Y) | 无 | 无 | **悬空 UI**: 纯静态文案"后续阶段" |
| `RemoteAssistPage.tsx` | 本设备ID/被控开关/对端连接 | `config.ts` / `connection.ts` | `get_app_config` / `start_host` / `stop_host` | **Windows 真实 A/B/C** |

### 1.3 远程会话(核心)
| UI 界面 | 数据展示点 | Service 函数 | Rust 命令 | 真实/模拟判定 |
|---|---|---|---|---|
| `RemoteCanvas.tsx:189-252` | 远程画面帧 | `capture.ts: onRemoteFrame` | 事件 `remote-frame` (`network.rs`) | **真实 B**: 解 `Msg::Frame` base64 JPEG 推送，`createImageBitmap` 绘制 |
| `RemoteCanvas.tsx:328-334` | "模拟画面"徽标 | `frame.simulated` | `capture-frame` | 按平台：Windows DXGI `false`；非 Windows `true` |
| `RemoteSessionView.tsx:596-617` | 性能浮窗(帧率/码率/延迟/丢包) | `session.ts` + `capture.ts` | `get_session_metrics` + `remote-frame` | **真实 A/B**: fps 真实帧计数；rtt 真实 Ping/Pong；lossPct 由 seq 连续性 |
| `RemoteSessionView.tsx:430-442` | 远程显示器下拉 | `session.ts` | `request_remote_monitors` → `remote-monitors` | **真实 A**: `network.rs` 主机侧 `list_monitors`(真实 `EnumDisplayDevicesW`) |
| `RemoteSessionView.tsx:444-453` | 静音按钮 | `audio.ts` | `get_audio_muted` / `audio-state` | **真实 A**: `audio.rs` 真实标志位 |
| `RemoteSessionView.tsx:714-721` | **"键盘输入设置"** | 无 | 无 | **未实现**: 硬编码 toast `setNotice('键盘输入设置暂未实现')` |
| `RemoteSessionView.tsx:308-326` | Video 模式(`<video>`,WebRTC) | 无 | 无 | **蓝图未实现**: 协议仅 JPEG/H264 字节流，无 `<video>` 渲染路径 |
| `RemoteCanvas.tsx` 输入 | 鼠标/键盘/滚轮 | `input.ts` | `inject_mouse_event` / `inject_key_event` | **Windows 真实 A**: `input_injector.rs` 真实 `SendInput` |
| `ControlBar.tsx` | 画质/分辨率/全屏/剪贴板 | `connection.ts` | `set_stream_quality` 等 | **真实 A/B**: 实时写 `STREAM_CFG` 经 `Msg::Stream` 下发 |

### 1.4 虚拟显示器面板
| UI 界面 | 数据展示点 | Service 函数 | Rust 命令 | 真实/模拟判定 |
|---|---|---|---|---|
| `VirtualDisplayPanel.tsx` | 虚拟屏列表/本机显示器/安装/增删 | `virtualDisplay.ts` / `capture.ts` | `list_virtual_monitors` / `install_virtual_display_driver` 等 | **Windows 真实 A**: 真实 `deviceinstaller64` 安装 IDD + 注册表；非 Windows 返回 `Err`(正确，未造假) |
| `VirtualDisplayPanel.tsx:192-203` | 本机预览抓帧 | `capture.ts` | `start_capture` / `stop_capture` | **Windows 真实 A**(DXGI)；非 Windows 模拟帧 |

### 1.5 文件传输
| UI 界面 | 数据展示点 | Service 函数 | Rust 命令 | 真实/模拟判定 |
|---|---|---|---|---|
| `FileTransferPage.tsx` 左栏 | 本机目录/文件 | `fileTransfer.ts: listDirectory` | `hbb_client::list_directory` | **真实 A/C**: 真实 `std::fs` 枚举 |
| `FileTransferPage.tsx` 右栏 | 远端目录/文件 | `fileTransfer.ts` | `request_remote_dir` → `remote-directory` | **真实 B**: `network.rs` 主机侧 `list_dir` 应答 |
| `FileTransferPage.tsx` 传输列表 | 进度/速度 | `fileTransfer.ts` | `send_file` / `file-progress` | **真实 B/C**: 真实 64KB 分块 + base64 经 TCP 落盘 |

### 1.6 设置页
| UI 界面 | 数据展示点 | Service 函数 | Rust 命令 | 真实/模拟判定 |
|---|---|---|---|---|
| `SettingsPage.tsx:800-905` **常规** | 开机自启/远程开机/休眠/自动更新/拖拽浮窗 | 无 | 无 | **未实现**: 全部 `ToggleSwitch disabled` + "(暂未实现)" |
| `SettingsPage.tsx:950-987` **键盘** | macOS 按键映射表 | 无 | 无 | **静态展示**: 硬编码映射，无后端接线 |
| `SettingsPage.tsx:989-1213` **网络** | 被控端口/信令/中继/设备ID/对端 | `connection.ts` + `config.ts` | `start_host` / `get_app_config` / `save_app_config` | **真实 A/B/C** |
| `SettingsPage.tsx:1216-1242` **账号** | 当前登录账号 | `config.ts` | `get_app_config` | **真实 C** |
| `SettingsPage.tsx:1244-1269` **日志** | 操作日志列表 | `logs.ts` | `operation_log::get_operation_logs` | **真实 C**: 真实 UTC 日轮转文件读取 |
| `SettingsPage.tsx:1271-1554` **诊断** | 媒体全链路/实时基准/音频回环 | `media.ts` / `bench.ts` | `run_media_pipeline_test` 等 | **真实 A/B**: 真实 DXGI / WASAPI |

---

## 二、真实数据来源清单（按真实度分级）

### A 级 — 真实系统 API(Windows)
- **DXGI 桌面复制抓屏**: `capture.rs:352-604` (`IDXGIOutputDuplication` + `AcquireNextFrame` + JPEG)
- **Win32 输入注入**: `input_injector.rs` (`SendInput`，非 Windows 仅日志)
- **Win32 剪贴板读写**: `hbb_client.rs:1033-1124`
- **WASAPI 系统回环音频采集**: `audio.rs:85-154`
- **IDD 虚拟显示器驱动**: `virtual_display.rs:38-216` 真实 `deviceinstaller64 install/enableidd` + 注册表
- **显示器枚举**: `capture.rs:290-348` `EnumDisplayDevicesW`

### B 级 — 真实网络协议
- **自研 LAN 远程控制 TCP 协议**: `network.rs:43-159` 完整 `Msg` 协议（Hello/Frame/Mouse/Key/Clipboard/Stream/Monitors/File*/DirList/Ping-Pong/Audio）
- **信令 + STUN 服务**: `server/src/signal.rs` (TCP 21116 + UDP 21115 RFC5389 STUN)
- **TURN-like 中继**: `server/src/relay.rs` (TCP 21117 `allocate` 配对透明转发)
- **HTTP 账号认证**: `auth.rs:32-232` 真实 `reqwest` → `server/src/admin.rs`

### C 级 — 真实文件持久化
- **应用配置**: `hbb_client.rs:281-311` `config.json`
- **操作日志**: `operation_log.rs:96-179` `operations-YYYYMMDD.log`
- **文件传输落盘**: `hbb_client.rs:754-848` 真实读写 `incoming/`

### D 级 — 模拟/伪造（全部受控、可识别）
- **非 Tauri 浏览器回退**: `connection.ts:46-51` `mockDevices`；`fileTransfer.ts:30-36` `mockDir`；`auth.ts:19,32` `token:'mock-token'`；`config.ts:39-48` `DEFAULT_CONFIG`；`virtualDisplay.ts` 安装返回"浏览器模式:跳过真实驱动安装"
- **非 Windows 编译占位**: `capture.rs:606-692` 程序化动画帧(`simulated:true`)；`hbb_client.rs:397-402,994-1005` `list_devices` 空、`start_host` **假报 running=true**；`input_injector.rs:166-173` 仅日志；`virtual_display.rs` 返回 `Err`(此处正确)
- **UI 硬编码假元素**: `RemoteAssistPage.tsx:252-259` 随机验证码；`RemoteSessionView.tsx:714-721` 键盘设置 toast；`SettingsPage.tsx:808-863` 全 disabled 开关；`App.tsx:328-336` 云市场静态文案

---

## 三、模拟/伪造数据详细清单（文件:行号 + 触发条件）

| 编号 | 位置 | 内容 | 触发条件 |
|---|---|---|---|
| D1 | `src/services/connection.ts:46-49,72-78` | `mockDevices` (`Desktop-Office (Mock)` 等) | 纯浏览器 `npm run dev` |
| D2 | `src/services/connection.ts:51,81-86` | `mockState = {connected:false, peerId:'mock-01'}` | 非 Tauri |
| D3 | `src/services/fileTransfer.ts:30-43` | `mockDir` (Documents/Downloads/报告.docx...) | 非 Tauri |
| D4 | `src/services/auth.ts:17-21,38-44` | `token:'mock-token'`，跳过校验直接放行 | 非 Tauri |
| D5 | `src/services/config.ts:39-48` | `DEFAULT_CONFIG` 写死 signalServer/relayServer/hostId | 非 Tauri |
| D6 | `src/services/virtualDisplay.ts:15-29` | 安装返回"浏览器模式:跳过真实驱动安装"；新增屏 `Date.now()%1000+1` | 非 Tauri |
| D7 | `src-tauri/src/capture.rs:606-692` | `generate_mock_frame()` 程序化 RGBA 动画，`simulated:true` | 非 Windows |
| D8 | `src-tauri/src/hbb_client.rs:994-1010` | 非 Windows `start_host` 直接 `emit host-state running=true` 返回 `Ok` | **非 Windows，后端侧假成功** ⚠️ **（已修正：改为 `Err("被控端(host)仅 Windows 平台支持...")` 并广播真实的 `running:false`，与虚拟屏一致）** |
| D9 | `src-tauri/src/bench.rs:173-244` | `synthetic=true` 或桌面无帧时回退 `synth_frame()`，报告 `synthetic:true` | 基准无真实帧时回退 |
| D10 | `RemoteAssistPage.tsx:252-259` / `RemoteSessionView.tsx:714-721` / `SettingsPage.tsx:808-863,950-987` / `App.tsx:328-336` | 随机验证码 / 键盘设置 toast / 常规全 disabled / 云市场静态文案 | UI 桩 |

---

## 四、链路完整性缺口

**G1 命令/事件注册完全对齐（无断裂）**：`main.rs:32-90` 注册的 40+ 个 `#[tauri::command]` 与前端 `invoke('xxx')` 调用逐一对应，无孤儿命令、无悬空事件。前端监听的 10 个事件在后端均有 `emit`，**链路闭合**。

**G2 设计性悬空 UI（非 bug，需明示）**
- 云设备市场 (`App.tsx:328-336`)：无 service 接线，纯占位
- 键盘输入设置 (`RemoteSessionView.tsx:714-721`)：点击仅弹"暂未实现"
- 常规 tab 全部开关 (`SettingsPage.tsx:808-863`)：disabled
- WebRTC Video 模式 (`RemoteCanvas.tsx:308-326`)：`<video>` 无 `src` 接入，协议层无渲染路径

**G3 真实功能但首屏"看似空"**：`hbb_client::list_devices` 无 LAN 自动发现 (mDNS/SSDP)，设备仅来自 `config.peers` 或信令服务器。首次运行无 peers/无信号服务器时主页为空——**真实限制而非伪造**，但 UI 未给添加引导。

**G4 远程画面依赖被控端已 `start_host`**：必须两端都真实运行 Windows 被控端，控制端才能收到 `remote-frame`。单端/纯前端无法验证远程画面（正常，但"端到端"需双机）。

**G5 非 Windows `start_host` 假成功（D8）—— 已修正**：原 `hbb_client.rs:994-1005` 在非 Windows 直接 `emit host-state running=true` 返回 `Ok`，制造"被控端运行中"假象（**唯一发现的后端侧造假成功信号**，与虚拟屏"正确返回 Err"处理不一致）。现已改为返回 `Err("被控端(host)仅 Windows 平台支持,当前平台不可用")`，并广播真实的 `host-state running:false`（host 实际未启动），与虚拟屏 `Err` 处理保持一致。

---

## 五、结论与风险等级

整体判定：项目真实化收尾基本属实，真实链路占比高。README §2.1 蓝图声明（hbb_common/scrap/WebRTC 为远期蓝图、网络层改自研 TCP）**经核对属实**：`src-tauri/Cargo.toml` 中 `hbb_common`/`scrap` 仅为注释掉的待启用项，全仓无任何真实依赖。

| 风险 | 界面/功能 | 等级 | 说明 |
|---|---|---|---|
| 看起来能用其实在演戏 | 非 Windows `start_host` | ✅ 已修正 | 原 `hbb_client.rs:994-1005` 假报"运行中"，现已改为 `Err("仅 Windows 平台支持")` 并广播真实 `running:false` |
| 看起来有用其实在演戏 | 浏览器 `npm run dev` 整体 | 🟠 中 | 全部 mock 数据，但 UI 已用"模拟模式"徽标标注 |
| 看起来有用其实在演戏 | 远程助手"验证码" | 🟢 低 | 自注"本地演示生成"，连接本身真实 |
| 设计性悬空 | 云市场/键盘设置/常规开关 | 🟠 中 | 无后端接线，需产品层面明示"未上线" |
| 真实但受限 | 设备列表首屏为空 | 🟢 低 | 无 LAN 自动发现，需手动加 peer |
| 端到端需双机 | 远程画面/控制/文件 | 🟢 低 | 必须真实 Windows 被控端在跑 |

**最有价值的两处"看起来能用其实在演戏"**：
1. **非 Windows 下 `start_host` (`hbb_client.rs:994-1010`) —— 已修正** —— 原后端直接伪造"running=true"（审查中唯一发现的**后端侧造假成功**信号），现已改为 `Err("仅 Windows 平台支持")` 并广播真实 `running:false`，与虚拟屏一致。
2. **纯浏览器 `npm run dev` 模式** —— 所有展示数据均为 mock，但已通过 `isTauri()` 守卫 + "模拟模式"徽标充分标注，生产 (tauri build) 路径不受影响。

**一句话总结**：在 Windows + Tauri 生产构建下，从登录、设备、远程画面、输入注入、虚拟屏、文件传输、设置持久化到诊断基准，**链路真实、数据真实、来源真实**；模拟与伪造被严格限制在"浏览器 dev 回退"和"非 Windows 占位"两类受控场景，并均有明确代码/UI 标注；仅剩少量 UI 入口（云市场、键盘设置、常规开关、WebRTC Video）属设计性未实现，以及一处非 Windows `start_host` 的假成功需修正。
