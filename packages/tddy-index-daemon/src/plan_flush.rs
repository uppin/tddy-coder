//! The per-root plan stores the index holds, and their debounced write-back.
//!
//! [`crate::index::WorkspaceIndex`] keeps one [`PlanStore`] per root a plan was loaded for, shared
//! ([`SharedPlanStore`]) between the requests that load, run and unload plans and the flusher here.
//! A changed plan waits in memory for [`PLAN_FLUSH_DEBOUNCE`] so a burst of changes is one write;
//! [`flush_eventually`] writes it back, never queueing behind a run that holds its store, and stops
//! once the index that owns the stores is gone. A run's per-operation writes, unload and shutdown
//! write for themselves and do not wait for it.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Weak;
use std::time::Duration;
use tddy_code_restructuring::plan_store::PlanStore;

/// How long a changed plan may wait in memory before it is written back.
///
/// Short enough that a plan a person opens in an editor a moment after a run sees the run's work,
/// long enough that a burst of changes is one write. A run's own per-operation writes do not wait
/// for it, and neither do unload and shutdown.
pub(crate) const PLAN_FLUSH_DEBOUNCE: Duration = Duration::from_secs(1);

/// How often the flusher looks for plans that have been dirty for longer than the debounce.
pub(crate) const PLAN_FLUSH_TICK: Duration = Duration::from_millis(250);

/// One root's loaded plans, shared between the requests that load, run and unload them and the
/// flusher.
///
/// A `std` mutex, not a tokio one: every holder is a blocking thread or a handler that has moved to
/// one, and a run holds it across the language-server calls that refresh its plan. The flusher only
/// ever `try_lock`s it, so a run in progress is never what it waits on.
pub type SharedPlanStore = Arc<std::sync::Mutex<PlanStore>>;

pub(crate) type PlanStores = std::sync::Mutex<BTreeMap<PathBuf, SharedPlanStore>>;

/// Write each store's plans that have been dirty for longer than the debounce, until the index
/// that owns the stores is gone.
///
/// A store a run is using is skipped for this tick rather than waited for: the run writes its own
/// plan after every operation, and a flusher that queued behind the run's language-server calls
/// would hold a thread for as long as they take.
pub(crate) async fn flush_eventually(plans: Weak<PlanStores>) {
    let mut tick = tokio::time::interval(PLAN_FLUSH_TICK);
    // What was last refused per root, so a plan that stays refused is reported once and not on
    // every tick for as long as it stays so.
    let mut reported: BTreeMap<PathBuf, String> = BTreeMap::new();
    loop {
        tick.tick().await;
        let Some(plans) = plans.upgrade() else {
            return;
        };
        let stores: Vec<(PathBuf, SharedPlanStore)> = plans
            .lock()
            .expect("the table of plan stores")
            .iter()
            .map(|(root, store)| (root.clone(), Arc::clone(store)))
            .collect();
        drop(plans);

        let outcomes = tokio::task::spawn_blocking(move || {
            stores
                .into_iter()
                .filter_map(|(root, store)| {
                    let mut held = store.try_lock().ok()?;
                    Some((
                        root,
                        held.flush_dirty().map(|_| ()).map_err(|f| f.to_string()),
                    ))
                })
                .collect::<Vec<_>>()
        })
        .await
        .expect("flushing the plans does not panic");

        for (root, outcome) in outcomes {
            match outcome {
                Ok(()) => {
                    reported.remove(&root);
                }
                Err(refusal) => {
                    if reported.get(&root) != Some(&refusal) {
                        log::warn!(
                            target: "tddy_index_daemon::index",
                            "could not write back a plan of {}: {refusal}", root.display()
                        );
                        reported.insert(root, refusal);
                    }
                }
            }
        }
    }
}
