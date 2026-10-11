//! Pixels: decoding a PNG, and the few things a border image needs done to it.
//!
//! The border images are line art: mostly transparent, drawn in strokes a few
//! pixels wide. Everything here is written with that in mind. Resizing weights
//! each pixel by its alpha, so a transparent pixel's colour cannot tint a
//! stroke beside it, and trimming removes the margins the canvases carry, which
//! otherwise would be reserved as terminal columns.

use std::io::Cursor;

/// More pixels than any border could want; refuse rather than allocate.
const MAX_PIXELS: usize = 40_000_000;

/// Alpha below this is not ink, for finding where the drawing is.
const INK_ALPHA: u8 = 24;

/// 8-bit RGBA, straight (not premultiplied).
#[derive(Clone, Debug, PartialEq)]
pub struct Bitmap {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

impl Bitmap {
    /// Fully transparent.
    pub fn blank(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            rgba: vec![0; w * h * 4],
        }
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut decoder = png::Decoder::new(Cursor::new(bytes));
        // Palettes, transparency chunks and 16-bit samples all become 8-bit
        // colour, so there are four layouts to read and not a dozen.
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);

        let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
        let (w, h) = {
            let info = reader.info();
            (info.width as usize, info.height as usize)
        };
        if w == 0 || h == 0 || w.saturating_mul(h) > MAX_PIXELS {
            return Err(format!("{w}x{h} is not a usable size"));
        }

        let mut buf = vec![0; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
        let data = &buf[..frame.buffer_size()];

        let mut rgba = Vec::with_capacity(w * h * 4);
        match frame.color_type {
            png::ColorType::Rgba => rgba.extend_from_slice(data),
            png::ColorType::Rgb => {
                for &[r, g, b] in data.as_chunks::<3>().0 {
                    rgba.extend_from_slice(&[r, g, b, 255]);
                }
            }
            png::ColorType::GrayscaleAlpha => {
                for &[g, a] in data.as_chunks::<2>().0 {
                    rgba.extend_from_slice(&[g, g, g, a]);
                }
            }
            png::ColorType::Grayscale => {
                for &g in data {
                    rgba.extend_from_slice(&[g, g, g, 255]);
                }
            }
            png::ColorType::Indexed => return Err("palette was not expanded".into()),
        }

        (rgba.len() == w * h * 4)
            .then_some(Self { w, h, rgba })
            .ok_or_else(|| "pixel data is the wrong length".to_string())
    }

    pub fn pixel(&self, x: usize, y: usize) -> [u8; 4] {
        let i = (y * self.w + x) * 4;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }

    /// The same picture with the hue taken out of every pixel. Transparency is
    /// left as it was.
    pub fn grayscale(mut self) -> Self {
        for px in self.rgba.as_chunks_mut::<4>().0 {
            let crate::color::Rgb(y, _, _) = crate::color::Rgb(px[0], px[1], px[2]).gray();
            px[..3].fill(y);
        }
        self
    }

    /// A quarter turn clockwise: what was the top is now the right.
    pub fn rotated_cw(&self) -> Self {
        let mut out = Self::blank(self.h, self.w);
        for y in 0..self.h {
            for x in 0..self.w {
                // (x, y) lands at column `h - 1 - y`, row `x`.
                let to = (x * out.w + (self.h - 1 - y)) * 4;
                let from = (y * self.w + x) * 4;
                out.rgba[to..to + 4].copy_from_slice(&self.rgba[from..from + 4]);
            }
        }
        out
    }

    /// Cut to the box that holds the drawing, or `None` if nothing is visible.
    pub fn trimmed(&self) -> Option<Self> {
        let (mut x0, mut y0, mut x1, mut y1) = (self.w, self.h, 0, 0);
        for y in 0..self.h {
            for x in 0..self.w {
                if self.rgba[(y * self.w + x) * 4 + 3] >= INK_ALPHA {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x + 1);
                    y1 = y1.max(y + 1);
                }
            }
        }
        if x1 <= x0 || y1 <= y0 {
            return None;
        }

