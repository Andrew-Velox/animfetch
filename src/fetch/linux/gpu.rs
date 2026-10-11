//! Graphics cards, named from sysfs and the id databases. No subprocess, so no
//! `lspci`.

use std::fs;
use std::path::Path;

use super::read_trimmed;

/// Where the PCI id database lives, by distro.
const PCI_IDS: &[&str] = &[
    "/usr/share/hwdata/pci.ids",
    "/usr/share/misc/pci.ids",
    "/usr/share/pci.ids",
];

/// libdrm's table of AMD marketing names, keyed by device and revision. Better
/// than `pci.ids` for AMD, which names a whole family at once.
const AMDGPU_IDS: &str = "/usr/share/libdrm/amdgpu.ids";

const GIB: u64 = 1 << 30;

/// Every graphics card, as `AMD RX 7600 XT [Discrete]`, from sysfs and the id
/// databases. No subprocess, so no `lspci`.
pub(super) fn read() -> Option<String> {
    let mut cards: Vec<String> = fs::read_dir("/sys/class/drm")
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        // `card0`, not the `card0-HDMI-A-1` connectors beside it.
        .filter(|n| {
            n.strip_prefix("card")
                .is_some_and(|r| r.bytes().all(|b| b.is_ascii_digit()))
        })
        .collect();
    cards.sort();
    // Read once and only when there is a card to name.
    let pci_ids = PCI_IDS.iter().find_map(|p| fs::read_to_string(p).ok());
    let amd_ids = fs::read_to_string(AMDGPU_IDS).ok();

    let names: Vec<String> = cards
        .iter()
        .filter_map(|card| {
            let dev = Path::new("/sys/class/drm").join(card).join("device");
            let hex = |file: &str| read_trimmed(dev.join(file)).and_then(|v| parse_hex(&v));
            let vendor = hex("vendor")? as u16;
            let device = hex("device")? as u16;
            let revision = hex("revision").unwrap_or(0) as u8;

            let name = gpu_name(
                vendor,
                device,
                revision,
                pci_ids.as_deref(),
                amd_ids.as_deref(),
            );

            // The bus is in the device's resolved path: `0000:12:00.0`.
            let on_root_bus = fs::canonicalize(&dev)
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .is_some_and(|addr| addr.get(5..7) == Some("00"));
            let vram =
                read_trimmed(dev.join("mem_info_vram_total")).and_then(|v| v.parse::<u64>().ok());

            Some(match gpu_kind(vendor, &name, on_root_bus, vram) {
                Some(kind) => format!("{name} [{kind}]"),
                None => name,
            })
        })
        .collect();

    (!names.is_empty()).then(|| names.join(", "))
}

fn parse_hex(text: &str) -> Option<u32> {
    u32::from_str_radix(text.trim().trim_start_matches("0x"), 16).ok()
}

/// `AMD RX 7600 XT`, `NVIDIA GeForce RTX 4070`, `Intel Iris Xe Graphics`.
fn gpu_name(
    vendor: u16,
    device: u16,
    revision: u8,
    pci_ids: Option<&str>,
    amd_ids: Option<&str>,
) -> String {
    let (vendor_name, device_name) = pci_ids
        .map(|text| pci_lookup(text, vendor, device))
        .unwrap_or((None, None));

    match vendor {
        // libdrm already says "AMD Radeon RX 7600 XT". "Radeon" is dropped to
        // keep the row short; the vendor and model say the rest.
        0x1002 => amd_ids
            .and_then(|ids| amdgpu_name(ids, device, revision))
            .map(|n| n.replace("Radeon ", ""))
            .or_else(|| device_name.map(|d| format!("AMD {}", bracketed(&d))))
            .unwrap_or_else(|| format!("AMD [{device:04x}]")),
        0x10de => format!(
            "NVIDIA {}",
            device_name.map_or_else(|| format!("[{device:04x}]"), |d| bracketed(&d))
        ),
        0x8086 => format!(
            "Intel {}",
            device_name.map_or_else(|| format!("[{device:04x}]"), |d| bracketed(&d))
        ),
        _ => match (vendor_name, device_name) {
            (Some(v), Some(d)) => format!("{} {}", bracketed(&v), bracketed(&d)),
            (Some(v), None) => format!("{} [{device:04x}]", bracketed(&v)),
            _ => format!("[{vendor:04x}:{device:04x}]"),
        },
    }
}

