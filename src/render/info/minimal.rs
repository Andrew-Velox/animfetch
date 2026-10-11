//! An icon and a value per row, and nothing else: no title, no labels, no
//! frame. One colour throughout, like the line art it is drawn beside.
//!
//! Rows go in the order of `modules`, and values are the fuller ones (a
//! version for the shell and terminal, the session for the compositor), since
//! with no labels the value has to say what it is.

use unicode_width::UnicodeWidthStr;

use super::classic::paint_pair;
use super::{Line, is_swatch, value};
use crate::config::Config;
use crate::fetch::{Detail, Item};
use crate::render::icons;
use crate::render::paint::Painter;
use crate::render::text::truncate;

pub struct Row {
    /// An icon, or its short name padded to a column where there are no icons.
    mark: String,
    /// The row as a plain pair, for a terminal too narrow for the icon.
    label: &'static str,
    text: String,
}

/// What a row says here. The model alone for the processor: its core count and
/// clock are not what anyone reads a row like this for.
fn text_of(item: &Item) -> String {
    match &item.detail {
        Some(Detail::Usage(u)) => match &u.name {
            Some(name) => format!("{name}: {}", u.spaced()),
            None => u.spaced(),
        },
        _ if item.label == "CPU" => crate::fetch::cpu_brand(&item.value),
        _ => value::text(item).to_string(),
    }
}

pub fn rows(items: &[Item], cfg: &Config) -> Vec<Row> {
    let shown: Vec<&Item> = items.iter().filter(|i| !is_swatch(i)).collect();

    // A row of your own has no entry in the table: it is named for itself.
    let short = |item: &Item| {
        icons::lookup(item.label).map_or_else(|| item.label.to_lowercase(), |g| g.short.to_string())
    };
    let short_w = shown.iter().map(|i| short(i).width()).max().unwrap_or(0);

    shown
        .into_iter()
        .map(|item| Row {
            mark: if cfg.icons {
                icons::glyph(item.label).icon.to_string()
            } else {
                format!("{:<short_w$}", short(item))
            },
            label: item.label,
            text: text_of(item),
        })
        .collect()
}

impl Line for Row {
    fn width(&self) -> usize {
        self.mark.width() + 1 + self.text.width()
    }

