//! Rewriting one `use` statement whose leaves a move re-points: Rule P (re-prefix in place when every
//! leaf agrees) and Rule S (kept members first, then one `use` per lifted member; a nested member
//! whose leaves disagree is flattened).
//!
//! One rule for every caller: a cross-crate move's own header, the callers it re-points with
//! `reexport: none`, and `repoint_facade_imports`. It lives here, below `backends`, because
//! `crate_move` must not depend on `backends::rust` — the move reads it, and `repoint_facade` reaches
//! down for it.
//!
//! A statement is read as [`Leaf`]s — the paths it writes and what each becomes — and rewritten whole
//! by [`statement_edit`]; [`statement_containing`] finds the statement a path sits in. The text
//! helpers the rule reads (`members_of`, `split_use`, `use_statements`) are here with it.

pub(crate) mod refusals;

use std::collections::BTreeSet;
use std::ops::Range;

use crate::crate_move::{manifest_edits, source_scan};
use crate::edit::TextEdit;
use crate::Result;

/// One leaf of a `use` statement: the path as written, and the path it is written as afterwards.
///
/// `rewritten == written` marks a leaf that keeps its path — a **kept** member under Rule S.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Leaf {
    /// The whole path, from the statement's own head: `crate::connection_service::seed_codebase`.
    pub(crate) written: String,
    /// What it becomes: `crate::seed_codebase`.
    pub(crate) rewritten: String,
}

impl Leaf {
    /// Whether the leaf keeps its path, so Rule S leaves its member where it is.
    fn is_kept(&self) -> bool {
        self.rewritten == self.written
    }
}

/// The one edit that replaces the `use` statement at `span` of `file`'s `text` with `leaves` applied
/// — the prefix alone when Rule P holds, the whole statement when Rule S splits it, every lifted line
/// carrying the statement's own indentation.
///
/// # Errors
///
/// Refuses a statement that must split while an attribute or doc comment sits on the line above it
/// (`refusals::attribute_above_a_split`), and one the rule cannot read.
pub(crate) fn statement_edit(
    file: &str,
    text: &str,
    span: Range<usize>,
    leaves: &[Leaf],
) -> Result<TextEdit> {
    let rewritten = group_rewrite(file, text, &span, leaves)?;
    Ok(manifest_edits::replacement(text, span, &rewritten))
}

/// The span of the `use` statement of `text` that holds byte offset `at` — from its visibility (or
/// its keyword) to the `;` that ends it — or `None` when `at` is in no `use` statement.
pub(crate) fn statement_containing(text: &str, at: usize) -> Option<Range<usize>> {
    use_statements(&source_scan::masked_code(text))
        .into_iter()
        .find(|span| span.contains(&at))
}

pub(crate) fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || !byte.is_ascii()
}

/// The span of every `use` item in the masked text, from its visibility (or the keyword) to the `;`
/// that ends it, at whatever depth it is written.
pub(crate) fn use_statements(masked: &str) -> Vec<Range<usize>> {
    let bytes = masked.as_bytes();
    let mut found = Vec::new();
    for (at, _) in masked.match_indices("use") {
        let end = at + "use".len();
        let whole = (at == 0 || !is_identifier_byte(bytes[at - 1]))
            && bytes
                .get(end)
                .is_some_and(|next| !is_identifier_byte(*next));
        let precise_capture = masked[end..].trim_start().starts_with('<');
        if !whole || precise_capture {
            continue;
        }
        let Some(semicolon) = masked[end..].find(';') else {
            continue;
        };
        found.push(with_visibility_before(masked, at)..end + semicolon + 1);
    }
    found
}

