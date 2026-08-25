//! User configuration. A missing or malformed file is never fatal: fall back to
//! defaults and report the problem afterwards.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::color::{Color, Gradient, Rgb};

/// How the art is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    /// Half blocks: two vertical samples per cell, solid glyphs only.
    Half,
    /// Quadrant blocks: 2x2 samples per cell, for art with fine detail.
    Quad,
    /// A density ramp, configured by `ramp`. Use for a classic ASCII look.
    Ramp,
    /// The art's own characters, unresampled. For hand-drawn ASCII whose
    /// glyphs are the picture rather than a coverage mask.
    Raw,
}

/// Where the accent, value and gradient colours come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorSource {
    /// The `accent`, `value` and `gradient` keys below.
    Config,
    /// A palette file from matugen, pywal or wallust. Undefined roles fall back
    /// to the keys below, so this can only add colour, never remove it.
    Palette,
}

/// Where raw art gets its colour, when its frames carry SGR colours of their
/// own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtColor {
    /// The colours embedded in the frames.
    #[default]
    Own,
    /// Ignore the embedded colours; the gradient paints the art like plain art.
    Theme,
}

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

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Directory name under `<config>/anim/`, or the bundled animation.
    pub animation: String,
    /// Frames per second. Also the cadence at which input is drained.
    pub fps: f32,
    /// How the art is drawn. `ramp` only applies when this is `"ramp"`.
    pub style: Style,
    /// Where raw art's colour comes from: the file's own, or the theme's.
    pub art_color: ArtColor,
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
    /// Take the colours above as written, or look them up in a palette file.
    pub color_source: ColorSource,
    /// Candidate palette files, preferred first. `~/` is expanded.
    pub palette_files: Vec<String>,
    /// Names to look up in that file. Anything missing keeps the fixed colour.
    pub palette_accent: String,
    pub palette_value: String,
    /// Gradient stops, in order. An empty result keeps `gradient`.
    pub palette_gradient: Vec<String>,
    /// Blank columns between the art and the info pane.
    pub gap: usize,
    /// Largest box the art may occupy, aspect ratio kept. 0 means fill.
    /// Both caps matter: a wide, short animation slips under a height cap alone.
    pub width: usize,
    pub height: usize,
    /// Seconds the startup form (`--play`) animates before handing back.
    pub play_seconds: f32,
    /// Emit colour. Forced off when piped or when `NO_COLOR` is set.
    pub color: bool,
    pub modules: Vec<Module>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            animation: crate::anim::DEFAULT_NAME.into(),
            fps: 12.0,
            style: Style::Half,
            art_color: ArtColor::Own,
            ramp: " ░▒▓█".into(),
            gradient: Gradient::new(vec![
                Rgb(0xf5, 0xc9, 0x5c),
                Rgb(0xf1, 0x7f, 0x5b),
                Rgb(0xf4, 0x5c, 0x82),
                Rgb(0xb0, 0x7c, 0xf4),
                Rgb(0x74, 0xa4, 0xff),
            ]),
            // Off by default: a gradient that crawls through the art draws the
            // eye away from the terminal, which is the thing you are meant to be
            // looking at.
            gradient_scroll: false,
            accent: Color(Rgb(0x74, 0xa4, 0xff)),
            value: Color(Rgb(0xc8, 0xcc, 0xd4)),
            // Off by default, so an update never repaints someone's fetch.
            color_source: ColorSource::Config,
            palette_files: vec![
                "~/.local/state/quickshell/user/generated/colors.json".into(),
                "~/.cache/wal/colors.json".into(),
                "~/.cache/wallust/colors.json".into(),
            ],
            // Material You names. pywal/wallust use `color0`..`color15`.
            palette_accent: "primary".into(),
            palette_value: "on_surface".into(),
            palette_gradient: vec!["primary".into(), "secondary".into(), "tertiary".into()],
            gap: 4,
            // About the size of a distro logo in a conventional fetch.
            width: 56,
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
    /// Frame interval from `fps`, clamped to what a terminal can keep up with.
    pub fn frame_interval(&self) -> std::time::Duration {
        let fps = self.fps.clamp(1.0, 120.0);
        std::time::Duration::from_secs_f32(1.0 / fps)
    }

    /// Columns the art may occupy, given how many are actually available.
    pub fn width(&self, available: usize) -> usize {
        if self.width == 0 {
            available
        } else {
            self.width.min(available)
        }
    }

    /// Rows the fetch may occupy, given how many are actually available.
    pub fn height(&self, available: usize) -> usize {
        if self.height == 0 {
            available
        } else {
            self.height.min(available)
        }
    }

    /// The density ramp as characters, never empty.
    pub fn ramp(&self) -> Vec<char> {
        let ramp: Vec<char> = self.ramp.chars().collect();
        if ramp.is_empty() {
            vec![' ', '█']
        } else {
            ramp
        }
    }

    /// How to turn coverage into glyphs, for the configured style.
    pub fn ink<'a>(&self, ramp: &'a [char]) -> crate::anim::Ink<'a> {
        match self.style {
            Style::Half => crate::anim::Ink::Half,
            Style::Quad => crate::anim::Ink::Quad,
            Style::Ramp => crate::anim::Ink::Ramp(ramp),
            // `theme` hands the art back plain, so the gradient applies to it
            // exactly as it would to escape-free art.
            Style::Raw => crate::anim::Ink::Raw {
                color: self.color && self.art_color == ArtColor::Own,
            },
        }
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

