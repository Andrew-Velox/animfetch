//! Command-line arguments.
//!
//! Everything here is a one-off override of what `config.toml` says, and
//! `--save` is how one becomes permanent.

use std::str::FromStr;

use crate::config::{ArtColor, Choice, ColorSource, Config, InfoStyle, Style};

/// What was asked for. Every setting is optional: unset means the config's.
#[derive(Default)]
pub struct Args {
    pub animation: Option<String>,
    pub fps: Option<f32>,
    pub style: Option<Style>,
    pub art_color: Option<ArtColor>,
    pub info_style: Option<InfoStyle>,
    pub width: Option<usize>,
    pub height: Option<usize>,
    pub seconds: Option<f32>,
    pub set: Option<String>,
    pub play: bool,
    pub pin: bool,
    pub unpin: bool,
    pub list: bool,
    pub save: bool,
    pub once: bool,
    pub no_color: bool,
    pub palette: bool,
    pub live: bool,
}

/// What the command line comes to.
pub enum Parsed {
    Run(Args),
    Help,
    Version,
}

type Argv<'a> = &'a mut dyn Iterator<Item = String>;

/// The word after `flag`, or why there is not one.
fn value(argv: Argv<'_>, flag: &str, needs: &str) -> Result<String, String> {
    argv.next().ok_or_else(|| format!("{flag} needs {needs}"))
}

fn number<T: FromStr>(argv: Argv<'_>, flag: &str, needs: &str) -> Result<T, String> {
    let raw = value(argv, flag, needs)?;
    raw.parse().map_err(|_| format!("not a number: {raw}"))
}

fn choice<T: Choice>(argv: Argv<'_>, flag: &str, what: &str) -> Result<T, String> {
    let raw = value(argv, flag, &T::list())?;
    T::from_word(&raw).ok_or_else(|| format!("unknown {what}: {raw} ({})", T::list()))
}

impl Args {
    pub fn parse() -> Result<Parsed, String> {
        Self::parse_from(std::env::args().skip(1))
    }

    pub fn parse_from(argv: impl IntoIterator<Item = String>) -> Result<Parsed, String> {
        let mut args = Args::default();
        let mut argv = argv.into_iter();

        while let Some(arg) = argv.next() {
            let argv: Argv<'_> = &mut argv;
            match arg.as_str() {
                "-h" | "--help" => return Ok(Parsed::Help),
                "-V" | "--version" => return Ok(Parsed::Version),
                "-1" | "--once" => args.once = true,
                "-p" | "--play" => args.play = true,
                "--pin" => args.pin = true,
                "--unpin" => args.unpin = true,
                "-l" | "--list" => args.list = true,
                "--save" => args.save = true,
                "--no-color" => args.no_color = true,
                "--palette" => args.palette = true,
                "--live" => args.live = true,
                "--set" => args.set = Some(value(argv, "--set", "an animation name")?),
                "-a" | "--animation" => {
                    args.animation = Some(value(argv, "--animation", "a name")?);
                }
                "-f" | "--fps" => args.fps = Some(number(argv, "--fps", "a number")?),
                "-W" | "--width" => {
                    args.width = Some(number(argv, "--width", "a number of columns")?);
                }
                "-H" | "--height" => {
                    args.height = Some(number(argv, "--height", "a number of rows")?);
                }
                "-s" | "--seconds" => {
                    args.seconds = Some(number(argv, "--seconds", "a number")?);
                }
                "--style" => args.style = Some(choice(argv, "--style", "style")?),
                "--art-color" => {
                    args.art_color = Some(choice(argv, "--art-color", "art colour")?);
                }
                "--info" => args.info_style = Some(choice(argv, "--info", "info style")?),
                other => return Err(format!("unknown option: {other}")),
            }
        }

        Ok(Parsed::Run(args))
    }

    /// Lay these overrides over the config.
    pub fn apply(&self, cfg: &mut Config) {
        if let Some(name) = &self.animation {
            cfg.animation = name.clone();
        }
        if let Some(v) = self.fps {
            cfg.fps = v;
        }
        if let Some(v) = self.style {
            cfg.style = v;
        }
        if let Some(v) = self.art_color {
            cfg.art_color = v;
        }
        if let Some(v) = self.info_style {
            cfg.info_style = v;
        }
        if let Some(v) = self.width {
            cfg.width = v;
        }
        if let Some(v) = self.height {
            cfg.height = v;
        }
        if let Some(v) = self.seconds {
            cfg.play_seconds = v;
        }
        if self.no_color {
            cfg.color = false;
        }
        if self.palette {
            cfg.color_source = ColorSource::Palette;
        }
        if self.live {
            cfg.live = true;
        }
    }

