mod rewrite;

use std::collections::BTreeSet;

#[cfg(test)]
use super::source_scan;
use super::survey::SurveyedPath;
use super::{Move, Result};
use crate::crate_move::header::rewrite::rewrite_of;
use crate::edit::{Position, TextEdit};
use crate::registry::Workspace;
use crate::{
    apply::byte_offset,
    crate_move::{survey, test_binary::segment_length},
};

/// What the moved file's paths say about the crates it needs, and the edits that re-point them.
pub(crate) struct Header {
    /// The edits that re-point it, empty when it names nothing that moved.
    pub(crate) edits: Vec<TextEdit>,
    /// Every crate the file's code names once re-pointed, by extern name.
    pub(crate) crates_named: BTreeSet<String>,
    /// The crates only a `#[cfg(test)]` item names, which the destination needs as
    /// `[dev-dependencies]`. A crate the rest of the file names too is in `crates_named` alone.
    pub(crate) dev_crates_named: BTreeSet<String>,
    /// The paths outside `#[cfg(test)]` that now name the crate the module left, for a refusal that
    /// has to list them.
    pub(crate) origin_paths: Vec<String>,
}

/// The moved file's paths, re-pointed at the crates they will name afterwards.
///
/// Every path comes from the survey, so a `use` at any depth and a path in a body are read the same
/// way, and each is rewritten by where it *ends up*:
///
/// - a path reaching a module in `co_moving` goes on saying `crate::`, because the destination
///   *is* `crate` for a file that has arrived in it — naming the destination by its package name from
///   inside it is `E0433`. A nested member is declared at the destination's root under its own last
///   segment, so `crate::model_registry::store` arrives as `crate::store`. A `self::` or `super::`
///   that stays inside the moved module still means the same thing and is left as written;
/// - a path whose item the destination defines — whether written as `destination::…`, or as a
///   `crate::…` the origin only forwards there — becomes `crate::…`;
/// - a path whose item the origin defines names the origin, which makes it an edge back;
/// - a path into any other crate names that crate.
///
/// A path in `co_moving` or in the destination names no crate the destination has to depend on —
/// least of all itself — and is no edge for a refusal to list. A path reaches a `co_moving` module
/// too when the origin only forwards it there, through a glob facade: it lands at that module. A
/// `pub(in …)` restriction is a visibility, never an edge: it names the module's new home when the
/// module moves, and is `pub(crate)` when it stays. A path under `#[cfg(test)]` is no edge
/// either: cargo allows a dev-dependency cycle, so its crates go to `dev_crates_named`.
///
/// A name the `use` leaves bound is kept: where following a re-export changes the last segment —
/// `use crate::roster;` over a module that is only `destination::records` — the new path is bound
/// `as roster`, so the paths in the body that use the name go on resolving.
///
/// `below` is the modules between the moved module and the file — empty for the module's own file,
/// `["b"]` for a carried `a/b.rs` — so the survey resolves its `self::` and `super::` paths against
/// `moving.home.path ++ below`, and one that stays inside the carried tree is left as written.
///
/// # Errors
///
/// Refuses a group that must split while an attribute or doc comment sits above it, a path spelled
/// across whitespace or comments, and everything the survey refuses.
pub(crate) fn repointed_header_at(
    workspace: &Workspace<'_>,
    text: &str,
    moving: &Move,
    below: &[String],
    co_moving: &BTreeSet<String>,
) -> Result<Header> {
    let module_path: Vec<String> = moving.home.path.iter().chain(below).cloned().collect();
    let survey = survey::survey_moved_file(workspace, text, &moving.origin, &module_path)?;

    let mut header = Header {
        edits: Vec::new(),
        crates_named: BTreeSet::new(),
        dev_crates_named: BTreeSet::new(),
        origin_paths: Vec::new(),
    };

    // The leaves of one `use` tree share a site, and are adjacent in the survey.
    for group in survey
        .paths
        .chunk_by(|one, other| !one.in_body && !other.in_body && one.site == other.site)
    {
        let reaches: Vec<Reach> = group
            .iter()
            .map(|path| reach(path, moving, co_moving))
            .collect();

        for (path, reached) in group.iter().zip(&reaches) {
            let Some(named) = &reached.names else {
                continue;
            };
            if path.in_test {
                header.dev_crates_named.insert(named.clone());
                continue;
            }
            header.crates_named.insert(named.clone());
            if reached.edge_back {
                push_once(&mut header.origin_paths, &path.defined_at);
            }
        }

        header
            .edits
            .extend(rewrite_of(&moving.source, text, group, &reaches)?);
    }

    header
        .dev_crates_named
        .retain(|named| !header.crates_named.contains(named));
    Ok(header)
}

