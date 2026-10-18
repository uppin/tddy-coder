//! `WatchRoot`: following a workspace root's index without ever starting one.
//!
//! The header of every session and any Code pane read a root's progress here, so this must be
//! passive: a watch that spawned rust-analyzer would warm every root anyone glanced at. It reports
//! what the root's own server reports through [`crate::graph::GraphLoad`] — which is folded from
//! the server's first notification, so a watch joining mid-load names every phase already running —
//! and what the index knows of a root nothing holds: `IDLE`, or `FAILED` with the reason a server
//! could not start or died unasked.
//!
//! **Event-driven, never polled.** A watch on a root nothing holds waits on the index's
//! [`RootSignals`], fired when a server is attached or fails to start; a watch on a held root waits
//! on that root's progress, whose sender dropping is the server going away. The watch runs until
//! its caller leaves.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tddy_rpc::Status;
use tokio::sync::watch;

use crate::activity::{Activity, Warmth};
use crate::graph::RootProgress;
use crate::index::WorkspaceIndex;
use crate::operations::{event_stream, EventSender};
use crate::proto::code_index::{ActivePhase, IndexProgress, IndexState};
use crate::service::EventStream;

/// Where a root's index stands for a follower that must not start one.
pub(crate) enum RootView {
    /// No language server holds the root.
    Idle,
    /// A language server holds it; its progress, to follow until the sender drops.
    Held(watch::Receiver<RootProgress>),
    /// The root's server could not start, or exited without being let go — and why.
    Failed(String),
}

/// What followers of a root hear besides its server's own progress: that some root gained a
/// server, and why a root's server could not start.
///
/// Held by [`WorkspaceIndex`] and told from the one place a server is reached
/// ([`WorkspaceIndex::client_for`]); kept here rather than in the index's per-root state so that
/// a root which only ever failed is not recorded as one the index holds.
#[derive(Clone)]
pub(crate) struct RootSignals {
    /// Fired whenever a root gains a server, or its start fails or recovers.
    changed: Arc<watch::Sender<()>>,
    /// Why the last attempt to start each root's server failed, until one succeeds.
    failures: Arc<Mutex<BTreeMap<PathBuf, String>>>,
}

impl Default for RootSignals {
    fn default() -> Self {
        Self {
            changed: Arc::new(watch::Sender::new(())),
            failures: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }
}

impl RootSignals {
    /// `root`'s server could not start, for `reason`.
    pub(crate) fn failed(&self, root: &Path, reason: &str) {
        self.failures_held()
            .insert(root.to_path_buf(), reason.to_string());
        self.changed.send_replace(());
    }

    /// `root` was handed a server — a newly spawned one when `new_server` — which ends any failure.
    pub(crate) fn served(&self, root: &Path, new_server: bool) {
        let recovered = self.failures_held().remove(root).is_some();
        if new_server || recovered {
            self.changed.send_replace(());
        }
    }

    fn failure_of(&self, root: &Path) -> Option<String> {
        self.failures_held().get(root).cloned()
    }

    fn subscribe(&self) -> watch::Receiver<()> {
        self.changed.subscribe()
    }

    fn failures_held(&self) -> std::sync::MutexGuard<'_, BTreeMap<PathBuf, String>> {
        self.failures.lock().expect("the table of failed roots")
    }
}

/// Where `root`'s index stands, read without starting anything.
///
/// A server that exited unasked is told apart from one let go by the registry: reaping and
/// shutdown drop a server's entry before stopping it, so a dead watcher on a root the registry
/// still holds a server for is a server that died on its own — and it reads so until the next
/// request respawns it.
async fn view_of(index: &WorkspaceIndex, root: &Path) -> RootView {
    if let Some(reason) = index.signals().failure_of(root) {
        return RootView::Failed(reason);
    }
    match index.graph_load_of(root).await {
        Some(graph) if graph.still_watching() => RootView::Held(graph.progress()),
        Some(_) if matches!(index.warmth_of(root).await, Warmth::AlreadyWarm) => {
            RootView::Failed(format!(
                "the language server holding `{}` exited without being asked to",
                root.display()
            ))
        }
        _ => RootView::Idle,
    }
}

