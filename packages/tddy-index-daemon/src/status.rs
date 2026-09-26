//! The one place a refusal becomes a status code.
//!
//! One function per error type, matched exhaustively, for two reasons. This service is served over
//! more than one transport at a time, so a refusal classified at the call site would reach gRPC as
//! one code and stdio as another, and a client's retry policy would depend on which one it happened
//! to dial. And a variant added to either error enum later has to be a compile error here rather
//! than silently folding into `Internal` — the same mistake, one level up, as reporting "the index
//! did not settle" as "your plan is malformed".

use tddy_code_analysis::AnalysisError;
use tddy_code_restructuring::RestructureError;
use tddy_lsp::LspError;
use tddy_rpc::Status;

/// The status a restructuring refusal reaches every transport as.
///
/// The classes are the ones a caller acts on differently: `InvalidArgument` means fix the plan and
/// ask again, `FailedPrecondition` means the tree is not in the state the plan was written
/// against, `DeadlineExceeded` means the wait ended before the index did, and `Unavailable` means
/// ask again unchanged.
pub fn status_of(error: &RestructureError) -> Status {
    let refusal = error.to_string();
    match error {
        // The request is at fault: the plan says something this executor will not do, and no
        // amount of waiting or repair to the tree changes that.
        RestructureError::MalformedPlan(_)
        | RestructureError::CodeTextInPlan { .. }
        | RestructureError::UnsupportedOp { .. }
        | RestructureError::NoBackend { .. } => Status::invalid_argument(refusal),
        // The tree is at fault: the request is well formed, and the state it names is not the
        // state on disk. Retrying it unchanged fails identically.
        //
        // `SeamRefused` belongs here rather than beside `MalformedPlan`: the plan says something
        // the executor would happily do, and the code it names will not permit it. A client that
        // rewrote its plan on this would ask again and be refused the same way.
        RestructureError::SeamRefused(_)
        | RestructureError::SnapshotMismatch { .. }
        | RestructureError::ItemChanged { .. }
        | RestructureError::PlanChangedOnDisk { .. }
        | RestructureError::NeedsIndexDaemon { .. }
        | RestructureError::StaleOperation { .. }
        | RestructureError::AnchorInvalidated { .. }
        | RestructureError::JournalExists
        | RestructureError::RepoScopedJournal { .. }
        | RestructureError::CheckpointDivergence { .. }
        | RestructureError::IndeterminateJournal { .. }
        | RestructureError::NotAGitWorktree { .. }
        // A tree that did not compile before the plan ran: nothing was written, and the same
        // request fails identically until the tree is repaired.
        | RestructureError::BaselineDoesNotCompile { .. } => Status::failed_precondition(refusal),
        // The wait ended before the index was ready. Nothing here says the plan is wrong, which is
        // the distinction `docs/dev/todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md`
        // records as actively misleading when it is lost.
        RestructureError::IndexingIncomplete { .. } | RestructureError::ServerNotSettled { .. } => {
            Status::deadline_exceeded(refusal)
        }
        // The server asked to be asked again, so the same request is worth repeating.
        RestructureError::ServerCatchingUp => Status::unavailable(refusal),
        // The caller stopped waiting mid-request, which is neither a slow server nor a bad plan.
        // The same class `LspError::Abandoned` gets in [`status_of_lsp`], and for the same reason:
        // reporting somebody's `^C` as a deadline sends whoever reads the log hunting a server
        // that was answering perfectly well. A client still listening never sees it — the run that
        // caused it is the one that has gone — so it is on the record for the log's sake.
        RestructureError::CallerStopped => Status::cancelled(refusal),
        // Nothing the caller did produced it and nothing the caller changes fixes it. The server
        // answered and the answer could not be used, which is this service's own problem to report
        // rather than the client's to act on.
        //
        // A tree the run's own accepted operations left uncompilable is the same kind of fault:
        // the executor produced it, and the caller asked for nothing it should not have.
        RestructureError::ServerDefect(_)
        | RestructureError::AppliedTreeDoesNotCompile { .. }
        | RestructureError::Io(_) => Status::internal(refusal),
    }
}

