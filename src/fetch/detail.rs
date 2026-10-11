//! The fuller form of a row's value, for the styles that have room for it.
//!
//! Not free: a version of the shell or window manager means running it, which
//! is a few milliseconds the classic style has never paid. See [`run_capture`].

use std::time::{Duration, Instant};

use super::common::{env, is_shell};
use super::{Detail, Item};

/// Give a row its fuller value, where it has one.
pub(super) fn enrich(item: &mut Item) {
    let text = match item.label {
        "Kernel" => sysname().map(|os| kernel_detail(&os, &item.value)),
        "Terminal" => terminal_detail(&item.value),
        "Shell" => shell_detail(&item.value),
        "WM" => wm_detail(&item.value),
        "CPU" => cpu_detail(&item.value),
        _ => None,
    };
    if let Some(text) = text.filter(|t| *t != item.value) {
        item.detail = Some(Detail::Text(text));
    }
}

/// `Intel(R) Core(TM) i7-8650U`: the model alone, without the core count and
/// clock speed that follow it in the classic text.
pub fn cpu_model(value: &str) -> &str {
    let head = value.split(" @ ").next().unwrap_or(value);
    match head.rfind(" (") {
        Some(i) if head.ends_with(')') => {
            let count = &head[i + 2..head.len() - 1];
            if !count.is_empty() && count.bytes().all(|b| b.is_ascii_digit()) {
                &head[..i]
            } else {
                head
            }
        }
        _ => head,
    }
}

/// `Intel i3-4000M @ 2.40GHz`: the model as short as it can be said, with the
/// clock speed but not the core count.
pub fn cpu_short(value: &str) -> String {
    let mut short = cpu_brand(value);
    if let Some((_, clock)) = value.split_once(" @ ") {
        short.push_str(" @ ");
        short.push_str(clock);
    }
    short
}

/// `AMD Ryzen 7 5700X3D`: the model alone, as short as it can be said. The
/// trademark marks, the word `CPU` and the `8-Core Processor` AMD appends are
/// noise at this length.
pub fn cpu_brand(value: &str) -> String {
    let model = cpu_model(value).replace("(R)", "").replace("(TM)", "");

    // `... 8-Core Processor`: a count, a word, and "Processor". Taken off
    // whole, before `Core` is dropped as a word, or it is no longer a suffix.
    let words: Vec<&str> = model.split_whitespace().collect();
    let words = match words.as_slice() {
        [rest @ .., cores, "Processor"] if cores.ends_with("-Core") => rest,
        all => all,
    };
    words
        .iter()
        .filter(|w| !matches!(**w, "CPU" | "Core"))
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A graphics card's name without the `[Discrete]` or `[Integrated]` tag, and
/// without the `Integrated Graphics Controller` the PCI database adds to an
/// Intel part, which says nothing the name did not.
pub fn gpu_model(value: &str) -> String {
    value
        .replace(" [Discrete]", "")
        .replace(" [Integrated]", "")
        .replace(" Integrated Graphics Controller", "")
}

/// `Linux 7.0.14-arch1-1`: the kernel's name in front of its release.
fn kernel_detail(sysname: &str, release: &str) -> String {
    if release.starts_with(sysname) {
        release.to_string()
    } else {
        format!("{sysname} {release}")
    }
}

/// `ghostty 1.3.1-arch2`, from the version the terminal puts in its
/// environment. Only trusted when the name came from the same place; a name
/// found by walking the process tree belongs to a program that may not set it.
fn terminal_detail(name: &str) -> Option<String> {
    if env("TERM_PROGRAM").as_deref() != Some(name) {
        return None;
    }
    Some(format!("{name} {}", env("TERM_PROGRAM_VERSION")?))
}

/// `zsh 5.9`, by asking the shell. Only a name `is_shell` vouches for is run,
/// so `$SHELL` pointing at something unexpected is never executed.
fn shell_detail(name: &str) -> Option<String> {
    if !is_shell(name) {
        return None;
    }
    let out = run_capture(&env("SHELL")?, &["--version"])?;
    Some(format!("{name} {}", parse_version(&out)?))
}

/// `Hyprland 0.55.4 (Wayland)`: the compositor's version and what kind of
/// session it is running.
fn wm_detail(value: &str) -> Option<String> {
    // "GNOME (Wayland)" already says what session it is; keep that part.
    let (name, session) = match value.split_once(" (") {
        Some((name, rest)) => (name, rest.strip_suffix(')').map(str::to_string)),
        None => (value, None),
    };

    let output = match name {
        "Hyprland" => run_capture("hyprctl", &["version"]),
        "sway" => run_capture("sway", &["--version"]),
        _ => None,
    };
    let version = output.as_deref().and_then(parse_version);

    let session = session.or_else(|| match env("XDG_SESSION_TYPE").as_deref() {
        Some("wayland") => Some("Wayland".into()),
        Some("x11") => Some("X11".into()),
        _ => None,
    });

    let mut text = name.to_string();
    if let Some(version) = version {
        text.push(' ');
        text.push_str(&version);
    }
    if let Some(session) = session {
        text.push_str(&format!(" ({session})"));
    }
    Some(text)
}

/// `@ 4.21 GHz`, not `@ 4.21GHz`.
fn cpu_detail(value: &str) -> Option<String> {
    let rest = value.strip_suffix("GHz")?;
    rest.ends_with(|c: char| c.is_ascii_digit())
        .then(|| format!("{rest} GHz"))
}

/// The first version number in the first line of `--version` output. Programs
/// disagree on everything around it (`zsh 5.9 (...)`, `GNU bash, version
/// 5.3.15(1)-release`, `fish, version 3.7.1`), so it looks for the shape
/// instead: a token starting with a digit and containing a dot.
fn parse_version(text: &str) -> Option<String> {
    let line = text.lines().next()?;
    line.split(|c: char| c.is_whitespace() || c == ',')
        .map(|t| t.trim_start_matches('v'))
        .filter(|t| t.starts_with(|c: char| c.is_ascii_digit()) && t.contains('.'))
        .map(|t| {
            t.chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect::<String>()
        })
        .map(|v| v.trim_end_matches('.').to_string())
        .find(|v| !v.is_empty())
}

/// Run a program and return what it printed, giving up after a moment. This is
/// the only place animfetch starts one, so the rule that nothing can hold up a
/// new shell is kept here: no stdin, no stderr, and a short deadline.
fn run_capture(program: &str, args: &[&str]) -> Option<String> {
    use std::io::Read as _;
    use std::process::{Command, Stdio};

    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let deadline = Instant::now() + Duration::from_millis(300);
    loop {
        match child.try_wait().ok()? {
            Some(status) if status.success() => break,
            Some(_) => return None,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(2)),
        }
    }

    let mut out = String::new();
    child
        .stdout
        .take()?
        .take(4096)
        .read_to_string(&mut out)
        .ok()?;
    Some(out)
}

