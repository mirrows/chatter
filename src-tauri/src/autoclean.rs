//! 定时清理（v3）：启动时删除接收目录中超过保留天数的文件。
//!
//! 范围：
//! - 接收目录**第一层文件**（用户在应用里可见的下载结果）；
//! - `<接收目录>/.chatters-partial/` 下的过期残留（未完成的 `.part` 与 `.json`）。
//!
//! 只按文件修改时间判断是否过期；子目录本身保留。删除结果会带上路径集合，
//! 供上层同步清理对应的历史传输记录，避免留下"点开必失败"的记录。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// 部分文件子目录名（与 `transfer::receiver` 保持一致）。
const PARTIAL_DIR: &str = ".chatters-partial";

/// 清理结果。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CleanReport {
    /// 删除的文件数。
    pub files: usize,
    /// 删除的字节数。
    pub bytes: u64,
    /// 被删除文件的绝对路径（用于同步清理历史记录）。
    pub removed: HashSet<PathBuf>,
}

/// 清理接收目录中超过 `days` 天未修改的文件。
pub async fn clean(dir: &Path, days: u32) -> CleanReport {
    let mut report = CleanReport::default();
    if days == 0 {
        return report;
    }
    let cutoff = cutoff_time(days);
    clean_dir_level(dir, cutoff, &mut report).await;
    clean_dir_level(&dir.join(PARTIAL_DIR), cutoff, &mut report).await;
    report
}

/// 天数的截止时间：早于（当前时间 - days 天）视为过期。
fn cutoff_time(days: u32) -> SystemTime {
    SystemTime::now()
        .checked_sub(Duration::from_secs(days as u64 * 86_400))
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

/// 判断修改时间是否过期（纯函数，便于单测）。
fn is_expired(modified: SystemTime, cutoff: SystemTime) -> bool {
    modified < cutoff
}

/// 清理单层目录里的过期文件（不递归子目录）。
async fn clean_dir_level(dir: &Path, cutoff: SystemTime, report: &mut CleanReport) {
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        let Ok(meta) = entry.metadata().await else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let Ok(modified) = meta.modified() else {
            continue;
        };
        if !is_expired(modified, cutoff) {
            continue;
        }
        if tokio::fs::remove_file(&path).await.is_ok() {
            report.files += 1;
            report.bytes += meta.len();
            report.removed.insert(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// 正常路径：超过保留期的文件被删除，未过期的保留。
    #[tokio::test]
    async fn clean_removes_expired_files_only() {
        let root = temp_root("expired");
        tokio::fs::create_dir_all(&root).await.expect("mkdir");
        let old = root.join("old.bin");
        let fresh = root.join("fresh.bin");
        tokio::fs::write(&old, b"12345").await.expect("write old");
        tokio::fs::write(&fresh, b"123").await.expect("write fresh");
        set_age(&old, 10).await;

        let report = clean(&root, 7).await;

        assert_eq!(report.files, 1);
        assert_eq!(report.bytes, 5);
        assert!(report.removed.contains(&old));
        assert!(!old.exists(), "过期文件应被删除");
        assert!(fresh.exists(), "未过期文件必须保留");

        tokio::fs::remove_dir_all(&root).await.ok();
    }

    /// 边界：`.chatters-partial` 下的过期残留也会被清理，目录本身保留。
    #[tokio::test]
    async fn clean_removes_expired_partial_residue() {
        let root = temp_root("partial");
        let partial = root.join(".chatters-partial");
        tokio::fs::create_dir_all(&partial).await.expect("mkdir");
        let part = partial.join("abc.part");
        let meta = partial.join("abc.json");
        tokio::fs::write(&part, b"xx").await.expect("write part");
        tokio::fs::write(&meta, b"{}").await.expect("write meta");
        set_age(&part, 30).await;
        set_age(&meta, 30).await;

        let report = clean(&root, 7).await;

        assert_eq!(report.files, 2);
        assert!(!part.exists() && !meta.exists());
        assert!(partial.is_dir(), "部分文件目录本身必须保留");

        tokio::fs::remove_dir_all(&root).await.ok();
    }

    /// 异常：目录不存在或天数为 0 时什么都不做。
    #[tokio::test]
    async fn clean_handles_missing_dir_and_zero_days() {
        let missing = temp_root("missing");
        assert_eq!(clean(&missing, 7).await, CleanReport::default());

        let root = temp_root("zero");
        tokio::fs::create_dir_all(&root).await.expect("mkdir");
        let file = root.join("old.bin");
        tokio::fs::write(&file, b"1").await.expect("write");
        set_age(&file, 30).await;
        assert_eq!(clean(&root, 0).await, CleanReport::default());
        assert!(file.exists(), "天数为 0 时不得删除文件");
        tokio::fs::remove_dir_all(&root).await.ok();
    }

    /// 边界：恰好等于截止时间的文件不算过期（严格小于）。
    #[test]
    fn is_expired_is_strict() {
        let now = SystemTime::now();
        assert!(is_expired(now - Duration::from_secs(10), now));
        assert!(!is_expired(now, now));
    }

    /// 生成独立的临时目录。
    fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("chatters-clean-{name}-{}", uuid::Uuid::new_v4()))
    }

    /// 把文件修改时间往前推 `days` 天（用 `File::set_modified`，无需额外依赖）。
    async fn set_age(path: &Path, days: u64) {
        let file = std::fs::File::options()
            .write(true)
            .open(path)
            .expect("open for mtime");
        let target = SystemTime::now() - Duration::from_secs(days * 86_400);
        file.set_modified(target).expect("set mtime");
    }
}
