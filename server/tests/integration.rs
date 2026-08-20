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
        },
    )
    .await
    .unwrap();
    let ack: SignalMsg = read_msg(&mut a).await.unwrap();
    match ack {
        SignalMsg::RegisterAck { ok, .. } => assert!(ok, "注册应成功"),
        other => panic!("期望 RegisterAck,得到 {other:?}"),
    }

    // 客户端 A:查找自己
    write_msg(&mut a, &SignalMsg::Lookup { id: "pc-a".into() })
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
    write_msg(&mut b, &SignalMsg::Lookup { id: "pc-a".into() })
        .await
        .unwrap();
    let ack: SignalMsg = read_msg(&mut b).await.unwrap();
    match ack {
        SignalMsg::LookupAck { online, .. } => assert!(online, "B 应能查到 A"),
        other => panic!("期望 LookupAck,得到 {other:?}"),
    }

    // 列表
    write_msg(&mut b, &SignalMsg::List).await.unwrap();
    let ack: SignalMsg = read_msg(&mut b).await.unwrap();
    match ack {
        SignalMsg::ListAck { peers } => {
            assert!(peers.iter().any(|p| p.id == "pc-a"), "列表应含 pc-a");
        }
        other => panic!("期望 ListAck,得到 {other:?}"),
    }

    // 注销后 B 查不到
    write_msg(&mut a, &SignalMsg::Unregister { id: "pc-a".into() })
        .await
        .unwrap();
    let _: SignalMsg = read_msg(&mut a).await.unwrap(); // register-ack
    write_msg(&mut b, &SignalMsg::Lookup { id: "pc-a".into() })
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
    write_msg(&mut c, &SignalMsg::Lookup { id: "ghost".into() })
        .await
        .unwrap();
    let ack: SignalMsg = read_msg(&mut c).await.unwrap();
    match ack {
        SignalMsg::LookupAck { online, .. } => assert!(!online),
        other => panic!("期望 LookupAck,得到 {other:?}"),
    }
    serve.abort();
}

/// STUN Binding:标准请求 → 响应,还原地址等于客户端真实地址。
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
