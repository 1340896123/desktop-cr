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
    /// `name`/`os`/`version`/`user` 为设备信息与归属用户(旧客户端缺省为空);
    /// `token` 为登录 JWT(登录时携带,服务端以其解析出的用户名为准,不信任 user);
    /// `external` 为客户端经 STUN Binding 探测到的反射地址("ip:udp端口",可带
    /// 独立的视频 UDP 端口;旧客户端缺省为空,服务端回退用 TCP 连接对端地址)。
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
        #[serde(default)]
        token: String,
        #[serde(default)]
        external: String,
    },
    /// 注册应答:`ok` 是否成功,`msg` 为错误信息(成功时可为空);
    /// `auth_error` 表示登录令牌无效/已过期(客户端应提示重新登录,而非静默重试)。
    RegisterAck {
        ok: bool,
        msg: String,
        #[serde(default)]
        auth_error: bool,
    },
    /// 心跳保活。
    Heartbeat { id: String },
    /// 注销。
    Unregister { id: String },
    /// 查找对端在线信息。`token` 为登录 JWT(登录时携带):服务端启用认证时
    /// 仅返回本账号设备(或未归属设备)的地址,防止跨账号地址泄露。
    Lookup {
        id: String,
        #[serde(default)]
        token: String,
    },
    /// 查找应答:`online` 是否在线;在线时 `lan`/`external` 为可达地址,
    /// `relay_hint` 为可选的中继服务器提示("host:port")。
    LookupAck {
        online: bool,
        lan: String,
        external: String,
        relay_hint: String,
    },
    /// 请求在线对端列表。`user` 为请求方账号(空串表示未登录),`token` 为登录
    /// JWT(登录时携带):服务端按令牌解析出的账号过滤(无令牌/令牌无效仅返回
    /// 未归属设备),避免跨账号地址泄露。
    List {
        #[serde(default)]
        user: String,
        #[serde(default)]
        token: String,
    },
    /// 在线对端列表应答。`auth_error` 表示提供了令牌但校验失败
    /// (客户端应提示重新登录;此时 peers 仅含未归属设备)。
    ListAck {
        peers: Vec<PeerEntry>,
        #[serde(default)]
        auth_error: bool,
    },
}

/// 在线对端条目(用于 `ListAck`)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeerEntry {
    /// 对端唯一标识。
    pub id: String,
    /// 设备名(客户端上报,取自设备档案)。
    #[serde(default)]
    pub name: String,
    /// 归属用户名(客户端上报,未登录为空串)。
    #[serde(default)]
    pub owner: String,
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

/// UDP 中继消息(JSON 字符串数据报,`t` 字段区分)。
///
/// 协议约定(视频 UDP 数据面,**二进制透传语义**):
/// - `data` 的 `payload` 为 base64(标准字母表)编码的**完整 UDP 分片帧**
///   (客户端 16 字节分片头 + Annex-B 切片,见客户端 transport.rs 线格式),
///   中继原样解码转发,不解析、不重组、不二次封装;
/// - 控制消息(`alloc-udp`/`allocated`)为纯 JSON,与二进制负载经 payload
///   base64 封装共存无歧义——数据报首字节为 `{` 即控制帧,分片帧只出现在
///   `data` 的 payload 内,宿主侧收到的是**裸二进制分片字节**(与直连 UDP
///   收到的完全一致,接收端同一套 parse/重组代码);
/// - 向后兼容:既有转发语义(payload base64 → 原样字节)不变。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "kebab-case")]
pub enum RelayUdpMsg {
    /// 登记某 id 的宿主 UDP 端点(发送者地址即宿主地址),服务端回 `{"t":"allocated"}`。
    AllocUdp { id: String },
    /// 向某 id 的宿主转发载荷(payload 为 base64 的完整 UDP 分片帧)。
    Data { id: String, payload: String },
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
                    token: "jwt-token".into(),
                    external: "203.0.113.9:40001".into(),
                },
                "register",
            ),
            (
                SignalMsg::RegisterAck {
                    ok: true,
                    msg: "ok".into(),
                    auth_error: false,
                },
                "register-ack",
            ),
            (SignalMsg::Heartbeat { id: "a".into() }, "heartbeat"),
            (SignalMsg::Unregister { id: "a".into() }, "unregister"),
            (
                SignalMsg::Lookup {
                    id: "a".into(),
                    token: "jwt".into(),
                },
                "lookup",
            ),
            (
                SignalMsg::LookupAck {
                    online: true,
                    lan: "10.0.0.2:9000".into(),
                    external: "1.2.3.4:9000".into(),
                    relay_hint: "r.example.com:9000".into(),
                },
                "lookup-ack",
            ),
            (
                SignalMsg::List {
                    user: "alice".into(),
                    token: "jwt".into(),
                },
                "list",
            ),
            (
                SignalMsg::ListAck {
                    peers: vec![],
                    auth_error: false,
                },
                "list-ack",
            ),
        ];
        for (msg, expect) in cases {
            let v = serde_json::to_value(&msg).unwrap();
            assert_eq!(
                v["t"].as_str().unwrap(),
                expect,
                "消息 {v} 的 t 标签应为 {expect}"
            );
        }
    }

    /// SignalMsg 序列化 → 反序列化往返一致(含 Register 的 external 反射地址字段)。
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

        // Register.external 有 serde default:旧客户端 JSON(无该字段)可正常解析为空串
        let legacy = r#"{"t":"register","id":"old","lan":"10.0.0.2:9000"}"#;
        let parsed: SignalMsg = serde_json::from_str(legacy).unwrap();
        match parsed {
            SignalMsg::Register { external, .. } => assert_eq!(external, ""),
            other => panic!("期望 Register,得到 {other:?}"),
        }
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
        let back: RelayMsg =
            serde_json::from_str(&serde_json::to_string(&allocated).unwrap()).unwrap();
        assert_eq!(allocated, back);
    }
}
