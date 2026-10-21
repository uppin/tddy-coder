# 2026-10-21 — The tddy-tools binary is built by a wiring crate

`#satellite` 15/15. `tddy-tools` behaves as before: same binary name and path, subcommands, flags,
environment variables and exit codes. The `tddy-tools` library no longer depends on LiveKit, and the
binary is built by its own wiring package, `tddy-tools-cli`, which always binds LiveKit as the room
transport; the build without LiveKit (`--no-default-features`) is gone. Building the binary by
package name uses `cargo build -p tddy-tools-cli`. With this, no library crate in the workspace
names LiveKit.
