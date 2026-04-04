//! tddy-daemon library — shared by binary and tests.

pub mod agent_list_mapping;
pub mod auth;
pub mod config;
pub mod connection_service;
pub mod multi_host;
pub mod project_storage;
pub mod server;
pub mod session_deletion;
pub mod session_list_enrichment;
pub mod session_reader;
pub mod session_workflow_files;
pub mod spawn_worker;
pub mod spawner;
pub mod tddy_user_config;
pub mod telegram_notifier;
pub mod telegram_session_subscriber;
pub mod token_provider;
pub mod user_sessions_path;
pub mod worktrees;