/// Adds each of `paths` to `into` unless it is already there, keeping the order they first appear in.
pub(crate) fn extend_once(into: &mut Vec<String>, paths: Vec<String>) {
    for path in &paths {
        push_once(into, path);
    }
}

/// How the paths of a file the move re-points are read before the header pass decides on them.
#[allow(
    dead_code,
    reason = "TODO(reshape-tests-follow): implement — `repointed_header_in` reads it"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Gate {
    /// Each path is test code only where the file itself marks it `#[cfg(test)]`.
    AsWritten,
    /// The file is reached through a `#[cfg(test)] mod` declaration, so every path in it is test
    /// code: its crates are `[dev-dependencies]` and it makes no edge back to the origin.
    Test,
}

/// Where a file the move re-points sits, for the survey and for the `self::`/`super::` rule.
#[allow(
    dead_code,
    reason = "TODO(reshape-tests-follow): implement — `test_modules::follow_changes` builds one per file"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileInTree {
    /// The file's own module path in the origin, outermost first: what its `self::` and `super::`
    /// resolve against.
    pub(crate) module_path: Vec<String>,
    /// The module path of the tree the file travels in — the moved module for a carried file, the
    /// test module itself for a following test module. A `self::`/`super::` path that stays inside
    /// it is left as written; one that leaves it for a co-moving module is rewritten to `crate::`.
    pub(crate) tree_root: Vec<String>,
    pub(crate) gate: Gate,
}

/// The header pass over a file at an explicit place in a moving tree: [`repointed_header_at`]
/// for a file below the moved module, and for a test module that follows the move from beside it.
///
/// A path is co-moving when what it reaches is inside `co_moving` as written **or** after following
/// the origin's re-exports (`defined_at`), the reading the body precondition already takes; it then
/// lands at `crate::<landing>…`. Under [`Gate::Test`] every surveyed path is test code.
///
/// # Errors
///
/// Refuses for every reason [`repointed_header`] does.
#[allow(
    dead_code,
    reason = "TODO(reshape-tests-follow): implement — `repointed_header_at` and the following test \
              modules call it"
)]
pub(crate) fn repointed_header_in(
    workspace: &Workspace<'_>,
    text: &str,
    moving: &Move,
    file: &FileInTree,
    co_moving: &BTreeSet<String>,
) -> Result<Header> {
    // TODO(reshape-tests-follow): implement
    let _ = (workspace, text, moving, file, co_moving);
    todo!("repointed_header_in")
}

fn push_once(paths: &mut Vec<String>, path: &str) {
    if !paths.iter().any(|known| known == path) {
        paths.push(path.to_string());
    }
}

/// Where one surveyed path ends up.
struct Reach {
    /// The path as the destination has to write it, `None` when what is written stays valid.
    rewritten: Option<String>,
    /// The crate the destination has to depend on for it, when it is one.
    names: Option<String>,
    /// Whether it makes the destination depend on the crate the module left.
    edge_back: bool,
    /// Whether it is a `pub(in …)` restriction naming a module that stays behind, whose whole
    /// visibility becomes `pub(crate)`.
    narrowed: bool,
}

