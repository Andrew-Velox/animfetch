//! A user box over two groups, OS and PC, drawn as a tree of icons beside a
//! column of `: value` lines.
//!
//! The set of rows is fixed by the layout. Anything else in `modules` has no
//! place in it and is left out rather than squeezed in.

use unicode_width::UnicodeWidthStr;

use super::classic::paint_pair;
use super::group::{Group, UNKNOWN};
use super::{Line, value};
use crate::config::Config;
use crate::fetch::{Detail, Item};
use crate::render::icons::ICON_COLS;
use crate::render::paint::Painter;
use crate::render::text::truncate;

const GROUPS: &[Group] = &[
    Group {
        name: "OS",
        head: &["OS"],
        children: &["Shell", "Terminal"],
    },
    Group {
        name: "PC",
        head: &["Host", "CPU"],
        children: &["GPU", "Memory", "Display"],
    },
];

/// A box's two borders.
const BORDERS: usize = 2;

/// The gap between the two boxes.
const GAP: usize = 1;

/// The fewest columns of value worth drawing a box around.
const MIN_VALUE: usize = 5;

/// Columns the right box spends beside its value: a space, the colon and its
/// space, and a space before the border.
const VALUE_MARGIN: usize = 4;

/// Trunk, space, branch and its dash.
const BRANCH: usize = 4;

