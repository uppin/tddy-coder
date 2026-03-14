//! tddy-tui: Ratatui View layer for tddy-coder.
//!
//! Implements PresenterView for the TUI. Key mapping: KeyEvent → UserIntent.
//! View-local state: scroll, text buffers, selection cursor.

pub mod capturing_writer;
pub mod event_loop;
pub mod key_map;
pub mod layout;
pub mod raw;
pub mod render;
pub mod tui_view;
pub mod ui;
pub mod view_state;
pub mod virtual_tui;

pub use capturing_writer::{ByteCallback, CapturingWriter};
pub use event_loop::run_event_loop;
pub use key_map::key_event_to_intent;
pub use raw::{disable_raw_mode, enable_raw_mode_keep_sig};
pub use tui_view::TuiView;
pub use view_state::ViewState;
pub use virtual_tui::run_virtual_tui;
