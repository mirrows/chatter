//! 接收端（M3-a/M3-b）：OFFER 落盘准备、数据块写入、sha256 校验、改名与简版续传。

use super::{
    path_to_string, safe_file_name, unique_path, Inner, Task, TransferConfig, TransferDirection,
    TransferError, TransferInfo, TransferKind, TransferState, CHUNK_SIZE, PROGRESS_THROTTLE,
};
use crate::protocol::{
    FileAccept, FileBlockMap, FileDone, FileHave, FileOffer, FileResult, Frame, FrameType,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::AsyncWriteExt;
use tokio::sync::Notify;

/// 部分文件元数据（简版续传匹配用，对应设计 Q8）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PartialMeta {
    filename: String,
    size: u64,
    sha256: String,
}

/// 部分文件目录：与最终文件分开放，避免用户误以为传输已完成。
fn partial_dir(config: &TransferConfig) -> PathBuf {
    config.download_dir.join(".chatters-partial")
}

/// 续传匹配键：文件名 + 大小（v1 简化口径）。
fn partial_key(filename: &str, size: u64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(filename.as_bytes());
    hasher.update(b"|");
    hasher.update(size.to_be_bytes());
    let hex = format!("{:x}", hasher.finalize());
    hex[..32].to_string()
}

/// 准备部分文件：命中可续传的旧文件时返回已收字节数，否则清理旧文件后从 0 开始。
async fn prepare_partial(
    inner: &Arc<Inner>,
    filename: &str,
    offer: &FileOffer,
) -> Result<(PathBuf, PathBuf, u64), TransferError> {
    let dir = partial_dir(&inner.settings().await);
    tokio::fs::create_dir_all(&dir).await?;
    let key = partial_key(filename, offer.size);
    let part = dir.join(format!("{key}.part"));
    let meta = dir.join(format!("{key}.json"));

    if part.exists() {
        let reusable = match tokio::fs::read_to_string(&meta).await {
            Ok(text) => matches!(
                serde_json::from_str::<PartialMeta>(&text),
                Ok(old)
                    if old.filename == filename
                        && old.size == offer.size
                        && old.sha256 == offer.sha256
            ),
            Err(_) => false,
        };
        if reusable {
            let received = tokio::fs::metadata(&part)
                .await
                .map(|m| m.len())
                .unwrap_or(0);
            if received <= offer.size {
                return Ok((part, meta, received));
            }
        }
        // 内容不一致或长度异常：丢弃旧的部分文件，避免污染本次传输。
        let _ = tokio::fs::remove_file(&part).await;
        let _ = tokio::fs::remove_file(&meta).await;
    }
    Ok((part, meta, 0))
}

/// 读取已有数据块并重建 sha256 累加器（续传必须基于已收内容继续校验）。
async fn hasher_from_file(path: &Path) -> Result<Sha256, TransferError> {
    let path = path.to_path_buf();
    let hasher = tokio::task::spawn_blocking(move || -> Result<Sha256, std::io::Error> {
        use std::io::Read;
        let mut file = std::fs::File::open(&path)?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; CHUNK_SIZE];
        loop {
            let read = file.read(&mut buf)?;
            if read == 0 {
                break;
            }
            hasher.update(&buf[..read]);
        }
        Ok(hasher)
    })
    .await
    .map_err(|e| TransferError::Io(std::io::Error::other(e)))??;
    Ok(hasher)
}

