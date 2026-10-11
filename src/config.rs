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

/// A setting written as one of a few words. The same table is what a config
/// file may hold, what a flag accepts, what `--save` writes back, and the list
/// of choices an error message offers, so none of them can disagree.
pub trait Choice: Copy + PartialEq + 'static {
    const WORDS: &'static [(&'static str, Self)];

    fn word(self) -> &'static str {
        Self::WORDS
            .iter()
            .find(|(_, v)| *v == self)
            .map_or("", |(w, _)| w)
    }

    fn from_word(word: &str) -> Option<Self> {
        Self::WORDS
            .iter()
            .find(|(w, _)| *w == word)
            .map(|(_, v)| *v)
    }

    /// `half, quad, ramp, or raw`, as an error message would put it.
    fn list() -> String {
        let words: Vec<&str> = Self::WORDS.iter().map(|(w, _)| *w).collect();
        match words.as_slice() {
            [] => String::new(),
            [only] => (*only).to_string(),
            [a, b] => format!("{a} or {b}"),
            [rest @ .., last] => format!("{}, or {last}", rest.join(", ")),
        }
    }
}

impl Choice for Style {
    const WORDS: &'static [(&'static str, Self)] = &[
        ("half", Style::Half),
        ("quad", Style::Quad),
        ("ramp", Style::Ramp),
        ("raw", Style::Raw),
    ];
}

impl Choice for ArtColor {
    const WORDS: &'static [(&'static str, Self)] =
        &[("own", ArtColor::Own), ("theme", ArtColor::Theme)];
}

impl Choice for InfoStyle {
    const WORDS: &'static [(&'static str, Self)] = &[
        ("classic", InfoStyle::Classic),
        ("boxed", InfoStyle::Boxed),
        ("tree", InfoStyle::Tree),
        ("sections", InfoStyle::Sections),
        ("chips", InfoStyle::Chips),
        ("iconic", InfoStyle::Iconic),
        ("card", InfoStyle::Card),
    ];
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
    /// Graphics card name, with discrete or integrated when it can be told.
    Gpu,
    Memory,
    Swap,
    Disk,
    /// Resolution and refresh rate of each display with a mode set.
    Display,
    /// The desktop's UI font, from GTK's settings. Skipped without them.
    Font,
    /// Every local filesystem, one row each, as opposed to `disk`, which is
    /// the root alone.
    Disks,
    /// Charge and power state, one row per battery. Skipped on a machine
    /// without one.
    Battery,
    /// The rows you wrote under `custom`, placed where this stands.
    Custom,
    /// The CPU's temperature, from its hwmon sensor. Skipped without one.
    #[serde(rename = "temperature")]
    Temperature,
    /// The full path of `$SHELL`, as opposed to `shell`, its name.
    #[serde(rename = "shell_path")]
    ShellPath,
    /// Time since the root filesystem was created, which is when the system
    /// was installed.
    #[serde(rename = "os_age")]
    OsAge,
    /// The classic 8-swatch terminal palette strip.
    Colors,
}

/// How the info pane is laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InfoStyle {
    /// `user@host`, a rule, then `Label: value` rows.
    #[default]
    Classic,
    /// Icons and short labels in a rounded box, values beside it. Reads fuller
    /// values (versions, filesystem) at the cost of a couple of subprocesses.
    Boxed,
    /// A user box over two groups, OS and PC, drawn as a tree of icons beside
    /// a column of `: value` lines. Shows a fixed set of rows.
    Tree,
    /// Titled sections (Hardware, Software, Uptime / Age) separated by rules,
    /// with a coloured dot row under them.
    Sections,
    /// Section headings and values as filled, rounded chips, with the colours
    /// as dots. Its values are short and bare, so it reads nothing extra.
    Chips,
    /// A bare coloured icon per row and no labels, under pill headings, with
    /// a row of outline shapes below.
    Iconic,
    /// A single rounded box: each row's label at the left edge and its value
    /// at the right, then a line of your own text below.
    Card,
}

/// A row of your own text, for `custom`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomRow {
    pub label: String,
    pub value: String,
}

const CLASSIC_MODULES: &[Module] = &[
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
];

const BOXED_MODULES: &[Module] = &[
    Module::Os,
    Module::Kernel,
    Module::Wm,
    Module::Terminal,
    Module::Shell,
    Module::Cpu,
    Module::Gpu,
    Module::Disk,
    Module::Memory,
    Module::Uptime,
    Module::OsAge,
];

