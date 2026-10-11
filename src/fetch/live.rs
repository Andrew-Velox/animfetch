//! Keeping the moving rows current while the fetch is on screen.

use std::time::{Duration, Instant};

use super::{Item, sys};
use crate::config::{Config, Module};

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
            modules: cfg.modules(),
            items: items.to_vec(),
            every,
            next: Instant::now() + every,
            // Nothing to watch means nothing to wake up for.
            active: cfg.live && cfg.modules().iter().any(|m| m.is_live()),
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

/// Write fresh readings over the rows they belong to. Rows that share a label
/// (several disks, several batteries) are paired in order, each used once: the
/// backends list them in a stable order, so the nth fresh `Disk` is the nth.
fn merge(items: &mut [Item], fresh: Vec<Item>) -> bool {
    let mut taken = vec![false; items.len()];
    let mut changed = false;

    for new in fresh {
        let slot = items
            .iter()
            .enumerate()
            .position(|(i, it)| !taken[i] && !it.label.is_empty() && it.label == new.label);
        let Some(i) = slot else { continue };
        taken[i] = true;

        if items[i] != new {
            // The numbers behind a usage row move with its text.
            items[i].value = new.value;
            items[i].detail = new.detail;
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::collect;
    use crate::fetch::format::format_usage;
    use crate::fetch::{Detail, Usage};

    fn item(label: &'static str, value: &str) -> Item {
        Item::new(label, value)
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
    fn rows_that_share_a_label_are_paired_in_order() {
        // Two disks: the second fresh reading must land on the second row, not
        // overwrite the first twice.
        let disk = |name: &str, used| {
            Item::usage(
                "Disk",
                Some(Usage::new(used, 1000).with_name(Some(name.into()))),
            )
            .unwrap()
        };
        let mut rows = vec![disk("/", 100), disk("/home", 200)];

        assert!(merge(&mut rows, vec![disk("/", 150), disk("/home", 250)]));
        assert_eq!(rows[0].value, disk("/", 150).value);
        assert_eq!(rows[1].value, disk("/home", 250).value);
    }

    #[test]
    fn a_missing_duplicate_leaves_its_row_in_place() {
        // A battery pulled out: the row stays, stale, rather than vanishing.
        let bat = |p| Item::charge("Battery", p, "Discharging");
        let mut rows = vec![bat(80), bat(60)];

        assert!(merge(&mut rows, vec![bat(70)]));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].value, "70% [Discharging]");
        assert_eq!(rows[1].value, "60% [Discharging]");
    }

    #[test]
    fn full_width_rows_are_never_matched_by_label() {
        // The swatches have an empty label; two of them must not alias.
        let mut rows = vec![item("", "swatches")];
        assert!(!merge(&mut rows, vec![item("", "other")]));
        assert_eq!(rows[0].value, "swatches");
    }

    #[test]
    fn merging_carries_the_numbers_behind_a_usage_row() {
        let mut rows = vec![Item::usage("Memory", Some(Usage::new(1024, 4096))).unwrap()];
        let fresh = Item::usage("Memory", Some(Usage::new(2048, 4096))).unwrap();

        assert!(merge(&mut rows, vec![fresh]));
        let Some(Detail::Usage(u)) = &rows[0].detail else {
            panic!("usage detail lost in the merge");
        };
        assert_eq!(u.used_kib, 2048);
        assert_eq!(rows[0].value, format_usage(2048, 4096));
    }

    #[test]
    fn refreshing_the_real_system_keeps_the_rows_it_started_with() {
        let modules = [Module::Os, Module::Uptime, Module::Memory, Module::Disk];
        let mut rows = collect(&modules, false);
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
}