/// `Linux`, `Darwin`: what `uname -s` prints.
fn sysname() -> Option<String> {
    // SAFETY: utsname is plain bytes, and uname fills it or fails.
    let mut name: libc::utsname = unsafe { std::mem::zeroed() };
    if unsafe { libc::uname(&mut name) } != 0 {
        return None;
    }
    let bytes: Vec<u8> = name
        .sysname
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8(bytes).ok().filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(label: &'static str, value: &str) -> Item {
        Item::new(label, value)
    }

    #[test]
    fn versions_are_found_in_whatever_shape_programs_print_them() {
        assert_eq!(
            parse_version("zsh 5.9 (x86_64-pc-linux-gnu)"),
            Some("5.9".into())
        );
        assert_eq!(
            parse_version("GNU bash, version 5.3.15(1)-release (x86_64-pc-linux-gnu)\nCopyright"),
            Some("5.3.15".into())
        );
        assert_eq!(parse_version("fish, version 3.7.1"), Some("3.7.1".into()));
        assert_eq!(
            parse_version("Hyprland 0.56.2 built from branch v0.56.2 at commit abc"),
            Some("0.56.2".into())
        );
        assert_eq!(parse_version("sway version 1.10"), Some("1.10".into()));
        assert_eq!(parse_version("v1.2.3"), Some("1.2.3".into()));
    }

    #[test]
    fn output_without_a_version_gives_none_rather_than_a_guess() {
        assert_eq!(parse_version(""), None);
        assert_eq!(parse_version("dash: illegal option --version"), None);
        // No dot, so a year or a build number is not mistaken for one.
        assert_eq!(parse_version("ksh 93u+ 2012-08-01"), None);
    }

    #[test]
    fn only_the_first_line_is_searched() {
        // A later line mentioning a library version must not be picked up.
        assert_eq!(parse_version("tool\nlinked against zlib 1.3.1"), None);
    }

    #[test]
    fn the_kernel_gets_its_name_once() {
        assert_eq!(
            kernel_detail("Linux", "7.0.14-arch1-1"),
            "Linux 7.0.14-arch1-1"
        );
        assert_eq!(kernel_detail("Linux", "Linux 7.0"), "Linux 7.0");
    }

    #[test]
    fn the_clock_speed_gets_a_space_before_its_unit() {
        assert_eq!(
            cpu_detail("AMD Ryzen 5 3600 (12) @ 4.21GHz"),
            Some("AMD Ryzen 5 3600 (12) @ 4.21 GHz".into())
        );
        assert_eq!(cpu_detail("Apple M1 (8)"), None);
        assert_eq!(cpu_detail("a GHz"), None, "needs a number in front");
    }

    #[test]
    fn a_window_manager_without_a_known_version_keeps_its_name() {
        // Not Hyprland or sway, so nothing is run and nothing is invented.
        assert_eq!(wm_detail("GNOME (Wayland)"), Some("GNOME (Wayland)".into()));
    }

    #[test]
    fn enrichment_leaves_a_row_alone_when_it_has_nothing_to_add() {
        let mut row = item("OS", "Arch Linux x86_64");
        enrich(&mut row);
        assert!(row.detail.is_none());

        // And the classic text is never rewritten, only given a companion.
        let mut cpu = item("CPU", "Ryzen (12) @ 4.21GHz");
        enrich(&mut cpu);
        assert_eq!(cpu.value, "Ryzen (12) @ 4.21GHz");
        assert!(matches!(cpu.detail, Some(Detail::Text(t)) if t.ends_with("4.21 GHz")));
    }

    #[test]
    fn a_cpu_model_loses_its_core_count_and_clock() {
        assert_eq!(
            cpu_model("Intel(R) Core(TM) i7-8650U (8) @ 4.20GHz"),
            "Intel(R) Core(TM) i7-8650U"
        );
        assert_eq!(cpu_model("Apple M1 (8)"), "Apple M1");
        assert_eq!(cpu_model("Apple M1"), "Apple M1");
        // A parenthesis that is part of the name stays.
        assert_eq!(cpu_model("Intel(R) Core(TM) i7"), "Intel(R) Core(TM) i7");
        assert_eq!(cpu_model("Foo (rev B) (4)"), "Foo (rev B)");
    }

    #[test]
    fn a_short_cpu_name_keeps_the_clock_and_drops_the_noise() {
        assert_eq!(
            cpu_short("Intel(R) Core(TM) i3-4000M CPU (4) @ 2.40GHz"),
            "Intel i3-4000M @ 2.40GHz"
        );
        assert_eq!(
            cpu_short("AMD Ryzen 7 7700 8-Core Processor (16) @ 5.39GHz"),
            "AMD Ryzen 7 7700 @ 5.39GHz"
        );
        assert_eq!(cpu_short("Apple M1 (8)"), "Apple M1", "no clock, no suffix");
        // A model whose name is not decoration is left whole.
        assert_eq!(
            cpu_short("AMD EPYC 7763 64-Core Processor"),
            "AMD EPYC 7763"
        );
    }

    #[test]
    fn a_brand_is_the_short_name_without_the_clock() {
        assert_eq!(
            cpu_brand("AMD Ryzen 7 5700X3D 8-Core Processor (16) @ 4.10GHz"),
            "AMD Ryzen 7 5700X3D"
        );
        assert_eq!(cpu_brand("Apple M1 (8)"), "Apple M1");
    }

    #[test]
    fn an_intel_part_loses_its_database_suffix() {
        assert_eq!(
            gpu_model("Intel 4th Gen Core Processor Integrated Graphics Controller [Integrated]"),
            "Intel 4th Gen Core Processor"
        );
    }

    #[test]
    fn a_gpu_name_loses_only_its_kind_tag() {
        assert_eq!(
            gpu_model("Intel UHD Graphics 620 [Integrated]"),
            "Intel UHD Graphics 620"
        );
        assert_eq!(
            gpu_model("AMD RX 7600 XT [Discrete], Intel Iris [Integrated]"),
            "AMD RX 7600 XT, Intel Iris"
        );
        assert_eq!(
            gpu_model("AMD [7480]"),
            "AMD [7480]",
            "a bracketed id is not a tag"
        );
    }

    #[test]
    fn a_program_that_hangs_is_given_up_on() {
        let start = Instant::now();
        assert_eq!(run_capture("sleep", &["5"]), None);
        assert!(start.elapsed() < Duration::from_secs(2), "waited too long");
    }

    #[test]
    fn a_missing_program_is_just_a_missing_version() {
        assert_eq!(run_capture("animfetch-no-such-program", &[]), None);
    }
}