/// Load `config.toml`. Returns defaults plus a warning if the file is broken.
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
    fn a_broken_file_falls_back_but_says_so() {
        // The fallback is deliberate: a stray comma should not stop the fetch
        // drawing. Reporting it is not optional though, or the user's settings
        // vanish with nothing said. `run` prints this for every mode.
        let dir = std::env::temp_dir().join(format!("animfetch-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.toml"), "animation = \"tree\"\ngap = = 3\n").unwrap();

        let (cfg, warning) = load(Some(&dir));
        assert_eq!(
            cfg.animation,
            Config::default().animation,
            "should be defaults"
        );
        assert!(warning.is_some(), "a broken config must not fail silently");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_valid_file_warns_about_nothing() {
        let dir = std::env::temp_dir().join(format!("animfetch-ok-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.toml"), "animation = \"tree\"\n").unwrap();

        let (cfg, warning) = load(Some(&dir));
        assert_eq!(cfg.animation, "tree");
        assert!(warning.is_none());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn partial_config_keeps_defaults_for_the_rest() {
        let cfg: Config = toml::from_str("fps = 30.0").unwrap();
        assert_eq!(cfg.fps, 30.0);
        assert_eq!(cfg.animation, "cat-run");
        assert!(!cfg.modules.is_empty());
    }

    #[test]
    fn width_cap_is_bounded_by_what_is_available() {
        let cfg: Config = toml::from_str("width = 56").unwrap();
        assert_eq!(cfg.width(120), 56);
        assert_eq!(cfg.width(30), 30);
        assert_eq!(
            toml::from_str::<Config>("width = 0").unwrap().width(120),
            120
        );
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
    fn style_selects_the_renderer() {
        let cfg: Config = toml::from_str(r#"style = "ramp""#).unwrap();
        assert!(matches!(cfg.ink(&[' ', '#']), crate::anim::Ink::Ramp(_)));
        assert!(matches!(
            Config::default().ink(&[' ', '#']),
            crate::anim::Ink::Half
        ));
    }

    #[test]
    fn art_color_selects_whose_colour_raw_art_wears() {
        let own: Config = toml::from_str(r#"style = "raw""#).unwrap();
        assert!(matches!(
            own.ink(&[' ', '#']),
            crate::anim::Ink::Raw { color: true }
        ));

        // `theme` hands the art back plain so the gradient applies, but the
        // global colour switch still wins over both.
        let theme: Config = toml::from_str("style = \"raw\"\nart_color = \"theme\"").unwrap();
        assert!(matches!(
            theme.ink(&[' ', '#']),
            crate::anim::Ink::Raw { color: false }
        ));

        let off: Config = toml::from_str("style = \"raw\"\ncolor = false").unwrap();
        assert!(matches!(
            off.ink(&[' ', '#']),
            crate::anim::Ink::Raw { color: false }
        ));
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

    #[test]
    fn the_shipped_example_config_parses() {
        // `deny_unknown_fields` means a drifted key breaks the shipped example.
        let text = include_str!("../config.example.toml");
        let cfg: Config = toml::from_str(text).expect("config.example.toml must parse");
        assert_eq!(
            cfg.color_source,
            ColorSource::Config,
            "the example must not change behaviour"
        );
    }
}
