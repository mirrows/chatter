//! 开发用对端探测程序（v2/M9 spike 联调用，不随应用发布）。
//!
//! 用途：在没有第二台桌面机的情况下，用它扮演"PC 端"：
//! 启动会话中心监听 TCP 端口，Android 客户端通过「添加设备」连接后，
//! 双方互相发送文字并打印到控制台；可选第 2 个参数指定图片/文件路径，
//! 连接成功后自动发起一次文件传输（用于验证 Android 接收与 SAF 导出）。
//!
//! 运行：`cargo run --offline --example peer_probe -- 28101 [C:\path\to\image.png]`

use chatters_lib::session::{start_session, SessionConfig, SessionEvent};
use chatters_lib::transfer::{TransferConfig, TransferEvent, TransferManager};
use std::path::PathBuf;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(28101);
    let file_to_send: Option<PathBuf> = std::env::args().nth(2).map(PathBuf::from);

    let (hub, mut events, _task) = start_session(SessionConfig::new(
        "desktop-probe",
        "PC-Probe",
        port,
        Duration::from_secs(5),
    ))
    .await?;

    // 探测端也做接收方，便于验证 Android → PC 的文件方向。
    let downloads = std::env::temp_dir().join("chatters-probe-downloads");
    let (transfer_tx, mut transfer_rx) = tokio::sync::mpsc::channel::<TransferEvent>(64);
    let manager = TransferManager::new(
        TransferConfig::new(downloads, true),
        hub.clone(),
        transfer_tx,
    );
    manager.spawn_maintenance();
    tokio::spawn(async move {
        while let Some(TransferEvent::Updated(info)) = transfer_rx.recv().await {
            println!(
                "transfer {} [{}] {}/{} bytes",
                info.filename,
                info.state.as_str(),
                info.transferred,
                info.size
            );
        }
    });

    println!("probe ready: listening on 0.0.0.0:{port}");
    if let Some(path) = &file_to_send {
        println!("probe will offer file after connect: {}", path.display());
    }

    while let Some(event) = events.recv().await {
        match event {
            SessionEvent::PeerConnected {
                device_id,
                name,
                addr,
            } => {
                println!("peer connected: id={device_id} name={name} addr={addr}");
                match hub
                    .send_text(&device_id, &format!("probe says hi to {name}"))
                    .await
                {
                    Ok(result) => println!("probe sent text: status={:?}", result.status),
                    Err(e) => println!("probe send failed: {e}"),
                }
                if let Some(path) = &file_to_send {
                    match manager.send_file(&device_id, path).await {
                        Ok(info) => println!(
                            "probe offered file: {} ({} bytes, kind={})",
                            info.filename,
                            info.size,
                            info.kind.as_str()
                        ),
                        Err(e) => println!("probe send_file failed: {e}"),
                    }
                }
            }
            SessionEvent::TextReceived {
                device_id, text, ..
            } => println!("probe received text from {device_id}: {text}"),
            // 关键：把传输帧交给 TransferManager，否则收不到 ACCEPT/RESULT，双方都会超时。
            SessionEvent::TransferFrame { device_id, frame } => {
                if let Err(e) = manager.handle_frame(&device_id, frame).await {
                    println!("probe transfer frame error: {e}");
                }
            }
            SessionEvent::PeerDisconnected { device_id } => {
                println!("peer disconnected: {device_id}");
                manager.handle_peer_disconnected(&device_id).await;
            }
            _ => {}
        }
    }
    Ok(())
}
