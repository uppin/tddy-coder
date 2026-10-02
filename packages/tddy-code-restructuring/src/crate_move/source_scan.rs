//! A token-level reading of one Rust file: the paths it writes, and what a module declares.
//!
//! The move reads `use` trees at any depth and paths in bodies, which a line scan cannot: a path in
//! a body shares its line with everything else, and a `use` group spreads over several. So the file
//! is masked first — comments and literals blanked, byte for byte, so every offset still addresses
//! the original text — and tokenised, and what is read out of the tokens is only ever *shapes*:
//! `a::b::c` sequences, `use` trees, `mod` blocks and the attribute that marks an item `cfg(test)`.
//! Nothing here resolves a name; [`super::survey`] and [`super::reexports`] do that.

use std::ops::Range;

use super::test_binary::{readable_spans, Prose};

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

/// One path the file writes: a `use` leaf, or a path in code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Sighting {
    /// The segments as written, with a glob's `*` left out.
    pub(crate) segments: Vec<String>,
    /// Where the path starts, as a byte offset. Every leaf of one `use` tree shares it.
    pub(crate) head_at: usize,
    pub(crate) in_use: bool,
    /// Whether it is written under a `#[cfg(test)]` item.
    pub(crate) in_test: bool,
    /// The inline modules enclosing it, outermost first.
    pub(crate) modules: Vec<String>,
}

/// An open `{ … }` block.
struct Frame {
    /// The name, when the block is an inline `mod`.
    module: Option<String>,
    test: bool,
    /// Open brackets of the enclosing block, restored when this one closes.
    outer_brackets: usize,
}

/// Every path of two or more segments the file writes, in the order it writes them.
///
/// A path in code opens a path only when what precedes it does not continue one: behind `::` it is a
/// segment of a longer path, behind `.` a field or method, behind `$` a macro variable and behind
/// `'` a lifetime.
pub(crate) fn sightings(text: &str) -> Vec<Sighting> {
    let scan = Scan::of(text);
    let mut found = Vec::new();
    let mut frames: Vec<Frame> = Vec::new();
    let mut pending_test = false;
    let mut brackets = 0usize;
    let mut module_block: Option<(usize, String)> = None;
    let mut at = 0usize;

    while at < scan.tokens.len() {
        let token = scan.tokens[at];
        let in_test = frames.last().is_some_and(|frame| frame.test);
        let modules = || -> Vec<String> {
            frames
                .iter()
                .filter_map(|frame| frame.module.clone())
                .collect()
        };

        match token.kind {
            Kind::Punct(b'#') if scan.spells(at, &["#", "[", "cfg", "(", "test", ")", "]"]) => {
                pending_test = true;
                at += 7;
                continue;
            }
            Kind::Punct(b'(' | b'[') => brackets += 1,
            Kind::Punct(b')' | b']') => brackets = brackets.saturating_sub(1),
            Kind::Punct(b'{') => {
                let consumes_attribute = pending_test && brackets == 0;
                if consumes_attribute {
                    pending_test = false;
                }
                let module = match module_block.take() {
                    Some((opens, name)) if opens == at => Some(name),
                    other => {
                        module_block = other;
                        None
                    }
                };
                frames.push(Frame {
                    module,
                    test: in_test || consumes_attribute,
                    outer_brackets: brackets,
                });
                brackets = 0;
            }
            Kind::Punct(b'}') => {
                if let Some(frame) = frames.pop() {
                    brackets = frame.outer_brackets;
                }
            }
            Kind::Punct(b';') if brackets == 0 => pending_test = false,
            Kind::Ident if scan.text(token) == "mod" => {
                if let (Some(name), true) = (scan.ident(at + 1), scan.is_punct(at + 2, b'{')) {
                    module_block = Some((at + 2, name.to_string()));
                }
            }
            Kind::Ident if scan.text(token) == "use" => {
                let mut cursor = at + 1;
                let head = match scan.kind(cursor) {
                    Some(Kind::Separator) => scan.tokens.get(cursor + 1),
                    _ => scan.tokens.get(cursor),
                };
                let head_at = head.map_or(token.end, |head| head.start);
                let mut leaves = Vec::new();
                scan.use_tree(&mut cursor, Vec::new(), &mut leaves);

                for leaf in leaves {
                    found.push(Sighting {
                        segments: leaf.segments,
                        head_at,
                        in_use: true,
                        in_test: in_test || pending_test,
                        modules: modules(),
                    });
                }
                pending_test = false;
                at = scan.statement_end(cursor) + 1;
                continue;
            }
            Kind::Ident => {
                let continues_a_path = at > 0
                    && matches!(
                        scan.kind(at - 1),
                        Some(Kind::Separator | Kind::Punct(b'.' | b'$' | b'\''))
                    );
                if continues_a_path {
                    at += 1;
                    continue;
                }

                let mut segments = vec![scan.text(token).to_string()];
                let mut last = at;
                while scan.kind(last + 1) == Some(Kind::Separator) {
                    let Some(next) = scan.ident(last + 2) else {
                        break;
                    };
                    segments.push(next.to_string());
                    last += 2;
                }
                if segments.len() >= 2 {
                    found.push(Sighting {
                        segments,
                        head_at: token.start,
                        in_use: false,
                        in_test: in_test || pending_test,
                        modules: modules(),
                    });
                }
                at = last + 1;
                continue;
            }
            _ => {}
        }
        at += 1;
    }
    found
}

