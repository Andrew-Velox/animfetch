//! Terminal state ownership.
//!
//! Raw mode is what lets the animation and the prompt share one loop: with
//! `ECHO`/`ICANON` off the kernel stops buffering a line before handing it to
//! us, so keystrokes are readable the moment they arrive instead of at Enter.
//!
//! A scroll region (DECSTBM) is what keeps the fetch pinned. Once set, the
//! terminal confines scrolling to the rows below it, so command output can fill
//! and scroll the lower part of the screen while the art above stays untouched
//! — no redrawing on our part, the terminal simply never scrolls those rows.
//!
//! Both settings outlive the process if we do not undo them, and a terminal
//! left in raw mode with a scroll region is thoroughly broken. Every exit path
//! therefore restores: `Drop` covers normal returns and `?` bail-outs, the panic
//! hook covers panics.

use std::io::{self, Write};
use std::os::fd::{AsRawFd, RawFd};

use crossterm::terminal;

/// Restores raw mode and the scroll region when it goes out of scope.
pub struct Guard {
    restored: bool,
}

impl Guard {
    /// Enters raw mode and hides the cursor.
    ///
    /// We deliberately stay on the main screen rather than switching to the
    /// alternate one, so what you run leaves its output in your scrollback.
    pub fn acquire() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        write!(io::stdout(), "{HIDE_CURSOR}")?;
        io::stdout().flush()?;

        install_panic_hook();
        Ok(Self { restored: false })
    }

    /// Hand the terminal back to a child process: cooked mode and a visible
    /// cursor, so it behaves exactly as it would under your shell.
    ///
    /// The scroll region deliberately stays set — that is what keeps the
    /// child's output below the fetch.
    pub fn suspend(&self) -> io::Result<()> {
        terminal::disable_raw_mode()?;
        let mut stdout = io::stdout();
        write!(stdout, "{SHOW_CURSOR}")?;
        stdout.flush()
    }

    /// Take the terminal back once the child has exited.
    pub fn resume(&self) -> io::Result<()> {
        terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        write!(stdout, "{HIDE_CURSOR}")?;
        stdout.flush()
    }

    /// Undo everything. Idempotent, so `Drop` after an explicit call is
    /// harmless.
    pub fn restore(&mut self) {
        if self.restored {
            return;
        }
        self.restored = true;

        let mut stdout = io::stdout();
        let _ = write!(stdout, "{RESET_SCROLL_REGION}{SHOW_CURSOR}");
        let _ = stdout.flush();
        let _ = terminal::disable_raw_mode();
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.restore();
    }
}

/// A panic in raw mode would otherwise print an unreadable backtrace into a
/// terminal with no echo, no line wrapping, and a scroll region still set.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = terminal::disable_raw_mode();
        let _ = write!(io::stdout(), "{RESET_SCROLL_REGION}{SHOW_CURSOR}\r\n");
        let _ = io::stdout().flush();
        previous(info);
    }));
}

pub const HIDE_CURSOR: &str = "\x1b[?25l";
pub const SHOW_CURSOR: &str = "\x1b[?25h";
pub const RESET_SCROLL_REGION: &str = "\x1b[r";

/// DECOM. With origin mode set, row addressing is relative to the scroll
/// region and the cursor cannot leave it.
///
/// This is what keeps `clear` from stranding the shell above a pinned fetch:
/// `clear` homes the cursor with `ESC[H`, which without this means screen row
/// 1 — inside the pinned rows, where the shell's prompt is then painted over.
pub const SET_ORIGIN_MODE: &str = "\x1b[?6h";
pub const RESET_ORIGIN_MODE: &str = "\x1b[?6l";

/// Home the cursor. Under origin mode this is the scroll region's top-left.
pub const HOME: &str = "\x1b[H";
pub const CLEAR_SCREEN: &str = "\x1b[2J";

/// Confine scrolling to `top..=bottom`, 1-based and inclusive.
///
/// Setting the region homes the cursor as a side effect, so callers must
/// reposition afterwards.
pub fn scroll_region(top: u16, bottom: u16) -> String {
    format!("\x1b[{top};{bottom}r")
}

/// Move the cursor to a 1-based row and column.
pub fn move_to(row: u16, col: u16) -> String {
    format!("\x1b[{row};{col}H")
}

/// Terminal size in cells, with a usable fallback when stdout is not a TTY
/// (piped output still needs *some* geometry to lay out against).
///
/// The ioctl is issued directly rather than through crossterm, which opens
/// `/dev/tty` on every call and, failing that, shells out to `tput`. That costs
/// about a millisecond — a third of the startup this tool is meant to add to a
/// shell, and paid again twice a second for as long as a pinned fetch runs.
pub fn size() -> (u16, u16) {
    // Whichever of our own streams is still a terminal knows the size already.
    for fd in [libc::STDOUT_FILENO, libc::STDERR_FILENO, libc::STDIN_FILENO] {
        if let Some(size) = winsize(fd) {
            return size;
        }
    }

    // All three are redirected, but a controlling terminal may still exist and
    // its width is the right one to lay out against. Only reached when the
    // cheap path found nothing, so the open costs nothing in the normal case.
    if let Ok(tty) = std::fs::File::open("/dev/tty")
        && let Some(size) = winsize(tty.as_raw_fd())
    {
        return size;
    }

    (100, 40)
}

/// `(columns, rows)` for a file descriptor, or `None` if it is not a terminal.
fn winsize(fd: RawFd) -> Option<(u16, u16)> {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };

    // SAFETY: TIOCGWINSZ writes a `winsize` through the pointer and nothing
    // else, `ws` is exactly that and is writable, and failure is reported by
    // the return value rather than by leaving `ws` in some invalid state.
    if unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &raw mut ws) } != 0 {
        return None;
    }

    // A terminal that reports zero is one we cannot lay out against; treat it
    // the same as no terminal at all and let the caller fall back.
    (ws.ws_col > 0 && ws.ws_row > 0).then_some((ws.ws_col, ws.ws_row))
}
