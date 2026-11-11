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

mod context_file;
mod counter_expr;
mod field_path;
mod gather;
mod jsonl;
mod registry;
mod section;
mod template;
pub mod token_accounting;
mod tree;

pub use context_file::{
    gather_session_usage_for, read_usage_context, write_usage_context, UsageContextFile,
    USAGE_CONTEXT_FILE,
};
pub use counter_expr::CounterExpr;
pub use field_path::FieldPath;
pub use gather::{account, gather_session_usage, merge_sources, UsageContext};
pub use registry::{
    register_conversation, registered_conversations, RegisteredConversation,
    CONVERSATION_REGISTRY_FILE,
};
pub use section::{
    validate_usage_section, Aggregate, ChildrenBy, ChildrenSpec, CounterMap, FieldEq, IdentitySpec,
    JsonlSource, MainMatch, MainSpec, NameSpec, RecordSpec, UsageSection, UsageSource,
};
pub use template::{PathTemplate, TemplateError};