/// The status an analysis refusal reaches every transport as.
///
/// The same four classes, read against the same question: can the caller fix this by asking
/// differently, by repairing the tree, or not at all? Coverage, CRAP and duplicate detection all
/// stand on artefacts a previous run wrote, so "capture first" is the commonest answer here and it
/// is a precondition rather than a defect in the request. The fourth class, `DeadlineExceeded`,
/// covers the two long operations stopping because nobody was left waiting for them.
pub fn status_of_analysis(error: &AnalysisError) -> Status {
    let refusal = error.to_string();
    match error {
        // The request is at fault: it named a path that holds no crate, or a source no Rust parser
        // accepts. Naming something else is the only thing that changes the answer.
        AnalysisError::NotACrate { .. } | AnalysisError::Syn(_) => {
            Status::invalid_argument(refusal)
        }
        // The state is at fault: the artefacts a report stands on were never captured, the tree
        // does not build, or this host has no llvm tooling. Each is repaired outside the request,
        // and none of them is answered by rewording it.
        AnalysisError::MissingCoverage { .. }
        | AnalysisError::MissingLlvmTool { .. }
        | AnalysisError::Cargo(_) => Status::failed_precondition(refusal),
        // The work stopped because its caller went away, which is the same class as a wait that
        // ended before the index did: the request was fine and the answer simply did not arrive in
        // the time it had. Deliberately not `Cancelled` — a client that is still listening never
        // sees this, and the one that caused it has already gone.
        AnalysisError::Cancelled { .. } => Status::deadline_exceeded(refusal),
        // Nothing the caller did caused it and nothing it can do fixes it: an unreadable file, an
        // artefact that will not parse as the JSON this pipeline writes, or a refusal the library
        // left as prose. `Message` lands here deliberately — a class a caller cannot act on is
        // exactly what an unclassified refusal is, and the honest fix is to give it a variant
        // rather than to guess a code for it here.
        AnalysisError::Io(_)
        | AnalysisError::Json(_)
        | AnalysisError::Message(_)
        | AnalysisError::Anyhow(_) => Status::internal(refusal),
    }
}

