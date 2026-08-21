//! 服务端操作日志模块(按 UTC 日期轮转的追加式文件日志 + 内存环形缓冲)。
//!
//! - 日志根目录由主程序入口注册(`register_log_dir`),缺省兜底
//!   `<临时目录>/dcr-server-logs`;日志文件位于 `<根>/logs/operations-YYYYMMDD.log`。
//! - 每行格式:`[2026-08-19T03:00:00.000Z] [module] action detail`(UTC ISO8601 毫秒精度)。
//! - 写入时同时落盘(持久化审计归档)与追加进内存环形缓冲;`read_operation_logs`
//!   直接读取内存缓冲,避免并发读写同一文件在 Windows 下偶发的瞬时打开失败/字节交错。
//! - 启动时(`register_log_dir`)从今天 + 昨天两个文件回填内存缓冲,重启后仍可读到近期记录。
//! - 写失败仅 warn 不 panic,不影响业务逻辑。

use serde::Serialize;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// 测试辅助:全局写日志锁,避免并发测试对共享日志文件的追加写入发生字节交错。
#[cfg(test)]
pub(crate) mod test_lock {
    use std::sync::Mutex;
    pub(crate) static LOG_WRITE_LOCK: Mutex<()> = Mutex::new(());
}

/// 内存缓冲最大保留条数(环形覆盖,仅影响实时读取,不影响落盘)。
const MEM_CAP: usize = 2000;

/// 日志根目录(由主程序入口注册)。
static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();

/// 全局写锁:保证多连接/多任务并发追加写入时,单行(含换行)不被拆段交错,
/// 避免并发交错导致行内容污染。
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// 实时读取用的内存环形缓冲(UTF-8 安全、无文件竞争)。
static MEMORY: Mutex<VecDeque<OperationLogEntry>> = Mutex::new(VecDeque::new());

/// 操作日志条目(供管理后台读取,字段 camelCase)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationLogEntry {
    pub time: String,
    pub module: String,
    pub action: String,
    pub detail: String,
}

/// 注册日志根目录(主程序入口调用,如 dcr-signal 的 data-dir)。
/// 同时回填今天 + 昨天的磁盘日志到内存缓冲,使重启后仍能读到近期记录。
pub fn register_log_dir(dir: PathBuf) {
    let is_new = LOG_DIR.get().is_none();
    if is_new {
        let _ = LOG_DIR.set(dir);
        // 仅在首次注册(进程启动)时回填;OnceLock 保证仅执行一次
        let (secs, _) = utc_now();
        for day in [secs, secs - 86400] {
            let path = log_root()
                .join("logs")
                .join(format!("operations-{}.log", date_stamp(day)));
            if let Ok(content) = std::fs::read_to_string(&path) {
                let mut mem = MEMORY.lock().unwrap_or_else(|e| e.into_inner());
                for line in content.lines() {
                    if let Some(entry) = parse_line(line) {
                        mem.push_back(entry);
                    }
                }
                while mem.len() > MEM_CAP {
                    mem.pop_front();
                }
            }
        }
    }
}

/// 兜底日志根目录:未注册时使用系统临时目录下的 dcr-server-logs。
fn default_log_dir() -> PathBuf {
    std::env::temp_dir().join("dcr-server-logs")
}

/// 当前日志根目录(已注册目录或兜底目录)。
fn log_root() -> PathBuf {
    LOG_DIR.get().cloned().unwrap_or_else(default_log_dir)
}

/// Unix 秒 → 公历日期 (y, m, d)(Howard Hinnant civil_from_days 算法)。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Unix 秒 → UTC ISO8601 字符串(毫秒精度,如 2026-08-19T03:00:00.000Z)。
fn format_utc(secs: i64, millis: u32) -> String {
    let (y, m, d) = civil_from_days(secs.div_euclid(86400));
    let rem = secs.rem_euclid(86400);
    let h = rem / 3600;
    let min = (rem % 3600) / 60;
    let s = rem % 60;
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}.{millis:03}Z")
}

/// Unix 秒 → UTC 日期戳(用于日志文件名,如 20260819)。
fn date_stamp(secs: i64) -> String {
    let (y, m, d) = civil_from_days(secs.div_euclid(86400));
    format!("{y:04}{m:02}{d:02}")
}

/// 当前 UTC 时间:(unix 秒, 毫秒)。
fn utc_now() -> (i64, u32) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    (now.as_secs() as i64, now.subsec_millis())
}

