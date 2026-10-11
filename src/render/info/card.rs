//! A single rounded box: each row's label at the left edge and its value at the
//! right, then a line of your own text under them.
//!
//! Rows go in the order of `modules`, so it is the one style that is a list
//! rather than a fixed layout. A disk is named for where it is mounted.

use unicode_width::UnicodeWidthStr;

use super::classic::paint_pair;
use super::value::{package_count, without_arch, without_session};
use super::{Line, is_swatch};
use crate::config::Config;
use crate::fetch::{Detail, Item};
use crate::render::paint::Painter;
use crate::render::text::truncate;

/// Blank columns between the border and what it holds, each side.
const PAD: usize = 3;

/// The least space between a label and its value.
const GAP: usize = 2;

/// A box's two borders.
const BORDERS: usize = 2;

/// How wide the box's content is, and the least it can be squeezed to.
#[derive(Clone, Copy)]
pub struct Frame {
    content: usize,
    min: usize,
}

impl Frame {
    /// The content width when only `width` columns are there, or `None` if it
    /// cannot be squeezed that far and is better shown as plain rows.
    fn fit(self, width: usize) -> Option<usize> {
        let room = width.checked_sub(BORDERS + 2 * PAD)?;
        (room >= self.min).then(|| self.content.min(room))
    }

    fn width(self) -> usize {
        self.content + 2 * PAD + BORDERS
    }
}

pub enum Row {
    Top(Frame),
    Bottom(Frame),
    /// An empty line inside the box.
    Blank(Frame),
    Entry {
        frame: Frame,
        label: String,
        label_w: usize,
        value: String,
    },
    Footer {
        frame: Frame,
        text: String,
    },
}

/// `Root` for `/`, otherwise the last part of where it is mounted:
/// `/mnt/Jogos` is `Jogos`, as udisks names the mount of a labelled volume.
fn mount_label(mount: Option<&str>) -> String {
    match mount {
        None | Some("/") => "Root".to_string(),
        Some(path) => path
            .rsplit('/')
            .find(|part| !part.is_empty())
            .unwrap_or(path)
            .to_string(),
    }
}

fn label_of(item: &Item) -> String {
    // Their own word wins over the style's ("Ram", the mount's name).
    if let Some(name) = item.renamed_to() {
        return name.to_string();
    }
    match (item.label, &item.detail) {
        ("Memory", _) => "Ram".to_string(),
        ("Disk", Some(Detail::Usage(u))) => mount_label(u.name.as_deref()),
        ("Disk", _) => mount_label(None),
        (label, _) => label.to_string(),
    }
}

/// Two decimals for memory, one for a small figure and none for a large one
/// for a disk: `4.23 / 31.25 GiB` next to `1.5 / 299 GiB`.
fn two(_: f64) -> usize {
    2
}

fn compact(figure: f64) -> usize {
    usize::from(figure < 10.0)
}

fn text_of(item: &Item) -> String {
    match (item.label, &item.detail) {
        ("Uptime", Some(Detail::Duration(secs))) => crate::fetch::compact_duration(*secs),
        (_, Some(Detail::Usage(u))) if matches!(item.label, "Memory" | "Swap") => u.shared(two),
        (_, Some(Detail::Usage(u))) => match &u.note {
            Some(fs) => format!("{} - {fs}", u.shared(compact)),
            None => u.shared(compact),
        },
        ("OS", _) => without_arch(&item.value).to_string(),
        ("WM", _) => without_session(&item.value).to_string(),
        ("CPU", _) => crate::fetch::cpu_brand(&item.value),
        ("Packages", _) => package_count(&item.value).to_string(),
        _ => item.value.clone(),
    }
}

