# WinUI Remote Desktop 生产环境部署指南

> 适用版本:0.1.0(WiX MSI / NSIS 安装包,zh-CN)
> 本文档面向在生产环境(局域网内受控机器)安装、配置、运维本远程桌面客户端的实施人员。

## 1. 部署形态总览

本应用为 **Windows 桌面应用**(Tauri v2 + React),真实功能仅在 Windows 上可用;非 Windows 平台编译产物仅含占位实现,不建议用于生产。

两种部署角色(同一安装包,角色由运行方式决定):

| 角色 | 用途 | 关键行为 |
| --- | --- | --- |
| **控制端** | 在另一台机器上远程查看/操控目标机 | 连接对端 `ip:port`,接收画面帧,注入鼠标/键盘,同步剪贴板 |
| **被控端** | 在本机提供远程控制服务 | 启动 host 后监听 TCP 端口,单连接策略(新连接踢掉旧连接) |

典型拓扑(纯局域网,无需外网):

```
┌──────────────┐         TCP (默认 21118)        ┌──────────────┐
│  控制端       │  ───────────────────────────▶  │  被控端       │
│ (本应用)      │   frame / mouse / key / clip   │ (本应用 host) │
└──────────────┘  ◀───────────────────────────  └──────────────┘
    │                                                │
    └── 需能访问被控端 IP 与端口                       └── 防火墙放行入站端口
```

## 2. 构建发布

### 2.1 前置环境(构建机)

| 依赖 | 版本要求 | 说明 |
| --- | --- | --- |
| Node.js | ≥ 18(推荐 20 LTS) | 前端构建 |
| Rust toolchain | stable(实测 1.97) | Rust 侧编译 |
| MSVC Build Tools | VS2022 生成工具 + Windows SDK | Windows 依赖编译 |
| WebView2 Runtime | 系统已内置(Win11)或 Evergreen 安装包 | **运行期依赖**,Tauri v2 必需 |
| WiX Toolset | 由 Tauri CLI 自动下载 | 生成 MSI |

> 注意:`Cargo.toml` 的 `[profile.dev] debug=1` 是为了规避 rustc 1.97 对 webview2-com 巨型类型生成 debuginfo 时的 ICE,构建机 Rust 版本建议保持 1.97 附近。

### 2.2 构建命令

```powershell
# 1. 安装前端依赖
npm install

# 2. 验收检查(发布前必跑)
cargo check              # src-tauri 下,零警告
cargo test               # src-tauri 下,Rust 单元测试全过
npm run build            # tsc && vite build,零错误
npm test                 # vitest run,前端纯函数测试全过

# 3. 打包安装包
npm run tauri build      # 或 npx tauri build
```

### 2.3 产物说明

安装包输出目录:`src-tauri/target/release/bundle/`

| 产物 | 路径 | 说明 |
| --- | --- | --- |
| WiX MSI | `bundle/msi/*.msi` | 企业静默安装首选(支持组策略分发) |
| NSIS | `bundle/nsis/*.exe` | 引导安装,适合普通用户 |
| 解包目录 | `bundle/nsis/*/`、`bundle/msi/*/` | 绿色版可参考,含 `resources/idd_driver/` 驱动资源 |

MSI 静默安装示例:

```powershell
msiexec /i "WinUI-Remote-Desktop_0.1.0_x64_zh-CN.msi" /qn
```

安装包内置 `resources/idd_driver/`(usbmmidd 签名驱动 + deviceinstaller + RustDesk dylib),随安装自动落盘,供虚拟显示器功能使用。

## 3. 被控端部署

被控端 = 被远程控制的机器。**核心目标是:开机可连、画面可达、驱动就绪。**

### 3.1 安装

1. 以管理员身份运行安装包(或 `msiexec /i ... /qn`)。
2. 首次运行建议**以管理员权限启动应用**(右键 → 以管理员身份运行),以便后续安装 IDD 驱动。

### 3.2 启动被控端服务