    fn paint(&self, p: &mut Painter<'_>, width: usize) {
        let Some(room) = width.checked_sub(self.mark.width() + 1).filter(|r| *r > 0) else {
            return paint_pair(p, self.label, &self.text, width);
        };

        let color = p.value();
        p.text(
            color,
            &format!("{} {}", self.mark, truncate(&self.text, room)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::InfoStyle;
    use crate::fetch::Usage;
    use crate::render::test_util::*;
    use crate::render::{compose, info_width};

    fn cfg(icons: bool) -> Config {
        Config {
            info_style: InfoStyle::Minimal,
            icons,
            color: false,
            ..plain_cfg()
        }
    }

    /// The rows of the reference, as the fuller values the style asks for.
    fn items() -> Vec<Item> {
        let rich = |label: &'static str, classic: &str, fuller: &str| {
            let mut item = Item::new(label, classic);
            item.detail = Some(Detail::Text(fuller.into()));
            item
        };
        vec![
            Item::new("OS", "Arch Linux x86_64"),
            rich("Kernel", "7.2.6-arch2-1", "Linux 7.2.6-arch2-1"),
            rich("WM", "mango", "mango (Wayland)"),
            rich("Shell", "zsh", "zsh 5.9.2"),
            rich("Terminal", "kitty", "kitty 0.48.2"),
            rich(
                "CPU",
                "AMD Ryzen 3 5300U with Radeon Graphics (8) @ 3.80GHz",
                "AMD Ryzen 3 5300U with Radeon Graphics (8) @ 3.80 GHz",
            ),
            Item::new("GPU", "AMD Lucienne [Integrated]"),
            Item::usage("Memory", Some(Usage::new(2_778_726, 23_886_561))).unwrap(),
            Item::duration("OS Age", 19 * 86400, "19 days"),
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
        // Without icons each row begins with a short name; what follows it is
        // the value, which is what the reference shows.
        let body = lines(&items(), &cfg(false), 80);
        let values: Vec<&str> = body
            .iter()
            .map(|l| l.split_once(' ').expect("a name, then the value").1.trim())
            .collect();

        assert_eq!(
            values,
            [
                "Arch Linux x86_64",
                "Linux 7.2.6-arch2-1",
                "mango (Wayland)",
                "zsh 5.9.2",
                "kitty 0.48.2",
                "AMD Ryzen 3 5300U with Radeon Graphics",
                "AMD Lucienne [Integrated]",
                "2.65 GiB / 22.78 GiB",
                "19 days",
            ]
        );
    }

    #[test]
    fn there_is_no_title_no_rule_and_no_labels_when_there_are_icons() {
        let body = lines(&items(), &cfg(true), 80);
        let text = body.join("\n");

        assert_eq!(body.len(), 9, "one line per row, and nothing above them");
        assert!(!text.contains("me@host") && !text.contains('─'), "{text}");
        for label in ["OS", "Kernel", "Shell", "Memory", "CPU", "Terminal"] {
            assert!(
                !body.iter().any(|l| l.starts_with(label)),
                "{label} is a label:\n{text}"
            );
        }
        assert!(body[0].starts_with('\u{f17c}'), "the distro icon leads");
    }

    #[test]
    fn rows_come_in_the_order_of_the_list_and_swatches_are_left_out() {
        let order = ["OS Age", "OS", "GPU"];
        let mut picked: Vec<Item> = order
            .iter()
            .flat_map(|l| items().into_iter().filter(move |i| i.label == *l))
            .collect();
        picked.push(Item::new("", "swatches"));

        let body = lines(&picked, &cfg(false), 80);
        assert_eq!(body.len(), 3, "{body:?}");
        assert!(body[0].contains("19 days"));
        assert!(body[1].contains("Arch Linux"));
        assert!(body[2].contains("Lucienne"));
    }

    #[test]
    fn without_icons_each_row_names_itself_in_one_aligned_column() {
        let body = lines(&items(), &cfg(false), 80);
        let col = |needle: &str| {
            let line = body.iter().find(|l| l.contains(needle)).unwrap();
            line[..line.find(needle).unwrap()].width()
        };

        assert_eq!(col("Arch Linux"), col("zsh 5.9.2"));
        assert_eq!(col("Arch Linux"), col("19 days"));
        assert!(body.iter().any(|l| l.starts_with("gpu")), "{body:?}");
    }

    #[test]
    fn a_row_of_your_own_is_shown_with_its_own_name() {
        let items = vec![Item::custom("Owner", "POKKITA"), Item::new("OS", "Arch")];

        let plain = lines(&items, &cfg(false), 80);
        assert!(plain[0].starts_with("owner"), "{plain:?}");
        let iconed = lines(&items, &cfg(true), 80);
        assert!(iconed[0].starts_with('\u{f10c}') && iconed[0].contains("POKKITA"));
    }

    #[test]
    fn a_disk_says_where_it_is() {
        let usage = Usage::new(10 * 1024 * 1024, 50 * 1024 * 1024).with_name(Some("/home".into()));
        let items = vec![Item::usage("Disk", Some(usage)).unwrap()];

        let body = lines(&items, &cfg(false), 80).join("\n");
        assert!(body.contains("/home: 10.00 GiB / 50.00 GiB"), "{body}");
    }

    #[test]
    fn a_value_too_long_for_the_room_is_cut_not_wrapped() {
        let body = lines(&items(), &cfg(false), 20);
        assert!(body.iter().any(|l| l.contains('…')));
        assert!(body.iter().all(|l| l.width() <= 20), "{body:?}");
    }

    #[test]
    fn the_measured_width_is_the_width_that_gets_drawn() {
        for icons in [true, false] {
            let c = cfg(icons);
            let drawn = lines(&items(), &c, 200)
                .iter()
                .map(|l| l.width())
                .max()
                .unwrap();
            assert_eq!(info_width("m@h", &items(), &c), drawn, "icons={icons}");
        }
    }

    #[test]
    fn colour_is_one_value_colour_throughout() {
        let on = Config {
            color: true,
            ..cfg(true)
        };
        let out = compose(&scene(&[], 0, "m@h", &items(), 80), &on);

        // Every line is a single colour run: icon and text share it.
        for line in &out {
            assert_eq!(line.matches("\x1b[38;2;").count(), 1, "{line:?}");
        }
    }
}
