//! 错误日志（v3）：按天目录归档，只保留最近 7 天。
//!
//! 目录结构：`<app_data>/logs/YYYY-MM-DD/errors.log`，每条错误一行：
//! `2026-11-14 09:30:12 [scope] message`。
//!
//! 约定：
//! - 时间统一按 UTC 计算日期，避免引入时区依赖（同一台机器上顺序仍然单调）；
//! - 启动时清理超过保留期的日期目录，只处理能解析成日期的目录，避免误删其它数据；
//! - 「是否展示日志入口」由目录里**是否存在非空日志文件**决定（保留 7 天，重启后仍显示）。

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// 保留的日志天数（含今天）。
pub const KEEP_DAYS: i64 = 7;

/// 一天的毫秒数。
const DAY_MS: i64 = 86_400_000;

/// 错误日志存储（进程内单例，由 `AppState` 持有）。
#[derive(Debug)]
pub struct ErrorLog {
    root: PathBuf,
}

impl ErrorLog {
    /// 打开日志根目录：创建目录并清理过期归档。
    pub fn open(root: PathBuf) -> Self {
        let _ = fs::create_dir_all(&root);
        cleanup_old(&root, crate::storage::now_millis() / DAY_MS, KEEP_DAYS);
        Self { root }
    }

    /// 日志根目录。
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 今天的日志文件路径（`<root>/YYYY-MM-DD/errors.log`）。
    pub fn today_file(&self) -> PathBuf {
        self.root
            .join(day_string(crate::storage::now_millis() / DAY_MS))
            .join("errors.log")
    }

    /// 日志目录里是否已有内容（决定设置页是否展示错误日志入口）。
    pub fn has_logs(&self) -> bool {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return false;
        };
        entries.flatten().any(|entry| {
            let file = entry.path().join("errors.log");
            fs::metadata(&file)
                .map(|meta| meta.len() > 0)
                .unwrap_or(false)
        })
    }

    /// 记录一条错误，返回写入的文件路径。
    pub fn record(&self, scope: &str, message: &str) -> Result<PathBuf, String> {
        let day = crate::storage::now_millis() / DAY_MS;
        let dir = self.root.join(day_string(day));
        fs::create_dir_all(&dir).map_err(|e| format!("创建日志目录失败：{e}"))?;
        let file = dir.join("errors.log");
        let mut handle = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file)
            .map_err(|e| format!("打开日志文件失败：{e}"))?;
        let line = format!(
            "{} [{}] {}\n",
            timestamp_string(crate::storage::now_millis()),
            sanitize(scope),
            sanitize(message)
        );
        handle
            .write_all(line.as_bytes())
            .map_err(|e| format!("写入日志失败：{e}"))?;
        Ok(file)
    }
}

/// 把多行内容压成单行，避免日志被换行打乱。
fn sanitize(text: &str) -> String {
    text.replace(['\r', '\n'], " ").trim().to_string()
}

/// UNIX 天数 → `YYYY-MM-DD`。
fn day_string(days: i64) -> String {
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// 毫秒时间戳 → `YYYY-MM-DD HH:MM:SS`（UTC）。
fn timestamp_string(ms: i64) -> String {
    let days = ms.div_euclid(DAY_MS);
    let millis_of_day = ms.rem_euclid(DAY_MS);
    let (year, month, day) = civil_from_days(days);
    let seconds = millis_of_day / 1000;
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}",
        seconds / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    )
}

/// UNIX 天数 → (年, 月, 日)；Howard Hinnant 的 civil_from_days 算法。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    } as u32;
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

/// `YYYY-MM-DD` → UNIX 天数（无法解析返回 `None`）。
fn parse_day(name: &str) -> Option<i64> {
    let mut parts = name.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month as u32, day as u32))
}

/// (年, 月, 日) → UNIX 天数；`civil_from_days` 的逆运算。
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = if month > 2 { month - 3 } else { month + 9 } as i64;
    let day_of_year = (153 * month_prime + 2) / 5 + day as i64 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// 删除超过保留期的日期目录（只处理能解析成日期的目录名）。
fn cleanup_old(root: &Path, today: i64, keep_days: i64) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let Some(day) = parse_day(name) else {
            continue;
        };
        if today - day >= keep_days {
            let _ = fs::remove_dir_all(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 正常路径：日期格式化与解析互逆（含闰年与跨年）。
    #[test]
    fn day_string_round_trips() {
        for day in [0_i64, 1, 11_016, 20_000, 20_500] {
            let text = day_string(day);
            assert_eq!(parse_day(&text), Some(day), "text = {text}");
        }
        assert_eq!(day_string(0), "1970-01-01");
    }

    /// 边界：非法目录名不参与清理。
    #[test]
    fn parse_day_rejects_invalid_names() {
        assert_eq!(parse_day("logs"), None);
        assert_eq!(parse_day("2026-13-01"), None);
        assert_eq!(parse_day("2026-11-14-extra"), None);
    }

    /// 正常路径：记录错误会写文件，并让 `has_logs` 变为真。
    #[test]
    fn record_writes_line_and_marks_has_logs() {
        let root = temp_root("record");
        let log = ErrorLog::open(root.clone());
        assert!(!log.has_logs());

        let file = log.record("transfer", "boom\nsecond line").expect("record");
        assert!(log.has_logs(), "写入后目录里应存在非空日志文件");
        let text = fs::read_to_string(&file).expect("read log");
        assert!(
            text.contains("[transfer] boom second line"),
            "text = {text}"
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// 边界：启动清理只删除超过 7 天的目录，保留期内与非日期目录不动。
    #[test]
    fn open_cleans_old_directories() {
        let root = temp_root("cleanup");
        fs::create_dir_all(&root).expect("create root");
        let today = crate::storage::now_millis() / DAY_MS;
        let old = root.join(day_string(today - 8));
        let fresh = root.join(day_string(today - 6));
        let other = root.join("keep-me");
        for dir in [&old, &fresh, &other] {
            fs::create_dir_all(dir).expect("create dir");
        }

        let _log = ErrorLog::open(root.clone());

        assert!(!old.exists(), "超过保留期的目录应被删除");
        assert!(fresh.exists(), "保留期内的目录必须保留");
        assert!(other.exists(), "非日期目录不得删除");

        let _ = fs::remove_dir_all(&root);
    }

    /// 生成独立的临时目录（测试结束后由调用方清理）。
    fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("chatters-log-{name}-{}", uuid::Uuid::new_v4()))
    }
}
