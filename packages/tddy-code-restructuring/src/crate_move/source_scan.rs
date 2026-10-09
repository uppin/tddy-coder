//! A token-level reading of one Rust file: the paths it writes, and what a module declares.
//!
//! The move reads `use` trees at any depth and paths in bodies, which a line scan cannot: a path in
//! a body shares its line with everything else, and a `use` group spreads over several. So the file
//! is masked first — comments and literals blanked, byte for byte, so every offset still addresses
//! the original text — and tokenised, and what is read out of the tokens is only ever *shapes*:
//! `a::b::c` sequences, `use` trees, `mod` blocks and the attribute that marks an item `cfg(test)`.
//! Nothing here resolves a name; [`super::survey`] and [`super::reexports`] do that.
//!
//! The `cfg(test)` marker is recognised as `#[cfg(test)]` and `#[cfg(all(test, …))]`. Any other
//! spelling that only builds under test — `#[cfg(not(not(test)))]`, a `cfg_attr` — reads as ordinary
//! code, which errs the safe way: a crate such a path names lands in `[dependencies]`, never missing
//! from the build.

use super::test_binary::{readable_spans, Prose};

/// The tokens that open a `cfg` attribute: `#[cfg(`.
const CFG_OPEN: [&str; 4] = ["#", "[", "cfg", "("];
/// The whole attribute `#[cfg(test)]`.
const CFG_TEST: [&str; 7] = ["#", "[", "cfg", "(", "test", ")", "]"];
/// The keywords that introduce an item with a name, as far as a module's top level is read.
const DEFINING_KEYWORDS: [&str; 8] = [
    "fn", "struct", "enum", "trait", "type", "const", "static", "union",
];

/// What a token is, as far as path reading needs to tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Ident,
    /// `::`.
    Separator,
    Number,
    /// Any other single character — brackets, `;`, `.`, `#`, `*`, `,`.
    Punct(u8),
}

#[derive(Debug, Clone, Copy)]
struct Token {
    kind: Kind,
    start: usize,
    end: usize,
}

/// A file's tokens, over the text they were read from.
struct Scan<'a> {
    text: &'a str,
    tokens: Vec<Token>,
}

impl<'a> Scan<'a> {
    fn of(text: &'a str) -> Self {
        Scan {
            text,
            tokens: tokens_of(&masked(text)),
        }
    }

