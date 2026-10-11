//! The animation's rows, coloured down the theme's gradient.

use crate::color::RESET;
use crate::config::Config;

/// Colour one art row from the vertical gradient.
pub fn paint_art_row(
    out: &mut String,
    row: &str,
    y: usize,
    height: usize,
    cfg: &Config,
    phase: usize,
) {
    if row.is_empty() {
        return;
    }

    // A row carrying its own escapes is authored-colour art (style `raw` over
    // frames with embedded SGR): pass it through untouched, or the gradient
    // would fight every per-cell colour.
    if row.contains('\x1b') || !cfg.color || cfg.gradient.is_empty() {
        out.push_str(row);
        return;
    }

    // Advanced by the frame counter so the ramp flows down through the art.
    let span = height.max(1);
    let offset = if cfg.gradient_scroll { phase } else { 0 };
    let t = ((y + offset) % span) as f32 / span as f32;

    if let Some(color) = cfg.gradient.sample(t) {
        color.fg(out);
        out.push_str(row);
        out.push_str(RESET);
    } else {
        out.push_str(row);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{Gradient, Rgb};
    use crate::render::test_util::plain_cfg;

    #[test]
    fn gradient_is_skipped_for_rows_carrying_authored_colour() {
        let mut cfg = plain_cfg();
        cfg.gradient = Gradient::new(vec![Rgb(255, 0, 0)]);
        cfg.color = true;

        let plain_row = "##";
        let colored_row = "\x1b[38;2;1;2;3m##\x1b[0m";

        let mut plain_out = String::new();
        paint_art_row(&mut plain_out, plain_row, 0, 1, &cfg, 0);
        assert!(
            plain_out.contains("\x1b[38;2;255;0;0m"),
            "gradient applies to plain"
        );

        let mut colored_out = String::new();
        paint_art_row(&mut colored_out, colored_row, 0, 1, &cfg, 0);
        assert_eq!(colored_out, colored_row, "authored colour must win");
    }
}
