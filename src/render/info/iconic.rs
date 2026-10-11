//! A bare coloured icon per row and no labels, under pill headings, with a row
//! of outline shapes below.
//!
//! The icons take their colours from along the theme's gradient, so the whole
//! pane follows a palette. With no icons the rows carry a short name instead,
//! since the icon was the only thing saying what each row is.

use unicode_width::UnicodeWidthStr;

use super::classic::paint_pair;
use super::value::without_session;
use super::{Line, is_swatch};
use crate::config::Config;
use crate::fetch::{Detail, Item};
use crate::render::icons;
use crate::render::paint::{Painter, chip_overhead};
use crate::render::text::truncate;

struct Section {
    title: &'static str,
    icon: &'static str,
    labels: &'static [&'static str],
}

const HARDWARE: Section = Section {
    title: "Hardware",
    icon: "\u{f013}",
    labels: &["Display", "GPU", "Memory", "Swap", "CPU", "Temperature"],
};

const SOFTWARE: Section = Section {
    title: "Software",
    icon: "\u{f121}",
    labels: &["Kernel", "WM", "Shell Path", "Packages"],
};

/// The rows above the first heading, after the user's name.
const TOP: &[&str] = &["Uptime"];

/// This style's own picture for a row. It is part of what makes the style
/// look as it does (a heart for the kernel, an eye for the graphics), so the
/// shared table, which names rows plainly, is not consulted for these.
const PICTURES: &[(&str, &str)] = &[
    ("Uptime", "\u{f017}"),
    ("Display", "\u{f108}"),
    ("GPU", "\u{f06e}"),
    ("Memory", "\u{f200}"),
    ("Swap", "\u{f1c0}"),
    ("CPU", "\u{f2db}"),
    ("Temperature", "\u{f0f4}"),
    ("Kernel", "\u{f004}"),
    ("WM", "\u{f2d0}"),
    ("Shell Path", "\u{f120}"),
    ("Packages", "\u{f1b2}"),
];

const USER_ICON: &str = "\u{f007}";

/// Outline shapes in the terminal's own colours, in place of the swatches.
const SHAPES: &[(u8, char)] = &[(1, '♡'), (2, '▷'), (3, '☆'), (4, '⬡'), (5, '□'), (6, '○')];
const SHAPE_GAP: usize = 2;
const SHAPES_WIDTH: usize = SHAPES.len() + (SHAPES.len() - 1) * SHAPE_GAP;

pub enum Row {
    /// An icon (or a short name) and a value.
    Line {
        mark: String,
        /// Where along the gradient the icon's colour is taken, 0.0 to 1.0.
        tone: f32,
        /// The row as a plain pair, for a terminal too narrow for the icon.
        label: &'static str,
        text: String,
    },
    Heading {
        icon: Option<&'static str>,
        title: &'static str,
        tone: f32,
        round: bool,
    },
    Shapes,
    Blank,
}

/// Just the user, from `user@host`: the host is in the prompt already.
fn user_of(title: &str) -> &str {
    title.split_once('@').map_or(title, |(user, _)| user)
}

/// `1366x768, 1920x1080`: each screen's size without its refresh rate.
fn resolutions(value: &str) -> String {
    value
        .split(", ")
        .map(|mode| mode.split(" @ ").next().unwrap_or(mode))
        .collect::<Vec<_>>()
        .join(", ")
}

/// What a row says here: short, and bare.
fn text_of(item: &Item) -> String {
    match (&item.detail, item.label) {
        (Some(Detail::Duration(secs)), _) => crate::fetch::long_duration(*secs),
        (Some(Detail::Usage(u)), "Memory" | "Swap") => {
            format!("{}MiB used ({}%)", u.used_kib / 1024, u.percent())
        }
        (_, "Display") => resolutions(&item.value),
        (_, "GPU") => crate::fetch::gpu_model(&item.value),
        (_, "CPU") => crate::fetch::cpu_short(&item.value),
        (_, "WM") => without_session(&item.value).to_string(),
        _ => item.value.clone(),
    }
}

/// One row before it is given a colour and a mark.
struct Entry {
    label: &'static str,
    icon: &'static str,
    short: &'static str,
    text: String,
}

