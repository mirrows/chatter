//! 系统分享入口（v3）：Android `ACTION_SEND` → 前端选择设备后发送。
//!
//! 数据流：
//! 1. `MainActivity` 收到分享 → 写入 Kotlin 侧 `ShareInbox`（进程内单例）；
//! 2. 前端启动或回到前台时调用 `take_share_payload` → 插件命令 `takeShare` 取走并清空；
//! 3. 文本直接发送；文件由前端选设备后调用 `import_shared_file(uri)` 复制到缓存再发送。
//!
//! 桌面端没有系统分享入口，`take_pending` 恒为 `None`。

use serde::{Deserialize, Serialize};

/// 分享内容（文本或单个文件）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharePayload {
    /// 类型：`text` 或 `file`。
    pub kind: String,
    /// 文本内容（`kind = text`）。
    #[serde(default)]
    pub text: Option<String>,
    /// `content://` uri（`kind = file`）。
    #[serde(default)]
    pub uri: Option<String>,
    /// 文件名（可选，来自 `EXTRA_TITLE`）。
    #[serde(default)]
    pub name: Option<String>,
}

/// 取走待处理的分享；Android 之外恒为 `None`。
pub fn take_pending() -> Option<SharePayload> {
    #[cfg(target_os = "android")]
    {
        crate::platform::android::take_share()
    }
    #[cfg(not(target_os = "android"))]
    {
        None
    }
}

/// 校验分享内容（纯函数，便于单测）：类型必须是 text/file 且内容非空。
pub fn validate(payload: &SharePayload) -> Result<(), String> {
    match payload.kind.as_str() {
        "text" => match payload.text.as_deref() {
            Some(text) if !text.trim().is_empty() => Ok(()),
            _ => Err("分享的文本为空".to_string()),
        },
        "file" => match payload.uri.as_deref() {
            Some(uri) if !uri.trim().is_empty() => Ok(()),
            _ => Err("分享的文件为空".to_string()),
        },
        other => Err(format!("不支持的分享类型：{other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 正常路径：文本非空、文件 uri 非空都可接受。
    #[test]
    fn validate_accepts_text_and_file() {
        let text = SharePayload {
            kind: "text".to_string(),
            text: Some("hello".to_string()),
            uri: None,
            name: None,
        };
        assert!(validate(&text).is_ok());

        let file = SharePayload {
            kind: "file".to_string(),
            text: None,
            uri: Some("content://media/1".to_string()),
            name: Some("a.png".to_string()),
        };
        assert!(validate(&file).is_ok());
    }

    /// 异常：空文本、空 uri、未知类型都拒绝。
    #[test]
    fn validate_rejects_empty_and_unknown() {
        let empty_text = SharePayload {
            kind: "text".to_string(),
            text: Some("   ".to_string()),
            uri: None,
            name: None,
        };
        assert_eq!(validate(&empty_text), Err("分享的文本为空".to_string()));

        let empty_file = SharePayload {
            kind: "file".to_string(),
            text: None,
            uri: None,
            name: None,
        };
        assert_eq!(validate(&empty_file), Err("分享的文件为空".to_string()));

        let unknown = SharePayload {
            kind: "video".to_string(),
            text: None,
            uri: None,
            name: None,
        };
        assert_eq!(
            validate(&unknown),
            Err("不支持的分享类型：video".to_string())
        );
    }
}