    /// What `--save` writes: each setting this run carried, as the config key
    /// and its TOML value, in the order the config file lists them.
    pub fn saved(&self) -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        if let Some(v) = &self.animation {
            out.push(("animation", format!("{v:?}")));
        }
        if let Some(v) = self.style {
            out.push(("style", format!("{:?}", v.word())));
        }
        if let Some(v) = self.art_color {
            out.push(("art_color", format!("{:?}", v.word())));
        }
        if let Some(v) = self.info_style {
            out.push(("info_style", format!("{:?}", v.word())));
        }
        if let Some(v) = self.fps {
            out.push(("fps", v.to_string()));
        }
        if let Some(v) = self.width {
            out.push(("width", v.to_string()));
        }
        if let Some(v) = self.height {
            out.push(("height", v.to_string()));
        }
        out
    }
}

pub const HELP: &str = "\
animfetch: animated system fetch with a live prompt

Usage: animfetch [OPTIONS]

Modes:
      --pin               Pin the fetch above your own shell and keep animating
                          in the background. Undo with --unpin.
  -p, --play              Animate in place for a few seconds, then exit
  -1, --once              Print one static frame and exit
  (default)               Interactive: fetch pinned above, animfetch's own
                          prompt below

Animations:
  -a, --animation <NAME>  Use NAME for this run only
  -l, --list              List available animations (* = current)
      --set <NAME>        Make NAME the default, saved to config.toml

Options:
  -f, --fps <N>           Frames per second
      --style <STYLE>     half (default), quad (finest detail), ramp (ASCII), raw
      --art-color <SRC>   Where raw art's colour comes from: own (the colours
                          in the frames, default) or theme (the gradient)
      --info <STYLE>      How the info pane looks: classic (default), boxed
                          (icons in a box), tree (user box over OS and PC
                          groups), sections (titled Hardware / Software /
                          Uptime blocks), chips (filled pills), iconic (a
                          coloured icon per row, no labels) or card (one box,
                          labels left, values right)
      --save              Keep the settings given this run, for every run after
  -W, --width <COLS>      Cap the art width (0 fills the screen)
  -H, --height <ROWS>     Cap the art height (0 fills the screen)
  -s, --seconds <N>       How long --play animates
      --palette           Take colours from your desktop's generated palette
                          (matugen, pywal, wallust) instead of the config
      --live              Keep uptime, memory, swap and disk current while
                          pinned or interactive (interval: live_interval)
      --no-color          Disable colour output
  -h, --help              Show this help
  -V, --version           Show the version

For a shell startup file, use --play (or --once). The default mode waits for
input, which is not what you want on every new terminal.

In the default mode the fetch stays pinned at the top of the screen, and your
prompt and everything you run scroll in the region below it.

While running:
  <text><Enter>  Run the command, output appears below the fetch
  Ctrl-C         Interrupt a running command; at an idle prompt, quit
  Esc, Ctrl-D    Quit
  Ctrl-U / -W    Clear the line / delete the last word

  cd and exit are handled internally; everything else goes to $SHELL -c.