/// 处理 FILE_OFFER：校验文件名、建立任务；图片按策略自动接收（F34）。
pub(crate) async fn on_offer(
    inner: Arc<Inner>,
    peer_id: &str,
    offer: FileOffer,
) -> Result<(), TransferError> {
    let filename = safe_file_name(&offer.filename)?;
    let kind = TransferKind::from_protocol(offer.kind);
    let config = inner.settings().await;
    // M11：双方声明 chunkmap 时启用块级续传（块大小与发送端按同一规则协商）。
    let block_size = super::negotiated_block_size(&inner, peer_id).await;
    let use_chunkmap = super::chunkmap_enabled(&inner, peer_id).await;
    let final_path = unique_path(&config.download_dir, &filename);
    let (part, meta, resume_from) = prepare_partial(&inner, &filename, &offer).await?;
    // chunkmap 模式下 `.part` 会被预分配为 size，文件长度不能代表已收进度，进度由位图决定。
    let resume_from = if use_chunkmap { 0 } else { resume_from };
    let info = TransferInfo {
        transfer_id: offer.transfer_id.clone(),
        peer_id: peer_id.to_string(),
        direction: TransferDirection::Receive,
        kind,
        filename,
        size: offer.size,
        transferred: resume_from,
        state: TransferState::Pending,
        path: None,
        error: None,
        ts: crate::storage::now_millis(),
    };

    {
        let mut tasks = inner.tasks.lock().await;
        if tasks.contains_key(&offer.transfer_id) {
            return Err(TransferError::InvalidState(format!(
                "duplicate transfer: {}",
                offer.transfer_id
            )));
        }
        tasks.insert(
            offer.transfer_id.clone(),
            Task {
                info: info.clone(),
                expected_sha256: offer.sha256,
                source: None,
                part: Some(part),
                final_path: Some(final_path),
                file: None,
                hasher: None,
                accept_tx: None,
                reject_tx: None,
                result_tx: None,
                cancel: Arc::new(AtomicBool::new(false)),
                notify: Arc::new(Notify::new()),
                last_emit: Instant::now(),
                last_activity: Instant::now(),
                resume_from,
                meta: Some(meta),
                use_chunkmap,
                block_size,
                chunk_map: None,
                bitmap: None,
                chunk_file: None,
                blocks_since_flush: 0,
                have_tx: None,
                auto_retried: false,
            },
        );
    }

    // F34：图片默认自动接收；文件需确认，除非用户在设置里开启自动接收。
    let auto_accept = (kind == TransferKind::Image && config.auto_accept_images)
        || (kind == TransferKind::File && config.auto_accept_files);
    if auto_accept {
        accept(&inner, &offer.transfer_id).await?;
    } else {
        // 等待用户确认：前端据 pending 状态弹窗（F34）。
        inner.emit(info).await;
    }
    Ok(())
}

/// 接受接收：创建/追加 `.part` 文件并回 FILE_ACCEPT（含续传偏移，Q8）。
pub(crate) async fn accept(inner: &Arc<Inner>, transfer_id: &str) -> Result<(), TransferError> {
    let (peer_id, part, meta, resume_from, expected_sha256, filename, size, use_chunkmap) = {
        let mut tasks = inner.tasks.lock().await;
        let task = tasks
            .get_mut(transfer_id)
            .ok_or_else(|| TransferError::UnknownTransfer(transfer_id.to_string()))?;
        if task.info.direction != TransferDirection::Receive {
            return Err(TransferError::InvalidState(
                "accept requires a receive task".to_string(),
            ));
        }
        if task.info.state != TransferState::Pending {
            return Err(TransferError::InvalidState(format!(
                "accept requires pending state, current: {}",
                task.info.state.as_str()
            )));
        }
        (
            task.info.peer_id.clone(),
            task.part
                .clone()
                .ok_or_else(|| TransferError::InvalidState("missing part path".to_string()))?,
            task.meta
                .clone()
                .ok_or_else(|| TransferError::InvalidState("missing meta path".to_string()))?,
            task.resume_from,
            task.expected_sha256.clone(),
            task.info.filename.clone(),
            task.info.size,
            task.use_chunkmap,
        )
    };

    if let Some(parent) = part.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // M11：块级续传——预分配 `.part`、恢复位图，并回 ACCEPT（偏移 0，由位图决定缺失块）。
    if use_chunkmap {
        return accept_chunkmap(
            inner,
            transfer_id,
            &peer_id,
            &part,
            &meta,
            expected_sha256,
            filename,
            size,
        )
        .await;
    }

    // 续传：追加写入并从已有内容重建哈希；首轮：新建文件与全新哈希。
    let (file, hasher) = if resume_from > 0 {
        let file = tokio::fs::OpenOptions::new()
            .append(true)
            .open(&part)
            .await?;
        (file, hasher_from_file(&part).await?)
    } else {
        (tokio::fs::File::create(&part).await?, Sha256::new())
    };

    // 元数据用于跨进程/跨任务的续传匹配，先落盘再开始接收。
    let meta_body = PartialMeta {
        filename,
        size,
        sha256: expected_sha256,
    };
    tokio::fs::write(&meta, serde_json::to_vec(&meta_body)?).await?;

    {
        let mut tasks = inner.tasks.lock().await;
        let task = tasks
            .get_mut(transfer_id)
            .ok_or_else(|| TransferError::UnknownTransfer(transfer_id.to_string()))?;
        task.file = Some(file);
        task.hasher = Some(hasher);
        task.info.transferred = resume_from;
    }

    let frame = Frame::json(
        FrameType::FileAccept,
        &FileAccept {
            transfer_id: transfer_id.to_string(),
            resume_offset: resume_from,
        },
    )?;
    if let Err(e) = inner.send(&peer_id, frame).await {
        cleanup_part(inner, transfer_id).await;
        inner
            .mark(transfer_id, TransferState::Failed, Some(e.to_string()))
            .await;
        return Err(e);
    }
    inner
        .mark(transfer_id, TransferState::Transferring, None)
        .await;
    Ok(())
}

