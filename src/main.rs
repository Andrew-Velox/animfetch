//! animfetch — an animated system fetch that stays usable while it animates.
//!
//! Two ideas carry the design.
//!
//! Raw mode plus a non-blocking event poll means the animation and the prompt
//! are not competing. Each pass through the loop draws a frame, then waits for
//! input *only until the next frame is due*. Keystrokes are handled the instant
//! they arrive, and the frame clock is absolute rather than accumulated, so
//! rendering cost does not make the animation drift slow.
//!
//! A scroll region pins the fetch. The terminal is told never to scroll the top
//! rows, so the prompt and everything you run live in the region below and
//! scroll freely there, while the art above stays put.

mod anim;
mod color;
mod config;
mod fetch;
mod prompt;
mod render;
mod term;

use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEventKind};

use anim::Animation;
use config::Config;
use fetch::Item;
use prompt::{Action, Prompt};
use render::Pane;

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
        Ok(Some(args)) => args,
        // --help / --version printed their output already.
        Ok(None) => return Ok(ExitCode::SUCCESS),
        Err(e) => {
            eprintln!("animfetch: {e}\nTry 'animfetch --help'.");
            return Ok(ExitCode::FAILURE);
        }
    };

    let dir = config::config_dir();
    let (mut cfg, warning) = config::load(dir.as_deref());
    args.apply(&mut cfg);

    let interactive = io::stdin().is_terminal() && io::stdout().is_terminal();
    // Colour is meaningless once the output is piped into a file, and NO_COLOR
    // is the conventional way for a user to say they never want it.
    if !io::stdout().is_terminal() || std::env::var_os("NO_COLOR").is_some() {
        cfg.color = false;
    }

    let anim_dir = dir.as_ref().map(|d| d.join("anim"));

    if args.list {
        list_animations(anim_dir.as_deref(), &cfg.animation);
        return Ok(ExitCode::SUCCESS);
    }

    if let Some(name) = &args.set {
        // Refuse to persist a name that will not load; a config that breaks
        // every future run is worse than a rejected command.
        Animation::load(anim_dir.as_deref(), name)?;

        let Some(dir) = dir.as_deref() else {
            return Err(io::Error::other("no config directory: set HOME or XDG_CONFIG_HOME"));
        };
        let path = set_animation(dir, name)?;
        println!("default animation set to {name:?} in {}", path.display());
        return Ok(ExitCode::SUCCESS);
    }

    let animation = Animation::load(anim_dir.as_deref(), &cfg.animation)?;

    let title = fetch::title();
    let items = fetch::collect(&cfg.modules);

    if !interactive || args.once {
        return draw_once(&animation, &cfg, &title, &items).map(|()| ExitCode::SUCCESS);
    }

    if args.play {
        return play(&animation, &cfg, &title, &items).map(|()| ExitCode::SUCCESS);
    }

    let status = shell_loop(&animation, &cfg, &title, &items)?;

    // Warnings are held back until the terminal is readable again; printing one
    // earlier would have been wiped by the first frame.
    if let Some(warning) = warning {
        eprintln!("animfetch: {warning}");
    }

    Ok(ExitCode::from(status))
}

