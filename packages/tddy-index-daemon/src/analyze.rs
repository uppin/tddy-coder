//! The four analysis operations: capture, report, duplicate detection and complexity.
//!
//! Split from [`crate::operations`] and [`crate::queries`] because the shape of the work is
//! different, not because the file was long. Analysis touches no language server at all —
//! `tddy_code_analysis` is synchronous, LSP-free and takes explicit paths — so nothing here warms a
//! root or waits for an index. What it does instead is shell out to cargo and llvm-cov for tens of
//! minutes, which is why two of the four stream.
//!
//! Three run under this root's queue: a capture builds into the tree and writes its artefacts
//! there, and a report writes its HTML beside them, so two of them at once on one root would fight
//! over the same files. [`serve_complexity`] deliberately does not — see its own note.

use std::path::{Path, PathBuf};

use tddy_code_analysis::complexity_cache::cached_file_complexity;
use tddy_code_analysis::coverage::{capture_coverage, CaptureProgress};
use tddy_code_analysis::duplicate_tests::DuplicateAnalysis;
use tddy_code_analysis::report::{generate_duplicate_tests_report, generate_report, report_path};
use tddy_rpc::Status;

use crate::activity::Activity;
use crate::index::WorkspaceIndex;
use crate::operations::{event_stream, joined, EventSender};
use crate::proto::code_index::{
    analyze_event, AnalyzeEvent, BuildFinished, BuildStarted, CaptureFinished, ComplexityRequest,
    ComplexityResponse, CoverageRequest, DuplicateGroup, DuplicateTestsFound,
    DuplicateTestsRequest, FunctionComplexity, HarnessStarted, ReportRequest, ReportResponse,
    SubsetRelation, TestCaptured,
};
use crate::service::EventStream;
use crate::status::status_of_analysis;

/// Build the instrumented tests, run each one and write its coverage profile.
pub(crate) async fn serve_coverage(
    index: &WorkspaceIndex,
    request: CoverageRequest,
) -> Result<EventStream<AnalyzeEvent>, Status> {
    let (activity, root) = Activity::arrived("coverage", index, &request.workspace_root).await?;
    let crate_path = activity.refusing(under(&root, &request.crate_path, "crate path"))?;
    let coverage_dir =
        activity.refusing(under(&root, &request.coverage_dir, "coverage directory"))?;
    let (events, stream) = event_stream();
    let index = index.clone();

    tokio::spawn(async move {
        let _queued = index.hold(&root).await;
        let captured = {
            let events = events.clone();
            tokio::task::spawn_blocking(move || {
                // TODO(tddy-code-analysis): a capture cannot be stopped. `capture_coverage` takes
                // no cancellation surface, so a client that disconnects after minute one leaves
                // the remaining 54 running for nobody. Giving it one means a parameter it can
                // check between tests — this crate's `CancellationToken` cannot cross into
                // `tddy-code-analysis`, which has no `tokio-util` — and that is an API change to
                // the capture pipeline rather than part of putting it behind an RPC.
                let mut listening = true;
                capture_coverage(&crate_path, &coverage_dir, &mut |progress| {
                    if !listening {
                        return;
                    }
                    if events.blocking_send(Ok(captured_event(&progress))).is_err() {
                        listening = false;
                        log::debug!(
                            target: "tddy_index_daemon::analyze",
                            "nobody is listening to this capture any more; it runs to completion \
                             because a capture cannot be stopped"
                        );
                    }
                })
            })
            .await
        };
        if reported(&events, captured, &activity).await.is_some() {
            activity.answered();
        }
    });

    Ok(stream)
}

/// Join a capture against the complexity of the tree it measured.
pub(crate) async fn serve_report(
    index: &WorkspaceIndex,
    request: ReportRequest,
) -> Result<ReportResponse, Status> {
    let (activity, root) = Activity::arrived("report", index, &request.workspace_root).await?;
    activity.recorded(joint_report(index, root, request).await)
}

/// The join itself, with the root already resolved and its arrival already recorded.
async fn joint_report(
    index: &WorkspaceIndex,
    root: PathBuf,
    request: ReportRequest,
) -> Result<ReportResponse, Status> {
    let coverage_dir = under(&root, &request.coverage_dir, "coverage directory")?;
    let crate_path = under(&root, &request.crate_path, "crate path")?;
    let scores = index.complexity_scores();

    let _queued = index.hold(&root).await;
    let written = report_path(&coverage_dir);
    let joint = {
        let coverage_dir = coverage_dir.clone();
        tokio::task::spawn_blocking(move || {
            generate_report(&coverage_dir, &crate_path, scores.as_ref())
        })
        .await
        .map_err(|failure| joined("report", &failure))?
        .map_err(|refusal| status_of_analysis(&refusal))?
    };

    Ok(ReportResponse {
        join_rate: joint.join_rate,
        matched: joint.functions.len() as u32,
        unmatched: joint.unmatched_functions as u32,
        report_path: written.to_string_lossy().to_string(),
    })
}

