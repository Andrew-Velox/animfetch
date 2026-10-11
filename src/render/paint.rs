//! The one place colour escapes are written.
//!
//! A style says what to draw and in which of the theme's colours; the painter
//! does the escapes, so none of them repeats the same `fg ... reset` pairing,
//! and colour being off (a pipe, `NO_COLOR`) is decided here once.

use unicode_width::UnicodeWidthStr;

use super::text::{clip_visible, ellipsize, truncate};
use crate::color::{RESET, Rgb};
use crate::config::Config;

/// Dims whatever follows, on top of its colour. Widely supported, and it keeps
/// the muted look tied to the theme rather than to a second set of colours.
const FAINT: &str = "\x1b[2m";

/// The half circles that round a chip's ends (Powerline Extra symbols).
const CAP_LEFT: char = '\u{e0b6}';
const CAP_RIGHT: char = '\u{e0b4}';

/// Columns a chip spends round its text: a space of padding either side, and
/// a cap either side when it is rounded.
pub const fn chip_overhead(round: bool) -> usize {
    if round { 4 } else { 2 }
}

/// How many dots a palette row has, and the columns they take with a space
/// between each.
pub const DOT_COUNT: u8 = 8;
pub const DOTS_WIDTH: usize = DOT_COUNT as usize * 2 - 1;

/// A run of text and its colour. `None` is the value colour.
pub type Seg = (String, Option<Rgb>);

pub fn segs_width(segs: &[Seg]) -> usize {
    segs.iter().map(|(t, _)| t.width()).sum()
}

pub struct Painter<'a> {
    out: &'a mut String,
    cfg: &'a Config,
}

impl<'a> Painter<'a> {
    pub fn new(out: &'a mut String, cfg: &'a Config) -> Self {
        Self { out, cfg }
    }

    pub fn accent(&self) -> Rgb {
        self.cfg.accent.0
    }

    pub fn value(&self) -> Rgb {
        self.cfg.value.0
    }

    /// A point along the theme's gradient, `t` from 0.0 to 1.0, so it follows
    /// a palette like everything else. The accent when there is no gradient.
    pub fn gradient(&self, t: f32) -> Rgb {
        self.cfg.gradient.sample(t).unwrap_or(self.cfg.accent.0)
    }

    /// A second colour from the theme: the far end of the gradient.
    pub fn secondary(&self) -> Rgb {
        self.gradient(1.0)
    }

    fn fg(&mut self, color: Rgb) {
        if self.cfg.color {
            self.cfg.tone(color).fg(self.out);
        }
    }

    fn reset(&mut self) {
        if self.cfg.color {
            self.out.push_str(RESET);
        }
    }

    /// Text exactly as given, in no colour of ours.
    pub fn raw(&mut self, s: &str) {
        self.out.push_str(s);
    }

    /// Already-coloured text, cut to `width` columns without breaking it.
    pub fn raw_fit(&mut self, s: &str, width: usize) {
        self.out.push_str(&clip_visible(s, width));
    }

    pub fn space(&mut self, n: usize) {
        self.out.extend(std::iter::repeat_n(' ', n));
    }

    pub fn text(&mut self, color: Rgb, s: &str) {
        self.fg(color);
        self.out.push_str(s);
        self.reset();
    }

    /// `s` in `color`, padded with spaces to `cols` columns.
    pub fn pad(&mut self, color: Rgb, s: &str, cols: usize) {
        self.fg(color);
        self.out.push_str(s);
        self.space(cols.saturating_sub(s.width()));
        self.reset();
    }

    /// Like [`Self::text`], dimmed: for lines that are structure, not content.
    pub fn faint(&mut self, color: Rgb, s: &str) {
        if self.cfg.color {
            self.out.push_str(FAINT);
        }
        self.text(color, s);
    }

    /// `text` on a filled background, with a space either side. Rounded, the
    /// ends take half-circle caps drawn in the background colour, so the chip
    /// looks like a pill whatever is behind it.
    ///
    /// Without colour there is no background to fill, so it is the padded text
    /// alone, the same width as a square chip.
    pub fn chip(&mut self, fg: Rgb, bg: Rgb, text: &str, round: bool) {
        if !self.cfg.color {
            self.out.push(' ');
            self.out.push_str(text);
            self.out.push(' ');
            return;
        }

        if round {
            self.fg(bg);
            self.out.push(CAP_LEFT);
            self.reset();
        }
        self.cfg.tone(bg).bg(self.out);
        self.fg(fg);
        self.out.push(' ');
        self.out.push_str(text);
        self.out.push(' ');
        self.reset();
        if round {
            self.fg(bg);
            self.out.push(CAP_RIGHT);
            self.reset();
        }
    }

