//! animfetch: an animated system fetch that stays usable while it animates.
//!
//! Two ideas carry it. Raw mode plus a non-blocking poll means each loop draws a
//! frame then waits for input only until the next one is due, so typing stays
//! instant. A scroll region pins the art, so everything you run scrolls below.

mod anim;
mod color;
mod config;
mod fetch;
mod palette;
mod pin;
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

/// Run whichever mode the arguments selected.
fn dispatch(args: Args, dir: Option<std::path::PathBuf>, mut cfg: Config) -> io::Result<ExitCode> {
    args.apply(&mut cfg);

    // Before anything is drawn, so every mode gets the same colours.
    palette::apply(&mut cfg);

    let interactive = io::stdin().is_terminal() && io::stdout().is_terminal();
    // Meaningless when piped, and NO_COLOR is how you opt out.
    if !io::stdout().is_terminal() || std::env::var_os("NO_COLOR").is_some() {
        cfg.color = false;
    }

    let anim_dir = dir.as_ref().map(|d| d.join("anim"));

    if args.list {
        list_animations(anim_dir.as_deref(), &cfg.animation);
        return Ok(ExitCode::SUCCESS);
    }

    if let Some(name) = &args.set {
        // A config that breaks every future run is worse than an error now.
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
    let fetch = Fetch { animation: &animation, cfg: &cfg, title: &title, items: &items };

    if !interactive || args.once {
        return draw_once(&fetch).map(|()| ExitCode::SUCCESS);
    }

    if args.unpin {
        if !pin::stop()? {
            eprintln!("animfetch: nothing pinned to this terminal");
        }
        return Ok(ExitCode::SUCCESS);
    }

    if args.pin {
        return pin::start(&fetch).map(|()| ExitCode::SUCCESS);
    }

    if args.play {
        return play(&fetch).map(|()| ExitCode::SUCCESS);
    }

    let status = shell_loop(&fetch)?;
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

/// Write `animation = "<name>"` into the config, creating it if needed. Edited
/// as text, so comments survive. A trailing comment on that line does not.
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

/// What the drawing code needs and never changes. Passed as one value because
/// these four travel together through every mode.
#[derive(Clone, Copy)]
pub struct Fetch<'a> {
    pub animation: &'a Animation,
    pub cfg: &'a Config,
    pub title: &'a str,
    pub items: &'a [Item],
}

/// Pre-scaled frames for one terminal size. Rebuilt on resize, which is exactly
/// when the art and every [`render::Scene`] built from it go stale.
pub struct Layout<'a> {
    frames: Vec<Vec<String>>,
    art_w: usize,
    title: &'a str,
    items: &'a [Item],
    /// Terminal width this layout was scaled for.
    width: usize,
}

impl<'a> Layout<'a> {
    /// Below this, drop the art and give the info pane the whole terminal.
    const MIN_SPLIT_WIDTH: usize = 40;

    /// Share the info pane may claim before values get truncated instead.
    const INFO_WIDTH_SHARE: usize = 45;

    /// Fit the art into whatever the info pane does not need. `max_h` is the
    /// pane's row budget, not the terminal height. `wanted` caps how many frames
    /// get scaled, since a still drawing has no use for the rest.
    fn build(fetch: &Fetch<'a>, cols: u16, max_h: usize, wanted: usize) -> Self {
        let &Fetch { animation, cfg, title, items } = fetch;
        let w = cols as usize;

        let (frames, art_w) = if w < Self::MIN_SPLIT_WIDTH {
            // One empty frame, so `phase % len` needs no special case.
            (vec![Vec::new()], 0)
        } else {
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

            // A long CPU model would otherwise squeeze the art to a blob.
            let info_w = widest.min(w * Self::INFO_WIDTH_SHARE / 100);

            let max_w = cfg.width(w.saturating_sub(info_w + cfg.gap).max(8));
            let (art_w, art_h) = animation.fit(max_w, max_h.max(4));
            let ramp = cfg.ramp();
            let ink = cfg.ink(&ramp);

            let frames = animation
                .frames
                .iter()
                .take(wanted)
                .map(|f| f.scale(art_w, art_h, ink))
                .collect();
            (frames, art_w)
        };

        Self { frames, art_w, title, items, width: w }
    }

    /// Layout for the pinned pane on a terminal of this size.
    pub fn pinned(fetch: &Fetch<'a>, cols: u16, rows: u16) -> Self {
        Self::build(fetch, cols, fetch.cfg.height(Split::budget(rows)), usize::MAX)
    }

    /// Owns the whole terminal but the row the shell prompt lands on.
    pub fn full_screen(fetch: &Fetch<'a>, cols: u16, rows: u16) -> Self {
        Self::build(fetch, cols, fetch.cfg.height(rows.saturating_sub(1) as usize), usize::MAX)
    }

