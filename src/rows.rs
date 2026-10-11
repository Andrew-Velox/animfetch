//! Rows the user describes in the config: where a row's text comes from, a
//! row of their own, and the changes they make to a built-in one.
//!
//! These are plain data, checked when the config is read so that a mistake is
//! reported with the file rather than found halfway through drawing.

use serde::Deserialize;

/// Where a row's text comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// This text, as written.
    Text(String),
    /// The first line a shell command prints. The command is yours: it runs
    /// with your privileges, like anything else in your shell's startup files.
    Command(String),
}

impl Source {
    /// From the two ways a config can spell one, exactly one of which may be
    /// present.
    fn from_keys(value: Option<String>, command: Option<String>) -> Result<Option<Self>, String> {
        match (value, command) {
            (Some(_), Some(_)) => Err("give either `value` or `command`, not both".into()),
            (Some(text), None) => Ok(Some(Self::Text(text))),
            (None, Some(command)) if command.trim().is_empty() => Err("`command` is empty".into()),
            (None, Some(command)) => Ok(Some(Self::Command(command))),
            (None, None) => Ok(None),
        }
    }
}

/// A row of your own text or command, for `custom`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "RawRow")]
pub struct CustomRow {
    pub label: String,
    pub source: Source,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRow {
    label: String,
    value: Option<String>,
    command: Option<String>,
}

impl TryFrom<RawRow> for CustomRow {
    type Error = String;

    fn try_from(raw: RawRow) -> Result<Self, String> {
        let source = Source::from_keys(raw.value, raw.command)?
            .ok_or("a custom row needs a `value` or a `command`")?;
        Ok(Self {
            label: raw.label,
            source,
        })
    }
}

/// What the user changes about one built-in row, for `overrides`.
///
/// Both halves are optional and independent: a row can be renamed and keep
/// the system's reading, or keep its name and say something else.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "RawOverride")]
pub struct Override {
    /// The row's name on screen.
    pub label: Option<String>,
    /// What to show in place of the system's reading.
    pub source: Option<Source>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOverride {
    label: Option<String>,
    value: Option<String>,
    command: Option<String>,
}

impl TryFrom<RawOverride> for Override {
    type Error = String;

    fn try_from(raw: RawOverride) -> Result<Self, String> {
        // An empty label is how a swatch row is told from a real one, so a
        // row cannot be given one.
        let label = match raw.label.map(|l| l.trim().to_string()) {
            Some(l) if l.is_empty() => return Err("`label` is empty".into()),
            other => other,
        };
        let source = Source::from_keys(raw.value, raw.command)?;
        if label.is_none() && source.is_none() {
            return Err("it changes nothing: give a `label`, a `value` or a `command`".into());
        }
        Ok(Self { label, source })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(toml: &str) -> Result<CustomRow, toml::de::Error> {
        toml::from_str(toml)
    }

    fn over(toml: &str) -> Result<Override, toml::de::Error> {
        toml::from_str(toml)
    }

    #[test]
    fn a_custom_row_is_text_or_a_command() {
        assert_eq!(
            row("label = 'Owner'\nvalue = 'me'").unwrap().source,
            Source::Text("me".into())
        );
        assert_eq!(
            row("label = 'IP'\ncommand = 'hostname -I'").unwrap().source,
            Source::Command("hostname -I".into())
        );
    }

    #[test]
    fn a_custom_row_is_not_both_nor_neither_nor_a_stray_key() {
        assert!(row("label = 'x'").is_err(), "no source");
        assert!(row("label = 'x'\nvalue = 'a'\ncommand = 'b'").is_err());
        assert!(row("label = 'x'\nvalue = 'a'\noops = 1").is_err());
        assert!(row("value = 'a'").is_err(), "a row needs a label");
        assert!(row("label = 'x'\ncommand = '  '").is_err(), "blank command");
    }

    #[test]
    fn an_override_may_rename_replace_or_both() {
        let renamed = over("label = 'Distro'").unwrap();
        assert_eq!(renamed.label.as_deref(), Some("Distro"));
        assert_eq!(renamed.source, None);

        let said = over("value = 'btw'").unwrap();
        assert_eq!(said.label, None);
        assert_eq!(said.source, Some(Source::Text("btw".into())));

        let both = over("label = 'Up'\ncommand = 'uptime -p'").unwrap();
        assert_eq!(both.source, Some(Source::Command("uptime -p".into())));
    }

    #[test]
    fn an_override_that_changes_nothing_or_contradicts_itself_is_refused() {
        assert!(over("").is_err(), "an empty table does nothing");
        assert!(over("value = 'a'\ncommand = 'b'").is_err());
        assert!(over("label = '  '").is_err(), "a blank label is a swatch");
        assert!(over("label = 'x'\noops = 1").is_err());
    }

    #[test]
    fn a_label_is_trimmed() {
        assert_eq!(
            over("label = '  Distro '").unwrap().label.as_deref(),
            Some("Distro")
        );
    }
}
