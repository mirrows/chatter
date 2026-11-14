//! 发送端（M3-a）：等待 ACCEPT、流式读取文件、分块发送、等待 FILE_RESULT。

use super::{
    chunk_seq_with, Inner, TransferError, TransferState, CHUNK_SIZE, CHUNK_SIZE_2M, OFFER_TIMEOUT,
    RESULT_TIMEOUT,
};
use crate::protocol::{encode_chunk, FileDone, FileResult, Frame, FrameType};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::oneshot;
use tokio::time::timeout;

/// 等待接收方回传 FILE_HAVE 的超时（超时按"全量缺失"处理，保证兼容）。
const HAVE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// 后台发送流程：握手确认 → 分块 → FILE_DONE → 等待校验结果。
pub(crate) async fn run(
    inner: Arc<Inner>,
    transfer_id: String,
    accept_rx: oneshot::Receiver<u64>,
    reject_rx: oneshot::Receiver<()>,
) -> Result<(), TransferError> {
    let (peer_id, source, size, cancel, notify, expected_sha256) = {
        let tasks = inner.tasks.lock().await;
        let task = tasks
            .get(&transfer_id)
            .ok_or_else(|| TransferError::UnknownTransfer(transfer_id.clone()))?;
        (
            task.info.peer_id.clone(),
            task.source
                .clone()
                .ok_or_else(|| TransferError::InvalidState("missing source path".to_string()))?,
            task.info.size,
            task.cancel.clone(),
            task.notify.clone(),
            task.expected_sha256.clone(),
        )
    };

    // 等待接收方确认；期间可被拒绝或本地取消打断。
    let offset = tokio::select! {
        result = timeout(OFFER_TIMEOUT, accept_rx) => match result {
            Ok(Ok(offset)) => offset,
            Ok(Err(_)) => return fail(inner, &transfer_id, "接收方未响应".to_string()).await,
            Err(_) => return fail(inner, &transfer_id, "等待接收确认超时".to_string()).await,
        },
        _ = reject_rx => {
            inner
                .mark(&transfer_id, TransferState::Rejected, Some("接收方拒绝接收".to_string()))
                .await;
            return Ok(());
        }
        _ = notify.notified() => return cancelled(inner, &transfer_id).await,
    };
    if cancel.load(Ordering::Relaxed) {
        return cancelled(inner, &transfer_id).await;
    }

    inner
        .mark(&transfer_id, TransferState::Transferring, None)
        .await;
    if offset > 0 {
        inner.advance(&transfer_id, offset, true).await;
    }

    let uuid = uuid::Uuid::parse_str(&transfer_id)
        .map_err(|e| TransferError::InvalidState(format!("bad transfer id: {e}")))?;
    // v2 能力协商：双方都声明 `chunk2m` 时使用 2MB 块，否则退回 1MB（v1 兼容）。
    let chunk_size = if inner
        .session
        .peer_features(&peer_id)
        .await
        .iter()
        .any(|feature| feature == "chunk2m")
    {
        CHUNK_SIZE_2M
    } else {
        CHUNK_SIZE
    };
    // M11：发送端在 OFFER 前已算好块表；这里发 BLOCK_MAP 并等待位图，只补缺失块。
    let (use_chunkmap, chunk_map) = {
        let tasks = inner.tasks.lock().await;
        let task = tasks
            .get(&transfer_id)
            .ok_or_else(|| TransferError::UnknownTransfer(transfer_id.clone()))?;
        (task.use_chunkmap, task.chunk_map.clone())
    };

    let mut file = tokio::fs::File::open(&source).await?;
    let mut buf = vec![0u8; chunk_size];
    let mut sent = offset;

    let mut bitmap = None;
    if use_chunkmap {
        if let Some(map) = chunk_map {
            bitmap =
                Some(send_block_map_and_wait_have(&inner, &transfer_id, &peer_id, &map).await?);
        }
    }

    if let Some(bitmap) = bitmap {
        // 只发送位图中缺失的块（seq = 块索引）。
        for index in bitmap.missing() {
            if cancel.load(Ordering::Relaxed) {
                return cancelled(inner, &transfer_id).await;
            }
            let block_offset = index as u64 * chunk_size as u64;
            if block_offset >= size {
                break;
            }
            let want = ((size - block_offset) as usize).min(chunk_size);
            file.seek(std::io::SeekFrom::Start(block_offset)).await?;
            let read = file.read(&mut buf[..want]).await?;
            if read == 0 {
                break;
            }
            let payload = encode_chunk(&uuid, index as u32, &buf[..read])?;
            if let Err(e) = inner
                .send(&peer_id, Frame::new(FrameType::FileChunk, payload))
                .await
            {
                return fail(inner, &transfer_id, format!("发送数据块失败：{e}")).await;
            }
            inner.touch(&transfer_id).await;
            inner.advance(&transfer_id, read as u64, false).await;
        }
    } else {
        if offset > 0 {
            file.seek(std::io::SeekFrom::Start(offset)).await?;
        }
        while sent < size {
            if cancel.load(Ordering::Relaxed) {
                return cancelled(inner, &transfer_id).await;
            }
            let remaining = (size - sent) as usize;
            let want = remaining.min(chunk_size);
            let read = file.read(&mut buf[..want]).await?;
            if read == 0 {
                break;
            }
            let payload = encode_chunk(&uuid, chunk_seq_with(sent, chunk_size), &buf[..read])?;
            if let Err(e) = inner
                .send(&peer_id, Frame::new(FrameType::FileChunk, payload))
                .await
            {
                return fail(inner, &transfer_id, format!("发送数据块失败：{e}")).await;
            }
            sent += read as u64;
            inner.touch(&transfer_id).await;
            inner.advance(&transfer_id, read as u64, false).await;
        }
    }

    // 先登记结果等待者，再发 FILE_DONE，避免对端回执早于登记。
    let (result_tx, result_rx) = oneshot::channel::<FileResult>();
    {
        let mut tasks = inner.tasks.lock().await;
        if let Some(task) = tasks.get_mut(&transfer_id) {
            task.result_tx = Some(result_tx);
        }
    }
    let done = Frame::json(
        FrameType::FileDone,
        &FileDone {
            transfer_id: transfer_id.clone(),
            sha256: expected_sha256,
        },
    )?;
    if let Err(e) = inner.send(&peer_id, done).await {
        return fail(inner, &transfer_id, format!("发送结束帧失败：{e}")).await;
    }

    match timeout(RESULT_TIMEOUT, result_rx).await {
        Ok(Ok(result)) if result.ok => {
            inner.mark(&transfer_id, TransferState::Done, None).await;
            inner.advance(&transfer_id, 0, true).await;
            Ok(())
        }
        Ok(Ok(result)) => {
            // M11：优先使用接收端给出的失败分类（missing_blocks / file_hash_mismatch）。
            let reason = result.error.unwrap_or_else(|| "接收端校验失败".to_string());
            fail(inner, &transfer_id, reason).await
        }
        Ok(Err(_)) => fail(inner, &transfer_id, "接收端未返回校验结果".to_string()).await,
        Err(_) => fail(inner, &transfer_id, "等待校验结果超时".to_string()).await,
    }
}