1. 进入「设置 → 被控端」页。
2. 确认/修改监听端口(默认 `21118`;如需更改,控制端添加设备时使用相同端口)。
3. 打开「被控端」开关,确认提示显示正在监听。

> 已知限制:设置页的「开机自动启动」开关当前**仅持久化到配置,未实现系统级自启**。生产环境如需开机自启,采用以下任一方案:
>
> - **方案 A(推荐)**:将应用快捷方式放入 `shell:startup` 启动文件夹,并配合系统「自动登录」;应用启动后到「被控端」页打开开关。
> - **方案 B**:创建计划任务(系统启动时触发,以管理员运行,`/at logon` 或 `ONSTART`)。注意 host 监听不需要管理员,但安装驱动/增删虚拟屏需要,故计划任务按需选择权限级别。

### 3.3 防火墙放行

被控端监听 TCP 端口,必须放行入站规则(以默认端口 21118 为例,管理员 PowerShell):

```powershell
netsh advfirewall firewall add rule name="WinUI Remote Desktop Host" `
  dir=in action=allow protocol=TCP localport=21118 profile=private,domain
```

> 建议只放行 `private,domain` 专用配置文件,不放行 `public`,降低在公共网络下的暴露面。若采用组策略/企业防火墙,请确保该端口在主机间可达。

### 3.4 虚拟显示器驱动安装(可选,用于远程扩展屏)

需要被控端有管理员权限:

1. 在「虚拟显示器」页点击「安装驱动」(内部执行 `deviceinstaller64 install usbmmIdd.inf usbmmidd`)。
2. 安装成功后一键添加 1080P / 2K / 4K 虚拟屏(最多 4 个),分辨率列表写入注册表 `HKLM\...\WUDF\Services\usbmmIdd\Parameters\Monitors`。
3. 不再需要时在面板中移除;卸载驱动见驱动资源自带工具。

> 若同时安装了 RustDesk 的 `dylib_virtual_display.dll`,应用会优先走 `libloading` 加载其控制接口,失败时静默回退 usbmmidd,无需干预。

### 3.5 被控端验收清单

- [ ] 应用以管理员身份可正常运行
- [ ] 被控端开关打开,端口正常监听(`netstat -ano | findstr 21118`)
- [ ] 本机防火墙/企业防火墙放行该端口
- [ ] 控制端可连接、画面流畅、鼠标键盘注入生效
- [ ] (可选)虚拟显示器驱动安装成功、增删虚拟屏正常

## 4. 控制端部署

1. 安装应用(同 §3.1)。
2. 「设备」页添加对端:填写被控端 `IP:端口`(默认 `21118`)。
3. 点击设备进入远程会话;通过控制栏调节画质/分辨率,均实时下发被控端生效。
4. 全屏、剪贴板同步、断线自动清理均内置,无需额外配置。

网络要求:控制端到被控端 **TCP 直连可达**(同网段或路由可达),无外网需求。

## 5. 数据与日志

### 5.1 数据目录(Windows)

`%APPDATA%\com.example.winui-remote-desktop\`

| 路径 | 内容 | 运维建议 |
| --- | --- | --- |
| `config.json` | 设备列表、被控端口、host 开关、流参数 | 定期备份;升级不会清除 |
| `logs\operations-YYYYMMDD.log` | 操作日志,按 UTC 日期轮转,追加式,单行 `[时间] [模块] 动作 详情` | 排障主依据;应用内「设置」页可读取(今天+昨天) |

### 5.2 日志读取

- 应用内:设置 → 操作日志(最近记录,最新在前)。
- 直接查看:`%APPDATA%\com.example.winui-remote-desktop\logs\`。
- 排障时重点看模块:`host`(监听/连接)、`capture`(抓帧)、`network`(协议/握手)、`virtual_display`(驱动)。

## 6. 公网部署方案

### 6.1 现状约束(决定方案的前提)

当前网络层实现(`src-tauri/src/network.rs`)决定公网部署的两个硬约束:

- **无加密**:4 字节长度前缀 JSON 帧明文传输,无 TLS;画面、剪贴板、注入指令均可见。
- **无鉴权**:握手仅 `Hello{id, app, ver} → HelloAck{id}`,不校验任何密钥;任何能连上 host 端口的客户端即获得完整控制权。
- **直连模型**:控制端主动连接被控端 `ip:port`;被控端处于 NAT 后时无法被直连。

结论:**不能把 host 端口直接暴露公网**,公网部署必须叠加「加密层」并解决「被控端可达性」。

### 6.2 方案选型

| 方案 | 代码改动 | 安全等级 | 适用场景 |
| --- | --- | --- | --- |
| A. Overlay VPN(WireGuard / Tailscale) | 零代码 | 强(隧道加密) | 自用/小团队,首选 |
| B. 公网 VPS + frp 反向中继 | 零代码 | 中(必须叠加隧道加密) | 被控端全部在 NAT 后 |
| C. 协议层升级(TLS + token 鉴权) | 改 `network.rs` | 强(端到端) | 面向公网长期运营 |

### 6.3 方案 A:Overlay VPN(推荐首选)

1. 自建 WireGuard 组网,或使用 Tailscale / ZeroTier 的私有虚拟网络。
2. 所有被控端与控制端加入同一虚拟网(控制端和被控端都安装组网客户端)。
3. 应用内设备地址填写**虚拟内网 IP:21118**,应用逻辑零改动。
4. 公网入口只暴露组网协议端口(如 WireGuard 的 UDP 51820),host 端口不对公网开放。

- 优点:零代码、全链路加密兜底、跨网段统一管理、天然免疫 NAT 穿透问题。
- 缺点:引入组网依赖;被控端需保持组网客户端在线。

### 6.4 方案 B:公网 VPS + frp 反向中继

解决被控端在 NAT 后无法被直连的问题。VPS 部署 frps,被控端运行 frpc 反向映射:

```
控制端 ──TCP──▶ VPS(frps, 公网 21118) ──反向隧道──▶ 被控端 NAT 后(frpc → 本机 21118)
```

VPS 侧 `frps.toml`:

```toml
bindPort = 7000
```

被控端 `frpc.toml`:

```toml
serverAddr = "VPS_公网IP"
serverPort = 7000

