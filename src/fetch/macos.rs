//! macOS system information, from `sysctl`, Mach host statistics and a couple
//! of well-known files. Never subprocesses, same as the Linux backend and for
//! the same reason: this runs on every new shell.

use std::fs;

use super::common::{arch, basename_of_env, env, os_age, swatches};
use super::format::{humanize_age, humanize_duration};
use super::{Item, Usage};
use crate::config::Module;

/// Collect the requested modules, skipping ones this machine can't answer.
pub fn collect(modules: &[Module]) -> Vec<Item> {
    let mut items = Vec::with_capacity(modules.len());

    for &module in modules {
        // Modules that are several rows, not one. macOS has the root volume
        // and no battery reader yet, so each is at most one row or none.
        match module {
            Module::Disks => {
                items.extend(Item::usage(
                    "Disk",
                    disk("/").map(|u| u.with_name(Some("/".into()))),
                ));
                continue;
            }
            // Needs IOKit, which this backend does not link.
            Module::Battery => continue,
            _ => {}
        }

        let item = match module {
            Module::Disks | Module::Battery => None, // handled above
            // Not a system fact: the caller puts the configured rows here.
            Module::Custom => None,
            Module::Os => Item::read("OS", os()),
            Module::Host => Item::read("Host", sysctl_string("hw.model")),
            Module::Kernel => Item::read("Kernel", sysctl_string("kern.osrelease")),
            Module::Uptime => uptime().map(|s| Item::duration("Uptime", s, humanize_duration(s))),
            Module::Packages => Item::read("Packages", packages()),
            Module::Shell => Item::read("Shell", basename_of_env("SHELL")),
            // The compositor is not configurable on macOS, so this is a
            // constant rather than a lookup.
            Module::Wm => Item::read("WM", Some("Quartz Compositor".into())),
            Module::Terminal => Item::read("Terminal", env("TERM_PROGRAM").or_else(|| env("TERM"))),
            Module::Cpu => Item::read("CPU", cpu()),
            // Not read here yet: it needs IOKit, and a row this machine cannot
            // answer is skipped rather than guessed.
            Module::Gpu => None,
            Module::Memory => Item::usage("Memory", memory()),
            Module::Swap => Item::usage("Swap", swap()),
            Module::Disk => Item::usage("Disk", disk("/")),
            // Needs CoreGraphics, which this backend does not link.
            Module::Display => None,
            // macOS keeps its UI font in a preferences database, not a file.
            Module::Font => None,
            Module::OsAge => os_age().map(|s| Item::duration("OS Age", s, humanize_age(s))),
            // Temperature lives behind SMC, which this backend does not read.
            Module::Temperature => None,
            Module::ShellPath => Item::read("Shell Path", env("SHELL")),
            Module::Colors => Item::read("", Some(swatches())),
        };
        items.extend(item);
    }

    items
}

pub fn hostname() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: gethostname writes at most `len` bytes into `buf` and
    // NUL-terminates on success; failure is reported by the return value.
    let ok = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } == 0;
    if !ok {
        return "localhost".into();
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    let name = String::from_utf8_lossy(&buf[..end]);
    // Bonjour appends ".local"; the short name is what a prompt shows.
    name.split('.').next().unwrap_or(&name).to_string()
}

// ---------------------------------------------------------------------------
// Modules
// ---------------------------------------------------------------------------

fn os() -> Option<String> {
    // sw_vers reads this plist; reading it directly skips the subprocess. The
    // format is stable enough that a plain scan beats a plist parser.
    let plist = fs::read_to_string("/System/Library/CoreServices/SystemVersion.plist").ok()?;
    let name = plist_value(&plist, "ProductName").unwrap_or_else(|| "macOS".into());
    let version = plist_value(&plist, "ProductVersion")?;

    match arch() {
        Some(arch) => Some(format!("{name} {version} {arch}")),
        None => Some(format!("{name} {version}")),
    }
}

fn uptime() -> Option<u64> {
    let boot: libc::timeval = sysctl_struct("kern.boottime")?;
    // SAFETY: time with a null pointer only returns the clock.
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let secs = now.checked_sub(boot.tv_sec)?;
    (secs >= 0).then_some(secs as u64)
}

fn packages() -> Option<String> {
    // Homebrew: one directory per installed keg. Apple Silicon and Intel use
    // different prefixes.
    for cellar in ["/opt/homebrew/Cellar", "/usr/local/Cellar"] {
        if let Ok(entries) = fs::read_dir(cellar) {
            let n = entries
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .count();
            if n > 0 {
                return Some(format!("{n} (brew)"));
            }
        }
    }
    None
}

fn cpu() -> Option<String> {
    let model = sysctl_string("machdep.cpu.brand_string")?;
    let cores: i32 = sysctl_struct("hw.logicalcpu")?;
    Some(format!("{model} ({cores})"))
}

fn memory() -> Option<Usage> {
    let total: u64 = sysctl_struct("hw.memsize")?;
    let page: u64 = sysctl_struct::<i64>("hw.pagesize")? as u64;

    let vm = vm_statistics()?;
    // App + wired + compressed is what Activity Monitor calls "used"; free and
    // reclaimable cache are left out, matching the MemAvailable maths on Linux.
    let used = (vm.active_count as u64 + vm.wire_count as u64 + vm.compressor_page_count as u64)
        .checked_mul(page)?;

    Some(Usage::new(used / 1024, total / 1024))
}

fn swap() -> Option<Usage> {
    let usage: XswUsage = sysctl_struct("vm.swapusage")?;
    Some(Usage::new(usage.xsu_used / 1024, usage.xsu_total / 1024))
}