fn reach(path: &SurveyedPath, moving: &Move, co_moving: &BTreeSet<String>) -> Reach {
    let origin = &moving.origin.extern_name;

    let inside_the_origin = path.resolved.strip_prefix(&format!("{origin}::"));
    if let Some(reached) = inside_the_origin
        .and_then(|rest| travels_with(rest, co_moving).map(|member| (rest, member)))
        .map(|(rest, member)| travelling_reach(path, moving, rest, member))
    {
        return reached;
    }
    if path.restriction {
        return Reach {
            rewritten: None,
            names: None,
            edge_back: false,
            narrowed: true,
        };
    }
    if let Some(reached) = reach_through_a_facade(path, origin, co_moving) {
        return reached;
    }

    let named = &path.defining_crate;
    if *named == moving.destination.extern_name {
        return Reach {
            rewritten: Some(format!("crate{}", &path.defined_at[named.len()..])),
            names: None,
            edge_back: false,
            narrowed: false,
        };
    }
    Reach {
        rewritten: Some(path.defined_at.clone()),
        names: Some(named.clone()),
        edge_back: named == origin,
        narrowed: false,
    }
}

/// A path whose own spelling reaches `member`, one of the modules moving together: `rest` is what
/// follows the origin's name in it.
fn travelling_reach(path: &SurveyedPath, moving: &Move, rest: &str, member: &str) -> Reach {
    let head = path.written.split("::").next().unwrap_or_default();
    let stays_inside_the_module =
        matches!(head, "self" | "super") && member == moving.home.path.join("::");
    Reach {
        rewritten: (!stays_inside_the_module).then(|| landing_in_the_destination(rest, member)),
        names: None,
        edge_back: false,
        narrowed: false,
    }
}

/// A path the origin only forwards, through a glob or a re-export, to a module moving together:
/// it lands at that module.
fn reach_through_a_facade(
    path: &SurveyedPath,
    origin: &str,
    co_moving: &BTreeSet<String>,
) -> Option<Reach> {
    let rest = path.defined_at.strip_prefix(&format!("{origin}::"))?;
    let member = travels_with(rest, co_moving)?;
    Some(Reach {
        rewritten: Some(landing_in_the_destination(rest, member)),
        names: None,
        edge_back: false,
        narrowed: false,
    })
}

/// `rest` — a path inside the origin that reaches `member` — as the destination spells it: a nested
/// member is declared at the destination's root under its own last segment, so
/// `model_registry::store` arrives as `crate::store`.
fn landing_in_the_destination(rest: &str, member: &str) -> String {
    let landing = member.rsplit("::").next().unwrap_or(member);
    format!("crate::{landing}{}", &rest[member.len()..])
}

/// The path written from the start of `from_head`, up to where a group, a glob, an alias or the end
/// of the declaration begins.
pub(crate) fn written_prefix(from_head: &str) -> &str {
    let mut end = segment_length(from_head);
    while let Some(after) = from_head[end..].strip_prefix("::") {
        let length = segment_length(after);
        if length == 0 {
            break;
        }
        end += "::".len() + length;
    }
    &from_head[..end]
}

/// The member of the set a `crate::`-relative path reaches, if it reaches one — the caller
/// needs which member, since that is what says where the path lands.
///
/// The path is read the way the pass around it reads one: `rest` is what followed the qualifier,
/// and a member is named by the path that reaches it or by anything inside it — `spawn_worker` and
/// `spawn_worker::Worker` both travel with `spawn_worker`, and `spawn_worker_pool` travels with
/// nothing.
pub(crate) fn travels_with<'a>(rest: &str, co_moving: &'a BTreeSet<String>) -> Option<&'a String> {
    co_moving
        .iter()
        .find(|member| rest == *member || rest.starts_with(&format!("{member}::")))
}

/// Every `use` item of the file at any module depth — inline `mod tests { … }` included — with its
/// byte offset, the tree it spells (`crate::runtime::boot`, `super::*`, `a::{b, c}`) and whether it
/// sits under `#[cfg(test)]`.
///
/// Read off [`source_scan::sightings`], the walk the survey takes, so the two cannot disagree about
/// which `use` items a file holds or where a `cfg(test)` module begins. The rewrite itself is the
/// survey's: this lists the items, and re-points nothing.
// FIXME(move-facades): test-only adapter over source_scan::sightings; delete with every_depth_tests once agreed
#[cfg(test)]
pub(crate) fn use_items_at_every_depth(text: &str) -> Vec<(usize, &str, bool)> {
    let mut items: Vec<(usize, &str, bool)> = Vec::new();
    for sighting in source_scan::sightings(text).iter().filter(|s| s.in_use) {
        // The leaves of one `use` tree share a site.
        if items.last().is_some_and(|(at, ..)| *at == sighting.head_at) {
            continue;
        }
        let tree = &text[sighting.head_at..];
        let tree = tree.split(';').next().unwrap_or(tree).trim_end();
        items.push((sighting.head_at, tree, sighting.in_test));
    }
    items
}

