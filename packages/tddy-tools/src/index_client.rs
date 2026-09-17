//! `restructure` against a warm index daemon, when a developer has opted into one.
//!
//! `TDDY_INDEX_SOCKET` names one endpoint and that is the whole discovery protocol. Set and
//! non-empty, the operation runs in the process holding a warm rust-analyzer index — six to ten
//! minutes of crate-graph load, paid once for the session instead of once per retry. Unset or
//! empty, [`tddy_code_restructuring::restructure_cli::run`] runs exactly as it does today, which
//! is what leaves CI unaffected by construction.
//!
//! **A socket that is set and unreachable is an error.** Falling back to the cold path would make
//! a misconfigured daemon indistinguishable from a working one — the run would simply be slow
//! forever — and it is the fallback `CLAUDE.md` forbids without explicit consent.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tddy_code_restructuring::restructure_cli::{
    RestructureAnchorsArgs, RestructureArgs, RestructureCheckArgs, RestructureCommand,
    RestructurePlanArgs, RestructureVerifyArgs,
};
use tddy_index_daemon::proto::code_index::{
    AnchorsRequest, ApplyRequest, CheckRequest, PlanStatusRequest, VerifyRequest,
};
use tddy_index_daemon::proto::tonic_code_index::code_index_service_client::CodeIndexServiceClient;
use tonic::transport::Channel;

use crate::index_console::{self, Rendered};

/// The environment variable a developer opts in with. `./run-index-daemon` prints the one line
/// that sets it.
const TDDY_INDEX_SOCKET: &str = "TDDY_INDEX_SOCKET";

/// Run `args` against the warm daemon if one has been named, else against today's cold path.
pub(crate) async fn run_restructure(args: RestructureArgs) -> Result<()> {
    match warm_index_socket(std::env::var(TDDY_INDEX_SOCKET).ok().as_deref()) {
        Some(socket) => restructure_at(&socket, args).await,
        None => tddy_code_restructuring::restructure_cli::run(args).await,
    }
}

/// The socket a warm index daemon is serving on, as the environment names it.
///
/// **Empty is unset**, the rule `LIVEKIT_TESTKIT_WS_URL` states
/// (`packages/tddy-livekit-testkit/src/livekit_testkit.rs:78-88`, cited as precedent by
/// `packages/tddy-vm-testkit/src/env_file.rs:81`): `export TDDY_INDEX_SOCKET=` is how a shell says
/// "not this one", and reading it as a path would turn the obvious way of turning the feature off
/// into a refusal. Taken as a parameter rather than read here so it can be pinned without
/// `std::env::set_var`, which is process-global while tests run in parallel threads.
fn warm_index_socket(named: Option<&str>) -> Option<PathBuf> {
    let named = named?.trim();
    (!named.is_empty()).then(|| PathBuf::from(named))
}

/// Run `args` against the daemon serving on `socket`.
async fn restructure_at(socket: &Path, args: RestructureArgs) -> Result<()> {
    // The one place the process directory becomes a workspace root, and the same place the cold
    // path puts it: a command line is the case where the operator has already chosen a tree by
    // standing in it. The daemon serves several trees and refuses a relative root, so this is also
    // what makes the request answerable at all.
    let root = named(&std::env::current_dir().context("current_dir")?)?;
    // On stderr for every command, `anchors` included: which endpoint served a run is narration
    // about the run, never part of the answer. Said at all because a warm run and a cold one
    // produce the same console otherwise, and a developer who cannot tell them apart cannot tell a
    // daemon serving the wrong tree from one serving theirs.
    eprintln!(
        "restructure: running against the warm index daemon at {}",
        socket.display()
    );

    let mut client = client_at(socket).await?;
    match args.command {
        RestructureCommand::Check(check) => self::check(&mut client, root, check).await,
        RestructureCommand::Apply(apply) => self::apply(&mut client, root, apply).await,
        RestructureCommand::Status(plan) => self::status(&mut client, root, plan).await,
        RestructureCommand::Anchors(anchors) => self::anchors(&mut client, root, anchors).await,
        RestructureCommand::Verify(verify) => self::verify(&mut client, root, verify).await,
    }
}

/// A `code_index.CodeIndexService` client over the AF_UNIX socket at `socket`.
///
/// `connect_uds_channel` rather than a connector of its own: the doc comment on it states the rule
/// — *"reused by any AF_UNIX tonic client … so the connector pattern lives in one place"*.
async fn client_at(socket: &Path) -> Result<CodeIndexServiceClient<Channel>> {
    let channel = tddy_sandbox_runner::connect_uds_channel(socket)
        .await
        .with_context(|| {
            format!(
                "reach the warm index daemon named by {TDDY_INDEX_SOCKET} at {}. Start one with \
                 ./run-index-daemon, or unset {TDDY_INDEX_SOCKET} to run this without a daemon.",
                socket.display()
            )
        })?;
    Ok(CodeIndexServiceClient::new(channel))
}

/// Everything wrong with a plan, without writing anything.
async fn check(
    client: &mut CodeIndexServiceClient<Channel>,
    workspace_root: String,
    args: RestructureCheckArgs,
) -> Result<()> {
    let request = CheckRequest {
        workspace_root,
        plan: named(&args.plan)?,
        deep: args.deep,
        // Zero is "no budget report", which is what the schema means by an unset `uint32` and what
        // the service reads it as: proto3 cannot tell an unset field from a zero one, and a budget
        // of zero lines would report every file the plan names as over it.
        file_budget: args.budget.unwrap_or(0) as u32,
    };

    let mut events = client.check(request).await.map_err(refused)?.into_inner();
    let mut rendered = Rendered::new(false);
    while let Some(event) = events.message().await.map_err(refused)? {
        rendered.event(&event)?;
    }
    rendered.verdict_on_findings()
}

