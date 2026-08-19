//! TCP 长度前缀消息帧。
//!
//! 帧格式:4 字节小端 u32 长度(不含长度字段本身)+ serde_json 序列化字节。
//! 单条消息长度上限 4MB,超限或为 0 视为非法。

use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// 单条消息长度上限:4MB。
const MAX_MSG_LEN: u32 = 4 * 1024 * 1024;

/// 将 `v` 序列化后按长度前缀帧写入流 `w`。
pub async fn write_msg<W: AsyncWrite + Unpin>(w: &mut W, v: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec(v).map_err(|e| format!("消息序列化失败: {e}"))?;
    let len = bytes.len() as u32;
    if len > MAX_MSG_LEN {
        return Err(format!("消息过大: {len} 字节(上限 {MAX_MSG_LEN} 字节)"));
    }
    w.write_all(&len.to_le_bytes())
        .await
        .map_err(|e| format!("写入长度前缀失败: {e}"))?;
    w.write_all(&bytes)
        .await
        .map_err(|e| format!("写入消息体失败: {e}"))?;
    Ok(())
}

/// 从流 `r` 读取一帧并反序列化为 `T`。
///
/// 长度字段为 0 或超过 4MB 时返回错误;EOF 视为连接关闭错误。
pub async fn read_msg<R: AsyncRead + Unpin, T: DeserializeOwned>(r: &mut R) -> Result<T, String> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)
        .await
        .map_err(|e| format!("读取长度前缀失败: {e}"))?;
    let len = u32::from_le_bytes(len_buf);
    if len == 0 || len > MAX_MSG_LEN {
        return Err(format!("非法消息长度: {len}"));
    }
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf)
        .await
        .map_err(|e| format!("读取消息体失败: {e}"))?;
    serde_json::from_slice(&buf).map_err(|e| format!("消息反序列化失败: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct TestMsg {
        t: String,
        n: u32,
    }

    /// write 后 read,验证帧往返一致。
    #[tokio::test]
    async fn write_read_roundtrip() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        let msg = TestMsg {
            t: "ping".to_string(),
            n: 42,
        };
        write_msg(&mut a, &msg).await.unwrap();
        let got: TestMsg = read_msg(&mut b).await.unwrap();
        assert_eq!(msg, got);
    }

    /// 长度字段为 0 应报错。
    #[tokio::test]
    async fn zero_length_rejected() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        a.write_all(&0u32.to_le_bytes()).await.unwrap();
        let res = read_msg::<_, TestMsg>(&mut b).await;
        assert!(res.is_err());
    }

    /// 长度字段超过 4MB 应报错。
    #[tokio::test]
    async fn oversized_length_rejected() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        a.write_all(&(MAX_MSG_LEN + 1).to_le_bytes()).await.unwrap();
        let res = read_msg::<_, TestMsg>(&mut b).await;
        assert!(res.is_err());
    }
}
