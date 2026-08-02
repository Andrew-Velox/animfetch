//! Animation frames: loading, and resampling to the terminal's size.
//!
//! Art is plain text, one file per frame, read as a coverage grid rather than as
//! characters, which is what lets us rescale it.
//!
//! [`Ink::Half`] uses half blocks: solid glyphs, crisp edges, small holes
//! survive. [`Ink::Ramp`] picks by density for a classic ASCII look, but shaded
//! blocks are stipple patterns in most fonts and read as noise.

use std::fs;
use std::io;
use std::path::Path;

/// The animation used when nothing else is configured.
pub const DEFAULT_NAME: &str = "cat-run";

/// Compiled in, so a fresh install has art. Frames run in the order listed.
const BUNDLED: &[(&str, &[&str])] = &[
    (
        "cat-run",
        &[
            include_str!("../assets/anim/cat-run/00.txt"),
            include_str!("../assets/anim/cat-run/01.txt"),
            include_str!("../assets/anim/cat-run/02.txt"),
            include_str!("../assets/anim/cat-run/03.txt"),
            include_str!("../assets/anim/cat-run/04.txt"),
        ],
    ),
    (
        "cat-tail",
        &[
            include_str!("../assets/anim/cat-tail/00.txt"),
            include_str!("../assets/anim/cat-tail/01.txt"),
            include_str!("../assets/anim/cat-tail/02.txt"),
            include_str!("../assets/anim/cat-tail/03.txt"),
            include_str!("../assets/anim/cat-tail/04.txt"),
            include_str!("../assets/anim/cat-tail/05.txt"),
            include_str!("../assets/anim/cat-tail/06.txt"),
            include_str!("../assets/anim/cat-tail/07.txt"),
        ],
    ),
    (
        "fox-run",
        &[
            include_str!("../assets/anim/fox-run/00.txt"),
            include_str!("../assets/anim/fox-run/01.txt"),
            include_str!("../assets/anim/fox-run/02.txt"),
            include_str!("../assets/anim/fox-run/03.txt"),
            include_str!("../assets/anim/fox-run/04.txt"),
            include_str!("../assets/anim/fox-run/05.txt"),
            include_str!("../assets/anim/fox-run/06.txt"),
            include_str!("../assets/anim/fox-run/07.txt"),
        ],
    ),
    (
        "dolphin-run",
        &[
            include_str!("../assets/anim/dolphin-run/00.txt"),
            include_str!("../assets/anim/dolphin-run/01.txt"),
            include_str!("../assets/anim/dolphin-run/02.txt"),
            include_str!("../assets/anim/dolphin-run/03.txt"),
            include_str!("../assets/anim/dolphin-run/04.txt"),
            include_str!("../assets/anim/dolphin-run/05.txt"),
            include_str!("../assets/anim/dolphin-run/06.txt"),
            include_str!("../assets/anim/dolphin-run/07.txt"),
            include_str!("../assets/anim/dolphin-run/08.txt"),
        ],
    ),
    (
        "blackhole",
        &[
            include_str!("../assets/anim/blackhole/00.txt"),
            include_str!("../assets/anim/blackhole/01.txt"),
            include_str!("../assets/anim/blackhole/02.txt"),
            include_str!("../assets/anim/blackhole/03.txt"),
            include_str!("../assets/anim/blackhole/04.txt"),
            include_str!("../assets/anim/blackhole/05.txt"),
            include_str!("../assets/anim/blackhole/06.txt"),
            include_str!("../assets/anim/blackhole/07.txt"),
            include_str!("../assets/anim/blackhole/08.txt"),
        ],
    ),
    (
        "butterfly",
        &[
            include_str!("../assets/anim/butterfly/00.txt"),
            include_str!("../assets/anim/butterfly/01.txt"),
            include_str!("../assets/anim/butterfly/02.txt"),
            include_str!("../assets/anim/butterfly/03.txt"),
            include_str!("../assets/anim/butterfly/04.txt"),
            include_str!("../assets/anim/butterfly/05.txt"),
            include_str!("../assets/anim/butterfly/06.txt"),
            include_str!("../assets/anim/butterfly/07.txt"),
            include_str!("../assets/anim/butterfly/08.txt"),
            include_str!("../assets/anim/butterfly/09.txt"),
            include_str!("../assets/anim/butterfly/10.txt"),
            include_str!("../assets/anim/butterfly/11.txt"),
            include_str!("../assets/anim/butterfly/12.txt"),
            include_str!("../assets/anim/butterfly/13.txt"),
            include_str!("../assets/anim/butterfly/14.txt"),
            include_str!("../assets/anim/butterfly/15.txt"),
        ],
    ),
    (
        "icosahedron",
        &[
            include_str!("../assets/anim/icosahedron/00.txt"),
            include_str!("../assets/anim/icosahedron/01.txt"),
            include_str!("../assets/anim/icosahedron/02.txt"),
            include_str!("../assets/anim/icosahedron/03.txt"),
            include_str!("../assets/anim/icosahedron/04.txt"),
            include_str!("../assets/anim/icosahedron/05.txt"),
            include_str!("../assets/anim/icosahedron/06.txt"),
            include_str!("../assets/anim/icosahedron/07.txt"),
            include_str!("../assets/anim/icosahedron/08.txt"),
            include_str!("../assets/anim/icosahedron/09.txt"),
            include_str!("../assets/anim/icosahedron/10.txt"),
            include_str!("../assets/anim/icosahedron/11.txt")
        ],
    ),
    (
        "rabbit-run",
        &[
            include_str!("../assets/anim/rabbit-run/00.txt"),
            include_str!("../assets/anim/rabbit-run/01.txt"),
            include_str!("../assets/anim/rabbit-run/02.txt"),
            include_str!("../assets/anim/rabbit-run/03.txt"),
            include_str!("../assets/anim/rabbit-run/04.txt"),
        ],
    ),
    (
        "mew",
        &[
            include_str!("../assets/anim/mew/00.txt"),
            include_str!("../assets/anim/mew/01.txt"),
            include_str!("../assets/anim/mew/02.txt"),
            include_str!("../assets/anim/mew/03.txt"),
            include_str!("../assets/anim/mew/04.txt"),
            include_str!("../assets/anim/mew/05.txt"),
            include_str!("../assets/anim/mew/06.txt"),
            include_str!("../assets/anim/mew/07.txt"),
        ],
    ),
    (
        "yin-yang",
        &[
            include_str!("../assets/anim/yin-yang/00.txt"),
            include_str!("../assets/anim/yin-yang/01.txt"),
            include_str!("../assets/anim/yin-yang/02.txt"),
            include_str!("../assets/anim/yin-yang/03.txt"),
            include_str!("../assets/anim/yin-yang/04.txt"),
            include_str!("../assets/anim/yin-yang/05.txt"),
            include_str!("../assets/anim/yin-yang/06.txt"),
            include_str!("../assets/anim/yin-yang/07.txt"),
            include_str!("../assets/anim/yin-yang/08.txt"),
        ],
    ),
];

