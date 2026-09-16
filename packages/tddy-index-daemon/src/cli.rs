//! What the command line may say, and which of the two lifetimes it asks for.
//!
//! Nothing here runs anything or writes anywhere: a subcommand becomes the **proto request struct**
//! its RPC carries, so the single-shot path has one thing left to do — `await` the trait method.
//! That is the whole reason this binary has one code path rather than two: the command line is a
//! second way of *constructing a request*, not a second implementation of the operations.
//!
//! Split from `main.rs` because the two answer different questions, the way
//! `tddy_code_restructuring::restructure_args` is split from `restructure_cli`: this module is the
//! shape of the arguments, and that one is the shape of the process.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand};
use tddy_index_daemon::proto::code_index::{
    AnchorsRequest, ApplyRequest, CheckRequest, PlanStatusRequest, VerifyRequest,
};

#[derive(Parser)]
#[command(
    name = "tddy-index-daemon",
    about = "A warm rust-analyzer index, served over gRPC and stdio — or one operation, run in \
             process, and gone"
)]
pub(crate) struct IndexDaemonArgs {
    #[command(subcommand)]
    pub(crate) operation: Option<Operation>,

    /// Serve gRPC on this socket address, e.g. `127.0.0.1:7777`.
    #[arg(long, value_name = "ADDR")]
    pub(crate) grpc: Option<SocketAddr>,

    /// Serve gRPC on this AF_UNIX socket path, for a caller that reaches this process by
    /// filesystem coordinate rather than by port.
    #[arg(long, value_name = "PATH")]
    pub(crate) grpc_uds: Option<PathBuf>,

    /// Serve over this process's own stdin and stdout, which dedicates fd 1 to RPC framing.
    #[arg(long)]
    pub(crate) stdio: bool,

    /// Send this process's stderr — its log, and anything a library writes there directly — to
    /// this file instead of the stream it inherited.
    ///
    /// Required rather than derived: where a daemon's log belongs is the installer's answer
    /// (`INSTALL_DAEMON_LOG_DIR`), not something a binary should invent a path for.
    #[arg(long, value_name = "PATH")]
    pub(crate) log_file: Option<PathBuf>,
}

/// The operations a single-shot run can ask for, grouped as the command line that already exists
/// for them groups them.
#[derive(Subcommand)]
pub(crate) enum Operation {
    /// Restructure a tree from a JSONL plan.
    Restructure {
        #[command(subcommand)]
        command: RestructureCommand,
    },
}

#[derive(Subcommand)]
pub(crate) enum RestructureCommand {
    /// Report everything wrong with a plan, without writing anything.
    Check(CheckArgs),
    /// Execute a plan against the working tree.
    Apply(ApplyArgs),
    /// Emit the range anchor covering a named run of items.
    Anchors(AnchorsArgs),
    /// Report how far a plan's journal got.
    Status(PlanArgs),
    /// Compare the tree's statement multiset against a git ref.
    Verify(VerifyArgs),
}

/// The tree an operation names.
///
/// Every subcommand carries it, for the reason every *request* carries it: this process serves
/// several worktrees and has no directory of its own to resolve a tree against.
#[derive(Args)]
pub(crate) struct WorkspaceRoot {
    /// Absolute path to the workspace root the operation applies to.
    #[arg(long, value_name = "DIR")]
    pub(crate) workspace_root: PathBuf,
}

#[derive(Args)]
pub(crate) struct CheckArgs {
    #[command(flatten)]
    pub(crate) root: WorkspaceRoot,

    /// Path to the JSONL plan, absolute or relative to the workspace root.
    pub(crate) plan: PathBuf,

    /// Also resolve every operation through the language server, rather than reading the text
    /// alone.
    #[arg(long)]
    pub(crate) deep: bool,

    /// Report every file the plan names that is longer than this many lines.
    #[arg(long, value_name = "LINES")]
    pub(crate) budget: Option<u32>,
}

#[derive(Args)]
pub(crate) struct ApplyArgs {
    #[command(flatten)]
    pub(crate) root: WorkspaceRoot,

    /// Path to the JSONL plan, absolute or relative to the workspace root.
    pub(crate) plan: PathBuf,

    /// Rehearse in an overlay; nothing is written.
    #[arg(long)]
    pub(crate) dry_run: bool,

