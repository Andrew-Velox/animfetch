//! Decorative borders: ornaments drawn beside and below the fetch.
//!
//! The images are PNG line art. Where the terminal can show pictures they are
//! drawn as pictures; where it cannot they are drawn in braille cells. Both
//! look the same to the layout: some columns between the art and the info, and
//! some rows beneath the data section, reserved before anything else is fitted.
//!
//! - [`bitmap`]: decoding, turning, trimming and scaling pixels
//! - [`catalog`]: which images exist, bundled and yours, and finding one
//! - [`kitty`], [`cells`]: the two ways of putting one on screen

mod bitmap;
mod catalog;
mod cells;
mod kitty;

use std::path::Path;

pub use bitmap::Bitmap;
pub use catalog::{Slot, Source, check, list, load};

use crate::config::{BorderRender, Config};

/// Below this many columns there is no room for an ornament and the info.
const MIN_COLS: usize = 60;

/// The widest a left border may be, in columns, and the share of the terminal.
const MAX_LEFT_COLS: usize = 16;

/// The tallest a bottom border may be, in rows.
const MAX_BOTTOM_ROWS: usize = 5;

/// Blank columns between the left border and what follows.
const GAP: usize = 2;

/// How much of a dot's box of the source must be ink for the dot to be lit.
/// Low enough that a thin stroke stays connected, high enough that the gaps
/// between strokes stay open: measured on the bundled images, 8-15% keeps
/// strokes whole, and 30% starts to break them.
const DOT_COVERAGE: f32 = 0.10;

/// Image ids. Fixed, so a new layout replaces the old pictures, and so they
/// can be taken away by id without touching anyone else's.
const ID_LEFT: u32 = 0x00AF_0001;
const ID_BOTTOM: u32 = 0x00AF_0002;

/// A terminal cell's size in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub w: usize,
    pub h: usize,
}

impl Cell {
    /// A braille cell: two dots across, four down.
    pub const DOTS: Cell = Cell {
        w: cells::DOTS_W,
        h: cells::DOTS_H,
    };

    /// The usual shape of a cell, for a terminal that will not say.
    pub const GUESS: Cell = Cell { w: 10, h: 20 };
}

/// How an ornament reaches the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Image,
    Cells,
}

impl Mode {
    /// How to draw, given what was asked for and whether the terminal shows
    /// pictures. `None` is no border at all.
    ///
    /// `Auto` shows the pictures where it can and nothing where it cannot.
    /// It does not turn them into text: that is a different, coarser drawing
    /// than the image, and is only ever drawn when asked for by name.
    pub fn resolve(wanted: BorderRender, images: bool) -> Option<Self> {
        match wanted {
            BorderRender::Image => Some(Mode::Image),
            BorderRender::Cells => Some(Mode::Cells),
            BorderRender::Auto => images.then_some(Mode::Image),
        }
    }
}

/// The largest box of whole cells, within `max_cols` by `max_rows`, that the
/// bitmap fits in at its own proportions.
fn fit_box(bmp: &Bitmap, max_cols: usize, max_rows: usize, cell: Cell) -> (usize, usize) {
    if max_cols == 0 || max_rows == 0 || bmp.w == 0 || bmp.h == 0 {
        return (0, 0);
    }
    let scale =
        ((max_cols * cell.w) as f64 / bmp.w as f64).min((max_rows * cell.h) as f64 / bmp.h as f64);

    let cols = (bmp.w as f64 * scale / cell.w as f64).ceil() as usize;
    let rows = (bmp.h as f64 * scale / cell.h as f64).ceil() as usize;
    (cols.clamp(1, max_cols), rows.clamp(1, max_rows))
}

/// The furthest a bottom picture may be raised, in rows.
const MAX_LIFT: f32 = 3.0;

/// The furthest a bottom picture may be moved to the right, in columns.
const MAX_SHIFT: f32 = 24.0;

/// How far right the bottom picture is moved: whole columns to move where it
/// starts, and the pixels to push it on inside the column it lands in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Shift {
    cols: usize,
    x_px: usize,
}

impl Shift {
    fn new(cols_right: f32, cell_w: usize) -> Self {
        let px = (cols_right.clamp(0.0, MAX_SHIFT) * cell_w as f32).round() as usize;
        if px == 0 || cell_w == 0 {
            return Self::default();
        }
        Self {
            cols: px / cell_w,
            x_px: px % cell_w,
        }
    }

    /// Columns the picture reaches into beyond where it would have ended: a
    /// push inside a column spills into the next one.
    fn reach(self) -> usize {
        self.cols + usize::from(self.x_px > 0)
    }
}

/// How far up the bottom picture is raised: whole rows to move where it
/// starts, and the pixels to push it back down inside that row.
///
/// A picture may start part-way down a cell but not part-way up one, so a
/// raise by half a row is "start in the row above, 10 px down".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Lift {
    rows: usize,
    y_px: usize,
}

