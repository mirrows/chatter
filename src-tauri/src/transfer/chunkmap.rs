//! 块级续传地基（M11 / S1）：块表、块位图与位图落盘。
//!
//! 约定（见 `02-technical-design.md` §4.7）：
//! - 块表：每块取 SHA-256 前 8 字节（十六进制 16 字符）；
//! - 位图：位序 = 块索引（低位优先），每 8 块 1 字节，尾部按字节补齐；
//! - 位图文件：`magic "CBM1" + 块数(u32) + 位图长度(u32) + 位图 + crc32`，原子落盘
//!   （临时文件 + 改名）。
//!
//! S1 只提供数据结构与读写能力，**不参与**现有传输流程（`chunkmap` 能力尚未声明）。

use crate::protocol::{FileBlockMap, BLOCK_HASH_HEX_LEN};
use crate::transfer::CHUNK_SIZE_2M;
use sha2::{Digest, Sha256};
use std::path::Path;

/// 每块哈希保存的字节数（SHA-256 前 8 字节）。
pub const HASH_BYTES: usize = 8;
/// 位图文件 magic。
const BITMAP_MAGIC: [u8; 4] = *b"CBM1";

/// 位图文件格式错误。
#[derive(Debug, PartialEq, Eq)]
pub enum BitmapError {
    /// 读取/写入失败。
    Io(String),
    /// magic 不匹配。
    BadMagic,
    /// 文件长度不足或块数不匹配。
    BadLayout,
    /// CRC32 校验失败（位图损坏）。
    BadChecksum,
}

impl std::fmt::Display for BitmapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BitmapError::Io(e) => write!(f, "bitmap io error: {e}"),
            BitmapError::BadMagic => write!(f, "bitmap magic mismatch"),
            BitmapError::BadLayout => write!(f, "bitmap layout mismatch"),
            BitmapError::BadChecksum => write!(f, "bitmap checksum mismatch"),
        }
    }
}

impl std::error::Error for BitmapError {}

/// 计算一块数据的短哈希（SHA-256 前 8 字节）。
pub fn block_hash(data: &[u8]) -> [u8; HASH_BYTES] {
    let digest = Sha256::digest(data);
    let mut out = [0u8; HASH_BYTES];
    out.copy_from_slice(&digest[..HASH_BYTES]);
    out
}

/// 块表：块大小 + 每块短哈希。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkMap {
    /// 块大小（字节，协商结果）。
    block_size: u32,
    /// 每块 8 字节哈希，按块索引排列。
    hashes: Vec<u8>,
}

impl ChunkMap {
    /// 新建空块表（边读边用 `push_hash` 追加）。
    pub fn empty(block_size: u32) -> Result<Self, String> {
        Self::new(block_size, Vec::new())
    }

    /// 追加一块数据的短哈希（流式计算块表时使用）。
    pub fn push_hash(&mut self, data: &[u8]) {
        self.hashes.extend_from_slice(&block_hash(data));
    }

    /// 用块大小与哈希序列构造（`hashes` 长度必须是 8 的整数倍）。
    pub fn new(block_size: u32, hashes: Vec<u8>) -> Result<Self, String> {
        if block_size == 0 || block_size as usize > CHUNK_SIZE_2M {
            return Err(format!("非法的块大小：{block_size}"));
        }
        if !hashes.len().is_multiple_of(HASH_BYTES) {
            return Err("块哈希长度不是 8 的整数倍".to_string());
        }
        Ok(Self { block_size, hashes })
    }

    /// 按顺序读取数据块，流式计算块表（每块取 SHA-256 前 8 字节）。
    pub fn from_reader(
        block_size: u32,
        mut read: impl FnMut(&mut Vec<u8>) -> usize,
    ) -> Result<Self, String> {
        if block_size == 0 || block_size as usize > CHUNK_SIZE_2M {
            return Err(format!("非法的块大小：{block_size}"));
        }
        let mut hashes = Vec::new();
        let mut buf = vec![0u8; block_size as usize];
        loop {
            let len = read(&mut buf);
            if len == 0 {
                break;
            }
            hashes.extend_from_slice(&block_hash(&buf[..len]));
            if len < buf.len() {
                break;
            }
        }
        Self::new(block_size, hashes)
    }