/// Print every available animation, marking the one currently in effect.
fn list_animations(dir: Option<&std::path::Path>, current: &str) {
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

/// Write `animation = "<name>"` into the config, creating it if needed.
///
/// This edits the file as text rather than reserialising the parsed config,
/// which would discard every comment and every key left at its default. Only
/// the `animation` line is rewritten — including any trailing comment on it,
/// which is the one thing this does lose.
fn set_animation(dir: &std::path::Path, name: &str) -> io::Result<std::path::PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join("config.toml");

    let existing = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };

    let setting = format!("animation = {name:?}");
    let mut replaced = false;
    let mut out = String::with_capacity(existing.len() + setting.len() + 1);

    for line in existing.lines() {
        if !replaced && line.trim_start().starts_with("animation") && line.contains('=') {
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

/// Erase from the cursor to the end of the line.
const CLEAR_LINE: &str = "\x1b[K";

/// Pre-scaled frames for one terminal size.
struct Layout {
    frames: Vec<Vec<String>>,
    art_w: usize,
}

impl Layout {
    /// Below this many columns there is no width worth splitting, so the art is
    /// dropped and the info pane gets the whole terminal.
    const MIN_SPLIT_WIDTH: usize = 40;

    /// Share of the terminal the info pane may claim before its values start
    /// being truncated instead of stealing more room from the art.
    const INFO_WIDTH_SHARE: usize = 45;

    /// Fit the art into whatever space the info pane does not need.
    ///
    /// `max_h` is the caller's row budget for the whole pane, not the terminal
    /// height — the interactive session keeps most of the screen for output.
    fn build(animation: &Animation, cfg: &Config, title: &str, items: &[Item], w: usize, max_h: usize) -> Self {
        if w < Self::MIN_SPLIT_WIDTH {
            // One empty frame rather than none, so the loop's `phase % len`
            // indexing stays valid without a special case.
            return Self { frames: vec![Vec::new()], art_w: 0 };
        }

        let widest = items
            .iter()
            .map(|i| {
                if i.label.is_empty() {
                    render::visible_width(&i.value)
                } else {
                    render::visible_width(i.label) + 2 + render::visible_width(&i.value)
                }
            })
            .chain(std::iter::once(render::visible_width(title)))
            .max()
            .unwrap_or(0);

        // A single long value — a CPU model is usually the culprit — would
        // otherwise squeeze the art down to an unrecognisable blob. Cap what
        // the info pane can claim and let `compose` ellipsise the overflow.
        let info_w = widest.min(w * Self::INFO_WIDTH_SHARE / 100);

        let max_w = cfg.width(w.saturating_sub(info_w + cfg.gap).max(8));
        let (art_w, art_h) = animation.fit(max_w, max_h.max(4));
        let ramp = cfg.ramp();

        Self {
            frames: animation.frames.iter().map(|f| f.scale(art_w, art_h, &ramp)).collect(),
            art_w,
        }
    }
}

/// How the screen is divided between the pinned fetch and the scrolling area.
struct Split {
    /// Rows the fetch pane occupies, starting at row 1.
    pane_h: usize,
    /// 1-based first row of the scroll region, one blank row below the pane.
    scroll_top: u16,
}

impl Split {
    /// Rows the scrolling area needs before splitting is worth it at all.
    const MIN_SCROLL_ROWS: usize = 4;

    /// Share of the screen the fetch may claim. The rest is where you actually
    /// work, so the art gets the smaller half — a pane sized to fill the
    /// terminal would leave only a few unusable lines for command output.
    const PANE_HEIGHT_SHARE: usize = 55;

    /// Row budget for the pane on a terminal of `rows` rows.
    fn budget(rows: u16) -> usize {
        let rows = rows as usize;
        (rows * Self::PANE_HEIGHT_SHARE / 100)
            .min(rows.saturating_sub(Self::MIN_SCROLL_ROWS + 1))
            .max(1)
    }

    /// `None` on a terminal too short to usefully divide, in which case the
    /// fetch is skipped and the whole screen behaves as a plain prompt.
    fn new(pane_lines: usize, rows: u16) -> Option<Self> {
        let max_pane = (rows as usize).checked_sub(Self::MIN_SCROLL_ROWS + 1)?;
        let pane_h = pane_lines.min(max_pane);
        (pane_h > 0).then(|| Self { pane_h, scroll_top: pane_h as u16 + 2 })
    }
}

/// The interactive session: fetch pinned above, prompt and command output
/// scrolling below.
///
/// The pinning is the terminal's own doing. Setting a scroll region (DECSTBM)
/// tells it never to scroll the top rows, so output can pour into the lower
/// part of the screen indefinitely without us redrawing the art on top of it.
/// We only repaint the pane to advance the animation.
///
/// Returns the exit status of the last command run.
fn shell_loop(
    animation: &Animation,
    cfg: &Config,
    title: &str,
    items: &[Item],
) -> io::Result<u8> {
    let mut guard = term::Guard::acquire()?;
    let mut stdout = io::stdout();

    let mut prompt = Prompt::new(cfg);
    let mut pane = Pane::new();

    let (mut cols, mut rows) = term::size();
    let mut layout =
        Layout::build(animation, cfg, title, items, cols as usize, cfg.height(Split::budget(rows)));
    let mut split = enter_split(&mut stdout, &layout, cfg, title, items, cols, rows)?;

    let interval = cfg.frame_interval();
    let mut next_frame = Instant::now() + interval;
    let mut phase = 0usize;
    let mut status = 0u8;

    // The prompt is only rewritten when it changes; the animation above it
    // redraws constantly, and reprinting an unchanged line would make the
    // cursor visibly stutter.
    let mut prompt_dirty = true;

    'session: loop {
        if let Some(split) = &split {
            let scene = render::Scene {
                art: &layout.frames[phase % layout.frames.len()],
                art_w: layout.art_w,
                title,
                items,
                phase,
                width: cols as usize,
            };
            pane.paint(&mut stdout, &render::compose(&scene, cfg), split.pane_h)?;
        }

        if prompt_dirty {
            write!(stdout, "\r{CLEAR_LINE}{}", prompt.render())?;
            stdout.flush()?;
            prompt_dirty = false;
        }

        // Handle every event that arrives before the next frame is due. Waiting
        // on the *remaining* time — rather than sleeping a fixed amount and
        // polling once — is what makes typing feel immediate at low frame
        // rates, and keeps the frame clock from drifting as rendering costs
        // vary.
        while let Some(remaining) = next_frame.checked_duration_since(Instant::now()) {
            if !event::poll(remaining)? {
                break;
            }

            match event::read()? {
                // Terminals may report press *and* release; acting on both
                // would double every keystroke.
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    match prompt.handle(key) {
                        Action::Continue => prompt_dirty = true,
                        Action::Quit => break 'session,
                        Action::Submit(command) => {
                            // Leave the command in scrollback the way a shell
                            // does, then move below it for whatever it prints.
                            write!(stdout, "\r{CLEAR_LINE}{}{command}\r\n", prompt.prefix())?;
                            stdout.flush()?;

                            match execute(&guard, &mut prompt, cfg, &command)? {
                                Some(code) => status = code,
                                None => break 'session,
                            }

                            // A child may have drawn anywhere, including over
                            // the pane; assume the worst and repaint it.
                            pane.invalidate();
                            prompt_dirty = true;
                            next_frame = Instant::now() + interval;
                            continue 'session;
                        }
                    }
                }
                Event::Resize(w, h) => {
                    (cols, rows) = (w, h);
                    let budget = cfg.height(Split::budget(h));
                    layout = Layout::build(animation, cfg, title, items, w as usize, budget);
                    split = enter_split(&mut stdout, &layout, cfg, title, items, w, h)?;
                    pane.invalidate();
                    prompt_dirty = true;
                }
                _ => continue,
            }

            // Something visible changed. Go back to the top to redraw now,
            // without advancing the animation: the frame is not due yet.
            continue 'session;
        }

        phase = phase.wrapping_add(1);
        next_frame += interval;

        // If the process was suspended or the terminal stalled, we may be many
        // intervals behind. Resync instead of racing to catch up.
        let now = Instant::now();
        if next_frame < now {
            next_frame = now + interval;
        }
    }

    guard.restore();
    // Put the shell's next prompt below everything rather than on top of it.
    write!(stdout, "{}\r\n", term::move_to(rows, 1))?;
    stdout.flush()?;

    Ok(status)
}

