//! Builders for the files an agent leaves behind, and the built-in bindings that read them.
//!
//! A transcript is written line by line the way Claude Code writes one: one JSON line per
//! content block, every block of one API message repeating that message's `id` and `usage`.

#![allow(dead_code)]

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use tddy_usage::token_accounting::ConversationRecord;
use tddy_usage::{UsageContext, UsageSection};

/// The `usage` object of one API message.
#[derive(Debug, Clone, Copy)]
pub struct Usage {
    pub input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub output: u64,
}

/// Fresh input and output only — no cache traffic.
pub fn usage(input: u64, output: u64) -> Usage {
    Usage {
        input,
        cache_read: 0,
        cache_write: 0,
        output,
    }
}

impl Usage {
    pub fn with_cache(mut self, cache_read: u64, cache_write: u64) -> Self {
        self.cache_read = cache_read;
        self.cache_write = cache_write;
        self
    }
}

/// A temporary HOME an agent writes its files under.
pub struct AgentHome {
    dir: tempfile::TempDir,
}

pub fn an_agent_home() -> AgentHome {
    AgentHome {
        dir: tempfile::tempdir().expect("tempdir"),
    }
}

impl AgentHome {
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// A transcript at `relative` under this home, created empty.
    pub fn transcript(&self, relative: &str) -> Transcript {
        let path = self.dir.path().join(relative);
        fs::create_dir_all(path.parent().expect("a transcript has a parent dir")).expect("mkdir");
        fs::write(&path, "").expect("create transcript");
        Transcript { path }
    }

    /// A JSON sidecar at `relative` under this home.
    pub fn sidecar(&self, relative: &str, json: serde_json::Value) {
        let path = self.dir.path().join(relative);
        fs::create_dir_all(path.parent().expect("a sidecar has a parent dir")).expect("mkdir");
        fs::write(&path, json.to_string()).expect("write sidecar");
    }

    /// A session reading this home, its main conversation `conversation_id`.
    pub fn session(&self, conversation_id: &str) -> UsageContext {
        UsageContext {
            agent_home: Some(self.dir.path().to_path_buf()),
            session_dir: self.dir.path().join("session"),
            conversation_id: Some(conversation_id.to_string()),
            worktree: None,
            agent_label: "claude".to_string(),
            fallback_model: "fallback-model".to_string(),
        }
    }
}

/// One transcript file, appended to line by line.
pub struct Transcript {
    path: PathBuf,
}

impl Transcript {
    /// One API message written as a single line.
    pub fn assistant_message(self, id: &str, model: &str, usage: Usage) -> Self {
        self.assistant_message_in_blocks(id, model, usage, 1)
    }

    /// One API message written as `blocks` lines — one per content block — each repeating the
    /// message's id and usage, as Claude Code writes them.
    pub fn assistant_message_in_blocks(
        self,
        id: &str,
        model: &str,
        usage: Usage,
        blocks: usize,
    ) -> Self {
        for _ in 0..blocks {
            self.append(serde_json::json!({
                "type": "assistant",
                "message": {
                    "id": id,
                    "model": model,
                    "usage": {
                        "input_tokens": usage.input,
                        "cache_read_input_tokens": usage.cache_read,
                        "cache_creation_input_tokens": usage.cache_write,
                        "output_tokens": usage.output,
                    }
                }
            }));
        }
        self
    }

    /// A user turn — a line the binding's filter drops.
    pub fn user_turn(self) -> Self {
        self.append(
            serde_json::json!({ "type": "user", "message": { "role": "user", "content": "go" } }),
        );
        self
    }

    /// A line that is not JSON at all.
    pub fn garbage_line(self) -> Self {
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&self.path)
            .expect("open");
        writeln!(file, "{{not json").expect("write");
        self
    }

    fn append(&self, line: serde_json::Value) {
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&self.path)
            .expect("open");
        writeln!(file, "{line}").expect("write");
    }
}

/// The `usage:` section of a built-in spec, parsed from the shipped YAML file — so a test proves
/// the binding as it ships, not a struct written for the test.
pub fn the_builtin_binding(spec_file: &str) -> UsageSection {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("tddy-usage lives under packages/")
        .join("tddy-coding-agent-specs")
        .join("specs")
        .join(spec_file);
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let spec: serde_yaml::Value = serde_yaml::from_str(&text).expect("spec is YAML");
    let usage = spec
        .get("extensions")
        .and_then(|e| e.get("coding_agent"))
        .and_then(|a| a.get("usage"))
        .unwrap_or_else(|| panic!("{spec_file} declares no extensions.coding_agent.usage"))
        .clone();
    serde_yaml::from_value(usage).unwrap_or_else(|e| panic!("{spec_file} usage: {e}"))
}

/// The row whose id is `id`.
pub fn the_row<'a>(records: &'a [ConversationRecord], id: &str) -> &'a ConversationRecord {
    records
        .iter()
        .find(|r| r.id == id)
        .unwrap_or_else(|| panic!("no row {id} in {records:#?}"))
}

/// Each row as `(id, parent_id)`, in snapshot order.
pub fn the_tree(records: &[ConversationRecord]) -> Vec<(String, Option<String>)> {
    records
        .iter()
        .map(|r| (r.id.clone(), r.parent_id.clone()))
        .collect()
}