/// Tests whose coverage signatures are identical, and those contained in another's.
pub(crate) async fn serve_duplicate_tests(
    index: &WorkspaceIndex,
    request: DuplicateTestsRequest,
) -> Result<EventStream<AnalyzeEvent>, Status> {
    let (activity, root) =
        Activity::arrived("duplicate tests", index, &request.workspace_root).await?;
    let coverage_dir =
        activity.refusing(under(&root, &request.coverage_dir, "coverage directory"))?;
    let out_dir = activity.refusing(under(&root, &request.out_dir, "output directory"))?;
    let min_signature = request.min_signature as usize;
    let subset_ratio = request.subset_ratio;
    let include_test_sources = request.include_test_sources;
    let (events, stream) = event_stream();
    let index = index.clone();

    tokio::spawn(async move {
        let _queued = index.hold(&root).await;
        // One terminal event and no progress, because the detection reports nothing while it runs:
        // `analyze_coverage_dir` takes no sink. The stream is still the right shape — it is what
        // makes a ~22-minute call cancellable by a client hanging up, and it is where progress
        // events will go when the detection can produce them.
        let analysed = tokio::task::spawn_blocking(move || {
            generate_duplicate_tests_report(
                &coverage_dir,
                &out_dir,
                min_signature,
                subset_ratio,
                include_test_sources,
            )
        })
        .await;

        let Some(analysis) = reported(&events, analysed, &activity).await else {
            return;
        };
        let _ = events.send(Ok(duplicates_event(&analysis))).await;
        activity.answered();
    });

    Ok(stream)
}

/// Per-function cyclomatic complexity of one source file, out of the warm scores where they hold.
///
/// The one analysis operation that does **not** take the root's queue. It reads a single file and
/// writes nothing, and its answer names the content it scored rather than a moment in the tree's
/// history — so queueing it behind a 55-minute capture would make a millisecond question
/// unanswerable for an hour and buy nothing in return.
pub(crate) async fn serve_complexity(
    index: &WorkspaceIndex,
    request: ComplexityRequest,
) -> Result<ComplexityResponse, Status> {
    let (activity, root) = Activity::arrived("complexity", index, &request.workspace_root).await?;
    activity.recorded(scored_file(index, root, request).await)
}

/// The scores themselves, with the root already resolved and its arrival already recorded.
async fn scored_file(
    index: &WorkspaceIndex,
    root: PathBuf,
    request: ComplexityRequest,
) -> Result<ComplexityResponse, Status> {
    let file = under(&root, &request.file, "file")?;
    if !file.is_file() {
        return Err(Status::failed_precondition(format!(
            "{} is not a file this process can read",
            file.display()
        )));
    }
    let scores = index.complexity_scores();

    let scored = tokio::task::spawn_blocking(move || {
        let source = std::fs::read_to_string(&file)?;
        cached_file_complexity(scores.as_ref(), &source)
    })
    .await
    .map_err(|failure| joined("complexity", &failure))?
    .map_err(|refusal| status_of_analysis(&refusal))?;

    Ok(ComplexityResponse {
        functions: scored
            .iter()
            .map(|function| FunctionComplexity {
                name: function.name.clone(),
                line: function.line,
                complexity: function.complexity,
            })
            .collect(),
    })
}

/// A path a request names: absolute as given, or under the root it named. Never under this
/// process's own directory — it serves several trees.
fn under(root: &Path, named: &str, what: &str) -> Result<PathBuf, Status> {
    if named.trim().is_empty() {
        return Err(Status::invalid_argument(format!(
            "the request names no {what}"
        )));
    }
    let given = Path::new(named);
    Ok(if given.is_absolute() {
        given.to_path_buf()
    } else {
        root.join(given)
    })
}

/// What an analysis produced, having reported its refusal into its own stream if it had one.
async fn reported<T>(
    events: &EventSender<AnalyzeEvent>,
    outcome: Result<tddy_code_analysis::Result<T>, tokio::task::JoinError>,
    activity: &Activity,
) -> Option<T> {
    let refusal = match outcome {
        Ok(Ok(produced)) => return Some(produced),
        Ok(Err(refusal)) => status_of_analysis(&refusal),
        Err(failure) => joined(activity.method(), &failure),
    };
    activity.refused(&refusal);
    let _ = events.send(Err(refusal)).await;
    None
}

