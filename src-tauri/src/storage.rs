//! 本地历史存储（M4）：消息与传输记录的持久化。
//!
//! 设计文档原计划 SQLite；离线构建环境无法获取 `rusqlite` 依赖，v1 改用
//! **JSON 文件 + 内存快照** 实现（同一接口语义：可读、可按对端筛选、可清空）。
//! 该偏差已记录在 `requirements/02-technical-design.md` 与 M4 里程碑报告。
//!
//! 写入采用"临时文件 + 改名"保证原子性；只保留最近若干条避免无限增长。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

/// 最多保留的消息条数。
pub const MAX_MESSAGES: usize = 2000;
/// 最多保留的传输记录条数。
pub const MAX_TRANSFERS: usize = 500;

/// 持久化的文字消息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredMessage {
    /// 消息 ID。
    pub msg_id: String,
    /// 对端设备 ID。
    pub peer_id: String,
    /// 方向：out | in。
    pub direction: String,
    /// 文本内容。
    pub text: String,
    /// 时间戳（Unix 毫秒）。
    pub ts: i64,
    /// 状态：sending | delivered | failed。
    pub status: String,
}

/// 持久化的传输记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredTransfer {
    /// 任务 ID。
    pub transfer_id: String,
    /// 对端设备 ID。
    pub peer_id: String,
    /// 方向：send | receive。
    pub direction: String,
    /// 类型：image | file。
    pub kind: String,
    /// 文件名。
    pub filename: String,
    /// 总字节数。
    pub size: u64,
    /// 已传输字节数。
    pub transferred: u64,
    /// 状态。
    pub state: String,
    /// 落盘路径。
    pub path: Option<String>,
    /// 最近更新时间（Unix 毫秒）。
    pub ts: i64,
}

/// 历史快照。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
    /// 消息列表（按时间升序）。
    #[serde(default)]
    pub messages: Vec<StoredMessage>,
    /// 传输记录列表。
    #[serde(default)]
    pub transfers: Vec<StoredTransfer>,
}

/// 历史存储：内存快照 + JSON 落盘。
#[derive(Clone)]
pub struct HistoryStore {
    path: PathBuf,
    inner: Arc<Mutex<History>>,
}

impl HistoryStore {
    /// 打开（或创建）历史文件；解析失败时从空历史开始，不阻塞启动。
    pub async fn open(path: PathBuf) -> Self {
        let history = match tokio::fs::read_to_string(&path).await {
            Ok(text) => serde_json::from_str::<History>(&text).unwrap_or_else(|e| {
                log::warn!("history file is corrupt, starting empty: {e}");
                History::default()
            }),
            Err(_) => History::default(),
        };
        Self {
            path,
            inner: Arc::new(Mutex::new(history)),
        }
    }

    /// 全量快照（用于按对端筛选或导出）。
    pub async fn snapshot(&self) -> History {
        self.inner.lock().await.clone()
    }

    /// 读取某对端的消息（按时间升序）；`peer_id` 为 None 时返回全部。
    pub async fn messages_for(&self, peer_id: Option<&str>) -> Vec<StoredMessage> {
        let history = self.inner.lock().await;
        history
            .messages
            .iter()
            .filter(|m| peer_id.is_none_or(|id| m.peer_id == id))
            .cloned()
            .collect()
    }

    /// 读取某对端的传输记录。
    pub async fn transfers_for(&self, peer_id: Option<&str>) -> Vec<StoredTransfer> {
        let history = self.inner.lock().await;
        history
            .transfers
            .iter()
            .filter(|t| peer_id.is_none_or(|id| t.peer_id == id))
            .cloned()
            .collect()
    }

    /// 追加一条消息并落盘（同 msg_id 覆盖，避免重复回执产生重复记录）。
    pub async fn append_message(&self, message: StoredMessage) -> Result<(), std::io::Error> {
        {
            let mut history = self.inner.lock().await;
            match history
                .messages
                .iter_mut()
                .find(|m| m.msg_id == message.msg_id)
            {
                Some(existing) => *existing = message,
                None => history.messages.push(message),
            }
            history.messages.sort_by_key(|m| m.ts);
            if history.messages.len() > MAX_MESSAGES {
                let overflow = history.messages.len() - MAX_MESSAGES;
                history.messages.drain(..overflow);
            }
        }
        self.persist().await
    }

    /// 插入或更新一条传输记录并落盘。
    pub async fn upsert_transfer(&self, transfer: StoredTransfer) -> Result<(), std::io::Error> {
        {
            let mut history = self.inner.lock().await;
            match history
                .transfers
                .iter_mut()
                .find(|t| t.transfer_id == transfer.transfer_id)
            {
                Some(existing) => *existing = transfer,
                None => history.transfers.push(transfer),
            }
            if history.transfers.len() > MAX_TRANSFERS {
                let overflow = history.transfers.len() - MAX_TRANSFERS;
                history.transfers.drain(..overflow);
            }
        }
        self.persist().await
    }

    /// 清空历史；`peer_id` 为 None 时清空全部（F40/F41）。
    pub async fn clear(&self, peer_id: Option<&str>) -> Result<(), std::io::Error> {
        {
            let mut history = self.inner.lock().await;
            match peer_id {
                Some(id) => {
                    history.messages.retain(|m| m.peer_id != id);
                    history.transfers.retain(|t| t.peer_id != id);
                }
                None => {
                    history.messages.clear();
                    history.transfers.clear();
                }
            }
        }
        self.persist().await
    }

