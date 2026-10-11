//! How the fetch is fitted to a terminal: the art scaled to the room the info
//! pane leaves, and the screen divided between the pane and the scrolling area.

use std::io::{self, Write};

use crate::anim::Animation;
use crate::border::{self, Borders, Frame, Plan};
use crate::config::Config;
use crate::fetch::Item;
use crate::render;

/// What the drawing code needs and never changes. Passed as one value because
/// these four travel together through every mode.
#[derive(Clone, Copy)]
pub struct Fetch<'a> {
    pub animation: &'a Animation,
    pub cfg: &'a Config,
    pub title: &'a str,
    pub items: &'a [Item],
    /// Ornaments for the left and bottom, if any were asked for.
    pub borders: Option<&'a Borders>,
}

/// Pre-scaled frames for one terminal size. Rebuilt on resize, which is exactly
/// when the art and every [`render::Scene`] built from it go stale.
pub struct Layout<'a> {
    frames: Vec<Vec<String>>,
    art_w: usize,
    title: &'a str,
    items: &'a [Item],
    /// Terminal width this layout was scaled for.
    width: usize,
    /// The borders, sized for this terminal.
    frame: Option<Frame>,
}

impl<'a> Layout<'a> {
    /// Below this, drop the art and give the info pane the whole terminal.
    const MIN_SPLIT_WIDTH: usize = 40;

    /// Share the info pane may claim before values get truncated instead.
    const INFO_WIDTH_SHARE: usize = 45;

    /// Fit the art into whatever the info pane does not need. `max_h` is the
    /// pane's row budget, not the terminal height. `wanted` caps how many frames
    /// get scaled, since a still drawing has no use for the rest.
    fn build(fetch: &Fetch<'a>, cols: u16, max_h: usize, wanted: usize) -> Self {
        let &Fetch {
            animation,
            cfg,
            title,
            items,
            borders,
        } = fetch;
        let w = cols as usize;

        let (frames, art_w, frame) = if w < Self::MIN_SPLIT_WIDTH {
            // One empty frame, so `phase % len` needs no special case.
            (vec![Vec::new()], 0, None)
        } else {
            let widest = render::info_width(title, items, cfg);

            // A long CPU model would otherwise squeeze the art to a blob.
            let info_w = widest.min(w * Self::INFO_WIDTH_SHARE / 100);

            let max_w = cfg.width(w.saturating_sub(info_w + cfg.gap).max(8));

            // With the art off there is nothing to fit, and the data section is
            // the whole pane.
            let fit = |max_w: usize, max_h: usize| {
                if cfg.art {
                    animation.fit(max_w, max_h.max(4))
                } else {
                    (0, 0)
                }
            };

            // The borders are sized for the pane as it would be without them,
            // and the art is then fitted to what they leave. Fitting the art
            // first and the border after would put the border over the info.
            let info_h = render::info_height(title, items, cfg);
            let plan = borders.map_or_else(Plan::default, |b| {
                let (bare_w, bare_h) = fit(max_w, max_h);
                // The data section is the info; the art and its gap are before it.
                let before = if bare_w == 0 { 0 } else { bare_w + cfg.gap };
                b.plan(w, info_h.max(bare_h), info_w, before)
            });

            let max_w = max_w.saturating_sub(plan.indent).max(8);
            let max_h = max_h.saturating_sub(plan.bottom_rows);
            let (art_w, art_h) = fit(max_w, max_h);
            let ramp = cfg.ramp();
            let ink = cfg.ink(&ramp);

            let frames = if cfg.art {
                animation
                    .frames
                    .iter()
                    .take(wanted)
                    .map(|f| f.scale(art_w, art_h, ink))
                    .collect()
            } else {
                vec![Vec::new()]
            };

            // The borders start where the data section does: after the art.
            let art_end = if art_w == 0 { 0 } else { art_w + cfg.gap };
            let frame = borders
                .filter(|_| plan != Plan::default())
                .map(|b| b.frame(&plan, art_h.max(info_h), art_end));
            (frames, art_w, frame)
        };

        Self {
            frames,
            art_w,
            title,
            items,
            width: w,
            frame,
        }
    }

    /// Layout for the pinned pane on a terminal of this size.
    pub fn pinned(fetch: &Fetch<'a>, cols: u16, rows: u16) -> Self {
        Self::build(
            fetch,
            cols,
            fetch.cfg.height(Split::budget(rows)),
            usize::MAX,
        )
    }

