/// Something open at the point the scan has reached.
pub(crate) enum Frame {
    /// A delimiter, and whether it opened a body of its own (a closure's, an `async` block's, a
    /// nested function's).
    Delimiter { boundary: bool },
    /// A closure's expression body, which has no delimiter to close it: it ends at the next `,`,
    /// `;` or closing delimiter at its own depth.
    ClosureExpression,
}

/// What the previous token leaves the scan expecting, which is all a `|` needs to be read by.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Previous {
    /// Nothing, an operator, an opening delimiter or a keyword: an expression may start here.
    ExpressionMayStart,
    /// An identifier, a literal, a closing delimiter or `?`: `|` here is an operator.
    ExpressionEnded,
}

pub(crate) struct Scan {
    pub(crate) frames: Vec<Frame>,
    pub(crate) previous: Previous,
    /// The depth at which `fn name` was read, whose next `{` there is that function's body.
    pub(crate) function_at: Option<usize>,
    /// The depth at which a closure's parameters ended, whose next `{` there is its body.
    pub(crate) closure_at: Option<usize>,
    /// Whether the previous token was `async` (or `async move`), whose next `{` is its block.
    pub(crate) after_async: bool,
    /// Byte offsets, within the scanned text, of each `return` that leaves the enclosing function.
    pub(crate) returns: Vec<usize>,
}

impl Default for Scan {
    fn default() -> Self {
        Self {
            frames: Vec::new(),
            previous: Previous::ExpressionMayStart,
            function_at: None,
            closure_at: None,
            after_async: false,
            returns: Vec::new(),
        }
    }
}

/// Keywords after which a value is not complete, so a `|` that follows starts a closure.
const EXPRESSION_KEYWORDS: [&str; 16] = [
    "return", "move", "async", "in", "else", "match", "if", "while", "for", "let", "break",
    "yield", "loop", "unsafe", "mut", "fn",
];

impl Scan {
    pub(crate) fn run(&mut self, code: &[u8]) {
        let mut at = 0usize;
        while at < code.len() {
            if code[at].is_ascii_whitespace() {
                at += 1;
                continue;
            }
            let after_async = std::mem::take(&mut self.after_async);
            at = self.token(code, at, after_async);
        }
    }

    /// Read the token that starts at `at`, and return where the next one may start.
    pub(crate) fn token(&mut self, code: &[u8], at: usize, after_async: bool) -> usize {
        let byte = code[at];
        if byte.is_ascii_alphabetic() || byte == b'_' || byte >= 0x80 {
            return self.identifier(code, at, after_async);
        }
        if byte.is_ascii_digit() {
            self.previous = Previous::ExpressionEnded;
            return word_end(code, at);
        }
        self.punctuation(code, at, after_async)
    }

    /// Read an identifier or keyword that starts at `at`, and return where it ends.
    pub(crate) fn identifier(&mut self, code: &[u8], at: usize, after_async: bool) -> usize {
        let end = word_end(code, at);
        let word = std::str::from_utf8(&code[at..end]).unwrap_or_default();
        // A raw identifier, `r#return`, is an identifier and never the keyword.
        if word == "r" && code.get(end) == Some(&b'#') {
            self.previous = Previous::ExpressionEnded;
            return word_end(code, end + 1);
        }
        self.word(word, code, end, after_async);
        end
    }

