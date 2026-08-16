//! System information.
//!
//! This file holds only what every platform shares: the row type, the title,
//! and the formatting. Where the numbers actually come from lives in a backend
//! module chosen below, so porting means writing one sibling of `linux.rs` and
//! changing nothing else.

use std::fmt::Write as _;

use crate::config::Module;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as sys;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as sys;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!(
    "animfetch supports Linux and macOS. The system-information backends live \
     in src/fetch/; a port means writing a sibling of linux.rs that exposes \
     the same `collect` and `hostname`."
);

/// One info row. An empty `label` spans the full width (the colour swatches).
pub struct Item {
    pub label: &'static str,
    pub value: String,
}

/// `user@host`, shown above the info rows.
pub fn title() -> String {
    let user = env("USER").or_else(|| env("LOGNAME")).unwrap_or_else(|| "user".into());
    format!("{user}@{}", sys::hostname())
}

/// Collect the requested modules, skipping ones this machine can't answer.
pub fn collect(modules: &[Module]) -> Vec<Item> {
    sys::collect(modules)
}

// ---------------------------------------------------------------------------
// Shared by every backend
// ---------------------------------------------------------------------------

/// The eight ANSI colours as solid blocks, so you can see your terminal theme.
fn swatches() -> String {
    let mut out = String::with_capacity(8 * 12);
    for code in 0..8 {
        let _ = write!(out, "\x1b[3{code}m███");
    }
    out.push_str(crate::color::RESET);
    out
}

fn arch() -> Option<String> {
    // std has no uname, but this is the same string.
    Some(std::env::consts::ARCH.to_string()).filter(|s| !s.is_empty())
}

// Only the Linux backend walks the process tree past the shell; the tests
// exercise it everywhere, so it stays shared rather than moving into linux.rs.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn is_shell(name: &str) -> bool {
    // Login shells appear as "-bash".
    let name = name.trim_start_matches('-');
    matches!(name, "sh" | "bash" | "zsh" | "fish" | "dash" | "ksh" | "tcsh" | "nu" | "elvish")
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
    fn byte_sizes_scale_and_keep_precision_where_it_matters() {
        assert_eq!(human_kib(512), "512KiB");
        assert_eq!(human_kib(1024), "1.00MiB");
        assert_eq!(human_kib(1024 * 1024), "1.00GiB");
        assert_eq!(human_kib(16 * 1024 * 1024), "16GiB");
    }

    #[test]
    fn usage_reports_percentage() {
        assert_eq!(format_usage(0, 0), "0KiB / 0KiB (0%)");
        assert_eq!(format_usage(512, 1024), "512KiB / 1.00MiB (50%)");
    }

    #[test]
    fn uptime_reads_naturally_at_every_scale() {
        assert_eq!(humanize_duration(0), "0 mins");
        assert_eq!(humanize_duration(60), "1 min");
        assert_eq!(humanize_duration(3600), "1 hour");
        assert_eq!(humanize_duration(3660), "1 hour, 1 min");
        assert_eq!(humanize_duration(90_000), "1 day, 1 hour");
    }

    #[test]
    fn login_shells_are_recognised() {
        assert!(is_shell("bash"));
        assert!(is_shell("-bash"));
        assert!(is_shell("zsh"));
        assert!(!is_shell("kitty"));
        assert!(!is_shell("alacritty"));
    }
}