/// Execute a plan against the working tree.
async fn apply(
    client: &mut CodeIndexServiceClient<Channel>,
    workspace_root: String,
    args: RestructurePlanArgs,
) -> Result<()> {
    let rehearsal = args.dry_run;
    let request = ApplyRequest {
        workspace_root,
        plan: named(&args.plan)?,
        dry_run: args.dry_run,
        resume: args.resume,
        from: args.from.map(|from| from as u32),
        stop_after: args.stop_after.map(|stop_after| stop_after as u32),
    };

    let mut events = client.apply(request).await.map_err(refused)?.into_inner();
    let mut rendered = Rendered::new(rehearsal);
    while let Some(event) = events.message().await.map_err(refused)? {
        rendered.event(&event)?;
    }
    Ok(())
}

/// How far a plan's journal got.
async fn status(
    client: &mut CodeIndexServiceClient<Channel>,
    workspace_root: String,
    args: RestructurePlanArgs,
) -> Result<()> {
    let response = client
        .plan_status(PlanStatusRequest {
            workspace_root,
            plan: named(&args.plan)?,
        })
        .await
        .map_err(refused)?
        .into_inner();
    index_console::plan_status(&response);
    Ok(())
}

/// The range anchor covering a named run of items.
async fn anchors(
    client: &mut CodeIndexServiceClient<Channel>,
    workspace_root: String,
    args: RestructureAnchorsArgs,
) -> Result<()> {
    let file = named(&args.file)?;
    let response = client
        .anchors(AnchorsRequest {
            workspace_root,
            file: file.clone(),
            items: normalised(args.items),
        })
        .await
        .map_err(refused)?
        .into_inner();
    index_console::anchors(&file, &response)
}

/// Hold the working tree's statements against a git ref's.
async fn verify(
    client: &mut CodeIndexServiceClient<Channel>,
    workspace_root: String,
    args: RestructureVerifyArgs,
) -> Result<()> {
    let response = client
        .verify(VerifyRequest {
            workspace_root,
            against: args.against,
        })
        .await
        .map_err(refused)?
        .into_inner();
    index_console::verify(&response)
}

/// `--items` as the service has always received it: trimmed, with empty elements dropped.
///
/// clap's `value_delimiter = ','` splits on the comma and stops there, so `--items "One, Two"`
/// would otherwise name an item literally called `" Two"` and `--items "A,,B"` would carry an
/// unnamed one — a wrong answer with no error. The cold path normalises in
/// `restructure_args::normalised_items` and the daemon's own command line in `cli::normalised`,
/// both private to their crates, so the third front end states it too rather than sending a
/// request the other two could not have sent.
fn normalised(items: Vec<String>) -> Vec<String> {
    items
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

/// A path as a request names it.
///
/// Refused rather than converted lossily: a `String` field cannot carry a path this platform
/// allows, and substituting replacement characters would send the daemon looking for a different
/// file than the one the operator named. The rule `tddy_index_daemon`'s own command line states.
fn named(path: &Path) -> Result<String> {
    path.to_str().map(str::to_string).ok_or_else(|| {
        anyhow::anyhow!(
            "{} is not valid UTF-8, and a request names its paths as UTF-8 text",
            path.display()
        )
    })
}

/// A refusal from the daemon, as this front end's failed run.
///
/// The code travels with the message because it is what distinguishes the classes a caller acts
/// differently on — a `DeadlineExceeded` index that did not settle from an `InvalidArgument` plan
/// that is malformed — and the whole reason the service maps its errors onto statuses at all.
fn refused(status: tonic::Status) -> anyhow::Error {
    anyhow::anyhow!(
        "the index daemon refused this run ({:?}): {}",
        status.code(),
        status.message()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_named_socket_is_the_endpoint_a_run_uses() {
        // Given the variable naming a socket
        // When it is read
        // Then that path is the endpoint
        assert_eq!(
            warm_index_socket(Some("/run/tddy/index.sock")),
            Some(PathBuf::from("/run/tddy/index.sock"))
        );
    }

    #[test]
    fn an_unset_variable_names_no_endpoint() {
        // Given no variable at all
        // When it is read
        // Then there is no endpoint, and the cold path runs
        assert_eq!(warm_index_socket(None), None);
    }

    /// `export TDDY_INDEX_SOCKET=` is how a shell says "not this one". The rule
    /// `LIVEKIT_TESTKIT_WS_URL` states, kept because a second spelling of empty-as-unset in one
    /// workspace is a convention nobody can rely on.
    #[test]
    fn an_empty_variable_names_no_endpoint() {
        // Given the variable exported with no value
        // When it is read
        // Then there is no endpoint
        assert_eq!(warm_index_socket(Some("")), None);
    }

    /// The same rule as empty, for the same reason: a variable holding only whitespace was not set
    /// to a path, and a run that tried to reach `"  "` would fail with a refusal about a socket
    /// nobody meant to name.
    #[test]
    fn a_whitespace_only_variable_names_no_endpoint() {
        // Given the variable holding nothing but spaces
        // When it is read
        // Then there is no endpoint
        assert_eq!(warm_index_socket(Some("   ")), None);
    }

    /// A path a shell left padded is still that path: `eval` of an `export` line is the documented
    /// way in, and a stray space must not produce a socket path with one in it.
    #[test]
    fn a_padded_variable_names_the_socket_without_the_padding() {
        // Given the variable naming a socket with whitespace around it
        // When it is read
        // Then the endpoint is the path, trimmed
        assert_eq!(
            warm_index_socket(Some("  /run/tddy/index.sock\n")),
            Some(PathBuf::from("/run/tddy/index.sock"))
        );
    }
}