/// One step of a capture, as the event the stream carries it in.
///
/// A total map over [`CaptureProgress`], so a phase the library learns to report is a compile
/// error here rather than a step that silently stops reaching the caller.
fn captured_event(progress: &CaptureProgress<'_>) -> AnalyzeEvent {
    let event = match progress {
        CaptureProgress::BuildStarted => analyze_event::Event::BuildStarted(BuildStarted {}),
        CaptureProgress::BuildFinished { harnesses } => {
            analyze_event::Event::BuildFinished(BuildFinished {
                harnesses: *harnesses as u32,
            })
        }
        CaptureProgress::HarnessStarted {
            index,
            total,
            spec,
            tests,
        } => analyze_event::Event::HarnessStarted(HarnessStarted {
            index: *index as u32,
            total: *total as u32,
            spec: (*spec).to_string(),
            tests: *tests as u32,
        }),
        CaptureProgress::TestCaptured {
            index,
            total,
            name,
            status,
        } => analyze_event::Event::TestCaptured(TestCaptured {
            index: *index as u32,
            total: *total as u32,
            name: (*name).to_string(),
            status: (*status).to_string(),
        }),
        CaptureProgress::Finished { tests, files } => {
            analyze_event::Event::CaptureFinished(CaptureFinished {
                tests: *tests as u32,
                files: *files as u32,
            })
        }
    };
    AnalyzeEvent { event: Some(event) }
}

/// Everything the duplicate detection found, as its one terminal event.
fn duplicates_event(analysis: &DuplicateAnalysis) -> AnalyzeEvent {
    AnalyzeEvent {
        event: Some(analyze_event::Event::DuplicateTests(DuplicateTestsFound {
            identical: analysis
                .identical
                .iter()
                .map(|group| DuplicateGroup {
                    signature_size: group.signature_size as u32,
                    tests: group.tests.clone(),
                })
                .collect(),
            subsets: analysis
                .subsets
                .iter()
                .map(|relation| SubsetRelation {
                    subset: relation.subset.clone(),
                    superset: relation.superset.clone(),
                    subset_size: relation.subset_size as u32,
                    superset_size: relation.superset_size as u32,
                    ratio: relation.ratio,
                })
                .collect(),
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carries_the_silent_build_phase_as_its_own_event() {
        // Given the phase a capture spends minutes in with nothing to say
        let progress = CaptureProgress::BuildStarted;

        // When it is put on the stream
        // Then it arrives as the event that distinguishes a building capture from a hung one
        assert_eq!(
            captured_event(&progress),
            AnalyzeEvent {
                event: Some(analyze_event::Event::BuildStarted(BuildStarted {})),
            }
        );
    }

    #[test]
    fn carries_a_captured_test_with_its_position_name_and_verdict() {
        // Given one test of many, captured
        let progress = CaptureProgress::TestCaptured {
            index: 3,
            total: 45,
            name: "connects",
            status: "failed",
        };

        // When it is put on the stream
        // Then everything a reader counts progress by crosses the wire, the verdict included: a
        // failing test does not stop a capture, so its status is information rather than an error
        assert_eq!(
            captured_event(&progress),
            AnalyzeEvent {
                event: Some(analyze_event::Event::TestCaptured(TestCaptured {
                    index: 3,
                    total: 45,
                    name: "connects".to_string(),
                    status: "failed".to_string(),
                })),
            }
        );
    }

    #[test]
    fn carries_the_denominator_as_the_event_that_ends_a_capture() {
        // Given the summary a completed capture ends with
        let progress = CaptureProgress::Finished {
            tests: 2014,
            files: 109,
        };

        // When it is put on the stream
        // Then both counts arrive, which is what says the denominator was written
        assert_eq!(
            captured_event(&progress),
            AnalyzeEvent {
                event: Some(analyze_event::Event::CaptureFinished(CaptureFinished {
                    tests: 2014,
                    files: 109,
                })),
            }
        );
    }

    #[test]
    fn reads_a_path_named_relatively_against_the_root_it_came_with() {
        // Given a coverage directory named relative to its workspace root
        let root = Path::new("/trees/one");

        // When it is resolved
        let resolved = under(root, "coverage", "coverage directory").expect("it resolves");

        // Then it sits under that root, not under this process's directory
        assert_eq!(resolved, Path::new("/trees/one/coverage"));
    }

    #[test]
    fn refuses_a_request_that_names_no_path_at_all() {
        // Given a request whose crate path is blank
        let root = Path::new("/trees/one");

        // When it is resolved
        let outcome = under(root, "   ", "crate path");

        // Then the request is named as the thing that is wrong, and it says which field
        let refusal = outcome.expect_err("a blank path is refused");
        assert_eq!(refusal.code(), tddy_rpc::Code::InvalidArgument);
        assert!(
            refusal.message().contains("crate path"),
            "the refusal did not say which path was missing: {}",
            refusal.message()
        );
    }
}
