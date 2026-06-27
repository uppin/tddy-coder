//! macOS Seatbelt (`sandbox-exec`) sandbox implementation.

mod profile;
mod spawn;

pub use profile::render_profile;
pub use spawn::{detect_allow_read_paths, spawn};
