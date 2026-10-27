//! Chatters Tauri backend entry.
//!
//! Currently the application is at M2-a (scaffold stage): only config and
//! discovery scaffolding exist. Network modules are added feature by feature
//! following `rules.md` (one feature, its tests, then the next).

pub mod config;
pub mod discovery;

/// Runs the Tauri application.
///
/// Error handling: `tauri::Error` is returned to the caller (main), which
/// currently logs and exits non-zero.
pub fn run() -> Result<(), tauri::Error> {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .run(tauri::generate_context!())
}

#[cfg(test)]
mod tests {
    // Scaffold smoke test: the lib compiles and links.
    #[test]
    fn scaffold_compiles() {
        // Compile-only check; use a value with a stable identity.
        assert_eq!(std::mem::size_of::<u64>(), 8);
    }
}
