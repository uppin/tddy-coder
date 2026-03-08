//! TUI module: ratatui + crossterm based terminal interface.
//!
//! Layout:
//!   ┌────────────────────────────────┐
//!   │  Activity Log (scrollable)     │
//!   ├────────────────────────────────┤
//!   │  Status Bar (1 line)           │
//!   ├────────────────────────────────┤
//!   │  Prompt Bar (fixed bottom)     │
//!   └────────────────────────────────┘

#![allow(dead_code)]

pub mod event;
pub mod input;
pub mod layout;
pub mod render;
pub mod run;
pub mod state;
pub mod ui;
