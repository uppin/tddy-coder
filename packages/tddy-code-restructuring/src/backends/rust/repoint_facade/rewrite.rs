//! Which surveyed paths are rewritten, and the preconditions a rewrite needs.
//!
//! A path is rewritten iff its `defined_at` differs from its `resolved` form and the crate that
//! defines it is not the file's own.
//!
//! TODO(repoint-facade): implement; unused until `resolve` calls it.

#![allow(dead_code)]

use super::refusals::unfinished;
use crate::crate_move::survey::PathSurvey;
use crate::Result;

/// One path to rewrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Rewrite {
    /// The path as written, `crate::config::Settings`.
    pub(super) written: String,
    /// The path where the item is defined, `kernel::config::Settings`.
    pub(super) defined_at: String,
    /// The one-based line it is written on.
    pub(super) line: u32,
    /// Whether it was a member of a grouped `use` that left the group (Rule S).
    pub(super) split_from_group: bool,
}

/// The rewrites `survey` of `text` calls for, or the refusal naming the path that cannot be
/// rewritten. `manifest` is the owning package's `Cargo.toml`, which must declare each defining
/// crate.
///
/// TODO(repoint-facade): implement.
pub(super) fn path_edits(
    _text: &str,
    _survey: &PathSurvey,
    _manifest: &str,
) -> Result<Vec<Rewrite>> {
    Err(unfinished())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crate_move::survey::SurveyedPath;
    use crate::edit::Position;

    const A_MANIFEST_OF_APP: &str =
        "[package]\nname = \"app\"\n\n[dependencies]\nkernel = { path = \"../kernel\" }\n";

    fn a_path(written: &str, resolved: &str, defined_at: &str) -> SurveyedPath {
        SurveyedPath {
            written: written.to_string(),
            resolved: resolved.to_string(),
            defining_crate: defined_at
                .split("::")
                .next()
                .unwrap_or_default()
                .to_string(),
            defined_at: defined_at.to_string(),
            in_test: false,
            in_body: false,
            site: Position { line: 1, col: 5 },
        }
    }

    /// The condition of the operation: the defining path differs from the resolved one, and the
    /// defining crate is not the file's own.
    #[test]
    fn a_path_forwarded_to_another_crate_is_rewritten_and_the_crates_own_paths_are_not() {
        // Given a path through a facade of `kernel`, an own path, and an in-crate facade
        let survey = PathSurvey {
            paths: vec![
                a_path("crate::config::A", "app::config::A", "kernel::config::A"),
                a_path("crate::b::Thing", "app::b::Thing", "app::b::Thing"),
                a_path("crate::Thing", "app::Thing", "app::inner::Thing"),
            ],
        };

        // When the rewrites are decided
        let rewrites = path_edits("use crate::config::A;\n", &survey, A_MANIFEST_OF_APP);

        // Then only the first is rewritten, with the line it is written on
        assert_eq!(
            rewrites.expect("the paths are decided"),
            [Rewrite {
                written: "crate::config::A".to_string(),
                defined_at: "kernel::config::A".to_string(),
                line: 1,
                split_from_group: false,
            }]
        );
    }

    /// A defining crate the package does not declare would need a manifest edit nobody asked for.
    #[test]
    fn a_defining_crate_the_manifest_does_not_declare_is_refused_naming_the_path() {
        // Given a path through a facade of a crate the manifest does not declare
        let survey = PathSurvey {
            paths: vec![a_path(
                "crate::roster::R",
                "app::roster::R",
                "agents::roster::R",
            )],
        };

        // When the rewrites are decided
        let refusal = path_edits("use crate::roster::R;\n", &survey, A_MANIFEST_OF_APP);

        // Then the refusal names the path and the crate
        let message = refusal
            .expect_err("an undeclared crate is refused")
            .to_string();
        assert!(
            message.contains("crate::roster::R") && message.contains("agents"),
            "{message}"
        );
    }

    /// Nothing to rewrite is a success, so a second run over the output is not an error.
    #[test]
    fn a_survey_of_own_paths_gives_no_rewrites() {
        // Given a survey of paths the crate defines itself
        let survey = PathSurvey {
            paths: vec![a_path("crate::b::Thing", "app::b::Thing", "app::b::Thing")],
        };

        // When the rewrites are decided
        let rewrites = path_edits("use crate::b::Thing;\n", &survey, A_MANIFEST_OF_APP);

        // Then there are none
        assert_eq!(rewrites.expect("the paths are decided"), []);
    }
}
