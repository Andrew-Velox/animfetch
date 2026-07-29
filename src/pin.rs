//! Pinning the fetch above your own shell.
//!
//! The interactive mode owns the prompt, which costs you your shell's history,
//! completion, and aliases. This mode does not: it sets up a scroll region,
//! detaches into the background, and paints the top rows while your real shell
//! runs underneath in the region below.
//!
//! Three things make that safe to do from a separate process:
//!
//! * The scroll region means the terminal never scrolls the pane's rows, so the
//!   shell's output cannot disturb the art and the art never has to be redrawn
//!   because of scrolling.
//! * Each frame is a single write bracketed by save/restore cursor, so it
//!   cannot interleave with the shell's own output or move the cursor you are
//!   typing at.
//! * Painting pauses whenever something other than the shell holds the
//!   terminal. That is what keeps the cat off the top of `vim`.

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::render::Pane;
use crate::{Fetch, Layout, Split, palette, render, term};

/// How often to re-check the terminal size while pinned.
const RESIZE_POLL: Duration = Duration::from_millis(500);

/// Set up the screen, then detach and animate until the terminal goes away.
pub fn start(fetch: &Fetch<'_>) -> io::Result<()> {
    if live_owner()?.is_some() {
        // Another instance already owns this terminal — a nested shell, most
        // likely. Two of them would paint different frames into the same rows.
        //
        // Silently, and not as an error: this runs from a shell startup file,
        // where a line of output every time you open a subshell is the kind of
        // noise that gets a tool removed.
        return Ok(());
    }

    let (cols, rows) = term::size();
    let layout = Layout::pinned(fetch, cols, rows);

    let Some(split) = Split::fit(&layout, fetch.cfg, rows) else {
        return Err(io::Error::other("terminal too short to pin a fetch"));
    };

    // The shell's pgid, captured before forking: our own parent is the shell,
    // and after the fork the child's parent is init.
    //
    // SAFETY: both calls only read process state and cannot fail meaningfully.
    let shell_pgid = unsafe { libc::getpgid(libc::getppid()) };

    // Do the screen setup here, in the process the shell is waiting on, so the
    // region exists before it prints its next prompt. Doing it after the fork
    // would race the prompt and leave it above the region.
    let mut stdout = io::stdout();
    write!(
        stdout,
        "{}{}{}{}",
        term::CLEAR_SCREEN,
        term::scroll_region(split.scroll_top, rows),
        term::SET_ORIGIN_MODE,
        term::HOME
    )?;
    stdout.flush()?;

    // The shell's pid, so the daemon can notice when its terminal is gone.
    // SAFETY: getppid only reads process state.
    let shell_pid = unsafe { libc::getppid() };

    // SAFETY: single-threaded at this point, so the child inherits no locks it
    // could deadlock on.
    match unsafe { libc::fork() } {
        -1 => return Err(io::Error::last_os_error()),
        0 => {}                    // child: fall through and animate
        _ => return Ok(()),        // parent: hand the terminal back to the shell
    }

    // Leave the shell's process group.
    //
    // Terminal-generated signals — SIGINT from Ctrl-C, SIGQUIT, SIGTSTP — go to
    // every process in the foreground group. Started from a shell startup file
    // we are *in* that group, because the shell runs rc commands without
    // forking a separate job, so a single Ctrl-C at the prompt would kill the
    // animation. Our own group keeps those signals where they belong.
    //
    // The session is deliberately left alone: `setsid` would drop the
    // controlling terminal, and `tcgetpgrp` needs it.
    //
    // SAFETY: setpgid on self with a new group is always valid here; we are not
    // a session leader.
    unsafe { libc::setpgid(0, 0) };

    write_pid_file()?;
    let result = animate(fetch, shell_pgid, shell_pid);
    let _ = std::fs::remove_file(pid_path()?);

    // The terminal is usually already gone by here; resetting is best effort.
    let _ = write!(
        io::stdout(),
        "{}{}",
        term::RESET_ORIGIN_MODE,
        term::RESET_SCROLL_REGION
    );
    let _ = io::stdout().flush();
    result
}

/// Stop the instance pinned to this terminal, if any.
pub fn stop() -> io::Result<bool> {
    let Some(pid) = live_owner()? else {
        return Ok(false);
    };

    // SAFETY: `pid` came from our own pid file and was just confirmed alive.
    unsafe { libc::kill(pid, libc::SIGTERM) };
    let _ = std::fs::remove_file(pid_path()?);

    let mut stdout = io::stdout();
    write!(
        stdout,
        "{}{}{}",
        term::RESET_ORIGIN_MODE,
        term::RESET_SCROLL_REGION,
        term::CLEAR_SCREEN
    )?;
    write!(stdout, "{}", term::move_to(1, 1))?;
    stdout.flush()?;
    Ok(true)
}