/// The status a failure to reach a language server for a root arrives as.
///
/// Distinct from [`status_of`] only in the error type it reads: a client cannot act on the
/// difference between "rust-analyzer is not installed" and "rust-analyzer died", so both are the
/// state of this host rather than a defect in the request.
pub fn status_of_lsp(error: &LspError) -> Status {
    let failure = error.to_string();
    match error {
        // This process serves no server for that language — a property of how the host was wired,
        // which the caller can neither see nor fix by rewording the request.
        LspError::LanguageNotAllowed(_) | LspError::ServerNotFound(_) => {
            Status::failed_precondition(failure)
        }
        LspError::Timeout => Status::deadline_exceeded(failure),
        // The caller stopped waiting, which is neither a slow server nor a bad request. Kept apart
        // from `Timeout` on purpose: reporting an interrupt as a deadline sends whoever reads the
        // log hunting a server that was answering perfectly well.
        LspError::Abandoned => Status::cancelled(failure),
        LspError::ServerExited => Status::unavailable(failure),
        // The server answered, and what it said made no sense. Nothing the caller did caused it.
        LspError::Protocol(_)
        | LspError::Server { .. }
        | LspError::DocumentNotOpen(_)
        | LspError::Io(_) => Status::internal(failure),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_rpc::Code;

    #[test]
    fn reports_a_plan_carrying_code_text_as_an_invalid_argument() {
        // Given a plan that supplied source text the engine should have produced
        let refusal = RestructureError::CodeTextInPlan {
            field: "text".to_string(),
        };

        // When it is classified
        let status = status_of(&refusal);

        // Then the request is named as the thing that is wrong
        assert_eq!(status.code(), Code::InvalidArgument);
    }

    /// A seam the code refuses is the tree's state, not the request's. A client told
    /// `InvalidArgument` rewrites its plan and asks again with the same result; told
    /// `FailedPrecondition`, it knows the plan is right and the cut is in the wrong place.
    #[test]
    fn reports_a_refused_seam_as_a_failed_precondition() {
        // Given a seam the code will not permit, on a plan that is well formed
        let refusal = RestructureError::SeamRefused("this seam cuts an `impl` in half".to_string());

        // When it is classified
        let status = status_of(&refusal);

        // Then the tree is named as the thing that is wrong
        assert_eq!(status.code(), Code::FailedPrecondition);
    }

    /// Nothing the caller did produced this and nothing the caller changes fixes it, which is what
    /// `Internal` means. Reporting it as `InvalidArgument` sends them editing a correct plan.
    #[test]
    fn reports_an_unusable_answer_from_the_server_as_internal() {
        // Given an extraction rust-analyzer produced before it could infer the signature
        let refusal =
            RestructureError::ServerDefect("rust-analyzer wrote `fn f(v: _) -> _`".to_string());

        // When it is classified
        let status = status_of(&refusal);

        // Then neither the plan nor the tree is blamed for it
        assert_eq!(status.code(), Code::Internal);
    }

    #[test]
    fn reports_an_existing_journal_as_a_failed_precondition() {
        // Given a root whose journal a previous run left behind
        let refusal = RestructureError::JournalExists;

        // When it is classified
        let status = status_of(&refusal);

        // Then the tree is named as the thing that is wrong, so a client does not rewrite its plan
        assert_eq!(status.code(), Code::FailedPrecondition);
    }

    /// The leg the misclassification entry is about: a wait that ended before the index did used to
    /// arrive as `MalformedPlan`, which sent the reader to rewrite anchors that were never wrong.
    #[test]
    fn reports_an_unsettled_server_as_a_deadline_rather_than_an_invalid_argument() {
        // Given a server that would not settle enough to answer
        let refusal = RestructureError::ServerNotSettled {
            method: "textDocument/codeAction".to_string(),
            seconds: 46,
            last: "loading crate graph; furthest indexing 12%".to_string(),
        };

        // When it is classified
        let status = status_of(&refusal);

        // Then it is a deadline, not a defective plan
        assert_eq!(status.code(), Code::DeadlineExceeded);
    }

    #[test]
    fn reports_a_server_still_catching_up_as_worth_asking_again() {
        // Given the server's own "ask me again"
        let refusal = RestructureError::ServerCatchingUp;

        // When it is classified
        let status = status_of(&refusal);

        // Then the caller is told to retry rather than to change anything
        assert_eq!(status.code(), Code::Unavailable);
    }

    /// A report over a coverage directory nothing has captured into is the commonest analysis
    /// refusal there is, and "run the capture first" is a precondition, not a defective request.
    #[test]
    fn reports_a_missing_capture_as_a_failed_precondition() {
        // Given a report asked for before anything was captured
        let refusal = AnalysisError::MissingCoverage {
            path: "/trees/one/coverage/rust-coverage-final.json".to_string(),
        };

        // When it is classified
        let status = status_of_analysis(&refusal);

        // Then the state is named as the thing to repair, not the request
        assert_eq!(status.code(), Code::FailedPrecondition);
    }

    #[test]
    fn reports_a_path_that_holds_no_crate_as_an_invalid_argument() {
        // Given a capture asked for over a directory with no manifest in it
        let refusal = AnalysisError::NotACrate {
            path: "/trees/one/Cargo.toml".to_string(),
        };

        // When it is classified
        let status = status_of_analysis(&refusal);

        // Then the request is named as the thing that is wrong
        assert_eq!(status.code(), Code::InvalidArgument);
    }

    /// The class the caller's own going-away deserves. Nothing about the request was wrong and
    /// nothing on the host is broken — the work simply did not get the time it needed, which is
    /// what `DeadlineExceeded` already means for a wait that ended before the index did.
    #[test]
    fn reports_a_capture_stopped_because_nobody_was_listening_as_a_deadline() {
        // Given a capture that stopped when its caller went away
        let refusal = AnalysisError::Cancelled {
            work: "coverage capture".to_string(),
            reached: "312 test(s) captured, and no denominator was written".to_string(),
        };

        // When it is classified
        let status = status_of_analysis(&refusal);

        // Then it is a deadline rather than a defective request or a broken server
        assert_eq!(status.code(), Code::DeadlineExceeded);
    }

    /// The caller's own going-away, arriving through the restructuring library this time rather
    /// than through the LSP client: a run interrupted by whoever asked for it is not a slow server
    /// and not a defective plan, and reporting it as either sends the reader hunting the wrong
    /// thing. Told apart from a wait that ended before the index did, which is a deadline.
    #[test]
    fn reports_a_run_its_caller_stopped_as_cancelled_rather_than_as_a_deadline() {
        // Given a run whose caller stopped waiting mid-request
        let stopped = RestructureError::CallerStopped;
        // And a wait that ended before the index was loaded
        let unfinished = RestructureError::IndexingIncomplete {
            seconds: 46,
            last: "loading crate graph; furthest indexing 12%".to_string(),
            environment: "rust-analyzer".to_string(),
        };

        // When each is classified
        // Then the interrupt is cancelled and the unfinished wait is a deadline
        assert_eq!(status_of(&stopped).code(), Code::Cancelled);
        assert_eq!(status_of(&unfinished).code(), Code::DeadlineExceeded);
    }

    #[test]
    fn reports_a_missing_language_server_as_a_failed_precondition() {
        // Given a host with no rust-analyzer to reach
        let failure = LspError::ServerNotFound("rust-analyzer".to_string());

        // When it is classified
        let status = status_of_lsp(&failure);

        // Then the host's state is named, not the request
        assert_eq!(status.code(), Code::FailedPrecondition);
    }

    #[test]
    fn reports_an_abandoned_request_as_cancelled_rather_than_as_a_deadline() {
        // Given a request whose caller stopped waiting for it
        let abandoned = LspError::Abandoned;
        // And a request that outlasted a bound instead
        let expired = LspError::Timeout;

        // When each is classified
        let stopped = status_of_lsp(&abandoned);
        let timed_out = status_of_lsp(&expired);

        // Then the two are told apart. Collapsing them would send whoever reads the log hunting a
        // slow server for what was somebody pressing ^C.
        assert_eq!(stopped.code(), Code::Cancelled);
        assert_eq!(timed_out.code(), Code::DeadlineExceeded);
    }
}