    /// Like [`Self::full_screen`], but only the one frame a still will use.
    pub fn still(fetch: &Fetch<'a>, cols: u16, rows: u16) -> Self {
        Self::build(fetch, cols, fetch.cfg.height(rows.saturating_sub(1) as usize), 1)
    }

    /// What to draw for frame `phase`.
    pub fn scene(&self, phase: usize) -> render::Scene<'_> {
        render::Scene {
            art: &self.frames[phase % self.frames.len()],
            art_w: self.art_w,
            title: self.title,
            items: self.items,
            phase,
            width: self.width,
        }
    }
}

/// How the screen is divided between the pinned fetch and the scrolling area.
pub struct Split {
    /// Rows the fetch pane occupies, starting at row 1.
    pub pane_h: usize,
    /// 1-based first row of the scroll region, one blank row below the pane.
    pub scroll_top: u16,
}

impl Split {
    /// Rows the scrolling area needs before splitting is worth it at all.
    const MIN_SCROLL_ROWS: usize = 4;

    /// Share the fetch may claim. The art gets the smaller half; the rest is
    /// where you actually work.
    const PANE_HEIGHT_SHARE: usize = 55;

    /// Row budget for the pane on a terminal of `rows` rows.
    fn budget(rows: u16) -> usize {
        let rows = rows as usize;
        (rows * Self::PANE_HEIGHT_SHARE / 100)
            .min(rows.saturating_sub(Self::MIN_SCROLL_ROWS + 1))
            .max(1)
    }

    /// `None` when the terminal is too short to usefully divide.
    fn new(pane_lines: usize, rows: u16) -> Option<Self> {
        let max_pane = (rows as usize).checked_sub(Self::MIN_SCROLL_ROWS + 1)?;
        let pane_h = pane_lines.min(max_pane);
        (pane_h > 0).then(|| Self { pane_h, scroll_top: pane_h as u16 + 2 })
    }

    /// The split for `layout`, whose composed height is what the pane needs.
    pub fn fit(layout: &Layout<'_>, cfg: &Config, rows: u16) -> Option<Self> {
        Self::new(render::compose(&layout.scene(0), cfg).len(), rows)
    }
}

/// The interactive session: fetch above, prompt and output scrolling below.
/// The terminal does the pinning. We only repaint to advance the animation.
/// Returns the exit status of the last command run.
fn shell_loop(fetch: &Fetch<'_>) -> io::Result<u8> {
    let cfg = fetch.cfg;
    let mut guard = term::Guard::acquire()?;
    let mut stdout = io::stdout();

    let mut prompt = Prompt::new(cfg);
    let mut pane = Pane::new();

    let (cols, mut rows) = term::size();
    let mut layout = Layout::pinned(fetch, cols, rows);
    let mut split = enter_split(&mut stdout, &layout, cfg, rows)?;

    let interval = cfg.frame_interval();
    let mut next_frame = Instant::now() + interval;
    let mut phase = 0usize;
    let mut status = 0u8;

    // Reprinting an unchanged prompt makes the cursor visibly stutter.
    let mut prompt_dirty = true;

    'session: loop {
        if let Some(split) = &split {
            let lines = render::compose(&layout.scene(phase), cfg);
            pane.paint(&mut stdout, &lines, split.pane_h)?;
        }

        if prompt_dirty {
            write!(stdout, "\r{CLEAR_LINE}{}", prompt.render())?;
            stdout.flush()?;
            prompt_dirty = false;
        }

        // Waiting on the *remaining* time, not a fixed sleep, is what makes
        // typing feel immediate at low frame rates.
        while let Some(remaining) = next_frame.checked_duration_since(Instant::now()) {
            if !event::poll(remaining)? {
                break;
            }

            match event::read()? {
                // Terminals may report release too, doubling every keystroke.
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    match prompt.handle(key) {
                        Action::Continue => prompt_dirty = true,
                        Action::Quit => break 'session,
                        Action::Submit(command) => {
                            // Leave it in scrollback the way a shell does.
                            write!(stdout, "\r{CLEAR_LINE}{}{command}\r\n", prompt.prefix())?;
                            stdout.flush()?;

                            match execute(&guard, &mut prompt, cfg, &command)? {
                                Some(code) => status = code,
                                None => break 'session,
                            }

                            // The child may have drawn over the pane.
                            pane.invalidate();
                            prompt_dirty = true;
                            next_frame = Instant::now() + interval;
                            continue 'session;
                        }
                    }
                }
                Event::Resize(w, h) => {
                    rows = h;
                    layout = Layout::pinned(fetch, w, h);
                    split = enter_split(&mut stdout, &layout, cfg, h)?;
                    pane.invalidate();
                    prompt_dirty = true;
                }
                _ => continue,
            }

            // Redraw now, but don't advance: the frame isn't due yet.
            continue 'session;
        }

        phase = phase.wrapping_add(1);
        next_frame += interval;

        // Many intervals behind after a suspend. Resync, don't catch up.
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

    // Cooked mode and a visible cursor, so the child behaves normally. The
    // region stays set, keeping its output below the fetch.
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

