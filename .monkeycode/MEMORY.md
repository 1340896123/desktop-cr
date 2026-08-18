# User Instruction Memory

This file records user instructions, preferences, and teachings for reference in future interactions.

## Format

### User Instruction Entry
User instruction entries should follow this format:

[User Instruction Summary]
- Date: [YYYY-MM-DD]
- Context: [Mentioned scenario or time]
- Instructions:
  - [Content of user teaching or instruction, described line by line]

### Project Knowledge Entry
Entries discovered by the Agent during task execution should follow this format:

[Project Knowledge Summary]
- Date: [YYYY-MM-DD]
- Context: Discovered by Agent while performing [specific task description]
- Category: [Operations & Deployment|Build Methods|Testing Methods|Troubleshooting & Debugging|Workflow & Collaboration|Environment Configuration]
- Instructions:
  - [Specific knowledge points, described line by line]

## Deduplication Strategy
- Before adding a new entry, check for similar or identical instructions.
- If a duplicate is found, skip the new entry or merge it with the existing one.
- When merging, update the context or date information.
- This helps avoid redundant entries and keeps the memory file tidy.

## Entries

[Project Knowledge Summary]
- Date: 2026-08-18
- Context: Discovered by Agent while performing 阶段一（POC）工程搭建（Tauri v2 + React + Fluent UI v9 远程桌面客户端）
- Category: Build Methods
- Instructions:
  - 前端构建：`cd /workspace && npm install && npm run build`
  - Rust 后端构建：`cd /workspace/src-tauri && cargo build`（首次编译约 5 分钟，产物在 target/debug/）
  - 前端预览：`cd /workspace && npm run dev`，Vite 默认端口 1420

[Project Knowledge Summary]
- Date: 2026-08-18
- Context: Discovered by Agent while performing 阶段一工程搭建
- Category: Environment Configuration
- Instructions:
  - Rust 工具链已安装于 `/root/.cargo/bin`，使用前需 `export PATH="$PATH:/root/.cargo/bin"`
  - Tauri Linux 编译所需系统库已安装：libwebkit2gtk-4.1-dev、libgtk-3-dev、libayatana-appindicator3-dev、librsvg2-dev 等
  - Windows 专属能力（DXGI 抓屏、IDD 驱动、SendInput）在 Linux 上用 `#[cfg(target_os = "windows")]` 隔离为 stub，Linux 返回 mock
  - RustDesk 的 hbb_common/scrap 依赖以注释保留在 Cargo.toml，未实际引入（避免 Linux 构建失败）

[Project Knowledge Summary]
- Date: 2026-08-18
- Context: Discovered by Agent while performing 阶段一工程搭建
- Category: Workflow & Collaboration
- Instructions:
  - 大型开发任务按 README 拆解后委托 subagent 执行，主 Agent 负责环境准备（工具链、系统依赖）与结果验证
  - 编译/构建类命令一律使用 background_terminal_create 后台执行，避免阻塞会话
