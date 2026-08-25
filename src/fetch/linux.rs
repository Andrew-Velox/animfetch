//! Linux system information, from `/proc`, `/sys` and environment variables.
//! Never subprocesses, which is why this runs in about a millisecond.
//!
//! Everything that knows where a number comes from lives here. Adding a
//! platform means writing a sibling of this file with the same `collect` and
//! `hostname`; nothing above cares.

use std::fs;
use std::path::Path;

use super::{
    Item, arch, basename_of_env, env, format_usage, humanize_duration, is_shell, swatches,
};
use crate::config::Module;

/// Collect the requested modules, skipping ones this machine can't answer.
pub fn collect(modules: &[Module]) -> Vec<Item> {
    let mut items = Vec::with_capacity(modules.len());

    // Memory and Swap read the same file. Do it once, and only if asked.
    let meminfo = modules
        .iter()
        .any(|m| matches!(m, Module::Memory | Module::Swap))
        .then(|| fs::read_to_string("/proc/meminfo").ok())
        .flatten();

    for &module in modules {
        let (label, value) = match module {
            Module::Os => ("OS", os()),
            Module::Host => ("Host", host()),
            Module::Kernel => ("Kernel", read_trimmed("/proc/sys/kernel/osrelease")),
            Module::Uptime => ("Uptime", uptime()),
            Module::Packages => ("Packages", packages()),
            Module::Shell => ("Shell", basename_of_env("SHELL")),
            Module::Wm => ("WM", wm()),
            Module::Terminal => ("Terminal", terminal()),
            Module::Cpu => ("CPU", cpu()),
            Module::Memory => ("Memory", meminfo.as_deref().and_then(memory)),
            Module::Swap => ("Swap", meminfo.as_deref().and_then(swap)),
            Module::Disk => ("Disk", disk("/")),
            Module::Colors => ("", Some(swatches())),
        };

        if let Some(value) = value {
            items.push(Item { label, value });
        }
    }

    items
}

fn os() -> Option<String> {
    let release = fs::read_to_string("/etc/os-release").ok()?;
    let pretty =
        os_release_field(&release, "PRETTY_NAME").or_else(|| os_release_field(&release, "NAME"))?;

    // `uname -m` without the subprocess.
    match arch() {
        Some(arch) => Some(format!("{pretty} {arch}")),
        None => Some(pretty),
    }
}

fn host() -> Option<String> {
    let dmi = Path::new("/sys/devices/virtual/dmi/id");
    let name = read_trimmed(dmi.join("product_name"))?;

    // Laptops put the useful string here; desktops leave filler.
    match read_trimmed(dmi.join("product_version")).filter(|v| is_meaningful(v)) {
        Some(version) => Some(format!("{name} {version}")),
        None => Some(name),
    }
    .filter(|v| is_meaningful(v))
}

fn uptime() -> Option<String> {
    let raw = fs::read_to_string("/proc/uptime").ok()?;
    let secs: f64 = raw.split_whitespace().next()?.parse().ok()?;
    Some(humanize_duration(secs as u64))
}

fn packages() -> Option<String> {
    // Arch: one directory per installed package.
    if let Ok(entries) = fs::read_dir("/var/lib/pacman/local") {
        let n = entries.filter_map(Result::ok).filter(is_dir).count();
        if n > 0 {
            return Some(format!("{n} (pacman)"));
        }
    }

    // Debian: one stanza per package in the status file.
    if let Ok(status) = fs::read_to_string("/var/lib/dpkg/status") {
        let n = status
            .lines()
            .filter(|l| l.starts_with("Package: "))
            .count();
        if n > 0 {
            return Some(format!("{n} (dpkg)"));
        }
    }

    None
}

fn wm() -> Option<String> {
    // More reliable than the generic env vars, which go stale.
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
        return Some("Hyprland".into());
    }
    if std::env::var_os("SWAYSOCK").is_some() {
        return Some("sway".into());
    }

    let name = env("XDG_CURRENT_DESKTOP")
        .or_else(|| env("XDG_SESSION_DESKTOP"))
        .or_else(|| env("DESKTOP_SESSION"))?;

    // Colon-separated, may carry a prefix: "wlroots:Hyprland".
    let name = name.rsplit(':').next().unwrap_or(&name).to_string();

    let session = match env("XDG_SESSION_TYPE").as_deref() {
        Some("wayland") => " (Wayland)",
        Some("x11") => " (X11)",
        _ => "",
    };
    Some(format!("{name}{session}"))
}