/// 块级续传的接受流程（M11）：先回 ACCEPT（offset = 0），
/// `.part` 预分配、位图与块表在收到 FILE_BLOCK_MAP 后由 `on_block_map` 准备。
///
/// 若对端未发送块表（异常/旧端），`on_chunk` 会懒创建顺序写句柄，保持旧行为。
// 参数为接受流程的各字段，拆结构体反而增加调用点噪音。
#[allow(clippy::too_many_arguments)]
async fn accept_chunkmap(
    inner: &Arc<Inner>,
    transfer_id: &str,
    peer_id: &str,
    part: &Path,
    meta: &Path,
    expected_sha256: String,
    filename: String,
    size: u64,
) -> Result<(), TransferError> {
    if let Some(parent) = part.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // 续传元数据与偏移续传共用，便于降级与排障。
    let meta_body = PartialMeta {
        filename,
        size,
        sha256: expected_sha256,
    };
    tokio::fs::write(meta, serde_json::to_vec(&meta_body)?).await?;

    {
        let mut tasks = inner.tasks.lock().await;
        let task = tasks
            .get_mut(transfer_id)
            .ok_or_else(|| TransferError::UnknownTransfer(transfer_id.to_string()))?;
        task.info.transferred = 0;
        task.blocks_since_flush = 0;
    }

    let frame = Frame::json(
        FrameType::FileAccept,
        &FileAccept {
            transfer_id: transfer_id.to_string(),
            resume_offset: 0,
        },
    )?;
    if let Err(e) = inner.send(peer_id, frame).await {
        cleanup_part(inner, transfer_id).await;
        inner
            .mark(transfer_id, TransferState::Failed, Some(e.to_string()))
            .await;
        return Err(e);
    }
    inner
        .mark(transfer_id, TransferState::Transferring, None)
        .await;
    Ok(())
}

