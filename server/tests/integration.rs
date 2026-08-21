//! dcr-server 集成测试:信令注册/查找、STUN Binding、中继双向转发。
//!
//! 全部走 loopback(127.0.0.1:0 / 随机端口),不依赖外部网络。

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};

use dcr_server::framing::{read_msg, write_msg};
use dcr_server::message::{RelayMsg, SignalMsg};
use dcr_server::signal;
use dcr_server::stun;

/// 信令服务完整流程:注册 → 查找(自身与第二个客户端)→ 列表 → 注销。
#[tokio::test]
async fn signal_register_lookup_flow() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let relay_hint = "relay.example.com:21117";
    let core = std::sync::Arc::new(signal::SignalCore::new(relay_hint));
    let serve = tokio::spawn(async move {
        let _ = signal::serve(listener, udp, core).await;
    });

    // 客户端 A:注册
    let mut a = TcpStream::connect(addr).await.unwrap();
    write_msg(
        &mut a,
        &SignalMsg::Register {
            id: "pc-a".into(),
            lan: "192.168.1.5:21118".into(),
            name: "办公室PC".into(),
            os: "Windows 11".into(),
            version: "0.1.0".into(),
            user: "alice".into(),
            token: String::new(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut a).await.unwrap();
    match ack {
        SignalMsg::RegisterAck { ok, msg, .. } => assert!(ok, "注册应成功: {msg}"),
        other => panic!("期望 RegisterAck,得到 {other:?}"),
    }

    // 客户端 A:查找自己
    write_msg(
        &mut a,
        &SignalMsg::Lookup {
            id: "pc-a".into(),
            token: String::new(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut a).await.unwrap();
    match ack {
        SignalMsg::LookupAck {
            online,
            lan,
            external,
            relay_hint,
        } => {
            assert!(online, "应在线");
            assert_eq!(lan, "192.168.1.5:21118");
            assert!(!external.is_empty(), "外部地址应非空");
            assert_eq!(relay_hint, "relay.example.com:21117");
        }
        other => panic!("期望 LookupAck,得到 {other:?}"),
    }

    // 客户端 B:查找 A
    let mut b = TcpStream::connect(addr).await.unwrap();
    write_msg(
        &mut b,
        &SignalMsg::Lookup {
            id: "pc-a".into(),
            token: String::new(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut b).await.unwrap();
    match ack {
        SignalMsg::LookupAck { online, .. } => assert!(online, "B 应能查到 A"),
        other => panic!("期望 LookupAck,得到 {other:?}"),
    }

    // 列表(alice 查询,应含自己账号的 pc-a)
    write_msg(
        &mut b,
        &SignalMsg::List {
            user: "alice".into(),
            token: String::new(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut b).await.unwrap();
    match ack {
        SignalMsg::ListAck { peers, .. } => {
            assert!(peers.iter().any(|p| p.id == "pc-a"), "列表应含 pc-a");
        }
        other => panic!("期望 ListAck,得到 {other:?}"),
    }

    // 注销后 B 查不到
    write_msg(&mut a, &SignalMsg::Unregister { id: "pc-a".into() })
        .await
        .unwrap();
    let _: SignalMsg = read_msg(&mut a).await.unwrap(); // register-ack
    write_msg(
        &mut b,
        &SignalMsg::Lookup {
            id: "pc-a".into(),
            token: String::new(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut b).await.unwrap();
    match ack {
        SignalMsg::LookupAck { online, .. } => assert!(!online, "注销后应离线"),
        other => panic!("期望 LookupAck,得到 {other:?}"),
    }

    drop(a);
    drop(b);
    serve.abort();
}

/// 查找不存在的 id → online=false。
#[tokio::test]
async fn signal_lookup_unknown() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let core = std::sync::Arc::new(signal::SignalCore::new(""));
    let serve = tokio::spawn(async move {
        let _ = signal::serve(listener, udp, core).await;
    });

    let mut c = TcpStream::connect(addr).await.unwrap();
    write_msg(
        &mut c,
        &SignalMsg::Lookup {
            id: "ghost".into(),
            token: String::new(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut c).await.unwrap();
    match ack {
        SignalMsg::LookupAck { online, .. } => assert!(!online),
        other => panic!("期望 LookupAck,得到 {other:?}"),
    }
    serve.abort();
}

/// 短连接契约:注册应答后立即断开 → 设备被注销(服务端将「连接断开」视为离线)。
/// 该行为是长连接保活协议的前提,锁定契约防回归。
#[tokio::test]
async fn signal_register_then_disconnect_unregisters() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let core = std::sync::Arc::new(signal::SignalCore::new(""));
    let serve = tokio::spawn(async move {
        let _ = signal::serve(listener, udp, core).await;
    });

    // 短连接:注册 → 读应答 → 立即断开(旧客户端 signal_query 行为)
    {
        let mut c = TcpStream::connect(addr).await.unwrap();
        write_msg(
            &mut c,
            &SignalMsg::Register {
                id: "pc-short".into(),
                lan: "192.168.1.9:21118".into(),
                name: "短连PC".into(),
                os: "Windows 11".into(),
                version: "0.1.0".into(),
                user: "alice".into(),
                token: String::new(),
            },
        )
        .await
        .unwrap();
        let _: SignalMsg = read_msg(&mut c).await.unwrap();
    } // drop:断开

    // 等待服务端感知断开并注销
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let mut q = TcpStream::connect(addr).await.unwrap();
    write_msg(
        &mut q,
        &SignalMsg::Lookup {
            id: "pc-short".into(),
            token: String::new(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut q).await.unwrap();
    match ack {
        SignalMsg::LookupAck { online, .. } => assert!(!online, "短连接断开后应注销"),
        other => panic!("期望 LookupAck,得到 {other:?}"),
    }
    serve.abort();
}

/// 长连接保活:同一连接上注册 + 多次心跳,设备持续在线;断开 → 注销;
/// 重新连接注册(host 断线重连路径)→ 恢复在线。
#[tokio::test]
async fn signal_persistent_connection_keeps_online() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let core = std::sync::Arc::new(signal::SignalCore::new(""));
    let serve = tokio::spawn(async move {
        let _ = signal::serve(listener, udp, core).await;
    });

    // host 长连接:注册后保持连接
    let mut host = TcpStream::connect(addr).await.unwrap();
    write_msg(
        &mut host,
        &SignalMsg::Register {
            id: "pc-long".into(),
            lan: "192.168.1.10:21118".into(),
            name: "长连PC".into(),
            os: "Windows 11".into(),
            version: "0.1.0".into(),
            user: "alice".into(),
            token: String::new(),
        },
    )
    .await
    .unwrap();
    let _: SignalMsg = read_msg(&mut host).await.unwrap();

    // 观测连接
    let mut q = TcpStream::connect(addr).await.unwrap();
    // 注册应答后连接不关闭:多次心跳后设备持续在线
    for i in 0..3 {
        write_msg(&mut host, &SignalMsg::Heartbeat { id: "pc-long".into() })
            .await
            .unwrap();
        let ack: SignalMsg = read_msg(&mut host).await.unwrap();
        assert!(
            matches!(ack, SignalMsg::RegisterAck { ok: true, .. }),
            "第 {i} 次心跳应成功"
        );
        write_msg(&mut q, &SignalMsg::Lookup { id: "pc-long".into(), token: String::new() })
            .await
            .unwrap();
        let ack: SignalMsg = read_msg(&mut q).await.unwrap();
        match ack {
            SignalMsg::LookupAck { online, .. } => assert!(online, "长连接心跳后应在线"),
            other => panic!("期望 LookupAck,得到 {other:?}"),
        }
    }

    // 断开长连接 → 注销
    drop(host);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    write_msg(&mut q, &SignalMsg::Lookup { id: "pc-long".into(), token: String::new() })
        .await
        .unwrap();
    let ack: SignalMsg = read_msg(&mut q).await.unwrap();
    match ack {
        SignalMsg::LookupAck { online, .. } => assert!(!online, "长连接断开后应注销"),
        other => panic!("期望 LookupAck,得到 {other:?}"),
    }

    // 重连注册(host 断线重连路径)→ 恢复在线
    let mut host2 = TcpStream::connect(addr).await.unwrap();
    write_msg(
        &mut host2,
        &SignalMsg::Register {
            id: "pc-long".into(),
            lan: "192.168.1.10:21118".into(),
            name: "长连PC".into(),
            os: "Windows 11".into(),
            version: "0.1.0".into(),
            user: "alice".into(),
            token: String::new(),
        },
    )
    .await
    .unwrap();
    let _: SignalMsg = read_msg(&mut host2).await.unwrap();
    write_msg(&mut q, &SignalMsg::Lookup { id: "pc-long".into(), token: String::new() })
        .await
        .unwrap();
    let ack: SignalMsg = read_msg(&mut q).await.unwrap();
    match ack {
        SignalMsg::LookupAck { online, .. } => assert!(online, "重连注册后应恢复在线"),
        other => panic!("期望 LookupAck,得到 {other:?}"),
    }
    serve.abort();
}

/// 在线列表按账号在服务端过滤:登录用户只见自己账号设备,未登录仅见未归属设备,
/// 旧客户端 `{"t":"list"}`(无 user 字段)兼容为未登录语义。
#[tokio::test]
async fn signal_list_filters_by_account() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let core = std::sync::Arc::new(signal::SignalCore::new(""));
    let serve = tokio::spawn(async move {
        let _ = signal::serve(listener, udp, core).await;
    });

    // 三个设备分属 alice / bob / 未归属(各占一条长连接,保持不断开)
    let mut conns: Vec<TcpStream> = Vec::new();
    for (id, user) in [
        ("pc-alice", "alice"),
        ("pc-bob", "bob"),
        ("pc-free", ""),
    ] {
        let mut c = TcpStream::connect(addr).await.unwrap();
        write_msg(
            &mut c,
            &SignalMsg::Register {
                id: id.into(),
                lan: "192.168.1.20:21118".into(),
                name: id.into(),
                os: "Windows 11".into(),
                version: "0.1.0".into(),
                user: user.into(),
                token: if user == "alice" {
                    "client-local-token".into()
                } else {
                    String::new()
                },
            },
        )
        .await
        .unwrap();
        let _: SignalMsg = read_msg(&mut c).await.unwrap();
        conns.push(c);
    }

    // 按账号过滤
    let mut q = TcpStream::connect(addr).await.unwrap();
    write_msg(
        &mut q,
        &SignalMsg::List {
            user: "alice".into(),
            token: String::new(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut q).await.unwrap();
    match ack {
        SignalMsg::ListAck { peers, .. } => {
            assert_eq!(peers.len(), 1, "alice 应只见自己的设备: {peers:?}");
            assert_eq!(peers[0].id, "pc-alice");
        }
        other => panic!("期望 ListAck,得到 {other:?}"),
    }

    // 开放认证模式没有 JWT 密钥,即使客户端仍携带本地登录令牌,
    // 也应按 user 字段过滤,不能被错误降级成匿名用户。
    write_msg(
        &mut q,
        &SignalMsg::List {
            user: "alice".into(),
            token: "client-local-token".into(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut q).await.unwrap();
    match ack {
        SignalMsg::ListAck { peers, auth_error } => {
            assert!(!auth_error, "开放模式不应把客户端令牌判为认证错误");
            assert_eq!(peers.len(), 1, "开放模式带令牌仍应看到 alice 设备: {peers:?}");
            assert_eq!(peers[0].id, "pc-alice");
        }
        other => panic!("期望 ListAck,得到 {other:?}"),
    }

    write_msg(
        &mut q,
        &SignalMsg::List {
            user: "bob".into(),
            token: String::new(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut q).await.unwrap();
    match ack {
        SignalMsg::ListAck { peers, .. } => {
            assert_eq!(peers.len(), 1, "bob 应只见自己的设备: {peers:?}");
            assert_eq!(peers[0].id, "pc-bob");
        }
        other => panic!("期望 ListAck,得到 {other:?}"),
    }

    // 未登录:仅未归属设备
    write_msg(
        &mut q,
        &SignalMsg::List {
            user: String::new(),
            token: String::new(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut q).await.unwrap();
    match ack {
        SignalMsg::ListAck { peers, .. } => {
            assert_eq!(peers.len(), 1, "未登录应只见未归属设备: {peers:?}");
            assert_eq!(peers[0].id, "pc-free");
        }
        other => panic!("期望 ListAck,得到 {other:?}"),
    }

    // 旧客户端兼容:`{"t":"list"}`(无 user 字段)等价于未登录语义
    let json = br#"{"t":"list"}"#;
    q.write_all(&(json.len() as u32).to_le_bytes()).await.unwrap();
    q.write_all(json).await.unwrap();
    let ack: SignalMsg = read_msg(&mut q).await.unwrap();
    match ack {
        SignalMsg::ListAck { peers, .. } => {
            assert_eq!(peers.len(), 1, "旧格式 list 应视为未登录: {peers:?}");
            assert_eq!(peers[0].id, "pc-free");
        }
        other => panic!("期望 ListAck,得到 {other:?}"),
    }
    drop(conns);
    serve.abort();
}

/// 重连竞态:旧连接断开不得误删已被新连接接管的同 id 记录。
/// 新连接注册同 id 后,旧连接因代次不符被跳过注销,设备仍在线。
#[tokio::test]
async fn signal_old_connection_disconnect_does_not_kill_new() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let core = std::sync::Arc::new(signal::SignalCore::new(""));
    let serve = tokio::spawn(async move {
        let _ = signal::serve(listener, udp, core).await;
    });

    // 连接 1:注册 pc-race
    let mut c1 = TcpStream::connect(addr).await.unwrap();
    write_msg(
        &mut c1,
        &SignalMsg::Register {
            id: "pc-race".into(),
            lan: "192.168.1.30:21118".into(),
            name: "竞态PC".into(),
            os: "Windows 11".into(),
            version: "0.1.0".into(),
            user: "alice".into(),
            token: String::new(),
        },
    )
    .await
    .unwrap();
    let _: SignalMsg = read_msg(&mut c1).await.unwrap();

    // 连接 2:同 id 重新注册(接管记录)
    let mut c2 = TcpStream::connect(addr).await.unwrap();
    write_msg(
        &mut c2,
        &SignalMsg::Register {
            id: "pc-race".into(),
            lan: "192.168.1.31:21118".into(),
            name: "竞态PC".into(),
            os: "Windows 11".into(),
            version: "0.1.0".into(),
            user: "alice".into(),
            token: String::new(),
        },
    )
    .await
    .unwrap();
    let _: SignalMsg = read_msg(&mut c2).await.unwrap();

    // 旧连接断开:不得误删新连接持有的记录
    drop(c1);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let mut q = TcpStream::connect(addr).await.unwrap();
    write_msg(&mut q, &SignalMsg::Lookup { id: "pc-race".into(), token: String::new() })
        .await
        .unwrap();
    let ack: SignalMsg = read_msg(&mut q).await.unwrap();
    match ack {
        SignalMsg::LookupAck { online, .. } => assert!(online, "旧连接断开后不应误删新连接持有的设备"),
        other => panic!("期望 LookupAck,得到 {other:?}"),
    }

    // 新连接也断开:此时才真正注销
    drop(c2);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    write_msg(&mut q, &SignalMsg::Lookup { id: "pc-race".into(), token: String::new() })
        .await
        .unwrap();
    let ack: SignalMsg = read_msg(&mut q).await.unwrap();
    match ack {
        SignalMsg::LookupAck { online, .. } => assert!(!online, "最后一个连接断开后才应注销"),
        other => panic!("期望 LookupAck,得到 {other:?}"),
    }
    serve.abort();
}

/// 设备数上限:已登记设备重连不占新名额;新设备达到上限后被拒绝。
#[tokio::test]
async fn signal_reconnect_allowed_at_device_limit() {
    use std::sync::{Arc, RwLock};
    use dcr_server::config::{ConfigStore, ServerConfig};
    use dcr_server::devices::DeviceStore;
    use dcr_server::sessions::SessionCore;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    // 隔离存储:单用户设备上限 = 1
    let dir = std::env::temp_dir().join(format!("dcr-limit-it-{}", std::process::id()));
    let cfg = Arc::new(RwLock::new(ConfigStore::new(
        &dir,
        ServerConfig {
            max_devices_per_user: 1,
            ..Default::default()
        },
    )));
    let core = Arc::new(signal::SignalCore::with_stores(
        "",
        Arc::new(DeviceStore::new(&dir)),
        Arc::new(SessionCore::new()),
        cfg,
        Vec::new(),
    ));
    let serve = tokio::spawn(async move {
        let _ = signal::serve(listener, udp, core).await;
    });

    // 注册 pc-a(alice):首个设备,允许
    let mut c1 = TcpStream::connect(addr).await.unwrap();
    write_msg(
        &mut c1,
        &SignalMsg::Register {
            id: "pc-a".into(),
            lan: "192.168.1.40:21118".into(),
            name: "PC-A".into(),
            os: "Windows 11".into(),
            version: "0.1.0".into(),
            user: "alice".into(),
            token: String::new(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut c1).await.unwrap();
    assert!(matches!(ack, SignalMsg::RegisterAck { ok: true, .. }));

    // 新设备 pc-b(alice):达到上限,拒绝
    let mut c2 = TcpStream::connect(addr).await.unwrap();
    write_msg(
        &mut c2,
        &SignalMsg::Register {
            id: "pc-b".into(),
            lan: "192.168.1.41:21118".into(),
            name: "PC-B".into(),
            os: "Windows 11".into(),
            version: "0.1.0".into(),
            user: "alice".into(),
            token: String::new(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut c2).await.unwrap();
    assert!(
        matches!(ack, SignalMsg::RegisterAck { ok: false, .. }),
        "新设备超上限应被拒绝,得到 {ack:?}"
    );

    // 已登记设备 pc-a 重连:不占新名额,允许
    let mut c3 = TcpStream::connect(addr).await.unwrap();
    write_msg(
        &mut c3,
        &SignalMsg::Register {
            id: "pc-a".into(),
            lan: "192.168.1.40:21118".into(),
            name: "PC-A".into(),
            os: "Windows 11".into(),
            version: "0.1.0".into(),
            user: "alice".into(),
            token: String::new(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut c3).await.unwrap();
    assert!(
        matches!(ack, SignalMsg::RegisterAck { ok: true, .. }),
        "已登记设备重连应允许,得到 {ack:?}"
    );
    serve.abort();
    std::fs::remove_dir_all(&dir).ok();
}

/// 账号认证:服务端校验 JWT 令牌,以令牌解析出的用户名为准,
/// 不信任客户端自报的 user;无效令牌拒绝注册、列表按未登录处理。
#[tokio::test]
async fn signal_auth_token_validation() {
    use std::sync::{Arc, RwLock};
    use dcr_server::auth::{AuthState, UserStore};
    use dcr_server::config::{ConfigStore, ServerConfig};
    use dcr_server::devices::DeviceStore;
    use dcr_server::sessions::SessionCore;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // 装配带认证的信令核心(真实密钥 + 用户存储)
    let dir = std::env::temp_dir().join(format!("dcr-auth-it-{}", std::process::id()));
    let store = Arc::new(UserStore::new(&dir));
    let secret = b"test-secret-0123456789abcdef012345".to_vec();
    let auth = AuthState::new(store.clone(), secret.clone());
    auth.store.create_user("alice", "pass123").unwrap();
    let alice_token = auth.login("alice", "pass123").unwrap();
    let devices = Arc::new(DeviceStore::new(&dir));
    let sessions = Arc::new(SessionCore::new());
    let cfg = Arc::new(RwLock::new(ConfigStore::new(&dir, ServerConfig::default())));
    let core = Arc::new(signal::SignalCore::with_stores("", devices, sessions, cfg, secret));
    let serve = tokio::spawn(async move {
        let _ = signal::serve(listener, udp, core).await;
    });

    // 携带有效令牌注册:归属以令牌为准,忽略客户端自报的 user
    let mut c1 = TcpStream::connect(addr).await.unwrap();
    write_msg(
        &mut c1,
        &SignalMsg::Register {
            id: "pc-a".into(),
            lan: "192.168.1.50:21118".into(),
            name: "PC-A".into(),
            os: "Windows 11".into(),
            version: "0.1.0".into(),
            user: "bogus".into(),
            token: alice_token.clone(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut c1).await.unwrap();
    assert!(matches!(ack, SignalMsg::RegisterAck { ok: true, .. }));

    // 无效令牌:拒绝注册
    let mut c2 = TcpStream::connect(addr).await.unwrap();
    write_msg(
        &mut c2,
        &SignalMsg::Register {
            id: "pc-b".into(),
            lan: "192.168.1.51:21118".into(),
            name: "PC-B".into(),
            os: "Windows 11".into(),
            version: "0.1.0".into(),
            user: "alice".into(),
            token: "forged-token".into(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut c2).await.unwrap();
    assert!(
        matches!(ack, SignalMsg::RegisterAck { ok: false, .. }),
        "无效令牌注册应被拒绝,得到 {ack:?}"
    );

    // 无令牌注册(即便谎称 alice):认证启用时按未登录处理 → 未归属设备
    let mut c3 = TcpStream::connect(addr).await.unwrap();
    write_msg(
        &mut c3,
        &SignalMsg::Register {
            id: "pc-c".into(),
            lan: "192.168.1.52:21118".into(),
            name: "PC-C".into(),
            os: "Windows 11".into(),
            version: "0.1.0".into(),
            user: "alice".into(),
            token: String::new(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut c3).await.unwrap();
    assert!(matches!(ack, SignalMsg::RegisterAck { ok: true, .. }));

    // 他账号令牌不得强占已归属设备(alice 的 pc-a)
    auth.store.create_user("bob", "pass456").unwrap();
    let bob_token = auth.login("bob", "pass456").unwrap();
    let mut c4 = TcpStream::connect(addr).await.unwrap();
    write_msg(
        &mut c4,
        &SignalMsg::Register {
            id: "pc-a".into(),
            lan: "192.168.1.50:21118".into(),
            name: "PC-A".into(),
            os: "Windows 11".into(),
            version: "0.1.0".into(),
            user: "bob".into(),
            token: bob_token.clone(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut c4).await.unwrap();
    assert!(
        matches!(ack, SignalMsg::RegisterAck { ok: false, .. }),
        "他账号令牌强占已归属设备应被拒绝,得到 {ack:?}"
    );

    // 归属账号本人(有效令牌)重连仍允许
    let mut c5 = TcpStream::connect(addr).await.unwrap();
    write_msg(
        &mut c5,
        &SignalMsg::Register {
            id: "pc-a".into(),
            lan: "192.168.1.50:21118".into(),
            name: "PC-A".into(),
            os: "Windows 11".into(),
            version: "0.1.0".into(),
            user: "alice".into(),
            token: alice_token.clone(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut c5).await.unwrap();
    assert!(matches!(ack, SignalMsg::RegisterAck { ok: true, .. }));

    // 列表过滤:有效令牌 → 只见令牌账号设备
    let mut q = TcpStream::connect(addr).await.unwrap();
    write_msg(
        &mut q,
        &SignalMsg::List {
            user: "alice".into(),
            token: alice_token.clone(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut q).await.unwrap();
    match ack {
        SignalMsg::ListAck { peers, .. } => {
            assert_eq!(peers.len(), 1, "alice 令牌应只见自己设备: {peers:?}");
            assert_eq!(peers[0].id, "pc-a");
        }
        other => panic!("期望 ListAck,得到 {other:?}"),
    }

    // 无效令牌 → 按未登录处理:仅未归属设备(pc-c),不得看到 alice 的 pc-a
    write_msg(
        &mut q,
        &SignalMsg::List {
            user: "alice".into(),
            token: "forged-token".into(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut q).await.unwrap();
    match ack {
        SignalMsg::ListAck { peers, .. } => {
            assert_eq!(peers.len(), 1, "无效令牌应只见未归属设备: {peers:?}");
            assert_eq!(peers[0].id, "pc-c");
        }
        other => panic!("期望 ListAck,得到 {other:?}"),
    }

    // 无令牌 → 同未登录
    write_msg(
        &mut q,
        &SignalMsg::List {
            user: "alice".into(),
            token: String::new(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut q).await.unwrap();
    match ack {
        SignalMsg::ListAck { peers, .. } => {
            assert_eq!(peers.len(), 1, "无令牌应只见未归属设备: {peers:?}");
            assert_eq!(peers[0].id, "pc-c");
        }
        other => panic!("期望 ListAck,得到 {other:?}"),
    }
    serve.abort();
    std::fs::remove_dir_all(&dir).ok();
}

/// 心跳所有权:仅本连接注册(且仍持有代次)的记录可续期;
/// 其他连接对同一 id 发心跳必须被拒绝,防止伪造在线。
#[tokio::test]
async fn signal_heartbeat_requires_connection_ownership() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let core = std::sync::Arc::new(signal::SignalCore::new(""));
    let serve = tokio::spawn(async move {
        let _ = signal::serve(listener, udp, core).await;
    });

    // 连接 A:注册 pc-hb(持有记录所有权)
    let mut a = TcpStream::connect(addr).await.unwrap();
    write_msg(
        &mut a,
        &SignalMsg::Register {
            id: "pc-hb".into(),
            lan: "192.168.1.60:21118".into(),
            name: "心跳PC".into(),
            os: "Windows 11".into(),
            version: "0.1.0".into(),
            user: "alice".into(),
            token: String::new(),
        },
    )
    .await
    .unwrap();
    let _: SignalMsg = read_msg(&mut a).await.unwrap();

    // 连接 B:对 pc-hb 发心跳 → 应被拒绝(非本连接注册)
    let mut b = TcpStream::connect(addr).await.unwrap();
    write_msg(&mut b, &SignalMsg::Heartbeat { id: "pc-hb".into() })
        .await
        .unwrap();
    let ack: SignalMsg = read_msg(&mut b).await.unwrap();
    assert!(
        matches!(ack, SignalMsg::RegisterAck { ok: false, .. }),
        "非本连接注册的心跳应被拒绝,得到 {ack:?}"
    );

    // B 的心跳不得续期 A 的记录:B 断开后 A 记录的 last_seen 未被刷新 → 超时后离线。
    // 用未心跳的 A 直接对比:正常情况 A 心跳有效
    write_msg(&mut a, &SignalMsg::Heartbeat { id: "pc-hb".into() })
        .await
        .unwrap();
    let ack: SignalMsg = read_msg(&mut a).await.unwrap();
    assert!(
        matches!(ack, SignalMsg::RegisterAck { ok: true, .. }),
        "注册连接自身的心跳应成功,得到 {ack:?}"
    );

    // 同 id 新连接接管后,旧连接的心跳失去所有权
    let mut c = TcpStream::connect(addr).await.unwrap();
    write_msg(
        &mut c,
        &SignalMsg::Register {
            id: "pc-hb".into(),
            lan: "192.168.1.61:21118".into(),
            name: "心跳PC".into(),
            os: "Windows 11".into(),
            version: "0.1.0".into(),
            user: "alice".into(),
            token: String::new(),
        },
    )
    .await
    .unwrap();
    let _: SignalMsg = read_msg(&mut c).await.unwrap();
    write_msg(&mut a, &SignalMsg::Heartbeat { id: "pc-hb".into() })
        .await
        .unwrap();
    let ack: SignalMsg = read_msg(&mut a).await.unwrap();
    assert!(
        matches!(ack, SignalMsg::RegisterAck { ok: false, .. }),
        "被新连接接管后旧连接的心跳应被拒绝,得到 {ack:?}"
    );

    drop(a);
    drop(b);
    drop(c);
    serve.abort();
}

/// Lookup 账号鉴权:启用认证时,仅本账号(或未归属)设备可被查询到地址,
/// 其他账号/匿名即使知道设备 ID 也拿不到 lan/external。
#[tokio::test]
async fn signal_lookup_requires_account_ownership() {
    use std::sync::{Arc, RwLock};
    use dcr_server::auth::{AuthState, UserStore};
    use dcr_server::config::{ConfigStore, ServerConfig};
    use dcr_server::devices::DeviceStore;
    use dcr_server::sessions::SessionCore;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let dir = std::env::temp_dir().join(format!("dcr-lookup-it-{}", std::process::id()));
    let store = Arc::new(UserStore::new(&dir));
    let secret = b"test-secret-lookupit-0123456789abc".to_vec();
    let auth = AuthState::new(store.clone(), secret.clone());
    auth.store.create_user("alice", "pass123").unwrap();
    auth.store.create_user("bob", "pass123").unwrap();
    let alice_token = auth.login("alice", "pass123").unwrap();
    let bob_token = auth.login("bob", "pass123").unwrap();
    let devices = Arc::new(DeviceStore::new(&dir));
    let sessions = Arc::new(SessionCore::new());
    let cfg = Arc::new(RwLock::new(ConfigStore::new(&dir, ServerConfig::default())));
    let core = Arc::new(signal::SignalCore::with_stores("", devices, sessions, cfg, secret));
    let serve = tokio::spawn(async move {
        let _ = signal::serve(listener, udp, core).await;
    });

    // alice 注册 pc-a(带 alice 令牌)
    let mut host = TcpStream::connect(addr).await.unwrap();
    write_msg(
        &mut host,
        &SignalMsg::Register {
            id: "pc-a".into(),
            lan: "192.168.1.70:21118".into(),
            name: "PC-A".into(),
            os: "Windows 11".into(),
            version: "0.1.0".into(),
            user: "alice".into(),
            token: alice_token.clone(),
        },
    )
    .await
    .unwrap();
    let _: SignalMsg = read_msg(&mut host).await.unwrap();

    // alice 本人可查到地址
    let mut q = TcpStream::connect(addr).await.unwrap();
    write_msg(
        &mut q,
        &SignalMsg::Lookup {
            id: "pc-a".into(),
            token: alice_token.clone(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut q).await.unwrap();
    match ack {
        SignalMsg::LookupAck { online, .. } => assert!(online, "归属账号本人应可查到"),
        other => panic!("期望 LookupAck,得到 {other:?}"),
    }

    // bob 查 pc-a:即使知道设备 ID 也查不到地址
    write_msg(
        &mut q,
        &SignalMsg::Lookup {
            id: "pc-a".into(),
            token: bob_token.clone(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut q).await.unwrap();
    match ack {
        SignalMsg::LookupAck {
            online, lan, external, ..
        } => {
            assert!(!online, "他账号不得查到 alice 设备");
            assert!(lan.is_empty() && external.is_empty(), "地址不得泄露: {lan}/{external}");
        }
        other => panic!("期望 LookupAck,得到 {other:?}"),
    }

    // 匿名(无令牌)查 pc-a:同样不可查
    write_msg(
        &mut q,
        &SignalMsg::Lookup {
            id: "pc-a".into(),
            token: String::new(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut q).await.unwrap();
    match ack {
        SignalMsg::LookupAck { online, .. } => assert!(!online, "匿名不得查到 alice 设备"),
        other => panic!("期望 LookupAck,得到 {other:?}"),
    }

    drop(host);
    drop(q);
    serve.abort();
    std::fs::remove_dir_all(&dir).ok();
}

/// List 认证错误标记:令牌无效时 ListAck 置 auth_error,客户端据此强制重新登录,
/// 避免令牌过期后「我的设备」静默为空。
#[tokio::test]
async fn signal_list_reports_auth_error() {
    use std::sync::{Arc, RwLock};
    use dcr_server::auth::{AuthState, UserStore};
    use dcr_server::config::{ConfigStore, ServerConfig};
    use dcr_server::devices::DeviceStore;
    use dcr_server::sessions::SessionCore;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let dir = std::env::temp_dir().join(format!("dcr-listauth-it-{}", std::process::id()));
    let store = Arc::new(UserStore::new(&dir));
    let secret = b"test-secret-listauth-0123456789abc".to_vec();
    let auth = AuthState::new(store.clone(), secret.clone());
    auth.store.create_user("alice", "pass123").unwrap();
    let alice_token = auth.login("alice", "pass123").unwrap();
    let devices = Arc::new(DeviceStore::new(&dir));
    let sessions = Arc::new(SessionCore::new());
    let cfg = Arc::new(RwLock::new(ConfigStore::new(&dir, ServerConfig::default())));
    let core = Arc::new(signal::SignalCore::with_stores("", devices, sessions, cfg, secret));
    let serve = tokio::spawn(async move {
        let _ = signal::serve(listener, udp, core).await;
    });

    // alice 注册 pc-a(保持长连接)
    let mut host = TcpStream::connect(addr).await.unwrap();
    write_msg(
        &mut host,
        &SignalMsg::Register {
            id: "pc-a".into(),
            lan: "192.168.1.80:21118".into(),
            name: "PC-A".into(),
            os: "Windows 11".into(),
            version: "0.1.0".into(),
            user: "alice".into(),
            token: alice_token.clone(),
        },
    )
    .await
    .unwrap();
    let _: SignalMsg = read_msg(&mut host).await.unwrap();

    let mut q = TcpStream::connect(addr).await.unwrap();

    // 有效令牌:auth_error=false,且可见自己的设备
    write_msg(
        &mut q,
        &SignalMsg::List {
            user: "alice".into(),
            token: alice_token.clone(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut q).await.unwrap();
    match ack {
        SignalMsg::ListAck { peers, auth_error } => {
            assert!(!auth_error, "有效令牌不应置认证错误");
            assert_eq!(peers.len(), 1, "应可见自己的设备: {peers:?}");
        }
        other => panic!("期望 ListAck,得到 {other:?}"),
    }

    // 无效令牌:auth_error=true(客户端据此提示重新登录),且仅见未归属设备
    write_msg(
        &mut q,
        &SignalMsg::List {
            user: "alice".into(),
            token: "forged-token".into(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut q).await.unwrap();
    match ack {
        SignalMsg::ListAck { peers, auth_error } => {
            assert!(auth_error, "无效令牌应置认证错误标记");
            assert!(peers.is_empty(), "无效令牌不得看到 alice 设备: {peers:?}");
        }
        other => panic!("期望 ListAck,得到 {other:?}"),
    }

    // 无令牌:按未登录处理,不置认证错误(正常语义,非失效场景)
    write_msg(
        &mut q,
        &SignalMsg::List {
            user: "alice".into(),
            token: String::new(),
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut q).await.unwrap();
    match ack {
        SignalMsg::ListAck { peers, auth_error } => {
            assert!(!auth_error, "无令牌是正常未登录语义,不应置认证错误");
            assert!(peers.is_empty(), "未登录不得看到 alice 设备: {peers:?}");
        }
        other => panic!("期望 ListAck,得到 {other:?}"),
    }

    drop(host);
    drop(q);
    serve.abort();
    std::fs::remove_dir_all(&dir).ok();
}
#[tokio::test]
async fn stun_binding_udp() {
    // 服务端
    let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let server_addr = server.local_addr().unwrap();
    let probe = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let serve = tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        loop {
            let (n, src) = server.recv_from(&mut buf).await.unwrap();
            signal::handle_stun_packet(&server, &probe, None, 0, buf[..n].to_vec(), src).await;
        }
    });

    // 客户端
    let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let client_addr = client.local_addr().unwrap();

    // 构造标准 Binding Request
    let txn: [u8; 12] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    let mut req = vec![0u8; 20];
    req[0..2].copy_from_slice(&stun::BINDING_REQUEST.to_be_bytes());
    req[2..4].copy_from_slice(&0u16.to_be_bytes());
    req[4..8].copy_from_slice(&stun::STUN_MAGIC_COOKIE.to_be_bytes());
    req[8..20].copy_from_slice(&txn);

    client.send_to(&req, server_addr).await.unwrap();
    let mut buf = [0u8; 2048];
    let (n, _) = tokio::time::timeout(std::time::Duration::from_secs(3), client.recv_from(&mut buf))
        .await
        .expect("STUN 响应超时")
        .unwrap();
    let (port, ip) = stun::parse_binding_response(&buf[..n]).unwrap();
    assert_eq!(port, client_addr.port(), "端口应还原为客户端端口");
    assert_eq!(ip, client_addr.ip(), "地址应还原为客户端地址");

    serve.abort();
}

/// TCP 中继:host 先连,client 后连,双向字节透传。
#[tokio::test]
async fn relay_pipe_host_first() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = listener.local_addr().unwrap();
    let serve = tokio::spawn(async move {
        let _ = dcr_server::relay::serve_tcp(listener, None).await;
    });

    // host 接入
    let mut host = TcpStream::connect(relay_addr).await.unwrap();
    write_msg(
        &mut host,
        &RelayMsg::Allocate {
            id: "peer-1".into(),
            role: "host".into(),
        },
    )
    .await
    .unwrap();
    let ack: RelayMsg = read_msg(&mut host).await.unwrap();
    assert!(
        matches!(ack, RelayMsg::Allocated { ref id, peer_connected: false } if id == "peer-1"),
        "host 应收到 peer_connected=false"
    );

    // client 接入
    let mut client = TcpStream::connect(relay_addr).await.unwrap();
    write_msg(
        &mut client,
        &RelayMsg::Allocate {
            id: "peer-1".into(),
            role: "client".into(),
        },
    )
    .await
    .unwrap();
    let ack: RelayMsg = read_msg(&mut client).await.unwrap();
    assert!(
        matches!(ack, RelayMsg::Allocated { ref id, peer_connected: true } if id == "peer-1"),
        "client 应收到 peer_connected=true"
    );

    // 双向透传:host→client
    let payload = b"hello-from-host".to_vec();
    host.write_all(&payload).await.unwrap();
    let mut got = vec![0u8; payload.len()];
    client.read_exact(&mut got).await.unwrap();
    assert_eq!(got, payload, "host→client 透传内容一致");

    // client→host
    let payload2 = b"ping-back".to_vec();
    client.write_all(&payload2).await.unwrap();
    let mut got2 = vec![0u8; payload2.len()];
    host.read_exact(&mut got2).await.unwrap();
    assert_eq!(got2, payload2, "client→host 透传内容一致");

    drop(host);
    drop(client);
    serve.abort();
}

/// TCP 中继:client 先连(等待 host),host 到达后配对。
#[tokio::test]
async fn relay_pipe_client_first_waits_host() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = listener.local_addr().unwrap();
    let serve = tokio::spawn(async move {
        let _ = dcr_server::relay::serve_tcp(listener, None).await;
    });

    // client 先连
    let mut client = TcpStream::connect(relay_addr).await.unwrap();
    write_msg(
        &mut client,
        &RelayMsg::Allocate {
            id: "peer-2".into(),
            role: "client".into(),
        },
    )
    .await
    .unwrap();

    // 稍等 client 进入等待,再让 host 接入
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let mut host = TcpStream::connect(relay_addr).await.unwrap();
    write_msg(
        &mut host,
        &RelayMsg::Allocate {
            id: "peer-2".into(),
            role: "host".into(),
        },
    )
    .await
    .unwrap();
    let _: RelayMsg = read_msg(&mut host).await.unwrap();

    let ack: RelayMsg = read_msg(&mut client).await.unwrap();
    assert!(
        matches!(ack, RelayMsg::Allocated { peer_connected: true, .. }),
        "等待后 client 应配对成功"
    );

    // 透传校验
    host.write_all(b"X").await.unwrap();
    let mut b = [0u8; 1];
    client.read_exact(&mut b).await.unwrap();
    assert_eq!(b, [b'X']);

    drop(client);
    drop(host);
    serve.abort();
}

/// UDP 中继:登记宿主 → 转发载荷到宿主。
#[tokio::test]
async fn relay_udp_forward() {
    let relay = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = relay.local_addr().unwrap();
    let serve = tokio::spawn(async move {
        let _ = dcr_server::relay::serve_udp(relay).await;
    });

    // 宿主
    let host = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let _host_addr = host.local_addr().unwrap();
    host.send_to(r#"{"t":"alloc-udp","id":"u1"}"#.as_bytes(), relay_addr)
        .await
        .unwrap();
    let mut buf = [0u8; 256];
    let (n, _) = tokio::time::timeout(std::time::Duration::from_secs(3), host.recv_from(&mut buf))
        .await
        .expect("登记应答超时")
        .unwrap();
    assert_eq!(&buf[..n], r#"{"t":"allocated"}"#.as_bytes());

    // 发送端发数据
    let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let payload = base64_std(b"video-frame-bytes");
    let msg = format!(r#"{{"t":"data","id":"u1","payload":"{payload}"}}"#);
    sender.send_to(msg.as_bytes(), relay_addr).await.unwrap();

    // 宿主收到原样载荷
    let (n, _) = tokio::time::timeout(std::time::Duration::from_secs(3), host.recv_from(&mut buf))
        .await
        .expect("转发超时")
        .unwrap();
    assert_eq!(&buf[..n], b"video-frame-bytes");

    drop(host);
    drop(sender);
    serve.abort();
}

/// 便捷 base64 编码(测试辅助)。
fn base64_std(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// 中继会话上报 → 信令会话记录(真实 UDP 事件链路)。
/// 中继配对成功后向信令的 UDP 端口上报 session-start,会话结束上报 session-end。
#[tokio::test]
async fn relay_reports_session_to_signal() {
    // 信令侧:模拟 serve 中建立的 SessionCore + UDP STUN socket
    let signal_udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let signal_udp_addr = signal_udp.local_addr().unwrap();
    let probe = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let sessions = std::sync::Arc::new(dcr_server::sessions::SessionCore::new());
    let udp_sessions = sessions.clone();
    let udp_listener = signal_udp;
    tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        loop {
            let (n, src) = udp_listener.recv_from(&mut buf).await.unwrap();
            signal::handle_stun_packet(
                &udp_listener,
                &probe,
                Some(udp_sessions.clone()),
                0,
                buf[..n].to_vec(),
                src,
            )
            .await;
        }
    });

    // 中继侧:开启会话上报到信令的 UDP 地址
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = listener.local_addr().unwrap();
    let serve = tokio::spawn(async move {
        let _ = dcr_server::relay::serve_tcp(listener, Some(signal_udp_addr)).await;
    });

    // host 接入
    let mut host = TcpStream::connect(relay_addr).await.unwrap();
    write_msg(
        &mut host,
        &RelayMsg::Allocate {
            id: "peer-s1".into(),
            role: "host".into(),
        },
    )
    .await
    .unwrap();
    let _: RelayMsg = read_msg(&mut host).await.unwrap();

    // client 接入 → 配对成功 → 上报 session-start
    let mut client = TcpStream::connect(relay_addr).await.unwrap();
    write_msg(
        &mut client,
        &RelayMsg::Allocate {
            id: "peer-s1".into(),
            role: "client".into(),
        },
    )
    .await
    .unwrap();
    let ack: RelayMsg = read_msg(&mut client).await.unwrap();
    assert!(matches!(ack, RelayMsg::Allocated { peer_connected: true, .. }));

    // 等待 UDP 事件到达信令
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while sessions.count() == 0 && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(sessions.count(), 1, "信令应记录到 1 个会话");
    let rec = sessions.list().remove(0);
    assert_eq!(rec.id, "peer-s1");
    assert_eq!(rec.via, "relay");
    assert!(!rec.host.is_empty(), "host 地址应非空");
    assert!(!rec.client.is_empty(), "client 地址应非空");

    // 断开 → 上报 session-end
    drop(client);
    drop(host);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while sessions.count() > 0 && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(sessions.count(), 0, "会话结束应上报并移除");

    serve.abort();
}
