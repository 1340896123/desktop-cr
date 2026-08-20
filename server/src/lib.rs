//! dcr-server 协议基础库。
//!
//! 为后续实现的信号服务器(dcr-signal)与中继服务器(dcr-relay)提供:
//! - `framing`:TCP 长度前缀消息的读写(4 字节小端 u32 长度 + serde_json 字节);
//! - `message`:信令/中继协议消息类型(serde 外部标签枚举,`t` 字段为 kebab-case 消息名);
//! - `stun`:RFC 5389 二进制 STUN 编解码(纯函数,供 NAT 探测/中继发现使用);
//! - `auth`:账号存储 / argon2 密码哈希 / JWT 令牌(Web 管理后台认证);
//! - `admin`:Web 管理后台 HTTP 服务(账号登录、用户管理、在线设备、静态界面)。

pub mod admin;
pub mod auth;
pub mod framing;
pub mod message;
pub mod relay;
pub mod signal;
pub mod stun;