impl Lift {
    fn new(rows_up: f32, cell_h: usize, body_rows: usize) -> Self {
        let up = (rows_up.clamp(0.0, MAX_LIFT) * cell_h as f32).round() as usize;
        if up == 0 || cell_h == 0 {
            return Self::default();
        }

        // Never above the top of the pane.
        let rows = up.div_ceil(cell_h).min(body_rows);
        let up = up.min(rows * cell_h);
        Self {
            rows,
            y_px: rows * cell_h - up,
        }
    }
}

/// The two border images, decoded and oriented, and how to draw them.
pub struct Borders {
    left: Option<Bitmap>,
    bottom: Option<Bitmap>,
    mode: Mode,
    /// Rows to raise the bottom picture by.
    lift: f32,
    /// Columns to move the bottom picture to the right by.
    shift: f32,
}

impl Borders {
    /// From images already in the orientation they are drawn in.
    pub fn new(left: Option<Bitmap>, bottom: Option<Bitmap>, mode: Mode) -> Self {
        Self {
            left,
            bottom,
            mode,
            lift: 0.0,
            shift: 0.0,
        }
    }

    /// Raise the bottom picture by `rows`, which may be a fraction.
    pub fn with_lift(mut self, rows: f32) -> Self {
        self.lift = if rows.is_nan() { 0.0 } else { rows };
        self
    }

    /// Move the bottom picture `cols` to the right, which may be a fraction.
    pub fn with_shift(mut self, cols: f32) -> Self {
        self.shift = if cols.is_nan() { 0.0 } else { cols };
        self
    }

    /// The borders the config asks for, or `None` if it asks for none.
    ///
    /// A bottom image is stored upright, hilt at the top, and is turned a
    /// quarter clockwise so it lies along the bottom with the hilt to the right.
    pub fn load(
        cfg: &Config,
        config_dir: Option<&Path>,
        images: bool,
        to_terminal: bool,
    ) -> Result<Option<Self>, String> {
        let asked = [
            (Slot::Left, cfg.border_left.trim()),
            (Slot::Bottom, cfg.border_bottom.trim()),
        ];
        if asked.iter().all(|(_, spec)| spec.is_empty()) {
            return Ok(None);
        }

        // Nothing to draw on (a pipe), or a terminal that cannot show the
        // pictures and was not told to use text. A name that is wrong is still
        // wrong, so it is still reported.
        //
        // Colour is not asked of a picture: `color = false` is about the text.
        // Only the text drawing of a border is a thing to colour, and without
        // colour it is not drawn. Grey is a colour setting of its own, and the
        // pictures are taken to grey with the rest.
        let mode = if to_terminal {
            Mode::resolve(cfg.border_render, images && kitty::supported())
                .filter(|m| *m == Mode::Image || cfg.color)
        } else {
            None
        };
        let Some(mode) = mode else {
            for (slot, spec) in asked.into_iter().filter(|(_, s)| !s.is_empty()) {
                check(slot, spec, config_dir)?;
            }
            return Ok(None);
        };

        let read = |slot: Slot, spec: &str| -> Result<Option<Bitmap>, String> {
            let spec = spec.trim();
            if spec.is_empty() {
                return Ok(None);
            }
            let bytes = load(slot, spec, config_dir)?;
            let bitmap = Bitmap::decode(&bytes).map_err(|e| format!("border {spec:?}: {e}"))?;
            bitmap
                .trimmed()
                .map(|b| if cfg.gray { b.grayscale() } else { b })
                .map(Some)
                .ok_or_else(|| format!("border {spec:?} has nothing visible in it"))
        };

        let left = read(Slot::Left, &cfg.border_left)?;
        let bottom = read(Slot::Bottom, &cfg.border_bottom)?.map(|b| b.rotated_cw());

        Ok(Some(
            Self::new(left, bottom, mode)
                .with_lift(cfg.border_bottom_lift)
                .with_shift(cfg.border_bottom_shift),
        ))
    }

    /// The cell size the sizes are worked out in.
    pub fn cell(&self) -> Cell {
        match self.mode {
            Mode::Cells => Cell::DOTS,
            Mode::Image => crate::term::cell_pixels().map_or(Cell::GUESS, |(w, h)| Cell { w, h }),
        }
    }

    /// The move to the right, in the units this mode can draw.
    fn shift_for(&self, cell: Cell) -> Shift {
        match self.mode {
            Mode::Image => Shift::new(self.shift, cell.w),
            // Cells are whole characters: only a picture can move by part of one.
            Mode::Cells => Shift {
                cols: self.shift.clamp(0.0, MAX_SHIFT).round() as usize,
                x_px: 0,
            },
        }
    }

    /// The same, held to at most `cols` columns.
    fn shift_for_cols(&self, cols: usize, cell: Cell) -> Shift {
        let held = Self {
            left: None,
            bottom: None,
            mode: self.mode,
            lift: 0.0,
            shift: self.shift.clamp(0.0, cols as f32),
        };
        held.shift_for(cell)
    }

