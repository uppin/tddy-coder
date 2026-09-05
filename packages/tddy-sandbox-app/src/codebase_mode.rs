//! Codebase-mode resolution shared by both platform paths (macOS in-process spawn and the Linux
//! daemon-assisted flow) — it maps the `--codebase-mode` / deprecated `--remote-codebase` flags to
//! a single [`CodebaseMode`], independent of how the sandbox is ultimately launched.
//!
//! It also holds the refusals that are about the *mode* rather than about one platform's flow:
//! which mode a path can serve at all ([`managed_codebase_for_daemon_path`]) and which flags mean
//! nothing outside the mode they were added for ([`refuse_unservable_codebase_home_dir`]).

use std::path::Path;

/// Where the checkout lives relative to the jail, and — as of `sandboxed` — which side of the jail
/// the agent is on.
///
/// The first two modes are variations on one placement: the agent is inside the jail, and the only
/// question is whether the repo came with it. `Sandboxed` inverts that (see
/// `docs/ft/coder/sandboxed-codebase-mode.md`), which is why this is an enum rather than the
/// managed/mounted boolean it replaces — the third value is not a shade of the other two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodebaseMode {
    /// Repo mounted read-write into the jail; the agent runs there and works on it directly.
    Mounted,
    /// Repo never mounted; the agent runs in the jail and reaches the host's repo only via
    /// `mcp__tddy-tools__*` calls the host relays.
    Managed,
    /// Repo mounted read-write into the jail and the agent runs on the **host**, reaching the
    /// checkout only via `mcp__tddy-tools__*` calls dispatched *into* the jail.
    Sandboxed,
}

/// Resolves the effective codebase mode from `--codebase-mode` and the deprecated
/// `--remote-codebase` boolean alias.
///
/// `--remote-codebase` predates `--codebase-mode` and remains a working alias for
/// `--codebase-mode managed`; any other explicit mode alongside it is a contradiction (the caller
/// asked for two placements at once) and is rejected rather than silently resolved to either value.
pub fn resolve_codebase_mode(
    codebase_mode: Option<&str>,
    remote_codebase_flag: bool,
) -> Result<CodebaseMode, String> {
    match codebase_mode {
        Some("managed") => Ok(CodebaseMode::Managed),
        Some("mounted") if remote_codebase_flag => Err(conflicting_with_the_alias("mounted")),
        Some("mounted") => Ok(CodebaseMode::Mounted),
        Some("sandboxed") if remote_codebase_flag => Err(conflicting_with_the_alias("sandboxed")),
        Some("sandboxed") => Ok(CodebaseMode::Sandboxed),
        Some(other) => Err(format!(
            "unrecognized --codebase-mode value {other:?}; expected \"mounted\", \"managed\" or \
             \"sandboxed\""
        )),
        None if remote_codebase_flag => Ok(CodebaseMode::Managed),
        None => Ok(CodebaseMode::Mounted),
    }
}

/// The refusal for a mode named alongside `--remote-codebase`. Naming the alias's meaning is the
/// point: an operator who wrote both flags read them as independent, and the message is where they
/// learn the second one already picked a placement.
fn conflicting_with_the_alias(mode: &str) -> String {
    format!(
        "conflicting codebase mode: --codebase-mode {mode} was given together with \
         --remote-codebase (which implies managed mode)"
    )
}

/// How a mode spells itself as a `--codebase-mode` value, so a refusal can name the session the
/// caller actually asked for rather than a debug rendering of an internal enum.
fn flag_value(mode: CodebaseMode) -> &'static str {
    match mode {
        CodebaseMode::Mounted => "mounted",
        CodebaseMode::Managed => "managed",
        CodebaseMode::Sandboxed => "sandboxed",
    }
}

/// Refuse `--codebase-home-dir` (or the config's `codebase_home_dir:`) outside `sandboxed` mode,
/// where nothing reads it.
///
/// The flag names the `$HOME` of the **build**, and only `sandboxed` runs a build inside the jail:
/// the other two placements put the *agent* there and give it `--claude-home-dir` /
/// `--cursor-home-dir` instead. Carrying an unread path into the session is how an operator who
/// pointed their dependency caches at a roomy volume finds out months later that `~/.tddy` filled
/// up anyway — the same failure `refuse_unservable_cwd` exists to prevent in the other direction,
/// where the flag is read by every mode but this one.
///
/// Shared by both platform paths deliberately. On Linux the mode is refused outright
/// ([`managed_codebase_for_daemon_path`]), which leaves *every* Linux session one that cannot read
/// this flag — so a silently dropped `--codebase-home-dir` is not a macOS-only hazard.
pub fn refuse_unservable_codebase_home_dir(
    mode: CodebaseMode,
    codebase_home_dir: Option<&Path>,
) -> Result<(), String> {
    match (mode, codebase_home_dir) {
        (CodebaseMode::Sandboxed, _) | (_, None) => Ok(()),
        (mode, Some(home)) => Err(format!(
            "--codebase-home-dir ({}) applies only to --codebase-mode sandboxed, and this session \
             is {}: the path names the $HOME of the build that runs *inside* the jail, and a mode \
             that runs the agent in the jail instead has no such build to give a home to. Drop \
             --codebase-home-dir / `codebase_home_dir:`, or use --codebase-mode sandboxed.",
            home.display(),
            flag_value(mode)
        )),
    }
}

