//! Tauri's build step: embed the configuration, and declare the two commands the webview may call.
//!
//! Application commands go through the same ACL as plugin commands, so a command that is not named
//! here has no permission to grant and is refused at runtime. `capabilities/default.json` grants
//! the two generated `allow-*` permissions to the dashboard window.

fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&["tddy_rpc_connect", "tddy_rpc_send"]),
    ))
    .expect("tauri-build failed")
}
