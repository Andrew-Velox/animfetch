//! The incremental painter for the pinned region.

use std::fmt::Write as _;
use std::io::{self, Write};
use std::time::{Duration, Instant};

/// How long the pictures are left alone before they are sent again.
///
/// A program that clears the screen, `clear` being the usual one, takes every
/// picture with it, and the terminal frees their data as well as their
/// placement. Nothing tells a background process that happened, so the
/// pictures are simply sent again now and then. About 55 KB, which a terminal
/// decodes without a visible cost, inside the frame's own synchronised write.
const PICTURE_REFRESH: Duration = Duration::from_secs(1);

/// Incremental painter for the pinned pane. Rewrites only changed lines and
/// leaves the cursor where it found it, since the user is typing below.
pub struct Pane {
    prev: Vec<String>,
    needs_clear: bool,
    /// Tallest pane painted since the last clear. A resize can shrink the pane,
    /// and the rows it no longer covers still hold the old art.
    painted_h: usize,
    /// Origin mode makes rows relative to the scroll region, which would put
    /// the art inside it. When set, each frame turns it off and back on.
    origin_mode: bool,
    /// Escapes that put the border pictures on screen: the pictures
    /// themselves, then where they go. Sent whenever the whole pane is
    /// repainted, since a repaint may have followed a program that cleared the
    /// screen, and again every `refresh` for the same reason.
    overlay: String,
    /// When the overlay last went out.
    overlay_sent: Option<Instant>,
    /// How long the overlay is left before it goes out again.
    refresh: Duration,
}

/// Save/restore cursor position and attributes (DECSC/DECRC).
const SAVE_CURSOR: &str = "\x1b7";
const RESTORE_CURSOR: &str = "\x1b8";

/// Synchronized output: the frame lands in one go, so it cannot tear.
const BEGIN_SYNC: &str = "\x1b[?2026h";
const END_SYNC: &str = "\x1b[?2026l";

impl Pane {
    pub fn new() -> Self {
        Self {
            prev: Vec::new(),
            needs_clear: true,
            painted_h: 0,
            origin_mode: false,
            overlay: String::new(),
            overlay_sent: None,
            refresh: PICTURE_REFRESH,
        }
    }

    /// Declare that the terminal is in origin mode, so painting can opt out of
    /// it for the duration of each frame.
    pub fn with_origin_mode(mut self) -> Self {
        self.origin_mode = true;
        self
    }

    /// The escapes that send and place the border pictures, sent with each
    /// full repaint and then every so often. Empty for none.
    pub fn set_overlay(&mut self, overlay: String) {
        self.overlay = overlay;
        self.needs_clear = true;
    }

    /// Send the pictures again after this long without a full repaint.
    #[cfg(test)]
    fn with_picture_refresh(mut self, every: Duration) -> Self {
        self.refresh = every;
        self
    }

    pub fn has_overlay(&self) -> bool {
        !self.overlay.is_empty()
    }

    /// Force a full repaint, after a resize or a child that drew over us.
    pub fn invalidate(&mut self) {
        self.needs_clear = true;
    }

