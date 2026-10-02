//! Resolving an item path through rust-analyzer's document outline.
//!
//! `textDocument/documentSymbol` answers a tree: modules, types and functions, with each `impl`
//! block a node of its own whose children are its members. An [`ItemPath`]'s segments walk that
//! tree — a type segment matches the type *and* every `impl` of it, a `<T as Trait>` segment only
//! that trait's `impl` — and the walk refuses rather than picks when a segment matches twice.

use serde_json::Value;

use super::{attached_trivia_starts_at, failure, server_defect, uri_of, LspPoint, RustBackend};
use crate::edit::{Position, Range};
use crate::item_anchor::{module_path_of, ItemAtResolver, ItemResolver, ResolvedItem};
use crate::plan::{Fingerprint, ItemPath, ItemSegment};
use crate::{RestructureError, Result};

/// Where a walk of the outline ended: the node's full range and its name's position, zero-based as
/// the server reports them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OutlineHit {
    pub(crate) range: Value,
    pub(crate) selection_start: Value,
}

/// Why a walk of the outline found no single node.
#[derive(Debug, PartialEq, Eq)]
enum Miss {
    /// Nothing in the outline answers to this segment.
    Absent(String),
    /// More than one node answers to the last segment.
    Ambiguous(usize),
    /// A node answered, and its answer carries no range to resolve to.
    Unreadable(String),
}

impl Miss {
    /// The refusal for `item`, said in terms of the file it was looked for in.
    fn refusal(self, item: &ItemPath, file: &str) -> RestructureError {
        match self {
            Miss::Absent(segment) => failure(format!(
                "`{item}` is not declared in {file}: nothing there is named `{segment}`"
            )),
            Miss::Ambiguous(count) => failure(format!(
                "`{item}` names {count} items in {file}{}",
                qualification_hint(item)
                    .map(|qualified| format!(" — qualify it with the trait, as `{qualified}`"))
                    .unwrap_or_default()
            )),
            Miss::Unreadable(segment) => server_defect(format!(
                "the outline of {file} answers `{segment}` without a range"
            )),
        }
    }
}

/// `item` with its parent segment written as `<Parent as Trait>`, when it has a parent to write so.
fn qualification_hint(item: &ItemPath) -> Option<String> {
    let segments = item.segments();
    let [before @ .., ItemSegment::Named(parent), last] = segments.as_slice() else {
        return None;
    };
    let mut pieces = vec![item.crate_name().to_string()];
    pieces.extend(before.iter().map(ItemSegment::spelled));
    pieces.push(format!("<{parent} as Trait>"));
    pieces.push(last.spelled());
    Some(pieces.join("::"))
}

/// Walk a `documentSymbol` answer down `segments`, refusing in the words of "this file".
///
/// The walk itself is [`walk`]; this is it spelled for a caller that has no file to name.
#[cfg(test)]
pub(crate) fn walk_outline(
    symbols: &Value,
    item: &ItemPath,
    segments: &[ItemSegment],
) -> Result<OutlineHit> {
    walk(symbols, segments).map_err(|miss| miss.refusal(item, "this file"))
}

/// Walk a `documentSymbol` answer down `segments`.
///
/// A segment matches every node it names: a module or type by its own name, and — before the last
/// segment — every `impl` of a type by the type it implements for, so members of several `impl`
/// blocks of one type are all in reach of the next segment. The last segment must name exactly one
/// node.
fn walk(symbols: &Value, segments: &[ItemSegment]) -> std::result::Result<OutlineHit, Miss> {
    let mut scopes = vec![symbols];

    for (index, segment) in segments.iter().enumerate() {
        let last = index + 1 == segments.len();
        let matches: Vec<&Value> = scopes
            .iter()
            .flat_map(|scope| scope.as_array().into_iter().flatten())
            .filter(|node| answers_to(node, segment, last))
            .collect();

        match matches.as_slice() {
            [] => return Err(Miss::Absent(segment.spelled())),
            [node] if last => return hit_of(node, segment),
            many if last => return Err(Miss::Ambiguous(many.len())),
            _ => {}
        }
        scopes = matches
            .iter()
            .filter_map(|node| node.get("children"))
            .collect();
    }

    Err(Miss::Absent(String::new()))
}