/// What is inside the last square brackets, which is the part of a PCI name
/// that people know the card by: `Navi 33 [Radeon RX 7600]` is a RX 7600.
fn bracketed(name: &str) -> String {
    match (name.rfind('['), name.rfind(']')) {
        (Some(open), Some(close)) if open < close => name[open + 1..close].to_string(),
        _ => name.to_string(),
    }
}

/// Vendor and device names from a `pci.ids` file: vendors start a line, their
/// devices follow indented by one tab.
fn pci_lookup(text: &str, vendor: u16, device: u16) -> (Option<String>, Option<String>) {
    let vendor_key = format!("{vendor:04x}  ");
    let device_key = format!("\t{device:04x}  ");

    let mut lines = text.lines();
    let Some(vendor_line) = lines.find(|l| l.starts_with(&vendor_key)) else {
        return (None, None);
    };
    let vendor_name = vendor_line[vendor_key.len()..].trim().to_string();

    let device_name = lines
        // Devices run until the next vendor, which is a line not led by a tab.
        .take_while(|l| l.starts_with('\t') || l.starts_with('#') || l.is_empty())
        .find_map(|l| l.strip_prefix(&device_key))
        .map(|n| n.trim().to_string());

    (Some(vendor_name), device_name)
}

/// The marketing name for an AMD device and revision, from `amdgpu.ids`, whose
/// lines read `7480,<TAB>C0,<TAB>AMD Radeon RX 7600 XT`.
///
/// The table is keyed by revision and has gaps, so when this exact revision is
/// absent the device's other rows are used, but only if they all agree. A
/// device id that is both an RX 7600 XT and an RX 7700S has no single answer,
/// and a wrong model is worse than the family name the caller falls back to.
fn amdgpu_name(ids: &str, device: u16, revision: u8) -> Option<String> {
    let mut same_device: Vec<(u32, &str)> = Vec::new();

    for line in ids.lines().filter(|l| !l.starts_with('#')) {
        let mut f = line.splitn(3, ',').map(str::trim);
        let (Some(dev), Some(rev), Some(name)) = (f.next(), f.next(), f.next()) else {
            continue;
        };
        let (Some(dev), Some(rev)) = (parse_hex(dev), parse_hex(rev)) else {
            continue;
        };
        if dev != u32::from(device) {
            continue;
        }
        if rev == u32::from(revision) {
            return Some(name.to_string());
        }
        same_device.push((rev, name));
    }

    let (_, first) = *same_device.first()?;
    same_device
        .iter()
        .all(|(_, name)| *name == first)
        .then(|| first.to_string())
}

