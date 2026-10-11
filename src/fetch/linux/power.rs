//! Batteries, from `/sys/class/power_supply`.

use std::fs;
use std::path::Path;

use super::read_trimmed;
use crate::fetch::Item;

/// How the state reads: plugged in and full says so, rather than a status
/// word that is "Full" on one machine and "Not charging" on the next.
fn charge_status(status: &str, on_ac: bool) -> String {
    match status {
        "Charging" => "Charging".into(),
        "Discharging" => "Discharging".into(),
        _ if on_ac => "AC Connected".into(),
        "" => "Unknown".into(),
        other => other.to_string(),
    }
}

/// One row per battery under `root` (`/sys/class/power_supply`). Peripherals
/// that report themselves as batteries (a mouse, a headset) have a `scope` of
/// `Device` and are not the machine's.
pub(super) fn batteries(root: &Path) -> Vec<Item> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut dirs: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    dirs.sort();

    let kind = |d: &Path| read_trimmed(d.join("type")).unwrap_or_default();
    let on_ac = dirs
        .iter()
        .any(|d| kind(d) == "Mains" && read_trimmed(d.join("online")).as_deref() == Some("1"));

    dirs.iter()
        .filter(|d| kind(d) == "Battery")
        .filter(|d| read_trimmed(d.join("scope")).as_deref() != Some("Device"))
        .filter_map(|d| {
            let percent: u8 = read_trimmed(d.join("capacity"))?.parse().ok()?;
            let status = read_trimmed(d.join("status")).unwrap_or_default();
            Some(Item::charge(
                "Battery",
                percent.min(100),
                charge_status(&status, on_ac),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugged_in_and_full_reads_as_connected() {
        assert_eq!(charge_status("Full", true), "AC Connected");
        assert_eq!(charge_status("Not charging", true), "AC Connected");
        assert_eq!(charge_status("Charging", true), "Charging");
        assert_eq!(charge_status("Discharging", false), "Discharging");
        assert_eq!(charge_status("", false), "Unknown");
    }

    /// A `power_supply` tree on disk, so the reader runs on a desktop too.
    fn power_tree(name: &str, supplies: &[(&str, &[(&str, &str)])]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("animfetch-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for (dir, files) in supplies {
            fs::create_dir_all(root.join(dir)).unwrap();
            for (file, text) in *files {
                fs::write(root.join(dir).join(file), format!("{text}\n")).unwrap();
            }
        }
        root
    }

    #[test]
    fn two_batteries_on_ac_read_like_the_reference() {
        let root = power_tree(
            "bat2",
            &[
                ("AC", &[("type", "Mains"), ("online", "1")]),
                (
                    "BAT0",
                    &[("type", "Battery"), ("capacity", "100"), ("status", "Full")],
                ),
                (
                    "BAT1",
                    &[
                        ("type", "Battery"),
                        ("capacity", "96"),
                        ("status", "Not charging"),
                    ],
                ),
            ],
        );
        let rows = batteries(&root);
        fs::remove_dir_all(&root).unwrap();

        let text: Vec<_> = rows.iter().map(|r| r.value.as_str()).collect();
        assert_eq!(text, ["100% [AC Connected]", "96% [AC Connected]"]);
    }

    #[test]
    fn a_battery_off_the_mains_says_it_is_discharging() {
        let root = power_tree(
            "bat1",
            &[
                ("AC", &[("type", "Mains"), ("online", "0")]),
                (
                    "BAT0",
                    &[
                        ("type", "Battery"),
                        ("capacity", "42"),
                        ("status", "Discharging"),
                    ],
                ),
            ],
        );
        let rows = batteries(&root);
        fs::remove_dir_all(&root).unwrap();

        assert_eq!(rows[0].value, "42% [Discharging]");
    }

    #[test]
    fn a_wireless_mouse_is_not_the_machines_battery() {
        let root = power_tree(
            "bat-mouse",
            &[(
                "hidpp_battery_0",
                &[
                    ("type", "Battery"),
                    ("scope", "Device"),
                    ("capacity", "55"),
                    ("status", "Discharging"),
                ],
            )],
        );
        let rows = batteries(&root);
        fs::remove_dir_all(&root).unwrap();

        assert!(rows.is_empty());
    }

    #[test]
    fn a_machine_without_batteries_has_no_battery_rows() {
        assert!(batteries(Path::new("/nonexistent/power_supply")).is_empty());
    }
}