fn hit_of(node: &Value, segment: &ItemSegment) -> std::result::Result<OutlineHit, Miss> {
    match (node.get("range"), node.pointer("/selectionRange/start")) {
        (Some(range), Some(selection_start)) => Ok(OutlineHit {
            range: range.clone(),
            selection_start: selection_start.clone(),
        }),
        _ => Err(Miss::Unreadable(segment.spelled())),
    }
}

/// Whether an outline node is what `segment` names.
fn answers_to(node: &Value, segment: &ItemSegment, last: bool) -> bool {
    let Some(name) = node.get("name").and_then(Value::as_str) else {
        return false;
    };
    match segment {
        ItemSegment::Named(wanted) => {
            name == wanted
                || (!last
                    && read_impl(name).is_some_and(|block| base_name(block.self_type) == wanted))
        }
        ItemSegment::TraitImpl {
            self_type,
            trait_name,
        } => read_impl(name).is_some_and(|block| {
            base_name(block.self_type) == base_name(self_type)
                && block
                    .trait_name
                    .is_some_and(|implemented| base_name(implemented) == base_name(trait_name))
        }),
    }
}

/// What an `impl` block's outline label says: `impl Stack` or `impl Display for Stack`.
struct ImplLabel<'a> {
    trait_name: Option<&'a str>,
    self_type: &'a str,
}

fn read_impl(label: &str) -> Option<ImplLabel<'_>> {
    let rest = label.strip_prefix("impl")?;
    let rest = match rest.chars().next()? {
        '<' => after_generics(rest)?,
        ' ' => rest,
        _ => return None,
    };
    let rest = rest.trim();
    Some(match rest.split_once(" for ") {
        Some((implemented, self_type)) => ImplLabel {
            trait_name: Some(implemented.trim()),
            self_type: self_type.trim(),
        },
        None => ImplLabel {
            trait_name: None,
            self_type: rest,
        },
    })
}

/// `text` without its leading `<…>` generic parameter list.
fn after_generics(text: &str) -> Option<&str> {
    let mut depth = 0usize;
    for (at, character) in text.char_indices() {
        match character {
            '<' => depth += 1,
            '>' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(&text[at + 1..]);
                }
            }
            _ => {}
        }
    }
    None
}

/// A type or trait as its bare name: no reference, no generic arguments, no path in front of it.
fn base_name(written: &str) -> &str {
    let written = written
        .trim()
        .trim_start_matches('&')
        .trim_start_matches("mut ")
        .trim_start_matches("dyn ")
        .trim();
    let without_arguments = written.split('<').next().unwrap_or(written);
    without_arguments
        .rsplit("::")
        .next()
        .unwrap_or(without_arguments)
        .trim()
}

/// An open document with the outline the server gave for it.
struct Outlined {
    root: std::path::PathBuf,
    text: String,
    symbols: Value,
}

impl RustBackend {
    /// Open `file` and read its outline once the server can give one.
    ///
    /// The documents opened here are the caller's to close: both entry points go through
    /// [`RustBackend::closing_what_it_opens`].
    fn outline_of(&mut self, file: &str) -> Result<Outlined> {
        let root = self.workspace_root()?;
        let absolute = root.join(file);
        let text = std::fs::read_to_string(&absolute)
            .map_err(|error| failure(format!("{file} could not be read: {error}")))?;
        let uri = uri_of(&absolute);

        self.start(&root)?;
        self.did_open(&uri, &text)?;
        let symbols = self.settled_outline(&uri)?;
        Ok(Outlined {
            root,
            text,
            symbols,
        })
    }

    fn resolve_item_opening(&mut self, file: &str, item: &ItemPath) -> Result<ResolvedItem> {
        let outlined = self.outline_of(file)?;
        let module = module_path_of(&outlined.root, file)?;
        let segments = segments_below(item, &module, file)?;

        let hit = walk(&outlined.symbols, &segments).map_err(|miss| miss.refusal(item, file))?;
        resolved_from(&outlined.text, &hit)
    }

