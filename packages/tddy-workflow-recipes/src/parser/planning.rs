use tddy_core::ParseError;

/// Parsed planning output. PRD must include a `## TODO` section (implementation milestones).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanningOutput {
    pub prd: String,
    /// PRD/feature name from plan agent (e.g. "Auth Feature").
    pub name: Option<String>,
    /// Discovery data (toolchain, scripts, doc locations) from plan goal.
    pub discovery: Option<tddy_core::changeset::DiscoveryData>,
    /// Demo plan for user verification.
    pub demo_plan: Option<DemoPlan>,
    /// Daemon mode: suggested git branch name for the feature.
    pub branch_suggestion: Option<String>,
    /// Daemon mode: suggested worktree directory name (e.g. "feature-auth").
    pub worktree_suggestion: Option<String>,
    /// Code-discovery knowledge (Code Map, diagrams, docs) to persist as `artifacts/exploration.md`.
    pub exploration: Option<String>,
}

/// Demo execution mode: how the running app is presented to the user.
///
/// `PortForward` — HTTP port inside the guest is forwarded to a host port; share link is
/// `http://localhost:<host_port>`. `ScreenShare` — VNC framebuffer → LiveKit H264 track;
/// share link is a LiveKit viewer URL. Mode is decided by the `plan` step.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DemoMode {
    PortForward,
    ScreenShare,
}

/// A single host ↔ guest port mapping for QEMU slirp `hostfwd`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PortMap {
    pub host_port: u16,
    pub guest_port: u16,
}

/// Demo plan for presenting the feature to the user.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DemoPlan {
    pub demo_type: String,
    pub setup_instructions: String,
    pub steps: Vec<DemoStep>,
    pub verification: String,
    /// Execution mode for the demo (port-forward or screen-share). Decided during plan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<DemoMode>,
    /// Host ↔ guest port mappings for QEMU slirp hostfwd (beyond SSH).
    #[serde(default)]
    pub hostfwd: Vec<PortMap>,
    /// Shell commands to run inside the guest (via SSH) after boot to deploy the app.
    #[serde(default)]
    pub deploy_steps: Vec<String>,
    /// Command to run inside the guest to assert the app is healthy after deploy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_command: Option<String>,
    /// The build target id (from BUILD.yaml) that produces the guest qcow2 image.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_target: Option<String>,
}

/// A single demo step.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DemoStep {
    pub description: String,
    pub command_or_action: String,
    pub expected_result: String,
}

#[derive(serde::Deserialize)]
struct StructuredPlan {
    pub(crate) goal: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) prd: Option<String>,
    pub(crate) discovery: Option<tddy_core::changeset::DiscoveryData>,
    pub(crate) demo_plan: Option<DemoPlan>,
    pub(crate) branch_suggestion: Option<String>,
    pub(crate) worktree_suggestion: Option<String>,
    pub(crate) exploration: Option<String>,
}

/// Parse LLM planning response. JSON must come from tddy-tools submit (no inline parsing).
pub fn parse_planning_response(s: &str) -> Result<PlanningOutput, ParseError> {
    parse_planning_response_impl(s, None)
}

/// Like parse_planning_response but resolves `prd` when it is a path to an MD file (relative to base_path).
pub fn parse_planning_response_with_base(
    s: &str,
    _base_path: &std::path::Path,
) -> Result<PlanningOutput, ParseError> {
    parse_planning_response_impl(s, Some(_base_path))
}

/// Heuristic: `prd` is a relative markdown file reference, not inline PRD body (which has newlines).
fn prd_value_looks_like_md_file_path(prd: &str) -> bool {
    const MAX_PRD_FILE_PATH_REF_LEN: usize = 260;
    let t = prd.trim();
    t.len() <= MAX_PRD_FILE_PATH_REF_LEN
        && !t.contains('\n')
        && !t.contains('\r')
        && t.ends_with(".md")
}

fn parse_planning_response_impl(
    s: &str,
    _base_path: Option<&std::path::Path>,
) -> Result<PlanningOutput, ParseError> {
    let s = s.trim();
    let parsed: StructuredPlan = serde_json::from_str(s)
        .map_err(|e| ParseError::Malformed(format!("invalid JSON: {}", e)))?;
    if parsed.goal.as_deref() != Some("plan") {
        return Err(ParseError::Malformed("goal is not plan".into()));
    }
    let mut prd = parsed
        .prd
        .filter(|x| !x.trim().is_empty())
        .ok_or_else(|| ParseError::Malformed("prd missing or empty".into()))?;
    if let Some(base) = _base_path {
        let path = base.join(prd.trim());
        if path.exists() && path.is_file() {
            prd = std::fs::read_to_string(&path).map_err(|e| {
                ParseError::Malformed(format!("failed to read prd file {}: {}", path.display(), e))
            })?;
        } else if prd_value_looks_like_md_file_path(&prd) {
            return Err(ParseError::Malformed(format!(
                "prd references markdown file {:?} but no such file was found under {}",
                prd.trim(),
                base.display()
            )));
        }
    }
    Ok(PlanningOutput {
        prd,
        name: parsed.name.filter(|s| !s.is_empty()),
        discovery: parsed.discovery,
        demo_plan: parsed.demo_plan,
        branch_suggestion: parsed.branch_suggestion.filter(|s| !s.is_empty()),
        worktree_suggestion: parsed.worktree_suggestion.filter(|s| !s.is_empty()),
        exploration: parsed.exploration.filter(|s| !s.trim().is_empty()),
    })
}
