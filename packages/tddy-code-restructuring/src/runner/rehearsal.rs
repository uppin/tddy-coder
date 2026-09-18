//! Rehearsing one operation against the language server, and reporting what it would cost.
//!
//! What `check --deep` does beyond reading text: resolve the operation to learn the refusal an
//! apply would give, and survey a cross-crate move to learn its blast radius. Writes nothing.

use crate::crate_move::{self, Survey};
use crate::plan::RefactorKind;
use crate::registry::{BackendRegistry, Workspace};
use crate::{Overlay, PositionLedger, Result};
use std::path::Path;

#[derive(Default)]
pub(super) struct Rehearsal {
    ledger: PositionLedger,
    overlay: Overlay,
}

/// What rehearsing one operation against the language server found.
pub(super) struct Rehearsed {
    /// The blast radius, for the one operation that has one to report before it is performed.
    pub(super) survey: Option<Survey>,
    /// The refusal an apply would give, where it would give one.
    pub(super) refusal: Option<String>,
}

impl Rehearsal {
    pub(super) fn rehearse(
        &mut self,
        root: &Path,
        registry: &mut BackendRegistry,
        op: &crate::plan::RefactorOp,
    ) -> Result<Rehearsed> {
        let at = self.ledger.translate_op(op)?;

        // Surveyed before it is resolved, because the two answer different questions: a refusal says
        // the move cannot happen, and the survey says what it would cost if it can. A plan author
        // who gets only the first has to run an apply to learn the second.
        let survey = match self.survey(root, registry, &at) {
            Ok(survey) => survey,
            Err(refusal) => {
                return Ok(Rehearsed {
                    survey: None,
                    refusal: Some(refusal.to_string()),
                })
            }
        };

        let resolved = registry
            .backend_for(Path::new(at.anchor.file()), at.op)?
            .resolve(
                &at,
                &Workspace {
                    root,
                    overlay: &self.overlay,
                },
            );

        match resolved {
            Ok(resolved) => {
                self.ledger.record(&resolved.edit);
                self.overlay.record(root, &resolved.edit)?;
                Ok(Rehearsed {
                    survey,
                    refusal: None,
                })
            }
            Err(refusal) => Ok(Rehearsed {
                survey,
                refusal: Some(refusal.to_string()),
            }),
        }
    }

    /// The blast radius of a cross-crate move, asked of the backend's own reference engine.
    ///
    /// Only `move_module_to_crate` has one worth reporting separately: every other operation's edits
    /// are whatever its assist returns, so a survey of one would be a second name for the resolution
    /// the rehearsal is about to take anyway. This costs the move a second `textDocument/references`
    /// pass on top of the one its resolution makes — which is the price of reporting the radius and
    /// the refusals in a run that writes nothing either way.
    ///
    /// `move_cluster_to_crate` is deliberately **not** surveyed. A [`Survey`] describes one module —
    /// one `source`, one caller list — so a cluster has one per member, and reporting the first
    /// member's alone would name a fraction of the blast radius as the whole of it. The rehearsal
    /// still resolves the operation, so a cluster's refusals are reported exactly as a single
    /// move's are; what `check --deep` withholds is the survey line, not a verdict.
    fn survey(
        &self,
        root: &Path,
        registry: &mut BackendRegistry,
        op: &crate::plan::RefactorOp,
    ) -> Result<Option<Survey>> {
        if op.op != RefactorKind::MoveModuleToCrate {
            return Ok(None);
        }

        let workspace = Workspace {
            root,
            overlay: &self.overlay,
        };
        let backend = registry.backend_for(Path::new(op.anchor.file()), op.op)?;
        let Some(engine) = backend.module_references() else {
            return Ok(None);
        };

        crate_move::survey(engine, &workspace, op).map(Some)
    }
}

/// A surveyed cross-crate move, as `check --deep` reports it: where the module is going, what
/// reaches it, and the path every caller would need.
///
/// Indented and prefixed rather than numbered like a finding, because a survey is not one — a move
/// with ninety callers is expensive, not defective, and a check that returned non-zero for it would
/// make the report unusable for deciding whether to write the plan that way.
pub(super) fn survey_lines(index: usize, survey: &Survey) -> Vec<String> {
    let mut lines = vec![format!(
        "   op {index} survey: {} -> {} ({}), {} item(s) reached from outside, {} caller(s)",
        survey.source,
        survey.destination.package,
        survey.destination.extern_name,
        survey.reached_from_outside.len(),
        survey.callers.len()
    )];

    if !survey.reached_from_outside.is_empty() {
        lines.push(format!(
            "      reached from outside: {}",
            survey.reached_from_outside.join(", ")
        ));
    }

    lines.extend(
        survey
            .callers
            .iter()
            .map(|caller| format!("      {}: {} -> {}", caller.path, caller.from, caller.to)),
    );
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crate_move::{CallerRewrite, Destination};

    fn a_survey_of_the_host_registry(
        reached_from_outside: &[&str],
        callers: Vec<CallerRewrite>,
    ) -> Survey {
        Survey {
            source: "packages/tddy-daemon/src/host_registry.rs".to_string(),
            destination: Destination {
                dir: "packages/tddy-daemon-kernel".to_string(),
                package: "tddy-daemon-kernel".to_string(),
                extern_name: "tddy_daemon_kernel".to_string(),
            },
            reached_from_outside: reached_from_outside.iter().map(|s| s.to_string()).collect(),
            callers,
        }
    }

    /// `check --deep` exists to answer "what would this cost" before an apply pays for it, and for a
    /// cross-crate move the reference set *is* the cost.
    #[test]
    fn reports_the_blast_radius_of_a_cross_crate_move() {
        // Given a surveyed move of one item, reached from one caller
        let survey = a_survey_of_the_host_registry(
            &["HostRegistry"],
            vec![CallerRewrite {
                path: "packages/tddy-daemon/src/runtime.rs".to_string(),
                from: "crate::host_registry::HostRegistry".to_string(),
                to: "tddy_daemon_kernel::host_registry::HostRegistry".to_string(),
            }],
        );

        // When the eighth operation of a plan is surveyed
        let lines = survey_lines(7, &survey);

        // Then the destination, the reached items and each caller's new path are all reported
        assert_eq!(
            lines,
            [
                "   op 7 survey: packages/tddy-daemon/src/host_registry.rs -> tddy-daemon-kernel \
                 (tddy_daemon_kernel), 1 item(s) reached from outside, 1 caller(s)",
                "      reached from outside: HostRegistry",
                "      packages/tddy-daemon/src/runtime.rs: crate::host_registry::HostRegistry -> \
                 tddy_daemon_kernel::host_registry::HostRegistry",
            ]
        );
    }

    /// A module nothing outside reaches is the move a reviewer can wave through, and the report says
    /// so in the same shape rather than by staying quiet.
    #[test]
    fn reports_a_cross_crate_move_no_caller_reaches() {
        // Given a surveyed move nothing outside the module names
        let survey = a_survey_of_the_host_registry(&[], Vec::new());

        // When
        let lines = survey_lines(0, &survey);

        // Then the header stands alone, with nothing claimed about callers
        assert_eq!(
            lines,
            [
                "   op 0 survey: packages/tddy-daemon/src/host_registry.rs -> tddy-daemon-kernel \
              (tddy_daemon_kernel), 0 item(s) reached from outside, 0 caller(s)"
            ]
        );
    }
}