/// 追加一条操作日志(写失败仅 warn,不 panic)。
///
/// 同时:写入内存环形缓冲(供 `read_operation_logs` 实时读取)与按日期轮转的文件
/// (持久化审计归档)。行格式:`[2026-08-19T03:00:00.000Z] [module] action detail`。
pub fn op_log(module: &str, action: &str, detail: &str) {
    let (secs, millis) = utc_now();
    let ts = format_utc(secs, millis);
    let body = if detail.is_empty() {
        action.to_string()
    } else {
        format!("{action} {detail}")
    };
    let line = format!("[{ts}] [{module}] {body}");
    log::info!("[{module}] {body}");

    let entry = OperationLogEntry {
        time: ts,
        module: module.to_string(),
        action: action.to_string(),
        detail: detail.to_string(),
    };

    // 写入内存环形缓冲(实时读取来源,无文件竞争)
    {
        let mut mem = MEMORY.lock().unwrap_or_else(|e| e.into_inner());
        mem.push_back(entry);
        while mem.len() > MEM_CAP {
            mem.pop_front();
        }
    }

    // 串行化落盘:整行(含换行)作为一个原子单元写入文件(持久化审计)
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = log_root().join("logs");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("[operation_log] 创建日志目录失败: {e}");
        return;
    }
    let path = dir.join(format!("operations-{}.log", date_stamp(secs)));
    use std::io::Write;
    match std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)
    {
        Ok(mut f) => {
            if let Err(e) = writeln!(f, "{line}") {
                log::warn!("[operation_log] 写入日志失败({}): {e}", path.display());
            }
        }
        Err(e) => log::warn!("[operation_log] 打开日志文件失败({}): {e}", path.display()),
    }
}

/// 解析单行日志为条目(格式不匹配返回 None)。
fn parse_line(line: &str) -> Option<OperationLogEntry> {
    let rest = line.trim_end().strip_prefix('[')?;
    let end = rest.find(']')?;
    let time = rest[..end].to_string();
    let rest = rest[end + 1..].trim_start();
    let rest = rest.strip_prefix('[')?;
    let end = rest.find(']')?;
    let module = rest[..end].to_string();
    let rest = rest[end + 1..].trim_start();
    let (action, detail) = match rest.find(char::is_whitespace) {
        Some(i) => (rest[..i].to_string(), rest[i..].trim_start().to_string()),
        None => (rest.to_string(), String::new()),
    };
    if time.is_empty() || module.is_empty() || action.is_empty() {
        return None;
    }
    Some(OperationLogEntry {
        time,
        module,
        action,
        detail,
    })
}

/// 读取操作日志:返回内存环形缓冲中的条目,最新在前,最多 limit 条。
///
/// 内存缓冲在 `register_log_dir` 时已从磁盘回填今天 + 昨天记录,故重启后仍可读到近期日志。
pub fn read_operation_logs(limit: usize) -> Vec<OperationLogEntry> {
    let mut entries: Vec<OperationLogEntry> = {
        let mem = MEMORY.lock().unwrap_or_else(|e| e.into_inner());
        mem.iter().cloned().collect()
    };
    // ISO8601(毫秒精度)字符串按字典序即时间序,降序 = 最新在前
    entries.sort_by(|a, b| b.time.cmp(&a.time));
    entries.truncate(limit);
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_log_roundtrip() {
        // 唯一临时子目录,避免与其他测试/真实日志冲突
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("dcr-server-oplog-{stamp}"));
        register_log_dir(dir.clone());

        // 持锁期间写入+读取,避免与其他触发 op_log 的测试并发写同一文件
        let _guard = test_lock::LOG_WRITE_LOCK.lock().unwrap();

        op_log("testmod", "action1", "detail one");
        // 保证时间戳严格递增,以便验证"最新在前"
        std::thread::sleep(std::time::Duration::from_millis(2));
        op_log("testmod", "action2", "");
        std::thread::sleep(std::time::Duration::from_millis(2));
        op_log("testmod", "action3", "detail three");

        // 其他测试可能并发写日志到同一目录,按本测试专属 module 过滤。
        // 用足够大的 limit 读取,避免 testmod(较早写入)被最新条目的截断丢弃。
        let logs: Vec<OperationLogEntry> = read_operation_logs(1_000_000)
            .into_iter()
            .filter(|e| e.module == "testmod")
            .collect();
        assert_eq!(logs.len(), 3);
        // 最新在前
        assert_eq!(logs[0].module, "testmod");
        assert_eq!(logs[0].action, "action3");
        assert_eq!(logs[0].detail, "detail three");
        assert_eq!(logs[1].module, "testmod");
        assert_eq!(logs[1].action, "action2");
        assert_eq!(logs[1].detail, "");
        assert_eq!(logs[2].module, "testmod");
        assert_eq!(logs[2].action, "action1");
        assert_eq!(logs[2].detail, "detail one");
        // 时间戳为 UTC ISO8601 毫秒格式
        for e in &logs {
            assert!(e.time.contains('T'));
            assert!(e.time.ends_with('Z'));
        }

        // 清理临时目录
        let _ = std::fs::remove_dir_all(&dir);
    }
}
