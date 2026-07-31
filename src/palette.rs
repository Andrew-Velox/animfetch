//! Dynamic colours from whatever themes your desktop (matugen, pywal, wallust).
//! They all write `name: colour` pairs somewhere; reading that is why changing
//! your wallpaper recolours the animation. Nothing here is ever fatal.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::color::{Color, Gradient, Rgb, parse_hex};
use crate::config::{ColorSource, Config};

/// The `name -> colour` pairs found in a palette file.
pub struct Palette {
    entries: Vec<(String, Rgb)>,
}

impl Palette {
    pub fn parse(text: &str) -> Self {
        Self { entries: scan(text) }
    }

    pub fn get(&self, name: &str) -> Option<Rgb> {
        self.entries.iter().find(|(k, _)| k == name).map(|&(_, c)| c)
    }
}

/// Every `"name": "#rrggbb"` pair in `text`, at any depth. A scanner, not a JSON
/// parser: works on flat files (matugen) and nested ones (pywal) alike.
fn scan(text: &str) -> Vec<(String, Rgb)> {
    let mut out = Vec::new();
    let mut chars = text.chars();
    let mut key: Option<String> = None;
    let mut after_colon = false;

    while let Some(c) = chars.next() {
        match c {
            ':' => after_colon = true,
            // Ends the current pair, so array strings aren't read as keys.
            ',' | '{' | '}' | '[' | ']' => {
                key = None;
                after_colon = false;
            }
            '"' => {
                let text = read_string(&mut chars);
                match (after_colon, key.take()) {
                    (true, Some(name)) => {
                        if let Some(rgb) = parse_hex(&text) {
                            out.push((name, rgb));
                        }
                        after_colon = false;
                    }
                    _ => {
                        key = Some(text);
                        after_colon = false;
                    }
                }
            }
            _ => {}
        }
    }

    out
}

/// Consume up to and including the closing quote, honouring backslash escapes.
fn read_string(chars: &mut std::str::Chars<'_>) -> String {
    let mut out = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => break,
            // Just enough to stop a quote ending the string early.
            '\\' => out.extend(chars.next()),
            _ => out.push(c),
        }
    }
    out
}

/// Expand a leading `~/`, which is how these paths are written in a config.
fn expand(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => match std::env::var_os("HOME").filter(|h| !h.is_empty()) {
            Some(home) => Path::new(&home).join(rest),
            None => PathBuf::from(path),
        },
        None => PathBuf::from(path),
    }
}