        let mut out = Self::blank(x1 - x0, y1 - y0);
        for y in y0..y1 {
            let from = (y * self.w + x0) * 4;
            let to = (y - y0) * out.w * 4;
            out.rgba[to..to + out.w * 4].copy_from_slice(&self.rgba[from..from + out.w * 4]);
        }
        Some(out)
    }

    /// Scaled to `w` by `h`, each new pixel the alpha-weighted average of the
    /// ones it covers.
    pub fn resized(&self, w: usize, h: usize) -> Self {
        let (w, h) = (w.max(1), h.max(1));
        let mut out = Self::blank(w, h);

        for oy in 0..h {
            let (y0, y1) = span(oy, h, self.h);
            for ox in 0..w {
                let (x0, x1) = span(ox, w, self.w);

                // Premultiplied sums: a pixel counts as much as it is visible.
                let (mut r, mut g, mut b, mut a, mut n) = (0u64, 0u64, 0u64, 0u64, 0u64);
                for y in y0..y1 {
                    for x in x0..x1 {
                        let i = (y * self.w + x) * 4;
                        let alpha = u64::from(self.rgba[i + 3]);
                        r += u64::from(self.rgba[i]) * alpha;
                        g += u64::from(self.rgba[i + 1]) * alpha;
                        b += u64::from(self.rgba[i + 2]) * alpha;
                        a += alpha;
                        n += 1;
                    }
                }

                // Nothing visible under it leaves the pixel clear.
                if let Some(red) = r.checked_div(a) {
                    let to = (oy * w + ox) * 4;
                    out.rgba[to] = red as u8;
                    out.rgba[to + 1] = (g / a) as u8;
                    out.rgba[to + 2] = (b / a) as u8;
                    out.rgba[to + 3] = (a / n) as u8;
                }
            }
        }
        out
    }

    /// Scaled to `w` by `h` for drawing in dots: a new pixel is opaque if at
    /// least `min_coverage` of the box it covers is ink, and clear otherwise,
    /// and takes the colour of the strongest pixel in that box.
    ///
    /// Averaging a stroke a pixel wide into a box ten across leaves it at a
    /// tenth of its alpha, which any threshold then throws away. Taking the
    /// strongest instead fills in anything dense: an engraved blade becomes a
    /// solid bar and thin strokes clump. Coverage keeps a stroke connected and
    /// leaves the gaps between strokes open, which is what a dot can show.
    pub fn dotted(&self, w: usize, h: usize, min_coverage: f32) -> Self {
        let (w, h) = (w.max(1), h.max(1));
        let mut out = Self::blank(w, h);

        for oy in 0..h {
            let (y0, y1) = span(oy, h, self.h);
            for ox in 0..w {
                let (x0, x1) = span(ox, w, self.w);

                let (mut ink, mut n) = (0usize, 0usize);
                let mut best = (y0 * self.w + x0) * 4;
                for y in y0..y1 {
                    for x in x0..x1 {
                        let i = (y * self.w + x) * 4;
                        n += 1;
                        if self.rgba[i + 3] >= INK_ALPHA {
                            ink += 1;
                        }
                        if self.rgba[i + 3] > self.rgba[best + 3] {
                            best = i;
                        }
                    }
                }

                if n > 0 && ink as f32 / n as f32 >= min_coverage {
                    let to = (oy * w + ox) * 4;
                    out.rgba[to..to + 3].copy_from_slice(&self.rgba[best..best + 3]);
                    out.rgba[to + 3] = 255;
                }
            }
        }
        out
    }

    /// `other` laid over this one with its top-left corner at (`x`, `y`),
    /// cut where it runs off. Both are transparent-aware: it replaces.
    pub fn paste(&mut self, other: &Bitmap, x: usize, y: usize) {
        for oy in 0..other.h.min(self.h.saturating_sub(y)) {
            let from = oy * other.w * 4;
            let n = other.w.min(self.w.saturating_sub(x));
            let to = ((y + oy) * self.w + x) * 4;
            self.rgba[to..to + n * 4].copy_from_slice(&other.rgba[from..from + n * 4]);
        }
    }
}

