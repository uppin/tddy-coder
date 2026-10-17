//! The tokens that admit a clone's helper to the askpass endpoint.
//!
//! A token is the whole of the helper's authority: it names the operator a prompt is raised for, so
//! one that outlived its clone, or could be guessed, would let a stranger put a passphrase prompt in
//! front of somebody. It belongs to the clone rather than to each question, because ssh asks again
//! after a wrong passphrase.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

/// What the daemon knows of one clone in flight. Dropped with its grant.
struct CloneState {
    operator: String,
    /// Key paths the operator has already answered for. Paths only, never a passphrase.
    answered_keys: HashSet<String>,
}

type Clones = Arc<Mutex<HashMap<String, CloneState>>>;

/// The clones in flight, by token.
#[derive(Clone, Default)]
pub struct CloneGrants {
    operators: Clones,
    endpoint_url: String,
}

impl CloneGrants {
    /// Grants that name no endpoint: [`CloneGrant::url`] is empty. Grants that a helper can use come
    /// from [`super::AskpassEndpoint::register_clone`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub(super) fn for_endpoint(endpoint_url: String) -> Self {
        Self {
            operators: Arc::default(),
            endpoint_url,
        }
    }

    /// Admit the helper of a clone `operator` started, until the returned grant is dropped.
    #[must_use]
    pub fn register(&self, operator: &str) -> CloneGrant {
        // Two v4 UUIDs: 244 random bits, 64 hex characters.
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        self.lock().insert(
            token.clone(),
            CloneState {
                operator: operator.to_string(),
                answered_keys: HashSet::new(),
            },
        );
        CloneGrant {
            token,
            url: self.endpoint_url.clone(),
            operators: Arc::clone(&self.operators),
        }
    }

    /// The operator whose clone `token` was issued for, while that clone runs.
    #[must_use]
    pub fn operator_for(&self, token: &str) -> Option<String> {
        self.lock().get(token).map(|clone| clone.operator.clone())
    }

    /// Whether the clone `token` names was already answered for `key` — so that ssh asking about
    /// it again means the answer was wrong.
    #[must_use]
    pub(super) fn was_answered_for(&self, token: &str, key: &str) -> bool {
        self.lock()
            .get(token)
            .is_some_and(|clone| clone.answered_keys.contains(key))
    }

    /// Remember that the clone `token` names was answered for `key`.
    pub(super) fn record_answer_for(&self, token: &str, key: &str) {
        if let Some(clone) = self.lock().get_mut(token) {
            clone.answered_keys.insert(key.to_string());
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, CloneState>> {
        self.operators.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// One clone's admission. Dropping it ends the clone's authority.
pub struct CloneGrant {
    token: String,
    url: String,
    operators: Clones,
}

impl CloneGrant {
    #[must_use]
    pub fn token(&self) -> String {
        self.token.clone()
    }

    /// The endpoint's base URL, `http://127.0.0.1:<port>`.
    #[must_use]
    pub fn url(&self) -> String {
        self.url.clone()
    }
}

impl Drop for CloneGrant {
    fn drop(&mut self) {
        self.operators
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.token);
    }
}
