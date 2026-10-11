//! Short lowercase labels with icons in a rounded box, values beside it.

use unicode_width::UnicodeWidthStr;

use super::classic::paint_pair;
use super::{Line, is_swatch, value};
use crate::config::Config;
use crate::fetch::{Detail, Item};
use crate::render::icons::{self, ICON_COLS};
use crate::render::paint::{Painter, Seg, segs_width};
use crate::render::text::visible_width;

/// Blank columns between the box and the values beside it.
const GAP: usize = 2;

pub enum Row<'a> {
    /// Top edge. The number is the width between the corners.
    Top(usize),
    Bottom(usize),
    /// One label inside the box, with its value beside it outside.
    Entry {
        inner: usize,
        mark: Option<&'static str>,
        name: String,
        name_w: usize,
        /// The row as plain text, for a terminal too narrow for the box.
        label: &'a str,
        plain: &'a str,
        segs: Vec<Seg>,
    },
    Raw(&'a str),
}

/// Labels in a rounded box, padded to one column so its edge is clean.
pub fn rows<'a>(items: &'a [Item], cfg: &Config) -> Vec<Row<'a>> {
    let entries: Vec<(&'static str, String, &Item)> = items
        .iter()
        .filter(|i| !is_swatch(i))
        .map(|i| {
            let (icon, name) = match icons::lookup(i.label) {
                Some(g) => (g.icon, g.name.to_string()),
                None => (icons::GENERIC.icon, i.label.to_lowercase()),
            };
            // Their own word, as they wrote it, over the short lowercase one.
            let name = i.renamed_to().map_or(name, str::to_string);
            (icon, name, i)
        })
        .collect();

    let mut rows = Vec::with_capacity(entries.len() + 3);

    if !entries.is_empty() {
        let name_w = entries.iter().map(|e| e.1.width()).max().unwrap_or(0);
        // A space of padding either side of the icon and label.
        let inner = 1 + if cfg.icons { ICON_COLS } else { 0 } + name_w + 1;

        rows.push(Row::Top(inner));
        for (icon, name, item) in entries {
            rows.push(Row::Entry {
                inner,
                mark: cfg.icons.then_some(icon),
                name,
                name_w,
                label: item.name(),
                plain: &item.value,
                segs: segs(item),
            });
        }
        rows.push(Row::Bottom(inner));
    }

    // Swatches, if asked for, sit under the box like any other raw row.
    if cfg.swatches() {
        rows.extend(
            items
                .iter()
                .filter(|i| is_swatch(i))
                .map(|i| Row::Raw(&i.value)),
        );
    }
    rows
}

/// A usage reading in full with its percentage painted; anything else as text.
fn segs(item: &Item) -> Vec<Seg> {
    match &item.detail {
        Some(Detail::Usage(u)) => value::usage_segs(u),
        _ => value::plain(value::text(item)),
    }
}