    /// 原子落盘：先写 `.tmp` 再改名，避免中途崩溃产生半截文件。
    async fn persist(&self) -> Result<(), std::io::Error> {
        let snapshot = { self.inner.lock().await.clone() };
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let body = serde_json::to_vec_pretty(&snapshot)
            .map_err(|e| std::io::Error::other(format!("serialize history: {e}")))?;
        let tmp = temp_path(&self.path);
        tokio::fs::write(&tmp, body).await?;
        tokio::fs::rename(&tmp, &self.path).await?;
        Ok(())
    }
}

/// 临时文件路径（同目录，保证改名原子性）。
fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

/// 当前 Unix 毫秒时间戳（历史记录使用）。
pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(id: &str, peer: &str, ts: i64) -> StoredMessage {
        StoredMessage {
            msg_id: id.to_string(),
            peer_id: peer.to_string(),
            direction: "in".to_string(),
            text: "hi".to_string(),
            ts,
            status: "delivered".to_string(),
        }
    }

    fn transfer_row(id: &str, peer: &str) -> StoredTransfer {
        StoredTransfer {
            transfer_id: id.to_string(),
            peer_id: peer.to_string(),
            direction: "receive".to_string(),
            kind: "file".to_string(),
            filename: "a.txt".to_string(),
            size: 10,
            transferred: 10,
            state: "done".to_string(),
            path: Some("C:/dl/a.txt".to_string()),
            ts: 1,
        }
    }

    /// 追加消息的简写（避免测试里出现超长链式调用）。
    async fn push(store: &HistoryStore, id: &str, peer: &str, ts: i64) {
        store
            .append_message(message(id, peer, ts))
            .await
            .expect("append message");
    }

    /// 写入传输记录的简写。
    async fn add_transfer(store: &HistoryStore, id: &str, peer: &str) {
        store
            .upsert_transfer(transfer_row(id, peer))
            .await
            .expect("upsert transfer");
    }

    /// 正常路径：消息与传输记录写入后可重新读回（跨实例持久化）。
    #[tokio::test]
    async fn history_persists_across_reopen() {
        let dir = std::env::temp_dir().join("chatters_history_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("history.json");
        std::fs::remove_file(&path).ok();

        let store = HistoryStore::open(path.clone()).await;
        push(&store, "m-2", "peer-1", 200).await;
        push(&store, "m-1", "peer-1", 100).await;
        add_transfer(&store, "t-1", "peer-1").await;

        let reopened = HistoryStore::open(path).await;
        let messages = reopened.messages_for(Some("peer-1")).await;
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].msg_id, "m-1", "按时间升序");
        assert_eq!(reopened.transfers_for(None).await.len(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// 边界：同一 msg_id 重复写入只保留最新一条（回执更新不产生重复）。
    #[tokio::test]
    async fn appending_same_message_updates_status() {
        let dir = std::env::temp_dir().join("chatters_history_test_dedupe");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("history.json");
        std::fs::remove_file(&path).ok();

        let store = HistoryStore::open(path).await;
        push(&store, "m-1", "peer-1", 100).await;
        let mut updated = message("m-1", "peer-1", 100);
        updated.status = "failed".to_string();
        store.append_message(updated).await.unwrap();

        let messages = store.messages_for(None).await;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].status, "failed");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// 正常路径：按对端清空只影响该对端，清空全部则清空两个表。
    #[tokio::test]
    async fn clear_history_by_peer_and_all() {
        let dir = std::env::temp_dir().join("chatters_history_test_clear");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("history.json");
        std::fs::remove_file(&path).ok();

        let store = HistoryStore::open(path).await;
        push(&store, "m-1", "peer-1", 100).await;
        push(&store, "m-2", "peer-2", 200).await;
        add_transfer(&store, "t-1", "peer-1").await;

        store.clear(Some("peer-1")).await.unwrap();
        assert_eq!(store.messages_for(None).await.len(), 1);
        assert!(store.transfers_for(None).await.is_empty());

        store.clear(None).await.unwrap();
        assert!(store.messages_for(None).await.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// 异常：历史文件损坏时从空历史启动，不 panic。
    #[tokio::test]
    async fn corrupt_history_starts_empty() {
        let dir = std::env::temp_dir().join("chatters_history_test_corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("history.json");
        std::fs::write(&path, "{ not json").unwrap();

        let store = HistoryStore::open(path).await;
        assert!(store.snapshot().await.messages.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// 边界：超过上限时丢弃最旧的消息。
    #[tokio::test]
    async fn history_trims_old_messages() {
        let dir = std::env::temp_dir().join("chatters_history_test_trim");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("history.json");
        std::fs::remove_file(&path).ok();

        let store = HistoryStore::open(path).await;
        for index in 0..(MAX_MESSAGES + 5) {
            store
                .append_message(message(&format!("m-{index}"), "peer-1", index as i64))
                .await
                .unwrap();
        }
        let messages = store.messages_for(None).await;
        assert_eq!(messages.len(), MAX_MESSAGES);
        assert_eq!(messages[0].msg_id, "m-5", "最旧的 5 条被裁剪");

        std::fs::remove_dir_all(&dir).ok();
    }
}