    /// What the borders will take, for a data section `body_rows` tall and
    /// `data_cols` wide, in a terminal `cols` wide whose first `art_cols`
    /// columns the art (and its gap) already holds.
    pub fn plan(&self, cols: usize, body_rows: usize, data_cols: usize, art_cols: usize) -> Plan {
        let cell = self.cell();
        let mut plan = Plan::default();
        if cols < MIN_COLS || body_rows < 3 {
            return plan;
        }

        if let Some(left) = &self.left {
            let (c, _) = fit_box(left, MAX_LEFT_COLS.min(cols / 4), body_rows, cell);
            plan.left_cols = c;
            plan.indent = c + GAP;
        }
        if let Some(bottom) = &self.bottom {
            // Under the data section, ornament and all, and no wider than the
            // room the art leaves.
            let room = cols.saturating_sub(art_cols + 2).max(8);
            let wanted = self.shift_for(cell);

            // The picture is narrowed by what it is moved, so that moving it
            // never pushes it past the edge of the terminal.
            let fits = room.saturating_sub(wanted.reach()).max(8);
            let width = (data_cols + plan.indent).clamp(8, fits);
            let (c, r) = fit_box(bottom, width, MAX_BOTTOM_ROWS, cell);
            plan.bottom_cols = c;
            plan.bottom_rows = r;

            // Whatever of the move still fits beside the picture.
            let spare = room.saturating_sub(c);
            plan.bottom_shift = if wanted.reach() <= spare {
                wanted
            } else {
                self.shift_for_cols(spare, cell)
            };
        }
        plan
    }

    /// The borders drawn for a pane that turned out `body_rows` tall, with the
    /// data section starting at column `col` (0-based), after the art.
    pub fn frame(&self, plan: &Plan, body_rows: usize, col: usize) -> Frame {
        let cell = self.cell();
        let mode = self.mode;

        let left = self
            .left
            .as_ref()
            .filter(|_| plan.left_cols > 0)
            .map(|b| piece(b, plan.left_cols, body_rows, cell, mode, ID_LEFT));
        let bottom = self
            .bottom
            .as_ref()
            .filter(|_| plan.bottom_rows > 0)
            .map(|b| piece(b, plan.bottom_cols, plan.bottom_rows, cell, mode, ID_BOTTOM));

        // Cells are whole characters, so only a picture can be raised by part
        // of a row.
        let lift = match mode {
            Mode::Image => Lift::new(self.lift, cell.h, body_rows),
            Mode::Cells => Lift::default(),
        };

        Frame {
            indent: plan.indent,
            col,
            body_rows,
            lift,
            shift: plan.bottom_shift,
            left,
            bottom,
        }
    }
}

/// What the borders reserve, worked out before the art is fitted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// Columns the left border and the gap after it take, before the info.
    pub indent: usize,
    left_cols: usize,
    /// Rows beneath the pane.
    pub bottom_rows: usize,
    bottom_cols: usize,
    bottom_shift: Shift,
}

/// One border, sized and ready: cells to print, or a picture to place.
pub struct Placed {
    cols: usize,
    rows: usize,
    look: Look,
}

enum Look {
    Cells(Vec<String>),
    Image {
        id: u32,
        w: usize,
        h: usize,
        rgba: Vec<u8>,
    },
}

/// `bmp` scaled to fit `max_cols` by `max_rows` cells, top-left in its box.
fn piece(
    bmp: &Bitmap,
    max_cols: usize,
    max_rows: usize,
    cell: Cell,
    mode: Mode,
    id: u32,
) -> Placed {
    let (cols, rows) = fit_box(bmp, max_cols, max_rows, cell);
    let (px_w, px_h) = (cols * cell.w, rows * cell.h);

    let scale = (px_w as f64 / bmp.w as f64).min(px_h as f64 / bmp.h as f64);
    let sw = ((bmp.w as f64 * scale).round() as usize).clamp(1, px_w.max(1));
    let sh = ((bmp.h as f64 * scale).round() as usize).clamp(1, px_h.max(1));

    // A dot is on or off, so cells are decided by coverage; a picture is
    // blended, as it will be shown.
    let scaled = match mode {
        Mode::Cells => bmp.dotted(sw, sh, DOT_COVERAGE),
        Mode::Image => bmp.resized(sw, sh),
    };

    // A whole number of cells, so the terminal never has to stretch it.
    let mut canvas = Bitmap::blank(px_w, px_h);
    canvas.paste(&scaled, 0, 0);

    let look = match mode {
        Mode::Cells => Look::Cells(cells::render(&canvas)),
        Mode::Image => Look::Image {
            id,
            w: px_w,
            h: px_h,
            rgba: canvas.rgba,
        },
    };
    Placed { cols, rows, look }
}