/// Every top-level `use` declaration in a file, as the byte offset of its path and the path itself.
///
/// `pub(crate)` because a test binary's header is read the same way and re-pointed by different
/// rules: what a moved test names and what a moved module names are different questions, but
/// *which* text answers either is one question, and two scanners would disagree about an indented
/// `use` before long.
pub(crate) fn use_declarations(text: &str) -> Vec<(usize, &str)> {
    let mut declarations = Vec::new();
    let mut offset = 0usize;

    for line in text.split_inclusive('\n') {
        let start = offset;
        offset += line.len();

        if let Some((at, path)) = use_path(line) {
            declarations.push((start + at, path));
        }
    }
    declarations
}

/// The path a top-level `use` declaration names, and where on the line it starts.
///
/// An indented `use` belongs to a nested module or a function body; only the file's own header is
/// this operation's to rewrite.
fn use_path(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_end();
    if trimmed.starts_with(char::is_whitespace) {
        return None;
    }
    for keyword in ["pub use ", "use "] {
        if let Some(path) = trimmed.strip_prefix(keyword) {
            return Some((keyword.len(), path.trim_end_matches(';')));
        }
    }
    None
}

/// The path this caller writes, ending at the identifier the server reported.
pub(crate) struct WrittenPath {
    pub(crate) text: String,
    pub(crate) span: std::ops::Range<usize>,
}

/// Read the whole `a::b::C` a reference sits at the end of.
///
/// The server reports where the item's own name is; what has to be replaced is everything leading
/// to it, which is only readable from the caller's text.
pub(crate) fn written_path_at(text: &str, at: Position) -> Result<WrittenPath> {
    let start = byte_offset(text, at.line, at.col)?;
    let end = start
        + text[start..]
            .find(|character: char| !is_path_character(character))
            .unwrap_or(text.len() - start);

    let mut first = start;
    while let Some(head) = text[..first].strip_suffix("::") {
        let segment = head.trim_end_matches(is_path_character);
        if segment.len() == head.len() {
            break;
        }
        first = segment.len();
    }

    Ok(WrittenPath {
        text: text[first..end].to_string(),
        span: first..end,
    })
}

/// Whether `character` can appear inside a path segment.
///
/// `pub(crate)` because the sibling scan reads a path out of a file's text the same way, and two
/// notions of where a path ends would disagree about `spawn_worker_pool`.
pub(crate) fn is_path_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// The same path, written from outside the crate the module is leaving.
///
/// `None` when the path never names the module: the caller reaches the item through a name bound
/// somewhere else in its own file, and that binding is a reference of its own.
pub(crate) fn repointed(written: &str, moving: &Move) -> Option<String> {
    let segments: Vec<&str> = written.split("::").collect();
    let at = segments
        .iter()
        .position(|segment| *segment == moving.module)?;
    Some(format!(
        "{}::{}",
        moving.destination.extern_name,
        segments[at..].join("::")
    ))
}

#[cfg(test)]
mod every_depth_tests {
    use super::*;

    #[test]
    fn use_items_inside_an_inline_test_module_are_found_and_marked_as_test() {
        // Given a file with a root `use` and one inside its `mod tests`
        let text = "use crate::runtime::boot;\n\n#[cfg(test)]\nmod tests {\n    \
                    use crate::clock::Clock;\n    use super::*;\n}\n";

        // When every use item is read
        let found: Vec<(&str, bool)> = use_items_at_every_depth(text)
            .into_iter()
            .map(|(_, path, in_test)| (path, in_test))
            .collect();

        // Then all three are found, the inner two marked as under test
        assert_eq!(
            found,
            vec![
                ("crate::runtime::boot", false),
                ("crate::clock::Clock", true),
                ("super::*", true),
            ]
        );
    }
}
