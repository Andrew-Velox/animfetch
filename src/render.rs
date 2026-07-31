//! Frame composition and painting.
//!
//! `compose` builds finished, coloured lines and does no I/O. `Pane` paints
//! them, rewriting only what changed. Clearing the screen every frame, which is
//! what most animated fetchers do, is the usual source of flicker.

// Anonymous import: both traits provide `write!`.
use std::borrow::Cow;
use std::fmt::Write as _;
use std::io::{self, Write};

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::color::{RESET, Rgb};
use crate::config::Config;
use crate::fetch::Item;

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

fn fg(out: &mut String, cfg: &Config, color: Rgb) {
    if cfg.color {
        color.fg(out);
    }
}

fn reset(out: &mut String, cfg: &Config) {
    if cfg.color {
        out.push_str(RESET);
    }
}

/// One row of the info pane before styling.
enum Row<'a> {
    Title(&'a str),
    Rule(usize),
    Pair { label: &'a str, value: &'a str },
    /// Pre-coloured content passed through untouched (the palette swatches).
    Raw(&'a str),
}

/// Everything that varies between frames, so `compose` takes one argument.
#[derive(Clone, Copy)]
pub struct Scene<'a> {
    /// Pre-scaled art rows for this frame.
    pub art: &'a [String],
    /// Column count the art was scaled to; the info pane starts after it.
    pub art_w: usize,
    pub title: &'a str,
    pub items: &'a [Item],
    /// Frame counter, used to scroll the gradient.
    pub phase: usize,
    /// Terminal width. Wider content is truncated; wrapping breaks the diff.
    pub width: usize,
}

/// Build the fetch pane: art on the left, info on the right. The prompt isn't
/// part of it; that lives below, in the scroll region.
pub fn compose(scene: &Scene<'_>, cfg: &Config) -> Vec<String> {
    let &Scene { art, art_w, title, items, phase, width: avail_w } = scene;

    let rows = info_rows(title, items, cfg);

    // No art means nothing to separate, so no indent.
    let info_col = if art_w == 0 { 0 } else { art_w + cfg.gap };
    let info_w = avail_w.saturating_sub(info_col);

    // Centre the shorter pane against the taller one.
    let height = art.len().max(rows.len());
    let art_top = (height - art.len()) / 2;
    let info_top = (height - rows.len()) / 2;

    let mut out = Vec::with_capacity(height + 2);

    for y in 0..height {
        // Room for the columns plus the row's SGR sequences, so no realloc.
        let mut line = String::with_capacity(avail_w + 64);

        let art_row = y.checked_sub(art_top).and_then(|i| art.get(i));
        if let Some(row) = art_row {
            paint_art_row(&mut line, row, y, height, cfg, phase);
        }

        let info_row = y.checked_sub(info_top).and_then(|i| rows.get(i));
        if let Some(row) = info_row {
            // Pad from the art's visible width; escapes occupy no columns.
            let drawn = art_row.map_or(0, |r| r.width());
            for _ in drawn..info_col {
                line.push(' ');
            }
            paint_info_row(&mut line, row, cfg, info_w);
        }

        // Trailing blanks are invisible but still cost bytes every repaint.
        line.truncate(line.trim_end().len());
        out.push(line);
    }

    out
}

fn info_rows<'a>(title: &'a str, items: &'a [Item], cfg: &Config) -> Vec<Row<'a>> {
    let mut rows = Vec::with_capacity(items.len() + 2);
    rows.push(Row::Title(title));
    rows.push(Row::Rule(title.width()));

    for item in items {
        if item.label.is_empty() {
            // Swatches are pure decoration; nothing to show without colour.
            if cfg.color {
                rows.push(Row::Raw(&item.value));
            }
        } else {
            rows.push(Row::Pair { label: item.label, value: &item.value });
        }
    }
    rows
}

/// Colour one art row from the vertical gradient.
fn paint_art_row(out: &mut String, row: &str, y: usize, height: usize, cfg: &Config, phase: usize) {
    if row.is_empty() {
        return;
    }

    if !cfg.color || cfg.gradient.is_empty() {
        out.push_str(row);
        return;
    }

    // Advanced by the frame counter so the ramp flows down through the art.
    let span = height.max(1);
    let offset = if cfg.gradient_scroll { phase } else { 0 };
    let t = ((y + offset) % span) as f32 / span as f32;

    if let Some(color) = cfg.gradient.sample(t) {
        color.fg(out);
        out.push_str(row);
        out.push_str(RESET);
    } else {
        out.push_str(row);
    }
}

fn paint_info_row(out: &mut String, row: &Row<'_>, cfg: &Config, width: usize) {
    if width == 0 {
        return;
    }

    match *row {
        Row::Title(title) => {
            fg(out, cfg, cfg.accent.0);
            out.push_str(&truncate(title, width));
            reset(out, cfg);
        }
        Row::Rule(len) => {
            fg(out, cfg, cfg.accent.0);
            out.extend(std::iter::repeat_n('─', len.min(width)));
            reset(out, cfg);
        }
        Row::Pair { label, value } => {
            fg(out, cfg, cfg.accent.0);
            out.push_str(&truncate(label, width));
            reset(out, cfg);

            // Reserve the label and its separator before truncating the value.
            let used = label.width().min(width) + 2;
            if let Some(room) = width.checked_sub(used).filter(|r| *r > 0) {
                out.push_str(": ");
                fg(out, cfg, cfg.value.0);
                out.push_str(&truncate(value, room));
                reset(out, cfg);
            }
        }
        Row::Raw(raw) => out.push_str(raw),
    }
}

