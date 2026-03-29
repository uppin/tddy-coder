//! Client-side sandbox path normalization (rsync-facing) — mirrors daemon VFS rules.

pub use tddy_service::sandbox_path::sandbox_relative_path as normalize_sandbox_relative_path;
