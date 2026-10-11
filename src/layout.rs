//! How the fetch is fitted to a terminal: the art scaled to the room the info
//! pane leaves, and the screen divided between the pane and the scrolling area.

use crate::anim::Animation;
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
        } = fetch;
        let w = cols as usize;

        let (frames, art_w) = if w < Self::MIN_SPLIT_WIDTH {
            // One empty frame, so `phase % len` needs no special case.
            (vec![Vec::new()], 0)
        } else {
            let widest = render::info_width(title, items, cfg);

            // A long CPU model would otherwise squeeze the art to a blob.
            let info_w = widest.min(w * Self::INFO_WIDTH_SHARE / 100);

            let max_w = cfg.width(w.saturating_sub(info_w + cfg.gap).max(8));
            let (art_w, art_h) = animation.fit(max_w, max_h.max(4));
            let ramp = cfg.ramp();
            let ink = cfg.ink(&ramp);

            let frames = animation
                .frames
                .iter()
                .take(wanted)
                .map(|f| f.scale(art_w, art_h, ink))
                .collect();
            (frames, art_w)
        };

        Self {
            frames,
            art_w,
            title,
            items,
            width: w,
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
        }
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
