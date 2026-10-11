//! System information.
//!
//! This module holds what every platform shares. Where the numbers actually
//! come from lives in a backend module chosen below, so porting means writing
//! one sibling of `linux` and changing nothing else.
//!
//! - [`item`]: the row type, and the numbers a row can carry
//! - [`detail`]: fuller values for the styles that want them
//! - [`live`]: re-reading the rows that move
//! - [`mod@format`], [`common`]: what the backends share

mod command;
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
use crate::rows::{CustomRow, Source};

/// `user@host`, shown above the info rows.
pub fn title() -> String {
    let user = common::env("USER")
        .or_else(|| common::env("LOGNAME"))
        .unwrap_or_else(|| "user".into());
    format!("{user}@{}", sys::hostname())
}

/// One piece of the info pane, worked out before any of it is read so that
/// every command can be started together.
enum Step<'a> {
    /// Modules the system answers, read together.
    System(Vec<Module>),
    /// A built-in row under the user's name, the system's reading kept.
    Renamed(Module, &'a str),
    /// A built-in row whose text is the user's own, under its usual label so
    /// it keeps its icon and its place, and under `name` if they gave one.
    Said {
        label: &'static str,
        name: Option<&'a str>,
        source: &'a Source,
    },
    /// A row of your own, from `custom`.
    Own(&'a CustomRow),
}

impl Step<'_> {
    /// The command this step needs run, if it is one.
    fn command(&self) -> Option<&str> {
        match self {
            Step::Said {
                source: Source::Command(line),
                ..
            }
            | Step::Own(CustomRow {
                source: Source::Command(line),
                ..
            }) => Some(line),
            _ => None,
        }
    }
}

/// What `modules`, `custom` and `overrides` add up to, in order.
///
/// A custom row with no label is left out: an empty label is how a swatch
/// row is told from a real one.
fn plan(cfg: &Config) -> Vec<Step<'_>> {
    let mut steps = Vec::new();
    // The system's modules between two of your own rows are read together.
    let mut run: Vec<Module> = Vec::new();
    let flush = |steps: &mut Vec<Step<'_>>, run: &mut Vec<Module>| {
        if !run.is_empty() {
            steps.push(Step::System(std::mem::take(run)));
        }
    };

    for &module in cfg.modules() {
        if module == Module::Custom {
            flush(&mut steps, &mut run);
            steps.extend(
                cfg.custom
                    .iter()
                    .filter(|row| !row.label.trim().is_empty())
                    .map(Step::Own),
            );
            continue;
        }

        let changed = cfg.overrides.get(&module).zip(module.label());
        match changed.map(|(over, label)| (over, label, &over.source, over.label.as_deref())) {
            Some((_, label, Some(source), name)) => {
                flush(&mut steps, &mut run);
                steps.push(Step::Said {
                    label,
                    name,
                    source,
                });
            }
            Some((_, _, None, Some(name))) => {
                flush(&mut steps, &mut run);
                steps.push(Step::Renamed(module, name));
            }
            _ => run.push(module),
        }
    }
    flush(&mut steps, &mut run);
    steps
}

