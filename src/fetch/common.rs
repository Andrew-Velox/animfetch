//! Small things every backend needs.

use std::fmt::Write as _;

/// The eight ANSI colours as solid blocks, so you can see your terminal theme.
pub(super) fn swatches() -> String {
    let mut out = String::with_capacity(8 * 12);
    for code in 0..8 {
        let _ = write!(out, "\x1b[3{code}m███");
    }
    out.push_str(crate::color::RESET);
    out
}

pub(super) fn arch() -> Option<String> {
    // std has no uname, but this is the same string.
    Some(std::env::consts::ARCH.to_string()).filter(|s| !s.is_empty())
}

pub(super) fn is_shell(name: &str) -> bool {
    // Login shells appear as "-bash".
    let name = name.trim_start_matches('-');
    matches!(
        name,
        "sh" | "bash" | "zsh" | "fish" | "dash" | "ksh" | "tcsh" | "nu" | "elvish"
    )
}

pub(super) fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

pub(super) fn basename_of_env(key: &str) -> Option<String> {
    let value = env(key)?;
    Some(value.rsplit('/').next().unwrap_or(&value).to_string())
}

/// How long ago the root filesystem was created, which is when the system was
/// installed. Not every filesystem records a birth time, hence the `Option`.
pub(super) fn os_age() -> Option<u64> {
    let born = std::fs::metadata("/").ok()?.created().ok()?;
    std::time::SystemTime::now()
        .duration_since(born)
        .ok()
        .map(|d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_shells_are_recognised() {
        assert!(is_shell("bash"));
        assert!(is_shell("-bash"));
        assert!(is_shell("zsh"));
        assert!(!is_shell("kitty"));
        assert!(!is_shell("alacritty"));
    }
}
