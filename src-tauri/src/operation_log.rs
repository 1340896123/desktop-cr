//! 操作日志模块(按 UTC 日期轮转的追加式文件日志)。
//!
//! - 日志根目录由 main.rs setup 注册(`app_config_dir`),未注册时兜底
//!   `%APPDATA%/<identifier>`;日志文件位于 `<根>/logs/operations-YYYYMMDD.log`。
//! - 每行格式:`[2026-08-19T03:00:00.000Z] [module] action detail`(UTC ISO8601 毫秒精度)。
//! - `read_operation_logs` 读取今天 + 昨天两个文件,最新在前,最多 limit 条。
//! - 写失败仅 warn 不 panic,不影响业务逻辑。

use serde::Serialize;
use std::path::PathBuf;
use std::sync::OnceLock;

/// 测试辅助:全局写日志锁,避免并发测试对共享日志文件的追加写入发生字节交错
/// (Windows 下两个线程对同一文件的 append 写入可能被拆分成段,污染行内容)。
#[cfg(test)]
pub(crate) mod test_lock {
    use std::sync::Mutex;
    pub(crate) static LOG_WRITE_LOCK: Mutex<()> = Mutex::new(());
}

/// Tauri 应用标识(与 hbb_client 保持一致,用于兜底日志目录解析)。
const APP_IDENTIFIER: &str = "com.example.winui-remote-desktop";

/// 日志根目录(由 main.rs setup 注册)。
static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();

/// 操作日志条目(供前端读取,字段 camelCase)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationLogEntry {
    pub time: String,
    pub module: String,
    pub action: String,
    pub detail: String,
}

/// 注册日志根目录(main.rs setup 中调用)。
pub fn register_log_dir(dir: PathBuf) {
    let _ = LOG_DIR.set(dir);
}

/// 兜底日志根目录:未注册时按 Tauri 规则解析 %APPDATA%/<identifier>(Windows)。
fn default_log_dir() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    base.join(APP_IDENTIFIER)
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
/// 行格式:`[2026-08-19T03:00:00.000Z] [module] action detail`。
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

    let dir = log_root().join("logs");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("[operation_log] 创建日志目录失败: {e}");
        return;
    }
    let path = dir.join(format!("operations-{}.log", date_stamp(secs)));
    match std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)
    {
        Ok(mut f) => {
            use std::io::Write;
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

/// 读取指定 UTC 日期文件中的日志条目。
fn read_date_log(secs: i64) -> Vec<OperationLogEntry> {
    let path = log_root()
        .join("logs")
        .join(format!("operations-{}.log", date_stamp(secs)));
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    content.lines().filter_map(parse_line).collect()
}

/// 读取操作日志:今天 + 昨天两个文件,行解析,最新在前,最多 limit 条。
pub fn read_operation_logs(limit: usize) -> Vec<OperationLogEntry> {
    let (secs, _) = utc_now();
    let mut entries = read_date_log(secs);
    entries.extend(read_date_log(secs - 86400));
    // ISO8601(毫秒精度)字符串按字典序即时间序,降序 = 最新在前
    entries.sort_by(|a, b| b.time.cmp(&a.time));
    entries.truncate(limit);
    entries
}

/// 查询操作日志(Tauri 命令,默认返回最近 100 条)。
#[tauri::command]
pub fn get_operation_logs(limit: Option<usize>) -> Vec<OperationLogEntry> {
    read_operation_logs(limit.unwrap_or(100).max(1))
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
        let dir = std::env::temp_dir().join(format!("desktop-cr-oplog-{stamp}"));
        register_log_dir(dir.clone());

        // 持锁期间写入+读取,避免与其他触发 op_log 的测试并发写同一文件
        let _guard = test_lock::LOG_WRITE_LOCK.lock().unwrap();

        op_log("testmod", "action1", "detail one");
        // 保证时间戳严格递增,以便验证"最新在前"
        std::thread::sleep(std::time::Duration::from_millis(2));
        op_log("testmod", "action2", "");
        std::thread::sleep(std::time::Duration::from_millis(2));
        op_log("testmod", "action3", "detail three");

        // 其他测试可能并发写日志到同一目录,按本测试专属 module 过滤
        let logs: Vec<OperationLogEntry> = read_operation_logs(10)
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