fn terminal() -> Option<String> {
    if let Some(prog) = env("TERM_PROGRAM") {
        return Some(prog);
    }

    // First ancestor that isn't a shell is the terminal that spawned us.
    let mut pid = parent_pid(std::process::id())?;
    for _ in 0..8 {
        let name = comm(pid)?;
        if !is_shell(&name) {
            return Some(name);
        }
        pid = parent_pid(pid)?;
        if pid <= 1 {
            break;
        }
    }

    env("TERM")
}

fn cpu() -> Option<String> {
    // cpuinfo repeats a block per core, so it is tens of kilobytes the kernel
    // formats as we read. The model name is in the first block, and the core
    // count lives elsewhere, so neither needs the rest.
    let head = online_cpus().zip(read_prefix("/proc/cpuinfo", 4096));
    let (model, cores) = match head {
        Some((cores, head)) => match model_name(&head) {
            Some(model) => (model, cores),
            // First block ran past the prefix; better slow than missing.
            None => whole_cpuinfo()?,
        },
        // Nothing to count online CPUs from, so the file is needed anyway.
        None => whole_cpuinfo()?,
    };

    // Nominal max clock, in kHz. Absent on VMs and some ARM boards.
    let ghz = read_trimmed("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq")
        .and_then(|v| v.parse::<f64>().ok())
        .map(|khz| format!(" @ {:.2}GHz", khz / 1_000_000.0))
        .unwrap_or_default();

    Some(format!("{model} ({cores}){ghz}"))
}

/// The slow path: whole file, counting `processor` lines.
fn whole_cpuinfo() -> Option<(String, usize)> {
    let all = fs::read_to_string("/proc/cpuinfo").ok()?;
    let cores = all.lines().filter(|l| l.starts_with("processor")).count();
    Some((model_name(&all)?, cores))
}

fn model_name(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix("model name").and_then(|r| r.split_once(':')))
        .map(|(_, v)| v.trim().to_string())
}

fn memory(info: &str) -> Option<String> {
    let total = meminfo_field(info, "MemTotal")?;

    // MemAvailable counts reclaimable cache; MemFree overstates usage.
    let available = meminfo_field(info, "MemAvailable")?;
    Some(format_usage(total - available, total))
}

fn swap(info: &str) -> Option<String> {
    let total = meminfo_field(info, "SwapTotal")?;
    if total == 0 {
        return None; // No swap configured; a row reading "0B / 0B" is noise.
    }
    let free = meminfo_field(info, "SwapFree")?;
    Some(format_usage(total - free, total))
}

fn disk(mount: &str) -> Option<String> {
    let path = std::ffi::CString::new(mount).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };

    // SAFETY: valid C string, correctly sized writable statvfs.
    if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
        return None;
    }

    // Block counts are in units of f_frsize.
    let unit = stat.f_frsize as u64;
    let total = stat.f_blocks as u64 * unit;
    // f_bavail excludes root-reserved blocks.
    let free = stat.f_bavail as u64 * unit;
    if total == 0 {
        return None;
    }

    Some(format_usage((total - free) / 1024, total / 1024))
}

/// Read a `KEY=value` field from os-release, stripping optional quoting.
fn os_release_field(text: &str, key: &str) -> Option<String> {
    text.lines()
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.trim().trim_matches('"').to_string())
        .filter(|v| !v.is_empty())
}

/// Read a `Key:  1234 kB` field from meminfo, in KiB.
fn meminfo_field(text: &str, key: &str) -> Option<u64> {
    text.lines()
        .filter_map(|l| l.split_once(':'))
        .find(|(k, _)| *k == key)
        .and_then(|(_, v)| v.split_whitespace().next()?.parse().ok())
}

pub fn hostname() -> String {
    read_trimmed("/proc/sys/kernel/hostname")
        .or_else(|| read_trimmed("/etc/hostname"))
        .unwrap_or_else(|| "localhost".into())
}

fn parent_pid(pid: u32) -> Option<u32> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // Field 2 can contain spaces and parens, so parse from the last ')'.
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(1)?.parse().ok()
}

fn comm(pid: u32) -> Option<String> {
    read_trimmed(format!("/proc/{pid}/comm"))
}

/// DMI fields are frequently placeholder text on desktops and VMs.
fn is_meaningful(value: &str) -> bool {
    !value.is_empty()
        && !value.eq_ignore_ascii_case("Default string")
        && !value.eq_ignore_ascii_case("To be filled by O.E.M.")
        && !value.eq_ignore_ascii_case("System Product Name")
        && !value.eq_ignore_ascii_case("None")
}

/// A directory, following symlinks like `is_dir`. The kind comes free with the
/// listing; `path().is_dir()` would stat every one of a thousand packages.
fn is_dir(entry: &fs::DirEntry) -> bool {
    match entry.file_type() {
        // Only case that still has to ask the filesystem.
        Ok(kind) if kind.is_symlink() => entry.path().is_dir(),
        Ok(kind) => kind.is_dir(),
        Err(_) => false,
    }
}