/// What the tree style draws: the OS group (distro, shell, terminal) and the
/// PC group (host, gpu, memory, display). `cpu` is only the PC group's heading
/// on a machine whose firmware gives no usable host name.
const TREE_MODULES: &[Module] = &[
    Module::Os,
    Module::Shell,
    Module::Terminal,
    Module::Host,
    Module::Cpu,
    Module::Gpu,
    Module::Memory,
    Module::Display,
];

/// The sections style shows these, grouped by what it knows about them.
const SECTIONS_MODULES: &[Module] = &[
    Module::Host,
    Module::Cpu,
    Module::Gpu,
    Module::Memory,
    Module::Disks,
    Module::Os,
    Module::Kernel,
    Module::Packages,
    Module::Shell,
    Module::Wm,
    Module::OsAge,
    Module::Uptime,
    Module::Battery,
    Module::Colors,
];

const CHIPS_MODULES: &[Module] = &[
    Module::Os,
    Module::Kernel,
    Module::Uptime,
    Module::Packages,
    Module::Wm,
    Module::Shell,
    Module::Terminal,
    Module::Font,
    Module::Cpu,
    Module::Gpu,
    Module::Memory,
    Module::Disk,
    Module::Colors,
];

const ICONIC_MODULES: &[Module] = &[
    Module::Uptime,
    Module::Display,
    Module::Gpu,
    Module::Memory,
    Module::Swap,
    Module::Cpu,
    Module::Temperature,
    Module::Kernel,
    Module::Wm,
    Module::ShellPath,
    Module::Packages,
    Module::Colors,
];

const CARD_MODULES: &[Module] = &[
    Module::Os,
    Module::Kernel,
    Module::Uptime,
    Module::Shell,
    Module::Wm,
    Module::OsAge,
    Module::Memory,
    Module::Cpu,
    Module::Disks,
    Module::Packages,
    Module::Custom,
];

impl InfoStyle {
    /// The rows this style shows when `modules` is not set.
    pub fn default_modules(self) -> &'static [Module] {
        match self {
            InfoStyle::Classic => CLASSIC_MODULES,
            InfoStyle::Boxed => BOXED_MODULES,
            InfoStyle::Tree => TREE_MODULES,
            InfoStyle::Sections => SECTIONS_MODULES,
            InfoStyle::Chips => CHIPS_MODULES,
            InfoStyle::Iconic => ICONIC_MODULES,
            InfoStyle::Card => CARD_MODULES,
        }
    }

    /// Whether rows should be collected with their fuller values. Not free:
    /// versions of the shell and window manager come from running them.
    pub fn wants_detail(self) -> bool {
        // Chips show names and bare figures, so they ask for nothing extra.
        !matches!(
            self,
            InfoStyle::Classic | InfoStyle::Chips | InfoStyle::Iconic | InfoStyle::Card
        )
    }
}

impl Module {
    /// Whether the row's value changes while the fetch is on screen. These are
    /// the only rows `live` re-reads; the rest (OS, kernel, CPU model...) are
    /// facts about the machine, and re-reading them would be wasted work.
    pub fn is_live(self) -> bool {
        matches!(
            self,
            Module::Uptime
                | Module::Memory
                | Module::Swap
                | Module::Disk
                | Module::Disks
                | Module::Battery
                | Module::Temperature
        )
    }
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
    /// How the info pane is laid out.
    pub info_style: InfoStyle,
    /// Draw an icon beside each label in styles that have them. They are Nerd
    /// Font glyphs, which render as empty boxes without one.
    pub icons: bool,
    /// Keep the info rows whose numbers move current while the fetch is pinned
    /// or interactive. Off by default, so an update never changes a fetch.
    pub live: bool,
    /// Seconds between refreshes when `live` is on. Clamped to 0.5-60.
    pub live_interval: f32,
    /// Emit colour. Forced off when piped or when `NO_COLOR` is set.
    pub color: bool,
    /// Rows to show, in order. Unset means the preset for `info_style`; set,
    /// it wins whatever the style is.
    pub modules: Option<Vec<Module>>,
    /// Rows of your own text. They appear where `custom` is in `modules`, and
    /// only in the styles that list rows freely.
    pub custom: Vec<CustomRow>,
    /// A line of your own text, in the styles that have room for one.
    pub footer: String,
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
            info_style: InfoStyle::Classic,
            icons: true,
            live: false,
            live_interval: 2.0,
            color: true,
            modules: None,
            custom: Vec::new(),
            footer: String::new(),
        }
    }
}

