//! The inline command prompt: the security-relevant part of the program.
//!
//! It draws something that looks like a shell prompt and hands what you type to
//! a real shell. Kept in one short file so you can read it end to end. No config
//! value and no animation file reaches `run`; only your keystrokes do.

use std::ffi::OsString;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitStatus};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::color::{RESET, Rgb};
use crate::config::Config;

/// Cap on input length. Long enough for any real command, short enough that a
/// stuck key cannot grow the buffer without bound.
const MAX_INPUT: usize = 4096;

/// What the caller should do after a keypress.
pub enum Action {
    /// Keep animating.
    Continue,
    /// Leave the animation and run this command.
    Submit(String),
    /// Leave the animation without running anything.
    Quit,
}

pub struct Prompt {
    /// Styled prefix, rebuilt only on resize.
    prefix: String,
    buffer: String,
}

impl Prompt {
    pub fn new(cfg: &Config) -> Self {
        let paint = |color: Rgb, text: &str| {
            if !cfg.color {
                return text.to_string();
            }
            let mut out = String::new();
            color.fg(&mut out);
            out.push_str(text);
            out.push_str(RESET);
            out
        };

        let prefix = format!(
            "{} {}{}",
            paint(cfg.accent.0, &crate::fetch::title()),
            paint(cfg.value.0, &cwd_display()),
            if is_root() { " # " } else { " $ " },
        );

        Self { prefix, buffer: String::new() }
    }

    /// The prompt line as it should appear this frame.
    pub fn render(&self) -> String {
        // A drawn block stands in for the real cursor, which stays hidden so it
        // cannot flicker across the art as each frame is painted.
        format!("{}{}█", self.prefix, self.buffer)
    }

    /// The prefix alone. Used to echo a submitted command into scrollback,
    /// without the block cursor, so it reads as a line you typed.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Rebuild the prefix after the working directory changes.
    pub fn refresh(&mut self, cfg: &Config) {
        *self = Self::new(cfg);
    }

    pub fn handle(&mut self, key: KeyEvent) -> Action {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        match key.code {
            KeyCode::Enter => {
                let command = self.buffer.trim().to_string();
                // Bare Enter at a shell prompt just gives you a new one.
                let action = if command.is_empty() {
                    Action::Continue
                } else {
                    Action::Submit(command)
                };
                self.buffer.clear();
                action
            }
            KeyCode::Esc => Action::Quit,

            // Raw mode disables the kernel's signal keys, so the usual control
            // characters have to be honoured by hand.
            KeyCode::Char('c') if ctrl => Action::Quit,
            KeyCode::Char('d') if ctrl && self.buffer.is_empty() => Action::Quit,
            KeyCode::Char('u') if ctrl => {
                self.buffer.clear();
                Action::Continue
            }
            KeyCode::Char('w') if ctrl => {
                let trimmed = self.buffer.trim_end();
                let cut = trimmed.rfind(char::is_whitespace).map_or(0, |i| i + 1);
                self.buffer.truncate(cut);
                Action::Continue
            }

            KeyCode::Backspace => {
                self.buffer.pop();
                Action::Continue
            }
            KeyCode::Char(c) if !ctrl && self.buffer.len() < MAX_INPUT => {
                self.buffer.push(c);
                Action::Continue
            }
            _ => Action::Continue,
        }
    }
}

/// Commands that cannot be delegated to a child process.
pub enum Builtin {
    /// A directory change was attempted; `Err` carries a message to show.
    Cd(Result<(), String>),
    /// Leave animfetch.
    Exit,
}

/// Recognise the builtins, or `None` to hand the command to the shell. `cd` in
/// a child does nothing observable, so every shell wrapper implements it.
pub fn builtin(command: &str) -> Option<Builtin> {
    // Anything with shell syntax in it belongs to the shell, even if it starts
    // with `cd`. `cd /tmp && ls` is not something we can interpret, and getting
    // it half-right would be worse than passing it on intact.
    if command.contains(['&', '|', ';', '>', '<', '$', '`', '(']) {
        return None;
    }

    match command.split_whitespace().next()? {
        "exit" | "logout" => Some(Builtin::Exit),
        "cd" => {
            let arg = command["cd".len()..].trim().trim_matches(['"', '\'']);
            Some(Builtin::Cd(change_dir(arg)))
        }
        _ => None,
    }
}

fn change_dir(arg: &str) -> Result<(), String> {
    let home = std::env::var_os("HOME").filter(|h| !h.is_empty());

    let target: std::path::PathBuf = match arg {
        "" | "~" => home.ok_or("cd: HOME is not set")?.into(),
        // `cd -` returns to the previous directory, as in any shell.
        "-" => std::env::var_os("OLDPWD")
            .filter(|p| !p.is_empty())
            .ok_or("cd: OLDPWD is not set")?
            .into(),
        path => match path.strip_prefix("~/") {
            Some(rest) => std::path::Path::new(&home.ok_or("cd: HOME is not set")?).join(rest),
            None => path.into(),
        },
    };

    let previous = std::env::current_dir().ok();
    std::env::set_current_dir(&target)
        .map_err(|e| format!("cd: {}: {e}", target.display()))?;

    // SAFETY: `set_var` is only unsound with another thread reading the
    // environment. We are single-threaded, and this runs between children.
    unsafe {
        if let Some(previous) = previous {
            std::env::set_var("OLDPWD", previous);
        }
        if let Ok(new) = std::env::current_dir() {
            std::env::set_var("PWD", new);
        }
    }
    Ok(())
}

