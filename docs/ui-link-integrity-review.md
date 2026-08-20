# 前端 UI → 数据链路完整性 / 真实性审查报告

> 审查范围：从 Web 管理端 UI 出发，逐层追踪「界面 → service → Tauri invoke/事件 → Rust 后端命令」链路，
> 判定数据是否模拟/伪造、数据来源是否真实。
> 审查日期：2026-08-20 ｜ 项目：desktop-cr（Tauri v2 + React 18 远程桌面客户端）

---

## 一、总体结论（先看这里）

| 运行路径 | 数据真实性 | 说明 |
|---|---|---|
| **Windows + Tauri 生产构建** | ✅ 真实 | 所有 invoke 命令在 `src-tauri/src/main.rs:32-90` 真实注册；Rust 端 Windows 分支为真实实现。界面数据均来自真实来源。 |
| **浏览器 `npm run dev` 预览** | ⚠️ 整套伪造 | `isTauri()` 为 false，走降级分支，返回一套写死的 mock 数据（设备、登录令牌、文件目录等）。易被误判为「真实联调」。 |
| **非 Windows 编译** | 🔴 部分伪造 | 抓屏推送 100% 动画伪造帧；输入注入返回「伪造成功」。其余非 Win 分支诚实返回空/报错。 |

**一句话结论**：该项目在「Windows + Tauri」生产路径上是**真实数据来源**，未发现把假数据硬编码进生产界面的逻辑；主要风险是 (a) 浏览器预览模式展示完整假数据，易被误判为真实联调结果；(b) 若计划支持非 Windows 平台，抓屏与输入注入会呈现伪造内容。

---

## 二、链路完整性（UI → 数据来源 映射）

UI 树：`App.tsx` 鉴权后按 `view` 切换 → Sidebar(设备树) / DevicePage / RemoteSessionView+RemoteCanvas / VirtualDisplayPanel / FileTransferPage / SettingsPage(7 Tab) / RemoteAssistPage / 云设备(占位)。

所有重点事件名均命中且拼写正确：`connection-state`、`remote-frame`、`capture-frame`、`host-state`、`clipboard-synced`、`virtual-monitors-changed`。

| 界面 | 数据源 service | invoke 命令 | 链路状态 |
|---|---|---|---|
| 设备列表 (Sidebar/DevicePage) | connection.getDevices | `list_devices` | ✅ 完整 |
| 登录/账号 | auth.* | `login_account`/`register_account`/`get_account`/`check_account_token`/`logout_account` | ✅ 完整(后端真实 HTTP) |
| 远程会话(显示器/性能指标/剪贴板) | session/capture/connection/audio | `request_remote_monitors`/`get_session_metrics`/`get_audio_muted` | ✅ 完整 |
| 远程画面 | capture.onFrame/onRemoteFrame | `capture-frame`/`remote-frame` | ✅ 完整(视频模式仅占位文案) |
| 虚拟显示器 | virtualDisplay/capture | `list_monitors`/`list_virtual_monitors`/`add_virtual_monitor` 等 | ✅ 完整 |
| 文件传输 | fileTransfer | `list_directory`/`send_file`/`request_remote_dir` 等 | ✅ 完整 |
| 设置/配置/日志 | config/logs/connection/media/bench | `get_app_config`/`get_operation_logs`/`start_host`/`run_media_pipeline_test` 等 | ✅ 完整 |
| 远程协助 | config/connection | `get_app_config`/`connect_to_device` | ✅ 完整 |

**链路断开 / 占位（无真实后端支撑）的 UI 部分**：
- 🚩 **通知中心** `TitleBar.tsx:188,254-261`：红点 `unread=2`、通知弹层写死「AAAAA 设备已上线」等，纯静态无数据源。
- 🚩 **顶部栏按钮** `TitleBar.tsx:228,266,269`：独立窗口/用户/菜单仅 `onShowToast('…开发中')` 占位。
- 🚩 **云设备市场** `App.tsx:328-336`：纯占位文案「后续阶段」，无数据接入。
- 🚩 **远程协助验证码** `RemoteAssistPage.tsx:251-259`：本地随机 8 位，注释「仅本地演示生成，无后端校验」。
- 🚩 **设置页常规/安全/键盘 Tab** `SettingsPage.tsx:808-862,919,958,968,974-983`：大量开关 `disabled` + 「暂未实现」、按键映射表硬编码静态、键盘 Tab 按钮无 onClick。
- 🚩 **远程会话键盘输入设置** `RemoteSessionView.tsx:713-721`：`centerItemDisabled` + 「暂未实现」。
- 🚩 **VirtualDisplayPanel 设置按钮** `VirtualDisplayPanel.tsx:305`：「虚拟显示器设置暂未提供」。
- 🚩 **画质/分辨率/预设分辨率** 为硬编码常量（`RemoteSessionView.tsx:352-363`、`VirtualDisplayPanel.tsx:94-98`），属配置项非业务数据。

---

## 三、模拟 / 伪造数据风险清单（按严重程度）

### 🔴 HIGH — 非 Windows 编译路径的伪造

1. **抓屏伪造动画帧** `src-tauri/src/capture.rs:607-708`
   非 Windows 下 `generate_mock_frame`/`mock_capture_loop` 程序化生成 RGBA 动画（移动亮带+光标+棋盘格），经 `capture-frame` 推送并打标 `simulated: true`。**控制端看到的「远程桌面」100% 是伪造画面**，仅非 Windows 分支生效；Windows 走真实 DXGI（`simulated:false`）。

