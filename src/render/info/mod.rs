//! The info pane, in seven styles.
//!
//! A style is a handful of row types. Each row is measured and painted by one
//! type, side by side, so the width the art is fitted to cannot drift from what
//! is drawn: [`build`] returns the rows, and both questions are asked of them.
//!
//! Adding a style is a module with a `Row` enum, a `rows` function and a
//! [`Line`] impl, plus one arm in [`build`] and one in `InfoStyle`.

mod boxed;
mod card;
mod chips;
mod classic;
mod group;
mod iconic;
mod sections;
mod tree;
mod value;

use super::paint::Painter;
use crate::config::{Config, InfoStyle};
use crate::fetch::Item;

/// One row of an info pane.
trait Line {
    /// Columns this row draws when given all it wants.
    fn width(&self) -> usize;
    /// Draw into `width` columns, cutting or simplifying to fit.
    fn paint(&self, p: &mut Painter<'_>, width: usize);
}

/// A laid-out pane, whatever its style.
pub trait Info {
    fn len(&self) -> usize;
    /// The widest row.
    fn width(&self) -> usize;
    fn paint(&self, row: usize, p: &mut Painter<'_>, width: usize);
}

struct Rows<L>(Vec<L>);

impl<L: Line> Info for Rows<L> {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn width(&self) -> usize {
        self.0.iter().map(Line::width).max().unwrap_or(0)
    }

    fn paint(&self, row: usize, p: &mut Painter<'_>, width: usize) {
        if width > 0 {
            self.0[row].paint(p, width);
        }
    }
}

/// The pane `cfg.info_style` asks for, from the rows collected.
pub fn build<'a>(title: &'a str, items: &'a [Item], cfg: &Config) -> Box<dyn Info + 'a> {
    match cfg.info_style {
        InfoStyle::Classic => Box::new(Rows(classic::rows(title, items, cfg))),
        // No title: the box and its labels are the whole of the pane.
        InfoStyle::Boxed => Box::new(Rows(boxed::rows(items, cfg))),
        // Its title is the user box, not a row above a rule.
        InfoStyle::Tree => Box::new(Rows(tree::rows(title, items, cfg))),
        // Sections carry their own titles; `user@host` is not among them.
        InfoStyle::Sections => Box::new(Rows(sections::rows(items, cfg))),
        // Its headings are chips; there is no row for `user@host` either.
        InfoStyle::Chips => Box::new(Rows(chips::rows(items, cfg))),
        // The user's name is a row of its own, and there are no labels.
        InfoStyle::Iconic => Box::new(Rows(iconic::rows(title, items, cfg))),
        // A list in `modules` order, with no row for `user@host`.
        InfoStyle::Card => Box::new(Rows(card::rows(items, cfg))),
    }
}

/// Swatch rows carry no label and are pre-coloured: they are decoration, so
/// there is nothing to show without colour.
fn is_swatch(item: &Item) -> bool {
    item.label.is_empty()
}
