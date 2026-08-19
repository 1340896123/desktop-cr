//! dcr-server 协议基础库。
//!
//! 为后续实现的信号服务器(dcr-signal)与中继服务器(dcr-relay)提供:
//! - `framing`:TCP 长度前缀消息的读写(4 字节小端 u32 长度 + serde_json 字节);
//! - `message`:信令/中继协议消息类型(serde 外部标签枚举,`t` 字段为 kebab-case 消息名);
//! - `stun`:RFC 5389 二进制 STUN 编解码(纯函数,供 NAT 探测/中继发现使用)。

pub mod framing;
pub mod message;
pub mod relay;
pub mod signal;
pub mod stun;