/// 处理 FILE_BLOCK_MAP（M11）：校验块表、准备位图并回 FILE_HAVE。
pub(crate) async fn on_block_map(
    inner: Arc<Inner>,
    map: FileBlockMap,
) -> Result<(), TransferError> {
    let transfer_id = map.transfer_id.clone();
    let (peer_id, part, size, block_size, use_chunkmap, state, error) = {
        let tasks = inner.tasks.lock().await;
        let task = tasks
            .get(&transfer_id)
            .ok_or_else(|| TransferError::UnknownTransfer(transfer_id.clone()))?;
        (
            task.info.peer_id.clone(),
            task.part
                .clone()
                .ok_or_else(|| TransferError::InvalidState("missing part path".to_string()))?,
            task.info.size,
            task.block_size,
            task.use_chunkmap,
            task.info.state,
            task.info.error.clone(),
        )
    };
    if !use_chunkmap {
        // 未启用块级续传：忽略（对端可能误发）。
        return Ok(());
    }
    // M11：允许在 Failed 状态下恢复（发送端自动补发缺失块时重发块表）。
    if !matches!(state, TransferState::Transferring | TransferState::Failed) {
        return Err(TransferError::InvalidState(format!(
            "block map in state {}",
            state.as_str()
        )));
    }
    // 上次因整文件哈希不符失败时位图不可信（可能全满但内容错误）→ 重置重传。
    let reset_bitmap = state == TransferState::Failed
        && !error
            .as_deref()
            .map(|reason| reason.starts_with("missing_blocks"))
            .unwrap_or(false);

    let chunk_map = super::chunkmap::ChunkMap::from_frame(&map)
        .map_err(|e| TransferError::InvalidState(format!("bad block map: {e}")))?;
    if chunk_map.block_size() as usize != block_size {
        return Err(TransferError::InvalidState(
            "block size mismatch".to_string(),
        ));
    }
    let blocks = (size as usize).div_ceil(block_size);
    if chunk_map.block_count() != blocks {
        return Err(TransferError::InvalidState(format!(
            "block count mismatch: {} != {blocks}",
            chunk_map.block_count()
        )));
    }

    // 预分配 `.part` 并打开随机写句柄；位图缺失/损坏时按块表重建。
    let part_clone = part.clone();
    let bitmap_path_load = super::chunkmap::bitmap_path_for(&part_clone);
    let bitmap_path_save = bitmap_path_load.clone();
    let map_for_rebuild = chunk_map.clone();
    let (std_file, bitmap) = tokio::task::spawn_blocking(
        move || -> Result<(std::fs::File, super::chunkmap::BlockBitmap), std::io::Error> {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(&part_clone)?;
            file.set_len(size)?;
            let bitmap = if reset_bitmap {
                // 位图不可信：清空并覆盖落盘，让发送端重传全部块。
                let bitmap = super::chunkmap::BlockBitmap::new(blocks);
                let _ = super::chunkmap::save_bitmap(&bitmap_path_save, &bitmap);
                bitmap
            } else {
                match super::chunkmap::load_bitmap(&bitmap_path_load, blocks) {
                    Ok(bitmap) => bitmap,
                    Err(_) => {
                        // 位图缺失/损坏：按 `.part` 已有内容逐块比对重建，并重新落盘。
                        let bitmap =
                            super::chunkmap::rebuild_bitmap(&part_clone, &map_for_rebuild, size)
                                .unwrap_or_else(|_| super::chunkmap::BlockBitmap::new(blocks));
                        let _ = super::chunkmap::save_bitmap(&bitmap_path_save, &bitmap);
                        bitmap
                    }
                }
            };
            Ok((file, bitmap))
        },
    )
    .await
    .map_err(|e| TransferError::Io(std::io::Error::other(e)))??;

    {
        let mut tasks = inner.tasks.lock().await;
        let task = tasks
            .get_mut(&transfer_id)
            .ok_or_else(|| TransferError::UnknownTransfer(transfer_id.clone()))?;
        task.chunk_map = Some(chunk_map);
        task.chunk_file = Some(Arc::new(std::sync::Mutex::new(std_file)));
        task.bitmap = Some(bitmap.clone());
        task.info.transferred = (bitmap.received() as u64 * block_size as u64).min(size);
    }
    // Failed → Transferring 的恢复路径：清掉上次错误并通知界面。
    if state == TransferState::Failed {
        inner
            .mark(&transfer_id, TransferState::Transferring, None)
            .await;
    }

    let frame = Frame::json(
        FrameType::FileHave,
        &FileHave {
            transfer_id,
            bitmap: bitmap.to_hex(),
        },
    )?;
    inner.send(&peer_id, frame).await?;
    Ok(())
}