/// One frame as a binary coverage mask on a `width * height` grid.
pub struct Frame {
    width: usize,
    height: usize,
    ink: Vec<bool>,
    /// Blank cells enclosed by ink (an eye, a nostril) rather than background.
    /// Part of the drawing, so preserved rather than averaged.
    hole: Vec<bool>,
}

impl Frame {
    fn parse(text: &str) -> Self {
        let height = text.lines().count();
        let width = text.lines().map(|line| line.chars().count()).max().unwrap_or(0);

        // Straight into the rectangle; short lines keep their padding.
        let mut ink = vec![false; width * height];
        for (y, line) in text.lines().enumerate() {
            let row = &mut ink[y * width..(y + 1) * width];
            for (cell, c) in row.iter_mut().zip(line.chars()) {
                *cell = !c.is_whitespace();
            }
        }

        let hole = Self::enclosed_holes(&ink, width, height);
        Self { width, height, ink, hole }
    }

    /// Blank cells unreachable from the border. Flooding inward is what tells a
    /// deliberate hole from empty space: a gap between legs opens out, an eye
    /// doesn't.
    fn enclosed_holes(ink: &[bool], width: usize, height: usize) -> Vec<bool> {
        if width == 0 || height == 0 {
            return Vec::new();
        }

        // Seed with the border, then flood inward.
        let border = (0..width)
            .flat_map(|x| [x, (height - 1) * width + x])
            .chain((0..height).flat_map(|y| [y * width, y * width + width - 1]));

        let mut outside = vec![false; width * height];
        let mut queue: Vec<usize> = Vec::new();
        for i in border {
            if !ink[i] && !outside[i] {
                outside[i] = true;
                queue.push(i);
            }
        }

        while let Some(i) = queue.pop() {
            for n in neighbors(i, width, height) {
                if !ink[n] && !outside[n] {
                    outside[n] = true;
                    queue.push(n);
                }
            }
        }

        let mut hole: Vec<bool> = (0..width * height).map(|i| !ink[i] && !outside[i]).collect();
        Self::drop_specks(&mut hole, width, height);
        hole
    }

