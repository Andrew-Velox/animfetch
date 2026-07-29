//! User configuration.
//!
//! A missing or malformed config is never fatal: we fall back to the defaults
//! below and report the problem afterwards, because a fetch tool that refuses
//! to draw because of a stray comma is worse than one that draws with defaults.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::color::{Color, Gradient, Rgb};

/// Which rows appear in the info pane, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Module {
    Os,
    Host,
    Kernel,
    Uptime,
    Packages,
    Shell,
    Wm,
    Terminal,
    Cpu,
    Memory,
    Swap,
    Disk,
    /// The classic 8-swatch terminal palette strip.
    Colors,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Directory name under `<config>/anim/`, or the bundled animation.
    pub animation: String,
    /// Frames per second. Also the cadence at which input is drained.
    pub fps: f32,
    /// Density ramp, lowest coverage first. Must be at least one character.
    pub ramp: String,
    /// Vertical colour ramp applied down the art.
    pub gradient: Gradient,
    /// Scroll the gradient through the art as it animates.
    pub gradient_scroll: bool,
    /// Colour for `user@host` and info labels.
    pub accent: Color,
    /// Colour for info values.
    pub value: Color,
    /// Blank columns between the art and the info pane.
    pub gap: usize,
    /// Largest number of rows the fetch may occupy. The art is scaled to fit,
    /// keeping its aspect ratio. 0 lets it use whatever space is available,
    /// which on a tall terminal means a very large cat.
    pub height: usize,
    /// Seconds the startup form (`--play`) animates before handing back.
    pub play_seconds: f32,
    /// Emit colour. Forced off when stdout is not a terminal or `NO_COLOR` is
    /// set, so this only decides the case where colour is possible.
    pub color: bool,
    pub modules: Vec<Module>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            animation: crate::anim::BUNDLED_NAME.into(),
            fps: 12.0,
            ramp: " ░▒▓█".into(),
            gradient: Gradient::new(vec![
                Rgb(0xf5, 0xc9, 0x5c),
                Rgb(0xf1, 0x7f, 0x5b),
                Rgb(0xf4, 0x5c, 0x82),
                Rgb(0xb0, 0x7c, 0xf4),
                Rgb(0x74, 0xa4, 0xff),
            ]),
            gradient_scroll: true,
            accent: Color(Rgb(0x74, 0xa4, 0xff)),
            value: Color(Rgb(0xc8, 0xcc, 0xd4)),
            gap: 4,
            // Roughly the height of a distro logo in a conventional fetch, and
            // a good match for the number of info rows beside it.
            height: 20,
            play_seconds: 3.0,
            color: true,
            modules: vec![
                Module::Os,
                Module::Host,
                Module::Kernel,
                Module::Uptime,
                Module::Packages,
                Module::Shell,
                Module::Wm,
                Module::Terminal,
                Module::Cpu,
                Module::Memory,
                Module::Swap,
                Module::Disk,
                Module::Colors,
            ],
        }
    }
}

impl Config {
    /// Frame interval derived from `fps`, clamped to something a terminal can
    /// actually keep up with.
    pub fn frame_interval(&self) -> std::time::Duration {
        let fps = self.fps.clamp(1.0, 120.0);
        std::time::Duration::from_secs_f32(1.0 / fps)
    }

    /// Rows the fetch may occupy, given how many are actually available.
    pub fn height(&self, available: usize) -> usize {
        if self.height == 0 { available } else { self.height.min(available) }
    }

    /// The density ramp as characters, never empty.
    pub fn ramp(&self) -> Vec<char> {
        let ramp: Vec<char> = self.ramp.chars().collect();
        if ramp.is_empty() { vec![' ', '█'] } else { ramp }
    }
}

/// `$XDG_CONFIG_HOME/animfetch`, else `$HOME/.config/animfetch`.
pub fn config_dir() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        return Some(Path::new(&xdg).join("animfetch"));
    }
    let home = std::env::var_os("HOME").filter(|v| !v.is_empty())?;
    Some(Path::new(&home).join(".config").join("animfetch"))
}

/// Load `config.toml` from `dir`. Returns the defaults plus a human-readable
/// warning when the file exists but cannot be used.
pub fn load(dir: Option<&Path>) -> (Config, Option<String>) {
    let Some(path) = dir.map(|d| d.join("config.toml")) else {
        return (Config::default(), None);
    };

    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        // Absent config is the normal case, not a problem worth reporting.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (Config::default(), None);
        }
        Err(e) => {
            return (Config::default(), Some(format!("{}: {e}", path.display())));
        }
    };

    match toml::from_str(&text) {
        Ok(cfg) => (cfg, None),
        Err(e) => (
            Config::default(),
            Some(format!("{}: {}", path.display(), e.message())),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_config_keeps_defaults_for_the_rest() {
        let cfg: Config = toml::from_str("fps = 30.0").unwrap();
        assert_eq!(cfg.fps, 30.0);
        assert_eq!(cfg.animation, "cat-run");
        assert!(!cfg.modules.is_empty());
    }

    #[test]
    fn height_cap_is_bounded_by_what_is_available() {
        let cfg: Config = toml::from_str("height = 20").unwrap();
        assert_eq!(cfg.height(40), 20, "cap applies when there is room");
        assert_eq!(cfg.height(12), 12, "never exceeds the space available");

        let uncapped: Config = toml::from_str("height = 0").unwrap();
        assert_eq!(uncapped.height(40), 40, "0 means fill");
    }

    #[test]
    fn empty_ramp_falls_back() {
        let cfg: Config = toml::from_str(r#"ramp = """#).unwrap();
        assert_eq!(cfg.ramp(), vec![' ', '█']);
    }

    #[test]
    fn fps_is_clamped_away_from_zero() {
        let cfg: Config = toml::from_str("fps = 0.0").unwrap();
        assert_eq!(cfg.frame_interval(), std::time::Duration::from_secs(1));
    }

    #[test]
    fn bad_gradient_entries_are_skipped_not_fatal() {
        // Extra hashes: the TOML itself contains a `"#` sequence.
        let cfg: Config = toml::from_str(r##"gradient = ["#ff0000", "nonsense"]"##).unwrap();
        assert_eq!(cfg.gradient.sample(0.0), Some(Rgb(255, 0, 0)));
    }

    #[test]
    fn unknown_key_is_reported_rather_than_silently_ignored() {
        assert!(toml::from_str::<Config>("fpz = 30.0").is_err());
    }
}