    fn text(&self, token: Token) -> &'a str {
        &self.text[token.start..token.end]
    }

    fn kind(&self, at: usize) -> Option<Kind> {
        self.tokens.get(at).map(|token| token.kind)
    }

    fn is_punct(&self, at: usize, character: u8) -> bool {
        self.kind(at) == Some(Kind::Punct(character))
    }

    /// The identifier at `at`, when it is one.
    fn ident(&self, at: usize) -> Option<&'a str> {
        let token = *self.tokens.get(at)?;
        (token.kind == Kind::Ident).then(|| self.text(token))
    }

    /// Whether the tokens from `at` spell exactly `words`.
    fn spells(&self, at: usize, words: &[&str]) -> bool {
        words.iter().enumerate().all(|(offset, word)| {
            self.tokens
                .get(at + offset)
                .is_some_and(|token| self.text(*token) == *word)
        })
    }

    /// How many tokens the attribute at `at` spans, when it marks the item after it `cfg(test)`:
    /// `#[cfg(test)]`, or `#[cfg(all(test, …))]` with `test` one of the `all`'s own arguments.
    fn cfg_test_attribute(&self, at: usize) -> Option<usize> {
        if self.spells(at, &CFG_TEST) {
            return Some(CFG_TEST.len());
        }
        if !self.spells(at, &CFG_OPEN) || !self.spells(at + CFG_OPEN.len(), &["all", "("]) {
            return None;
        }

        let arguments = at + CFG_OPEN.len() + 2;
        let mut depth = 0usize;
        let mut names_test = false;
        for index in arguments..self.tokens.len() {
            match self.kind(index)? {
                Kind::Punct(b'(' | b'[' | b'{') => depth += 1,
                Kind::Punct(b')' | b']' | b'}') if depth == 0 => {
                    let closes_the_attribute = self.is_punct(index, b')')
                        && self.is_punct(index + 1, b')')
                        && self.is_punct(index + 2, b']');
                    return (names_test && closes_the_attribute).then_some(index + 3 - at);
                }
                Kind::Punct(b')' | b']' | b'}') => depth -= 1,
                Kind::Ident if depth == 0 && self.text(self.tokens[index]) == "test" => {
                    let opens_an_argument =
                        self.is_punct(index - 1, b'(') || self.is_punct(index - 1, b',');
                    let ends_an_argument =
                        self.is_punct(index + 1, b',') || self.is_punct(index + 1, b')');
                    names_test |= opens_an_argument && ends_an_argument;
                }
                _ => {}
            }
        }
        None
    }

    /// The `use` item whose keyword is at `at`: where its tree starts as a byte offset, its leaves,
    /// and the index of the token after the item.
    fn use_item(&self, at: usize) -> (usize, Vec<UseLeaf>, usize) {
        let mut cursor = at + 1;
        let head = match self.kind(cursor) {
            Some(Kind::Separator) => self.tokens.get(cursor + 1),
            _ => self.tokens.get(cursor),
        };
        let head_at = head.map_or(self.tokens[at].end, |head| head.start);
        let mut leaves = Vec::new();
        self.use_tree(&mut cursor, Vec::new(), &mut leaves);
        (head_at, leaves, self.statement_end(cursor) + 1)
    }

    /// Whether the identifier at `at` continues what precedes it rather than opening a path: it is
    /// behind `::` a segment of a longer path, behind `.` a field or method, behind `$` a macro
    /// variable and behind `'` a lifetime.
    fn continues_a_path(&self, at: usize) -> bool {
        at > 0
            && matches!(
                self.kind(at - 1),
                Some(Kind::Separator | Kind::Punct(b'.' | b'$' | b'\''))
            )
    }

    /// The path of identifiers joined by `::` that starts at `at`, and the index of its last token.
    fn path_from(&self, at: usize) -> (Vec<String>, usize) {
        let mut segments = vec![self.text(self.tokens[at]).to_string()];
        let mut last = at;
        while self.kind(last + 1) == Some(Kind::Separator) {
            let Some(next) = self.ident(last + 2) else {
                break;
            };
            segments.push(next.to_string());
            last += 2;
        }
        (segments, last)
    }

    /// The index of the next `;` at or after `at`, or the end of the file.
    fn statement_end(&self, at: usize) -> usize {
        (at..self.tokens.len())
            .find(|index| self.is_punct(*index, b';'))
            .unwrap_or(self.tokens.len())
    }

    /// Read one `use` tree starting at token `at`, appending a leaf per path it names.
    ///
    /// `path` is what the enclosing groups have already spelled. A leaf ending in `self` names the
    /// group's own prefix, so the `self` is dropped.
    fn use_tree(&self, at: &mut usize, mut path: Vec<String>, leaves: &mut Vec<UseLeaf>) {
        if self.kind(*at) == Some(Kind::Separator) {
            *at += 1;
        }
        loop {
            let Some(token) = self.tokens.get(*at).copied() else {
                return;
            };
            match token.kind {
                Kind::Ident => {
                    path.push(self.text(token).to_string());
                    *at += 1;
                }
                Kind::Punct(b'*') => {
                    *at += 1;
                    leaves.push(UseLeaf {
                        segments: path,
                        alias: None,
                        glob: true,
                        visibility: None,
                    });
                    return;
                }
                Kind::Punct(b'{') => {
                    *at += 1;
                    self.use_group(at, &path, leaves);
                    return;
                }
                _ => return,
            }
            if self.kind(*at) == Some(Kind::Separator) {
                *at += 1;
                continue;
            }
            break;
        }

        let mut alias = None;
        if self.ident(*at) == Some("as") {
            alias = self.ident(*at + 1).map(str::to_string);
            *at += 2;
        }
        if path.len() > 1 && path.last().is_some_and(|last| last == "self") {
            path.pop();
        }
        leaves.push(UseLeaf {
            segments: path,
            alias,
            glob: false,
            visibility: None,
        });
    }

    /// The members of a `{ … }` group, each a tree of its own under `prefix`.
    fn use_group(&self, at: &mut usize, prefix: &[String], leaves: &mut Vec<UseLeaf>) {
        loop {
            if self.is_punct(*at, b'}') {
                *at += 1;
                return;
            }
            let before = *at;
            self.use_tree(at, prefix.to_vec(), leaves);
            if self.is_punct(*at, b',') {
                *at += 1;
            } else if self.is_punct(*at, b'}') {
                *at += 1;
                return;
            } else if *at == before {
                return;
            }
        }
    }
}

