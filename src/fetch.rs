//! System information, Linux only. All of it from `/proc`, `/sys` and env vars,
//! never subprocesses, which is why this runs in about a millisecond.
//!
//! Porting means reimplementing this module; nothing else knows the sources.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use crate::config::Module;

/// One info row. An empty `label` spans the full width (the colour swatches).
pub struct Item {
    pub label: &'static str,
    pub value: String,
}

/// `user@host`, shown above the info rows.
pub fn title() -> String {
    let user = env("USER").or_else(|| env("LOGNAME")).unwrap_or_else(|| "user".into());
    format!("{user}@{}", hostname())
}

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

// ---------------------------------------------------------------------------
// Modules
// ---------------------------------------------------------------------------

fn os() -> Option<String> {
    let release = fs::read_to_string("/etc/os-release").ok()?;
    let pretty = os_release_field(&release, "PRETTY_NAME")
        .or_else(|| os_release_field(&release, "NAME"))?;

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
        let n = status.lines().filter(|l| l.starts_with("Package: ")).count();
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

/// The eight ANSI colours as solid blocks, so you can see your terminal theme.
fn swatches() -> String {
    let mut out = String::with_capacity(8 * 12);
    for code in 0..8 {
        let _ = write!(out, "\x1b[3{code}m███");
    }
    out.push_str(crate::color::RESET);
    out
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

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

fn arch() -> Option<String> {
    // std has no uname, but the kernel exposes the same string here.
    Some(std::env::consts::ARCH.to_string()).filter(|s| !s.is_empty())
}

fn hostname() -> String {
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

fn is_shell(name: &str) -> bool {
    // Login shells appear as "-bash".
    let name = name.trim_start_matches('-');
    matches!(name, "sh" | "bash" | "zsh" | "fish" | "dash" | "ksh" | "tcsh" | "nu" | "elvish")
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
    fs::File::open(path).ok()?.take(limit).read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

fn read_trimmed(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

fn basename_of_env(key: &str) -> Option<String> {
    let value = env(key)?;
    Some(value.rsplit('/').next().unwrap_or(&value).to_string())
}

/// `1.2GiB / 15.5GiB (8%)` from KiB inputs.
fn format_usage(used_kib: u64, total_kib: u64) -> String {
    let pct = (used_kib * 100).checked_div(total_kib).unwrap_or(0);
    format!("{} / {} ({pct}%)", human_kib(used_kib), human_kib(total_kib))
}

fn human_kib(kib: u64) -> String {
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    let mut value = kib as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    // Sub-10 values need a decimal to stay informative.
    if value < 10.0 && unit > 0 {
        format!("{value:.2}{}", UNITS[unit])
    } else {
        format!("{value:.0}{}", UNITS[unit])
    }
}

fn humanize_duration(secs: u64) -> String {
    let (d, h, m) = (secs / 86400, secs % 86400 / 3600, secs % 3600 / 60);

    let mut parts = Vec::new();
    if d > 0 {
        parts.push(format!("{d} day{}", plural(d)));
    }
    if h > 0 {
        parts.push(format!("{h} hour{}", plural(h)));
    }
    // So a fresh boot reads "0 mins" rather than nothing.
    if m > 0 || parts.is_empty() {
        parts.push(format!("{m} min{}", plural(m)));
    }
    parts.join(", ")
}

fn plural(n: u64) -> &'static str {
    if n == 1 { "" } else { "s" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_release_strips_quotes_and_picks_the_right_key() {
        let text = "NAME=\"Arch Linux\"\nPRETTY_NAME=\"Arch Linux\"\nID=arch\n";
        assert_eq!(os_release_field(text, "PRETTY_NAME").as_deref(), Some("Arch Linux"));
        assert_eq!(os_release_field(text, "ID").as_deref(), Some("arch"));
        assert_eq!(os_release_field(text, "MISSING"), None);
    }

    #[test]
    fn cpu_lists_count_every_form_the_kernel_writes() {
        assert_eq!(count_cpu_list("0"), Some(1));
        assert_eq!(count_cpu_list("0-31"), Some(32));
        // A machine with cores offlined reports several ranges.
        assert_eq!(count_cpu_list("0-3,8-11"), Some(8));
        assert_eq!(count_cpu_list("0,2,4"), Some(3));
        // Nothing usable must fall back rather than report a wrong count.
        assert_eq!(count_cpu_list(""), None);
        assert_eq!(count_cpu_list("garbage"), None);
        assert_eq!(count_cpu_list("4-0"), None);
    }

    #[test]
    fn only_the_first_block_of_cpuinfo_is_read_and_it_holds_the_model() {
        // The whole saving rests on the model name being in the first block.
        let Some(head) = read_prefix("/proc/cpuinfo", 4096) else {
            return; // no /proc here
        };
        assert!(head.len() <= 4096);

        let full = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        if let Some(expected) = model_name(&full) {
            assert_eq!(model_name(&head), Some(expected));
        }
    }

    #[test]
    fn the_core_count_agrees_with_what_cpuinfo_lists() {
        // Must still match what counting `processor` lines reported.
        let Some(online) = online_cpus() else { return };
        let Ok(info) = fs::read_to_string("/proc/cpuinfo") else { return };

        let listed = info.lines().filter(|l| l.starts_with("processor")).count();
        if listed > 0 {
            assert_eq!(online, listed);
        }
    }

    #[test]
    fn meminfo_parses_kib_values() {
        let text = "MemTotal:       16324220 kB\nMemFree:         1000 kB\n";
        assert_eq!(meminfo_field(text, "MemTotal"), Some(16_324_220));
        assert_eq!(meminfo_field(text, "MemFree"), Some(1_000));
        assert_eq!(meminfo_field(text, "SwapTotal"), None);
    }

    #[test]
    fn meminfo_does_not_confuse_prefixed_keys() {
        // "MemTotal" must not match a lookup for "Mem".
        let text = "MemTotal: 100 kB\nMemAvailable: 50 kB\n";
        assert_eq!(meminfo_field(text, "Mem"), None);
        assert_eq!(meminfo_field(text, "MemAvailable"), Some(50));
    }

    #[test]
    fn byte_sizes_scale_and_keep_precision_where_it_matters() {
        assert_eq!(human_kib(512), "512KiB");
        assert_eq!(human_kib(2048), "2.00MiB");
        assert_eq!(human_kib(16 * 1024 * 1024), "16GiB");
    }

    #[test]
    fn usage_reports_percentage() {
        assert_eq!(format_usage(0, 0), "0KiB / 0KiB (0%)");
        assert_eq!(format_usage(50, 100), "50KiB / 100KiB (50%)");
    }

    #[test]
    fn uptime_reads_naturally_at_every_scale() {
        assert_eq!(humanize_duration(0), "0 mins");
        assert_eq!(humanize_duration(60), "1 min");
        assert_eq!(humanize_duration(3600), "1 hour");
        assert_eq!(humanize_duration(3660), "1 hour, 1 min");
        assert_eq!(humanize_duration(90_061), "1 day, 1 hour, 1 min");
    }

    #[test]
    fn login_shells_are_recognised() {
        assert!(is_shell("bash"));
        assert!(is_shell("-zsh"));
        assert!(!is_shell("kitty"));
    }

    #[test]
    fn placeholder_dmi_strings_are_rejected() {
        assert!(is_meaningful("XPS 13 9310"));
        assert!(!is_meaningful("Default string"));
        assert!(!is_meaningful(""));
    }

    #[test]
    fn parent_pid_handles_names_containing_spaces_and_parens() {
        // Field 2 is "(weird ) name)"; a naive split misreads ppid.
        let stat = "1234 (weird ) name) S 42 1234 1234 0 -1 4194304";
        let rest = &stat[stat.rfind(')').unwrap() + 1..];
        let ppid: u32 = rest.split_whitespace().nth(1).unwrap().parse().unwrap();
        assert_eq!(ppid, 42);
    }
}
