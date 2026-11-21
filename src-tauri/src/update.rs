//! 自动更新守卫（v3）：GitHub Releases 更新后的启动自检与自动回滚。
//!
//! 目录 `<app_data>/update/`：
//! - `previous.exe`：更新安装前备份的旧可执行文件（回滚目标，保留最早一份）；
//! - `pending.json`：更新安装前写入，`attempts` 每启动一次自增；
//! - `last-rollback.json`：回滚执行记录，界面提示一次后删除。
//!
//! 判定规则：
//! - 更新安装前 `begin_update` 备份当前 exe 并写 `pending(attempts = 0)`；
//! - 新版本启动时 `on_startup` 把 `attempts` 加一；前端在窗口稳定后调用
//!   `confirm_healthy` 删除 pending，表示「新版本可正常使用」；
//! - 若前端始终没确认（Rust panic / 前端白屏 / 进程秒退），下次启动 `attempts`
//!   达到 `MAX_UNCONFIRMED_STARTS` 且备份存在 → 自动回滚到上一版本；
//! - 没有备份时只继续尝试、不回滚（避免把应用弄成不可用）。
//!
//! 本模块刻意不依赖 Tauri，便于单元测试。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// 连续多少次启动未确认健康就自动回滚。
pub const MAX_UNCONFIRMED_STARTS: u32 = 2;

/// 待确认的更新记录（`pending.json`）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingUpdate {
    /// 更新前版本。
    pub from: String,
    /// 更新后版本。
    pub to: String,
    /// 已启动次数（每次启动自增）。
    #[serde(default)]
    pub attempts: u32,
    /// 写入时间（毫秒时间戳）。
    #[serde(default)]
    pub at: i64,
}

/// 回滚执行记录（`last-rollback.json`）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RollbackRecord {
    /// 出问题的版本。
    pub from: String,
    /// 回滚到的版本。
    pub to: String,
    /// 执行时间（毫秒时间戳）。
    #[serde(default)]
    pub at: i64,
}

/// 启动自检结果。
#[derive(Debug, Clone, PartialEq)]
pub enum StartupOutcome {
    /// 没有待确认的更新。
    Clean,
    /// 更新后启动，但尚未确认健康（`attempts` 为本次启动序号）。
    Pending(PendingUpdate),
    /// 连续多次未确认健康，需要回滚到备份版本。
    RollbackRequired(PendingUpdate),
}

/// 更新守卫：负责备份、pending 状态机与回滚脚本。
#[derive(Debug, Clone)]
pub struct UpdateGuard {
    dir: PathBuf,
    exe: PathBuf,
}

impl UpdateGuard {
    /// 打开守卫目录（不存在则创建），`exe` 为当前可执行文件路径。
    pub fn open(dir: PathBuf, exe: PathBuf) -> Self {
        let _ = fs::create_dir_all(&dir);
        Self { dir, exe }
    }

    /// 守卫目录。
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// 当前可执行文件路径。
    pub fn exe(&self) -> &Path {
        &self.exe
    }

    /// 备份文件路径（`previous.exe`）。
    pub fn backup_path(&self) -> PathBuf {
        self.dir.join("previous.exe")
    }

    /// 是否已有可回滚的备份。
    pub fn has_backup(&self) -> bool {
        self.backup_path().is_file()
    }

    /// 读取待确认更新；文件缺失或损坏时返回 None（不阻塞启动）。
    pub fn pending(&self) -> Option<PendingUpdate> {
        let text = fs::read_to_string(self.dir.join("pending.json")).ok()?;
        serde_json::from_str::<PendingUpdate>(&text).ok()
    }

    /// 读取上次回滚记录（界面提示一次）。
    pub fn last_rollback(&self) -> Option<RollbackRecord> {
        let text = fs::read_to_string(self.dir.join("last-rollback.json")).ok()?;
        serde_json::from_str::<RollbackRecord>(&text).ok()
    }

    /// 删除回滚记录（界面已提示）。
    pub fn ack_rollback(&self) {
        let _ = fs::remove_file(self.dir.join("last-rollback.json"));
    }

    /// 更新安装前调用：备份当前 exe 并写入 pending。
    ///
    /// 备份只在不存在时写入，保留最早（最稳定）的一份，多次升级也能回到最初版本。
    pub fn begin_update(&self, from: &str, to: &str) -> Result<(), String> {
        fs::create_dir_all(&self.dir).map_err(|e| format!("创建更新目录失败：{e}"))?;
        let backup = self.backup_path();
        if !backup.is_file() {
            fs::copy(&self.exe, &backup).map_err(|e| format!("备份当前版本失败：{e}"))?;
        }
        let pending = PendingUpdate {
            from: from.to_string(),
            to: to.to_string(),
            attempts: 0,
            at: crate::storage::now_millis(),
        };
        self.write_pending(&pending)
    }