/// Where a `pub`, `pub(crate)` or `pub(in path)` written just before `at` begins, else `at`.
fn with_visibility_before(masked: &str, at: usize) -> usize {
    let before = masked[..at].trim_end();
    let keyword_end = if before.ends_with(')') {
        before
            .rfind("pub(")
            .map_or(before.len(), |open| open + "pub".len())
    } else {
        before.len()
    };
    let head = &before[..keyword_end];
    let is_pub = head.ends_with("pub")
        && (head.len() == 3 || !is_identifier_byte(head.as_bytes()[head.len() - 4]));
    if is_pub {
        head.len() - "pub".len()
    } else {
        at
    }
}

/// A `use` item split into what precedes the keyword (its visibility) and its tree, without the `;`.
pub(crate) fn split_use(statement: &str) -> Option<(&str, &str)> {
    let keyword = statement
        .match_indices("use")
        .map(|(at, _)| at)
        .find(|at| {
            let before = statement[..*at].bytes().next_back();
            let after = statement[at + 3..].chars().next();
            before.is_none_or(|byte| !is_identifier_byte(byte))
                && after.is_some_and(|character| character.is_whitespace() || character == '{')
        })?;
    let tree = statement[keyword + 3..].trim_end().strip_suffix(';')?;
    Some((&statement[..keyword], tree.trim()))
}

/// The members of a `use` group, split at the commas that are not inside a nested group.
pub(crate) fn members_of(inner: &str) -> Vec<&str> {
    let mut members = Vec::new();
    let (mut depth, mut from) = (0usize, 0usize);
    for (at, character) in inner.char_indices() {
        match character {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                members.push(inner[from..at].trim());
                from = at + 1;
            }
            _ => {}
        }
    }
    members.push(inner[from..].trim());
    members.retain(|member| !member.is_empty());
    members
}

/// The last segment of a path.
pub(crate) fn last_segment(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path)
}

/// `statement`, one whole `use` item, with `leaves` (its rewritten paths) applied.
// TODO(reshape-8-hand-fix): `Leaf` in place of `repoint_facade`'s `Rewrite` — `crate_move` may not
// name `backends`, and the move's own header and callers read this rule too.
pub(crate) fn split_or_reprefix(statement: &str, leaves: &[Leaf]) -> Result<String> {
    let (head, tree) = split_use(statement).ok_or_else(|| refusals::unreadable_use(statement))?;
    let Some(open) = tree.find('{') else {
        return plain(head, tree, leaves);
    };
    if !tree.ends_with('}') {
        return Err(refusals::unreadable_use(statement));
    }

    let prefix = tree[..open].trim_end().trim_end_matches("::");
    let members: Vec<Member> = members_of(&tree[open + 1..tree.len() - 1])
        .into_iter()
        .map(Member::parse)
        .collect();

    let mut matched: BTreeSet<usize> = BTreeSet::new();
    let mut kept: Vec<&str> = Vec::new();
    let mut lifted: Vec<String> = Vec::new();
    let mut replaced_prefixes: Vec<String> = Vec::new();
    let mut all_proper = true;

    for member in &members {
        let indices = member.matches(prefix, leaves);
        matched.extend(indices.iter().copied());
        if indices.iter().all(|index| leaves[*index].is_kept()) {
            kept.push(member.text);
            continue;
        }
        for index in &indices {
            match rule_p_prefix(&leaves[*index], prefix) {
                Some(replacement) => replaced_prefixes.push(replacement),
                None => all_proper = false,
            }
        }
        lifted.extend(member.lifted(prefix, leaves, &indices)?);
    }

    if matched.len() != leaves.len() {
        return Err(refusals::unreadable_use(statement));
    }
    if lifted.is_empty() {
        return Ok(statement.to_string());
    }

    if kept.is_empty() && all_proper {
        let distinct: BTreeSet<&String> = replaced_prefixes.iter().collect();
        if distinct.len() == 1 {
            let replacement = distinct.into_iter().next().expect("one prefix");
            return Ok(format!("{head}use {replacement}{};", &tree[prefix.len()..]));
        }
    }

    let mut lines = Vec::new();
    if !kept.is_empty() {
        lines.push(format!("{head}use {prefix}::{{{}}};", kept.join(", ")));
    }
    lines.extend(lifted.iter().map(|member| format!("{head}use {member};")));
    Ok(lines.join("\n"))
}

