//! Rewrites `avt`'s screen dump into the dialect every client accepts, and keeps cells the
//! application never wrote empty.
//!
//! `avt` cannot tell a blank cell the application wrote a space into from one it never touched, so
//! its dump prints a space for every blank cell. A client that distinguishes the two (copy and
//! select do, and so does the reference emulator the snapshot tests compare against) would show
//! the reconnected screen as written where it was not. The dump therefore skips default-pen
//! blanks with cursor-forward instead of printing them, except a lone blank between two pieces of
//! content, which is far more likely a word gap than an untouched cell.
//!
//! Blanks with a non-default pen are never skipped: their colour is visible.

/// One lexical piece of a dump.
enum Item {
    /// A printable character.
    Text(char),
    /// `CSI <n> b` — repeat the previous printable character `n` more times.
    Repeat(usize),
    /// `CSI … m` — a pen change.
    Pen(String),
    /// Any other escape sequence or control character.
    Other(String),
    /// `CR LF` — the end of an unwrapped row.
    RowEnd,
}

/// Rewrite a dump for a grid `cols` wide. See the module documentation.
///
/// The dump is cut into runs of content (text, repeats and pen changes) at every row end and every
/// other escape sequence, because those move the cursor: what is leading or trailing is decided
/// within a run, and a run is rewritten only when it cannot span two rows.
pub(crate) fn portable_dump(dump: &str, cols: usize) -> String {
    let items = lex(&dump.replace('\u{9b}', "\x1b["));
    let mut out = String::with_capacity(dump.len());
    let mut probe = Probe::new();
    for run in items.split_inclusive(|item| matches!(item, Item::RowEnd | Item::Other(_))) {
        let (content, barrier) = match run.split_last() {
            Some((barrier @ (Item::RowEnd | Item::Other(_)), content)) => (content, Some(barrier)),
            _ => (run, None),
        };
        if spans_rows(content, cols, &mut probe) {
            content.iter().for_each(|item| write_item(&mut out, item));
            content.iter().for_each(|item| probe.observe(item));
        } else {
            write_segment(&mut out, content, &mut probe);
        }
        if let Some(barrier) = barrier {
            write_item(&mut out, barrier);
        }
    }
    out.replace("\x1b[?1047h", "\x1b[?1049h")
        .replace("\x1b[?1047l", "\x1b[?1049l")
}

fn lex(text: &str) -> Vec<Item> {
    let mut items = Vec::new();
    let mut chars = text.chars().peekable();
    // A `while let`: the body peeks at `chars`, which a `for` over it would keep borrowed.
    #[allow(clippy::while_let_on_iterator)]
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => items.push(lex_escape(&mut chars)),
            '\r' if chars.peek() == Some(&'\n') => {
                chars.next();
                items.push(Item::RowEnd);
            }
            c if c >= ' ' && c != '\x7f' => items.push(Item::Text(c)),
            c => items.push(Item::Other(c.to_string())),
        }
    }
    items
}