/// Every row the info pane shows: the system's, the rows the user wrote in
/// the config put where `custom` stands in `modules`, and the changes they
/// asked for to the system's own.
///
/// Commands are all started before any is waited for, so a fetch with five of
/// them takes as long as the slowest. A command that prints nothing or runs
/// out of time leaves its row out, as a module this machine cannot answer does.
pub fn gather(cfg: &Config) -> Vec<Item> {
    let detail = cfg.info_style.wants_detail();
    let steps = plan(cfg);

    let lines: Vec<&str> = steps.iter().filter_map(Step::command).collect();
    // One answer per command step, in the order they come up below.
    let mut answers = command::run_all(&lines, cfg.command_timeout()).into_iter();
    let mut text_of = |source: &Source| match source {
        Source::Text(text) => Some(text.clone()),
        Source::Command(_) => answers.next().flatten(),
    };

    let mut items = Vec::new();
    for step in steps {
        match step {
            Step::System(run) => items.extend(collect(&run, detail)),
            Step::Renamed(module, name) => {
                items.extend(
                    collect(&[module], detail)
                        .into_iter()
                        .map(|item| item.renamed(name)),
                );
            }
            Step::Said {
                label,
                name,
                source,
            } => {
                if let Some(text) = text_of(source) {
                    let row = Item::new(label, text);
                    items.push(match name {
                        Some(name) => row.renamed(name),
                        None => row,
                    });
                }
            }
            Step::Own(row) => {
                if let Some(text) = text_of(&row.source) {
                    items.push(Item::custom(&row.label, &text));
                }
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(toml: &str) -> Config {
        toml::from_str(toml).unwrap()
    }

    fn rows(cfg: &Config) -> Vec<(&'static str, &'static str, String)> {
        gather(cfg)
            .into_iter()
            .map(|i| (i.label, i.name(), i.value))
            .collect()
    }

    #[test]
    fn a_built_in_row_keeps_the_system_reading_under_the_name_given() {
        let plain = rows(&cfg("modules = [\"kernel\"]"));
        let renamed = rows(&cfg(
            "modules = [\"kernel\"]\n[overrides.kernel]\nlabel = \"Core\"",
        ));

        // The row is the same row, the value the same value: only what it is
        // called differs, and what it *is* (its label) does not move.
        assert_eq!(plain.len(), 1, "this machine has a kernel to read");
        assert_eq!(renamed.len(), 1);
        assert_eq!(renamed[0].0, "Kernel");
        assert_eq!(renamed[0].1, "Core");
        assert_eq!(renamed[0].2, plain[0].2);
        assert_eq!(plain[0].1, "Kernel");
    }

    #[test]
    fn a_value_replaces_the_reading_and_keeps_the_rows_identity() {
        let got = rows(&cfg(
            "modules = [\"os\"]\n[overrides.os]\nvalue = \"Arch, btw\"",
        ));
        assert_eq!(got, [("OS", "OS", "Arch, btw".to_string())]);
    }

    #[test]
    fn a_value_and_a_name_together() {
        let got = rows(&cfg(
            "modules = [\"os\"]\n[overrides.os]\nlabel = \"Distro\"\nvalue = \"Arch\"",
        ));
        assert_eq!(got, [("OS", "Distro", "Arch".to_string())]);
    }

    #[test]
    fn a_command_supplies_the_value() {
        let got = rows(&cfg(
            "modules = [\"os\"]\n[overrides.os]\ncommand = \"echo from-a-command\"",
        ));
        assert_eq!(got, [("OS", "OS", "from-a-command".to_string())]);
    }

    #[test]
    fn a_row_whose_command_prints_nothing_is_left_out() {
        let got = rows(&cfg(
            "modules = [\"os\", \"custom\"]\n\
             [overrides.os]\ncommand = \"true\"\n\
             [[custom]]\nlabel = \"Nope\"\ncommand = \"exit 1\"\n\
             [[custom]]\nlabel = \"Yes\"\nvalue = \"here\"",
        ));
        assert_eq!(got, [("Yes", "Yes", "here".to_string())]);
    }

    #[test]
    fn a_row_may_be_overridden_even_where_the_machine_has_nothing_to_say() {
        // `battery` is skipped on a machine with no battery, but a row the
        // user wrote out is theirs to have wherever they run it.
        let got = rows(&cfg(
            "modules = [\"battery\"]\n[overrides.battery]\nvalue = \"mains\"",
        ));
        assert_eq!(got, [("Battery", "Battery", "mains".to_string())]);
    }

    #[test]
    fn custom_rows_and_overrides_keep_the_order_modules_gives() {
        let got = rows(&cfg(
            "modules = [\"os\", \"custom\", \"kernel\", \"shell\"]\n\
             [overrides.os]\nvalue = \"A\"\n\
             [overrides.shell]\nvalue = \"C\"\n\
             [[custom]]\nlabel = \"Own\"\ncommand = \"echo B\"",
        ));
        let values: Vec<_> = got.iter().map(|r| r.2.as_str()).collect();

        assert_eq!(got.len(), 4);
        assert_eq!(values[0], "A");
        assert_eq!(values[1], "B");
        assert_eq!(values[3], "C");
        assert_eq!(got[2].0, "Kernel", "the system's row sits between them");
    }

    #[test]
    fn answers_stay_with_their_rows_when_some_are_skipped() {
        // Three commands, the middle one silent: the third must still get its
        // own answer and not the second's slot.
        let got = rows(&cfg(
            "modules = [\"custom\"]\n\
             [[custom]]\nlabel = \"a\"\ncommand = \"echo one\"\n\
             [[custom]]\nlabel = \"b\"\ncommand = \"true\"\n\
             [[custom]]\nlabel = \"c\"\ncommand = \"echo three\"",
        ));
        assert_eq!(
            got,
            [("a", "a", "one".to_string()), ("c", "c", "three".to_string())]
        );
    }

    #[test]
    fn a_custom_row_with_no_label_is_still_skipped_and_costs_no_command() {
        // An empty label is how a swatch row is told from a real one. It must
        // not take the command's answer either, or the next row gets the wrong
        // one.
        let got = rows(&cfg(
            "modules = [\"custom\"]\n\
             [[custom]]\nlabel = \"\"\ncommand = \"echo skipped\"\n\
             [[custom]]\nlabel = \"k\"\ncommand = \"echo kept\"",
        ));
        assert_eq!(got, [("k", "k", "kept".to_string())]);
    }

    #[test]
    fn no_overrides_is_exactly_what_it_was() {
        let c = cfg("modules = [\"os\", \"kernel\", \"shell\"]");
        let before: Vec<_> = collect(c.modules(), false)
            .into_iter()
            .map(|i| (i.label, i.value))
            .collect();
        let after: Vec<_> = gather(&c).into_iter().map(|i| (i.label, i.value)).collect();
        assert_eq!(before, after);
    }

    #[test]
    fn every_name_in_the_label_table_is_what_the_backend_calls_the_row() {
        // The table is what lets a value be given to a row the machine could
        // not read, so it must agree with what the backends use when they can.
        let every = [
            Module::Os,
            Module::Host,
            Module::Kernel,
            Module::Uptime,
            Module::Packages,
            Module::Shell,
            Module::Wm,
            Module::Terminal,
            Module::Cpu,
            Module::Gpu,
            Module::Memory,
            Module::Swap,
            Module::Disk,
            Module::Disks,
            Module::Display,
            Module::Font,
            Module::Battery,
            Module::Temperature,
            Module::ShellPath,
            Module::OsAge,
        ];
        for module in every {
            for item in collect(&[module], false) {
                assert_eq!(Some(item.label), module.label(), "{module:?}");
            }
        }
    }
}