pub fn rows(items: &[Item], cfg: &Config) -> Vec<Row> {
    let entries: Vec<(String, String)> = items
        .iter()
        .filter(|i| !is_swatch(i))
        .map(|i| (label_of(i), text_of(i)))
        .collect();
    let footer = cfg.footer.trim();

    if entries.is_empty() && footer.is_empty() {
        return Vec::new();
    }

    let label_w = entries.iter().map(|(l, _)| l.width()).max().unwrap_or(0);
    let widest_entry = entries
        .iter()
        .map(|(_, v)| label_w + GAP + v.width())
        .max()
        .unwrap_or(0);
    let frame = Frame {
        content: widest_entry.max(footer.width()),
        // A label, one blank, and a column of value.
        min: if entries.is_empty() { 1 } else { label_w + 2 },
    };

    let mut rows = vec![Row::Top(frame), Row::Blank(frame)];
    for (label, value) in entries {
        rows.push(Row::Entry {
            frame,
            label,
            label_w,
            value,
        });
    }
    if !footer.is_empty() {
        rows.push(Row::Blank(frame));
        rows.push(Row::Footer {
            frame,
            text: footer.to_string(),
        });
    }
    rows.push(Row::Blank(frame));
    rows.push(Row::Bottom(frame));
    rows
}

impl Line for Row {
    fn width(&self) -> usize {
        match self {
            Row::Top(f) | Row::Bottom(f) | Row::Blank(f) => f.width(),
            Row::Entry { frame, .. } | Row::Footer { frame, .. } => frame.width(),
        }
    }

