# AGENTS.md

WinUI-style remote desktop client: **Tauri v2 + React 18 + Fluent UI React v9**. 项目已完成真实化收尾:Windows 上为真实实现(真实 DXGI 抓屏、真实 LAN TCP 远程控制协议、真实 IDD 虚拟显示器、真实剪贴板/注入/全屏、真实配置持久化);非 Windows 平台保留 `#[cfg(not(target_os = "windows"))]` 编译占位。注释与 UI 文案均为简体中文,保持此约定。

## Commands

- `npm run dev` — Vite dev server, **port 1420 strict** (`vite.config.ts` sets `root: 'src'`, output to `../dist`, ignores `src-tauri/**` in watch)
- `npm run build` — `tsc && vite build`; this is the only typecheck/lint gate. **No eslint, no test framework, no CI.** tsconfig is strict with `noUnusedLocals`/`noUnusedParameters` — unused code fails the build
- `npm run tauri dev` / `npm run tauri build` — run/build the desktop app
- `cargo check` / `cargo build` in `src-tauri/` — Rust side. Windows 专属依赖只在 `[target.'cfg(windows)'.dependencies]`(windows crate + 相关 features)
- `cargo test` in `src-tauri/` — Rust 单元测试(Rust 无独立测试框架,用 `#[cfg(test)]` 内嵌于各模块;42 条 + 9 ignored:capture 缩放/BGRA + DXGI 吞吐基准(ignored)、network 协议标签与 framing 往返 + 文件传输状态机/全双工/速率基准 + 信令长连接保活回归 + STUN 探测回环 + UDP 通道直连/中继回环 + UDP 半开看门狗回环 + 回退切换丢包基线重置、transport 分片重组/超时丢帧/重复分片/**关键帧门控** + 500 帧回环、hbb_client 配置与流参数(码率档位/旧配置迁移)+ 账号服务器解析、diagnostics 参数收敛/传输模式/哨兵/F2 评估 + TCP/UDP 真实链路自检(ignored)、input_injector code→VK 映射、virtual_display 注册表值、operation_log 读写、ffmpeg_hw 编码往返/基准/能力报告/画质档位码率差异(ignored))
- `cargo test -- --ignored --nocapture` in `src-tauri/` — 真实硬件链路基准(需活动桌面):DXGI 采集基准、编解码往返、编码基准、能力报告、**诊断 TCP/UDP 端到端回环**、画质档位码率差异(F-2 验证)。**默认并行即可**:所有创建 DXGI DuplicateOutput 的测试(采集基准 + 诊断 TCP/UDP 链路)共用 `capture::tests::TEST_DXGI_MUTEX` 进程级静态互斥锁串行化临界区(F-3 修复,无需 `--test-threads=1`)
- `server/` — 独立 crate(信令/STUN/TURN 服务):`cargo build --release` 产出 `server/target/release/dcr-signal.exe`、`dcr-relay.exe`;`cargo test` 60 条(42 单元 + 18 集成 loopback)
- `server/admin-ui/` — 管理后台 React UI:根 `package.json` 无该依赖,需在 `server/admin-ui` 下独立 `npm install` + `npm run build`,产出 `admin-ui/dist`(vite `base: './'`、port 5174、`/api` 代理 21120)
- `npm test` — vitest(vitest run),前端纯函数测试(src/utils/coords.ts、src/services/config.ts、src/utils/lossStats.ts 回退切换丢包基线重置)
- 验收标准 = `cargo check` 零警告(两处)+ `cargo test` 全过(两处)+ `npm run build` 零错误 + `npm test` 全过

## Deploy(部署约定)

**所有服务端交付物统一产出到 `server/deploy/`**(含 UI 与可执行文件),任何改代码后重新部署必须刷新该目录,禁止绕过。

- 构成:`dcr-signal.exe` / `dcr-relay.exe`(来自 `server/target/release/`)、`web/`(管理后台 UI,来自 `server/admin-ui/dist`)、运行时 DLL(`msvcp140.dll`/`vcruntime140.dll`/`vcruntime140_1.dll`,VC++ 运行库)、启动脚本 `start-signal.bat`(指向 `--admin-ui web`)/`start-relay.bat`、`dcr-server-win64.zip`(以上全部打包)。
- 刷新流程:先 `cargo build --release`(server)+ `npm run build`(admin-ui),再将新 exe 与 `admin-ui/dist` 内容复制进 deploy(覆盖 exe、清空重写 `web/`),最后重新压缩 `dcr-server-win64.zip`。
- 注意:打包/复制前需先停止正在运行的 `dcr-signal.exe`/`dcr-relay.exe`(Windows 文件锁定会报 os error 5 拒绝访问)。
- 客户端安装包(`npm run tauri build`)不放入 deploy,产出在 `src-tauri/target/release/bundle/`。

## Architecture

- `src/services/*.ts` wrap Tauri `invoke`/`listen`. Every function guards on `isTauri()` (`'__TAURI_INTERNALS__' in window`) and falls back to dev data/console warnings — **so `npm run dev` in a plain browser works without Tauri**. Don't remove the guard when touching services.
- Rust commands (`src-tauri/src/`, snake_case, 注册于 `main.rs`):
  - `capture.rs` — **真实 DXGI 抓屏**(IDXGIOutputDuplication + D3D11)+ 真实显示器枚举 `list_monitors`(EnumDisplayDevices)。持久抓帧循环 BGRA 直入 `HwEncoder::encode_frame` 编码 H.264/H.265(Annex-B 存 `LATEST_VIDEO`;编码器以 `(w,h,tw,th,fps,bitrate_kbps,codec)` 为重建 key,**画质档位码率 STREAM_CFG.bitrate_kbps 真实进编码器**——F-2 闭环;`request_video_keyframe` 支持丢帧反馈强制 IDR——F-1a);本机 `capture-frame` 预览复用编码帧,FFmpeg 不可用时回退小尺寸 BGRA 原始字节(前端 putImageData 直绘);`grab_frame_once` 一次性真实抓屏(独立创建桌面复制取一帧,供诊断)。非 Windows 显式报错"仅 Windows 支持"。
  - `network.rs` — **真实远程控制协议(控制面 TCP + 视频面 UDP)**:TCP + 4 字节长度前缀 JSON 帧,消息以 `t` 字段区分(`hello/hello-ack/frame/mouse/key/clipboard/ping/pong/stream/udp-init/udp-init-ack/keyframe-request/udp-dead`),`PROTOCOL_VERSION = 4`。被控端 `run_host`/`serve_host`(单连接策略),控制端 `connect_peer`。TCP 模式视频帧为 base64 H.264/H.265 Annex-B(`Msg::Frame` 字段 `data`);UDP 模式经 `transport::split_packet` 分片走 `UdpChannel`(直连打洞失败回退中继 21119,再失败回退 TCP,会话不中断)。**UDP 丢帧恢复(F-1a)**:接收端重组器关键帧门控(丢帧后 delta 不透传),丢帧时经 TCP 控制面回发 `keyframe-request`,被控端下一帧强制编出 IDR(forced_idr,不等 GOP);**UDP 半开检测(F-1b)**:host 周期发 udp-keepalive 数据报,控制端看门狗(约 60 推帧周期无分片/保活)判定通道死亡 → 回退 TCP 拉流 + `udp-dead` 通知被控端 + `transport` 更新 "tcp" + op_log `udp_fallback`,会话不中断。控制端收到帧后**原样透传**前端 WebCodecs 解码(`RemoteFrameEvent { width, height, data, seq, dur, key, codec }`;UDP 模式 `dur = null`,前端显示 "--")。STUN:客户端 `stun_probe` 向信令 UDP 21115 发 Binding 取反射地址,host 注册 external 用反射地址。信令/中继对接:`connect_peer` 连接回退链 **直连(配置 LAN)→ 信令外部地址 → 中继兜底**(`open_transport`),host 启动时经 `signal_register_loop` 向信令注册+心跳;协议消息类型与 framing 复用 `server/` 的 `dcr_server` 库(路径依赖)。`SessionMetrics` 含 `transport`("tcp"/"udp"/"relay-udp")。
  - `transport.rs` — **UDP 数据面原语**:自定义二进制分片帧协议(16 字节小端头 magic "URPD" + frame_id/frag_idx/frag_cnt/flags/codec,单片 ≤1200B,200ms 重组超时)+ `FragmentReassembler`(乱序放位/超时丢帧/**关键帧门控**:丢帧后 delta 帧不透传直到下一关键帧,计数 `gated_frames`/统计)+ `UdpChannel`(直连/中继两模式,接收侧共用重组);UDP 保活常量(`UDP_KEEPALIVE_INTERVAL_MS`/`UDP_KEEPALIVE_TEXT`,F-1b 半开检测载体)。
  - `hbb_client.rs` — 真实会话管理、配置持久化(`app_config_dir/config.json`,命令 `get_app_config`/`save_app_config`)、TCP 在线探测、被控端管理(`start_host`/`stop_host`)、剪贴板读写(Win32)、全屏(真实 Tauri 窗口)。
  - `virtual_display.rs` — **真实 IDD 虚拟显示器**:usbmmidd 签名驱动(资源目录 `resources/idd_driver/` 已含 inf/cat/dll/deviceinstaller64),注册表写入分辨率列表 + `deviceinstaller64 enableidd 1/0` 增删,`libloading` 可加载 RustDesk `dylib_virtual_display.dll`(失败静默回退 usbmmidd)。
  - `input_injector.rs` — 真实 SendInput 鼠标/键盘注入(code→VK 全映射表)。
  - `operation_log.rs` — 操作日志持久化(`app_config_dir/logs/operations-YYYYMMDD.log`,按 UTC 日期轮转),命令 `get_operation_logs`,各模块关键操作经 `op_log` 写入。
  - `diagnostics.rs` — **DXGI 回传自检**(仅 Windows,设置页「诊断」tab,命令 `run_dxgi_loopback`,入参含 `transport: "tcp"|"udp"`):真实 DXGI 抓屏(`capture::grab_frame_once`)→ H.264 编码(硬编优先)→ TCP 生产协议帧回环(`Msg::Frame`)或 **UDP 生产数据面回环**(`split_packet` 分片 → `UdpChannel` → `FragmentReassembler` 重组)→ FFmpeg 解码(D3D11VA 硬解优先),输出各阶段耗时/帧率/端到端延迟(F2 如实评估:硬编硬解基线 80ms/软编软解 150ms,超基线给阶段分解)+ UDP 统计(分片数/丢片/乱片/丢帧/平均重组耗时);回环到达的编码帧经 `dxgi-loop-frame` 事件(负载含 `codec` 字段)回传前端 WebCodecs 解码预览;报告与 UDP 统计落 `operations-*.log`。**全程标准视频编解码(H.264),禁止 JPEG**;真实抓屏失败显式报错,不回退合成帧。命令 `get_ffmpeg_capability` 输出编码能力报告(硬编/软编、硬解/软解实际路径)。`#[test] #[ignore]` TCP/UDP 两条真实链路自检(多线程 flavor,同步抓屏会阻塞工作线程)。
  - `audio.rs` — 远程会话音频链路:被控端常驻采集(`start_audio_capture`,真实 WASAPI 回环/回退 cpal,1s 切块发布),`host_write_loop` 以 `Msg::Audio` 推送新块;控制端 `peer_read_loop` 收到后 `play_audio` 经 cpal i16 输出流播放,断会停止;无输出设备静默跳过。
  - `ffmpeg_hw.rs` — 视频默认 H.264:编码走硬件(`preferred_encoder` 按 GPU 选 NVENC/QSV/AMF → 软件回退);解码按 RustDesk 技术路线实现 **D3D11VA 硬件解码**(创建 AV_HWDEVICE_TYPE_D3D11VA 设备 → 按 n8.0/avcodec-63 公开布局写 `AVCodecContext.hw_device_ctx` → `av_hwframe_transfer_data` 拷回 NV12 → swscale 转 RGB24),`avcodec_version()` 校验 major==63 且用编译期 `offset_of!` 断言字段偏移,不符或失败自动回退软件解码;`using_hwaccel()` 暴露解码路径。
  - `tray.rs` — 系统托盘(tauri `tray-icon` feature):常驻图标复用 `default_window_icon`,菜单 = 显示主窗口 / 允许他人协助(CheckMenuItem,与设置页同一接线:写 `host_enabled` + 真实启停 host)/ 文件传输(复用单例窗口)/ 退出;左键单击显示主窗口,右键弹菜单;关闭主窗口拦截 `CloseRequested` 隐藏到托盘(后台被控端继续运行),真正退出仅经托盘 `app.exit`;监听 `host-state` 事件同步勾选态与 tooltip。- `server/` — **独立 crate**(Windows 可运行 exe):
  - `dcr-signal.exe` — 信令 + STUN:TCP 21116 注册/心跳/查找/在线列表,连接断开自动注销;UDP 21115 RFC 5389 二进制 STUN Binding(XOR-MAPPED-ADDRESS)+ 不同源端口 NAT 探测 + `{"t":"stun"}` 调试;`--relay-hint` 下发中继地址。
  - `dcr-relay.exe` — TURN-like 中继:TCP 21117 `allocate {id,role}` 配对后双向字节透明转发(copy_bidirectional,上层 framing 透传);UDP 21119 `alloc-udp`/`data` 数据报转发。
  - 客户端(src-tauri)通过 `dcr-server = { path = "../server" }` 共享协议类型与 framing。
- 流参数闭环:`set_stream_quality`(quality low/medium/high → H.264 码率档位 ≈1.5/4/8 Mbps,可选 bitrate kbps 优先)/`set_stream_resolution` 写入本机 `STREAM_CFG` 并经会话下发 `stream` 消息(码率档位经协议历史 u8 字段承载),被控端 `apply_stream_cfg` 实时生效(codec 仅接受 h264/hevc,旧值 "jpeg" 自动迁移 h264)。
- **编码规范(全链路 H.264/H.265 + WebCodecs/FFmpeg 解码)**:音视频链路一律使用标准编解码——视频 H.264(可选 HEVC)走 `ffmpeg_hw` 硬编(NVENC/QSV/AMF→软件回退)与 D3D11VA 硬解,前端 WebCodecs VideoDecoder 解码渲染;音频 WAV/PCM。`Msg::Frame`/`capture-frame`/`remote-frame`/`dxgi-loop-frame` 的 `codec` 只能为 `h264`/`hevc`(本机预览兜底允许 `bgra` 原始字节直绘)。**jpeg-encoder/image 依赖已移除,禁止回潮**:不得重新引入 JPEG 编码/解码依赖与调用,前端不得用 `image/jpeg` MIME 渲染远端画面。
- 前端事件: `connection-state` / `remote-frame`(`{width,height,data:编码帧字节,seq,dur,key,codec}`,WebCodecs 解码;UDP 模式 width/height=0 由解码输出尺寸自适应) / `capture-frame`(`{monitorId,width,height,key,codec,data}`,h264/hevc 走 WebCodecs、bgra 走 putImageData) / `dxgi-loop-frame`(诊断回传编码帧,负载含 `codec` 字段,WebCodecs 预览) / `host-state` / `clipboard-synced` / `virtual-monitors-changed`。
- 设计令牌:`src/theme/tokens.ts`(自定义调色板/间距/圆角,非 Fluent tokens);组件用 `@fluentui/react-components` 的 `makeStyles`。设计规范见 `docs/design-system.md`。
- README §2.1 中 hbb_common/scrap/WebRTC 仍为**远期蓝图**(依赖 RustDesk 官方信令服务器与重依赖链);当前网络层为自研直连 TCP 协议 + 自研信令/STUN/TURN 服务(`server/`),已在 `network.rs` 中隔离,未来可替换。

## Gotchas

- `src-tauri/resources/idd_driver/` 已含真实驱动文件(usbmmidd 签名驱动 + 官方 deviceinstaller + RustDesk dylib),`tauri.conf.json` `bundle.resources` 打包。安装驱动/添加虚拟屏需要管理员权限。
- `tauri.conf.json` CSP 为 `connect-src ipc: http://ipc.localhost` — 网络调用全部在 Rust 侧(TCP/UDP),前端无直连,无需改 CSP;若未来前端直连 HBBS/HBBR 需更新。
- 被控端权限:IDD 驱动安装/`enableidd` 需 UAC 管理员;host 端口监听无特殊要求。
- 前端 dev server `allowedHosts` 包含 `.monkeycode-ai.online`(monkeycode 预览域)。
- Windows 安装包为 WiX,`zh-CN` 语言;版本 0.1.0,中文 commit 信息。