    /// 块大小。
    pub fn block_size(&self) -> u32 {
        self.block_size
    }

    /// 块数量。
    pub fn block_count(&self) -> usize {
        self.hashes.len() / HASH_BYTES
    }

    /// 第 `index` 块的哈希。
    pub fn hash_at(&self, index: usize) -> Option<[u8; HASH_BYTES]> {
        let start = index.checked_mul(HASH_BYTES)?;
        let end = start.checked_add(HASH_BYTES)?;
        let slice = self.hashes.get(start..end)?;
        let mut out = [0u8; HASH_BYTES];
        out.copy_from_slice(slice);
        Some(out)
    }

    /// 转换为协议块表帧（`hashes` 为连续十六进制串）。
    pub fn to_frame(&self, transfer_id: &str) -> FileBlockMap {
        let mut hashes = String::with_capacity(self.hashes.len() * 2);
        for byte in &self.hashes {
            hashes.push_str(&format!("{byte:02x}"));
        }
        FileBlockMap {
            transfer_id: transfer_id.to_string(),
            block_size: self.block_size,
            hashes,
        }
    }

    /// 从协议块表帧还原并校验结构。
    pub fn from_frame(frame: &FileBlockMap) -> Result<Self, String> {
        if !frame.hashes.len().is_multiple_of(BLOCK_HASH_HEX_LEN) {
            return Err("块表哈希长度不是 16 的整数倍".to_string());
        }
        let mut hashes = Vec::with_capacity(frame.hashes.len() / 2);
        let bytes = frame.hashes.as_bytes();
        for pair in bytes.chunks(2) {
            let text =
                std::str::from_utf8(pair).map_err(|_| "块表包含非 ASCII 字符".to_string())?;
            let value = u8::from_str_radix(text, 16)
                .map_err(|_| format!("块表包含非法十六进制：{text}"))?;
            hashes.push(value);
        }
        Self::new(frame.block_size, hashes)
    }
}

/// 位图文件路径（与 `.part` 同目录、同前缀：`<key>.part` → `<key>.bitmap`）。
pub fn bitmap_path_for(part: &Path) -> std::path::PathBuf {
    part.with_extension("bitmap")
}

/// 按 `.part` 现有内容与块表重建位图（位图缺失/损坏时的兜底）。
///
/// 逐块读取并比对短哈希：内容一致即视为已收，避免整文件重传；
/// 预分配产生的未写区域若恰为全零，也会被判定为一致（最终整文件哈希仍能校验通过）。
pub fn rebuild_bitmap(path: &Path, map: &ChunkMap, size: u64) -> Result<BlockBitmap, String> {
    use std::io::{Read, Seek, SeekFrom};
    let block_size = map.block_size() as usize;
    let blocks = map.block_count();
    let mut bitmap = BlockBitmap::new(blocks);
    if blocks == 0 {
        return Ok(bitmap);
    }
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    let mut buf = vec![0u8; block_size];
    for index in 0..blocks {
        let offset = index as u64 * block_size as u64;
        if offset >= len || offset >= size {
            break;
        }
        let want = ((size - offset) as usize).min(block_size);
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| e.to_string())?;
        if file.read_exact(&mut buf[..want]).is_err() {
            break;
        }
        if map.hash_at(index) == Some(block_hash(&buf[..want])) {
            bitmap.set(index);
        }
    }
    Ok(bitmap)
}

/// 块位图：位序 = 块索引（低位优先），每 8 块 1 字节。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockBitmap {
    blocks: usize,
    bits: Vec<u8>,
}

impl BlockBitmap {
    /// 新建空位图（全部块均未收到）。
    pub fn new(blocks: usize) -> Self {
        Self {
            blocks,
            bits: vec![0u8; blocks.div_ceil(8)],
        }
    }

    /// 块数量。
    pub fn blocks(&self) -> usize {
        self.blocks
    }