/// What goes in the left box on one line. Marks are resolved when the rows are
/// built, so painting does not need to know whether icons are on.
pub enum Cell {
    /// The user icon, or the word in its place, centred above the groups.
    User { mark: &'static str },
    /// A group's heading: its icon, if there are icons, and its name.
    Head {
        icon: Option<&'static str>,
        label: &'static str,
    },
    /// A child of the group above, hung off its trunk.
    Branch { mark: &'static str, last: bool },
}

pub enum Row {
    /// Top or bottom edge of the two boxes side by side.
    Edge {
        bottom: bool,
        left: usize,
        right: usize,
    },
    /// A cell in the left box, a value in the right.
    Line {
        cell: Cell,
        left: usize,
        right: usize,
        /// What the row is, for the plain fallback on a very narrow terminal.
        name: &'static str,
        text: String,
    },
    /// Between the user box and the groups.
    Blank,
}

/// The text a row shows here. Usage is the bare reading; the percentage and
/// its colour belong to the other styles.
fn text_of(item: &Item) -> String {
    match &item.detail {
        Some(Detail::Usage(u)) => u.short(),
        _ => value::text(item).to_string(),
    }
}

pub fn rows(title: &str, items: &[Item], cfg: &Config) -> Vec<Row> {
    // (cell, what it is, its text), top to bottom.
    let mut lines: Vec<(Cell, &'static str, String)> = Vec::new();
    for group in GROUPS {
        let Some(members) = group.members(items) else {
            continue;
        };

        lines.push((
            Cell::Head {
                icon: cfg.icons.then(|| group.icon()),
                label: group.name,
            },
            group.name,
            members.head.map_or_else(|| UNKNOWN.to_string(), text_of),
        ));
        let last = members.leaves.len().saturating_sub(1);
        for (n, (item, glyph)) in members.leaves.into_iter().enumerate() {
            lines.push((
                Cell::Branch {
                    mark: if cfg.icons { glyph.icon } else { glyph.short },
                    last: n == last,
                },
                item.name(),
                text_of(item),
            ));
        }
    }

    // One width for both right-hand boxes, so the sections line up.
    let widest = lines
        .iter()
        .map(|l| l.2.width())
        .chain(std::iter::once(title.width()))
        .max()
        .unwrap_or(0);
    let right = widest + VALUE_MARGIN;

    // The user box is measured as what it holds, or it can come out narrower
    // than its own content when no group is there to widen the column.
    let user = Cell::User {
        mark: if cfg.icons { "\u{f007}" } else { "user" },
    };
    let left = lines
        .iter()
        .map(|l| &l.0)
        .chain(std::iter::once(&user))
        .map(cell_width)
        .max()
        .unwrap_or(1)
        // A space of padding either side.
        + 2;

    let edge = |bottom| Row::Edge {
        bottom,
        left,
        right,
    };
    let mut rows = Vec::with_capacity(lines.len() + 6);

    rows.push(edge(false));
    rows.push(Row::Line {
        cell: user,
        left,
        right,
        name: "user",
        text: title.to_string(),
    });
    rows.push(edge(true));

    if !lines.is_empty() {
        rows.push(Row::Blank);
        rows.push(edge(false));
        for (cell, name, text) in lines {
            rows.push(Row::Line {
                cell,
                left,
                right,
                name,
                text,
            });
        }
        rows.push(edge(true));
    }
    rows
}

fn cell_width(cell: &Cell) -> usize {
    match cell {
        Cell::User { mark } => mark.width(),
        Cell::Head { icon, label } => label.width() + icon.map_or(0, |_| ICON_COLS),
        Cell::Branch { mark, .. } => BRANCH + mark.width(),
    }
}

/// The right box's width when only `width` columns are there, or `None` if
/// there is not room for even a few characters of it. Shrinking it keeps the
/// boxes on a tight terminal; the value is cut instead.
fn fit(left: usize, right: usize, width: usize) -> Option<usize> {
    let room = width.checked_sub(left + BORDERS + GAP + BORDERS)?;
    (room >= MIN_VALUE).then(|| right.min(room))
}

impl Line for Row {
    fn width(&self) -> usize {
        match *self {
            Row::Edge { left, right, .. } | Row::Line { left, right, .. } => {
                left + BORDERS + GAP + right + BORDERS
            }
            Row::Blank => 0,
        }
    }

    fn paint(&self, p: &mut Painter<'_>, width: usize) {
        match self {
            Row::Blank => {}
            Row::Edge {
                bottom,
                left,
                right,
            } => {
                if let Some(right) = fit(*left, *right, width) {
                    paint_edge(p, *bottom, *left, right);
                }
            }
            Row::Line {
                cell,
                left,
                right,
                name,
                text,
            } => match fit(*left, *right, width) {
                Some(right) => paint_line(p, cell, *left, right, text),
                // No room for two boxes: the same facts, plainly.
                None => paint_pair(p, name, text, width),
            },
        }
    }
}

fn paint_edge(p: &mut Painter<'_>, bottom: bool, left: usize, right: usize) {
    let (l, r) = if bottom {
        ('┗', '┛')
    } else {
        ('┏', '┓')
    };
    let mut edge = String::new();
    for (n, inner) in [left, right].into_iter().enumerate() {
        if n > 0 {
            edge.push(' ');
        }
        edge.push(l);
        edge.extend(std::iter::repeat_n('━', inner));
        edge.push(r);
    }
    let accent = p.accent();
    p.text(accent, &edge);
}

fn paint_line(p: &mut Painter<'_>, cell: &Cell, left: usize, right: usize, text: &str) {
    let (accent, color) = (p.accent(), p.value());

    // Left box.
    p.text(accent, "┃");
    match cell {
        Cell::User { mark } => {
            let lead = (left - mark.width()) / 2;
            p.space(lead);
            p.pad(color, mark, left - lead);
        }
        Cell::Head { icon, label } => {
            p.space(1);
            let mut head = String::new();
            if let Some(icon) = icon {
                head.push_str(icon);
                head.push(' ');
            }
            head.push_str(label);
            p.pad(color, &head, left - 1);
        }
        Cell::Branch { mark, last } => {
            p.space(1);
            // The trunk runs down the group and ends at its last child.
            let trunk = if *last { "└ └─" } else { "│ ├─" };
            p.pad(accent, trunk, BRANCH);
            p.pad(color, mark, left - 1 - BRANCH);
        }
    }
    p.text(accent, "┃");
    p.space(GAP);

    // Right box. The colon sits inside it, apart from the value.
    let room = right - VALUE_MARGIN;
    p.text(accent, "┃");
    p.space(1);
    p.pad(color, ": ", 2);
    p.pad(color, &truncate(text, room), room);
    p.space(1);
    p.text(accent, "┃");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::InfoStyle;
    use crate::render::test_util::*;
    use crate::render::{compose, info_width};

    fn tree_cfg() -> Config {
        Config {
            info_style: InfoStyle::Tree,
            icons: true,
            color: false,
            ..plain_cfg()
        }
    }

    fn tree_items() -> Vec<Item> {
        use crate::fetch::Usage;
        vec![
            Item::new("OS", "Fedora Linux"),
            Item::new("Shell", "zsh 5.9"),
            Item::new("Terminal", "kitty 0.44.0"),
            Item::new("Host", "HP Laptop 15s-eq2xxx"),
            Item::new("GPU", "AMD Integrated"),
            Item::usage("Memory", Some(Usage::new(2_694_000, 15_660_000))).unwrap(),
            Item::new("Display", "1920x1080 @ 60.00 Hz"),
            // Not part of the layout, so it must not appear.
            Item::new("Kernel", "6.9.0"),
        ]
    }

    fn tree_lines(items: &[Item], cfg: &Config, width: usize) -> Vec<String> {
        compose(&scene(&[], 0, "harsh@fedora", items, width), cfg)
            .iter()
            .map(|l| strip_ansi(l))
            .collect()
    }

    #[test]
    fn tree_is_two_sections_of_boxes_that_all_share_one_width() {
        let lines = tree_lines(&tree_items(), &tree_cfg(), 100);
        let drawn: Vec<&String> = lines.iter().filter(|l| !l.is_empty()).collect();

        let width = drawn[0].width();
        assert!(
            drawn.iter().all(|l| l.width() == width),
            "ragged: {drawn:#?}"
        );
        assert!(drawn[0].starts_with('┏') && drawn[0].ends_with('┓'));
        assert!(lines.iter().any(|l| l.is_empty()), "a gap between sections");
        assert!(lines[1].contains(": harsh@fedora"));
    }

    #[test]
    fn tree_groups_hang_their_children_off_a_trunk_that_ends_at_the_last() {
        let lines = tree_lines(&tree_items(), &tree_cfg(), 100);
        let body = lines.join("\n");

        // OS has two children, PC three: five branches, two groups' last.
        assert_eq!(body.matches('├').count(), 3, "{body}");
        assert_eq!(body.matches("└─").count(), 2, "{body}");
        assert!(body.contains("OS") && body.contains("PC"));
        assert!(body.contains(": Fedora Linux"));
        assert!(
            body.contains(": HP Laptop 15s-eq2xxx"),
            "PC is headed by the host"
        );
    }

    #[test]
    fn tree_shows_memory_as_the_bare_reading_and_leaves_other_rows_out() {
        let body = tree_lines(&tree_items(), &tree_cfg(), 100).join("\n");

        assert!(body.contains(": 2.57 GiB/14.93 GiB"), "{body}");
        assert!(!body.contains('%'), "no percentage in this style: {body}");
        assert!(!body.contains("6.9.0"), "kernel has no place here: {body}");
    }

    #[test]
    fn tree_falls_back_to_the_cpu_for_a_machine_with_no_host_name() {
        let mut items = tree_items();
        items.retain(|i| i.label != "Host");
        items.push(Item::new("CPU", "AMD Ryzen 7 7700"));

        let body = tree_lines(&items, &tree_cfg(), 100).join("\n");
        assert!(body.contains(": AMD Ryzen 7 7700"), "{body}");
    }

    #[test]
    fn tree_names_a_heading_it_cannot_fill_rather_than_dropping_its_children() {
        let items = vec![Item::new("GPU", "AMD Integrated")];
        let body = tree_lines(&items, &tree_cfg(), 100).join("\n");

        assert!(body.contains("PC"), "{body}");
        assert!(body.contains(": unknown"), "{body}");
        assert!(body.contains(": AMD Integrated"), "{body}");
        assert!(!body.contains("OS"), "an empty group is not drawn: {body}");
    }

    #[test]
    fn tree_without_icons_names_each_branch_and_stays_aligned() {
        let cfg = Config {
            icons: false,
            ..tree_cfg()
        };
        let lines = tree_lines(&tree_items(), &cfg, 100);
        let drawn: Vec<&String> = lines.iter().filter(|l| !l.is_empty()).collect();
        let body = lines.join("\n");

        assert!(body.contains("shell") && body.contains("term") && body.contains("disp"));
        assert!(!body.contains('\u{f17c}'));
        let width = drawn[0].width();
        assert!(
            drawn.iter().all(|l| l.width() == width),
            "ragged: {drawn:#?}"
        );
    }

    #[test]
    fn tree_keeps_its_boxes_on_a_tight_terminal_by_cutting_the_value() {
        let items = tree_items();
        let lines = tree_lines(&items, &tree_cfg(), 28);

        assert!(
            lines.iter().any(|l| l.starts_with('┏')),
            "boxes were dropped"
        );
        assert!(lines.iter().any(|l| l.contains('…')), "value was not cut");
        for line in &lines {
            assert!(line.width() <= 28, "too wide: {line:?}");
        }
    }

    #[test]
    fn tree_never_exceeds_a_terminal_however_narrow() {
        for width in [4, 8, 12, 16, 20, 24, 40] {
            for line in tree_lines(&tree_items(), &tree_cfg(), width) {
                assert!(line.width() <= width, "{width}: {line:?}");
            }
        }
    }

    #[test]
    fn tree_without_icons_and_without_rows_still_holds_its_user_word() {
        // The user box was measured as one icon column and painted as the four
        // letters of "user", which underflowed. Nothing else widens it when no
        // group is drawn, so this is the case that showed it.
        let cfg = Config {
            icons: false,
            ..tree_cfg()
        };
        let lines = tree_lines(&[], &cfg, 100);

        assert!(lines.iter().any(|l| l.contains("user")), "{lines:?}");
        let width = lines[0].width();
        assert!(
            lines
                .iter()
                .filter(|l| !l.is_empty())
                .all(|l| l.width() == width)
        );
    }

    #[test]
    fn tree_measures_the_width_it_draws() {
        let cfg = tree_cfg();
        let items = tree_items();
        let drawn = tree_lines(&items, &cfg, 200)
            .iter()
            .map(|l| l.width())
            .max()
            .unwrap();

        assert_eq!(info_width("harsh@fedora", &items, &cfg), drawn);
    }
}
