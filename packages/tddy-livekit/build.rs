//! Link configuration for the LiveKit WebRTC FFI. The RPC envelope proto is compiled once, in
//! `tddy-rpc` (`tddy_rpc::envelope`), and re-exported here — see `src/lib.rs`'s `proto` module.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // WebRTC (via livekit-ffi) bundles Objective-C categories on NSString.
    // Without -ObjC, the linker strips them, causing "unrecognized selector" at runtime.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "macos" || target_os == "ios" {
        println!("cargo:rustc-link-arg=-ObjC");
    }
    Ok(())
}
