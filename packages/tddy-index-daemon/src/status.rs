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
        RestructureError::SnapshotMismatch { .. }
        | RestructureError::AnchorInvalidated { .. }
        | RestructureError::JournalExists
        | RestructureError::CheckpointDivergence { .. }
        | RestructureError::IndeterminateJournal { .. }
        | RestructureError::NotAGitWorktree { .. } => Status::failed_precondition(refusal),
        // The wait ended before the index was ready. Nothing here says the plan is wrong, which is
        // the distinction `docs/dev/todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md`
        // records as actively misleading when it is lost.
        RestructureError::IndexingIncomplete { .. } | RestructureError::ServerNotSettled { .. } => {
            Status::deadline_exceeded(refusal)
        }
        // The server asked to be asked again, so the same request is worth repeating.
        RestructureError::ServerCatchingUp => Status::unavailable(refusal),
        RestructureError::Io(_) => Status::internal(refusal),
    }
}

/// The status an analysis refusal reaches every transport as.
///
/// The same four classes, read against the same question: can the caller fix this by asking
/// differently, by repairing the tree, or not at all? Coverage, CRAP and duplicate detection all
/// stand on artefacts a previous run wrote, so "capture first" is the commonest answer here and it
/// is a precondition rather than a defect in the request.
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

    #[test]
    fn reports_a_missing_language_server_as_a_failed_precondition() {
        // Given a host with no rust-analyzer to reach
        let failure = LspError::ServerNotFound("rust-analyzer".to_string());

        // When it is classified
        let status = status_of_lsp(&failure);

        // Then the host's state is named, not the request
        assert_eq!(status.code(), Code::FailedPrecondition);
    }
}