    /// Smallest region treated as a deliberate feature. Stray single-cell gaps
    /// would punch speckles into a solid body. An eye is six cells.
    const MIN_HOLE_CELLS: usize = 2;

    /// Erase enclosed regions smaller than [`Self::MIN_HOLE_CELLS`].
    fn drop_specks(hole: &mut [bool], width: usize, height: usize) {
        let mut seen = vec![false; width * height];
        let mut component = Vec::new();

        for start in 0..width * height {
            if !hole[start] || seen[start] {
                continue;
            }

            component.clear();
            let mut stack = vec![start];
            seen[start] = true;

            while let Some(i) = stack.pop() {
                component.push(i);
                for n in neighbors(i, width, height) {
                    if hole[n] && !seen[n] {
                        seen[n] = true;
                        stack.push(n);
                    }
                }
            }

            if component.len() < Self::MIN_HOLE_CELLS {
                for &i in &component {
                    hole[i] = false;
                }
            }
        }
    }

    /// Fraction of the source block for cell `(ox, oy)` that is inked. A hole
    /// anywhere forces 0.0, otherwise it flickers as the art moves across the
    /// output grid. Works upscaling too, where the block is one source cell.
    fn coverage(&self, ox: usize, oy: usize, out_w: usize, out_h: usize) -> f32 {
        let (x0, x1) = span(ox, out_w, self.width);
        let (y0, y1) = span(oy, out_h, self.height);

        let mut hits = 0usize;
        for y in y0..y1 {
            for x in x0..x1 {
                let i = y * self.width + x;
                if self.hole[i] {
                    return 0.0;
                }
                hits += self.ink[i] as usize;
            }
        }
        hits as f32 / ((y1 - y0) * (x1 - x0)) as f32
    }

    /// Resample to `out_w * out_h` character cells.
    pub fn scale(&self, out_w: usize, out_h: usize, ink: Ink<'_>) -> Vec<String> {
        if out_w == 0 || out_h == 0 || self.width == 0 || self.height == 0 {
            return Vec::new();
        }

        (0..out_h)
            .map(|oy| {
                let mut line = String::with_capacity(out_w * 3);
                for ox in 0..out_w {
                    line.push(self.cell(ox, oy, out_w, out_h, ink));
                }
                // Trailing blanks cost bytes on every repaint and render
                // identically to nothing.
                line.truncate(line.trim_end().len());
                line
            })
            .collect()
    }