/// The `managed_codebase` boolean the Linux daemon-assisted path forwards on
/// `StartSessionRequest`, or a refusal for a mode that path cannot serve.
///
/// `Sandboxed` needs a `--workspace-tools` jail the app provisions itself, which on Linux it cannot
/// do (cgroup v2 delegation containment — see `packages/tddy-sandbox/docs/architecture.md`). The
/// daemon would have to provision it, and does not yet know how to. Refusing here names macOS
/// rather than letting the flag reach a daemon that would quietly start an ordinary session.
pub fn managed_codebase_for_daemon_path(mode: CodebaseMode) -> Result<bool, String> {
    match mode {
        CodebaseMode::Mounted => Ok(false),
        CodebaseMode::Managed => Ok(true),
        CodebaseMode::Sandboxed => Err(
            "--codebase-mode sandboxed is supported only on macOS: it needs a --workspace-tools \
             jail this app provisions itself, which the Linux daemon-assisted path cannot yet do"
                .to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Codebase-mode resolution ──────────────────────────────────────────────────
    //
    // Feature: docs/ft/coder/sandboxed-codebase-mode.md (criteria 1, 10),
    //          docs/ft/coder/managed-codebase-subagents.md (criteria 11-12)
    // Changeset: docs/dev/1-WIP/2026-09-05-sandboxed-codebase-mode.md

    /// `--codebase-mode managed` resolves to managed mode, independent of the deprecated
    /// `--remote-codebase` boolean flag.
    #[test]
    fn resolve_codebase_mode_returns_managed_for_explicit_managed_mode() {
        // Given / When
        let mode = resolve_codebase_mode(Some("managed"), false)
            .expect("'managed' must be a valid codebase mode");

        // Then
        assert_eq!(mode, CodebaseMode::Managed);
    }

    /// `--codebase-mode mounted` resolves to mounted mode.
    #[test]
    fn resolve_codebase_mode_returns_mounted_for_explicit_mounted_mode() {
        // Given / When
        let mode = resolve_codebase_mode(Some("mounted"), false)
            .expect("'mounted' must be a valid codebase mode");

        // Then
        assert_eq!(mode, CodebaseMode::Mounted);
    }

    /// `--codebase-mode sandboxed` resolves to the inverted placement: the code in the jail, the
    /// agent on the host.
    #[test]
    fn resolve_codebase_mode_returns_sandboxed_for_explicit_sandboxed_mode() {
        // Given / When
        let mode = resolve_codebase_mode(Some("sandboxed"), false)
            .expect("'sandboxed' must be a valid codebase mode");

        // Then
        assert_eq!(mode, CodebaseMode::Sandboxed);
    }

    /// With no `--codebase-mode` given, the deprecated `--remote-codebase` boolean flag remains a
    /// working alias for managed mode.
    #[test]
    fn resolve_codebase_mode_treats_remote_codebase_flag_as_a_managed_alias() {
        // Given / When
        let mode = resolve_codebase_mode(None, true)
            .expect("the --remote-codebase alias must resolve without error");

        // Then
        assert_eq!(mode, CodebaseMode::Managed);
    }

    /// With neither flag given, the default is mounted — today's non-remote default behavior is
    /// preserved.
    #[test]
    fn resolve_codebase_mode_defaults_to_mounted_when_neither_flag_is_given() {
        // Given / When
        let mode =
            resolve_codebase_mode(None, false).expect("the default must resolve without error");

        // Then
        assert_eq!(mode, CodebaseMode::Mounted);
    }

    /// An explicit `--codebase-mode mounted` together with the deprecated `--remote-codebase` flag
    /// is a contradictory combination — it must be rejected, not silently resolved to either value.
    #[test]
    fn resolve_codebase_mode_errors_when_mounted_conflicts_with_the_remote_codebase_alias() {
        // Given / When
        let result = resolve_codebase_mode(Some("mounted"), true);

        // Then
        assert!(
            result.is_err(),
            "conflicting --codebase-mode mounted + --remote-codebase must be rejected"
        );
    }

    /// The same contradiction, for the new mode: `--remote-codebase` means managed, and managed is
    /// the opposite placement from sandboxed. Asking for both is asking for two jails.
    #[test]
    fn resolve_codebase_mode_errors_when_sandboxed_conflicts_with_the_remote_codebase_alias() {
        // Given / When
        let result = resolve_codebase_mode(Some("sandboxed"), true);

        // Then
        assert!(
            result.is_err(),
            "conflicting --codebase-mode sandboxed + --remote-codebase must be rejected"
        );
    }

    /// An unrecognized `--codebase-mode` value is a typed error, not a silent fallback.
    #[test]
    fn resolve_codebase_mode_errors_on_an_unrecognized_value() {
        // Given / When
        let result = resolve_codebase_mode(Some("bogus"), false);

        // Then
        assert!(
            result.is_err(),
            "an unrecognized --codebase-mode value must be rejected"
        );
    }

    /// The refusal has to be actionable: an operator who mistyped a mode learns what the modes are
    /// from the message, not from the source.
    #[test]
    fn an_unrecognized_codebase_mode_names_every_accepted_value() {
        // Given / When
        let message = resolve_codebase_mode(Some("bogus"), false)
            .expect_err("an unrecognized value must be rejected");

        // Then
        for accepted in ["mounted", "managed", "sandboxed"] {
            assert!(
                message.contains(accepted),
                "the refusal must name '{accepted}'; message was: {message}"
            );
        }
    }

    // ─── Which mode reads the build's home ─────────────────────────────────────────

    /// `--codebase-home-dir` names the `$HOME` of the build inside the jail, and a mounted session
    /// runs no build there. Accepting the path and never reading it is how an operator who moved
    /// their dependency caches to a roomy volume discovers, when `~/.tddy` fills up, that they
    /// never moved.
    #[test]
    fn a_mounted_session_refuses_a_build_home_it_would_never_read() {
        // Given / When
        let message = refuse_unservable_codebase_home_dir(
            CodebaseMode::Mounted,
            Some(Path::new("/Volumes/build-cache/tddy-codebase-homes")),
        )
        .expect_err("a mode that reads no build home must refuse one it was handed");

        // Then
        assert!(
            message.contains("--codebase-home-dir")
                && message.contains("sandboxed")
                && message.contains("/Volumes/build-cache/tddy-codebase-homes"),
            "the refusal must name the flag, the mode that reads it and the path that was \
             dropped; message was: {message}"
        );
    }

    /// The same for managed mode — its jail holds the agent too, so the flag is just as unread
    /// there. (On Linux this is every session there is: `sandboxed` is refused outright, so a
    /// dropped `--codebase-home-dir` would be silent on the whole platform.)
    #[test]
    fn a_managed_session_refuses_a_build_home_it_would_never_read() {
        // Given / When
        let message = refuse_unservable_codebase_home_dir(
            CodebaseMode::Managed,
            Some(Path::new("/Volumes/build-cache/tddy-codebase-homes")),
        )
        .expect_err("a mode that reads no build home must refuse one it was handed");

        // Then
        assert!(
            message.contains("managed"),
            "the refusal must name the session the caller actually asked for; message was: \
             {message}"
        );
    }

    /// The one mode whose jail runs the build takes the home it was given.
    #[test]
    fn a_sandboxed_session_is_given_the_build_home_it_asked_for() {
        // Given / When
        let served = refuse_unservable_codebase_home_dir(
            CodebaseMode::Sandboxed,
            Some(Path::new("/Volumes/build-cache/tddy-codebase-homes")),
        );

        // Then
        assert_eq!(served, Ok(()));
    }

    /// The refusal is about the flag, not about the mode: a mounted session that named no build
    /// home runs exactly as it does today.
    #[test]
    fn a_mounted_session_that_named_no_build_home_is_served() {
        // Given / When
        let served = refuse_unservable_codebase_home_dir(CodebaseMode::Mounted, None);

        // Then
        assert_eq!(served, Ok(()));
    }

    /// …and so does a managed one.
    #[test]
    fn a_managed_session_that_named_no_build_home_is_served() {
        // Given / When
        let served = refuse_unservable_codebase_home_dir(CodebaseMode::Managed, None);

        // Then
        assert_eq!(served, Ok(()));
    }

    // ─── What the Linux daemon-assisted path can serve ─────────────────────────────

    /// Mounted mode forwards `managed_codebase = false`, as it does today.
    #[test]
    fn the_daemon_path_forwards_an_unmanaged_codebase_for_mounted_mode() {
        // Given / When
        let managed = managed_codebase_for_daemon_path(CodebaseMode::Mounted)
            .expect("mounted must be servable by the daemon path");

        // Then
        assert!(!managed);
    }

    /// Managed mode forwards `managed_codebase = true`, as it does today.
    #[test]
    fn the_daemon_path_forwards_a_managed_codebase_for_managed_mode() {
        // Given / When
        let managed = managed_codebase_for_daemon_path(CodebaseMode::Managed)
            .expect("managed must be servable by the daemon path");

        // Then
        assert!(managed);
    }

    /// The daemon path has no way to provision the no-agent jail this mode needs, so the flag is
    /// refused there rather than reaching a daemon that would start an ordinary session under a
    /// name that promises confinement.
    #[test]
    fn the_daemon_path_refuses_sandboxed_mode_and_names_macos() {
        // Given / When
        let message = managed_codebase_for_daemon_path(CodebaseMode::Sandboxed)
            .expect_err("the daemon path must refuse sandboxed mode");

        // Then
        assert!(
            message.contains("macOS"),
            "the refusal must name the host that can serve this mode; message was: {message}"
        );
        assert!(
            message.contains("sandboxed"),
            "the refusal must name the mode it refuses; message was: {message}"
        );
    }
}