    /// Continue an existing journal for this plan rather than refusing it.
    #[arg(long)]
    pub(crate) resume: bool,

    /// Start at this operation index instead of the journal's next.
    #[arg(long, value_name = "INDEX")]
    pub(crate) from: Option<u32>,

    /// Stop after this operation index.
    #[arg(long, value_name = "INDEX")]
    pub(crate) stop_after: Option<u32>,
}

#[derive(Args)]
pub(crate) struct AnchorsArgs {
    #[command(flatten)]
    pub(crate) root: WorkspaceRoot,

    /// The file the items live in, relative to the workspace root.
    pub(crate) file: PathBuf,

    /// The items the anchor must cover, in source order.
    #[arg(long, value_delimiter = ',', value_name = "NAMES")]
    pub(crate) items: Vec<String>,
}

#[derive(Args)]
pub(crate) struct PlanArgs {
    #[command(flatten)]
    pub(crate) root: WorkspaceRoot,

    /// Path to the JSONL plan, absolute or relative to the workspace root.
    pub(crate) plan: PathBuf,
}

#[derive(Args)]
pub(crate) struct VerifyArgs {
    #[command(flatten)]
    pub(crate) root: WorkspaceRoot,

    /// The git ref to compare the working tree against.
    #[arg(long, value_name = "REF")]
    pub(crate) against: String,
}

/// One operation, as the request its RPC carries — the only form the single-shot path deals in.
#[derive(Debug)]
pub(crate) enum Requested {
    Check(CheckRequest),
    Apply(ApplyRequest),
    Anchors(AnchorsRequest),
    PlanStatus(PlanStatusRequest),
    Verify(VerifyRequest),
}

/// Which of the two lifetimes a command line asks for.
#[derive(Debug)]
pub(crate) enum Lifetime {
    /// Run one operation against the service in process, render it, and exit with a status.
    SingleShot(Requested),
    /// Serve that same implementation over these transports and stay alive.
    Serve(Transports),
}

/// Where a serving process answers.
#[derive(Debug)]
pub(crate) struct Transports {
    pub(crate) grpc: Option<SocketAddr>,
    pub(crate) grpc_uds: Option<PathBuf>,
    pub(crate) stdio: bool,
}

impl Transports {
    /// Whether anything at all would be served.
    fn any(&self) -> bool {
        self.grpc.is_some() || self.grpc_uds.is_some() || self.stdio
    }
}

/// Resolve what a command line asked this process to be.
///
/// A transport argument present means serve; none means run the named operation once. **Neither is
/// an error, not a default**, the rule `tddy_sandbox_runner::resolve_workspace_tools_transport`
/// states: started with nothing to serve and nothing to do, the process would come up and wait for
/// nobody, and failing fast beats that. Both together is an error for the mirror-image reason —
/// serving would silently discard an operation the caller asked for.
pub(crate) fn lifetime_of(args: IndexDaemonArgs) -> Result<Lifetime, String> {
    let transports = Transports {
        grpc: args.grpc,
        grpc_uds: args.grpc_uds,
        stdio: args.stdio,
    };
    match (args.operation, transports.any()) {
        (None, true) => Ok(Lifetime::Serve(transports)),
        (Some(operation), false) => Ok(Lifetime::SingleShot(requested(operation)?)),
        (Some(_), true) => Err(
            "an operation and a transport ask for different lifetimes: drop \
                                the subcommand to serve, or drop --grpc/--grpc-uds/--stdio to run \
                                the operation once"
                .to_string(),
        ),
        (None, false) => Err(
            "nothing to do and nothing to serve: name an operation to run once, \
                              or pass --grpc, --grpc-uds or --stdio to serve"
                .to_string(),
        ),
    }
}