    /// 下载/安装失败时调用：撤销 pending（不动备份，手动回滚仍可用）。
    pub fn cancel_pending(&self) {
        let _ = fs::remove_file(self.dir.join("pending.json"));
    }

    /// 前端确认新版本健康：删除 pending。
    pub fn confirm_healthy(&self) {
        self.cancel_pending();
    }

    /// 启动自检：有 pending 则 attempts 加一并写回，返回处理结果。
    pub fn on_startup(&self) -> StartupOutcome {
        let Some(mut pending) = self.pending() else {
            return StartupOutcome::Clean;
        };
        pending.attempts = pending.attempts.saturating_add(1);
        if let Err(e) = self.write_pending(&pending) {
            log::warn!("failed to persist update pending state: {e}");
        }
        if pending.attempts >= MAX_UNCONFIRMED_STARTS && self.has_backup() {
            StartupOutcome::RollbackRequired(pending)
        } else {
            StartupOutcome::Pending(pending)
        }
    }

    /// 记录一次回滚并清除 pending（回滚脚本启动后调用）。
    pub fn mark_rolled_back(&self, pending: &PendingUpdate) {
        let record = RollbackRecord {
            from: pending.to.clone(),
            to: pending.from.clone(),
            at: crate::storage::now_millis(),
        };
        if let Ok(text) = serde_json::to_string_pretty(&record) {
            let _ = fs::write(self.dir.join("last-rollback.json"), text);
        }
        self.cancel_pending();
    }

    /// 安排「退出后用备份覆盖当前 exe 并重启」。
    ///
    /// 当前进程无法覆盖自身可执行文件，因此启动一个隐藏的 PowerShell 脚本
    /// 等待本进程退出后再替换。调用方随后应尽快退出应用。
    pub fn schedule_rollback(&self) -> Result<(), String> {
        if !self.has_backup() {
            return Err("没有可用的旧版本备份".to_string());
        }
        schedule_replace_and_restart(&self.exe, &self.backup_path())
    }

    /// 删除备份（用户确认不再需要回滚时）。
    pub fn discard_backup(&self) {
        let _ = fs::remove_file(self.backup_path());
    }

    /// 写 pending.json（写失败时下次启动按 Clean 处理）。
    fn write_pending(&self, pending: &PendingUpdate) -> Result<(), String> {
        let text = serde_json::to_string_pretty(pending)
            .map_err(|e| format!("序列化更新状态失败：{e}"))?;
        fs::write(self.dir.join("pending.json"), text).map_err(|e| format!("写入更新状态失败：{e}"))
    }
}

/// 启动隐藏脚本：等待当前进程退出 → 用备份覆盖 exe → 重新启动。
#[cfg(windows)]
fn schedule_replace_and_restart(exe: &Path, backup: &Path) -> Result<(), String> {
    use std::process::Command;

    // PowerShell 单引号字符串里单引号需成对转义。
    fn quote(path: &Path) -> String {
        path.to_string_lossy().replace('\'', "''")
    }
    let script = format!(
        "Start-Sleep -Milliseconds 1500; \
         Copy-Item -LiteralPath '{backup}' -Destination '{exe}' -Force; \
         Start-Process -FilePath '{exe}'",
        backup = quote(backup),
        exe = quote(exe)
    );
    Command::new("powershell.exe")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("启动回滚脚本失败：{e}"))
}

