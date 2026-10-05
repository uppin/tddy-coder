//! `canonical_paths`: spelling a facade path in moved text as the path that defines what it names.
//!
//! `crate::config::Settings` where `crate::config` is `pub use kernel::config;` travels with a moved
//! signature as written, and the module that now holds it cannot later leave its crate: the facade
//! stays behind. With `canonical_paths` on the plan line, each such path reads `kernel::config::Settings`
//! instead, resolved by the same survey the cross-crate moves use ([`crate::crate_move::survey`]).
//!
//! Off by default: a plan written without the field keeps its result byte for byte.

use std::ops::Range;

use super::assemble::Moving;
use super::text::Edit;
use crate::crate_move::survey::PathSurvey;
use crate::{RestructureError, Result};

/// What the pass changes in the moved text, and what it says about every path it met.
// TODO(move-fidelity): constructed once `defining_paths` is implemented.
#[allow(dead_code)]
pub(super) struct Rewritten {
    /// Replacements, in the coordinates of the text the pass was given.
    pub(super) edits: Vec<Edit>,
    /// One line per path rewritten and one per path left as written, each naming the path as written
    /// and the path that defines it.
    pub(super) notes: Vec<String>,
}

/// Every `crate::`-headed path in `moved_text` whose defining path differs from the one written,
/// replaced by the defining path; `claimed` are the spans `rebase` already edits, which are left to it.
///
/// # Errors
///
/// Refuses while the pass is not implemented, naming the node that delivers it: an operation that
/// cannot do what the plan asked must not pretend to have done it.
pub(super) fn defining_paths(
    _moving: &Moving<'_>,
    _moved_text: &str,
    _claimed: &[Range<usize>],
) -> Result<Rewritten> {
    // TODO(move-fidelity): implement — survey the moved text with `crate_move::survey::survey_moved_file`
    // (read the origin with `Destination::read`), then hand the survey to [`rewrite`].
    Err(not_implemented_yet())
}

/// The rewrite itself, a function of the text and a survey of it: each `crate::`-headed path whose
/// `defined_at` differs from what is written, and whose span reads back as written, is replaced by
/// `defined_at` (spelled from `crate` when `own_crate` defines it); every path met, rewritten or left,
/// is named in the notes.
// TODO(move-fidelity): called by `defining_paths` once implemented.
#[allow(dead_code)]
pub(super) fn rewrite(
    _moved_text: &str,
    _survey: &PathSurvey,
    _claimed: &[Range<usize>],
    _own_crate: &str,
) -> Result<Rewritten> {
    // TODO(move-fidelity): implement.
    Err(not_implemented_yet())
}

fn not_implemented_yet() -> RestructureError {
    RestructureError::UnsupportedOp {
        backend: "rust".to_string(),
        op: "move_item with `canonical_paths` (not implemented yet: node `move-fidelity`)"
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::text::applied;
    use super::*;
    use crate::crate_move::survey::SurveyedPath;
    use crate::edit::Position;

    /// A path of the moved text, written at (`line`, `col`), that the survey says is defined at
    /// `defined_at`.
    fn a_path(written: &str, defined_at: &str, line: u32, col: u32) -> SurveyedPath {
        SurveyedPath {
            written: written.to_string(),
            resolved: format!("app::{}", written.trim_start_matches("crate::")),
            defining_crate: defined_at
                .split("::")
                .next()
                .unwrap_or_default()
                .to_string(),
            defined_at: defined_at.to_string(),
            in_test: false,
            in_body: true,
            site: Position { line, col },
        }
    }

    fn surveying(paths: Vec<SurveyedPath>) -> PathSurvey {
        PathSurvey { paths }
    }

    #[test]
    fn replaces_a_crate_headed_path_with_the_defining_path_the_survey_gives() {
        // Given a signature naming a facade path, and a survey that resolves it to its definition
        let text = "pub fn f(c: &crate::config::Settings) {}\n";
        let survey = surveying(vec![a_path(
            "crate::config::Settings",
            "kernel::config::Settings",
            1,
            15,
        )]);

        // When the paths are rewritten
        let rewritten = rewrite(text, &survey, &[], "app").unwrap();

        // Then the defining path stands where the facade path was, and the notes say so
        assert_eq!(
            applied(text, &rewritten.edits).unwrap(),
            "pub fn f(c: &kernel::config::Settings) {}\n"
        );
        assert_eq!(rewritten.notes.len(), 1);
        assert!(rewritten.notes[0].contains("crate::config::Settings"));
        assert!(rewritten.notes[0].contains("kernel::config::Settings"));
    }

    #[test]
    fn leaves_a_path_whose_written_text_does_not_read_back() {
        // Given a path written with spaces, so the span does not read back as `crate::config::Mode`
        let text = "pub fn f(m: &crate :: config :: Mode) {}\n";
        let survey = surveying(vec![a_path(
            "crate::config::Mode",
            "kernel::config::Mode",
            1,
            15,
        )]);

        // When the paths are rewritten
        let rewritten = rewrite(text, &survey, &[], "app").unwrap();

        // Then nothing is edited, and the notes name the path that was left
        assert!(rewritten.edits.is_empty());
        assert_eq!(rewritten.notes.len(), 1);
        assert!(rewritten.notes[0].contains("crate::config::Mode"));
    }

    #[test]
    fn leaves_a_use_item_in_a_group_and_names_it() {
        // Given a path that sits in a grouped `use`
        let text = "use crate::{config::Level, own::Marker};\n";
        let mut level = a_path("crate::config::Level", "kernel::config::Level", 1, 5);
        level.in_body = false;
        let survey = surveying(vec![level]);

        // When the paths are rewritten
        let rewritten = rewrite(text, &survey, &[], "app").unwrap();

        // Then the group is untouched, and the notes name the path left
        assert!(rewritten.edits.is_empty());
        assert_eq!(rewritten.notes.len(), 1);
        assert!(rewritten.notes[0].contains("crate::config::Level"));
    }

    #[test]
    fn spells_a_path_defined_in_the_same_crate_from_crate() {
        // Given a facade whose definition is in the moving crate itself
        let text = "pub fn f(t: &crate::shapes::Thing) {}\n";
        let survey = surveying(vec![a_path(
            "crate::shapes::Thing",
            "app::own::Thing",
            1,
            15,
        )]);

        // When the paths are rewritten
        let rewritten = rewrite(text, &survey, &[], "app").unwrap();

        // Then the defining path is written from `crate`, not by the crate's extern name
        assert_eq!(
            applied(text, &rewritten.edits).unwrap(),
            "pub fn f(t: &crate::own::Thing) {}\n"
        );
    }
}