2. **输入注入返回伪造成功** `src-tauri/src/input_injector.rs:166-175(鼠标) / 258-267(键盘)`
   非 Windows 分支不执行任何系统输入，却 `return Ok(InputEventReceipt{ ok:true, message:"…(simulated)" })`。**唯一一处「伪造成功」桩**。被控端因 `start_host` 非 Win 报错不会实际运行，但前端若直接 invoke 会拿到假成功。

### 🟠 MEDIUM — 前端 `!isTauri()` 浏览器降级假数据（生产 Tauri 不触发，但浏览器预览会展示）

3. **设备列表/连接状态写死** `src/services/connection.ts:46-49,51,57,75,83`：`mockDevices`(Desktop-Office (Mock)/NAS-Server (Mock))、`mockState`、`connectToDevice` 非 Tauri 返回 `{connected:true}`。
4. **登录/注册假令牌** `src/services/auth.ts:19,32`：非 Tauri 返回 `{token:'mock-token'}`；`checkAccountToken` 跳过校验。
5. **文件传输假目录** `src/services/fileTransfer.ts:30-36,42,50,59`：`mockDir`(Documents/Downloads/报告.docx/照片.png)、`getIncomingDir` 返回 `'C:\incoming(mock)'`、`sendFile` 返回随机伪 id。
6. **虚拟显示器伪 id** `src/services/virtualDisplay.ts:27`：`addVirtualMonitor` 非 Tauri 返回 `(Date.now()%1000)+1`。
7. **写死默认服务器地址** `src/services/config.ts:39-48`：`DEFAULT_CONFIG` 含 `120.78.77.248:21116/21117`（真实存在的开发者 VPS，属默认配置非伪造业务数据；但默认指向公网是部署/隐私关注点）。

### 🟢 LOW / 无害

8. **基准合成帧（透明标注）** `src-tauri/src/bench.rs` `synthetic` 模式：显式开关 + 报告 `synthetic_used` 标记 + UI `SettingsPage.tsx:1471` 警告，非伪造。
9. **良性非 Win 占位桩**：`virtual_display.rs`/`audio.rs`/`hbb_client.rs`/`media_pipeline.rs`/`network.rs` 等返回 `Err`/空列表/仅日志，明确避免伪造「运行中」假象，**不构成伪造**。
10. **未实现功能 UI 已透明提示**「暂未实现」（见第二节）。
11. **测试专用合成数据** `media_pipeline`/`bench` 的 `synth_frame` 仅 `#[cfg(test)]`，不进入生产命令路径。

---

## 四、数据来源真实性判定（逐后端模块）

| Rust 模块 | Windows 真实性 | 非 Win |
|---|---|---|
| capture（抓屏/枚举） | ✅ 真实 DXGI + EnumDisplayDevices | 动画桩(`simulated:true`) / 空 |
| network（TCP 协议/文件传输/信令/中继） | ✅ 真实 TCP + 4字节长度前缀 JSON 帧 | 真实协议(输入仅 log) |
| hbb_client（配置/被控端/剪贴板/在线探测） | ✅ 真实落盘 + 真实 TCP 探测 + Win32 剪贴板 | 空/`Err`(诚实) |
| virtual_display（IDD 驱动） | ✅ 真实注册表 + deviceinstaller64 | `Err`(诚实) |
| input_injector（SendInput） | ✅ 真实 SendInput | ⚠️ 伪造成功 |
| operation_log | ✅ 真实按 UTC 日期落盘 | 同左 |
| media_pipeline / bench / audio | ✅ Win 真实链路(真实+可选合成) | `Err`(诚实) |
| ffmpeg_hw（仅 Win 编译） | ✅ 真实 FFmpeg FFI + D3D11VA 硬件解码 | 不编译 |
| auth（账号登录） | ✅ 真实 HTTP + JWT | 同左 |

---

## 五、建议整改优先级

1. **P0（真实伪造隐患）**：`input_injector.rs` 非 Win 两命令改为返回 `Err`，与 `start_host` 非 Win 行为一致，消除「伪造成功」。
2. **P1（避免误判）**：浏览器 `!isTauri()` 降级分支应在 UI 上**显式标注「Demo/模拟模式」**，而非伪装成真实连接（当前仅 DevicePage/Sidebar 有「模拟模式」徽标，设备列表等其余界面无提示）。
3. **P1（非 Win 抓屏）**：若未来支持非 Windows，UI 对 `simulated:true` 帧需做醒目「模拟画面」标识（当前靠字段区分，前端未强制提示）。
4. **P2（部署/隐私）**：默认 `signal_server/relay_server` 指向公网 VPS `120.78.77.248`；纯局域网自托管场景应允许配置覆盖并默认提示。
5. **P2（占位 UI）**：通知中心、云设备市场、设置页多个「暂未实现」开关、远程协助验证码——明确标注为规划中功能，避免用户误以为可用。

---

*本报告由 4 个并行探查 Agent 审计得出（前端 UI 映射 / 服务层降级 / Rust 后端实现 / 全局模拟检测），未修改任何源文件。所有判定均附文件:行号证据。*
