//! 开发用对端探测程序（v2/M9 spike 联调用，不随应用发布）。
//!
//! 用途：在没有第二台桌面机的情况下，用它扮演"PC 端"：
//! 启动会话中心监听 TCP 端口，Android 客户端通过「添加设备」连接后，
//! 双方互相发送文字并打印到控制台，用于验证 HELLO / 文字 ACK 链路。
//!
//! 运行：`cargo run --offline --example peer_probe -- 28101`

use chatters_lib::session::{start_session, SessionConfig, SessionEvent};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(28101);

    let (hub, mut events, _task) = start_session(SessionConfig::new(
        "desktop-probe",
        "PC-Probe",
        port,
        Duration::from_secs(5),
    ))
    .await?;
    println!("probe ready: listening on 0.0.0.0:{port}");

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
            }
            SessionEvent::TextReceived {
                device_id, text, ..
            } => println!("probe received text from {device_id}: {text}"),
            SessionEvent::PeerDisconnected { device_id } => {
                println!("peer disconnected: {device_id}");
            }
            _ => {}
        }
    }
    Ok(())
}
