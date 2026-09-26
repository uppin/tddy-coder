//! The path survey: every path a moved file names, and the crate that defines what each reaches.
//!
//! A cross-crate move used to decide what the moved file depends on, and how its paths must be
//! rewritten, from its header `use` lines read as strings. Four defects followed from that — a
//! facade followed back into the destination, the destination named by its own extern name, a
//! crate named only in a body carried nowhere, a `super::` import read as an edge back — and all of
//! them are the same gap: the move never looked at every path, and never resolved the ones it did.
//!
//! The survey reads `use` items at any depth **and** paths in bodies, resolves `self::` and
//! `super::` against the file's own module path before anything else, and follows each through the
//! origin's re-exports (glob and chained) to the crate that **defines** the item. The header
//! rewrite, the edge test and the manifest pass all read it, so they cannot disagree about what the
//! file names. `check-parity` reads it too, for the findings `check --deep` owes.

use super::destination::Destination;
use super::Result;
use crate::edit::Position;
use crate::registry::Workspace;

/// One path the moved file writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SurveyedPath {
    /// The path as written: `super::helper`, `crate::config::DaemonConfig`, `shared::Clock`.
    pub(crate) written: String,
    /// The path with `self`/`super`/`crate` resolved against the moved file's module path,
    /// crate-rooted by extern name: `origin::outer::helper`.
    pub(crate) resolved: String,
    /// The extern name of the crate that defines the item, after following re-exports.
    pub(crate) defining_crate: String,
    /// Whether the path is written under a `#[cfg(test)]` item — what sends a crate to
    /// `[dev-dependencies]`.
    pub(crate) in_test: bool,
    /// Whether the path is in a body rather than a `use` item.
    pub(crate) in_body: bool,
    /// Where it is written, one-based.
    pub(crate) site: Position,
}

/// Every path a moved file names, in the order it names them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PathSurvey {
    pub(crate) paths: Vec<SurveyedPath>,
}

/// Survey `text`, the moved file, which sits at `module_path` (`["outer", "inner"]`) inside
/// `origin`.
// TODO(move-paths): the header pass, the edge test and the manifest pass read this once
// implemented; `check-parity` reads it for its findings.
#[allow(dead_code)]
pub(crate) fn survey_moved_file(
    workspace: &Workspace<'_>,
    text: &str,
    origin: &Destination,
    module_path: &[String],
) -> Result<PathSurvey> {
    // TODO(move-paths): implement
    let _ = (workspace, text, origin, module_path);
    todo!("move-paths: survey every path the moved file names")
}

/// `self::`, `super::` (any depth) and `crate::` resolved against `module_path` in `crate_name`,
/// by segment — never by string prefix. A path that climbs above the crate root is refused.
#[allow(dead_code)] // TODO(move-paths): called by `survey_moved_file`.
pub(crate) fn resolved_against(
    written: &str,
    crate_name: &str,
    module_path: &[String],
) -> Result<String> {
    // TODO(move-paths): implement
    let _ = (written, crate_name, module_path);
    todo!("move-paths: resolve a relative path against a module path")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_module_path(segments: &[&str]) -> Vec<String> {
        segments.iter().map(|segment| segment.to_string()).collect()
    }

    #[test]
    fn super_resolves_to_the_parent_module() {
        assert_eq!(
            resolved_against(
                "super::helper",
                "origin",
                &a_module_path(&["outer", "inner"])
            )
            .ok(),
            Some("origin::outer::helper".to_string())
        );
    }

    #[test]
    fn super_super_climbs_two_modules() {
        assert_eq!(
            resolved_against(
                "super::super::helper",
                "origin",
                &a_module_path(&["a", "b", "c"])
            )
            .ok(),
            Some("origin::a::helper".to_string())
        );
    }

    #[test]
    fn self_resolves_to_the_module_itself() {
        assert_eq!(
            resolved_against("self::local", "origin", &a_module_path(&["outer", "inner"])).ok(),
            Some("origin::outer::inner::local".to_string())
        );
    }

    #[test]
    fn crate_resolves_to_the_crate_root() {
        assert_eq!(
            resolved_against(
                "crate::config::DaemonConfig",
                "origin",
                &a_module_path(&["outer"])
            )
            .ok(),
            Some("origin::config::DaemonConfig".to_string())
        );
    }

    #[test]
    fn a_path_that_climbs_above_the_crate_root_is_refused() {
        assert!(resolved_against("super::super::x", "origin", &a_module_path(&["outer"])).is_err());
    }

    #[test]
    fn an_extern_path_is_left_as_written() {
        assert_eq!(
            resolved_against("shared::Clock", "origin", &a_module_path(&["outer"])).ok(),
            Some("shared::Clock".to_string())
        );
    }
}
