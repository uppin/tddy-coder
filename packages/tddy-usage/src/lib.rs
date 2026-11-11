//! Per-conversation token accounting that names no coding agent.
//!
//! An agent's spec declares where its usage lives and how to read it — a [`UsageSection`] under
//! `extensions.coding_agent.usage` — and this crate evaluates any such binding over the files the
//! agent writes: path templates locate a conversation's files and its children's, a record filter
//! and field mapping read each line, and an aggregation folds them into one
//! [`token_accounting::ConversationRecord`] per conversation. Which agent wrote the files is data
//! the caller passes in; nothing here knows one agent from another.
//!
//! Feature: docs/ft/coder/session-token-accounting.md

mod field_path;
mod gather;
mod jsonl;
mod section;
mod template;
pub mod token_accounting;
mod tree;

pub use field_path::FieldPath;
pub use gather::{account, gather_session_usage, UsageContext};
pub use section::{
    validate_usage_section, Aggregate, ChildrenSpec, CounterMap, FieldEq, JsonlSource, MainSpec,
    NameSpec, RecordSpec, UsageSection, UsageSource,
};
pub use template::{PathTemplate, TemplateError};
