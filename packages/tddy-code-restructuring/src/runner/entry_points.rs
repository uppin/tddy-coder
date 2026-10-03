//! One entry point per subcommand, the routing table over them, and the registries they resolve
//! through.
//!
//! Each returns its result and reports its running account into the sinks its caller installed;
//! the `runner` module doc states why none of this prints.

use crate::backends::rust::ProgressSink;
use crate::backends::RustBackend;
use crate::plan::RefactorKind;
use crate::plan_store::{FlushPolicy, PlanStore};
use crate::registry::BackendRegistry;
use crate::{Plan, RestructureError, Result};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tddy_lsp::client::LspClient;
use tokio_util::sync::CancellationToken;

use super::comparison::verify;
use super::options::usage;
use super::{parse_options, Command, Options, Outcome, RunSummary};

/// Dispatch a restructuring subcommand given a raw command line.
///
/// `client` is required for LSP-backed operations (`apply`, `anchors`, `check --deep`), and
/// `cancel` is how those operations learn that whoever asked for them has stopped waiting.
pub fn run(
    root: &Path,
    args: &[String],
    client: Option<Arc<LspClient>>,
    cancel: CancellationToken,
) -> Result<Outcome> {
    let command = args.first().map(String::as_str).unwrap_or_default();

    if !matches!(
        command,
        "apply" | "status" | "check" | "anchors" | "verify" | "snapshot"
    ) {
        return Err(usage(format!("unknown command `{command}`")));
    }

    dispatch(root, parse_options(args)?, client, cancel)
}

/// Dispatch a restructuring subcommand that has already been parsed.
///
/// This is what [`crate::restructure_cli`] calls: clap parsed the command line once, and re-parsing
/// its own output is what `cli_vector` used to do when the CLI lived in another package.
///
/// Returns the run's result rather than printing it, so that the front end decides what a result
/// means and where it goes. A caller serving a protocol on its own stdout — the reason this
/// matters — could not use a dispatch that wrote into that stream.
pub fn dispatch(
    root: &Path,
    options: Options,
    client: Option<Arc<LspClient>>,
    cancel: CancellationToken,
) -> Result<Outcome> {
    match options.command {
        Command::Apply => apply(root, options, client, cancel).map(Outcome::Applied),
        Command::Status => check_entry_points::status(root, options).map(Outcome::Status),
        Command::Check => {
            check_entry_points::check(root, options, client, cancel).map(Outcome::Checked)
        }
        Command::Anchors => anchor_entry_points::item_anchors(root, options, client, cancel)
            .map(Outcome::ItemAnchored),
        Command::Verify => verify(root, options).map(Outcome::Verified),
        Command::Snapshot => check_entry_points::snapshot(root, options).map(Outcome::Snapshotted),
        // Held across requests, so only a process that outlives one has anything to load into: a
        // run with no daemon has a store for its own length and nothing to name afterwards.
        Command::Load => Err(RestructureError::NeedsIndexDaemon {
            command: "load".to_string(),
        }),
        Command::Unload => Err(RestructureError::NeedsIndexDaemon {
            command: "unload".to_string(),
        }),
        Command::Plans => Err(RestructureError::NeedsIndexDaemon {
            command: "plans".to_string(),
        }),
    }
}

mod store_run;
pub use store_run::{apply_from_store, open_plan_run, record_applied_op, PlanRun};

/// Build a registry for static checks only (no LSP connection).
fn registry_for_static() -> BackendRegistry {
    let mut registry = BackendRegistry::new();
    registry.register(Box::new(RustBackend::new(
        "/usr/bin/rust-analyzer",
        "/tmp",
        "/tmp",
    )));
    registry
}

/// Build a registry backed by rust-analyzer through the shared LSP client.
///
/// The token is what ends a wait for an index that is still loading: this library states no budget
/// of its own, so the only bound on such a wait is the caller it belongs to.
///
/// Both destinations are the caller's: `progress` takes the server's own indexing lines, and
/// `trace` takes the diagnostic account of a seam — but only when `RESTRUCTURE_TRACE` asks for one,
/// which is read here so that a run gets a trace without every caller having to look.
pub fn registry_for(
    client: Arc<LspClient>,
    cancel: CancellationToken,
    progress: ProgressSink,
    trace: fn(&str),
) -> BackendRegistry {
    let mut registry = BackendRegistry::new();
    let mut rust = RustBackend::from_lsp_client(client, Some(cancel), progress);
    if wants_trace(std::env::var_os(TRACE_VARIABLE).as_deref()) {
        rust = rust.with_trace(trace);
    }
    registry.register(Box::new(rust));
    registry
}

/// Execute a plan against the working tree under `root`.
///
/// Returns what the whole run amounted to; the account of each operation as it lands goes to
/// [`Options::account`] while the run is still going, because that is the only time it is worth
/// anything.
///
/// The plan is loaded into a store that lives for this call and runs through
/// [`apply_from_store`], so a run with no daemon still gives its operations ids, refreshes their
/// anchors and writes the plan back — the same thing the daemon does over a store it keeps.
pub fn apply(
    root: &Path,
    options: Options,
    client: Option<Arc<LspClient>>,
    cancel: CancellationToken,
) -> Result<RunSummary> {
    if client.is_none() {
        return Err(RestructureError::MalformedPlan(
            "apply requires a rust-analyzer LSP session".into(),
        ));
    }
    let plan_path = options.plan()?;
    // Nothing but this call reads the store, so there is no later moment for a debounce to wait for:
    // the flushes are the ones the run asks for.
    let mut store = PlanStore::new(
        root,
        FlushPolicy {
            debounce: Duration::ZERO,
        },
    );
    store.load(std::slice::from_ref(&plan_path))?;
    let key = store.key_for(&plan_path)?;
    store_run::apply_from_store(root, &mut store, &key, options, client, cancel)
}