    fn cell(&self, ox: usize, oy: usize, out_w: usize, out_h: usize, ink: Ink<'_>) -> char {
        match ink {
            Ink::Half => {
                // Two stacked samples per cell, which come out square, so
                // `fit` needs no aspect correction.
                let rows = out_h * 2;
                let solid = |y: usize| self.coverage(ox, y, out_w, rows) >= 0.5;

                match (solid(oy * 2), solid(oy * 2 + 1)) {
                    (true, true) => '█',
                    (true, false) => '▀',
                    (false, true) => '▄',
                    (false, false) => ' ',
                }
            }
            Ink::Quad => {
                let (cols, rows) = (out_w * 2, out_h * 2);
                let solid = |x: usize, y: usize| self.coverage(x, y, cols, rows) >= 0.5;

                let (x, y) = (ox * 2, oy * 2);
                let bits = (solid(x, y) as usize) << 3
                    | (solid(x + 1, y) as usize) << 2
                    | (solid(x, y + 1) as usize) << 1
                    | (solid(x + 1, y + 1) as usize);
                QUADRANTS[bits]
            }
            Ink::Ramp(ramp) => {
                debug_assert!(!ramp.is_empty());
                let top = (ramp.len() - 1) as f32;
                let coverage = self.coverage(ox, oy, out_w, out_h);
                ramp[(coverage * top).round() as usize]
            }
        }
    }
}

/// How coverage is turned back into characters.
#[derive(Clone, Copy)]
pub enum Ink<'a> {
    /// Two vertical samples per cell. Crisp, twice the vertical resolution.
    Half,
    /// 2x2 samples per cell. Double `Half` in both directions, which is what a
    /// detail two source columns wide needs.
    Quad,
    /// One sample per cell, mapped onto a density ramp by coverage.
    Ramp(&'a [char]),
}

/// Indexed by the four samples as bits: TL, TR, BL, BR.
const QUADRANTS: [char; 16] = [
    ' ', '▗', '▖', '▄', '▝', '▐', '▞', '▟', '▘', '▚', '▌', '▙', '▀', '▜', '▛', '█',
];

/// Orthogonal neighbours of `i`, clipped at the edges. Four-connectivity means
/// a diagonal touch isn't an opening, so a diagonally bounded hole stays one.
fn neighbors(i: usize, width: usize, height: usize) -> impl Iterator<Item = usize> {
    let (x, y) = (i % width, i / width);
    // Backward steps are lazy: they underflow at the edge they guard.
    [
        (x > 0).then(|| i - 1),
        (x + 1 < width).then_some(i + 1),
        (y > 0).then(|| i - width),
        (y + 1 < height).then_some(i + width),
    ]
    .into_iter()
    .flatten()
}

/// Half-open source range covering output index `i`, guaranteed non-empty.
fn span(i: usize, out_len: usize, src_len: usize) -> (usize, usize) {
    let start = i * src_len / out_len;
    let end = ((i + 1) * src_len).div_ceil(out_len).min(src_len);
    (start, end.max(start + 1))
}

/// A loaded animation, every frame on a shared canvas.
pub struct Animation {
    pub frames: Vec<Frame>,
    /// Canvas size, used to preserve aspect ratio when fitting to the terminal.
    pub width: usize,
    pub height: usize,
}

impl Animation {
    fn from_texts(texts: impl IntoIterator<Item = String>) -> Option<Self> {
        let frames: Vec<Frame> = texts.into_iter().map(|t| Frame::parse(&t)).collect();
        if frames.is_empty() {
            return None;
        }

        // Authored on one canvas, but be forgiving if they are not.
        let width = frames.iter().map(|f| f.width).max().unwrap_or(0);
        let height = frames.iter().map(|f| f.height).max().unwrap_or(0);

        Some(Self { frames, width, height })
    }

