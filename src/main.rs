//! animfetch: an animated system fetch that stays usable while it animates.
//!
//! Two ideas carry it. Raw mode plus a non-blocking poll means each loop draws a
//! frame then waits for input only until the next one is due, so typing stays
//! instant. A scroll region pins the art, so everything you run scrolls below.
//!
//! - [`cli`], [`config`]: what was asked for, and what is configured
//! - [`fetch`]: the system's facts, collected into rows
//! - [`anim`], [`render`]: the art and the composed frame
//! - [`layout`]: fitting both to the terminal
//! - [`modes`], [`pin`], [`prompt`], [`term`]: running it

mod anim;
mod border;
mod cli;
mod color;
mod config;
mod fetch;
mod layout;
mod modes;
mod palette;
mod pin;
mod prompt;
mod render;
mod rows;
mod term;

use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anim::Animation;
use cli::{Args, Parsed};
use config::Config;
use layout::Fetch;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("animfetch: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> io::Result<ExitCode> {
    let args = match Args::parse() {
        Ok(Parsed::Run(args)) => args,
        Ok(Parsed::Help) => {
            print!("{}", cli::HELP);
            return Ok(ExitCode::SUCCESS);
        }
        Ok(Parsed::Version) => {
            println!("animfetch {}", env!("CARGO_PKG_VERSION"));
            return Ok(ExitCode::SUCCESS);
        }
        Err(e) => {
            eprintln!("animfetch: {e}\nTry 'animfetch --help'.");
            return Ok(ExitCode::FAILURE);
        }
    };

    let dir = config::config_dir();
    let (cfg, warning) = config::load(dir.as_deref());
    let result = dispatch(args, dir, cfg);

    // Every mode funnels through here. Reporting after the mode has finished is
    // what keeps a cleared screen or a first frame from wiping it, and routing
    // all of them through one exit is what stops `--pin`, `--play` and `--once`
    // from swallowing it entirely.
    if let Some(warning) = warning {
        eprintln!("animfetch: {warning}");
    }

    result
}

/// The config directory, or the error saying there is none.
fn require_dir(dir: Option<&Path>) -> io::Result<&Path> {
    dir.ok_or_else(|| io::Error::other("no config directory: set HOME or XDG_CONFIG_HOME"))
}