[[proxies]]
name = "host-21118"
type = "tcp"
localIP = "127.0.0.1"
localPort = 21118
remotePort = 21118
```

控制端添加设备时地址填 `VPS_公网IP:21118` 即可。

> ⚠️ **安全红线**:frp 通道本身是明文,公网场景必须叠加隧道加密(stunnel / `ssh -L` / WireGuard 包一层),否则等于把无鉴权的控制端口裸奔在公网。

### 6.5 方案 C:协议层升级(面向公网长期运营)

在 `network.rs` 做最小改造即可获得真正公网就绪:

1. **传输加密**:引入 `tokio-rustls`,对 `connect_peer` / `serve_host` 的 `TcpStream` 做 TLS 包装;证书使用自建 CA 签发或公钥固定(pinned cert),避免部署证书体系。
2. **身份鉴权**:`Hello` 握手扩展 `token` 字段;被控端在设置页配置共享密钥(持久化到 `config.json`),握手校验失败立即断开,配合单连接策略可有效防暴力。
3. **访问控制**(可选):host 侧增加来源 IP 白名单。

改动面:仅 `network.rs`(IO 层 + 握手)+ `hbb_client.rs`(配置加密码字段)+ 设置页 UI 一个输入框。

> 中继模型(RustDesk HBBS/HBBR 形态:被控端主动注册、控制端经服务器拉流)属远期蓝图,改造范围大,当前不建议引入。

### 6.6 推荐落地路线

| 阶段 | 动作 | 效果 |
| --- | --- | --- |
| 短期(零代码) | 方案 A 组网 | 公网可达 + 全程加密,立即可用 |
| 中期 | NAT 复杂时叠加方案 B(仅隧道,不裸奔) | 覆盖任意网络拓扑 |
| 长期 | 按方案 C 升级协议 | 去隧道化,可无隧道直连公网 |

## 7. 安全与运维说明

1. **信任边界(重要)**:当前网络层为自研直连 TCP 协议(`network.rs`,4 字节长度前缀 JSON 帧),**无 TLS 加密**,且控制端一旦连接即可注入鼠标键盘并读取剪贴板。**仅允许部署在可信局域网**;不要将 host 端口暴露到公网/不可信网络。
2. **协议隔离**:未来接入 RustDesk HBBS/HBBR(带 TLS)时,仅替换 `network.rs` 层,不影响前端与注入/抓帧模块。CSP 已限定 `connect-src ipc: http://ipc.localhost`,前端无直连网络。
3. **单连接策略**:同一被控端同一时刻仅一个控制端;后连者踢掉前者,避免双人并发操控冲突。
4. **权限最小化**:普通远程控制不需要管理员权限;仅在「安装驱动 / 增删虚拟屏」时授予 UAC 管理员。
5. **端口与占用**:默认 `21118` 被占用时,修改「设置 → 被控端端口」并同步更新防火墙规则。
6. **性能基线**:默认流参数 15 FPS / JPEG 质量 70 / 1920×1080;画面卡顿时降低分辨率或帧率(设置页实时调节),局域网千兆下建议 25–30 FPS。