    /// Load `name`, preferring a directory on disk over the bundled copy, so
    /// built-in art can be replaced. An unknown name errors rather than falling
    /// back, since a silent substitution would hide a typo.
    pub fn load(dir: Option<&Path>, name: &str) -> io::Result<Self> {
        if let Some(dir) = dir {
            let path = dir.join(name);
            if path.is_dir()
                && let Some(anim) = Self::from_texts(read_frame_dir(&path)?)
            {
                return Ok(anim);
            }
        }

        if let Some((_, frames)) = BUNDLED.iter().find(|(n, _)| *n == name) {
            return Self::from_texts(frames.iter().map(|s| s.to_string()))
                .ok_or_else(|| io::Error::other("bundled animation is empty"));
        }

        let where_ = dir.map_or_else(
            || "no animation directory is configured".to_string(),
            |d| format!("looked in {}", d.join(name).display()),
        );
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no animation named {name:?} ({where_}); try --list"),
        ))
    }

    /// Largest size fitting the box, aspect ratio kept. Cell shape is already
    /// baked into art authored for a character grid.
    pub fn fit(&self, max_w: usize, max_h: usize) -> (usize, usize) {
        let by_width = max_w as f32 / self.width as f32;
        let by_height = max_h as f32 / self.height as f32;
        // Capped at 1:1: upscaling a mask gives chunkier blocks, not detail.
        let scale = by_width.min(by_height).min(1.0);

        let w = ((self.width as f32 * scale).round() as usize).clamp(1, max_w.max(1));
        let h = ((self.height as f32 * scale).round() as usize).clamp(1, max_h.max(1));
        (w, h)
    }
}

/// One installed animation, as reported by `--list`.
pub struct Entry {
    pub name: String,
    pub frames: usize,
    /// Where it came from, or `None` when it is compiled into the binary.
    pub path: Option<std::path::PathBuf>,
}

/// Every animation available, sorted by name. Directories shadow bundled art
/// here too, so the listing agrees with what `load` would give you.
pub fn list(dir: Option<&Path>) -> Vec<Entry> {
    let mut entries: Vec<Entry> = BUNDLED
        .iter()
        .map(|(name, frames)| Entry {
            name: (*name).to_string(),
            frames: frames.len(),
            path: None,
        })
        .collect();

    if let Some(dir) = dir
        && let Ok(read) = fs::read_dir(dir)
    {
        for path in read.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_dir()) {
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let frames = read_frame_dir(&path).map_or(0, |f| f.len());
            if frames == 0 {
                continue;
            }

            let entry = Entry { name: name.to_string(), frames, path: Some(path.clone()) };
            match entries.iter_mut().find(|e| e.name == name) {
                Some(existing) => *existing = entry,
                None => entries.push(entry),
            }
        }
    }

    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

