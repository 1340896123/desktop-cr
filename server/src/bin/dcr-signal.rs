//! dcr-signal 信令 + STUN 服务入口。
//!
//! 用法:
//!   dcr-signal [--bind 0.0.0.0] [--port 21116] [--udp-port 21115] [--relay-hint host:port]
//!
//! TCP 21116:设备注册/心跳/查找/列表;UDP 21115:RFC 5389 STUN Binding + NAT 探测。

use std::env;
use std::net::SocketAddr;
use std::sync::OnceLock;

use tokio::net::{TcpListener, UdpSocket};

static ARGS: OnceLock<Args> = OnceLock::new();

/// 命令行参数。
struct Args {
    bind: String,
    tcp_port: u16,
    udp_port: u16,
    relay_hint: String,
}

fn parse_args() -> Result<Args, String> {
    let mut bind = "0.0.0.0".to_string();
    let mut tcp_port: u16 = 21116;
    let mut udp_port: u16 = 21115;
    let mut relay_hint = String::new();
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!(
                    "dcr-signal: 信令 + STUN 服务\n\
                     用法: dcr-signal [--bind IP] [--port TCP] [--udp-port UDP] [--relay-hint host:port]\n\
                     默认: --bind 0.0.0.0 --port 21116 --udp-port 21115"
                );
                std::process::exit(0);
            }
            "--bind" => bind = args.next().ok_or("--bind 缺少参数")?,
            "--port" => {
                tcp_port = args
                    .next()
                    .ok_or("--port 缺少参数")?
                    .parse()
                    .map_err(|e| format!("--port 解析失败: {e}"))?
            }
            "--udp-port" => {
                udp_port = args
                    .next()
                    .ok_or("--udp-port 缺少参数")?
                    .parse()
                    .map_err(|e| format!("--udp-port 解析失败: {e}"))?
            }
            "--relay-hint" => relay_hint = args.next().ok_or("--relay-hint 缺少参数")?,
            other => return Err(format!("未知参数: {other}")),
        }
    }
    Ok(Args {
        bind,
        tcp_port,
        udp_port,
        relay_hint,
    })
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("参数错误: {e}");
            std::process::exit(2);
        }
    };
    let _ = ARGS.set(args);

    let bind = ARGS.get().unwrap();
    let tcp_addr: SocketAddr = format!("{}:{}", bind.bind, bind.tcp_port)
        .parse::<SocketAddr>()
        .map_err(|e| e.to_string())
        .unwrap_or_else(|e| {
            eprintln!("地址解析失败: {e}");
            std::process::exit(2)
        });
    let udp_addr: SocketAddr = format!("{}:{}", bind.bind, bind.udp_port)
        .parse::<SocketAddr>()
        .unwrap_or_else(|e| {
            eprintln!("地址解析失败: {e}");
            std::process::exit(2)
        });

    let listener = TcpListener::bind(tcp_addr).await.unwrap_or_else(|e| {
        eprintln!("TCP 监听 {tcp_addr} 失败: {e}");
        std::process::exit(1)
    });
    let udp_socket = UdpSocket::bind(udp_addr).await.unwrap_or_else(|e| {
        eprintln!("UDP 监听 {udp_addr} 失败: {e}");
        std::process::exit(1)
    });

    log::info!(
        "dcr-signal 启动: TCP {} (信令) / UDP {} (STUN) / relay_hint={}",
        listener.local_addr().unwrap(),
        udp_socket.local_addr().unwrap(),
        if bind.relay_hint.is_empty() {
            "(无)"
        } else {
            &bind.relay_hint
        }
    );

    if let Err(e) = dcr_server::signal::serve(listener, udp_socket, &bind.relay_hint).await {
        log::error!("dcr-signal 退出: {e}");
        std::process::exit(1);
    }
}