    /// The palette row: dots in the terminal's own colours, as many as fit in
    /// `width` columns.
    pub fn dots(&mut self, width: usize) {
        let all: Vec<(u8, char)> = (0..DOT_COUNT).map(|code| (code, '●')).collect();
        self.marks(&all, 1, width);
    }

    /// Glyphs in the terminal's own colours with `gap` blanks between, as many
    /// as fit in `width` columns. Each is `(colour 0-7, glyph)`.
    pub fn marks(&mut self, marks: &[(u8, char)], gap: usize, width: usize) {
        // n glyphs take n + (n - 1) * gap columns.
        let fit = (width + gap) / (1 + gap);
        for (n, &(code, glyph)) in marks.iter().take(fit).enumerate() {
            if n > 0 {
                self.space(gap);
            }
            self.mark(code, glyph);
        }
    }

    /// A glyph in one of the terminal's own eight colours. Without colour it
    /// is the plain glyph, so colour being off never leaves an escape behind.
    pub fn mark(&mut self, code: u8, glyph: char) {
        if self.cfg.swatches() {
            self.out.push_str(&format!("\x1b[3{code}m"));
        }
        self.out.push(glyph);
        self.reset();
    }

    /// Runs of text in their colours within `room` columns.
    ///
    /// What does not fit is cut, and the cut is marked with an ellipsis so a
    /// value never looks whole when it is not: `(` left hanging at the end of
    /// `4.39 GiB / 15.56 GiB (28%)` would read as a finished reading. The one
    /// exception is a break before a space. `100%` followed by a dropped
    /// ` [AC Connected]` is complete as it stands, and says so.
    pub fn segs(&mut self, segs: &[Seg], room: usize) {
        let paint = |p: &mut Self, text: &str, color: &Option<Rgb>| {
            let color = color.unwrap_or(p.value());
            p.text(color, text);
        };

        if segs_width(segs) <= room {
            for (text, color) in segs {
                paint(self, text, color);
            }
            return;
        }

        let mut used = 0;
        for (i, (text, color)) in segs.iter().enumerate() {
            // A run only goes in whole if the column for the ellipsis is left
            // over when more follows, since all of it will not fit. Unless
            // what follows begins with a space: that is a clean place to stop.
            let more = segs
                .get(i + 1)
                .is_some_and(|(next, _)| !next.starts_with(char::is_whitespace));
            let width = text.width();

            if used + width + usize::from(more) <= room {
                paint(self, text, color);
                used += width;
            } else {
                let left = room - used;
                if left > 0 {
                    paint(self, &ellipsize(text, left), color);
                }
                return;
            }
        }
    }