/// How many CPUs are online. `online` not `present`, to match what counting
/// cpuinfo's `processor` lines used to report.
fn online_cpus() -> Option<usize> {
    count_cpu_list(&read_trimmed("/sys/devices/system/cpu/online")?)
}

/// Size of a kernel CPU list: comma-separated `n` or `n-m` ranges, inclusive.
fn count_cpu_list(text: &str) -> Option<usize> {
    let mut total = 0usize;
    for range in text.split(',') {
        let (first, last) = range.split_once('-').unwrap_or((range, range));
        let first: usize = first.trim().parse().ok()?;
        let last: usize = last.trim().parse().ok()?;
        total += last.checked_sub(first)? + 1;
    }
    (total > 0).then_some(total)
}

/// Read at most `limit` bytes. Under `/proc` this also stops the kernel
/// formatting the rest. Bad UTF-8 is replaced, since callers match on ASCII.
fn read_prefix(path: &str, limit: u64) -> Option<String> {
    use std::io::Read as _;

    let mut buf = Vec::with_capacity(limit as usize);
    fs::File::open(path)
        .ok()?
        .take(limit)
        .read_to_end(&mut buf)
        .ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

fn read_trimmed(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_release_strips_quotes_and_picks_the_right_key() {
        let text = "NAME=\"Arch Linux\"\nPRETTY_NAME=\"Arch Linux\"\nID=arch\n";
        assert_eq!(
            os_release_field(text, "PRETTY_NAME"),
            Some("Arch Linux".into())
        );
        assert_eq!(os_release_field(text, "ID"), Some("arch".into()));
        assert_eq!(os_release_field(text, "MISSING"), None);
    }

    #[test]
    fn cpu_lists_count_every_form_the_kernel_writes() {
        assert_eq!(count_cpu_list("0-15"), Some(16));
        assert_eq!(count_cpu_list("0"), Some(1));
        assert_eq!(count_cpu_list("0-3,8-11"), Some(8));
        assert_eq!(count_cpu_list("0,2,4"), Some(3));
        assert_eq!(count_cpu_list(""), None);
        assert_eq!(count_cpu_list("nonsense"), None);
    }

    #[test]
    fn only_the_first_block_of_cpuinfo_is_read_and_it_holds_the_model() {
        // The saving in `cpu` rests on this being true.
        let head = read_prefix("/proc/cpuinfo", 4096);
        if let Some(head) = head {
            assert!(head.len() <= 4096);
            assert!(
                model_name(&head).is_some(),
                "model name not in the first 4KiB"
            );
        }
    }

    #[test]
    fn the_core_count_agrees_with_what_cpuinfo_lists() {
        // Must still match what counting `processor` lines reported.
        if let (Some(online), Ok(all)) = (online_cpus(), std::fs::read_to_string("/proc/cpuinfo")) {
            let listed = all.lines().filter(|l| l.starts_with("processor")).count();
            assert_eq!(online, listed);
        }
    }

    #[test]
    fn meminfo_parses_kib_values() {
        let text = "MemTotal:       16283812 kB\nMemAvailable:    9000000 kB\n";
        assert_eq!(meminfo_field(text, "MemTotal"), Some(16_283_812));
        assert_eq!(meminfo_field(text, "MemAvailable"), Some(9_000_000));
    }

    #[test]
    fn meminfo_does_not_confuse_prefixed_keys() {
        // "MemTotal" must not match a lookup for "Mem".
        let text = "MemTotal:  100 kB\nSwapTotal: 200 kB\n";
        assert_eq!(meminfo_field(text, "Mem"), None);
        assert_eq!(meminfo_field(text, "SwapTotal"), Some(200));
    }

    #[test]
    fn placeholder_dmi_strings_are_rejected() {
        assert!(is_meaningful("B650M K"));
        assert!(!is_meaningful(""));
        assert!(!is_meaningful("Default string"));
        assert!(!is_meaningful("To Be Filled By O.E.M."));
        assert!(!is_meaningful("None"));
    }

    #[test]
    fn parent_pid_handles_names_containing_spaces_and_parens() {
        // Field 2 is "(weird ) name)"; a naive split misreads ppid.
        let stat = "42 (weird ) name) S 7 42 42 0 -1 4194304 0 0";
        let rest = &stat[stat.rfind(')').unwrap() + 1..];
        let ppid: u32 = rest.split_whitespace().nth(1).unwrap().parse().unwrap();
        assert_eq!(ppid, 7);
    }
}
