//! `user@host`, a rule, then `Label: value` rows.

use unicode_width::UnicodeWidthStr;

use super::{Line, is_swatch};
use crate::config::Config;
use crate::fetch::Item;
use crate::render::paint::Painter;
use crate::render::text::{truncate, visible_width};

pub enum Row<'a> {
    Title(&'a str),
    Rule(usize),
    Pair {
        label: &'a str,
        value: &'a str,
    },
    /// Pre-coloured content passed through untouched (the palette swatches).
    Raw(&'a str),
}

pub fn rows<'a>(title: &'a str, items: &'a [Item], cfg: &Config) -> Vec<Row<'a>> {
    let mut rows = Vec::with_capacity(items.len() + 2);
    rows.push(Row::Title(title));
    rows.push(Row::Rule(title.width()));

    for item in items {
        if !is_swatch(item) {
            rows.push(Row::Pair {
                label: item.label,
                value: &item.value,
            });
        } else if cfg.color {
            rows.push(Row::Raw(&item.value));
        }
    }
    rows
}

/// `label: value`, the label cut first and the value into what remains. The
/// plain form every style falls back to when it has no room for its own.
pub fn paint_pair(p: &mut Painter<'_>, label: &str, value: &str, width: usize) {
    let (accent, color) = (p.accent(), p.value());
    p.text(accent, &truncate(label, width));

    // Reserve the label and its separator before truncating the value.
    let used = label.width().min(width) + 2;
    if let Some(room) = width.checked_sub(used).filter(|r| *r > 0) {
        p.raw(": ");
        p.text(color, &truncate(value, room));
    }
}

impl Line for Row<'_> {
    fn width(&self) -> usize {
        match *self {
            Row::Title(t) => t.width(),
            Row::Rule(n) => n,
            Row::Pair { label, value } => label.width() + 2 + value.width(),
            Row::Raw(raw) => visible_width(raw),
        }
    }

    fn paint(&self, p: &mut Painter<'_>, width: usize) {
        let accent = p.accent();
        match *self {
            Row::Title(title) => p.text(accent, &truncate(title, width)),
            Row::Rule(len) => p.text(accent, &"─".repeat(len.min(width))),
            Row::Pair { label, value } => paint_pair(p, label, value, width),
            Row::Raw(raw) => p.raw_fit(raw, width),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::fetch::Item;
    use crate::render::compose;
    use crate::render::test_util::*;
    use unicode_width::UnicodeWidthStr;

    fn items_with_detail() -> Vec<Item> {
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
    fn classic_ignores_the_detail_a_row_carries() {
        // The default style must be byte-for-byte what it was.
        let items = items_with_detail();
        let cfg = plain_cfg();
        let out = compose(&scene(&[], 0, "me@host", &items, 100), &cfg);
        let joined: String = out
            .iter()
            .map(|l| strip_ansi(l))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(joined.contains("me@host"));
        assert!(
            joined.contains("OS: Arch Linux") || joined.contains("OS"),
            "{joined}"
        );
        assert!(!joined.contains('╭'));
        assert!(
            !joined.contains(" GiB"),
            "boxed formatting leaked: {joined}"
        );
    }

    #[test]
    fn the_pair_falls_back_to_what_fits() {
        let cfg = Config {
            color: false,
            ..plain_cfg()
        };
        let items = vec![Item::new("Memory", "9.5GiB / 16GiB (59%)")];
        let line = |width| {
            compose(&scene(&[], 0, "me", &items, width), &cfg)
                .into_iter()
                .find(|l| l.starts_with("Memory"))
                .unwrap_or_default()
        };

        assert_eq!(line(40), "Memory: 9.5GiB / 16GiB (59%)");
        assert_eq!(line(12), "Memory: 9.5…");
        // No room even for one character of the value: the separator goes too.
        assert_eq!(line(7), "Memory");
        assert!(line(4).width() <= 4);
    }

    #[test]
    fn swatches_are_dropped_without_colour_and_kept_in_place_with_it() {
        let items = vec![
            Item::new("OS", "Arch"),
            Item::new("", "SWATCH"),
            Item::new("WM", "Hyprland"),
        ];
        let at = |color| {
            let cfg = Config {
                color,
                ..plain_cfg()
            };
            let lines: Vec<String> = compose(&scene(&[], 0, "me", &items, 40), &cfg)
                .iter()
                .map(|l| strip_ansi(l))
                .collect();
            lines.iter().position(|l| l == "SWATCH")
        };

        assert_eq!(at(true), Some(3), "between OS and WM");
        assert_eq!(at(false), None);
    }
}