/// A plain `use` (no group) is not this function's to rewrite — the caller replaces the path so it
/// can read the text after it. Read here so a single-member group without braces is not a surprise.
fn plain(head: &str, tree: &str, leaves: &[Leaf]) -> Result<String> {
    let [only] = leaves else {
        return Err(refusals::unreadable_use(tree));
    };
    Ok(format!("{head}use {};", only.rewritten))
}

/// The prefix that would replace the group's own, when doing so leaves this leaf's path unchanged.
///
/// `None` when it would not: the leaf's tail after the group prefix is not what the rewritten path
/// ends with, so the member's own text has to change and the group cannot carry it.
fn rule_p_prefix(leaf: &Leaf, prefix: &str) -> Option<String> {
    if leaf.written == prefix {
        return Some(leaf.rewritten.clone());
    }
    let tail = leaf.written.strip_prefix(&format!("{prefix}::"))?;
    leaf.rewritten
        .strip_suffix(&format!("::{tail}"))
        .map(str::to_string)
}

/// One member of a `use` group.
struct Member<'a> {
    /// The member verbatim, for a kept member to stay as it was written.
    text: &'a str,
    /// The member's path relative to the group prefix, `config::Limits` or `self`.
    path_part: &'a str,
    /// The name the member binds, when it renames.
    alias: Option<&'a str>,
    /// The `{…}` a nested member carries, when it is a nested group.
    nested: Option<&'a str>,
    /// Whether the member ends in `*`.
    glob: bool,
}