impl Config {
    /// Frame interval from `fps`, clamped to what a terminal can keep up with.
    pub fn frame_interval(&self) -> std::time::Duration {
        let fps = self.fps.clamp(1.0, 120.0);
        std::time::Duration::from_secs_f32(1.0 / fps)
    }

    /// The rows to show: the configured list, else the style's own.
    pub fn modules(&self) -> &[Module] {
        self.modules
            .as_deref()
            .unwrap_or_else(|| self.info_style.default_modules())
    }

    /// How often live rows are re-read, clamped so a typo can neither spin the
    /// loop nor leave the numbers frozen for minutes.
    pub fn live_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs_f32(self.live_interval.clamp(0.5, 60.0))
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

/// Write `key = value` into the config, creating it if needed.
///
/// Edits the file as text so comments and every key left at its default
/// survive; reserialising the parsed config would discard both. A trailing
/// comment on the line being replaced is the one thing this loses.
pub fn set_key(dir: &Path, key: &str, value: &str) -> std::io::Result<PathBuf> {
    use std::io::ErrorKind;

    std::fs::create_dir_all(dir)?;
    let path = dir.join("config.toml");

    let existing = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };

    // The key itself, not another that begins with it: `live` must not take
    // over the `live_interval` line.
    let assigns = |line: &str| {
        line.trim_start()
            .strip_prefix(key)
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    };

    let setting = format!("{key} = {value}");
    let mut replaced = false;
    let mut out = String::with_capacity(existing.len() + setting.len() + 1);