/// Hand `command` to the user's shell, inheriting all three streams. The caller
/// must have restored cooked mode first.
pub fn run(command: &str) -> io::Result<ExitStatus> {
    let shell = std::env::var_os("SHELL").unwrap_or_else(|| OsString::from("/bin/sh"));

    // Plain `-c`, not `-l` or `-i`: those source startup files, and startup
    // files print things. We inherit the environment anyway. The cost is that
    // aliases aren't expanded.
    let mut builder = Command::new(&shell);
    builder.arg("-c").arg(command);

    unsafe {
        // SAFETY: `setpgid` is async-signal-safe and touches nothing owned by
        // the Rust runtime, which is all `pre_exec` permits between fork and
        // exec.
        builder.pre_exec(|| {
            // Put the child in its own process group so it, not us, is what
            // Ctrl-C interrupts.
            if libc::setpgid(0, 0) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let mut child = builder.spawn()?;
    let pgid = child.id() as libc::pid_t;

    // Also set the group from this side: whichever of the two runs first wins,
    // which closes the window where the child has already exec'd.
    unsafe { libc::setpgid(pgid, pgid) };

    let previous = foreground_group();
    set_foreground_group(pgid);
    let status = child.wait();
    // Take the terminal back before reporting anything, even on error.
    if let Some(previous) = previous {
        set_foreground_group(previous);
    }

    status
}

/// The terminal's current foreground process group, if we have a terminal.
fn foreground_group() -> Option<libc::pid_t> {
    // SAFETY: tcgetpgrp only reads, and reports failure through its return.
    let pgid = unsafe { libc::tcgetpgrp(libc::STDIN_FILENO) };
    (pgid >= 0).then_some(pgid)
}

/// Hand the terminal to `pgid`, so signals from the keyboard reach it.
fn set_foreground_group(pgid: libc::pid_t) {
    // We get SIGTTOU for this while not in the foreground group. Every shell
    // does the same dance.
    unsafe {
        let previous = libc::signal(libc::SIGTTOU, libc::SIG_IGN);
        libc::tcsetpgrp(libc::STDIN_FILENO, pgid);
        libc::signal(libc::SIGTTOU, previous);
    }
}

/// `~`-abbreviated working directory.
fn cwd_display() -> String {
    let Ok(cwd) = std::env::current_dir() else {
        return ".".into();
    };

    if let Some(home) = std::env::var_os("HOME").filter(|h| !h.is_empty())
        && let Ok(rest) = cwd.strip_prefix(&home)
    {
        return if rest.as_os_str().is_empty() {
            "~".into()
        } else {
            format!("~/{}", rest.display())
        };
    }

    cwd.display().to_string()
}

fn is_root() -> bool {
    // SAFETY: getuid is always safe; it takes no arguments and cannot fail.
    unsafe { libc::getuid() == 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn prompt() -> Prompt {
        Prompt { prefix: String::new(), buffer: String::new() }
    }

    fn type_str(p: &mut Prompt, s: &str) {
        for c in s.chars() {
            p.handle(key(KeyCode::Char(c)));
        }
    }

    #[test]
    fn typing_then_enter_submits_the_trimmed_command() {
        let mut p = prompt();
        type_str(&mut p, "  ls -la  ");
        match p.handle(key(KeyCode::Enter)) {
            Action::Submit(cmd) => assert_eq!(cmd, "ls -la"),
            _ => panic!("expected submit"),
        }
    }

    #[test]
    fn bare_enter_does_not_submit() {
        let mut p = prompt();
        type_str(&mut p, "   ");
        assert!(matches!(p.handle(key(KeyCode::Enter)), Action::Continue));
        assert!(p.buffer.is_empty());
    }

    #[test]
    fn ctrl_c_quits_and_ctrl_d_only_quits_when_empty() {
        let mut p = prompt();
        assert!(matches!(p.handle(ctrl('c')), Action::Quit));

        type_str(&mut p, "x");
        assert!(matches!(p.handle(ctrl('d')), Action::Continue));
        p.handle(key(KeyCode::Backspace));
        assert!(matches!(p.handle(ctrl('d')), Action::Quit));
    }

    #[test]
    fn ctrl_u_clears_and_ctrl_w_deletes_a_word() {
        let mut p = prompt();
        type_str(&mut p, "git commit -m");
        p.handle(ctrl('w'));
        assert_eq!(p.buffer, "git commit ");

        p.handle(ctrl('u'));
        assert_eq!(p.buffer, "");
        // Ctrl-W on an empty buffer must not panic or underflow.
        p.handle(ctrl('w'));
        assert_eq!(p.buffer, "");
    }

    #[test]
    fn control_chars_are_not_inserted_as_text() {
        let mut p = prompt();
        p.handle(ctrl('a'));
        assert_eq!(p.buffer, "");
    }

    #[test]
    fn input_is_bounded() {
        let mut p = prompt();
        p.buffer = "x".repeat(MAX_INPUT);
        p.handle(key(KeyCode::Char('y')));
        assert_eq!(p.buffer.len(), MAX_INPUT);
    }

    #[test]
    fn backspace_removes_whole_multibyte_chars() {
        let mut p = prompt();
        type_str(&mut p, "café");
        p.handle(key(KeyCode::Backspace));
        assert_eq!(p.buffer, "caf");
    }
}