/// Run one submitted command. `None` means the user asked to leave.
fn execute(
    guard: &term::Guard,
    prompt: &mut Prompt,
    cfg: &Config,
    command: &str,
) -> io::Result<Option<u8>> {
    if let Some(builtin) = prompt::builtin(command) {
        return Ok(match builtin {
            prompt::Builtin::Exit => None,
            prompt::Builtin::Cd(result) => Some(match result {
                Ok(()) => {
                    // The prefix shows the working directory, which just moved.
                    prompt.refresh(cfg);
                    0
                }
                Err(message) => {
                    let mut stdout = io::stdout();
                    write!(stdout, "{message}\r\n")?;
                    stdout.flush()?;
                    1
                }
            }),
        });
    }

    // Hand the terminal over so the child behaves as it would under a shell:
    // cooked mode, visible cursor, all three streams inherited. The scroll
    // region stays set, which is what keeps its output below the fetch.
    guard.suspend()?;
    let status = prompt::run(command);
    guard.resume()?;

    let status = status?;
    Ok(Some(match status.code() {
        Some(code) => code.clamp(0, 255) as u8,
        // Killed by a signal; report it the way a shell does.
        None => 128,
    }))
}

/// Clear the screen, establish the scroll region, and park the cursor inside
/// it. Also used on resize, where the geometry has to be rebuilt from scratch.
fn enter_split(
    out: &mut impl Write,
    layout: &Layout,
    cfg: &Config,
    title: &str,
    items: &[Item],
    cols: u16,
    rows: u16,
) -> io::Result<Option<Split>> {
    let scene = render::Scene {
        art: &layout.frames[0],
        art_w: layout.art_w,
        title,
        items,
        phase: 0,
        width: cols as usize,
    };
    let split = Split::new(render::compose(&scene, cfg).len(), rows);

    write!(out, "{}", term::CLEAR_SCREEN)?;
    match &split {
        // Setting a scroll region homes the cursor, so move into the region
        // explicitly afterwards rather than relying on where it landed.
        Some(s) => write!(
            out,
            "{}{}",
            term::scroll_region(s.scroll_top, rows),
            term::move_to(s.scroll_top, 1)
        )?,
        None => write!(out, "{}{}", term::RESET_SCROLL_REGION, term::move_to(1, 1))?,
    }
    out.flush()?;

    Ok(split)
}