/// Follow `workspace_root`'s index until the caller leaves, starting nothing.
pub(crate) async fn serve_watch_root(
    index: &WorkspaceIndex,
    workspace_root: &str,
) -> Result<EventStream<IndexProgress>, Status> {
    let (activity, root) = Activity::arrived("watch root", index, workspace_root).await?;
    let (events, stream) = event_stream();
    let index = index.clone();
    tokio::spawn(async move {
        follow(&index, &root, &events).await;
        // A watch has no answer of its own to finish with: its caller leaving is how it ends.
        activity.answered();
    });
    Ok(stream)
}

/// Report `root`'s standing and every change to it into `events`, until nobody is listening.
async fn follow(index: &WorkspaceIndex, root: &Path, events: &EventSender<IndexProgress>) {
    let mut roots_changed = index.signals().subscribe();
    let mut sent = Sent::default();
    loop {
        // Marked seen *before* the root is read, so a server attached in between still wakes the
        // wait below rather than being missed.
        roots_changed.borrow_and_update();
        match view_of(index, root).await {
            RootView::Held(progress) => {
                if forward(progress, events, &mut sent).await == Followed::CallerGone {
                    return;
                }
            }
            standing => {
                if !sent.send_if_new(events, standing_of(root, &standing)).await {
                    return;
                }
                tokio::select! {
                    changed = roots_changed.changed() => {
                        if changed.is_err() {
                            return;
                        }
                    }
                    () = events.closed() => return,
                }
            }
        }
    }
}

/// How following one server's progress ended.
#[derive(Debug, PartialEq, Eq)]
enum Followed {
    /// The server is gone; what the root is now has to be read afresh.
    ServerGone,
    /// Nobody is left to hear about it.
    CallerGone,
}

/// Forward every change of one server's progress, until the server or the caller is gone.
async fn forward(
    mut progress: watch::Receiver<RootProgress>,
    events: &EventSender<IndexProgress>,
    sent: &mut Sent,
) -> Followed {
    loop {
        let now = reported(&progress.borrow_and_update());
        if !sent.send_if_new(events, now).await {
            return Followed::CallerGone;
        }
        tokio::select! {
            changed = progress.changed() => {
                if changed.is_err() {
                    return Followed::ServerGone;
                }
            }
            // The only disconnect signal a stream has besides a failed send, and a root can go a
            // long time between changes.
            () = events.closed() => return Followed::CallerGone,
        }
    }
}

/// The last message a watch sent, so a wake that changed nothing it reports sends nothing.
#[derive(Default)]
struct Sent {
    last: Option<IndexProgress>,
}

impl Sent {
    /// Send `progress` unless it repeats the last message; `false` once nobody is listening.
    async fn send_if_new(
        &mut self,
        events: &EventSender<IndexProgress>,
        progress: IndexProgress,
    ) -> bool {
        if self.last.as_ref() == Some(&progress) {
            return true;
        }
        self.last = Some(progress.clone());
        events.send(Ok(progress)).await.is_ok()
    }
}

/// A root's progress as the wire carries it: `READY` when the server's latest status said it is
/// quiescent, `LOADING` otherwise — including before it has said anything.
///
/// `ready` is left false: it is `Warm`'s "this stream is done" flag, set only on a warm's last
/// message, and `state` is what a follower reads.
pub(crate) fn reported(progress: &RootProgress) -> IndexProgress {
    let (percentage, phase) = progress
        .current
        .as_ref()
        .map_or((0, ""), |(percentage, phase)| (*percentage, phase.as_str()));
    IndexProgress {
        line: progress.line.clone(),
        phase: phase.to_string(),
        percentage: percentage as u32,
        furthest: progress.furthest.clone(),
        ready: false,
        state: if progress.quiescent {
            IndexState::Ready
        } else {
            IndexState::Loading
        } as i32,
        phases: progress
            .phases
            .iter()
            .map(|active| ActivePhase {
                title: active.title.clone(),
                message: active.message.clone(),
                percentage: active.percentage.map(|percentage| percentage as u32),
            })
            .collect(),
        error: String::new(),
    }
}

/// The message for a root no live server holds.
fn standing_of(root: &Path, standing: &RootView) -> IndexProgress {
    match standing {
        RootView::Failed(reason) => IndexProgress {
            line: format!("the index of {} failed: {reason}", root.display()),
            state: IndexState::Failed as i32,
            error: reason.clone(),
            ..IndexProgress::default()
        },
        RootView::Idle | RootView::Held(_) => IndexProgress {
            line: format!("no language server holds {}", root.display()),
            state: IndexState::Idle as i32,
            ..IndexProgress::default()
        },
    }
}
