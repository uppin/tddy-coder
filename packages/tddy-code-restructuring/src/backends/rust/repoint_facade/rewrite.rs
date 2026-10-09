//! Which surveyed paths are rewritten, and the preconditions a rewrite needs.
//!
//! A path is rewritten iff its `defined_at` differs from its `resolved` form and the crate that
//! defines it is not the file's own. A path that passes that test is then held to four
//! preconditions — the defining crate is declared, the path is spelled on one line without a
//! comment inside it, a body path keeps its last segment, and the rewrite does not bind a name the
//! scope already binds — and refused, naming the path, the file and the line, when one fails.

use crate::apply::byte_offset;
use crate::backends::rust::item_move::text::enclosing_modules;
use crate::crate_move::header::written_prefix;
use crate::crate_move::manifest_edits::{self, Table};
use crate::crate_move::source_scan::sightings;
use crate::crate_move::survey::{PathSurvey, SurveyedPath};
use crate::Result;

use super::refusals;

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
/// crate; `file` names the file in every refusal, so a reader can find the line.
pub(super) fn path_edits(
    text: &str,
    survey: &PathSurvey,
    manifest: &str,
    file: &str,
) -> Result<Vec<Rewrite>> {
    let uses = use_bindings(text);
    let mut rewrites = Vec::new();

    for path in &survey.paths {
        if !goes_through_a_facade(path) {
            continue;
        }
        if !is_declared(manifest, &path.defining_crate, path.in_test) {
            return Err(refusals::undeclared_crate(
                file,
                path.site.line,
                &path.written,
                &path.defining_crate,
            ));
        }
        let at = byte_offset(text, path.site.line, path.site.col)?;
        if !spelled_on_one_line(text, at, &path.written) {
            return Err(refusals::spelled_across(
                file,
                path.site.line,
                &path.written,
            ));
        }
        if path.in_body && last_segment(&path.written) != last_segment(&path.defined_at) {
            return Err(refusals::renamed_in_a_body(file, &path.written));
        }
        if !path.in_body {
            let scope = enclosing_modules(text, at);
            let name = last_segment(&path.written);
            if let Some(other) = uses
                .iter()
                .find(|binding| binding.at != at && binding.scope == scope && binding.name == name)
            {
                return Err(refusals::binds_a_name_twice(
                    file,
                    path.site.line,
                    &path.written,
                    &other.written,
                ));
            }
        }
        rewrites.push(Rewrite {
            written: path.written.clone(),
            defined_at: path.defined_at.clone(),
            line: path.site.line,
            split_from_group: false,
        });
    }

    Ok(rewrites)
}

/// The condition of the operation: the defining path differs from the resolved one, and the crate
/// that defines the item is not the file's own.
///
/// `resolved` is crate-rooted at the file's own crate for every relative path, so its first segment
/// *is* that crate; a path already written with a dependency's name comes back unchanged from the
/// walk, so the first condition leaves it alone whatever its head reads.
fn goes_through_a_facade(path: &SurveyedPath) -> bool {
    let origin = path.resolved.split("::").next().unwrap_or_default();
    path.defined_at != path.resolved && path.defining_crate != origin
}

/// Whether the package's manifest declares the crate an item is defined in.
///
/// A path under `#[cfg(test)]` may be served by either table, since cargo compiles a test against
/// `[dependencies]` and `[dev-dependencies]` alike; a path anywhere else needs the ordinary table.
fn is_declared(manifest: &str, krate: &str, in_test: bool) -> bool {
    if in_test {
        manifest_edits::declares_dependency_in_either_table(manifest, krate)
    } else {
        manifest_edits::declares_dependency(manifest, Table::Dependencies, krate)
    }
}

/// Whether the path at `at` is written contiguously, with no whitespace or comment between its
/// segments.
///
/// The path the survey read is `written`; the text is read greedily from `at`. They agree when the
/// text spells the whole path, or when it spells a prefix of it that a `{` group opens — the only
/// way a member of a group is written shorter than the path it names. Anything else is a path split
/// across whitespace or carrying a comment.
fn spelled_on_one_line(text: &str, at: usize, written: &str) -> bool {
    let actual = written_prefix(&text[at..]);
    let actual_segments: Vec<&str> = actual.split("::").collect();
    let written_segments: Vec<&str> = written.split("::").collect();

    if actual_segments.len() == written_segments.len() {
        return actual_segments == written_segments;
    }
    actual_segments.len() < written_segments.len()
        && written_segments.starts_with(&actual_segments)
        && text[at + actual.len()..].starts_with("::{")
}

/// The last segment of a path.
pub(super) fn last_segment(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path)
}

/// One `use` leaf, as the duplicate-binding check reads it.
struct UseBinding {
    /// Where the leaf's path starts.
    at: usize,
    /// The inline modules enclosing it, outermost first.
    scope: Vec<String>,
    /// The name it binds.
    name: String,
    /// The path as written.
    written: String,
}

/// Every `use` leaf of the file, with the name it binds and the scope it binds it in.
fn use_bindings(text: &str) -> Vec<UseBinding> {
    sightings(text)
        .into_iter()
        .filter(|sighting| sighting.in_use)
        .map(|sighting| UseBinding {
            at: sighting.head_at,
            scope: sighting.modules,
            name: sighting.segments.last().cloned().unwrap_or_default(),
            written: sighting.segments.join("::"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
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
            restriction: false,
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
        let rewrites = path_edits(
            "use crate::config::A;\n",
            &survey,
            A_MANIFEST_OF_APP,
            "app/src/a.rs",
        );

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
        let refusal = path_edits(
            "use crate::roster::R;\n",
            &survey,
            A_MANIFEST_OF_APP,
            "app/src/a.rs",
        );

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
        let rewrites = path_edits(
            "use crate::b::Thing;\n",
            &survey,
            A_MANIFEST_OF_APP,
            "app/src/a.rs",
        );

        // Then there are none
        assert_eq!(rewrites.expect("the paths are decided"), []);
    }
}