/// One path a `use` item names, with its groups expanded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UseLeaf {
    /// The segments, outermost first, with a trailing `*` left out.
    pub(crate) segments: Vec<String>,
    pub(crate) alias: Option<String>,
    pub(crate) glob: bool,
    /// The `pub…` text of the `use` item that holds the leaf (`pub`, `pub(crate)`, `pub(in crate::a)`),
    /// `None` for a private one. Read only by [`items_of_module`]; every other reader of a `use` tree
    /// leaves it `None`.
    #[allow(
        dead_code,
        reason = "TODO(reshape-move-item-paths): read by `bindings::import_target` (rule R3) at green"
    )]
    pub(crate) visibility: Option<String>,
}

impl UseLeaf {
    /// The name this leaf brings into scope: its alias, else its last segment. A glob binds none
    /// this scan can name.
    pub(crate) fn bound_name(&self) -> Option<&str> {
        if self.glob {
            return None;
        }
        match self.alias.as_deref() {
            Some("_") => None,
            Some(alias) => Some(alias),
            None => self.segments.last().map(String::as_str),
        }
    }
}

/// `text` with every comment and literal blanked and every other byte kept where it was.
fn masked(text: &str) -> Vec<u8> {
    let mut bytes: Vec<u8> = text
        .bytes()
        .map(|byte| if byte == b'\n' { byte } else { b' ' })
        .collect();
    for (span, prose) in readable_spans(text) {
        if prose == Prose::Code {
            bytes[span.clone()].copy_from_slice(&text.as_bytes()[span]);
        }
    }
    bytes
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80
}

fn tokens_of(masked: &[u8]) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut at = 0usize;

    while at < masked.len() {
        let byte = masked[at];
        if byte.is_ascii_whitespace() {
            at += 1;
            continue;
        }
        let (kind, length) = if is_identifier_byte(byte) {
            let length = masked[at..]
                .iter()
                .take_while(|byte| is_identifier_byte(**byte))
                .count();
            let kind = if byte.is_ascii_digit() {
                Kind::Number
            } else {
                Kind::Ident
            };
            (kind, length)
        } else if masked[at..].starts_with(b"::") {
            (Kind::Separator, 2)
        } else {
            (Kind::Punct(byte), 1)
        };
        tokens.push(Token {
            kind,
            start: at,
            end: at + length,
        });
        at += length;
    }
    tokens
}

mod sighting_walk;
pub(crate) use sighting_walk::sightings;

mod module_items;
pub(crate) use module_items::{items_of_module, ChildModule, ModuleItems};

#[cfg(test)]
mod tests {
    use super::*;

    fn written(sightings: &[sighting_walk::Sighting]) -> Vec<String> {
        sightings
            .iter()
            .map(|sighting| sighting.segments.join("::"))
            .collect()
    }

    #[test]
    fn a_path_in_a_body_is_sighted_beside_one_in_a_use() {
        // Given a file naming `shared` in a `use` and in a function body
        let text = "use shared::Clock;\n\npub fn now() -> u64 {\n    shared::tick()\n}\n";

        // When
        let found = sighting_walk::sightings(text);

        // Then both are paths, the first in a `use` and the second not
        assert_eq!(written(&found), vec!["shared::Clock", "shared::tick"]);
        assert_eq!(
            found.iter().map(|path| path.in_use).collect::<Vec<_>>(),
            vec![true, false]
        );
    }

    #[test]
    fn a_use_group_is_expanded_into_one_path_per_member_sharing_a_head() {
        // Given a group nested one level deep
        let text = "use crate::{runtime::Clock, config::{self, Settings}};\n";

        // When
        let found = sighting_walk::sightings(text);

        // Then each member is its own path and all start where the tree does
        assert_eq!(
            written(&found),
            vec![
                "crate::runtime::Clock",
                "crate::config",
                "crate::config::Settings"
            ]
        );
        assert!(found.iter().all(|path| path.head_at == "use ".len()));
    }

