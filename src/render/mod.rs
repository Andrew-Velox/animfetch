//! Frame composition and painting.
//!
//! `compose` builds finished, coloured lines and does no I/O. [`Pane`] paints
//! them, rewriting only what changed. Clearing the screen every frame, which is
//! what most animated fetchers do, is the usual source of flicker.
//!
//! - [`art`]: the animation's rows, coloured down a gradient
//! - [`info`]: the info pane, in four styles that are each a few row types
//! - [`paint`], [`text`], [`scale`], [`icons`]: what those styles share
//! - [`pane`]: the incremental painter for the pinned region

mod art;
mod icons;
mod info;
mod paint;
mod pane;
mod scale;
#[cfg(test)]
mod test_util;
mod text;

pub use pane::Pane;

use crate::config::Config;
use crate::fetch::Item;
use paint::Painter;
use text::visible_width;

/// Everything that varies between frames, so `compose` takes one argument.
#[derive(Clone, Copy)]
pub struct Scene<'a> {
    /// Pre-scaled art rows for this frame.
    pub art: &'a [String],
    /// Column count the art was scaled to; the info pane starts after it.
    pub art_w: usize,
    pub title: &'a str,
    pub items: &'a [Item],
    /// Frame counter, used to scroll the gradient.
    pub phase: usize,
    /// Terminal width. Wider content is truncated; wrapping breaks the diff.
    pub width: usize,
}

/// Build the fetch pane: art on the left, info on the right. The prompt isn't
/// part of it; that lives below, in the scroll region.
pub fn compose(scene: &Scene<'_>, cfg: &Config) -> Vec<String> {
    let &Scene {
        art,
        art_w,
        title,
        items,
        phase,
        width: avail_w,
    } = scene;

    let info = info::build(title, items, cfg);

    // No art means nothing to separate, so no indent.
    let info_col = if art_w == 0 { 0 } else { art_w + cfg.gap };
    let info_w = avail_w.saturating_sub(info_col);

    // Centre the shorter pane against the taller one.
    let height = art.len().max(info.len());
    let art_top = (height - art.len()) / 2;
    let info_top = (height - info.len()) / 2;

    let mut out = Vec::with_capacity(height + 2);

    for y in 0..height {
        // Room for the columns plus the row's SGR sequences, so no realloc.
        let mut line = String::with_capacity(avail_w + 64);

        let art_row = y.checked_sub(art_top).and_then(|i| art.get(i));
        if let Some(row) = art_row {
            art::paint_art_row(&mut line, row, y, height, cfg, phase);
        }

        let info_row = y.checked_sub(info_top).filter(|i| *i < info.len());
        if let Some(row) = info_row {
            // Pad from the art's visible width; escapes occupy no columns.
            // `visible_width` rather than `width()`: raw-coloured art carries
            // SGR bytes that a plain width would count as columns.
            let drawn = art_row.map_or(0, |r| visible_width(r));
            for _ in drawn..info_col {
                line.push(' ');
            }
            info.paint(row, &mut Painter::new(&mut line, cfg), info_w);
        }

        // Trailing blanks are invisible but still cost bytes every repaint.
        line.truncate(line.trim_end().len());
        out.push(line);
    }

    out
}

/// The widest the info pane will draw, so the art can be fitted to what is
/// left. It is the width of the very rows `compose` paints, not a second
/// calculation that could drift from them.
pub fn info_width(title: &str, items: &[Item], cfg: &Config) -> usize {
    info::build(title, items, cfg).width()
}