    /// 第 `index` 块是否已收到（越界返回 false）。
    pub fn get(&self, index: usize) -> bool {
        if index >= self.blocks {
            return false;
        }
        self.bits[index / 8] & (1 << (index % 8)) != 0
    }

    /// 标记第 `index` 块已收到（越界忽略）。
    pub fn set(&mut self, index: usize) {
        if index >= self.blocks {
            return;
        }
        self.bits[index / 8] |= 1 << (index % 8);
    }

    /// 已收到的块数。
    pub fn received(&self) -> usize {
        (0..self.blocks).filter(|index| self.get(*index)).count()
    }

    /// 是否所有块都已收到。
    pub fn all_set(&self) -> bool {
        self.received() == self.blocks
    }

    /// 缺失块的索引（升序）。
    pub fn missing(&self) -> Vec<usize> {
        (0..self.blocks).filter(|index| !self.get(*index)).collect()
    }

    /// 十六进制编码（每字节两位小写）。
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(self.bits.len() * 2);
        for byte in &self.bits {
            out.push_str(&format!("{byte:02x}"));
        }
        out
    }

    /// 从十六进制还原；长度必须恰好覆盖 `blocks` 个块，尾部填充位会被清零。
    pub fn from_hex(hex: &str, blocks: usize) -> Result<Self, String> {
        let expected_bytes = blocks.div_ceil(8);
        if hex.len() != expected_bytes * 2 {
            return Err(format!(
                "位图长度不匹配：期望 {} 个字符，实际 {}",
                expected_bytes * 2,
                hex.len()
            ));
        }
        let mut bits = Vec::with_capacity(expected_bytes);
        for pair in hex.as_bytes().chunks(2) {
            let text =
                std::str::from_utf8(pair).map_err(|_| "位图包含非 ASCII 字符".to_string())?;
            let value = u8::from_str_radix(text, 16)
                .map_err(|_| format!("位图包含非法十六进制：{text}"))?;
            bits.push(value);
        }
        let mut bitmap = Self { blocks, bits };
        bitmap.clear_padding();
        Ok(bitmap)
    }

    /// 清掉最后一个字节里超出块数的填充位，避免误判。
    fn clear_padding(&mut self) {
        let remainder = self.blocks % 8;
        if remainder != 0 {
            if let Some(last) = self.bits.last_mut() {
                *last &= (1u8 << remainder) - 1;
            }
        }
    }
}

/// 位图落盘（原子：临时文件 + 改名）。
pub fn save_bitmap(path: &Path, bitmap: &BlockBitmap) -> Result<(), BitmapError> {
    let blocks = u32::try_from(bitmap.blocks).map_err(|_| BitmapError::BadLayout)?;
    let bits_len = u32::try_from(bitmap.bits.len()).map_err(|_| BitmapError::BadLayout)?;
    let mut body = Vec::with_capacity(12 + bitmap.bits.len() + 4);
    body.extend_from_slice(&BITMAP_MAGIC);
    body.extend_from_slice(&blocks.to_be_bytes());
    body.extend_from_slice(&bits_len.to_be_bytes());
    body.extend_from_slice(&bitmap.bits);
    let crc = crc32(&body);
    body.extend_from_slice(&crc.to_be_bytes());

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| BitmapError::Io(e.to_string()))?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, &body).map_err(|e| BitmapError::Io(e.to_string()))?;
    std::fs::rename(&tmp, path).map_err(|e| BitmapError::Io(e.to_string()))?;
    Ok(())
}

/// 读取位图：magic / 块数 / 长度 / CRC 任一不符即返回错误（调用方可回退重建）。
pub fn load_bitmap(path: &Path, expected_blocks: usize) -> Result<BlockBitmap, BitmapError> {
    let body = std::fs::read(path).map_err(|e| BitmapError::Io(e.to_string()))?;
    if body.len() < 16 || body[..4] != BITMAP_MAGIC {
        return Err(BitmapError::BadMagic);
    }
    let blocks = u32::from_be_bytes([body[4], body[5], body[6], body[7]]) as usize;
    let bits_len = u32::from_be_bytes([body[8], body[9], body[10], body[11]]) as usize;
    if blocks != expected_blocks || bits_len != expected_blocks.div_ceil(8) {
        return Err(BitmapError::BadLayout);
    }
    if body.len() != 12 + bits_len + 4 {
        return Err(BitmapError::BadLayout);
    }
    let (payload, checksum) = body.split_at(12 + bits_len);
    let expected = u32::from_be_bytes([checksum[0], checksum[1], checksum[2], checksum[3]]);
    if crc32(payload) != expected {
        return Err(BitmapError::BadChecksum);
    }
    let mut bitmap = BlockBitmap {
        blocks,
        bits: payload[12..].to_vec(),
    };
    bitmap.clear_padding();
    Ok(bitmap)
}

