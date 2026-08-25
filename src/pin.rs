//! Pinning the fetch above your own shell, keeping its history and aliases.
//!
//! Safe from a separate process because: the scroll region means our rows are
//! never scrolled, each frame is one write bracketed by save/restore cursor,
//! and painting pauses when something other than the shell holds the terminal.
//! That last one is what keeps the cat off the top of `vim`.

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::render::Pane;
use crate::{Fetch, Layout, Split, palette, render, term};

/// How often to re-check the terminal size while pinned.
const RESIZE_POLL: Duration = Duration::from_millis(500);

/// Set up the screen, then detach and animate until the terminal goes away.
pub fn start(fetch: &Fetch<'_>) -> io::Result<()> {
    // Claiming is one atomic create, not a check followed by a write: two
    // shells starting on the same terminal at once could otherwise both see no
    // owner and both go on to animate the same rows.
    if !claim_terminal()? {
        // A nested shell, most likely. Silent because this runs from an rc
        // file, where noise on every subshell gets a tool uninstalled.
        return Ok(());
    }

    let (cols, rows) = term::size();
    let layout = Layout::pinned(fetch, cols, rows);

    let Some(split) = Split::fit(&layout, fetch.cfg, rows) else {
        release_terminal();
        return Err(io::Error::other("terminal too short to pin a fetch"));
    };

    // Captured before forking, while our parent is still the shell.
    // SAFETY: both calls only read process state.
    let shell_pgid = unsafe { libc::getpgid(libc::getppid()) };

    // Before the fork, so the region exists before the shell's next prompt.
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

    // SAFETY: getppid only reads process state.
    let shell_pid = unsafe { libc::getppid() };

    // SAFETY: single-threaded here, so the child inherits no locks.
    match unsafe { libc::fork() } {
        -1 => {
            release_terminal();
            return Err(io::Error::last_os_error());
        }
        0 => {} // child: fall through and animate
        child => {
            // Written here rather than in the child: the parent must not return
            // to the shell until the file names the process doing the drawing,
            // or an --unpin run straight afterwards finds nothing pinned.
            write_pid_file(child)?;
            return Ok(());
        }
    }

    // Leave the shell's group, or a Ctrl-C at its prompt kills the animation.
    // Not `setsid`: that drops the controlling terminal, which `tcgetpgrp` needs.
    // SAFETY: valid on self here, since we are not a session leader.
    unsafe { libc::setpgid(0, 0) };

    let result = animate(fetch, shell_pgid, shell_pid);
    let _ = std::fs::remove_file(pid_path()?);

    // Usually already gone by here, so best effort.
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

    // SAFETY: `pid` came from our pid file and was just confirmed alive.
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

fn animate(fetch: &Fetch<'_>, shell_pgid: libc::pid_t, shell_pid: libc::pid_t) -> io::Result<()> {
    let cfg = fetch.cfg;
    let mut stdout = io::stdout();
    let mut pane = Pane::new().with_origin_mode();

    // Only colours can change under a pinned fetch. `cfg` still owns geometry.
    let mut theme = cfg.clone();
    let mut palette = palette::Watch::new(cfg);

    let (mut cols, mut rows) = term::size();
    let mut layout = Layout::pinned(fetch, cols, rows);
    let mut split = Split::fit(&layout, cfg, rows);

    let interval = cfg.frame_interval();
    let mut phase = 0usize;
    let mut next_resize_check = Instant::now() + RESIZE_POLL;
    // So we can tell when a command just finished.
    let mut had_terminal = true;

    loop {
        // Nothing signals us any more, so notice the shell leaving ourselves.
        // SAFETY: signal 0 checks permission without delivering.
        if unsafe { libc::kill(shell_pid, 0) } != 0 {
            return Ok(());
        }

        let foreground = foreground_group();
        if foreground < 0 {
            return Ok(()); // the controlling terminal went away
        }
        let holds_terminal = foreground == shell_pgid;

        // Every frame, not on the slow poll. Terminals drop the scroll region
        // when the window resizes, and until we set it again our absolute rows
        // scroll with everything else, smearing copies of the art down the
        // screen. A `size` call is an ioctl on a fd we already hold, so this is
        // far cheaper than the frame it protects.
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

        if holds_terminal {
            if !had_terminal {
                // A full-screen program may have reset the region on exit.
                let top = split.as_ref().map_or(1, |s| s.scroll_top);
                reassert_region(&mut stdout, top, rows)?;
                pane.invalidate();
            }

            if let Some(split) = &split {
                let lines = render::compose(&layout.scene(phase), &theme);
                // A failed write means the terminal is gone.
                if pane.paint(&mut stdout, &lines, split.pane_h).is_err() {
                    return Ok(());
                }
            }
            phase = phase.wrapping_add(1);
        }
        had_terminal = holds_terminal;

        if Instant::now() >= next_resize_check {
            next_resize_check = Instant::now() + RESIZE_POLL;

            // Rethemed since the last check; take the new colours.
            if palette.changed() {
                crate::palette::apply(&mut theme);
                pane.invalidate();
            }
        }

        std::thread::sleep(interval);
    }
}

/// Re-establish the scroll region. Bracketed with save/restore because setting
/// a region homes the cursor, and the shell's must come back where it was.
fn reassert_region(out: &mut impl Write, top: u16, rows: u16) -> io::Result<()> {
    write!(
        out,
        "\x1b7{}{}\x1b8",
        term::scroll_region(top, rows),
        term::SET_ORIGIN_MODE
    )
}

/// The process group currently holding the terminal, or -1 if we have none.
fn foreground_group() -> libc::pid_t {
    // SAFETY: reads terminal state only.
    unsafe { libc::tcgetpgrp(libc::STDIN_FILENO) }
}

/// Pid file keyed by terminal device, so nested shells find each other.
fn pid_path() -> io::Result<PathBuf> {
    // SAFETY: ttyname returns a static buffer or null.
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

    // SAFETY: signal 0 tests existence without delivering anything.
    if unsafe { libc::kill(pid, 0) } == 0 {
        Ok(Some(pid))
    } else {
        // Stale file from a terminal that went away.
        let _ = std::fs::remove_file(&path);
        Ok(None)
    }
}

fn write_pid_file(pid: libc::pid_t) -> io::Result<()> {
    std::fs::write(pid_path()?, pid.to_string())
}

/// Take ownership of this terminal, or report that someone else holds it.
///
/// `create_new` is the whole point: the check and the claim are one syscall, so
/// two instances racing cannot both win. The placeholder is our own pid, which
/// is alive for as long as it matters: the parent replaces it with the child's
/// before returning, and anyone reading in between sees a live owner either way.
fn claim_terminal() -> io::Result<bool> {
    let path = pid_path()?;

    for _ in 0..2 {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                write!(file, "{}", std::process::id())?;
                return Ok(true);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                // Someone owns it, or left a file behind. `live_owner` deletes
                // the file when the owner is gone, so one retry settles it.
                if live_owner()?.is_some() {
                    return Ok(false);
                }
            }
            Err(e) => return Err(e),
        }
    }

    Ok(false)
}

