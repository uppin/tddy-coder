use super::super::budget::files_named_by;

use super::super::budget::measured;

use super::super::budget::budget_report;

use super::super::rehearsal::survey_lines;

use crate::{crate_move, item_anchor, Anchor, Overlay, Plan};

use crate::registry::Workspace;

use super::super::rehearsal::Rehearsal;

use crate::item_anchor::has_item_anchors;

use super::registry_for_static;

use super::registry_for;

use crate::RestructureError;

use super::super::report_drifted_hints;

use super::super::Finding;

use tokio_util::sync::CancellationToken;

use tddy_lsp::client::LspClient;

use std::sync::Arc;

use crate::journal::OpStatus;

use crate::journal::Journal;

use super::super::refuse_repo_scoped_state;

use super::super::StatePaths;

use super::read_plan;

use super::super::PlanProgress;

use super::super::SnapshotRewrite;

use crate::Result;

use super::super::Options;

use std::path::Path;

/// Rewrite a plan's snapshot header to the working tree under `root` as it stands.
///
/// Every edit to a snapshotted file invalidates the header, and until this existed recomputing it
/// was a shell pipeline each author had to invent around [`crate::apply::hash_file`] — which is
/// public, and which nothing exposed.
///
/// **Line 1 and nothing else.** The plan is a command log, and a subcommand that rewrote an
/// operation would be editing the author's intent rather than restating what the tree holds. So
/// the operations are carried through as the bytes they arrived as, rather than parsed and
/// re-serialized: a plan is also a file people diff, and a round trip through `serde_json` would
/// renumber its whitespace and reorder its keys for nothing.
///
/// Nothing is written when the header already matches, so a `snapshot` of a current plan leaves
/// its mtime alone.
///
/// Nothing goes to [`Options::progress`] either. That sink carries how far an index has got, and
/// this reads a file and hashes what it names — a run with nothing to wait for has no progress to
/// report, and a line there would be narration about an index that was never consulted.
pub fn snapshot(root: &Path, options: Options) -> Result<SnapshotRewrite> {
    let path = options.plan()?;
    let text = std::fs::read_to_string(&path)?;
    let plan = Plan::parse(&text)?;
    let header = plan.rehashed_header(root)?;

    // The header is the first line that is *not blank*, which is where `Plan::parse` reads it from.
    // Taking "everything before the first newline" instead would rewrite a leading blank line and
    // leave the real header behind to be parsed as an operation.
    let blank: usize = text
        .split_inclusive('\n')
        .take_while(|line| line.trim().is_empty())
        .map(str::len)
        .sum();
    let produced = match text[blank..].split_once('\n') {
        Some((_, operations)) => format!("{}{header}\n{operations}", &text[..blank]),
        None => format!("{}{header}", &text[..blank]),
    };

    let rewritten = produced != text;
    if rewritten {
        std::fs::write(&path, &produced)?;
    }

    Ok(SnapshotRewrite {
        plan: path.to_string_lossy().to_string(),
        paths: plan.snapshot.len() + plan.files.len(),
        rewritten,
        stale: Vec::new(),
    })
}

/// [`snapshot`], and for a plan with item anchors the anchors are re-resolved too: each hint
/// follows its item through the edits made since the plan was written, and an operation whose item
/// changed or went is reported and left as written ([`rebase_plan_file`]).
///
/// An item anchor is resolved by a language server, so a plan that has any needs `client`; a plan
/// without one is what [`snapshot`] has always handled and asks nothing of it.
pub fn snapshot_resolving(
    root: &Path,
    options: Options,
    client: Option<Arc<LspClient>>,
    cancel: CancellationToken,
) -> Result<SnapshotRewrite> {
    let path = options.plan()?;
    if !has_item_anchors(&read_plan(&path)?) {
        return snapshot(root, options);
    }
    let client = client.ok_or_else(|| {
        RestructureError::MalformedPlan(
            "snapshot of a plan with item anchors requires a rust-analyzer LSP session".into(),
        )
    })?;
    let mut registry = registry_for(client, cancel, Arc::clone(&options.progress), options.trace);
    let stale = crate::plan_store::rebase_plan_file(root, &path, &mut registry)?;
    let rewrite = snapshot(root, options)?;
    Ok(SnapshotRewrite { stale, ..rewrite })
}