    /// `prefix` in `color`, then `segs` in whatever room is left. A prefix
    /// that does not fit is cut and the values are dropped.
    pub fn prefixed(&mut self, prefix: &str, color: Rgb, segs: &[Seg], width: usize) {
        self.text(color, &truncate(prefix, width));
        if let Some(room) = width.checked_sub(prefix.width()).filter(|r| *r > 0) {
            self.segs(segs, room);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::test_util::{plain_cfg, strip_ansi};

    fn colored() -> Config {
        Config {
            color: true,
            ..plain_cfg()
        }
    }

    fn uncolored() -> Config {
        Config {
            color: false,
            ..plain_cfg()
        }
    }

    #[test]
    fn grey_writes_the_same_text_with_no_hue_in_any_colour() {
        let cfg = Config {
            gray: true,
            ..colored()
        };
        let mut out = String::new();
        let mut p = Painter::new(&mut out, &cfg);
        p.text(Rgb(255, 0, 0), "a");
        p.chip(Rgb(0, 0, 255), Rgb(0, 255, 0), "b", false);
        p.mark(1, '●');

        // Every colour that went out has equal red, green and blue.
        let mut colours = 0;
        for part in out.split("\x1b[").skip(1) {
            let Some(params) = part.split('m').next() else {
                continue;
            };
            let n: Vec<u8> = params.split(';').filter_map(|v| v.parse().ok()).collect();
            if let [38 | 48, 2, r, g, b] = n[..] {
                colours += 1;
                assert!(r == g && g == b, "{params} has a hue in {out:?}");
            }
            // And not one of the terminal's own palette colours.
            assert!(!matches!(n[..], [30..=37]), "{out:?}");
        }
        assert!(colours >= 3, "{out:?}");
        assert_eq!(strip_ansi(&out), "a b ●");
    }

    #[test]
    fn colour_off_writes_text_and_nothing_else() {
        let cfg = uncolored();
        let mut out = String::new();
        let mut p = Painter::new(&mut out, &cfg);
        p.text(Rgb(1, 2, 3), "a");
        p.faint(Rgb(1, 2, 3), "b");
        p.pad(Rgb(1, 2, 3), "c", 3);
        p.mark(1, '●');

        assert_eq!(out, "abc  ●");
    }

    #[test]
    fn a_chip_is_text_on_a_background_between_caps_drawn_in_that_background() {
        let cfg = colored();
        let mut out = String::new();
        Painter::new(&mut out, &cfg).chip(Rgb(1, 1, 1), Rgb(9, 8, 7), "ab", true);

        assert_eq!(
            out,
            "\x1b[38;2;9;8;7m\u{e0b6}\x1b[0m\
             \x1b[48;2;9;8;7m\x1b[38;2;1;1;1m ab \x1b[0m\
             \x1b[38;2;9;8;7m\u{e0b4}\x1b[0m"
        );
    }

    #[test]
    fn a_square_chip_has_no_caps() {
        let cfg = colored();
        let mut out = String::new();
        Painter::new(&mut out, &cfg).chip(Rgb(1, 1, 1), Rgb(9, 8, 7), "ab", false);

        assert!(!out.contains('\u{e0b6}') && !out.contains('\u{e0b4}'));
        assert!(out.contains(" ab "));
    }

    #[test]
    fn without_colour_a_chip_is_its_padded_text_and_as_wide_as_a_square_one() {
        let cfg = uncolored();
        let mut out = String::new();
        Painter::new(&mut out, &cfg).chip(Rgb(1, 1, 1), Rgb(9, 8, 7), "ab", false);

        assert_eq!(out, " ab ");
        assert_eq!(out.width(), "ab".width() + chip_overhead(false));
    }

    #[test]
    fn overhead_counts_the_caps_only_when_there_are_some() {
        assert_eq!(chip_overhead(true), chip_overhead(false) + 2);
    }

    #[test]
    fn the_dot_row_shows_only_as_many_dots_as_fit() {
        let cfg = uncolored();
        let dots = |width| {
            let mut out = String::new();
            Painter::new(&mut out, &cfg).dots(width);
            out
        };

        assert_eq!(dots(DOTS_WIDTH).matches('●').count(), 8);
        assert_eq!(dots(DOTS_WIDTH + 10).matches('●').count(), 8);
        assert_eq!(dots(5), "● ● ●");
        assert_eq!(dots(4), "● ●");
        assert_eq!(dots(1), "●");
        assert_eq!(dots(0), "");
        for width in 0..20 {
            assert!(dots(width).width() <= width, "{width}");
        }
    }

    #[test]
    fn marks_are_spaced_by_the_gap_and_cut_to_what_fits() {
        let cfg = uncolored();
        let marks: Vec<(u8, char)> = vec![(1, 'a'), (2, 'b'), (3, 'c')];
        let show = |gap, width| {
            let mut out = String::new();
            Painter::new(&mut out, &cfg).marks(&marks, gap, width);
            out
        };

        assert_eq!(show(2, 100), "a  b  c");
        assert_eq!(show(2, 7), "a  b  c");
        assert_eq!(show(2, 6), "a  b");
        assert_eq!(show(2, 3), "a  b".chars().take(1).collect::<String>());
        assert_eq!(show(0, 2), "ab");
        for width in 0..12 {
            assert!(show(2, width).width() <= width, "{width}");
        }
    }

    #[test]
    fn the_gradient_is_the_themes_and_the_accent_when_there_is_none() {
        let mut cfg = uncolored();
        cfg.gradient = crate::color::Gradient::new(vec![Rgb(0, 0, 0), Rgb(200, 100, 50)]);
        let mut out = String::new();
        let p = Painter::new(&mut out, &cfg);
        assert_eq!(p.gradient(0.0), Rgb(0, 0, 0));
        assert_eq!(p.gradient(1.0), Rgb(200, 100, 50));
        assert_eq!(p.secondary(), p.gradient(1.0));

        let mut bare = uncolored();
        bare.gradient = crate::color::Gradient::default();
        let mut out = String::new();
        let p = Painter::new(&mut out, &bare);
        assert_eq!(p.gradient(0.5), bare.accent.0);
    }

    #[test]
    fn a_dot_uses_the_terminals_own_colour_when_there_is_colour() {
        let cfg = colored();
        let mut out = String::new();
        Painter::new(&mut out, &cfg).mark(1, '●');
        assert_eq!(out, "\x1b[31m●\x1b[0m");
    }

    #[test]
    fn text_is_wrapped_in_its_colour_and_a_reset() {
        let cfg = colored();
        let mut out = String::new();
        Painter::new(&mut out, &cfg).text(Rgb(1, 2, 3), "x");
        assert_eq!(out, "\x1b[38;2;1;2;3mx\x1b[0m");
    }

    #[test]
    fn faint_dims_before_colouring() {
        let cfg = colored();
        let mut out = String::new();
        Painter::new(&mut out, &cfg).faint(Rgb(1, 2, 3), "x");
        assert_eq!(out, "\x1b[2m\x1b[38;2;1;2;3mx\x1b[0m");
    }

    #[test]
    fn padding_is_inside_the_colour() {
        let cfg = colored();
        let mut out = String::new();
        Painter::new(&mut out, &cfg).pad(Rgb(1, 2, 3), "ab", 5);
        assert_eq!(out, "\x1b[38;2;1;2;3mab   \x1b[0m");
    }

    #[test]
    fn segments_are_cut_where_the_room_runs_out_and_the_rest_dropped() {
        let cfg = uncolored();
        let mut out = String::new();
        let segs: Vec<Seg> = vec![("hello".into(), None), (" world".into(), None)];
        Painter::new(&mut out, &cfg).segs(&segs, 8);

        assert_eq!(out, "hello w…");
    }

    #[test]
    fn a_cut_on_a_run_boundary_is_still_marked() {
        // "(" would otherwise be the last thing shown, as if the value ended
        // there. The cut run gives up a column for the ellipsis instead.
        let cfg = uncolored();
        let mut out = String::new();
        let segs: Vec<Seg> = vec![
            ("40.05 GiB / 47.68 GiB (".into(), None),
            ("84%".into(), None),
            (")".into(), None),
        ];
        Painter::new(&mut out, &cfg).segs(&segs, 23);

        assert_eq!(out, "40.05 GiB / 47.68 GiB …");
    }

    #[test]
    fn a_break_before_a_space_is_clean_and_needs_no_mark() {
        // `100%` is a whole reading; only the note after it was dropped.
        let cfg = uncolored();
        let mut out = String::new();
        let segs: Vec<Seg> = vec![("100%".into(), None), (" [AC Connected]".into(), None)];
        Painter::new(&mut out, &cfg).segs(&segs, 4);

        assert_eq!(out, "100%");
    }

    #[test]
    fn a_note_that_starts_but_does_not_finish_is_marked() {
        let cfg = uncolored();
        let mut out = String::new();
        let segs: Vec<Seg> = vec![("100%".into(), None), (" [AC Connected]".into(), None)];
        Painter::new(&mut out, &cfg).segs(&segs, 9);

        assert_eq!(out, "100% [AC…");
    }

    #[test]
    fn everything_that_fits_exactly_is_not_cut() {
        let cfg = uncolored();
        let mut out = String::new();
        let segs: Vec<Seg> = vec![("abc".into(), None), ("def".into(), None)];
        Painter::new(&mut out, &cfg).segs(&segs, 6);

        assert_eq!(out, "abcdef");
    }

    #[test]
    fn no_room_paints_nothing() {
        let cfg = uncolored();
        let mut out = String::new();
        let segs: Vec<Seg> = vec![("abc".into(), None)];
        Painter::new(&mut out, &cfg).segs(&segs, 0);

        assert_eq!(out, "");
    }

    #[test]
    fn a_prefix_that_does_not_fit_leaves_no_room_for_values() {
        let cfg = uncolored();
        let mut out = String::new();
        let segs: Vec<Seg> = vec![("value".into(), None)];
        Painter::new(&mut out, &cfg).prefixed("a long prefix", Rgb(0, 0, 0), &segs, 6);

        assert_eq!(out, "a lon…");
    }
}