/// Give the claim back, for the paths that fail after taking it.
fn release_terminal() {
    if let Ok(path) = pid_path() {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `claim_terminal` writes through `pid_path`, which needs a tty, so these
    /// exercise the claim logic against a path we control instead.
    fn claim_at(path: &std::path::Path, pid: libc::pid_t) -> io::Result<bool> {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
        {
            Ok(mut file) => {
                write!(file, "{pid}")?;
                Ok(true)
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(false),
            Err(e) => Err(e),
        }
    }

    #[test]
    fn only_one_claim_can_win() {
        // The bug this guards: a check followed by a separate write let two
        // instances both see no owner and both start animating the same rows.
        let dir = std::env::temp_dir().join(format!("animfetch-claim-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.pid");
        let _ = std::fs::remove_file(&path);

        assert!(claim_at(&path, 111).unwrap(), "first claim should win");
        assert!(!claim_at(&path, 222).unwrap(), "second claim must lose");

        // The winner's pid is the one recorded, not the loser's.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "111");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_released_claim_can_be_taken_again() {
        let dir = std::env::temp_dir().join(format!("animfetch-rel-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.pid");
        let _ = std::fs::remove_file(&path);

        assert!(claim_at(&path, 111).unwrap());
        std::fs::remove_file(&path).unwrap();
        assert!(
            claim_at(&path, 222).unwrap(),
            "release must free the terminal"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