/// 非 Windows 桌面暂未实现就地回滚（后续 macOS/Linux 打包时补）。
#[cfg(not(windows))]
fn schedule_replace_and_restart(_exe: &Path, _backup: &Path) -> Result<(), String> {
    Err("当前平台暂不支持自动回滚".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 生成独立的守卫与临时目录（测试结束由调用方清理）。
    fn guard(name: &str) -> (UpdateGuard, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("chatters-update-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create root");
        let exe = root.join("current.exe");
        fs::write(&exe, b"version-1").expect("write exe");
        let guard = UpdateGuard::open(root.join("state"), exe);
        (guard, root)
    }

    /// 正常路径：安装更新前备份当前 exe 并写入 pending。
    #[test]
    fn begin_update_backs_up_and_writes_pending() {
        let (guard, root) = guard("begin");
        assert!(!guard.has_backup());

        guard.begin_update("1.0.0", "1.0.1").expect("begin");

        assert!(guard.has_backup());
        assert_eq!(fs::read(guard.backup_path()).unwrap(), b"version-1");
        let pending = guard.pending().expect("pending");
        assert_eq!(pending.from, "1.0.0");
        assert_eq!(pending.to, "1.0.1");
        assert_eq!(pending.attempts, 0);

        let _ = fs::remove_dir_all(&root);
    }

    /// 正常路径：第一次启动只计数，不触发回滚。
    #[test]
    fn first_start_after_update_is_pending() {
        let (guard, root) = guard("first");
        guard.begin_update("1.0.0", "1.0.1").expect("begin");

        match guard.on_startup() {
            StartupOutcome::Pending(pending) => assert_eq!(pending.attempts, 1),
            other => panic!("expected pending, got {other:?}"),
        }
        assert!(guard.has_backup(), "未确认健康时备份必须保留");

        let _ = fs::remove_dir_all(&root);
    }

    /// 正常路径：连续两次未确认健康 → 触发回滚。
    #[test]
    fn second_start_without_confirmation_requires_rollback() {
        let (guard, root) = guard("second");
        guard.begin_update("1.0.0", "1.0.1").expect("begin");
        let _ = guard.on_startup();

        match guard.on_startup() {
            StartupOutcome::RollbackRequired(pending) => {
                assert_eq!(pending.attempts, MAX_UNCONFIRMED_STARTS);
                assert_eq!(pending.from, "1.0.0");
            }
            other => panic!("expected rollback, got {other:?}"),
        }

        let _ = fs::remove_dir_all(&root);
    }

    /// 正常路径：确认健康后清除 pending，后续启动不再计数。
    #[test]
    fn confirm_healthy_clears_pending() {
        let (guard, root) = guard("confirm");
        guard.begin_update("1.0.0", "1.0.1").expect("begin");
        let _ = guard.on_startup();

        guard.confirm_healthy();

        assert!(guard.pending().is_none());
        assert_eq!(guard.on_startup(), StartupOutcome::Clean);
        assert!(guard.has_backup(), "确认健康后仍保留备份供手动回滚");

        let _ = fs::remove_dir_all(&root);
    }

    /// 异常：没有备份时即使超过次数也不回滚（避免应用不可用）。
    #[test]
    fn no_backup_never_rolls_back() {
        let (guard, root) = guard("nobackup");
        // 手工写 pending 模拟「备份被用户删掉」的情况。
        guard
            .write_pending(&PendingUpdate {
                from: "1.0.0".into(),
                to: "1.0.1".into(),
                attempts: 0,
                at: 0,
            })
            .expect("write pending");

        for _ in 0..3 {
            assert!(matches!(guard.on_startup(), StartupOutcome::Pending(_)));
        }

        let _ = fs::remove_dir_all(&root);
    }

    /// 边界：下载失败调用 cancel_pending 后回到 Clean，且备份仍可用于手动回滚。
    #[test]
    fn cancel_pending_restores_clean_state() {
        let (guard, root) = guard("cancel");
        guard.begin_update("1.0.0", "1.0.1").expect("begin");

        guard.cancel_pending();

        assert_eq!(guard.on_startup(), StartupOutcome::Clean);
        assert!(guard.has_backup());

        let _ = fs::remove_dir_all(&root);
    }

    /// 正常路径：回滚记录写入后可读取，ack 后清除。
    #[test]
    fn rollback_record_round_trip() {
        let (guard, root) = guard("record");
        guard.begin_update("1.0.0", "1.0.1").expect("begin");
        let pending = guard.pending().expect("pending");

        guard.mark_rolled_back(&pending);

        let record = guard.last_rollback().expect("record");
        assert_eq!(record.from, "1.0.1");
        assert_eq!(record.to, "1.0.0");
        assert!(guard.pending().is_none(), "回滚后不应留下 pending");

        guard.ack_rollback();
        assert!(guard.last_rollback().is_none());

        let _ = fs::remove_dir_all(&root);
    }

    /// 边界：已存在备份时不覆盖（保留最早版本作为回滚目标）。
    #[test]
    fn begin_update_keeps_first_backup() {
        let (guard, root) = guard("keep");
        guard.begin_update("1.0.0", "1.0.1").expect("begin");
        // 模拟「升级到 1.0.1 后又升级到 1.0.2」：当前 exe 已变化，但备份应保持 1.0.0。
        fs::write(guard.exe(), b"version-2").expect("rewrite exe");

        guard.begin_update("1.0.1", "1.0.2").expect("begin again");

        assert_eq!(fs::read(guard.backup_path()).unwrap(), b"version-1");

        let _ = fs::remove_dir_all(&root);
    }

    /// 异常：pending.json 损坏时视为 Clean（不阻塞启动）。
    #[test]
    fn corrupt_pending_is_ignored() {
        let (guard, root) = guard("corrupt");
        fs::write(guard.dir().join("pending.json"), b"not json").expect("write");

        assert!(guard.pending().is_none());
        assert_eq!(guard.on_startup(), StartupOutcome::Clean);

        let _ = fs::remove_dir_all(&root);
    }
}
