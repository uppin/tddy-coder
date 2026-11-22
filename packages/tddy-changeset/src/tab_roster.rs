//! A session's **tab roster**: every terminal tab it opened, each as the **launch chain** it was
//! started through — an ordered list of nodes, outermost first, each a `kind` and a free-form
//! `properties` map.
//!
//! Stored as `tabs.yaml` beside `.session.yaml`, not inside it:
//!
//! - `.session.yaml` is `deny_unknown_fields`, so a new key there would make every daemon that
//!   predates it unable to read the session at all;
//! - `.session.yaml` is rewritten by unlocked read-modify-writes (hook and status updates), which
//!   would clobber a roster written beside them. This file has one writer, the daemon's terminal
//!   roster, which serialises its rewrites.
//!
//! Unknown content survives every rewrite: a node kind, a property key, an entry key or a top-level
//! key this daemon does not know is read and written back verbatim, so a later wrapper (`ssh` with
//! `host`, …) needs no schema change, and an older daemon never drops what a newer one wrote.
//!
//! ```yaml
//! tabs:
//!   - terminal_id: main
//!     launch:
//!       - kind: tmux
//!         properties: { session: tddy-01a1…, os_user: dev }
//!       - kind: agent
//!         properties: { harness: claude-cli, conversation_id: 5f3c… }
//! ```
//!
//! PRD: docs/ft/daemon/session-liveness.md

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The file this module reads and writes, inside a session directory.
pub const TAB_ROSTER_FILENAME: &str = "tabs.yaml";

/// The node kind of an agent: the harness a tab runs, and the conversation it holds.
pub const AGENT_KIND: &str = "agent";
/// The node kind of the session user's login shell.
pub const SHELL_KIND: &str = "shell";
/// The node kind of a tmux session the daemon created to wrap a tab.
pub const TMUX_KIND: &str = "tmux";
/// The node kind of a tab attached to the user's own tmux session.
pub const TMUX_ATTACH_KIND: &str = "tmux-attach";
/// The node kind of a jail of its own a tab runs in: a `tddy-sandbox-runner` over its session's
/// worktree and sandbox grants, wrapping what the tab runs (`#session-liveness` 5/8).
pub const SANDBOX_KIND: &str = "sandbox";
/// The node kind of a session's target a tab runs on: the machine the session's `.session.yaml`
/// `target` names, reached through its remote shell, in the session's worktree there, wrapping what
/// the tab runs (`#session-liveness` 6/8).
pub const TARGET_KIND: &str = "target";

/// An [`AGENT_KIND`] node's harness: the session type it runs (`claude-cli`, …).
pub const HARNESS_PROPERTY: &str = "harness";
/// An [`AGENT_KIND`] node's conversation id.
pub const CONVERSATION_ID_PROPERTY: &str = "conversation_id";
/// A [`TMUX_KIND`] or [`TMUX_ATTACH_KIND`] node's tmux session name.
pub const SESSION_PROPERTY: &str = "session";
/// A [`TMUX_KIND`] or [`TMUX_ATTACH_KIND`] node's OS user: tmux servers are per uid, and the session
/// records no user.
pub const OS_USER_PROPERTY: &str = "os_user";
/// A [`TMUX_KIND`] node's agent start: when the process its pane runs started, in whole seconds
/// since the Unix epoch — what tells that process from whatever later holds its pid, so a delete
/// signals the agent and nothing else (`#tab-hardening` 2/4). Absent for a node recorded before it
/// existed, and for a tab's tmux node.
pub const START_PROPERTY: &str = "start";

/// Every tab a session opened, in the order they were recorded.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TabRoster {
    #[serde(default)]
    pub tabs: Vec<TabEntry>,
    /// Top-level keys this daemon does not know, kept verbatim.
    #[serde(flatten)]
    pub unknown: BTreeMap<String, serde_yaml::Value>,
}

/// One tab: its terminal id, and the chain it was launched through, outermost first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabEntry {
    pub terminal_id: String,
    #[serde(default)]
    pub launch: Vec<LaunchNode>,
    /// Entry keys this daemon does not know, kept verbatim.
    #[serde(flatten)]
    pub unknown: BTreeMap<String, serde_yaml::Value>,
}