/// How far a plan's journal under `root` got.
///
/// `in_flight` discounts the operations that went on to complete — the journal holds a record of
/// each — and `pending` is what the plan still has left.
pub fn status(root: &Path, options: Options) -> Result<PlanProgress> {
    let plan_path = options.plan()?;
    let plan = read_plan(&plan_path)?;
    status_of_plan(root, &plan_path, &plan)
}

/// [`status`] of a plan the caller already holds — the daemon's, out of its store — rather than one
/// read from `plan_path`, which still says where the plan's run state is keyed.
pub fn status_of_plan(root: &Path, plan_path: &Path, plan: &Plan) -> Result<PlanProgress> {
    let paths = StatePaths::for_plan(root, plan_path)?;
    refuse_repo_scoped_state(root, &paths)?;
    let journal = Journal::load(&paths.journal)?;
    let counted = |wanted: OpStatus| {
        journal
            .records_in_force()
            .filter(|record| record.status == wanted)
            .count()
    };
    let completed = counted(OpStatus::Completed);

    Ok(PlanProgress {
        completed,
        in_flight: counted(OpStatus::InFlight).saturating_sub(completed),
        pending: plan.ops.len().saturating_sub(completed),
        failed: counted(OpStatus::Failed),
    })
}

/// Everything wrong with a plan, without writing anything.
///
/// A plan with findings is an `Ok` carrying them, not a refusal: a caller that receives them as
/// values decides for itself what they mean — a front end fails the run, a plan author reads the
/// report, and a host serving the check forwards them. Only a plan that could not be *checked* —
/// one that will not parse, or whose snapshot does not match the tree — is an error.
///
/// The two things a check produces that are not findings go to [`Options::account`]: a deep
/// check's blast-radius survey, which is a cost rather than a defect, and the file-budget report,
/// which is a record of where the tree stands.
pub fn check(
    root: &Path,
    options: Options,
    client: Option<Arc<LspClient>>,
    cancel: CancellationToken,
) -> Result<Vec<Finding>> {
    let plan = read_plan(&options.plan()?)?;
    check_plan(root, plan, options, client, cancel)
}