## 8. 升级与回滚

- **升级**:直接运行新版安装包即可就地升级(保留 `%APPDATA%` 配置与日志)。
- **回滚**:卸载新版 → 安装旧版安装包 → 配置仍在(版本间配置结构若变更,以旧版可解析为准)。
- **发布流程建议**:按 §2.2 全量验收 → 打包 → 在干净 Windows 11 机器上验证安装/连接/虚拟屏 → 分阶段灰度(先测试机,后生产)。

## 9. 常见问题排查(FAQ)

| 现象 | 排查步骤 |
| --- | --- |
| 控制端连不上被控端 | 1) 被控端是否已开 host(设置页开关);2) `netstat -ano | findstr <端口>` 是否 LISTENING;3) 控制端 `ping` / `Test-NetConnection <ip> -Port <端口>` 是否可达;4) 防火墙是否放行;5) 是否跨网段且路由未通 |
| 画面黑屏/不出帧 | 1) 查看被控端操作日志 `capture` 模块(通常 DXGI 采集被独占);2) 远程会话中是否有 RDP/全屏独占;3) 调低分辨率/帧率 |
| 虚拟屏添加失败 | 1) 是否管理员权限;2) 是否已安装驱动(先「安装驱动」);3) 是否超过 4 个;4) 查看 `virtual_display` 模块日志(可能回退到了 usbmmidd 路径) |
| 连接后马上被踢 | 单连接策略:检查是否已有另一个控制端连着同一被控端 |
| 鼠标/键盘无响应 | 1) 确认会话处于活动状态(未最小化/失焦);2) 被控端是否锁屏或 UAC 弹窗拦截(Secure Desktop 会吞掉注入);3) 查看注入相关日志 |
| 安装包被杀毒拦截 | 驱动安装器(deviceinstaller)属敏感操作,请在安全软件中放行安装目录与 `resources/idd_driver/` |

## 10. 发布前验收清单

- [ ] `cargo check` 零警告、`cargo test` 全过、`npm run build` 零错误、`npm test` 全过
- [ ] 干净 Windows 11 机器上 MSI 静默安装成功
- [ ] 被控端:host 监听、防火墙放行、控制端连接成功、画面+注入+剪贴板全通
- [ ] 虚拟显示器:驱动安装、增删虚拟屏成功(管理员环境)
- [ ] 数据目录 `%APPDATA%\com.example.winui-remote-desktop\` 生成 config.json 与日志
- [ ] 仅部署在可信局域网,host 端口未暴露公网
