//! Titled sections one under another: hardware, software, then ages and
//! battery as `key : value` lines, and the colour dots below.

use unicode_width::UnicodeWidthStr;

use super::group::{Group, UNKNOWN};
use super::{Line, is_swatch, value};
use crate::config::Config;
use crate::fetch::{Detail, Item};
use crate::render::icons::ICON_COLS;
use crate::render::paint::{DOTS_WIDTH, Painter, Seg, segs_width};
use crate::render::text::truncate;

const HARDWARE: Group = Group {
    name: "PC",
    head: &["Host", "CPU"],
    children: &["CPU", "GPU", "Memory", "Disk"],
};

const SOFTWARE: Group = Group {
    name: "OS",
    head: &["OS"],
    children: &["Kernel", "Packages", "Shell", "WM"],
};

/// The rows of the last section, which are `key : value` rather than a tree.
const AGE_FIELDS: &[&str] = &["OS Age", "Uptime", "Battery"];

/// Readings shown with how full they are; other usage is a size and a name.
const SHOWS_PERCENT: &[&str] = &["Memory", "Swap"];

/// Columns of tree glyphs before a leaf's icon: ` ├─ `.
const TRUNK: usize = 4;

/// A rule's corners, its two dashes, and the spaces round its title.
const RULE_TRIM: usize = 6;

pub enum Row {
    /// A titled rule between sections: `┌──── Hardware ────┐`.
    Rule {
        title: &'static str,
        first: bool,
        width: usize,
    },
    /// A group's heading, `PC: 20L5S1R500`.
    Head {
        icon: Option<&'static str>,
        label: &'static str,
        segs: Vec<Seg>,
    },
    /// A line under a heading, hung off its trunk.
    Leaf {
        last: bool,
        /// An icon, or its short name padded to a column.
        mark: String,
        segs: Vec<Seg>,
    },
    /// `Uptime  : 47 mins`.
    Field {
        key: &'static str,
        key_w: usize,
        segs: Vec<Seg>,
    },
    /// The eight terminal colours as dots.
    Dots,
    Blank,
}

/// What a row says here, with its figures coloured.
fn segs_of(item: &Item) -> Vec<Seg> {
    match &item.detail {
        Some(Detail::Usage(u)) if SHOWS_PERCENT.contains(&item.label) => value::usage_segs(u),
        Some(Detail::Usage(u)) => {
            let figures = u.spaced();
            value::plain(match &u.name {
                Some(name) => format!("{name}: {figures}"),
                None => figures,
            })
        }
        Some(Detail::Charge(c)) => value::charge_segs(c),
        // The model alone: this style's rows are short on purpose.
        _ if item.label == "CPU" => value::plain(crate::fetch::cpu_model(&item.value)),
        _ if item.label == "GPU" => value::plain(crate::fetch::gpu_model(&item.value)),
        _ => value::plain(value::text(item)),
    }
}

/// One heading and its leaves, or nothing if the machine had none of them.
fn group_rows(group: &Group, items: &[Item], cfg: &Config) -> Vec<Row> {
    let Some(members) = group.members(items) else {
        return Vec::new();
    };

    let short_w = members
        .leaves
        .iter()
        .map(|(_, g)| g.short.width())
        .max()
        .unwrap_or(0);
    let mut rows = Vec::with_capacity(members.leaves.len() + 1);

    rows.push(Row::Head {
        icon: cfg.icons.then(|| group.icon()),
        label: group.name,
        segs: members.head.map_or_else(|| value::plain(UNKNOWN), segs_of),
    });

    let last = members.leaves.len().saturating_sub(1);
    for (n, (item, glyph)) in members.leaves.into_iter().enumerate() {
        rows.push(Row::Leaf {
            last: n == last,
            mark: if cfg.icons {
                glyph.icon.to_string()
            } else {
                format!("{:<short_w$}", glyph.short)
            },
            segs: segs_of(item),
        });
    }
    rows
}