/// One link of a launch chain: what it is, and what it was started with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaunchNode {
    /// [`AGENT_KIND`], [`SHELL_KIND`], [`TMUX_KIND`], [`TMUX_ATTACH_KIND`], [`SANDBOX_KIND`],
    /// [`TARGET_KIND`], or a kind a later daemon added.
    pub kind: String,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
    /// Node keys this daemon does not know, kept verbatim.
    #[serde(flatten)]
    pub unknown: BTreeMap<String, serde_yaml::Value>,
}

impl TabEntry {
    /// The entry of `terminal_id`, launched through `launch` (outermost first).
    pub fn new(terminal_id: impl Into<String>, launch: Vec<LaunchNode>) -> Self {
        TabEntry {
            terminal_id: terminal_id.into(),
            launch,
            unknown: BTreeMap::new(),
        }
    }
}

impl LaunchNode {
    /// A node of `kind` with `properties`.
    pub fn new<'a>(
        kind: impl Into<String>,
        properties: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Self {
        LaunchNode {
            kind: kind.into(),
            properties: properties
                .into_iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
            unknown: BTreeMap::new(),
        }
    }

    /// An agent running `harness` (its session type) in `conversation_id`.
    pub fn agent(harness: &str, conversation_id: &str) -> Self {
        LaunchNode::new(
            AGENT_KIND,
            [
                (HARNESS_PROPERTY, harness),
                (CONVERSATION_ID_PROPERTY, conversation_id),
            ],
        )
    }

    /// The session user's login shell.
    pub fn shell() -> Self {
        LaunchNode::new(SHELL_KIND, [])
    }

    /// A tmux session `session` the daemon created, on `os_user`'s tmux server.
    pub fn tmux(session: &str, os_user: &str) -> Self {
        LaunchNode::new(
            TMUX_KIND,
            [(SESSION_PROPERTY, session), (OS_USER_PROPERTY, os_user)],
        )
    }

    /// A client attached to the user's own tmux session `session`, on `os_user`'s tmux server.
    pub fn tmux_attach(session: &str, os_user: &str) -> Self {
        LaunchNode::new(
            TMUX_ATTACH_KIND,
            [(SESSION_PROPERTY, session), (OS_USER_PROPERTY, os_user)],
        )
    }

    /// A jail of its own over the session's worktree and sandbox grants. It records no properties:
    /// a resume rebuilds the jail from the session, as it rebuilds the session's main jail.
    pub fn sandbox() -> Self {
        LaunchNode::new(SANDBOX_KIND, [])
    }

    /// The session's target. It records no properties: the transport (ssh alias or Coder workspace)
    /// and the worktree there are the session's, read from its `.session.yaml` on every start and
    /// resume, never duplicated in the roster.
    pub fn target() -> Self {
        LaunchNode::new(TARGET_KIND, [])
    }

    /// The property `key`, when the node carries it.
    pub fn property(&self, key: &str) -> Option<&str> {
        self.properties.get(key).map(String::as_str)
    }
}

/// The roster of the session in `session_dir`; an empty roster when the session has none yet.
pub fn read_tab_roster(session_dir: &Path) -> anyhow::Result<TabRoster> {
    let path = session_dir.join(TAB_ROSTER_FILENAME);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(TabRoster::default())
        }
        Err(error) => return Err(anyhow::anyhow!("read {}: {error}", path.display())),
    };
    serde_yaml::from_str(&text)
        .map_err(|error| anyhow::anyhow!("parse {}: {error}", path.display()))
}

/// Replace the roster of the session in `session_dir` with `roster`, atomically: a failed rewrite
/// leaves the previous file standing.
pub fn write_tab_roster(session_dir: &Path, roster: &TabRoster) -> anyhow::Result<()> {
    let path = session_dir.join(TAB_ROSTER_FILENAME);
    let text = serde_yaml::to_string(roster)
        .map_err(|error| anyhow::anyhow!("serialize {}: {error}", path.display()))?;
    crate::atomic_file::write_atomic_labelled(&path, text).map_err(|error| anyhow::anyhow!(error))
}

#[cfg(test)]
#[path = "tab_roster_tests.rs"]
mod tests;
