//! Sizes and durations as people read them.

/// `1.2GiB / 15.5GiB (8%)` from KiB inputs.
pub(super) fn format_usage(used_kib: u64, total_kib: u64) -> String {
    let pct = (used_kib * 100).checked_div(total_kib).unwrap_or(0);
    format!(
        "{} / {} ({pct}%)",
        human_kib(used_kib),
        human_kib(total_kib)
    )
}

/// How many KiB make the unit `kib` is best said in, and that unit's name:
/// `(1048576.0, "GiB")` for a few gibibytes. A power of two, so dividing by it
/// loses nothing.
fn unit_of(kib: u64) -> (f64, &'static str) {
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    let mut divisor = 1.0;
    let mut unit = 0;
    while kib as f64 / divisor >= 1024.0 && unit < UNITS.len() - 1 {
        divisor *= 1024.0;
        unit += 1;
    }
    (divisor, UNITS[unit])
}

pub(super) fn human_kib(kib: u64) -> String {
    let (divisor, unit) = unit_of(kib);
    let value = kib as f64 / divisor;
    // Sub-10 values need a decimal to stay informative.
    if value < 10.0 && divisor > 1.0 {
        format!("{value:.2}{unit}")
    } else {
        format!("{value:.0}{unit}")
    }
}

/// `4.39 GiB`: two decimals and a space, the form the boxed style uses. Kept
/// apart from `human_kib` so the classic text does not move.
pub(super) fn human_kib_fixed(kib: u64) -> String {
    let (divisor, unit) = unit_of(kib);
    format!("{:.2} {unit}", kib as f64 / divisor)
}

/// `46 / 928 GiB`: both figures in the unit the total is best said in, named
/// once. `places` says how many decimals a figure of that size gets, so a disk
/// can show `1.5 / 299` and a memory `4.23 / 31.25`.
pub(super) fn shared_unit(used_kib: u64, total_kib: u64, places: fn(f64) -> usize) -> String {
    let (divisor, unit) = unit_of(total_kib);
    let (used, total) = (used_kib as f64 / divisor, total_kib as f64 / divisor);
    format!(
        "{used:.u$} / {total:.t$} {unit}",
        u = places(used),
        t = places(total)
    )
}

pub(super) fn humanize_duration(secs: u64) -> String {
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

/// `194 days`: an age wants one unit, where uptime wants the breakdown.
pub(super) fn humanize_age(secs: u64) -> String {
    match secs / 86400 {
        0 => humanize_duration(secs),
        d => format!("{d} day{}", plural(d)),
    }
}

/// `3d 2h 5m`: the units that are not zero, a letter each.
pub fn compact_duration(secs: u64) -> String {
    let (d, h, m) = (secs / 86400, secs % 86400 / 3600, secs % 3600 / 60);

    let mut parts = Vec::new();
    if d > 0 {
        parts.push(format!("{d}d"));
    }
    if h > 0 {
        parts.push(format!("{h}h"));
    }
    // So a fresh boot reads "0m" rather than nothing.
    if m > 0 || parts.is_empty() {
        parts.push(format!("{m}m"));
    }
    parts.join(" ")
}

/// `3days 18hours 45mins 1sec`: every unit that is not zero, seconds too.
pub fn long_duration(secs: u64) -> String {
    let units = [
        (secs / 86400, "days"),
        (secs % 86400 / 3600, "hours"),
        (secs % 3600 / 60, "mins"),
        (secs % 60, "sec"),
    ];
    let parts: Vec<String> = units
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, unit)| format!("{n}{unit}"))
        .collect();

    if parts.is_empty() {
        "0sec".into()
    } else {
        parts.join(" ")
    }
}

pub(super) fn plural(n: u64) -> &'static str {
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
    fn both_figures_share_the_unit_of_the_total() {
        let two = |_| 2;
        let gib = 1024 * 1024;
        assert_eq!(
            shared_unit(4 * gib + gib / 4, 31 * gib + gib / 4, two),
            "4.25 / 31.25 GiB"
        );
        // The used figure follows the total's unit even when it is small.
        assert_eq!(shared_unit(512 * 1024, 10 * gib, two), "0.50 / 10.00 GiB");
        assert_eq!(
            shared_unit(0, 0, two),
            "0.00 / 0.00 KiB",
            "an empty total must not divide by zero"
        );
    }

    #[test]
    fn the_decimals_follow_the_size_of_each_figure() {
        let compact: fn(f64) -> usize = |v| if v < 10.0 { 1 } else { 0 };
        let gib = 1024 * 1024;
        assert_eq!(shared_unit(46 * gib, 928 * gib, compact), "46 / 928 GiB");
        assert_eq!(
            shared_unit(gib + gib / 2, 299 * gib, compact),
            "1.5 / 299 GiB"
        );
    }

    #[test]
    fn the_classic_size_text_is_what_it_always_was() {
        // `human_kib` and `human_kib_fixed` were two copies of one loop.
        assert_eq!(human_kib(1023), "1023KiB");
        assert_eq!(human_kib(1024), "1.00MiB");
        assert_eq!(human_kib(10 * 1024), "10MiB");
        assert_eq!(human_kib(1024 * 1024 * 1024 * 3), "3.00TiB");
        assert_eq!(human_kib_fixed(1024 * 1024 * 1024 * 2048), "2048.00 TiB");
    }

    #[test]
    fn fixed_sizes_always_carry_two_decimals() {
        assert_eq!(human_kib_fixed(192 * 1024 * 1024), "192.00 GiB");
        assert_eq!(human_kib_fixed(1536), "1.50 MiB");
        assert_eq!(human_kib_fixed(0), "0.00 KiB");
    }

    #[test]
    fn a_compact_span_drops_what_is_zero_and_keeps_the_letters() {
        assert_eq!(compact_duration(6000), "1h 40m");
        assert_eq!(compact_duration(3600), "1h");
        assert_eq!(compact_duration(90_000), "1d 1h");
        assert_eq!(compact_duration(0), "0m");
        assert_eq!(compact_duration(59), "0m", "seconds are not shown here");
    }

    #[test]
    fn a_long_span_runs_the_units_together_with_seconds() {
        assert_eq!(
            long_duration(3 * 86400 + 18 * 3600 + 45 * 60 + 1),
            "3days 18hours 45mins 1sec"
        );
        assert_eq!(long_duration(60), "1mins");
        assert_eq!(long_duration(5), "5sec");
        assert_eq!(long_duration(0), "0sec");
    }

    #[test]
    fn an_age_reads_in_the_one_unit_that_matters() {
        assert_eq!(humanize_age(194 * 86400 + 5000), "194 days");
        assert_eq!(humanize_age(86400), "1 day");
        assert_eq!(humanize_age(3600), "1 hour");
        assert_eq!(humanize_age(30), "0 mins");
    }
}