/// Every `.txt` in `dir`, sorted. Sorting defines frame order, so zero-pad.
fn read_frame_dir(dir: &Path) -> io::Result<Vec<String>> {
    let mut paths: Vec<_> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "txt"))
        .collect();
    paths.sort();

    paths.iter().map(fs::read_to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAMP: Ink<'static> = Ink::Ramp(&[' ', '░', '▒', '▓', '█']);

    #[test]
    fn spans_are_never_empty() {
        // Upscaling maps several output cells onto one source cell.
        for out in 1..40usize {
            for src in 1..40usize {
                for i in 0..out {
                    let (a, b) = span(i, out, src);
                    assert!(b > a, "empty span for i={i} out={out} src={src}");
                    assert!(b <= src);
                }
            }
        }
    }

    #[test]
    fn solid_block_stays_solid_at_any_size() {
        let frame = Frame::parse("####\n####\n####\n####\n");
        for (w, h) in [(2, 2), (4, 4), (9, 7), (1, 1)] {
            let out = frame.scale(w, h, RAMP);
            assert_eq!(out.len(), h);
            assert!(
                out.iter().all(|l| l.chars().all(|c| c == '█')),
                "{w}x{h} produced {out:?}"
            );
        }
    }

    #[test]
    fn thin_stroke_survives_downscaling() {
        // Nearest-neighbour would drop most of this diagonal.
        let mut rows = vec![vec!['.'; 8]; 8];
        for (i, row) in rows.iter_mut().enumerate() {
            row[i] = '#';
        }
        let text: String = rows
            .iter()
            .map(|r| r.iter().collect::<String>() + "\n")
            .collect();

        // '.' is not whitespace, so build the mask from a space-padded version.
        let text = text.replace('.', " ");
        let frame = Frame::parse(&text);

        let out = frame.scale(4, 4, RAMP);
        assert_eq!(out.len(), 4);
        assert!(
            out.iter().all(|l| l.chars().any(|c| c != ' ')),
            "stroke lost: {out:?}"
        );
    }

    #[test]
    fn blank_canvas_renders_empty_lines() {
        let frame = Frame::parse("    \n    \n");
        assert_eq!(frame.scale(2, 2, RAMP), vec!["".to_string(), "".to_string()]);
        assert_eq!(frame.scale(2, 2, Ink::Half), vec!["".to_string(), "".to_string()]);
    }

    #[test]
    fn quadrants_resolve_all_four_corners() {
        // One inked corner at a time must select the matching glyph.
        for (art, expected) in [
            ("#.\n..\n", '▘'),
            (".#\n..\n", '▝'),
            ("..\n#.\n", '▖'),
            ("..\n.#\n", '▗'),
            ("#.\n.#\n", '▚'),
            ("##\n##\n", '█'),
        ] {
            let frame = Frame::parse(&art.replace('.', " "));
            assert_eq!(frame.scale(1, 1, Ink::Quad), vec![expected.to_string()], "{art:?}");
        }
    }

    #[test]
    fn quadrants_keep_a_hole_half_blocks_would_lose() {
        // Two source columns wide into four-column cells: needs Quad.
        let mut rows = vec![vec!['#'; 32]; 8];
        for row in rows.iter_mut() {
            row[12] = ' ';
            row[13] = ' ';
        }
        let text: String = rows.iter().map(|r| r.iter().collect::<String>() + "\n").collect();
        let frame = Frame::parse(&text);

        let quad = frame.scale(8, 4, Ink::Quad);
        assert!(
            quad.iter().all(|l| l.contains('▐') || l.contains('▌') || l.contains(' ')),
            "hole lost in quad mode: {quad:?}"
        );
    }

    #[test]
    fn half_blocks_use_only_solid_glyphs() {
        // The point of this mode is that no glyph is a stipple pattern.
        let frame = Frame::parse("##  \n#  #\n  ##\n####\n");
        let out = frame.scale(4, 2, Ink::Half);
        assert!(
            out.iter().flat_map(|l| l.chars()).all(|c| " ▀▄█".contains(c)),
            "{out:?}"
        );
    }

    #[test]
    fn half_blocks_resolve_two_source_rows_per_cell() {
        // Ink on top only, blank below: one cell, and it must be the upper half
        // rather than a full block or a shade.
        let frame = Frame::parse("##\n  \n");
        assert_eq!(frame.scale(2, 1, Ink::Half), vec!["▀▀".to_string()]);

        let frame = Frame::parse("  \n##\n");
        assert_eq!(frame.scale(2, 1, Ink::Half), vec!["▄▄".to_string()]);
    }

    #[test]
    fn enclosed_holes_are_told_apart_from_background() {
        // A ring: the centre is enclosed, everything outside it is not.
        let frame = Frame::parse("#####\n#   #\n#####\n");
        assert!(frame.hole[5 + 2], "centre of a ring must be a hole");
        assert!(!frame.hole[0], "border must be background");

        // The same shape opened at the bottom is a bay, not a hole.
        let frame = Frame::parse("#####\n#   #\n#   #\n");
        assert!(frame.hole.iter().all(|h| !h), "an open gap is not a hole");
    }

    #[test]
    fn a_hole_survives_every_alignment_against_the_output_grid() {
        // A hole that shifts a column per frame. Plain averaging makes it
        // appear in some frames and vanish in others, which reads as flicker.
        for offset in 0..12 {
            let mut rows = vec![vec!['#'; 40]; 9];
            for row in rows.iter_mut().take(6).skip(3) {
                row[10 + offset] = ' ';
                row[11 + offset] = ' ';
            }
            let text: String =
                rows.iter().map(|r| r.iter().collect::<String>() + "\n").collect();
            let frame = Frame::parse(&text);

            for width in [19usize, 23, 25, 31] {
                let out = frame.scale(width, 4, Ink::Half);
                assert!(
                    out.iter().any(|l| l.contains(' ')),
                    "hole at offset {offset} vanished at width {width}: {out:?}"
                );
            }
        }
    }

    #[test]
    fn a_hole_stays_a_hole_instead_of_becoming_a_shade() {
        // Averaging turns an eye into a mid-ramp char; thresholding keeps it.
        let mut rows = vec![vec!['#'; 12]; 8];
        for row in rows.iter_mut().take(5).skip(2) {
            row[4] = ' ';
            row[5] = ' ';
        }
        let text: String = rows.iter().map(|r| r.iter().collect::<String>() + "\n").collect();
        let frame = Frame::parse(&text);

        let out = frame.scale(12, 4, Ink::Half);
        assert!(
            out.iter().any(|l| l.contains(' ')),
            "the hole was filled in: {out:?}"
        );
    }

    #[test]
    fn embedded_frames_share_one_canvas() {
        let anim = Animation::load(None, DEFAULT_NAME).expect("bundled art must load");
        assert_eq!(anim.frames.len(), 5);
        // Registration between poses depends on this.
        assert!(anim.frames.iter().all(|f| f.height == anim.height));
    }

    #[test]
    fn every_bundled_animation_loads_and_is_listed() {
        let listed = list(None);
        assert_eq!(listed.len(), BUNDLED.len());

        for (name, frames) in BUNDLED {
            let anim = Animation::load(None, name).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(anim.frames.len(), frames.len(), "{name}");
            // Poses have to be registered against a shared canvas, or the art
            // jitters as it plays.
            assert!(anim.frames.iter().all(|f| f.height == anim.height), "{name}");
            assert!(listed.iter().any(|e| e.name == *name && e.path.is_none()));
        }
    }

    /// Connected runs of enclosed hole cells in the source, as pixel lists.
    fn hole_components(frame: &Frame) -> Vec<Vec<(usize, usize)>> {
        let mut seen = vec![false; frame.width * frame.height];
        let mut components = Vec::new();

        for start in 0..frame.width * frame.height {
            if !frame.hole[start] || seen[start] {
                continue;
            }
            let mut component = Vec::new();
            let mut stack = vec![start];
            seen[start] = true;

            while let Some(i) = stack.pop() {
                component.push((i % frame.width, i / frame.width));
                for n in neighbors(i, frame.width, frame.height) {
                    if frame.hole[n] && !seen[n] {
                        seen[n] = true;
                        stack.push(n);
                    }
                }
            }
            components.push(component);
        }
        components
    }

    /// Does this hole leave any mark in the output, or was it swallowed whole?
    fn hole_is_visible(
        component: &[(usize, usize)],
        frame: &Frame,
        out: &[String],
        w: usize,
        h: usize,
    ) -> bool {
        component.iter().any(|&(x, y)| {
            let (ox, oy) = (x * w / frame.width, y * h / frame.height);
            // A short row was trimmed, so the cell is blank: still visible.
            out.get(oy)
                .is_none_or(|line| line.chars().nth(ox).is_none_or(|c| c != '█'))
        })
    }

    #[test]
    fn no_bundled_frame_loses_its_holes_at_any_usable_size() {
        // Holes surviving in some poses and not others reads as flicker.
        for (name, _) in BUNDLED {
            let anim = Animation::load(None, name).unwrap();

            for (i, frame) in anim.frames.iter().enumerate() {
                if !frame.hole.iter().any(|&h| h) {
                    continue; // this pose genuinely has no holes to keep
                }

                for cap in [44usize, 50, 56, 62, 68, 74] {
                    let (w, h) = anim.fit(cap, 20);
                    for ink in [Ink::Half, Ink::Quad] {
                        let out = frame.scale(w, h, ink);
                        for component in hole_components(frame) {
                            assert!(
                                hole_is_visible(&component, frame, &out, w, h),
                                "{name} frame {i}: a {}-cell hole at {:?} was \
                                 swallowed at {w}x{h}",
                                component.len(),
                                component[0],
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn an_unknown_animation_is_an_error_not_a_silent_cat() {
        // An earlier version used "fox-run", which then got bundled.
        let missing = "no-such-animation";
        let Err(err) = Animation::load(None, missing) else {
            panic!("a typo must not silently resolve to the bundled art");
        };
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert!(err.to_string().contains(missing), "{err}");
    }
}