/// 处理 FILE_CHUNK：顺序写入 `.part` 并累加 sha256。
pub(crate) async fn on_chunk(inner: Arc<Inner>, frame: &Frame) -> Result<(), TransferError> {
    let (id, seq, data) = super::decode_transfer_chunk(frame)?;
    let transfer_id = id.to_string();
    // M11：块级续传模式走随机写 + 位图（顺序写会写错位置）。
    let chunkmap_mode = {
        let tasks = inner.tasks.lock().await;
        tasks
            .get(&transfer_id)
            .map(|task| task.use_chunkmap && task.chunk_map.is_some())
            .unwrap_or(false)
    };
    if chunkmap_mode {
        return on_chunk_map(inner, &transfer_id, seq, &data).await;
    }

    // 异常/旧端：声明了 chunkmap 但未发块表 → 懒创建顺序写句柄，退回旧顺序写行为。
    let needs_lazy = {
        let tasks = inner.tasks.lock().await;
        tasks
            .get(&transfer_id)
            .map(|task| task.use_chunkmap && task.chunk_map.is_none() && task.file.is_none())
            .unwrap_or(false)
    };
    if needs_lazy {
        let part = {
            let tasks = inner.tasks.lock().await;
            tasks.get(&transfer_id).and_then(|task| task.part.clone())
        };
        if let Some(part) = part {
            let file = tokio::fs::File::create(&part).await?;
            let mut tasks = inner.tasks.lock().await;
            if let Some(task) = tasks.get_mut(&transfer_id) {
                if task.file.is_none() {
                    task.file = Some(file);
                    task.hasher = Some(Sha256::new());
                    task.info.transferred = 0;
                }
            }
        }
    }
    let progress = {
        let mut tasks = inner.tasks.lock().await;
        let task = tasks
            .get_mut(&transfer_id)
            .ok_or_else(|| TransferError::UnknownTransfer(transfer_id.clone()))?;
        if task.info.direction != TransferDirection::Receive {
            return Err(TransferError::InvalidState(
                "chunk received for a send task".to_string(),
            ));
        }
        if task.cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        if task.info.state != TransferState::Transferring {
            return Err(TransferError::InvalidState(format!(
                "chunk in state {}",
                task.info.state.as_str()
            )));
        }
        let (Some(file), Some(hasher)) = (task.file.as_mut(), task.hasher.as_mut()) else {
            return Err(TransferError::InvalidState(
                "chunk arrived before accept".to_string(),
            ));
        };
        file.write_all(&data).await?;
        hasher.update(&data);
        task.info.transferred = task.info.transferred.saturating_add(data.len() as u64);
        task.last_activity = Instant::now();
        let now = Instant::now();
        if now.duration_since(task.last_emit) < PROGRESS_THROTTLE {
            None
        } else {
            task.last_emit = now;
            Some(task.info.clone())
        }
    };
    if let Some(info) = progress {
        inner.emit(info).await;
    }
    Ok(())
}

/// 块级续传模式的数据块处理（M11）：校验块哈希 → 随机写 → 位图落盘。
async fn on_chunk_map(
    inner: Arc<Inner>,
    transfer_id: &str,
    seq: u32,
    data: &[u8],
) -> Result<(), TransferError> {
    let (file, flush, progress, offset, bitmap_path) = {
        let mut tasks = inner.tasks.lock().await;
        let task = tasks
            .get_mut(transfer_id)
            .ok_or_else(|| TransferError::UnknownTransfer(transfer_id.to_string()))?;
        if task.cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        if task.info.state != TransferState::Transferring {
            return Err(TransferError::InvalidState(format!(
                "chunk in state {}",
                task.info.state.as_str()
            )));
        }
        let block_size = task.block_size;
        let offset = seq as u64 * block_size as u64;
        if task.info.size > 0 && offset >= task.info.size {
            return Err(TransferError::InvalidState(format!(
                "block {seq} out of range"
            )));
        }
        let expected = task
            .chunk_map
            .as_ref()
            .and_then(|map| map.hash_at(seq as usize))
            .ok_or_else(|| TransferError::InvalidState(format!("missing block map for {seq}")))?;
        if super::chunkmap::block_hash(data) != expected {
            return Err(TransferError::InvalidState(format!(
                "block {seq} hash mismatch"
            )));
        }
        let file = task.chunk_file.clone().ok_or_else(|| {
            TransferError::InvalidState("chunk map mode not prepared".to_string())
        })?;
        let bitmap = task
            .bitmap
            .as_mut()
            .ok_or_else(|| TransferError::InvalidState("missing bitmap".to_string()))?;
        bitmap.set(seq as usize);
        task.blocks_since_flush += 1;
        let flush = if task.blocks_since_flush >= 32 {
            task.blocks_since_flush = 0;
            Some(bitmap.clone())
        } else {
            None
        };
        let size = task.info.size;
        task.info.transferred = (bitmap.received() as u64 * block_size as u64).min(size);
        task.last_activity = Instant::now();
        let now = Instant::now();
        let progress = if now.duration_since(task.last_emit) < PROGRESS_THROTTLE {
            None
        } else {
            task.last_emit = now;
            Some(task.info.clone())
        };
        let part = task
            .part
            .clone()
            .ok_or_else(|| TransferError::InvalidState("missing part path".to_string()))?;
        (
            file,
            flush,
            progress,
            offset,
            super::chunkmap::bitmap_path_for(&part),
        )
    };

    let payload = data.to_vec();
    tokio::task::spawn_blocking(move || -> Result<(), std::io::Error> {
        use std::io::{Seek, SeekFrom, Write};
        let mut file = file
            .lock()
            .map_err(|_| std::io::Error::other("file lock poisoned"))?;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&payload)?;
        if let Some(bitmap) = flush {
            super::chunkmap::save_bitmap(&bitmap_path, &bitmap).map_err(std::io::Error::other)?;
        }
        Ok(())
    })
    .await
    .map_err(|e| TransferError::Io(std::io::Error::other(e)))??;

    if let Some(info) = progress {
        inner.emit(info).await;
    }
    Ok(())
}