/// A `mod` a module declares, inline or backed by a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChildModule {
    pub(crate) name: String,
    /// The span between an inline module's braces; `None` for `mod name;`.
    pub(crate) body: Option<Range<usize>>,
}

/// What a module's own text declares at its top level.
#[derive(Debug, Default)]
pub(crate) struct ModuleItems {
    pub(crate) children: Vec<ChildModule>,
    pub(crate) uses: Vec<UseLeaf>,
    /// Names of the items it defines: functions, types, traits, constants, statics, macros.
    pub(crate) defined: Vec<String>,
}

/// The top level of a module's text: its `mod`s, its `use`s and the items it defines.
pub(crate) fn items_of_module(text: &str) -> ModuleItems {
    let scan = Scan::of(text);
    let mut items = ModuleItems::default();
    let mut depth = 0usize;
    let mut open_child: Option<usize> = None;
    let mut at = 0usize;

    while at < scan.tokens.len() {
        let token = scan.tokens[at];
        match token.kind {
            Kind::Punct(b'{') => depth += 1,
            Kind::Punct(b'}') => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    if let Some(child) = open_child.take() {
                        if let Some(body) = items.children[child].body.as_mut() {
                            body.end = token.start;
                        }
                    }
                }
            }
            Kind::Ident if depth == 0 => match scan.text(token) {
                "mod" => {
                    if let Some(name) = scan.ident(at + 1) {
                        let body = scan
                            .tokens
                            .get(at + 2)
                            .filter(|_| scan.is_punct(at + 2, b'{'))
                            .map(|opens| opens.end..opens.end);
                        if body.is_some() {
                            open_child = Some(items.children.len());
                        }
                        items.children.push(ChildModule {
                            name: name.to_string(),
                            body,
                        });
                    }
                }
                "use" => {
                    let mut cursor = at + 1;
                    scan.use_tree(&mut cursor, Vec::new(), &mut items.uses);
                    at = scan.statement_end(cursor) + 1;
                    continue;
                }
                "fn" | "struct" | "enum" | "trait" | "type" | "const" | "static" | "union" => {
                    items.defined.extend(scan.ident(at + 1).map(str::to_string));
                }
                "macro_rules" if scan.is_punct(at + 1, b'!') => {
                    items.defined.extend(scan.ident(at + 2).map(str::to_string));
                }
                _ => {}
            },
            _ => {}
        }
        at += 1;
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn written(sightings: &[Sighting]) -> Vec<String> {
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
        let found = sightings(text);

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
        let found = sightings(text);

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
        let found = sightings(text);

        // Then nothing is sighted
        assert_eq!(written(&found), Vec::<String>::new());
    }

    #[test]
    fn a_path_under_cfg_test_is_marked_and_the_one_after_it_is_not() {
        // Given a test module followed by ordinary code
        let text = "#[cfg(test)]\nmod tests {\n    fn a() {\n        shared::one();\n    }\n}\n\nfn b() {\n    shared::two();\n}\n";

        // When
        let found = sightings(text);

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
        let found = sightings(text);

        // Then only the path that opens one is sighted
        assert_eq!(written(&found), vec!["outer::shared::item"]);
    }

    #[test]
    fn a_module_lists_what_it_declares_at_its_top_level_only() {
        // Given a module with an inline child, a file child, a `use`, and a nested definition
        let text = "pub mod inline {\n    pub fn hidden() {}\n}\nmod filed;\npub use other::Thing as Renamed;\npub fn open() {}\n";

        // When
        let items = items_of_module(text);

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
}
