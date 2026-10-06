//! `canonical_paths`: spelling a facade path in moved text as the path that defines what it names.
//!
//! `crate::config::Settings` where `crate::config` is `pub use kernel::config;` travels with a moved
//! signature as written, and the module that now holds it cannot later leave its crate: the facade
//! stays behind. With `canonical_paths` on the plan line, each such path reads `kernel::config::Settings`
//! instead, resolved by the same survey the cross-crate moves use ([`crate::crate_move::survey`]).
//!
//! A path whose defining module is not `pub` is left as written and named in the notes: it cannot be
//! spelled from outside its own crate, and writing it would turn a working move into one that does
//! not compile (O3).
//!
//! Off by default: a plan written without the field keeps its result byte for byte.

use std::ops::Range;

use super::assemble::Moving;
use super::text::Edit;
use crate::crate_move::module_files::{children_directory, file_of_child};
use crate::crate_move::source_scan::items_of_module;
use crate::crate_move::survey::{survey_moved_file, PathSurvey, SurveyedPath};
use crate::crate_move::Destination;
use crate::edit::Position;
use crate::registry::Workspace;
use crate::Result;

/// What the pass changes in the moved text, and what it says about every path it met.
pub(super) struct Rewritten {
    /// Replacements, in the coordinates of the text the pass was given.
    pub(super) edits: Vec<Edit>,
    /// One line per path rewritten and one per path left as written, each naming the path as written
    /// and the path that defines it.
    pub(super) notes: Vec<String>,
}