    /// Read the punctuation that starts at `at`, and return where the next token may start.
    pub(crate) fn punctuation(&mut self, code: &[u8], at: usize, after_async: bool) -> usize {
        match code[at] {
            // A lifetime or a label; character literals were masked to `0`.
            b'\'' => {
                self.previous = Previous::ExpressionEnded;
                return word_end(code, at + 1);
            }
            b'{' => self.open_brace(after_async),
            b'(' | b'[' => {
                self.frames.push(Frame::Delimiter { boundary: false });
                self.previous = Previous::ExpressionMayStart;
            }
            b')' | b']' | b'}' => {
                self.end_closure_expressions();
                self.frames.pop();
                self.previous = Previous::ExpressionEnded;
            }
            b';' => {
                self.end_closure_expressions();
                // A function declared without a body — a trait method's signature.
                if self.function_at == Some(self.frames.len()) {
                    self.function_at = None;
                }
                self.previous = Previous::ExpressionMayStart;
            }
            b',' => {
                self.end_closure_expressions();
                self.previous = Previous::ExpressionMayStart;
            }
            b'?' => self.previous = Previous::ExpressionEnded,
            b'|' if self.previous == Previous::ExpressionMayStart => {
                let after = if code.get(at + 1) == Some(&b'|') {
                    at + 2
                } else {
                    closure_parameters_end(code, at + 1)
                };
                self.closure_body(code, after);
                return after;
            }
            b'|' if code.get(at + 1) == Some(&b'|') => {
                self.previous = Previous::ExpressionMayStart;
                return at + 2;
            }
            _ => self.previous = Previous::ExpressionMayStart,
        }
        at + 1
    }

    /// Open a `{`, which is a body of its own when a closure, an `async` block or a nested `fn`
    /// was waiting for it at this depth.
    pub(crate) fn open_brace(&mut self, after_async: bool) {
        let depth = self.frames.len();
        let boundary =
            after_async || self.function_at == Some(depth) || self.closure_at == Some(depth);
        if boundary {
            self.function_at = None;
            self.closure_at = None;
        }
        self.frames.push(Frame::Delimiter { boundary });
        self.previous = Previous::ExpressionMayStart;
    }

    /// Read one identifier or keyword.
    pub(crate) fn word(&mut self, word: &str, code: &[u8], after: usize, after_async: bool) {
        match word {
            "return" if !self.inside_a_body_of_its_own() => {
                self.returns.push(after - word.len());
            }
            "fn" if next_is_identifier(code, after) => {
                self.function_at = Some(self.frames.len());
            }
            "async" => self.after_async = true,
            "move" => self.after_async = after_async,
            _ => {}
        }
        self.previous = if EXPRESSION_KEYWORDS.contains(&word) {
            Previous::ExpressionMayStart
        } else {
            Previous::ExpressionEnded
        };
    }

    /// After a closure's parameters: a `-> T { … }` or `{ … }` body, or an expression body.
    pub(crate) fn closure_body(&mut self, code: &[u8], after: usize) {
        let next = code[after..]
            .iter()
            .position(|byte| !byte.is_ascii_whitespace())
            .map(|offset| after + offset);
        match next.map(|at| &code[at..]) {
            Some(rest) if rest.starts_with(b"->") || rest.starts_with(b"{") => {
                self.closure_at = Some(self.frames.len());
            }
            _ => self.frames.push(Frame::ClosureExpression),
        }
        self.previous = Previous::ExpressionMayStart;
    }

    /// End every closure expression body open at the current depth.
    pub(crate) fn end_closure_expressions(&mut self) {
        while matches!(self.frames.last(), Some(Frame::ClosureExpression)) {
            self.frames.pop();
        }
    }

    pub(crate) fn inside_a_body_of_its_own(&self) -> bool {
        self.frames.iter().any(|frame| {
            matches!(
                frame,
                Frame::ClosureExpression | Frame::Delimiter { boundary: true }
            )
        })
    }
}

/// Where the identifier-like run starting at `at` ends.
pub(crate) fn word_end(code: &[u8], at: usize) -> usize {
    code[at..]
        .iter()
        .position(|byte| !(byte.is_ascii_alphanumeric() || *byte == b'_' || *byte >= 0x80))
        .map_or(code.len(), |offset| at + offset)
}

/// Whether the next token after `at` is an identifier — `fn name`, not the pointer type `fn(`.
pub(crate) fn next_is_identifier(code: &[u8], at: usize) -> bool {
    code[at..]
        .iter()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_' || *byte >= 0x80)
}

/// Just past the `|` closing a closure's parameter list, which may nest `(…)` and `[…]`.
fn closure_parameters_end(code: &[u8], from: usize) -> usize {
    let mut depth = 0usize;
    for (offset, byte) in code[from..].iter().enumerate() {
        match byte {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth = depth.saturating_sub(1),
            b'|' if depth == 0 => return from + offset + 1,
            _ => {}
        }
    }
    code.len()
}
