//! Measuring and cutting text by what it occupies on screen.

use std::borrow::Cow;

use unicode_width::UnicodeWidthChar;

/// Display width of `s`, ignoring SGR escapes. A plain `width()` would count
/// escape bytes as visible columns.
pub fn visible_width(s: &str) -> usize {
    let mut width = 0;
    let mut chars = s.chars();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip to the terminating byte of the sequence.
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            width += char_width(c);
        }
    }
    width
}

/// Display width of one character. Control characters count as zero.
fn char_width(c: char) -> usize {
    UnicodeWidthChar::width(c).unwrap_or(0)
}

/// `s` cut to `width` visible columns, with its escape sequences kept whole
/// and unmeasured. For text that is already coloured, which `truncate` would
/// cut through the middle of an escape. A cut string is closed with a reset so
/// its colour cannot run on into what follows.
pub fn clip_visible(s: &str, width: usize) -> Cow<'_, str> {
    if visible_width(s) <= width {
        return Cow::Borrowed(s);
    }

    let mut out = String::with_capacity(s.len());
    let mut used = 0;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            out.push(c);
            for c in chars.by_ref() {
                out.push(c);
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            let w = char_width(c);
            if used + w > width {
                break;
            }
            out.push(c);
            used += w;
        }
    }
    if out.contains('\x1b') {
        out.push_str("\x1b[0m");
    }
    Cow::Owned(out)
}

/// Cut `s` to `width` display columns, marking the cut with an ellipsis.
/// Borrows when nothing is cut, which is twice per info row per frame.
pub fn truncate(s: &str, width: usize) -> Cow<'_, str> {
    use unicode_width::UnicodeWidthStr;

    if s.width() <= width {
        Cow::Borrowed(s)
    } else {
        Cow::Owned(ellipsize(s, width))
    }
}

/// `s` cut to `width` columns ending in an ellipsis, even if it would have
/// fit. For the caller that knows something follows `s` which does not.
pub fn ellipsize(s: &str, width: usize) -> String {
    if width <= 1 {
        return "…".repeat(width);
    }

    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let w = char_width(c);
        if used + w > width - 1 {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipping_coloured_text_keeps_escapes_whole_and_closes_the_colour() {
        let s = "\x1b[31m██\x1b[32m██\x1b[0m";

        assert_eq!(clip_visible(s, 4), s, "fits: untouched");
        assert_eq!(clip_visible(s, 3), "\x1b[31m██\x1b[32m█\x1b[0m");
        // Stops at the first cell that does not fit, so the second colour is
        // never reached; the first is still closed.
        assert_eq!(clip_visible(s, 0), "\x1b[31m\x1b[0m");
        for width in 0..6 {
            assert!(visible_width(&clip_visible(s, width)) <= width, "{width}");
        }
        // Plain text is cut without a stray reset.
        assert_eq!(clip_visible("hello", 3), "hel");
    }

    #[test]
    fn ellipsize_marks_a_cut_even_when_the_text_would_fit() {
        assert_eq!(ellipsize("hello", 5), "hell…");
        assert_eq!(ellipsize("hello", 1), "…");
        assert_eq!(ellipsize("hello", 0), "");
        // And it agrees with `truncate` wherever `truncate` cuts.
        assert_eq!(ellipsize("hello", 4), truncate("hello", 4));
    }

    #[test]
    fn truncate_respects_display_width() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello", 5), "hello");
        assert_eq!(truncate("hello", 4), "hel…");
        assert_eq!(truncate("hello", 1), "…");
        assert_eq!(truncate("hello", 0), "");
    }
}