#[cfg(test)]
mod tests {
    use super::test_util::*;
    use super::*;
    use crate::config::InfoStyle;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn no_style_ever_draws_wider_than_the_terminal() {
        // The one invariant every style must keep, checked at every width a
        // terminal could be, with colour and icons each way and the row sets
        // that stress the fallbacks. A wrapped line breaks the frame diff.
        let full = sample_items();
        let no_host: Vec<Item> = full.iter().filter(|i| i.label != "Host").cloned().collect();
        let sets: [(&str, &[Item]); 3] = [("full", &full), ("no-host", &no_host), ("empty", &[])];
        let art: Vec<String> = vec!["####".into(), "#  #".into()];

        for style in [
            InfoStyle::Classic,
            InfoStyle::Boxed,
            InfoStyle::Tree,
            InfoStyle::Sections,
            InfoStyle::Chips,
            InfoStyle::Iconic,
            InfoStyle::Card,
        ] {
            for (color, icons) in [(true, true), (true, false), (false, true), (false, false)] {
                let cfg = Config {
                    info_style: style,
                    color,
                    icons,
                    // Wide characters, which the styles that use it must count.
                    footer: "！一 我々は自由だ！".into(),
                    ..plain_cfg()
                };
                for (set, items) in sets {
                    for width in 0..=70 {
                        // Art is not cut to the terminal, so only give it room.
                        for art in [&[][..], &art[..]] {
                            let art_w = if art.is_empty() { 0 } else { 4 };
                            if art_w > 0 && width < art_w + cfg.gap {
                                continue;
                            }
                            let scene = Scene {
                                art,
                                art_w,
                                title: "harsh@fedora",
                                items,
                                phase: 0,
                                width,
                            };
                            for line in compose(&scene, &cfg) {
                                let seen = visible_width(&line);
                                assert!(
                                    seen <= width,
                                    "{style:?} color={color} icons={icons} set={set} \
                                     width={width}: {seen} columns in {:?}",
                                    strip_ansi(&line)
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn info_pane_starts_at_a_fixed_column_regardless_of_art_row() {
        // Trailing blanks are trimmed, so padding has to be computed.
        let cfg = plain_cfg();
        let art = vec!["####".to_string(), "#".to_string()];
        let items = vec![
            Item::new("OS", "Arch"),
            Item::new("CPU", "Ryzen"),
            Item::new("WM", "Hyprland"),
        ];

        let out = compose(&scene(&art, 4, "me@host", &items, 80), &cfg);
        let cols: Vec<usize> = out
            .iter()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                strip_ansi(l)
                    .find(|c: char| !c.is_whitespace())
                    .unwrap_or(0)
            })
            .collect();

        // Every info-bearing row must start at art_w + gap == 6, except rows
        // that also carry art (which start at column 0).
        assert!(cols.iter().all(|&c| c == 0 || c == 6), "columns: {cols:?}");
    }

    #[test]
    fn info_padding_ignores_escapes_in_raw_coloured_art() {
        // Art rows with embedded SGR must measure by visible columns only.
        let cfg = plain_cfg();
        let art = vec!["\x1b[38;2;9;9;9m####\x1b[0m".to_string(), "#".to_string()];
        let items = vec![Item::new("OS", "Arch")];
        let out = compose(&scene(&art, 4, "me@host", &items, 80), &cfg);

        let info_line = out.iter().find(|l| l.contains("OS")).unwrap();
        let stripped = strip_ansi(info_line);
        assert_eq!(stripped.find("OS"), Some(6), "info drifted: {stripped:?}");
    }

    #[test]
    fn without_art_the_info_pane_starts_at_column_zero() {
        let cfg = plain_cfg();
        let items = vec![Item::new("OS", "Arch")];
        let out = compose(&scene(&[], 0, "me@host", &items, 30), &cfg);

        assert!(
            out.iter()
                .filter(|l| !l.trim().is_empty())
                .all(|l| !l.starts_with(' ')),
            "unexpected indent: {out:?}"
        );
    }

    #[test]
    fn narrow_terminal_never_produces_lines_wider_than_it() {
        let cfg = plain_cfg();
        let items = vec![Item::new(
            "CPU",
            "AMD Ryzen 9 7950X 16-Core Processor (32) @ 5.88GHz",
        )];
        let art = vec!["####".to_string()];
        let out = compose(&scene(&art, 4, "me@host", &items, 20), &cfg);
        for line in &out {
            assert!(
                strip_ansi(line).width() <= 20,
                "too wide: {:?}",
                strip_ansi(line)
            );
        }
    }
}