/// [`check`] of a plan the caller already holds — the daemon's, out of its store — rather than one
/// read from `options.target`.
pub fn check_plan(
    root: &Path,
    plan: Plan,
    options: Options,
    client: Option<Arc<LspClient>>,
    cancel: CancellationToken,
) -> Result<Vec<Finding>> {
    plan.verify_snapshot(root)?;
    report_drifted_hints(&plan, root, &options.progress);

    let mut registry = if options.deep {
        let client = client.ok_or_else(|| {
            RestructureError::MalformedPlan(
                "deep check requires a rust-analyzer LSP session".into(),
            )
        })?;
        registry_for(client, cancel, Arc::clone(&options.progress), options.trace)
    } else {
        registry_for_static()
    };
    let mut findings: Vec<Finding> = Vec::new();
    // Resolved before anything reads an anchor, so every check below sees the ranges an apply would
    // act on. A static check has no server to resolve them with, so it cannot vouch for an
    // operation anchored by item: that is a finding for each, not a quiet skip, because a plan of
    // item anchors passing `check` green would be a claim nothing examined.
    let plan = if !has_item_anchors(&plan) {
        plan
    } else if options.deep {
        item_anchor::resolve_item_anchors(&plan, root, &mut registry)?
    } else {
        findings.extend(unresolvable_without_a_server(&plan));
        plan
    };
    let mut rehearsal = Rehearsal::default();
    let total = plan.ops.len();
    (options.progress)(&format!(
        "check: {total} operation(s){}",
        if options.deep { ", deep" } else { "" }
    ));

    let static_workspace = Workspace {
        root,
        overlay: &Overlay::new(),
    };
    // Every member of a cluster operation, not only the module its anchor names — otherwise
    // `check` passes a set one of whose members `apply` then refuses, which is the parity this
    // whole static pass exists to hold.
    for (operation, detail) in crate_move::unrunnable(&static_workspace, &plan.ops)? {
        findings.push(Finding { operation, detail });
    }

    // Read across the plan rather than per operation: whether a module's siblings come along is a
    // question about the plan, and an operation that is viable on its own is exactly how a
    // mutually-referencing set gets left half moved.
    for (operation, detail) in crate_move::stranded_siblings(&static_workspace, &plan.ops)? {
        findings.push(Finding { operation, detail });
    }

    for (index, op) in plan.ops.iter().enumerate() {
        (options.progress)(&format!(
            "op {index} of {total}: static check {:?} in `{}`",
            op.op,
            op.anchor.file()
        ));
        let statics = registry
            .backend_for(Path::new(op.anchor.file()), op.op)?
            .check(
                op,
                &Workspace {
                    root,
                    overlay: &Overlay::new(),
                },
            )?;
        let statically_sound = statics.is_empty();
        findings.extend(statics.into_iter().map(|detail| Finding {
            operation: index,
            detail,
        }));

        if !options.deep || !statically_sound {
            continue;
        }

        (options.progress)(&format!(
            "op {index} of {total}: deep resolve {:?} in `{}`",
            op.op,
            op.anchor.file()
        ));
        let rehearsed = rehearsal.rehearse(root, &mut registry, op)?;
        if let Some(survey) = &rehearsed.survey {
            for line in survey_lines(index, survey) {
                (options.account)(&line);
            }
        }
        if let Some(refusal) = rehearsed.refusal {
            findings.push(Finding {
                operation: index,
                detail: refusal,
            });
        }
    }

    let findings = one_finding_per_refused_group(&plan, findings);

    if let Some(budget) = options.budget {
        for line in budget_report(&measured(root, &files_named_by(&plan))?, budget) {
            (options.account)(&line);
        }
    }

    Ok(findings)
}

/// `findings`, with every group's findings merged into one that names the group.
///
/// A group stands or falls whole, so a refused member refuses the group: reporting one finding per
/// member would read as though each could be fixed and applied alone. The merged finding sits where
/// the group's first finding was, at that member's index, and lists each member's reason.
fn one_finding_per_refused_group(plan: &Plan, findings: Vec<Finding>) -> Vec<Finding> {
    let group_of = |finding: &Finding| {
        plan.ops
            .get(finding.operation)
            .and_then(|op| op.group.as_deref())
    };
    let mut merged: Vec<Finding> = Vec::new();
    let mut position_of_group: std::collections::BTreeMap<&str, usize> = Default::default();
    let mut reasons: std::collections::BTreeMap<&str, Vec<String>> = Default::default();
    for finding in &findings {
        let Some(group) = group_of(finding) else {
            merged.push(finding.clone());
            continue;
        };
        reasons
            .entry(group)
            .or_default()
            .push(format!("op {}: {}", finding.operation, finding.detail));
        position_of_group.entry(group).or_insert_with(|| {
            merged.push(Finding {
                operation: finding.operation,
                detail: String::new(),
            });
            merged.len() - 1
        });
    }
    for (group, position) in position_of_group {
        merged[position].detail = format!(
            "group `{group}` is refused whole, because a member of it is: {}",
            reasons[group].join("; ")
        );
    }
    merged
}

/// A finding for each operation a static check cannot examine because an anchor of it names items.
fn unresolvable_without_a_server(plan: &Plan) -> impl Iterator<Item = Finding> + '_ {
    plan.ops
        .iter()
        .enumerate()
        .filter(|(_, op)| {
            op.anchors()
                .any(|anchor| matches!(anchor, Anchor::Item { .. } | Anchor::Items { .. }))
        })
        .map(|(index, op)| Finding {
            operation: index,
            detail: format!(
                "{:?} in `{}` anchors by item, which only a deep check can resolve, so this \
                 static check did not examine it — run `check --deep`",
                op.op,
                op.anchor.file()
            ),
        })
}
