//! Starting programs. The only place animfetch does, so the rule that nothing
//! can hold up a new shell is kept here: no stdin, no stderr, a deadline, and
//! the whole process group killed when it passes.

use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Most bytes of a program's output that are read.
const MAX_OUTPUT: usize = 4096;
/// Most characters of a configured command's line that are shown.
const MAX_CHARS: usize = 200;

/// What a program printed, and whether it said it succeeded.
pub struct Captured {
    pub text: String,
    pub success: bool,
}

/// Run `cmd` and return the first `cap` bytes it printed, giving up after
/// `within`. `None` when it cannot be started or does not finish in time.
///
/// A program that prints more than `cap` is not waited for: the pipe is closed
/// on it, which ends one that is only writing.
pub fn capture(mut cmd: Command, within: Duration, cap: usize) -> Option<Captured> {
    use std::io::Read as _;
    use std::os::unix::process::CommandExt as _;

    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        // Its own group, so that a shell's children go when it does.
        .process_group(0)
        .spawn()
        .ok()?;

    let Some(stdout) = child.stdout.take() else {
        give_up(&mut child);
        return None;
    };
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.take(cap as u64).read_to_end(&mut bytes);
        let _ = tx.send(bytes);
    });

    let deadline = Instant::now() + within;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                give_up(&mut child);
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(2)),
            Err(_) => {
                give_up(&mut child);
                return None;
            }
        }
    };

    // The program has exited; its output has only to be collected. Something
    // it left running may still hold the pipe, so this waits no longer than
    // the deadline either.
    let left = deadline.saturating_duration_since(Instant::now());
    let bytes = rx.recv_timeout(left).ok()?;
    Some(Captured {
        text: String::from_utf8_lossy(&bytes).into_owned(),
        success: status.success(),
    })
}

fn give_up(child: &mut std::process::Child) {
    // SAFETY: kill(2) with a process group id we created and a signal number.
    // A group that has already gone is an error nobody needs to hear about.
    unsafe {
        libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// The first line a shell command prints, made safe to draw, or `None` when it
/// prints nothing or runs out of time.
///
/// The exit status is not asked about: a command that printed what you wanted
/// and then complained is still the answer you wrote it to give.
pub fn shell(line: &str, within: Duration) -> Option<String> {
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(line);
    first_line(&capture(cmd, within, MAX_OUTPUT)?.text)
}

/// Every command, started together, answered in the order they were given.
/// The wait is the slowest one, not the sum.
pub fn run_all(lines: &[&str], within: Duration) -> Vec<Option<String>> {
    if lines.is_empty() {
        return Vec::new();
    }
    std::thread::scope(|scope| {
        let started: Vec<_> = lines
            .iter()
            .map(|line| scope.spawn(move || shell(line, within)))
            .collect();
        started
            .into_iter()
            .map(|handle| handle.join().ok().flatten())
            .collect()
    })
}

/// The first line with something in it, without the control characters that
/// would move the cursor or recolour what follows, and a sane length.
fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(|line| {
            line.chars()
                .map(|c| if c == '\t' { ' ' } else { c })
                .filter(|c| !c.is_control())
                .collect::<String>()
        })
        .map(|line| line.trim().to_string())
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(MAX_CHARS).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const WAIT: Duration = Duration::from_secs(2);

    #[test]
    fn the_first_line_with_something_in_it_is_the_answer() {
        assert_eq!(
            shell("printf '\\n  \\nfirst\\nsecond\\n'", WAIT),
            Some("first".into())
        );
        assert_eq!(shell("echo hello world", WAIT), Some("hello world".into()));
    }

    #[test]
    fn nothing_printed_is_no_answer() {
        assert_eq!(shell("true", WAIT), None);
        assert_eq!(shell("printf '   \\n'", WAIT), None);
    }

    #[test]
    fn output_is_kept_even_when_the_command_complains() {
        assert_eq!(shell("echo fine; exit 3", WAIT), Some("fine".into()));
        assert_eq!(shell("exit 3", WAIT), None);
    }

    #[test]
    fn escapes_and_control_characters_cannot_reach_the_screen() {
        // An escape sequence in a value would recolour or move the pane.
        let out = shell("printf 'a\\033[31mred\\033[0m\\tb\\r'", WAIT).unwrap();
        assert!(!out.contains('\u{1b}') && !out.contains('\r'), "{out:?}");
        assert_eq!(out, "a[31mred[0m b");
    }

    #[test]
    fn a_long_line_is_cut() {
        let out = shell("head -c 900 /dev/zero | tr '\\0' x", WAIT).unwrap();
        assert_eq!(out.chars().count(), MAX_CHARS);
    }

    #[test]
    fn a_command_that_never_ends_is_given_up_on_and_its_children_with_it() {
        let start = Instant::now();
        assert_eq!(
            shell("sleep 5; echo late", Duration::from_millis(300)),
            None
        );
        assert!(start.elapsed() < Duration::from_secs(2), "waited too long");
    }

    #[test]
    fn a_command_that_will_not_stop_printing_is_not_waited_for() {
        let start = Instant::now();
        assert_eq!(shell("yes", WAIT), Some("y".into()));
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn commands_run_together_and_come_back_in_order() {
        let start = Instant::now();
        let out = run_all(&["sleep 1; echo a", "sleep 1; echo b", "true"], WAIT);

        assert_eq!(out, [Some("a".into()), Some("b".into()), None]);
        assert!(
            start.elapsed() < Duration::from_millis(1800),
            "they ran one after another: {:?}",
            start.elapsed()
        );
        assert!(run_all(&[], WAIT).is_empty());
    }

    #[test]
    fn its_standard_error_is_not_shown() {
        assert_eq!(shell("echo oops >&2", WAIT), None);
    }
}
