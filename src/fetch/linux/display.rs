//! Screens: the resolution and refresh rate each is running at, from the
//! kernel's modesetting interface.

use std::fs;
use std::path::Path;

// The kernel's modesetting ABI (`drm.h`, `drm_mode.h`). Only reading is done,
// and the two calls used need no master: asking what is set, not setting it.
#[repr(C)]
struct DrmCardRes {
    fb_id_ptr: u64,
    crtc_id_ptr: u64,
    connector_id_ptr: u64,
    encoder_id_ptr: u64,
    count_fbs: u32,
    count_crtcs: u32,
    count_connectors: u32,
    count_encoders: u32,
    min_width: u32,
    max_width: u32,
    min_height: u32,
    max_height: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct DrmModeInfo {
    /// Pixel clock in kHz.
    clock: u32,
    hdisplay: u16,
    hsync_start: u16,
    hsync_end: u16,
    htotal: u16,
    hskew: u16,
    vdisplay: u16,
    vsync_start: u16,
    vsync_end: u16,
    vtotal: u16,
    vscan: u16,
    vrefresh: u32,
    flags: u32,
    kind: u32,
    name: [u8; 32],
}

#[repr(C)]
struct DrmCrtc {
    set_connectors_ptr: u64,
    count_connectors: u32,
    crtc_id: u32,
    fb_id: u32,
    x: u32,
    y: u32,
    gamma_size: u32,
    mode_valid: u32,
    mode: DrmModeInfo,
}

/// `_IOWR('d', 0xA0, struct drm_mode_card_res)` and `_IOWR('d', 0xA1, struct
/// drm_mode_crtc)`. Spelled out, since libc has no DRM.
const DRM_IOCTL_MODE_GETRESOURCES: u32 = 0xC040_64A0;
const DRM_IOCTL_MODE_GETCRTC: u32 = 0xC068_64A1;

const DRM_MODE_FLAG_INTERLACE: u32 = 1 << 4;
const DRM_MODE_FLAG_DBLSCAN: u32 = 1 << 5;

/// `1920x1080 @ 60.00 Hz` for each display that has a mode set, read from the
/// kernel's current state rather than a monitor's preferred mode, which is what
/// sysfs and EDID would give and is not always what is running.
///
/// `None` when the nodes cannot be opened (a plain SSH session has no seat),
/// which skips the row rather than guessing.
pub(super) fn read() -> Option<String> {
    let mut nodes: Vec<String> = fs::read_dir("/dev/dri")
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| {
            n.strip_prefix("card")
                .is_some_and(|r| r.bytes().all(|b| b.is_ascii_digit()))
        })
        .collect();
    nodes.sort();

    let shown: Vec<String> = nodes
        .iter()
        .filter_map(|n| fs::File::open(Path::new("/dev/dri").join(n)).ok())
        .flat_map(|f| active_modes(&f))
        .map(|m| format!("{}x{} @ {:.2} Hz", m.hdisplay, m.vdisplay, refresh_hz(&m)))
        .collect();

    (!shown.is_empty()).then(|| shown.join(", "))
}

/// The mode of every CRTC on this device that is driving something.
fn active_modes(card: &fs::File) -> Vec<DrmModeInfo> {
    use std::os::fd::AsRawFd as _;
    let fd = card.as_raw_fd();

    // SAFETY: both structs are plain integers, valid when zeroed.
    let mut res: DrmCardRes = unsafe { std::mem::zeroed() };
    // SAFETY: `res` is the argument this request is defined to take.
    if unsafe { libc::ioctl(fd, DRM_IOCTL_MODE_GETRESOURCES as _, &mut res) } != 0 {
        return Vec::new();
    }

    // Second call fills the CRTC ids. The other counts are zeroed so the
    // kernel only reports them, rather than copying to pointers we left null.
    let mut crtcs = vec![0u32; res.count_crtcs as usize];
    res.crtc_id_ptr = crtcs.as_mut_ptr() as u64;
    (res.fb_id_ptr, res.connector_id_ptr, res.encoder_id_ptr) = (0, 0, 0);
    (res.count_fbs, res.count_connectors, res.count_encoders) = (0, 0, 0);
    // SAFETY: `crtc_id_ptr` points at `count_crtcs` writable u32s.
    if unsafe { libc::ioctl(fd, DRM_IOCTL_MODE_GETRESOURCES as _, &mut res) } != 0 {
        return Vec::new();
    }

    crtcs
        .into_iter()
        .filter_map(|id| {
            let mut crtc: DrmCrtc = unsafe { std::mem::zeroed() };
            crtc.crtc_id = id;
            // SAFETY: `crtc` is the argument this request is defined to take.
            let ok = unsafe { libc::ioctl(fd, DRM_IOCTL_MODE_GETCRTC as _, &mut crtc) } == 0;
            (ok && crtc.mode_valid != 0 && crtc.mode.hdisplay > 0).then_some(crtc.mode)
        })
        .collect()
}