impl<'a> Member<'a> {
    fn parse(text: &'a str) -> Member<'a> {
        let text = text.trim();
        // A nested group's own members carry their aliases; one alias per member is only a plain
        // member's.
        let (path, alias) = match text.split_once(" as ") {
            Some((path, alias)) if !text.contains('{') => (path.trim(), Some(alias.trim())),
            _ => (text, None),
        };
        let (path_part, nested) = match path.find('{') {
            Some(open) => (
                path[..open].trim_end().trim_end_matches("::"),
                Some(&path[open..]),
            ),
            None => (path, None),
        };
        let glob = path_part.ends_with('*');
        let path_part = if glob {
            path_part.trim_end_matches('*').trim_end_matches("::")
        } else {
            path_part
        };
        Member {
            text,
            path_part,
            alias,
            nested,
            glob,
        }
    }

    /// The member's own path, rooted at the group prefix, `crate::config` or `crate::config::Limits`.
    fn full_prefix(&self, prefix: &str) -> String {
        if self.path_part == "self" {
            prefix.to_string()
        } else {
            format!("{prefix}::{}", self.path_part)
        }
    }

    /// The leaves this member's path reaches.
    fn matches(&self, prefix: &str, leaves: &[Leaf]) -> Vec<usize> {
        let full = self.full_prefix(prefix);
        let inside = format!("{full}::");
        (0..leaves.len())
            .filter(|index| {
                let written = &leaves[*index].written;
                match self.nested {
                    Some(_) => *written == full || written.starts_with(&inside),
                    None => *written == full,
                }
            })
            .collect()
    }

    /// The statements this member becomes when it leaves the group (Rule S): one, or — for a nested
    /// member whose leaves do not share one new prefix — one per leaf.
    fn lifted(&self, prefix: &str, leaves: &[Leaf], indices: &[usize]) -> Result<Vec<String>> {
        let Some(nested) = self.nested else {
            return Ok(vec![self.named(&leaves[indices[0]])]);
        };
        let full = self.full_prefix(prefix);
        let inside = format!("{full}::");
        let prefixes: Option<BTreeSet<&str>> = indices
            .iter()
            .map(|index| {
                let leaf = &leaves[*index];
                let inner = leaf.written.strip_prefix(&inside)?;
                leaf.rewritten.strip_suffix(&format!("::{inner}"))
            })
            .collect();
        match prefixes {
            Some(shared) if shared.len() == 1 => {
                let new_prefix = shared.into_iter().next().unwrap_or_default();
                Ok(vec![format!("{new_prefix}::{nested}")])
            }
            _ => flattened(&full, nested, leaves, indices),
        }
    }

    /// The lifted path of `leaf`, keeping the name the member bound when the rewrite would change it.
    fn named(&self, leaf: &Leaf) -> String {
        if self.glob {
            return format!("{}::*", leaf.rewritten);
        }
        if self.alias.is_some() {
            return self.with_alias(leaf.rewritten.clone());
        }
        let was = last_segment(&leaf.written);
        if last_segment(&leaf.rewritten) != was {
            format!("{} as {was}", leaf.rewritten)
        } else {
            leaf.rewritten.clone()
        }
    }

    /// The lifted path under the member's own alias, when it has one.
    fn with_alias(&self, path: String) -> String {
        match self.alias {
            Some(alias) => format!("{path} as {alias}"),
            None => path,
        }
    }
}

/// One statement per leaf of the nested group `nested`, spelled under `full`, in the order the
/// group spells them — each keeping the name its member bound.
fn flattened(full: &str, nested: &str, leaves: &[Leaf], indices: &[usize]) -> Result<Vec<String>> {
    let inner = nested
        .strip_prefix('{')
        .and_then(|group| group.strip_suffix('}'))
        .ok_or_else(|| refusals::unreadable_use(nested))?;

    let mut statements = Vec::new();
    for text in members_of(inner) {
        let member = Member::parse(text);
        let path = member.full_prefix(full);
        if let Some(group) = member.nested {
            statements.extend(flattened(&path, group, leaves, indices)?);
            continue;
        }
        let leaf = indices
            .iter()
            .map(|index| &leaves[*index])
            .find(|leaf| leaf.written == path)
            .ok_or_else(|| refusals::unreadable_use(text))?;
        statements.push(member.named(leaf));
    }
    Ok(statements)
}

/// Whether a `use` item is a group — its tree carries a `{`.
pub(crate) fn is_a_group(statement: &str) -> bool {
    split_use(statement).is_some_and(|(_, tree)| tree.contains('{'))
}

/// The path of the leaf `written` at byte `at` of `statement`, from the statement's own head: what
/// each group enclosing it writes before its `{`, then `written` — `crate::a::X` for `X` in
/// `use crate::{a::{X, Y}};`.
pub(crate) fn leaf_path(statement: &str, at: usize, written: &str) -> String {
    let mut prefixes: Vec<&str> = Vec::new();
    let mut closed = 0usize;
    for (index, character) in statement[..at].char_indices().rev() {
        match character {
            '}' => closed += 1,
            '{' if closed > 0 => closed -= 1,
            '{' => prefixes.push(path_before(&statement[..index])),
            _ => {}
        }
    }
    prefixes.reverse();
    prefixes.push(written);
    prefixes
        .into_iter()
        .filter(|piece| !piece.is_empty())
        .collect::<Vec<_>>()
        .join("::")
}

/// The path written at the end of `text`, without a trailing `::`: `crate::a` in `use crate::a::`.
fn path_before(text: &str) -> &str {
    let text = text.trim_end();
    let text = text.strip_suffix("::").unwrap_or(text);
    let start = text
        .char_indices()
        .rev()
        .take_while(|(_, character)| character.is_alphanumeric() || matches!(character, '_' | ':'))
        .last()
        .map_or(text.len(), |(index, _)| index);
    &text[start..]
}

/// One whole `use` item's replacement, with the indentation of the statement it replaced added to
/// every line after the first, and a refusal when a group that must split carries an attribute.
pub(crate) fn group_rewrite(
    file: &str,
    text: &str,
    span: &std::ops::Range<usize>,
    leaves: &[Leaf],
) -> Result<String> {
    let statement = &text[span.clone()];
    let line = manifest_edits::position_of(text, span.start).line;
    let rewritten = split_or_reprefix(statement, leaves)?;
    if rewritten.contains('\n') && attribute_above(text, line) {
        return Err(refusals::attribute_above_a_split(file, line));
    }

    let indent = indentation(text, span.start);
    if indent.is_empty() {
        Ok(rewritten)
    } else {
        Ok(rewritten.replace('\n', &format!("\n{indent}")))
    }
}

/// The whitespace that stands between the start of `offset`'s line and `offset`.
fn indentation(text: &str, offset: usize) -> String {
    let start = text[..offset].rfind('\n').map_or(0, |at| at + 1);
    text[start..offset]
        .chars()
        .take_while(|character| *character == ' ' || *character == '\t')
        .collect()
}

/// Whether the line above one-based `line` is an attribute or a doc comment.
fn attribute_above(text: &str, line: u32) -> bool {
    if line <= 1 {
        return false;
    }
    let previous = text
        .lines()
        .nth(line as usize - 2)
        .unwrap_or_default()
        .trim_start();
    previous.starts_with("#[")
        || previous.starts_with("///")
        || previous.starts_with("//!")
        || previous.starts_with("/**")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_leaf(written: &str, rewritten: &str) -> Leaf {
        Leaf {
            written: written.to_string(),
            rewritten: rewritten.to_string(),
        }
    }

    #[test]
    fn finds_the_statement_holding_an_offset_from_its_visibility_to_its_semicolon() {
        // Given a `pub use` group after a plain `use`
        let text = "use std::fmt;\npub use crate::{a::X, b::Y};\n";
        let inside = text.find("b::Y").expect("the leaf is written");

        // When the statement holding a leaf of the group is asked for
        let span = statement_containing(text, inside);

        // Then it is the whole `pub use` statement
        assert_eq!(
            span.map(|span| &text[span]),
            Some("pub use crate::{a::X, b::Y};")
        );
    }

    #[test]
    fn an_offset_in_no_use_statement_has_no_statement() {
        // Given a body path
        let text = "fn f() -> u32 {\n    crate::a::count()\n}\n";
        let inside = text.find("count").expect("the path is written");

        // When
        let span = statement_containing(text, inside);

        // Then
        assert_eq!(span, None);
    }

    #[test]
    fn a_statement_edit_lifts_a_disagreeing_member_with_the_statements_indentation() {
        // Given a group inside an inline module whose second leaf lands in another crate
        let text = "mod inner {\n    use crate::{a::X, clock::Clock};\n}\n";
        let span = statement_containing(text, text.find("clock").expect("the leaf"))
            .expect("the statement");
        let leaves = [
            a_leaf("crate::a::X", "crate::a::X"),
            a_leaf("crate::clock::Clock", "shared::clock::Clock"),
        ];

        // When the statement is rewritten
        let edit = statement_edit("src/inner.rs", text, span, &leaves).expect("the edit");

        // Then the kept member stays first and the lifted one follows at the same indentation
        assert_eq!(
            crate::apply::edited(text.to_string(), &[edit]).expect("the edit applies"),
            "mod inner {\n    use crate::{a::X};\n    use shared::clock::Clock;\n}\n"
        );
    }

    /// Rule P: when every member agrees on what the common prefix becomes, the prefix is replaced in
    /// place and the group keeps its shape.
    #[test]
    fn a_group_whose_members_agree_keeps_its_shape_under_the_new_prefix() {
        // Given a group whose members both go through `crate::config`
        let leaves = [
            a_leaf("crate::config::A", "kernel::config::A"),
            a_leaf("crate::config::B", "kernel::config::B"),
        ];

        // When it is re-pointed
        let statement = split_or_reprefix("use crate::config::{A, B};", &leaves);

        // Then the prefix is replaced in place
        assert_eq!(
            statement.expect("the group is re-pointed"),
            "use kernel::config::{A, B};"
        );
    }

    /// Rule S: members that need different qualifiers leave the group, kept members first.
    #[test]
    fn a_group_whose_members_disagree_keeps_its_kept_members_first_and_lifts_the_rest() {
        // Given a group of one member through a facade and one of the crate's own
        let leaves = [a_leaf("crate::config::Limits", "kernel::config::Limits")];

        // When it is split
        let statement = split_or_reprefix("use crate::{config::Limits, b::Thing};", &leaves);

        // Then the kept member stays and the lifted one follows on its own line
        assert_eq!(
            statement.expect("the group is split"),
            "use crate::{b::Thing};\nuse kernel::config::Limits;"
        );
    }

    /// A group with no kept member has nothing left to stand in.
    #[test]
    fn a_group_with_no_kept_member_disappears_into_its_lifted_statements() {
        // Given a group whose members are re-exported from two crates, behind a visibility
        let leaves = [
            a_leaf("crate::config::Limits", "kernel::config::Limits"),
            a_leaf("crate::roster::Roster", "agents::roster::Roster"),
        ];

        // When it is split
        let statement =
            split_or_reprefix("pub use crate::{config::Limits, roster::Roster};", &leaves);

        // Then one public statement per member remains
        assert_eq!(
            statement.expect("the group is split"),
            "pub use kernel::config::Limits;\npub use agents::roster::Roster;"
        );
    }

    #[test]
    fn reads_the_indentation_of_the_statement_it_replaces() {
        assert_eq!(indentation("mod a {\n    use x::y;\n}\n", 12), "    ");
        assert_eq!(indentation("use x::y;\n", 0), "");
    }

    #[test]
    fn sees_an_attribute_or_doc_comment_above_a_statement() {
        assert!(attribute_above("#[cfg(unix)]\nuse a;\n", 2));
        assert!(attribute_above("/// The limits.\nuse a;\n", 2));
        assert!(!attribute_above("use a;\nuse b;\n", 2));
        assert!(!attribute_above("use a;\n", 1));
    }

    #[test]
    fn a_leafs_path_carries_the_prefix_of_every_group_around_it() {
        // Given a leaf written inside a group nested in another
        let statement = "use crate::{x::Y, a::{b::Z, c}};";
        let at = statement.find("Z").expect("the leaf is written") - "b::".len();

        // When its path is read from the statement's head
        let path = leaf_path(statement, at, "b::Z");

        // Then the enclosing groups' prefixes come first, and a sibling group's do not
        assert_eq!(path, "crate::a::b::Z");
    }

    #[test]
    fn a_nested_member_flattened_keeps_each_leafs_alias_and_glob() {
        // Given a nested member whose leaves land in two crates, one aliased and one a glob
        let statement = "use crate::{mix::{Clock as Tick, roster::*}};";
        let leaves = [
            a_leaf("crate::mix::Clock", "shared::clock::Clock"),
            a_leaf("crate::mix::roster", "agents::roster"),
        ];

        // When it is split
        let split = split_or_reprefix(statement, &leaves);

        // Then each leaf is a statement of its own, under the name it bound
        assert_eq!(
            split.expect("the group is split"),
            "use shared::clock::Clock as Tick;\nuse agents::roster::*;"
        );
    }

    #[test]
    fn a_leaf_that_keeps_its_path_stays_in_the_group_beside_one_that_moves() {
        // Given a group whose first leaf is unchanged and whose second goes to another crate
        let leaves = [
            a_leaf("crate::a::X", "crate::a::X"),
            a_leaf("crate::b::Y", "shared::b::Y"),
        ];

        // When it is split
        let split = split_or_reprefix("use crate::{a::X, b::Y};", &leaves);

        // Then the unchanged one stays under the old prefix
        assert_eq!(
            split.expect("the group is split"),
            "use crate::{a::X};\nuse shared::b::Y;"
        );
    }
}
