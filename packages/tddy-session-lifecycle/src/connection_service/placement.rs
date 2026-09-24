/// Where a session's git worktree lives relative to the daemon running its agent.
///
/// The second placement axis added by `docs/ft/daemon/remote-managed-worktree.md`:
/// `daemon_instance_id` still decides where the agent process runs, and this decides whose
/// filesystem holds the worktree it works in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodebasePlacement {
    /// Agent and worktree on the same daemon — every session created before split placement existed.
    CoLocated,
    /// Agent here, worktree on `codebase_instance_id`.
    Split { codebase_instance_id: String },
    /// Agent and worktree on this daemon, and the worktree inside a `--workspace-tools` jail the
    /// agent reaches only through `mcp__tddy-tools__*`.
    ///
    /// The inversion [`CodebasePlacement::Split`] performs across two hosts, performed on one: the
    /// code is confined and the agent is not. Requested explicitly by `sandboxed_codebase`, never
    /// inferred — see [`classify_placement`].
    SandboxedCodebase,
}

/// Everything a start request says about *where its codebase goes*, gathered into one value.
///
/// A struct rather than a widening parameter list because the three placements are decided
/// together: the refusals that make them mutually exclusive each read two or three of these
/// fields, and a positional signature long enough to carry them all is one a caller can transpose
/// silently.
#[derive(Debug, Clone)]
pub struct PlacementRequest {
    pub local_instance_id: String,
    pub requested_codebase_id: String,
    pub eligible_ids: Vec<String>,
    pub managed_codebase: bool,
    pub sandbox: bool,
    pub sandboxed_codebase: bool,
    pub session_type: String,
    pub recipe: String,
    /// Whether the request asks the agent to skip its permission prompts.
    ///
    /// Part of *where the codebase goes* because on the placements that confine through a
    /// withdrawn tool surface it is one of the two confinement layers that is at stake. It no
    /// longer refuses anything: the jail around the checkout is untouched by the flag, and what
    /// the operator gives up is the guarantee that the agent reaches the code *only* through
    /// `mcp__tddy-tools__*`. [`classify_placement`] records the combination rather than serving
    /// it silently — see
    /// `docs/ft/daemon/amendments/PRD-2026-09-20-sandboxed-codebase-managed-workflow.md`.
    pub dangerously_skip_permissions: bool,
}

/// Classify a start request across all three placements, refusing any request that asks for more
/// than one.
///
/// The split/co-located half is [`classify_codebase_placement`] unchanged — this adds the third
/// placement in front of it rather than folding a new rule into it, because its self-match rule
/// (*"an empty or self-matching id is co-located"*) is what every session created before this
/// feature depends on. A request that does not set `sandboxed_codebase` reaches that function with
/// exactly the arguments it has always been given.
///
/// Every refusal names **both** placements it found, so the caller learns which flag to drop
/// rather than that the request was bad.
pub fn classify_placement(request: &PlacementRequest) -> Result<CodebasePlacement, String> {
    if !request.sandboxed_codebase {
        return classify_codebase_placement(
            &request.local_instance_id,
            &request.requested_codebase_id,
            &request.eligible_ids,
            request.managed_codebase,
            &request.session_type,
        );
    }

    // Checked before the flags below because a request naming a codebase host has asked for the
    // *same* inversion twice — once here, once across two hosts — and that is the more useful
    // thing to say about it than which flag it also set.
    let requested = request.requested_codebase_id.trim();
    if !requested.is_empty() {
        return Err(format!(
            "sandboxed_codebase is mutually exclusive with codebase_daemon_instance_id {requested:?}: both jail the codebase and leave the agent unconfined, and codebase_daemon_instance_id is the cross-host form of that same placement — drop one"
        ));
    }
    if request.sandbox {
        return Err(
            "sandboxed_codebase is mutually exclusive with sandbox: sandboxed_codebase jails the codebase, sandbox jails the agent — opposite placements, and a session has one"
                .to_string(),
        );
    }
    let session_type = request.session_type.trim();
    if session_type != "claude-cli" {
        return Err(format!(
            "sandboxed_codebase is only supported for session_type \"claude-cli\", not {session_type:?}: the placement's confinement is the withdrawal of the agent's native filesystem and shell tools, and no other agent's tool surface can be withdrawn"
        ));
    }
    let recipe = request.recipe.trim();
    if !recipe.is_empty() {
        return Err(format!(
            "sandboxed_codebase cannot carry recipe {recipe:?}: a workflow recipe resolves TDDY_REPO_DIR where the agent runs, and on this placement the code is not there"
        ));
    }
    log::info!(
        "classify_placement: codebase jailed on this daemon, agent beside it{}",
        match request.dangerously_skip_permissions {
            // Confined by the kernel and unconfined by policy: the jail still holds the checkout,
            // but the withdrawn-tool deny list that forces every access through
            // `mcp__tddy-tools__*` may not survive the bypass. The operator asked for it, so it is
            // served — and written down, because nothing else would say it happened.
            true =>
                ", with permission prompts skipped: the jail holds, the withdrawn-tool deny list may not",
            false => "",
        }
    );
    Ok(CodebasePlacement::SandboxedCodebase)
}

/// Classify a start request's codebase placement, refusing a split that cannot be honoured.
///
/// Mirrors [`crate::livekit_peer_discovery::classify_peer_route`]: a pure decision with every
/// precondition named in its error, so an operator learns *which* one failed rather than that the
/// request was bad. An empty or self-matching id is co-located — the pre-existing behaviour, which
/// this must never change.
///
/// A split needs `managed_codebase` (an agent that kept its native filesystem tools has nothing to
/// proxy through) and `session_type == "claude-cli"` (only Claude's `--allowedTools` /
/// `--disallowedTools` make the restriction enforceable rather than advisory — see the PRD
/// § Why claude-cli only), and the named daemon must be in the current eligible list.
pub fn classify_codebase_placement(
    local_instance_id: &str,
    requested_codebase_id: &str,
    eligible_ids: &[String],
    managed_codebase: bool,
    session_type: &str,
) -> Result<CodebasePlacement, String> {
    let requested = requested_codebase_id.trim();
    if requested.is_empty() || requested == local_instance_id.trim() {
        return Ok(CodebasePlacement::CoLocated);
    }
    if !managed_codebase {
        return Err(format!(
            "codebase_daemon_instance_id {requested:?} requires managed_codebase = true: an agent holding native filesystem tools has no reason to reach a worktree on another daemon"
        ));
    }
    let session_type = session_type.trim();
    if session_type != "claude-cli" {
        return Err(format!(
            "codebase_daemon_instance_id {requested:?} is only supported for session_type \"claude-cli\", not {session_type:?}: no other agent can be prevented from using its native filesystem tools"
        ));
    }
    if !eligible_ids.iter().any(|id| id.trim() == requested) {
        return Err(format!(
            "unknown or not connected codebase_daemon_instance_id {requested:?}: peer is not in the current eligible daemon list (configure livekit.common_room and ensure the peer is in the same LiveKit room)"
        ));
    }
    log::info!("classify_codebase_placement: codebase placed on peer instance_id={requested}");
    Ok(CodebasePlacement::Split {
        codebase_instance_id: requested.to_string(),
    })
}