    /// Owns the whole terminal but the row the shell prompt lands on.
    pub fn full_screen(fetch: &Fetch<'a>, cols: u16, rows: u16) -> Self {
        Self::build(
            fetch,
            cols,
            fetch.cfg.height(rows.saturating_sub(1) as usize),
            usize::MAX,
        )
    }

    /// Like [`Self::full_screen`], but only the one frame a still will use.
    pub fn still(fetch: &Fetch<'a>, cols: u16, rows: u16) -> Self {
        Self::build(
            fetch,
            cols,
            fetch.cfg.height(rows.saturating_sub(1) as usize),
            1,
        )
    }

    /// What to draw for frame `phase`.
    pub fn scene(&self, phase: usize) -> render::Scene<'_> {
        self.scene_with(phase, self.items)
    }

    /// Like [`Self::scene`], with the info rows swapped for fresher ones. The
    /// art stays as scaled: it was sized from the rows this layout was built
    /// with, and a changing number is not a reason to resize it.
    pub fn scene_with<'s>(&'s self, phase: usize, items: &'s [Item]) -> render::Scene<'s> {
        render::Scene {
            art: &self.frames[phase % self.frames.len()],
            art_w: self.art_w,
            title: self.title,
            items,
            phase,
            width: self.width,
            border: self.frame.as_ref(),
        }
    }

    /// The border pictures for a pane painted by a [`Pane`](crate::render::Pane),
    /// which has rows to address: send them, and have the pane place them
    /// again with each full repaint. Takes the previous layout's pictures away
    /// first, since a resize changes their size.
    pub fn show_borders(
        &self,
        out: &mut impl Write,
        pane: &mut crate::render::Pane,
    ) -> io::Result<()> {
        let mut escapes = String::new();
        if pane.has_overlay() {
            border::clear(&mut escapes);
        }

        let mut overlay = String::new();
        if let Some(frame) = self.frame.as_ref().filter(|f| f.has_images()) {
            frame.transmit(&mut escapes);
            frame.place_at(1, &mut overlay);
        }
        pane.set_overlay(overlay);

        out.write_all(escapes.as_bytes())?;
        out.flush()
    }

    /// The border pictures for a mode that prints down the page, with the
    /// cursor standing where the pane's top-left will be.
    pub fn print_borders(&self, out: &mut impl Write) -> io::Result<()> {
        let Some(frame) = self.frame.as_ref().filter(|f| f.has_images()) else {
            return Ok(());
        };
        let mut escapes = String::new();
        frame.transmit(&mut escapes);
        frame.place_here(&mut escapes);

        out.write_all(escapes.as_bytes())?;
        out.flush()
    }

    /// Whether this layout draws any picture, as opposed to cells or nothing.
    pub fn has_pictures(&self) -> bool {
        self.frame.as_ref().is_some_and(Frame::has_images)
    }
}

/// How the screen is divided between the pinned fetch and the scrolling area.
pub struct Split {
    /// Rows the fetch pane occupies, starting at row 1.
    pub pane_h: usize,
    /// 1-based first row of the scroll region, one blank row below the pane.
    pub scroll_top: u16,
}

impl Split {
    /// Rows the scrolling area needs before splitting is worth it at all.
    const MIN_SCROLL_ROWS: usize = 4;

    /// Share the fetch may claim. The art gets the smaller half; the rest is
    /// where you actually work.
    const PANE_HEIGHT_SHARE: usize = 55;

    /// Row budget for the pane on a terminal of `rows` rows.
    fn budget(rows: u16) -> usize {
        let rows = rows as usize;
        (rows * Self::PANE_HEIGHT_SHARE / 100)
            .min(rows.saturating_sub(Self::MIN_SCROLL_ROWS + 1))
            .max(1)
    }

    /// `None` when the terminal is too short to usefully divide.
    fn new(pane_lines: usize, rows: u16) -> Option<Self> {
        let max_pane = (rows as usize).checked_sub(Self::MIN_SCROLL_ROWS + 1)?;
        let pane_h = pane_lines.min(max_pane);
        (pane_h > 0).then(|| Self {
            pane_h,
            scroll_top: pane_h as u16 + 2,
        })
    }