/// CRC32（IEEE 802.3，与 zlib 一致）；自实现避免新增依赖。
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in data {
        crc = (crc >> 8) ^ CRC32_TABLE[((crc ^ u32::from(*byte)) & 0xFF) as usize];
    }
    !crc
}

/// CRC32 查表（编译期生成）。
const CRC32_TABLE: [u32; 256] = build_crc32_table();

const fn build_crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut index = 0;
    while index < 256 {
        let mut crc = index as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                0xEDB8_8320 ^ (crc >> 1)
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[index] = crc;
        index += 1;
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::FileBlockMap;

    /// 生成独立临时目录（测试结束由调用方清理）。
    fn temp_root(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("chatters-chunkmap-{name}-{}", uuid::Uuid::new_v4()))
    }

    /// 正常路径：CRC32 与参考值一致（zlib/IEEE）。
    #[test]
    fn crc32_matches_reference() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    /// 正常路径：位图置位/读取/缺失统计与全满判断。
    #[test]
    fn bitmap_set_get_and_missing() {
        let mut bitmap = BlockBitmap::new(10);
        assert_eq!(bitmap.received(), 0);
        assert_eq!(bitmap.missing(), (0..10).collect::<Vec<_>>());

        bitmap.set(0);
        bitmap.set(9);
        assert!(bitmap.get(0));
        assert!(!bitmap.get(1));
        assert_eq!(bitmap.received(), 2);
        assert_eq!(bitmap.missing(), vec![1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(!bitmap.all_set());

        // 越界 set/get 不 panic、不改变统计
        bitmap.set(99);
        assert!(!bitmap.get(99));

        for index in 0..10 {
            bitmap.set(index);
        }
        assert!(bitmap.all_set());
        assert!(bitmap.missing().is_empty());
    }

    /// 正常路径：拒绝非十六进制字符（异常输入）。
    #[test]
    fn bitmap_from_hex_rejects_bad_input() {
        assert!(BlockBitmap::from_hex("0", 8).is_err());
        assert!(BlockBitmap::from_hex("zz", 8).is_err());
    }

    /// 正常路径：十六进制位图互转，且尾部填充位会被清零。
    #[test]
    fn bitmap_hex_round_trip_and_padding() {
        let mut bitmap = BlockBitmap::new(10);
        bitmap.set(1);
        bitmap.set(8);
        let hex = bitmap.to_hex();
        assert_eq!(hex, "0201");
        let restored = BlockBitmap::from_hex(&hex, 10).expect("decode");
        assert_eq!(restored, bitmap);

        // 3 块时 "ff" 的填充位（3..8）应被清掉
        let padded = BlockBitmap::from_hex("ff", 3).expect("decode");
        assert!(padded.all_set());
        assert!(!padded.get(3));
    }

    /// 正常路径：位图落盘后可回读；损坏时按类型报错（供调用方重建）。
    #[test]
    fn bitmap_save_and_load_round_trip() {
        let root = temp_root("bitmap");
        let path = root.join("abc.bitmap");
        let mut bitmap = BlockBitmap::new(10);
        bitmap.set(0);
        bitmap.set(7);

        save_bitmap(&path, &bitmap).expect("save");
        assert_eq!(load_bitmap(&path, 10).expect("load"), bitmap);

        // 块数不符
        assert_eq!(load_bitmap(&path, 11), Err(BitmapError::BadLayout));

        // CRC 损坏
        let mut body = std::fs::read(&path).expect("read");
        let last = body.len() - 1;
        body[last] ^= 0xFF;
        std::fs::write(&path, &body).expect("write");
        assert_eq!(load_bitmap(&path, 10), Err(BitmapError::BadChecksum));

        // magic 损坏
        let mut body = std::fs::read(&path).expect("read");
        body[0] = b'X';
        std::fs::write(&path, &body).expect("write");
        assert_eq!(load_bitmap(&path, 10), Err(BitmapError::BadMagic));

        std::fs::remove_dir_all(&root).ok();
    }

    /// 正常路径：位图缺失时按 `.part` 内容与块表重建（只标记内容一致的块）。
    #[test]
    fn rebuild_bitmap_from_partial_data() {
        let root = temp_root("rebuild");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("data.part");
        // 3 块：4 + 4 + 2 字节，块大小 4。
        let payload: Vec<u8> = (0..10u8).collect();
        let mut map = ChunkMap::empty(4).unwrap();
        for chunk in payload.chunks(4) {
            map.push_hash(chunk);
        }

        std::fs::write(&path, &payload).unwrap();
        let bitmap = rebuild_bitmap(&path, &map, payload.len() as u64).unwrap();
        assert!(bitmap.all_set(), "完整内容应重建为全满");

        // 只写入前 5 字节（第 0 块完整、第 1 块不完整）：仅第 0 块被标记。
        std::fs::write(&path, &payload[..5]).unwrap();
        let bitmap = rebuild_bitmap(&path, &map, payload.len() as u64).unwrap();
        assert_eq!(bitmap.missing(), vec![1, 2]);

        // 内容与块表不一致：不标记（避免把损坏数据当成已收）。
        let wrong: Vec<u8> = payload.iter().map(|byte| byte ^ 0xFF).collect();
        std::fs::write(&path, &wrong).unwrap();
        let bitmap = rebuild_bitmap(&path, &map, payload.len() as u64).unwrap();
        assert_eq!(bitmap.missing(), vec![0, 1, 2]);

        std::fs::remove_dir_all(&root).ok();
    }

    /// 正常路径：流式读取生成块表，与协议帧互转后保持一致。
    #[test]
    fn chunk_map_from_reader_and_frame_round_trip() {
        let data = vec![7u8; 5 * 1024];
        let block_size = 1024u32;
        let mut offset = 0usize;
        let map = ChunkMap::from_reader(block_size, |buf| {
            let remaining = data.len() - offset;
            let take = remaining.min(buf.len());
            buf[..take].copy_from_slice(&data[offset..offset + take]);
            offset += take;
            take
        })
        .expect("build map");

        assert_eq!(map.block_size(), 1024);
        assert_eq!(map.block_count(), 5);
        assert_eq!(map.hash_at(0), Some(block_hash(&data[..1024])));
        assert_eq!(map.hash_at(5), None);

        let frame = map.to_frame("t-1");
        assert_eq!(frame.block_count(), 5);
        assert_eq!(frame.hash_at(0), Some(block_hash(&data[..1024])));

        let restored = ChunkMap::from_frame(&frame).expect("restore");
        assert_eq!(restored, map);
    }

    /// 异常：非法块大小、非 8 倍数哈希、非法十六进制都被拒绝。
    #[test]
    fn chunk_map_rejects_bad_input() {
        assert!(ChunkMap::new(0, Vec::new()).is_err());
        assert!(ChunkMap::new(CHUNK_SIZE_2M as u32 + 1, Vec::new()).is_err());
        assert!(ChunkMap::new(1024, vec![0u8; 3]).is_err());

        let too_long = FileBlockMap {
            transfer_id: "t-1".to_string(),
            block_size: 1024,
            hashes: "0".to_string(),
        };
        assert!(ChunkMap::from_frame(&too_long).is_err());

        let bad_hex = FileBlockMap {
            transfer_id: "t-1".to_string(),
            block_size: 1024,
            hashes: "zzzzzzzzzzzzzzzz".to_string(),
        };
        assert!(ChunkMap::from_frame(&bad_hex).is_err());
    }
}