/// 处理 FILE_DONE：校验大小与 sha256，通过则改名为正式文件并回 FILE_RESULT。
pub(crate) async fn on_done(inner: Arc<Inner>, done: FileDone) -> Result<(), TransferError> {
    // M11：块级续传模式按位图 + 重新计算文件哈希判定（顺序哈希累加器不适用）。
    let chunkmap_mode = {
        let tasks = inner.tasks.lock().await;
        tasks
            .get(&done.transfer_id)
            .map(|task| task.use_chunkmap && task.chunk_map.is_some())
            .unwrap_or(false)
    };
    if chunkmap_mode {
        return on_done_map(inner, done).await;
    }
    let (peer_id, expected, part, final_path, actual, size_ok, meta) = {
        let mut tasks = inner.tasks.lock().await;
        let task = tasks
            .get_mut(&done.transfer_id)
            .ok_or_else(|| TransferError::UnknownTransfer(done.transfer_id.clone()))?;
        if task.cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        if task.info.state != TransferState::Transferring {
            return Err(TransferError::InvalidState(format!(
                "done in state {}",
                task.info.state.as_str()
            )));
        }
        // 先关闭写句柄，Windows 上未关闭的文件无法改名。
        task.file.take();
        let hasher = task
            .hasher
            .take()
            .ok_or_else(|| TransferError::InvalidState("missing hasher".to_string()))?;
        let actual = format!("{:x}", hasher.finalize());
        (
            task.info.peer_id.clone(),
            task.expected_sha256.clone(),
            task.part.clone(),
            task.final_path.clone(),
            actual,
            task.info.transferred == task.info.size,
            task.meta.clone(),
        )
    };

    let verified = size_ok && actual == done.sha256 && done.sha256 == expected;
    let (ok, path) = if verified {
        let target = final_path
            .ok_or_else(|| TransferError::InvalidState("missing final path".to_string()))?;
        let part_path =
            part.ok_or_else(|| TransferError::InvalidState("missing part path".to_string()))?;
        tokio::fs::rename(&part_path, &target).await?;
        // 传输完成：清掉续传元数据，避免下次同名传输误判。
        if let Some(meta) = &meta {
            let _ = tokio::fs::remove_file(meta).await;
        }
        let display = path_to_string(&target);
        {
            let mut tasks = inner.tasks.lock().await;
            if let Some(task) = tasks.get_mut(&done.transfer_id) {
                task.info.path = Some(display.clone());
            }
        }
        inner
            .mark(&done.transfer_id, TransferState::Done, None)
            .await;
        (true, Some(display))
    } else {
        cleanup_part(&inner, &done.transfer_id).await;
        let reason = if size_ok {
            "哈希校验失败".to_string()
        } else {
            "文件大小不一致".to_string()
        };
        inner
            .mark(&done.transfer_id, TransferState::Failed, Some(reason))
            .await;
        (false, None)
    };

    let result = FileResult {
        transfer_id: done.transfer_id.clone(),
        ok,
        path,
        error: None,
    };
    // 回执失败不影响本地状态（对端可能已断开）。
    if let Err(e) = inner
        .send(&peer_id, Frame::json(FrameType::FileResult, &result)?)
        .await
    {
        log::warn!("failed to send file result: {e}");
    }
    Ok(())
}

