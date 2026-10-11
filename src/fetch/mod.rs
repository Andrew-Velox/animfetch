//! System information.
//!
//! This module holds what every platform shares. Where the numbers actually
//! come from lives in a backend module chosen below, so porting means writing
//! one sibling of `linux` and changing nothing else.
//!
//! - [`item`]: the row type, and the numbers a row can carry
//! - [`detail`]: fuller values for the styles that want them
//! - [`live`]: re-reading the rows that move
//! - [`format`], [`common`]: what the backends share

mod common;
mod detail;
mod format;
mod item;
mod live;

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
     in src/fetch/; a port means writing a sibling of linux that exposes the \
     same `collect` and `hostname`."
);

pub use detail::{cpu_brand, cpu_model, cpu_short, gpu_model};
pub use format::{compact_duration, long_duration};
pub use item::{Charge, Detail, Item, Usage};
pub use live::Live;

use crate::config::{Config, Module};

/// `user@host`, shown above the info rows.
pub fn title() -> String {
    let user = common::env("USER")
        .or_else(|| common::env("LOGNAME"))
        .unwrap_or_else(|| "user".into());
    format!("{user}@{}", sys::hostname())
}

/// Every row the info pane shows: the system's, with the rows the user wrote
/// in the config put where `custom` stands in `modules`.
///
/// A custom row with no label is skipped: an empty label is how a swatch
/// row is told from a real one.
pub fn gather(cfg: &Config) -> Vec<Item> {
    let detail = cfg.info_style.wants_detail();
    let mut items = Vec::new();
    // The system's modules between two `custom`s are collected together.
    let mut run: Vec<Module> = Vec::new();

    for &module in cfg.modules() {
        if module != Module::Custom {
            run.push(module);
            continue;
        }
        items.extend(collect(&run, detail));
        run.clear();
        items.extend(
            cfg.custom
                .iter()
                .filter(|row| !row.label.trim().is_empty())
                .map(|row| Item::custom(&row.label, &row.value)),
        );
    }
    items.extend(collect(&run, detail));
    items
}

/// Collect the requested modules, skipping ones this machine can't answer.
///
/// `detail` also reads the fuller values some styles show. That is where the
/// cost is: a version of the shell or window manager means running it, which is
/// a few milliseconds the classic style has never paid.
pub fn collect(modules: &[Module], detail: bool) -> Vec<Item> {
    let mut items = sys::collect(modules);
    if detail {
        items.iter_mut().for_each(self::detail::enrich);
    }
    items
}
