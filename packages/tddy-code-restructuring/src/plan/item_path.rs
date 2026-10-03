use serde::{Deserialize, Serialize};

use super::malformed;

use crate::Result;

/// A crate-rooted path to an item: the crate's name, its module path, then the item and member
/// segments — `tddy_core::workflow::Stack::new`.
///
/// A member of a trait impl that shares its name with another member of the same type is addressed
/// with the trait named, the way Rust's own qualified path does: `tddy_core::workflow::<Stack as
/// Display>::fmt`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ItemPath(String);

/// One step of an [`ItemPath`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemSegment {
    /// A module, type, function or member, by name.
    Named(String),
    /// A member reached through one trait impl of a type: `<Stack as Display>`.
    TraitImpl {
        self_type: String,
        trait_name: String,
    },
    /// The inherent `impl` block(s) of a type: `<Stack>`, or `<Stack>#2` for the second in source
    /// order. The type is written as the block writes it, generic arguments included.
    InherentImpl {
        self_type: String,
        nth: Option<usize>,
    },
}

impl ItemPath {
    /// Parse and validate a path; a path with fewer than two segments names no item in a crate.
    pub fn parse(text: &str) -> Result<ItemPath> {
        let pieces = split_path(text).filter(|pieces| {
            pieces.len() >= 2
                && matches!(read_segment(pieces[0]), Some(ItemSegment::Named(_)))
                && pieces.iter().all(|piece| read_segment(piece).is_some())
        });
        if pieces.is_none() {
            let last = text.rsplit("::").next().unwrap_or(text);
            return Err(malformed(format!(
                "`{text}` is not an item path — write it crate-rooted, as \
                 `<crate>::<module>::{last}`"
            )));
        }
        Ok(ItemPath(text.to_string()))
    }

    /// The crate the path is rooted in.
    pub fn crate_name(&self) -> &str {
        self.pieces()[0]
    }

    /// Every segment after the crate's.
    pub fn segments(&self) -> Vec<ItemSegment> {
        self.pieces()
            .into_iter()
            .skip(1)
            .filter_map(read_segment)
            .collect()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The path's pieces, which [`ItemPath::parse`] has already shown to be well formed.
    pub(crate) fn pieces(&self) -> Vec<&str> {
        split_path(&self.0).unwrap_or_default()
    }
}

impl ItemSegment {
    /// The segment as a path spells it: `Stack`, or `<Stack as Debug>`.
    pub fn spelled(&self) -> String {
        match self {
            ItemSegment::Named(name) => name.clone(),
            ItemSegment::TraitImpl {
                self_type,
                trait_name,
            } => format!("<{self_type} as {trait_name}>"),
            ItemSegment::InherentImpl { self_type, nth } => match nth {
                Some(nth) => format!("<{self_type}>#{nth}"),
                None => format!("<{self_type}>"),
            },
        }
    }
}

/// `text` split at every `::` outside angle brackets, or `None` when the brackets do not balance
/// or a piece is empty.
pub(crate) fn split_path(text: &str) -> Option<Vec<&str>> {
    let mut pieces = Vec::new();
    let mut depth = 0usize;
    let mut from = 0usize;
    let bytes = text.as_bytes();
    let mut at = 0usize;
    while at < bytes.len() {
        match bytes[at] {
            b'<' => depth += 1,
            b'>' => depth = depth.checked_sub(1)?,
            b':' if depth == 0 && bytes.get(at + 1) == Some(&b':') => {
                pieces.push(&text[from..at]);
                at += 1;
                from = at + 1;
            }
            _ => {}
        }
        at += 1;
    }
    if depth != 0 {
        return None;
    }
    pieces.push(&text[from..]);
    Some(pieces)
}

/// One piece of an item path as a segment: a plain name, a `<Type as Trait>` qualification, or a
/// `<Type>` / `<Type>#N` inherent impl.
fn read_segment(piece: &str) -> Option<ItemSegment> {
    if !piece.starts_with('<') {
        return plain_name(piece).then(|| ItemSegment::Named(piece.to_string()));
    }
    let (bracketed, nth) = split_ordinal(piece)?;
    let inside = bracketed.strip_prefix('<')?.strip_suffix('>')?;
    match find_trait_separator(inside) {
        Some(at) if nth.is_none() => read_trait_impl(&inside[..at], &inside[at + " as ".len()..]),
        Some(_) => None,
        None => read_inherent_impl(inside, nth),
    }
}

fn plain_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | ':'))
}

/// `piece` without a trailing `#N` ordinal, and the ordinal: absent, or a number of at least one.
/// `None` when a `#` is there but no such number follows it.
fn split_ordinal(piece: &str) -> Option<(&str, Option<usize>)> {
    let (bracketed, ordinal) = piece.split_at(piece.rfind('>')? + 1);
    if ordinal.is_empty() {
        return Some((bracketed, None));
    }
    let nth: usize = ordinal.strip_prefix('#')?.parse().ok()?;
    (nth >= 1).then_some((bracketed, Some(nth)))
}

/// Where ` as ` separates a self type from a trait, outside any nested angle brackets.
fn find_trait_separator(inside: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (at, character) in inside.char_indices() {
        match character {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ' ' if depth == 0 && inside[at..].starts_with(" as ") => return Some(at),
            _ => {}
        }
    }
    None
}

fn read_trait_impl(self_type: &str, trait_name: &str) -> Option<ItemSegment> {
    let (self_type, trait_name) = (self_type.trim(), trait_name.trim());
    (!self_type.is_empty() && !trait_name.is_empty()).then(|| ItemSegment::TraitImpl {
        self_type: self_type.to_string(),
        trait_name: trait_name.to_string(),
    })
}

fn read_inherent_impl(self_type: &str, nth: Option<usize>) -> Option<ItemSegment> {
    let self_type = self_type.trim();
    (!self_type.is_empty()).then(|| ItemSegment::InherentImpl {
        self_type: self_type.to_string(),
        nth,
    })
}

impl TryFrom<String> for ItemPath {
    type Error = crate::RestructureError;

    fn try_from(text: String) -> Result<ItemPath> {
        ItemPath::parse(&text)
    }
}

impl From<ItemPath> for String {
    fn from(path: ItemPath) -> String {
        path.0
    }
}

impl std::fmt::Display for ItemPath {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// `sha256:<hex>` of an item's text: every line the item's full range touches, whole — leading
/// indentation, outer attributes and doc comments included — joined by `\n`, without a trailing
/// newline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Fingerprint(pub String);

impl Fingerprint {
    /// The fingerprint of `text`, exactly as the anchored item reads.
    pub fn of(text: &str) -> Fingerprint {
        use sha2::{Digest, Sha256};
        Fingerprint(format!("sha256:{:x}", Sha256::digest(text.as_bytes())))
    }
}