fn entries(labels: &[&str], items: &[Item]) -> Vec<Entry> {
    labels
        .iter()
        .flat_map(|&label| items.iter().filter(move |i| i.label == label))
        .map(|item| Entry {
            label: item.name(),
            icon: PICTURES
                .iter()
                .find(|(l, _)| *l == item.label)
                .map_or(icons::glyph(item.label).icon, |(_, icon)| icon),
            short: item.renamed_to().unwrap_or(icons::glyph(item.label).short),
            text: text_of(item),
        })
        .collect()
}

pub fn rows(title: &str, items: &[Item], cfg: &Config) -> Vec<Row> {
    let round = cfg.icons && cfg.color;

    let mut top = vec![Entry {
        label: "user",
        icon: USER_ICON,
        short: "user",
        text: user_of(title).to_string(),
    }];
    top.extend(entries(TOP, items));

    let sections: Vec<(&Section, Vec<Entry>)> = [&HARDWARE, &SOFTWARE]
        .into_iter()
        .map(|s| (s, entries(s.labels, items)))
        .filter(|(_, e)| !e.is_empty())
        .collect();

    // One tone per row across the whole pane, so the colours run on from the
    // top to the bottom rather than starting over in each section.
    let total = top.len() + sections.iter().map(|(_, e)| e.len()).sum::<usize>();
    let short_w = top
        .iter()
        .chain(sections.iter().flat_map(|(_, e)| e))
        .map(|e| e.short.width())
        .max()
        .unwrap_or(0);

    let mut placed = 0;
    let mut lines = |block: Vec<Entry>| -> Vec<Row> {
        block
            .into_iter()
            .map(|e| {
                let tone = if total > 1 {
                    placed as f32 / (total - 1) as f32
                } else {
                    0.0
                };
                placed += 1;
                Row::Line {
                    mark: if cfg.icons {
                        e.icon.to_string()
                    } else {
                        format!("{:<short_w$}", e.short)
                    },
                    tone,
                    label: e.label,
                    text: e.text,
                }
            })
            .collect()
    };

    let mut rows = lines(top);
    let count = sections.len();
    for (idx, (section, block)) in sections.into_iter().enumerate() {
        rows.push(Row::Blank);
        rows.push(Row::Heading {
            icon: cfg.icons.then_some(section.icon),
            title: section.title,
            tone: if count > 1 {
                idx as f32 / (count - 1) as f32
            } else {
                0.0
            },
            round,
        });
        rows.push(Row::Blank);
        rows.extend(lines(block));
    }

    if cfg.color && items.iter().any(is_swatch) {
        rows.push(Row::Blank);
        rows.push(Row::Shapes);
    }
    rows
}

impl Line for Row {
    fn width(&self) -> usize {
        match self {
            Row::Line { mark, text, .. } => mark.width() + 1 + text.width(),
            Row::Heading {
                icon, title, round, ..
            } => {
                let icon = icon.map_or(0, |_| icons::ICON_COLS);
                icon + title.width() + chip_overhead(*round)
            }
            Row::Shapes => SHAPES_WIDTH,
            Row::Blank => 0,
        }
    }

    fn paint(&self, p: &mut Painter<'_>, width: usize) {
        match self {
            Row::Blank => {}
            Row::Line {
                mark,
                tone,
                label,
                text,
            } => {
                let used = mark.width() + 1;
                let Some(room) = width.checked_sub(used).filter(|r| *r > 0) else {
                    return paint_pair(p, label, text, width);
                };
                let (color, value) = (p.gradient(*tone), p.value());
                p.text(color, mark);
                p.space(1);
                p.text(value, &truncate(text, room));
            }
            Row::Heading {
                icon,
                title,
                tone,
                round,
            } => {
                let color = p.gradient(*tone);
                let mut text = String::new();
                if let Some(icon) = icon {
                    text.push_str(icon);
                    text.push(' ');
                }
                text.push_str(title);

                match width.checked_sub(chip_overhead(*round)).filter(|r| *r > 0) {
                    Some(room) => p.chip(color.contrast(), color, &truncate(&text, room), *round),
                    None => p.text(color, &truncate(title, width)),
                }
            }
            Row::Shapes => p.marks(SHAPES, SHAPE_GAP, width),
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
            info_style: InfoStyle::Iconic,
            icons,
            color,
            ..plain_cfg()
        }
    }

