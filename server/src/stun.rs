//! RFC 5389 二进制 STUN 编解码(纯函数,无 IO)。
//!
//! 报文头固定 20 字节:2 字节消息类型 + 2 字节长度(属性区长度)+ 4 字节 magic cookie
//! (0x2112A442)+ 12 字节 transaction id。本模块目前支持 Binding Request 解析
//! 与 Binding Response(XOR-MAPPED-ADDRESS 属性)构造/解析。

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// STUN magic cookie(网络字节序 0x2112A442)。
pub const STUN_MAGIC_COOKIE: u32 = 0x2112_A442;
/// Binding Request 消息类型。
pub const BINDING_REQUEST: u16 = 0x0001;
/// Binding Response 消息类型。
pub const BINDING_RESPONSE: u16 = 0x0101;
/// XOR-MAPPED-ADDRESS 属性类型。
pub const ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;
/// 地址族:IPv4。
pub const ATTR_FAMILY_IPV4: u8 = 0x01;
/// 地址族:IPv6。
pub const ATTR_FAMILY_IPV6: u8 = 0x02;

/// 报文头长度。
const HEADER_LEN: usize = 20;
/// XOR 端口使用的掩码:port ^ (magic cookie 高 16 位)。
const XOR_PORT_MASK: u16 = (STUN_MAGIC_COOKIE >> 16) as u16;

/// 解析 Binding Request,校验长度、消息类型与 magic cookie,返回 12 字节 transaction id。
pub fn parse_binding_request(bytes: &[u8]) -> Result<[u8; 12], String> {
    if bytes.len() < HEADER_LEN {
        return Err(format!("报文过短: {} 字节(至少需要 {HEADER_LEN})", bytes.len()));
    }
    let mtype = u16::from_be_bytes([bytes[0], bytes[1]]);
    if mtype != BINDING_REQUEST {
        return Err(format!("不是 Binding Request(type=0x{mtype:04x})"));
    }
    let cookie = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    if cookie != STUN_MAGIC_COOKIE {
        return Err(format!("magic cookie 不匹配(0x{cookie:08x})"));
    }
    let mut txn = [0u8; 12];
    txn.copy_from_slice(&bytes[8..HEADER_LEN]);
    Ok(txn)
}

/// 构造 Binding Response,携带 XOR-MAPPED-ADDRESS 属性(源地址 `source`)。
///
/// - XOR 端口 = port ^ (magic cookie >> 16);
/// - XOR IPv4 地址 = 地址 4 字节各与 magic cookie 4 字节 XOR;
/// - XOR IPv6 地址 = 地址 16 字节各与 (magic cookie 4 字节 + transaction id 12 字节) XOR;
/// - 属性区按 4 字节对齐补 0。
pub fn build_binding_response(txn_id: &[u8; 12], source: SocketAddr) -> Result<Vec<u8>, String> {
    let (family, xored_addr) = match source.ip() {
        IpAddr::V4(addr) => {
            let cookie = STUN_MAGIC_COOKIE.to_be_bytes();
            let bytes = addr.octets();
            let mut xored = [0u8; 4];
            for i in 0..4 {
                xored[i] = bytes[i] ^ cookie[i];
            }
            (ATTR_FAMILY_IPV4, xored.to_vec())
        }
        IpAddr::V6(addr) => {
            // XOR 密钥 = magic cookie 4 字节 + transaction id 12 字节,共 16 字节。
            let mut key = [0u8; 16];
            key[0..4].copy_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
            key[4..16].copy_from_slice(txn_id);
            let bytes = addr.octets();
            let mut xored = [0u8; 16];
            for i in 0..16 {
                xored[i] = bytes[i] ^ key[i];
            }
            (ATTR_FAMILY_IPV6, xored.to_vec())
        }
    };

    // 属性值:1 字节保留 + 1 字节地址族 + 2 字节 XOR 端口 + 地址字节。
    let attr_value_len = 4 + xored_addr.len();
    // 报文头长度字段 = 属性区总长(含属性自身的 4 字节头),不含补零。
    let header_len = 4 + attr_value_len;
    let padding = (4 - (attr_value_len % 4)) % 4;
    let msg_len = HEADER_LEN + header_len + padding;

    let mut out = Vec::with_capacity(msg_len);
    out.extend_from_slice(&BINDING_RESPONSE.to_be_bytes());
    out.extend_from_slice(&(header_len as u16).to_be_bytes());
    out.extend_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
    out.extend_from_slice(txn_id);
    out.extend_from_slice(&ATTR_XOR_MAPPED_ADDRESS.to_be_bytes());
    out.extend_from_slice(&(attr_value_len as u16).to_be_bytes());
    out.push(0x00); // 保留字节
    out.push(family);
    out.extend_from_slice(&(source.port() ^ XOR_PORT_MASK).to_be_bytes());
    out.extend_from_slice(&xored_addr);
    out.resize(out.len() + padding, 0); // 4 字节对齐补 0
    Ok(out)
}

