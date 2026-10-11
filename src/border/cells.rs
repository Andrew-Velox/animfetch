//! A picture as braille cells, for a terminal that cannot show pictures.
//!
//! A braille cell is a grid of dots, two across and four down, which is four
//! times what a half-block cell holds, and a dot is a single point rather than
//! half a cell. For line art that is the difference between a stroke and a
//! smear. A cell has one colour, though, so the dots in it share the average
//! of theirs; and it cannot be blended with what is behind it, so a dot is
//! either lit or not.

use super::Bitmap;

/// Alpha at which a dot is lit.
const LIT: u8 = 48;

const RESET: &str = "\x1b[0m";

/// Columns and rows of dots in one cell.
pub const DOTS_W: usize = 2;
pub const DOTS_H: usize = 4;

/// The bit for the dot at column `dx`, row `dy` of a cell. Unicode numbers
/// braille dots down the left column then down the right, and then adds the
/// bottom row as dots 7 and 8, which is why this is not simply row-major.
const BIT: [[u8; DOTS_H]; DOTS_W] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];

/// One text line per four pixel rows, two pixel columns to a character. A
/// canvas that is not a whole number of cells is clear beyond its edge.
pub fn render(canvas: &Bitmap) -> Vec<String> {
    let (cols, rows) = (canvas.w.div_ceil(DOTS_W), canvas.h.div_ceil(DOTS_H));
    let mut lines = Vec::with_capacity(rows);

    for cy in 0..rows {
        let mut line = String::new();
        let mut painted = false;

        for cx in 0..cols {
            let (mut mask, mut r, mut g, mut b, mut n) = (0u8, 0u32, 0u32, 0u32, 0u32);

            for (dx, column) in BIT.iter().enumerate() {
                for (dy, bit) in column.iter().enumerate() {
                    let (x, y) = (cx * DOTS_W + dx, cy * DOTS_H + dy);
                    if x >= canvas.w || y >= canvas.h {
                        continue;
                    }
                    let px = canvas.pixel(x, y);
                    if px[3] >= LIT {
                        mask |= bit;
                        r += u32::from(px[0]);
                        g += u32::from(px[1]);
                        b += u32::from(px[2]);
                        n += 1;
                    }
                }
            }

            match char::from_u32(0x2800 + u32::from(mask)).filter(|_| mask != 0) {
                Some(dots) => {
                    line.push_str(&format!(
                        "\x1b[38;2;{};{};{}m{dots}{RESET}",
                        r / n,
                        g / n,
                        b / n
                    ));
                    painted = true;
                }
                None => line.push(' '),
            }
        }

        // A line with nothing in it is plain spaces, which cost less to repaint.
        lines.push(if painted { line } else { " ".repeat(cols) });
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::visible_width_for_tests as width;

    fn canvas(w: usize, h: usize, set: &[(usize, usize, [u8; 4])]) -> Bitmap {
        let mut b = Bitmap::blank(w, h);
        for &(x, y, px) in set {
            let i = (y * w + x) * 4;
            b.rgba[i..i + 4].copy_from_slice(&px);
        }
        b
    }

    const WHITE: [u8; 4] = [255, 255, 255, 255];

    /// The braille character a one-cell canvas with these dots lit makes.
    fn glyph(dots: &[(usize, usize)]) -> char {
        let set: Vec<_> = dots.iter().map(|&(x, y)| (x, y, WHITE)).collect();
        render(&canvas(2, 4, &set))[0]
            .chars()
            .find(|c| ('\u{2800}'..='\u{28ff}').contains(c))
            .expect("a braille character")
    }

    #[test]
    fn each_dot_is_the_bit_unicode_gives_it() {
        // Dots 1-3 down the left, 4-6 down the right, 7 and 8 the bottom row.
        assert_eq!(glyph(&[(0, 0)]), '\u{2801}');
        assert_eq!(glyph(&[(0, 1)]), '\u{2802}');
        assert_eq!(glyph(&[(0, 2)]), '\u{2804}');
        assert_eq!(glyph(&[(1, 0)]), '\u{2808}');
        assert_eq!(glyph(&[(1, 1)]), '\u{2810}');
        assert_eq!(glyph(&[(1, 2)]), '\u{2820}');
        assert_eq!(glyph(&[(0, 3)]), '\u{2840}');
        assert_eq!(glyph(&[(1, 3)]), '\u{2880}');
    }

    #[test]
    fn a_full_cell_is_the_full_braille_block_and_two_dots_add() {
        let all: Vec<_> = (0..2).flat_map(|x| (0..4).map(move |y| (x, y))).collect();
        assert_eq!(glyph(&all), '\u{28ff}');
        assert_eq!(glyph(&[(0, 0), (1, 3)]), '\u{2881}');
    }

    #[test]
    fn four_pixel_rows_and_two_columns_make_one_character() {
        let lines = render(&canvas(10, 12, &[(3, 5, WHITE)]));

        assert_eq!(lines.len(), 3);
        assert!(lines.iter().all(|l| width(l) == 5), "{lines:?}");
    }

    #[test]
    fn nothing_visible_is_plain_spaces_with_no_escapes() {
        let lines = render(&canvas(6, 8, &[(2, 1, [255, 255, 255, LIT - 1])]));
        assert_eq!(lines, vec!["   ".to_string(), "   ".to_string()]);
    }

    #[test]
    fn a_cell_takes_the_average_colour_of_its_lit_dots() {
        let lines = render(&canvas(
            2,
            4,
            &[(0, 0, [200, 0, 0, 255]), (1, 0, [0, 100, 0, 255])],
        ));
        assert!(lines[0].contains("38;2;100;50;0m"), "{lines:?}");
    }

    #[test]
    fn a_dot_that_is_barely_there_is_not_lit() {
        let lit = render(&canvas(2, 4, &[(0, 0, [255, 255, 255, LIT])]));
        let not = render(&canvas(2, 4, &[(0, 0, [255, 255, 255, LIT - 1])]));

        assert!(lit[0].contains('\u{2801}'));
        assert!(!not[0].contains('\u{2801}'));
    }

    #[test]
    fn a_canvas_that_is_not_whole_cells_is_clear_past_its_edge() {
        // 3 wide, 5 tall: two columns of cells, two rows.
        let lines = render(&canvas(3, 5, &[(2, 4, WHITE)]));

        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|l| width(l) == 2));
        assert!(
            lines[1].contains('\u{2801}'),
            "the last row, in its own cell"
        );
    }

    #[test]
    fn every_painted_cell_closes_its_colour() {
        // A colour left open would run on into the info beside it.
        let lines = render(&canvas(6, 4, &[(0, 0, WHITE), (5, 3, WHITE)]));
        let opens = lines[0].matches("\x1b[38;2;").count();
        assert_eq!(lines[0].matches(RESET).count(), opens);
        assert_eq!(opens, 2);
    }
}