/// Cut `s` to `width` display columns, marking the cut with an ellipsis.
/// Borrows when nothing is cut, which is twice per info row per frame.
fn truncate(s: &str, width: usize) -> Cow<'_, str> {
    if s.width() <= width {
        return Cow::Borrowed(s);
    }
    if width <= 1 {
        return Cow::Owned("…".repeat(width));
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
    Cow::Owned(out)
}

/// Incremental painter for the pinned pane. Rewrites only changed lines and
/// leaves the cursor where it found it, since the user is typing below.
pub struct Pane {
    prev: Vec<String>,
    needs_clear: bool,
    /// Origin mode makes rows relative to the scroll region, which would put
    /// the art inside it. When set, each frame turns it off and back on.
    origin_mode: bool,
}

/// Save/restore cursor position and attributes (DECSC/DECRC).
const SAVE_CURSOR: &str = "\x1b7";
const RESTORE_CURSOR: &str = "\x1b8";

/// Synchronized output: the frame lands in one go, so it cannot tear.
const BEGIN_SYNC: &str = "\x1b[?2026h";
const END_SYNC: &str = "\x1b[?2026l";

impl Pane {
    pub fn new() -> Self {
        Self { prev: Vec::new(), needs_clear: true, origin_mode: false }
    }

    /// Declare that the terminal is in origin mode, so painting can opt out of
    /// it for the duration of each frame.
    pub fn with_origin_mode(mut self) -> Self {
        self.origin_mode = true;
        self
    }

    /// Force a full repaint, after a resize or a child that drew over us.
    pub fn invalidate(&mut self) {
        self.needs_clear = true;
    }

