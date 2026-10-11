//! The CPU's temperature, from the `hwmon` sensors the kernel exposes.
//!
//! Not the first sensor found: a machine has several, and most are not the
//! CPU's (`acpitz` is often a fixed 16.8 °C, `nvme` is the disk). Only chips
//! known to report the package are read, best first.

use std::fs;
use std::path::{Path, PathBuf};

use super::read_trimmed;

/// Chip names that report the CPU, in the order they are trusted.
const CPU_SENSORS: &[&str] = &[
    "coretemp",
    "k10temp",
    "zenpower",
    "cpu_thermal",
    "cpu-thermal",
];

/// `50.50°C`.
pub(super) fn read() -> Option<String> {
    read_in(Path::new("/sys/class/hwmon")).or_else(|| read_zone(Path::new("/sys/class/thermal")))
}

fn read_in(root: &Path) -> Option<String> {
    let mut chips: Vec<(usize, PathBuf)> = fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter_map(|dir| {
            let name = read_trimmed(dir.join("name"))?;
            let rank = CPU_SENSORS.iter().position(|s| *s == name)?;
            Some((rank, dir))
        })
        .collect();
    chips.sort();

    chips
        .into_iter()
        .find_map(|(_, dir)| celsius(&read_trimmed(dir.join("temp1_input"))?))
}

/// Intel exposes the package as a thermal zone on machines without `coretemp`.
fn read_zone(root: &Path) -> Option<String> {
    fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|z| read_trimmed(z.join("type")).as_deref() == Some("x86_pkg_temp"))
        .find_map(|z| celsius(&read_trimmed(z.join("temp"))?))
}

/// Millidegrees as `50.50°C`. A reading of zero, or one no CPU could have, is
/// a sensor that is not wired, and is no reading.
fn celsius(millis: &str) -> Option<String> {
    let milli: i64 = millis.trim().parse().ok()?;
    (1..150_000)
        .contains(&milli)
        .then(|| format!("{:.2}°C", milli as f64 / 1000.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sensors(name: &str, chips: &[(&str, &str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("animfetch-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for (dir, chip, temp) in chips {
            fs::create_dir_all(root.join(dir)).unwrap();
            fs::write(root.join(dir).join("name"), format!("{chip}\n")).unwrap();
            fs::write(root.join(dir).join("temp1_input"), format!("{temp}\n")).unwrap();
        }
        root
    }

    #[test]
    fn millidegrees_become_celsius() {
        assert_eq!(celsius("50500").as_deref(), Some("50.50°C"));
        assert_eq!(celsius("60000\n").as_deref(), Some("60.00°C"));
    }

    #[test]
    fn a_sensor_that_is_not_wired_is_no_reading() {
        assert_eq!(celsius("0"), None);
        assert_eq!(celsius("-5"), None);
        assert_eq!(celsius("999999"), None);
        assert_eq!(celsius("hot"), None);
    }

    #[test]
    fn the_cpu_chip_is_read_and_the_others_are_not() {
        // acpitz first in the listing, and a lie; the CPU's chip is k10temp.
        let root = sensors(
            "hwmon",
            &[
                ("hwmon0", "acpitz", "16800"),
                ("hwmon1", "nvme", "45850"),
                ("hwmon2", "k10temp", "50500"),
            ],
        );
        let found = read_in(&root);
        fs::remove_dir_all(&root).unwrap();

        assert_eq!(found.as_deref(), Some("50.50°C"));
    }

    #[test]
    fn a_better_sensor_wins_over_a_lesser_one() {
        let root = sensors(
            "hwmon-rank",
            &[
                ("hwmon0", "cpu_thermal", "70000"),
                ("hwmon1", "coretemp", "55000"),
            ],
        );
        let found = read_in(&root);
        fs::remove_dir_all(&root).unwrap();

        assert_eq!(found.as_deref(), Some("55.00°C"));
    }

    #[test]
    fn a_machine_with_only_other_sensors_has_no_temperature() {
        let root = sensors(
            "hwmon-none",
            &[("hwmon0", "acpitz", "16800"), ("hwmon1", "nvme", "40000")],
        );
        let found = read_in(&root);
        fs::remove_dir_all(&root).unwrap();

        assert_eq!(found, None);
    }

    #[test]
    fn a_missing_directory_is_just_no_temperature() {
        assert_eq!(read_in(Path::new("/nonexistent/hwmon")), None);
        assert_eq!(read_zone(Path::new("/nonexistent/thermal")), None);
    }
}
