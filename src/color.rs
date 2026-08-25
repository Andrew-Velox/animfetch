//! Truecolor helpers. Raw SGR escapes, not crossterm commands, so colour lives
//! in the line strings and the frame diff catches colour changes for free.

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// SGR foreground sequence, e.g. `\x1b[38;2;255;0;0m`. Built by hand because
    /// a frame emits dozens and `write!` was the biggest cost of composing one.
    pub fn fg(self, out: &mut String) {
        let Rgb(r, g, b) = self;
        out.push_str("\x1b[38;2;");
        push_dec(out, r);
        out.push(';');
        push_dec(out, g);
        out.push(';');
        push_dec(out, b);
        out.push('m');
    }

    fn lerp(self, other: Self, t: f32) -> Self {
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Rgb(
            mix(self.0, other.0),
            mix(self.1, other.1),
            mix(self.2, other.2),
        )
    }
}

/// Append a byte in decimal, shortest form. Same output as `{}`.
fn push_dec(out: &mut String, n: u8) {
    if n >= 100 {
        out.push((b'0' + n / 100) as char);
    }
    if n >= 10 {
        out.push((b'0' + n / 10 % 10) as char);
    }
    out.push((b'0' + n % 10) as char);
}

pub const RESET: &str = "\x1b[0m";

/// What one `\x1b[...m` sequence asks for.
pub enum Sgr {
    Reset,
    Fg(Rgb),
    Ignore,
}

pub fn parse_sgr(params: &str) -> Sgr {
    // Empty params ("\x1b[m") means reset.
    let mut parts = params.split(';');
    match parts.next() {
        None | Some("") | Some("0") => Sgr::Reset,
        Some("38") => match parts.collect::<Vec<_>>()[..] {
            ["2", r, g, b] => match (r.parse(), g.parse(), b.parse()) {
                (Ok(r), Ok(g), Ok(b)) => Sgr::Fg(Rgb(r, g, b)),
                _ => Sgr::Ignore,
            },
            _ => Sgr::Ignore,
        },
        Some(_) => Sgr::Ignore, // 39 (default fg), bold, 256-color, ...
    }
}

/// Remove every ANSI escape sequence, leaving only printable characters.
pub fn strip_sgr(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Parse `#rgb`, `#rrggbb`, or either without the `#`. Matched on bytes, since
/// a six-byte string from a config may have no char boundary to slice at.
pub fn parse_hex(s: &str) -> Option<Rgb> {
    let nibble = |b: &u8| (*b as char).to_digit(16).map(|d| d as u8);

    match s.trim().trim_start_matches('#').as_bytes() {
        // #abc expands to #aabbcc.
        [r, g, b] => Some(Rgb(nibble(r)? * 17, nibble(g)? * 17, nibble(b)? * 17)),
        [r1, r0, g1, g0, b1, b0] => Some(Rgb(
            nibble(r1)? << 4 | nibble(r0)?,
            nibble(g1)? << 4 | nibble(g0)?,
            nibble(b1)? << 4 | nibble(b0)?,
        )),
        _ => None,
    }
}

/// A colour ramp sampled by position. Bad hex entries are dropped, so one typo
/// in a config doesn't cost you the animation.
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
        Ok(Gradient::new(
            raw.iter().filter_map(|s| parse_hex(s)).collect(),
        ))
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
    fn fg_emits_exactly_what_formatting_would() {
        // `fg` is hand-built for speed; this keeps it honest.
        for rgb in [
            Rgb(0, 0, 0),
            Rgb(255, 255, 255),
            Rgb(1, 10, 100),
            Rgb(9, 99, 199),
        ] {
            let mut out = String::new();
            rgb.fg(&mut out);
            let Rgb(r, g, b) = rgb;
            assert_eq!(out, format!("\x1b[38;2;{r};{g};{b}m"));
        }
    }

    #[test]
    fn parses_both_hex_forms() {
        assert_eq!(parse_hex("#ff8000"), Some(Rgb(255, 128, 0)));
        assert_eq!(parse_hex("ff8000"), Some(Rgb(255, 128, 0)));
        assert_eq!(parse_hex("#f80"), Some(Rgb(255, 136, 0)));
        assert_eq!(parse_hex("#xyzxyz"), None);
        assert_eq!(parse_hex("#ff80"), None);
    }

    #[test]
    fn non_ascii_is_rejected_rather_than_slicing_mid_character() {
        // Six bytes, no char boundary at byte 2. Indexing there would panic.
        assert_eq!(parse_hex("€xxx"), None);
        assert_eq!(parse_hex("#€é"), None);
        // from_str_radix used to accept a sign; a nibble must be a bare digit.
        assert_eq!(parse_hex("+f+f+f"), None);
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

    #[test]
    fn sgr_parser_reads_truecolor_and_reset() {
        assert!(matches!(
            parse_sgr("38;2;255;128;0"),
            Sgr::Fg(Rgb(255, 128, 0))
        ));
        assert!(matches!(parse_sgr("0"), Sgr::Reset));
        assert!(matches!(parse_sgr(""), Sgr::Reset));
        assert!(matches!(parse_sgr("39"), Sgr::Ignore));
    }

    #[test]
    fn strip_removes_sequences_but_keeps_art() {
        assert_eq!(strip_sgr("\x1b[38;2;1;2;3mW\x1b[0m"), "W");
        assert_eq!(strip_sgr("plain"), "plain");
    }
}
