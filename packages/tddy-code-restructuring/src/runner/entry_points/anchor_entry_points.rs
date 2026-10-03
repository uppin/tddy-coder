use super::super::open_run_gated;

use crate::{journal::Journal, runner::resume};

use crate::registry::BackendRegistry;

use super::super::StatePaths;

use crate::Plan;

use crate::item_anchor::items_anchor;

use crate::item_anchor::item_anchor_at;

use super::super::options::usage;

use super::registry_for;

use crate::RestructureError;

use crate::plan::Anchor;

use crate::Result;

use tokio_util::sync::CancellationToken;

use tddy_lsp::client::LspClient;

use std::sync::Arc;

use super::super::Options;

use std::path::Path;

/// The anchor `restructure anchors` emits: an `items` anchor over `options.items`, or — with
/// `options.at` — the `item` anchor of the innermost item enclosing that position.
pub fn item_anchors(
    root: &Path,
    options: Options,
    client: Option<Arc<LspClient>>,
    cancel: CancellationToken,
) -> Result<Anchor> {
    let client = client.ok_or_else(|| {
        RestructureError::MalformedPlan("anchors requires a rust-analyzer LSP session".into())
    })?;
    let source = options.source()?;
    let file = source.to_string_lossy().to_string();

    let mut registry = registry_for(client, cancel, Arc::clone(&options.progress), options.trace);
    match options.at {
        Some(_) if !options.items.is_empty() => {
            Err(usage("anchors takes --items or --at, not both"))
        }
        Some(at) => {
            (options.progress)(&format!(
                "anchors: the item enclosing {}:{} in `{file}`",
                at.start.line, at.start.col
            ));
            item_anchor_at(root, &file, at, &mut registry)
        }
        None if options.items.is_empty() => {
            Err(usage("anchors needs --items A,B,C or --at LINE:COL"))
        }
        None => {
            (options.progress)(&format!(
                "anchors: `{file}` ({} item(s))",
                options.items.len()
            ));
            items_anchor(root, &file, &options.items, &mut registry)
        }
    }
}

/// Open a run the way both apply loops must: the plan checked against the journal, item anchors
/// resolved, then the baseline compile check, then `.restructure/` written — and the plan to run,
/// with every anchor lowered to the coordinates the run translates, handed back beside the journal.
///
/// One function so the CLI's apply and the daemon's cannot diverge on the order. It is this order
/// because resolving is cheap and refuses for the commonest reasons — an item edited since the plan
/// was written, absent, or not in its file — while the baseline gate takes minutes of `cargo check`;
/// and both come before the first write, so their refusals' "nothing was written" stays true and
/// leaves no `.restructure/` behind.
///
/// Item anchors are resolved against the tree the run *starts* on, and only the operations the run
/// will execute: a run that continues a journal starts on a tree that holds the journal's edits,
/// and the plan's pending anchors were written back to describe exactly that tree
/// ([`record_applied_op`]). What a continued run first has to know is that they were — see
/// `resume::refuse_a_plan_the_journal_cannot_vouch_for` — and it asks twice: of the journal as
/// found before the gate, and of the one the open returns, since opening a continued run may adopt
/// a repository-scoped journal the first look could not see.
pub fn open_run_resolving_anchors(
    plan: &Plan,
    root: &Path,
    paths: &StatePaths,
    options: &Options,
    registry: &mut BackendRegistry,
    baseline_gate: impl FnOnce() -> Result<()>,
) -> Result<(Journal, Plan)> {
    let (journal, resolved) = open_run_gated(plan, root, paths, options, || {
        let found = Journal::load(&paths.journal)?;
        resume::refuse_a_plan_the_journal_cannot_vouch_for(plan, &found)?;
        let start = resume::start_of(plan, options, &found)?;
        let resolved = resume::lower_pending(plan, start, root, registry)?;
        baseline_gate()?;
        Ok(resolved)
    })?;
    resume::refuse_a_plan_the_journal_cannot_vouch_for(plan, &journal)?;
    Ok((journal, resolved))
}
