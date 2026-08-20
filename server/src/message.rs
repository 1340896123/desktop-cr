//! 信令/中继协议消息类型。
//!
//! 均使用 serde 外部标签枚举(`#[serde(tag = "t")]`),消息名(变体名)为 kebab-case,
//! 字段名为 snake_case。JSON 示例:`{"t":"register-ack","ok":true,"msg":"ok"}`。

use serde::{Deserialize, Serialize};

/// 信号服务器(Signal)与客户端之间的消息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "kebab-case")]
pub enum SignalMsg {
    /// 注册:`id` 为对端唯一标识,`lan` 为局域网地址("ip:port");
    /// `name`/`os`/`version`/`user` 为设备信息与归属用户(旧客户端缺省为空)。
    Register {
        id: String,
        lan: String,
        #[serde(default)]
        name: String,
        #[serde(default)]
        os: String,
        #[serde(default)]
        version: String,
        #[serde(default)]
        user: String,
    },
    /// 注册应答:`ok` 是否成功,`msg` 为错误信息(成功时可为空)。
    RegisterAck { ok: bool, msg: String },
    /// 心跳保活。
    Heartbeat { id: String },
    /// 注销。
    Unregister { id: String },
    /// 查找对端在线信息。
    Lookup { id: String },
    /// 查找应答:`online` 是否在线;在线时 `lan`/`external` 为可达地址,
    /// `relay_hint` 为可选的中继服务器提示("host:port")。
    LookupAck {
        online: bool,
        lan: String,
        external: String,
        relay_hint: String,
    },
    /// 请求在线对端列表。
    List,
    /// 在线对端列表应答。
    ListAck { peers: Vec<PeerEntry> },
}

/// 在线对端条目(用于 `ListAck`)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeerEntry {
    /// 对端唯一标识。
    pub id: String,
    /// 局域网地址("ip:port")。
    pub lan: String,
    /// 外部地址("ip:port")。
    pub external: String,
}

/// 中继服务器(Relay)与客户端之间的消息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "kebab-case")]
pub enum RelayMsg {
    /// 申请中继通道:`id` 为对端标识,`role` 为 "host" 或 "client"。
    Allocate { id: String, role: String },
    /// 通道分配结果:`peer_connected` 表示对端是否已接入。
    Allocated { id: String, peer_connected: bool },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 断言各变体序列化后的 "t" 标签为期望的 kebab-case 值。
    #[test]
    fn signal_msg_tags() {
        let cases: Vec<(SignalMsg, &str)> = vec![
            (
                SignalMsg::Register {
                    id: "a".into(),
                    lan: "10.0.0.2:9000".into(),
                    name: "PC".into(),
                    os: "Windows 11".into(),
                    version: "0.1.0".into(),
                    user: "alice".into(),
                },
                "register",
            ),
            (
                SignalMsg::RegisterAck {
                    ok: true,
                    msg: "ok".into(),
                },
                "register-ack",
            ),
            (SignalMsg::Heartbeat { id: "a".into() }, "heartbeat"),
            (SignalMsg::Unregister { id: "a".into() }, "unregister"),
            (SignalMsg::Lookup { id: "a".into() }, "lookup"),
            (
                SignalMsg::LookupAck {
                    online: true,
                    lan: "10.0.0.2:9000".into(),
                    external: "1.2.3.4:9000".into(),
                    relay_hint: "r.example.com:9000".into(),
                },
                "lookup-ack",
            ),
            (SignalMsg::List, "list"),
            (SignalMsg::ListAck { peers: vec![] }, "list-ack"),
        ];
        for (msg, expect) in cases {
            let v = serde_json::to_value(&msg).unwrap();
            assert_eq!(v["t"].as_str().unwrap(), expect, "消息 {v} 的 t 标签应为 {expect}");
        }
    }

    /// SignalMsg 序列化 → 反序列化往返一致。
    #[test]
    fn signal_msg_roundtrip() {
        let msg = SignalMsg::LookupAck {
            online: true,
            lan: "192.168.1.5:9000".into(),
            external: "203.0.113.9:9000".into(),
            relay_hint: "relay.example.com:3478".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: SignalMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, back);
    }

    /// RelayMsg 各变体 "t" 标签与往返一致。
    #[test]
    fn relay_msg_tags_and_roundtrip() {
        let allocate = RelayMsg::Allocate {
            id: "peer-1".into(),
            role: "host".into(),
        };
        assert_eq!(serde_json::to_value(&allocate).unwrap()["t"], "allocate");
        let allocated = RelayMsg::Allocated {
            id: "peer-1".into(),
            peer_connected: true,
        };
        assert_eq!(serde_json::to_value(&allocated).unwrap()["t"], "allocated");
        let back: RelayMsg = serde_json::from_str(&serde_json::to_string(&allocated).unwrap()).unwrap();
        assert_eq!(allocated, back);
    }
}
