//! Disks: how full the root is, and every local filesystem.

use std::fs;

use crate::fetch::{Item, Usage};

pub(super) fn disk(mount: &str) -> Option<Usage> {
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

    let used = total.checked_sub(free)?;
    let note = fs::read_to_string("/proc/self/mounts")
        .ok()
        .and_then(|mounts| fs_type(&mounts, mount));
    Some(Usage::new(used / 1024, total / 1024).with_note(note))
}

/// Filesystems that live on a disk of the machine's own. A whitelist, because
/// asking a network mount how full it is can hang, and because most of what is
/// mounted (tmpfs, overlays, `/proc`) is not a disk anyone means.
const LOCAL_FS: &[&str] = &[
    "ext2", "ext3", "ext4", "btrfs", "xfs", "f2fs", "zfs", "bcachefs", "jfs", "reiserfs", "ntfs",
    "ntfs3", "exfat", "fuseblk",
];

/// Where container runtimes keep their layers, which are not the user's disks.
const NOT_DISKS: &[&str] = &["/var/lib/docker", "/var/lib/containers", "/var/lib/snapd"];

/// Mount points of the machine's own filesystems, from a `mounts` listing, in
/// order and at most once per device. A Btrfs volume with `/` and `/home` as
/// subvolumes is mounted twice and has one size; listing it twice would be two
/// rows saying the same thing.
fn local_mounts(mounts: &str) -> Vec<String> {
    let mut seen: Vec<&str> = Vec::new();
    let mut found = Vec::new();

    for line in mounts.lines() {
        let mut f = line.split_whitespace();
        let (Some(device), Some(at), Some(kind)) = (f.next(), f.next(), f.next()) else {
            continue;
        };
        if !LOCAL_FS.contains(&kind) || seen.contains(&device) {
            continue;
        }
        // Spaces in a path are written as `\040`.
        let at = at.replace("\\040", " ");
        if NOT_DISKS.iter().any(|p| at.starts_with(p)) {
            continue;
        }
        seen.push(device);
        found.push(at);
    }
    found
}

/// One row per local filesystem, named by where it is mounted.
pub(super) fn disks() -> Vec<Item> {
    let Ok(mounts) = fs::read_to_string("/proc/self/mounts") else {
        return Vec::new();
    };
    local_mounts(&mounts)
        .into_iter()
        // Enough for any real machine; a runaway list would run off the pane.
        .take(8)
        .filter_map(|at| {
            let usage = disk(&at)?.with_name(Some(at));
            Item::usage("Disk", Some(usage))
        })
        .collect()
}

/// The filesystem mounted at `mount`. The last matching line wins, since a
/// later mount shadows an earlier one at the same point.
fn fs_type(mounts: &str, mount: &str) -> Option<String> {
    mounts
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let (_device, at, kind) = (fields.next()?, fields.next()?, fields.next()?);
            (at == mount).then(|| kind.to_string())
        })
        .next_back()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_local_filesystems_are_listed_and_each_device_once() {
        let mounts = "\
/dev/sda2 / btrfs rw,subvol=/@ 0 0
/dev/sda2 /home btrfs rw,subvol=/@home 0 0
/dev/sdb1 /mnt/data ext4 rw 0 0
/dev/sda1 /boot vfat rw 0 0
tmpfs /tmp tmpfs rw 0 0
srv:/export /nfs nfs4 rw 0 0
overlay /var/lib/docker/overlay2/x overlay rw 0 0
/dev/sdc1 /var/lib/docker ext4 rw 0 0
";
        assert_eq!(local_mounts(mounts), ["/", "/mnt/data"]);
    }

    #[test]
    fn a_mount_point_with_spaces_is_unescaped() {
        let mounts = "/dev/sdb1 /run/media/me/My\\040Disk ext4 rw 0 0\n";
        assert_eq!(local_mounts(mounts), ["/run/media/me/My Disk"]);
    }

    #[test]
    fn the_filesystem_is_the_last_thing_mounted_at_that_point() {
        let mounts = "sysfs /sys sysfs rw 0 0\n/dev/sda1 / ext4 rw 0 0\noverlay / overlay rw 0 0\n";
        assert_eq!(fs_type(mounts, "/").as_deref(), Some("overlay"));
        assert_eq!(fs_type(mounts, "/sys").as_deref(), Some("sysfs"));
        assert_eq!(fs_type(mounts, "/nope"), None);
    }
}