    fn item_enclosing_opening(
        &mut self,
        file: &str,
        range: Range,
    ) -> Result<(ItemPath, ResolvedItem)> {
        let outlined = self.outline_of(file)?;
        let module = module_path_of(&outlined.root, file)?;
        let text = &outlined.text;

        let mut chain: Vec<&Value> = Vec::new();
        let mut level = Some(&outlined.symbols);
        while let Some(nodes) = level {
            let inner = nodes
                .as_array()
                .into_iter()
                .flatten()
                .find(|node| contains(node, range));
            let Some(inner) = inner else { break };
            chain.push(inner);
            level = inner.get("children");
        }

        let Some(innermost) = chain.last() else {
            return Err(failure(format!(
                "the position {}:{} is inside no item of {file}",
                range.start.line, range.start.col
            )));
        };

        let mut pieces = module;
        for node in &chain {
            pieces.push(segment_of(node)?);
        }
        if let Some(label) = innermost.get("name").and_then(Value::as_str) {
            if read_impl(label).is_some_and(|block| block.trait_name.is_none()) {
                return Err(failure(format!(
                    "the position {}:{} is inside `{label}` but in none of its items — anchor a \
                     member of it, or the type itself",
                    range.start.line, range.start.col
                )));
            }
        }

        let item = ItemPath::parse(&pieces.join("::"))?;
        let hit = hit_of(innermost, &ItemSegment::Named(item.to_string()))
            .map_err(|miss| miss.refusal(&item, file))?;
        Ok((item, resolved_from(text, &hit)?))
    }
}

impl ItemResolver for RustBackend {
    fn resolve_item(&mut self, file: &str, item: &ItemPath) -> Result<ResolvedItem> {
        self.closing_what_it_opens(|backend| backend.resolve_item_opening(file, item))
    }
}

impl ItemAtResolver for RustBackend {
    fn item_enclosing(&mut self, file: &str, range: Range) -> Result<(ItemPath, ResolvedItem)> {
        self.closing_what_it_opens(|backend| backend.item_enclosing_opening(file, range))
    }
}

/// The segments of `item` that lie below the module `file` is, refusing a path that is not in it.
fn segments_below(item: &ItemPath, module: &[String], file: &str) -> Result<Vec<ItemSegment>> {
    let segments = item.segments();
    let modules = &module[1..];
    let in_this_module = item.crate_name() == module[0]
        && segments.len() >= modules.len()
        && segments
            .iter()
            .zip(modules)
            .all(|(segment, name)| segment == &ItemSegment::Named(name.clone()));
    if !in_this_module {
        return Err(failure(format!(
            "`{item}` is not in {file}, which is module `{}`",
            module.join("::")
        )));
    }

    let below = segments[modules.len()..].to_vec();
    if below.is_empty() {
        return Err(failure(format!(
            "`{item}` names the module {file} is, not an item in it"
        )));
    }
    Ok(below)
}

/// How a node of the outline is written in an item path: a type's `impl` is its type, a trait's
/// `impl` is qualified with the trait.
fn segment_of(node: &Value) -> Result<String> {
    let name = node
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| server_defect("an outline node carries no name"))?;
    Ok(match read_impl(name) {
        Some(ImplLabel {
            trait_name: Some(implemented),
            self_type,
        }) => ItemSegment::TraitImpl {
            self_type: base_name(self_type).to_string(),
            trait_name: base_name(implemented).to_string(),
        }
        .spelled(),
        Some(ImplLabel {
            trait_name: None,
            self_type,
        }) => base_name(self_type).to_string(),
        None => name.to_string(),
    })
}

/// A server position as the one-based position a plan carries.
///
/// Columns are the server's own, which this client negotiates to be byte offsets: that is the unit
/// every position a plan carries is sent back to the server in, so nothing is converted.
fn plan_position(point: LspPoint) -> Position {
    Position {
        line: point.line as u32 + 1,
        col: point.character as u32 + 1,
    }
}

/// Whether `node`'s extent holds `range`, both as one-based plan positions.
fn contains(node: &Value, range: Range) -> bool {
    let bounds = (
        LspPoint::read(node.pointer("/range/start")),
        LspPoint::read(node.pointer("/range/end")),
    );
    let (Ok(start), Ok(end)) = bounds else {
        return false;
    };
    let (start, end) = (plan_position(start), plan_position(end));
    let key = |position: Position| (position.line, position.col);
    key(start) <= key(range.start) && key(range.end) <= key(end)
}