/// The first of `paths` that exists. Order is the user's preference order.
fn find(paths: &[String]) -> Option<PathBuf> {
    paths.iter().map(|p| expand(p)).find(|p| p.is_file())
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

/// Resolve dynamic colours into `cfg`. Roles the file omits keep their
/// configured value, so this falls back per colour rather than all or nothing.
pub fn apply(cfg: &mut Config) -> Option<PathBuf> {
    if cfg.color_source != ColorSource::Palette {
        return None;
    }

    let path = find(&cfg.palette_files)?;
    let palette = Palette::parse(&std::fs::read_to_string(&path).ok()?);

    if let Some(rgb) = palette.get(&cfg.palette_accent) {
        cfg.accent = Color(rgb);
    }
    if let Some(rgb) = palette.get(&cfg.palette_value) {
        cfg.value = Color(rgb);
    }

    let stops: Vec<Rgb> = cfg.palette_gradient.iter().filter_map(|n| palette.get(n)).collect();
    if !stops.is_empty() {
        cfg.gradient = Gradient::new(stops);
    }

    Some(path)
}

/// Notices when the palette file is rewritten, so `--pin` can recolour live.
pub struct Watch {
    path: Option<PathBuf>,
    seen: Option<SystemTime>,
}

impl Watch {
    pub fn new(cfg: &Config) -> Self {
        let path = match cfg.color_source {
            ColorSource::Palette => find(&cfg.palette_files),
            ColorSource::Config => None,
        };
        let seen = path.as_deref().and_then(mtime);
        Self { path, seen }
    }

    /// True once per rewrite. A file absent at startup is never picked up.
    pub fn changed(&mut self) -> bool {
        let Some(path) = self.path.as_deref() else {
            return false;
        };

        let now = mtime(path);
        if now == self.seen {
            return false;
        }
        self.seen = now;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_flat_material_you_file() {
        let text = r##"{
          "background": "#0e141c",
          "primary": "#4fd8eb",
          "on_surface": "#dee2ef"
        }"##;
    let p = Palette::parse(text);
        assert_eq!(p.get("primary"), Some(Rgb(0x4f, 0xd8, 0xeb)));
        assert_eq!(p.get("on_surface"), Some(Rgb(0xde, 0xe2, 0xef)));
        assert_eq!(p.get("nope"), None);
    }

    #[test]
    fn reads_colours_nested_under_a_parent_key() {
        // pywal's shape: the useful names are one level down.
        let text = r##"{
          "special": { "background": "#111111" },
          "colors": { "color0": "#000000", "color1": "#ff0000" }
        }"##;
    let p = Palette::parse(text);
        assert_eq!(p.get("color1"), Some(Rgb(255, 0, 0)));
        assert_eq!(p.get("background"), Some(Rgb(0x11, 0x11, 0x11)));
        // The parent keys are not colours and must not appear as entries.
        assert_eq!(p.get("colors"), None);
    }

    #[test]
    fn non_colour_values_are_skipped_not_mispaired() {
        // A string value that is not a colour must not shift the pairing and
        // turn the following key into a value.
        let text = r##"{ "name": "Tokyo Night", "primary": "#4fd8eb" }"##;
    let p = Palette::parse(text);
        assert_eq!(p.get("name"), None);
        assert_eq!(p.get("primary"), Some(Rgb(0x4f, 0xd8, 0xeb)));
    }

    #[test]
    fn strings_in_an_array_are_not_read_as_names() {
        let text = r##"{ "ramp": ["#ff0000", "#00ff00"], "primary": "#0000ff" }"##;
    let p = Palette::parse(text);
        assert_eq!(p.get("ramp"), None);
        assert_eq!(p.get("primary"), Some(Rgb(0, 0, 255)));
    }

    #[test]
    fn an_escaped_quote_does_not_end_the_string_early() {
        let text = r##"{ "a\"b": "#010203", "primary": "#4fd8eb" }"##;
    let p = Palette::parse(text);
        assert_eq!(p.get("a\"b"), Some(Rgb(1, 2, 3)));
        assert_eq!(p.get("primary"), Some(Rgb(0x4f, 0xd8, 0xeb)));
    }

    #[test]
    fn junk_yields_nothing_rather_than_panicking() {
        for text in ["", "{", "\"", "not json at all", "{\"a\":", "{\"a\": \"#zz\"}"] {
            assert!(Palette::parse(text).get("a").is_none(), "{text:?}");
        }
    }

    #[test]
    fn apply_is_a_no_op_unless_the_source_is_palette() {
        let mut cfg = Config::default();
        let before = cfg.accent.0;
        assert_eq!(cfg.color_source, ColorSource::Config, "default must not change behaviour");
        assert!(apply(&mut cfg).is_none());
        assert_eq!(cfg.accent.0, before);
    }

    #[test]
    fn a_missing_palette_file_leaves_the_configured_colours_alone() {
        let mut cfg = Config {
            color_source: ColorSource::Palette,
            palette_files: vec!["/nonexistent/animfetch-test/colors.json".into()],
            ..Config::default()
        };
        let before = cfg.accent.0;
        assert!(apply(&mut cfg).is_none());
        assert_eq!(cfg.accent.0, before);
    }

    #[test]
    fn roles_the_file_omits_keep_their_configured_colour() {
        let text = r##"{ "primary": "#4fd8eb" }"##;
    let p = Palette::parse(text);
        // `apply` only assigns when the lookup succeeds; this mirrors that.
        assert!(p.get("primary").is_some());
        assert!(p.get("on_surface").is_none());
    }

    /// A config pointing at one palette file, with the source turned on.
    fn watching(path: &Path) -> Config {
        Config {
            color_source: ColorSource::Palette,
            palette_files: vec![path.display().to_string()],
            ..Config::default()
        }
    }

    #[test]
    fn a_rewritten_palette_is_noticed_once() {
        let path = std::env::temp_dir().join(format!("animfetch-watch-{}.json", std::process::id()));
        std::fs::write(&path, r##"{"primary": "#4fd8eb"}"##).unwrap();

        let mut watch = Watch::new(&watching(&path));
        assert!(!watch.changed(), "nothing has happened yet");

        // Set mtime explicitly; sleeping is flaky on coarse filesystems.
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(SystemTime::now() + std::time::Duration::from_secs(5)).unwrap();

        assert!(watch.changed(), "a rewrite must be noticed");
        assert!(!watch.changed(), "and only once, not every poll after");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn nothing_is_watched_when_the_source_is_the_config() {
        let path = std::env::temp_dir().join(format!("animfetch-nowatch-{}.json", std::process::id()));
        std::fs::write(&path, r##"{"primary": "#4fd8eb"}"##).unwrap();

        let cfg = Config { color_source: ColorSource::Config, ..watching(&path) };
        assert!(!Watch::new(&cfg).changed());

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn reload_picks_up_the_new_colours() {
        let path = std::env::temp_dir().join(format!("animfetch-reload-{}.json", std::process::id()));
        std::fs::write(&path, r##"{"primary": "#ff0000", "on_surface": "#00ff00"}"##).unwrap();

        let mut cfg = watching(&path);
        assert!(apply(&mut cfg).is_some());
        assert_eq!(cfg.accent.0, Rgb(255, 0, 0));
        assert_eq!(cfg.value.0, Rgb(0, 255, 0));

        // What a retheme looks like from here: same path, new contents.
        std::fs::write(&path, r##"{"primary": "#0000ff", "on_surface": "#ffffff"}"##).unwrap();
        assert!(apply(&mut cfg).is_some());
        assert_eq!(cfg.accent.0, Rgb(0, 0, 255));
        assert_eq!(cfg.value.0, Rgb(255, 255, 255));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn tilde_expands_against_home() {
        // Reads HOME rather than setting it: set_var is unsound with threads.
        if let Some(home) = std::env::var_os("HOME").filter(|h| !h.is_empty()) {
            assert_eq!(expand("~/x/y"), Path::new(&home).join("x/y"));
        }
        assert_eq!(expand("/abs/path"), Path::new("/abs/path"));
    }
}