    fn paint(&self, p: &mut Painter<'_>, width: usize) {
        let color = p.value();

        match self {
            Row::Top(f) | Row::Bottom(f) => {
                let Some(content) = f.fit(width) else { return };
                let (l, r) = if matches!(self, Row::Top(_)) {
                    ('╭', '╮')
                } else {
                    ('╰', '╯')
                };
                let rule = "─".repeat(content + 2 * PAD);
                p.text(color, &format!("{l}{rule}{r}"));
            }
            Row::Blank(f) => {
                let Some(content) = f.fit(width) else { return };
                p.text(color, "│");
                p.space(content + 2 * PAD);
                p.text(color, "│");
            }
            Row::Entry {
                frame,
                label,
                label_w,
                value,
            } => {
                let Some(content) = frame.fit(width) else {
                    return paint_pair(p, label, value, width);
                };
                let value = truncate(value, content - label_w - 1);

                p.text(color, "│");
                p.space(PAD);
                p.text(color, label);
                p.space(content - label.width() - value.width());
                p.text(color, &value);
                p.space(PAD);
                p.text(color, "│");
            }
            Row::Footer { frame, text } => {
                let Some(content) = frame.fit(width) else {
                    return p.text(color, &truncate(text, width));
                };
                let text = truncate(text, content);
                let lead = (content - text.width()) / 2;

                p.text(color, "│");
                p.space(PAD + lead);
                p.text(color, &text);
                p.space(PAD + content - lead - text.width());
                p.text(color, "│");
            }
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

    const MOTTO: &str = "！一 我々は自由だ！";

    fn cfg(footer: &str) -> Config {
        Config {
            info_style: InfoStyle::Card,
            color: false,
            footer: footer.to_string(),
            ..plain_cfg()
        }
    }

    fn disk(at: Option<&str>, used_gib: u64, total_gib: u64, fs: &str) -> Item {
        let gib = 1024 * 1024;
        let usage = Usage::new(used_gib * gib, total_gib * gib)
            .with_name(at.map(str::to_string))
            .with_note(Some(fs.to_string()));
        Item::usage("Disk", Some(usage)).unwrap()
    }

    fn items() -> Vec<Item> {
        let gib = 1024 * 1024;
        vec![
            Item::new("OS", "CachyOS x86_64"),
            Item::new("Kernel", "7.2.8"),
            Item::duration("Uptime", 20 * 60, "20 mins"),
            Item::new("Shell", "zsh"),
            Item::new("WM", "KDE (Wayland)"),
            Item::duration("OS Age", 18 * 86400, "18 days"),
            Item::usage(
                "Memory",
                Some(Usage::new(4 * gib + gib / 4 + 1_000, 31 * gib + gib / 4)),
            )
            .unwrap(),
            Item::new("CPU", "AMD Ryzen 7 5700X3D 8-Core Processor (16) @ 4.10GHz"),
            disk(Some("/"), 46, 928, "btrfs"),
            disk(Some("/mnt/Jogos"), 233, 916, "ext4"),
            disk(Some("/run/media/me/VIDS/"), 1, 299, "btrfs"),
            Item::new("Packages", "1450 (pacman)"),
            Item::custom("Owner", "POKKITA"),
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
    fn it_reads_like_the_reference() {
        let text = lines(&items(), &cfg(MOTTO), 100).join("\n");

        for needle in [
            "CachyOS",
            "7.2.8",
            "20m",
            "zsh",
            "KDE",
            "18 days",
            "AMD Ryzen 7 5700X3D",
            "1450",
            "POKKITA",
            MOTTO,
        ] {
            assert!(text.contains(needle), "missing {needle}:\n{text}");
        }
        assert!(text.contains("4.25 / 31.25 GiB"), "{text}");
        assert!(text.contains("46 / 928 GiB - btrfs"), "{text}");
        assert!(text.contains("233 / 916 GiB - ext4"), "{text}");
        assert!(text.contains("1.0 / 299 GiB - btrfs"), "{text}");
        for noise in [
            "x86_64",
            "Wayland",
            "(pacman)",
            "4.10",
            "Processor",
            "me@host",
        ] {
            assert!(!text.contains(noise), "{noise} leaked:\n{text}");
        }
    }

    #[test]
    fn rows_are_labelled_by_where_a_disk_is_and_memory_is_ram() {
        let body = lines(&items(), &cfg(""), 100);
        let starts = |label: &str| {
            body.iter()
                .any(|l| l.trim_start_matches(['│', ' ']).starts_with(label))
        };

        for label in [
            "OS", "Kernel", "Uptime", "Ram", "CPU", "Root", "Jogos", "VIDS", "Packages", "Owner",
        ] {
            assert!(starts(label), "no row labelled {label}:\n{body:#?}");
        }
        assert!(!starts("Memory") && !starts("Disk"), "{body:#?}");
    }

    #[test]
    fn labels_sit_left_and_every_value_ends_at_the_same_column() {
        let body = lines(&items(), &cfg(""), 100);
        let entries: Vec<&String> = body
            .iter()
            .filter(|l| l.contains("CachyOS") || l.contains("7.2.8") || l.contains("POKKITA"))
            .collect();

        let ends: Vec<usize> = entries
            .iter()
            .map(|l| l.trim_end_matches('│').trim_end().width())
            .collect();
        assert!(
            ends.windows(2).all(|w| w[0] == w[1]),
            "ragged: {entries:#?}"
        );
        // Labels start in one column too, just inside the padding.
        let starts: Vec<usize> = entries
            .iter()
            .map(|l| l.chars().skip(1).take_while(|c| *c == ' ').count())
            .collect();
        assert!(starts.iter().all(|s| *s == PAD), "{starts:?}");
    }

    #[test]
    fn it_is_one_closed_box_of_one_width() {
        let body = lines(&items(), &cfg(MOTTO), 100);
        let drawn: Vec<&String> = body.iter().filter(|l| !l.is_empty()).collect();
        let width = drawn[0].width();

        assert!(drawn[0].starts_with('╭') && drawn[0].ends_with('╮'));
        assert!(drawn.last().unwrap().starts_with('╰') && drawn.last().unwrap().ends_with('╯'));
        assert!(
            drawn.iter().all(|l| l.width() == width),
            "ragged: {drawn:#?}"
        );
        assert!(
            drawn[1..drawn.len() - 1]
                .iter()
                .all(|l| l.starts_with('│') && l.ends_with('│')),
            "{drawn:#?}"
        );
    }

    #[test]
    fn the_footer_is_centred_under_a_blank_line_and_only_when_there_is_one() {
        let with = lines(&items(), &cfg(MOTTO), 100);
        let at = with.iter().position(|l| l.contains(MOTTO)).unwrap();
        assert!(
            with[at - 1].trim_matches(['│', ' ']).is_empty(),
            "no gap above"
        );

        let inner = with[at].trim_matches('│');
        let (lead, trail) = (
            inner.chars().take_while(|c| *c == ' ').count(),
            inner.chars().rev().take_while(|c| *c == ' ').count(),
        );
        assert!(lead.abs_diff(trail) <= 1, "off centre: {lead} vs {trail}");

        let without = lines(&items(), &cfg(""), 100);
        assert!(!without.iter().any(|l| l.contains(MOTTO)));
        assert!(without.len() < with.len());
    }

    #[test]
    fn a_footer_wider_than_the_rows_sets_the_width_of_the_box() {
        let long = "a line of text far longer than any of the rows could be";
        let narrow = lines(&items(), &cfg(""), 200);
        let wide = lines(&items(), &cfg(long), 200);

        assert!(wide[0].width() > narrow[0].width());
        assert!(wide.iter().any(|l| l.contains(long)));
    }

    #[test]
    fn a_tight_terminal_keeps_the_box_by_cutting_values() {
        let body = lines(&items(), &cfg(""), 30);

        assert!(body.iter().any(|l| l.starts_with('╭')), "{body:#?}");
        assert!(body.iter().any(|l| l.contains('…')), "nothing was cut");
        for line in &body {
            assert!(line.width() <= 30, "{line:?}");
        }
    }

    #[test]
    fn where_the_box_cannot_fit_the_rows_are_plain_pairs() {
        let body = lines(&items(), &cfg(""), 12);

        assert!(
            body.iter().all(|l| !l.contains('│') && !l.contains('╭')),
            "{body:#?}"
        );
        assert!(body.iter().any(|l| l.starts_with("OS")), "{body:#?}");
        assert!(body.iter().all(|l| l.width() <= 12));
    }

    #[test]
    fn with_nothing_to_show_there_is_no_box() {
        assert!(lines(&[], &cfg(""), 80).is_empty());
        // Only a footer is still a box, holding it.
        let only = lines(&[], &cfg("hello"), 80).join("\n");
        assert!(only.contains("hello") && only.contains('╭'), "{only}");
    }

    #[test]
    fn the_measured_width_is_the_width_that_gets_drawn() {
        for footer in ["", MOTTO] {
            let c = cfg(footer);
            let drawn = lines(&items(), &c, 200)
                .iter()
                .map(|l| l.width())
                .max()
                .unwrap();
            assert_eq!(info_width("m@h", &items(), &c), drawn, "{footer:?}");
        }
    }

    #[test]
    fn colour_off_leaves_no_escapes_and_colour_on_uses_the_value_colour() {
        let plain = compose(&scene(&[], 0, "m@h", &items(), 100), &cfg(""));
        assert!(plain.iter().all(|l| !l.contains('\x1b')));

        let on = Config {
            color: true,
            ..cfg("")
        };
        let out = compose(&scene(&[], 0, "m@h", &items(), 100), &on);
        assert!(out[0].contains("\x1b[38;2;"), "{:?}", out[0]);
    }

    #[test]
    fn a_mount_is_named_for_its_last_part() {
        assert_eq!(mount_label(Some("/")), "Root");
        assert_eq!(mount_label(None), "Root");
        assert_eq!(mount_label(Some("/mnt/Jogos")), "Jogos");
        assert_eq!(mount_label(Some("/run/media/me/VIDS/")), "VIDS");
        assert_eq!(mount_label(Some("/home")), "home");
    }

    #[test]
    fn disk_figures_use_one_decimal_when_small_and_none_when_large() {
        assert_eq!(compact(1.5), 1);
        assert_eq!(compact(9.99), 1);
        assert_eq!(compact(10.0), 0);
        assert_eq!(compact(928.0), 0);
        assert_eq!(two(123.0), 2);
    }
}