/// 块级续传模式的完成判定（M11）：位图全满 + 重新计算文件哈希。
async fn on_done_map(inner: Arc<Inner>, done: FileDone) -> Result<(), TransferError> {
    let (peer_id, expected, part, final_path, bitmap, meta) = {
        let mut tasks = inner.tasks.lock().await;
        let task = tasks
            .get_mut(&done.transfer_id)
            .ok_or_else(|| TransferError::UnknownTransfer(done.transfer_id.clone()))?;
        if task.cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        // 关闭随机写与可能的顺序写句柄（Windows 上未关闭的文件无法改名）。
        task.chunk_file = None;
        task.file.take();
        task.hasher = None;
        (
            task.info.peer_id.clone(),
            task.expected_sha256.clone(),
            task.part
                .clone()
                .ok_or_else(|| TransferError::InvalidState("missing part path".to_string()))?,
            task.final_path
                .clone()
                .ok_or_else(|| TransferError::InvalidState("missing final path".to_string()))?,
            task.bitmap
                .clone()
                .ok_or_else(|| TransferError::InvalidState("missing bitmap".to_string()))?,
            task.meta.clone(),
        )
    };

    // 最终位图落盘（失败不影响判定，仅影响下次续传的恢复速度）。
    let part_for_bitmap = part.clone();
    let bitmap_for_save = bitmap.clone();
    let _ = tokio::task::spawn_blocking(move || {
        super::chunkmap::save_bitmap(
            &super::chunkmap::bitmap_path_for(&part_for_bitmap),
            &bitmap_for_save,
        )
    })
    .await;

    let bitmap_path = super::chunkmap::bitmap_path_for(&part);

    let (ok, path, error) = if !bitmap.all_set() {
        (
            false,
            None,
            Some(format!("missing_blocks:{}", bitmap.missing().len())),
        )
    } else {
        match super::hash_file(&part).await {
            Ok(actual) if actual == expected && done.sha256 == expected => {
                tokio::fs::rename(&part, &final_path).await?;
                let _ = tokio::fs::remove_file(&bitmap_path).await;
                if let Some(meta) = &meta {
                    let _ = tokio::fs::remove_file(meta).await;
                }
                let display = path_to_string(&final_path);
                {
                    let mut tasks = inner.tasks.lock().await;
                    if let Some(task) = tasks.get_mut(&done.transfer_id) {
                        task.info.path = Some(display.clone());
                    }
                }
                (true, Some(display), None)
            }
            _ => (false, None, Some("file_hash_mismatch".to_string())),
        }
    };

    if ok {
        inner
            .mark(&done.transfer_id, TransferState::Done, None)
            .await;
    } else {
        // 失败保留 `.part` + `.bitmap`，用户重试时只补缺失块。
        inner
            .mark(&done.transfer_id, TransferState::Failed, error.clone())
            .await;
    }

    let result = FileResult {
        transfer_id: done.transfer_id.clone(),
        ok,
        path,
        error,
    };
    if let Err(e) = inner
        .send(&peer_id, Frame::json(FrameType::FileResult, &result)?)
        .await
    {
        log::warn!("failed to send file result: {e}");
    }
    Ok(())
}

/// 移除未完成的部分文件与元数据（取消/拒绝/校验失败时调用）。
pub(crate) async fn cleanup_part(inner: &Arc<Inner>, transfer_id: &str) {
    let (part, meta) = {
        let mut tasks = inner.tasks.lock().await;
        let Some(task) = tasks.get_mut(transfer_id) else {
            return;
        };
        task.file.take();
        task.hasher = None;
        task.chunk_file = None;
        task.bitmap = None;
        task.chunk_map = None;
        task.blocks_since_flush = 0;
        task.resume_from = 0;
        task.info.transferred = 0;
        (task.part.clone(), task.meta.clone())
    };
    let bitmap = part.as_deref().map(super::chunkmap::bitmap_path_for);
    remove_quietly(part.as_deref()).await;
    remove_quietly(bitmap.as_deref()).await;
    remove_quietly(meta.as_deref()).await;
}

/// 中断时保留部分文件与元数据，仅关闭写句柄（供续传使用）。
pub(crate) async fn detach_partial(inner: &Arc<Inner>, transfer_id: &str) {
    let mut tasks = inner.tasks.lock().await;
    if let Some(task) = tasks.get_mut(transfer_id) {
        task.file.take();
        task.hasher = None;
        // M11：关闭随机写句柄，位图与块表保留供重试续传。
        task.chunk_file = None;
    }
}

/// 删除文件，忽略不存在的情况。
async fn remove_quietly(path: Option<&Path>) {
    let Some(path) = path else {
        return;
    };
    match tokio::fs::remove_file(path).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => log::warn!("failed to remove partial file: {e}"),
    }
}
