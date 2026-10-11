//! Section headings and values as filled, rounded chips, with the colours as
//! dots beneath.
//!
//! The values are short and bare (no versions, no percentages), which is why
//! this style asks the backends for nothing extra.

use unicode_width::UnicodeWidthStr;

use super::classic::paint_pair;
use super::value::without_arch;
use super::{Line, is_swatch};
use crate::config::Config;
use crate::fetch::{Detail, Item};
use crate::render::icons::{self, ICON_COLS};
use crate::render::paint::{DOTS_WIDTH, Painter, chip_overhead};
use crate::render::text::truncate;

/// How much of the value colour a chip's fill keeps. Dark enough to sit under
/// light text, light enough to show against a dark terminal.
const KEY_FILL: f32 = 0.30;
const VALUE_FILL: f32 = 0.20;

struct Section {
    title: &'static str,
    icon: &'static str,
    labels: &'static [&'static str],
}

const SECTIONS: &[Section] = &[
    Section {
        title: "SYSTEM",
        icon: "\u{f17c}",
        labels: &["OS", "Kernel", "Uptime", "Packages"],
    },
    Section {
        title: "SOFTWARE",
        icon: "\u{f2d0}",
        labels: &["WM", "Shell", "Terminal", "Font"],
    },
    Section {
        title: "HARDWARE",
        icon: "\u{f2db}",
        labels: &["CPU", "GPU", "Memory", "Disk"],
    },
];

pub enum Row<'a> {
    /// A section's heading, as a chip filled with the accent.
    Header {
        icon: Option<&'static str>,
        title: &'static str,
        round: bool,
    },
    /// A key chip and a value chip.
    Entry {
        key: String,
        round: bool,
        /// The row as plain text, for a terminal too narrow for chips.
        label: &'a str,
        text: String,
    },
    /// The eight terminal colours as dots.
    Dots,
    Blank,
}

/// What a row says here: short and bare.
fn text_of(item: &Item) -> String {
    match (&item.detail, item.label) {
        (Some(Detail::Usage(u)), _) => {
            let mut text = String::new();
            if let Some(name) = &u.name {
                text.push_str(name);
                text.push_str(": ");
            }
            text.push_str(&u.spaced());
            if let Some(note) = &u.note {
                text.push_str(&format!(" ({note})"));
            }
            text
        }
        (_, "OS") => without_arch(&item.value).to_string(),
        (Some(Detail::Duration(secs)), _) => crate::fetch::compact_duration(*secs),
        (_, "CPU") => crate::fetch::cpu_model(&item.value).to_string(),
        (_, "GPU") => crate::fetch::gpu_model(&item.value),
        _ => item.value.clone(),
    }
}