/// The borders for one layout.
pub struct Frame {
    /// Columns the left border and its gap take, between the art and the info.
    pub indent: usize,
    /// The column (0-based) the left border starts at: right after the art.
    col: usize,
    body_rows: usize,
    /// How far the bottom picture is raised.
    lift: Lift,
    /// How far the bottom picture is moved to the right.
    shift: Shift,
    left: Option<Placed>,
    bottom: Option<Placed>,
}

impl Frame {
    /// The text of the left border's row `y`, if it is drawn as cells.
    pub fn left_line(&self, y: usize) -> &str {
        match self.left.as_ref().map(|p| &p.look) {
            Some(Look::Cells(lines)) => lines.get(y).map_or("", String::as_str),
            _ => "",
        }
    }

    /// Where the data section begins, which is where the borders start.
    #[cfg(test)]
    pub fn col(&self) -> usize {
        self.col
    }

    /// Columns of space to put before the bottom border's text.
    pub fn bottom_indent(&self) -> usize {
        self.shift.cols
    }

    /// Rows the bottom border takes under the pane.
    pub fn bottom_rows(&self) -> usize {
        self.bottom.as_ref().map_or(0, |p| p.rows)
    }

    pub fn bottom_line(&self, r: usize) -> &str {
        match self.bottom.as_ref().map(|p| &p.look) {
            Some(Look::Cells(lines)) => lines.get(r).map_or("", String::as_str),
            _ => "",
        }
    }

    fn images(&self) -> impl Iterator<Item = (&Placed, u32, usize, usize, &[u8])> {
        [&self.left, &self.bottom]
            .into_iter()
            .flatten()
            .filter_map(|p| match &p.look {
                Look::Image { id, w, h, rgba } => Some((p, *id, *w, *h, rgba.as_slice())),
                Look::Cells(_) => None,
            })
    }

    pub fn has_images(&self) -> bool {
        self.images().next().is_some()
    }

    /// Send the pictures, replacing any this program sent before.
    pub fn transmit(&self, out: &mut String) {
        for (_, id, w, h, rgba) in self.images() {
            kitty::transmit(out, id, w, h, rgba);
        }
    }

    /// Show them, the pane's first row being screen row `top` (1-based).
    pub fn place_at(&self, top: usize, out: &mut String) {
        let col = self.col + 1;

        if let Some(p) = &self.left
            && let Look::Image { id, .. } = p.look
        {
            out.push_str(&format!("\x1b[{top};{col}H"));
            kitty::place(out, id, p.cols, p.rows, 0, 0);
        }
        if let Some(p) = &self.bottom
            && let Look::Image { id, .. } = p.look
        {
            let row = top + self.body_rows - self.lift.rows;
            let at = col + self.shift.cols;
            out.push_str(&format!("\x1b[{row};{at}H"));
            kitty::place(out, id, p.cols, p.rows, self.shift.x_px, self.lift.y_px);
        }
    }

    /// Show them with the cursor standing at the pane's top-left, for the
    /// modes that print down the page and cannot address rows.
    pub fn place_here(&self, out: &mut String) {
        let col = self.col + 1;

        if let Some(p) = &self.left
            && let Look::Image { id, .. } = p.look
        {
            out.push_str(&format!("\x1b[{col}G"));
            kitty::place(out, id, p.cols, p.rows, 0, 0);
        }
        if let Some(p) = &self.bottom
            && let Look::Image { id, .. } = p.look
        {
            // `0` would mean one to a terminal, so there is no move at all.
            let down = self.body_rows - self.lift.rows;
            if down > 0 {
                out.push_str(&format!("\x1b[{down}B"));
            }
            out.push_str(&format!("\x1b[{}G", col + self.shift.cols));
            kitty::place(out, id, p.cols, p.rows, self.shift.x_px, self.lift.y_px);
            if down > 0 {
                out.push_str(&format!("\x1b[{down}A"));
            }
        }
    }
}

/// Whether this terminal shows pictures, in words, and what its cells measure.
/// For `--list-borders`, so that "nothing appeared" can be told from "this
/// terminal cannot".
pub fn report() -> String {
    let cell = crate::term::cell_pixels().map_or_else(
        || "cell size unknown".to_string(),
        |(w, h)| format!("cells are {w}x{h} px"),
    );
    if kitty::supported() {
        format!("this terminal shows pictures ({cell})")
    } else {
        "this terminal is not known to show pictures, so borders are skipped here \
         (they need kitty, WezTerm or Ghostty, and not inside tmux)"
            .to_string()
    }
}

/// Take this program's pictures off the screen and out of the terminal's
/// memory, whether or not a frame still holds them.
pub fn clear(out: &mut String) {
    kitty::delete(out, ID_LEFT);
    kitty::delete(out, ID_BOTTOM);
}