/// 发送 BLOCK_MAP 并等待接收方 FILE_HAVE；超时按"全部缺失"处理（保证降级可用）。
async fn send_block_map_and_wait_have(
    inner: &Arc<Inner>,
    transfer_id: &str,
    peer_id: &str,
    map: &super::chunkmap::ChunkMap,
) -> Result<super::chunkmap::BlockBitmap, TransferError> {
    let (have_tx, have_rx) = oneshot::channel();
    {
        let mut tasks = inner.tasks.lock().await;
        if let Some(task) = tasks.get_mut(transfer_id) {
            task.have_tx = Some(have_tx);
        }
    }
    let frame = Frame::json(FrameType::FileBlockMap, &map.to_frame(transfer_id))?;
    inner.send(peer_id, frame).await?;
    match timeout(HAVE_TIMEOUT, have_rx).await {
        Ok(Ok(bitmap)) => Ok(bitmap),
        _ => Ok(super::chunkmap::BlockBitmap::new(map.block_count())),
    }
}

/// 标记失败并返回错误（由调用方记录日志）。
async fn fail(inner: Arc<Inner>, transfer_id: &str, reason: String) -> Result<(), TransferError> {
    inner
        .mark(transfer_id, TransferState::Failed, Some(reason.clone()))
        .await;
    Err(TransferError::InvalidState(reason))
}

/// 本地已取消：状态由取消入口写入，这里直接结束。
async fn cancelled(inner: Arc<Inner>, transfer_id: &str) -> Result<(), TransferError> {
    inner
        .mark(transfer_id, TransferState::Cancelled, None)
        .await;
    Ok(())
}