/// The source range `[lo, hi)` that destination index `i` of `dst` covers,
/// at least one source pixel wide so an enlargement still has something to read.
fn span(i: usize, dst: usize, src: usize) -> (usize, usize) {
    let lo = (i * src / dst).min(src - 1);
    let hi = ((i + 1) * src).div_ceil(dst).clamp(lo + 1, src);
    (lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: usize, h: usize, px: [u8; 4]) -> Bitmap {
        let mut b = Bitmap::blank(w, h);
        for chunk in b.rgba.as_chunks_mut::<4>().0 {
            *chunk = px;
        }
        b
    }

    /// A PNG made by the encoder, so decoding is tested against the real
    /// thing without a file on disk.
    fn encode(
        w: u32,
        h: u32,
        color: png::ColorType,
        data: &[u8],
        extra: impl FnOnce(&mut png::Encoder<&mut Vec<u8>>),
    ) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(color);
        enc.set_depth(png::BitDepth::Eight);
        extra(&mut enc);
        enc.write_header().unwrap().write_image_data(data).unwrap();
        out
    }

    #[test]
    fn rgba_decodes_as_it_was_written() {
        let png = encode(
            2,
            1,
            png::ColorType::Rgba,
            &[1, 2, 3, 4, 5, 6, 7, 8],
            |_| {},
        );
        let b = Bitmap::decode(&png).unwrap();

        assert_eq!((b.w, b.h), (2, 1));
        assert_eq!(b.pixel(0, 0), [1, 2, 3, 4]);
        assert_eq!(b.pixel(1, 0), [5, 6, 7, 8]);
    }

    #[test]
    fn a_palette_and_its_transparency_chunk_become_rgba() {
        // The borders are exactly this: 8-bit palette, with `tRNS`.
        let png = encode(2, 1, png::ColorType::Indexed, &[0, 1], |enc| {
            enc.set_palette(vec![255, 0, 0, 0, 255, 0]);
            enc.set_trns(vec![0, 128]);
        });
        let b = Bitmap::decode(&png).unwrap();

        assert_eq!(b.pixel(0, 0), [255, 0, 0, 0], "entry 0: red, transparent");
        assert_eq!(b.pixel(1, 0), [0, 255, 0, 128], "entry 1: green, half");
    }

    #[test]
    fn greys_and_plain_rgb_gain_the_alpha_they_lack() {
        let g = Bitmap::decode(&encode(1, 1, png::ColorType::Grayscale, &[77], |_| {})).unwrap();
        assert_eq!(g.pixel(0, 0), [77, 77, 77, 255]);

        let ga = Bitmap::decode(&encode(
            1,
            1,
            png::ColorType::GrayscaleAlpha,
            &[77, 9],
            |_| {},
        ))
        .unwrap();
        assert_eq!(ga.pixel(0, 0), [77, 77, 77, 9]);

        let rgb = Bitmap::decode(&encode(1, 1, png::ColorType::Rgb, &[1, 2, 3], |_| {})).unwrap();
        assert_eq!(rgb.pixel(0, 0), [1, 2, 3, 255]);
    }

    #[test]
    fn something_that_is_not_a_png_is_an_error_not_a_panic() {
        assert!(Bitmap::decode(b"not a png at all").is_err());
        assert!(Bitmap::decode(&[0xff, 0xd8, 0xff, 0xe0]).is_err(), "a JPEG");
        assert!(Bitmap::decode(&[]).is_err());
    }

    #[test]
    fn a_quarter_turn_clockwise_puts_the_top_on_the_right() {
        // 2 wide, 3 tall: marks at the top-left (1) and bottom-right (2).
        let mut b = Bitmap::blank(2, 3);
        b.rgba[0..4].copy_from_slice(&[1, 0, 0, 255]);
        b.rgba[(2 * 2 + 1) * 4..(2 * 2 + 1) * 4 + 4].copy_from_slice(&[2, 0, 0, 255]);

        let r = b.rotated_cw();
        assert_eq!((r.w, r.h), (3, 2), "width and height swap");
        // Top-left goes to the top-right; bottom-right to the bottom-left.
        assert_eq!(r.pixel(2, 0)[0], 1);
        assert_eq!(r.pixel(0, 1)[0], 2);
    }

    #[test]
    fn four_quarter_turns_are_no_turn() {
        let mut b = solid(3, 5, [9, 8, 7, 255]);
        b.rgba[4..8].copy_from_slice(&[1, 2, 3, 4]);
        let back = b.rotated_cw().rotated_cw().rotated_cw().rotated_cw();
        assert_eq!(back, b);
    }

    #[test]
    fn a_sword_pointing_down_points_left_once_turned() {
        // The bottom-border images are portrait: hilt (wide) at the top, blade
        // below. Turned clockwise the hilt must be on the right.
        let mut b = Bitmap::blank(5, 9);
        for x in 0..5 {
            b.rgba[(5 + x) * 4 + 3] = 255; // crossguard: row 1 of a 5-wide image
        }
        for y in 2..9 {
            b.rgba[(y * 5 + 2) * 4 + 3] = 255; // blade, running to the bottom
        }
        let r = b.rotated_cw();

        let ink_in_column = |x: usize| (0..r.h).filter(|&y| r.pixel(x, y)[3] > 0).count();
        assert!(
            ink_in_column(r.w - 1 - 1) > ink_in_column(0),
            "the guard is at the right"
        );
        assert_eq!(ink_in_column(0), 1, "and the point at the left");
    }

    #[test]
    fn trimming_keeps_the_drawing_and_drops_the_margin() {
        let mut b = Bitmap::blank(10, 10);
        b.rgba[(3 * 10 + 4) * 4..(3 * 10 + 4) * 4 + 4].copy_from_slice(&[5, 5, 5, 255]);
        b.rgba[(6 * 10 + 7) * 4..(6 * 10 + 7) * 4 + 4].copy_from_slice(&[6, 6, 6, 200]);

        let t = b.trimmed().unwrap();
        assert_eq!((t.w, t.h), (4, 4), "x 4..=7, y 3..=6");
        assert_eq!(t.pixel(0, 0), [5, 5, 5, 255]);
        assert_eq!(t.pixel(3, 3), [6, 6, 6, 200]);
    }

    #[test]
    fn a_whisper_of_alpha_is_not_ink_and_an_empty_image_is_none() {
        let mut b = Bitmap::blank(4, 4);
        b.rgba[3] = INK_ALPHA - 1;
        assert!(b.trimmed().is_none());
        assert!(Bitmap::blank(3, 3).trimmed().is_none());
    }

    #[test]
    fn resizing_averages_by_visibility_so_a_clear_pixel_cannot_tint() {
        // A white stroke beside a transparent pixel whose colour is red. The
        // average must be white, not pink.
        let mut b = Bitmap::blank(2, 1);
        b.rgba[0..4].copy_from_slice(&[255, 255, 255, 255]);
        b.rgba[4..8].copy_from_slice(&[255, 0, 0, 0]);

        let r = b.resized(1, 1);
        assert_eq!(&r.rgba[0..3], &[255, 255, 255], "colour is that of the ink");
        assert_eq!(r.rgba[3], 127, "and the coverage is half");
    }

    fn put(b: &mut Bitmap, x: usize, y: usize, px: [u8; 4]) {
        let i = (y * b.w + x) * 4;
        b.rgba[i..i + 4].copy_from_slice(&px);
    }

    #[test]
    fn a_dot_is_lit_by_how_much_of_its_box_is_ink_not_by_how_strong_the_strongest_is() {
        // A 10x10 box with 12 inked pixels is 12% covered.
        let mut b = Bitmap::blank(10, 10);
        for i in 0..12 {
            put(&mut b, i % 10, i / 10, [200, 100, 50, 255]);
        }
        assert_eq!(
            b.dotted(1, 1, 0.10).rgba,
            vec![200, 100, 50, 255],
            "12% lights at 10%"
        );
        assert_eq!(
            b.dotted(1, 1, 0.15).rgba,
            vec![0, 0, 0, 0],
            "and not at 15%"
        );
    }

    #[test]
    fn a_lone_speck_does_not_light_a_dot() {
        // One strong pixel in a hundred: the case max-pooling got wrong by
        // lighting it, and averaging got wrong by losing a real stroke.
        let mut b = Bitmap::blank(10, 10);
        put(&mut b, 4, 4, [255, 255, 255, 255]);
        assert_eq!(b.dotted(1, 1, 0.10).rgba[3], 0);
    }

    #[test]
    fn a_lit_dot_takes_the_colour_of_the_strongest_pixel_in_its_box() {
        let mut b = Bitmap::blank(2, 2);
        put(&mut b, 0, 0, [10, 10, 10, 100]);
        put(&mut b, 1, 0, [200, 0, 0, 250]);
        put(&mut b, 0, 1, [20, 20, 20, 120]);
        put(&mut b, 1, 1, [30, 30, 30, 90]);

        assert_eq!(b.dotted(1, 1, 0.5).rgba, vec![200, 0, 0, 255]);
    }

    #[test]
    fn faint_pixels_below_the_ink_threshold_do_not_count_toward_coverage() {
        let b = solid(4, 4, [255, 255, 255, INK_ALPHA - 1]);
        assert_eq!(b.dotted(1, 1, 0.01).rgba[3], 0);
        let b = solid(4, 4, [255, 255, 255, INK_ALPHA]);
        assert_eq!(b.dotted(1, 1, 0.01).rgba[3], 255);
    }

    #[test]
    fn dotting_has_the_requested_size_and_a_clear_box_stays_clear() {
        let d = Bitmap::blank(30, 50).dotted(7, 9, 0.1);
        assert_eq!((d.w, d.h), (7, 9));
        assert!(d.rgba.iter().all(|v| *v == 0));
        assert_eq!(
            Bitmap::blank(4, 4).dotted(0, 0, 0.1).w,
            1,
            "zero is clamped"
        );
    }

    #[test]
    fn resizing_down_and_back_keeps_flat_colour_and_size_is_exact() {
        let b = solid(40, 70, [10, 20, 30, 255]);
        let r = b.resized(7, 11);

        assert_eq!((r.w, r.h), (7, 11));
        assert!(
            r.rgba
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [10, 20, 30, 255])
        );
        let up = b.resized(80, 140);
        assert_eq!((up.w, up.h), (80, 140), "enlarging has something to read");
    }

    #[test]
    fn a_zero_size_is_clamped_not_a_divide_by_zero() {
        let r = solid(4, 4, [1, 1, 1, 255]).resized(0, 0);
        assert_eq!((r.w, r.h), (1, 1));
    }

    #[test]
    fn pasting_is_clipped_to_the_canvas() {
        let mut canvas = Bitmap::blank(4, 4);
        canvas.paste(&solid(3, 3, [1, 1, 1, 255]), 2, 2);

        assert_eq!(canvas.pixel(3, 3), [1, 1, 1, 255]);
        assert_eq!(canvas.pixel(1, 1), [0, 0, 0, 0]);
        assert_eq!(canvas.rgba.len(), 4 * 4 * 4, "nothing grew");
    }

    #[test]
    fn grayscale_drops_the_hue_and_leaves_transparency_alone() {
        let b = Bitmap {
            w: 2,
            h: 1,
            rgba: vec![255, 0, 0, 255, 10, 200, 30, 0],
        }
        .grayscale();

        for px in b.rgba.as_chunks::<4>().0 {
            assert!(px[0] == px[1] && px[1] == px[2], "{px:?}");
        }
        assert_eq!((b.rgba[3], b.rgba[7]), (255, 0));
    }
}
