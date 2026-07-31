//! Raw mode and the scroll region: the two bits of terminal state we own.
//!
//! Both outlive the process if we don't undo them, so every exit path restores.
//! `Drop` handles returns, the panic hook handles panics.

use std::io::{self, Write};
use std::os::fd::{AsRawFd, RawFd};

use crossterm::terminal;

/// Restores raw mode and the scroll region when it goes out of scope.
pub struct Guard {
    restored: bool,
}

impl Guard {
    /// Enter raw mode and hide the cursor. Stays on the main screen so command
    /// output lands in your scrollback.
    pub fn acquire() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        write!(io::stdout(), "{HIDE_CURSOR}")?;
        io::stdout().flush()?;

        install_panic_hook();
        Ok(Self { restored: false })
    }

    /// Hand the terminal to a child: cooked mode, visible cursor. The scroll
    /// region stays set, which is what keeps the child's output below the fetch.
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

    /// Undo everything. Idempotent, so `Drop` after an explicit call is fine.
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

/// Without this a panic dumps its backtrace into a terminal still in raw mode.
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

/// DECOM: row addressing becomes relative to the scroll region. Without it,
/// `clear` homes to screen row 1 and paints the shell's prompt over the art.
pub const SET_ORIGIN_MODE: &str = "\x1b[?6h";
pub const RESET_ORIGIN_MODE: &str = "\x1b[?6l";

/// Home the cursor. Under origin mode this is the scroll region's top-left.
pub const HOME: &str = "\x1b[H";
pub const CLEAR_SCREEN: &str = "\x1b[2J";

/// Confine scrolling to `top..=bottom`, 1-based. Homes the cursor as a side
/// effect, so callers must reposition.
pub fn scroll_region(top: u16, bottom: u16) -> String {
    format!("\x1b[{top};{bottom}r")
}

/// Move the cursor to a 1-based row and column.
pub fn move_to(row: u16, col: u16) -> String {
    format!("\x1b[{row};{col}H")
}

/// Terminal size in cells, falling back to something usable when piped.
///
/// Direct ioctl, not crossterm: that one opens `/dev/tty` every call and can
/// shell out to `tput`, costing about a millisecond.
pub fn size() -> (u16, u16) {
    // Whichever of our own streams is still a terminal knows the size already.
    for fd in [libc::STDOUT_FILENO, libc::STDERR_FILENO, libc::STDIN_FILENO] {
        if let Some(size) = winsize(fd) {
            return size;
        }
    }

    // All redirected, but a controlling terminal may still exist.
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

    // SAFETY: TIOCGWINSZ only writes a `winsize`, which is what `ws` is.
    if unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &raw mut ws) } != 0 {
        return None;
    }

    // Zero is unusable, so treat it as no terminal at all.
    (ws.ws_col > 0 && ws.ws_row > 0).then_some((ws.ws_col, ws.ws_row))
}