/// Animate where the cursor already is, then leave the last frame behind.
///
/// This is the shell-startup form. Two properties make it safe there, and both
/// come from what it does *not* do:
///
/// * It never enters raw mode, so anything typed during the intro stays in the
///   kernel's line buffer and arrives at your real prompt instead of being
///   swallowed here.
/// * It never clears the screen or positions absolutely. Space is reserved by
///   printing blank lines, exactly as any ordinary command's output would, so
///   scrollback above it survives.
fn play(animation: &Animation, cfg: &Config, title: &str, items: &[Item]) -> io::Result<()> {
    let (cols, rows) = term::size();
    let budget = cfg.height(rows.saturating_sub(1) as usize);
    let layout = Layout::build(animation, cfg, title, items, cols as usize, budget);

    let scene = |phase: usize| render::Scene {
        art: &layout.frames[phase % layout.frames.len()],
        art_w: layout.art_w,
        title,
        items,
        phase,
        width: cols as usize,
    };

    let height = render::compose(&scene(0), cfg).len();
    if height == 0 {
        return Ok(());
    }

    let mut stdout = io::stdout();
    // Reserve the rows first: this scrolls the terminal if needed, and after
    // moving back up, redrawing in place can never scroll again — which is what
    // lets save/restore-cursor stay valid for the whole animation.
    write!(stdout, "{}\x1b[{height}A{}", "\n".repeat(height), term::HIDE_CURSOR)?;

    let result = animate_in_place(&mut stdout, cfg, scene, height);

    // Restore the cursor whatever happened, then step below the art so the
    // shell prompt lands after it rather than on top of it.
    write!(stdout, "{}\x1b[{height}B\r", term::SHOW_CURSOR)?;
    stdout.flush()?;
    result
}

fn animate_in_place<'a>(
    out: &mut impl Write,
    cfg: &Config,
    scene: impl Fn(usize) -> render::Scene<'a>,
    height: usize,
) -> io::Result<()> {
    let interval = cfg.frame_interval();
    let deadline = Instant::now() + Duration::from_secs_f32(cfg.play_seconds.clamp(0.0, 60.0));
    let mut next_frame = Instant::now();
    let mut phase = 0usize;

    while Instant::now() < deadline {
        let lines = render::compose(&scene(phase), cfg);

        // One buffer, one write, cursor saved and restored around it — the same
        // discipline the interactive pane uses, for the same reason.
        let mut buf = String::with_capacity(height * 96);
        buf.push_str("\x1b7");
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                buf.push_str("\r\n");
            }
            buf.push_str("\r\x1b[K");
            buf.push_str(line);
        }
        buf.push_str("\x1b8");
        out.write_all(buf.as_bytes())?;
        out.flush()?;

        phase = phase.wrapping_add(1);
        next_frame += interval;
        if let Some(wait) = next_frame.checked_duration_since(Instant::now()) {
            std::thread::sleep(wait);
        } else {
            next_frame = Instant::now();
        }
    }
    Ok(())
}