/// Clear, set the scroll region, park the cursor inside it. Also used on
/// resize, where the geometry is rebuilt from scratch.
fn enter_split(
    out: &mut impl Write,
    layout: &Layout<'_>,
    cfg: &Config,
    rows: u16,
) -> io::Result<Option<Split>> {
    let split = Split::fit(layout, cfg, rows);

    write!(out, "{}", term::CLEAR_SCREEN)?;
    match &split {
        // Setting a region homes the cursor, so move in explicitly.
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

/// Animate in place, then leave the last frame behind. The shell-startup form.
///
/// Safe there because of what it avoids: no raw mode, so anything you type
/// reaches your real prompt, and no clearing or absolute positioning, so
/// scrollback survives. Space is reserved by printing blank lines.
fn play(fetch: &Fetch<'_>) -> io::Result<()> {
    let cfg = fetch.cfg;
    let (cols, rows) = term::size();
    let layout = Layout::full_screen(fetch, cols, rows);

    let height = render::compose(&layout.scene(0), cfg).len();
    if height == 0 {
        return Ok(());
    }

    let mut stdout = io::stdout();
    // Reserve rows first, so redrawing in place can never scroll again and
    // save/restore-cursor stays valid throughout.
    write!(stdout, "{}\x1b[{height}A{}", "\n".repeat(height), term::HIDE_CURSOR)?;

    let result = animate_in_place(&mut stdout, &layout, cfg, height);

    // Step below the art so the shell prompt lands after it.
    write!(stdout, "{}\x1b[{height}B\r", term::SHOW_CURSOR)?;
    stdout.flush()?;
    result
}

fn animate_in_place(
    out: &mut impl Write,
    layout: &Layout<'_>,
    cfg: &Config,
    height: usize,
) -> io::Result<()> {
    let interval = cfg.frame_interval();
    let deadline = Instant::now() + Duration::from_secs_f32(cfg.play_seconds.clamp(0.0, 60.0));
    let mut next_frame = Instant::now();
    let mut phase = 0usize;

    while Instant::now() < deadline {
        let lines = render::compose(&layout.scene(phase), cfg);

        // One buffer, one write, same discipline as the interactive pane.
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

/// One static frame, no raw mode, no clearing. Runs when piped, or on `--once`.
fn draw_once(fetch: &Fetch<'_>) -> io::Result<()> {
    let (cols, rows) = term::size();
    let layout = Layout::still(fetch, cols, rows);
    let lines = render::compose(&layout.scene(0), fetch.cfg);

    let mut out = io::stdout().lock();
    for line in &lines {
        writeln!(out, "{line}")?;
    }
    out.flush()
}

/// Command-line overrides, for one-off experiments. All also live in config.
#[derive(Default)]
struct Args {
    animation: Option<String>,
    fps: Option<f32>,
    style: Option<config::Style>,
    width: Option<usize>,
    height: Option<usize>,
    play: bool,
    pin: bool,
    unpin: bool,
    seconds: Option<f32>,
    list: bool,
    set: Option<String>,
    once: bool,
    no_color: bool,
    palette: bool,
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
                "--pin" => args.pin = true,
                "--unpin" => args.unpin = true,
                "-l" | "--list" => args.list = true,
                "--set" => {
                    args.set = Some(argv.next().ok_or("--set needs an animation name")?);
                }
                "--no-color" => args.no_color = true,
                "--palette" => args.palette = true,
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
                "--style" => {
                    let raw = argv.next().ok_or("--style needs half, quad, or ramp")?;
                    args.style = Some(match raw.as_str() {
                        "half" => config::Style::Half,
                        "quad" => config::Style::Quad,
                        "ramp" => config::Style::Ramp,
                        other => return Err(format!("unknown style: {other} (half, quad, or ramp)")),
                    });
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
        if let Some(style) = self.style {
            cfg.style = style;
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
        if self.palette {
            cfg.color_source = config::ColorSource::Palette;
        }
    }
}

const HELP: &str = "\
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
      --style <STYLE>     half (default), quad (finest detail), ramp (ASCII)
  -W, --width <COLS>      Cap the art width (0 fills the screen)
  -H, --height <ROWS>     Cap the art height (0 fills the screen)
  -s, --seconds <N>       How long --play animates
      --palette           Take colours from your desktop's generated palette
                          (matugen, pywal, wallust) instead of the config
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
