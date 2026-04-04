//! In-process TUI testing: VT100 screen parsing and VirtualTui input encoding.
//!
//! Use [`TuiTestkit`] with [`tddy_service::start_virtual_tui_session`] to drive a headless
//! ratatui session from async tests without gRPC or subprocesses.

pub mod input_encoding;
pub mod screen_parser;
mod tui_testkit;

pub use input_encoding::{encode_resize, event_to_bytes};
pub use screen_parser::{
    assert_segmented_echo, compact_screen, compact_screen_for_echo_assertions,
    eventually_segmented_echo, longest_echo_prefix_len_in_compact, longest_echo_prefix_raw_compact,
    segmented_echo_complete, ScreenParser, SegmentedEchoFailureStyle, SegmentedEchoWaitParams,
};
pub use tui_testkit::TuiTestkit;
