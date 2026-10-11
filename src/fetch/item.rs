//! The row type every backend produces and every style reads.

use super::format::{format_usage, human_kib_fixed, shared_unit};

/// One info row. An empty `label` spans the full width (the colour swatches).
#[derive(Clone, PartialEq)]
pub struct Item {
    /// What the row is: how a style finds its icon and its place, and how a
    /// refresh pairs a reading with its row. Never the user's wording.
    pub label: &'static str,
    /// The user's own name for the row, from `overrides`. Shown instead of the
    /// style's usual name for it, wherever a style shows one.
    renamed: Option<&'static str>,
    /// The text every style can show.
    pub value: String,
    /// What a style with room for more shows instead of `value`. The classic
    /// style never reads it, so its output does not depend on it.
    pub detail: Option<Detail>,
}

/// A fuller form of a row's value.
#[derive(Clone, PartialEq)]
pub enum Detail {
    /// The same reading with more in it, such as a version.
    Text(String),
    /// A reading that is a part of a whole. Kept as numbers rather than text so
    /// a style can format it and colour the percentage as it likes.
    Usage(Usage),
    /// A battery. Kept apart from `Usage` because the colour runs the other
    /// way: a full disk is a warning, a full battery is not.
    ///
    /// Only the Linux backend reads batteries so far.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Charge(Charge),
    /// How long, in seconds. Kept as a number so a style can say it as
    /// `1h 40m` or `1days 4hours` without parsing the classic words back out.
    Duration(u64),
}

#[derive(Clone, PartialEq)]
pub struct Charge {
    pub percent: u8,
    /// `AC Connected`, `Charging`, `Discharging`.
    pub status: String,
}

#[derive(Clone, PartialEq)]
pub struct Usage {
    pub used_kib: u64,
    pub total_kib: u64,
    /// Trailing remark, such as the filesystem a disk uses.
    pub note: Option<String>,
    /// What this is a usage of, when there are several (a mount point).
    pub name: Option<String>,
}

impl Usage {
    pub fn new(used_kib: u64, total_kib: u64) -> Self {
        Self {
            used_kib,
            total_kib,
            note: None,
            name: None,
        }
    }

    pub fn with_name(mut self, name: Option<String>) -> Self {
        self.name = name;
        self
    }

    pub fn with_note(mut self, note: Option<String>) -> Self {
        self.note = note;
        self
    }

    pub fn percent(&self) -> u64 {
        (self.used_kib * 100)
            .checked_div(self.total_kib)
            .unwrap_or(0)
    }

    /// `46 / 928 GiB`: both figures in the total's unit, named once, with
    /// `places` decimals for a figure of that size.
    pub fn shared(&self, places: fn(f64) -> usize) -> String {
        shared_unit(self.used_kib, self.total_kib, places)
    }

    /// `9.91 GiB / 48.91 GiB`: both figures, no percentage.
    pub fn spaced(&self) -> String {
        format!(
            "{} / {}",
            human_kib_fixed(self.used_kib),
            human_kib_fixed(self.total_kib)
        )
    }

    /// `2.57 GiB/14.94 GiB`: the bare reading, for styles with no room or need
    /// for the percentage.
    pub fn short(&self) -> String {
        format!(
            "{}/{}",
            human_kib_fixed(self.used_kib),
            human_kib_fixed(self.total_kib)
        )
    }

    /// `4.39 GiB / 15.56 GiB (`, `28%`, `) - ext4`. Split so the percentage can
    /// be painted on its own, and so the three parts still join into the line.
    pub fn long(&self) -> (String, String, String) {
        let head = format!(
            "{} / {} (",
            human_kib_fixed(self.used_kib),
            human_kib_fixed(self.total_kib)
        );
        let tail = match &self.note {
            Some(note) => format!(") - {note}"),
            None => ")".into(),
        };
        (head, format!("{}%", self.percent()), tail)
    }
}

impl Item {
    pub fn new(label: &'static str, value: impl Into<String>) -> Self {
        Self {
            label,
            renamed: None,
            value: value.into(),
            detail: None,
        }
    }

    /// The same row under the user's own name. The name is leaked, like a
    /// custom row's label: rows carry `&'static str`, there are a handful, and
    /// they are made once.
    pub fn renamed(mut self, name: &str) -> Self {
        self.renamed = Some(Box::leak(name.to_string().into_boxed_str()));
        self
    }