/// One line of per-operation progress, in the wording every front end uses for it.
///
/// A thin spelling of [`crate::console::operation`], which is where the line itself lives now:
/// `tddy_tools::index_console` and `tddy_index_daemon::render` render the same line from an event
/// rather than from a [`RefactorKind`], so the published function takes the kind as text and this
/// states it. Two numbers, because they answer different questions and are not interchangeable:
/// `[4/29]` is how far the run has got, and `op 3` is the operation's own index — the one `--from`
/// and `--stop-after` take and the one the journal records.
fn progress_line(
    index: usize,
    done: usize,
    total: usize,
    op: RefactorKind,
    files: usize,
    applied: bool,
) -> String {
    crate::console::operation(index, done, total, &format!("{op:?}"), files, applied)
}

mod check_entry_points;
pub use check_entry_points::{check, check_plan, snapshot, status, status_of_plan};

mod anchor_entry_points;
pub use anchor_entry_points::{item_anchors, open_run_resolving_anchors};

fn read_plan(path: &Path) -> Result<Plan> {
    Plan::parse(&std::fs::read_to_string(path)?)
}

const TRACE_VARIABLE: &str = "RESTRUCTURE_TRACE";

fn wants_trace(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|value| !value.is_empty() && value != "0")
}

/// What an operation had to do beyond the edit itself, as lines in the run's account.
///
/// A widened visibility and a note are consequences a reader has to see while the run is going,
/// and both are already carried back in the [`crate::Resolution`] for a caller that wants them as
/// values — which is what the daemon's own apply loop reads instead of this.
fn report_visibility(account: &ProgressSink, resolved: &crate::Resolution) {
    for change in &resolved.report {
        account(&crate::console::visibility(&crate::console::widening(
            change,
        )));
    }
    for note in &resolved.notes {
        account(&crate::console::note(note));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traces_when_the_variable_is_set_to_anything_meaningful() {
        // Given meaningful trace env values
        // When wants_trace is asked
        // Then tracing is enabled
        assert!(wants_trace(Some(std::ffi::OsStr::new("1"))));
        assert!(wants_trace(Some(std::ffi::OsStr::new("verbose"))));
    }

    #[test]
    fn stays_silent_unless_asked() {
        // Given unset, empty, or zero trace env values
        // When wants_trace is asked
        // Then tracing stays off
        assert!(!wants_trace(None));
        assert!(!wants_trace(Some(std::ffi::OsStr::new(""))));
        assert!(!wants_trace(Some(std::ffi::OsStr::new("0"))));
    }

    /// An apply that rewrites the tree has to say what it did as it does it. The line is printed
    /// after the commit, so its presence means the edit reached disk.
    ///
    /// What is asserted *here* is that the operation's own kind reaches the line — this is the only
    /// front end that has a [`RefactorKind`] rather than the text of one, so it is the only place
    /// the naming can go wrong. The line's shape is pinned beside the line, in
    /// [`crate::console`].
    #[test]
    fn names_the_operations_own_kind_in_the_line_that_reports_it() {
        // Given the fourth operation of a 29-operation plan, whose index is 3
        let line = progress_line(3, 3, 29, RefactorKind::ExtractModuleToFile, 3, true);

        // Then the line carries how far the run has got, the resumable index, and the edit's width
        assert_eq!(
            line,
            "[4/29] op 3: ExtractModuleToFile -> 3 file(s) applied"
        );
    }

    /// The counter is what the run has completed; the index is what `--from` would resume at. A
    /// plan run with `--from 20` has them far apart, and conflating them would print the wrong
    /// number to resume from.
    #[test]
    fn keeps_the_counter_and_the_index_independent() {
        // Given a run resumed at operation 20, on its first operation
        let line = progress_line(20, 0, 29, RefactorKind::ExtractMethod, 1, true);

        // Then the counter restarts while the index stays absolute
        assert!(line.starts_with("[1/29] op 20:"), "{line}");
    }

    /// The survey is engine-informed: it asks `textDocument/references` which callers exist. The
    /// backend a check builds has to offer that seam, or a deep check would rehearse the move
    /// without ever reporting its blast radius.
    #[test]
    fn offers_the_reference_engine_a_cross_crate_move_survey_needs() {
        // Given the registry a check builds
        let mut registry = registry_for_static();

        // When the backend for a Rust module is asked for its reference engine
        let backend = registry
            .backend_for(
                Path::new("packages/tddy-daemon/src/host_registry.rs"),
                RefactorKind::MoveModuleToCrate,
            )
            .unwrap();

        // Then it offers one
        assert!(
            backend.module_references().is_some(),
            "the Rust backend answers references and must offer the seam a survey needs"
        );
    }
}
