use crate::activity::Activity;
use crate::index::routing::Route;
use crate::index::WorkspaceIndex;
use crate::operations::joined;
use crate::proto::code_index::{VerifyRequest, VerifyResponse};
use crate::status::status_of;
use std::collections::BTreeSet;
use std::path::PathBuf;
use tddy_code_restructuring::runner::{self, Command, Options};
use tddy_code_restructuring::LanguageSupport;
use tddy_rpc::Status;

/// Hold the working tree's statements against a git ref's, as multisets.
pub(crate) async fn serve_verify(
    index: &WorkspaceIndex,
    request: VerifyRequest,
) -> Result<VerifyResponse, Status> {
    let (activity, root) = Activity::arrived("verify", index, &request.workspace_root).await?;
    activity.recorded(comparison_against(index, root, request).await)
}

/// The comparison itself, with the root already resolved and its arrival already recorded.
async fn comparison_against(
    index: &WorkspaceIndex,
    root: PathBuf,
    request: VerifyRequest,
) -> Result<VerifyResponse, Status> {
    if request.against.trim().is_empty() {
        return Err(Status::invalid_argument(
            "the request names no git ref to verify against",
        ));
    }
    let options = Options {
        command: Command::Verify,
        against: Some(request.against),
        retargets: request.retargets,
        repoints: request.repoints,
        rebinds: request.rebinds,
        detaches: request.detaches,
        ..Options::default()
    };

    let _queued = index.hold(&root).await;
    let (options, changed) = with_extension_verifies(index, &root, options).await?;
    let (comparison, findings) =
        tokio::task::spawn_blocking(move || runner::verify_with_sources(&root, options, &changed))
            .await
            .map_err(|failure| joined("verify", &failure))?
            .map_err(|refusal| status_of(&refusal))?;

    // A comparison that does not hold is this answer with `holds` false, not an error: the
    // comparison *is* what was asked for, and a caller that receives the statements it lost and
    // gained decides for itself what they mean. Only a tree that could not be compared at all —
    // one that is not a git worktree, or a ref git will not read — refuses the request.
    Ok(VerifyResponse {
        holds: comparison.holds() && findings.is_empty(),
        source_findings: tddy_code_restructuring::console::source_findings(&findings),
        before: comparison.before as u32,
        after: comparison.after as u32,
        missing: comparison.missing,
        added: comparison.added,
        repointed: comparison.excused.repointed as u32,
        visibility_normalised: comparison.excused.visibility as u32,
        cfg_test_gates: comparison.excused.cfg_test_gates as u32,
    })
}

/// `options` with the verify of every installed extension language one of the changed files
/// belongs to (`#reshape` extension-code-actions, R5), each over the root's warm server for that
/// language — started if this process holds none — so its new error diagnostics can be read. A
/// language with no server is verified by its grammar's parse alone; a server that will not start
/// refuses the request.
///
/// The changed files come back with the options, so the verify reads them once: none when no
/// extension language is installed, since there is then no verify to hand them to.
async fn with_extension_verifies(
    index: &WorkspaceIndex,
    root: &std::path::Path,
    options: Options,
) -> Result<(Options, BTreeSet<String>), Status> {
    let languages = index.verified_languages();
    if languages.is_empty() {
        return Ok((options, BTreeSet::new()));
    }
    let (options, changed) = {
        let root = root.to_path_buf();
        tokio::task::spawn_blocking(move || {
            runner::changed_sources(&root, &options).map(|changed| (options, changed))
        })
        .await
        .map_err(|failure| joined("verify", &failure))?
        .map_err(|refusal| status_of(&refusal))?
    };
    let mut servers = Vec::new();
    for language in languages.iter() {
        let touched = changed
            .iter()
            .any(|path| language.claims(std::path::Path::new(path)));
        let Some(launch) = language.launch().filter(|_| touched) else {
            continue;
        };
        let route = Route::Extension {
            language: language.id().clone(),
            launch: Box::new(launch),
        };
        let served = index.client_for_route(root, &route).await?;
        servers.push((language.id().clone(), served.client));
    }
    let options = Options {
        verifies: tddy_zed_extensions::ExtensionVerify::for_languages(&languages, &servers),
        ..options
    };
    Ok((options, changed))
}
