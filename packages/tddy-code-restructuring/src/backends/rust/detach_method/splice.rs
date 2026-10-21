//! The edits a detach makes to one file, and how they compose.
//!
//! A call is rewritten from its receiver and its arguments, which may hold further edits (a call
//! inside an argument, a `self` inside a receiver), so an edit is not always a plain replacement of
//! its span. [`render`] writes a span with every edit inside it applied: the outermost edit wins
//! its span, and it asks [`render`] for the text it reads from. Two edits that overlap without
//! nesting cannot both be honoured, and the survey never builds one.

use std::ops::Range;

use super::calls::Lend;

/// One edit over the original text of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Splice {
    /// `span` replaced by `text`.
    Text { span: Range<usize>, text: String },
    /// A call re-pointed: `receiver.method(args)` over `span` becomes
    /// `written method(BORROW receiver, args)`, the receiver and the arguments rendered in turn.
    Call {
        span: Range<usize>,
        written: String,
        method: String,
        lend: Lend,
        receiver: Range<usize>,
        args: Option<Range<usize>>,
    },
}

impl Splice {
    /// The span of the original text this edit replaces.
    pub(super) fn span(&self) -> &Range<usize> {
        match self {
            Splice::Text { span, .. } | Splice::Call { span, .. } => span,
        }
    }

    /// What this edit writes in place of its span, with the edits inside it applied.
    fn rendered(&self, text: &str, splices: &[Splice]) -> String {
        match self {
            Splice::Text { text, .. } => text.clone(),
            Splice::Call {
                written,
                method,
                lend,
                receiver,
                args,
                ..
            } => {
                let borrow = match lend {
                    Lend::Pass => "",
                    Lend::Borrow { mutable: true } => "&mut ",
                    Lend::Borrow { mutable: false } => "&",
                };
                let mut out = format!(
                    "{written}{method}({borrow}{}",
                    render(text, receiver.clone(), splices)
                );
                if let Some(args) = args {
                    // Arguments keep their own bytes; a list wrapped onto its own lines keeps its
                    // line break rather than a space after the comma.
                    let separator = if text[args.clone()].starts_with('\n') {
                        ","
                    } else {
                        ", "
                    };
                    out.push_str(separator);
                    out.push_str(&render(text, args.clone(), splices));
                }
                out.push(')');
                out
            }
        }
    }
}

/// `text[span]` with every edit of `splices` that lies within `span` applied.
pub(super) fn render(text: &str, span: Range<usize>, splices: &[Splice]) -> String {
    let mut inside: Vec<&Splice> = splices
        .iter()
        .filter(|splice| splice.span().start >= span.start && splice.span().end <= span.end)
        .collect();
    // Outermost first at a shared start; an insertion before a replacement that begins where it
    // does.
    inside.sort_by_key(|splice| {
        let edge = splice.span();
        (
            edge.start,
            edge.end != edge.start,
            std::cmp::Reverse(edge.end),
        )
    });

    let mut out = String::with_capacity(span.len());
    let mut cursor = span.start;
    for splice in inside {
        let edge = splice.span();
        if edge.start < cursor {
            continue;
        }
        out.push_str(&text[cursor..edge.start]);
        out.push_str(&splice.rendered(text, splices));
        cursor = edge.end;
    }
    out.push_str(&text[cursor..span.end]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_inside_an_argument_is_rendered_inside_the_outer_call() {
        // Given `h.bump(h.total())`, with both calls re-pointed
        let text = "h.bump(h.total())";
        let inner = Splice::Call {
            span: 7..16,
            written: String::new(),
            method: "total".to_string(),
            lend: Lend::Pass,
            receiver: 7..8,
            args: None,
        };
        let outer = Splice::Call {
            span: 0..17,
            written: "host::".to_string(),
            method: "bump".to_string(),
            lend: Lend::Pass,
            receiver: 0..1,
            args: Some(7..16),
        };

        let rendered = render(text, 0..text.len(), &[inner, outer]);

        assert_eq!(rendered, "host::bump(h, total(h))");
    }

    #[test]
    fn an_insertion_at_the_start_of_a_replacement_comes_before_it() {
        let text = "ab";
        let splices = [
            Splice::Text {
                span: 0..1,
                text: "X".to_string(),
            },
            Splice::Text {
                span: 0..0,
                text: "I".to_string(),
            },
        ];

        assert_eq!(render(text, 0..2, &splices), "IXb");
    }
}
