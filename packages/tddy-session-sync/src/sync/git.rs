use crate::attach::SessionAddress;
use crate::credentials::DaemonToken;
use crate::sync::LOCAL_WIP_REF;
use crate::sync::ORIGIN;
use crate::sync::REMOTE_GIT_SSH_COMMAND;

/// One git command, with the environment it needs beyond the inherited one.
///
/// `env` carries the daemon token the transport authenticates with, so it is **never rendered** —
/// [`GitInvocation::command_line`] is what failures and logs print, and it shows the argv only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitInvocation {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl GitInvocation {
    /// What a log line or an error names this command by. The argv alone: see the type's note.
    pub fn command_line(&self) -> String {
        format!("git {}", self.args.join(" "))
    }
}

/// What `tddy-remote-git-repo` needs in its environment to reach the daemon on the syncer's behalf.
///
/// Not `Debug`, and not rendered anywhere: it holds a daemon credential.
#[derive(Clone)]
pub struct GitTransport {
    pub daemon_url: String,
    pub token: DaemonToken,
}

impl GitTransport {
    /// The environment variables a git subprocess is given.
    ///
    /// The token is passed on **as the kind it was given as**. A refresh token is not exchanged for
    /// this leg even though the syncer holds an access token by now: an access token lives five
    /// minutes and a clone of a large repository does not fit in five minutes, so handing over the
    /// short-lived one would be a transport that fails partway through a first attach.
    pub fn vars(&self) -> Vec<(String, String)> {
        let (token_var, token) = match &self.token {
            DaemonToken::Access(token) => ("TDDY_SESSION_TOKEN", token),
            DaemonToken::Refresh(token) => ("TDDY_REFRESH_TOKEN", token),
        };
        vec![
            (
                "GIT_SSH_COMMAND".to_string(),
                REMOTE_GIT_SSH_COMMAND.to_string(),
            ),
            ("TDDY_DAEMON_URL".to_string(), self.daemon_url.clone()),
            (token_var.to_string(), token.clone()),
        ]
    }
}

/// The remote a project is reached at over the existing git transport: `{daemon}:{project}`.
///
/// The right-hand side is a project **name or id** in the daemon's registry, never a filesystem
/// path — the daemon resolves the repository itself, so nothing here can select a directory its
/// registry does not already name.
pub fn remote_url(daemon_instance_id: &str, project: &str) -> String {
    format!("{daemon_instance_id}:{project}")
}

/// The ref the daemon points at each tick's WIP commit.
pub fn wip_ref(session_id: &str) -> String {
    format!("refs/tddy/session/{session_id}/wip")
}

/// Turn an empty owned destination into a mirror of the session (AC27).
///
/// `git init` + `git remote add` rather than `git clone`, for one reason: by the time this runs the
/// destination already carries the `.tddy-session-sync.json` marker that says the syncer owns it,
/// and `git clone` refuses a directory that is not empty. Writing the marker afterwards is not an
/// option — it is what establishes ownership, and writing into a directory before establishing that
/// is precisely what AC26 refuses to do. The fetch that follows carries the same objects a clone
/// would have, by the same transport.
pub fn first_attach_commands(
    address: &SessionAddress,
    transport: &GitTransport,
) -> Vec<GitInvocation> {
    let mut commands = vec![
        GitInvocation {
            args: vec!["init".to_string(), "--quiet".to_string()],
            env: Vec::new(),
        },
        GitInvocation {
            args: vec![
                "remote".to_string(),
                "add".to_string(),
                ORIGIN.to_string(),
                remote_url(&address.daemon_instance_id, &address.project_id),
            ],
            env: Vec::new(),
        },
    ];
    commands.extend(reconcile_commands(&address.session_id, transport));
    commands
}

/// Restore the mirror from git — the whole reconcile surface (AC31).
///
/// The last two commands are one move split in two, and the split is the point. A plain
/// `git reset --hard refs/tddy/wip` would leave `HEAD` at the **WIP commit**, which is a commit the
/// session's own checkout is not on: every delta afterwards is cut from the session's real `HEAD`,
/// so every one of them would be refused as a base-commit mismatch and every tick would reconcile
/// again, forever. So `HEAD` is put on the WIP commit's parent — which is exactly the session's
/// `HEAD` — and the working tree is then filled from the WIP tree, leaving the mirror in the state
/// the session's checkout is actually in: on the session's commit, with the agent's uncommitted
/// edits present and uncommitted.
pub fn reconcile_commands(session_id: &str, transport: &GitTransport) -> Vec<GitInvocation> {
    vec![
        GitInvocation {
            args: vec![
                "fetch".to_string(),
                ORIGIN.to_string(),
                // Forced, because the ref moves to a new commit every tick and its old value is
                // not an ancestor of its new one.
                format!("+{}:{LOCAL_WIP_REF}", wip_ref(session_id)),
            ],
            env: transport.vars(),
        },
        GitInvocation {
            args: vec![
                "reset".to_string(),
                "--hard".to_string(),
                format!("{LOCAL_WIP_REF}^"),
            ],
            env: Vec::new(),
        },
        GitInvocation {
            args: vec![
                "read-tree".to_string(),
                "-u".to_string(),
                "--reset".to_string(),
                LOCAL_WIP_REF.to_string(),
            ],
            env: Vec::new(),
        },
    ]
}
