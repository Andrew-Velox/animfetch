//! Frame composition and painting.
//!
//! Two separate concerns live here:
//!
//! * `compose` turns art + info + prompt into a flat `Vec<String>` of finished,
//!   already-coloured lines. It does no I/O.
//! * `Screen` paints such a vector, rewriting only the lines that differ from
//!   the previous frame. Clearing the whole screen every frame — which is what
//!   most animated fetchers do — is the usual source of flicker and of wasted
//!   bytes on a slow link.

// Both traits provide `write!`; the anonymous import keeps `Write` unambiguous
// while still allowing formatting into a String.
use std::fmt::Write as _;
use std::io::{self, Write};

use unicode_width::UnicodeWidthStr;

use crate::color::{RESET, Rgb};
use crate::config::Config;
use crate::fetch::Item;

/// Display width of `s`, ignoring any SGR escape sequences it contains.
///
/// Needed because several strings here carry their own colour, so a plain
/// `width()` would count escape bytes as visible columns.
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
            width += UnicodeWidthStr::width(c.to_string().as_str());
        }
    }
    width
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

/// Everything that varies between frames, gathered so `compose` does not grow
/// a long positional parameter list that is easy to call wrongly.
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
    /// Terminal width. Anything wider is truncated rather than wrapped, since a
    /// wrapped line would desynchronise the whole diff.
    pub width: usize,
}

/// Build the fetch pane: art on the left, info on the right.
///
/// The prompt is not part of this. It lives below the pane in the terminal's
/// scroll region, where it is drawn and scrolled by the shell loop rather than
/// composed into a fixed row here.
pub fn compose(scene: &Scene<'_>, cfg: &Config) -> Vec<String> {
    let &Scene { art, art_w, title, items, phase, width: avail_w } = scene;

    let rows = info_rows(title, items, cfg);

    // The gap only separates two panes; with no art there is nothing to
    // separate, so the info should not sit needlessly indented.
    let info_col = if art_w == 0 { 0 } else { art_w + cfg.gap };
    let info_w = avail_w.saturating_sub(info_col);

    // Centre the shorter pane against the taller one so the art and the text
    // stay visually balanced instead of both hugging the top.
    let height = art.len().max(rows.len());
    let art_top = (height - art.len()) / 2;
    let info_top = (height - rows.len()) / 2;

    let mut out = Vec::with_capacity(height + 2);

    for y in 0..height {
        let mut line = String::new();

        let art_row = y.checked_sub(art_top).and_then(|i| art.get(i));
        if let Some(row) = art_row {
            paint_art_row(&mut line, row, y, height, cfg, phase);
        }

        let info_row = y.checked_sub(info_top).and_then(|i| rows.get(i));
        if let Some(row) = info_row {
            // Pad from the art's *visible* width; the string also holds escape
            // sequences, which occupy no columns.
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
            // Pre-coloured rows (the palette swatches) are pure decoration and
            // have nothing to show once colour is off.
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

    // Position within the gradient, optionally advanced by the frame counter so
    // the ramp appears to flow downward through the art.
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
fn truncate(s: &str, width: usize) -> String {
    if s.width() <= width {
        return s.to_string();
    }
    if width <= 1 {
        return "…".repeat(width);
    }

    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let w = UnicodeWidthStr::width(c.to_string().as_str());
        if used + w > width - 1 {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}

/// Incremental painter for the pinned fetch pane.
///
/// Holds the previously drawn frame so it can rewrite only what changed, and
/// leaves the cursor exactly where it found it — the prompt lives below the
/// pane, and the user is typing at it while these frames are being drawn.
pub struct Pane {
    prev: Vec<String>,
    needs_clear: bool,
}

/// Save/restore cursor position and attributes (DECSC/DECRC).
const SAVE_CURSOR: &str = "\x1b7";
const RESTORE_CURSOR: &str = "\x1b8";

/// Synchronized output: terminals that understand it present the frame in one
/// go instead of mid-paint, which removes any chance of a visible tear. Those
/// that do not simply ignore both sequences.
const BEGIN_SYNC: &str = "\x1b[?2026h";
const END_SYNC: &str = "\x1b[?2026l";

impl Pane {
    pub fn new() -> Self {
        Self { prev: Vec::new(), needs_clear: true }
    }

    /// Force a full repaint. Required after a resize, and after any child
    /// process that may have drawn over the pane.
    pub fn invalidate(&mut self) {
        self.needs_clear = true;
    }

    /// Draw `lines` into rows `1..=height`.
    ///
    /// The whole frame is assembled into a single buffer and issued as one
    /// write, so it cannot be interleaved with anything else writing to the
    /// terminal.
    pub fn paint(&mut self, out: &mut impl Write, lines: &[String], height: usize) -> io::Result<()> {
        let visible = lines.len().min(height);
        let mut buf = String::with_capacity(visible * 96);

        buf.push_str(BEGIN_SYNC);
        buf.push_str(SAVE_CURSOR);

        if self.needs_clear {
            // Clear only the pane's own rows. Wiping the whole screen would
            // take the command output below it too.
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
            // Absolute positioning per line; never rely on the cursor having
            // ended up where the previous write left it.
            let _ = write!(buf, "\x1b[{};1H\x1b[2K{line}", y + 1);
        }

        // The frame shrank: erase the rows the previous one occupied.
        for y in visible..self.prev.len().min(height) {
            let _ = write!(buf, "\x1b[{};1H\x1b[2K", y + 1);
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
        // Art rows have their trailing blanks trimmed, so padding has to be
        // computed rather than assumed — this is what keeps the info pane from
        // jittering as the animation plays.
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
        // The user is typing at a prompt below the pane while these frames are
        // drawn; a frame that moved the cursor would scatter their input.
        let mut screen = Pane::new();
        let mut buf = Vec::new();
        screen.paint(&mut buf, &["a".into()], 10).unwrap();

        let written = String::from_utf8(buf).unwrap();
        assert!(written.starts_with("\x1b[?2026h\x1b7"), "{written:?}");
        assert!(written.ends_with("\x1b8\x1b[?2026l"), "{written:?}");
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
