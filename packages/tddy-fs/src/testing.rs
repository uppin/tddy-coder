//! Test support for crates migrating onto the port.

use std::path::Path;

/// Every place under `crate_dir/src` where production code reaches the disk without the port:
/// `std::fs`, `tokio::fs`, or a bare `fs::` path, as `"<relative file>:<line>: <text>"`.
///
/// Test code is not production code: a `#[cfg(test)]` item (a `mod tests { … }` included) is
/// skipped, as are comments, so a migrated crate's own tests may keep using a temp directory.
/// `build.rs` is outside `src/` and never scanned.
#[must_use]
pub fn direct_disk_io_in(_crate_dir: &Path) -> Vec<String> {
    // TODO(fs-crate): implement
    vec!["TODO(fs-crate): implement".to_string()]
}