pub fn rows(items: &[Item], cfg: &Config) -> Vec<Row> {
    let fields: Vec<(&'static str, Vec<Seg>)> = AGE_FIELDS
        .iter()
        .flat_map(|&key| items.iter().filter(move |i| i.label == key))
        .map(|i| (i.name(), segs_of(i)))
        .collect();
    // One key column, so the colons line up.
    let key_w = fields.iter().map(|(k, _)| k.width()).max().unwrap_or(0);
    let fields: Vec<Row> = fields
        .into_iter()
        .map(|(key, segs)| Row::Field { key, key_w, segs })
        .collect();

    let sections: Vec<(&'static str, Vec<Row>)> = vec![
        ("Hardware", group_rows(&HARDWARE, items, cfg)),
        ("Software", group_rows(&SOFTWARE, items, cfg)),
        ("Uptime / Age", fields),
    ]
    .into_iter()
    .filter(|(_, rows)| !rows.is_empty())
    .collect();

    // The rules span the widest line, and at least their own title.
    let widest = sections
        .iter()
        .flat_map(|(_, rows)| rows.iter().map(Line::width))
        .max()
        .unwrap_or(0);
    let titles = sections
        .iter()
        .map(|(t, _)| t.width() + RULE_TRIM)
        .max()
        .unwrap_or(0);
    let width = widest.max(titles);

    let mut rows = Vec::new();
    for (n, (title, body)) in sections.into_iter().enumerate() {
        rows.push(Row::Rule {
            title,
            first: n == 0,
            width,
        });
        rows.extend(body);
    }

    if cfg.swatches() && items.iter().any(is_swatch) {
        if !rows.is_empty() {
            rows.push(Row::Blank);
        }
        rows.push(Row::Dots);
    }
    rows
}

impl Line for Row {
    fn width(&self) -> usize {
        match self {
            Row::Rule { width, .. } => *width,
            Row::Head { icon, label, segs } => {
                icon.map_or(0, |_| ICON_COLS) + label.width() + 2 + segs_width(segs)
            }
            Row::Leaf { mark, segs, .. } => TRUNK + mark.width() + 1 + segs_width(segs),
            Row::Field { key_w, segs, .. } => key_w + 3 + segs_width(segs),
            Row::Dots => DOTS_WIDTH,
            Row::Blank => 0,
        }
    }

    fn paint(&self, p: &mut Painter<'_>, width: usize) {
        let (accent, color) = (p.accent(), p.value());

        match self {
            Row::Blank => {}
            Row::Rule {
                title,
                first,
                width: rule,
            } => paint_rule(p, title, *first, (*rule).min(width)),
            Row::Head { icon, label, segs } => {
                let mut prefix = String::new();
                if let Some(icon) = icon {
                    prefix.push_str(icon);
                    prefix.push(' ');
                }
                prefix.push_str(label);
                prefix.push_str(": ");
                p.prefixed(&prefix, accent, segs, width);
            }
            Row::Leaf { last, mark, segs } => {
                let trunk = if *last { " └─ " } else { " ├─ " };
                p.faint(color, &truncate(trunk, width));

                if let Some(room) = width.checked_sub(TRUNK).filter(|r| *r > 0) {
                    p.prefixed(&format!("{mark} "), color, segs, room);
                }
            }
            Row::Field { key, key_w, segs } => {
                let key = format!("{key:<key_w$}");
                let secondary = p.secondary();
                p.text(secondary, &truncate(&key, width));

                if let Some(room) = width.checked_sub(key_w + 3).filter(|r| *r > 0) {
                    p.raw(" : ");
                    p.segs(segs, room);
                }
            }
            Row::Dots => p.dots(width),
        }
    }
}

fn paint_rule(p: &mut Painter<'_>, title: &str, first: bool, width: usize) {
    let (l, r) = if first {
        ('┌', '┐')
    } else {
        ('├', '┤')
    };
    let (accent, color) = (p.accent(), p.value());

    // One column holds a corner and no more: both would be a column too wide.
    if width < 2 {
        p.faint(color, &l.to_string());
        return;
    }
    let inner = width - 2;
    let label = format!(" {title} ");

    if inner < label.width() {
        // Too tight for the title: keep the rule, lose the words.
        p.faint(color, &format!("{l}{}{r}", "─".repeat(inner)));
    } else {
        let left = (inner - label.width()) / 2;
        p.faint(color, &format!("{l}{}", "─".repeat(left)));
        p.text(accent, &label);
        p.faint(
            color,
            &format!("{}{r}", "─".repeat(inner - label.width() - left)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::InfoStyle;
    use crate::render::test_util::*;
    use crate::render::{compose, info_width};

    fn sections_cfg() -> Config {
        Config {
            info_style: InfoStyle::Sections,
            icons: true,
            color: false,
            ..plain_cfg()
        }
    }

    fn sections_items() -> Vec<Item> {
        use crate::fetch::Usage;
        let disk = |at: &str, used| {
            Item::usage(
                "Disk",
                Some(Usage::new(used, 50_000_000).with_name(Some(at.into()))),
            )
            .unwrap()
        };
        let host = {
            let mut h = Item::new("Host", "20L5S1R500 ThinkPad T480");
            h.detail = Some(crate::fetch::Detail::Text(
                "20L5S1R500 (ThinkPad T480)".into(),
            ));
            h
        };
        vec![
            host,
            Item::new("CPU", "Intel(R) Core(TM) i7-8650U (8) @ 4.20GHz"),
            Item::new("GPU", "Intel UHD Graphics 620 [Integrated]"),
            Item::usage("Memory", Some(Usage::new(4_460_000, 16_100_000))).unwrap(),
            disk("/", 10_400_000),
            disk("/home", 12_700_000),
            Item::new("OS", "Artix Linux x86_64"),
            Item::new("Kernel", "6.17.9-artix1-1"),
            Item::new("Packages", "575 (pacman)"),
            Item::new("Shell", "zsh"),
            Item::new("WM", "Hyprland"),
            Item::new("OS Age", "337 days"),
            Item::new("Uptime", "47 mins"),
            Item::charge("Battery", 100, "AC Connected"),
            Item::charge("Battery", 96, "AC Connected"),
            Item::new("", "swatches"),
        ]
    }

    fn sections_lines(items: &[Item], cfg: &Config, width: usize) -> Vec<String> {
        compose(&scene(&[], 0, "me@host", items, width), cfg)
            .iter()
            .map(|l| strip_ansi(l))
            .collect()
    }

    #[test]
    fn sections_are_separated_by_titled_rules_of_one_width() {
        let lines = sections_lines(&sections_items(), &sections_cfg(), 100);
        let rules: Vec<&String> = lines
            .iter()
            .filter(|l| l.starts_with('┌') || l.starts_with('├'))
            .collect();

        assert_eq!(rules.len(), 3, "{lines:#?}");
        assert!(rules[0].starts_with('┌') && rules[0].ends_with('┐'));
        for rule in &rules[1..] {
            assert!(rule.starts_with('├') && rule.ends_with('┤'), "{rule}");
        }
        assert!(rules[0].contains(" Hardware "));
        assert!(rules[1].contains(" Software "));
        assert!(rules[2].contains(" Uptime / Age "));
        assert!(rules.iter().all(|r| r.width() == rules[0].width()));
    }

    #[test]
    fn sections_head_each_group_and_end_its_trunk_at_the_last_leaf() {
        let body = sections_lines(&sections_items(), &sections_cfg(), 100).join("\n");

        assert!(body.contains("PC: 20L5S1R500 (ThinkPad T480)"), "{body}");
        assert!(body.contains("OS: Artix Linux x86_64"), "{body}");
        // Leaves only: the rules between sections also begin with `├─`.
        // Hardware has five leaves and Software four, so seven branches and
        // one trunk end for each group.
        assert_eq!(body.matches(" ├─").count(), 7, "{body}");
        assert_eq!(body.matches(" └─").count(), 2, "{body}");
    }

    #[test]
    fn sections_show_a_cpu_and_gpu_by_name_alone() {
        let body = sections_lines(&sections_items(), &sections_cfg(), 100).join("\n");

        assert!(body.contains("Intel(R) Core(TM) i7-8650U"));
        assert!(!body.contains("4.20"), "clock speed leaked: {body}");
        assert!(!body.contains("(8)"), "core count leaked: {body}");
        assert!(body.contains("Intel UHD Graphics 620"));
        assert!(!body.contains("[Integrated]"), "tag leaked: {body}");
    }

    #[test]
    fn sections_list_every_disk_by_its_mount_point() {
        let body = sections_lines(&sections_items(), &sections_cfg(), 100).join("\n");

        assert!(body.contains("/: 9.92 GiB / 47.68 GiB"), "{body}");
        assert!(body.contains("/home: 12.11 GiB / 47.68 GiB"), "{body}");
    }

    #[test]
    fn sections_align_the_keys_of_the_last_block() {
        let lines = sections_lines(&sections_items(), &sections_cfg(), 100);
        // Only `key : value` lines: "Uptime" is also in a rule's title.
        let colon = |key: &str| {
            let line = lines
                .iter()
                .find(|l| l.starts_with(key) && l.contains(" : "))
                .unwrap();
            line[..line.find(" : ").unwrap()].width()
        };

        // "Battery" is the longest, so the others are padded out to it.
        assert_eq!(colon("OS Age"), colon("Battery"));
        assert_eq!(colon("Uptime"), colon("Battery"));
        assert_eq!(
            lines.iter().filter(|l| l.starts_with("Battery")).count(),
            2,
            "one line per battery"
        );
    }

    #[test]
    fn sections_paint_charge_green_when_full_and_red_when_empty() {
        let cfg = Config {
            color: true,
            ..sections_cfg()
        };
        let mut items = sections_items();
        items.retain(|i| i.label != "Battery");
        items.push(Item::charge("Battery", 96, "AC Connected"));
        items.push(Item::charge("Battery", 12, "Discharging"));

        let lines: Vec<String> = compose(&scene(&[], 0, "m", &items, 100), &cfg);
        let full = lines.iter().find(|l| l.contains("96%")).unwrap();
        let low = lines.iter().find(|l| l.contains("12%")).unwrap();

        assert!(full.contains("\x1b[38;2;142;192;124m96%"), "{full:?}");
        assert!(low.contains("\x1b[38;2;224;108;117m12%"), "{low:?}");
    }

    #[test]
    fn sections_paint_memory_by_how_full_it_is_not_how_charged() {
        let cfg = Config {
            color: true,
            ..sections_cfg()
        };
        let mut items = sections_items();
        items.retain(|i| i.label != "Memory");
        items.push(
            Item::usage(
                "Memory",
                Some(crate::fetch::Usage::new(9_500_000, 10_000_000)),
            )
            .unwrap(),
        );

        let lines = compose(&scene(&[], 0, "m", &items, 100), &cfg);
        let mem = lines.iter().find(|l| l.contains("95%")).unwrap();
        // 95% *full* is red, though 95% *charge* would be green.
        assert!(mem.contains("\x1b[38;2;224;108;117m95%"), "{mem:?}");
    }

    #[test]
    fn sections_name_the_cpu_as_the_heading_when_there_is_no_host() {
        let mut items = sections_items();
        items.retain(|i| i.label != "Host");
        let body = sections_lines(&items, &sections_cfg(), 100).join("\n");

        assert!(body.contains("PC: Intel(R) Core(TM) i7-8650U"), "{body}");
        assert_eq!(
            body.matches("i7-8650U").count(),
            1,
            "the heading is not also a leaf: {body}"
        );
    }

    #[test]
    fn sections_omit_a_block_the_machine_has_nothing_for() {
        // A desktop: no battery, and with no ages either the block is gone.
        let items = vec![Item::new("OS", "Arch"), Item::new("Shell", "zsh")];
        let body = sections_lines(&items, &sections_cfg(), 100).join("\n");

        assert!(body.contains("Software"));
        assert!(!body.contains("Hardware"), "{body}");
        assert!(!body.contains("Uptime / Age"), "{body}");
    }

    #[test]
    fn sections_draw_the_dots_only_when_there_is_colour() {
        let plain = sections_lines(&sections_items(), &sections_cfg(), 100).join("\n");
        assert!(!plain.contains('●'));

        let cfg = Config {
            color: true,
            ..sections_cfg()
        };
        let lines = compose(&scene(&[], 0, "m", &sections_items(), 100), &cfg);
        let dots = lines.iter().find(|l| l.contains('●')).unwrap();
        assert_eq!(dots.matches('●').count(), 8);
    }

    #[test]
    fn sections_without_icons_name_each_leaf_in_an_aligned_column() {
        let cfg = Config {
            icons: false,
            ..sections_cfg()
        };
        let body = sections_lines(&sections_items(), &cfg, 100).join("\n");

        assert!(body.contains("├─ cpu  "), "{body}");
        assert!(body.contains("├─ mem  "), "{body}");
        assert!(body.contains("└─ disk "), "{body}");
        assert!(!body.contains('\u{f4bc}'));
    }

    #[test]
    fn sections_dots_never_exceed_a_narrow_terminal() {
        // Colour on, which is the only time the dots are drawn at all. They
        // were painted at full width whatever the room.
        let cfg = Config {
            color: true,
            ..sections_cfg()
        };
        for width in [1, 3, 5, 8, 14, 20] {
            for line in compose(&scene(&[], 0, "m", &sections_items(), width), &cfg) {
                assert!(
                    strip_ansi(&line).width() <= width,
                    "{width}: {:?}",
                    strip_ansi(&line)
                );
            }
        }
    }

    #[test]
    fn sections_never_exceed_a_narrow_terminal() {
        for width in [1, 2, 3, 8, 14, 20, 30, 45] {
            for line in sections_lines(&sections_items(), &sections_cfg(), width) {
                assert!(line.width() <= width, "{width}: {line:?}");
            }
        }
    }

    #[test]
    fn sections_measure_the_width_they_draw() {
        let cfg = sections_cfg();
        let items = sections_items();
        let drawn = sections_lines(&items, &cfg, 200)
            .iter()
            .map(|l| l.width())
            .max()
            .unwrap();

        assert_eq!(info_width("me@host", &items, &cfg), drawn);
    }
}
