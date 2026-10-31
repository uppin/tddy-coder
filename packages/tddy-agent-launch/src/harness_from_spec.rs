//! What an `opencode-cli`, `muse-cli` or `warp-cli` launch binds when it renders its tool
//! definition (`tddy-coding-agent-specs/specs/{opencode,muse,warp}.yaml`) — on the host through
//! [`crate::spec_launch::launch_host_pty`], in a jail through
//! [`crate::launch_ports::LaunchSessions::launch_jailed`] — and the spec-driven refusal of a
//! placement the spec excludes.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-harness-clis-from-specs.md` (`#agent-tools 8/12`).
//!
//! Nothing here builds an argv: these are the template parameters the three specs name. A resume
//! is `resume` for the agents that continue their worktree's session (`--continue`), and
//! `resume_token` for Warp, whose token the `warp-resume-token` adapter read off the last run.

use tddy_actions::tool::TemplateParams;
use tddy_coding_agent_specs::{CodingAgentSpec, Placement};
use tddy_rpc::Status;

/// The `opencode` tool's id in the agent registry.
pub const OPENCODE_AGENT_ID: &str = "opencode";
/// The `muse` tool's id in the agent registry.
pub const MUSE_AGENT_ID: &str = "muse";
/// The `warp` tool's id in the agent registry.
pub const WARP_AGENT_ID: &str = "warp";

/// One start or resume of a CLI harness, as its spec's template is rendered for it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HarnessLaunch<'a> {
    /// The model the operator chose; empty when none was.
    pub model: &'a str,
    /// The first turn, when the session starts with one.
    pub prompt: Option<&'a str>,
    /// A resume of an existing session rather than its first start.
    pub resume: bool,
    /// The token the agent's last run printed (`harness_resume_token`), when one was recorded.
    pub resume_token: Option<&'a str>,
}

/// The parameters `opencode.yaml`, `muse.yaml` and `warp.yaml`'s `run.argv` is rendered with:
/// `model` and `prompt` each bound when non-empty (trimmed) on a start; `resume` set on a resume,
/// with `resume_token` bound when a non-blank one was recorded. A resume binds neither model nor
/// prompt — the agent continues the session it had.
pub fn harness_template_params(launch: &HarnessLaunch<'_>) -> TemplateParams {
    // TODO(harness-clis): implement.
    let _ = launch;
    todo!("harness-clis: harness_template_params")
}

/// `Ok` when `spec`'s placements include `placement`; otherwise `FAILED_PRECONDITION` naming the
/// session type and the placements the spec declares — the refusal of a sandboxed `warp-cli`
/// start, read from `warp.yaml`'s `placements: [host]` instead of `sandboxed_agent_kind`'s match.
pub fn require_spec_placement(
    spec: &CodingAgentSpec,
    session_type: &str,
    placement: Placement,
) -> Result<(), Status> {
    // TODO(harness-clis): implement — `spec.require_placement(placement)`, its error as the
    // message after "the sandbox runner does not host {session_type}: ".
    let _ = (spec, session_type, placement);
    todo!("harness-clis: require_spec_placement")
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tddy_actions::tool::RunSection;
    use tddy_coding_agent_specs::{CodingAgentSection, WARP_SPEC_YAML};
    use tddy_rpc::Code;

    use super::*;

    #[test]
    fn a_start_binds_the_model_and_prompt_it_was_given() {
        // When
        let params = harness_template_params(&HarnessLaunch {
            model: "anthropic/claude-sonnet-4-5",
            prompt: Some("  fix the build  "),
            ..HarnessLaunch::default()
        });

        // Then
        assert_eq!(
            params,
            TemplateParams::new()
                .with("model", "anthropic/claude-sonnet-4-5")
                .with("prompt", "fix the build")
        );
    }

    #[test]
    fn a_start_with_an_empty_model_and_a_blank_prompt_binds_nothing() {
        // When
        let params = harness_template_params(&HarnessLaunch {
            model: "",
            prompt: Some("   "),
            ..HarnessLaunch::default()
        });

        // Then
        assert_eq!(params, TemplateParams::new());
    }

    #[test]
    fn a_resume_sets_resume_and_binds_neither_model_nor_prompt() {
        // When
        let params = harness_template_params(&HarnessLaunch {
            model: "muse-spark-1.3",
            prompt: Some("go on"),
            resume: true,
            resume_token: None,
        });

        // Then
        assert_eq!(params, TemplateParams::new().with_flag("resume", true));
    }

    #[test]
    fn a_resume_with_a_recorded_token_binds_it_and_a_blank_one_is_left_unbound() {
        // When
        let recorded = harness_template_params(&HarnessLaunch {
            resume: true,
            resume_token: Some(" 01JB7Q2N5W "),
            ..HarnessLaunch::default()
        });
        let blank = harness_template_params(&HarnessLaunch {
            resume: true,
            resume_token: Some("  "),
            ..HarnessLaunch::default()
        });

        // Then
        assert_eq!(
            (recorded, blank),
            (
                TemplateParams::new()
                    .with_flag("resume", true)
                    .with("resume_token", "01JB7Q2N5W"),
                TemplateParams::new().with_flag("resume", true),
            )
        );
    }

    /// `warp.yaml`, read straight from its YAML.
    fn the_builtin_warp_spec() -> CodingAgentSpec {
        let document: serde_yaml::Value = serde_yaml::from_str(WARP_SPEC_YAML).unwrap();
        let run: RunSection = serde_yaml::from_value(document["run"].clone()).unwrap();
        let agent: CodingAgentSection =
            serde_yaml::from_value(document["extensions"]["coding_agent"].clone()).unwrap();
        CodingAgentSpec {
            id: WARP_AGENT_ID.to_string(),
            run,
            agent,
        }
    }

    #[test]
    fn a_jailed_warp_is_refused_failed_precondition_naming_its_placements() {
        // When
        let refused = require_spec_placement(&the_builtin_warp_spec(), "warp-cli", Placement::Jail)
            .map_err(|status| (status.code, status.message));

        // Then
        assert_eq!(
            refused,
            Err((
                Code::FailedPrecondition,
                "the sandbox runner does not host warp-cli: warp cannot run in the jail: its \
                 spec's placements are [host]"
                    .to_string()
            ))
        );
    }

    #[test]
    fn a_host_warp_is_admitted() {
        // When
        let admitted =
            require_spec_placement(&the_builtin_warp_spec(), "warp-cli", Placement::Host)
                .map_err(|status| status.message);

        // Then
        assert_eq!(admitted, Ok(()));
    }
}
