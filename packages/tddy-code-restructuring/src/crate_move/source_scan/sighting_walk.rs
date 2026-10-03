use super::Kind;

use super::Scan;

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
    pub(crate) module: Option<String>,
    pub(crate) test: bool,
    /// Open brackets of the enclosing block, restored when this one closes.
    pub(crate) outer_brackets: usize,
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
            Kind::Punct(b'#') => {
                if let Some(length) = scan.cfg_test_attribute(at) {
                    pending_test = true;
                    at += length;
                    continue;
                }
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
                let (head_at, leaves, next) = scan.use_item(at);
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
                at = next;
                continue;
            }
            Kind::Ident if scan.continues_a_path(at) => {}
            Kind::Ident => {
                let (segments, last) = scan.path_from(at);
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