    /// The user's name for the row, if they gave one.
    pub fn renamed_to(&self) -> Option<&'static str> {
        self.renamed
    }

    /// What the row is called on screen where a style prints its label as it
    /// is: the user's name, else the label.
    pub fn name(&self) -> &'static str {
        self.renamed.unwrap_or(self.label)
    }

    /// A row from a reading that may be missing, which skips the row.
    pub fn read(label: &'static str, value: Option<String>) -> Option<Self> {
        value.map(|value| Self::new(label, value))
    }

    /// A row from a usage reading. `value` is the compact text the classic
    /// style has always shown; the numbers ride along in `detail`.
    pub fn usage(label: &'static str, usage: Option<Usage>) -> Option<Self> {
        let usage = usage?;
        let figures = format_usage(usage.used_kib, usage.total_kib);
        Some(Self {
            label,
            renamed: None,
            // Rows that repeat a label are told apart by what they measure.
            value: match &usage.name {
                Some(name) => format!("{name}: {figures}"),
                None => figures,
            },
            detail: Some(Detail::Usage(usage)),
        })
    }

    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    /// A row the user wrote, in the config.
    ///
    /// The label is leaked: rows carry `&'static str` labels, being almost all
    /// fixed names, and a configured row lives as long as the process does.
    /// There are a handful, made once.
    pub fn custom(label: &str, value: &str) -> Self {
        Self::new(Box::leak(label.to_string().into_boxed_str()), value)
    }

    /// A span of time: `text` is what the classic style shows, and the seconds
    /// ride along for the styles that say it their own way.
    pub fn duration(label: &'static str, secs: u64, text: impl Into<String>) -> Self {
        Self {
            label,
            renamed: None,
            value: text.into(),
            detail: Some(Detail::Duration(secs)),
        }
    }

    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn charge(label: &'static str, percent: u8, status: impl Into<String>) -> Self {
        let status = status.into();
        Self {
            label,
            renamed: None,
            value: format!("{percent}% [{status}]"),
            detail: Some(Detail::Charge(Charge { percent, status })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_named_usage_says_what_it_measures() {
        let row = Item::usage(
            "Disk",
            Some(Usage::new(512, 1024).with_name(Some("/home".into()))),
        )
        .unwrap();
        assert_eq!(row.value, "/home: 512KiB / 1.00MiB (50%)");
        assert_eq!(Usage::new(512, 1024).spaced(), "512.00 KiB / 1.00 MiB");
    }

    #[test]
    fn a_duration_keeps_its_seconds_beside_the_classic_words() {
        let row = Item::duration("Uptime", 6000, "1 hour, 40 mins");
        assert_eq!(row.value, "1 hour, 40 mins");
        assert!(matches!(row.detail, Some(Detail::Duration(6000))));
    }

    #[test]
    fn a_battery_row_keeps_its_percentage_apart_from_its_state() {
        let row = Item::charge("Battery", 96, "AC Connected");
        assert_eq!(row.value, "96% [AC Connected]");
        assert!(matches!(row.detail, Some(Detail::Charge(c)) if c.percent == 96));
    }

    #[test]
    fn a_usage_row_keeps_the_text_the_classic_style_always_showed() {
        let row = Item::usage("Memory", Some(Usage::new(512, 1024))).unwrap();
        assert_eq!(row.value, "512KiB / 1.00MiB (50%)");
    }

    #[test]
    fn the_long_form_joins_into_one_line_and_names_the_filesystem() {
        let u = Usage::new(4_604_000, 16_320_000).with_note(Some("ext4".into()));
        let (head, pct, tail) = u.long();

        assert_eq!(pct, "28%");
        assert_eq!(
            format!("{head}{pct}{tail}"),
            "4.39 GiB / 15.56 GiB (28%) - ext4"
        );
        assert_eq!(Usage::new(1, 2).long().2, ")", "no note, no dash");
    }

    #[test]
    fn the_short_form_is_the_bare_reading() {
        assert_eq!(
            Usage::new(2_694_000, 15_660_000).short(),
            "2.57 GiB/14.93 GiB"
        );
    }
}