/// An outline node's extent in the coordinates plans use, and the fingerprint of its lines.
///
/// The extent begins where the comments and attributes attached to the item begin, for the reason
/// `anchors --items` has always begun there: a range that starts at the keyword leaves the item's
/// own documentation behind.
fn resolved_from(text: &str, hit: &OutlineHit) -> Result<ResolvedItem> {
    let start = plan_position(LspPoint::read(hit.range.get("start"))?);
    let end = plan_position(LspPoint::read(hit.range.get("end"))?);
    let name = plan_position(LspPoint::read(Some(&hit.selection_start))?);

    let first_line = attached_trivia_starts_at(text, start.line);
    let start = if first_line == start.line {
        start
    } else {
        Position {
            line: first_line,
            col: 1,
        }
    };

    let item_lines: Vec<&str> = text
        .split('\n')
        .skip(start.line as usize - 1)
        .take(end.line.saturating_sub(start.line) as usize + 1)
        .collect();

    Ok(ResolvedItem {
        range: Range { start, end },
        name,
        fingerprint: Fingerprint::of(&item_lines.join("\n")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// An outline node the way rust-analyzer reports one: zero-based range, children for members.
    fn a_node(name: &str, first: u32, last: u32, children: Value) -> Value {
        json!({
            "name": name,
            "range": { "start": { "line": first, "character": 0 }, "end": { "line": last, "character": 1 } },
            "selectionRange": { "start": { "line": first, "character": 4 }, "end": { "line": first, "character": 8 } },
            "children": children,
        })
    }

    /// `mod workflow` holding `Stack`, two `impl`s of it with a `fmt` each, and `impl Stack { new }`.
    fn an_outline() -> Value {
        json!([a_node(
            "workflow",
            0,
            40,
            json!([
                a_node("Stack", 1, 4, json!([])),
                a_node(
                    "impl Stack",
                    6,
                    11,
                    json!([a_node("new", 7, 10, json!([]))])
                ),
                a_node(
                    "impl Display for Stack",
                    13,
                    17,
                    json!([a_node("fmt", 14, 16, json!([]))])
                ),
                a_node(
                    "impl Debug for Stack",
                    19,
                    23,
                    json!([a_node("fmt", 20, 22, json!([]))])
                ),
            ])
        )])
    }

    fn segments_of(path: &ItemPath) -> Vec<ItemSegment> {
        path.segments()
    }

    #[test]
    fn an_inherent_method_is_found_through_its_types_impl() {
        let path = ItemPath::parse("stacks::workflow::Stack::new").unwrap();

        let hit = walk_outline(&an_outline(), &path, &segments_of(&path));

        assert_eq!(
            hit.ok().map(|hit| hit.range["start"]["line"].clone()),
            Some(json!(7))
        );
    }

    #[test]
    fn a_name_two_trait_impls_define_is_refused() {
        let path = ItemPath::parse("stacks::workflow::Stack::fmt").unwrap();

        let refused = walk_outline(&an_outline(), &path, &segments_of(&path));

        assert!(matches!(
            refused,
            Err(crate::RestructureError::MalformedPlan(_))
        ));
    }

    #[test]
    fn a_trait_qualified_segment_picks_that_traits_impl() {
        let path = ItemPath::parse("stacks::workflow::<Stack as Debug>::fmt").unwrap();

        let hit = walk_outline(&an_outline(), &path, &segments_of(&path));

        assert_eq!(
            hit.ok().map(|hit| hit.range["start"]["line"].clone()),
            Some(json!(20))
        );
    }

    #[test]
    fn a_segment_nothing_answers_to_is_refused() {
        let path = ItemPath::parse("stacks::workflow::Deque::new").unwrap();

        let refused = walk_outline(&an_outline(), &path, &segments_of(&path));

        assert!(matches!(
            refused,
            Err(crate::RestructureError::MalformedPlan(_))
        ));
    }

    #[test]
    fn an_impl_label_names_its_type_and_the_trait_it_implements() {
        let label = read_impl("impl<T> fmt::Display for Wrapper<T>").unwrap();

        assert_eq!(
            (label.trait_name.map(base_name), base_name(label.self_type)),
            (Some("Display"), "Wrapper")
        );
    }

    #[test]
    fn a_function_whose_name_starts_with_impl_is_not_an_impl_block() {
        assert!(read_impl("implement").is_none());
    }

    #[test]
    fn a_member_of_a_generic_impl_is_reached_through_its_bare_type() {
        let outline = json!([
            a_node("Stack", 0, 2, json!([])),
            a_node(
                "impl<T> Stack<T>",
                4,
                9,
                json!([a_node("peek", 5, 8, json!([]))])
            ),
        ]);
        let path = ItemPath::parse("c::Stack::peek").unwrap();

        let hit = walk_outline(&outline, &path, &path.segments());

        assert_eq!(
            hit.ok().map(|hit| hit.range["start"]["line"].clone()),
            Some(json!(5))
        );
    }
}
