//! The single writer of every session's tab roster (`tabs.yaml`, see
//! [`tddy_core::tab_roster`]).
//!
//! Each rewrite is a read-modify-write of the whole file, so two tabs starting at once in one session
//! would lose one of them without a lock. One lock per session directory serialises them; sessions
//! never wait on each other.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_core::tab_roster::{read_tab_roster, write_tab_roster, TabEntry, TabRoster};

/// Records and forgets the tabs of every session this daemon serves, one rewrite at a time per
/// session.
#[derive(Default)]
pub struct TabRosterStore {
    /// One lock per session directory, held across a whole read-modify-write of its roster.
    locks: tokio::sync::Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>,
}

impl TabRosterStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record `entry` in the roster of the session in `session_dir`, replacing an entry of the same
    /// terminal id.
    pub async fn record(&self, session_dir: &Path, entry: TabEntry) -> anyhow::Result<()> {
        self.rewrite(session_dir, move |roster| {
            match roster
                .tabs
                .iter_mut()
                .find(|tab| tab.terminal_id == entry.terminal_id)
            {
                Some(existing) => *existing = entry,
                None => roster.tabs.push(entry),
            }
            true
        })
        .await
    }

    /// Remove `terminal_id` from the roster of the session in `session_dir`; every other entry is
    /// kept as it is.
    pub async fn forget(&self, session_dir: &Path, terminal_id: &str) -> anyhow::Result<()> {
        self.rewrite(session_dir, |roster| {
            let before = roster.tabs.len();
            roster.tabs.retain(|tab| tab.terminal_id != terminal_id);
            roster.tabs.len() != before
        })
        .await
    }

    /// One read-modify-write of the roster of `session_dir`, under that session's lock. `change`
    /// answers whether it changed anything: an unchanged roster is not rewritten.
    async fn rewrite(
        &self,
        session_dir: &Path,
        change: impl FnOnce(&mut TabRoster) -> bool,
    ) -> anyhow::Result<()> {
        let lock = {
            let mut locks = self.locks.lock().await;
            Arc::clone(locks.entry(session_dir.to_path_buf()).or_default())
        };
        let _held = lock.lock().await;
        let mut roster = read_tab_roster(session_dir)?;
        if !change(&mut roster) {
            return Ok(());
        }
        write_tab_roster(session_dir, &roster)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use pretty_assertions::assert_eq;
    use tddy_core::tab_roster::{read_tab_roster, LaunchNode, TabEntry};

    use super::TabRosterStore;

    /// How many tabs start at once in the concurrency test: enough that an unlocked rewrite loses one.
    const CONCURRENT_TABS: usize = 32;

    fn a_shell_tab(terminal_id: &str) -> TabEntry {
        TabEntry::new(terminal_id, vec![LaunchNode::shell()])
    }

    fn the_recorded_terminal_ids(session_dir: &std::path::Path) -> Vec<String> {
        let mut ids: Vec<String> = read_tab_roster(session_dir)
            .expect("the roster reads")
            .tabs
            .into_iter()
            .map(|tab| tab.terminal_id)
            .collect();
        ids.sort();
        ids
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn tabs_recorded_at_once_in_one_session_are_all_kept() {
        // Given one store and one session directory
        let store = Arc::new(TabRosterStore::new());
        let session_dir = tempfile::tempdir().unwrap();

        // When many tabs are recorded at the same time
        let records: Vec<_> = (0..CONCURRENT_TABS)
            .map(|index| {
                let (store, dir) = (Arc::clone(&store), session_dir.path().to_path_buf());
                tokio::spawn(async move {
                    store
                        .record(&dir, a_shell_tab(&format!("tab-{index:02}")))
                        .await
                })
            })
            .collect();
        for record in records {
            record
                .await
                .expect("the record task ran")
                .expect("the tab is recorded");
        }

        // Then the roster holds every one of them
        let mut expected: Vec<String> = (0..CONCURRENT_TABS)
            .map(|index| format!("tab-{index:02}"))
            .collect();
        expected.sort();
        assert_eq!(the_recorded_terminal_ids(session_dir.path()), expected);
    }

    #[tokio::test]
    async fn forgetting_a_tab_removes_only_that_tab() {
        // Given a session that recorded three tabs
        let store = TabRosterStore::new();
        let session_dir = tempfile::tempdir().unwrap();
        for terminal_id in ["tab-a", "tab-b", "tab-c"] {
            store
                .record(session_dir.path(), a_shell_tab(terminal_id))
                .await
                .expect("the tab is recorded");
        }

        // When the middle one is forgotten
        store
            .forget(session_dir.path(), "tab-b")
            .await
            .expect("the tab is forgotten");

        // Then the other two are still recorded
        assert_eq!(
            the_recorded_terminal_ids(session_dir.path()),
            vec!["tab-a".to_string(), "tab-c".to_string()]
        );
    }

    #[tokio::test]
    async fn recording_a_terminal_again_replaces_its_entry() {
        // Given a session that recorded a tab as a shell
        let store = TabRosterStore::new();
        let session_dir = tempfile::tempdir().unwrap();
        store
            .record(session_dir.path(), a_shell_tab("tab-a"))
            .await
            .expect("the tab is recorded");

        // When the same terminal is recorded with another launch chain
        let relaunched = TabEntry::new("tab-a", vec![LaunchNode::agent("muse-cli", "c-1")]);
        store
            .record(session_dir.path(), relaunched.clone())
            .await
            .expect("the tab is recorded again");

        // Then the roster holds it once, as last recorded
        assert_eq!(
            read_tab_roster(session_dir.path())
                .expect("the roster reads")
                .tabs,
            vec![relaunched]
        );
    }
}