/// Whether a card is discrete or integrated, when that can be told from what
/// sysfs offers. `None` is an answer: a wrong tag is worse than no tag.
fn gpu_kind(vendor: u16, name: &str, on_root_bus: bool, vram: Option<u64>) -> Option<&'static str> {
    match vendor {
        0x10de => Some("Discrete"),
        // Intel's integrated graphics sit on the root bus; Arc cards do not.
        0x8086 => Some(if on_root_bus {
            "Integrated"
        } else {
            "Discrete"
        }),
        0x1002 => match vram {
            // An APU carves a small slice of system memory out as "VRAM".
            Some(v) if v < GIB => Some("Integrated"),
            // Discrete families are named as such. A big carve-out on an APU
            // (some handhelds) must not be taken for a card with its own.
            Some(_)
                if ["RX ", "Pro W", "Pro V", "Radeon VII", "Instinct"]
                    .iter()
                    .any(|m| name.contains(m)) =>
            {
                Some("Discrete")
            }
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PCI_SAMPLE: &str = "\
# comment
1002  Advanced Micro Devices, Inc. [AMD/ATI]
\t164e  Raphael
\t\t1002 164e  Some Subsystem
\t7480  Navi 33 [Radeon RX 7600/7600 XT/7600M XT]
10de  NVIDIA Corporation
\t2786  AD104 [GeForce RTX 4070]
";

    #[test]
    fn pci_names_are_found_under_their_own_vendor() {
        let (v, d) = pci_lookup(PCI_SAMPLE, 0x10de, 0x2786);
        assert_eq!(v.as_deref(), Some("NVIDIA Corporation"));
        assert_eq!(d.as_deref(), Some("AD104 [GeForce RTX 4070]"));

        // A device id that exists, but under a different vendor, must miss.
        assert_eq!(pci_lookup(PCI_SAMPLE, 0x10de, 0x7480).1, None);
        assert_eq!(pci_lookup(PCI_SAMPLE, 0xbeef, 0x0001), (None, None));
    }

    #[test]
    fn the_part_of_a_name_people_know_a_card_by_is_the_bracket() {
        assert_eq!(bracketed("AD104 [GeForce RTX 4070]"), "GeForce RTX 4070");
        assert_eq!(bracketed("Raphael"), "Raphael");
    }

    #[test]
    fn amd_cards_use_the_marketing_name_for_their_revision() {
        let ids = "# header\n7480,\tC0,\tAMD Radeon RX 7600 XT\n7480,\tCF,\tAMD Radeon RX 7600\n";
        assert_eq!(
            amdgpu_name(ids, 0x7480, 0xc0).as_deref(),
            Some("AMD Radeon RX 7600 XT")
        );
        assert_eq!(
            amdgpu_name(ids, 0x7480, 0xcf).as_deref(),
            Some("AMD Radeon RX 7600")
        );
        assert_eq!(
            amdgpu_name(ids, 0x7480, 0x01),
            None,
            "two models share this device id, so an unlisted revision is unknown"
        );
    }

    #[test]
    fn an_unlisted_revision_borrows_the_name_when_every_row_agrees() {
        // Seen on a real Raphael iGPU: revision C5, listed D8 to DC, all 610M.
        let ids = "164E,\tD8,\tAMD Radeon 610M\n164E,\tD9,\tAMD Radeon 610M\n";
        assert_eq!(
            amdgpu_name(ids, 0x164e, 0xc5).as_deref(),
            Some("AMD Radeon 610M")
        );
        // An exact match still beats the shared name.
        let mixed = "164E,\tC5,\tAMD Radeon Graphics\n164E,\tD8,\tAMD Radeon 610M\n";
        assert_eq!(
            amdgpu_name(mixed, 0x164e, 0xc5).as_deref(),
            Some("AMD Radeon Graphics")
        );
    }

    #[test]
    fn the_gpu_row_reads_like_the_reference() {
        let ids = "7480,\tC0,\tAMD Radeon RX 7600 XT\n";
        assert_eq!(
            gpu_name(0x1002, 0x7480, 0xc0, Some(PCI_SAMPLE), Some(ids)),
            "AMD RX 7600 XT"
        );
        assert_eq!(
            gpu_name(0x10de, 0x2786, 0, Some(PCI_SAMPLE), None),
            "NVIDIA GeForce RTX 4070"
        );
    }

    #[test]
    fn a_card_without_id_databases_still_gets_an_honest_name() {
        assert_eq!(gpu_name(0x1002, 0x7480, 0xc0, None, None), "AMD [7480]");
        assert_eq!(gpu_name(0xbeef, 0x0001, 0, None, None), "[beef:0001]");
    }

    #[test]
    fn discrete_and_integrated_are_only_claimed_when_it_can_be_told() {
        let big = Some(16 * GIB);
        let small = Some(GIB / 2);
        assert_eq!(
            gpu_kind(0x1002, "AMD RX 7600 XT", false, big),
            Some("Discrete")
        );
        assert_eq!(
            gpu_kind(0x1002, "AMD 610M", true, small),
            Some("Integrated")
        );
        assert_eq!(gpu_kind(0x10de, "NVIDIA X", false, None), Some("Discrete"));
        assert_eq!(gpu_kind(0x8086, "Intel X", true, None), Some("Integrated"));
        assert_eq!(gpu_kind(0x8086, "Intel Arc", false, None), Some("Discrete"));

        // An APU with a large carve-out is not a card with its own memory.
        assert_eq!(gpu_kind(0x1002, "AMD Graphics", false, big), None);
        assert_eq!(gpu_kind(0xbeef, "Unknown", false, big), None);
    }
}
