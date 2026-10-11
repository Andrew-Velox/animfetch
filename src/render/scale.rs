//! Colours that carry meaning, as opposed to the theme's, which decorate.
//!
//! Fixed rather than themed: a theme's accent would say nothing about whether
//! a disk is nearly full.

use crate::color::Rgb;

const GOOD: Rgb = Rgb(0x8e, 0xc0, 0x7c);
const WARN: Rgb = Rgb(0xe5, 0xc0, 0x7b);
const BAD: Rgb = Rgb(0xe0, 0x6c, 0x75);

/// For a reading that is bad as it fills: memory, swap, a disk.
pub fn fill_color(percent: u64) -> Rgb {
    match percent {
        0..=59 => GOOD,
        60..=79 => WARN,
        _ => BAD,
    }
}

/// For a reading that is bad as it empties: a battery.
pub fn charge_color(percent: u8) -> Rgb {
    match percent {
        60..=u8::MAX => GOOD,
        30..=59 => WARN,
        _ => BAD,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_filling_reading_turns_red_and_a_draining_one_does() {
        assert_eq!(fill_color(10), GOOD);
        assert_eq!(fill_color(70), WARN);
        assert_eq!(fill_color(95), BAD);

        assert_eq!(charge_color(96), GOOD);
        assert_eq!(charge_color(40), WARN);
        assert_eq!(charge_color(12), BAD);
    }

    #[test]
    fn the_same_number_means_opposite_things() {
        // 95% full is a warning; 95% charged is not.
        assert_ne!(fill_color(95), charge_color(95));
    }

    #[test]
    fn the_scales_agree_at_their_edges() {
        assert_eq!(fill_color(59), GOOD);
        assert_eq!(fill_color(60), WARN);
        assert_eq!(fill_color(79), WARN);
        assert_eq!(fill_color(80), BAD);
        assert_eq!(charge_color(60), GOOD);
        assert_eq!(charge_color(59), WARN);
        assert_eq!(charge_color(30), WARN);
        assert_eq!(charge_color(29), BAD);
    }
}
