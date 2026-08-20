//! dcr-relay TURN-like 中继服务入口。
//!
//! 用法:
//!   dcr-relay [--bind 0.0.0.0] [--port 21117] [--udp-port 21119] [--report-to host:port]
//!
//! TCP 21117:通道分配 + 双向字节透明转发;UDP 21119:数据报转发。
//! `--report-to` 指定 dcr-signal 的 UDP 地址(默认 21115),配对/断开时上报
//! 会话事件供管理后台会话监控;缺省不上报。

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
    /// 会话上报目标(dcr-signal 的 UDP 地址)。
    report_to: Option<SocketAddr>,
}

fn parse_args() -> Result<Args, String> {
    let mut bind = "0.0.0.0".to_string();
    let mut tcp_port: u16 = 21117;
    let mut udp_port: u16 = 21119;
    let mut report_to: Option<SocketAddr> = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!(
                    "dcr-relay: TURN-like 中继服务\n\
                     用法: dcr-relay [--bind IP] [--port TCP] [--udp-port UDP] [--report-to host:port]\n\
                     默认: --bind 0.0.0.0 --port 21117 --udp-port 21119\n\
                     --report-to 指定 dcr-signal 的 UDP 地址(如 127.0.0.1:21115),上报会话事件用于后台监控"
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
            "--report-to" => {
                let addr = args.next().ok_or("--report-to 缺少参数")?;
                report_to = Some(addr.parse().map_err(|e| format!("--report-to 解析失败: {e}"))?);
            }
            other => return Err(format!("未知参数: {other}")),
        }
    }
    Ok(Args {
        bind,
        tcp_port,
        udp_port,
        report_to,
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
        .parse()
        .unwrap_or_else(|e| {
            eprintln!("地址解析失败: {e}");
            std::process::exit(2)
        });
    let udp_addr: SocketAddr = format!("{}:{}", bind.bind, bind.udp_port)
        .parse()
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

    let report_to = bind.report_to;
    log::info!(
        "dcr-relay 启动: TCP {} (字节中继) / UDP {} (数据报中继) / 会话上报: {}",
        listener.local_addr().unwrap(),
        udp_socket.local_addr().unwrap(),
        report_to.map(|a| a.to_string()).unwrap_or_else(|| "(无)".into())
    );

    // UDP 与 TCP 中继并行
    let tcp_listener = listener;
    let udp_task = tokio::spawn(dcr_server::relay::serve_udp(udp_socket));
    if let Err(e) = dcr_server::relay::serve_tcp(tcp_listener, report_to).await {
        log::error!("dcr-relay TCP 服务退出: {e}");
        udp_task.abort();
        std::process::exit(1);
    }
}
