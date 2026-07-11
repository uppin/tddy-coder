//! Codebase-mode resolution shared by both platform paths (macOS in-process spawn and the Linux
//! daemon-assisted flow) — it maps the `--codebase-mode` / deprecated `--remote-codebase` flags to
//! a single managed/mounted boolean, independent of how the sandbox is ultimately launched.

/// Resolves the effective codebase mode from `--codebase-mode` and the deprecated
/// `--remote-codebase` boolean alias. Returns `true` for managed mode, `false` for mounted mode.
///
/// `--remote-codebase` predates `--codebase-mode` and remains a working alias for
/// `--codebase-mode managed`; an explicit `--codebase-mode mounted` alongside it is a
/// contradiction (the caller asked for both at once) and is rejected rather than silently
/// resolved to either value.
pub(crate) fn resolve_codebase_mode(
    codebase_mode: Option<&str>,
    remote_codebase_flag: bool,
) -> Result<bool, String> {
    match codebase_mode {
        Some("managed") => Ok(true),
        Some("mounted") => {
            if remote_codebase_flag {
                Err(
                    "conflicting codebase mode: --codebase-mode mounted was given together with \
                     --remote-codebase (which implies managed mode)"
                        .to_string(),
                )
            } else {
                Ok(false)
            }
        }
        Some(other) => Err(format!(
            "unrecognized --codebase-mode value {other:?}; expected \"mounted\" or \"managed\""
        )),
        None => Ok(remote_codebase_flag),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Managed-codebase mode + discovery subagent wiring ─────────────────────────
    //
    // Feature: docs/ft/coder/managed-codebase-subagents.md (criteria 11-12)
    // Changeset: docs/dev/1-WIP/2026-07-01-changeset-managed-codebase-subagents.md

    /// `--codebase-mode managed` resolves to managed mode (`true`), independent of the deprecated
    /// `--remote-codebase` boolean flag.
    #[test]
    fn resolve_codebase_mode_returns_true_for_explicit_managed_mode() {
        // Given / When
        let managed = resolve_codebase_mode(Some("managed"), false)
            .expect("'managed' must be a valid codebase mode");

        // Then
        assert!(
            managed,
            "--codebase-mode managed must resolve to managed mode"
        );
    }

    /// `--codebase-mode mounted` resolves to unmanaged mode (`false`).
    #[test]
    fn resolve_codebase_mode_returns_false_for_explicit_mounted_mode() {
        // Given / When
        let managed = resolve_codebase_mode(Some("mounted"), false)
            .expect("'mounted' must be a valid codebase mode");

        // Then
        assert!(
            !managed,
            "--codebase-mode mounted must resolve to unmanaged mode"
        );
    }

    /// With no `--codebase-mode` given, the deprecated `--remote-codebase` boolean flag remains a
    /// working alias for managed mode.
    #[test]
    fn resolve_codebase_mode_treats_remote_codebase_flag_as_a_managed_alias() {
        // Given / When
        let managed = resolve_codebase_mode(None, true)
            .expect("the --remote-codebase alias must resolve without error");

        // Then
        assert!(
            managed,
            "--remote-codebase must remain equivalent to --codebase-mode managed"
        );
    }

    /// With neither flag given, the default is unmanaged (mounted) mode — today's non-remote
    /// default behavior is preserved.
    #[test]
    fn resolve_codebase_mode_defaults_to_unmanaged_when_neither_flag_is_given() {
        // Given / When
        let managed =
            resolve_codebase_mode(None, false).expect("the default must resolve without error");

        // Then
        assert!(
            !managed,
            "default codebase mode must be unmanaged (mounted)"
        );
    }

    /// An explicit `--codebase-mode mounted` together with the deprecated `--remote-codebase` flag
    /// is a contradictory combination — it must be rejected, not silently resolved to either value.
    #[test]
    fn resolve_codebase_mode_errors_when_flags_conflict() {
        // Given / When
        let result = resolve_codebase_mode(Some("mounted"), true);

        // Then
        assert!(
            result.is_err(),
            "conflicting --codebase-mode mounted + --remote-codebase must be rejected"
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
}