fn disk(mount: &str) -> Option<Usage> {
    let path = std::ffi::CString::new(mount).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };

    // SAFETY: valid C string, correctly sized writable statvfs.
    if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
        return None;
    }

    let frsize = stat.f_frsize as u64;
    let total = stat.f_blocks as u64 * frsize;
    let avail = stat.f_bavail as u64 * frsize;
    let used = total.checked_sub(avail)?;
    (total > 0).then(|| Usage::new(used / 1024, total / 1024).with_note(fs_type(mount)))
}

/// `apfs`, `hfs`: the filesystem mounted at `mount`.
fn fs_type(mount: &str) -> Option<String> {
    let path = std::ffi::CString::new(mount).ok()?;
    // SAFETY: valid C string, correctly sized writable statfs.
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    let bytes: Vec<u8> = stat
        .f_fstypename
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8(bytes).ok().filter(|s| !s.is_empty())
}

// ---------------------------------------------------------------------------
// sysctl and Mach plumbing
// ---------------------------------------------------------------------------

/// A string-valued sysctl, e.g. `hw.model`.
fn sysctl_string(name: &str) -> Option<String> {
    let cname = std::ffi::CString::new(name).ok()?;
    let mut len: libc::size_t = 0;

    // SAFETY: a null output buffer asks only for the required length.
    if unsafe {
        libc::sysctlbyname(
            cname.as_ptr(),
            std::ptr::null_mut(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return None;
    }

    let mut buf = vec![0u8; len];
    // SAFETY: `buf` is exactly the size the kernel just reported.
    if unsafe {
        libc::sysctlbyname(
            cname.as_ptr(),
            buf.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return None;
    }

    buf.truncate(len);
    while buf.last() == Some(&0) {
        buf.pop();
    }
    let s = String::from_utf8_lossy(&buf).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// A fixed-size sysctl value, e.g. `kern.boottime` as a `timeval`.
///
/// Refuses a size mismatch rather than reading half a struct: a kernel that
/// disagrees about the layout should yield "no value", not garbage numbers.
fn sysctl_struct<T: Copy>(name: &str) -> Option<T> {
    let cname = std::ffi::CString::new(name).ok()?;
    let mut value = std::mem::MaybeUninit::<T>::uninit();
    let mut len = std::mem::size_of::<T>() as libc::size_t;

    // SAFETY: the buffer is exactly `len` bytes and the kernel writes at most
    // that; failure and truncation are both reported.
    let ok = unsafe {
        libc::sysctlbyname(
            cname.as_ptr(),
            value.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    } == 0;

    if !ok || len != std::mem::size_of::<T>() {
        return None;
    }
    // SAFETY: the kernel filled all `len` bytes and T is Copy.
    Some(unsafe { value.assume_init() })
}

/// `vm.swapusage`, from <sys/sysctl.h>.
#[repr(C)]
#[derive(Clone, Copy)]
struct XswUsage {
    xsu_total: u64,
    xsu_avail: u64,
    xsu_used: u64,
    xsu_pagesize: u32,
    xsu_encrypted: u32,
}

/// `vm_statistics64`, from <mach/vm_statistics.h>. Field order and widths must
/// match the header exactly; `sysctl_struct`-style size checking is done via
/// the count the kernel returns.
#[repr(C)]
#[derive(Clone, Copy)]
struct VmStatistics64 {
    free_count: u32,
    active_count: u32,
    inactive_count: u32,
    wire_count: u32,
    zero_fill_count: u64,
    reactivations: u64,
    pageins: u64,
    pageouts: u64,
    faults: u64,
    cow_faults: u64,
    lookups: u64,
    hits: u64,
    purges: u64,
    purgeable_count: u32,
    speculative_count: u32,
    decompressions: u64,
    compressions: u64,
    swapins: u64,
    swapouts: u64,
    compressor_page_count: u32,
    throttled_count: u32,
    external_page_count: u32,
    internal_page_count: u32,
    total_uncompressed_pages_in_compressor: u64,
}

const HOST_VM_INFO64: libc::c_int = 4;

unsafe extern "C" {
    fn mach_host_self() -> libc::c_uint;
    fn host_statistics64(
        host: libc::c_uint,
        flavor: libc::c_int,
        info: *mut libc::c_int,
        count: *mut libc::c_uint,
    ) -> libc::c_int;
}

fn vm_statistics() -> Option<VmStatistics64> {
    let mut stats = std::mem::MaybeUninit::<VmStatistics64>::uninit();
    let mut count = (std::mem::size_of::<VmStatistics64>() / std::mem::size_of::<libc::c_int>())
        as libc::c_uint;

    // SAFETY: `stats` is exactly `count` machine words and the kernel writes at
    // most that many; KERN_SUCCESS (0) is required before reading it back.
    let ok = unsafe {
        host_statistics64(
            mach_host_self(),
            HOST_VM_INFO64,
            stats.as_mut_ptr().cast(),
            &mut count,
        )
    } == 0;

    // SAFETY: the kernel reported success, so the struct is initialised.
    ok.then(|| unsafe { stats.assume_init() })
}

/// The `<string>` following `<key>{key}</key>` in a plist.
fn plist_value(plist: &str, key: &str) -> Option<String> {
    let tag = format!("<key>{key}</key>");
    let rest = &plist[plist.find(&tag)? + tag.len()..];
    let start = rest.find("<string>")? + "<string>".len();
    let end = rest.find("</string>")?;
    let value = rest[start..end].trim();
    (!value.is_empty()).then(|| value.to_string())
}
