//! In-process TUI testing: VT100 screen parsing and VirtualTui input encoding.
//!
//! Use [`TuiTestkit`] with [`tddy_service::start_virtual_tui_session`] to drive a headless
//! ratatui session from async tests without gRPC or subprocesses.

pub mod input_encoding;
mod tui_testkit;

pub use input_encoding::{encode_resize, event_to_bytes};
pub use tui_testkit::TuiTestkit;