    #[test]
    fn paths_in_comments_and_strings_are_not_sighted() {
        // Given a crate named only in prose, and in a string
        let text = "// shared::Clock\n/* shared::Tick */\nfn f() -> &'static str {\n    \"shared::Name\"\n}\n";

        // When
        let found = sighting_walk::sightings(text);

        // Then nothing is sighted
        assert_eq!(written(&found), Vec::<String>::new());
    }

    #[test]
    fn a_path_under_cfg_test_is_marked_and_the_one_after_it_is_not() {
        // Given a test module followed by ordinary code
        let text = "#[cfg(test)]\nmod tests {\n    fn a() {\n        shared::one();\n    }\n}\n\nfn b() {\n    shared::two();\n}\n";

        // When
        let found = sighting_walk::sightings(text);

        // Then only the first is in a test, and it records the module it sits in
        assert_eq!(written(&found), vec!["shared::one", "shared::two"]);
        assert_eq!(
            found.iter().map(|path| path.in_test).collect::<Vec<_>>(),
            vec![true, false]
        );
        assert_eq!(found[0].modules, vec!["tests".to_string()]);
        assert!(found[1].modules.is_empty());
    }

    #[test]
    fn a_path_continuing_another_or_following_a_dot_is_not_a_head() {
        // Given a method call and a longer path whose middle segment is a crate's name
        let text = "fn f() {\n    value.shared::<u8>();\n    outer::shared::item();\n}\n";

        // When
        let found = sighting_walk::sightings(text);

        // Then only the path that opens one is sighted
        assert_eq!(written(&found), vec!["outer::shared::item"]);
    }

    #[test]
    fn a_module_lists_what_it_declares_at_its_top_level_only() {
        // Given a module with an inline child, a file child, a `use`, and a nested definition
        let text = "pub mod inline {\n    pub fn hidden() {}\n}\nmod filed;\npub use other::Thing as Renamed;\npub fn open() {}\n";

        // When
        let items = module_items::items_of_module(text);

        // Then the nested function is not the module's own
        assert_eq!(
            items
                .children
                .iter()
                .map(|child| (child.name.as_str(), child.body.is_some()))
                .collect::<Vec<_>>(),
            vec![("inline", true), ("filed", false)]
        );
        assert_eq!(items.defined, vec!["open".to_string()]);
        assert_eq!(items.uses[0].bound_name(), Some("Renamed"));
    }

    #[test]
    fn a_const_fn_defines_its_own_name_and_not_the_keyword() {
        // Given a module with a `const fn` and a `const`
        let text = "pub const fn limit() -> u32 {\n    3\n}\npub const MAX: u32 = 4;\n";

        // When
        let items = module_items::items_of_module(text);

        // Then the names defined are the function's and the constant's
        assert_eq!(items.defined, vec!["limit".to_string(), "MAX".to_string()]);
    }

    #[test]
    fn a_path_under_cfg_all_test_is_marked_and_one_under_cfg_any_test_is_not() {
        // Given an item gated by `all(test, …)` and one gated by `any(test, …)`
        let text = "#[cfg(all(test, feature = \"slow\"))]\nmod slow {\n    fn a() {\n        shared::one();\n    }\n}\n\n#[cfg(any(test, unix))]\nmod either {\n    fn b() {\n        shared::two();\n    }\n}\n";

        // When
        let found = sighting_walk::sightings(text);

        // Then only the first is in a test
        assert_eq!(written(&found), vec!["shared::one", "shared::two"]);
        assert_eq!(
            found.iter().map(|path| path.in_test).collect::<Vec<_>>(),
            vec![true, false]
        );
    }

    #[test]
    fn reads_a_path_after_a_struct_update_or_a_range_operator_as_a_path() {
        // Given a struct update, an exclusive range and an inclusive range, each followed by a
        // crate-rooted path, beside a method called on a field
        let text = "fn f(state: State) -> Meta {\n    \
                    let _ = state.registry.len();\n    \
                    let _ = 0..crate::limits::MAX;\n    \
                    let _ = 1..=crate::limits::MIN;\n    \
                    Meta {\n        id: 1,\n        \
                    ..crate::connection_service::starting_session_metadata()\n    }\n}\n";

        // When
        let found = sighting_walk::sightings(text);

        // Then each path after `..` and `..=` is sighted, and the field access is not
        assert_eq!(
            written(&found),
            vec![
                "crate::limits::MAX",
                "crate::limits::MIN",
                "crate::connection_service::starting_session_metadata",
            ]
        );
    }
}
