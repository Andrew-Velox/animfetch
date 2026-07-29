//! Animation frames: loading, and resampling to whatever size the terminal is.
//!
//! Source art is stored as plain text, one file per frame, all frames sharing a
//! common canvas so poses stay registered. We read it as a *coverage* grid
//! (ink / no ink) rather than keeping the original characters, which is what
//! lets us rescale to any size.
//!
//! Two ways of turning coverage back into glyphs, because they fail differently:
//!
//! * [`Ink::Half`] takes two vertical samples per cell and draws them with half
//!   blocks. Twice the vertical detail, and every glyph is a solid shape, so
//!   edges stay crisp and small holes — an eye, a gap between legs — survive as
//!   holes rather than being smeared.
//! * [`Ink::Ramp`] takes one sample per cell and picks a character by density.
//!   Good for a classic ASCII look, but the shaded block characters it defaults
//!   to are stipple patterns in most fonts, which reads as noise on edges, and
//!   averaging fills small holes in.

use std::fs;
use std::io;
use std::path::Path;

/// The animation used when nothing else is configured.
pub const DEFAULT_NAME: &str = "cat-run";

/// Animations compiled into the binary, so a fresh install has art without
/// needing any files on disk. Frames are ordered as listed.
const BUNDLED: &[(&str, &[&str])] = &[
    (
        "cat-run",
        &[
            include_str!("../assets/anim/cat-run/01.txt"),
            include_str!("../assets/anim/cat-run/02.txt"),
            include_str!("../assets/anim/cat-run/03.txt"),
            include_str!("../assets/anim/cat-run/04.txt"),
            include_str!("../assets/anim/cat-run/05.txt"),
        ],
    ),
    (
        "cat-tail",
        &[
            include_str!("../assets/anim/cat-tail/01.txt"),
            include_str!("../assets/anim/cat-tail/02.txt"),
            include_str!("../assets/anim/cat-tail/03.txt"),
            include_str!("../assets/anim/cat-tail/04.txt"),
            include_str!("../assets/anim/cat-tail/05.txt"),
            include_str!("../assets/anim/cat-tail/06.txt"),
            include_str!("../assets/anim/cat-tail/07.txt"),
            include_str!("../assets/anim/cat-tail/08.txt"),
        ],
    ),
    (
        "fox-run",
        &[
            include_str!("../assets/anim/fox-run/01.txt"),
            include_str!("../assets/anim/fox-run/02.txt"),
            include_str!("../assets/anim/fox-run/03.txt"),
            include_str!("../assets/anim/fox-run/04.txt"),
            include_str!("../assets/anim/fox-run/05.txt"),
            include_str!("../assets/anim/fox-run/06.txt"),
            include_str!("../assets/anim/fox-run/07.txt"),
            include_str!("../assets/anim/fox-run/08.txt"),
        ],
    ),
    (
        "dolphin-run",
        &[
            include_str!("../assets/anim/dolphin-run/01.txt"),
            include_str!("../assets/anim/dolphin-run/02.txt"),
            include_str!("../assets/anim/dolphin-run/03.txt"),
            include_str!("../assets/anim/dolphin-run/04.txt"),
            include_str!("../assets/anim/dolphin-run/05.txt"),
            include_str!("../assets/anim/dolphin-run/06.txt"),
            include_str!("../assets/anim/dolphin-run/07.txt"),
            include_str!("../assets/anim/dolphin-run/08.txt"),
            include_str!("../assets/anim/dolphin-run/09.txt"),
        ],
    ),
    (
        "blackhole",
        &[
            include_str!("../assets/anim/blackhole/01.txt"),
            include_str!("../assets/anim/blackhole/02.txt"),
            include_str!("../assets/anim/blackhole/03.txt"),
            include_str!("../assets/anim/blackhole/04.txt"),
            include_str!("../assets/anim/blackhole/05.txt"),
            include_str!("../assets/anim/blackhole/06.txt"),
            include_str!("../assets/anim/blackhole/07.txt"),
            include_str!("../assets/anim/blackhole/08.txt"),
            include_str!("../assets/anim/blackhole/09.txt"),
        ],
    ),
];

/// One frame as a binary coverage mask on a `width * height` grid.
pub struct Frame {
    width: usize,
    height: usize,
    ink: Vec<bool>,
    /// Blank cells fully enclosed by ink — an eye, a nostril — as opposed to
    /// the background outside the figure. These are part of the drawing, and
    /// are preserved rather than averaged. See [`Frame::enclosed_holes`].
    hole: Vec<bool>,
}

impl Frame {
    fn parse(text: &str) -> Self {
        let rows: Vec<Vec<bool>> = text
            .lines()
            .map(|line| line.chars().map(|c| !c.is_whitespace()).collect())
            .collect();

        let height = rows.len();
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);

        // Flatten into a rectangle; short lines pad with blanks.
        let mut ink = vec![false; width * height];
        for (y, row) in rows.iter().enumerate() {
            ink[y * width..y * width + row.len()].copy_from_slice(row);
        }