/// Run whichever mode the arguments selected.
fn dispatch(args: Args, dir: Option<PathBuf>, mut cfg: Config) -> io::Result<ExitCode> {
    args.apply(&mut cfg);

    // Before anything is drawn, so every mode gets the same colours.
    palette::apply(&mut cfg);

    let to_terminal = io::stdout().is_terminal();
    let interactive = io::stdin().is_terminal() && to_terminal;
    // Meaningless when piped, and NO_COLOR is how you opt out.
    if !to_terminal || std::env::var_os("NO_COLOR").is_some() {
        cfg.color = false;
    }

    // Persist the overrides this run carried. Done after `apply`, so anything
    // the parser rejected never reaches the file, and before the modes so the
    // saved settings are the ones you also see this time.
    //
    // What it says is held back under `--pin`, whose setup clears the screen
    // and would take the line with it; it is said below the pane instead.
    let mut said = None;
    let mut saved_nothing = false;
    if args.save {
        let dir = require_dir(dir.as_deref())?;
        let saved = args.saved();
        for (key, value) in &saved {
            config::set_key(dir, key, value)?;
        }

        if saved.is_empty() {
            saved_nothing = true;
            eprintln!("animfetch: --save needs a setting to save, such as --style raw");
        } else {
            let shown: Vec<String> = saved.iter().map(|(k, v)| format!("{k} = {v}")).collect();
            let line = format!(
                "saved to {}: {}",
                dir.join("config.toml").display(),
                shown.join(", ")
            );
            if args.pin {
                said = Some(line);
            } else {
                println!("{line}");
            }
        }
    }

    let anim_dir = dir.as_ref().map(|d| d.join("anim"));

    if args.list_borders {
        list_borders(dir.as_deref(), &cfg);
        return Ok(ExitCode::SUCCESS);
    }

    if args.list {
        list_animations(anim_dir.as_deref(), &cfg.animation);
        return Ok(ExitCode::SUCCESS);
    }

    if let Some(name) = &args.set {
        // A config that breaks every future run is worse than an error now.
        Animation::load(anim_dir.as_deref(), name)?;

        let path = config::set_key(
            require_dir(dir.as_deref())?,
            "animation",
            &format!("{name:?}"),
        )?;
        println!("default animation set to {name:?} in {}", path.display());
        return Ok(ExitCode::SUCCESS);
    }

    let mut animation = Animation::load(anim_dir.as_deref(), &cfg.animation)?;
    if cfg.color && cfg.gray {
        animation.grayscale();
    }

    // A border that cannot be read is an error now, like an unknown animation,
    // rather than a pane that quietly has no ornament.
    let borders = border::Borders::load(&cfg, dir.as_deref(), interactive, to_terminal)
        .map_err(io::Error::other)?;

    // Said, not run: a session started here would clear the screen over it.
    // Past the animation and border checks above, so a name that is wrong is
    // still reported, as it always was.
    if args.saves_only() {
        return Ok(if saved_nothing {
            ExitCode::from(2)
        } else {
            ExitCode::SUCCESS
        });
    }

    let title = fetch::title();
    let items = fetch::gather(&cfg);
    let fetch = Fetch {
        animation: &animation,
        cfg: &cfg,
        title: &title,
        items: &items,
        borders: borders.as_ref(),
    };

    if !interactive || args.once {
        if let Some(line) = said.take() {
            println!("{line}");
        }
        return modes::draw_once(&fetch).map(|()| ExitCode::SUCCESS);
    }

    if args.unpin {
        if !pin::stop()? {
            eprintln!("animfetch: nothing pinned to this terminal");
        }
        return Ok(ExitCode::SUCCESS);
    }

    if args.pin {
        let existing = args.existing_pin();
        let started = pin::start(&fetch, existing)?;

        // Only the process the shell ran; the one drawing has nothing to add.
        if started != pin::Started::Finished {
            if let Some(line) = said.take() {
                println!("{line}");
            }
            if started == pin::Started::Kept && existing == pin::Existing::KeepAndSay {
                println!("{}", pin::KEPT_NOTE);
            }
        }
        return Ok(ExitCode::SUCCESS);
    }

    if args.play {
        return modes::play(&fetch).map(|()| ExitCode::SUCCESS);
    }

    let status = modes::shell_loop(&fetch)?;
    Ok(ExitCode::from(status))
}

/// Print every available border image, marking the ones in effect.
///
/// One list, since any image goes on either edge: `L` marks the one in use on
/// the left and `B` the one along the bottom.
fn list_borders(config: Option<&Path>, cfg: &Config) {
    println!("{}\n", border::report());

    // The two edges share everything but a folder of your own for one of them.
    let mut entries = border::list(border::Slot::Left, config);
    for entry in border::list(border::Slot::Bottom, config) {
        if !entries.iter().any(|e| e.name == entry.name) {
            entries.push(entry);
        }
    }

    println!("borders (any can go on either edge; the bottom one is turned a quarter):");
    let width = entries.iter().map(|e| e.name.len()).max().unwrap_or(0);
    for entry in &entries {
        let left = if entry.name == cfg.border_left { 'L' } else { ' ' };
        let bottom = if entry.name == cfg.border_bottom { 'B' } else { ' ' };
        let source = match &entry.source {
            border::Source::Bundled => "built in".to_string(),
            border::Source::File(path) => path.display().to_string(),
        };
        println!("{left}{bottom} {:width$}  {source}", entry.name);
    }
    if entries.is_empty() {
        println!("  none");
    }

    if let Some(dir) = config {
        println!(
            "\nyour own go in {}/border/, and are for either edge; one in \
             border/left_border or border/bottom_border is for that edge only",
            dir.display()
        );
    }
}

/// Print every available animation, marking the one currently in effect.
fn list_animations(dir: Option<&Path>, current: &str) {
    let entries = anim::list(dir);
    let width = entries.iter().map(|e| e.name.len()).max().unwrap_or(0);

    for entry in &entries {
        let mark = if entry.name == current { "*" } else { " " };
        let source = match &entry.path {
            Some(path) => path.display().to_string(),
            None => "built in".to_string(),
        };
        println!(
            "{mark} {:width$}  {:>2} frames  {source}",
            entry.name, entry.frames
        );
    }

    if entries.is_empty() {
        println!("no animations found");
    }
}
