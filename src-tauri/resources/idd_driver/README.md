# rustdesk-idd-driver 资源目录

本目录用于存放编译好的虚拟显示器驱动文件，当前阶段为占位说明。

## 所需文件（Windows 打包时提供）

- `rustdesk-idd-driver.dll`：RustDesk 基于微软 IDD 框架的间接显示驱动。
- `cert.cer` / 签名证书：驱动签名所需证书。
- `setup.exe` / `nefcon`：驱动安装辅助程序。

## 使用说明

1. 将编译好的驱动文件放入本目录。
2. 通过 Tauri command `install_virtual_display_driver` 触发安装（调用 nefcon / devcon）。
3. 通过 `add_virtual_monitor(width, height, fps)` 动态挂载虚拟显示器，
   支持 1080P / 2K / 4K 多分辨率与多刷新率。

## 参考

- RustDesk IDD Driver: https://github.com/rustdesk/rustdesk-idd-driver
- README.md 第 2.2 节「虚拟屏幕 (Virtual Display) 驱动集成」

> 注意：Linux 开发环境下本目录仅作占位，不参与编译。