/// Read the rest of an escape sequence whose `ESC` was just consumed.
fn lex_escape(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Item {
    let mut sequence = String::from('\x1b');
    let csi = chars.peek() == Some(&'[');
    for c in chars.by_ref() {
        sequence.push(c);
        let final_byte = if csi {
            sequence.len() > 2 && ('\x40'..='\x7e').contains(&c)
        } else {
            !('\x20'..='\x2f').contains(&c)
        };
        if final_byte {
            break;
        }
    }
    classify_escape(sequence)
}

fn classify_escape(sequence: String) -> Item {
    let Some(body) = sequence.strip_prefix("\x1b[") else {
        return Item::Other(sequence);
    };
    if body.ends_with('m') {
        return Item::Pen(sequence);
    }
    match body.strip_suffix('b').map(str::parse::<usize>) {
        Some(Ok(count)) => Item::Repeat(count),
        _ => Item::Other(sequence),
    }
}

fn write_item(out: &mut String, item: &Item) {
    match item {
        Item::Text(c) => out.push(*c),
        Item::Repeat(count) => out.push_str(&format!("\x1b[{count}b")),
        Item::Pen(sequence) | Item::Other(sequence) => out.push_str(sequence),
        Item::RowEnd => out.push_str("\r\n"),
    }
}

/// Whether the cells of a segment fill more than one row, in which case a row break inside it is a
/// wrap and skipping cells would move content across it.
fn spans_rows(segment: &[Item], cols: usize, probe: &mut Probe) -> bool {
    let mut cells = 0usize;
    let mut width = 0usize;
    for item in segment {
        match item {
            Item::Text(c) => {
                width = probe.width(*c);
                cells += width;
            }
            Item::Repeat(count) => cells += count * width,
            _ => {}
        }
    }
    cells > cols
}

/// Lets `avt` answer the two questions the rewrite asks of the dump's own content: whether the pen
/// the dump has set is the default one, and how many columns a character takes.
struct Probe {
    vt: avt::Vt,
}

impl Probe {
    fn new() -> Self {
        Self {
            vt: avt::Vt::new(4, 1),
        }
    }

    /// Feed the dump's pen changes through, so [`Self::is_default_pen`] follows them.
    fn observe(&mut self, item: &Item) {
        if let Item::Pen(sequence) = item {
            self.vt.feed_str(sequence);
        }
    }

    fn is_default_pen(&mut self) -> bool {
        self.first_cell_after(" ")
            .is_some_and(avt::Cell::is_default)
    }

    fn width(&mut self, c: char) -> usize {
        if c.is_ascii() {
            return 1;
        }
        self.first_cell_after(c.encode_utf8(&mut [0; 4]))
            .map_or(1, |cell| usize::from(cell.width()).max(1))
    }

    fn first_cell_after(&mut self, text: &str) -> Option<&avt::Cell> {
        self.vt.feed_str("\x1b[H");
        self.vt.feed_str(text);
        self.vt.feed_str("\x1b[H");
        self.vt.line(0).cells().first()
    }
}

/// Write one unwrapped row.
fn write_segment(out: &mut String, segment: &[Item], probe: &mut Probe) {
    let mut at = 0;
    let mut content_before = false;
    while at < segment.len() {
        let (blanks, next) = default_blank_run(segment, at, probe);
        if blanks > 0 {
            let trailing = only_pen_changes(&segment[next..]);
            write_blank_run(out, blanks, content_before && !trailing, trailing);
            at = next;
            continue;
        }
        let item = &segment[at];
        write_item(out, item);
        probe.observe(item);
        content_before |= matches!(item, Item::Text(_) | Item::Repeat(_));
        at += 1;
    }
}

/// The count of default-pen blanks starting at `at` (a space and its repeats), and the index after
/// them. `0` when the item there is not such a blank.
fn default_blank_run(segment: &[Item], at: usize, probe: &mut Probe) -> (usize, usize) {
    if !matches!(segment[at], Item::Text(' ')) || !probe.is_default_pen() {
        return (0, at);
    }
    let mut blanks = 1;
    let mut next = at + 1;
    while let Some(Item::Repeat(count)) = segment.get(next) {
        blanks += count;
        next += 1;
    }
    while matches!(segment.get(next), Some(Item::Text(' '))) {
        blanks += 1;
        next += 1;
        while let Some(Item::Repeat(count)) = segment.get(next) {
            blanks += count;
            next += 1;
        }
    }
    (blanks, next)
}

fn only_pen_changes(rest: &[Item]) -> bool {
    rest.iter().all(|item| matches!(item, Item::Pen(_)))
}

fn write_blank_run(out: &mut String, blanks: usize, between_content: bool, trailing: bool) {
    if trailing {
        return;
    }
    if between_content && blanks == 1 {
        out.push(' ');
    } else {
        out.push_str(&format!("\x1b[{blanks}C"));
    }
}

#[cfg(test)]
mod tests {
    use super::portable_dump;

    #[test]
    fn leading_blanks_are_skipped_and_trailing_ones_dropped() {
        assert_eq!(portable_dump("   ab  \r\n", 10), "\x1b[3Cab\r\n");
    }

    #[test]
    fn a_lone_blank_between_content_stays_a_space() {
        assert_eq!(portable_dump("a b", 10), "a b");
    }

    #[test]
    fn a_longer_blank_run_between_content_is_skipped() {
        assert_eq!(portable_dump("a  b", 10), "a\x1b[2Cb");
    }

    #[test]
    fn blanks_with_a_colour_are_kept() {
        let dump = "\x1b[41m  \x1b[0m";
        assert_eq!(portable_dump(dump, 10), dump);
    }

    #[test]
    fn a_run_that_may_span_two_rows_is_left_alone() {
        assert_eq!(portable_dump("ab  cd  ", 4), "ab  cd  ");
    }

    #[test]
    fn the_screen_switch_is_the_one_every_client_knows() {
        assert_eq!(
            portable_dump("\u{9b}?1047hx\u{9b}?1047l", 10),
            "\x1b[?1049hx\x1b[?1049l"
        );
    }
}
