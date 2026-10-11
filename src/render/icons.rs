//! What each row is called and drawn as, in one place.
//!
//! Every info style names its rows by the same labels, so this table is the
//! only thing that knows an icon. Icons are Nerd Font glyphs, which is why
//! styles that draw them can be told not to.

/// An icon and the names a row goes by.
pub struct Glyph {
    pub icon: &'static str,
    /// The lowercase name the boxed style uses.
    pub name: &'static str,
    /// The narrow name that stands in for the icon where there are none.
    pub short: &'static str,
    /// Two capitals, for the style that keys each row with a small chip.
    pub code: &'static str,
}

/// An icon and the space after it.
pub const ICON_COLS: usize = 2;

/// For a row with no entry: still a mark, so the column keeps its shape.
pub const GENERIC: Glyph = Glyph {
    icon: "\u{f10c}",
    name: "",
    short: "",
    code: "??",
};

const GLYPHS: &[(&str, Glyph)] = &[
    ("OS", g("\u{f17c}", "distro", "os", "OS")),
    ("Host", g("\u{f109}", "host", "host", "HO")),
    ("Kernel", g("\u{f013}", "kernel", "kernel", "KE")),
    ("Uptime", g("\u{f017}", "uptime", "up", "UP")),
    ("OS Age", g("\u{f252}", "os age", "age", "AG")),
    ("Packages", g("\u{f1b2}", "pkgs", "pkgs", "PK")),
    ("Shell", g("\u{f489}", "shell", "shell", "SH")),
    ("WM", g("\u{f2d0}", "wm", "wm", "WM")),
    ("Terminal", g("\u{f120}", "term", "term", "TR")),
    ("CPU", g("\u{f4bc}", "cpu", "cpu", "CP")),
    ("GPU", g("\u{f2db}", "gpu", "gpu", "GP")),
    ("Memory", g("\u{f1c0}", "memory", "mem", "RA")),
    ("Swap", g("\u{f0ec}", "swap", "swap", "SW")),
    ("Disk", g("\u{f0a0}", "disk", "disk", "DI")),
    ("Display", g("\u{f108}", "display", "disp", "DP")),
    ("Battery", g("\u{f240}", "battery", "bat", "BA")),
    ("Font", g("\u{f031}", "font", "font", "FT")),
    ("Temperature", g("\u{f2c9}", "temp", "temp", "TE")),
    ("Shell Path", g("\u{f489}", "shell", "shell", "SP")),
];

const fn g(
    icon: &'static str,
    name: &'static str,
    short: &'static str,
    code: &'static str,
) -> Glyph {
    Glyph {
        icon,
        name,
        short,
        code,
    }
}

/// The glyph for a row's label, if it has one.
pub fn lookup(label: &str) -> Option<&'static Glyph> {
    GLYPHS.iter().find(|(l, _)| *l == label).map(|(_, g)| g)
}

/// The glyph for a row's label, or [`GENERIC`].
pub fn glyph(label: &str) -> &'static Glyph {
    lookup(label).unwrap_or(&GENERIC)
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn every_icon_is_one_column_so_the_layouts_can_count_on_it() {
        for (label, g) in GLYPHS {
            assert_eq!(g.icon.width(), 1, "{label}");
        }
        assert_eq!(GENERIC.icon.width(), 1);
    }

    #[test]
    fn labels_are_unique_so_a_lookup_has_one_answer() {
        for (i, (a, _)) in GLYPHS.iter().enumerate() {
            assert!(GLYPHS[i + 1..].iter().all(|(b, _)| a != b), "{a} twice");
        }
    }

    #[test]
    fn short_names_fit_the_columns_that_use_them() {
        // Several styles pad these into a column beside a tree trunk.
        for (label, g) in GLYPHS {
            assert!(g.short.width() <= 6, "{label}: {}", g.short);
            assert!(!g.name.is_empty() && !g.short.is_empty(), "{label}");
        }
    }

    #[test]
    fn key_codes_are_two_capitals_and_distinct() {
        for (label, g) in GLYPHS {
            assert_eq!(g.code.len(), 2, "{label}");
            assert!(g.code.bytes().all(|b| b.is_ascii_uppercase()), "{label}");
        }
        for (i, (a, ga)) in GLYPHS.iter().enumerate() {
            assert!(
                GLYPHS[i + 1..].iter().all(|(_, gb)| ga.code != gb.code),
                "{a}'s code is used twice"
            );
        }
    }

    #[test]
    fn an_unknown_row_still_gets_a_mark() {
        assert!(lookup("Nonsense").is_none());
        assert_eq!(glyph("Nonsense").icon, GENERIC.icon);
        assert_eq!(glyph("GPU").name, "gpu");
    }
}