    /// Draw `lines` into rows `1..=height`, as one write so nothing can
    /// interleave with it.
    pub fn paint(
        &mut self,
        out: &mut impl Write,
        lines: &[String],
        height: usize,
    ) -> io::Result<()> {
        let visible = lines.len().min(height);
        let full = self.needs_clear;
        let mut buf = String::with_capacity(visible * 96);

        buf.push_str(BEGIN_SYNC);
        buf.push_str(SAVE_CURSOR);
        if self.origin_mode {
            buf.push_str(crate::term::RESET_ORIGIN_MODE);
        }

        if self.needs_clear {
            // Up to the tallest we have ever painted, not just the current
            // height: a resize can shrink the pane, and the rows it used to
            // cover still hold art that nothing else will erase.
            //
            // Only our rows; a full clear would take the output below too.
            for y in 0..height.max(self.painted_h) {
                let _ = write!(buf, "\x1b[{};1H\x1b[2K", y + 1);
            }
            self.prev.clear();
            self.painted_h = 0;
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

        let stale = self.overlay_sent.is_none_or(|at| at.elapsed() >= self.refresh);
        if !self.overlay.is_empty() && (full || stale) {
            buf.push_str(&self.overlay);
            self.overlay_sent = Some(Instant::now());
        }

        if self.origin_mode {
            buf.push_str(crate::term::SET_ORIGIN_MODE);
        }
        buf.push_str(RESTORE_CURSOR);
        buf.push_str(END_SYNC);

        self.prev.clear();
        self.prev.extend_from_slice(&lines[..visible]);
        self.painted_h = self.painted_h.max(visible);

        out.write_all(buf.as_bytes())?;
        out.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pictures_are_placed_on_a_full_repaint_and_not_on_every_frame() {
        let mut pane = Pane::new();
        pane.set_overlay("<PLACE>".to_string());
        let mut buf = Vec::new();

        pane.paint(&mut buf, &["a".into()], 4).unwrap();
        assert!(String::from_utf8(buf.clone()).unwrap().contains("<PLACE>"));

        buf.clear();
        pane.paint(&mut buf, &["a".into()], 4).unwrap();
        assert!(
            !String::from_utf8(buf.clone()).unwrap().contains("<PLACE>"),
            "resent"
        );

        // After something drew over us they are placed again.
        pane.invalidate();
        buf.clear();
        pane.paint(&mut buf, &["a".into()], 4).unwrap();
        assert!(String::from_utf8(buf).unwrap().contains("<PLACE>"));
    }

    #[test]
    fn the_pictures_are_sent_again_after_a_while_without_repainting_the_rows() {
        // `clear` frees them in the terminal and nothing says it happened.
        let mut pane = Pane::new().with_picture_refresh(Duration::ZERO);
        pane.set_overlay("<PLACE>".to_string());
        let mut buf = Vec::new();
        pane.paint(&mut buf, &["a".into()], 4).unwrap();

        buf.clear();
        pane.paint(&mut buf, &["a".into()], 4).unwrap();
        let written = String::from_utf8(buf).unwrap();
        assert!(written.contains("<PLACE>"), "{written:?}");
        // Only the pictures: the rows are not wiped and drawn again.
        assert!(!written.contains("\x1b[2K"), "{written:?}");
    }

    #[test]
    fn nothing_is_sent_again_while_the_refresh_is_not_due() {
        let mut pane = Pane::new().with_picture_refresh(Duration::from_secs(3600));
        pane.set_overlay("<PLACE>".to_string());
        let mut buf = Vec::new();
        pane.paint(&mut buf, &["a".into()], 4).unwrap();

        for _ in 0..3 {
            buf.clear();
            pane.paint(&mut buf, &["a".into()], 4).unwrap();
            assert!(!String::from_utf8(buf.clone()).unwrap().contains("<PLACE>"));
        }
    }

    #[test]
    fn a_pane_with_no_pictures_never_sends_any() {
        let mut pane = Pane::new().with_picture_refresh(Duration::ZERO);
        let mut buf = Vec::new();
        for _ in 0..2 {
            pane.paint(&mut buf, &["a".into()], 4).unwrap();
        }
        assert!(!String::from_utf8(buf).unwrap().contains("\x1b_G"));
    }

    #[test]
    fn the_pictures_go_inside_the_bracket_that_restores_the_cursor() {
        let mut pane = Pane::new().with_origin_mode();
        pane.set_overlay("<PLACE>".to_string());
        let mut buf = Vec::new();
        pane.paint(&mut buf, &["a".into()], 4).unwrap();

        let out = String::from_utf8(buf).unwrap();
        let place = out.find("<PLACE>").unwrap();
        assert!(
            place < out.rfind("\x1b8").unwrap(),
            "after the restore: {out:?}"
        );
        assert!(
            place > out.find("\x1b[?6l").unwrap(),
            "before origin mode is off"
        );
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
        assert!(
            written.starts_with("\x1b[?2026h\x1b7\x1b[?6l"),
            "{written:?}"
        );
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
        assert!(
            !written.contains("\x1b[2J"),
            "cleared whole screen: {written:?}"
        );
        assert!(written.contains("\x1b[3;1H\x1b[2K"));
        assert!(
            !written.contains("\x1b[4;1H"),
            "touched a row below the pane"
        );
    }

    #[test]
    fn paint_rewrites_only_changed_lines() {
        let mut screen = Pane::new();
        let mut buf = Vec::new();

        screen
            .paint(&mut buf, &["a".into(), "b".into()], 10)
            .unwrap();
        buf.clear();

        // Second line changes; the first must not be rewritten.
        screen
            .paint(&mut buf, &["a".into(), "c".into()], 10)
            .unwrap();
        let written = String::from_utf8(buf).unwrap();
        assert!(written.contains('c'));
        assert!(
            !written.contains('a'),
            "unchanged line was repainted: {written:?}"
        );
    }

    #[test]
    fn paint_erases_rows_left_behind_by_a_shorter_frame() {
        let mut screen = Pane::new();
        let mut buf = Vec::new();

        screen
            .paint(&mut buf, &["a".into(), "b".into(), "c".into()], 10)
            .unwrap();
        buf.clear();

        screen.paint(&mut buf, &["a".into()], 10).unwrap();
        let written = String::from_utf8(buf).unwrap();
        // Rows 2 and 3 must be explicitly cleared, not left showing stale art.
        assert!(written.contains("\x1b[2;1H\x1b[2K"), "{written:?}");
        assert!(written.contains("\x1b[3;1H\x1b[2K"), "{written:?}");
    }

    #[test]
    fn invalidate_erases_rows_a_shrinking_pane_no_longer_covers() {
        // A resize can leave the pane shorter than it was. Clearing only the
        // new height left the rows between the two showing old art, which is
        // what smeared copies of the fetch down the screen on resize.
        let mut screen = Pane::new();
        let mut buf = Vec::new();

        let tall: Vec<String> = (0..8).map(|i| format!("row{i}")).collect();
        screen.paint(&mut buf, &tall, 8).unwrap();
        buf.clear();

        screen.invalidate();
        screen.paint(&mut buf, &["only".into()], 1).unwrap();
        let written = String::from_utf8(buf).unwrap();

        for row in 2..=8 {
            assert!(
                written.contains(&format!("\x1b[{row};1H\x1b[2K")),
                "row {row} was left uncleared: {written:?}"
            );
        }
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
