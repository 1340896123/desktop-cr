# AGENTS.md

WinUI-style remote desktop client: **Tauri v2 + React 18 + Fluent UI React v9**. 项目已完成真实化收尾:Windows 上为真实实现(真实 DXGI 抓屏、真实 LAN TCP 远程控制协议、真实 IDD 虚拟显示器、真实剪贴板/注入/全屏、真实配置持久化);非 Windows 平台保留 `#[cfg(not(target_os = "windows"))]` 编译占位。注释与 UI 文案均为简体中文,保持此约定。

## Commands

- `npm run dev` — Vite dev server, **port 1420 strict** (`vite.config.ts` sets `root: 'src'`, output to `../dist`, ignores `src-tauri/**` in watch)
- `npm run build` — `tsc && vite build`; this is the only typecheck/lint gate. **No eslint, no test framework, no CI.** tsconfig is strict with `noUnusedLocals`/`noUnusedParameters` — unused code fails the build
- `npm run tauri dev` / `npm run tauri build` — run/build the desktop app
- `cargo check` / `cargo build` in `src-tauri/` — Rust side. Windows 专属依赖只在 `[target.'cfg(windows)'.dependencies]`(windows crate + 相关 features)
- `cargo test` in `src-tauri/` — Rust 单元测试(Rust 无独立测试框架,用 `#[cfg(test)]` 内嵌于各模块;13 条:capture 缩放/BGRA、network 协议标签与 TCP framing 往返、hbb_client 配置与流参数、input_injector code→VK 映射、virtual_display 注册表值、operation_log 读写、media_pipeline 音视频合成链路)
- `npm test` — vitest(vitest run),前端纯函数测试(src/utils/coords.ts、src/services/config.ts)
- 验收标准 = `cargo check` 零警告 + `cargo test` 全过 + `npm run build` 零错误 + `npm test` 全过

## Architecture

- `src/services/*.ts` wrap Tauri `invoke`/`listen`. Every function guards on `isTauri()` (`'__TAURI_INTERNALS__' in window`) and falls back to dev data/console warnings — **so `npm run dev` in a plain browser works without Tauri**. Don't remove the guard when touching services.
- Rust commands (`src-tauri/src/`, snake_case, 注册于 `main.rs`):
  - `capture.rs` — **真实 DXGI 抓屏**(IDXGIOutputDuplication + D3D11,JPEG 编码,jpeg-encoder crate)+ 真实显示器枚举 `list_monitors`(EnumDisplayDevices)。非 Windows 为动画帧模拟。
  - `network.rs` — **真实 LAN 远程控制协议**:TCP + 4 字节长度前缀 JSON 帧,消息以 `t` 字段区分(`hello/hello-ack/frame/mouse/key/clipboard/ping/pong/stream`)。被控端 `run_host`/`serve_host`(单连接策略),控制端 `connect_peer`。帧为 base64 JPEG,输入/剪贴板/流参数实时双向。
  - `hbb_client.rs` — 真实会话管理、配置持久化(`app_config_dir/config.json`,命令 `get_app_config`/`save_app_config`)、TCP 在线探测、被控端管理(`start_host`/`stop_host`)、剪贴板读写(Win32)、全屏(真实 Tauri 窗口)。
  - `virtual_display.rs` — **真实 IDD 虚拟显示器**:usbmmidd 签名驱动(资源目录 `resources/idd_driver/` 已含 inf/cat/dll/deviceinstaller64),注册表写入分辨率列表 + `deviceinstaller64 enableidd 1/0` 增删,`libloading` 可加载 RustDesk `dylib_virtual_display.dll`(失败静默回退 usbmmidd)。
  - `input_injector.rs` — 真实 SendInput 鼠标/键盘注入(code→VK 全映射表)。
  - `operation_log.rs` — 操作日志持久化(`app_config_dir/logs/operations-YYYYMMDD.log`,按 UTC 日期轮转),命令 `get_operation_logs`,各模块关键操作经 `op_log` 写入。
  - `media_pipeline.rs` — 音视频全链路测试管道:采集(DXGI 单帧 / cpal WASAPI)→ 编码(JPEG / WAV)→ 传输(network.rs 真实 TCP framing loopback)→ 解码(image / hound)→ 存本地文件;命令 `run_media_pipeline_test`;`#[cfg(test)]` 用合成数据覆盖合成链路。
- 流参数闭环:`set_stream_quality`/`set_stream_resolution` 写入本机 `STREAM_CFG` 并经会话下发 `stream` 消息,被控端 `apply_stream_cfg` 实时生效。
- 前端事件: `connection-state` / `remote-frame`(JPEG) / `capture-frame`(JPEG 本机预览) / `host-state` / `clipboard-synced` / `virtual-monitors-changed`。
- 设计令牌:`src/theme/tokens.ts`(自定义调色板/间距/圆角,非 Fluent tokens);组件用 `@fluentui/react-components` 的 `makeStyles`。设计规范见 `docs/design-system.md`。
- README §2.1 中 hbb_common/scrap/WebRTC 仍为**远期蓝图**(依赖 RustDesk 官方信令服务器与重依赖链),当前网络层为自研直连 TCP 协议,已在 `network.rs` 中隔离,未来可替换。

## Gotchas

- `src-tauri/resources/idd_driver/` 已含真实驱动文件(usbmmidd 签名驱动 + 官方 deviceinstaller + RustDesk dylib),`tauri.conf.json` `bundle.resources` 打包。安装驱动/添加虚拟屏需要管理员权限。
- `tauri.conf.json` CSP 为 `connect-src ipc: http://ipc.localhost` — 网络调用全部在 Rust 侧(TCP),前端无直连,无需改 CSP;若未来前端直连 HBBS/HBBR 需更新。
- 被控端权限:IDD 驱动安装/`enableidd` 需 UAC 管理员;host 端口监听无特殊要求。
- 前端 dev server `allowedHosts` 包含 `.monkeycode-ai.online`(monkeycode 预览域)。
- Windows 安装包为 WiX,`zh-CN` 语言;版本 0.1.0,中文 commit 信息。