    fn items() -> Vec<Item> {
        vec![
            Item::duration(
                "Uptime",
                3 * 86400 + 18 * 3600 + 45 * 60 + 1,
                "3 days, 18 hours",
            ),
            Item::new("Display", "1366x768 @ 60.00 Hz"),
            Item::new(
                "GPU",
                "Intel 4th Gen Core Processor Integrated Graphics Controller [Integrated]",
            ),
            Item::usage("Memory", Some(Usage::new(2_150 * 1024, 5_972 * 1024))).unwrap(),
            Item::usage("Swap", Some(Usage::new(133 * 1024, 4_433 * 1024))).unwrap(),
            Item::new("CPU", "Intel(R) Core(TM) i3-4000M CPU (4) @ 2.40GHz"),
            Item::new("Temperature", "60.00°C"),
            Item::new("Kernel", "6.11.2-arch1-1"),
            Item::new("WM", "XFCE (X11)"),
            Item::new("Shell Path", "/bin/zsh"),
            Item::new("Packages", "1108 (pacman)"),
            Item::new("", "swatches"),
        ]
    }

    fn lines(items: &[Item], cfg: &Config, width: usize) -> Vec<String> {
        compose(&scene(&[], 0, "mehedi@arch", items, width), cfg)
            .iter()
            .map(|l| strip_ansi(l))
            .collect()
    }

    #[test]
    fn it_reads_like_the_reference() {
        let text = lines(&items(), &cfg(false, false), 80).join("\n");

        assert!(
            text.contains("mehedi") && !text.contains("mehedi@") && !text.contains("@arch"),
            "user only: {text}"
        );
        assert!(text.contains("3days 18hours 45mins 1sec"), "{text}");
        assert!(
            text.contains("1366x768") && !text.contains("60.00 Hz"),
            "{text}"
        );
        assert!(text.contains("Intel 4th Gen Core Processor"), "{text}");
        assert!(
            !text.contains("Integrated"),
            "database suffix leaked: {text}"
        );
        assert!(text.contains("2150MiB used (36%)"), "{text}");
        assert!(text.contains("133MiB used (3%)"), "{text}");
        assert!(text.contains("Intel i3-4000M @ 2.40GHz"), "{text}");
        assert!(text.contains("60.00°C"), "{text}");
        assert!(text.contains("6.11.2-arch1-1"), "{text}");
        assert!(text.contains("XFCE") && !text.contains("X11"), "{text}");
        assert!(
            text.contains("/bin/zsh") && text.contains("1108 (pacman)"),
            "{text}"
        );
    }

    #[test]
    fn the_user_and_uptime_come_first_then_two_headed_sections() {
        let body = lines(&items(), &cfg(false, false), 80);
        let at = |needle: &str| body.iter().position(|l| l.contains(needle)).unwrap();

        assert!(at("mehedi") < at("3days"));
        assert!(at("3days") < at("Hardware"));
        assert!(at("Hardware") < at("1366x768"));
        assert!(at("1366x768") < at("Software"));
        assert!(at("Software") < at("6.11.2"));
        // A gap before each heading, and one under it.
        assert!(body[at("Hardware") - 1].trim().is_empty());
        assert!(body[at("Hardware") + 1].trim().is_empty());
        assert!(body[at("Software") - 1].trim().is_empty());
    }

    #[test]
    fn rows_carry_no_labels_only_their_icons_when_there_are_icons() {
        let body = lines(&items(), &cfg(true, false), 80).join("\n");

        for word in [
            "disp", "gpu", "mem", "swap", "temp", "kernel", "pkgs", "Uptime",
        ] {
            assert!(!body.contains(word), "{word} is a label:\n{body}");
        }
        for icon in ['\u{f108}', '\u{f06e}', '\u{f200}', '\u{f2db}', '\u{f004}'] {
            assert!(body.contains(icon), "missing {icon:?}:\n{body}");
        }
    }

