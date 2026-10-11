//! The ways animfetch runs: interactive, played in place, or one frame.

use std::io::{self, Write};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEventKind};

use crate::config::Config;
use crate::layout::{Fetch, Layout, Split};
use crate::prompt::{self, Action, Prompt};
use crate::render::{self, Pane};
use crate::term;

/// Erase from the cursor to the end of the line.
const CLEAR_LINE: &str = "\x1b[K";

/// The interactive session: fetch above, prompt and output scrolling below.
/// The terminal does the pinning. We only repaint to advance the animation.
/// Returns the exit status of the last command run.
pub fn shell_loop(fetch: &Fetch<'_>) -> io::Result<u8> {
    let cfg = fetch.cfg;
    let mut guard = term::Guard::acquire()?;
    let mut stdout = io::stdout();

    let mut prompt = Prompt::new(cfg);
    let mut pane = Pane::new();
    let mut live = crate::fetch::Live::new(cfg, fetch.items);

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
        live.tick();

        if let Some(split) = &split {
            let lines = render::compose(&layout.scene_with(phase, live.items()), cfg);
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
pub fn play(fetch: &Fetch<'_>) -> io::Result<()> {
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
    write!(
        stdout,
        "{}\x1b[{height}A{}",
        "\n".repeat(height),
        term::HIDE_CURSOR
    )?;

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
pub fn draw_once(fetch: &Fetch<'_>) -> io::Result<()> {
    let (cols, rows) = term::size();
    let layout = Layout::still(fetch, cols, rows);
    let lines = render::compose(&layout.scene(0), fetch.cfg);

    let mut out = io::stdout().lock();
    for line in &lines {
        writeln!(out, "{line}")?;
    }
    out.flush()
}