/// [`clear`], but only on a terminal that speaks the protocol: for the process
/// that did not draw the pictures and cannot know whether there were any.
pub fn clear_if_supported(out: &mut String) {
    if kitty::supported() {
        clear(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An opaque rectangle: a stand-in for an ornament of that shape.
    fn slab(w: usize, h: usize) -> Bitmap {
        let mut b = Bitmap::blank(w, h);
        for px in b.rgba.as_chunks_mut::<4>().0 {
            *px = [255, 255, 255, 255];
        }
        b
    }

    fn borders(left: Option<Bitmap>, bottom: Option<Bitmap>, mode: Mode) -> Borders {
        Borders::new(left, bottom, mode)
    }

    #[test]
    fn a_picture_keeps_its_proportions_inside_the_box_it_is_given() {
        let cell = Cell { w: 10, h: 20 };
        // 100x400 px is 1:4. In 20 rows (400 px) it is 100 px: 10 columns.
        assert_eq!(fit_box(&slab(100, 400), 30, 20, cell), (10, 20));
        // Capped at 5 columns it is shrunk, not squashed: 5 cols is 50 px, so
        // 200 px tall is 10 rows.
        assert_eq!(fit_box(&slab(100, 400), 5, 20, cell), (5, 10));
        // A box with no room has no picture.
        assert_eq!(fit_box(&slab(100, 400), 0, 20, cell), (0, 0));
    }

    #[test]
    fn braille_cells_are_two_dots_across_and_four_down() {
        // A square held to 3 rows is 12 dots tall, so 12 across: 6 columns.
        assert_eq!(fit_box(&slab(12, 12), 40, 3, Cell::DOTS), (6, 3));
    }

    #[test]
    fn a_small_picture_is_enlarged_to_fill_its_box() {
        // Fitting is to the box, not to the picture's own size: a 12 px square
        // in a 40 by 40 cell box is as big as the box allows, in proportion.
        assert_eq!(fit_box(&slab(12, 12), 40, 40, Cell::DOTS), (40, 20));
    }

    #[test]
    fn the_left_border_takes_columns_and_the_gap_and_is_no_taller_than_the_pane() {
        let b = borders(Some(slab(100, 400)), None, Mode::Image);
        let plan = b.plan(120, 20, 60, 0);
        let frame = b.frame(&plan, 20, 0);
        let left = frame.left.as_ref().expect("a left border was asked for");

        assert!(plan.indent > GAP, "{plan:?}");
        assert_eq!(plan.bottom_rows, 0);
        assert_eq!(frame.indent, plan.indent);
        assert!(left.rows <= 20, "taller than the pane: {}", left.rows);
        assert!(left.cols + GAP <= plan.indent, "wider than reserved");
    }

    #[test]
    fn the_left_border_never_takes_more_than_its_share_of_the_terminal() {
        // Very wide art for a very short pane would want a lot of columns.
        let b = borders(Some(slab(1000, 100)), None, Mode::Cells);
        let plan = b.plan(80, 20, 50, 0);

        assert!(plan.indent - GAP <= MAX_LEFT_COLS.min(80 / 4), "{plan:?}");
    }

    #[test]
    fn the_bottom_border_takes_rows_and_is_capped() {
        // Landscape, 10:1, as a sword lying down.
        let b = borders(None, Some(slab(1000, 100)), Mode::Image);
        let plan = b.plan(120, 10, 50, 0);

        assert!(
            (1..=MAX_BOTTOM_ROWS).contains(&plan.bottom_rows),
            "{plan:?}"
        );
        assert_eq!(plan.indent, 0, "no left border, no indent");
    }

    #[test]
    fn a_terminal_too_narrow_for_an_ornament_gets_none() {
        let b = borders(Some(slab(100, 400)), Some(slab(400, 100)), Mode::Image);
        assert_eq!(b.plan(MIN_COLS - 1, 20, 40, 0), Plan::default());
        assert_eq!(
            b.plan(120, 2, 40, 0),
            Plan::default(),
            "and a pane too short"
        );
    }

    #[test]
    fn cells_are_text_the_width_of_the_reservation_and_a_picture_is_not() {
        let b = borders(Some(slab(40, 80)), Some(slab(80, 20)), Mode::Cells);
        let plan = b.plan(100, 12, 40, 0);
        let frame = b.frame(&plan, 12, 0);

        assert!(!frame.has_images());
        let widest = (0..12)
            .map(|y| crate::render::visible_width_for_tests(frame.left_line(y)))
            .max()
            .unwrap();
        assert!(widest > 0 && widest <= plan.indent - GAP);
        assert_eq!(frame.bottom_rows(), plan.bottom_rows);
        assert!((0..frame.bottom_rows()).any(|r| !frame.bottom_line(r).trim().is_empty()));

        let image = borders(Some(slab(40, 80)), None, Mode::Image).frame(&plan, 12, 0);
        assert!(image.has_images());
        assert_eq!(image.left_line(0), "", "a picture is not printed");
    }

    #[test]
    fn pictures_are_sent_then_placed_at_the_pane_by_id() {
        let b = borders(Some(slab(40, 80)), Some(slab(80, 20)), Mode::Image);
        let plan = b.plan(100, 12, 40, 0);
        let frame = b.frame(&plan, 12, 0);

        let (mut sent, mut placed) = (String::new(), String::new());
        frame.transmit(&mut sent);
        frame.place_at(3, &mut placed);

        assert_eq!(sent.matches("a=t").count(), 2, "one picture per border");
        assert!(sent.contains(&format!("i={ID_LEFT}")) && sent.contains(&format!("i={ID_BOTTOM}")));
        assert!(!sent.contains("a=p"), "sending must not show anything");

        // The left starts at the pane's first row, the bottom below its body.
        assert!(placed.contains("\x1b[3;1H"), "{placed:?}");
        assert!(
            placed.contains(&format!("\x1b[{};1H", 3 + 12)),
            "{placed:?}"
        );
        assert_eq!(placed.matches("a=p").count(), 2);
    }

    #[test]
    fn placing_at_the_cursor_moves_down_for_the_bottom_and_back_again() {
        let b = borders(Some(slab(40, 80)), Some(slab(80, 20)), Mode::Image);
        let frame = b.frame(&b.plan(100, 12, 40, 0), 12, 0);

        let mut out = String::new();
        frame.place_here(&mut out);

        let down = out.find("\x1b[12B").expect("moves down by the body");
        let up = out.find("\x1b[12A").expect("and returns");
        assert!(down < up);
        assert!(
            out[..down].contains("a=p"),
            "the left goes where the cursor is"
        );
        assert!(out[down..up].contains("a=p"), "the bottom after the move");
    }

    #[test]
    fn half_a_row_is_the_row_above_pushed_down_by_half_a_cell() {
        // A cell 20 px high, lifted 0.5 row: start in the row above, 10 px down.
        assert_eq!(Lift::new(0.5, 20, 10), Lift { rows: 1, y_px: 10 });
        // A whole row is the row above with no push back.
        assert_eq!(Lift::new(1.0, 20, 10), Lift { rows: 1, y_px: 0 });
        // A row and a half is two rows up, pushed down half a cell.
        assert_eq!(Lift::new(1.5, 20, 10), Lift { rows: 2, y_px: 10 });
        assert_eq!(Lift::new(0.0, 20, 10), Lift::default());
    }

    #[test]
    fn the_lift_is_what_it_says_in_pixels() {
        // Origin row minus rows, plus the push: the picture's top edge sits
        // exactly `lift` rows above where it would have been.
        for (lift, cell_h) in [(0.25, 20), (0.5, 20), (1.0, 20), (1.75, 24), (0.3, 17)] {
            let l = Lift::new(lift, cell_h, 12);
            let up = l.rows * cell_h - l.y_px;
            let wanted = (lift * cell_h as f32).round() as usize;
            assert_eq!(up, wanted, "lift {lift} at {cell_h} px");
            assert!(l.y_px < cell_h, "an offset must be inside one cell");
        }
    }

    #[test]
    fn the_lift_is_clamped_and_never_leaves_the_pane() {
        assert_eq!(Lift::new(50.0, 20, 10).rows, 3, "capped at the maximum");
        assert_eq!(Lift::new(-1.0, 20, 10), Lift::default(), "no negative lift");
        assert_eq!(Lift::new(f32::NAN, 20, 10), Lift::default());
        // A pane one row tall cannot be lifted by three.
        assert!(Lift::new(3.0, 20, 1).rows <= 1);
        assert_eq!(
            Lift::new(0.5, 0, 10),
            Lift::default(),
            "no cell size, no lift"
        );
    }

    #[test]
    fn a_lifted_bottom_picture_starts_a_row_higher_with_an_offset() {
        let b = borders(None, Some(slab(80, 20)), Mode::Image).with_lift(0.5);
        let cell = b.cell();
        let frame = b.frame(&b.plan(100, 12, 40, 0), 12, 0);

        let mut out = String::new();
        frame.place_at(3, &mut out);

        // One row higher than 3 + 12, and pushed back down by half a cell.
        assert!(out.contains(&format!("\x1b[{};1H", 3 + 12 - 1)), "{out:?}");
        assert!(out.contains(&format!(",Y={},", cell.h / 2)), "{out:?}");
    }

    #[test]
    fn no_lift_places_the_picture_exactly_as_before() {
        let b = borders(None, Some(slab(80, 20)), Mode::Image);
        let frame = b.frame(&b.plan(100, 12, 40, 0), 12, 0);

        let mut out = String::new();
        frame.place_at(3, &mut out);
        assert!(out.contains(&format!("\x1b[{};1H", 3 + 12)), "{out:?}");
        assert!(!out.contains("Y="));
    }

    #[test]
    fn placing_at_the_cursor_moves_one_row_less_when_lifted() {
        let b = borders(None, Some(slab(80, 20)), Mode::Image).with_lift(0.5);
        let frame = b.frame(&b.plan(100, 12, 40, 0), 12, 0);

        let mut out = String::new();
        frame.place_here(&mut out);

        assert!(out.contains("\x1b[11B"), "{out:?}");
        assert!(out.contains("\x1b[11A"), "and back the same way: {out:?}");
        assert!(out.contains("Y="));
    }

    #[test]
    fn a_lift_that_leaves_no_row_to_move_down_emits_no_zero_move() {
        // `CSI 0 B` moves one row, so a move of none must not be written.
        let b = borders(None, Some(slab(80, 20)), Mode::Image).with_lift(1.0);
        let frame = b.frame(&b.plan(100, 3, 40, 0), 1, 0);

        let mut out = String::new();
        frame.place_here(&mut out);
        assert!(!out.contains("[0B") && !out.contains("[0A"), "{out:?}");
    }

    #[test]
    fn cells_cannot_be_raised_by_part_of_a_row_so_they_are_not() {
        let b = borders(None, Some(slab(80, 20)), Mode::Cells).with_lift(0.5);
        let frame = b.frame(&b.plan(100, 12, 40, 0), 12, 0);
        assert_eq!(frame.lift, Lift::default());
    }

    #[test]
    fn a_shift_is_whole_columns_and_the_pixels_left_over() {
        // 10 px columns: two and a half columns is two, and 5 px more.
        assert_eq!(Shift::new(2.5, 10), Shift { cols: 2, x_px: 5 });
        assert_eq!(Shift::new(2.0, 10), Shift { cols: 2, x_px: 0 });
        assert_eq!(Shift::new(0.0, 10), Shift::default());
        assert_eq!(Shift::new(-3.0, 10), Shift::default(), "never to the left");
        assert_eq!(Shift::new(f32::NAN, 10), Shift::default());
        assert_eq!(
            Shift::new(2.0, 0),
            Shift::default(),
            "no cell size, no move"
        );
        assert_eq!(Shift::new(99.0, 10).cols, 24, "capped at the maximum");
    }

    #[test]
    fn a_shift_is_what_it_says_in_pixels_and_stays_inside_one_cell() {
        for (shift, cell_w) in [(0.25, 10), (1.0, 12), (2.5, 9), (3.3, 17)] {
            let s = Shift::new(shift, cell_w);
            let wanted = (shift * cell_w as f32).round() as usize;
            assert_eq!(
                s.cols * cell_w + s.x_px,
                wanted,
                "shift {shift} at {cell_w}"
            );
            assert!(s.x_px < cell_w, "an offset must be inside one cell");
        }
    }

    #[test]
    fn a_shifted_bottom_picture_starts_further_right_with_an_offset() {
        let b = borders(None, Some(slab(80, 20)), Mode::Image).with_shift(2.5);
        let cell = b.cell();
        let frame = b.frame(&b.plan(100, 12, 40, 0), 12, 0);

        let mut out = String::new();
        frame.place_at(3, &mut out);

        // Two columns on from column 1, and half a cell more inside it.
        assert!(out.contains(&format!("\x1b[{};3H", 3 + 12)), "{out:?}");
        assert!(out.contains(&format!(",X={},", cell.w / 2)), "{out:?}");
    }

    #[test]
    fn placing_at_the_cursor_moves_to_the_shifted_column() {
        let b = borders(None, Some(slab(80, 20)), Mode::Image).with_shift(3.0);
        let frame = b.frame(&b.plan(100, 12, 40, 0), 12, 4);

        let mut out = String::new();
        frame.place_here(&mut out);

        // Column 4 (0-based) is 5, and three on from there is 8.
        assert!(out.contains("\x1b[8G"), "{out:?}");
        assert!(!out.contains("X="), "a whole column needs no offset");
    }

    #[test]
    fn no_shift_places_the_picture_exactly_as_before() {
        let b = borders(None, Some(slab(80, 20)), Mode::Image);
        let frame = b.frame(&b.plan(100, 12, 40, 0), 12, 0);

        let mut out = String::new();
        frame.place_at(3, &mut out);
        assert!(!out.contains("X="), "{out:?}");
        assert_eq!(frame.bottom_indent(), 0);
    }

    #[test]
    fn a_shifted_picture_never_runs_past_the_terminal() {
        for cols in [MIN_COLS, 70, 100, 140] {
            for shift in [0.0, 1.0, 2.5, 8.0, 24.0] {
                let b = borders(None, Some(slab(400, 40)), Mode::Image).with_shift(shift);
                let plan = b.plan(cols, 12, 200, 0);
                let reach = plan.bottom_shift.cols
                    + usize::from(plan.bottom_shift.x_px > 0)
                    + plan.bottom_cols;
                assert!(reach <= cols, "{reach} > {cols} at shift {shift}");
            }
        }
    }

    #[test]
    fn cells_move_by_whole_columns_and_the_text_is_indented_to_match() {
        let b = borders(None, Some(slab(80, 20)), Mode::Cells).with_shift(2.4);
        let frame = b.frame(&b.plan(100, 12, 40, 0), 12, 0);
        assert_eq!(frame.bottom_indent(), 2);
        assert_eq!(frame.shift.x_px, 0);
    }

    #[test]
    fn clearing_removes_both_pictures_by_id_and_only_those() {
        let mut out = String::new();
        clear(&mut out);

        assert_eq!(out.matches("a=d,d=I").count(), 2);
        assert!(out.contains(&format!("i={ID_LEFT}")) && out.contains(&format!("i={ID_BOTTOM}")));
        assert!(
            !out.contains("d=A"),
            "must not delete other programs' pictures"
        );
    }

    #[test]
    fn the_mode_follows_the_terminal_unless_it_is_told_otherwise() {
        assert_eq!(Mode::resolve(BorderRender::Auto, true), Some(Mode::Image));
        assert_eq!(Mode::resolve(BorderRender::Cells, true), Some(Mode::Cells));
        assert_eq!(Mode::resolve(BorderRender::Image, false), Some(Mode::Image));
    }

    #[test]
    fn auto_never_turns_a_picture_into_text() {
        // The point of the setting: where pictures cannot be shown there is no
        // border, not a drawing of one. Text is only for "cells", by name.
        assert_eq!(Mode::resolve(BorderRender::Auto, false), None);
    }

    #[test]
    #[cfg(feature = "bundled-borders")]
    fn a_terminal_that_cannot_show_pictures_gets_no_border_and_no_decoding() {
        let cfg = Config {
            color: true,
            border_left: "00".into(),
            border_bottom: "00".into(),
            border_render: BorderRender::Auto,
            ..Config::default()
        };

        // `images` is false: not a terminal that shows them.
        assert!(Borders::load(&cfg, None, false, true).unwrap().is_none());

        // A mistake is still a mistake.
        let wrong = Config {
            border_left: "nope".into(),
            ..cfg
        };
        assert!(Borders::load(&wrong, None, false, true).is_err());
    }

    #[test]
    #[cfg(feature = "bundled-borders")]
    fn no_colour_still_shows_pictures_but_not_the_text_drawing() {
        let mut cfg = Config {
            color: false,
            border_left: "00".into(),
            border_render: BorderRender::Image,
            ..Config::default()
        };

        let b = Borders::load(&cfg, None, true, true)
            .unwrap()
            .expect("borders");
        assert_eq!(b.mode, Mode::Image, "a picture has no colour to turn off");

        cfg.border_render = BorderRender::Cells;
        assert!(
            Borders::load(&cfg, None, true, true).unwrap().is_none(),
            "text in no colour is not drawn"
        );
    }

    #[test]
    #[cfg(feature = "bundled-borders")]
    fn a_pipe_gets_no_border_but_a_wrong_name_is_still_an_error() {
        let mut cfg = Config {
            border_left: "00".into(),
            border_render: BorderRender::Image,
            ..Config::default()
        };

        // Not a terminal: no pictures, even when they are asked for by name.
        assert!(Borders::load(&cfg, None, true, false).unwrap().is_none());

        cfg.border_left = "no-such-border".into();
        let err = Borders::load(&cfg, None, true, false)
            .err()
            .expect("an error");
        assert!(err.contains("no-such-border"), "{err}");

        // Nothing asked for is nothing.
        let none = Config::default();
        assert!(Borders::load(&none, None, true, true).unwrap().is_none());
    }

    #[test]
    #[cfg(feature = "bundled-borders")]
    fn a_wanted_border_is_decoded_oriented_and_given_a_mode() {
        let cfg = Config {
            color: true,
            border_left: "00".into(),
            border_bottom: "00".into(),
            border_render: BorderRender::Cells,
            ..Config::default()
        };
        let b = Borders::load(&cfg, None, true, true)
            .unwrap()
            .expect("borders");

        let left = b.left.as_ref().unwrap();
        let bottom = b.bottom.as_ref().unwrap();
        assert!(left.h > left.w, "a left border stays upright");
        assert!(
            bottom.w > bottom.h,
            "a bottom one has been turned onto its side"
        );
        assert_eq!(b.mode, Mode::Cells);
    }

    #[test]
    fn a_frame_with_no_images_clears_nothing_and_places_nothing() {
        let frame = borders(None, None, Mode::Image).frame(&Plan::default(), 10, 0);
        let mut out = String::new();
        frame.transmit(&mut out);
        frame.place_at(1, &mut out);
        frame.place_here(&mut out);

        assert!(out.is_empty());
        assert_eq!(frame.bottom_rows(), 0);
    }
}
