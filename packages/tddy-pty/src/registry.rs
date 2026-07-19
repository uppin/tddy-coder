//! Side-table mapping task IDs to live PTY master handles for resize/SIGWINCH.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use portable_pty::{MasterPty, PtySize};
use tddy_task::TaskId;
use tokio::sync::RwLock;

/// Live PTY control state for a running task.
pub struct PtyControl {
    pub master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    pub current_size: Arc<Mutex<PtySize>>,
    pub terminal_id: String,
    pub kind: String,
}

/// Registry of PTY masters keyed by [`TaskId`].
///
/// Output bytes flow through [`tddy_task::TaskChannel`]; this table holds only the
/// master handle needed for resize and redraw.
#[derive(Clone, Default)]
pub struct PtyRegistry {
    inner: Arc<RwLock<HashMap<TaskId, PtyControl>>>,
}

impl PtyRegistry {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn insert(&self, task_id: TaskId, control: PtyControl) {
        self.inner.write().await.insert(task_id, control);
    }

    pub async fn get(&self, task_id: &TaskId) -> Option<PtyControl> {
        // Clone the Arc handles so callers can use them without holding the registry lock.
        self.inner.read().await.get(task_id).map(|c| PtyControl {
            master: Arc::clone(&c.master),
            current_size: Arc::clone(&c.current_size),
            terminal_id: c.terminal_id.clone(),
            kind: c.kind.clone(),
        })
    }

    pub async fn remove(&self, task_id: &TaskId) {
        self.inner.write().await.remove(task_id);
    }

    /// Resize the PTY for `task_id` and update the stored dimensions.
    pub async fn resize(&self, task_id: &TaskId, rows: u16, cols: u16) -> bool {
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        let reg = self.inner.write().await;
        let Some(control) = reg.get(task_id) else {
            return false;
        };
        if let Ok(m) = control.master.lock() {
            let _ = m.resize(size);
        }
        if let Ok(mut s) = control.current_size.lock() {
            *s = size;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use portable_pty::{native_pty_system, PtySize};

    #[tokio::test]
    async fn insert_get_remove_round_trip() {
        // Given
        let registry = PtyRegistry::new();
        let task_id = TaskId::new();
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty");
        let control = PtyControl {
            master: Arc::new(Mutex::new(pair.master)),
            current_size: Arc::new(Mutex::new(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })),
            terminal_id: "main".into(),
            kind: "claude-cli".into(),
        };

        // When
        registry.insert(task_id.clone(), control).await;

        // Then
        assert!(registry.get(&task_id).await.is_some());
        registry.remove(&task_id).await;
        assert!(registry.get(&task_id).await.is_none());
    }
}