/// What the pass decided about one `crate::`-headed path whose defining path differs from the one
/// written, before the defining module's visibility is known.
enum Decision {
    /// Replace the path's span.
    Rewrite(Range<usize>),
    /// Leave it, for a reason that does not depend on visibility.
    Leave(&'static str),
}

/// Every `crate::`-headed path in `moved_text` whose defining path differs from the one written,
/// replaced by the defining path; `claimed` are the spans `rebase` already edits, which are left to it.
///
/// # Errors
///
/// Refuses when the moved file's origin crate cannot be read, or the survey of its paths cannot be
/// taken: the pass must not rewrite a path it never resolved.
pub(super) fn defining_paths(
    moving: &Moving<'_>,
    moved_text: &str,
    claimed: &[Range<usize>],
) -> Result<Rewritten> {
    let origin = Destination::read(moving.workspace.root, &moving.package.dir.to_string_lossy())?;
    let survey = survey_moved_file(moving.workspace, moved_text, &origin, moving.source)?;
    let decisions = decisions(moved_text, &survey, claimed);
    collected(decisions, &moving.package.crate_name, |path| {
        nameable(moving.workspace, &origin, &path.defined_at)
    })
}

/// The rewrite itself, a function of the text and a survey of it: each `crate::`-headed path whose
/// `defined_at` differs from what is written, and whose span reads back as written, is replaced by
/// `defined_at` (spelled from `crate` when `own_crate` defines it); every path met, rewritten or left,
/// is named in the notes.
///
/// This is the pure, server-free seam the unit tests drive; [`defining_paths`] is the same walk with
/// the defining module's visibility consulted, which needs a workspace.
///
/// # Errors
///
/// Refuses when the text at a path's span cannot be read back as written — which never happens, so it
/// is an error rather than a silent leave.
#[allow(dead_code)]
pub(super) fn rewrite(
    moved_text: &str,
    survey: &PathSurvey,
    claimed: &[Range<usize>],
    own_crate: &str,
) -> Result<Rewritten> {
    let decisions = decisions(moved_text, survey, claimed);
    collected(decisions, own_crate, |_| Ok(true))
}

/// One decision per `crate::`-headed surveyed path whose defining path differs from the one written,
/// in the survey's order. The span is located only when the text reads back as written; a path that
/// is in a `use` item is left (a `use` item is `repoint-facade`'s to split).
fn decisions(
    moved_text: &str,
    survey: &PathSurvey,
    claimed: &[Range<usize>],
) -> Vec<(SurveyedPath, Decision)> {
    let mut decisions = Vec::new();
    for path in &survey.paths {
        // Only a `crate::`-headed path is this pass's: a `super::`/`self::` head is `rebase`'s, and a
        // crate-name head is already canonical. A path the survey resolved to its own definition
        // (`defined_at == resolved`) needs no rewrite and no note.
        if !path.written.starts_with("crate::") || path.defined_at == path.resolved {
            continue;
        }
        let decision = match span_read_back(moved_text, path) {
            None => Decision::Leave("its span does not read back as written"),
            Some(span) if claimed.iter().any(|claim| overlaps(claim, &span)) => {
                Decision::Leave("another edit claims its span")
            }
            Some(_) if !path.in_body => {
                Decision::Leave("it sits in a `use` item, which is not this pass's")
            }
            Some(span) => Decision::Rewrite(span),
        };
        decisions.push((path.clone(), decision));
    }
    decisions
}

/// Turn the decisions into edits and notes, asking `nameable` before a rewrite whether the defining
/// path may be spelled from the moving crate at all; a path it may not is left as written and named.
fn collected(
    decisions: Vec<(SurveyedPath, Decision)>,
    own_crate: &str,
    mut nameable: impl FnMut(&SurveyedPath) -> Result<bool>,
) -> Result<Rewritten> {
    let mut edits = Vec::new();
    let mut notes = Vec::new();
    for (path, decision) in decisions {
        match decision {
            Decision::Leave(why) => notes.push(left_note(&path, why)),
            Decision::Rewrite(span) => {
                if nameable(&path)? {
                    edits.push(Edit::replace(span, defining_path(&path, own_crate)));
                    notes.push(format!(
                        "canonical_paths: rewrote `{}` to `{}`",
                        path.written, path.defined_at
                    ));
                } else {
                    notes.push(format!(
                        "canonical_paths: left `{}` as written: its defining module `{}` is private, \
                         so the path cannot be spelled here; it is defined at `{}`",
                        path.written,
                        defining_module(&path.defined_at),
                        path.defined_at
                    ));
                }
            }
        }
    }
    Ok(Rewritten { edits, notes })
}

/// The path `path` is defined at, spelled as the moved file's own crate writes it: a definition in
/// the moving crate reads `crate::…`, one in another crate keeps its extern name.
fn defining_path(path: &SurveyedPath, own_crate: &str) -> String {
    match path.defined_at.strip_prefix(&format!("{own_crate}::")) {
        Some(rest) => format!("crate::{rest}"),
        None => path.defined_at.clone(),
    }
}

/// The module an item path ends in: `kernel::inner` for `kernel::inner::Thing`.
fn defining_module(defined_at: &str) -> &str {
    defined_at
        .rsplit_once("::")
        .map_or(defined_at, |(module, _)| module)
}

/// One line naming a path left as written and the path that defines it.
fn left_note(path: &SurveyedPath, why: &str) -> String {
    format!(
        "canonical_paths: left `{}` as written ({why}); it is defined at `{}`",
        path.written, path.defined_at
    )
}

/// Whether every module on the way to `defined_at` is declared `pub` by the crate that defines it,
/// so the moved file may name the path. A module that crate does not declare, or declares with any
/// other visibility, is not nameable.
fn nameable(workspace: &Workspace<'_>, origin: &Destination, defined_at: &str) -> Result<bool> {
    let mut segments = defined_at.split("::");
    let Some(head) = segments.next() else {
        return Ok(false);
    };
    let Some(dir) = crate_directory(workspace, origin, head)? else {
        return Ok(false);
    };
    let rest: Vec<&str> = segments.collect();
    let modules = rest.split_last().map_or(&[][..], |(_, modules)| modules);

    let Some(root) = crate_root_file(workspace, &dir) else {
        return Ok(false);
    };
    let mut directory = children_directory(&root);
    let mut text = workspace.read(&root)?;
    for name in modules {
        let items = items_of_module(&text);
        let Some(child) = items.children.iter().find(|child| child.name == *name) else {
            return Ok(false);
        };
        if !child.is_public {
            return Ok(false);
        }
        match &child.body {
            Some(body) => {
                text = text[body.clone()].to_string();
                directory = directory.join(name);
            }
            None => {
                let Some((child_file, child_text)) = file_of_child(workspace, &directory, name)
                else {
                    return Ok(false);
                };
                directory = children_directory(&child_file);
                text = child_text;
            }
        }
    }
    Ok(true)
}

/// The directory of the crate `head` names, relative to the repository root.
fn crate_directory(
    workspace: &Workspace<'_>,
    origin: &Destination,
    head: &str,
) -> Result<Option<String>> {
    if head == origin.extern_name {
        return Ok(Some(origin.dir.clone()));
    }
    Ok(origin
        .path_dependency(workspace.root, head)?
        .map(|destination| destination.dir))
}

/// The crate root file under `dir`: `src/lib.rs`, else `src/main.rs`.
fn crate_root_file(workspace: &Workspace<'_>, dir: &str) -> Option<String> {
    ["lib.rs", "main.rs"].iter().find_map(|name| {
        let candidate = format!("{dir}/src/{name}");
        workspace.read(&candidate).ok().map(|_| candidate)
    })
}

/// Whether two spans share a byte.
fn overlaps(one: &Range<usize>, other: &Range<usize>) -> bool {
    one.start < other.end && other.start < one.end
}

/// The byte span of `path`'s written text, when the text there reads back exactly as the survey says
/// it was written. The survey gives the path's head, so the occurrence is the one the head falls in.
fn span_read_back(text: &str, path: &SurveyedPath) -> Option<Range<usize>> {
    let at = offset_of(text, path.site)?;
    let length = path.written.len();
    text.match_indices(path.written.as_str())
        .map(|(start, _)| start..start + length)
        .find(|span| span.contains(&at))
}

/// The byte offset of the one-based `site` in `text`.
fn offset_of(text: &str, site: Position) -> Option<usize> {
    let mut offset = 0usize;
    for (index, line) in text.split('\n').enumerate() {
        if index + 1 == site.line as usize {
            let column = site.col.checked_sub(1)? as usize;
            return line
                .char_indices()
                .nth(column)
                .map(|(byte, _)| offset + byte)
                .or_else(|| (column == line.chars().count()).then_some(offset + line.len()));
        }
        offset += line.len() + 1;
    }
    None
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