    /// Draw `lines` into rows `1..=height`, as one write so nothing can
    /// interleave with it.
    pub fn paint(&mut self, out: &mut impl Write, lines: &[String], height: usize) -> io::Result<()> {
        let visible = lines.len().min(height);
        let mut buf = String::with_capacity(visible * 96);

        buf.push_str(BEGIN_SYNC);
        buf.push_str(SAVE_CURSOR);
        if self.origin_mode {
            buf.push_str(crate::term::RESET_ORIGIN_MODE);
        }

        if self.needs_clear {
            // Only our rows; a full clear would take the output below too.
            for y in 0..height {
                let _ = write!(buf, "\x1b[{};1H\x1b[2K", y + 1);
            }
            self.prev.clear();
            self.needs_clear = false;
        }

        for (y, line) in lines[..visible].iter().enumerate() {
            if self.prev.get(y).is_some_and(|p| p == line) {
                continue;
            }
            // Absolute per line; never trust where the last write left it.
            let _ = write!(buf, "\x1b[{};1H\x1b[2K{line}", y + 1);
        }

        // The frame shrank: erase the rows the previous one occupied.
        for y in visible..self.prev.len().min(height) {
            let _ = write!(buf, "\x1b[{};1H\x1b[2K", y + 1);
        }

        if self.origin_mode {
            buf.push_str(crate::term::SET_ORIGIN_MODE);
        }
        buf.push_str(RESTORE_CURSOR);
        buf.push_str(END_SYNC);

        self.prev.clear();
        self.prev.extend_from_slice(&lines[..visible]);

        out.write_all(buf.as_bytes())?;
        out.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{Color, Gradient, Rgb};

    fn plain_cfg() -> Config {
        Config {
            gradient: Gradient::default(),
            gradient_scroll: false,
            accent: Color(Rgb(0, 0, 0)),
            value: Color(Rgb(0, 0, 0)),
            gap: 2,
            ..Config::default()
        }
    }

    fn scene<'a>(
        art: &'a [String],
        art_w: usize,
        title: &'a str,
        items: &'a [Item],
        width: usize,
    ) -> Scene<'a> {
        Scene { art, art_w, title, items, phase: 0, width }
    }

    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn truncate_respects_display_width() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello", 5), "hello");
        assert_eq!(truncate("hello", 4), "hel…");
        assert_eq!(truncate("hello", 1), "…");
        assert_eq!(truncate("hello", 0), "");
    }

    #[test]
    fn info_pane_starts_at_a_fixed_column_regardless_of_art_row() {
        // Trailing blanks are trimmed, so padding has to be computed.
        let cfg = plain_cfg();
        let art = vec!["####".to_string(), "#".to_string()];
        let items = vec![
            Item { label: "OS", value: "Arch".into() },
            Item { label: "CPU", value: "Ryzen".into() },
            Item { label: "WM", value: "Hyprland".into() },
        ];

        let out = compose(&scene(&art, 4, "me@host", &items, 80), &cfg);
        let cols: Vec<usize> = out
            .iter()
            .filter(|l| !l.trim().is_empty())
            .map(|l| strip_ansi(l).find(|c: char| !c.is_whitespace()).unwrap_or(0))
            .collect();

        // Every info-bearing row must start at art_w + gap == 6, except rows
        // that also carry art (which start at column 0).
        assert!(cols.iter().all(|&c| c == 0 || c == 6), "columns: {cols:?}");
    }

    #[test]
    fn without_art_the_info_pane_starts_at_column_zero() {
        let cfg = plain_cfg();
        let items = vec![Item { label: "OS", value: "Arch".into() }];
        let out = compose(&scene(&[], 0, "me@host", &items, 30), &cfg);

        assert!(
            out.iter().filter(|l| !l.trim().is_empty()).all(|l| !l.starts_with(' ')),
            "unexpected indent: {out:?}"
        );
    }

    #[test]
    fn narrow_terminal_never_produces_lines_wider_than_it() {
        let cfg = plain_cfg();
        let items = vec![Item {
            label: "CPU",
            value: "AMD Ryzen 9 7950X 16-Core Processor (32) @ 5.88GHz".into(),
        }];
        let art = vec!["####".to_string()];
        let out = compose(&scene(&art, 4, "me@host", &items, 20), &cfg);
        for line in &out {
            assert!(strip_ansi(line).width() <= 20, "too wide: {:?}", strip_ansi(line));
        }
    }

    #[test]
    fn paint_leaves_the_cursor_where_it_found_it() {
        // A frame that moved the cursor would scatter what the user types.
        let mut screen = Pane::new();
        let mut buf = Vec::new();
        screen.paint(&mut buf, &["a".into()], 10).unwrap();

        let written = String::from_utf8(buf).unwrap();
        assert!(written.starts_with("\x1b[?2026h\x1b7"), "{written:?}");
        assert!(written.ends_with("\x1b8\x1b[?2026l"), "{written:?}");
    }

    #[test]
    fn origin_mode_is_suspended_for_the_paint_and_restored() {
        // Absolute addressing needs origin mode off.
        let mut pane = Pane::new().with_origin_mode();
        let mut buf = Vec::new();
        pane.paint(&mut buf, &["a".into()], 4).unwrap();

        let written = String::from_utf8(buf).unwrap();
        assert!(written.starts_with("\x1b[?2026h\x1b7\x1b[?6l"), "{written:?}");
        assert!(written.ends_with("\x1b[?6h\x1b8\x1b[?2026l"), "{written:?}");

        // The default pane must not emit them at all.
        let mut plain = Pane::new();
        let mut buf = Vec::new();
        plain.paint(&mut buf, &["a".into()], 4).unwrap();
        assert!(!String::from_utf8(buf).unwrap().contains("\x1b[?6"));
    }

    #[test]
    fn first_paint_clears_only_the_panes_own_rows() {
        // A full-screen clear would erase the command output scrolling below.
        let mut screen = Pane::new();
        let mut buf = Vec::new();
        screen.paint(&mut buf, &["a".into()], 3).unwrap();

        let written = String::from_utf8(buf).unwrap();
        assert!(!written.contains("\x1b[2J"), "cleared whole screen: {written:?}");
        assert!(written.contains("\x1b[3;1H\x1b[2K"));
        assert!(!written.contains("\x1b[4;1H"), "touched a row below the pane");
    }

    #[test]
    fn paint_rewrites_only_changed_lines() {
        let mut screen = Pane::new();
        let mut buf = Vec::new();

        screen.paint(&mut buf, &["a".into(), "b".into()], 10).unwrap();
        buf.clear();

        // Second line changes; the first must not be rewritten.
        screen.paint(&mut buf, &["a".into(), "c".into()], 10).unwrap();
        let written = String::from_utf8(buf).unwrap();
        assert!(written.contains('c'));
        assert!(!written.contains('a'), "unchanged line was repainted: {written:?}");
    }

    #[test]
    fn paint_erases_rows_left_behind_by_a_shorter_frame() {
        let mut screen = Pane::new();
        let mut buf = Vec::new();

        screen.paint(&mut buf, &["a".into(), "b".into(), "c".into()], 10).unwrap();
        buf.clear();

        screen.paint(&mut buf, &["a".into()], 10).unwrap();
        let written = String::from_utf8(buf).unwrap();
        // Rows 2 and 3 must be explicitly cleared, not left showing stale art.
        assert!(written.contains("\x1b[2;1H\x1b[2K"), "{written:?}");
        assert!(written.contains("\x1b[3;1H\x1b[2K"), "{written:?}");
    }

    #[test]
    fn paint_clips_to_terminal_height() {
        let mut screen = Pane::new();
        let mut buf = Vec::new();
        let lines: Vec<String> = (0..50).map(|i| format!("line{i}")).collect();

        screen.paint(&mut buf, &lines, 5).unwrap();
        let written = String::from_utf8(buf).unwrap();
        assert!(written.contains("line4"));
        assert!(!written.contains("line5"), "drew past the last row");
    }
}
