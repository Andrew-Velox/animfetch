//! How a row's reading becomes text, for the styles that show more than the
//! classic string.

use crate::fetch::{Charge, Detail, Item, Usage};
use crate::render::paint::Seg;
use crate::render::scale::{charge_color, fill_color};

/// The fullest text a row has: its detail if it carries one, else its value.
pub fn text(item: &Item) -> &str {
    match &item.detail {
        Some(Detail::Text(t)) => t,
        _ => &item.value,
    }
}

/// `4.39 GiB / 15.56 GiB (`, the percentage painted by how full it is, `)`.
pub fn usage_segs(usage: &Usage) -> Vec<Seg> {
    let (head, pct, tail) = usage.long();
    vec![
        (head, None),
        (pct, Some(fill_color(usage.percent()))),
        (tail, None),
    ]
}

/// `96%` painted by how charged it is, then the state.
pub fn charge_segs(charge: &Charge) -> Vec<Seg> {
    vec![
        (
            format!("{}%", charge.percent),
            Some(charge_color(charge.percent)),
        ),
        (format!(" [{}]", charge.status), None),
    ]
}

pub fn plain(text: impl Into<String>) -> Vec<Seg> {
    vec![(text.into(), None)]
}

/// The architecture words the OS row carries as a suffix.
const ARCHES: &[&str] = &[
    "x86_64", "aarch64", "arm64", "armv7l", "i686", "i386", "riscv64", "ppc64le",
];

/// `Arch Linux x86_64` as `Arch Linux`.
pub fn without_arch(os: &str) -> &str {
    match os.rsplit_once(' ') {
        Some((name, arch)) if ARCHES.contains(&arch) => name,
        _ => os,
    }
}

/// `XFCE (X11)` as `XFCE`: the desktop, not the protocol it speaks.
pub fn without_session(wm: &str) -> &str {
    ["X11", "Wayland"]
        .iter()
        .find_map(|s| wm.strip_suffix(&format!(" ({s})")))
        .unwrap_or(wm)
}

/// `1450 (pacman)` as `1450`: the count, without the manager it came from.
pub fn package_count(packages: &str) -> &str {
    match packages.split_once(' ') {
        Some((count, _)) if !count.is_empty() && count.bytes().all(|b| b.is_ascii_digit()) => count,
        _ => packages,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::paint::segs_width;

    #[test]
    fn the_operating_system_loses_only_a_trailing_architecture() {
        assert_eq!(without_arch("Arch Linux x86_64"), "Arch Linux");
        assert_eq!(without_arch("macOS 14.2 arm64"), "macOS 14.2");
        assert_eq!(without_arch("Arch Linux"), "Arch Linux");
        assert_eq!(without_arch("x86_64"), "x86_64", "a lone word is the name");
    }

    #[test]
    fn the_desktop_loses_only_a_session_suffix() {
        assert_eq!(without_session("XFCE (X11)"), "XFCE");
        assert_eq!(without_session("KDE (Wayland)"), "KDE");
        assert_eq!(without_session("Hyprland"), "Hyprland");
        assert_eq!(without_session("KDE (Plasma)"), "KDE (Plasma)");
    }

    #[test]
    fn packages_are_their_count() {
        assert_eq!(package_count("1450 (pacman)"), "1450");
        assert_eq!(package_count("1450"), "1450");
        assert_eq!(package_count("many (pacman)"), "many (pacman)");
        assert_eq!(package_count(""), "");
    }

    #[test]
    fn a_row_shows_its_detail_when_it_has_one() {
        let mut item = Item::new("Shell", "zsh");
        assert_eq!(text(&item), "zsh");
        item.detail = Some(Detail::Text("zsh 5.9".into()));
        assert_eq!(text(&item), "zsh 5.9");
    }

    #[test]
    fn usage_joins_back_into_one_line_with_only_the_percentage_coloured() {
        let segs = usage_segs(&Usage::new(2_000_000, 8_000_000));
        let joined: String = segs.iter().map(|s| s.0.as_str()).collect();

        assert_eq!(joined, "1.91 GiB / 7.63 GiB (25%)");
        assert_eq!(segs.iter().filter(|s| s.1.is_some()).count(), 1);
        assert_eq!(segs_width(&segs), joined.len());
    }

    #[test]
    fn charge_colours_the_percentage_and_leaves_the_state_alone() {
        let segs = charge_segs(&Charge {
            percent: 96,
            status: "AC Connected".into(),
        });
        assert_eq!(segs[0].0, "96%");
        assert!(segs[0].1.is_some());
        assert_eq!(segs[1], (" [AC Connected]".to_string(), None));
    }
}