    /// The split for `layout`, whose composed height is what the pane needs.
    pub fn fit(layout: &Layout<'_>, cfg: &Config, rows: u16) -> Option<Self> {
        Self::new(render::compose(&layout.scene(0), cfg).len(), rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::border::{Bitmap, Mode};
    use crate::render::test_util::sample_items;
    use crate::render::{compose, visible_width_for_tests as width_of};

    fn slab(w: usize, h: usize) -> Bitmap {
        let mut b = Bitmap::blank(w, h);
        b.rgba.iter_mut().for_each(|v| *v = 255);
        b
    }

    /// The pane the layout composes, for a terminal `cols` wide.
    fn pane(borders: Option<&Borders>, cols: u16) -> (Vec<String>, usize, usize) {
        let animation = Animation::load(None, "cat-run").unwrap();
        let cfg = Config {
            color: true,
            ..Config::default()
        };
        let items = sample_items();
        let fetch = Fetch {
            animation: &animation,
            cfg: &cfg,
            title: "me@host",
            items: &items,
            borders,
        };

        let layout = Layout::pinned(&fetch, cols, 50);
        let lines = compose(&layout.scene(0), &cfg);
        let indent = layout.frame.as_ref().map_or(0, |f| f.indent);
        let bottom = layout.frame.as_ref().map_or(0, |f| f.bottom_rows());
        (lines, indent, bottom)
    }

    fn both(mode: Mode) -> Borders {
        Borders::new(Some(slab(60, 300)), Some(slab(400, 60)), mode)
    }

    #[test]
    fn with_the_art_off_the_data_is_the_whole_pane_and_the_border_leads() {
        let animation = Animation::load(None, "cat-run").unwrap();
        let items = sample_items();
        let borders = both(Mode::Image);

        // (art width, border column, border indent, composed lines)
        let build = |art: bool| {
            let cfg = Config {
                color: true,
                art,
                ..Config::default()
            };
            let fetch = Fetch {
                animation: &animation,
                cfg: &cfg,
                title: "me@host",
                items: &items,
                borders: Some(&borders),
            };
            let layout = Layout::pinned(&fetch, 100, 50);
            let frame = layout.frame.as_ref().expect("a border");
            let lines = compose(&layout.scene(0), &cfg);
            (layout.art_w, frame.col(), frame.indent, lines)
        };

        let (art_w, _, _, with_lines) = build(true);
        let (none_w, col, indent, lines) = build(false);

        assert!(art_w > 0);
        assert_eq!(none_w, 0, "no art was fitted");
        assert_eq!(col, 0, "so the border is the first thing on the line");
        // More room to the right, so the bottom ornament may be wider and so
        // taller; what must not happen is the info losing rows.
        assert!(
            lines.len() + 8 >= with_lines.len(),
            "{} vs {}",
            lines.len(),
            with_lines.len()
        );

        // The info starts right after the border, and nothing is before it.
        let info_rows = lines
            .iter()
            .map(|l| crate::color::strip_sgr(l))
            .filter(|l| !l.trim().is_empty());
        for plain in info_rows {
            let lead = plain.chars().take_while(|c| *c == ' ').count();
            assert_eq!(lead, indent, "{plain:?}");
        }
    }

    #[test]
    fn a_border_never_pushes_a_line_past_the_terminal() {
        for mode in [Mode::Cells, Mode::Image] {
            let borders = both(mode);
            for cols in [60u16, 70, 80, 100, 140, 200] {
                let (lines, ..) = pane(Some(&borders), cols);
                for line in &lines {
                    assert!(
                        width_of(line) <= cols as usize,
                        "{mode:?} at {cols}: {} columns",
                        width_of(line)
                    );
                }
            }
        }
    }

    /// Where the borders begin and how much they take, for a layout of this
    /// terminal: (column, columns taken beside the info, art width).
    fn geometry(borders: &Borders, cols: u16) -> (usize, usize, usize) {
        let animation = Animation::load(None, "cat-run").unwrap();
        let cfg = Config {
            color: true,
            ..Config::default()
        };
        let items = sample_items();
        let fetch = Fetch {
            animation: &animation,
            cfg: &cfg,
            title: "me@host",
            items: &items,
            borders: Some(borders),
        };
        let layout = Layout::pinned(&fetch, cols, 50);
        let frame = layout.frame.as_ref().expect("room for a border");
        (frame.col(), frame.indent, layout.art_w)
    }

    #[test]
    fn the_border_stands_between_the_art_and_the_info() {
        // The ornament belongs to the data, so it is right of the art and left
        // of the info: art | border | info, not border | art | info.
        let borders = both(Mode::Image);
        let (col, indent, art_w) = geometry(&borders, 120);
        let (lines, _, bottom) = pane(Some(&borders), 120);

        assert!(indent > 0 && art_w > 0);
        assert!(
            col >= art_w,
            "it begins after the art: col {col}, art {art_w}"
        );

        // Columns are counted on what is seen: the pane is composed with colour.
        let body: Vec<String> = lines[..lines.len() - bottom]
            .iter()
            .map(|l| crate::color::strip_sgr(l))
            .collect();

        // Nothing is drawn in the columns the picture covers...
        for line in &body {
            let covered: String = line.chars().skip(col).take(indent).collect();
            assert!(
                covered.chars().all(|c| c == ' '),
                "under the picture: {covered:?}"
            );
        }
        // ...the art is drawn before it, and the info after it.
        assert!(
            body.iter()
                .any(|l| l.chars().take(col).any(|c| !c.is_whitespace())),
            "no art before the border"
        );
        assert!(
            body.iter()
                .any(|l| l.chars().skip(col + indent).any(|c| !c.is_whitespace())),
            "no info after the border"
        );
    }

    #[test]
    fn cells_are_printed_where_the_picture_would_stand() {
        let borders = both(Mode::Cells);
        let (col, indent, _) = geometry(&borders, 120);
        let (lines, ..) = pane(Some(&borders), 120);

        // Some row has an ornament cell (a braille character) inside its band.
        let in_band = |l: &String| {
            l.chars().filter(|c| !c.is_ascii()).count() > 0
                && crate::render::visible_width_for_tests(l) >= col + indent
        };
        assert!(lines.iter().any(in_band));

        // And the first thing in the band is not before the art ends.
        for line in &lines {
            let plain = crate::color::strip_sgr(line);
            let before: String = plain.chars().take(col).collect();
            assert!(
                !before
                    .chars()
                    .any(|c| ('\u{2800}'..='\u{28ff}').contains(&c)),
                "an ornament cell is over the art: {before:?}"
            );
        }
    }

    #[test]
    fn the_bottom_border_starts_under_the_data_not_under_the_art() {
        let borders = both(Mode::Cells);
        let (col, _, _) = geometry(&borders, 120);
        let (lines, _, bottom) = pane(Some(&borders), 120);
        assert!(bottom > 0);

        // The last `bottom` rows are the ornament. None of it is left of `col`.
        for line in &lines[lines.len() - bottom..] {
            let plain = crate::color::strip_sgr(line);
            let before: String = plain.chars().take(col).collect();
            assert!(before.trim().is_empty(), "under the art: {before:?}");
        }
        assert!(
            lines[lines.len() - bottom..]
                .iter()
                .any(|l| !l.trim().is_empty()),
            "the bottom border drew nothing"
        );
    }

    #[test]
    fn the_rows_of_the_bottom_border_are_part_of_the_panes_height() {
        // `Split` sizes the scroll region from the composed height, so a
        // border that was not counted would be drawn over the prompt.
        let (plain, ..) = pane(None, 100);
        let (framed, _, bottom) = pane(Some(&both(Mode::Image)), 100);

        assert!(bottom > 0);
        assert!(framed.len() >= bottom, "{} rows", framed.len());
        // The art gave up the rows the border took.
        assert!(
            framed.len() <= plain.len() + bottom,
            "the border grew the pane by more than its own height"
        );
    }

    #[test]
    fn no_border_is_exactly_what_it_was() {
        // The default: nothing asked for must change nothing at all.
        let none = Borders::new(None, None, Mode::Image);
        for cols in [40u16, 80, 120] {
            let (without, ..) = pane(None, cols);
            let (with, indent, bottom) = pane(Some(&none), cols);

            assert_eq!((indent, bottom), (0, 0));
            assert_eq!(with, without, "at {cols} columns");
        }
    }

    #[test]
    fn a_terminal_too_narrow_for_an_ornament_gets_the_plain_pane() {
        let borders = both(Mode::Image);
        let (framed, indent, bottom) = pane(Some(&borders), 50);
        let (plain, ..) = pane(None, 50);

        assert_eq!((indent, bottom), (0, 0));
        assert_eq!(framed, plain);
    }

    #[test]
    fn the_art_gives_up_the_columns_the_border_takes() {
        let animation = Animation::load(None, "cat-run").unwrap();
        let cfg = Config::default();
        let items = sample_items();
        let build = |borders: Option<&Borders>| {
            let fetch = Fetch {
                animation: &animation,
                cfg: &cfg,
                title: "me@host",
                items: &items,
                borders,
            };
            Layout::pinned(&fetch, 100, 50).art_w
        };

        let with = build(Some(&both(Mode::Image)));
        let without = build(None);
        assert!(with <= without, "the art grew: {with} vs {without}");
    }
}
