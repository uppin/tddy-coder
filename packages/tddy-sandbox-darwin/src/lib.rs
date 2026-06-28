//! macOS Seatbelt (`sandbox-exec`) sandbox implementation.

mod profile;
pub mod runner;
mod spawn;

pub use profile::render_profile;
pub use runner::{connect_sandbox_client, run_sandbox_runner, SandboxRunnerArgs};
pub use spawn::{detect_allow_read_paths, spawn};