impl Line for Row<'_> {
    fn width(&self) -> usize {
        match self {
            Row::Top(inner) | Row::Bottom(inner) => inner + 2,
            Row::Entry { inner, segs, .. } => inner + 2 + GAP + segs_width(segs),
            Row::Raw(raw) => visible_width(raw),
        }
    }

    fn paint(&self, p: &mut Painter<'_>, width: usize) {
        let (accent, color) = (p.accent(), p.value());

        match self {
            Row::Top(inner) | Row::Bottom(inner) => {
                let (l, r) = if matches!(self, Row::Top(_)) {
                    ('╭', '╮')
                } else {
                    ('╰', '╯')
                };
                let mut edge = String::with_capacity(inner * 3 + 6);
                edge.push(l);
                edge.extend(std::iter::repeat_n('─', *inner));
                edge.push(r);
                // A cut edge is clearer than a wrapped one.
                p.faint(accent, &edge.chars().take(width).collect::<String>());
            }
            Row::Entry {
                inner,
                mark,
                name,
                name_w,
                label,
                plain,
                segs,
            } => {
                let box_w = inner + 2;
                // No room for the box and a value beside it: show the row
                // plainly rather than a box cut in half.
                if width < box_w + GAP + 1 {
                    return paint_pair(p, label, plain, width);
                }

                let mut lead = String::new();
                if let Some(mark) = mark {
                    lead.push_str(mark);
                    lead.push(' ');
                }
                lead.push_str(name);
                lead.extend(std::iter::repeat_n(' ', name_w - name.width()));

                p.faint(accent, "│");
                p.space(1);
                p.faint(color, &lead);
                p.space(1);
                p.faint(accent, "│");
                p.space(GAP);
                p.segs(segs, width - box_w - GAP);
            }
            Row::Raw(raw) => p.raw_fit(raw, width),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::InfoStyle;
    use crate::render::test_util::*;
    use crate::render::{compose, info_width};

    fn boxed_cfg() -> Config {
        Config {
            info_style: InfoStyle::Boxed,
            icons: true,
            color: false,
            ..plain_cfg()
        }
    }

    fn boxed_items() -> Vec<Item> {
        use crate::fetch::Usage;
        vec![
            Item::new("OS", "Arch Linux x86_64"),
            Item::new("Terminal", "ghostty"),
            Item::usage(
                "Disk",
                Some(Usage::new(201_000_000, 243_900_000).with_note(Some("ext4".into()))),
            )
            .unwrap(),
            Item::usage("Memory", Some(Usage::new(4_604_000, 16_320_000))).unwrap(),
            Item::new("OS Age", "194 days"),
        ]
    }

    #[test]
    fn boxed_draws_a_closed_rounded_box_with_no_title() {
        let items = boxed_items();
        let out = compose(&scene(&[], 0, "me@host", &items, 100), &boxed_cfg());
        let plain: Vec<String> = out.iter().map(|l| strip_ansi(l)).collect();

        assert!(plain.first().unwrap().starts_with('╭'), "{plain:?}");
        assert!(plain.last().unwrap().starts_with('╰'), "{plain:?}");
        assert!(plain.iter().all(|l| !l.contains("me@host")), "no title row");
        // Edges and entries share one width, so the right border lines up.
        let edge = plain[0].width();
        assert_eq!(plain.last().unwrap().width(), edge);
        for line in &plain[1..plain.len() - 1] {
            let right = line
                .char_indices()
                .filter(|(_, c)| *c == '│')
                .nth(1)
                .unwrap()
                .0;
            assert_eq!(line[..right].width() + 1, edge, "border drifted: {line:?}");
        }
    }

    #[test]
    fn boxed_uses_short_names_and_aligns_every_value() {
        let items = boxed_items();
        let out = compose(&scene(&[], 0, "me@host", &items, 100), &boxed_cfg());
        let plain: Vec<String> = out.iter().map(|l| strip_ansi(l)).collect();

        assert!(plain.iter().any(|l| l.contains("distro")));
        assert!(plain.iter().any(|l| l.contains("term")));
        assert!(plain.iter().any(|l| l.contains("os age")));
        assert!(
            plain.iter().all(|l| !l.contains("OS Age")),
            "labels are lowercase"
        );

        let col = |needle: &str| {
            let line = plain.iter().find(|l| l.contains(needle)).unwrap();
            line[..line.find(needle).unwrap()].width()
        };
        assert_eq!(col("Arch Linux"), col("ghostty"));
        assert_eq!(col("Arch Linux"), col("194 days"));
    }

    #[test]
    fn boxed_shows_usage_in_full_with_the_filesystem() {
        let items = boxed_items();
        let out = compose(&scene(&[], 0, "me@host", &items, 100), &boxed_cfg());
        let joined: String = out
            .iter()
            .map(|l| strip_ansi(l))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(joined.contains("GiB / "), "{joined}");
        assert!(joined.contains("(82%) - ext4"), "{joined}");
        assert!(joined.contains("(28%)"), "{joined}");
    }

    #[test]
    fn boxed_paints_the_percentage_by_how_full_it_is() {
        let items = boxed_items();
        let cfg = Config {
            color: true,
            ..boxed_cfg()
        };
        let out = compose(&scene(&[], 0, "me@host", &items, 100), &cfg);
        let disk = out.iter().find(|l| l.contains("ext4")).unwrap();
        let memory = out.iter().find(|l| l.contains("28%")).unwrap();

        // 82% is red, 28% is green, and each is painted just before its number.
        assert!(disk.contains("\x1b[38;2;224;108;117m82%"), "{disk:?}");
        assert!(memory.contains("\x1b[38;2;142;192;124m28%"), "{memory:?}");
    }

    #[test]
    fn boxed_icons_can_be_turned_off() {
        let items = boxed_items();
        let with = compose(&scene(&[], 0, "m", &items, 100), &boxed_cfg());
        let without = compose(
            &scene(&[], 0, "m", &items, 100),
            &Config {
                icons: false,
                ..boxed_cfg()
            },
        );

        assert!(with.iter().any(|l| l.contains('\u{f17c}')));
        assert!(without.iter().all(|l| !l.contains('\u{f17c}')));
        assert_eq!(
            strip_ansi(&with[0]).width(),
            strip_ansi(&without[0]).width() + ICON_COLS,
            "the box shrinks by the icon column"
        );
    }

    #[test]
    fn boxed_never_exceeds_a_narrow_terminal() {
        let items = boxed_items();
        for width in [6, 12, 20, 30, 45] {
            let out = compose(&scene(&[], 0, "m", &items, width), &boxed_cfg());
            for line in &out {
                assert!(
                    strip_ansi(line).width() <= width,
                    "{width}: {:?}",
                    strip_ansi(line)
                );
            }
        }
    }

    #[test]
    fn boxed_measures_the_width_it_draws() {
        // The art is fitted to this number, so a drift means overlap or a gap.
        let items = boxed_items();
        let cfg = boxed_cfg();
        let out = compose(&scene(&[], 0, "me@host", &items, 200), &cfg);
        let drawn = out.iter().map(|l| strip_ansi(l).width()).max().unwrap();

        assert_eq!(info_width("me@host", &items, &cfg), drawn);
    }

    #[test]
    fn a_label_with_no_glyph_is_lowercased_and_still_marked() {
        let items = vec![Item::new("Brightness", "80%")];
        let cfg = Config {
            color: false,
            ..super::super::super::test_util::plain_cfg()
        };
        let cfg = Config {
            info_style: InfoStyle::Boxed,
            ..cfg
        };
        let lines: Vec<String> = compose(&scene(&[], 0, "m", &items, 60), &cfg);

        assert!(lines.iter().any(|l| l.contains("brightness")), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains('\u{f10c}')));
    }
}
