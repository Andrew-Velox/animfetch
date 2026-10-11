//! Helpers the render tests share.

use crate::color::{Color, Gradient, Rgb};
use crate::config::Config;
use crate::fetch::Item;

use super::Scene;

/// A config with colour off and the gradient out of the way, so a test sees
/// only the layout.
pub fn plain_cfg() -> Config {
    Config {
        gradient: Gradient::default(),
        gradient_scroll: false,
        accent: Color(Rgb(0, 0, 0)),
        value: Color(Rgb(0, 0, 0)),
        gap: 2,
        ..Config::default()
    }
}

pub fn scene<'a>(
    art: &'a [String],
    art_w: usize,
    title: &'a str,
    items: &'a [Item],
    width: usize,
) -> Scene<'a> {
    Scene {
        art,
        art_w,
        title,
        items,
        phase: 0,
        width,
        border: None,
    }
}

pub use crate::color::strip_sgr as strip_ansi;

/// A machine's worth of rows, with every kind of value a style has to handle:
/// plain text, a detail, a usage, a named usage, a charge, a swatch.
pub fn sample_items() -> Vec<Item> {
    use crate::fetch::{Detail, Usage};

    let text = |label: &'static str, classic: &str, rich: &str| {
        let mut item = Item::new(label, classic);
        item.detail = Some(Detail::Text(rich.into()));
        item
    };
    let disk = |at: &str| {
        Item::usage(
            "Disk",
            Some(
                Usage::new(10_400_000, 50_000_000)
                    .with_name(Some(at.into()))
                    .with_note(Some("ext4".into())),
            ),
        )
        .unwrap()
    };

    vec![
        text("OS", "Arch Linux x86_64", "Arch Linux x86_64"),
        text("Host", "20L5S1R500 ThinkPad", "20L5S1R500 (ThinkPad)"),
        text("Kernel", "6.17.9-artix1-1", "Linux 6.17.9-artix1-1"),
        Item::duration("Uptime", 3 * 3600 + 19 * 60 + 7, "3 hours, 19 mins"),
        Item::new("Packages", "575 (pacman)"),
        text("Shell", "zsh", "zsh 5.9"),
        text("WM", "Hyprland", "Hyprland 0.52.2 (Wayland)"),
        text("Terminal", "ghostty", "ghostty 1.3.1-arch2"),
        Item::new("Font", "Roboto (11pt)"),
        Item::new("CPU", "Intel(R) Core(TM) i7-8650U (8) @ 4.20GHz"),
        Item::new("GPU", "Intel UHD Graphics 620 [Integrated]"),
        Item::usage("Memory", Some(Usage::new(4_460_000, 16_100_000))).unwrap(),
        Item::usage("Swap", Some(Usage::new(9_000_000, 10_000_000))).unwrap(),
        disk("/"),
        disk("/home"),
        Item::new("Display", "1920x1080 @ 60.00 Hz"),
        Item::duration("OS Age", 337 * 86400, "337 days"),
        Item::new("Temperature", "50.50°C"),
        Item::new("Shell Path", "/usr/bin/zsh"),
        Item::custom("Owner", "POKKITA"),
        Item::charge("Battery", 100, "AC Connected"),
        Item::charge("Battery", 12, "Discharging"),
        Item::new("", "\x1b[30m███\x1b[31m███\x1b[0m"),
    ]
}