    for line in existing.lines() {
        if !replaced && assigns(line) {
            out.push_str(&setting);
            replaced = true;
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    if !replaced {
        out.push_str(&setting);
        out.push('\n');
    }

    std::fs::write(&path, out)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_a_key_leaves_the_rest_of_the_config_alone() {
        // The point of editing as text: comments and untouched keys survive.
        // Reserialising the parsed config would discard both.
        let dir = std::env::temp_dir().join(format!("animfetch-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.toml"),
            "# keep me\nanimation = \"tree\"\nstyle = \"half\"\nwidth = 62\n",
        )
        .unwrap();

        set_key(&dir, "style", "\"quad\"").unwrap();
        let out = std::fs::read_to_string(dir.join("config.toml")).unwrap();

        assert!(out.contains("# keep me"), "comment lost: {out}");
        assert!(out.contains("style = \"quad\""), "not replaced: {out}");
        assert!(
            out.contains("animation = \"tree\""),
            "other key lost: {out}"
        );
        assert!(out.contains("width = 62"), "other key lost: {out}");
        assert_eq!(out.matches("style =").count(), 1, "duplicated: {out}");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn saving_a_key_the_file_lacks_appends_it() {
        let dir = std::env::temp_dir().join(format!("animfetch-add-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.toml"), "animation = \"tree\"\n").unwrap();

        set_key(&dir, "fps", "8").unwrap();
        let out = std::fs::read_to_string(dir.join("config.toml")).unwrap();

        assert!(out.contains("animation = \"tree\""));
        assert!(out.contains("fps = 8"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn saving_a_key_does_not_take_over_one_that_merely_begins_with_it() {
        let dir = std::env::temp_dir().join(format!("animfetch-prefix-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.toml"),
            "live_interval = 5.0\n# live = false\nlive = false\n",
        )
        .unwrap();

        set_key(&dir, "live", "true").unwrap();
        let out = std::fs::read_to_string(dir.join("config.toml")).unwrap();

        assert!(out.contains("live_interval = 5.0"), "clobbered: {out}");
        assert!(out.contains("# live = false"), "comment rewritten: {out}");
        assert!(out.contains("\nlive = true\n"), "not set: {out}");
        assert_eq!(out.matches("live = ").count(), 2, "{out}");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_word_tables_agree_with_what_a_config_file_accepts() {
        // `Choice` and serde each know these words. Drift would mean a flag
        // that works and a config key that does not.
        fn agrees<T: Choice + std::fmt::Debug + for<'de> Deserialize<'de>>() {
            for (word, value) in T::WORDS {
                #[derive(Deserialize)]
                struct One<T> {
                    v: T,
                }
                let parsed: One<T> = toml::from_str(&format!("v = {word:?}"))
                    .unwrap_or_else(|e| panic!("config rejects {word:?}: {e}"));
                assert_eq!(parsed.v, *value, "{word}");
                assert_eq!(value.word(), *word);
                assert_eq!(T::from_word(word), Some(*value));
            }
        }
        agrees::<Style>();
        agrees::<ArtColor>();
        agrees::<InfoStyle>();
    }

    #[test]
    fn choices_read_the_way_an_error_message_would_say_them() {
        assert_eq!(Style::list(), "half, quad, ramp, or raw");
        assert_eq!(ArtColor::list(), "own or theme");
        // The shape is what is pinned; which styles there are is not.
        let words: Vec<&str> = InfoStyle::WORDS.iter().map(|(w, _)| *w).collect();
        let (last, rest) = words.split_last().unwrap();
        assert_eq!(InfoStyle::list(), format!("{}, or {last}", rest.join(", ")));
        assert_eq!(Style::from_word("nonsense"), None);
    }

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
        assert!(!cfg.modules().is_empty());
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
    fn live_is_off_unless_asked_for() {
        // An update must not start re-reading the system under anyone.
        let cfg = Config::default();
        assert!(!cfg.live);
        assert_eq!(cfg.live_interval(), std::time::Duration::from_secs(2));
    }

    #[test]
    fn live_interval_is_clamped_both_ways() {
        let fast: Config = toml::from_str("live_interval = 0.0").unwrap();
        assert_eq!(fast.live_interval(), std::time::Duration::from_millis(500));

        let slow: Config = toml::from_str("live_interval = 9999.0").unwrap();
        assert_eq!(slow.live_interval(), std::time::Duration::from_secs(60));
    }

    #[test]
    fn only_rows_that_move_are_live() {
        assert!(Module::Memory.is_live());
        assert!(Module::Uptime.is_live());
        assert!(Module::Battery.is_live(), "charge moves while it is shown");
        assert!(Module::Temperature.is_live());
        assert!(!Module::Cpu.is_live());
        assert!(!Module::Os.is_live());
        assert!(!Module::Colors.is_live());
    }

    #[test]
    fn classic_is_the_default_and_keeps_its_rows() {
        // An update must not change what an existing fetch shows.
        let cfg = Config::default();
        assert_eq!(cfg.info_style, InfoStyle::Classic);
        assert_eq!(cfg.modules(), CLASSIC_MODULES);
        assert!(!cfg.modules().contains(&Module::Gpu));
    }

    #[test]
    fn a_style_brings_its_own_rows_unless_modules_is_set() {
        let boxed: Config = toml::from_str(r#"info_style = "boxed""#).unwrap();
        assert_eq!(boxed.modules(), BOXED_MODULES);

        // An explicit list always wins, whichever style is chosen.
        let own: Config =
            toml::from_str("info_style = \"boxed\"\nmodules = [\"os\", \"os_age\"]").unwrap();
        assert_eq!(own.modules(), [Module::Os, Module::OsAge]);
    }

    #[test]
    fn custom_rows_and_a_footer_are_read_from_the_config() {
        let cfg: Config = toml::from_str(
            "footer = \"hello\"\ncustom = [{ label = \"Owner\", value = \"me\" }]\nmodules = [\"os\", \"custom\"]",
        )
        .unwrap();

        assert_eq!(cfg.footer, "hello");
        assert_eq!(
            cfg.custom,
            [CustomRow {
                label: "Owner".into(),
                value: "me".into()
            }]
        );
        assert_eq!(cfg.modules(), [Module::Os, Module::Custom]);
    }

    #[test]
    fn a_custom_row_needs_both_halves_and_rejects_strays() {
        assert!(toml::from_str::<Config>("custom = [{ label = \"x\" }]").is_err());
        assert!(
            toml::from_str::<Config>("custom = [{ label = \"x\", value = \"y\", oops = 1 }]")
                .is_err()
        );
    }

    #[test]
    fn only_the_boxed_style_pays_for_detail() {
        assert!(InfoStyle::Boxed.wants_detail());
        assert!(InfoStyle::Tree.wants_detail());
        assert!(
            !InfoStyle::Chips.wants_detail()
                && !InfoStyle::Iconic.wants_detail()
                && !InfoStyle::Card.wants_detail(),
            "bare-value styles must not pay for versions"
        );
        assert!(!InfoStyle::Classic.wants_detail());
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
