//! 配置读写模块（M2-a）。
//!
//! 配置以 JSON 文件形式存放在应用数据目录，包含昵称、device_id、端口、
//! 下载目录、主题。此模块为纯 Rust（无 Tauri 依赖），便于单元测试。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// 默认设备发现端口（UDP）。
pub const DEFAULT_DISCOVERY_PORT: u16 = 28100;
/// 默认 TCP 通信端口。
pub const DEFAULT_TCP_PORT: u16 = 28101;

/// 应用配置。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppConfig {
    /// 设备唯一 ID（首装生成，持久化）。
    pub device_id: String,
    /// 设备昵称。
    pub nickname: String,
    /// UDP 发现端口。
    pub discovery_port: u16,
    /// TCP 通信端口。
    pub tcp_port: u16,
    /// 下载目录（绝对路径）。
    pub download_dir: String,
    /// 主题："light" 或 "dark"。
    pub theme: String,
    /// 是否自动接收图片（F34，默认开启）。
    #[serde(default = "default_true")]
    pub auto_accept_images: bool,
    /// 是否自动接收普通文件（F34/F52，默认关闭，需用户确认）。
    #[serde(default)]
    pub auto_accept_files: bool,
}

/// serde 默认值：true（用于旧配置缺字段时保持图片自动接收）。
fn default_true() -> bool {
    true
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            device_id: uuid::Uuid::new_v4().to_string(),
            nickname: default_nickname(),
            discovery_port: DEFAULT_DISCOVERY_PORT,
            tcp_port: DEFAULT_TCP_PORT,
            download_dir: default_download_dir(),
            theme: "light".to_string(),
            auto_accept_images: true,
            auto_accept_files: false,
        }
    }
}

/// 生成默认昵称，如 "PC-8f3a"（取随机 UUID 前 4 位十六进制）。
pub fn default_nickname() -> String {
    let hex = uuid::Uuid::new_v4().simple().to_string();
    format!("PC-{}", &hex[..4])
}

/// 默认下载目录：用户目录下的 Downloads/Chatters（无法定位用户目录时回退当前目录）。
pub fn default_download_dir() -> String {
    crate::platform::paths::download_dir()
        .to_string_lossy()
        .to_string()
}

/// 配置存储错误。
#[derive(Debug)]
pub enum ConfigError {
    /// 读取或解析失败。
    Io(std::io::Error),
    /// JSON 反序列化失败。
    Parse(serde_json::Error),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "config io error: {}", e),
            ConfigError::Parse(e) => write!(f, "config parse error: {}", e),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError::Io(e)
    }
}

impl From<serde_json::Error> for ConfigError {
    fn from(e: serde_json::Error) -> Self {
        ConfigError::Parse(e)
    }
}

/// 读取配置文件；文件不存在时返回默认配置（不落盘，由调用方决定是否保存）。
pub fn load(path: &Path) -> Result<AppConfig, ConfigError> {
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let text = fs::read_to_string(path)?;
    Ok(serde_json::from_str::<AppConfig>(&text)?)
}

/// 保存配置到文件（父目录不存在则创建）。
pub fn save(path: &Path, config: &AppConfig) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(config)?;
    fs::write(path, text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 正常路径：保存后能读回相同配置。
    #[test]
    fn save_then_load_roundtrip() {
        let dir = std::env::temp_dir().join("chatters_cfg_test_1");
        let path = dir.join("config.json");
        let cfg = AppConfig {
            nickname: "tester".to_string(),
            theme: "dark".to_string(),
            ..AppConfig::default()
        };
        save(&path, &cfg).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded, cfg);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 边界：文件不存在时返回默认配置。
    #[test]
    fn load_missing_file_returns_default() {
        let path = std::env::temp_dir().join("chatters_cfg_no_such_file.json");
        std::fs::remove_file(&path).ok();
        let cfg = load(&path).unwrap();
        assert_eq!(cfg.discovery_port, DEFAULT_DISCOVERY_PORT);
        assert_eq!(cfg.tcp_port, DEFAULT_TCP_PORT);
        assert!(!cfg.device_id.is_empty());
    }

    /// 异常：损坏的 JSON 内容应返回 Parse 错误而非 panic。
    #[test]
    fn load_corrupt_json_returns_parse_error() {
        let dir = std::env::temp_dir().join("chatters_cfg_test_3");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, "not a json {{{").unwrap();
        match load(&path) {
            Err(ConfigError::Parse(_)) => { /* expected */ }
            other => panic!("expected parse error, got {:?}", other.map(|_| ())),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 默认昵称格式符合 "PC-xxxx"。
    #[test]
    fn default_nickname_format() {
        let n = default_nickname();
        assert!(n.starts_with("PC-") && n.len() == 7);
    }
}
