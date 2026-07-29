//! Truecolor helpers.
//!
//! Everything here emits raw SGR escapes rather than going through crossterm's
//! command types, because the renderer builds whole lines as strings and
//! compares them between frames. Keeping colour *in* the string means the diff
//! catches colour changes for free.

use std::fmt::Write as _;

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// SGR foreground sequence, e.g. `\x1b[38;2;255;0;0m`.
    pub fn fg(self, out: &mut String) {
        let Rgb(r, g, b) = self;
        let _ = write!(out, "\x1b[38;2;{r};{g};{b}m");
    }

    fn lerp(self, other: Self, t: f32) -> Self {
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Rgb(mix(self.0, other.0), mix(self.1, other.1), mix(self.2, other.2))
    }
}

pub const RESET: &str = "\x1b[0m";

/// Parse `#rgb`, `#rrggbb`, or the same without the leading `#`.
pub fn parse_hex(s: &str) -> Option<Rgb> {
    let s = s.trim().trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();

    match s.len() {
        6 => Some(Rgb(byte(0)?, byte(2)?, byte(4)?)),
        3 => {
            // #abc expands to #aabbcc.
            let nib = |i: usize| u8::from_str_radix(&s[i..i + 1], 16).ok().map(|v| v * 17);
            Some(Rgb(nib(0)?, nib(1)?, nib(2)?))
        }
        _ => None,
    }
}

/// A colour ramp sampled by position. Deserialised straight from a list of hex
/// strings; unparseable entries are dropped rather than failing the whole load,
/// so one typo in a config does not cost you the animation.
#[derive(Debug, Clone, Default)]
pub struct Gradient {
    stops: Vec<Rgb>,
}

impl Gradient {
    pub fn new(stops: Vec<Rgb>) -> Self {
        Self { stops }
    }

    pub fn is_empty(&self) -> bool {
        self.stops.is_empty()
    }

    /// Sample at `t` in `0.0..=1.0`, interpolating between adjacent stops.
    pub fn sample(&self, t: f32) -> Option<Rgb> {
        match self.stops.len() {
            0 => None,
            1 => Some(self.stops[0]),
            n => {
                let pos = t.clamp(0.0, 1.0) * (n - 1) as f32;
                let i = (pos.floor() as usize).min(n - 2);
                Some(self.stops[i].lerp(self.stops[i + 1], pos - i as f32))
            }
        }
    }
}

impl<'de> Deserialize<'de> for Gradient {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = Vec::<String>::deserialize(d)?;
        Ok(Gradient::new(raw.iter().filter_map(|s| parse_hex(s)).collect()))
    }
}

/// A single colour in config position, with the same lenient parsing.
#[derive(Debug, Clone, Copy)]
pub struct Color(pub Rgb);

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        parse_hex(&raw)
            .map(Color)
            .ok_or_else(|| serde::de::Error::custom(format!("not a hex colour: {raw:?}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_hex_forms() {
        assert_eq!(parse_hex("#ff8000"), Some(Rgb(255, 128, 0)));
        assert_eq!(parse_hex("ff8000"), Some(Rgb(255, 128, 0)));
        assert_eq!(parse_hex("#f80"), Some(Rgb(255, 136, 0)));
        assert_eq!(parse_hex("#xyzxyz"), None);
        assert_eq!(parse_hex("#ff80"), None);
    }

    #[test]
    fn gradient_hits_its_endpoints() {
        let g = Gradient::new(vec![Rgb(0, 0, 0), Rgb(100, 100, 100), Rgb(200, 200, 200)]);
        assert_eq!(g.sample(0.0), Some(Rgb(0, 0, 0)));
        assert_eq!(g.sample(1.0), Some(Rgb(200, 200, 200)));
        assert_eq!(g.sample(0.5), Some(Rgb(100, 100, 100)));
    }

    #[test]
    fn gradient_clamps_out_of_range() {
        let g = Gradient::new(vec![Rgb(0, 0, 0), Rgb(10, 10, 10)]);
        assert_eq!(g.sample(-5.0), Some(Rgb(0, 0, 0)));
        assert_eq!(g.sample(5.0), Some(Rgb(10, 10, 10)));
        assert_eq!(Gradient::default().sample(0.5), None);
    }
}