/// The request a subcommand carries.
fn requested(operation: Operation) -> Result<Requested, String> {
    let Operation::Restructure { command } = operation;
    Ok(match command {
        RestructureCommand::Check(check) => Requested::Check(CheckRequest {
            workspace_root: named(&check.root.workspace_root)?,
            plan: named(&check.plan)?,
            deep: check.deep,
            // Zero is "no budget report", which is what the schema means by an unset `uint32` and
            // what `operations::file_budget` reads it as.
            file_budget: check.budget.unwrap_or(0),
        }),
        RestructureCommand::Apply(apply) => Requested::Apply(ApplyRequest {
            workspace_root: named(&apply.root.workspace_root)?,
            plan: named(&apply.plan)?,
            dry_run: apply.dry_run,
            resume: apply.resume,
            from: apply.from,
            stop_after: apply.stop_after,
        }),
        RestructureCommand::Anchors(anchors) => Requested::Anchors(AnchorsRequest {
            workspace_root: named(&anchors.root.workspace_root)?,
            file: named(&anchors.file)?,
            items: normalised(anchors.items),
        }),
        RestructureCommand::Status(status) => Requested::PlanStatus(PlanStatusRequest {
            workspace_root: named(&status.root.workspace_root)?,
            plan: named(&status.plan)?,
        }),
        RestructureCommand::Verify(verify) => Requested::Verify(VerifyRequest {
            workspace_root: named(&verify.root.workspace_root)?,
            against: verify.against,
        }),
    })
}

/// A path as the request names it.
///
/// Refused rather than converted lossily: a `String` field cannot carry a path this platform
/// allows, and silently substituting replacement characters would send the service looking for a
/// different file than the one the operator named.
fn named(path: &Path) -> Result<String, String> {
    path.to_str().map(str::to_string).ok_or_else(|| {
        format!(
            "{} is not valid UTF-8, and a request names its paths as UTF-8 text",
            path.display()
        )
    })
}

