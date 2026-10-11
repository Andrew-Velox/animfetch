//! The desktop's UI font, from GTK's settings files. No subprocess, so not
//! `gsettings`: a machine that keeps its font only in dconf has no row.

use std::fs;
use std::path::PathBuf;

/// `Roboto (11pt)`, from `gtk-font-name` in the first settings file with one.
pub(super) fn read() -> Option<String> {
    candidates().into_iter().find_map(|path| {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| from_settings(&text))
    })
}

fn candidates() -> Vec<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));

    let mut found = Vec::new();
    if let Some(config) = config {
        found.push(config.join("gtk-4.0/settings.ini"));
        found.push(config.join("gtk-3.0/settings.ini"));
    }
    found.push("/etc/gtk-4.0/settings.ini".into());
    found.push("/etc/gtk-3.0/settings.ini".into());
    found
}

/// The `gtk-font-name` of a settings file, described.
fn from_settings(text: &str) -> Option<String> {
    text.lines()
        .filter_map(|l| l.split_once('='))
        .find(|(key, _)| key.trim() == "gtk-font-name")
        .and_then(|(_, value)| describe(value))
}

/// `Roboto 11` as `Roboto (11pt)`. A name with no size is left as it is.
fn describe(name: &str) -> Option<String> {
    let name = name.trim().trim_matches('"');
    // Pango may append variation settings: `Inter 11 @wght=500`.
    let name = name.split(" @").next()?.trim();
    if name.is_empty() {
        return None;
    }

    match name.rsplit_once(char::is_whitespace) {
        Some((family, size)) if size.parse::<f32>().is_ok() => {
            let family = family.trim().trim_end_matches(',').trim();
            Some(format!("{family} ({size}pt)"))
        }
        _ => Some(name.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_and_a_size_read_as_the_reference_does() {
        assert_eq!(describe("Roboto 11").as_deref(), Some("Roboto (11pt)"));
        assert_eq!(
            describe("Noto Sans Medium 10").as_deref(),
            Some("Noto Sans Medium (10pt)")
        );
    }

    #[test]
    fn a_comma_before_the_size_is_not_part_of_the_name() {
        assert_eq!(
            describe("Cantarell, 11").as_deref(),
            Some("Cantarell (11pt)")
        );
    }

    #[test]
    fn variation_settings_and_fractions_are_handled() {
        assert_eq!(
            describe("Google Sans Flex Medium 11 @opsz=11,wght=500").as_deref(),
            Some("Google Sans Flex Medium (11pt)")
        );
        assert_eq!(describe("Inter 10.5").as_deref(), Some("Inter (10.5pt)"));
    }

    #[test]
    fn a_name_without_a_size_is_kept_and_an_empty_one_is_nothing() {
        assert_eq!(describe("Sans").as_deref(), Some("Sans"));
        assert_eq!(describe("   "), None);
        // A number that is part of the name is not a size when it is alone.
        assert_eq!(describe("11").as_deref(), Some("11"));
    }

    #[test]
    fn the_setting_is_found_among_other_keys_and_comments() {
        let ini = "\
[Settings]
gtk-theme-name=Adwaita-dark
# gtk-font-name=Wrong 99
gtk-font-name = Roboto 11
gtk-icon-theme-name=Papirus
";
        assert_eq!(from_settings(ini).as_deref(), Some("Roboto (11pt)"));
        assert_eq!(from_settings("[Settings]\ngtk-theme-name=x\n"), None);
    }
}
