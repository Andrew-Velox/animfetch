//! Command-line arguments.
//!
//! Everything here is a one-off override of what `config.toml` says, and
//! `--save` is how one becomes permanent.

use std::str::FromStr;

use crate::config::{ArtColor, BorderRender, Choice, ColorSource, Config, InfoStyle, Style};

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
    pub border_left: Option<String>,
    pub border_bottom: Option<String>,
    pub border_render: Option<BorderRender>,
    pub border_bottom_lift: Option<f32>,
    pub border_bottom_shift: Option<f32>,
    pub no_border: bool,
    pub no_art: bool,
    pub list_borders: bool,
    pub play: bool,
    pub pin: bool,
    pub unpin: bool,
    pub list: bool,
    pub save: bool,
    pub once: bool,
    pub no_color: bool,
    pub def_color: bool,
    /// The command line was `--pin` and nothing else, which is what a shell's
    /// startup file runs.
    pub bare_pin: bool,
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
        // Options, not the words that follow them.
        let mut options = 0usize;

        while let Some(arg) = argv.next() {
            options += 1;
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
                "--def-color" => args.def_color = true,
                "--palette" => args.palette = true,
                "--live" => args.live = true,
                "--no-border" => args.no_border = true,
                "--no-art" => args.no_art = true,
                "--list-borders" => args.list_borders = true,
                "--lb" | "--border-left" => {
                    args.border_left = Some(value(argv, &arg, "a name or a path")?);
                }
                "--db" | "--border-bottom" => {
                    args.border_bottom = Some(value(argv, &arg, "a name or a path")?);
                }
                "--border-bottom-lift" => {
                    args.border_bottom_lift =
                        Some(number(argv, "--border-bottom-lift", "a number of rows")?);
                }
                "--border-bottom-shift" => {
                    args.border_bottom_shift = Some(number(
                        argv,
                        "--border-bottom-shift",
                        "a number of columns",
                    )?);
                }
                "--border-render" => {
                    args.border_render = Some(choice(argv, "--border-render", "border mode")?);
                }
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

        args.bare_pin = args.pin && options == 1;
        Ok(Parsed::Run(args))
    }

    /// A `--save` that asks for nothing to be run: it keeps the settings and
    /// says so. Starting a session on top would clear the screen over what it
    /// had just said, and the run was for the config, not for a session.
    pub fn saves_only(&self) -> bool {
        self.save && !(self.pin || self.play || self.once || self.unpin)
    }

    /// What happens to a pin already on this terminal. A bare `--pin` is the
    /// startup file's, which is no reason to disturb it. A run that saves is
    /// writing for the terminals still to be opened: it leaves this one as it
    /// was, and says so. One with settings of its own that does not save is the
    /// run to look at, and takes the terminal over.
    pub fn existing_pin(&self) -> crate::pin::Existing {
        use crate::pin::Existing;

        if self.save {
            Existing::KeepAndSay
        } else if self.bare_pin {
            Existing::Keep
        } else {
            Existing::Replace
        }
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
        // Each of the two decides both settings, so a config left the other
        // way cannot turn `--no-color` into colour or `--def-color` into grey.
        // Grey is the plainer, so it wins when both are asked for.
        if self.def_color {
            cfg.color = true;
            cfg.gray = false;
        }
        if self.no_color {
            cfg.color = true;
            cfg.gray = true;
        }
        if self.palette {
            cfg.color_source = ColorSource::Palette;
        }
        if self.live {
            cfg.live = true;
        }
        if let Some(v) = &self.border_left {
            cfg.border_left = v.clone();
        }
        if let Some(v) = &self.border_bottom {
            cfg.border_bottom = v.clone();
        }
        if let Some(v) = self.border_render {
            cfg.border_render = v;
        }
        if let Some(v) = self.border_bottom_lift {
            cfg.border_bottom_lift = v;
        }
        if let Some(v) = self.border_bottom_shift {
            cfg.border_bottom_shift = v;
        }
        if self.no_art {
            cfg.art = false;
        }
        // Last, so it wins over a border named earlier on the same line.
        if self.no_border {
            cfg.border_left.clear();
            cfg.border_bottom.clear();
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
        if self.palette {
            out.push(("color_source", "\"palette\"".to_string()));
        }
        if let Some(v) = self.info_style {
            out.push(("info_style", format!("{:?}", v.word())));
        }
        // The plainer of the two, as `apply` takes it. Both keys, so that the
        // file never holds a pair that disagrees with the one just chosen.
        if self.no_color {
            out.push(("color", "true".to_string()));
            out.push(("gray", "true".to_string()));
        } else if self.def_color {
            out.push(("color", "true".to_string()));
            out.push(("gray", "false".to_string()));
        }
        if self.no_art {
            out.push(("art", "false".to_string()));
        }
        if self.no_border {
            // No border is two empty names, which is what the config defaults
            // to. It wins over a name on the same line, as it does in `apply`.
            out.push(("border_left", "\"\"".to_string()));
            out.push(("border_bottom", "\"\"".to_string()));
        } else {
            if let Some(v) = &self.border_left {
                out.push(("border_left", format!("{v:?}")));
            }
            if let Some(v) = &self.border_bottom {
                out.push(("border_bottom", format!("{v:?}")));
            }
        }
        if let Some(v) = self.border_render {
            out.push(("border_render", format!("{:?}", v.word())));
        }
        if let Some(v) = self.border_bottom_lift {
            out.push(("border_bottom_lift", v.to_string()));
        }
        if let Some(v) = self.border_bottom_shift {
            out.push(("border_bottom_shift", v.to_string()));
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
        if let Some(v) = self.seconds {
            out.push(("play_seconds", v.to_string()));
        }
        if self.live {
            out.push(("live", "true".to_string()));
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
                          coloured icon per row, no labels), card (one box,
                          labels left, values right) or minimal (an icon and a
                          value per row, nothing else)
      --save              Keep the settings given this run, for every run after.
                          On its own it only saves; with --pin, --play or
                          --once it saves and then runs
  -W, --width <COLS>      Cap the art width (0 fills the screen)
  -H, --height <ROWS>     Cap the art height (0 fills the screen)
  -s, --seconds <N>       How long --play animates
      --palette           Take colours from your desktop's generated palette
                          (matugen, pywal, wallust) instead of the config
      --live              Keep uptime, memory, swap and disk current while
                          pinned or interactive (interval: live_interval)
      --lb, --border-left <B>
                          An ornament down the left edge: the name of a built-in
                          or your own image (see --list-borders), or a PNG path
      --db, --border-bottom <B>
                          An ornament along the bottom; the image is upright and
                          is turned a quarter clockwise
      --border-render <M> auto (pictures where the terminal can show them,
                          else no border), image (force pictures), or cells
                          (redraw as braille text, which works anywhere)
      --border-bottom-lift <N>
                          Raise the bottom ornament by N rows (may be a
                          fraction, default 2.15), closer to the info
      --border-bottom-shift <N>
                          Move the bottom ornament N columns to the right
                          (may be a fraction, default 12)
      --no-border         Draw no border, whatever the config says; with --save
                          the config is left with none
      --no-art            Draw no animation: the info is the whole pane
      --list-borders      List the available border images
      --no-color          Draw everything in shades of grey, no colour at all
      --def-color         The default colours, from your theme: undoes --no-color
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
    fn only_a_line_of_nothing_but_pin_is_a_bare_pin() {
        assert!(run(&["--pin"]).bare_pin);
        assert!(!run(&["--pin", "--no-color"]).bare_pin);
        assert!(!run(&["--no-color", "--pin"]).bare_pin);
        assert!(!run(&["--pin", "-a", "tree"]).bare_pin);
        assert!(!run(&["--no-color"]).bare_pin);
        assert!(!run(&[]).bare_pin);
    }

    #[test]
    fn a_pin_already_there_is_kept_for_a_bare_pin_and_for_a_save() {
        use crate::pin::Existing;

        assert_eq!(run(&["--pin"]).existing_pin(), Existing::Keep);
        // A save says so, since that terminal looks as if nothing happened.
        assert_eq!(
            run(&["--pin", "--no-color", "--save"]).existing_pin(),
            Existing::KeepAndSay
        );
        assert_eq!(
            run(&["--save", "--pin", "--info", "card"]).existing_pin(),
            Existing::KeepAndSay
        );

        // Settings given to look at now take the terminal over.
        assert_eq!(
            run(&["--pin", "--no-color"]).existing_pin(),
            Existing::Replace
        );
        assert_eq!(
            run(&["--pin", "-a", "tree", "--live"]).existing_pin(),
            Existing::Replace
        );
    }

    #[test]
    fn each_colour_flag_wins_over_what_the_config_had_the_other_way() {
        // The states a config is left in by saving one and then the other, and
        // by `color = false`, which is no colour codes at all.
        for (color, gray) in [(false, false), (true, false)] {
            let mut cfg = Config {
                color,
                gray,
                ..Config::default()
            };
            run(&["--no-color"]).apply(&mut cfg);
            assert!(cfg.color && cfg.gray, "grey came out otherwise: {color} {gray}");
        }

        for (color, gray) in [(false, false), (true, true)] {
            let mut cfg = Config {
                color,
                gray,
                ..Config::default()
            };
            run(&["--def-color"]).apply(&mut cfg);
            assert!(cfg.color && !cfg.gray, "colour came out otherwise: {color} {gray}");
        }
    }

    #[test]
    fn no_color_is_grey_and_def_color_is_the_default_colours() {
        let mut grey = Config::default();
        run(&["--no-color"]).apply(&mut grey);
        // Still coloured on the wire, with no hue in it.
        assert!(grey.gray && grey.color);

        let mut def = Config::default();
        run(&["--def-color"]).apply(&mut def);
        assert!(def.color && !def.gray);

        // Grey is the plainer, so it wins, whichever order they come in.
        for args in [["--no-color", "--def-color"], ["--def-color", "--no-color"]] {
            let mut both = Config::default();
            run(&args).apply(&mut both);
            assert!(both.color && both.gray, "{args:?}");
        }
    }

    #[test]
    fn overrides_are_laid_over_the_config_and_only_those() {
        let mut cfg = Config::default();
        let before = (cfg.fps, cfg.width, cfg.style);

        run(&["--info", "tree", "--live", "--no-color"]).apply(&mut cfg);

        assert_eq!(cfg.info_style, InfoStyle::Tree);
        assert!(cfg.live && cfg.gray && cfg.color);
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
    fn the_short_border_flags_are_the_long_ones() {
        let short = run(&["--lb", "03", "--db", "~/x.png"]);
        let long = run(&["--border-left", "03", "--border-bottom", "~/x.png"]);
        assert_eq!(short.border_left, long.border_left);
        assert_eq!(short.border_bottom, long.border_bottom);
        assert_eq!(short.saved(), long.saved());

        // The error names the flag the way it was typed.
        assert_eq!(err(&["--lb"]), "--lb needs a name or a path");
        assert_eq!(err(&["--db"]), "--db needs a name or a path");
    }

    #[test]
    fn borders_are_named_laid_over_the_config_and_saved() {
        let a = run(&[
            "--border-left",
            "03",
            "--border-bottom",
            "~/x.png",
            "--border-render",
            "cells",
        ]);
        let mut cfg = Config::default();
        a.apply(&mut cfg);

        assert_eq!(cfg.border_left, "03");
        assert_eq!(cfg.border_bottom, "~/x.png");
        assert_eq!(cfg.border_render, BorderRender::Cells);
        assert_eq!(
            a.saved(),
            vec![
                ("border_left", "\"03\"".to_string()),
                ("border_bottom", "\"~/x.png\"".to_string()),
                ("border_render", "\"cells\"".to_string()),
            ]
        );
    }

    #[test]
    fn no_border_wins_over_a_border_named_on_the_same_line_or_in_the_config() {
        let mut cfg = Config {
            border_left: "01".into(),
            border_bottom: "02".into(),
            ..Config::default()
        };
        run(&["--no-border", "--border-left", "03"]).apply(&mut cfg);

        assert!(cfg.border_left.is_empty() && cfg.border_bottom.is_empty());
        // On its own it is for this run. With `--save` it is kept, as two
        // empty names: what the config holds when there is no border.
        let none = [
            ("border_left", "\"\"".to_string()),
            ("border_bottom", "\"\"".to_string()),
        ];
        assert_eq!(run(&["--no-border"]).saved(), none);
        // And it wins over a name given on the same line, as it does when run.
        assert_eq!(run(&["--border-left", "03", "--no-border"]).saved(), none);

        let text: String = none.iter().map(|(k, v)| format!("{k} = {v}\n")).collect();
        let cfg: Config = toml::from_str(&text).unwrap();
        assert!(cfg.border_left.is_empty() && cfg.border_bottom.is_empty());
    }

    #[test]
    fn no_art_turns_the_animation_off_and_can_be_saved() {
        let mut cfg = Config::default();
        run(&["--no-art"]).apply(&mut cfg);
        assert!(!cfg.art);

        assert_eq!(
            run(&["--no-art"]).saved(),
            vec![("art", "false".to_string())]
        );
        assert!(Config::default().art, "and it is on by default");
    }

    #[test]
    fn a_border_flag_needs_its_argument_and_a_known_mode() {
        assert_eq!(
            err(&["--border-left"]),
            "--border-left needs a name or a path"
        );
        assert_eq!(
            err(&["--border-bottom"]),
            "--border-bottom needs a name or a path"
        );
        assert_eq!(
            err(&["--border-render", "sixel"]),
            format!("unknown border mode: sixel ({})", BorderRender::list())
        );
        assert!(run(&["--list-borders"]).list_borders);
    }

    #[test]
    fn the_colour_choices_can_be_saved_and_read_back() {
        let grey = run(&["--no-color"]).saved();
        assert_eq!(
            grey,
            [("color", "true".to_string()), ("gray", "true".to_string())]
        );

        let def = run(&["--def-color"]).saved();
        assert_eq!(
            def,
            [("color", "true".to_string()), ("gray", "false".to_string())]
        );

        // Both is the plainer, as it is when the run is applied.
        let both = run(&["--no-color", "--def-color"]).saved();
        assert_eq!(both, grey);

        for saved in [grey, def] {
            let text: String = saved.iter().map(|(k, v)| format!("{k} = {v}\n")).collect();
            toml::from_str::<Config>(&text).unwrap();
        }
    }

    #[test]
    fn a_save_on_its_own_does_not_start_a_session() {
        assert!(run(&["--no-color", "--save"]).saves_only());
        assert!(run(&["--save"]).saves_only());

        // With a mode named, it saves and then does what was asked.
        assert!(!run(&["--pin", "--no-color", "--save"]).saves_only());
        assert!(!run(&["--play", "--save"]).saves_only());
        assert!(!run(&["--once", "--style", "quad", "--save"]).saves_only());
        // Nothing to save is nothing to hold back.
        assert!(!run(&["--no-color"]).saves_only());
    }

    #[test]
    fn every_setting_flag_can_be_saved_and_read_back() {
        let a = run(&["--live", "--palette", "-s", "1.5"]);
        let saved = a.saved();
        assert!(saved.contains(&("live", "true".to_string())), "{saved:?}");
        assert!(
            saved.contains(&("color_source", "\"palette\"".to_string())),
            "{saved:?}"
        );
        assert!(
            saved.contains(&("play_seconds", "1.5".to_string())),
            "{saved:?}"
        );

        let text: String = saved.iter().map(|(k, v)| format!("{k} = {v}\n")).collect();
        let cfg: Config = toml::from_str(&text).unwrap();
        assert!(cfg.live);
        assert_eq!(cfg.color_source, ColorSource::Palette);
        assert_eq!(cfg.play_seconds, 1.5);
    }

    #[test]
    fn saving_nothing_is_an_empty_list() {
        // The modes are what to run, not settings to keep.
        assert!(run(&["--save", "--pin"]).saved().is_empty());
        assert!(run(&["--save", "--play", "--once"]).saved().is_empty());
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
            "--def-color",
            "--animation",
            "--fps",
            "--width",
            "--height",
            "--seconds",
            "--style",
            "--art-color",
            "--info",
            "--border-left",
            "--lb",
            "--border-bottom",
            "--db",
            "--border-render",
            "--border-bottom-lift",
            "--border-bottom-shift",
            "--no-border",
            "--no-art",
            "--list-borders",
        ] {
            assert!(HELP.contains(flag), "{flag} is parsed but not documented");
        }
    }
}
