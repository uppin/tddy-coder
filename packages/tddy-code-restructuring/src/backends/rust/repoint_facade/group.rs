//! Rewriting one `use` statement: Rule P (re-prefix in place when every member agrees) and Rule S
//! (kept members first, then one `use` per lifted member).
//!
//! A group has one prefix, so it can only carry one. Rule P applies when replacing the group's
//! prefix with a single new prefix leaves every member's own path unchanged — the case
//! `repointed_header` handles today. Otherwise Rule S applies: the members that need a new
//! qualifier leave the group as statements of their own, in member order, and the rest stay.
//! A nested member is lifted whole when all its leaves share one new prefix, and refused otherwise.

use std::collections::BTreeSet;

use crate::backends::rust::item_move::sites::members_of;
use crate::backends::rust::item_move::text::split_use;
use crate::Result;

use super::refusals;
use super::rewrite::{last_segment, Rewrite};

/// `statement`, one whole `use` item, with `leaves` (its rewritten paths) applied.
pub(super) fn split_or_reprefix(statement: &str, leaves: &[Rewrite]) -> Result<String> {
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
        if indices.is_empty() {
            kept.push(member.text);
            continue;
        }
        for index in &indices {
            matched.insert(*index);
            match rule_p_prefix(&leaves[*index], prefix) {
                Some(replacement) => replaced_prefixes.push(replacement),
                None => all_proper = false,
            }
        }
        lifted.push(member.lifted(prefix, leaves, &indices)?);
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
fn plain(head: &str, tree: &str, leaves: &[Rewrite]) -> Result<String> {
    let [only] = leaves else {
        return Err(refusals::unreadable_use(tree));
    };
    Ok(format!("{head}use {};", only.defined_at))
}

/// The prefix that would replace the group's own, when doing so leaves this leaf's path unchanged.
///
/// `None` when it would not: the leaf's tail after the group prefix is not what the defining path
/// ends with, so the member's own text has to change and the group cannot carry it.
fn rule_p_prefix(leaf: &Rewrite, prefix: &str) -> Option<String> {
    if leaf.written == prefix {
        return Some(leaf.defined_at.clone());
    }
    let tail = leaf.written.strip_prefix(&format!("{prefix}::"))?;
    leaf.defined_at
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
        let (path, alias) = match text.split_once(" as ") {
            Some((path, alias)) => (path.trim(), Some(alias.trim())),
            None => (text, None),
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
    fn matches(&self, prefix: &str, leaves: &[Rewrite]) -> Vec<usize> {
        let full = self.full_prefix(prefix);
        if self.nested.is_some() {
            let inside = format!("{full}::");
            (0..leaves.len())
                .filter(|index| leaves[*index].written.starts_with(&inside))
                .collect()
        } else {
            (0..leaves.len())
                .filter(|index| leaves[*index].written == full)
                .collect()
        }
    }

    /// The statement this member becomes when it leaves the group (Rule S).
    fn lifted(&self, prefix: &str, leaves: &[Rewrite], indices: &[usize]) -> Result<String> {
        if let Some(nested) = self.nested {
            let full = self.full_prefix(prefix);
            let inside = format!("{full}::");
            let mut prefixes = BTreeSet::new();
            for index in indices {
                let leaf = &leaves[*index];
                let inner = leaf.written.strip_prefix(&inside).unwrap_or(&leaf.written);
                prefixes.insert(strip_tail(&leaf.defined_at, inner));
            }
            if prefixes.len() != 1 {
                return Err(refusals::nested_member_reaches_two_crates(self.text));
            }
            let path = format!(
                "{}::{nested}",
                prefixes.into_iter().next().unwrap_or_default()
            );
            return Ok(self.with_alias(path));
        }

        let leaf = &leaves[indices[0]];
        let path = if self.glob {
            format!("{}::*", leaf.defined_at)
        } else {
            leaf.defined_at.clone()
        };
        Ok(self.named(path, leaf))
    }

    /// The lifted path, keeping the name the member bound when the rewrite would change it.
    fn named(&self, path: String, leaf: &Rewrite) -> String {
        if self.alias.is_some() {
            return self.with_alias(path);
        }
        let was = last_segment(&leaf.written);
        if last_segment(&path) != was {
            format!("{path} as {was}")
        } else {
            path
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

/// `path` with a trailing `::{tail}` removed, or unchanged when it does not end with it.
fn strip_tail(path: &str, tail: &str) -> String {
    path.strip_suffix(&format!("::{tail}"))
        .unwrap_or(path)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_leaf(written: &str, defined_at: &str) -> Rewrite {
        Rewrite {
            written: written.to_string(),
            defined_at: defined_at.to_string(),
            line: 1,
            split_from_group: false,
        }
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
}
