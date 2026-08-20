//! dcr-signal 信令 + STUN + Web 管理后台服务入口。
//!
//! 用法:
//!   dcr-signal [--bind 0.0.0.0] [--port 21116] [--udp-port 21115] [--relay-hint host:port]
//!             [--admin-port 21120] [--admin-ui DIR] [--data-dir DIR] [--admin-pass PASS]
//!
//! - TCP 21116:设备注册/心跳/查找/列表;UDP 21115:RFC 5389 STUN Binding + NAT 探测;
//! - Web 管理后台(默认 http://0.0.0.0:21120):账号登录 + 用户管理 + 在线设备 + React 后台界面。
//! - 账号数据与 JWT 密钥持久化于 `--data-dir`(默认 ./data);首次启动自动创建 admin 账号。

use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;

use tokio::net::{TcpListener, UdpSocket};

use dcr_server::admin::{self, AdminState};
use dcr_server::auth::{AuthState, UserStore};
use dcr_server::signal::SignalCore;

static ARGS: OnceLock<Args> = OnceLock::new();

/// 命令行参数。
struct Args {
    bind: String,
    tcp_port: u16,
    udp_port: u16,
    relay_hint: String,
    admin_port: u16,
    admin_ui: Option<PathBuf>,
    data_dir: PathBuf,
    admin_pass: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut bind = "0.0.0.0".to_string();
    let mut tcp_port: u16 = 21116;
    let mut udp_port: u16 = 21115;
    let mut relay_hint = String::new();
    let mut admin_port: u16 = 21120;
    let mut admin_ui: Option<PathBuf> = None;
    let mut data_dir = PathBuf::from("./data");
    let mut admin_pass: Option<String> = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!(
                    "dcr-signal: 信令 + STUN + Web 管理后台服务\n\
                     用法: dcr-signal [--bind IP] [--port TCP] [--udp-port UDP] [--relay-hint host:port]\n\
                           [--admin-port PORT] [--admin-ui DIR] [--data-dir DIR] [--admin-pass PASS]\n\
                     默认: --bind 0.0.0.0 --port 21116 --udp-port 21115 --admin-port 21120 --data-dir ./data\n\
                     首次启动自动创建 admin 账号(--admin-pass 指定密码,缺省随机生成并打印到日志)"
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
            "--admin-port" => {
                admin_port = args
                    .next()
                    .ok_or("--admin-port 缺少参数")?
                    .parse()
                    .map_err(|e| format!("--admin-port 解析失败: {e}"))?
            }
            "--admin-ui" => admin_ui = Some(PathBuf::from(args.next().ok_or("--admin-ui 缺少参数")?)),
            "--data-dir" => data_dir = PathBuf::from(args.next().ok_or("--data-dir 缺少参数")?),
            "--admin-pass" => admin_pass = Some(args.next().ok_or("--admin-pass 缺少参数")?),
            other => return Err(format!("未知参数: {other}")),
        }
    }
    Ok(Args {
        bind,
        tcp_port,
        udp_port,
        relay_hint,
        admin_port,
        admin_ui,
        data_dir,
        admin_pass,
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

    // 账号与认证:用户存储 + JWT 密钥(持久化到 data-dir)
    let store = Arc::new(UserStore::new(&bind.data_dir));
    if let Some(pw) = store.ensure_bootstrap(bind.admin_pass.as_deref()) {
        if bind.admin_pass.is_none() {
            log::warn!("[main] 首次运行,初始管理员账号 admin,随机密码: {pw}(请立即登录后台修改)");
        } else {
            log::info!("[main] 首次运行,已创建初始管理员账号 admin");
        }
    }
    let secret = dcr_server::auth::load_or_create_secret(&bind.data_dir);
    let auth = AuthState::new(store, secret);

    // 共享信令核心:同时供信令服务与 Web 管理后台(在线设备列表)
    let core = Arc::new(SignalCore::new(&bind.relay_hint));

    // Web 管理后台(独立任务,不阻塞信令)
    let admin_state = AdminState {
        auth,
        core: (*core).clone(),
    };
    let admin_port = bind.admin_port;
    let admin_ui = bind.admin_ui.clone();
    let admin_task = tokio::spawn(async move {
        if let Err(e) = admin::serve(admin_state, admin_ui, admin_port).await {
            log::error!("[main] Web 管理后台退出: {e}");
        }
    });

    log::info!(
        "dcr-signal 启动: TCP {} (信令) / UDP {} (STUN) / 管理后台 http://0.0.0.0:{} / relay_hint={}",
        listener.local_addr().unwrap(),
        udp_socket.local_addr().unwrap(),
        admin_port,
        if bind.relay_hint.is_empty() {
            "(无)"
        } else {
            &bind.relay_hint
        }
    );

    if let Err(e) = dcr_server::signal::serve(listener, udp_socket, core).await {
        log::error!("dcr-signal 退出: {e}");
        admin_task.abort();
        std::process::exit(1);
    }
}