fn animate(
    fetch: &Fetch<'_>,
    shell_pgid: libc::pid_t,
    shell_pid: libc::pid_t,
) -> io::Result<()> {
    let cfg = fetch.cfg;
    let mut stdout = io::stdout();
    let mut pane = Pane::new().with_origin_mode();

    // Colours are the one thing that can change under a pinned fetch, when the
    // wallpaper is rethemed. `cfg` still decides everything about geometry;
    // this copy carries only the colours, and is re-resolved when the palette
    // file is rewritten.
    let mut theme = cfg.clone();
    let mut palette = palette::Watch::new(cfg);

    let (mut cols, mut rows) = term::size();
    let mut layout = Layout::pinned(fetch, cols, rows);
    let mut split = Split::fit(&layout, cfg, rows);

    let interval = cfg.frame_interval();
    let mut phase = 0usize;
    let mut next_resize_check = Instant::now() + RESIZE_POLL;
    // Whether the shell held the terminal last frame, so we can tell when a
    // command has just finished and the region may need re-asserting.
    let mut had_terminal = true;

    loop {
        // Now that terminal signals no longer reach us, nothing will tell us to
        // stop — so notice for ourselves when there is nothing left to paint
        // on. A failed write catches it too, but only while we are painting,
        // and we stop painting once the shell is gone.
        //
        // SAFETY: signal 0 performs the permission check without delivering.
        if unsafe { libc::kill(shell_pid, 0) } != 0 {
            return Ok(());
        }

        let foreground = foreground_group();
        if foreground < 0 {
            return Ok(()); // the controlling terminal went away
        }
        let holds_terminal = foreground == shell_pgid;

        if holds_terminal {
            if !had_terminal {
                // A full-screen program may have reset the scroll region on its
                // way out. Re-assert it, and repaint what it drew over.
                let top = split.as_ref().map_or(1, |s| s.scroll_top);
                reassert_region(&mut stdout, top, rows)?;
                pane.invalidate();
            }

            if let Some(split) = &split {
                let lines = render::compose(&layout.scene(phase), &theme);
                // A failed write means the terminal is gone; that is our cue to
                // stop, not an error worth reporting to nobody.
                if pane.paint(&mut stdout, &lines, split.pane_h).is_err() {
                    return Ok(());
                }
            }
            phase = phase.wrapping_add(1);
        }
        had_terminal = holds_terminal;

        if Instant::now() >= next_resize_check {
            next_resize_check = Instant::now() + RESIZE_POLL;

            // Rethemed since the last check: take the new colours and repaint,
            // rather than showing the palette from whenever this was started.
            if palette.changed() {
                crate::palette::apply(&mut theme);
                pane.invalidate();
            }

            let (w, h) = term::size();
            if (w, h) != (cols, rows) {
                (cols, rows) = (w, h);
                layout = Layout::pinned(fetch, cols, rows);
                split = Split::fit(&layout, cfg, rows);

                if let Some(s) = &split {
                    reassert_region(&mut stdout, s.scroll_top, rows)?;
                }
                pane.invalidate();
            }
        }

        std::thread::sleep(interval);
    }
}

/// Re-establish the scroll region without disturbing the cursor.
///
/// Setting a region homes the cursor as a side effect, so it is bracketed with
/// save/restore — the shell's cursor must come back exactly where it was.
fn reassert_region(out: &mut impl Write, top: u16, rows: u16) -> io::Result<()> {
    write!(out, "\x1b7{}{}\x1b8", term::scroll_region(top, rows), term::SET_ORIGIN_MODE)
}

/// The process group currently holding the terminal, or -1 if we have none.
fn foreground_group() -> libc::pid_t {
    // SAFETY: reads terminal state only, and reports failure by return value.
    unsafe { libc::tcgetpgrp(libc::STDIN_FILENO) }
}

/// Pid file for this terminal, so one instance can find another.
///
/// Keyed by the terminal device rather than by user: one pinned fetch per
/// terminal is the unit that makes sense, and nested shells share a device.
fn pid_path() -> io::Result<PathBuf> {
    // SAFETY: ttyname returns a pointer to a static buffer, or null.
    let name = unsafe {
        let raw = libc::ttyname(libc::STDIN_FILENO);
        if raw.is_null() {
            return Err(io::Error::other("not attached to a terminal"));
        }
        std::ffi::CStr::from_ptr(raw).to_string_lossy().into_owned()
    };

    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);

    let slug: String = name
        .trim_start_matches('/')
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    Ok(dir.join(format!("animfetch-{slug}.pid")))
}

/// The pid pinned to this terminal, if one is recorded and still running.
fn live_owner() -> io::Result<Option<libc::pid_t>> {
    let path = pid_path()?;
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    let Ok(pid) = text.trim().parse::<libc::pid_t>() else {
        return Ok(None);
    };

    // Signal 0 tests for existence without delivering anything.
    //
    // SAFETY: kill with signal 0 has no effect beyond the permission check.
    if unsafe { libc::kill(pid, 0) } == 0 {
        Ok(Some(pid))
    } else {
        // Stale file from a terminal that went away without cleaning up.
        let _ = std::fs::remove_file(&path);
        Ok(None)
    }
}

fn write_pid_file() -> io::Result<()> {
    std::fs::write(pid_path()?, std::process::id().to_string())
}