/// `--items` as the service has always received it: trimmed, with empty elements dropped.
///
/// The same normalisation `restructure_args::normalised_items` performs, and for the same reason:
/// clap's `value_delimiter` splits on the comma and stops there, so `--items "One, Two"` would
/// otherwise name an item literally called `" Two"` — a wrong answer with no error.
fn normalised(items: Vec<String>) -> Vec<String> {
    items
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(argv: &[&str]) -> Result<Lifetime, String> {
        let mut all = vec!["tddy-index-daemon"];
        all.extend_from_slice(argv);
        lifetime_of(IndexDaemonArgs::parse_from(all))
    }

    fn requested_by(argv: &[&str]) -> Requested {
        match parse(argv).expect("the command line resolves") {
            Lifetime::SingleShot(requested) => requested,
            Lifetime::Serve(_) => panic!("expected a single-shot run"),
        }
    }

    fn served_by(argv: &[&str]) -> Transports {
        match parse(argv).expect("the command line resolves") {
            Lifetime::Serve(transports) => transports,
            Lifetime::SingleShot(_) => panic!("expected a serving process"),
        }
    }

    #[test]
    fn a_subcommand_with_no_transport_runs_once() {
        // Given a check with no transport argument
        // When the command line is resolved
        let requested = requested_by(&[
            "restructure",
            "check",
            "--workspace-root",
            "/trees/one",
            "plan.jsonl",
        ]);

        // Then it is the check request, carrying the tree and the plan the operator named
        let Requested::Check(check) = requested else {
            panic!("expected a check request");
        };
        assert_eq!(
            check,
            CheckRequest {
                workspace_root: "/trees/one".to_string(),
                plan: "plan.jsonl".to_string(),
                deep: false,
                file_budget: 0,
            }
        );
    }

    #[test]
    fn a_check_carries_its_depth_and_its_file_length_budget() {
        // Given a deep check with a file-length budget
        let requested = requested_by(&[
            "restructure",
            "check",
            "--workspace-root",
            "/trees/one",
            "plan.jsonl",
            "--deep",
            "--budget",
            "500",
        ]);

        // Then both reach the request
        let Requested::Check(check) = requested else {
            panic!("expected a check request");
        };
        assert_eq!((check.deep, check.file_budget), (true, 500));
    }

    #[test]
    fn an_apply_carries_every_flag_the_request_has_a_field_for() {
        // Given an apply with every flag it takes
        let requested = requested_by(&[
            "restructure",
            "apply",
            "--workspace-root",
            "/trees/one",
            "plan.jsonl",
            "--dry-run",
            "--resume",
            "--from",
            "3",
            "--stop-after",
            "7",
        ]);

        // Then the request carries them as the values clap already parsed
        let Requested::Apply(apply) = requested else {
            panic!("expected an apply request");
        };
        assert_eq!(
            apply,
            ApplyRequest {
                workspace_root: "/trees/one".to_string(),
                plan: "plan.jsonl".to_string(),
                dry_run: true,
                resume: true,
                from: Some(3),
                stop_after: Some(7),
            }
        );
    }

    #[test]
    fn anchors_carries_the_items_as_a_list_rather_than_a_comma_joined_string() {
        // Given an anchors run over three items, spelled the way a human writes them
        let requested = requested_by(&[
            "restructure",
            "anchors",
            "--workspace-root",
            "/trees/one",
            "src/lib.rs",
            "--items",
            "One, Two ,,Three",
        ]);

        // Then the request names the three items, with neither whitespace nor an unnamed one
        let Requested::Anchors(anchors) = requested else {
            panic!("expected an anchors request");
        };
        assert_eq!(
            anchors,
            AnchorsRequest {
                workspace_root: "/trees/one".to_string(),
                file: "src/lib.rs".to_string(),
                items: vec!["One".to_string(), "Two".to_string(), "Three".to_string()],
            }
        );
    }

    #[test]
    fn verify_carries_only_the_tree_and_the_ref_it_holds_it_against() {
        // Given a verify against a ref
        let requested = requested_by(&[
            "restructure",
            "verify",
            "--workspace-root",
            "/trees/one",
            "--against",
            "HEAD~1",
        ]);

        // Then that ref and that tree are the whole request
        let Requested::Verify(verify) = requested else {
            panic!("expected a verify request");
        };
        assert_eq!(
            verify,
            VerifyRequest {
                workspace_root: "/trees/one".to_string(),
                against: "HEAD~1".to_string(),
            }
        );
    }

    #[test]
    fn status_asks_for_one_plan_under_one_tree() {
        // Given a status for a plan
        let requested = requested_by(&[
            "restructure",
            "status",
            "--workspace-root",
            "/trees/one",
            "plan.jsonl",
        ]);

        // Then the request names both
        let Requested::PlanStatus(status) = requested else {
            panic!("expected a plan status request");
        };
        assert_eq!(
            status,
            PlanStatusRequest {
                workspace_root: "/trees/one".to_string(),
                plan: "plan.jsonl".to_string(),
            }
        );
    }

    #[test]
    fn a_transport_with_no_subcommand_serves() {
        // Given both transports at once
        let transports = served_by(&["--stdio", "--grpc", "127.0.0.1:7777"]);

        // Then both are served: neither flag is a default for the other, so passing both must not
        // silently pick one
        assert_eq!(
            (transports.stdio, transports.grpc),
            (
                true,
                Some("127.0.0.1:7777".parse().expect("a socket address"))
            )
        );
    }

    #[test]
    fn a_unix_socket_is_a_transport_of_its_own() {
        // Given only a socket path
        let transports = served_by(&["--grpc-uds", "/run/tddy/index.sock"]);

        // Then that is what is served, with no port and no stdio
        assert_eq!(
            (transports.grpc_uds, transports.grpc, transports.stdio),
            (Some(PathBuf::from("/run/tddy/index.sock")), None, false)
        );
    }

    /// The rule `resolve_workspace_tools_transport` states: a process with no transport at all
    /// would serve nobody, and failing fast beats coming up and waiting forever.
    #[test]
    fn refuses_a_command_line_with_neither_an_operation_nor_a_transport() {
        // Given nothing to do and nothing to serve
        // When the command line is resolved
        let refusal = parse(&[]).expect_err("a process that serves nobody must not come up");

        // Then the refusal names both ways out
        assert_eq!(
            refusal,
            "nothing to do and nothing to serve: name an operation to run once, or pass --grpc, \
             --grpc-uds or --stdio to serve"
        );
    }

    /// The mirror image: serving would discard the operation, so the ambiguity is refused rather
    /// than resolved in favour of one of them.
    #[test]
    fn refuses_a_command_line_asking_to_serve_and_to_run_an_operation_at_once() {
        // Given an operation and a transport together
        let refusal = parse(&[
            "--stdio",
            "restructure",
            "status",
            "--workspace-root",
            "/trees/one",
            "plan.jsonl",
        ])
        .expect_err("two lifetimes at once are refused");

        // Then the refusal says which argument to drop for which lifetime
        assert!(
            refusal.contains("ask for different lifetimes"),
            "the refusal did not name the ambiguity: {refusal}"
        );
    }
}