/// Frames per second of a mode, from its timings. Computed rather than taken
/// from `vrefresh`, which the kernel rounds to a whole number: 59.94 Hz
/// would otherwise read as 60.
fn refresh_hz(mode: &DrmModeInfo) -> f64 {
    let total = f64::from(mode.htotal) * f64::from(mode.vtotal);
    if total == 0.0 {
        return 0.0;
    }
    let mut hz = f64::from(mode.clock) * 1000.0 / total;
    // An interlaced mode sends half the lines per pass; a doubled one repeats.
    if mode.flags & DRM_MODE_FLAG_INTERLACE != 0 {
        hz *= 2.0;
    }
    if mode.flags & DRM_MODE_FLAG_DBLSCAN != 0 {
        hz /= 2.0;
    }
    if mode.vscan > 1 {
        hz /= f64::from(mode.vscan);
    }
    hz
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timing(clock: u32, htotal: u16, vtotal: u16, flags: u32) -> DrmModeInfo {
        DrmModeInfo {
            clock,
            hdisplay: 0,
            hsync_start: 0,
            hsync_end: 0,
            htotal,
            hskew: 0,
            vdisplay: 0,
            vsync_start: 0,
            vsync_end: 0,
            vtotal,
            vscan: 0,
            vrefresh: 0,
            flags,
            kind: 0,
            name: [0; 32],
        }
    }

    #[test]
    fn the_drm_structs_match_the_kernels_sizes() {
        // The request numbers encode these sizes, and the kernel writes
        // through our pointers: a drift here is memory corruption, not a typo.
        assert_eq!(std::mem::size_of::<DrmCardRes>(), 64);
        assert_eq!(std::mem::size_of::<DrmModeInfo>(), 68);
        assert_eq!(std::mem::size_of::<DrmCrtc>(), 104);
        assert_eq!(DRM_IOCTL_MODE_GETRESOURCES >> 16 & 0x3fff, 64);
        assert_eq!(DRM_IOCTL_MODE_GETCRTC >> 16 & 0x3fff, 104);
    }

    #[test]
    fn refresh_comes_from_the_timings_not_the_rounded_field() {
        // 1920x1080 as listed by a typical monitor: 148.5 MHz, 2200 x 1125.
        assert_eq!(
            format!("{:.2}", refresh_hz(&timing(148_500, 2200, 1125, 0))),
            "60.00"
        );
        // The 59.94 variant divides the clock by 1.001.
        assert_eq!(
            format!("{:.2}", refresh_hz(&timing(148_352, 2200, 1125, 0))),
            "59.94"
        );
    }

    #[test]
    fn interlaced_and_doubled_modes_are_corrected() {
        let base = refresh_hz(&timing(74_250, 2200, 1125, 0));
        let interlaced = refresh_hz(&timing(74_250, 2200, 1125, DRM_MODE_FLAG_INTERLACE));
        let doubled = refresh_hz(&timing(74_250, 2200, 1125, DRM_MODE_FLAG_DBLSCAN));
        assert_eq!(interlaced, base * 2.0);
        assert_eq!(doubled, base / 2.0);
    }

    #[test]
    fn a_mode_with_no_timings_is_zero_not_a_division_by_zero() {
        assert_eq!(refresh_hz(&timing(0, 0, 0, 0)), 0.0);
    }
}