/// Non-interactive output: one static frame, no raw mode, no clearing.
///
/// This is what runs when stdout is a pipe, and what `--once` forces.
fn draw_once(animation: &Animation, cfg: &Config, title: &str, items: &[Item]) -> io::Result<()> {
    let (cols, rows) = term::size();
    let budget = cfg.height(rows.saturating_sub(1) as usize);
    let layout = Layout::build(animation, cfg, title, items, cols as usize, budget);

    let lines = render::compose(
        &render::Scene {
            art: &layout.frames[0],
            art_w: layout.art_w,
            title,
            items,
            phase: 0,
            width: cols as usize,
        },
        cfg,
    );

    let mut out = io::stdout().lock();
    for line in &lines {
        writeln!(out, "{line}")?;
    }
    out.flush()
}

/// Command-line overrides. Anything settable here is also settable in the
/// config file; these exist for one-off experiments.
#[derive(Default)]
struct Args {
    animation: Option<String>,
    fps: Option<f32>,
    width: Option<usize>,
    height: Option<usize>,
    play: bool,
    seconds: Option<f32>,
    list: bool,
    set: Option<String>,
    once: bool,
    no_color: bool,
}

impl Args {
    /// `Ok(None)` means the program should exit successfully without drawing.
    fn parse() -> Result<Option<Self>, String> {
        let mut args = Args::default();
        let mut argv = std::env::args().skip(1);

        while let Some(arg) = argv.next() {
            match arg.as_str() {
                "-h" | "--help" => {
                    print!("{HELP}");
                    return Ok(None);
                }
                "-V" | "--version" => {
                    println!("animfetch {}", env!("CARGO_PKG_VERSION"));
                    return Ok(None);
                }
                "-1" | "--once" => args.once = true,
                "-p" | "--play" => args.play = true,
                "-l" | "--list" => args.list = true,
                "--set" => {
                    args.set = Some(argv.next().ok_or("--set needs an animation name")?);
                }
                "--no-color" => args.no_color = true,
                "-a" | "--animation" => {
                    args.animation = Some(argv.next().ok_or("--animation needs a name")?);
                }
                "-f" | "--fps" => {
                    let raw = argv.next().ok_or("--fps needs a number")?;
                    args.fps = Some(raw.parse().map_err(|_| format!("not a number: {raw}"))?);
                }
                "-W" | "--width" => {
                    let raw = argv.next().ok_or("--width needs a number of columns")?;
                    args.width = Some(raw.parse().map_err(|_| format!("not a number: {raw}"))?);
                }
                "-H" | "--height" => {
                    let raw = argv.next().ok_or("--height needs a number of rows")?;
                    args.height = Some(raw.parse().map_err(|_| format!("not a number: {raw}"))?);
                }
                "-s" | "--seconds" => {
                    let raw = argv.next().ok_or("--seconds needs a number")?;
                    args.seconds = Some(raw.parse().map_err(|_| format!("not a number: {raw}"))?);
                }
                other => return Err(format!("unknown option: {other}")),
            }
        }

        Ok(Some(args))
    }

    fn apply(&self, cfg: &mut Config) {
        if let Some(name) = &self.animation {
            cfg.animation = name.clone();
        }
        if let Some(fps) = self.fps {
            cfg.fps = fps;
        }
        if let Some(width) = self.width {
            cfg.width = width;
        }
        if let Some(height) = self.height {
            cfg.height = height;
        }
        if let Some(seconds) = self.seconds {
            cfg.play_seconds = seconds;
        }
        if self.no_color {
            cfg.color = false;
        }
    }
}

const HELP: &str = "\
animfetch — animated system fetch with a live prompt

Usage: animfetch [OPTIONS]

Modes:
  (default)               Interactive: fetch pinned above, prompt below
  -p, --play              Animate in place for a few seconds, then exit
  -1, --once              Print one static frame and exit

Animations:
  -a, --animation <NAME>  Use NAME for this run only
  -l, --list              List available animations (* = current)
      --set <NAME>        Make NAME the default, saved to config.toml

Options:
  -f, --fps <N>           Frames per second
  -W, --width <COLS>      Cap the art width (0 fills the screen)
  -H, --height <ROWS>     Cap the art height (0 fills the screen)
  -s, --seconds <N>       How long --play animates
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