    #[test]
    fn without_icons_each_row_names_itself_in_an_aligned_column() {
        let body = lines(&items(), &cfg(false, false), 80);
        let col = |needle: &str| {
            let line = body.iter().find(|l| l.contains(needle)).unwrap();
            line[..line.find(needle).unwrap()].width()
        };

        assert!(body.iter().any(|l| l.starts_with("gpu")), "{body:?}");
        assert_eq!(col("1366x768"), col("2150MiB"));
        assert_eq!(col("1366x768"), col("6.11.2"));
    }

    #[test]
    fn icons_are_coloured_along_the_gradient_one_row_after_another() {
        // `plain_cfg` has no gradient, which is the accent for every icon.
        let themed = Config {
            gradient: Config::default().gradient,
            ..cfg(true, true)
        };
        let out = compose(&scene(&[], 0, "m@h", &items(), 80), &themed);
        let icon_colours: Vec<&str> = out
            .iter()
            .filter_map(|l| {
                let i = l.find(|c: char| ('\u{f000}'..='\u{f2ff}').contains(&c))?;
                let before = &l[..i];
                before.rfind("\x1b[38;2;").map(|at| &before[at..])
            })
            .collect();

        assert!(icon_colours.len() >= 8, "{icon_colours:?}");
        let distinct: std::collections::HashSet<_> = icon_colours.iter().collect();
        assert!(
            distinct.len() >= 4,
            "the icons should not all share a colour"
        );
    }

    #[test]
    fn the_headings_are_pills_filled_from_the_gradient_with_readable_text() {
        let out = compose(&scene(&[], 0, "m@h", &items(), 80), &cfg(true, true));
        let head = out.iter().find(|l| l.contains("Hardware")).unwrap();

        assert!(head.contains("\x1b[48;2;"), "no fill: {head:?}");
        assert!(head.contains('\u{e0b6}'), "square ends: {head:?}");

        // Without icons the pill is square, and without colour it is text.
        let sq = compose(&scene(&[], 0, "m@h", &items(), 80), &cfg(false, true));
        assert!(sq.iter().all(|l| !l.contains('\u{e0b6}')));
        let plain = lines(&items(), &cfg(true, false), 80).join("\n");
        assert!(plain.contains("Hardware") && !plain.contains('\u{e0b6}'));
    }

    #[test]
    fn the_shapes_are_drawn_only_with_colour_and_all_six_when_there_is_room() {
        let plain = lines(&items(), &cfg(true, false), 80).join("\n");
        assert!(!plain.contains('☆'));

        let coloured = lines(&items(), &cfg(true, true), 80);
        let shapes = coloured.last().unwrap();
        for glyph in ['♡', '▷', '☆', '⬡', '□', '○'] {
            assert!(shapes.contains(glyph), "{glyph} missing: {shapes:?}");
        }
    }

    #[test]
    fn a_section_with_nothing_has_no_heading() {
        let only = vec![Item::new("Kernel", "6.11")];
        let text = lines(&only, &cfg(false, false), 80).join("\n");

        assert!(
            text.contains("Software") && !text.contains("Hardware"),
            "{text}"
        );
    }

    #[test]
    fn the_user_alone_is_still_a_pane() {
        let text = lines(&[], &cfg(false, false), 80).join("\n");
        assert!(text.contains("mehedi"), "{text}");
        assert!(!text.contains("Hardware") && !text.contains("Software"));
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
            assert_eq!(
                info_width("mehedi@arch", &items(), &c),
                drawn,
                "icons={icons} color={color}"
            );
        }
    }

    #[test]
    fn resolutions_drop_the_refresh_rate_of_every_screen() {
        assert_eq!(resolutions("1366x768 @ 60.00 Hz"), "1366x768");
        assert_eq!(
            resolutions("1920x1080 @ 143.98 Hz, 1366x768 @ 60.00 Hz"),
            "1920x1080, 1366x768"
        );
        assert_eq!(resolutions("1366x768"), "1366x768");
    }

    #[test]
    fn the_user_is_what_comes_before_the_at() {
        assert_eq!(user_of("mehedi@arch"), "mehedi");
        assert_eq!(user_of("nohost"), "nohost");
    }
}
