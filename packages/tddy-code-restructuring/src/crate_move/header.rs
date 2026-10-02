use std::collections::BTreeSet;

#[cfg(test)]
use super::source_scan;
use super::survey::SurveyedPath;
use super::{malformed, Move, Result};
use crate::edit::{Position, TextEdit};
use crate::registry::Workspace;
use crate::RestructureError;
use crate::{
    apply::byte_offset,
    crate_move::{manifest_edits, survey, test_binary::segment_length},
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
    /// The subset of `origin_paths` written in the file's own top-level `use` header — the only
    /// ones `check` reads.
    // TODO(check-parity-header): the one reader, `cluster::paths_naming_the_origin`, is the
    // stranded-sibling finding, and it asks about the top-level `use` header alone. Reading
    // `origin_paths` instead would also report bodies and nested `use` items, which `apply`'s
    // cycle refusal (`refusals::refuse_a_dependency_cycle`) already reads — so the field goes when
    // that finding is widened to match it on purpose, with its own tests and wording, not as a
    // side effect of removing a field. Bodies are covered separately by
    // `preconditions::stays_behind_through_a_body`.
    pub(crate) header_origin_paths: Vec<String>,
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
/// least of all itself — and is no edge for a refusal to list. A path under `#[cfg(test)]` is no edge
/// either: cargo allows a dev-dependency cycle, so its crates go to `dev_crates_named`.
///
/// A name the `use` leaves bound is kept: where following a re-export changes the last segment —
/// `use crate::roster;` over a module that is only `destination::records` — the new path is bound
/// `as roster`, so the paths in the body that use the name go on resolving.
///
/// # Errors
///
/// Refuses a `use` group whose members would need different qualifiers, because one prefix is all a
/// group has, and for everything the survey refuses.
pub(crate) fn repointed_header(
    workspace: &Workspace<'_>,
    text: &str,
    moving: &Move,
    co_moving: &BTreeSet<String>,
) -> Result<Header> {
    let survey = survey::survey_moved_file(workspace, text, &moving.origin, &moving.home.path)?;
    let header_lines: BTreeSet<u32> = use_declarations(text)
        .into_iter()
        .map(|(at, _)| manifest_edits::position_of(text, at).line)
        .collect();

    let mut header = Header {
        edits: Vec::new(),
        crates_named: BTreeSet::new(),
        dev_crates_named: BTreeSet::new(),
        origin_paths: Vec::new(),
        header_origin_paths: Vec::new(),
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
                if !path.in_body && header_lines.contains(&path.site.line) {
                    push_once(&mut header.header_origin_paths, &path.defined_at);
                }
            }
        }

        header.edits.extend(rewrite_of(text, group, &reaches)?);
    }

    header
        .dev_crates_named
        .retain(|named| !header.crates_named.contains(named));
    Ok(header)
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
}

fn reach(path: &SurveyedPath, moving: &Move, co_moving: &BTreeSet<String>) -> Reach {
    let origin = &moving.origin.extern_name;

    let inside_the_origin = path.resolved.strip_prefix(&format!("{origin}::"));
    if let Some((rest, member)) = inside_the_origin
        .and_then(|rest| travels_with(rest, co_moving).map(|member| (rest, member)))
    {
        let head = path.written.split("::").next().unwrap_or_default();
        let stays_inside_the_module =
            matches!(head, "self" | "super") && *member == moving.home.path.join("::");
        let landing = member.rsplit("::").next().unwrap_or(member);
        return Reach {
            rewritten: (!stays_inside_the_module)
                .then(|| format!("crate::{landing}{}", &rest[member.len()..])),
            names: None,
            edge_back: false,
        };
    }

    let named = &path.defining_crate;
    if *named == moving.destination.extern_name {
        return Reach {
            rewritten: Some(format!("crate{}", &path.defined_at[named.len()..])),
            names: None,
            edge_back: false,
        };
    }
    Reach {
        rewritten: Some(path.defined_at.clone()),
        names: Some(named.clone()),
        edge_back: named == origin,
    }
}

/// The one edit that re-points a `use` tree or a path in a body, if it needs one.
///
/// A tree is replaced by its **prefix**: `crate` in `use crate::{a, b};`. Every member has to agree
/// on what that prefix becomes, since there is only one of it to write.
fn rewrite_of(text: &str, group: &[SurveyedPath], reaches: &[Reach]) -> Result<Option<TextEdit>> {
    let site = group[0].site;
    let at = byte_offset(text, site.line, site.col)?;
    let prefix = written_prefix(&text[at..]);
    let prefix_segments = prefix.split("::").count();

    let mut new_prefixes = BTreeSet::new();
    for (path, reached) in group.iter().zip(reaches) {
        let segments: Vec<&str> = path.written.split("::").collect();
        if segments.len() < prefix_segments || segments[..prefix_segments].join("::") != prefix {
            return Err(malformed(format!(
                "`{}` is spelled across whitespace or comments, which the path rewrite cannot \
                 address — write it on one line",
                path.written
            )));
        }
        let tail = segments[prefix_segments..].join("::");

        let new_prefix = match (&reached.rewritten, tail.is_empty()) {
            (None, _) => prefix.to_string(),
            (Some(new), true) => new.clone(),
            (Some(new), false) => new
                .strip_suffix(&format!("::{tail}"))
                .ok_or_else(|| one_use_per_path(path))?
                .to_string(),
        };
        new_prefixes.insert(new_prefix);
    }
    if new_prefixes.len() != 1 {
        return Err(one_use_per_path(&group[0]));
    }
    let new_prefix = new_prefixes.into_iter().next().unwrap_or_default();
    if new_prefix == prefix {
        return Ok(None);
    }

    let replacement = match keeps_its_name(text, at + prefix.len(), group, prefix, &new_prefix) {
        Some(name) => format!("{new_prefix} as {name}"),
        None => new_prefix,
    };
    Ok(Some(manifest_edits::replacement(
        text,
        at..at + prefix.len(),
        &replacement,
    )))
}

/// The name a plain `use` has to go on binding when re-pointing it changes its last segment.
fn keeps_its_name<'a>(
    text: &str,
    after_prefix: usize,
    group: &[SurveyedPath],
    prefix: &'a str,
    new_prefix: &str,
) -> Option<&'a str> {
    let plain_use = group.len() == 1 && !group[0].in_body && group[0].written == prefix;
    let ends_there = text[after_prefix..].trim_start().starts_with(';');
    let was = prefix.rsplit("::").next()?;
    (plain_use && ends_there && new_prefix.rsplit("::").next()? != was).then_some(was)
}

fn one_use_per_path(path: &SurveyedPath) -> RestructureError {
    malformed(format!(
        "`{}` is one of several paths a single `use` writes, and moving the module gives them \
         different qualifiers — write one `use` per path so each can be re-pointed on its own",
        path.written
    ))
}

/// The path written from the start of `from_head`, up to where a group, a glob, an alias or the end
/// of the declaration begins.
fn written_prefix(from_head: &str) -> &str {
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