/// 解析 Binding Response,返回还原后的真实 `(端口, 地址)`(内部完成 XOR 还原)。
pub fn parse_binding_response(bytes: &[u8]) -> Result<(u16, IpAddr), String> {
    if bytes.len() < HEADER_LEN {
        return Err(format!("报文过短: {} 字节(至少需要 {HEADER_LEN})", bytes.len()));
    }
    let mtype = u16::from_be_bytes([bytes[0], bytes[1]]);
    if mtype != BINDING_RESPONSE {
        return Err(format!("不是 Binding Response(type=0x{mtype:04x})"));
    }
    let cookie = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    if cookie != STUN_MAGIC_COOKIE {
        return Err(format!("magic cookie 不匹配(0x{cookie:08x})"));
    }
    let txn = &bytes[8..HEADER_LEN];
    let header_len = u16::from_be_bytes([bytes[2], bytes[3]]) as usize;
    let end = (HEADER_LEN + header_len).min(bytes.len());

    // 遍历属性区查找 XOR-MAPPED-ADDRESS。
    let mut pos = HEADER_LEN;
    while pos + 4 <= end {
        let atype = u16::from_be_bytes([bytes[pos], bytes[pos + 1]]);
        let alen = u16::from_be_bytes([bytes[pos + 2], bytes[pos + 3]]) as usize;
        if pos + 4 + alen > end {
            return Err("属性长度越界".into());
        }
        let value = &bytes[pos + 4..pos + 4 + alen];
        if atype == ATTR_XOR_MAPPED_ADDRESS {
            if value.len() < 8 {
                return Err("XOR-MAPPED-ADDRESS 属性过短".into());
            }
            let family = value[1];
            let x_port = u16::from_be_bytes([value[2], value[3]]);
            let port = x_port ^ XOR_PORT_MASK;
            let ip = match family {
                ATTR_FAMILY_IPV4 => {
                    if value.len() < 8 {
                        return Err("IPv4 XOR-MAPPED-ADDRESS 属性过短".into());
                    }
                    let cookie = STUN_MAGIC_COOKIE.to_be_bytes();
                    let mut addr = [0u8; 4];
                    for i in 0..4 {
                        addr[i] = value[4 + i] ^ cookie[i];
                    }
                    IpAddr::V4(Ipv4Addr::from(addr))
                }
                ATTR_FAMILY_IPV6 => {
                    if value.len() < 20 {
                        return Err("IPv6 XOR-MAPPED-ADDRESS 属性过短".into());
                    }
                    let mut key = [0u8; 16];
                    key[0..4].copy_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
                    key[4..16].copy_from_slice(txn);
                    let mut addr = [0u8; 16];
                    for i in 0..16 {
                        addr[i] = value[4 + i] ^ key[i];
                    }
                    IpAddr::V6(Ipv6Addr::from(addr))
                }
                other => return Err(format!("未知地址族: {other}")),
            };
            return Ok((port, ip));
        }
        let step = 4 + alen + ((4 - (alen % 4)) % 4);
        pos += step;
    }
    Err("未找到 XOR-MAPPED-ADDRESS 属性".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// IPv4:构造 response 后解析,还原源地址一致。
    #[test]
    fn ipv4_roundtrip() {
        let src = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 33)), 5555);
        let txn = [7u8; 12];
        let resp = build_binding_response(&txn, src).unwrap();
        assert_eq!(resp.len() % 4, 0, "报文总长应按 4 字节对齐");
        let (port, ip) = parse_binding_response(&resp).unwrap();
        assert_eq!(port, 5555);
        assert_eq!(ip, src.ip());
    }

    /// IPv6:构造 response 后解析,还原源地址一致。
    #[test]
    fn ipv6_roundtrip() {
        let src = SocketAddr::new(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1).into(), 9000);
        let txn = [0xABu8; 12];
        let resp = build_binding_response(&txn, src).unwrap();
        assert_eq!(resp.len() % 4, 0, "报文总长应按 4 字节对齐");
        let (port, ip) = parse_binding_response(&resp).unwrap();
        assert_eq!(port, 9000);
        assert_eq!(ip, src.ip());
    }

    /// 构造 Binding Request,断言解析成功且 transaction id 一致。
    #[test]
    fn parse_request_ok() {
        let txn: [u8; 12] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let mut req = vec![0u8; HEADER_LEN];
        req[0..2].copy_from_slice(&BINDING_REQUEST.to_be_bytes());
        req[2..4].copy_from_slice(&0u16.to_be_bytes()); // 无属性
        req[4..8].copy_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
        req[8..HEADER_LEN].copy_from_slice(&txn);
        let got = parse_binding_request(&req).unwrap();
        assert_eq!(txn, got);
    }

    /// 类型/长度/magic cookie 非法的请求应报错。
    #[test]
    fn parse_request_rejects_bad_input() {
        assert!(parse_binding_request(&[0u8; 10]).is_err(), "过短应报错");
        let mut req = vec![0u8; HEADER_LEN];
        req[0..2].copy_from_slice(&BINDING_RESPONSE.to_be_bytes()); // 类型错误
        req[4..8].copy_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
        assert!(parse_binding_request(&req).is_err(), "类型错误应报错");
        let mut req = vec![0u8; HEADER_LEN];
        req[0..2].copy_from_slice(&BINDING_REQUEST.to_be_bytes());
        req[4..8].copy_from_slice(&0u32.to_be_bytes()); // cookie 错误
        assert!(parse_binding_request(&req).is_err(), "cookie 错误应报错");
    }
}
