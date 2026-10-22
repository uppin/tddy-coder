//! Muse Code CLI sandbox recipe — policy for `muse` running in the runner's PTY.
//!
//! Muse Code is a native binary, so its read grants are the generic binary grants the plan already
//! adds (see [`crate::plan`]); only its policy is harness-specific.

use tddy_sandbox::builder::PolicySpec;

/// Policy for Muse Code's TUI: the interactive TUI policy without run-time code generation, which a
/// native binary does not need. Everything else (a PTY, forks, Mach lookups, sysctl reads) stays as
/// the interactive TUI policy grants it.
// TODO(muse-cli-facts): the no-JIT assumption is unverified against a real `muse` binary — see
// docs/dev/todo/2026-10-22-muse-code-cli-facts-unconfirmed.md.
pub fn muse_interactive_policy() -> PolicySpec {
    PolicySpec {
        allow_dynamic_code_generation: false,
        ..crate::claude_cli::claude_interactive_policy()
    }
}