Config: ~/.config/animfetch/config.toml
Art:    ~/.config/animfetch/anim/<name>/*.txt
";

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Parsed, String> {
        Args::parse_from(args.iter().map(|s| s.to_string()))
    }

    fn run(args: &[&str]) -> Args {
        match parse(args) {
            Ok(Parsed::Run(a)) => a,
            _ => panic!("expected a run for {args:?}"),
        }
    }

    fn err(args: &[&str]) -> String {
        match parse(args) {
            Err(e) => e,
            _ => panic!("expected an error for {args:?}"),
        }
    }

    #[test]
    fn help_and_version_are_answers_not_side_effects() {
        assert!(matches!(parse(&["--help"]), Ok(Parsed::Help)));
        assert!(matches!(parse(&["-h"]), Ok(Parsed::Help)));
        assert!(matches!(parse(&["-V"]), Ok(Parsed::Version)));
    }

    #[test]
    fn flags_and_their_short_forms_agree() {
        let a = run(&[
            "-1", "-p", "-l", "-a", "tree", "-f", "20", "-W", "40", "-H", "10",
        ]);
        assert!(a.once && a.play && a.list);
        assert_eq!(a.animation.as_deref(), Some("tree"));
        assert_eq!(a.fps, Some(20.0));
        assert_eq!((a.width, a.height), (Some(40), Some(10)));
    }

    #[test]
    fn a_choice_flag_takes_any_word_its_table_lists() {
        let a = run(&[
            "--style",
            "quad",
            "--art-color",
            "theme",
            "--info",
            "sections",
        ]);
        assert_eq!(a.style, Some(Style::Quad));
        assert_eq!(a.art_color, Some(ArtColor::Theme));
        assert_eq!(a.info_style, Some(InfoStyle::Sections));
    }

    #[test]
    fn errors_name_the_flag_and_what_it_wanted() {
        assert_eq!(err(&["--style"]), "--style needs half, quad, ramp, or raw");
        assert_eq!(err(&["--art-color"]), "--art-color needs own or theme");
        assert_eq!(err(&["--width"]), "--width needs a number of columns");
        assert_eq!(err(&["--set"]), "--set needs an animation name");
        assert_eq!(
            err(&["--info", "nonsense"]),
            // The list is the table's own, so adding a style does not break this.
            format!("unknown info style: nonsense ({})", InfoStyle::list())
        );
        assert_eq!(err(&["--fps", "fast"]), "not a number: fast");
        assert_eq!(run(&["--info", "chips"]).info_style, Some(InfoStyle::Chips));
        assert_eq!(err(&["--nope"]), "unknown option: --nope");
    }

    #[test]
    fn overrides_are_laid_over_the_config_and_only_those() {
        let mut cfg = Config::default();
        let before = (cfg.fps, cfg.width, cfg.style);

        run(&["--info", "tree", "--live", "--no-color"]).apply(&mut cfg);

        assert_eq!(cfg.info_style, InfoStyle::Tree);
        assert!(cfg.live && !cfg.color);
        assert_eq!(
            (cfg.fps, cfg.width, cfg.style),
            before,
            "untouched keys move"
        );
    }

    #[test]
    fn save_writes_what_the_run_carried_in_the_configs_order() {
        let a = run(&[
            "--height", "9", "--info", "boxed", "-f", "8", "--style", "raw", "-a", "fox-run",
        ]);
        assert_eq!(
            a.saved(),
            vec![
                ("animation", "\"fox-run\"".to_string()),
                ("style", "\"raw\"".to_string()),
                ("info_style", "\"boxed\"".to_string()),
                ("fps", "8".to_string()),
                ("height", "9".to_string()),
            ]
        );
    }

    #[test]
    fn saving_nothing_is_an_empty_list() {
        assert!(run(&["--save", "--pin", "--live"]).saved().is_empty());
    }

    #[test]
    fn every_saved_value_is_one_the_config_reads_back() {
        // A value `--save` writes that the config then rejects would break
        // every later run, which is what writing it is meant to avoid.
        let a = run(&[
            "--style",
            "ramp",
            "--art-color",
            "theme",
            "--info",
            "tree",
            "-f",
            "7.5",
        ]);
        let text: String = a
            .saved()
            .iter()
            .map(|(k, v)| format!("{k} = {v}\n"))
            .collect();
        let cfg: Config = toml::from_str(&text).unwrap();

        assert_eq!(cfg.style, Style::Ramp);
        assert_eq!(cfg.info_style, InfoStyle::Tree);
        assert_eq!(cfg.fps, 7.5);
    }

    #[test]
    fn the_help_text_mentions_every_flag_the_parser_accepts() {
        for flag in [
            "--once",
            "--play",
            "--pin",
            "--unpin",
            "--list",
            "--set",
            "--save",
            "--live",
            "--palette",
            "--no-color",
            "--animation",
            "--fps",
            "--width",
            "--height",
            "--seconds",
            "--style",
            "--art-color",
            "--info",
        ] {
            assert!(HELP.contains(flag), "{flag} is parsed but not documented");
        }
    }
}
