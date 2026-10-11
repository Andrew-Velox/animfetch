//! System information.
//!
//! This file holds only what every platform shares: the row type, the title,
//! and the formatting. Where the numbers actually come from lives in a backend
//! module chosen below, so porting means writing one sibling of `linux.rs` and
//! changing nothing else.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use crate::config::{Config, Module};

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
#[derive(Clone)]
pub struct Item {
    pub label: &'static str,
    pub value: String,
}

/// `user@host`, shown above the info rows.
pub fn title() -> String {
    let user = env("USER")
        .or_else(|| env("LOGNAME"))
        .unwrap_or_else(|| "user".into());
    format!("{user}@{}", sys::hostname())
}

/// Collect the requested modules, skipping ones this machine can't answer.
pub fn collect(modules: &[Module]) -> Vec<Item> {
    sys::collect(modules)
}

/// The info rows, kept current while the fetch is on screen.
///
/// Holds its own copy, so the rows the layout was measured from stay exactly as
/// they were. That is deliberate: the art is sized once from those, and a
/// refresh must never be able to rescale it.
///
/// Inert unless `live` is configured, in which case `items` is just the rows
/// collected at startup and `tick` does nothing.
pub struct Live<'a> {
    modules: &'a [Module],
    items: Vec<Item>,
    every: Duration,
    next: Instant,
    active: bool,
}

impl<'a> Live<'a> {
    pub fn new(cfg: &'a Config, items: &[Item]) -> Self {
        let every = cfg.live_interval();
        Self {
            modules: &cfg.modules,
            items: items.to_vec(),
            every,
            next: Instant::now() + every,
            // Nothing to watch means nothing to wake up for.
            active: cfg.live && cfg.modules.iter().any(|m| m.is_live()),
        }
    }

    /// Re-read the moving rows if the interval has elapsed.
    pub fn tick(&mut self) {
        if !self.active {
            return;
        }
        let now = Instant::now();
        if now < self.next {
            return;
        }
        // From now, not from the old deadline: after a long stretch of not
        // being asked (a full-screen program held the terminal) one refresh is
        // enough, not a burst to catch up.
        self.next = now + self.every;
        refresh(self.modules, &mut self.items);
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }
}

/// Re-read only the modules whose values move, writing them into `items` in
/// place. Returns whether any value changed.
///
/// Rows are matched by label and are never added or removed. The pane's height
/// fixes the scroll region beneath it, so a row that vanished because a read
/// failed once would leave the region and the pane disagreeing. A failed read
/// keeps the last good value instead.
fn refresh(modules: &[Module], items: &mut [Item]) -> bool {
    let moving: Vec<Module> = modules.iter().copied().filter(|m| m.is_live()).collect();
    if moving.is_empty() {
        return false;
    }
    merge(items, sys::collect(&moving))
}

fn merge(items: &mut [Item], fresh: Vec<Item>) -> bool {
    let mut changed = false;
    for new in fresh {
        let slot = items
            .iter_mut()
            .find(|i| !i.label.is_empty() && i.label == new.label);
        if let Some(item) = slot
            && item.value != new.value
        {
            item.value = new.value;
            changed = true;
        }
    }
    changed
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
    matches!(
        name,
        "sh" | "bash" | "zsh" | "fish" | "dash" | "ksh" | "tcsh" | "nu" | "elvish"
    )
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
    format!(
        "{} / {} ({pct}%)",
        human_kib(used_kib),
        human_kib(total_kib)
    )
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

    fn item(label: &'static str, value: &str) -> Item {
        Item {
            label,
            value: value.into(),
        }
    }

    #[test]
    fn merging_updates_values_and_reports_a_change() {
        let mut rows = vec![item("OS", "Arch"), item("Memory", "1GiB")];
        let changed = merge(&mut rows, vec![item("Memory", "2GiB")]);

        assert!(changed);
        assert_eq!(rows[0].value, "Arch", "an untouched row must not move");
        assert_eq!(rows[1].value, "2GiB");
    }

    #[test]
    fn merging_identical_values_is_not_a_change() {
        let mut rows = vec![item("Memory", "1GiB")];
        assert!(!merge(&mut rows, vec![item("Memory", "1GiB")]));
    }

    #[test]
    fn merging_never_adds_or_drops_a_row() {
        // The pane height fixes the scroll region. A read that fails once must
        // not make a row disappear, and a stray label must not make one appear.
        let mut rows = vec![item("Memory", "1GiB"), item("Swap", "0B")];
        merge(&mut rows, vec![item("Disk", "9GiB")]);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].value, "1GiB");
        assert_eq!(rows[1].value, "0B", "a missing fresh value keeps the old");
    }

    #[test]
    fn full_width_rows_are_never_matched_by_label() {
        // The swatches have an empty label; two of them must not alias.
        let mut rows = vec![item("", "swatches")];
        assert!(!merge(&mut rows, vec![item("", "other")]));
        assert_eq!(rows[0].value, "swatches");
    }

    #[test]
    fn refreshing_the_real_system_keeps_the_rows_it_started_with() {
        let modules = [Module::Os, Module::Uptime, Module::Memory, Module::Disk];
        let mut rows = collect(&modules);
        let labels: Vec<_> = rows.iter().map(|r| r.label).collect();

        refresh(&modules, &mut rows);

        let after: Vec<_> = rows.iter().map(|r| r.label).collect();
        assert_eq!(labels, after);
    }

    #[test]
    fn live_does_nothing_unless_enabled() {
        let cfg = Config::default();
        let rows = vec![item("Memory", "1GiB")];
        let mut live = Live::new(&cfg, &rows);

        // Backdate the deadline so a refresh *would* be due if it were active.
        live.next = Instant::now() - Duration::from_secs(60);
        live.tick();
        assert_eq!(live.items()[0].value, "1GiB");
        assert!(live.next < Instant::now(), "an inert Live must not re-arm");
    }

    #[test]
    fn live_rearms_from_now_so_a_long_pause_gives_one_refresh() {
        let cfg = Config {
            live: true,
            ..Config::default()
        };
        let mut live = Live::new(&cfg, &[item("Memory", "stale")]);
        live.next = Instant::now() - Duration::from_secs(3600);

        live.tick();
        assert!(live.next > Instant::now(), "must schedule into the future");
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