        let hole = Self::enclosed_holes(&ink, width, height);
        Self { width, height, ink, hole }
    }

    /// Blank cells that cannot be reached from the border without crossing ink.
    ///
    /// Flood filling the background inward is what separates a deliberate hole
    /// from ordinary empty space: a gap between two legs opens onto the border
    /// and is background, an eye does not and is not.
    fn enclosed_holes(ink: &[bool], width: usize, height: usize) -> Vec<bool> {
        if width == 0 || height == 0 {
            return Vec::new();
        }

        // Every blank cell reachable from the border is background. Seed the
        // frontier with the border itself, then flood inward.
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

    /// Smallest enclosed region treated as a deliberate feature.
    ///
    /// Hand-drawn art picks up isolated single-cell gaps, and since a hole
    /// forces its whole output cell blank, keeping those would punch visible
    /// speckles into a solid body. Anything drawn on purpose — an eye is six
    /// cells in the bundled art — clears this comfortably.
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

    /// Fraction of the source block mapping to cell `(ox, oy)` that is inked.
    ///
    /// An enclosed hole anywhere in the block forces the result to 0.0. Without
    /// that, a hole only a couple of source cells wide survives or vanishes
    /// depending on how it happens to straddle the output grid — which, across
    /// an animation, makes it flicker in and out from frame to frame.
    ///
    /// Works in both directions: when upscaling, the block is a single source
    /// cell and the result is simply 0.0 or 1.0.
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
                // Each cell is two stacked samples, so the sample grid is twice
                // as tall as the cell grid. Samples end up square — a character
                // cell is about twice as tall as it is wide — which is why this
                // needs no change to the aspect ratio maths in `fit`.
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
    /// Two vertical samples per cell, drawn with half blocks. Crisp, and twice
    /// the vertical resolution.
    Half,
    /// Four samples per cell in a 2x2 grid, drawn with quadrant blocks. Twice
    /// the resolution of `Half` in *both* directions, which is what it takes to
    /// hold on to a detail only a couple of source columns wide.
    Quad,
    /// One sample per cell, mapped onto a density ramp by coverage.
    Ramp(&'a [char]),
}

/// Quadrant glyphs indexed by the four samples as bits, most significant
/// first: top-left, top-right, bottom-left, bottom-right.
const QUADRANTS: [char; 16] = [
    ' ', '▗', '▖', '▄', '▝', '▐', '▞', '▟', '▘', '▚', '▌', '▙', '▀', '▜', '▛', '█',
];

/// The orthogonal neighbours of cell `i`, clipped at the grid edges.
///
/// Both flood fills below walk the grid this way; four-connectivity is what
/// makes a diagonal touch *not* count as an opening, so a hole bounded only
/// diagonally stays a hole.
fn neighbors(i: usize, width: usize, height: usize) -> impl Iterator<Item = usize> {
    let (x, y) = (i % width, i / width);
    // The two backward steps are lazy because they underflow at the edge they
    // are guarded against; the forward ones cannot and so are eager.
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

/// A loaded animation: every frame on a shared canvas.
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

        // Frames are authored on one canvas, but be forgiving if they are not.
        let width = frames.iter().map(|f| f.width).max().unwrap_or(0);
        let height = frames.iter().map(|f| f.height).max().unwrap_or(0);

        Some(Self { frames, width, height })
    }

    /// Load `name`, preferring a directory on disk over the bundled copy.
    ///
    /// A user folder shadows a bundled animation of the same name, so the
    /// built-in art can be replaced without touching the binary. An unknown
    /// name is an error rather than a fallback: silently substituting different
    /// art would hide a typo, which is easy to miss once several are installed.
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

    /// Largest `(width, height)` that fits inside the given box while keeping
    /// the canvas aspect ratio. Terminal cells are already accounted for by the
    /// art itself, which is authored for a character grid.
    pub fn fit(&self, max_w: usize, max_h: usize) -> (usize, usize) {
        let by_width = max_w as f32 / self.width as f32;
        let by_height = max_h as f32 / self.height as f32;
        // Capped at 1:1 — upscaling a coverage mask only produces chunkier
        // blocks, never more detail, so a huge terminal gets the art at its
        // authored size rather than a blown-up version of it.
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

/// Every animation available, sorted by name.
///
/// A directory shadows a bundled animation of the same name, matching what
/// `load` does — the listing must agree with what you would actually get.
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

/// Read every `.txt` in `dir` in filename order. Sorting is what defines frame
/// order, so zero-padded names (`01.txt`) matter.
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
        // Upscaling maps several output cells onto one source cell; each must
        // still resolve to a real sample rather than a zero-width range.
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
        // A one-cell diagonal on an 8x8 canvas. Nearest-neighbour sampling
        // would drop most of it; coverage averaging must keep every row inked.
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
        // A hole two source columns wide, rendered into cells each covering
        // four source columns: only the doubled horizontal resolution of Quad
        // can still represent it.
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
        // The regression this guards: a two-cell hole whose position shifts by
        // one column per frame. Under plain averaging it appears in some frames
        // and is swallowed in others, which reads as flicker.
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
        // An eye: a small gap inside a solid body. Averaging turns it into a
        // mid-ramp character; thresholding keeps it blank.
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
            // A row shorter than `ox` had its trailing blanks trimmed, so the
            // cell is blank — which counts as visible.
            out.get(oy)
                .is_none_or(|line| line.chars().nth(ox).is_none_or(|c| c != '█'))
        })
    }

    #[test]
    fn no_bundled_frame_loses_its_holes_at_any_usable_size() {
        // The flicker this guards against: holes surviving in some frames of an
        // animation and being swallowed in others, purely because of how each
        // pose happens to line up with the output grid.
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
        // Deliberately a name no animation will ever have — an earlier version
        // of this test used "fox-run", which then got bundled and made it pass
        // for the wrong reason.
        let missing = "no-such-animation";
        let Err(err) = Animation::load(None, missing) else {
            panic!("a typo must not silently resolve to the bundled art");
        };
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert!(err.to_string().contains(missing), "{err}");
    }
}