pub fn rows<'a>(items: &'a [Item], cfg: &Config) -> Vec<Row<'a>> {
    // Rounded needs both the glyphs to draw caps with and a background to fill.
    let round = cfg.icons && cfg.color;

    let key_of = |item: &Item| {
        let code = item.renamed_to().map_or_else(
            || {
                icons::lookup(item.label).map_or_else(
                    || {
                        item.label
                            .chars()
                            .take(2)
                            .collect::<String>()
                            .to_uppercase()
                    },
                    |g| g.code.to_string(),
                )
            },
            str::to_string,
        );
        match (cfg.icons, icons::lookup(item.label)) {
            (true, glyph) => {
                let icon = glyph.map_or(icons::GENERIC.icon, |g| g.icon);
                format!("{icon} {code}")
            }
            (false, _) => code,
        }
    };

    let mut sections: Vec<Vec<Row<'a>>> = Vec::new();
    for section in SECTIONS {
        let entries: Vec<Row<'a>> = section
            .labels
            .iter()
            .flat_map(|&label| items.iter().filter(move |i| i.label == label))
            .map(|item| Row::Entry {
                key: key_of(item),
                round,
                label: item.name(),
                text: text_of(item),
            })
            .collect();
        if entries.is_empty() {
            continue;
        }

        let mut body = Vec::with_capacity(entries.len() + 1);
        body.push(Row::Header {
            icon: cfg.icons.then_some(section.icon),
            title: section.title,
            round,
        });
        body.extend(entries);
        sections.push(body);
    }

    // One key width, so the value chips start in one column.
    let key_w = sections
        .iter()
        .flatten()
        .filter_map(|r| match r {
            Row::Entry { key, .. } => Some(key.width()),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    for row in sections.iter_mut().flatten() {
        if let Row::Entry { key, .. } = row {
            let pad = key_w - key.width();
            key.extend(std::iter::repeat_n(' ', pad));
        }
    }

    let mut rows = Vec::new();
    for (n, body) in sections.into_iter().enumerate() {
        if n > 0 {
            rows.push(Row::Blank);
        }
        rows.extend(body);
    }

    if cfg.color && items.iter().any(is_swatch) {
        if !rows.is_empty() {
            rows.push(Row::Blank);
        }
        rows.push(Row::Dots);
    }
    rows
}

impl Line for Row<'_> {
    fn width(&self) -> usize {
        match self {
            Row::Header { icon, title, round } => {
                let icon = icon.map_or(0, |_| ICON_COLS);
                icon + title.width() + chip_overhead(*round)
            }
            Row::Entry {
                key, round, text, ..
            } => {
                let o = chip_overhead(*round);
                key.width() + o + 1 + text.width() + o
            }
            Row::Dots => DOTS_WIDTH,
            Row::Blank => 0,
        }
    }

    fn paint(&self, p: &mut Painter<'_>, width: usize) {
        let (accent, color) = (p.accent(), p.value());

        match self {
            Row::Blank => {}
            Row::Header { icon, title, round } => {
                let mut text = String::new();
                if let Some(icon) = icon {
                    text.push_str(icon);
                    text.push(' ');
                }
                text.push_str(title);

                match width.checked_sub(chip_overhead(*round)).filter(|r| *r > 0) {
                    Some(room) => p.chip(accent.contrast(), accent, &truncate(&text, room), *round),
                    None => p.text(accent, &truncate(title, width)),
                }
            }
            Row::Entry {
                key,
                round,
                label,
                text,
            } => {
                let o = chip_overhead(*round);
                let key_w = key.width() + o;
                // A key chip, a space, and at least one character of value.
                let Some(room) = width.checked_sub(key_w + 1 + o).filter(|r| *r > 0) else {
                    return paint_pair(p, label, text, width);
                };

                p.chip(accent, color.scale(KEY_FILL), key, *round);
                p.space(1);
                p.chip(
                    color,
                    color.scale(VALUE_FILL),
                    &truncate(text, room),
                    *round,
                );
            }
            Row::Dots => p.dots(width),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::InfoStyle;
    use crate::fetch::Usage;
    use crate::render::test_util::*;
    use crate::render::{compose, info_width};

    fn cfg(icons: bool, color: bool) -> Config {
        Config {
            info_style: InfoStyle::Chips,
            icons,
            color,
            ..plain_cfg()
        }
    }

    fn items() -> Vec<Item> {
        let disk = Item::usage(
            "Disk",
            Some(Usage::new(253_153_000, 934_000_000).with_note(Some("btrfs".into()))),
        )
        .unwrap();
        vec![
            Item::new("OS", "Arch Linux x86_64"),
            Item::new("Kernel", "6.18.7-arch1-1"),
            Item::duration("Uptime", 6000, "1 hour, 40 mins"),
            Item::new("Packages", "999 (pacman)"),
            Item::new("WM", "Hyprland"),
            Item::new("Shell", "fish"),
            Item::new("Terminal", "kitty"),
            Item::new("Font", "Roboto (11pt)"),
            Item::new("CPU", "AMD Ryzen 5 5600X (12) @ 4.65GHz"),
            Item::new("GPU", "AMD RX 7900 GRE [Discrete]"),
            Item::usage("Memory", Some(Usage::new(4_260_000, 16_300_000))).unwrap(),
            disk,
            Item::new("", "swatches"),
        ]
    }

    fn lines(items: &[Item], cfg: &Config, width: usize) -> Vec<String> {
        compose(&scene(&[], 0, "me@host", items, width), cfg)
            .iter()
            .map(|l| strip_ansi(l))
            .collect()
    }

    #[test]
    fn three_sections_each_under_a_heading_with_a_gap_between() {
        let body = lines(&items(), &cfg(false, false), 80);
        let text = body.join("\n");

        let at = |needle: &str| body.iter().position(|l| l.contains(needle)).unwrap();
        assert!(at("SYSTEM") < at("SOFTWARE") && at("SOFTWARE") < at("HARDWARE"));
        // A blank line before each heading but the first.
        assert!(body[at("SOFTWARE") - 1].trim().is_empty(), "{text}");
        assert!(body[at("HARDWARE") - 1].trim().is_empty(), "{text}");
        assert!(!body[0].trim().is_empty());
    }

    #[test]
    fn each_row_is_a_two_letter_key_and_its_bare_value() {
        let text = lines(&items(), &cfg(false, false), 80).join("\n");

        for needle in [
            " OS ", " KE ", " UP ", " PK ", " WM ", " SH ", " TR ", " FT ", " CP ", " GP ", " RA ",
            " DI ",
        ] {
            assert!(text.contains(needle), "missing key {needle:?}:\n{text}");
        }
    }

    #[test]
    fn values_read_short_like_the_reference() {
        let text = lines(&items(), &cfg(false, false), 80).join("\n");

        assert!(text.contains("Arch Linux"), "{text}");
        assert!(!text.contains("x86_64"), "arch leaked: {text}");
        assert!(text.contains("1h 40m"), "{text}");
        assert!(text.contains("AMD Ryzen 5 5600X"), "{text}");
        assert!(!text.contains("4.65"), "clock leaked: {text}");
        assert!(text.contains("AMD RX 7900 GRE"), "{text}");
        assert!(!text.contains("[Discrete]"), "tag leaked: {text}");
        assert!(text.contains("4.06 GiB / 15.54 GiB"), "{text}");
        assert!(text.contains("(btrfs)"), "{text}");
        assert!(!text.contains('%'), "no percentages here: {text}");
    }

    #[test]
    fn every_value_chip_starts_in_one_column() {
        let body = lines(&items(), &cfg(false, false), 80);
        let col = |value: &str| {
            let line = body.iter().find(|l| l.contains(value)).unwrap();
            line[..line.find(value).unwrap()].width()
        };

        assert_eq!(col("Arch Linux"), col("Hyprland"));
        assert_eq!(col("Arch Linux"), col("AMD Ryzen"));
        assert_eq!(col("Arch Linux"), col("Roboto"));
    }

    #[test]
    fn rounded_chips_need_both_icons_and_colour() {
        let draws_caps = |icons, color| {
            compose(&scene(&[], 0, "m", &items(), 80), &cfg(icons, color))
                .iter()
                .any(|l| l.contains('\u{e0b6}'))
        };

        assert!(draws_caps(true, true));
        assert!(!draws_caps(false, true), "no icons, no caps");
        assert!(!draws_caps(true, false), "no colour, nothing to round");
    }

    #[test]
    fn the_heading_is_filled_with_the_accent_and_its_text_can_be_read() {
        let mut c = cfg(true, true);
        // A light accent must get dark text, a dark one light.
        c.accent = crate::color::Color(crate::color::Rgb(0xf4, 0xa7, 0xa7));
        let out = compose(&scene(&[], 0, "m", &items(), 80), &c);
        let head = out.iter().find(|l| l.contains("SYSTEM")).unwrap();

        assert!(head.contains("\x1b[48;2;244;167;167m"), "{head:?}");
        assert!(
            head.contains("\x1b[38;2;20;20;20m"),
            "dark text on pink: {head:?}"
        );

        c.accent = crate::color::Color(crate::color::Rgb(0x20, 0x30, 0x80));
        let out = compose(&scene(&[], 0, "m", &items(), 80), &c);
        let head = out.iter().find(|l| l.contains("SYSTEM")).unwrap();
        assert!(
            head.contains("\x1b[38;2;245;245;245m"),
            "light text on navy: {head:?}"
        );
    }

    #[test]
    fn a_section_with_nothing_in_it_has_no_heading() {
        let only_hw = vec![Item::new("CPU", "Ryzen"), Item::new("", "s")];
        let text = lines(&only_hw, &cfg(false, false), 80).join("\n");

        assert!(text.contains("HARDWARE"));
        assert!(
            !text.contains("SYSTEM") && !text.contains("SOFTWARE"),
            "{text}"
        );
    }

    #[test]
    fn a_row_with_no_known_key_still_gets_two_letters() {
        let items = vec![Item::new("Brightness", "80%"), Item::new("OS", "Arch")];
        // Not in any section, so it is not drawn: the layout is fixed.
        let text = lines(&items, &cfg(false, false), 80).join("\n");
        assert!(!text.contains("Brightness") && !text.contains(" BR "));
    }

    #[test]
    fn dots_are_drawn_only_with_colour() {
        let plain = lines(&items(), &cfg(true, false), 80).join("\n");
        assert!(!plain.contains('●'));

        let out = compose(&scene(&[], 0, "m", &items(), 80), &cfg(true, true));
        let dots = out.iter().find(|l| l.contains('●')).unwrap();
        assert_eq!(dots.matches('●').count(), 8);
    }

    #[test]
    fn a_narrow_terminal_falls_back_to_plain_rows_and_never_overflows() {
        for width in [1, 2, 3, 8, 12, 16, 20, 30, 45] {
            for icons in [true, false] {
                for line in lines(&items(), &cfg(icons, true), width) {
                    assert!(line.width() <= width, "{width}/{icons}: {line:?}");
                }
            }
        }
    }

    #[test]
    fn the_measured_width_is_the_width_that_gets_drawn() {
        for (icons, color) in [(true, true), (false, true), (true, false), (false, false)] {
            let c = cfg(icons, color);
            let drawn = lines(&items(), &c, 200)
                .iter()
                .map(|l| l.width())
                .max()
                .unwrap();
            let measured = info_width("m", &items(), &c);

            // Without colour a chip's closing space is blank, and a trailing
            // blank is trimmed from the line: one column reserved, not drawn.
            let trimmed = usize::from(!color);
            assert_eq!(measured, drawn + trimmed, "icons={icons} color={color}");
        }
    }

    #[test]
    fn uptime_is_said_from_its_seconds() {
        let at = |secs| text_of(&Item::duration("Uptime", secs, "classic words"));
        assert_eq!(at(6000), "1h 40m");
        assert_eq!(at(3 * 86400 + 2 * 3600 + 5 * 60), "3d 2h 5m");
        assert_eq!(at(0), "0m");
        // A reading with no seconds attached is shown as it came.
        assert_eq!(text_of(&Item::new("Uptime", "sometime")), "sometime");
    }

    #[test]
    fn named_disks_say_where_they_are() {
        let disk = Item::usage(
            "Disk",
            Some(
                Usage::new(10_485_760, 52_428_800)
                    .with_name(Some("/home".into()))
                    .with_note(Some("ext4".into())),
            ),
        )
        .unwrap();
        assert_eq!(text_of(&disk), "/home: 10.00 GiB / 50.00 GiB (ext4)");
    }
}
